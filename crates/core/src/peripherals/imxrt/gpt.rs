// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 General Purpose Timer (GPT1 `0x401E_C000`, GPT2
//! `0x401F_0000`, IMXRT1060RM §52).
//!
//! A 32-bit up-counter clocked by the selected source (`CR.CLKSRC`) divided
//! by `PR.PRESCALER`+1 (and `PR.PRESCALER24M`+1 for the 24 MHz crystal
//! source). Three output compares set `SR.OF1..3`; in restart mode
//! (`CR.FRR` = 0) the counter returns to 0 after matching `OCR1`, in
//! free-run mode it wraps at 2^32 and sets `SR.ROV`. The interrupt line is
//! `SR & IR`. `CR.SWR` is a self-clearing software reset. The counter is a
//! pure function of simulated time, so reads are exact.
//!
//! The source clock rates are chip configuration (`config:` keys), since the
//! simulator does not derive the CCM clock tree: ipg_clk 150 MHz and
//! perclk (ipg_clk_highfreq) 75 MHz are the MCUXpresso `BootClockRUN`
//! values for a 600 MHz core.

use super::{byte_of, Timebase};
use crate::{Peripheral, PeripheralTickResult, SimResult};
use std::any::Any;
use std::cell::Cell;

const CR: u32 = 0x00;
const PR: u32 = 0x04;
const SR: u32 = 0x08;
const IR: u32 = 0x0C;
const OCR1: u32 = 0x10;
const OCR2: u32 = 0x14;
const OCR3: u32 = 0x18;
const ICR1: u32 = 0x1C;
const ICR2: u32 = 0x20;
const CNT: u32 = 0x24;

const CR_EN: u32 = 1 << 0;
const CR_ENMOD: u32 = 1 << 1;
const CR_FRR: u32 = 1 << 9;
const CR_EN_24M: u32 = 1 << 10;
const CR_SWR: u32 = 1 << 15;

/// Clock rates of the GPT sources (Hz).
#[derive(Debug, Clone, Copy)]
pub struct GptClocks {
    pub ipg_hz: u64,
    pub perclk_hz: u64,
    pub osc_hz: u64,
    pub low_hz: u64,
}

impl Default for GptClocks {
    fn default() -> Self {
        Self {
            ipg_hz: 150_000_000,
            perclk_hz: 75_000_000,
            osc_hz: 24_000_000,
            low_hz: 32_768,
        }
    }
}

#[derive(Debug)]
pub struct ImxrtGpt {
    cr: u32,
    pr: u32,
    ir: u32,
    ocr: [u32; 3],
    sr: Cell<u32>,
    /// Counter value at `base_cycle`.
    base_cnt: u64,
    base_cycle: u64,
    /// Counts already folded into `sr` (relative to the base).
    evaluated: Cell<u64>,
    swr_until: u64,
    clocks: GptClocks,
    time: Timebase,
}

impl Default for ImxrtGpt {
    fn default() -> Self {
        Self::new(GptClocks::default())
    }
}

/// Number of integers `c` in `(prev, cur]` with `(base + c) % period == target`.
fn hits(base: i128, prev: i128, cur: i128, target: i128, period: i128) -> i128 {
    let f = |c: i128| (base + c - target).div_euclid(period);
    f(cur) - f(prev)
}

impl ImxrtGpt {
    pub fn new(clocks: GptClocks) -> Self {
        Self {
            cr: 0,
            pr: 0,
            ir: 0,
            ocr: [u32::MAX; 3],
            sr: Cell::new(0),
            base_cnt: 0,
            base_cycle: 0,
            evaluated: Cell::new(0),
            swr_until: 0,
            clocks,
            time: Timebase::default(),
        }
    }

    /// Source clock in Hz after the prescalers, or 0 when stopped.
    fn count_hz(&self) -> (u64, u64) {
        let pre = (self.pr & 0xFFF) as u64 + 1;
        match (self.cr >> 6) & 0x7 {
            1 => (self.clocks.ipg_hz, pre),
            2 => (self.clocks.perclk_hz, pre),
            4 => (self.clocks.low_hz, pre),
            5 if self.cr & CR_EN_24M != 0 => {
                let pre24 = ((self.pr >> 12) & 0xF) as u64 + 1;
                (self.clocks.osc_hz, pre * pre24)
            }
            _ => (0, 1),
        }
    }

    /// Counts elapsed since the base.
    fn counts(&self) -> u64 {
        if self.cr & CR_EN == 0 {
            return 0;
        }
        let (hz, div) = self.count_hz();
        if hz == 0 {
            return 0;
        }
        let dt = self.time.now().saturating_sub(self.base_cycle) as u128;
        (dt * hz as u128 / (self.time.cpu_hz() as u128 * div as u128)) as u64
    }

    fn period(&self) -> i128 {
        if self.cr & CR_FRR == 0 {
            self.ocr[0] as i128 + 1
        } else {
            1i128 << 32
        }
    }

    fn cnt(&self) -> u32 {
        let c = self.base_cnt as i128 + self.counts() as i128;
        (c.rem_euclid(self.period())) as u32
    }

    /// Fold compare events up to now into SR.
    fn eval(&self) {
        if self.cr & CR_EN == 0 {
            return;
        }
        let cur = self.counts() as i128;
        let prev = self.evaluated.get() as i128;
        if cur <= prev {
            return;
        }
        let base = self.base_cnt as i128;
        let period = self.period();
        let mut sr = self.sr.get();
        for (i, &ocr) in self.ocr.iter().enumerate() {
            let target = ocr as i128;
            if target < period && hits(base, prev, cur, target, period) > 0 {
                sr |= 1 << i;
            }
        }
        if self.cr & CR_FRR != 0 && hits(base, prev, cur, 0, period) > 0 {
            sr |= 1 << 5; // ROV
        }
        self.sr.set(sr);
        self.evaluated.set(cur as u64);
    }

    /// Restart counting from `cnt` now.
    fn rebase(&mut self, cnt: u64) {
        self.base_cnt = cnt;
        self.base_cycle = self.time.now();
        self.evaluated.set(0);
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        self.eval();
        match off & !3 {
            CR => {
                if self.time.now() < self.swr_until {
                    self.cr | CR_SWR
                } else {
                    self.cr
                }
            }
            PR => self.pr,
            SR => self.sr.get(),
            IR => self.ir,
            OCR1 => self.ocr[0],
            OCR2 => self.ocr[1],
            OCR3 => self.ocr[2],
            ICR1 | ICR2 => 0,
            CNT => self.cnt(),
            _ => 0,
        }
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        self.eval();
        let v = value & mask;
        match off & !3 {
            CR => {
                let old = self.cr;
                let new = (old & !mask) | v;
                if new & CR_SWR != 0 {
                    // Software reset: all registers but EN/ENMOD/STOPEN/...
                    // to reset; the bit reads 1 for a few ipg cycles.
                    self.cr = new & (CR_EN | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5));
                    self.pr = 0;
                    self.ir = 0;
                    self.ocr = [u32::MAX; 3];
                    self.sr.set(0);
                    self.swr_until = self.time.now() + 8;
                    self.rebase(0);
                    return;
                }
                let cnt_now = self.cnt() as u64;
                self.cr = new & !CR_SWR;
                if old & CR_EN == 0 && new & CR_EN != 0 {
                    // ENMOD=1: counter restarts from 0 when enabled.
                    let start = if new & CR_ENMOD != 0 { 0 } else { cnt_now };
                    self.rebase(start);
                } else if (old ^ new) & (0x7 << 6 | CR_FRR) != 0 {
                    self.rebase(cnt_now);
                }
            }
            PR => {
                let cnt_now = self.cnt() as u64;
                self.pr = (self.pr & !mask) | v;
                self.rebase(cnt_now);
            }
            SR => self.sr.set(self.sr.get() & !(v & 0x3F)),
            IR => self.ir = ((self.ir & !mask) | v) & 0x3F,
            o @ (OCR1 | OCR2 | OCR3) => {
                let i = ((o - OCR1) / 4) as usize;
                self.ocr[i] = (self.ocr[i] & !mask) | v;
                if i == 0 && self.cr & CR_FRR == 0 {
                    // Writing OCR1 in restart mode resets the counter.
                    self.rebase(0);
                } else {
                    let cnt_now = self.cnt() as u64;
                    self.rebase(cnt_now);
                }
            }
            _ => {}
        }
    }

    /// Cycle of the next compare/rollover event, when counting.
    fn next_event_cycle(&self) -> Option<u64> {
        if self.cr & CR_EN == 0 || self.ir == 0 {
            return None;
        }
        let (hz, div) = self.count_hz();
        if hz == 0 {
            return None;
        }
        let period = self.period();
        let cnt = self.cnt() as i128;
        let mut best: Option<i128> = None;
        for &o in &self.ocr {
            let t = o as i128;
            if t >= period {
                continue;
            }
            let mut d = (t - cnt).rem_euclid(period);
            if d == 0 {
                d = period;
            }
            best = Some(best.map_or(d, |b: i128| b.min(d)));
        }
        let counts = best? as u128;
        let cyc = counts * self.time.cpu_hz() as u128 * div as u128 / hz as u128 + 1;
        Some(self.time.now() + cyc.min(u64::MAX as u128 / 2) as u64)
    }

    fn irq(&self) -> bool {
        self.eval();
        self.sr.get() & self.ir & 0x3F != 0
    }
}

impl ImxrtGpt {
    /// Recompute the cached interrupt line (see `Timebase::level`).
    fn refresh_irq(&self) {
        self.time.set_level(self.irq());
    }

    fn tick_inner(&mut self, cycles: u64) -> PeripheralTickResult {
        self.time.advance(cycles);
        let now = self.time.now();
        super::wake_hint(now, self.next_event_cycle())
    }
}

impl Peripheral for ImxrtGpt {
    /// Walked only while timed work is in flight or the interrupt line is
    /// asserted (so its deassert is reconciled); MMIO re-arms it.
    fn legacy_tick_active(&self) -> bool {
        (self.cr & CR_EN != 0 && self.ir != 0) || self.time.level()
    }
    fn legacy_tick_dynamic(&self) -> bool {
        true
    }
    fn read(&self, offset: u64) -> SimResult<u8> {
        let v = byte_of(self.read_reg(offset as u32), offset);
        self.refresh_irq();
        Ok(v)
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        self.write_reg(offset as u32, (value as u32) << shift, 0xFF << shift);
        self.refresh_irq();
        Ok(())
    }
    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        let v = self.read_reg(offset as u32);
        self.refresh_irq();
        Ok(v)
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        self.write_reg(offset as u32, value, u32::MAX);
        self.refresh_irq();
        Ok(())
    }
    fn peek(&self, offset: u64) -> Option<u8> {
        self.read(offset).ok()
    }
    fn tick_elapsed(&mut self, cycles: u64) -> PeripheralTickResult {
        let r = self.tick_inner(cycles);
        self.refresh_irq();
        r
    }
    fn irq_line_level(&self) -> Option<bool> {
        Some(self.time.level())
    }
    fn attach_cycle_clock(&mut self, clock: crate::CycleClock) {
        self.time.attach_clock(clock);
    }
    fn attach_cpu_hz(&mut self, hz: u64) {
        self.time.attach_cpu_hz(hz);
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleClock;

    #[test]
    fn restart_mode_millisecond_tick() {
        let mut g = ImxrtGpt::default();
        let c = CycleClock::default();
        g.attach_cycle_clock(c.clone());
        // SWR, then perclk source, prescaler 75 -> 1 MHz, OCR1 = 999 (1 ms).
        g.write_reg(CR, CR_SWR, u32::MAX);
        assert_ne!(g.read_reg(CR) & CR_SWR, 0);
        c.publish(100);
        assert_eq!(g.read_reg(CR) & CR_SWR, 0);
        g.write_reg(PR, 74, u32::MAX);
        g.write_reg(OCR1, 999, u32::MAX);
        g.write_reg(IR, 1, u32::MAX);
        g.write_reg(CR, (2 << 6) | CR_ENMOD | CR_EN, u32::MAX);
        let t0 = 100u64;
        c.publish(t0 + 598_800); // 998 us: counter 998
        assert_eq!(g.read_reg(SR) & 1, 0);
        g.tick_elapsed(0);
        assert!(!g.irq_line_level().unwrap());
        c.publish(t0 + 599_400); // counter reaches OCR1 = 999
        assert_eq!(g.read_reg(SR) & 1, 1);
        g.tick_elapsed(0);
        assert!(g.irq_line_level().unwrap());
        c.publish(t0 + 600_000); // next count restarts at 0
        assert_eq!(g.read_reg(CNT), 0, "restarted after OCR1");
        g.write_reg(SR, 1, u32::MAX);
        g.tick_elapsed(0);
        assert!(!g.irq_line_level().unwrap());
        c.publish(t0 + 1_500_000);
        assert_eq!(g.read_reg(CNT), 500);
        assert_eq!(g.read_reg(SR) & 1, 1);
    }
}
