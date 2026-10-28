// SPDX-License-Identifier: MIT
//! Opt-in SAMD51 OSCCTRL slice (DS60001507F 28): the reset-enabled DFLL48M and
//! the two DPLLs referenced from a generic clock channel. Lock is reached only
//! once the DPLL's reference channel (GCLK PCHCTRL[1 + n]) is enabled; lock time
//! and jitter are not modelled. The DFLL's USB clock recovery (DFLLCTRLB MODE
//! and USBCRM, DFLLMUL) is admitted: `usb_clock_recovery` reports it, which the
//! USB model uses to decide whether its bit clock follows the host's start of
//! frame. Crystal oscillators, closed loop on a GCLK reference, interrupts and
//! events are refused.
use crate::{Bus, Peripheral, SimResult, SimulationError};
use std::any::Any;

const GCLK_PCHCTRL: u64 = 0x40001c80;
const CHEN: u32 = 1 << 6;
const DFLL_CTRLA: u64 = 0x1c;
const DFLL_CTRLB: u64 = 0x20;
const DFLL_VAL: u64 = 0x24;
const DFLL_MUL: u64 = 0x28;
const DFLL_SYNC: u64 = 0x2c;
const DFLLCTRLB_MODE: u8 = 1;
const DFLLCTRLB_USBCRM: u8 = 1 << 3;
/// DFLLVAL after reset: the calibration NVM loads (COARSE and FINE midpoints).
const DFLLVAL_RESET: u32 = 0x1f << 10 | 0x80;
const DPLL_BASE: u64 = 0x30;
const DPLL_STRIDE: u64 = 0x14;
const DPLL_ENABLE: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, Default)]
struct Dpll {
    ctrla: u8,
    ratio: u32,
    ctrlb: u32,
    locked: bool,
}

#[derive(Debug)]
pub struct Samd51Oscctrl {
    dfll_ctrla: u8,
    dfll_ctrlb: u8,
    dfll_val: u32,
    dfll_mul: u32,
    dpll: [Dpll; 2],
}

impl Default for Samd51Oscctrl {
    fn default() -> Self {
        // DFLLCTRLA reset value 0x82: ENABLE | ONDEMAND.
        Self {
            dfll_ctrla: 0x82,
            dfll_ctrlb: 0,
            dfll_val: DFLLVAL_RESET,
            dfll_mul: 0,
            dpll: [Dpll::default(); 2],
        }
    }
}

impl Samd51Oscctrl {
    pub fn new() -> Self {
        Self::default()
    }
    fn refuse<T>(what: &str) -> SimResult<T> {
        Err(SimulationError::NotImplemented(format!(
            "samd51 OSCCTRL slice: {what}"
        )))
    }
    fn dpll_index(offset: u64) -> Option<(usize, u64)> {
        let relative = offset.checked_sub(DPLL_BASE)?;
        let index = (relative / DPLL_STRIDE) as usize;
        (index < 2).then_some((index, relative % DPLL_STRIDE))
    }
    /// LDR + 1 + LDRFRAC / 32, as a rational over 32.
    pub fn dpll_ratio_x32(&self, index: usize) -> Option<u64> {
        let d = self.dpll[index];
        d.locked.then(|| {
            let ldr = u64::from(d.ratio & 0x1fff);
            let frac = u64::from((d.ratio >> 16) & 0x1f);
            (ldr + 1) * 32 + frac
        })
    }
    pub fn dfll_enabled(&self) -> bool {
        self.dfll_ctrla & 2 != 0
    }
    /// The DFLL48M in USB clock recovery (DS60001507 28.6.4): enabled, closed
    /// loop (MODE) on the start of frame (USBCRM), 48000 output periods per
    /// 1 ms SOF (DFLLMUL.MUL = 0xBB80).
    pub fn usb_clock_recovery(&self) -> bool {
        self.dfll_enabled()
            && self.dfll_ctrlb & (DFLLCTRLB_MODE | DFLLCTRLB_USBCRM)
                == DFLLCTRLB_MODE | DFLLCTRLB_USBCRM
            && self.dfll_mul & 0xffff == 48_000
    }
    fn status(&self) -> u32 {
        let mut s = 0;
        if self.dfll_enabled() {
            s |= 1 << 8; // DFLLRDY
        }
        for (i, d) in self.dpll.iter().enumerate() {
            if d.locked {
                s |= 1 << (16 + 8 * i); // DPLLnLCKR
            }
        }
        s
    }
    fn read_reg(&self, offset: u64) -> SimResult<u32> {
        if let Some((n, reg)) = Self::dpll_index(offset) {
            let d = self.dpll[n];
            return Ok(match reg {
                0x0 => u32::from(d.ctrla),
                0x4 => d.ratio,
                0x8 => d.ctrlb,
                0xc => 0, // DPLLSYNCBUSY: writes complete immediately
                0x10 => u32::from(d.locked) * 0b11, // LOCK | CLKRDY
                _ => return Self::refuse("DPLL register"),
            });
        }
        match offset {
            0x10 => Ok(self.status()),
            DFLL_CTRLA => Ok(u32::from(self.dfll_ctrla)),
            DFLL_CTRLB => Ok(u32::from(self.dfll_ctrlb)),
            DFLL_VAL => Ok(self.dfll_val),
            DFLL_MUL => Ok(self.dfll_mul),
            DFLL_SYNC => Ok(0), // DFLLSYNC: writes complete immediately
            0x0..=0x0c | 0x14..=0x18 => Ok(0),
            _ => Self::refuse("register"),
        }
    }
}

impl Peripheral for Samd51Oscctrl {
    fn on_system_reset(&mut self) {
        *self = Self::new();
    }
    fn read(&self, offset: u64) -> SimResult<u8> {
        let word = self.read_reg(offset & !3)?;
        Ok((word >> (8 * (offset & 3))) as u8)
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        if let Some((_, 0)) = Self::dpll_index(offset) {
            return self.write_u32(offset, u32::from(value));
        }
        // DFLLCTRLA, DFLLCTRLB and DFLLSYNC are 8-bit registers.
        if matches!(offset, DFLL_CTRLA | DFLL_CTRLB | DFLL_SYNC) {
            return self.write_u32(offset, u32::from(value));
        }
        Self::refuse("byte store")
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        if let Some((n, reg)) = Self::dpll_index(offset) {
            let d = &mut self.dpll[n];
            match reg {
                0x0 if value & !u32::from(DPLL_ENABLE | 0xc0) == 0 => {
                    d.ctrla = value as u8;
                    if d.ctrla & DPLL_ENABLE == 0 {
                        d.locked = false;
                    }
                }
                0x4 if value & !0x001f_1fff == 0 => d.ratio = value,
                // REFCLK must select GCLK (0); filters, wake-up and lock bypass stay default.
                0x8 if value == 0 => d.ctrlb = value,
                _ => return Self::refuse("DPLL configuration outside the GCLK-referenced slice"),
            }
            return Ok(());
        }
        match offset {
            DFLL_CTRLA if value == 0x82 || value == 0x02 => {
                self.dfll_ctrla = value as u8;
                Ok(())
            }
            // Closed loop only on the USB start of frame; a GCLK reference
            // (MODE without USBCRM) is not modelled.
            DFLL_CTRLB
                if value & !0xff == 0
                    && value as u8 & (DFLLCTRLB_MODE | DFLLCTRLB_USBCRM) != DFLLCTRLB_MODE =>
            {
                self.dfll_ctrlb = value as u8;
                Ok(())
            }
            DFLL_MUL => {
                self.dfll_mul = value;
                Ok(())
            }
            DFLL_VAL => {
                self.dfll_val = value;
                Ok(())
            }
            DFLL_SYNC if value & !0x80 == 0 => Ok(()), // READREQ
            _ => Self::refuse("unadmitted store"),
        }
    }
    fn peek(&self, offset: u64) -> Option<u8> {
        self.read(offset).ok()
    }
    fn needs_bus_tick(&self) -> bool {
        self.dpll
            .iter()
            .any(|d| d.ctrla & DPLL_ENABLE != 0 && !d.locked)
    }
    fn idle_poll_bus_tick(&self) -> bool {
        self.needs_bus_tick()
    }
    fn tick_with_bus(&mut self, bus: &mut dyn Bus) {
        for (n, d) in self.dpll.iter_mut().enumerate() {
            if d.ctrla & DPLL_ENABLE != 0 && !d.locked {
                let channel = bus.read_u32(GCLK_PCHCTRL + 4 * (1 + n as u64)).unwrap_or(0);
                d.locked = channel & CHEN != 0;
            }
        }
    }
    fn legacy_tick_active(&self) -> bool {
        // No work lives in tick/tick_elapsed. Bus service and pending operations
        // remain independently mandatory and prohibit quiescent chunks.
        false
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"peripheral": "samd51_oscctrl", "dfll_ctrla": self.dfll_ctrla,
            "dpll_locked": [self.dpll[0].locked, self.dpll[1].locked],
            "dpll_ratio": [self.dpll[0].ratio, self.dpll[1].ratio]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpll_locks_only_with_an_enabled_reference_channel() {
        let mut o = Samd51Oscctrl::new();
        assert_eq!(o.read_u32(0x10).unwrap() & (1 << 8), 1 << 8);
        o.write_u32(0x34, 119).unwrap();
        o.write_u32(0x38, 0).unwrap();
        o.write_u32(0x30, 2).unwrap();
        assert!(o.needs_bus_tick());
        assert_eq!(o.read_u32(0x40).unwrap(), 0);
        assert_eq!(o.dpll_ratio_x32(0), None);
        o.dpll[0].locked = true; // as tick_with_bus does once PCHCTRL[1].CHEN is set
        assert_eq!(o.read_u32(0x40).unwrap(), 3);
        assert_eq!(o.dpll_ratio_x32(0), Some(120 * 32));
        o.write_u32(0x30, 0).unwrap();
        assert_eq!(o.read_u32(0x40).unwrap(), 0);
    }

    #[test]
    fn usb_clock_recovery_needs_mode_usbcrm_and_the_sof_multiplier() {
        let mut o = Samd51Oscctrl::new();
        assert!(!o.usb_clock_recovery(), "open loop after reset");
        // USBCRM without MODE (the ASF and Arduino settings) stays open loop.
        o.write(0x20, 0x98).unwrap(); // WAITLOCK | CCDIS | USBCRM
        o.write_u32(0x28, 48_000).unwrap();
        assert!(!o.usb_clock_recovery());
        o.write(0x20, 0x19).unwrap(); // CCDIS | USBCRM | MODE
        assert!(o.usb_clock_recovery());
        o.write_u32(0x28, 0).unwrap();
        assert!(
            !o.usb_clock_recovery(),
            "MUL must count 48000 periods per SOF"
        );
        assert!(
            o.write(0x20, 0x01).is_err(),
            "closed loop on a GCLK reference"
        );
    }

    #[test]
    fn unadmitted_configuration_is_refused() {
        let mut o = Samd51Oscctrl::new();
        assert!(o.write_u32(0x38, 1 << 5).is_err(), "XOSC32 reference");
        assert!(o.write_u32(0x14, 2).is_err(), "crystal oscillator");
        assert!(o.write_u32(0x34, 1 << 31).is_err(), "reserved ratio bits");
        assert!(o.read(0x80).is_err());
    }
}
