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
    if std::env::var("LABWIRED_REQUIRE_SENSOR_FEATURE_OFF").as_deref() == Ok("1") {
        assert!(
            !cfg!(feature = "event-scheduler"),
            "feature-off guest must not enable event-scheduler"
        );
        assert_eq!(
            interval, 1,
            "feature-off guest must use the per-cycle profile"
        );
    }
    println!(
        "MICROBIT_SENSOR_GUEST_PROFILE event_scheduler={} interval={interval}",
        cfg!(feature = "event-scheduler")
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf =
        std::env::temp_dir().join(format!("sensor-drdy-{}-{interval}.elf", std::process::id()));
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
        .arg("motion-drdy.S")
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(&elf).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    println!("MICROBIT_SENSOR_DRDY_GUEST_SHA256={hash};interval={interval}");
    if let Ok(dir) = std::env::var("LABWIRED_GUEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            PathBuf::from(dir).join(format!("sensor-drdy-{hash}-{interval}.elf")),
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
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 0);
    command(&mut m, 11); // start acceleration XYZ, route off
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 0);
    command(&mut m, 9); // expose already-ready acceleration
    observed(&mut m, 0, 1);
    command(&mut m, 12); // acceleration low byte not acknowledgement
    command(&mut m, 13); // X high only: Y/Z still pending
    observed(&mut m, 0, 1);
    command(&mut m, 20);
    assert_eq!(m.bus.read_u8(RAM + 0x18).unwrap(), 0x0e);
    command(&mut m, 13); // duplicate X high cannot clear another axis
    command(&mut m, 14);
    observed(&mut m, 0, 1);
    command(&mut m, 1); // second sensor now drives same collector line
    observed(&mut m, 0, 1); // cannot release acceleration
    command(&mut m, 15); // last accel high; magnetometer still holds line
    observed(&mut m, 0, 1);
    command(&mut m, 4); // mag high-only acknowledgement releases final driver
    observed(&mut m, 1, 1);
    command(&mut m, 2); // keep magnetometer masked for accel controls
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 0, 2); // autonomous acceleration recurrence
    command(&mut m, 10); // accel route mask releases, does not ack
    observed(&mut m, 1, 2);
    command(&mut m, 9);
    observed(&mut m, 0, 3);
    command(&mut m, 13);
    command(&mut m, 14);
    command(&mut m, 15);
    observed(&mut m, 1, 3);
    command(&mut m, 19); // cancel new conversion
    command(&mut m, 16); // active-low inactive raw high pulls collector low
    observed(&mut m, 0, 4);
    command(&mut m, 18); // start only X/Y
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 4); // active raw low releases board collector
    command(&mut m, 13);
    observed(&mut m, 1, 4);
    command(&mut m, 14); // Z disabled: only X/Y required
    observed(&mut m, 0, 5); // inactive raw high asserts collector
    command(&mut m, 17); // return default polarity
    observed(&mut m, 1, 5);
    command(&mut m, 19);
    m.run(Some(2_000_000)).unwrap();
    observed(&mut m, 1, 5); // no stale completion in power-down
}
#[test]
#[cfg(feature = "event-scheduler")]
fn guest_observes_sensor_drdy_precise_scheduler() {
    proof(1);
}
#[test]
#[cfg(feature = "event-scheduler")]
fn guest_observes_sensor_drdy_tick512_scheduler() {
    proof(512);
}

#[test]
#[cfg(not(feature = "event-scheduler"))]
fn guest_observes_sensor_drdy_feature_off_tick1() {
    proof(1);
}
