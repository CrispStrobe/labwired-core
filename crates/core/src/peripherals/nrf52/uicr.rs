// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Nordic nRF52 UICR (User Information Configuration Registers).
//!
//! Source: nRF52840 PS rev 1.7 §6.33 (UICR). Customer-programmable
//! one-time configuration: APPROTECT, customer regs, PSELRESET, NFCPINS,
//! REGOUT0. Erased value is 0xFFFFFFFF for every field; the NVMC programs
//! it (single-write per bit, like flash). We model it as a stateful
//! peripheral where writes are masked (0→1 transitions are dropped, only
//! 1→0 takes effect), matching real flash semantics.

use crate::{Peripheral, SimResult};

const OFF_NRFFW_FIRST: u64 = 0x014;
const OFF_NRFFW_LAST: u64 = 0x050;
const OFF_NRFHW_FIRST: u64 = 0x050;
const OFF_NRFHW_LAST: u64 = 0x07C;
const OFF_CUSTOMER_FIRST: u64 = 0x080;
const OFF_CUSTOMER_LAST: u64 = 0x0FC;
const OFF_PSELRESET0: u64 = 0x200;
const OFF_PSELRESET1: u64 = 0x204;
const OFF_APPROTECT: u64 = 0x208;
const OFF_NFCPINS: u64 = 0x20C;
const OFF_DEBUGCTRL: u64 = 0x210;
const OFF_REGOUT0: u64 = 0x304;

const ERASED: u32 = 0xFFFF_FFFF;

#[derive(Debug)]
pub struct Nrf52Uicr {
    customer: [u32; 32], // 0x080..0x100, 32 words
    nrffw: [u32; 16],
    nrfhw: [u32; 12],
    pselreset: [u32; 2],
    approtect: u32,
    nfcpins: u32,
    debugctrl: u32,
    regout0: u32,
}

impl Default for Nrf52Uicr {
    fn default() -> Self {
        Self {
            customer: [ERASED; 32],
            nrffw: [ERASED; 16],
            nrfhw: [ERASED; 12],
            pselreset: [ERASED; 2],
            approtect: ERASED,
            // NFCPINS: 0xFFFFFFFE on this bench board — NFC pins are configured
            // (bit 0 cleared = use P0.09/P0.10 as NFC antenna, not GPIO).
            // Confirmed by live silicon read on the DK board used for hw-oracle tests.
            nfcpins: 0xFFFF_FFFE,
            debugctrl: ERASED,
            regout0: ERASED,
        }
    }
}

impl Nrf52Uicr {
    pub fn new() -> Self {
        Self::default()
    }

    /// Flash-write semantics: bits can only transition 1 → 0. A write
    /// effectively ANDs the new value with the current value.
    fn flash_write(slot: &mut u32, value: u32) {
        *slot &= value;
    }

    /// ERASEUICR: back to the all-erased state (every field 0xFFFFFFFF,
    /// except the silicon-fixed NFCPINS default). Drained by the machine
    /// boundary when the NVMC latches the request.
    pub fn erase(&mut self) {
        *self = Self::default();
    }
}

impl Peripheral for Nrf52Uicr {
    /// Walk-independent for every firmware state: this model overrides neither
    /// `tick()` nor `tick_elapsed()` with time-driven work that the walk must
    /// deliver. Observable effects land on MMIO writes and/or the separate
    /// `tick_with_bus` path (`bus_tick_indices`), which still runs when the
    /// legacy walk is deleted. Marking `needs_legacy_walk = false` therefore
    /// drops only empty dispatch from the per-cycle walk — byte-identical.
    fn needs_legacy_walk(&self) -> bool {
        false
    }

    /// Byte lane of the containing word. UICR is memory-mapped flash, so an
    /// LDRB reads the same bits an LDR does — not a constant 0xFF, which
    /// disagreed with `read_u32` as soon as any field was programmed.
    fn read(&self, offset: u64) -> SimResult<u8> {
        let shift = (offset & 3) * 8;
        Ok((self.read_u32(offset & !3)? >> shift) as u8)
    }

    /// Byte-lane program of the containing word, with the same 1 → 0 flash
    /// semantics as `write_u32` (the other three lanes are left as they are).
    ///
    /// This is the path a firmware image's UICR record takes at load time:
    /// `Machine::load_firmware` writes segments that miss flash/RAM byte by
    /// byte through the bus. Intel HEX images for nRF52 routinely carry one
    /// (the micro:bit V2 MakeCode hex programs NRFFW[0] = 0x77000, the
    /// bootloader start, at 0x10001014). Dropping these writes left the
    /// fields erased, and CODAL then placed its flash-storage page at
    /// 0xFFFFFFFF - 3 * 4096 and hard-faulted reading it.
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        let lane = !(0xFFu32 << shift) | (u32::from(value) << shift);
        self.write_u32(offset & !3, lane)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        Ok(match offset {
            OFF_NRFFW_FIRST..OFF_NRFFW_LAST if offset.is_multiple_of(4) => {
                self.nrffw[((offset - OFF_NRFFW_FIRST) / 4) as usize]
            }
            OFF_NRFHW_FIRST..OFF_NRFHW_LAST if offset.is_multiple_of(4) => {
                self.nrfhw[((offset - OFF_NRFHW_FIRST) / 4) as usize]
            }
            OFF_CUSTOMER_FIRST..=OFF_CUSTOMER_LAST if offset.is_multiple_of(4) => {
                self.customer[((offset - OFF_CUSTOMER_FIRST) / 4) as usize]
            }
            OFF_PSELRESET0 => self.pselreset[0],
            OFF_PSELRESET1 => self.pselreset[1],
            OFF_APPROTECT => self.approtect,
            OFF_NFCPINS => self.nfcpins,
            OFF_DEBUGCTRL => self.debugctrl,
            OFF_REGOUT0 => self.regout0,
            _ => ERASED,
        })
    }

    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        match offset {
            OFF_NRFFW_FIRST..OFF_NRFFW_LAST if offset.is_multiple_of(4) => {
                let i = ((offset - OFF_NRFFW_FIRST) / 4) as usize;
                Self::flash_write(&mut self.nrffw[i], value);
            }
            OFF_NRFHW_FIRST..OFF_NRFHW_LAST if offset.is_multiple_of(4) => {
                let i = ((offset - OFF_NRFHW_FIRST) / 4) as usize;
                Self::flash_write(&mut self.nrfhw[i], value);
            }
            OFF_CUSTOMER_FIRST..=OFF_CUSTOMER_LAST if offset.is_multiple_of(4) => {
                let i = ((offset - OFF_CUSTOMER_FIRST) / 4) as usize;
                Self::flash_write(&mut self.customer[i], value);
            }
            OFF_PSELRESET0 => Self::flash_write(&mut self.pselreset[0], value),
            OFF_PSELRESET1 => Self::flash_write(&mut self.pselreset[1], value),
            OFF_APPROTECT => Self::flash_write(&mut self.approtect, value),
            OFF_NFCPINS => Self::flash_write(&mut self.nfcpins, value),
            OFF_DEBUGCTRL => Self::flash_write(&mut self.debugctrl, value),
            OFF_REGOUT0 => Self::flash_write(&mut self.regout0, value),
            _ => {
                crate::census_reg!("nrf52.uicr:Nrf52Uicr", offset, "write");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approtect_starts_erased() {
        let u = Nrf52Uicr::new();
        assert_eq!(u.read_u32(OFF_APPROTECT).unwrap(), 0xFFFF_FFFF);
    }

    #[test]
    fn write_zero_clears_bits() {
        let mut u = Nrf52Uicr::new();
        u.write_u32(OFF_APPROTECT, 0x0000_00FF).unwrap();
        assert_eq!(u.read_u32(OFF_APPROTECT).unwrap(), 0x0000_00FF);
    }

    /// The Intel-HEX UICR record of the micro:bit V2 MakeCode base image
    /// (`:081014000070070000E0070076`), delivered the way
    /// `Machine::load_firmware` delivers a segment outside flash/RAM: one
    /// byte at a time.
    #[test]
    fn byte_writes_program_the_containing_word() {
        let mut u = Nrf52Uicr::new();
        for (i, b) in [0x00u8, 0x70, 0x07, 0x00, 0x00, 0xE0, 0x07, 0x00]
            .iter()
            .enumerate()
        {
            u.write(OFF_NRFFW_FIRST + i as u64, *b).unwrap();
        }
        assert_eq!(u.read_u32(OFF_NRFFW_FIRST).unwrap(), 0x0007_7000);
        assert_eq!(u.read_u32(OFF_NRFFW_FIRST + 4).unwrap(), 0x0007_E000);
        // Byte reads agree with the word view.
        assert_eq!(u.read(OFF_NRFFW_FIRST + 1).unwrap(), 0x70);
        assert_eq!(u.read(OFF_NRFFW_FIRST + 2).unwrap(), 0x07);
        // Untouched fields stay erased.
        assert_eq!(u.read_u32(OFF_NRFFW_FIRST + 8).unwrap(), ERASED);
        assert_eq!(u.read(OFF_APPROTECT).unwrap(), 0xFF);
    }

    #[test]
    fn byte_write_keeps_flash_semantics_and_other_lanes() {
        let mut u = Nrf52Uicr::new();
        u.write_u32(OFF_CUSTOMER_FIRST, 0x1234_5678).unwrap();
        u.write(OFF_CUSTOMER_FIRST + 1, 0xFF).unwrap(); // cannot set bits
        assert_eq!(u.read_u32(OFF_CUSTOMER_FIRST).unwrap(), 0x1234_5678);
        u.write(OFF_CUSTOMER_FIRST + 3, 0x02).unwrap(); // 0x12 & 0x02
        assert_eq!(u.read_u32(OFF_CUSTOMER_FIRST).unwrap(), 0x0234_5678);
    }

    #[test]
    fn write_cannot_set_bits() {
        let mut u = Nrf52Uicr::new();
        u.write_u32(OFF_APPROTECT, 0).unwrap();
        u.write_u32(OFF_APPROTECT, 0xFFFF_FFFF).unwrap();
        assert_eq!(u.read_u32(OFF_APPROTECT).unwrap(), 0);
    }
}
