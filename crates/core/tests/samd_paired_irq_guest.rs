// SPDX-License-Identifier: MIT
//! Prospective authored paired-ABI controls, not real CODAL callers or fibers.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{bus::SystemBus, system::cortex_m::configure_cortex_m, Bus, Machine};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

#[test]
#[ignore = "prospective hosted controls; qualified source and dedicated execution required"]
fn authored_paired_irq_delivery_and_four_terminal_refusals() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = PathBuf::from(std::env::var("LABWIRED_PAIRED_FIXTURE_DIR").unwrap());
    for (mode, expected_status, expected_fault) in [
        (0, 0x600d, 0),
        (1, 0x201, 3),
        (2, 0x202, 5),
        (3, 0x203, 4),
        (4, 0x204, 6),
    ] {
        let path = directory.join(mode.to_string()).join("control.elf");
        let bytes = std::fs::read(&path).unwrap();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let elf = goblin::elf::Elf::parse(&bytes).expect("owned unstripped ELF");
        let symbol = |name| {
            let matches: Vec<_> = elf
                .syms
                .iter()
                .filter(|sym| sym.st_shndx != 0 && elf.strtab.get_at(sym.st_name) == Some(name))
                .collect();
            assert_eq!(matches.len(), 1, "unique defined symbol {name}");
            assert!(matches[0].st_value <= u32::MAX as u64);
            matches[0].st_value
        };
        let status = symbol("control_status");
        let fault = symbol("bw_simulation_irq_fault");
        let witnesses = [
            "irq_count",
            "irq_stage_seen",
            "irq_mask_seen",
            "irq_exception_seen",
        ]
        .map(symbol);
        let mut slots: Vec<_> = [status, fault].into_iter().chain(witnesses).collect();
        for address in &slots {
            assert!((0x2000_0000..0x2003_0000).contains(address));
            assert_eq!(address % 4, 0);
        }
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), 6, "distinct status/fault/handler witnesses");
        let identity = symbol("fixture_identity");
        assert!((0x2000_0000..0x2003_0000).contains(&identity));
        assert_eq!(identity % 4, 0);
        assert!(!slots.contains(&identity));
        // Exact compiled refusal function extent, not guessed instruction bytes.
        let apply: Vec<_> = elf
            .syms
            .iter()
            .filter(|sym| {
                sym.st_shndx != 0
                    && sym.st_type() == goblin::elf::sym::STT_FUNC
                    && elf
                        .strtab
                        .get_at(sym.st_name)
                        .is_some_and(|name| name.starts_with("_ZL5apply"))
            })
            .collect();
        assert_eq!(apply.len(), 1, "unique compiled terminal apply function");
        let apply_start = apply[0].st_value & !1;
        let apply_end = apply_start.checked_add(apply[0].st_size).unwrap();
        assert!(apply_end > apply_start && apply_end <= 0x10_0000);
        assert_eq!(symbol("authored_vectors"), 0);
        assert_eq!(symbol("authored_vectors_end"), 1024);
        let handlers = [
            symbol("reset_entry"),
            symbol("authored_irq0"),
            symbol("unexpected_exception"),
        ];
        let chip = ChipDescriptor::from_file(root.join("configs/chips/atsamd51.yaml")).unwrap();
        let manifest: SystemManifest = serde_yaml::from_str(
            "name: authored-paired-irq\nchip: ../chips/atsamd51.yaml\nexternal_devices: []\n",
        )
        .unwrap();
        let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
        let (cpu, _) = configure_cortex_m(&mut bus);
        let mut machine = Machine::new(cpu, bus);
        machine
            .load_firmware(&labwired_loader::load_elf(&path).unwrap())
            .unwrap();
        assert_eq!(machine.bus.read_u32(0).unwrap(), 0x2003_0000);
        for vector in 1..256 {
            let word = machine.bus.read_u32(vector * 4).unwrap();
            if [7, 8, 9, 10, 13].contains(&vector) {
                assert_eq!(word, 0);
            } else {
                let target = match vector {
                    1 => handlers[0],
                    16 => handlers[1],
                    _ => handlers[2],
                };
                assert_eq!(word & 1, 1);
                assert_eq!(u64::from(word & !1), target & !1);
            }
        }
        for address in &slots {
            machine.bus.write_u32(*address, 0xa5a5_a5a5).unwrap();
        }
        // Poison only authored .data/BSS, never NVIC state or guest code.
        machine.bus.write_u32(identity, 0xa5a5_a5a5).unwrap();
        let mut saw_zero = false;
        let mut saw_entry = false;
        let mut irq_entries = 0;
        let mut previous_exception = 0;
        let mut saw_return = false;
        let mut reached = false;
        let mut reached_step = 0;
        for step in 1..=100_000 {
            machine.step().expect("guest instruction must not fault");
            let observed = machine.bus.read_u32(status).unwrap();
            let observed_fault = machine.bus.read_u32(fault).unwrap();
            let values = witnesses.map(|address| machine.bus.read_u32(address).unwrap());
            saw_zero |= observed == 0 && observed_fault == 0 && values == [0; 4];
            if observed == 0x100 && !saw_entry {
                assert_eq!(
                    machine.bus.read_u32(identity).unwrap(),
                    1,
                    "startup .data copied"
                );
                saw_entry = true;
            }
            if machine.cpu.active_exception == 16 && previous_exception != 16 {
                irq_entries += 1;
            }
            saw_return |= previous_exception == 16 && machine.cpu.active_exception == 0;
            previous_exception = machine.cpu.active_exception;
            assert!(
                ![0xbad, 0xf00d].contains(&observed),
                "forbidden guest result"
            );
            if ![0, 0x100, 0xa5a5_a5a5].contains(&observed) {
                assert_eq!(observed, expected_status, "exact result for mode {mode}");
                // A refusal status is written BEFORE the terminal operation.
                // It is not acceptance until the actual fault has been latched.
                if mode != 0 && observed_fault == 0 {
                    continue;
                }
                assert_eq!(observed_fault, expected_fault);
                assert!(machine.cpu.primask && machine.cpu.active_exception == 0);
                assert_eq!(values, if mode == 0 { [1, 2, 0, 16] } else { [0; 4] });
                assert_eq!(irq_entries, if mode == 0 { 1 } else { 0 });
                if mode == 0 {
                    assert!(saw_return);
                } else {
                    let mut pcs = BTreeSet::new();
                    for _ in 0..64 {
                        machine
                            .step()
                            .expect("terminal refusal must continue executing");
                        assert_eq!(machine.bus.read_u32(status).unwrap(), expected_status);
                        assert_eq!(machine.bus.read_u32(fault).unwrap(), expected_fault);
                        assert_eq!(
                            witnesses.map(|address| machine.bus.read_u32(address).unwrap()),
                            [0; 4]
                        );
                        assert!(machine.cpu.primask && machine.cpu.active_exception == 0);
                        let pc = u64::from(machine.cpu.pc & !1);
                        assert!(
                            (apply_start..apply_end).contains(&pc),
                            "refusal cannot return to caller"
                        );
                        pcs.insert(pc);
                    }
                    assert!(pcs.len() >= 2, "executed loop, not a stopped CPU");
                }
                reached = true;
                reached_step = step;
                break;
            }
        }
        assert!(
            saw_zero && saw_entry && reached,
            "timeout is not successful refusal"
        );
        println!("SAMD_PAIRED_IRQ mode={mode} elf_sha256={hash} status={expected_status:#x} fault={expected_fault} steps={reached_step} bss_clear=true data_copy=true entry=true vectors=true irq_entries={irq_entries} refusal_steps={}", if mode == 0 { 0 } else { 64 });
    }
}
