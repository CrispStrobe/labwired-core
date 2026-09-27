// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! NXP i.MX RT10xx (MIMXRT1062) peripheral models.
//!
//! Register maps and reset values come from the vendored NXP SVD
//! (`tests/fixtures/real_world/mimxrt1062.svd`, MIMXRT1062 v1.0); behaviour
//! from the i.MX RT1060 Reference Manual (IMXRT1060RM rev. 3). Status bits
//! that firmware polls are finite-state machines driven by simulated time
//! (the bus cycle clock), never constants: a PLL reports LOCK only once it is
//! powered and its lock time has elapsed, and a divider handshake reports BUSY
//! until the change has settled.

pub mod adc;
pub mod anadig;
pub mod ccm;
pub mod dcdc;
pub mod edma;
pub mod flexio;
pub mod flexspi;
pub mod gpt;
pub mod lpi2c;
pub mod lpuart;
pub mod sai;
pub mod usb;

use crate::{CycleClock, PeripheralTickResult};

/// Default core clock used for µs -> cycle conversions until the bus attaches
/// the system's `cpu_hz` (MIMXRT1062DVL6A: 600 MHz).
pub const DEFAULT_CPU_HZ: u64 = 600_000_000;

/// Simulated time as seen by a model: the bus-published cycle clock when one
/// is attached, else the cycles delivered through `tick_elapsed`.
#[derive(Debug, Clone)]
pub struct Timebase {
    clock: Option<CycleClock>,
    ticked: u64,
    cpu_hz: u64,
    /// The model's interrupt line as last computed. The bus samples
    /// `irq_line_level` on every walk tick, so models recompute it only when
    /// their state can change (MMIO access, their own wake-up tick) and
    /// serve this cached value in between.
    level: std::cell::Cell<bool>,
}

impl Default for Timebase {
    fn default() -> Self {
        Self {
            clock: None,
            ticked: 0,
            cpu_hz: DEFAULT_CPU_HZ,
            level: std::cell::Cell::new(false),
        }
    }
}

impl Timebase {
    /// Current simulated cycle.
    pub fn now(&self) -> u64 {
        match &self.clock {
            Some(c) => c.now().max(self.ticked),
            None => self.ticked,
        }
    }
    /// Cycles in `us` microseconds at the attached core clock.
    pub fn us(&self, us: u64) -> u64 {
        (self.cpu_hz / 1_000_000).max(1) * us
    }
    pub fn attach_clock(&mut self, clock: CycleClock) {
        self.clock = Some(clock);
    }
    pub fn attach_cpu_hz(&mut self, hz: u64) {
        if hz > 0 {
            self.cpu_hz = hz;
        }
    }
    pub fn cpu_hz(&self) -> u64 {
        self.cpu_hz
    }
    /// Cached interrupt level (see the field doc).
    pub fn level(&self) -> bool {
        self.level.get()
    }
    pub fn set_level(&self, on: bool) {
        self.level.set(on);
    }
    /// Advance the fallback time base (called from `tick_elapsed`).
    pub fn advance(&mut self, cycles: u64) {
        self.ticked = self.ticked.saturating_add(cycles);
    }
}

/// Legacy-walk cadence hint: wake at `until` (an absolute cycle), or after
/// `IDLE_TICKS` when nothing is scheduled. Any MMIO access to the model
/// re-arms the walk immediately, so a long idle hint never delays firmware.
pub fn wake_hint(now: u64, until: Option<u64>) -> PeripheralTickResult {
    let ticks = match until {
        Some(t) => t.saturating_sub(now).max(1),
        None => IDLE_TICKS,
    };
    PeripheralTickResult {
        ticks_until_next: Some(ticks),
        ..Default::default()
    }
}

/// Idle re-check period of a quiescent model (~1.7 ms at 600 MHz).
pub const IDLE_TICKS: u64 = 1 << 20;

/// Merge one byte into a 32-bit register image (byte-lane write).
pub fn merge_byte(word: u32, offset: u64, value: u8) -> u32 {
    let shift = ((offset & 3) * 8) as u32;
    (word & !(0xFF << shift)) | ((value as u32) << shift)
}

/// Byte `offset & 3` of a 32-bit register image.
pub fn byte_of(word: u32, offset: u64) -> u8 {
    (word >> ((offset & 3) * 8)) as u8
}
