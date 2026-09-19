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
