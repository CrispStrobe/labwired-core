// SPDX-License-Identifier: MIT
//! GPIOTE event regression on the unchanged DK/XIAO system manifests.
//! P0.17 and PCF8574 are explicit synthetic test wiring, not onboard hardware.
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
const RAM: u64 = 0x20000100;

fn machine(board: &str, interval: u32, polarity: u32) -> Machine<CortexM> {
    if std::env::var("LABWIRED_REQUIRE_GPIOTE_FEATURE_OFF").as_deref() == Ok("1") {
        assert!(
            !cfg!(feature = "event-scheduler"),
            "GPIOTE feature-off must disable event-scheduler"
        );
        assert_eq!(interval, 1, "GPIOTE feature-off must use per-cycle ticking");
    }
    println!("NRF52840_GPIOTE_GUEST_PROFILE board={board} event_scheduler={} interval={interval} polarity={polarity}", cfg!(feature = "event-scheduler"));
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf = std::env::temp_dir().join(format!(
        "gpiote-{board}-{}-{sequence}-{interval}-{polarity}.elf",
        std::process::id()
    ));
    assert!(Command::new("arm-none-eabi-gcc")
        .current_dir(root.join("examples/microbit-v2"))
        .args([
            "-mcpu=cortex-m4",
            "-mthumb",
            "-nostdlib",
            "-Wl,-T,board-io.ld",
            "-DGPIOTE_DRIVE_PCF",
            "-DGPIOTE_TEST_PIN=17"
        ])
        .arg(format!("-DGPIOTE_TEST_POLARITY={polarity}"))
        .arg("-o")
        .arg(&elf)
        .arg("shared-gpiote.S")
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(&elf).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    println!(
        "NRF52840_GPIOTE_GUEST_SHA256={hash};board={board};interval={interval};polarity={polarity}"
    );
    if let Ok(dir) = std::env::var("LABWIRED_GUEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            PathBuf::from(dir).join(format!(
                "gpiote-{board}-event{}-tick{interval}-polarity{polarity}-{hash}.elf",
                u8::from(cfg!(feature = "event-scheduler"))
            )),
            bytes,
        )
        .unwrap();
    }
    let system = root.join(format!("configs/systems/{board}.yaml"));
    let mut manifest = SystemManifest::from_file(&system).unwrap();
    assert_eq!(manifest.name, board);
    assert!(
        manifest.external_devices.is_empty(),
        "review fixture ownership if a real device is added to either manifest"
    );
    assert!(
        !manifest
            .board_io
            .iter()
            .any(|b| b.peripheral == "gpio0" && b.pin == 17),
        "fixture must not override a board I/O binding"
    );
    let chip_path = system.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    assert_eq!(chip.name, "nrf52840");
    manifest.chip = chip_path.to_str().unwrap().to_string();
    manifest.external_devices.push(ExternalDevice {
        id: "gpiote-pad-fixture".into(),
        r#type: "pcf8574".into(),
        connection: "i2c0".into(),
        channel: None,
        route: Default::default(),
        config: [
            ("i2c_address".into(), serde_yaml::Value::from(0x20)),
            ("p0_pin".into(), serde_yaml::Value::from("P0.17")),
            (
                "open_drain_pullups".into(),
                serde_yaml::from_str("[P0]").unwrap(),
            ),
        ]
        .into_iter()
        .collect(),
    });
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
    assert_eq!(m.bus.read_u32(RAM).unwrap(), line, "guest GPIO input");
    assert_eq!(
        m.bus.read_u32(RAM + 8).unwrap(),
        falls,
        "guest falling events"
    );
    assert_eq!(m.bus.read_u32(RAM + 12).unwrap(), 0, "guest TWIM errors");
}
fn command(m: &mut Machine<CortexM>, code: u32) {
    let before = m.bus.read_u32(RAM + 4).unwrap();
    m.bus.write_u32(RAM + 0x14, 0).unwrap();
    m.bus.write_u32(RAM + 0x10, code).unwrap();
    m.run(Some(10_000)).unwrap();
    assert_eq!(
        m.bus.read_u32(RAM + 0x14).unwrap(),
        code,
        "fresh guest command completion"
    );
    assert_eq!(m.bus.read_u32(RAM + 0x10).unwrap(), 0);
    assert_eq!(
        m.bus.read_u32(RAM + 0x1c).unwrap(),
        1,
        "actual one-byte EasyDMA transfer"
    );
    assert!(
        m.bus.read_u32(RAM + 4).unwrap() > before,
        "fresh guest sample"
    );
}
fn proof(board: &str, interval: u32, polarity: u32) {
    let mut m = machine(board, interval, polarity);
    m.run(Some(10_000)).unwrap();
    observed(&mut m, 1, 0); // input high before GPIOTE is configured
    command(&mut m, 1);
    observed(&mut m, 0, 1); // first falling edge must not be discarded
    command(&mut m, 1);
    observed(&mut m, 0, 1); // repeated low is not a second edge
    command(&mut m, 2);
    observed(&mut m, 1, 1); // rising transition masked
    command(&mut m, 1);
    observed(&mut m, 0, 2); // second falling edge still works
}
#[test]
#[cfg(feature = "event-scheduler")]
fn dk_first_falling_edge_scheduler_tick1() {
    proof("nrf52840-dk", 1, 2);
}
#[test]
#[cfg(feature = "event-scheduler")]
fn dk_first_falling_edge_scheduler_tick512() {
    proof("nrf52840-dk", 512, 2);
}
#[test]
#[cfg(feature = "event-scheduler")]
fn xiao_first_falling_edge_scheduler_tick1() {
    proof("seeed-xiao-nrf52840-sense", 1, 2);
}
#[test]
#[cfg(feature = "event-scheduler")]
fn xiao_first_falling_edge_scheduler_tick512() {
    proof("seeed-xiao-nrf52840-sense", 512, 2);
}
#[test]
#[cfg(not(feature = "event-scheduler"))]
fn dk_first_falling_edge_feature_off_tick1() {
    proof("nrf52840-dk", 1, 2);
}
#[test]
#[cfg(not(feature = "event-scheduler"))]
fn xiao_first_falling_edge_feature_off_tick1() {
    proof("seeed-xiao-nrf52840-sense", 1, 2);
}
#[test]
fn first_falling_control_rejects_rising_only_guest() {
    let mut m = machine("nrf52840-dk", 1, 1);
    m.run(Some(10_000)).unwrap();
    observed(&mut m, 1, 0);
    command(&mut m, 1);
    observed(&mut m, 0, 0); // the bus/guest ran, but rising-only missed the fall
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            observed(&mut m, 0, 1);
        }))
        .is_err(),
        "the unchanged first-falling criterion must reject this guest"
    );
    command(&mut m, 2);
    observed(&mut m, 1, 1); // event path is live: the rising-only guest sees release
}
