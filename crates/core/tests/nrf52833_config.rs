// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Config-build gate for the native nRF52833 / micro:bit v2 descriptors.

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::SystemBus;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn nrf52833_from_config_builds() {
    let sys = workspace_root().join("configs/systems/microbit-v2.yaml");
    let mut manifest =
        SystemManifest::from_file(&sys).unwrap_or_else(|e| panic!("load microbit-v2: {e}"));
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path)
        .unwrap_or_else(|e| panic!("load chip for microbit-v2: {e}"));
    manifest.chip = chip_path.to_str().expect("utf-8 chip path").to_string();
    let bus = SystemBus::from_config(&chip, &manifest).expect("nrf52833 / microbit-v2 must build");
    // micro:bit v2 UART is bridged to the interface MCU through UARTE0
    // (target TX = P0.06); the two GPIO ports carry the buttons and matrix.
    assert!(
        bus.find_peripheral_index_by_name("uart0").is_some(),
        "bus must expose uart0 (UARTE0 console)"
    );
    assert!(
        bus.find_peripheral_index_by_name("gpio0").is_some(),
        "bus must expose gpio0 (P0)"
    );
    assert!(
        bus.find_peripheral_index_by_name("gpio1").is_some(),
        "bus must expose gpio1 (P1)"
    );
    assert!(
        bus.find_peripheral_index_by_name("clock").is_some(),
        "bus must expose clock"
    );
}

/// A firmware image's UICR record must survive `Machine::load_firmware`.
///
/// Segments that miss flash and RAM are written byte by byte through the bus;
/// the UICR model used to accept byte writes and drop them, so the record the
/// micro:bit V2 MakeCode hex carries (NRFFW[0] = 0x77000, the bootloader start;
/// NRFFW[1] = 0x7E000, the MBR params page) vanished without a warning. CODAL
/// computes its flash-storage page from NRFFW[0] and hard-faulted on the
/// erased 0xFFFFFFFF.
#[test]
fn nrf52833_load_firmware_programs_the_uicr_record() {
    use labwired_core::{cpu::cortex_m::CortexM, memory::ProgramImage, Arch, Bus, Machine};

    let sys = workspace_root().join("configs/systems/microbit-v2.yaml");
    let mut manifest = SystemManifest::from_file(&sys).expect("load microbit-v2");
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).expect("load nrf52833 chip");
    manifest.chip = chip_path.to_str().expect("utf-8 chip path").to_string();
    let bus = SystemBus::from_config(&chip, &manifest).expect("nrf52833 bus");
    let mut machine = Machine::new(CortexM::new(), bus);

    let mut image = ProgramImage::new(0x101, Arch::Arm);
    // Minimal vector table: SP, reset -> 0x100 (thumb).
    let mut flash = vec![0u8; 0x104];
    flash[0..4].copy_from_slice(&0x2000_4000u32.to_le_bytes());
    flash[4..8].copy_from_slice(&0x0000_0101u32.to_le_bytes());
    flash[0x100..0x102].copy_from_slice(&0xE7FEu16.to_le_bytes()); // b .
    image.add_segment(0, flash);
    // The hex record `:081014000070070000E0070076`.
    image.add_segment(
        0x1000_1014,
        vec![0x00, 0x70, 0x07, 0x00, 0x00, 0xE0, 0x07, 0x00],
    );
    machine.load_firmware(&image).expect("load firmware");

    assert_eq!(
        machine.bus.read_u32(0x1000_1014).unwrap(),
        0x0007_7000,
        "UICR NRFFW[0] must hold the image's bootloader address"
    );
    assert_eq!(machine.bus.read_u32(0x1000_1018).unwrap(), 0x0007_E000);
    assert_eq!(
        machine.bus.read_u32(0x1000_101C).unwrap(),
        0xFFFF_FFFF,
        "fields the image does not program stay erased"
    );
}
