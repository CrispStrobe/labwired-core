// SPDX-License-Identifier: MIT
//! Guest-owned EGU event, late-enabled interrupt and PPI task delivery.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::CortexM;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::{Bus, Machine};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn run_guest(board: &str, interval: u32) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let elf = std::env::temp_dir().join(format!(
        "egu-ppi-{}-{}.elf",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
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
        .arg("egu-irq-ppi.S")
        .status()
        .unwrap()
        .success());
    let system = root.join(format!("configs/systems/{board}.yaml"));
    let mut manifest = SystemManifest::from_file(&system).unwrap();
    let chip_path = system.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    assert_eq!(chip.name, "nrf52840");
    manifest.chip = chip_path.to_str().unwrap().to_string();
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut machine: Machine<CortexM> = Machine::new(cpu, bus);
    machine.config.peripheral_tick_interval = interval;
    machine.bus.config.peripheral_tick_interval = interval;
    machine
        .load_firmware(&labwired_loader::load_elf(&elf).unwrap())
        .unwrap();
    std::fs::remove_file(elf).unwrap();
    machine.run(Some(200_000)).unwrap();
    assert_eq!(
        machine.bus.read_u32(0x2000_0100).unwrap(),
        1,
        "guest observed PPI task event"
    );
    assert_eq!(
        machine.bus.read_u32(0x2000_0104).unwrap(),
        1,
        "late-enabled IRQ entered exactly once"
    );
    assert_eq!(
        machine.bus.read_u32(0x2000_0108).unwrap(),
        1,
        "guest completed"
    );
    assert_eq!(
        machine.bus.read_u32(0x2000_010c).unwrap(),
        0,
        "no IRQ while the EGU interrupt was masked"
    );
    assert_eq!(
        machine.bus.read_u32(0x4001_4100).unwrap(),
        0,
        "IRQ handler cleared event"
    );
}

#[test]
fn guest_routes_egu_event_and_services_late_enabled_irq() {
    let intervals: &[u32] = if labwired_core::cycle_clock::event_scheduler_enabled() {
        &[1, 512]
    } else {
        &[1]
    };
    for board in ["nrf52840-dk", "seeed-xiao-nrf52840-sense"] {
        for &interval in intervals {
            run_guest(board, interval);
        }
    }
}
