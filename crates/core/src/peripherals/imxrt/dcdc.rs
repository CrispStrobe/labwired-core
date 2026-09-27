// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 DC-DC converter (DCDC, `0x4008_0000`, IMXRT1060RM §18).
//!
//! `REG0.STS_DC_OK` (bit 31) reports the output in regulation. It drops when
//! the target voltage (`REG3.TRG`, or `REG3.TARGET_LP`) changes and rises
//! again after the output has slewed to it; the SDK's clock bring-up raises
//! VDD_SOC before switching to 600 MHz and spins on this bit.

use super::{byte_of, Timebase};
use crate::{Peripheral, SimResult};
use std::any::Any;

/// Output settling time after a target change (DCDC step ramp, RM §18.5).
const SETTLE_US: u64 = 20;

const REG0: u32 = 0x0;
const REG3: u32 = 0xC;
const STS_DC_OK: u32 = 1 << 31;
/// REG3 fields that change the regulated voltage: TRG[4:0], TARGET_LP[10:8].
const TARGET_FIELDS: u32 = 0x1F | (0x7 << 8);

#[derive(Debug)]
pub struct ImxrtDcdc {
    /// REG0..REG3, SVD reset values.
    regs: [u32; 4],
    ok_at: u64,
    time: Timebase,
}

impl Default for ImxrtDcdc {
    fn default() -> Self {
        Self::new()
    }
}

impl ImxrtDcdc {
    pub fn new() -> Self {
        Self {
            regs: [0x1403_0111, 0x111B_A29C, 0x0000_4009, 0x0000_010E],
            ok_at: 0,
            time: Timebase::default(),
        }
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        let i = (off / 4) as usize;
        if i >= 4 {
            return 0;
        }
        let v = self.regs[i];
        if off & !3 == REG0 {
            let ok = self.time.now() >= self.ok_at;
            (v & !STS_DC_OK) | if ok { STS_DC_OK } else { 0 }
        } else {
            v
        }
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        let i = (off / 4) as usize;
        if i >= 4 {
            return;
        }
        let old = self.regs[i];
        let mask = if off & !3 == REG0 { mask & !STS_DC_OK } else { mask };
        let new = (old & !mask) | (value & mask);
        self.regs[i] = new;
        if off & !3 == REG3 && (old ^ new) & TARGET_FIELDS != 0 {
            self.ok_at = self.time.now() + self.time.us(SETTLE_US);
        }
    }
}

impl Peripheral for ImxrtDcdc {
    fn read(&self, offset: u64) -> SimResult<u8> {
        Ok(byte_of(self.read_reg(offset as u32 & !3), offset))
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        self.write_reg(offset as u32 & !3, (value as u32) << shift, 0xFF << shift);
        Ok(())
    }
    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        Ok(self.read_reg(offset as u32))
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        self.write_reg(offset as u32, value, u32::MAX);
        Ok(())
    }
    fn peek(&self, offset: u64) -> Option<u8> {
        self.read(offset).ok()
    }
    fn needs_legacy_walk(&self) -> bool {
        false
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
    fn target_change_drops_dc_ok_until_settled() {
        let mut d = ImxrtDcdc::new();
        let c = CycleClock::default();
        d.attach_cycle_clock(c.clone());
        assert_ne!(d.read_reg(REG0) & STS_DC_OK, 0);
        // SDK: REG3 = (REG3 & ~TRG_MASK) | TRG(0x12) for 1.275 V.
        d.write_reg(REG3, (0x10E & !0x1F) | 0x12, u32::MAX);
        assert_eq!(d.read_reg(REG0) & STS_DC_OK, 0);
        c.publish(d.time.us(SETTLE_US));
        assert_ne!(d.read_reg(REG0) & STS_DC_OK, 0);
    }
}
