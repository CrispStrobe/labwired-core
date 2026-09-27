//! EXPLORATORY probe (temporary): boot the BLE_notify / BLE_client fixtures
//! and print their USB-CDC serial. Replaced by the real gate once it works.
#![cfg(feature = "event-scheduler")]

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::boot::esp32c3_rom::{
    build_rom_boot_machine, c3_rom_data_init_writes, inject_rom_regions, RomBootOpts,
};
use labwired_core::boot::esp32s3_rom::RomImages;
use labwired_core::bus::SystemBus;
use labwired_core::cpu::RiscV;
use labwired_core::memory::ProgramImage;
use labwired_core::network::SimMqttFabric;
use labwired_core::peripherals::ble_air::BleAirBus;
use labwired_core::peripherals::nrf52::radio::VirtualAirBus;
use labwired_core::{Arch, Bus, Cpu, Machine};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn bootloader_image(flash: &[u8]) -> ProgramImage {
    let segment_count = flash[1] as usize;
    let entry = u32::from_le_bytes(flash[4..8].try_into().unwrap()) as u64;
    let mut program = ProgramImage::new(entry, Arch::RiscV);
    let mut cursor = 24;
    for _ in 0..segment_count {
        let load_addr = u32::from_le_bytes(flash[cursor..cursor + 4].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(flash[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        cursor += 8;
        program.add_segment(load_addr, flash[cursor..cursor + len].to_vec());
        cursor += len;
    }
    program
}

struct Node {
    machine: Machine<RiscV>,
    serial: Arc<Mutex<Vec<u8>>>,
}

fn build_node(flash: &[u8], ble: &BleAirBus, node_id: &str) -> Node {
    let chip = ChipDescriptor::from_file(root().join("../../configs/chips/esp32c3.yaml")).unwrap();
    let manifest =
        SystemManifest::from_file(root().join("../../configs/systems/esp32c3-devkit.yaml"))
            .unwrap();
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let irom = std::fs::read(root().join("roms/esp32c3/esp32c3_rom.bin")).unwrap();
    let drom = std::fs::read(root().join("roms/esp32c3/esp32c3_drom.bin")).unwrap();
    assert!(inject_rom_regions(
        &mut bus,
        &RomImages {
            irom: irom.clone(),
            drom
        }
    ));
    for (dst, bytes) in c3_rom_data_init_writes(&irom) {
        for (i, b) in bytes.iter().enumerate() {
            let _ = bus.write_u8(dst as u64 + i as u64, *b);
        }
    }
    let serial = Arc::new(Mutex::new(Vec::new()));
    bus.attach_uart_tx_sink(serial.clone(), false);
    let bootloader = bootloader_image(flash);
    let mut machine = build_rom_boot_machine(
        bus,
        flash.to_vec(),
        RomBootOpts {
            pinned_efuse_mac: None,
            usb_serial_sink: Some(serial.clone()),
        },
        |c| c,
    );
    let nrf = VirtualAirBus::new();
    machine
        .bus
        .attach_lab_air(node_id, nrf, ble.clone(), SimMqttFabric::new());
    for segment in &bootloader.segments {
        if machine.bus.flash.load_from_segment(segment)
            || machine.bus.ram.load_from_segment(segment)
            || machine
                .bus
                .extra_mem
                .iter_mut()
                .any(|m| m.load_from_segment(segment))
        {
            continue;
        }
        for (i, byte) in segment.data.iter().enumerate() {
            machine
                .bus
                .write_u8(segment.start_addr + i as u64, *byte)
                .unwrap();
        }
    }
    let sp_top = (chip.ram.base + chip.ram.size) as u32;
    machine.cpu.set_sp(sp_top & !0xF);
    machine.cpu.set_pc(bootloader.entry_point as u32);
    let rec = machine.bus.max_safe_tick_interval();
    machine.config.peripheral_tick_interval = rec;
    machine.bus.config.peripheral_tick_interval = rec;
    machine.config.idle_fast_forward_enabled = true;
    Node { machine, serial }
}

fn console(n: &Node) -> String {
    String::from_utf8_lossy(&n.serial.lock().unwrap()).into_owned()
}

#[test]
#[ignore]
fn probe() {
    let dir = root().join("../../fixtures/esp32c3-ble");
    let names: Vec<String> = std::env::var("PROBE_NODES")
        .unwrap_or_else(|_| "c3-ble-gatt-notify-flash,c3-ble-gatt-client-flash".into())
        .split(',')
        .map(|s| s.to_string())
        .collect();
    let cycles: u64 = std::env::var("PROBE_CYCLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400_000_000);
    let ble = BleAirBus::new();
    let mut nodes: Vec<Node> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let flash = std::fs::read(dir.join(format!("{n}.bin"))).unwrap();
            build_node(&flash, &ble, &format!("n{i}"))
        })
        .collect();
    // Time-synchronised lockstep: every node runs until its cycle count reaches
    // the common target, in quanta.
    let quantum: u64 = 1_600; // 10 us at 160 MHz
    let mut target = 0u64;
    while target < cycles {
        target += quantum;
        for n in nodes.iter_mut() {
            while n.machine.total_cycles < target {
                if let Err(e) = n.machine.step() {
                    eprintln!("halt: {e}");
                    break;
                }
            }
        }
        if target % 40_000_000 == 0 {
            eprintln!("[probe] {} Mcycles", target / 1_000_000);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        eprintln!("===== node {i} ({}) =====\n{}", names[i], console(n));
    }
    let air = ble.trace_snapshot();
    eprintln!("air frames in trace: {}", air.len());
    for f in air.iter().take(20) {
        eprintln!(
            "  src={} ch={} aa={:#x} pdu={:02x?}",
            f.source, f.channel, f.access_address, f.pdu
        );
    }
}
