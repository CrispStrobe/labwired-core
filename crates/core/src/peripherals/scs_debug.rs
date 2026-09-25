// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! `DHCSR` at `0xE000_EDF0`. Only `C_DEBUGEN` and `C_HALT` are stored.
//! `C_STEP` and `C_MASKINTS` are dropped. The CPU shares this `Arc`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::SimResult;

const DBGKEY: u32 = 0xA05F;
const S_REGRDY: u32 = 1 << 16;
const S_HALT: u32 = 1 << 17;

#[derive(Debug)]
pub struct DebugHaltState {
    /// Bit 0 `C_DEBUGEN`, bit 1 `C_HALT`. Every other bit stays 0.
    controls: AtomicU32,
}

impl DebugHaltState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            controls: AtomicU32::new(0),
        })
    }

    pub fn read_dhcsr(&self) -> u32 {
        let c = self.controls.load(Ordering::Relaxed) & 0x3;
        let halt = if c == 0x3 { S_HALT } else { 0 };
        c | S_REGRDY | halt
    }

    pub fn write_dhcsr(&self, value: u32) {
        if (value >> 16) != DBGKEY {
            return;
        }
        self.controls.store(value & 0x3, Ordering::Relaxed);
    }

    pub fn halted(&self) -> bool {
        self.controls.load(Ordering::Relaxed) & 0x3 == 0x3
    }

    pub fn clear(&self) {
        self.controls.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct ScsDebug {
    pub state: Arc<DebugHaltState>,
}

impl ScsDebug {
    pub fn new(state: Arc<DebugHaltState>) -> Self {
        Self { state }
    }
}

impl crate::Peripheral for ScsDebug {
    fn needs_legacy_walk(&self) -> bool {
        false
    }

    fn read(&self, offset: u64) -> SimResult<u8> {
        if offset >= 4 {
            return Ok(0);
        }
        let word = self.state.read_dhcsr();
        let shift = (offset as u32) * 8;
        Ok(((word >> shift) & 0xFF) as u8)
    }

    fn write(&mut self, _offset: u64, _value: u8) -> SimResult<()> {
        // A byte store must not apply a partial key. The word path is `write_u32`.
        Ok(())
    }

    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        if offset == 0 {
            Ok(self.state.read_dhcsr())
        } else {
            Ok(0)
        }
    }

    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        if offset == 0 {
            self.state.write_dhcsr(value);
        }
        Ok(())
    }

    fn peek(&self, offset: u64) -> Option<u8> {
        self.read(offset).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Peripheral;

    #[test]
    fn wrong_key_does_not_change_dhcsr() {
        let s = DebugHaltState::new();
        s.write_dhcsr(0x0000_0003);
        assert_eq!(s.read_dhcsr(), 0x0001_0000);
    }

    #[test]
    fn debug_key_and_halt_sets_s_halt() {
        let s = DebugHaltState::new();
        s.write_dhcsr(0xA05F_0003);
        assert_eq!(s.read_dhcsr(), 0x0001_0003 | (1 << 17));
        assert!(s.halted());
    }

    #[test]
    fn c_step_bit_is_not_stored() {
        let s = DebugHaltState::new();
        s.write_dhcsr(0xA05F_0005);
        let word = s.read_dhcsr();
        assert_eq!(word & (1 << 2), 0, "C_STEP must read as 0, word {word:#x}");
        assert_eq!(
            word & (1 << 17),
            0,
            "S_HALT must stay clear, word {word:#x}"
        );
        assert!(!s.halted());
    }

    #[test]
    fn scs_keyed_word_write_sets_s_halt_and_byte_lanes() {
        let mut dev = ScsDebug::new(DebugHaltState::new());
        dev.write_u32(0, 0xA05F_0003).unwrap();
        let word = dev.read_u32(0).unwrap();
        assert_ne!(word & (1 << 17), 0, "S_HALT set, word {word:#x}");
        assert_eq!(word, 0x0003_0003);
        for lane in 0..4u64 {
            let byte = ((word >> (lane * 8)) & 0xFF) as u8;
            assert_eq!(dev.read(lane).unwrap(), byte, "lane {lane}");
            assert_eq!(dev.peek(lane), Some(byte), "peek lane {lane}");
        }
    }

    #[test]
    fn byte_write_does_not_change_dhcsr() {
        let mut dev = ScsDebug::new(DebugHaltState::new());
        dev.write(0, 0x03).unwrap();
        assert_eq!(dev.read_u32(0).unwrap(), 0x0001_0000);

        dev.write_u32(0, 0xA05F_0003).unwrap();
        let keyed = dev.read_u32(0).unwrap();
        dev.write(0, 0x00).unwrap();
        assert_eq!(dev.read_u32(0).unwrap(), keyed);
    }

    #[test]
    fn reads_past_dhcsr_are_zero() {
        let mut dev = ScsDebug::new(DebugHaltState::new());
        dev.write_u32(0, 0xA05F_0003).unwrap();
        assert_eq!(dev.read(4).unwrap(), 0);
        assert_eq!(dev.peek(4), Some(0));
        assert_eq!(dev.read_u16(4).unwrap(), 0);
        assert_eq!(dev.read_u32(4).unwrap(), 0);
    }

    #[test]
    fn c_halt_without_debugen_does_not_set_s_halt() {
        let mut dev = ScsDebug::new(DebugHaltState::new());
        dev.write_u32(0, 0xA05F_0002).unwrap();
        let word = dev.read_u32(0).unwrap();
        assert_eq!(word & 0x3, 0x2, "C_HALT stored, word {word:#x}");
        assert_eq!(
            word & (1 << 17),
            0,
            "S_HALT must stay clear, word {word:#x}"
        );
        assert!(!dev.state.halted());
    }
}
