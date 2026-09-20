// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! ESP32-C6 PCR clock-gate enforcement, pinned on the real chip descriptor.
//!
//! `configs/chips/esp32c6.yaml` declares `esp32c6_pcr` at 0x6009_6000 and
//! wires `clock:` gates from uart0/uart1/timg0/timg1/gdma to named PCR
//! registers (`UART0_CONF`, ...). The bus resolves the names through
//! `Peripheral::clock_gate_reg_offset` and enforces them in
//! `is_peripheral_clocked` on EVERY MMIO access: the gate register is read
//! live, so closing `UART0_CONF.CLK_EN` mid-run silences UART0 immediately.
//!
//! These tests build the C6 bus from the chip + devkit system manifests and
//! drive it through plain bus reads/writes — no CPU, no firmware — to pin:
//!
//!   * every declared gate resolves (a typo in the YAML is a hard build error
//!     in `resolve_clock_gates`, so this is the positive control);
//!   * with `CLK_EN=1` (the SVD reset), UART0 registers answer and store;
//!   * with `CLK_EN=0`, a stored UART0 register reads back 0 AND a write is
//!     dropped (the pre-gate value survives re-enabling);
//!   * the PCR register map itself is never gated (it is the controller) and
//!     round-trips;
//!   * timg0's gate resolves to `TIMERGROUP0_CONF` and its read path answers
//!     while clocked.

use crate::bus::SystemBus;
use crate::Bus;
use labwired_config::{ChipDescriptor, SystemManifest};
use std::path::PathBuf;

const UART0_BASE: u64 = 0x6000_0000;
const UART0_CLKDIV: u64 = UART0_BASE + 0x14;
const PCR_BASE: u64 = 0x6009_6000;
const UART0_CONF: u64 = PCR_BASE;
const UART1_CONF: u64 = PCR_BASE + 0x0C;
const TIMERGROUP0_CONF: u64 = PCR_BASE + 0x3C;
const TIMERGROUP1_CONF: u64 = PCR_BASE + 0x48;
const GDMA_CONF: u64 = PCR_BASE + 0xBC;
const SYSCLK_CONF: u64 = PCR_BASE + 0x110;

fn c6_bus() -> SystemBus {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let chip_path = root.join("../../configs/chips/esp32c6.yaml");
    let system_path = root.join("../../configs/systems/esp32c6-devkitc.yaml");
    let chip = ChipDescriptor::from_file(&chip_path).expect("load esp32c6 chip yaml");
    let mut manifest = SystemManifest::from_file(&system_path).expect("load c6 system yaml");
    manifest.chip = system_path
        .parent()
        .unwrap()
        .join(&manifest.chip)
        .to_string_lossy()
        .into_owned();
    SystemBus::from_config(&chip, &manifest).expect("build c6 devkit bus")
}

fn gate_of(bus: &SystemBus, name: &str) -> crate::bus::ResolvedClockGate {
    let idx = bus
        .find_peripheral_index_by_name(name)
        .unwrap_or_else(|| panic!("peripheral {name} not on the C6 bus"));
    bus.peripherals[idx]
        .clock_gate
        .clone()
        .unwrap_or_else(|| panic!("peripheral {name} has no resolved clock gate"))
}

#[test]
fn all_declared_gates_resolve_to_the_pcr_registers() {
    let bus = c6_bus();
    // Gate register offsets are RELATIVE to the controller (the PCR window).
    let cases = [
        ("uart0", 0x00),
        ("uart1", 0x0C),
        ("timg0", 0x3C),
        ("timg1", 0x48),
        ("gdma", 0xBC),
    ];
    let pcr_idx = bus
        .find_peripheral_index_by_name("pcr")
        .expect("pcr on bus");
    assert_eq!(
        bus.peripherals[pcr_idx].base, PCR_BASE,
        "pcr at 0x6009_6000"
    );
    for (name, reg_offset) in cases {
        let gate = gate_of(&bus, name);
        assert_eq!(gate.requires.len(), 1, "{name}: one gate bit");
        let req = &gate.requires[0];
        assert_eq!(req.controller_idx, pcr_idx, "{name}: controller is PCR");
        assert_eq!(req.reg_offset, reg_offset, "{name}: PCR-relative offset");
        assert_eq!(req.bit, 0, "{name}: CLK_EN is bit 0");
    }
}

#[test]
fn pcr_register_map_round_trips_and_is_never_gated() {
    let mut bus = c6_bus();
    // SVD reset values.
    assert_eq!(bus.read_u32(UART0_CONF).unwrap(), 1);
    assert_eq!(bus.read_u32(UART1_CONF).unwrap(), 1);
    assert_eq!(bus.read_u32(TIMERGROUP0_CONF).unwrap(), 1);
    assert_eq!(bus.read_u32(TIMERGROUP1_CONF).unwrap(), 1);
    assert_eq!(bus.read_u32(GDMA_CONF).unwrap(), 1);
    assert_eq!(bus.read_u32(SYSCLK_CONF).unwrap(), 0x2800_0200);
    // Clock-source register round-trip.
    bus.write_u32(SYSCLK_CONF, 0x2800_0202).unwrap();
    assert_eq!(bus.read_u32(SYSCLK_CONF).unwrap(), 0x2800_0202);
}

#[test]
fn closing_uart0_clk_en_makes_uart0_dead_and_reopening_restores_it() {
    let mut bus = c6_bus();

    // Clocked: the CLKDIV register stores and answers.
    bus.write_u32(UART0_CLKDIV, 0x234).unwrap();
    assert_eq!(bus.read_u32(UART0_CLKDIV).unwrap() & 0xFFF, 0x234);

    // Gate closed: reads are dead (0) ...
    bus.write_u32(UART0_CONF, 0).unwrap();
    assert_eq!(
        bus.read_u32(UART0_CLKDIV).unwrap(),
        0,
        "gated UART0 reads must return 0"
    );
    // ... and writes are dropped.
    bus.write_u32(UART0_CLKDIV, 0xBAD).unwrap();

    // Gate reopened: the pre-gate value survives — the gated write was
    // dropped, not queued.
    bus.write_u32(UART0_CONF, 1).unwrap();
    assert_eq!(
        bus.read_u32(UART0_CLKDIV).unwrap() & 0xFFF,
        0x234,
        "a write while gated must be dropped, not buffered"
    );

    // Closing the gate again silences it again (live read, not latched).
    bus.write_u32(UART0_CONF, 0).unwrap();
    assert_eq!(bus.read_u32(UART0_CLKDIV).unwrap(), 0);
}

#[test]
fn timg0_is_clocked_at_reset_and_answers_its_counter_registers() {
    let mut bus = c6_bus();
    // TIMG0 T0UPDATE/T0LO at 0x6000_8000; the model is live and clocked.
    let timg0 = 0x6000_8000u64;
    bus.write_u32(timg0 + 0x0C, 1).unwrap();
    assert_eq!(bus.read_u32(timg0 + 0x04).unwrap(), 0);
    // Closing the TIMG0 gate makes the window dead.
    bus.write_u32(TIMERGROUP0_CONF, 0).unwrap();
    assert_eq!(bus.read_u32(timg0 + 0x04).unwrap(), 0);
    // (Reopen for cleanliness; the bus is dropped after this test.)
    bus.write_u32(TIMERGROUP0_CONF, 1).unwrap();
}
