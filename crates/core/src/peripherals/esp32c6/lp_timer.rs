// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! ESP32-C6 LP_TIMER (`0x600B_0C00`) — the RTC main timer behind
//! `rtc_time_get()`.
//!
//! ## Why this exists
//!
//! The C6 keeps RTC time in the LP_TIMER block, not in the C3's RTC_CNTL
//! (`0x6000_8000`) and not in the C3's `Esp32c3RtcTimer` register shape: the
//! LP_TIMER window is TAR0/TAR1 @0x00..0x0C, UPDATE @0x10, four MAIN_BUF*
//! snapshot words @0x14..0x20, MAIN_OVERFLOW @0x24 and the INT_* pair at
//! 0x28..0x34 (esp32c6.svd `LP_TIMER`). So the C3 model cannot be reused with
//! the C6 base, and this is the minimal honest substitute.
//!
//! ## Snapshot protocol (esp-idf v5.3, `hal/lp_timer_hal.c`)
//!
//! `lp_timer_hal_get_cycle_count()` drives the real protocol:
//!
//! ```text
//! dev->update.update = 1;                       // strobe: counter -> buffer 0
//! lo = dev->counter[0].lo; hi = dev->counter[0].hi;
//! return (hi << 32) | lo;
//! ```
//!
//! and the LL documents the shift: "Shifts current count to buffer 0, and the
//! value in buffer 0 to buffer 1". This model implements exactly that:
//!
//! * a free-running 48-bit counter that advances with elapsed simulated time,
//! * a write of `UPDATE.MAIN_TIMER_UPDATE` (bit 28) latches it into MAIN_BUF0
//!   and shifts the previous MAIN_BUF0 into MAIN_BUF1,
//! * MAIN_BUF0/BUF1 reads return the latched snapshots (16-bit high words, per
//!   the SVD field width) and do **not** advance between strobes.
//!
//! ## Time source
//!
//! Same two drive modes as the C3 `Esp32c3RtcTimer` (the walk-free contract):
//! with an event-scheduler [`CycleClock`] attached the counter advances lazily
//! via [`Peripheral::sync_to`] at MMIO access, anchored idempotently; without
//! one (hand-built buses, feature off) the legacy walk drives
//! [`Self::tick_elapsed`] one tick per elapsed CPU cycle. Either way the
//! observable contract is monotonic advance — the absolute RTC-slow rate
//! (32.768 kHz XTAL / RC_SLOW) is NOT modelled.
//!
//! ## What is deliberately NOT modelled
//!
//! * The alarm comparators TAR0/TAR1 (stored, never compared) and their
//!   MAIN_OVERFLOW/alarm interrupts; INT_RAW/INT_ST/INT_ENA/INT_CLR are
//!   plain register-backed storage, no source ever asserts.
//! * The RTC-slow clock rate, XTAL_OFF / SYS_STALL / SYS_RST update bits.
//! * Retention across sleep/deep-sleep and `lp_timer_ll_*` power domains.

use crate::{CycleClock, Peripheral, SimResult};
use std::cell::Cell;

/// The whole LP_PERI sub-block is one 0x400 window (SVD `LP_TIMER` DATE
/// @0x3FC). The base itself lives in the chip YAML
/// (`configs/chips/esp32c6.yaml`, `lp_timer` @0x600B_0C00) — a copy here would
/// be a second home for the same fact and is rejected by the
/// `yaml_owned_base_contract` gate.
pub const LP_TIMER_SIZE: u64 = 0x400;

const UPDATE: u64 = 0x10;
const MAIN_BUF0_LOW: u64 = 0x14;
const MAIN_BUF0_HIGH: u64 = 0x18;
const MAIN_BUF1_LOW: u64 = 0x1C;
const MAIN_BUF1_HIGH: u64 = 0x20;

/// UPDATE.MAIN_TIMER_UPDATE (bit 28) — the snapshot strobe.
const MAIN_TIMER_UPDATE_BIT: u32 = 1 << 28;
/// MAIN_TIMER_BUF0/1_HIGH is a 16-bit field (SVD); the counter is 48 bits.
const HIGH_MASK: u32 = 0xFFFF;
/// The LP main timer is 48 bits wide.
const COUNTER_MASK: u64 = (1u64 << 48) - 1;

#[derive(Debug)]
pub struct Esp32c6LpTimer {
    /// Register-backed storage for the whole window (non-snapshot registers).
    regs: Vec<u32>,
    /// Free-running 48-bit counter, one step per elapsed CPU cycle.
    counter: Cell<u64>,
    /// MAIN_BUF0: the most recent snapshot (fresh at each UPDATE strobe).
    buf0: Cell<u64>,
    /// MAIN_BUF1: the snapshot before that (shifted on each strobe).
    buf1: Cell<u64>,
    /// Lazy-path anchor: absolute CPU cycle `counter` was last advanced to.
    anchor_tick: Cell<u64>,
    /// Bus-published cycle clock. `Some` once `SystemBus::add_peripheral`
    /// attaches it; `None` keeps the model on the legacy walk path.
    clock: Option<CycleClock>,
}

impl Default for Esp32c6LpTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl Esp32c6LpTimer {
    pub fn new() -> Self {
        Self {
            regs: vec![0u32; (LP_TIMER_SIZE / 4) as usize],
            counter: Cell::new(0),
            buf0: Cell::new(0),
            buf1: Cell::new(0),
            anchor_tick: Cell::new(0),
            clock: None,
        }
    }

    crate::cycle_clock::scheduler_mode!();

    /// Lazy advance to absolute CPU cycle `now` — callable from `&self`
    /// (all mutated state is in `Cell`). Idempotent and monotonic.
    fn advance_to(&self, now: u64) {
        let anchor = self.anchor_tick.get();
        if now <= anchor {
            return;
        }
        self.counter
            .set(self.counter.get().wrapping_add(now - anchor) & COUNTER_MASK);
        self.anchor_tick.set(now);
    }

    /// Pull "now" from the bus-published clock and advance. No-op without an
    /// attached clock (legacy mode — the walk advances the counter instead).
    fn sync_from_clock(&self) {
        if let Some(clock) = &self.clock {
            if self.scheduler_mode() {
                self.advance_to(clock.now());
            }
        }
    }

    /// The LP_TIMER snapshot strobe: counter → BUF0, old BUF0 → BUF1.
    fn snapshot(&self) {
        self.sync_from_clock();
        self.buf1.set(self.buf0.get());
        self.buf0.set(self.counter.get());
    }

    /// Test/differential knob: detach the cycle clock, pinning the model to
    /// the legacy walk path (`uses_scheduler() == false`). Mirrors
    /// `Esp32c3RtcTimer::force_legacy_walk`.
    pub fn force_legacy_walk(&mut self) {
        self.clock = None;
    }

    /// Live counter (debug helper / test introspection).
    pub fn counter(&self) -> u64 {
        self.counter.get()
    }
}

impl Peripheral for Esp32c6LpTimer {
    fn read(&self, offset: u64) -> SimResult<u8> {
        let w = self.read_u32(offset & !3)?;
        Ok((w >> ((offset & 3) * 8)) as u8)
    }

    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let aligned = offset & !3;
        let sh = (offset & 3) * 8;
        let cur = self.read_u32(aligned)?;
        self.write_u32(aligned, (cur & !(0xFFu32 << sh)) | ((value as u32) << sh))
    }

    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        Ok(match offset {
            MAIN_BUF0_LOW => self.buf0.get() as u32,
            MAIN_BUF0_HIGH => ((self.buf0.get() >> 32) as u32) & HIGH_MASK,
            MAIN_BUF1_LOW => self.buf1.get() as u32,
            MAIN_BUF1_HIGH => ((self.buf1.get() >> 32) as u32) & HIGH_MASK,
            _ => *self.regs.get((offset / 4) as usize).unwrap_or(&0),
        })
    }

    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        if offset == UPDATE && (value & MAIN_TIMER_UPDATE_BIT) != 0 {
            // The strobe consumes itself on silicon (the IDF LL writes 1 and
            // immediately reads the buffers); store the register with the
            // strobe bit clear so a readback shows a self-cleared strobe.
            self.snapshot();
            if let Some(slot) = self.regs.get_mut((UPDATE / 4) as usize) {
                *slot = value & !MAIN_TIMER_UPDATE_BIT;
            }
            return Ok(());
        }
        if let Some(slot) = self.regs.get_mut((offset / 4) as usize) {
            *slot = value;
        }
        Ok(())
    }

    fn tick(&mut self) -> crate::PeripheralTickResult {
        self.tick_elapsed(1)
    }

    /// Legacy walk drive: one tick per elapsed CPU cycle. Inert in scheduler
    /// mode (the walk never calls it there; the guard keeps a stray direct
    /// call from double-counting against the lazy anchor).
    fn tick_elapsed(&mut self, cycles: u64) -> crate::PeripheralTickResult {
        if !self.scheduler_mode() {
            self.counter
                .set(self.counter.get().wrapping_add(cycles) & COUNTER_MASK);
        }
        crate::PeripheralTickResult::default()
    }

    fn uses_scheduler(&self) -> bool {
        self.scheduler_mode()
    }

    fn sync_to(&mut self, now_cycle: u64) {
        self.advance_to(now_cycle);
    }

    fn attach_cycle_clock(&mut self, clock: CycleClock) {
        self.anchor_tick.set(clock.now());
        self.clock = Some(clock);
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(t: &mut Esp32c6LpTimer) -> (u64, u64) {
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT).unwrap();
        let b0 = (t.read_u32(MAIN_BUF0_LOW).unwrap() as u64)
            | ((t.read_u32(MAIN_BUF0_HIGH).unwrap() as u64) << 32);
        let b1 = (t.read_u32(MAIN_BUF1_LOW).unwrap() as u64)
            | ((t.read_u32(MAIN_BUF1_HIGH).unwrap() as u64) << 32);
        (b0, b1)
    }

    #[test]
    fn counter_advances_and_latches() {
        let mut t = Esp32c6LpTimer::new();
        let (b0, _) = snapshot(&mut t);
        assert_eq!(b0, 0);
        for _ in 0..1000 {
            t.tick();
        }
        let (b1, prev) = snapshot(&mut t);
        assert_eq!(b1, 1000, "counter must advance one tick per cycle");
        assert_eq!(prev, 0, "the previous snapshot shifts into BUF1");
    }

    #[test]
    fn readout_frozen_until_next_update() {
        let mut t = Esp32c6LpTimer::new();
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT).unwrap();
        let snap = t.read_u32(MAIN_BUF0_LOW).unwrap();
        for _ in 0..50 {
            t.tick();
        }
        // No new strobe: the readout must be unchanged.
        assert_eq!(t.read_u32(MAIN_BUF0_LOW).unwrap(), snap);
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT).unwrap();
        assert_eq!(t.read_u32(MAIN_BUF0_LOW).unwrap(), snap + 50);
    }

    #[test]
    fn update_bit_self_clears_on_write() {
        let mut t = Esp32c6LpTimer::new();
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT | 0x1).unwrap();
        assert_eq!(
            t.read_u32(UPDATE).unwrap() & MAIN_TIMER_UPDATE_BIT,
            0,
            "the snapshot strobe consumes itself"
        );
        assert_eq!(t.read_u32(UPDATE).unwrap() & 0x1, 0x1, "other bits stored");
    }

    #[test]
    fn high_word_is_16_bits() {
        let mut t = Esp32c6LpTimer::new();
        // Advance past 2^32 with a pattern in the upper bits: 0x1234_5678_9ABC.
        t.tick_elapsed(0x1234_5678_9ABC);
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT).unwrap();
        assert_eq!(t.read_u32(MAIN_BUF0_LOW).unwrap(), 0x5678_9ABC);
        assert_eq!(t.read_u32(MAIN_BUF0_HIGH).unwrap(), 0x1234);
    }

    #[test]
    fn counter_wraps_at_48_bits() {
        let mut t = Esp32c6LpTimer::new();
        t.tick_elapsed(COUNTER_MASK);
        for _ in 0..2 {
            t.tick();
        }
        t.write_u32(UPDATE, MAIN_TIMER_UPDATE_BIT).unwrap();
        assert_eq!(t.counter(), 1, "48-bit wrap");
    }

    #[test]
    fn without_clock_stays_on_legacy_tick_path() {
        let t = Esp32c6LpTimer::new();
        assert!(!t.uses_scheduler());
    }

    #[test]
    fn clock_attach_flips_to_scheduler_and_latch_tracks_published_clock() {
        let clock = CycleClock::default();
        let mut t = Esp32c6LpTimer::new();
        t.attach_cycle_clock(clock.clone());
        if !t.uses_scheduler() {
            // Legacy (no `event-scheduler`) build: attach is inert and the
            // walk path is covered by the test above. The scheduler path this
            // test drives only exists with the feature compiled in.
            return;
        }

        clock.publish(1234);
        let (b0, _) = snapshot(&mut t);
        assert_eq!(b0, 1234);
        clock.publish(1234 + 4096);
        let (b0, prev) = snapshot(&mut t);
        assert_eq!(b0, 1234 + 4096);
        assert_eq!(prev, 1234, "double-buffer shift across clock syncs");

        // Idempotent at the same published cycle.
        let (b0, _) = snapshot(&mut t);
        assert_eq!(b0, 1234 + 4096);
    }

    #[test]
    fn other_registers_are_register_backed() {
        let mut t = Esp32c6LpTimer::new();
        t.write_u32(0x00, 0xDEAD_BEEF).unwrap(); // TAR0_LOW
        assert_eq!(t.read_u32(0x00).unwrap(), 0xDEAD_BEEF);
        t.write_u32(0x28, 0x0000_4000).unwrap(); // INT_RAW
        assert_eq!(t.read_u32(0x28).unwrap(), 0x0000_4000);
    }
}
