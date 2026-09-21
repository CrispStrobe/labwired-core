// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Differential gate for the ATSAMD21 SERCOM USART scheduler migration.
//!
//! The SERCOM is the third shape this migration takes, and the reason all
//! three needed their own gate rather than one shared assumption:
//!
//! | | nRF54L UARTE | nRF54L TWIM | SERCOM |
//! |---|---|---|---|
//! | deferred transfer | EasyDMA | EasyDMA | **none** — TX is synchronous in the DATA write |
//! | async input | host thread | no | **host thread** |
//! | IRQ shape | edge | level | edge |
//!
//! With no deferred transfer there is nothing to re-expose to the bare-CPU
//! oracle, so this model overrides `tick_elapsed_forced` alone and not the
//! `needs_bus_tick_forced` / `tick_with_bus_forced` pair. What it DOES share
//! with the UARTE is the dangerous part: `effective_intflag()` derives
//! `INT_RXC` from `rx_queued()`, and those bytes arrive from a HOST, on
//! another thread. Nothing on the bus fires when one lands. Miss that and the
//! console goes deaf the instant the walk is deleted — the firmware just
//! waits forever, with no error and no output.
//!
//! atsamd21g18a is the worst-clamped family in the tree (~47x: 2567.5 Ir/step
//! against nrf52840's 54.7 on the same fixture and ISA), and all six of its
//! SERCOM instances are this one model.

#![cfg(feature = "event-scheduler")]

mod common;

use common::thumb_asm::Asm;
use common::walk_differential::{assert_modes_differ, assert_probes_identical, Probe, WalkMode};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::peripherals::sam::sercom_usart::SamSercomUsart;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::Bus;
use labwired_core::{DebugControl, Machine};

const SERCOM_BASE: u32 = 0x5000_0000;
const SERCOM_IRQ: u32 = 16;

const R_CTRLA: u32 = SERCOM_BASE + 0x00;
const R_CTRLB: u32 = SERCOM_BASE + 0x04;
const R_INTENSET: u32 = SERCOM_BASE + 0x16;
const R_DATA: u32 = SERCOM_BASE + 0x28;
const NVIC_ISER0: u32 = 0xE000_E100;

/// CTRLA.ENABLE | CTRLA.MODE = 1 (USART, internal clock).
const CTRLA_ENABLE_USART: u8 = (1 << 1) | (1 << 2);
/// CTRLB.TXEN | CTRLB.RXEN, both above bit 15 — built with a shift rather
/// than a literal because `movs` takes an 8-bit immediate.
const CTRLB_TXEN_BIT: u8 = 16;
const CTRLB_RXEN_BIT: u8 = 17;
/// INTENSET.RXC.
const INT_RXC: u8 = 1 << 2;

/// What the host types, and WHEN. The step indices matter as much as the
/// bytes: each is injected while the receiver is already armed and idle, so
/// the only thing that can notice it is the scheduler's poll.
///
/// Pre-seeding the queue instead — the obvious way to write this — would make
/// the whole fixture vacuous. The byte would already be there when CTRLA
/// enables the receiver, the level would go 1 on that MMIO write, and
/// `take_scheduled_events` would arm the wake immediately. The poll path, the
/// entire reason `has_active_work()` carries a receiver-armed term, would
/// never run, and a model that polled not at all would pass.
///
/// They are also spaced so the ISR drains the first before the second lands:
/// `rx_queued()` staying true holds the level up, and with no 1->0 transition
/// there is no second edge. Two bytes arriving together produce ONE interrupt
/// on this model, on the walk and on the scheduler alike.
const RX_INJECTIONS: &[(u64, u8)] = &[(600, b'h'), (1200, b'i')];

const RX_SINK: u32 = 0x2000_0010;
const ISR_COUNT_ADDR: u64 = 0x2000_0000;
const MAIN_COUNT_ADDR: u64 = 0x2000_0004;
const INITIAL_SP: u32 = 0x2000_8000;

const ISR_ENTRY: u32 = 0xC0;
const MAIN_ENTRY: u32 = 0x140;

fn write_bytes(bus: &mut SystemBus, base: u32, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        bus.write_u8(base as u64 + i as u64, *b).unwrap();
    }
}

/// ISR: read DATA (which consumes the byte and drops RXC, dropping the level),
/// store it at RX_SINK + isr_count, bump isr_count.
fn assemble_isr(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    // r0 = isr_count
    a.ldr_pool(2, "isrcnt").ldr0(0, 2);
    // r1 = DATA (consumes)
    a.ldr_pool(3, "data").ldr0(1, 3);
    // *(RX_SINK + isr_count) = r1  — via r3 = sink + count, byte store.
    a.ldr_pool(3, "sink").add_reg(3, 0).strb0(1, 3);
    // isr_count++
    a.adds(0, 1).str0(0, 2);
    a.bx(14);
    a.word("isrcnt", ISR_COUNT_ADDR as u32)
        .word("data", R_DATA)
        .word("sink", RX_SINK);
    a.assemble()
}

/// Main firmware: enable the NVIC line, bring up the USART with RX enabled and
/// RXC interrupts armed, then spin bumping a counter.
fn assemble_main(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    a.ldr_pool(5, "iser")
        .movs(1, 1)
        .lsls(1, 1, SERCOM_IRQ as u8)
        .str0(1, 5);
    // CTRLB = TXEN | RXEN (both above bit 15).
    a.movs(1, 1).lsls(1, 1, CTRLB_TXEN_BIT);
    a.movs(2, 1).lsls(2, 2, CTRLB_RXEN_BIT);
    a.orrs(1, 2);
    a.ldr_pool(5, "ctrlb").str0(1, 5);
    // INTENSET = RXC.
    a.ldr_pool(5, "intenset").movs(1, INT_RXC).str0(1, 5);
    // CTRLA = ENABLE | MODE(USART, internal clock) — last, so the receiver
    // only goes live once it is fully configured.
    a.ldr_pool(5, "ctrla")
        .movs(1, CTRLA_ENABLE_USART)
        .str0(1, 5);
    // Main loop.
    a.ldr_pool(2, "maincnt").movs(3, 0);
    a.label("loop");
    a.adds(3, 1).str0(3, 2).ldr0(4, 2).b("loop");
    a.word("iser", NVIC_ISER0)
        .word("ctrlb", R_CTRLB)
        .word("intenset", R_INTENSET)
        .word("ctrla", R_CTRLA)
        .word("maincnt", MAIN_COUNT_ADDR as u32);
    a.assemble()
}

fn build(mode: WalkMode, tick_interval: u32) -> Machine<CortexM> {
    let mut bus = SystemBus::new();
    let (cpu, _nvic) = configure_cortex_m(&mut bus);

    let mut sercom = SamSercomUsart::new();
    sercom.set_sink(
        Some(std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))),
        false,
    );
    bus.add_peripheral(
        "sercom",
        SERCOM_BASE as u64,
        0x100,
        Some(SERCOM_IRQ),
        Box::new(sercom),
    );

    let idx = bus.find_peripheral_index_by_name("sercom").unwrap();
    {
        let dev = bus.peripherals[idx]
            .dev
            .as_any_mut()
            .unwrap()
            .downcast_mut::<SamSercomUsart>()
            .unwrap();
        // NOTE: the RX queue starts EMPTY. Bytes arrive mid-run, from
        // `run_probed_with_host` — see RX_INJECTIONS for why pre-seeding
        // would make this gate vacuous.
        if mode.is_legacy_walk() {
            dev.force_legacy_walk();
        }
    }

    bus.write_u32(((16 + SERCOM_IRQ) * 4) as u64, ISR_ENTRY | 1)
        .unwrap();
    let (isr_base, isr) = assemble_isr(ISR_ENTRY);
    assert!(
        isr_base + isr.len() as u32 <= MAIN_ENTRY,
        "ISR image ({} bytes) overruns the main entry",
        isr.len()
    );
    write_bytes(&mut bus, isr_base, &isr);
    let (main_base, main) = assemble_main(MAIN_ENTRY);
    write_bytes(&mut bus, main_base, &main);

    let mut machine = Machine::new(cpu, bus);
    machine.config.peripheral_tick_interval = tick_interval;
    machine.bus.config.peripheral_tick_interval = tick_interval;
    machine.cpu.sp = INITIAL_SP;
    machine
}

fn uses_scheduler(machine: &Machine<CortexM>) -> bool {
    let idx = machine.bus.find_peripheral_index_by_name("sercom").unwrap();
    machine.bus.peripherals[idx].dev.uses_scheduler()
}

fn observables(m: &Machine<CortexM>) -> Vec<(&'static str, u64)> {
    vec![
        (
            "isr_count",
            u64::from(m.bus.read_u32(ISR_COUNT_ADDR).unwrap()),
        ),
        (
            "main_count",
            u64::from(m.bus.read_u32(MAIN_COUNT_ADDR).unwrap()),
        ),
        ("rx0", u64::from(m.bus.read_u8(RX_SINK as u64).unwrap())),
        ("rx1", u64::from(m.bus.read_u8(RX_SINK as u64 + 1).unwrap())),
    ]
}

/// Push `byte` into the SERCOM's host-input queue, the way
/// `Bus::attach_uart_rx_source_named` lets a runner do mid-run.
fn inject(machine: &mut Machine<CortexM>, byte: u8) {
    let idx = machine.bus.find_peripheral_index_by_name("sercom").unwrap();
    machine.bus.peripherals[idx]
        .dev
        .as_any_mut()
        .unwrap()
        .downcast_mut::<SamSercomUsart>()
        .unwrap()
        .rx_buffer()
        .lock()
        .unwrap()
        .push_back(byte);
}

/// `run_probed` with a deterministic host attached.
///
/// The harness's own runner cannot do this: its `extra` closure takes `&Machine`,
/// which is right for an observable and wrong for a stimulus. Injecting at a
/// fixed STEP index (rather than from a real thread at a wall-clock instant)
/// is what keeps the two lanes comparable — a real host would land at a
/// different cycle in each run, and the lanes would diverge because of the
/// fixture rather than because of the model.
fn run_probed_with_host(machine: &mut Machine<CortexM>, entry: u32, steps: u64) -> Vec<Probe> {
    machine.cpu.pc = entry;
    let mut probes = Vec::with_capacity(steps as usize);
    for s in 0..steps {
        for (at, byte) in RX_INJECTIONS {
            if *at == s {
                inject(machine, *byte);
            }
        }
        machine.run(Some(1)).expect("machine step");
        let mut regs = [0u32; 16];
        for (i, r) in regs.iter_mut().enumerate() {
            *r = machine.read_core_reg(i as u8);
        }
        probes.push(Probe {
            step: s + 1,
            total_cycles: machine.total_cycles,
            pc: machine.get_pc(),
            regs,
            extra: observables(machine),
        });
    }
    probes
}

#[test]
fn sercom_rx_interrupt_firmware_is_byte_identical_at_interval_1() {
    const STEPS: u64 = 2_000;

    assert_modes_differ(
        uses_scheduler(&build(WalkMode::LegacyWalk, 1)),
        uses_scheduler(&build(WalkMode::Scheduler, 1)),
        "atsamd21 sercom",
    );

    let mut walk = build(WalkMode::LegacyWalk, 1);
    let reference = run_probed_with_host(&mut walk, MAIN_ENTRY, STEPS);

    // The fixture must exercise what it claims to, or the comparison below is
    // two runs of a peripheral that did nothing.
    let isr_count = walk.bus.read_u32(ISR_COUNT_ADDR).unwrap();
    assert_eq!(
        isr_count,
        RX_INJECTIONS.len() as u32,
        "reference lane must take one RXC interrupt per host byte -- got          {isr_count} for {} injections. Fewer means the bytes arrived too          close together for the level to drop between them, which makes this          gate blind to a receiver that delivers once and stops.",
        RX_INJECTIONS.len()
    );
    for (i, (_, byte)) in RX_INJECTIONS.iter().enumerate() {
        assert_eq!(
            walk.bus.read_u8(RX_SINK as u64 + i as u64).unwrap(),
            *byte,
            "reference lane must have read injection {i} back through DATA"
        );
    }

    let mut sched = build(WalkMode::Scheduler, 1);
    let candidate = run_probed_with_host(&mut sched, MAIN_ENTRY, STEPS);

    assert_probes_identical(&reference, &candidate, "atsamd21 sercom rx firmware");
}

/// Across batch widths: IRQ DELIVERY quantises to the batch grid at
/// interval > 1 (documented, bounded by one interval), so per-instruction
/// state is not compared. What must NOT change is the bytes -- every host byte
/// delivered, once, in order.
#[test]
fn every_host_byte_arrives_exactly_once_at_any_batch_width() {
    const STEPS: u64 = 4_000;

    for interval in [1u32, 8, 64] {
        let mut m = build(WalkMode::Scheduler, interval);
        run_probed_with_host(&mut m, MAIN_ENTRY, STEPS);
        let isr_count = m.bus.read_u32(ISR_COUNT_ADDR).unwrap();
        assert_eq!(
            isr_count,
            RX_INJECTIONS.len() as u32,
            "interval {interval}: expected one RXC interrupt per host byte,              got {isr_count} -- a duplicated wake shows up here as too many,              a receiver that stopped polling as too few"
        );
        for (i, (_, byte)) in RX_INJECTIONS.iter().enumerate() {
            assert_eq!(
                m.bus.read_u8(RX_SINK as u64 + i as u64).unwrap(),
                *byte,
                "interval {interval}: injection {i} must still land, in order"
            );
        }
    }
}
