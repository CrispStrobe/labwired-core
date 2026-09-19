// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Config-build gate for the native STM32G071RB / NUCLEO-G071RB descriptors.

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
fn stm32g071_from_config_builds() {
    let sys = workspace_root().join("configs/systems/nucleo-g071rb.yaml");
    let mut manifest =
        SystemManifest::from_file(&sys).unwrap_or_else(|e| panic!("load nucleo-g071rb: {e}"));
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path)
        .unwrap_or_else(|e| panic!("load chip for nucleo-g071rb: {e}"));
    manifest.chip = chip_path.to_str().expect("utf-8 chip path").to_string();
    let bus =
        SystemBus::from_config(&chip, &manifest).expect("stm32g071 / nucleo-g071rb must build");

    // Peripheral bases the smoke path and the board wiring depend on.
    // USART2 is the ST-LINK VCP console; GPIOA carries LD4 (PA5), GPIOC
    // carries B1 (PC13).
    for id in ["rcc", "gpioa", "gpioc", "usart2", "usart1", "systick"] {
        assert!(
            bus.find_peripheral_index_by_name(id).is_some(),
            "bus must expose {id}"
        );
    }

    // DS12232: 128 KB flash @ 0x08000000, 36 KB SRAM @ 0x20000000. The RAM
    // window is larger than the CMSIS header's SRAM_SIZE_MAX (32 KB), which
    // is the family maximum for the smaller SKUs.
    assert_eq!(chip.flash.base, 0x0800_0000, "flash base");
    assert_eq!(chip.flash.size, 128 * 1024, "flash size (DS12232)");
    assert_eq!(chip.ram.base, 0x2000_0000, "SRAM base");
    assert_eq!(chip.ram.size, 36 * 1024, "SRAM size (DS12232)");

    // Every `clock:` gate in the yaml resolved through the dedicated
    // `stm32g0` RCC layout (SystemBus::from_config errors on an unresolved
    // gate), so this additionally pins the G0 enable-register offsets.
    assert_eq!(chip.cpu_hz, 64_000_000, "max core clock (DS12232)");
}
