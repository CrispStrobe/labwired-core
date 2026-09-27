// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 12-bit SAR ADC (ADC1 `0x400C_4000`, ADC2 `0x400C_8000`,
//! IMXRT1060RM §66).
//!
//! Software-triggered conversions (`CFG.ADTRG` = 0): a write to `HC0` with
//! `ADCH` != 0x1F starts a conversion of that channel; after the conversion
//! time `R0` holds the result and `HS.COCO0` is set (cleared by reading `R0`
//! or writing `HC0`). `GC.ADCO` repeats. `GC.CAL` runs the calibration
//! sequence: `GS.CALF` is cleared, and after the calibration time `GC.CAL`
//! self-clears and `HS.COCO0` is set (RM §66.5.6). The interrupt line is
//! `COCO0 & HC0.AIEN`.
//!
//! Channel voltages are board inputs (`Peripheral::set_adc_channel_input`,
//! millivolts); the reference is VREFH = 3.3 V.

use super::{byte_of, Timebase};
use crate::{Peripheral, PeripheralTickResult, SimResult};
use std::any::Any;
use std::cell::Cell;
use std::collections::BTreeMap;

const HC0: u32 = 0x00;
const HS: u32 = 0x20;
const R0: u32 = 0x24;
const CFG: u32 = 0x44;
const GC: u32 = 0x48;
const GS: u32 = 0x4C;
const CV: u32 = 0x50;
const OFS: u32 = 0x54;
const CAL: u32 = 0x58;

const GC_ADCO: u32 = 1 << 6;
const GC_CAL: u32 = 1 << 7;
const GS_ADACT: u32 = 1 << 0;
const GS_CALF: u32 = 1 << 1;
const GS_AWKST: u32 = 1 << 2;
const HC_AIEN: u32 = 1 << 7;
const CFG_ADTRG: u32 = 1 << 13;

/// Conversion time: with the SDK defaults (ADACK, long sample off,
/// hardware average 4) one result takes a few µs.
const CONVERSION_US: u64 = 3;
/// Calibration: several hundred ADC clocks (RM §66.5.6).
const CALIBRATION_US: u64 = 50;
const VREF_MV: u32 = 3300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Busy {
    Idle,
    Convert { channel: u8, done_at: u64 },
    Calibrate { done_at: u64 },
}

#[derive(Debug)]
pub struct ImxrtAdc {
    hc: [u32; 8],
    r: [Cell<u32>; 8],
    cfg: u32,
    gc: Cell<u32>,
    gs: Cell<u32>,
    cv: u32,
    ofs: u32,
    cal: u32,
    coco: Cell<bool>,
    busy: Cell<Busy>,
    inputs: BTreeMap<u8, u16>,
    time: Timebase,
}

impl Default for ImxrtAdc {
    fn default() -> Self {
        Self::new()
    }
}

impl ImxrtAdc {
    pub fn new() -> Self {
        Self {
            hc: [0x1F; 8],
            r: Default::default(),
            cfg: 0x0000_0200,
            gc: Cell::new(0),
            gs: Cell::new(0),
            cv: 0,
            ofs: 0,
            cal: 0,
            coco: Cell::new(false),
            busy: Cell::new(Busy::Idle),
            inputs: BTreeMap::new(),
            time: Timebase::default(),
        }
    }

    fn resolution_bits(&self) -> u32 {
        match (self.cfg >> 2) & 3 {
            0 => 8,
            1 => 10,
            _ => 12,
        }
    }

    fn sample(&self, channel: u8) -> u32 {
        let mv = self.inputs.get(&channel).copied().unwrap_or(0) as u32;
        let full = (1u32 << self.resolution_bits()) - 1;
        (mv.min(VREF_MV) * full + VREF_MV / 2) / VREF_MV
    }

    /// Bring the conversion/calibration state machine up to "now".
    fn settle(&self) {
        let now = self.time.now();
        match self.busy.get() {
            Busy::Convert { channel, done_at } if now >= done_at => {
                self.r[0].set(self.sample(channel));
                self.coco.set(true);
                if self.gc.get() & GC_ADCO != 0 {
                    self.busy.set(Busy::Convert {
                        channel,
                        done_at: done_at + self.time.us(CONVERSION_US),
                    });
                } else {
                    self.busy.set(Busy::Idle);
                }
            }
            Busy::Calibrate { done_at } if now >= done_at => {
                self.gc.set(self.gc.get() & !GC_CAL);
                self.coco.set(true);
                self.busy.set(Busy::Idle);
            }
            _ => {}
        }
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        self.settle();
        match off & !3 {
            o @ 0x00..=0x1C => self.hc[(o / 4) as usize],
            HS => self.coco.get() as u32,
            o @ 0x24..=0x40 => {
                let i = ((o - R0) / 4) as usize;
                if i == 0 {
                    // Reading R0 clears COCO0.
                    self.coco.set(false);
                }
                self.r[i].get()
            }
            CFG => self.cfg,
            GC => self.gc.get(),
            GS => {
                let active = !matches!(self.busy.get(), Busy::Idle);
                (self.gs.get() & !GS_ADACT) | if active { GS_ADACT } else { 0 }
            }
            CV => self.cv,
            OFS => self.ofs,
            CAL => self.cal,
            _ => 0,
        }
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        self.settle();
        let merge = |old: u32| (old & !mask) | (value & mask);
        match off & !3 {
            HC0 => {
                self.hc[0] = merge(self.hc[0]);
                self.coco.set(false);
                let ch = (self.hc[0] & 0x1F) as u8;
                if ch == 0x1F {
                    self.busy.set(Busy::Idle);
                } else if self.cfg & CFG_ADTRG == 0 {
                    self.busy.set(Busy::Convert {
                        channel: ch,
                        done_at: self.time.now() + self.time.us(CONVERSION_US),
                    });
                }
            }
            o @ 0x04..=0x1C => self.hc[(o / 4) as usize] = merge(self.hc[(o / 4) as usize]),
            CFG => self.cfg = merge(self.cfg) & 0x0001_FFFF,
            GC => {
                let new = merge(self.gc.get()) & 0xFF;
                self.gc.set(new);
                if new & GC_CAL != 0 && !matches!(self.busy.get(), Busy::Calibrate { .. }) {
                    // Starting calibration clears CALF and any result.
                    self.gs.set(self.gs.get() & !GS_CALF);
                    self.coco.set(false);
                    self.busy.set(Busy::Calibrate {
                        done_at: self.time.now() + self.time.us(CALIBRATION_US),
                    });
                }
            }
            GS => {
                // CALF and AWKST are write-1-to-clear.
                let clr = value & mask & (GS_CALF | GS_AWKST);
                self.gs.set(self.gs.get() & !clr);
            }
            CV => self.cv = merge(self.cv),
            OFS => self.ofs = merge(self.ofs) & 0x1FFF,
            CAL => self.cal = merge(self.cal) & 0xF,
            _ => {}
        }
    }

    fn irq_level(&self) -> bool {
        self.settle();
        self.coco.get() && self.hc[0] & HC_AIEN != 0
    }
}

impl ImxrtAdc {
    /// Recompute the cached interrupt line (see `Timebase::level`).
    fn refresh_irq(&self) {
        self.time.set_level(self.irq_level());
    }

    fn tick_inner(&mut self, cycles: u64) -> PeripheralTickResult {
        self.time.advance(cycles);
        self.settle();
        let until = match self.busy.get() {
            Busy::Convert { done_at, .. } | Busy::Calibrate { done_at } => Some(done_at),
            Busy::Idle => None,
        };
        super::wake_hint(self.time.now(), until)
    }
}

impl Peripheral for ImxrtAdc {
    /// Walked only while timed work is in flight or the interrupt line is
    /// asserted (so its deassert is reconciled); MMIO re-arms it.
    fn legacy_tick_active(&self) -> bool {
        (!matches!(self.busy.get(), Busy::Idle)) || self.time.level()
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
        // No side effects: R0 reads here must not clear COCO0.
        let off = offset as u32 & !3;
        let v = match off {
            0x24 => self.r[0].get(),
            HS => self.coco.get() as u32,
            _ => self.read_reg(off),
        };
        Some(byte_of(v, offset))
    }
    fn tick_elapsed(&mut self, cycles: u64) -> PeripheralTickResult {
        let r = self.tick_inner(cycles);
        self.refresh_irq();
        r
    }
    fn irq_line_level(&self) -> Option<bool> {
        Some(self.time.level())
    }
    fn set_adc_channel_input(&mut self, channel: u8, millivolts: u16) -> bool {
        if channel < 16 {
            self.inputs.insert(channel, millivolts);
            true
        } else {
            false
        }
    }
    fn adc_channel_count(&self) -> Option<u8> {
        Some(16)
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

    fn adc() -> (ImxrtAdc, CycleClock) {
        let mut a = ImxrtAdc::new();
        let c = CycleClock::default();
        a.attach_cycle_clock(c.clone());
        (a, c)
    }

    #[test]
    fn calibration_completes_and_sets_coco() {
        let (mut a, c) = adc();
        // SDK ADC_DoAutoCalibration: GS = CALF (w1c), GC |= CAL, poll CAL.
        a.write_reg(GS, GS_CALF, u32::MAX);
        a.write_reg(GC, GC_CAL, u32::MAX);
        assert_ne!(a.read_reg(GC) & GC_CAL, 0, "calibration is not instant");
        c.publish(a.time.us(CALIBRATION_US));
        assert_eq!(a.read_reg(GC) & GC_CAL, 0);
        assert_eq!(a.read_reg(GS) & GS_CALF, 0);
        assert_eq!(a.read_reg(HS) & 1, 1);
    }

    #[test]
    fn conversion_reads_the_channel_voltage() {
        let (mut a, c) = adc();
        a.write_reg(CFG, 2 << 2, u32::MAX); // 12-bit
        a.set_adc_channel_input(9, 2500);
        a.write_reg(HC0, 9, u32::MAX);
        assert_eq!(a.read_reg(HS), 0);
        c.publish(a.time.us(CONVERSION_US));
        assert_eq!(a.read_reg(HS), 1);
        assert_eq!(a.read_reg(R0), (2500 * 4095 + 1650) / 3300);
        assert_eq!(a.read_reg(HS), 0, "reading R0 clears COCO0");
    }
}
