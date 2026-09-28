// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 Clock Controller Module (CCM, `0x400F_C000`, IMXRT1060RM §14).
//!
//! A register file with the SVD reset values plus the two things firmware
//! waits on:
//! * `CDHIPR` divider/mux handshake: changing `CBCDR.SEMC_PODF`,
//!   `CBCDR.AHB_PODF`, `CBCDR.PERIPH_CLK_SEL` or `CACRR.ARM_PODF` sets the
//!   matching BUSY bit, which clears after `HANDSHAKE_CYCLES` and latches the
//!   `*_LOADED` bit in `CISR` (write-1-to-clear).
//! * `CSR.COSC_READY` follows `CCR.COSC_EN` after the oscillator start-up
//!   count (`CCR.OSCNT` periods of the 32 kHz clock).
//!
//! Clock gates (`CCGR0..7`) are plain storage: the simulator does not stall
//! the bus on an access to a gated peripheral (silicon hangs the AHB).

use super::{byte_of, Timebase};
use crate::{Peripheral, SimResult};
use std::any::Any;

/// Divider/mux handshake latency in core cycles (a few cycles of the slower
/// of the two clock domains; RM §14.5.1.2).
const HANDSHAKE_CYCLES: u64 = 64;

const CCR: u32 = 0x00;
const CSR: u32 = 0x08;
const CACRR: u32 = 0x10;
const CBCDR: u32 = 0x14;
const CDHIPR: u32 = 0x48;
const CISR: u32 = 0x58;

/// (offset, SVD reset value, read-only)
const REGS: &[(u32, u32, bool)] = &[
    (0x00, 0x0401_107F, false), // CCR
    (0x08, 0x0000_0010, true),  // CSR
    (0x0C, 0x0000_0100, false), // CCSR
    (0x10, 0x0000_0001, false), // CACRR
    (0x14, 0x000A_8300, false), // CBCDR
    (0x18, 0x2DAE_8324, false), // CBCMR
    (0x1C, 0x0490_0000, false), // CSCMR1
    (0x20, 0x1319_2F06, false), // CSCMR2
    (0x24, 0x0649_0B00, false), // CSCDR1
    (0x28, 0x0EC1_02C1, false), // CS1CDR
    (0x2C, 0x0073_36C1, false), // CS2CDR
    (0x30, 0x33F7_1F92, false), // CDCDR
    (0x38, 0x0002_9150, false), // CSCDR2
    (0x3C, 0x0003_0841, false), // CSCDR3
    (0x48, 0x0000_0000, true),  // CDHIPR
    (0x54, 0x0000_0079, false), // CLPCR
    (0x58, 0x0000_0000, false), // CISR (w1c)
    (0x5C, 0xFFFF_FFFF, false), // CIMR
    (0x60, 0x000A_0001, false), // CCOSR
    (0x64, 0x0000_FE62, false), // CGPR
    (0x68, 0xFFFF_FFFF, false), // CCGR0
    (0x6C, 0xFFFF_FFFF, false), // CCGR1
    (0x70, 0xFC3F_FFFF, false), // CCGR2
    (0x74, 0xFFFF_FFCF, false), // CCGR3
    (0x78, 0xFFFF_FFFF, false), // CCGR4
    (0x7C, 0xFFFF_FFFF, false), // CCGR5
    (0x80, 0xFFFF_FFFF, false), // CCGR6
    (0x84, 0xFFFF_FFFF, false), // CCGR7
    (0x88, 0xFFFF_FFFF, false), // CMEOR
];

/// (register, field mask, CDHIPR busy bit, CISR loaded bit)
const HANDSHAKES: &[(u32, u32, u32, u32)] = &[
    (CBCDR, 0x7 << 16, 1 << 0, 1 << 17), // SEMC_PODF
    (CBCDR, 0x7 << 10, 1 << 1, 1 << 20), // AHB_PODF
    (CBCDR, 1 << 25, 1 << 5, 1 << 22),   // PERIPH_CLK_SEL
    (CACRR, 0x7, 1 << 16, 1 << 26),      // ARM_PODF
];

const COSC_EN: u32 = 1 << 12;
const COSC_READY_CSR: u32 = 1 << 5;
const COSC_READY_CISR: u32 = 1 << 6;

#[derive(Debug)]
pub struct ImxrtCcm {
    regs: [u32; 0x8C / 4 + 1],
    /// Pending handshakes: (busy bit, loaded bit, done-at cycle).
    busy: std::cell::RefCell<Vec<(u32, u32, u64)>>,
    /// CISR bits already latched from completed handshakes.
    cisr_latched: std::cell::Cell<u32>,
    cosc_ready_at: Option<u64>,
    time: Timebase,
}

impl Default for ImxrtCcm {
    fn default() -> Self {
        Self::new()
    }
}

impl ImxrtCcm {
    pub fn new() -> Self {
        let mut regs = [0u32; 0x8C / 4 + 1];
        for &(off, rst, _) in REGS {
            regs[(off / 4) as usize] = rst;
        }
        Self {
            regs,
            busy: std::cell::RefCell::new(Vec::new()),
            cisr_latched: std::cell::Cell::new(0),
            // COSC_EN is set at reset and the ROM already waited for it.
            cosc_ready_at: Some(0),
            time: Timebase::default(),
        }
    }

    fn known(off: u32) -> Option<bool> {
        REGS.iter().find(|(o, ..)| *o == off).map(|&(_, _, ro)| ro)
    }

    /// Fold completed handshakes into CISR (read side, `&self`).
    fn settle(&self) {
        let now = self.time.now();
        let mut loaded = 0;
        self.busy.borrow_mut().retain(|&(_, l, t)| {
            if now >= t {
                loaded |= l;
                false
            } else {
                true
            }
        });
        if loaded != 0 {
            self.cisr_latched.set(self.cisr_latched.get() | loaded);
        }
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        let off = off & !3;
        if Self::known(off).is_none() {
            return 0;
        }
        let now = self.time.now();
        match off {
            CDHIPR => self
                .busy
                .borrow()
                .iter()
                .filter(|&&(_, _, t)| now < t)
                .fold(0, |acc, &(b, _, _)| acc | b),
            CSR => {
                let v = self.regs[(CSR / 4) as usize] & !COSC_READY_CSR;
                if self.cosc_ready_at.is_some_and(|t| now >= t) {
                    v | COSC_READY_CSR
                } else {
                    v
                }
            }
            CISR => {
                self.settle();
                let mut v = self.regs[(CISR / 4) as usize] | self.cisr_latched.get();
                if self.cosc_ready_at.is_some_and(|t| now >= t) && self.cisr_cosc_pending() {
                    v |= COSC_READY_CISR;
                }
                v
            }
            _ => self.regs[(off / 4) as usize],
        }
    }

    fn cisr_cosc_pending(&self) -> bool {
        self.regs[(CISR / 4) as usize] & COSC_READY_CISR != 0
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        let off = off & !3;
        match Self::known(off) {
            None | Some(true) => return,
            Some(false) => {}
        }
        let idx = (off / 4) as usize;
        let old = self.regs[idx];
        if off == CISR {
            // Write-1-to-clear.
            self.settle();
            let clr = value & mask;
            self.cisr_latched.set(self.cisr_latched.get() & !clr);
            self.regs[idx] &= !clr;
            return;
        }
        let new = (old & !mask) | (value & mask);
        self.regs[idx] = new;
        let now = self.time.now();
        self.settle();
        for &(reg, field, busy, loaded) in HANDSHAKES {
            if reg == off && (old ^ new) & field != 0 {
                self.busy
                    .borrow_mut()
                    .push((busy, loaded, now + HANDSHAKE_CYCLES));
            }
        }
        if off == CCR && (old ^ new) & COSC_EN != 0 {
            if new & COSC_EN != 0 {
                // OSCNT periods of the 32.768 kHz clock: (OSCNT+1) * ~30.5 µs.
                let oscnt = (new & 0xFF) as u64 + 1;
                self.cosc_ready_at = Some(now + self.time.us(oscnt * 31));
                self.regs[(CISR / 4) as usize] |= 0; // latched on completion
            } else {
                self.cosc_ready_at = None;
            }
        }
    }
}

impl Peripheral for ImxrtCcm {
    fn read(&self, offset: u64) -> SimResult<u8> {
        Ok(byte_of(self.read_reg(offset as u32), offset))
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        self.write_reg(offset as u32, (value as u32) << shift, 0xFF << shift);
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
            "peripheral": "imxrt_ccm",
            "cbcdr": self.read_reg(CBCDR),
            "cacrr": self.read_reg(CACRR),
            "cdhipr": self.read_reg(CDHIPR),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleClock;

    #[test]
    fn periph_clk_sel_handshake_goes_busy_then_done() {
        let mut ccm = ImxrtCcm::new();
        let c = CycleClock::default();
        ccm.attach_cycle_clock(c.clone());
        c.publish(1000);
        assert_eq!(ccm.read_reg(CDHIPR), 0);
        let v = ccm.read_reg(CBCDR) | (1 << 25);
        ccm.write_reg(CBCDR, v, u32::MAX);
        assert_eq!(ccm.read_reg(CDHIPR), 1 << 5, "PERIPH_CLK_SEL_BUSY");
        c.publish(1000 + HANDSHAKE_CYCLES);
        assert_eq!(ccm.read_reg(CDHIPR), 0);
        assert_ne!(ccm.read_reg(CISR) & (1 << 22), 0, "PERIPH_CLK_SEL_LOADED");
        ccm.write_reg(CISR, 1 << 22, u32::MAX);
        assert_eq!(ccm.read_reg(CISR) & (1 << 22), 0, "w1c");
        // Writing the same value again is not a change.
        ccm.write_reg(CBCDR, v, u32::MAX);
        assert_eq!(ccm.read_reg(CDHIPR), 0);
    }

    #[test]
    fn reset_values_and_read_only_csr() {
        let mut ccm = ImxrtCcm::new();
        assert_eq!(ccm.read_reg(0x18), 0x2DAE_8324);
        assert_ne!(ccm.read_reg(CSR) & COSC_READY_CSR, 0);
        ccm.write_reg(CSR, 0, u32::MAX);
        assert_ne!(ccm.read_reg(CSR) & COSC_READY_CSR, 0);
    }
}
