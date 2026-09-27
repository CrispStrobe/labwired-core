// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! i.MX RT1060 LPUART (LPUART1..8, IMXRT1060RM §49).
//!
//! The RT LPUART carries VERID/PARAM/GLOBAL/PINCFG in front of the Kinetis
//! register block, so BAUD/STAT/CTRL/DATA sit at 0x10/0x14/0x18/0x1C (the
//! generic `Uart` model's Kinetis layout has them at 0x00..0x0C).
//!
//! Modelled: 4-entry TX and RX FIFOs (`PARAM` = 0x0202), `FIFO`/`WATER`
//! watermarks and counts, a transmitter that shifts one character per frame
//! time derived from `BAUD` (SBR/OSR) and the LPUART functional clock, `STAT`
//! TDRE/TC/RDRF/IDLE/OR with the write-1-to-clear flags, the level interrupt
//! (`CTRL` TIE/TCIE/RIE/ILIE/ORIE) and the TX/RX DMA requests (`BAUD`
//! TDMAE/RDMAE). Transmitted characters go to the console sink and the bus
//! trace; received characters are injected by the host (`push_rx`).

use super::{byte_of, Timebase};
use crate::bus::bus_trace::{BusDir, BusPayload, BusTrace};
use crate::{Peripheral, PeripheralTickResult, SimResult};
use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const VERID: u32 = 0x00;
const PARAM: u32 = 0x04;
const GLOBAL: u32 = 0x08;
const PINCFG: u32 = 0x0C;
const BAUD: u32 = 0x10;
const STAT: u32 = 0x14;
const CTRL: u32 = 0x18;
const DATA: u32 = 0x1C;
const MATCH: u32 = 0x20;
const MODIR: u32 = 0x24;
const FIFO: u32 = 0x28;
const WATER: u32 = 0x2C;

const FIFO_DEPTH: usize = 4;

// STAT
const ST_TDRE: u32 = 1 << 23;
const ST_TC: u32 = 1 << 22;
const ST_RDRF: u32 = 1 << 21;
const ST_IDLE: u32 = 1 << 20;
const ST_OR: u32 = 1 << 19;
/// Write-1-to-clear flags: LBKDIF, RXEDGIF, IDLE, OR, NF, FE, PF, MA1F, MA2F.
const ST_W1C: u32 = (1 << 31) | (1 << 30) | (0x1F << 16) | (1 << 15) | (1 << 14);
/// Plain read-write STAT bits: LBKDE, BRK13, RWUID, RXINV, MSBF.
const ST_RW: u32 = (1 << 25) | (1 << 26) | (1 << 27) | (1 << 28) | (1 << 29);

// CTRL
const CT_TIE: u32 = 1 << 23;
const CT_TCIE: u32 = 1 << 22;
const CT_RIE: u32 = 1 << 21;
const CT_ILIE: u32 = 1 << 20;
const CT_TE: u32 = 1 << 19;
const CT_RE: u32 = 1 << 18;
const CT_ORIE: u32 = 1 << 27;

// BAUD
const BD_TDMAE: u32 = 1 << 23;
const BD_RDMAE: u32 = 1 << 21;

// FIFO
const FF_RXFE: u32 = 1 << 3;
const FF_TXFE: u32 = 1 << 7;
const FF_RXFLUSH: u32 = 1 << 14;
const FF_TXFLUSH: u32 = 1 << 15;
const FF_RXEMPT: u32 = 1 << 22;
const FF_TXEMPT: u32 = 1 << 23;

/// LPUART functional clock the baud divider runs from when the chip YAML
/// does not say otherwise: PLL3 / 6 = 80 MHz (`CSCDR1.UART_CLK_SEL` = 0,
/// `UART_CLK_PODF` = 0), the MCUXpresso SDK board default.
pub const DEFAULT_UART_CLK_HZ: u64 = 80_000_000;

#[derive(Debug)]
struct Inner {
    baud: u32,
    stat: u32,
    ctrl: u32,
    matchr: u32,
    modir: u32,
    fifo: u32,
    water: u32,
    global: u32,
    pincfg: u32,
    tx_fifo: VecDeque<u8>,
    /// Character on the wire and the cycle its stop bit ends.
    shifting: Option<(u8, u64)>,
    rx_fifo: VecDeque<u8>,
    /// Last time the transmitter state was brought forward.
    synced: u64,
    /// Bytes completed but not yet handed to the sink (drained on `&mut`).
    tx_done: Vec<u8>,
}

impl Inner {
    fn new() -> Self {
        Self {
            baud: 0x0F00_0004,
            stat: 0x00C0_0000,
            ctrl: 0,
            matchr: 0,
            modir: 0,
            fifo: 0x00C0_0011,
            water: 0,
            global: 0,
            pincfg: 0,
            tx_fifo: VecDeque::new(),
            shifting: None,
            rx_fifo: VecDeque::new(),
            synced: 0,
            tx_done: Vec::new(),
        }
    }

    fn tx_depth(&self) -> usize {
        if self.fifo & FF_TXFE != 0 {
            FIFO_DEPTH
        } else {
            1
        }
    }
    fn rx_depth(&self) -> usize {
        if self.fifo & FF_RXFE != 0 {
            FIFO_DEPTH
        } else {
            1
        }
    }
    fn tdre(&self) -> bool {
        if self.fifo & FF_TXFE != 0 {
            self.tx_fifo.len() <= (self.water & 0x3) as usize
        } else {
            self.tx_fifo.is_empty()
        }
    }
    fn tc(&self) -> bool {
        self.tx_fifo.is_empty() && self.shifting.is_none()
    }
    fn rdrf(&self) -> bool {
        if self.fifo & FF_RXFE != 0 {
            self.rx_fifo.len() > ((self.water >> 16) & 0x3) as usize
        } else {
            !self.rx_fifo.is_empty()
        }
    }
    fn stat_view(&self) -> u32 {
        let mut s = self.stat & (ST_W1C | ST_RW);
        if self.tdre() {
            s |= ST_TDRE;
        }
        if self.tc() {
            s |= ST_TC;
        }
        if self.rdrf() {
            s |= ST_RDRF;
        }
        s
    }
    fn irq(&self) -> bool {
        let s = self.stat_view();
        (self.ctrl & CT_TIE != 0 && s & ST_TDRE != 0)
            || (self.ctrl & CT_TCIE != 0 && s & ST_TC != 0)
            || (self.ctrl & CT_RIE != 0 && s & ST_RDRF != 0)
            || (self.ctrl & CT_ILIE != 0 && s & ST_IDLE != 0)
            || (self.ctrl & CT_ORIE != 0 && s & ST_OR != 0)
    }
}

#[derive(Debug)]
pub struct ImxrtLpuart {
    inner: RefCell<Inner>,
    time: Timebase,
    uart_clk_hz: u64,
    sink: Option<Arc<Mutex<Vec<u8>>>>,
    echo_stdout: bool,
    trace: BusTrace,
    trace_name: String,
    /// Every byte this instance transmitted (for inspection and tests).
    tx_log: Vec<u8>,
}

impl Default for ImxrtLpuart {
    fn default() -> Self {
        Self::new(DEFAULT_UART_CLK_HZ)
    }
}

impl ImxrtLpuart {
    pub fn new(uart_clk_hz: u64) -> Self {
        Self {
            inner: RefCell::new(Inner::new()),
            time: Timebase::default(),
            uart_clk_hz: uart_clk_hz.max(1),
            sink: None,
            echo_stdout: false,
            trace: BusTrace::default(),
            trace_name: "lpuart".into(),
            tx_log: Vec::new(),
        }
    }

    pub fn set_sink(&mut self, sink: Option<Arc<Mutex<Vec<u8>>>>, echo_stdout: bool) {
        self.sink = sink;
        self.echo_stdout = echo_stdout;
    }

    /// Everything this LPUART has put on its TX line so far.
    pub fn tx_log(&self) -> &[u8] {
        &self.tx_log
    }

    /// Host -> device: a character arrives on RX. Returns false (overrun)
    /// when the receiver is off or the FIFO is full.
    pub fn push_rx(&mut self, byte: u8) -> bool {
        let mut i = self.inner.borrow_mut();
        if i.ctrl & CT_RE == 0 {
            return false;
        }
        if i.rx_fifo.len() >= i.rx_depth() {
            i.stat |= ST_OR;
            return false;
        }
        i.rx_fifo.push_back(byte);
        self.trace.push(
            &self.trace_name,
            BusPayload::Uart {
                direction: BusDir::Rx,
                byte,
            },
        );
        true
    }

    /// CPU cycles one character occupies on the wire:
    /// (start + 8/9/10 data + parity + stop) bits * (OSR+1) * SBR / clk.
    fn frame_cycles(&self, i: &Inner) -> u64 {
        let sbr = (i.baud & 0x1FFF).max(1) as u64;
        let osr = (((i.baud >> 24) & 0x1F) as u64).max(3) + 1;
        let data_bits = if i.ctrl & (1 << 4) != 0 { 9 } else { 8 };
        let parity = (i.ctrl >> 1) & 1;
        let stop = if i.baud & (1 << 13) != 0 { 2 } else { 1 };
        let bits = 1 + data_bits + parity as u64 + stop;
        let per_bit = osr * sbr * self.time.cpu_hz() / self.uart_clk_hz;
        (bits * per_bit).max(1)
    }

    /// Bring the transmitter forward to `now`.
    fn sync(&self) {
        let now = self.time.now();
        let mut i = self.inner.borrow_mut();
        let frame = self.frame_cycles(&i);
        loop {
            match i.shifting {
                Some((b, end)) if end <= now => {
                    i.tx_done.push(b);
                    i.shifting = None;
                    if let Some(next) = i.tx_fifo.pop_front() {
                        i.shifting = Some((next, end + frame));
                    }
                }
                None => {
                    if i.ctrl & CT_TE != 0 {
                        if let Some(next) = i.tx_fifo.pop_front() {
                            i.shifting = Some((next, now + frame));
                            continue;
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        i.synced = now;
    }

    fn flush_tx_done(&mut self) {
        let done = std::mem::take(&mut self.inner.borrow_mut().tx_done);
        for b in done {
            self.tx_log.push(b);
            if let Some(sink) = &self.sink {
                if let Ok(mut s) = sink.lock() {
                    s.push(b);
                }
            }
            if self.echo_stdout {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&[b]);
            }
            self.trace.push(
                &self.trace_name,
                BusPayload::Uart {
                    direction: BusDir::Tx,
                    byte: b,
                },
            );
        }
    }

    pub fn read_reg(&self, off: u32) -> u32 {
        self.sync();
        let mut i = self.inner.borrow_mut();
        match off & !3 {
            VERID => 0x0401_0003,
            PARAM => 0x0000_0202,
            GLOBAL => i.global,
            PINCFG => i.pincfg,
            BAUD => i.baud,
            STAT => i.stat_view(),
            CTRL => i.ctrl,
            DATA => match i.rx_fifo.pop_front() {
                Some(b) => b as u32,
                None => 1 << 12, // RXEMPT
            },
            MATCH => i.matchr,
            MODIR => i.modir,
            FIFO => {
                let mut v = i.fifo & !(FF_RXEMPT | FF_TXEMPT);
                if i.rx_fifo.is_empty() {
                    v |= FF_RXEMPT;
                }
                if i.tx_fifo.is_empty() {
                    v |= FF_TXEMPT;
                }
                v
            }
            WATER => {
                (i.water & 0x0003_0003)
                    | ((i.tx_fifo.len() as u32) << 8)
                    | ((i.rx_fifo.len() as u32) << 24)
            }
            _ => 0,
        }
    }

    pub fn write_reg(&mut self, off: u32, value: u32, mask: u32) {
        self.sync();
        {
            let mut i = self.inner.borrow_mut();
            let v = value & mask;
            let merge = |old: u32| (old & !mask) | v;
            match off & !3 {
                GLOBAL => {
                    i.global = merge(i.global) & 0x2;
                    if i.global & 0x2 != 0 {
                        // Software reset: every register but GLOBAL to reset.
                        let g = i.global;
                        *i = Inner::new();
                        i.global = g;
                    }
                }
                PINCFG => i.pincfg = merge(i.pincfg) & 0x3,
                BAUD => i.baud = merge(i.baud),
                STAT => {
                    let clr = v & ST_W1C;
                    i.stat &= !clr;
                    i.stat = (i.stat & !(ST_RW & mask)) | (v & ST_RW);
                }
                CTRL => {
                    i.ctrl = merge(i.ctrl);
                    if i.ctrl & CT_TE == 0 {
                        // Transmitter off: the queue does not drain.
                    }
                }
                DATA => {
                    if mask & 0xFF != 0 {
                        let depth = i.tx_depth();
                        if i.tx_fifo.len() < depth {
                            i.tx_fifo.push_back(v as u8);
                        } else {
                            // TXOF: TX FIFO overflow.
                            i.fifo |= 1 << 17;
                        }
                    }
                }
                MATCH => i.matchr = merge(i.matchr),
                MODIR => i.modir = merge(i.modir),
                FIFO => {
                    // TXOF/RXUF are w1c; flush bits act and read as zero.
                    let w1c = v & ((1 << 16) | (1 << 17));
                    let keep = merge(i.fifo) & !(FF_RXFLUSH | FF_TXFLUSH) & !((1 << 16) | (1 << 17));
                    i.fifo = keep | (i.fifo & ((1 << 16) | (1 << 17)) & !w1c);
                    if v & FF_TXFLUSH != 0 {
                        i.tx_fifo.clear();
                    }
                    if v & FF_RXFLUSH != 0 {
                        i.rx_fifo.clear();
                    }
                }
                WATER => i.water = merge(i.water) & 0x0003_0003,
                _ => {}
            }
        }
        self.sync();
        self.flush_tx_done();
    }

    /// DMA request line: 0 = TX (TDMAE & TDRE), 1 = RX (RDMAE & RDRF).
    pub fn dma_request(&self, line: u8) -> bool {
        self.sync();
        let i = self.inner.borrow();
        match line {
            0 => i.baud & BD_TDMAE != 0 && i.ctrl & CT_TE != 0 && i.tdre(),
            1 => i.baud & BD_RDMAE != 0 && i.rdrf(),
            _ => false,
        }
    }
}

impl Peripheral for ImxrtLpuart {
    fn read(&self, offset: u64) -> SimResult<u8> {
        // Byte reads of DATA pop the FIFO once (the low byte).
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
        let off = offset as u32 & !3;
        if off == DATA {
            let i = self.inner.borrow();
            return Some(i.rx_fifo.front().copied().unwrap_or(0));
        }
        Some(byte_of(self.read_reg(off), offset))
    }
    fn tick_elapsed(&mut self, cycles: u64) -> PeripheralTickResult {
        self.time.advance(cycles);
        self.sync();
        self.flush_tx_done();
        PeripheralTickResult::default()
    }
    fn irq_line_level(&self) -> Option<bool> {
        self.sync();
        Some(self.inner.borrow().irq())
    }
    fn attach_cycle_clock(&mut self, clock: crate::CycleClock) {
        self.time.attach_clock(clock);
    }
    fn attach_cpu_hz(&mut self, hz: u64) {
        self.time.attach_cpu_hz(hz);
    }
    fn attach_bus_trace(&mut self, name: &str, trace: &BusTrace) {
        self.trace = trace.clone();
        self.trace_name = name.to_string();
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
            "peripheral": "imxrt_lpuart",
            "baud": i.baud,
            "ctrl": i.ctrl,
            "stat": i.stat_view(),
            "tx_bytes": self.tx_log.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleClock;

    fn uart() -> (ImxrtLpuart, CycleClock) {
        let mut u = ImxrtLpuart::new(DEFAULT_UART_CLK_HZ);
        let c = CycleClock::default();
        u.attach_cycle_clock(c.clone());
        (u, c)
    }

    #[test]
    fn transmit_takes_a_frame_time_then_sets_tc() {
        let (mut u, c) = uart();
        // 115200 baud at 80 MHz: OSR=15 (16x), SBR = 43.
        u.write_reg(BAUD, (15 << 24) | 43, u32::MAX);
        u.write_reg(CTRL, CT_TE | CT_RE, u32::MAX);
        assert_ne!(u.read_reg(STAT) & ST_TC, 0);
        u.write_reg(DATA, b'A' as u32, u32::MAX);
        assert_eq!(u.read_reg(STAT) & ST_TC, 0, "shifting");
        assert_ne!(u.read_reg(STAT) & ST_TDRE, 0, "buffer free again");
        let frame = 10 * 16 * 43 * 600 / 80;
        c.publish(frame as u64);
        assert_ne!(u.read_reg(STAT) & ST_TC, 0);
        u.write_reg(CTRL, CT_TE | CT_RE, u32::MAX);
        assert_eq!(u.tx_log(), b"A");
    }

    #[test]
    fn rx_fifo_and_rxempt() {
        let (mut u, _) = uart();
        u.write_reg(CTRL, CT_RE | CT_RIE, u32::MAX);
        assert_eq!(u.read_reg(DATA), 1 << 12);
        assert!(!u.irq_line_level().unwrap());
        assert!(u.push_rx(0x42));
        assert!(u.irq_line_level().unwrap());
        assert_ne!(u.read_reg(STAT) & ST_RDRF, 0);
        assert_eq!(u.read_reg(DATA), 0x42);
        assert!(!u.irq_line_level().unwrap());
    }
}
