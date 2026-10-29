// SPDX-License-Identifier: MIT
//! Guest owns TWIM0, sensor routing/acknowledgement and GPIO/GPIOTE reads.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::{Bus, DebugControl, Machine};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Command};
const RAM: u64 = 0x20000100;

fn machine(interval: u32) -> Machine<CortexM> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf = std::env::temp_dir().join(format!("mag-drdy-{}-{interval}.elf", std::process::id()));
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
        .arg("mag-drdy.S")
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(&elf).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    println!("MICROBIT_MAG_DRDY_GUEST_SHA256={hash};interval={interval}");
    if let Ok(dir) = std::env::var("LABWIRED_GUEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            PathBuf::from(dir).join(format!("mag-drdy-{hash}-{interval}.elf")),
            bytes,
        )
        .unwrap();
    }
    let system = root.join("configs/systems/microbit-v2.yaml");
    let mut manifest = SystemManifest::from_file(&system).unwrap();
    let chip_path = system.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    manifest.chip = chip_path.to_str().unwrap().to_string();
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    m.config.peripheral_tick_interval = interval;
    m.bus.config.peripheral_tick_interval = interval;
    m.load_firmware(&labwired_loader::load_elf(&elf).unwrap())
        .unwrap();
    std::fs::remove_file(elf).unwrap();
    m
}
fn observed(m: &mut Machine<CortexM>, line: u32, falls: u32) {
    assert_eq!(m.bus.read_u32(RAM).unwrap(), line, "guest GPIO P0.25");
    assert_eq!(
        m.bus.read_u32(RAM + 8).unwrap(),
        falls,
        "guest falling events"
    );
    assert_eq!(m.bus.read_u32(RAM + 12).unwrap(), 0, "guest bus errors");
}
fn command(m: &mut Machine<CortexM>, code: u32) {
    m.bus.write_u32(RAM + 0x14, 0).unwrap();
    m.bus.write_u32(RAM + 0x10, code).unwrap();
    m.run(Some(10_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(RAM + 0x14).unwrap(),
        code,
        "guest completed command"
    );
    assert_eq!(m.bus.read_u32(RAM + 0x10).unwrap(), 0);
}
fn proof(interval: u32) {
    let mut m = machine(interval);
    m.run(Some(2_000_000)).unwrap(); // conversion without sensor/status reads
    assert_eq!(m.bus.read_u8(RAM + 0x1c).unwrap(), 0x40);
    observed(&mut m, 1, 0); // ready but route default masked
    command(&mut m, 1); // expose an already-ready sample
    observed(&mut m, 0, 1);
    command(&mut m, 7); // status is not acknowledgement
    assert_eq!(m.bus.read_u8(RAM + 0x18).unwrap() & 8, 8);
    observed(&mut m, 0, 1);
    command(&mut m, 8); // threshold source is not acknowledgement
    command(&mut m, 3); // low byte is not acknowledgement
    observed(&mut m, 0, 1);
    command(&mut m, 2); // routing mask releases collector
    observed(&mut m, 1, 1);
    command(&mut m, 1); // sample still ready
    observed(&mut m, 0, 2);
    command(&mut m, 4); // high-byte-only acknowledges
    observed(&mut m, 1, 2);
    command(&mut m, 7);
    assert_eq!(m.bus.read_u8(RAM + 0x18).unwrap() & 8, 0);
    m.run(Some(2_000_000)).unwrap(); // second autonomous conversion
    observed(&mut m, 0, 3);
    command(&mut m, 5); // idle preserves unread ready sample
    observed(&mut m, 0, 3);
    command(&mut m, 4);
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 3); // no new conversion in idle
    command(&mut m, 6);
    command(&mut m, 5); // cancel in-flight conversion
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 3);
}
#[test]
fn guest_observes_mag_drdy_precise_scheduler() {
    proof(1);
}
#[test]
fn guest_observes_mag_drdy_tick512_scheduler() {
    proof(512);
}
