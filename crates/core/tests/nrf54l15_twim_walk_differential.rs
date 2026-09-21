// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Differential gate for the nRF54L TWIM scheduler migration.
//!
//! The TWIM is the mirror image of the UARTE next door, and the contrast is
//! the reason both needed their own gate rather than one shared assumption:
//!
//! * **Its work is entirely write-triggered.** An EasyDMA leg is armed by a
//!   `TASKS_DMA.{TX,RX}.START` write, and nothing external ever pokes the
//!   model. So it needs no poll — `take_scheduled_events` after each MMIO
//!   write sees every transition, where the UARTE had to keep asking because
//!   its RX bytes arrive from a host thread.
//! * **Its IRQ is a LEVEL, not an edge.** The walk re-asserted
//!   `inten & event_bitmap()` on every single cycle until firmware cleared the
//!   event. A scheduler wake that fired once and stopped would silently drop
//!   re-pends the legacy path performed — so this model re-arms at delay 1
//!   while a latched-and-enabled event holds the line, and only falls silent
//!   when the ISR clears it.
//!
//! Neither property is visible in the model's own unit tests, which never
//! attach a `CycleClock` and so never leave the legacy path. This gate runs
//! the SAME hand-built Cortex-M33 machine and the SAME hand-assembled Thumb
//! firmware twice — walk-pinned (the reference: it is what shipped) vs
//! scheduler-driven — and compares every observable at every instruction
//! boundary, the bytes the I²C slave actually received included.

#![cfg(feature = "event-scheduler")]

mod common;

use common::thumb_asm::Asm;
use common::walk_differential::{
    assert_modes_differ, assert_probes_identical, run_probed, WalkMode,
};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::peripherals::device::I2cDevice;
use labwired_core::peripherals::nrf54l::twim::Nrf54lTwim;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::Bus;
use labwired_core::Machine;
use std::sync::{Arc, Mutex};

const TWIM_BASE: u32 = 0x5000_0000;
const TWIM_IRQ: u32 = 16;

const R_TASKS_STOP: u32 = TWIM_BASE + 0x004;
const R_TASKS_DMA_RX_START: u32 = TWIM_BASE + 0x028;
const R_TASKS_DMA_TX_START: u32 = TWIM_BASE + 0x050;
const R_EVENTS_STOPPED: u32 = TWIM_BASE + 0x104;
const R_INTEN: u32 = TWIM_BASE + 0x300;
const R_ENABLE: u32 = TWIM_BASE + 0x500;
const R_ADDRESS: u32 = TWIM_BASE + 0x588;
const R_DMA_RX_PTR: u32 = TWIM_BASE + 0x704;
const R_DMA_RX_MAXCNT: u32 = TWIM_BASE + 0x708;
const R_DMA_TX_PTR: u32 = TWIM_BASE + 0x73C;
const R_DMA_TX_MAXCNT: u32 = TWIM_BASE + 0x740;
const NVIC_ISER0: u32 = 0xE000_E100;

/// ENABLE = 6 selects TWIM on this family.
const ENABLE_TWIM: u8 = 6;
/// INTEN bit for STOPPED. The model's own `INTEN_STOPPED` is the authority
/// and equals `1 << 0`; ERROR is bit 1.
const INTEN_STOPPED_MASK: u8 = 1;

const SLAVE_ADDR: u8 = 0x68;
/// WHO_AM_I register index and the value the fake sensor returns for it.
const REG_WHO_AM_I: u8 = 0x75;
const WHO_AM_I_VALUE: u8 = 0x68;

const TX_BUF: u32 = 0x2000_0100;
const RX_BUF: u32 = 0x2000_0120;
const ISR_COUNT_ADDR: u64 = 0x2000_0000;
const MAIN_COUNT_ADDR: u64 = 0x2000_0004;
const INITIAL_SP: u32 = 0x2000_8000;

const ISR_ENTRY: u32 = 0xC0;
const MAIN_ENTRY: u32 = 0x140;

/// A register-pointer slave, the same shape as a real sensor, that also
/// RECORDS every byte it was written.
///
/// The recording is the point. A TWIM that stopped running transactions
/// changes no architectural state until firmware notices, so without an
/// observable on the I²C side the differential would be comparing two runs of
/// a peripheral that does nothing — and passing.
struct RecordingSensor {
    regs: [u8; 256],
    ptr: u8,
    addr_written: bool,
    written: Arc<Mutex<Vec<u8>>>,
}

impl RecordingSensor {
    fn new(written: Arc<Mutex<Vec<u8>>>) -> Self {
        let mut regs = [0u8; 256];
        regs[REG_WHO_AM_I as usize] = WHO_AM_I_VALUE;
        Self {
            regs,
            ptr: 0,
            addr_written: false,
            written,
        }
    }
}

impl I2cDevice for RecordingSensor {
    fn address(&self) -> u8 {
        SLAVE_ADDR
    }
    fn read(&mut self) -> u8 {
        let v = self.regs[self.ptr as usize];
        self.ptr = self.ptr.wrapping_add(1);
        v
    }
    fn write(&mut self, data: u8) {
        if let Ok(mut g) = self.written.lock() {
            g.push(data);
        }
        if !self.addr_written {
            self.ptr = data;
            self.addr_written = true;
        } else {
            self.regs[self.ptr as usize] = data;
            self.ptr = self.ptr.wrapping_add(1);
        }
    }
    fn stop(&mut self) {
        self.addr_written = false;
    }
}

fn write_bytes(bus: &mut SystemBus, base: u32, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        bus.write_u8(base as u64 + i as u64, *b).unwrap();
    }
}

/// ISR: clear EVENTS_STOPPED (which drops the level), bump the counter.
fn assemble_isr(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    a.movs(1, 0);
    a.ldr_pool(0, "ev_stopped").str0(1, 0);
    a.ldr_pool(0, "isrcnt").ldr0(1, 0).adds(1, 1).str0(1, 0);
    a.bx(14);
    a.word("ev_stopped", R_EVENTS_STOPPED)
        .word("isrcnt", ISR_COUNT_ADDR as u32);
    a.assemble()
}

/// Main firmware: enable the TWIM and its NVIC line with STOPPED interrupts
/// on, write the register pointer to the slave, then read one byte back, then
/// spin bumping a counter and polling the received byte into r4.
fn assemble_main(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    a.ldr_pool(5, "iser")
        .movs(1, 1)
        .lsls(1, 1, TWIM_IRQ as u8)
        .str0(1, 5);
    a.ldr_pool(5, "enable").movs(1, ENABLE_TWIM).str0(1, 5);
    a.ldr_pool(5, "address").movs(1, SLAVE_ADDR).str0(1, 5);
    a.ldr_pool(5, "inten")
        .movs(1, INTEN_STOPPED_MASK)
        .str0(1, 5);
    // TX leg: one byte, the register pointer.
    a.ldr_pool(5, "txptr").ldr_pool(1, "txbuf").str0(1, 5);
    a.ldr_pool(5, "txmax").movs(1, 1).str0(1, 5);
    a.ldr_pool(5, "txstart").movs(1, 1).str0(1, 5);
    // RX leg: one byte back.
    a.ldr_pool(5, "rxptr").ldr_pool(1, "rxbuf").str0(1, 5);
    a.ldr_pool(5, "rxmax").movs(1, 1).str0(1, 5);
    a.ldr_pool(5, "rxstart").movs(1, 1).str0(1, 5);
    // TASKS_STOP: end the transaction. This is what latches EVENTS_STOPPED,
    // and EVENTS_STOPPED with INTEN.STOPPED set is the LEVEL-held IRQ this
    // migration had to preserve. Without it the fixture runs two EasyDMA legs
    // and never raises an interrupt at all -- which the sanity assertion
    // below catches, but only because it is there.
    a.ldr_pool(5, "stop").movs(1, 1).str0(1, 5);
    // Main loop: main_count++, poll the received byte into r4.
    a.ldr_pool(2, "maincnt").ldr_pool(6, "rxbuf").movs(3, 0);
    a.label("loop");
    a.adds(3, 1).str0(3, 2).ldr0(4, 6).b("loop");
    a.word("iser", NVIC_ISER0)
        .word("enable", R_ENABLE)
        .word("address", R_ADDRESS)
        .word("inten", R_INTEN)
        .word("txptr", R_DMA_TX_PTR)
        .word("txbuf", TX_BUF)
        .word("txmax", R_DMA_TX_MAXCNT)
        .word("txstart", R_TASKS_DMA_TX_START)
        .word("rxptr", R_DMA_RX_PTR)
        .word("rxbuf", RX_BUF)
        .word("rxmax", R_DMA_RX_MAXCNT)
        .word("rxstart", R_TASKS_DMA_RX_START)
        .word("stop", R_TASKS_STOP)
        .word("maincnt", MAIN_COUNT_ADDR as u32);
    a.assemble()
}

type Written = Arc<Mutex<Vec<u8>>>;

fn build(mode: WalkMode, tick_interval: u32, written: &Written) -> Machine<CortexM> {
    let mut bus = SystemBus::new();
    let (cpu, _nvic) = configure_cortex_m(&mut bus);

    let mut twim = Nrf54lTwim::new();
    twim.push_slave(Box::new(RecordingSensor::new(written.clone())));
    bus.add_peripheral(
        "twim",
        TWIM_BASE as u64,
        0x1000,
        Some(TWIM_IRQ),
        Box::new(twim),
    );

    if mode.is_legacy_walk() {
        let idx = bus.find_peripheral_index_by_name("twim").unwrap();
        bus.peripherals[idx]
            .dev
            .as_any_mut()
            .unwrap()
            .downcast_mut::<Nrf54lTwim>()
            .unwrap()
            .force_legacy_walk();
    }

    bus.write_u32(((16 + TWIM_IRQ) * 4) as u64, ISR_ENTRY | 1)
        .unwrap();
    let (isr_base, isr) = assemble_isr(ISR_ENTRY);
    assert!(
        isr_base + isr.len() as u32 <= MAIN_ENTRY,
        "ISR image ({} bytes) overruns the main entry",
        isr.len()
    );
    write_bytes(&mut bus, isr_base, &isr);
    let (main_base, main) = assemble_main(MAIN_ENTRY);
    write_bytes(&mut bus, main_base, &main);
    write_bytes(&mut bus, TX_BUF, &[REG_WHO_AM_I]);

    let mut machine = Machine::new(cpu, bus);
    machine.config.peripheral_tick_interval = tick_interval;
    machine.bus.config.peripheral_tick_interval = tick_interval;
    machine.cpu.sp = INITIAL_SP;
    machine
}

fn uses_scheduler(machine: &Machine<CortexM>) -> bool {
    let idx = machine.bus.find_peripheral_index_by_name("twim").unwrap();
    machine.bus.peripherals[idx].dev.uses_scheduler()
}

#[test]
fn twim_transaction_firmware_is_byte_identical_at_interval_1() {
    const STEPS: u64 = 1_500;

    let walk_written: Written = Arc::new(Mutex::new(Vec::new()));
    let sched_written: Written = Arc::new(Mutex::new(Vec::new()));

    assert_modes_differ(
        uses_scheduler(&build(WalkMode::LegacyWalk, 1, &walk_written)),
        uses_scheduler(&build(WalkMode::Scheduler, 1, &sched_written)),
        "nrf54l15 twim",
    );

    let extra_for = |written: Written| {
        move |m: &Machine<CortexM>| -> Vec<(&'static str, u64)> {
            vec![
                (
                    "isr_count",
                    u64::from(m.bus.read_u32(ISR_COUNT_ADDR).unwrap()),
                ),
                (
                    "main_count",
                    u64::from(m.bus.read_u32(MAIN_COUNT_ADDR).unwrap()),
                ),
                ("rx_byte", u64::from(m.bus.read_u8(RX_BUF as u64).unwrap())),
                ("i2c_bytes_written", written.lock().unwrap().len() as u64),
            ]
        }
    };

    // Fresh recorders for the measured runs: the two probe builds above
    // already drove a transaction each, and a shared recorder would carry
    // their bytes into the comparison.
    let walk_written: Written = Arc::new(Mutex::new(Vec::new()));
    let mut walk = build(WalkMode::LegacyWalk, 1, &walk_written);
    let reference = run_probed(
        &mut walk,
        MAIN_ENTRY,
        STEPS,
        &extra_for(walk_written.clone()),
    );

    // The fixture must exercise what it claims to. Without this the whole file
    // could pass on firmware that never armed a transaction.
    assert_eq!(
        &*walk_written.lock().unwrap(),
        &[REG_WHO_AM_I],
        "reference lane must have written the register pointer to the slave"
    );
    assert_eq!(
        walk.bus.read_u8(RX_BUF as u64).unwrap(),
        WHO_AM_I_VALUE,
        "reference lane must have read WHO_AM_I back over I2C"
    );
    let last = reference.last().unwrap();
    assert!(
        last.extra.iter().any(|(n, v)| *n == "isr_count" && *v >= 1),
        "reference must take the STOPPED interrupt — that is the LEVEL path \
         this migration had to preserve"
    );
    assert!(
        last.extra
            .iter()
            .any(|(n, v)| *n == "main_count" && *v > 100),
        "the main loop must run"
    );

    let sched_written: Written = Arc::new(Mutex::new(Vec::new()));
    let mut sched = build(WalkMode::Scheduler, 1, &sched_written);
    let candidate = run_probed(
        &mut sched,
        MAIN_ENTRY,
        STEPS,
        &extra_for(sched_written.clone()),
    );

    assert_probes_identical(&reference, &candidate, "nrf54l15 twim transaction firmware");
}

/// Across batch widths: the I²C traffic must not duplicate or vanish. A
/// walk+scheduler double-run shows up here as the pointer byte written twice.
#[test]
fn the_i2c_traffic_survives_a_wide_batch() {
    const STEPS: u64 = 1_500;
    let no_extra = |_: &Machine<CortexM>| Vec::new();

    for interval in [1u32, 8, 64] {
        let written: Written = Arc::new(Mutex::new(Vec::new()));
        let mut m = build(WalkMode::Scheduler, interval, &written);
        run_probed(&mut m, MAIN_ENTRY, STEPS, &no_extra);
        assert_eq!(
            &*written.lock().unwrap(),
            &[REG_WHO_AM_I],
            "interval {interval}: the slave must see the pointer byte exactly \
             once"
        );
        assert_eq!(
            m.bus.read_u8(RX_BUF as u64).unwrap(),
            WHO_AM_I_VALUE,
            "interval {interval}: and the read-back byte must still land"
        );
    }
}
