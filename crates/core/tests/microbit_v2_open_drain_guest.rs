// SPDX-License-Identifier: MIT
//! Source-built guest samples a shared GPIO; host writes exercise two I2C outputs.
use labwired_config::{ChipDescriptor, ExternalDevice, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::{Bus, DebugControl, Machine};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn machine() -> Machine<CortexM> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf = std::env::temp_dir().join(format!(
        "microbit-open-drain-{}-{sequence}.elf",
        std::process::id()
    ));
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
        .arg("shared-open-drain.S")
        .status()
        .unwrap()
        .success());
    let payload = std::fs::read(&elf).unwrap();
    let hash = format!("{:x}", Sha256::digest(&payload));
    println!("MICROBIT_OPEN_DRAIN_GUEST_SHA256={hash}");
    if let Ok(dir) = std::env::var("LABWIRED_GUEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            PathBuf::from(dir).join(format!("open-drain-{hash}-{sequence}.elf")),
            &payload,
        )
        .unwrap();
    }
    let system = root.join("configs/systems/microbit-v2.yaml");
    let mut manifest = SystemManifest::from_file(&system).unwrap();
    let chip_path = system.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    manifest.chip = chip_path.to_str().unwrap().to_string();
    for (id, address) in [("probe-a", 0x20), ("probe-b", 0x21)] {
        manifest.external_devices.push(ExternalDevice {
            id: id.into(),
            r#type: "pcf8574".into(),
            connection: "i2c0".into(),
            channel: None,
            route: Default::default(),
            config: [
                ("i2c_address".into(), serde_yaml::Value::from(address)),
                ("p0_pin".into(), serde_yaml::Value::from("P0.25")),
                (
                    "open_drain_pullups".into(),
                    serde_yaml::from_str("[P0]").unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
        });
    }
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let interval = bus.max_safe_tick_interval();
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut machine = Machine::new(cpu, bus);
    machine.config.peripheral_tick_interval = interval;
    machine.bus.config.peripheral_tick_interval = interval;
    machine
        .load_firmware(&labwired_loader::load_elf(&elf).unwrap())
        .unwrap();
    std::fs::remove_file(elf).unwrap();
    machine
}

/// Host-configure the actual TWIM and EasyDMA; the source-built guest keeps
/// sampling GPIO while the controller sends a byte to the attached device.
fn probe_transfer(machine: &mut Machine<CortexM>, address: u8, value: u8) -> (u32, u32, u32) {
    const TWIM: u64 = 0x4000_3000;
    const BUFFER: u64 = 0x2000_0200;
    machine.bus.write_u8(BUFFER, value).unwrap();
    machine.bus.write_u32(TWIM + 0x500, 6).unwrap(); // ENABLE = TWIM
    machine.bus.write_u32(TWIM + 0x104, 0).unwrap(); // EVENTS_STOPPED
    machine.bus.write_u32(TWIM + 0x124, 0).unwrap(); // EVENTS_ERROR
    machine.bus.write_u32(TWIM + 0x4c4, 6).unwrap(); // clear ERRORSRC
    machine.bus.write_u32(TWIM + 0x200, 1 << 9).unwrap(); // LASTTX_STOP
    machine
        .bus
        .write_u32(TWIM + 0x588, u32::from(address))
        .unwrap();
    machine.bus.write_u32(TWIM + 0x544, BUFFER as u32).unwrap();
    machine.bus.write_u32(TWIM + 0x548, 1).unwrap();
    machine.bus.write_u32(TWIM + 0x008, 1).unwrap(); // TASKS_STARTTX
    machine.run(Some(100_000)).unwrap();
    assert_eq!(
        machine.bus.read_u32(TWIM + 0x104).unwrap(),
        1,
        "transfer stopped"
    );
    (
        machine.bus.read_u32(TWIM + 0x124).unwrap(),
        machine.bus.read_u32(TWIM + 0x4c4).unwrap(),
        machine.bus.read_u32(TWIM + 0x54c).unwrap(),
    )
}

fn write_probe(machine: &mut Machine<CortexM>, address: u8, value: u8) {
    assert_eq!(
        probe_transfer(machine, address, value),
        (0, 0, 1),
        "probe must ACK and receive one byte"
    );
}

#[test]
fn twim_probe_rejects_an_absent_address() {
    let mut machine = machine();
    let (error, source, amount) = probe_transfer(&mut machine, 0x26, 0xff);
    assert_eq!(
        (error, source & 2, amount),
        (1, 2, 0),
        "missing probe must NACK"
    );
}

#[test]
fn guest_observes_two_independent_retained_drivers() {
    let mut machine = machine();
    let mut last_count = 0;
    for (address, value, expected) in [
        (0x20, 0xff, 1),
        (0x20, 0xfe, 0),
        (0x21, 0xfe, 0),
        (0x20, 0xff, 0),
        (0x21, 0xff, 1),
        (0x21, 0xfe, 0),
        (0x20, 0xfe, 0),
        (0x21, 0xff, 0),
        (0x20, 0xff, 1),
    ] {
        write_probe(&mut machine, address, value);
        machine.run(Some(10_000)).unwrap();
        assert_eq!(machine.bus.read_u32(0x20000100).unwrap(), expected);
        let count = machine.bus.read_u32(0x20000104).unwrap();
        assert!(
            count > last_count,
            "guest must sample after each transition"
        );
        last_count = count;
    }
}
