// SPDX-License-Identifier: MIT
//! Guest-triggered SAADC DMA completion and late-enabled IRQ7.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{
    bus::SystemBus, cpu::CortexM, system::cortex_m::configure_cortex_m, Bus, DebugControl, Machine,
};
use std::{path::PathBuf, process::Command};
#[test]
fn saadc_end_latches_until_guest_enables_and_acknowledges_irq() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf = std::env::temp_dir().join(format!("m1-saadc-irq-{}.elf", std::process::id()));
    assert!(Command::new("arm-none-eabi-gcc")
        .current_dir(root.join("examples/microbit-v2"))
        .args([
            "-mcpu=cortex-m4",
            "-mthumb",
            "-nostdlib",
            "-Wl,-T,board-io.ld",
            "-o"
        ])
        .arg(&elf)
        .arg("saadc-end-irq.S")
        .status()
        .unwrap()
        .success());
    use sha2::{Digest, Sha256};
    println!(
        "SAADC_IRQ_GUEST_SHA256={:x}",
        Sha256::digest(std::fs::read(&elf).unwrap())
    );
    let system = root.join("configs/systems/microbit-v2.yaml");
    let mut manifest = SystemManifest::from_file(&system).unwrap();
    let chip_path = system.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    manifest.chip = chip_path.to_str().unwrap().to_owned();
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let adc = bus.find_peripheral_index_by_name("saadc").unwrap();
    assert!(bus.peripherals[adc].dev.set_adc_channel_input(3, 1350));
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut m: Machine<CortexM> = Machine::new(cpu, bus);
    m.config.peripheral_tick_interval = 1;
    m.bus.config.peripheral_tick_interval = 1;
    m.load_firmware(&labwired_loader::load_elf(&elf).unwrap())
        .unwrap();
    std::fs::remove_file(elf).unwrap();
    m.run(Some(200_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x2000_0100).unwrap(),
        0,
        "masked IRQ entries"
    );
    assert_eq!(m.bus.read_u32(0x2000_0104).unwrap(), 1, "IRQ entries");
    assert_eq!(m.bus.read_u32(0x2000_0108).unwrap(), 1, "guest completed");
    assert_eq!(m.bus.read_u32(0x2000_010c).unwrap(), 1, "DMA amount");
    assert_eq!(
        m.bus.read_u32(0x4000_7104).unwrap(),
        0,
        "handler cleared END"
    );
}
