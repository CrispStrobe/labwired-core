// SPDX-License-Identifier: MIT
//! Opt-in SAMD51 TC slice (DS60001507F 48): 8- and 16-bit counters with the
//! NFRQ/MFRQ/NPWM/MPWM wave generators, compare-match and overflow interrupts,
//! the CTRLB commands and READSYNC. The counter advances from CPU cycles by the
//! nominal ratio f_TC / f_CPU resolved from the live clock tree; it stops when
//! its bus clock, generic clock channel or source is not running. Waveform pins,
//! capture, events, one-shot retrigger timing, 32-bit pairing and DMA are refused
//! or not modelled.
use super::clock_tree;
use crate::{Bus, Peripheral, PeripheralTickResult, SimResult, SimulationError};
use std::any::Any;

const MCLK: u64 = 0x40000800;
const SWRST: u32 = 1;
const ENABLE: u32 = 1 << 1;
const OVF: u8 = 1;
const MC0: u8 = 1 << 4;

#[derive(Clone, Copy, Debug)]
pub struct TcInstance {
    pub base: u64,
    pub irq: u32,
    pub gclk_channel: u64,
    /// (MCLK mask register offset, bit)
    pub bus_clock: (u64, u32),
}

/// TC0..TC5 on the SAMD51J19A (ATDF instance parameters); G-variant parts have TC0..TC3.
pub const TC_INSTANCES: [TcInstance; 6] = [
    TcInstance {
        base: 0x40003800,
        irq: 107,
        gclk_channel: 9,
        bus_clock: (0x14, 14),
    },
    TcInstance {
        base: 0x40003c00,
        irq: 108,
        gclk_channel: 9,
        bus_clock: (0x14, 15),
    },
    TcInstance {
        base: 0x4101a000,
        irq: 109,
        gclk_channel: 26,
        bus_clock: (0x18, 13),
    },
    TcInstance {
        base: 0x4101c000,
        irq: 110,
        gclk_channel: 26,
        bus_clock: (0x18, 14),
    },
    TcInstance {
        base: 0x42001400,
        irq: 111,
        gclk_channel: 30,
        bus_clock: (0x1c, 5),
    },
    TcInstance {
        base: 0x42001800,
        irq: 112,
        gclk_channel: 30,
        bus_clock: (0x1c, 6),
    },
];

#[derive(Debug)]
pub struct Samd51Tc {
    instance: TcInstance,
    ctrla: u32,
    ctrlb: u8,
    inten: u8,
    intflag: u8,
    wave: u8,
    drvctrl: u8,
    dbgctrl: u8,
    count: u32,
    per: u8,
    cc: [u32; 2],
    // Resolved clock ratio (f_TC, f_CPU) and the fractional carry between ticks.
    rate: Option<(u64, u64)>,
    rate_dirty: bool,
    carry: u64,
    refusal: Option<String>,
}

impl Samd51Tc {
    pub fn new(instance: TcInstance) -> Self {
        Self {
            instance,
            ctrla: 0,
            ctrlb: 0,
            inten: 0,
            intflag: 0,
            wave: 0,
            drvctrl: 0,
            dbgctrl: 0,
            count: 0,
            per: 0xff,
            cc: [0; 2],
            rate: None,
            rate_dirty: true,
            carry: 0,
            refusal: None,
        }
    }
    fn refuse<T>(what: &str) -> SimResult<T> {
        Err(SimulationError::NotImplemented(format!(
            "samd51 TC slice: {what}"
        )))
    }
    fn mode(&self) -> u32 {
        (self.ctrla >> 2) & 3 // 0 COUNT16, 1 COUNT8, 2 COUNT32
    }
    fn enabled(&self) -> bool {
        self.ctrla & ENABLE != 0
    }
    fn width_mask(&self) -> u32 {
        if self.mode() == 1 {
            0xff
        } else {
            0xffff
        }
    }
    fn prescaler(&self) -> u64 {
        [1, 2, 4, 8, 16, 64, 256, 1024][((self.ctrla >> 8) & 7) as usize]
    }
    /// TOP: MAX for NFRQ/NPWM, CC0 for MFRQ/MPWM; PER replaces MAX in 8-bit mode.
    fn top(&self) -> u32 {
        match self.wave & 3 {
            1 | 3 => self.cc[0],
            _ if self.mode() == 1 => u32::from(self.per),
            _ => self.width_mask(),
        }
    }
    fn reset(&mut self) {
        *self = Self::new(self.instance);
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    fn line(&self) -> bool {
        self.intflag & self.inten != 0
    }
    /// Advance by `ticks` counter clocks, raising overflow and compare matches.
    fn advance(&mut self, mut ticks: u64) {
        let down = self.ctrlb & 1 != 0;
        let top = u64::from(self.top());
        while ticks > 0 {
            let here = u64::from(self.count);
            // Distance to the next wrap; matches are checked inside that span.
            let span = if down {
                here + 1
            } else {
                top.saturating_sub(here) + 1
            };
            let step = ticks.min(span);
            for (i, cc) in self.cc.iter().enumerate() {
                let cc = u64::from(*cc);
                let hit = if down {
                    cc <= here && here - cc < step
                } else {
                    cc > here && cc - here <= step
                };
                if hit {
                    self.intflag |= MC0 << i;
                }
            }
            if step == span {
                self.intflag |= OVF;
                self.count = if down { top as u32 } else { 0 };
                if self.ctrlb & 4 != 0 {
                    // ONESHOT: stop at the wrap.
                    self.ctrla &= !ENABLE;
                    return;
                }
            } else {
                self.count = if down {
                    (here - step) as u32
                } else {
                    (here + step) as u32
                };
            }
            ticks -= step;
        }
    }
    fn read_reg(&self, offset: u64) -> SimResult<u32> {
        Ok(match offset {
            0x00 => self.ctrla,
            0x04 | 0x05 => u32::from(self.ctrlb),
            0x06 => 0,
            0x08 | 0x09 => u32::from(self.inten),
            0x0a => u32::from(self.intflag),
            0x0b => u32::from(!self.enabled()), // STATUS.STOP
            0x0c => u32::from(self.wave),
            0x0d => u32::from(self.drvctrl),
            0x0f => u32::from(self.dbgctrl),
            0x10 => 0, // SYNCBUSY: writes complete immediately
            0x14 => self.count,
            0x1b if self.mode() == 1 => u32::from(self.per),
            0x1c | 0x1d if self.mode() == 1 => self.cc[(offset - 0x1c) as usize],
            0x1c if self.mode() == 0 => self.cc[0],
            0x1e if self.mode() == 0 => self.cc[1],
            _ => return Self::refuse("register outside the 8/16-bit slice"),
        })
    }
    fn write_reg(&mut self, offset: u64, value: u32, size: u32) -> SimResult<()> {
        match (offset, size) {
            (0x00, 4) => {
                if value & SWRST != 0 {
                    self.reset();
                    return Ok(());
                }
                let protected = !(ENABLE | SWRST);
                if self.enabled() && (value ^ self.ctrla) & protected != 0 {
                    return Self::refuse("CTRLA is enable-protected");
                }
                // MODE (COUNT8/16), PRESCSYNC, RUNSTDBY, ONDEMAND, PRESCALER; no capture/DMA/ALOCK.
                if value & !0x0000_07ff != 0 || (value >> 2) & 3 > 1 {
                    return Self::refuse("CTRLA capture, lock or 32-bit mode");
                }
                self.ctrla = value;
                self.rate_dirty = true;
            }
            (0x04, 1) | (0x05, 1) => {
                let set = offset == 0x05;
                let flags = (value as u8) & 0x07;
                let command = (value as u8) >> 5;
                if set {
                    self.ctrlb |= flags;
                } else {
                    self.ctrlb &= !flags;
                }
                match command {
                    0 | 4 => {} // NONE, READSYNC: COUNT is always current
                    1 => {
                        // RETRIGGER: restart from the counting direction's origin.
                        self.count = if self.ctrlb & 1 != 0 { self.top() } else { 0 };
                        self.ctrla |= ENABLE;
                    }
                    2 => self.ctrla &= !ENABLE, // STOP
                    _ => return Self::refuse("CTRLB UPDATE/DMAOS command"),
                }
            }
            (0x06, 2) if value == 0 => {}
            (0x08, 1) => self.inten &= !(value as u8),
            (0x09, 1) => self.inten |= (value as u8) & 0x33,
            (0x0a, 1) => self.intflag &= !(value as u8),
            (0x0c, 1) if self.enabled() => return Self::refuse("WAVE is enable-protected"),
            (0x0c, 1) => self.wave = (value as u8) & 3,
            (0x0d, 1) => self.drvctrl = (value as u8) & 3,
            (0x0f, 1) => self.dbgctrl = (value as u8) & 1,
            (0x14, 2) | (0x14, 1) => self.count = value & self.width_mask(),
            (0x1b, 1) if self.mode() == 1 => self.per = value as u8,
            (0x1c, 1) | (0x1d, 1) if self.mode() == 1 => {
                self.cc[(offset - 0x1c) as usize] = value & 0xff
            }
            (0x1c, 2) if self.mode() == 0 => self.cc[0] = value & 0xffff,
            (0x1e, 2) if self.mode() == 0 => self.cc[1] = value & 0xffff,
            _ => return Self::refuse("store outside the 8/16-bit slice"),
        }
        Ok(())
    }
    fn checked(&mut self, result: SimResult<()>) -> SimResult<()> {
        if let Err(e) = &result {
            self.refusal.get_or_insert_with(|| e.to_string());
        }
        result
    }
}

impl Peripheral for Samd51Tc {
    fn read(&self, offset: u64) -> SimResult<u8> {
        let aligned = match offset {
            0x14..=0x17 => 0x14,
            0x1c..=0x1d if self.mode() == 0 => 0x1c,
            0x1e..=0x1f if self.mode() == 0 => 0x1e,
            0x00..=0x03 | 0x10..=0x13 => offset & !3,
            _ => offset,
        };
        Ok((self.read_reg(aligned)? >> (8 * (offset - aligned))) as u8)
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let r = self.write_reg(offset, u32::from(value), 1);
        self.checked(r)
    }
    fn write_u16(&mut self, offset: u64, value: u16) -> SimResult<()> {
        let r = self.write_reg(offset, u32::from(value), 2);
        self.checked(r)
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        let r = match offset {
            // 16-bit COUNT/CC registers stored with a word access keep their low half.
            0x14 | 0x1c if self.mode() == 0 && value >> 16 == 0 => self.write_reg(offset, value, 2),
            _ => self.write_reg(offset, value, 4),
        };
        self.checked(r)
    }
    fn peek(&self, offset: u64) -> Option<u8> {
        self.read(offset).ok()
    }
    fn needs_bus_tick(&self) -> bool {
        self.enabled() && self.rate_dirty
    }
    fn tick_with_bus(&mut self, bus: &mut dyn Bus) {
        let (mask, bit) = self.instance.bus_clock;
        let clocked = bus.read_u32(MCLK + mask).is_ok_and(|m| m & (1 << bit) != 0);
        let tc = clock_tree::peripheral_hz(bus, self.instance.gclk_channel);
        let cpu = clock_tree::cpu_hz(bus);
        self.rate = match (clocked, tc, cpu) {
            (true, Some(tc), Some(cpu)) if cpu > 0 => Some((tc / self.prescaler(), cpu)),
            _ => None,
        };
        self.rate_dirty = false;
    }
    fn legacy_tick_dynamic(&self) -> bool {
        true
    }
    fn legacy_tick_active(&self) -> bool {
        (self.enabled() && self.rate.is_some()) || self.line()
    }
    fn irq_line_levels(&self, report: &mut dyn FnMut(u32, bool)) {
        report(self.instance.irq, self.line());
    }
    fn tick_elapsed(&mut self, cycles: u64) -> PeripheralTickResult {
        if self.enabled() && !self.rate_dirty {
            if let Some((tc, cpu)) = self.rate {
                self.carry += cycles * tc;
                let ticks = self.carry / cpu;
                self.carry %= cpu;
                if ticks > 0 {
                    self.advance(ticks);
                }
            }
        }
        PeripheralTickResult {
            explicit_irqs: self.line().then(|| vec![self.instance.irq]),
            ..Default::default()
        }
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"peripheral": "samd51_tc", "base": self.instance.base, "ctrla": self.ctrla,
            "count": self.count, "cc": self.cc, "intflag": self.intflag, "inten": self.inten,
            "rate": self.rate, "refusal": self.refusal})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(tc: &mut Samd51Tc, rate: (u64, u64)) {
        tc.rate = Some(rate);
        tc.rate_dirty = false;
    }

    #[test]
    fn count16_compare_and_overflow_from_cpu_cycles() {
        let mut tc = Samd51Tc::new(TC_INSTANCES[0]);
        tc.write_u32(0, 3 << 8).unwrap(); // COUNT16, DIV8
        tc.write_u16(0x1c, 100).unwrap();
        tc.write(0x09, MC0).unwrap();
        tc.write_u32(0, 3 << 8 | ENABLE).unwrap();
        running(&mut tc, (1_000_000, 120_000_000));
        tc.tick_elapsed(120 * 99);
        assert_eq!(tc.count(), 99);
        assert!(!tc.line());
        tc.tick_elapsed(120);
        assert_eq!(tc.count(), 100);
        assert!(tc.line());
        tc.write(0x0a, MC0).unwrap();
        assert!(!tc.line());
        tc.tick_elapsed(120 * (0x10000 - 100));
        assert_eq!(tc.count(), 0);
        assert_eq!(tc.read(0x0a).unwrap() & OVF, OVF);
    }

    #[test]
    fn stopped_without_a_resolved_clock_and_refusals() {
        let mut tc = Samd51Tc::new(TC_INSTANCES[2]);
        tc.write_u32(0, ENABLE).unwrap();
        tc.tick_elapsed(1_000_000);
        assert_eq!(tc.count(), 0, "no clock resolved: the counter does not run");
        assert!(
            tc.write_u32(0, ENABLE | 3 << 8).is_err(),
            "enable-protected prescaler"
        );
        assert!(tc.write_u32(0, 2 << 2).is_err(), "32-bit pairing");
        assert!(tc.write_u16(0x06, 1).is_err(), "events");
        tc.write_u32(0, SWRST).unwrap();
        assert_eq!(tc.read_u32(0).unwrap(), 0);
    }
}
