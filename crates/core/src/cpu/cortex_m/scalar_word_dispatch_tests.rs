// SPDX-License-Identifier: MIT
use super::*;
use crate::peripherals::gpio::{GpioPort, GpioRegisterLayout};

fn fixture(opcode: u16, enabled: bool) -> (CortexM, SystemBus) {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.scalar_word_dispatch_enabled = enabled;
    cpu.pc = 0x100;
    cpu.sp = 0x20001000;
    cpu.r0 = 1;
    cpu.r1 = 0x20000100;
    cache(&mut cpu, opcode);
    assert!(bus.flash.write_u16(0x100, opcode));
    bus.add_peripheral(
        "GPIOA",
        0x48000000,
        0x400,
        None,
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::Stm32V2)),
    );
    bus.current_cycle = 37;
    (cpu, bus)
}
fn cache(cpu: &mut CortexM, opcode: u16) {
    cpu.decode_cache[0x80] = Some(DecodeCacheEntry {
        tag: 0x100,
        instruction: decode_thumb_16(opcode),
        opcode: u32::from(opcode),
        pc_increment: 2,
        cycles: 1,
    });
}
fn same(actual: &CortexM, reference: &CortexM, bus: &SystemBus, reference_bus: &SystemBus) {
    assert_eq!(
        serde_json::to_value(actual.snapshot()).unwrap(),
        serde_json::to_value(reference.snapshot()).unwrap()
    );
    assert_eq!(bus.ram.data, reference_bus.ram.data);
    assert_eq!(bus.access_counts(), reference_bus.access_counts());
    assert_eq!(bus.current_cycle, reference_bus.current_cycle);
    assert_eq!(actual.pending_data_fault, reference.pending_data_fault);
    assert_eq!(
        actual.pending_undef_instruction,
        reference.pending_undef_instruction
    );
    assert_eq!(actual.sysreset_latched(), reference.sysreset_latched());
}

#[test]
fn scalar_word_dispatch_exhaustive_halfwords_match_original_scalar_execution() {
    let (mut actual, mut bus) = fixture(0xbf00, true);
    let (mut reference, mut reference_bus) = fixture(0xbf00, false);
    let config = bus.config.clone();
    for opcode in 0..=u16::MAX {
        for cpu in [&mut actual, &mut reference] {
            cpu.pc = 0x100;
            cpu.xpsr = 0xa1000000;
            for reg in 0..8 {
                cpu.write_reg(reg, 0x20000100 + u32::from(reg) * 4);
            }
            cache(cpu, opcode);
        }
        let selected = matches!(opcode & 0xf800, 0x6000 | 0x6800);
        let before = actual.scalar_word_dispatch_count;
        if selected {
            actual.step_internal(&mut bus, &[], &config).unwrap();
            reference
                .step_internal(&mut reference_bus, &[], &config)
                .unwrap();
            assert_eq!(
                actual.scalar_word_dispatch_count,
                before + 1,
                "opcode={opcode:x}"
            );
            same(&actual, &reference, &bus, &reference_bus);
        } else {
            let snapshot = serde_json::to_value(actual.snapshot()).unwrap();
            let counts = bus.access_counts();
            assert!(!actual
                .try_exec_scalar_word(&mut bus, decode_thumb_16(opcode), 2)
                .unwrap());
            assert_eq!(actual.scalar_word_dispatch_count, before);
            assert_eq!(serde_json::to_value(actual.snapshot()).unwrap(), snapshot);
            assert_eq!(bus.access_counts(), counts);
        }
    }
    assert_eq!(actual.scalar_word_dispatch_count, 4096);
    assert_eq!(reference.scalar_word_dispatch_count, 0);
}

#[test]
fn scalar_word_dispatch_ram_mmio_bounds_and_errors_match_original_path() {
    for opcode in [0x6008, 0x6808] {
        for addr in [0x20000100, 0x20000101, 0x48000014, 0x48000018, u32::MAX] {
            for cached in [false, true] {
                let (mut actual, mut bus) = fixture(opcode, true);
                let (mut reference, mut reference_bus) = fixture(opcode, false);
                for cpu in [&mut actual, &mut reference] {
                    cpu.r1 = addr;
                }
                for b in [&mut bus, &mut reference_bus] {
                    b.write_u32(0x20000100, 0xa5a55a5a).unwrap();
                }
                let mut config = bus.config.clone();
                config.decode_cache_enabled = cached;
                let a = actual.step_internal(&mut bus, &[], &config);
                let r = reference.step_internal(&mut reference_bus, &[], &config);
                assert_eq!(
                    format!("{a:?}"),
                    format!("{r:?}"),
                    "opcode={opcode:x} addr={addr:x}"
                );
                same(&actual, &reference, &bus, &reference_bus);
                for address in [0x48000014, 0x48000018] {
                    assert_eq!(
                        bus.read_u32(address).unwrap(),
                        reference_bus.read_u32(address).unwrap()
                    );
                }
                assert_eq!(bus.current_cycle, 37);
            }
        }
    }
}

#[derive(Debug, Default)]
struct Counter(AtomicU32);
impl SimulationObserver for Counter {
    fn on_step_start(&self, _pc: u32, _opcode: u32) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn scalar_word_dispatch_it_observers_debug_and_wfe_keep_canonical_guards() {
    for case in 0..5 {
        let (mut actual, mut bus) = fixture(0x6008, true);
        let (mut reference, mut reference_bus) = fixture(0x6008, false);
        for cpu in [&mut actual, &mut reference] {
            match case {
                0 | 1 => {
                    cpu.it_state = 0x08;
                    cpu.xpsr = if case == 0 { 0x41000000 } else { 0x01000000 };
                }
                3 => {
                    let halt = crate::peripherals::scs_debug::DebugHaltState::new();
                    halt.write_dhcsr(0xa05f0003);
                    cpu.set_debug_halt(halt);
                }
                4 => cpu.waiting_for_event = true,
                _ => {}
            }
        }
        let counter_a = Arc::new(Counter::default());
        let counter_r = Arc::new(Counter::default());
        let oa: Vec<Arc<dyn SimulationObserver>> = if case == 2 {
            vec![counter_a.clone()]
        } else {
            vec![]
        };
        let or: Vec<Arc<dyn SimulationObserver>> = if case == 2 {
            vec![counter_r.clone()]
        } else {
            vec![]
        };
        let config = bus.config.clone();
        actual.step_internal(&mut bus, &oa, &config).unwrap();
        reference
            .step_internal(&mut reference_bus, &or, &config)
            .unwrap();
        same(&actual, &reference, &bus, &reference_bus);
        assert_eq!(actual.scalar_word_dispatch_count, 0, "case={case}");
        assert_eq!(
            counter_a.0.load(Ordering::Relaxed),
            counter_r.0.load(Ordering::Relaxed)
        );
        if case == 2 {
            assert_eq!(counter_a.0.load(Ordering::Relaxed), 1);
        }
    }
}

#[test]
fn scalar_word_dispatch_wide_entries_are_read_only_refusals() {
    let (mut cpu, mut bus) = fixture(0x6008, true);
    let snapshot = serde_json::to_value(cpu.snapshot()).unwrap();
    let counts = bus.access_counts();
    for width in [0, 1, 4, u32::MAX] {
        assert!(!cpu
            .try_exec_scalar_word(&mut bus, decode_thumb_16(0x6008), width)
            .unwrap());
        assert_eq!(serde_json::to_value(cpu.snapshot()).unwrap(), snapshot);
        assert_eq!(bus.access_counts(), counts);
        assert_eq!(cpu.scalar_word_dispatch_count, 0);
    }
}
