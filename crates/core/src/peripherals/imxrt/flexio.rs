// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 FlexIO (FLEXIO1 `0x401A_C000`, FLEXIO2 `0x401B_0000`,
//! IMXRT1060RM §50): 4 shifters, 4 timers (`PARAM` = 0x0210_0404).
//!
//! Modelled at the transfer level, which is how the SDK drivers use the
//! block (FlexIO SPI / UART / I2S / MCULCD all pair a shifter with a baud
//! timer):
//! * A transmit shifter (`SMOD` = 2) whose buffer holds data loads it when
//!   its timer is enabled by its trigger (`TIMCTL.TRGSEL` = shifter status
//!   flag, the SDK's choice) and shifts it out for the number of bits the
//!   timer's `TIMCMP` programs (dual 8-bit baud/bit mode: bit period
//!   `2*(CMP[7:0]+1)` FlexIO clocks, `(CMP[15:8]+1)/2` bits). `SHIFTSTAT`
//!   is set again when the buffer is free — the DMA/IRQ request that feeds
//!   the next byte — and `TIMSTAT` when the timer finishes.
//! * A receive shifter (`SMOD` = 1) clocked by the same timer captures the
//!   bits the attached device drives back, MSB first into `SHIFTBUF[31]`
//!   downward, exactly like silicon, so `SHIFTBUFBIS` reads the byte.
//! * The buffer aliases (`SHIFTBUFBIS/BYS/BBS/NBS/HWS/NIS`) apply their bit,
//!   byte, nibble and half-word swaps on both read and write.
//!
//! What the pins carry is published as a log of wire words (bits in wire
//! order) with the pin numbers; an optional SPI device on the transmit pin
//! receives MSB-first bytes and answers on the receive pin.

use super::{byte_of, Timebase};
use crate::peripherals::spi::SpiDevice;
use crate::{Peripheral, PeripheralTickResult, SimResult};
use std::any::Any;
use std::cell::RefCell;

const N: usize = 4;

const VERID: u32 = 0x000;
const PARAM: u32 = 0x004;
const CTRL: u32 = 0x008;
const PIN: u32 = 0x00C;
const SHIFTSTAT: u32 = 0x010;
const SHIFTERR: u32 = 0x014;
const TIMSTAT: u32 = 0x018;
const SHIFTSIEN: u32 = 0x020;
const SHIFTEIEN: u32 = 0x024;
const TIMIEN: u32 = 0x028;
const SHIFTSDEN: u32 = 0x030;
const SHIFTSTATE: u32 = 0x040;

/// One word shifted out on the pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireWord {
    /// Cycle the last bit left the pin.
    pub cycle: u64,
    /// Shifter output pin (`SHIFTCTL.PINSEL`) and width (PWIDTH+1).
    pub pin: u8,
    pub width: u8,
    /// The shifted value as the device sees it: for 1-bit shifters the
    /// bits MSB-first (first bit on the wire is the MSB); for parallel
    /// shifters the low `width` bits of each beat, first beat first.
    pub beats: Vec<u32>,
    pub bits: u8,
}

#[derive(Debug, Default, Clone, Copy)]
struct Shifter {
    ctl: u32,
    cfg: u32,
    buf: u32,
    /// Data waiting in the buffer (transmit) / unread data (receive).
    full: bool,
    /// Transmit in progress: (value, completion cycle).
    shifting: Option<(u32, u64)>,
}

#[derive(Debug)]
struct Inner {
    ctrl: u32,
    shifterr: u32,
    timstat: u32,
    shiftsien: u32,
    shifteien: u32,
    timien: u32,
    shiftsden: u32,
    shiftstate: u32,
    sh: [Shifter; N],
    timctl: [u32; N],
    timcfg: [u32; N],
    timcmp: [u32; N],
    wire: Vec<WireWord>,
}

impl Inner {
    fn new() -> Self {
        Self {
            ctrl: 0,
            shifterr: 0,
            timstat: 0,
            shiftsien: 0,
            shifteien: 0,
            timien: 0,
            shiftsden: 0,
            shiftstate: 0,
            sh: [Shifter::default(); N],
            timctl: [0; N],
            timcfg: [0; N],
            timcmp: [0; N],
            wire: Vec::new(),
        }
    }
    fn smod(&self, n: usize) -> u32 {
        self.sh[n].ctl & 0x7
    }
    fn timsel(&self, n: usize) -> usize {
        ((self.sh[n].ctl >> 24) & 0x3) as usize
    }
    /// SHIFTSTAT: transmit = buffer empty, receive = buffer full.
    fn ssf(&self) -> u32 {
        let mut v = 0;
        for n in 0..N {
            let f = match self.smod(n) {
                2 => !self.sh[n].full,
                1 => self.sh[n].full,
                _ => false,
            };
            if f {
                v |= 1 << n;
            }
        }
        v
    }
}

pub struct ImxrtFlexio {
    inner: RefCell<Inner>,
    time: Timebase,
    clk_hz: u64,
    /// Optional SPI device clocked by the FlexIO SPI master.
    device: RefCell<Option<Box<dyn SpiDevice>>>,
}

impl std::fmt::Debug for ImxrtFlexio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImxrtFlexio")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

/// FlexIO functional clock when the chip YAML does not say otherwise: the
/// CCM reset value (PLL3 480 MHz / 2 / 8 = 30 MHz).
pub const DEFAULT_FLEXIO_CLK_HZ: u64 = 30_000_000;

impl Default for ImxrtFlexio {
    fn default() -> Self {
        Self::new(DEFAULT_FLEXIO_CLK_HZ)
    }
}

fn bit_rev(v: u32) -> u32 {
    v.reverse_bits()
}
fn byte_swap(v: u32) -> u32 {
    v.swap_bytes()
}
fn nibble_swap(v: u32) -> u32 {
    let mut out = 0;
    for b in 0..4 {
        let byte = (v >> (8 * b)) & 0xFF;
        out |= (((byte & 0xF) << 4) | (byte >> 4)) << (8 * b);
    }
    out
}
fn half_swap(v: u32) -> u32 {
    v.rotate_left(16)
}

/// Alias transform for buffer window `win` (0x200..0x7FF, 0x80 each).
fn alias(win: u32, v: u32) -> u32 {
    match win {
        0x280 => bit_rev(v),                 // SHIFTBUFBIS
        0x300 => byte_swap(v),               // SHIFTBUFBYS
        0x380 => bit_rev(byte_swap(v)),      // SHIFTBUFBBS
        0x680 => nibble_swap(byte_swap(v)),  // SHIFTBUFNBS
        0x700 => half_swap(v),               // SHIFTBUFHWS
        0x780 => nibble_swap(v),             // SHIFTBUFNIS
        _ => v,                              // SHIFTBUF
    }
}

impl ImxrtFlexio {
    pub fn new(clk_hz: u64) -> Self {
        Self {
            inner: RefCell::new(Inner::new()),
            time: Timebase::default(),
            clk_hz: clk_hz.max(1),
            device: RefCell::new(None),
        }
    }

    /// Attach the device the FlexIO SPI master talks to.
    pub fn attach_spi_device(&mut self, dev: Box<dyn SpiDevice>) {
        *self.device.get_mut() = Some(dev);
    }

    /// Everything shifted out so far.
    pub fn wire_log(&self) -> Vec<WireWord> {
        self.inner.borrow().wire.clone()
    }

    /// Bits and bit period (core cycles) of a transfer on timer `t`.
    fn timer_geometry(&self, i: &Inner, t: usize) -> (u32, u64) {
        let cmp = i.timcmp[t];
        let (bits, clocks_per_bit) = match i.timctl[t] & 0x3 {
            1 => {
                let bits = ((cmp >> 8) & 0xFF) / 2 + 1;
                (bits, 2 * ((cmp & 0xFF) as u64 + 1))
            }
            2 => (1, 2 * ((cmp & 0xFF) as u64 + 1)),
            _ => (32, 2),
        };
        let per_bit = (clocks_per_bit * self.time.cpu_hz() / self.clk_hz).max(1);
        (bits.clamp(1, 32), per_bit)
    }

    /// Shifters whose timer is triggered by their own status flag start
    /// when their buffer has data (the SDK FlexIO SPI/UART/I2S wiring).
    fn sync(&self) {
        let now = self.time.now();
        let mut i = self.inner.borrow_mut();
        if i.ctrl & 1 == 0 {
            return;
        }
        for n in 0..N {
            // Completion of a transfer in flight.
            if let Some((val, done)) = i.sh[n].shifting {
                if now < done {
                    continue;
                }
                i.sh[n].shifting = None;
                let t = i.timsel(n);
                let (bits, _) = self.timer_geometry(&i, t);
                let width = ((i.sh[n].cfg >> 16) & 0x1F) as u8 + 1;
                let pin = ((i.sh[n].ctl >> 8) & 0x1F) as u8;
                let mut beats = Vec::new();
                let mut rx_bits: u32 = 0;
                if width == 1 {
                    // Serial: first bit out is bit 0 of the buffer.
                    let mut msb_first = 0u32;
                    for b in 0..bits {
                        msb_first = (msb_first << 1) | ((val >> b) & 1);
                    }
                    beats.push(msb_first);
                    if bits <= 8 {
                        if let Some(dev) = self.device.borrow_mut().as_mut() {
                            rx_bits = dev.transfer(msb_first as u8) as u32;
                        }
                    }
                } else {
                    let mask = if width >= 32 { u32::MAX } else { (1 << width) - 1 };
                    let shifts = (bits / width as u32).max(1);
                    for s in 0..shifts {
                        beats.push((val >> (s * width as u32)) & mask);
                    }
                }
                i.wire.push(WireWord {
                    cycle: done,
                    pin,
                    width,
                    beats,
                    bits: bits as u8,
                });
                i.timstat |= 1 << t;
                // Receive shifters on the same timer capture MISO: MSB
                // first into bit 31, shifting right.
                for m in 0..N {
                    if m != n && i.smod(m) == 1 && i.timsel(m) == t {
                        let v = (rx_bits.reverse_bits() >> (32 - bits.min(32))) << (32 - bits.min(32));
                        if i.sh[m].full {
                            i.shifterr |= 1 << m;
                        }
                        i.sh[m].buf = v;
                        i.sh[m].full = true;
                    }
                }
            }
            // Start the next transfer: buffer -> shifter.
            if i.smod(n) == 2 && i.sh[n].full && i.sh[n].shifting.is_none() {
                let t = i.timsel(n);
                let timod = i.timctl[t] & 0x3;
                if timod == 0 {
                    continue; // timer disabled: the shifter never clocks
                }
                let (bits, per_bit) = self.timer_geometry(&i, t);
                let val = i.sh[n].buf;
                i.sh[n].full = false;
                i.sh[n].shifting = Some((val, now + bits as u64 * per_bit));
            }
        }
    }

    fn buf_window(off: u32) -> Option<(u32, usize)> {
        let win = off & !0x7F;
        let idx = ((off & 0x7F) / 4) as usize;
        matches!(win, 0x200 | 0x280 | 0x300 | 0x380 | 0x680 | 0x700 | 0x780)
            .then_some((win, idx))
            .filter(|&(_, i)| i < N)
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        self.sync();
        let mut i = self.inner.borrow_mut();
        match off & !3 {
            VERID => 0x0101_0001,
            PARAM => 0x0210_0404,
            CTRL => i.ctrl,
            PIN => 0,
            SHIFTSTAT => i.ssf(),
            SHIFTERR => i.shifterr,
            TIMSTAT => i.timstat,
            SHIFTSIEN => i.shiftsien,
            SHIFTEIEN => i.shifteien,
            TIMIEN => i.timien,
            SHIFTSDEN => i.shiftsden,
            SHIFTSTATE => i.shiftstate,
            o if (0x080..0x090).contains(&o) => i.sh[((o - 0x80) / 4) as usize].ctl,
            o if (0x100..0x110).contains(&o) => i.sh[((o - 0x100) / 4) as usize].cfg,
            o if (0x400..0x410).contains(&o) => i.timctl[((o - 0x400) / 4) as usize],
            o if (0x480..0x490).contains(&o) => i.timcfg[((o - 0x480) / 4) as usize],
            o if (0x500..0x510).contains(&o) => i.timcmp[((o - 0x500) / 4) as usize],
            o => match Self::buf_window(o) {
                Some((win, n)) => {
                    // Reading a receive buffer consumes it.
                    if i.smod(n) == 1 {
                        i.sh[n].full = false;
                    }
                    alias(win, i.sh[n].buf)
                }
                None => 0,
            },
        }
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        self.sync();
        {
            let mut i = self.inner.borrow_mut();
            let v = value & mask;
            let merge = |old: u32| (old & !mask) | v;
            match off & !3 {
                CTRL => {
                    let new = merge(i.ctrl);
                    if new & 0x2 != 0 {
                        // SWRST: every register but CTRL to reset.
                        let wire = std::mem::take(&mut i.wire);
                        *i = Inner::new();
                        i.wire = wire;
                    }
                    i.ctrl = new & 0xC000_0007;
                }
                SHIFTSTAT => {
                    // w1c only for match-mode shifters; transmit/receive
                    // flags follow the buffers.
                }
                SHIFTERR => i.shifterr &= !v,
                TIMSTAT => i.timstat &= !v,
                SHIFTSIEN => i.shiftsien = merge(i.shiftsien) & 0xF,
                SHIFTEIEN => i.shifteien = merge(i.shifteien) & 0xF,
                TIMIEN => i.timien = merge(i.timien) & 0xF,
                SHIFTSDEN => i.shiftsden = merge(i.shiftsden) & 0xF,
                SHIFTSTATE => i.shiftstate = merge(i.shiftstate) & 0x7,
                o if (0x080..0x090).contains(&o) => {
                    let n = ((o - 0x80) / 4) as usize;
                    i.sh[n].ctl = merge(i.sh[n].ctl);
                }
                o if (0x100..0x110).contains(&o) => {
                    let n = ((o - 0x100) / 4) as usize;
                    i.sh[n].cfg = merge(i.sh[n].cfg);
                }
                o if (0x400..0x410).contains(&o) => {
                    let n = ((o - 0x400) / 4) as usize;
                    i.timctl[n] = merge(i.timctl[n]);
                }
                o if (0x480..0x490).contains(&o) => {
                    let n = ((o - 0x480) / 4) as usize;
                    i.timcfg[n] = merge(i.timcfg[n]);
                }
                o if (0x500..0x510).contains(&o) => {
                    let n = ((o - 0x500) / 4) as usize;
                    i.timcmp[n] = merge(i.timcmp[n]);
                }
                o => {
                    if let Some((win, n)) = Self::buf_window(o) {
                        // Partial writes merge through the alias view.
                        let view = (alias(win, i.sh[n].buf) & !mask) | v;
                        // Every alias transform is its own inverse.
                        i.sh[n].buf = alias(win, view);
                        if i.smod(n) == 2 {
                            i.sh[n].full = true;
                        }
                    }
                }
            }
        }
        self.sync();
    }

    fn irq(&self) -> bool {
        self.sync();
        let i = self.inner.borrow();
        i.ssf() & i.shiftsien != 0 || i.shifterr & i.shifteien != 0 || i.timstat & i.timien != 0
    }
}

impl ImxrtFlexio {
    /// Recompute the cached interrupt line (see `Timebase::level`).
    fn refresh_irq(&self) {
        self.time.set_level(self.irq());
    }

    fn tick_inner(&mut self, cycles: u64) -> PeripheralTickResult {
        self.time.advance(cycles);
        self.sync();
        let now = self.time.now();
        let i = self.inner.borrow();
        let mut until: Option<u64> = None;
        for n in 0..N {
            let t = match i.sh[n].shifting {
                Some((_, done)) => Some(done),
                None if i.smod(n) == 2 && i.sh[n].full => Some(now + 1),
                None => None,
            };
            until = match (until, t) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        super::wake_hint(now, until)
    }
}

impl Peripheral for ImxrtFlexio {
    /// Walked only while timed work is in flight or the interrupt line is
    /// asserted (so its deassert is reconciled); MMIO re-arms it.
    fn legacy_tick_active(&self) -> bool {
        ({
            let i = self.inner.borrow();
            i.sh.iter().any(|s| s.shifting.is_some() || s.full)
        }) || self.time.level()
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
    fn write_u16(&mut self, offset: u64, value: u16) -> SimResult<()> {
        let shift = (offset & 3) * 8;
        self.write_reg(offset as u32, (value as u32) << shift, 0xFFFF << shift);
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
        let off = offset as u32;
        if Self::buf_window(off & !3).is_some() {
            let i = self.inner.borrow();
            let (win, n) = Self::buf_window(off & !3).unwrap();
            return Some(byte_of(alias(win, i.sh[n].buf), offset));
        }
        Some(byte_of(self.read_reg(off & !3), offset))
    }
    fn tick_elapsed(&mut self, cycles: u64) -> PeripheralTickResult {
        let r = self.tick_inner(cycles);
        self.refresh_irq();
        r
    }
    fn irq_line_level(&self) -> Option<bool> {
        Some(self.time.level())
    }
    /// Line 0: shifter 0/1 requests (DMAMUX "Request0Request1"),
    /// line 2: shifter 2/3.
    fn dma_request_active(&self, line: u8) -> bool {
        self.sync();
        let i = self.inner.borrow();
        let req = i.ssf() & i.shiftsden;
        match line {
            0 => req & 0x3 != 0,
            2 => req & 0xC != 0,
            _ => false,
        }
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
        let i = self.inner.borrow();
        serde_json::json!({
            "peripheral": "imxrt_flexio",
            "ctrl": i.ctrl,
            "words": i.wire.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleClock;

    #[test]
    fn spi_master_transmit_msb_first_through_the_bit_swapped_alias() {
        let mut f = ImxrtFlexio::default();
        let c = CycleClock::default();
        f.attach_cycle_clock(c.clone());
        // The FB200 stock configuration (FlexIO SPI master, SDK driver).
        f.write_reg(CTRL, 0xC000_0001, u32::MAX);
        f.write_reg(0x100, 0x21, u32::MAX); // SHIFTCFG0
        f.write_reg(0x080, 0x0003_0202, u32::MAX); // SHIFTCTL0: TX pin 2
        f.write_reg(0x104, 0, u32::MAX);
        f.write_reg(0x084, 0x0080_0001, u32::MAX); // SHIFTCTL1: RX
        f.write_reg(0x480, 0x0100_2222, u32::MAX); // TIMCFG0
        f.write_reg(0x500, 0x0000_0F02, u32::MAX); // TIMCMP0: 8 bits, /6
        f.write_reg(0x400, 0x01C3_0001, u32::MAX); // TIMCTL0
        assert_eq!(f.read_reg(SHIFTSTAT) & 1, 1, "TX buffer empty");
        // DMA writes one byte to SHIFTBUFBIS[0] + 3.
        f.write(0x283, 0x2A).unwrap();
        c.publish(1);
        f.tick_elapsed(0);
        let per_bit = 6 * 600 / 30;
        c.publish(1 + 8 * per_bit);
        f.tick_elapsed(0);
        let w = f.wire_log();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].beats, vec![0x2A], "MSB-first on the wire");
        assert_eq!(w[0].pin, 2);
    }
}
