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

fn machine(autonomous: bool, invert_a: bool, gpiote_guest: bool) -> Machine<CortexM> {
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
        .arg(if gpiote_guest {
            "shared-gpiote.S"
        } else {
            "shared-open-drain.S"
        })
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
        if invert_a && id == "probe-a" {
            manifest.external_devices.last_mut().unwrap().config.insert(
                "inverting_collectors".into(),
                serde_yaml::from_str("[P0]").unwrap(),
            );
        }
    }
    if autonomous {
        let desc = labwired_config::DeviceDescriptor::from_yaml(r#"
schema: labwired.part/v1
type: autonomous_ready_probe
behavior:
  primitive: i2c_device
  i2c:
    default_address: 0x22
    registers:
      - { name: START, addr: 0, width: 1, endian: be, access: rw, reset: 0 }
      - { name: STATUS, addr: 1, width: 1, endian: be, access: r, reset: 0 }
      - { name: RESULT, addr: 2, width: 1, endian: be, access: r, reset: 0 }
    data_ready:
      - { name: sample, start_register: START, start_mask: 1, ready_register: STATUS, ready_mask: 1, conversion_us: 1000, clear_on_read: [RESULT] }
  outputs: [INT]
  output_pins: { INT: int_pin }
  rules:
    - on: { data_ready: sample }
      do: [ { pin: INT, level: 0 } ]
    - on: { read: RESULT }
      do: [ { pin: INT, level: 1 } ]
"#).unwrap();
        manifest
            .parts
            .push(labwired_config::PartPack::Inline(Box::new(desc)));
        manifest.external_devices.push(ExternalDevice {
            id: "autonomous-probe".into(),
            r#type: "autonomous_ready_probe".into(),
            connection: "i2c0".into(),
            channel: None,
            route: Default::default(),
            config: [
                ("int_pin".into(), serde_yaml::Value::from("P0.25")),
                (
                    "open_drain_pullups".into(),
                    serde_yaml::from_str("[INT]").unwrap(),
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

/// Host-configure the actual TWIM and EasyDMA while the guest samples GPIO.
/// A read uses the controller's repeated-START shortcut after the pointer.
fn probe_transfer(
    machine: &mut Machine<CortexM>,
    address: u8,
    bytes: &[u8],
    read: bool,
) -> (u32, u32, u32, u32) {
    const TWIM: u64 = 0x4000_3000;
    const TX: u64 = 0x2000_0200;
    const RX: u64 = 0x2000_0210;
    for (index, byte) in bytes.iter().enumerate() {
        machine.bus.write_u8(TX + index as u64, *byte).unwrap();
    }
    machine.bus.write_u32(TWIM + 0x500, 6).unwrap();
    machine.bus.write_u32(TWIM + 0x104, 0).unwrap();
    machine.bus.write_u32(TWIM + 0x124, 0).unwrap();
    machine.bus.write_u32(TWIM + 0x4c4, 6).unwrap();
    machine
        .bus
        .write_u32(
            TWIM + 0x200,
            if read { (1 << 7) | (1 << 12) } else { 1 << 9 },
        )
        .unwrap();
    machine
        .bus
        .write_u32(TWIM + 0x588, u32::from(address))
        .unwrap();
    machine.bus.write_u32(TWIM + 0x544, TX as u32).unwrap();
    machine
        .bus
        .write_u32(TWIM + 0x548, bytes.len() as u32)
        .unwrap();
    if read {
        machine.bus.write_u32(TWIM + 0x534, RX as u32).unwrap();
        machine.bus.write_u32(TWIM + 0x538, 1).unwrap();
    }
    machine.bus.write_u32(TWIM + 0x008, 1).unwrap();
    // Poll promptly so this test can cancel a still-pending conversion;
    // an arbitrary long post-transfer run could pass its deadline first.
    for _ in 0..5_000 {
        if machine.bus.read_u32(TWIM + 0x104).unwrap() != 0 {
            return (
                machine.bus.read_u32(TWIM + 0x124).unwrap(),
                machine.bus.read_u32(TWIM + 0x4c4).unwrap(),
                machine.bus.read_u32(TWIM + 0x54c).unwrap(),
                if read {
                    machine.bus.read_u32(TWIM + 0x53c).unwrap()
                } else {
                    0
                },
            );
        }
        machine.run(Some(32)).unwrap();
    }
    panic!("TWIM transfer did not stop");
}

fn write_probe(machine: &mut Machine<CortexM>, address: u8, value: u8) {
    assert_eq!(
        probe_transfer(machine, address, &[value], false),
        (0, 0, 1, 0)
    );
}

fn write_probe_register(machine: &mut Machine<CortexM>, register: u8, value: u8) {
    assert_eq!(
        probe_transfer(machine, 0x22, &[register, value], false),
        (0, 0, 2, 0)
    );
}

fn read_probe_register(machine: &mut Machine<CortexM>, register: u8) -> u8 {
    assert_eq!(
        probe_transfer(machine, 0x22, &[register], true),
        (0, 0, 1, 1)
    );
    machine.bus.read_u8(0x2000_0210).unwrap()
}

#[test]
fn twim_probe_rejects_an_absent_address() {
    let mut machine = machine(false, false, false);
    let (error, source, tx, rx) = probe_transfer(&mut machine, 0x26, &[0xff], false);
    assert_eq!((error, source & 2, tx, rx), (1, 2, 0, 0));
}

#[test]
fn guest_observes_two_independent_retained_drivers() {
    let mut machine = machine(false, false, false);
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

/// The host starts a fixture conversion; the guest samples P0.25 without a
/// status-register read. This is a timing/event fixture, not a sensor model.
#[test]
fn guest_sees_autonomous_data_ready_without_polling() {
    let mut m = machine(true, false, false);
    m.run(Some(1_000)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 1, "initial pull-up");
    write_probe_register(&mut m, 0, 1);
    m.run(Some(100)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 1, "no early assertion");
    write_probe_register(&mut m, 0, 0);
    m.run(Some(200_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x20000100).unwrap(),
        1,
        "cancelled conversion stays released"
    );
    write_probe_register(&mut m, 0, 1);
    m.run(Some(100)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 1, "no early assertion");
    m.run(Some(200_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x20000100).unwrap(),
        0,
        "guest saw completion without polling status"
    );
    read_probe_register(&mut m, 2);
    m.run(Some(100)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x20000100).unwrap(),
        1,
        "result read releases the line"
    );
    m.run(Some(200_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x20000100).unwrap(),
        0,
        "next conversion asserts again"
    );
}

/// Test the explicit board adapter with host-driven device outputs and a
/// guest reading the resolved pin. This fixture is not an LSM303AGR model.
#[test]
fn guest_observes_inverting_collector_and_direct_shared_driver() {
    let mut m = machine(false, true, false);
    for (address, value, expected) in [
        (0x20, 0xfe, 1),
        (0x21, 0xff, 1),
        (0x20, 0xff, 0),
        (0x21, 0xfe, 0),
        (0x20, 0xfe, 0),
        (0x21, 0xff, 1),
        (0x21, 0xfe, 0),
        (0x20, 0xff, 0),
        (0x21, 0xff, 0),
        (0x20, 0xfe, 1),
    ] {
        write_probe(&mut m, address, value);
        m.run(Some(10_000)).unwrap();
        assert_eq!(
            m.bus.read_u32(0x20000100).unwrap(),
            expected,
            "after {address:#04x}={value:#04x}"
        );
    }
}

/// Configure a falling-edge GPIOTE channel in the source-built guest after
/// the shared line is already high. Its first assertion must be observable.
#[test]
fn guest_observes_first_gpiote_falling_edge_after_channel_enable() {
    let mut m = machine(false, true, true);
    m.run(Some(10_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(0x20000100).unwrap(),
        1,
        "initial pulled-up input"
    );
    assert_eq!(m.bus.read_u32(0x20000108).unwrap(), 0, "no startup event");
    write_probe(&mut m, 0x20, 0xff); // high source asserts through transistor
    m.run(Some(10_000)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 0);
    assert_eq!(
        m.bus.read_u32(0x20000108).unwrap(),
        1,
        "first falling event"
    );
    write_probe(&mut m, 0x20, 0xfe); // low source releases
    m.run(Some(10_000)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 1);
    assert_eq!(m.bus.read_u32(0x20000108).unwrap(), 1, "rising edge masked");
    write_probe(&mut m, 0x21, 0xfe); // direct driver pulls low
    m.run(Some(10_000)).unwrap();
    assert_eq!(m.bus.read_u32(0x20000100).unwrap(), 0);
    assert_eq!(
        m.bus.read_u32(0x20000108).unwrap(),
        2,
        "second falling event"
    );
}
