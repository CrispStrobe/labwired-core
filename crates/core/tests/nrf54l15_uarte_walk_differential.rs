// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Differential gate for the nRF54L UARTE scheduler migration.
//!
//! The UARTE is the hardest of the nrf54l15 walk forcers to move, because it
//! does two unrelated things on the per-cycle walk and only one of them is
//! driven by the firmware:
//!
//! * **EasyDMA**, armed by a `TASKS_DMA.{TX,RX}.START` write — an MMIO write,
//!   so the scheduler can arm an event off it directly; and
//! * **an RX drain whose trigger is a HOST**, injecting bytes into
//!   `rx_source` from another thread. Nothing on the bus fires when a byte
//!   lands. The walk polled for that byte on every cycle, for free; the
//!   scheduler has to ask, which is why this model carries a
//!   self-perpetuating WAKE rather than only write-armed events.
//!
//! Getting either wrong is silent. A TX that never runs prints nothing, and
//! "the console is empty" is indistinguishable from firmware that had not got
//! there yet. So the gate is a differential: the SAME hand-built Cortex-M33
//! machine and the SAME hand-assembled Thumb firmware run twice — once with
//! the UARTE pinned onto the walk (`force_legacy_walk`, the reference: it is
//! what shipped) and once scheduler-driven — and every observable is compared
//! at every instruction boundary, captured TX bytes included.
//!
//! See `common::walk_differential` for the harness and, in particular, for
//! why [`assert_modes_differ`] is not optional: without it a model whose
//! `uses_scheduler()` quietly returns false makes this whole file compare the
//! walk against itself and report success.

#![cfg(feature = "event-scheduler")]

mod common;

use common::thumb_asm::Asm;
use common::walk_differential::{
    assert_modes_differ, assert_walk_differential, run_probed, WalkMode,
};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::peripherals::nrf54l::uarte::Nrf54lUarte;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::Bus;
use labwired_core::Machine;
use std::sync::{Arc, Mutex};

// UARTE in the SoC peripheral window, clear of the SCB/NVIC/SysTick block.
const UARTE_BASE: u32 = 0x5000_0000;
const UARTE_IRQ: u32 = 16;

// Register absolute addresses (base + nRF54L offset — NOT the nRF52 ones).
const R_TASKS_DMA_RX_START: u32 = UARTE_BASE + 0x028;
const R_TASKS_DMA_TX_START: u32 = UARTE_BASE + 0x050;
const R_EVENTS_DMA_RX_END: u32 = UARTE_BASE + 0x14C;
const R_EVENTS_DMA_TX_END: u32 = UARTE_BASE + 0x168;
const R_INTENSET: u32 = UARTE_BASE + 0x304;
const R_ENABLE: u32 = UARTE_BASE + 0x500;
const R_DMA_RX_PTR: u32 = UARTE_BASE + 0x704;
const R_DMA_RX_MAXCNT: u32 = UARTE_BASE + 0x708;
const R_DMA_TX_PTR: u32 = UARTE_BASE + 0x73C;
const R_DMA_TX_MAXCNT: u32 = UARTE_BASE + 0x740;
const NVIC_ISER0: u32 = 0xE000_E100;

/// EVENTS_DMA.TX.END is event index (0x168 - 0x100) / 4 = 26, which is also
/// its INTEN bit — that identity is exact on this family.
const INTEN_BIT_DMA_TX_END: u8 = 26;

const TX_MSG: &[u8] = b"LW54L\n";
const RX_MSG: &[u8] = b"ok";

const TX_BUF: u32 = 0x2000_0100;
const RX_BUF: u32 = 0x2000_0120;
const ISR_COUNT_ADDR: u64 = 0x2000_0000;
const MAIN_COUNT_ADDR: u64 = 0x2000_0004;
const INITIAL_SP: u32 = 0x2000_8000;

const ISR_ENTRY: u32 = 0xC0;
const MAIN_ENTRY: u32 = 0x140;

fn write_bytes(bus: &mut SystemBus, base: u32, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        bus.write_u8(base as u64 + i as u64, *b).unwrap();
    }
}

/// ISR: clear both completion events, bump the ISR counter, return.
fn assemble_isr(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    a.movs(1, 0);
    a.ldr_pool(0, "ev_tx").str0(1, 0);
    a.ldr_pool(0, "ev_rx").str0(1, 0);
    a.ldr_pool(0, "isrcnt").ldr0(1, 0).adds(1, 1).str0(1, 0);
    a.bx(14);
    a.word("ev_tx", R_EVENTS_DMA_TX_END)
        .word("ev_rx", R_EVENTS_DMA_RX_END)
        .word("isrcnt", ISR_COUNT_ADDR as u32);
    a.assemble()
}

/// Main firmware: enable the UARTE and its NVIC line, arm an INTEN-enabled
/// EasyDMA TX of [`TX_MSG`] and an EasyDMA RX into [`RX_BUF`], then spin
/// bumping a counter and polling EVENTS_DMA.RX.END into r4.
///
/// Both DMA directions are armed because they reach the migration through
/// different doors: TX runs off the write-armed event, RX off the
/// self-perpetuating poll.
fn assemble_main(base: u32) -> (u32, Vec<u8>) {
    let mut a = Asm::new(base);
    // NVIC ISER0 |= 1 << UARTE_IRQ.
    a.ldr_pool(5, "iser")
        .movs(1, 1)
        .lsls(1, 1, UARTE_IRQ as u8)
        .str0(1, 5);
    // ENABLE = 8 (UARTE mode on this family; 4 is the nRF52 legacy-UART
    // personality and does not exist here).
    a.ldr_pool(5, "enable").movs(1, 8).str0(1, 5);
    // INTENSET = 1 << DMA_TX_END.
    a.ldr_pool(5, "intenset")
        .movs(1, 1)
        .lsls(1, 1, INTEN_BIT_DMA_TX_END)
        .str0(1, 5);
    // Arm RX first, so the drain is already pending when TX runs.
    a.ldr_pool(5, "rxptr").ldr_pool(1, "rxbuf").str0(1, 5);
    a.ldr_pool(5, "rxmax")
        .movs(1, RX_MSG.len() as u8)
        .str0(1, 5);
    a.ldr_pool(5, "rxstart").movs(1, 1).str0(1, 5);
    // Arm and start TX.
    a.ldr_pool(5, "txptr").ldr_pool(1, "txbuf").str0(1, 5);
    a.ldr_pool(5, "txmax")
        .movs(1, TX_MSG.len() as u8)
        .str0(1, 5);
    a.ldr_pool(5, "txstart").movs(1, 1).str0(1, 5);
    // Main loop: main_count++, poll EVENTS_DMA.RX.END into r4.
    a.ldr_pool(2, "maincnt").ldr_pool(6, "ev_rx").movs(3, 0);
    a.label("loop");
    a.adds(3, 1).str0(3, 2).ldr0(4, 6).b("loop");
    a.word("iser", NVIC_ISER0)
        .word("enable", R_ENABLE)
        .word("intenset", R_INTENSET)
        .word("rxptr", R_DMA_RX_PTR)
        .word("rxbuf", RX_BUF)
        .word("rxmax", R_DMA_RX_MAXCNT)
        .word("rxstart", R_TASKS_DMA_RX_START)
        .word("txptr", R_DMA_TX_PTR)
        .word("txbuf", TX_BUF)
        .word("txmax", R_DMA_TX_MAXCNT)
        .word("txstart", R_TASKS_DMA_TX_START)
        .word("maincnt", MAIN_COUNT_ADDR as u32)
        .word("ev_rx", R_EVENTS_DMA_RX_END);
    a.assemble()
}

/// The captured console. Handed to the UARTE as its TX sink, and read back as
/// the differential's `extra` observable — the one that makes this gate bite,
/// since a UARTE that stopped moving bytes changes no architectural state at
/// all until the firmware notices.
type Sink = Arc<Mutex<Vec<u8>>>;

fn build(mode: WalkMode, tick_interval: u32, sink: &Sink) -> Machine<CortexM> {
    let mut bus = SystemBus::new();
    let (cpu, _nvic) = configure_cortex_m(&mut bus);

    let mut uarte = Nrf54lUarte::new();
    uarte.set_sink(Some(sink.clone()), false);
    bus.add_peripheral(
        "uarte",
        UARTE_BASE as u64,
        0x1000,
        Some(UARTE_IRQ),
        Box::new(uarte),
    );

    let idx = bus.find_peripheral_index_by_name("uarte").unwrap();
    {
        let dev = bus.peripherals[idx]
            .dev
            .as_any_mut()
            .unwrap()
            .downcast_mut::<Nrf54lUarte>()
            .unwrap();
        // Pre-seed the RX injection queue. Doing it BEFORE the run keeps both
        // lanes deterministic: a host writing mid-run would inject at a
        // wall-clock instant, and the two lanes would legitimately disagree
        // about which cycle saw the byte — a divergence in the fixture, not in
        // the model.
        dev.rx_buffer()
            .lock()
            .unwrap()
            .extend(RX_MSG.iter().copied());
        if mode.is_legacy_walk() {
            dev.force_legacy_walk();
        }
    }

    // Vector table entry for exception (16 + UARTE_IRQ).
    bus.write_u32(((16 + UARTE_IRQ) * 4) as u64, ISR_ENTRY | 1)
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
    write_bytes(&mut bus, TX_BUF, TX_MSG);

    let mut machine = Machine::new(cpu, bus);
    machine.config.peripheral_tick_interval = tick_interval;
    machine.bus.config.peripheral_tick_interval = tick_interval;
    machine.cpu.sp = INITIAL_SP;
    machine
}

fn uses_scheduler(machine: &Machine<CortexM>) -> bool {
    let idx = machine.bus.find_peripheral_index_by_name("uarte").unwrap();
    machine.bus.peripherals[idx].dev.uses_scheduler()
}

/// The gate. Every instruction boundary: cycles, PC, all 16 registers, the ISR
/// and main-loop counters, the RX bytes that landed in RAM, and the number of
/// TX bytes that reached the console.
#[test]
fn uarte_dma_firmware_is_byte_identical_at_interval_1() {
    const STEPS: u64 = 1_500;

    // One sink per lane — a shared one would make the candidate's byte count
    // include the reference's, and the comparison would pass for the wrong
    // reason.
    let walk_sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let sched_sink: Sink = Arc::new(Mutex::new(Vec::new()));

    // Vacuity guard FIRST: if the two lanes are the same mode there is nothing
    // to learn from the comparison, and the failure must say so rather than
    // print a green tick.
    assert_modes_differ(
        uses_scheduler(&build(WalkMode::LegacyWalk, 1, &walk_sink)),
        uses_scheduler(&build(WalkMode::Scheduler, 1, &sched_sink)),
        "nrf54l15 uarte",
    );

    let extra_for = |sink: Sink| {
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
                ("rx_word", u64::from(m.bus.read_u32(RX_BUF as u64).unwrap())),
                ("tx_bytes", sink.lock().unwrap().len() as u64),
            ]
        }
    };

    let mut walk = build(WalkMode::LegacyWalk, 1, &walk_sink);
    let reference = run_probed(&mut walk, MAIN_ENTRY, STEPS, &extra_for(walk_sink.clone()));

    // The fixture must actually exercise the surface it claims to. Without
    // this the whole file could pass on firmware that never armed a transfer.
    assert_eq!(
        &*walk_sink.lock().unwrap(),
        TX_MSG,
        "reference lane must have completed the EasyDMA TX"
    );
    assert_eq!(
        &walk.bus.read_u8(RX_BUF as u64).unwrap(),
        &RX_MSG[0],
        "reference lane must have drained the RX injection queue into RAM"
    );
    let last = reference.last().unwrap();
    assert!(
        last.extra.iter().any(|(n, v)| *n == "isr_count" && *v >= 1),
        "reference must take the DMA.TX.END interrupt"
    );
    assert!(
        last.extra
            .iter()
            .any(|(n, v)| *n == "main_count" && *v > 100),
        "the main loop must run"
    );

    let mut sched = build(WalkMode::Scheduler, 1, &sched_sink);
    let candidate = run_probed(
        &mut sched,
        MAIN_ENTRY,
        STEPS,
        &extra_for(sched_sink.clone()),
    );

    common::walk_differential::assert_probes_identical(
        &reference,
        &candidate,
        "nrf54l15 uarte dma firmware",
    );
}

/// The same machine at tick interval 8, both lanes scheduler-vs-walk. IRQ
/// delivery quantises to the batch grid at interval > 1 (documented, bounded
/// by one interval), so this asserts the thing that must NOT quantise: the
/// bytes. Same message, same order, exactly once.
#[test]
fn the_console_bytes_survive_a_wide_batch() {
    const STEPS: u64 = 1_500;
    let no_extra = |_: &Machine<CortexM>| Vec::new();

    for interval in [1u32, 8, 64] {
        let sink: Sink = Arc::new(Mutex::new(Vec::new()));
        let mut m = build(WalkMode::Scheduler, interval, &sink);
        run_probed(&mut m, MAIN_ENTRY, STEPS, &no_extra);
        assert_eq!(
            &*sink.lock().unwrap(),
            TX_MSG,
            "interval {interval}: the console must carry the message exactly \
             once — a duplicated walk+scheduler transfer shows up here as \
             double the bytes, a dropped one as none"
        );
    }
}

/// Anti-vacuity for the harness ITSELF, on this fixture: a deliberately broken
/// candidate must be caught. Uses the harness the same way the real gate does,
/// but hands it a scheduler lane whose RX queue was never seeded — so the RX
/// drain never happens and the firmware's r4 poll diverges.
#[test]
#[should_panic(expected = "first divergence at step")]
fn a_uarte_that_stops_moving_bytes_is_caught() {
    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let extra = |m: &Machine<CortexM>| -> Vec<(&'static str, u64)> {
        vec![("rx_word", u64::from(m.bus.read_u32(RX_BUF as u64).unwrap()))]
    };
    assert_walk_differential(
        "sabotaged uarte",
        MAIN_ENTRY,
        1_500,
        |mode| {
            let mut m = build(mode, 1, &sink);
            if !mode.is_legacy_walk() {
                let idx = m.bus.find_peripheral_index_by_name("uarte").unwrap();
                m.bus.peripherals[idx]
                    .dev
                    .as_any_mut()
                    .unwrap()
                    .downcast_mut::<Nrf54lUarte>()
                    .unwrap()
                    .rx_buffer()
                    .lock()
                    .unwrap()
                    .clear();
            }
            m
        },
        extra,
    );
}
