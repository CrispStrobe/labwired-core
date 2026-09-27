// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 analog block at `0x400D_8000`: CCM_ANALOG (PLLs, PFDs), PMU
//! (LDO regulators), USB_ANALOG, XTALOSC24M and TEMPMON share one register
//! window (the SVD lists them as five peripherals at the same base).
//!
//! Every read-write register has the i.MX SET/CLR/TOG aliases at +4/+8/+0xC
//! (IMXRT1060RM §13/§14/§15, and the `*_SET/_CLR/_TOG` registers in the SVD).
//!
//! Status bits are FSMs over simulated time:
//! * `PLL_*.LOCK` (bit 31) rises `PLL_LOCK_US` after the PLL is powered
//!   (POWERDOWN=0, or POWER=1 for the USB PLLs) and drops the moment it is
//!   powered down or its loop divider changes.
//! * `PMU_REG_{1P1,3P0,2P5}.OK_*` (bit 17) rise `LDO_OK_US` after
//!   `ENABLE_LINREG` is set.
//! * `LOWPWR_CTRL.XTALOSC_PWRUP_STAT` follows `MISC0.XTAL_24M_PWD`=0 after
//!   the oscillator start-up time; `MISC0.OSC_XTALOK` additionally needs
//!   `MISC0.OSC_XTALOK_EN`.
//! * `TEMPSENSE0.FINISHED` rises one measurement time after `MEASURE_TEMP`.

use super::{byte_of, Timebase};
use crate::{Peripheral, SimResult};
use std::any::Any;

/// PLL lock time. IMXRT1060RM gives lock in < 50 µs for the analog PLLs.
const PLL_LOCK_US: u64 = 50;
/// LDO regulator OK delay after enabling (settling, RM §15 "PMU").
const LDO_OK_US: u64 = 20;
/// 24 MHz crystal oscillator start-up (XTALOSC_PWRUP_DELAY default 0.5 ms
/// is the worst case; the oscillator runs from POR on silicon).
const XTAL_UP_US: u64 = 500;
/// TEMPMON one-shot measurement time (RM §16: ~ 1/MEASURE_FREQ period).
const TEMP_MEASURE_US: u64 = 100;

/// (offset, SVD reset value, has SET/CLR/TOG aliases, read-only)
const REGS: &[(u32, u32, bool, bool)] = &[
    (0x000, 0x0001_3063, true, false),  // PLL_ARM
    (0x010, 0x0001_2000, true, false),  // PLL_USB1
    (0x020, 0x0001_2000, true, false),  // PLL_USB2
    (0x030, 0x0001_3001, true, false),  // PLL_SYS
    (0x040, 0x0000_0000, false, false), // PLL_SYS_SS
    (0x050, 0x0000_0000, false, false), // PLL_SYS_NUM
    (0x060, 0x0000_0012, false, false), // PLL_SYS_DENOM
    (0x070, 0x0001_1006, true, false),  // PLL_AUDIO
    (0x080, 0x05F5_E100, false, false), // PLL_AUDIO_NUM
    (0x090, 0x2964_619C, false, false), // PLL_AUDIO_DENOM
    (0x0A0, 0x0001_100C, true, false),  // PLL_VIDEO
    (0x0B0, 0x05F5_E100, false, false), // PLL_VIDEO_NUM
    (0x0C0, 0x10A2_4447, false, false), // PLL_VIDEO_DENOM
    (0x0E0, 0x0001_1001, true, false),  // PLL_ENET
    (0x0F0, 0x1311_100C, true, false),  // PFD_480
    (0x100, 0x1018_101B, true, false),  // PFD_528
    (0x110, 0x0000_1073, true, false),  // PMU_REG_1P1
    (0x120, 0x0000_0F74, true, false),  // PMU_REG_3P0
    (0x130, 0x0000_1073, true, false),  // PMU_REG_2P5
    (0x140, 0x0048_2012, true, false),  // PMU_REG_CORE
    (0x150, 0x0400_0000, true, false),  // MISC0
    (0x160, 0x0000_0000, true, false),  // MISC1
    (0x170, 0x0027_2727, true, false),  // MISC2
    (0x180, 0x0000_0001, true, false),  // TEMPMON_TEMPSENSE0
    (0x190, 0x0000_0001, true, false),  // TEMPMON_TEMPSENSE1
    (0x1A0, 0x0010_0004, true, false),  // USB1_VBUS_DETECT
    (0x1B0, 0x0000_0000, true, false),  // USB1_CHRG_DETECT
    (0x1C0, 0x0000_0000, false, true),  // USB1_VBUS_DETECT_STAT
    (0x1D0, 0x0000_0000, false, true),  // USB1_CHRG_DETECT_STAT
    (0x1E0, 0x0000_0000, true, false),  // USB1_LOOPBACK
    (0x1F0, 0x0000_0002, true, false),  // USB1_MISC
    (0x200, 0x0010_0004, true, false),  // USB2_VBUS_DETECT
    (0x210, 0x0000_0000, true, false),  // USB2_CHRG_DETECT
    (0x220, 0x0000_0000, false, true),  // USB2_VBUS_DETECT_STAT
    (0x230, 0x0000_0000, false, true),  // USB2_CHRG_DETECT_STAT
    (0x240, 0x0000_0000, true, false),  // USB2_LOOPBACK
    (0x250, 0x0000_0002, true, false),  // USB2_MISC
    (0x260, 0x006C_0000, false, true),  // DIGPROG (silicon revision)
    (0x270, 0x0000_4001, true, false),  // XTALOSC24M_LOWPWR_CTRL
    (0x290, 0x0000_0000, true, false),  // TEMPMON_TEMPSENSE2
    (0x2A0, 0x0000_1020, true, false),  // XTALOSC24M_OSC_CONFIG0
    (0x2B0, 0x0000_02EE, true, false),  // XTALOSC24M_OSC_CONFIG1
    (0x2C0, 0x0001_02E2, true, false),  // XTALOSC24M_OSC_CONFIG2
];

const WINDOW: usize = 0x2D0 / 4;

const PLL_ARM: u32 = 0x000;
const PLL_USB1: u32 = 0x010;
const PLL_USB2: u32 = 0x020;
const PLL_SYS: u32 = 0x030;
const PLL_AUDIO: u32 = 0x070;
const PLL_VIDEO: u32 = 0x0A0;
const PLL_ENET: u32 = 0x0E0;
const REG_1P1: u32 = 0x110;
const REG_3P0: u32 = 0x120;
const REG_2P5: u32 = 0x130;
const MISC0: u32 = 0x150;
const TEMPSENSE0: u32 = 0x180;
const USB1_VBUS_DETECT_STAT: u32 = 0x1C0;
const USB2_VBUS_DETECT_STAT: u32 = 0x220;
const LOWPWR_CTRL: u32 = 0x270;

const LOCK: u32 = 1 << 31;
const OK_LDO: u32 = 1 << 17;
const MISC0_OSC_XTALOK: u32 = 1 << 15;
const MISC0_OSC_XTALOK_EN: u32 = 1 << 16;
const MISC0_XTAL_24M_PWD: u32 = 1 << 30;
const LOWPWR_XTALOSC_PWRUP_STAT: u32 = 1 << 16;
const TEMP_POWER_DOWN: u32 = 1 << 0;
const TEMP_MEASURE: u32 = 1 << 1;
const TEMP_FINISHED: u32 = 1 << 2;

/// The PLLs: (register, "powered" predicate over the control word, the bits
/// whose change forces a re-lock).
/// (register, "powered" predicate, re-lock field mask)
type PllSpec = (u32, fn(u32) -> bool, u32);

const PLLS: &[PllSpec] = &[
    (PLL_ARM, |v| v & (1 << 12) == 0, 0x7F),
    (PLL_USB1, |v| v & (1 << 12) != 0, 0x2),
    (PLL_USB2, |v| v & (1 << 12) != 0, 0x2),
    (PLL_SYS, |v| v & (1 << 12) == 0, 0x1),
    (PLL_AUDIO, |v| v & (1 << 12) == 0, 0x7F),
    (PLL_VIDEO, |v| v & (1 << 12) == 0, 0x7F),
    (PLL_ENET, |v| v & (1 << 12) == 0, 0xF),
];

#[derive(Debug)]
pub struct ImxrtAnadig {
    regs: [u32; WINDOW],
    /// Cycle at which each timed status bit becomes (or became) true;
    /// `None` while its enabling condition is false. Keyed by register.
    ready_at: std::collections::BTreeMap<u32, u64>,
    time: Timebase,
    /// VBUS presence reported by `USBx_VBUS_DETECT_STAT` (board-level: the
    /// FB200 is USB powered, so VBUS is present on USB1).
    vbus: [bool; 2],
}

impl Default for ImxrtAnadig {
    fn default() -> Self {
        Self::new()
    }
}

fn reg_info(off: u32) -> Option<(u32, bool, bool)> {
    REGS.iter()
        .find(|(o, ..)| *o == off)
        .map(|&(_, rst, sct, ro)| (rst, sct, ro))
}

impl ImxrtAnadig {
    pub fn new() -> Self {
        let mut regs = [0u32; WINDOW];
        for &(off, rst, _, _) in REGS {
            regs[(off / 4) as usize] = rst;
        }
        let mut s = Self {
            regs,
            ready_at: Default::default(),
            time: Timebase::default(),
            vbus: [true, false],
        };
        // Everything the boot ROM leaves running is already settled at t=0:
        // the reset-value PLLs that are powered (none but the USB1 PLL the
        // ROM used, which reset powers down again), the enabled LDOs and the
        // 24 MHz oscillator.
        for off in [REG_1P1, REG_3P0, REG_2P5, MISC0, LOWPWR_CTRL] {
            s.rearm(off, 0, true);
        }
        for &(off, powered, _) in PLLS {
            if powered(s.regs[(off / 4) as usize]) {
                s.ready_at.insert(off, 0);
            }
        }
        s
    }

    /// Set whether VBUS is present on USB1 (index 0) / USB2 (index 1).
    pub fn set_vbus(&mut self, port: usize, present: bool) {
        if port < 2 {
            self.vbus[port] = present;
        }
    }

    fn get(&self, off: u32) -> u32 {
        self.regs.get((off / 4) as usize).copied().unwrap_or(0)
    }

    /// Re-evaluate the enable condition of a timed status bit after a
    /// control write. `settled` starts it already complete (reset state).
    fn rearm(&mut self, off: u32, old: u32, settled: bool) {
        let v = self.get(off);
        let now = if settled { 0 } else { self.time.now() };
        match off {
            REG_1P1 | REG_3P0 | REG_2P5 => {
                if v & 1 != 0 {
                    if old & 1 == 0 || settled {
                        self.ready_at
                            .insert(off, now + if settled { 0 } else { self.time.us(LDO_OK_US) });
                    }
                } else {
                    self.ready_at.remove(&off);
                }
            }
            MISC0 | LOWPWR_CTRL => {
                // The crystal: powered while MISC0.XTAL_24M_PWD = 0.
                let misc0 = self.get(MISC0);
                if misc0 & MISC0_XTAL_24M_PWD == 0 {
                    if !self.ready_at.contains_key(&MISC0) {
                        let t = if settled {
                            0
                        } else {
                            now + self.time.us(XTAL_UP_US)
                        };
                        self.ready_at.insert(MISC0, t);
                    }
                } else {
                    self.ready_at.remove(&MISC0);
                }
            }
            TEMPSENSE0 => {
                if v & TEMP_MEASURE != 0 && v & TEMP_POWER_DOWN == 0 {
                    if old & TEMP_MEASURE == 0 || old & TEMP_POWER_DOWN != 0 {
                        self.ready_at
                            .insert(off, now + self.time.us(TEMP_MEASURE_US));
                    }
                } else if v & TEMP_MEASURE == 0 {
                    // A new measurement starts from a clean FINISHED.
                    self.ready_at.remove(&off);
                }
            }
            _ => {
                if let Some(&(_, powered, relock)) = PLLS.iter().find(|(o, ..)| *o == off) {
                    if powered(v) {
                        if !powered(old)
                            || (old ^ v) & relock != 0
                            || !self.ready_at.contains_key(&off)
                        {
                            self.ready_at.insert(off, now + self.time.us(PLL_LOCK_US));
                        }
                    } else {
                        self.ready_at.remove(&off);
                    }
                }
            }
        }
    }

    fn ready(&self, off: u32) -> bool {
        self.ready_at
            .get(&off)
            .is_some_and(|&t| self.time.now() >= t)
    }

    /// The register value the CPU reads, status bits included.
    pub fn read_reg(&self, off: u32) -> u32 {
        // Aliases read back the base register.
        let base = off & !0xF;
        let alias = off & 0xF;
        let Some((_, sct, _)) = reg_info(base) else {
            return 0;
        };
        if alias != 0 && !sct {
            return 0;
        }
        let v = self.get(base);
        match base {
            PLL_ARM | PLL_USB1 | PLL_USB2 | PLL_SYS | PLL_AUDIO | PLL_VIDEO | PLL_ENET => {
                (v & !LOCK) | if self.ready(base) { LOCK } else { 0 }
            }
            REG_1P1 | REG_3P0 | REG_2P5 => {
                (v & !OK_LDO) | if self.ready(base) { OK_LDO } else { 0 }
            }
            MISC0 => {
                let ok = self.ready(MISC0) && v & MISC0_OSC_XTALOK_EN != 0;
                (v & !MISC0_OSC_XTALOK) | if ok { MISC0_OSC_XTALOK } else { 0 }
            }
            LOWPWR_CTRL => {
                (v & !LOWPWR_XTALOSC_PWRUP_STAT)
                    | if self.ready(MISC0) {
                        LOWPWR_XTALOSC_PWRUP_STAT
                    } else {
                        0
                    }
            }
            TEMPSENSE0 => {
                if self.ready(TEMPSENSE0) {
                    // TEMP_CNT for ~25 C with the OCOTP ANA1 defaults the
                    // TEMPMON driver reads (hot 105 C count 0x56, room 25 C
                    // count ~ 0x5C). Firmware converts through the fuses.
                    (v & !(0xFFF << 8)) | TEMP_FINISHED | (0x5C << 8)
                } else {
                    v & !TEMP_FINISHED
                }
            }
            USB1_VBUS_DETECT_STAT => vbus_stat(self.vbus[0]),
            USB2_VBUS_DETECT_STAT => vbus_stat(self.vbus[1]),
            _ => v,
        }
    }

    /// A CPU write of `value` under byte-lane `mask`.
    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        let base = off & !0xF;
        let alias = off & 0xF;
        let Some((_, sct, ro)) = reg_info(base) else {
            return;
        };
        if ro || (alias != 0 && !sct) {
            return;
        }
        let old = self.get(base);
        let v = value & mask;
        let new = match alias {
            0x0 => (old & !mask) | v,
            0x4 => old | v,
            0x8 => old & !v,
            0xC => old ^ v,
            _ => return,
        };
        self.regs[(base / 4) as usize] = new;
        self.rearm(base, old, false);
    }
}

fn vbus_stat(present: bool) -> u32 {
    if present {
        // VBUS_VALID | AVALID | BVALID
        (1 << 3) | (1 << 2) | (1 << 1)
    } else {
        1 // SESSEND
    }
}

impl Peripheral for ImxrtAnadig {
    fn read(&self, offset: u64) -> SimResult<u8> {
        Ok(byte_of(self.read_reg((offset & !3) as u32), offset))
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        self.write_reg((offset & !3) as u32, (value as u32) << shift, 0xFF << shift);
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
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "peripheral": "imxrt_anadig",
            "pll_arm": self.read_reg(PLL_ARM),
            "pll_sys": self.read_reg(PLL_SYS),
            "pll_usb1": self.read_reg(PLL_USB1),
            "pll_audio": self.read_reg(PLL_AUDIO),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleClock;

    fn with_clock() -> (ImxrtAnadig, CycleClock) {
        let mut a = ImxrtAnadig::new();
        let c = CycleClock::default();
        a.attach_cycle_clock(c.clone());
        (a, c)
    }

    #[test]
    fn arm_pll_locks_after_power_up_and_unlocks_on_power_down() {
        let (mut a, c) = with_clock();
        // Reset: POWERDOWN=1, not locked.
        assert_eq!(a.read_reg(PLL_ARM) & LOCK, 0);
        // SDK CLOCK_InitArmPll: clear POWERDOWN, DIV_SELECT=100.
        a.write_reg(PLL_ARM, (1 << 13) | 100, u32::MAX);
        assert_eq!(a.read_reg(PLL_ARM) & LOCK, 0, "lock is not instant");
        c.publish(a.time.us(PLL_LOCK_US) - 1);
        assert_eq!(a.read_reg(PLL_ARM) & LOCK, 0);
        c.publish(a.time.us(PLL_LOCK_US));
        assert_ne!(a.read_reg(PLL_ARM) & LOCK, 0);
        // Loop divider change re-locks.
        a.write_reg(PLL_ARM, (1 << 13) | 88, u32::MAX);
        assert_eq!(a.read_reg(PLL_ARM) & LOCK, 0);
        // Power down through the SET alias.
        a.write_reg(PLL_ARM + 4, 1 << 12, u32::MAX);
        c.publish(1_000_000_000);
        assert_eq!(a.read_reg(PLL_ARM) & LOCK, 0);
    }

    #[test]
    fn usb_pll_power_bit_is_active_high() {
        let (mut a, c) = with_clock();
        a.write_reg(PLL_USB1 + 4, (1 << 12) | (1 << 13), u32::MAX);
        c.publish(a.time.us(PLL_LOCK_US));
        assert_ne!(a.read_reg(PLL_USB1) & LOCK, 0);
        assert_eq!(a.read_reg(PLL_USB2) & LOCK, 0, "USB2 PLL still off");
    }

    #[test]
    fn set_clr_tog_aliases() {
        let (mut a, _) = with_clock();
        a.write_reg(0x164, 0x3 << 16, u32::MAX); // MISC1_SET
        assert_eq!(a.read_reg(0x160), 0x3 << 16);
        a.write_reg(0x168, 1 << 16, u32::MAX); // MISC1_CLR
        assert_eq!(a.read_reg(0x160), 1 << 17);
        a.write_reg(0x16C, 0x3 << 16, u32::MAX); // MISC1_TOG
        assert_eq!(a.read_reg(0x160), 1 << 16);
        // DENOM has no aliases.
        a.write_reg(0x064, 0xFFFF, u32::MAX);
        assert_eq!(a.read_reg(0x060), 0x12);
    }

    #[test]
    fn xtal_ok_needs_the_enable_bit() {
        let (mut a, _) = with_clock();
        assert_eq!(a.read_reg(MISC0) & MISC0_OSC_XTALOK, 0);
        assert_ne!(a.read_reg(LOWPWR_CTRL) & LOWPWR_XTALOSC_PWRUP_STAT, 0);
        a.write_reg(MISC0 + 4, MISC0_OSC_XTALOK_EN, u32::MAX);
        assert_ne!(a.read_reg(MISC0) & MISC0_OSC_XTALOK, 0);
    }
}
