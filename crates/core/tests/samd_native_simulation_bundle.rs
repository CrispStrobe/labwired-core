// SPDX-License-Identifier: MIT
//! Assembly controls only. Actual native/WASM game qualification is separate.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{bus::SystemBus, Bus};

#[test]
fn explicit_simulation_bundle_selects_models_and_named_inputs() {
    let chip: ChipDescriptor = serde_yaml::from_str(include_str!(
        "../../../configs/chips/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    let manifest: SystemManifest = serde_yaml::from_str(include_str!(
        "../../../configs/systems/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    assert_eq!(chip.reset_vector_offset, 0);
    let selected = |id: &str, kind: &str| {
        assert_eq!(
            chip.peripherals
                .iter()
                .filter(|p| p.id == id && p.r#type == kind)
                .count(),
            1
        );
    };
    selected("usb", "samd51_usb_device_banks");
    selected("dmac", "samd51_dmac_board");
    selected("nvmctrl", "samd51_nvmctrl_manual_storage");
    selected("tcc0", "samd51_tcc");
    assert_eq!(chip.memory_regions.len(), 2);
    assert_eq!(chip.memory_regions[0].base, 0x00800000);
    assert_eq!(chip.memory_regions[1].base, 0x00806000);
    assert_eq!(manifest.external_devices.len(), 2);
    let bus = SystemBus::from_config(&chip, &manifest).expect("registered explicit assembly");
    assert!(bus.peripherals.iter().any(|p| p.name == "sercom4"));
    assert!(
        !bus.gpio_devices.is_empty(),
        "actual button/reset/backlight kits attached"
    );
}

#[test]
fn narrow_base_controls_do_not_admit_reserved_word_reads() {
    let chip: ChipDescriptor = serde_yaml::from_str(include_str!(
        "../../../configs/chips/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    let manifest: SystemManifest = serde_yaml::from_str(include_str!(
        "../../../configs/systems/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    let bus = SystemBus::from_config(&chip, &manifest).unwrap();
    assert!(bus.read_u16(0x41004000).is_ok(), "actual NVM CTRLA width");
    assert!(
        bus.read_u32(0x41004000).is_err(),
        "reserved bytes stay refused"
    );
    assert!(bus.read_u8(0x41000000).is_ok(), "actual USB CTRLA width");
    assert!(
        bus.read_u32(0x41000000).is_err(),
        "reserved USB bytes stay refused"
    );
    assert!(bus.read_u8(0x43002400).is_ok(), "actual DAC CTRLA width");
    assert!(
        bus.read_u32(0x43002400).is_err(),
        "reserved bytes stay refused"
    );
}
