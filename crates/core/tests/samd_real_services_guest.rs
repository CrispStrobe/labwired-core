// SPDX-License-Identifier: MIT
//! Prospective actual CODAL services, not whole native Arcade admission.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{
    bus::SystemBus, system::cortex_m::configure_cortex_m, Bus, Machine, SimulationObserver,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

const CORE: &str = "2ed19d64ec3359143409a4b0a9758b4aeef3ee7d";
const NAMES: &[&str] = &[
    "runtime_status",
    "runtime_checks",
    "runtime_heap_reused",
    "runtime_switches",
    "runtime_restores",
    "runtime_clock_irqs",
    "runtime_listener_calls",
    "runtime_listener_exception",
    "runtime_fiber_started",
    "runtime_fiber_woken",
    "runtime_main_woken",
    "runtime_main_identity",
    "runtime_worker_identity",
];
#[derive(Debug)]
struct Calls {
    swap: u32,
    restore: u32,
    swaps: AtomicU32,
    restores: AtomicU32,
}
impl SimulationObserver for Calls {
    fn on_step_start(&self, pc: u32, _opcode: u32) {
        if pc == self.swap {
            self.swaps.fetch_add(1, Ordering::Relaxed);
        }
        if pc == self.restore {
            self.restores.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn positive(v: &[u32], entries: u32, returns: u32, swaps: u32, restores: u32) {
    assert_eq!(v.len(), NAMES.len());
    assert_eq!(v[0..3], [0x600d, 3, 1]);
    assert!(swaps >= 3 && v[3] == swaps, "actual assembly switches");
    assert_eq!(v[4], restores);
    assert!(entries > 0 && entries == returns && v[5] == entries);
    assert_eq!(v[6..11], [1, 15, 1, 1, 1]);
    assert!(v[11] != 0 && v[12] != 0 && v[11] != v[12]);
}
#[test]
fn manufactured_missing_services_witness_refuses() {
    let good = [
        0x600d, 3, 1, 3, 0, 5, 1, 15, 1, 1, 1, 0x20001000, 0x20001100,
    ];
    positive(&good, 5, 5, 3, 0);
    for index in 0..good.len() {
        let mut bad = good;
        bad[index] = if index == 4 { 1 } else { 0 };
        assert!(std::panic::catch_unwind(|| positive(&bad, 5, 5, 3, 0)).is_err());
    }
    for (entries, returns, swaps, restores) in
        [(0, 5, 3, 0), (5, 4, 3, 0), (5, 5, 2, 0), (5, 5, 3, 1)]
    {
        assert!(
            std::panic::catch_unwind(|| positive(&good, entries, returns, swaps, restores))
                .is_err()
        );
    }
}

#[test]
#[ignore = "prospective real services; dedicated source-bound hosted execution required"]
fn actual_services_deliver_timer_event_to_real_waiting_fibers_and_refuse_closed_switch() {
    let directory = PathBuf::from(std::env::var("LABWIRED_SERVICES_FIXTURE_DIR").unwrap());
    let source = std::env::var("LABWIRED_SERVICES_SOURCE").unwrap();
    assert!(source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    for (mode, name) in [(0, "positive"), (1, "closed-switch")] {
        let folder = directory.join(name);
        let provenance: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("build-provenance.json")).unwrap())
                .unwrap();
        assert_eq!(provenance["source"].as_str(), Some(source.as_str()));
        assert_eq!(provenance["core_source"], CORE);
        assert_eq!(provenance["caller_mode"], mode);
        assert_eq!(provenance["cpu_hz"], 120000000);
        assert_eq!(provenance["runtime_admission"], false);
        let path = folder.join("control.elf");
        let bytes = std::fs::read(&path).unwrap();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(provenance["elf_sha256"].as_str(), Some(hash.as_str()));
        let map_hash = format!(
            "{:x}",
            Sha256::digest(std::fs::read(folder.join("control.map")).unwrap())
        );
        assert_eq!(provenance["map_sha256"].as_str(), Some(map_hash.as_str()));
        let elf = goblin::elf::Elf::parse(&bytes).unwrap();
        let symbol = |name| {
            let matches: Vec<_> = elf
                .syms
                .iter()
                .filter(|s| s.st_shndx != 0 && elf.strtab.get_at(s.st_name) == Some(name))
                .collect();
            assert_eq!(matches.len(), 1, "unique defined symbol {name}");
            matches[0].st_value
        };
        let addresses: Vec<_> = NAMES.iter().map(|name| symbol(name)).collect();
        let mut distinct = addresses.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), NAMES.len());
        let bss = symbol("_bss_start")..symbol("_bss_end");
        let data = symbol("_data_start")..symbol("_data_end");
        assert!(data.start >= 0x20000000 && data.end <= bss.start && bss.end < 0x2002e000);
        assert!(addresses.iter().all(|a| bss.contains(a) && a % 4 == 0));
        assert_eq!(symbol("runtime_vectors"), 0);
        assert_eq!(symbol("runtime_vectors_end"), 1024);
        let calls = Arc::new(Calls {
            swap: (symbol("swap_context") & !1) as u32,
            restore: (symbol("restore_register_context") & !1) as u32,
            swaps: AtomicU32::new(0),
            restores: AtomicU32::new(0),
        });
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let chip = ChipDescriptor::from_file(root.join("configs/chips/atsamd51.yaml")).unwrap();
        assert_eq!(chip.cpu_hz, 120000000);
        let manifest: SystemManifest = serde_yaml::from_str(
            "name: real-services\nchip: ../chips/atsamd51.yaml\nexternal_devices: []\n",
        )
        .unwrap();
        let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
        let (cpu, _) = configure_cortex_m(&mut bus);
        let mut machine = Machine::new(cpu, bus);
        machine.add_observer(calls.clone());
        machine
            .load_firmware(&labwired_loader::load_elf(&path).unwrap())
            .unwrap();
        assert_eq!(machine.bus.read_u32(0).unwrap(), 0x20030000);
        for vector in 1..256 {
            let word = machine.bus.read_u32(vector * 4).unwrap();
            let target = match vector {
                1 => "reset_entry",
                15 => "runtime_systick",
                _ => "runtime_fault",
            };
            assert_eq!(word & 1, 1);
            assert_eq!(u64::from(word & !1), symbol(target) & !1);
        }
        let load = symbol("_data_load");
        let expected_data: Vec<_> = (0..data.end - data.start)
            .step_by(4)
            .map(|offset| machine.bus.read_u32(load + offset).unwrap())
            .collect();
        for address in bss.clone().step_by(4) {
            machine.bus.write_u32(address, 0xa5a5a5a5).unwrap();
        }
        for address in data.clone().step_by(4) {
            machine.bus.write_u32(address, 0x5a5a5a5a).unwrap();
        }
        let entry = symbol("runtime_probe") & !1;
        let mut startup = false;
        let mut previous_exception = 0;
        let mut entries = 0;
        let mut returns = 0;
        let mut final_step = None;
        for step in 1..=4_000_000 {
            machine.step().expect("real services guest must not fault");
            if u64::from(machine.cpu.pc & !1) == entry && !startup {
                for address in bss.clone().step_by(4) {
                    assert_eq!(machine.bus.read_u32(address).unwrap(), 0);
                }
                for (address, expected) in data.clone().step_by(4).zip(&expected_data) {
                    assert_eq!(machine.bus.read_u32(address).unwrap(), *expected);
                }
                assert!(machine.cpu.primask && machine.cpu.active_exception == 0);
                startup = true;
            }
            let exception = machine.cpu.active_exception;
            assert!(
                [0, 15].contains(&exception),
                "unexpected architectural exception"
            );
            if exception == 15 && previous_exception == 0 {
                entries += 1;
            }
            if exception == 0 && previous_exception == 15 {
                returns += 1;
            }
            previous_exception = exception;
            if !startup {
                continue;
            }
            let v: Vec<_> = addresses
                .iter()
                .map(|a| machine.bus.read_u32(*a).unwrap())
                .collect();
            let swaps = calls.swaps.load(Ordering::Relaxed);
            let restores = calls.restores.load(Ordering::Relaxed);
            if mode == 0 {
                assert!(
                    [0, 0x100, 0x600d].contains(&v[0]),
                    "explicit services failure {:#x}",
                    v[0]
                );
                if v[0] != 0x600d {
                    continue;
                }
                positive(&v, entries, returns, swaps, restores);
            } else {
                assert!(
                    [0, 0x100, 0xe0000d10].contains(&v[0]),
                    "wrong refusal {:#x}",
                    v[0]
                );
                if v[0] != 0xe0000d10 {
                    continue;
                }
                assert_eq!(&v[0..5], &[0xe0000d10, 2, 1, 0, 0]);
                assert_eq!(&v[6..11], &[0, 0, 0, 0, 0]);
                assert_eq!(swaps, 0);
                assert_eq!(restores, 0);
                assert!(v[11] != 0 && v[12] == 0);
            }
            assert!(machine.cpu.primask && exception == 0);
            assert_eq!(machine.bus.read_u32(0xe000ed28).unwrap(), 0);
            assert_eq!(machine.bus.read_u32(0xe000ed2c).unwrap(), 0);
            println!("SAMD_REAL_SERVICES mode={name} source={source} core={CORE} elf_sha256={hash} steps={step} bss_clear=true data_copy=true vectors=true witnesses={v:?} irq_entries={entries} irq_returns={returns} actual_swaps={swaps} actual_restores={restores} final_mask=1 final_exception=0");
            final_step = Some(step);
            break;
        }
        assert!(
            startup && final_step.is_some(),
            "timeout is not real service qualification"
        );
    }
}
