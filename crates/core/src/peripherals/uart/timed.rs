// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Timed mode of the STM32 (F1/F2/F4 register layout) USART, used when the
//! USART is attached to a [`TimedUartNet`](crate::network::timed_uart) link.
//!
//! The default model hands a written byte to the sink at once and keeps TXE
//! set forever, and it queues received bytes without bound. That is fine for
//! a console and wrong for a network. In timed mode the USART is the one on
//! the silicon (RM0368 §19, STM32F401; the F1/F2/F4 USARTs share it):
//!
//! * **TX:** a transmit data register (TDR) in front of a shift register. A
//!   write to DR fills TDR (TXE = 0). When the shifter is free the character
//!   moves into it (TXE = 1 again) and takes one frame on the wire at the
//!   programmed baud rate; TC = 1 only when both are empty. A write while
//!   TXE = 0 overwrites TDR, as on silicon.
//! * **RX:** one data register. RXNE is set at the middle of the stop bit.
//!   A character that completes while RXNE is still set is lost and sets ORE
//!   (the data register keeps the older character). FE and PE are set with
//!   RXNE for that character. ORE, FE and PE clear by a read of SR followed
//!   by a read of DR; RXNE clears by a read of DR.
//! * **Interrupt:** `RXNEIE & (RXNE | ORE) | TXEIE & TXE | TCIE & TC |
//!   PEIE & PE | EIE & (FE | ORE)`. It is raised on each rising edge of that
//!   line (the edge pends the NVIC), and reported as a level to the bus so a
//!   write that lowers it drops the pend.
//!
//! Not modelled here, and said so: DMA reception (CR3.DMAR — recorded on the
//! timeline once when set), the IDLE and LIN-break flags, hardware flow
//! control, 9-bit transmit data (the high bit of DR is not captured), and the
//! APB prescaler (BRR is read in core clock cycles — true when PCLK = HCLK).

use super::Uart;
use crate::network::timed_uart::{LineFormat, Parity, TimedUartPort};
use std::sync::Mutex;

// CR1 (RM0368 §19.6.4).
const CR1_RE: u32 = 1 << 2;
const CR1_TE: u32 = 1 << 3;
const CR1_RXNEIE: u32 = 1 << 5;
const CR1_TCIE: u32 = 1 << 6;
const CR1_TXEIE: u32 = 1 << 7;
const CR1_PEIE: u32 = 1 << 8;
const CR1_PS: u32 = 1 << 9;
const CR1_PCE: u32 = 1 << 10;
const CR1_M: u32 = 1 << 12;
const CR1_UE: u32 = 1 << 13;
const CR1_OVER8: u32 = 1 << 15;
// CR3.
const CR3_EIE: u32 = 1 << 0;
const CR3_DMAR: u32 = 1 << 6;
// SR.
const SR_PE: u8 = 1 << 0;
const SR_FE: u8 = 1 << 1;
const SR_ORE: u8 = 1 << 3;
const SR_RXNE: u8 = 1 << 5;
const SR_TC: u8 = 1 << 6;
const SR_TXE: u8 = 1 << 7;

/// Registers the timed mode adds, behind a lock because `read` is `&self`
/// and a DR read has side effects (it clears RXNE, and ORE/FE/PE after an SR
/// read).
#[derive(Debug, Default)]
pub(crate) struct TimedRegs {
    tdr: Option<u16>,
    tdr_written_cycle: u64,
    /// The shifter holds a character until this cycle.
    shift_busy_until: u64,
    tx_active: bool,
    /// TC was cleared by software (write 0 to SR.TC) and has not been set by
    /// a later transmission end.
    tc_cleared: bool,
    rdr: u16,
    rxne: bool,
    ore: bool,
    fe: bool,
    pe: bool,
    /// SR was read while ORE/FE/PE were set: the next DR read clears them.
    sr_read_armed: bool,
    irq_prev: bool,
    /// The cycle our pending scheduler wake is for, if any.
    next_wake: Option<u64>,
    dmar_noted: bool,
}

#[derive(Debug)]
pub(crate) struct TimedUart {
    pub(crate) port: TimedUartPort,
    pub(crate) regs: Mutex<TimedRegs>,
}

impl TimedRegs {
    fn tc(&self) -> bool {
        self.tdr.is_none() && !self.tx_active && !self.tc_cleared
    }
    fn sr(&self) -> u8 {
        let mut v = 0;
        if self.pe {
            v |= SR_PE;
        }
        if self.fe {
            v |= SR_FE;
        }
        if self.ore {
            v |= SR_ORE;
        }
        if self.rxne {
            v |= SR_RXNE;
        }
        if self.tc() {
            v |= SR_TC;
        }
        if self.tdr.is_none() {
            v |= SR_TXE;
        }
        v
    }
    fn level(&self, cr1: u32, cr3: u32) -> bool {
        (cr1 & CR1_RXNEIE != 0 && (self.rxne || self.ore))
            || (cr1 & CR1_TXEIE != 0 && self.tdr.is_none())
            || (cr1 & CR1_TCIE != 0 && self.tc())
            || (cr1 & CR1_PEIE != 0 && self.pe)
            || (cr3 & CR3_EIE != 0 && (self.fe || self.ore))
    }
}

impl Uart {
    /// Put this USART on a timed link. Only the STM32 F1/F2/F4 register
    /// layout has a timed mode.
    pub fn attach_timed_port(&mut self, port: TimedUartPort) -> anyhow::Result<()> {
        if !matches!(self.layout, super::UartRegisterLayout::Stm32F1) {
            anyhow::bail!(
                "a timed UART link needs the STM32 F1/F2/F4 USART register layout; this UART is {:?}",
                self.layout
            );
        }
        // The console sink would splice protocol bytes into the log.
        self.set_sink(None, false);
        self.timed = Some(Box::new(TimedUart {
            port,
            regs: Mutex::new(TimedRegs::default()),
        }));
        self.timed_publish();
        Ok(())
    }

    pub fn is_timed(&self) -> bool {
        self.timed.is_some()
    }

    pub(crate) fn timed_now(&self) -> u64 {
        self.stream_clock.as_ref().map_or(0, |c| c.now())
    }

    /// Bit period in core cycles, from BRR (16× or 8× oversampling).
    fn timed_bit_cycles(&self) -> u64 {
        let brr = u64::from(self.brr & 0xFFFF);
        if self.cr1 & CR1_OVER8 != 0 {
            ((brr & 0xFFF0) | ((brr & 0x7) << 1)) / 2
        } else {
            brr
        }
    }

    /// The line format the registers program.
    pub(crate) fn timed_format(&self) -> LineFormat {
        let Some(t) = &self.timed else {
            return LineFormat::default();
        };
        let bit_cycles = self.timed_bit_cycles();
        let hz = t.port.hz();
        let bit_ps = if bit_cycles >= 16 && hz > 0 {
            crate::network::timed_uart::cycles_to_ps(bit_cycles, hz)
        } else {
            0
        };
        let word = if self.cr1 & CR1_M != 0 { 9 } else { 8 };
        let (data_bits, parity) = if self.cr1 & CR1_PCE != 0 {
            (
                word - 1,
                if self.cr1 & CR1_PS != 0 {
                    Parity::Odd
                } else {
                    Parity::Even
                },
            )
        } else {
            (word, Parity::None)
        };
        let stop_half_bits = match (self.cr2 >> 12) & 3 {
            0 => 2,
            1 => 1,
            2 => 4,
            _ => 3,
        };
        let ue = self.cr1 & CR1_UE != 0;
        LineFormat {
            bit_ps,
            data_bits,
            parity,
            stop_half_bits,
            rx_enabled: ue && self.cr1 & CR1_RE != 0,
            tx_enabled: ue && self.cr1 & CR1_TE != 0,
        }
    }

    fn timed_frame_cycles(&self, fmt: &LineFormat) -> u64 {
        let bit = self.timed_bit_cycles();
        bit * fmt.bits_before_stop() + bit * u64::from(fmt.stop_half_bits) / 2
    }

    pub(crate) fn timed_publish(&self) {
        if let Some(t) = &self.timed {
            t.port.publish_format(self.timed_format());
        }
    }

    /// `read` of SR/DR in timed mode; `None` for any other offset.
    pub(crate) fn timed_read(&self, offset: u64) -> Option<u8> {
        let t = self.timed.as_ref()?;
        let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        match offset {
            0x00 => {
                let sr = r.sr();
                if sr & (SR_ORE | SR_FE | SR_PE) != 0 {
                    r.sr_read_armed = true;
                }
                Some(sr)
            }
            0x01..=0x03 => Some(0),
            0x04 => {
                let v = (r.rdr & 0xFF) as u8;
                r.rxne = false;
                if r.sr_read_armed {
                    r.ore = false;
                    r.fe = false;
                    r.pe = false;
                    r.sr_read_armed = false;
                }
                let level = r.level(self.cr1, self.cr3);
                r.irq_prev &= level;
                Some(v)
            }
            0x05 => Some(((r.rdr >> 8) & 1) as u8),
            0x06 | 0x07 => Some(0),
            _ => None,
        }
    }

    /// Side-effect-free SR/DR view for a debugger.
    pub(crate) fn timed_peek(&self, offset: u64) -> Option<u8> {
        let t = self.timed.as_ref()?;
        let r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        match offset {
            0x00 => Some(r.sr()),
            0x01..=0x03 | 0x06 | 0x07 => Some(0),
            0x04 => Some((r.rdr & 0xFF) as u8),
            0x05 => Some(((r.rdr >> 8) & 1) as u8),
            _ => None,
        }
    }

    /// `write` of SR/DR in timed mode. Returns false for other offsets (the
    /// config registers go through the shared F1 path).
    pub(crate) fn timed_write(&mut self, offset: u64, value: u8) -> bool {
        let now = self.timed_now();
        let (cr1, cr3) = (self.cr1, self.cr3);
        let fmt = self.timed_format();
        let Some(t) = &self.timed else {
            return false;
        };
        let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        match offset {
            // SR: RXNE and TC are rc_w0 — writing 0 clears them.
            0x00 => {
                if value & SR_RXNE == 0 {
                    r.rxne = false;
                }
                if value & SR_TC == 0 && r.tc() {
                    r.tc_cleared = true;
                }
            }
            0x01..=0x03 | 0x05..=0x07 => {}
            0x04 => {
                if fmt.tx_enabled {
                    // TXE = 0 overwrites TDR: the earlier character is lost.
                    r.tdr = Some(u16::from(value));
                    r.tdr_written_cycle = now;
                    r.tc_cleared = false;
                }
            }
            _ => return false,
        }
        let level = r.level(cr1, cr3);
        r.irq_prev &= level;
        true
    }

    /// After a config register write: publish the new format and note DMA
    /// reception once, since timed mode does not carry it out.
    pub(crate) fn timed_config_written(&mut self) {
        self.timed_publish();
        let now = self.timed_now();
        let cr3 = self.cr3;
        if let Some(t) = &self.timed {
            let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
            if cr3 & CR3_DMAR != 0 && !r.dmar_noted {
                r.dmar_noted = true;
                t.port
                    .note_unmodelled(now, "USART DMA reception (CR3.DMAR)");
            }
        }
    }

    /// Bring the USART up to `now`: finish or start transmissions, receive
    /// due characters. Returns true on a rising edge of the interrupt line.
    pub(crate) fn timed_service(&mut self, now: u64) -> bool {
        let fmt = self.timed_format();
        let frame_cycles = self.timed_frame_cycles(&fmt);
        let (cr1, cr3) = (self.cr1, self.cr3);
        let Some(t) = &self.timed else {
            return false;
        };
        let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        // TX: a finished character frees the shifter; a waiting TDR moves in.
        if r.tx_active && now >= r.shift_busy_until {
            r.tx_active = false;
        }
        if let Some(value) = r.tdr {
            if !r.tx_active {
                let start = r.shift_busy_until.max(r.tdr_written_cycle + 1);
                if start <= now {
                    r.tdr = None;
                    if fmt.tx_enabled && fmt.configured() {
                        t.port.transmit(start, value, fmt);
                        r.tx_active = true;
                        r.shift_busy_until = start + frame_cycles;
                        if now >= r.shift_busy_until {
                            r.tx_active = false;
                        }
                    }
                }
            }
        }
        // RX.
        let (loaded, overruns) = t.port.receive_due(now, &fmt, r.rxne);
        if overruns > 0 {
            r.ore = true;
        }
        if let Some(d) = loaded {
            r.rdr = d.value;
            r.rxne = true;
            r.fe = d.framing_error;
            r.pe = d.parity_error;
        }
        let level = r.level(cr1, cr3);
        let rising = level && !r.irq_prev;
        r.irq_prev = level;
        if rising && cr1 & CR1_RXNEIE != 0 && (r.rxne || r.ore) {
            t.port.note_rx_irq(now, if r.ore { "ore" } else { "rxne" });
        }
        rising
    }

    /// The next cycle after `now` this USART must be serviced at, if any.
    pub(crate) fn timed_due(&self, now: u64) -> Option<u64> {
        let fmt = self.timed_format();
        let t = self.timed.as_ref()?;
        let r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        let mut due: Option<u64> = None;
        let mut want = |c: u64| {
            let c = c.max(now + 1);
            due = Some(due.map_or(c, |d| d.min(c)));
        };
        if r.tx_active {
            want(r.shift_busy_until);
        }
        if r.tdr.is_some() && !r.tx_active {
            want(r.shift_busy_until.max(r.tdr_written_cycle + 1));
        }
        drop(r);
        if let Some(c) = t.port.next_due_cycle(&fmt) {
            want(c);
        }
        due
    }

    /// Ask for a wake at the next due cycle unless an earlier (or equal) one
    /// is already pending. Returns the cycle to schedule.
    pub(crate) fn timed_claim_wake(&self, now: u64) -> Option<u64> {
        let target = self.timed_due(now)?;
        let t = self.timed.as_ref()?;
        let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        let pending = r.next_wake.filter(|w| *w > now);
        if pending.is_some_and(|w| w <= target) {
            return None;
        }
        r.next_wake = Some(target);
        Some(target)
    }

    /// The interrupt line as a level, for the bus's NVIC reconcile.
    pub(crate) fn timed_level(&self) -> Option<bool> {
        let t = self.timed.as_ref()?;
        let r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
        Some(r.level(self.cr1, self.cr3))
    }

    /// Node reset (NRST/SYSRESETREQ): every USART register returns to its
    /// reset value, a character being shifted out stops mid-way.
    pub(crate) fn timed_reset(&mut self) {
        let now = self.timed_now();
        if let Some(t) = &self.timed {
            let mut r = t.regs.lock().unwrap_or_else(|e| e.into_inner());
            if r.tx_active && now < r.shift_busy_until {
                t.port.abort_tx(now);
            }
            *r = TimedRegs::default();
        }
        self.cr1 = 0;
        self.cr2 = 0;
        self.cr3 = 0;
        self.brr = 0;
        self.gtpr = 0;
        self.timed_publish();
    }
}
