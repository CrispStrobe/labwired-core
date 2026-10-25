// SPDX-License-Identifier: MIT
//! Opt-in SAMD51 SERCOM I2C master slice (DS60001507F 36): 7-bit addressing,
//! writes, smart-mode reads, repeated start, STOP and the MB/SB/RXNACK
//! handshake against attached `I2cDevice` slaves. Each bus operation completes
//! on the next bus tick and only while the SERCOM's bus clock and core generic
//! clock run and a PAD0 (SDA) and PAD1 (SCL) pad are muxed to the SERCOM; an
//! address nobody claims is NACKed. Bus timing, clock
//! stretching, arbitration, timeouts, 10-bit/high-speed modes, DMA and the
//! slave mode are not modelled; their settings are refused.
use crate::peripherals::device::{I2cDevice, I2cSlaveHost};
use crate::{Bus, Peripheral, PeripheralTickResult, SimResult, SimulationError};
use std::any::Any;
use std::cell::{Cell, RefCell};

const MB: u8 = 1;
const SB: u8 = 1 << 1;
const ERROR: u8 = 1 << 7;
const RXNACK: u16 = 1 << 2;
const BUSSTATE: u16 = 3 << 4;
const IDLE: u16 = 1 << 4;
const OWNER: u16 = 2 << 4;
const MODE_I2C_MASTER: u32 = 5 << 2;

#[derive(Clone, Copy, Debug)]
pub struct SercomInstance {
    pub base: u64,
    pub first_irq: u32,
    pub core_gclk: u64,
    /// (MCLK mask register offset, bit)
    pub bus_clock: (u64, u32),
}

/// SERCOM0..SERCOM5 on the SAMD51J19A (ATDF instance parameters).
pub const SERCOM_INSTANCES: [SercomInstance; 6] = [
    SercomInstance {
        base: 0x40003000,
        first_irq: 46,
        core_gclk: 7,
        bus_clock: (0x14, 12),
    },
    SercomInstance {
        base: 0x40003400,
        first_irq: 50,
        core_gclk: 8,
        bus_clock: (0x14, 13),
    },
    SercomInstance {
        base: 0x41012000,
        first_irq: 54,
        core_gclk: 23,
        bus_clock: (0x18, 9),
    },
    SercomInstance {
        base: 0x41014000,
        first_irq: 58,
        core_gclk: 24,
        bus_clock: (0x18, 10),
    },
    SercomInstance {
        base: 0x43000000,
        first_irq: 62,
        core_gclk: 34,
        bus_clock: (0x20, 0),
    },
    SercomInstance {
        base: 0x43000400,
        first_irq: 66,
        core_gclk: 35,
        bus_clock: (0x20, 1),
    },
];

/// (SERCOM, PAD, port, pin, PMUX function) for SERCOM PAD0/PAD1 on the
/// ATSAMD51J19A (ATDF, Microchip SAMD51 DFP 3.9.276, Apache-2.0); C = 2, D = 3.
const PADS: [(u8, u8, u8, u8, u8); 30] = [
    (0, 0, 0, 4, 3),
    (0, 0, 0, 8, 2),
    (0, 1, 0, 5, 3),
    (0, 1, 0, 9, 2),
    (1, 0, 0, 0, 3),
    (1, 0, 0, 16, 2),
    (1, 1, 0, 1, 3),
    (1, 1, 0, 17, 2),
    (2, 0, 0, 9, 3),
    (2, 0, 0, 12, 2),
    (2, 1, 0, 8, 3),
    (2, 1, 0, 13, 2),
    (3, 0, 0, 17, 3),
    (3, 0, 0, 22, 2),
    (3, 1, 0, 16, 3),
    (3, 1, 0, 23, 2),
    (4, 0, 0, 13, 3),
    (4, 0, 1, 8, 3),
    (4, 0, 1, 12, 2),
    (4, 1, 0, 12, 3),
    (4, 1, 1, 9, 3),
    (4, 1, 1, 13, 2),
    (5, 0, 0, 23, 3),
    (5, 0, 1, 2, 3),
    (5, 0, 1, 16, 2),
    (5, 0, 1, 31, 3),
    (5, 1, 0, 22, 3),
    (5, 1, 1, 3, 3),
    (5, 1, 1, 17, 2),
    (5, 1, 1, 30, 3),
];

/// True when some pad carrying `pad` of SERCOM `sercom` is muxed to it.
fn pad_routed(bus: &dyn Bus, sercom: u8, pad: u8) -> bool {
    PADS.iter()
        .filter(|r| r.0 == sercom && r.1 == pad)
        .any(|&(_, _, port, pin, function)| {
            let group = 0x41008000 + 0x80 * u64::from(port);
            let pincfg = bus.read_u8(group + 0x40 + u64::from(pin)).unwrap_or(0);
            let pmux = bus.read_u8(group + 0x30 + u64::from(pin / 2)).unwrap_or(0);
            let selected = if pin & 1 != 0 { pmux >> 4 } else { pmux & 0xf };
            pincfg & 1 != 0 && selected == function
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Address(u8),
    Write(u8),
    ReadNext,
    Stop,
}

pub struct Samd51SercomI2cm {
    instance: SercomInstance,
    ctrla: u32,
    ctrlb: u32,
    baud: u32,
    inten: u8,
    intflag: Cell<u8>,
    status: u16,
    addr: u32,
    data: Cell<u8>,
    reading: bool,
    selected: Option<usize>,
    pending: Cell<Option<Op>>,
    devices: Vec<RefCell<Box<dyn I2cDevice>>>,
    transactions: u64,
    /// The first address bytes put on the bus (evidence, bounded).
    addresses: Vec<u8>,
}

impl Samd51SercomI2cm {
    pub fn new(instance: SercomInstance) -> Self {
        Self {
            instance,
            ctrla: 0,
            ctrlb: 0,
            baud: 0,
            inten: 0,
            intflag: Cell::new(0),
            status: 0,
            addr: 0,
            data: Cell::new(0),
            reading: false,
            selected: None,
            pending: Cell::new(None),
            devices: Vec::new(),
            transactions: 0,
            addresses: Vec::new(),
        }
    }
    fn refuse<T>(what: &str) -> SimResult<T> {
        Err(SimulationError::NotImplemented(format!(
            "samd51 SERCOM I2C master slice: {what}"
        )))
    }
    fn enabled(&self) -> bool {
        self.ctrla & 2 != 0
    }
    pub fn transactions(&self) -> u64 {
        self.transactions
    }
    fn reset(&mut self) {
        // SWRST resets registers; attached devices and the run's transaction
        // count (evidence, not register state) survive it.
        let devices = std::mem::take(&mut self.devices);
        let (transactions, addresses) = (self.transactions, std::mem::take(&mut self.addresses));
        *self = Self::new(self.instance);
        self.devices = devices;
        self.transactions = transactions;
        self.addresses = addresses;
    }
    fn stop_selected(&mut self) {
        if let Some(i) = self.selected.take() {
            self.devices[i].borrow_mut().stop();
        }
        self.reading = false;
        self.status = (self.status & !BUSSTATE) | IDLE;
    }
    fn perform(&mut self, op: Op) {
        match op {
            Op::Address(byte) => {
                if let Some(i) = self.selected.take() {
                    self.devices[i].borrow_mut().stop(); // repeated start ends the previous transfer
                }
                let address = byte >> 1;
                self.reading = byte & 1 != 0;
                self.status = (self.status & !(BUSSTATE | RXNACK)) | OWNER;
                self.transactions += 1;
                if self.addresses.len() < 32 {
                    self.addresses.push(byte);
                }
                match self
                    .devices
                    .iter()
                    .position(|d| d.borrow().claims_address(address))
                {
                    Some(i) => {
                        let mut d = self.devices[i].borrow_mut();
                        d.select_address(address);
                        d.start();
                        if self.reading {
                            self.data.set(d.read());
                            self.intflag.set(self.intflag.get() | SB);
                        } else {
                            self.intflag.set(self.intflag.get() | MB);
                        }
                        drop(d);
                        self.selected = Some(i);
                    }
                    None => {
                        // Address NACK: MB with RXNACK, for both directions.
                        self.status |= RXNACK;
                        self.intflag.set(self.intflag.get() | MB);
                    }
                }
            }
            Op::Write(byte) => match self.selected {
                Some(i) if !self.reading => {
                    self.devices[i].borrow_mut().write(byte);
                    self.status &= !RXNACK;
                    self.intflag.set(self.intflag.get() | MB);
                }
                _ => self.intflag.set(self.intflag.get() | ERROR),
            },
            Op::ReadNext => {
                if let (Some(i), true) = (self.selected, self.reading) {
                    self.data.set(self.devices[i].borrow_mut().read());
                    self.intflag.set(self.intflag.get() | SB);
                }
            }
            Op::Stop => self.stop_selected(),
        }
    }
    fn command(&mut self, cmd: u32) {
        match cmd {
            2 if self.reading => self.pending.set(Some(Op::ReadNext)),
            3 => self.pending.set(Some(Op::Stop)),
            _ => {}
        }
    }
    fn lines(&self) -> u8 {
        if self.enabled() {
            self.intflag.get() & self.inten
        } else {
            0
        }
    }
    fn read_reg(&self, offset: u64) -> SimResult<u32> {
        Ok(match offset {
            0x00 => self.ctrla,
            0x04 => self.ctrlb,
            0x08 => 0,
            0x0c => self.baud,
            0x14 | 0x16 => u32::from(self.inten),
            0x18 => u32::from(self.intflag.get()),
            0x1a => u32::from(self.status),
            0x1c => 0, // SYNCBUSY: SWRST/ENABLE/SYSOP complete immediately
            0x24 => self.addr,
            0x28 => {
                // Smart mode: reading DATA acknowledges and fetches the next
                // byte unless ACKACT selects NACK for the last one.
                let value = self.data.get();
                if self.ctrlb & (1 << 8) != 0 && self.reading && self.ctrlb & (1 << 18) == 0 {
                    self.intflag.set(self.intflag.get() & !SB);
                    self.pending.set(Some(Op::ReadNext));
                }
                u32::from(value)
            }
            0x30 => 0,
            _ => return Self::refuse("register"),
        })
    }
    fn write_reg(&mut self, offset: u64, value: u32) -> SimResult<()> {
        match offset {
            0x00 if value & 1 != 0 => self.reset(),
            0x00 => {
                // I2C master mode; SDAHOLD and RUNSTDBY admitted; PINOUT, timeouts,
                // SCLSM and speed modes refused.
                if value & 0x1c != MODE_I2C_MASTER || value & !(0x0030_0000 | 0x80 | 0x1e) != 0 {
                    return Self::refuse("CTRLA outside standard-mode I2C master");
                }
                if self.enabled() && (value ^ self.ctrla) & !2 != 0 {
                    return Self::refuse("CTRLA is enable-protected");
                }
                if value & 2 == 0 {
                    self.stop_selected();
                    self.status &= !BUSSTATE; // disabled: bus state unknown
                }
                self.ctrla = value;
            }
            0x04 => {
                if value & !((1 << 8) | (3 << 16) | (1 << 18)) != 0 {
                    return Self::refuse("CTRLB quick command or reserved bits");
                }
                self.ctrlb = value & !(3 << 16);
                self.command((value >> 16) & 3);
            }
            0x0c => {
                if self.enabled() {
                    return Self::refuse("BAUD is enable-protected");
                }
                if value & !0xffff != 0 {
                    return Self::refuse("high-speed baud fields");
                }
                self.baud = value;
            }
            0x14 => self.inten &= !(value as u8),
            0x16 => self.inten |= value as u8 & (MB | SB | ERROR),
            0x18 => self.intflag.set(self.intflag.get() & !(value as u8)),
            0x1a => {
                // BUSSTATE is writable only to force IDLE; error bits are W1C.
                let state = value as u16 & BUSSTATE;
                if state != 0 && state != IDLE {
                    return Self::refuse("only IDLE may be forced into BUSSTATE");
                }
                if state == IDLE && self.enabled() {
                    self.status = (self.status & !BUSSTATE) | IDLE;
                }
                self.status &= !(value as u16 & 0x0747);
            }
            0x24 => {
                if !self.enabled() || value & !0x7ff != 0 || value & 0x700 != 0 {
                    return Self::refuse("ADDR needs an enabled master and a 7-bit address");
                }
                if self.status & BUSSTATE != IDLE && self.status & BUSSTATE != OWNER {
                    return Self::refuse("ADDR with an unknown bus state");
                }
                self.addr = value;
                self.intflag.set(self.intflag.get() & !(MB | SB));
                self.pending.set(Some(Op::Address(value as u8)));
            }
            0x28 => {
                self.intflag.set(self.intflag.get() & !MB);
                self.pending.set(Some(Op::Write(value as u8)));
            }
            0x30 => {}
            _ => return Self::refuse("store outside the I2C master slice"),
        }
        Ok(())
    }
}

impl std::fmt::Debug for Samd51SercomI2cm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Samd51SercomI2cm")
            .field("base", &self.instance.base)
            .field("ctrla", &self.ctrla)
            .field("status", &self.status)
            .field("devices", &self.devices.len())
            .finish()
    }
}

impl I2cSlaveHost for Samd51SercomI2cm {
    fn attach_slave(&mut self, device: Box<dyn I2cDevice>) {
        self.devices.push(RefCell::new(device));
    }
}

impl Peripheral for Samd51SercomI2cm {
    fn on_system_reset(&mut self) {
        self.reset();
    }
    fn read(&self, offset: u64) -> SimResult<u8> {
        let base = match offset {
            0x00..=0x0f | 0x1c..=0x1f | 0x24..=0x27 => offset & !3,
            0x1a..=0x1b => 0x1a,
            _ => offset,
        };
        if base == 0x28 && offset != 0x28 {
            return Ok(0);
        }
        Ok((self.read_reg(base)? >> (8 * (offset - base))) as u8)
    }
    fn read_u16(&self, offset: u64) -> SimResult<u16> {
        Ok(self.read_reg(offset)? as u16)
    }
    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        self.read_reg(offset)
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        match offset {
            0x14 | 0x16 | 0x18 | 0x28 | 0x30 => self.write_reg(offset, u32::from(value)),
            _ => Self::refuse("byte store to a wide register"),
        }
    }
    fn write_u16(&mut self, offset: u64, value: u16) -> SimResult<()> {
        self.write_reg(offset, u32::from(value))
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        self.write_reg(offset, value)
    }
    fn peek(&self, offset: u64) -> Option<u8> {
        // Inspection must not trigger the DATA read side effect.
        if offset == 0x28 {
            return Some(self.data.get());
        }
        self.read(offset).ok()
    }
    fn i2c_slave_host(&mut self) -> Option<&mut dyn I2cSlaveHost> {
        Some(self)
    }
    fn needs_bus_tick(&self) -> bool {
        self.pending.get().is_some()
    }
    fn tick_with_bus(&mut self, bus: &mut dyn Bus) {
        let (mask, bit) = self.instance.bus_clock;
        let clocked = bus
            .read_u32(0x40000800 + mask)
            .is_ok_and(|m| m & (1 << bit) != 0)
            && bus
                .read_u32(0x40001c80 + 4 * self.instance.core_gclk)
                .is_ok_and(|c| c & (1 << 6) != 0);
        let sercom = SERCOM_INSTANCES
            .iter()
            .position(|i| i.base == self.instance.base)
            .unwrap_or(0) as u8;
        let routed = pad_routed(bus, sercom, 0) && pad_routed(bus, sercom, 1);
        if !clocked || !self.enabled() || !routed {
            // An unclocked, disabled or unrouted master drives no bus: the
            // operation stays pending and the guest sees no MB/SB.
            return;
        }
        if let Some(op) = self.pending.take() {
            self.perform(op);
        }
    }
    fn legacy_tick_dynamic(&self) -> bool {
        true
    }
    fn legacy_tick_active(&self) -> bool {
        self.lines() != 0
    }
    fn irq_line_levels(&self, report: &mut dyn FnMut(u32, bool)) {
        let lines = self.lines();
        report(self.instance.first_irq, lines & MB != 0);
        report(self.instance.first_irq + 1, lines & SB != 0);
        report(self.instance.first_irq + 3, lines & ERROR != 0);
    }
    fn tick_elapsed(&mut self, _cycles: u64) -> PeripheralTickResult {
        let lines = self.lines();
        let mut irqs = vec![];
        for (flag, line) in [(MB, 0), (SB, 1), (ERROR, 3)] {
            if lines & flag != 0 {
                irqs.push(self.instance.first_irq + line);
            }
        }
        PeripheralTickResult {
            explicit_irqs: (!irqs.is_empty()).then_some(irqs),
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
        serde_json::json!({"peripheral": "samd51_sercom_i2cm", "base": self.instance.base, "ctrla": self.ctrla,
            "status": self.status, "intflag": self.intflag.get(), "transactions": self.transactions,
            "addresses": self.addresses, "devices": self.devices.len()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Regs {
        address: u8,
        pointer: u8,
        memory: [u8; 256],
    }
    impl I2cDevice for Regs {
        fn address(&self) -> u8 {
            self.address
        }
        fn write(&mut self, data: u8) {
            self.pointer = data;
        }
        fn read(&mut self) -> u8 {
            let v = self.memory[usize::from(self.pointer)];
            self.pointer = self.pointer.wrapping_add(1);
            v
        }
    }

    fn master() -> Samd51SercomI2cm {
        let mut m = Samd51SercomI2cm::new(SERCOM_INSTANCES[2]);
        let mut memory = [0u8; 256];
        memory[0xd0] = 0x58;
        memory[0xd1] = 0x77;
        m.attach_slave(Box::new(Regs {
            address: 0x77,
            pointer: 0,
            memory,
        }));
        m.write_u32(0, MODE_I2C_MASTER | (2 << 20)).unwrap();
        m.write_u32(4, 1 << 8).unwrap(); // smart mode
        m.write_u32(0, MODE_I2C_MASTER | (2 << 20) | 2).unwrap();
        m.write_u16(0x1a, IDLE).unwrap();
        m
    }
    fn run(m: &mut Samd51SercomI2cm) {
        let op = m.pending.take().expect("pending operation");
        m.perform(op);
    }

    #[test]
    fn register_pointer_write_then_smart_mode_read() {
        let mut m = master();
        m.write_u32(0x24, 0x77 << 1).unwrap();
        run(&mut m);
        assert_eq!(m.read(0x18).unwrap() & MB, MB);
        assert_eq!(m.read_u16(0x1a).unwrap() & RXNACK, 0);
        m.write(0x28, 0xd0).unwrap();
        run(&mut m);
        m.write_u32(0x24, (0x77 << 1) | 1).unwrap(); // repeated start, read
        run(&mut m);
        assert_eq!(m.read(0x18).unwrap() & SB, SB);
        assert_eq!(m.read(0x28).unwrap(), 0x58);
        run(&mut m); // the smart-mode acknowledge fetched the next byte
        m.write_u32(4, (1 << 8) | (1 << 18) | (3 << 16)).unwrap(); // NACK + STOP
        assert_eq!(m.read(0x28).unwrap(), 0x77);
        run(&mut m);
        assert_eq!(m.read_u16(0x1a).unwrap() & BUSSTATE, IDLE);
        assert_eq!(m.transactions(), 2);
    }

    #[test]
    fn unclaimed_address_is_nacked_and_settings_refused() {
        let mut m = master();
        m.write_u32(0x24, 0x10 << 1).unwrap();
        run(&mut m);
        assert_eq!(m.read(0x18).unwrap() & MB, MB);
        assert_eq!(m.read_u16(0x1a).unwrap() & RXNACK, RXNACK);
        assert!(m.write_u32(0x0c, 10).is_err(), "BAUD while enabled");
        assert!(m.write_u32(0x24, 0x400).is_err(), "10-bit address");
        assert!(m.write_u32(4, 1 << 9).is_err(), "quick command");
        let mut d = Samd51SercomI2cm::new(SERCOM_INSTANCES[2]);
        assert!(
            d.write_u32(0, 1 << 2).is_err(),
            "USART mode on the I2C slice"
        );
        assert!(d.write_u32(0x24, 0x77 << 1).is_err(), "ADDR while disabled");
    }
}
