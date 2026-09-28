// SPDX-License-Identifier: MIT

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{
    bus::SystemBus, cpu::cortex_m::CortexM, memory::ProgramImage, Arch, Cpu, Machine,
};
use std::path::PathBuf;

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn pybadge_system_builds_with_exact_onboard_wiring() {
    let chip = ChipDescriptor::from_file(root("configs/chips/atsamd51-pybadge.yaml"))
        .expect("SAMD51 chip descriptor");
    let manifest = SystemManifest::from_file(root("configs/systems/pybadge.yaml"))
        .expect("PyBadge system manifest");
    let bus = SystemBus::from_config(&chip, &manifest).expect("PyBadge bus builds");

    assert_eq!(chip.reset_vector_offset, 0x4000);

    for name in ["porta", "portb", "sercom1"] {
        assert!(
            bus.peripherals.iter().any(|entry| entry.name == name),
            "missing {name}"
        );
    }
    assert_eq!(
        chip.pins.get("PA15").map(|p| (p.gpio.as_str(), p.bit)),
        Some(("porta", 15))
    );
    assert_eq!(
        chip.pins.get("PB30").map(|p| (p.gpio.as_str(), p.bit)),
        Some(("portb", 30))
    );

    // MakeCode Arcade UF2 application payloads omit the resident bootloader
    // and begin with their vector table at 0x4000. Prove that this is a bootable
    // contract, not merely a loader address accepted by the YAML parser.
    let mut machine = Machine::new(CortexM::new(), bus);
    let mut image = ProgramImage::new(0x4101, Arch::Arm);
    let mut application = vec![0_u8; 0x104];
    application[0..4].copy_from_slice(&0x2000_4000_u32.to_le_bytes());
    application[4..8].copy_from_slice(&0x0000_4101_u32.to_le_bytes());
    application[0x100..0x102].copy_from_slice(&0xe7fe_u16.to_le_bytes());
    image.add_segment(0x4000, application);
    machine
        .load_firmware(&image)
        .expect("load PyBadge application at 0x4000");
    assert_eq!(machine.cpu.get_sp(), 0x2000_4000);
    assert_eq!(machine.cpu.get_pc(), 0x4100);
}
