// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Closed-form catch-up for one decoded C3 loop, not timer-read admission.
//!
//! SW UPDATE; LW snapshot; LW GPIO input; JAL x0,-12 has no data-dependent
//! branch. The two load destinations must not feed ANY address or UPDATE
//! operand. Only the final iteration's values survive. The concrete timer
//! projects its final UPDATE using its live counter, fractional carry and
//! enable bits; the concrete GPIO input is inert between machine events.
//! All other stores, arithmetic, branches and register aliasing refuse.
//! Alarms, observation gates and active reservations refuse as well. Machine
//! deadlines still bound the skip; a CLINT edge is bounded here too. The plan
//! is re-proved against live code, CPU and device state before commit.

use super::{decode_rv32, Instruction, RiscV, RiscVCoreProfile};
use crate::Bus;

const PERIOD: u64 = 4;
const MAX_SKIP: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan {
    head: u32,
    words: [u32; 4],
    regs: [u32; 32],
    mtime: u64,
    mtimecmp: u64,
    mstatus: u32,
    mie: u32,
    mip: u32,
    start: u64,
    cycles: u64,
    timer_rd: u8,
    gpio_rd: u8,
    addresses: [u64; 3],
    snapshot: u64,
    gpio: u32,
}

fn code_words(bus: &dyn Bus, pc: u32) -> Option<[u32; 4]> {
    // Plain immutable borrow, never speculative MMIO reads. A patched RAM
    // loop is decoded again on every proof and every application.
    let (base, end, bytes) = bus.plain_memory_slice(u64::from(pc))?;
    let last = u64::from(pc).checked_add(16)?;
    if last > end
        || last > u64::from(u32::MAX) + 1
        || pc & 3 != 0
        || (u64::from(pc)..last).any(|address| bus.has_mmio_window(address))
    {
        return None;
    }
    let off = usize::try_from(u64::from(pc).checked_sub(base)?).ok()?;
    let raw = bytes.get(off..off.checked_add(16)?)?;
    Some(std::array::from_fn(|i| {
        u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap())
    }))
}

impl RiscV {
    fn systimer_poll_proof(&self, bus: &dyn Bus, limit: u64) -> Option<Plan> {
        if self.core_profile != RiscVCoreProfile::Esp32C3
            || self.systimer_poll_anchor != Some(self.pc)
            || self.waiting_for_interrupt
            || self.reservation.is_some()
            || self.triggers.armed()
            || (self.mip & self.mie) | bus.external_irq_lines() != 0
        {
            return None;
        }
        let words = code_words(bus, self.pc)?;
        if self.fetch_len != 0 {
            let base = u64::from(self.fetch_base);
            let end = base + u64::from(self.fetch_len);
            let head = u64::from(self.pc);
            if head < end && head + 16 > base {
                if head < base || head + 16 > end {
                    return None;
                }
                let off = (head - base) as usize;
                for (i, word) in words.iter().enumerate() {
                    if self.fetch_bytes[off + 4 * i..off + 4 * i + 4] != word.to_le_bytes() {
                        return None;
                    }
                }
            }
        }
        let Instruction::Sw {
            rs1: tb,
            rs2: value,
            imm: 4,
        } = decode_rv32(words[0])
        else {
            return None;
        };
        let Instruction::Lw {
            rd: timer_rd,
            rs1: read_tb,
            imm: 0x44,
        } = decode_rv32(words[1])
        else {
            return None;
        };
        let Instruction::Lw {
            rd: gpio_rd,
            rs1: gb,
            imm: 0x3c,
        } = decode_rv32(words[2])
        else {
            return None;
        };
        if !matches!(decode_rv32(words[3]), Instruction::Jal { rd: 0, imm: -12 })
            || tb != read_tb
            || self.read_reg(value) != 1 << 30
            || timer_rd == 0
            || gpio_rd == 0
            || timer_rd == gpio_rd
            || [tb, gb, value]
                .iter()
                .any(|r| *r == timer_rd || *r == gpio_rd)
        {
            return None;
        }
        let addresses = [
            u64::from(self.read_reg(tb).wrapping_add(4)),
            u64::from(self.read_reg(read_tb).wrapping_add(0x44)),
            u64::from(self.read_reg(gb).wrapping_add(0x3c)),
        ];
        let start = bus.current_cycle();
        let mut cycles = limit.min(MAX_SKIP).min(u64::MAX - self.mtime);
        if self.mtimecmp > self.mtime {
            // A step can take the CLINT interrupt immediately after retiring.
            // Leave that instruction to the interpreter, including a final
            // JAL that would otherwise appear to return to the loop head.
            cycles = cycles.min(self.mtimecmp - self.mtime - 1);
        }
        if let Some(deadline) = bus.next_cpu_interrupt_deadline_cycle() {
            cycles = cycles.min(deadline.saturating_sub(start));
        }
        cycles -= cycles % PERIOD;
        if cycles < PERIOD {
            return None;
        }
        let (snapshot, gpio) = bus.c3_systimer_poll_values(start, cycles, addresses)?;
        Some(Plan {
            head: self.pc,
            words,
            regs: self.x,
            mtime: self.mtime,
            mtimecmp: self.mtimecmp,
            mstatus: self.mstatus,
            mie: self.mie,
            mip: self.mip,
            start,
            cycles,
            timer_rd,
            gpio_rd,
            addresses,
            snapshot,
            gpio,
        })
    }

    pub(super) fn prove_systimer_poll(&mut self, bus: &dyn Bus, limit: u64) -> Option<(u64, u64)> {
        self.systimer_poll_plan = None;
        let plan = self.systimer_poll_proof(bus, limit)?;
        let budget = plan.cycles;
        self.systimer_poll_plan = Some(plan);
        Some((PERIOD, budget))
    }

    pub(super) fn apply_systimer_poll(&mut self, bus: &mut dyn Bus, cycles: u64) -> Option<u64> {
        let held = self.systimer_poll_plan.as_ref()?;
        if cycles == 0 || cycles % PERIOD != 0 || cycles > held.cycles {
            return None;
        }
        // First revalidate the entire original plan, then derive the final
        // values for a machine deadline that shortened its whole-loop budget.
        if self.systimer_poll_proof(bus, held.cycles).as_ref() != Some(held) {
            return None;
        }
        let plan = self.systimer_poll_proof(bus, cycles)?;
        let (snapshot, gpio) = bus.c3_commit_systimer_poll(plan.start, cycles, plan.addresses)?;
        // No fallible step follows the bus commit.
        self.write_reg(plan.timer_rd, snapshot);
        self.write_reg(plan.gpio_rd, gpio);
        self.update_mtime_after_elapsed_cycles(cycles);
        self.systimer_poll_plan = None;
        Some(cycles)
    }
}
