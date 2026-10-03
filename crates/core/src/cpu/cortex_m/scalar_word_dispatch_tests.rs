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
    assert_eq!(actual.it_state, reference.it_state);
    assert_eq!(actual.it_state_restored, reference.it_state_restored);
    assert_eq!(actual.sleeping, reference.sleeping);
    assert_eq!(actual.active_exception, reference.active_exception);
    assert_eq!(actual.msp, reference.msp);
    assert_eq!(actual.psp, reference.psp);
    assert_eq!(actual.control, reference.control);
    assert_eq!(actual.basepri, reference.basepri);
    assert_eq!(actual.faultmask, reference.faultmask);
    assert_eq!(actual.event_register, reference.event_register);
    assert_eq!(actual.pending_div0, reference.pending_div0);
    assert_eq!(actual.live_fault_regs(), reference.live_fault_regs());
    assert_eq!(actual.fault_entry, reference.fault_entry);
    assert_eq!(actual.fault_entry_regs, reference.fault_entry_regs);
    assert_eq!(actual.lockup, reference.lockup);
    assert_eq!(actual.fault_capture(), reference.fault_capture());
    assert_eq!(actual.debug_halted(), reference.debug_halted());
    assert_eq!(actual.firmware_exit, reference.firmware_exit);
    assert_eq!(
        actual.vectactive.load(Ordering::Relaxed),
        reference.vectactive.load(Ordering::Relaxed)
    );
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
struct Counter {
    starts: AtomicU32,
    ends: AtomicU32,
    retirement: std::sync::Mutex<Vec<(u32, Vec<u32>)>>,
}
impl SimulationObserver for Counter {
    fn on_step_start(&self, _pc: u32, _opcode: u32) {
        self.starts.fetch_add(1, Ordering::Relaxed);
    }
    fn on_step_end(&self, cycles: u32, registers: &[u32]) {
        self.ends.fetch_add(1, Ordering::Relaxed);
        self.retirement
            .lock()
            .unwrap()
            .push((cycles, registers.to_vec()));
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
            counter_a.starts.load(Ordering::Relaxed),
            counter_r.starts.load(Ordering::Relaxed)
        );
        assert_eq!(
            counter_a.ends.load(Ordering::Relaxed),
            counter_r.ends.load(Ordering::Relaxed)
        );
        assert_eq!(
            *counter_a.retirement.lock().unwrap(),
            *counter_r.retirement.lock().unwrap()
        );
        if case == 2 {
            assert_eq!(counter_a.starts.load(Ordering::Relaxed), 1);
            assert_eq!(counter_a.ends.load(Ordering::Relaxed), 1);
            let retirement = counter_a.retirement.lock().unwrap();
            assert_eq!(retirement[0].0, 1);
            assert_eq!(retirement[0].1[18], 0x102);
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

fn wired_fixture(opcode: u16, enabled: bool) -> (CortexM, SystemBus) {
    let (_, mut bus) = fixture(opcode, enabled);
    let (mut cpu, _) = crate::system::cortex_m::configure_cortex_m(&mut bus);
    cpu.scalar_word_dispatch_enabled = enabled;
    cpu.set_faults_enabled(true);
    cpu.pc = 0x100;
    cpu.sp = 0x20001000;
    cpu.msp = cpu.sp;
    cpu.r0 = 1;
    cpu.r1 = 0x20000100;
    cache(&mut cpu, opcode);
    (cpu, bus)
}

#[test]
fn scalar_word_dispatch_wired_fault_escalation_and_lockup_match_original() {
    for opcode in [0x6008, 0x6808] {
        for case in 0..4 {
            let (mut actual, mut bus) = wired_fixture(opcode, true);
            let (mut reference, mut reference_bus) = wired_fixture(opcode, false);
            for (cpu, b) in [
                (&mut actual, &mut bus),
                (&mut reference, &mut reference_bus),
            ] {
                cpu.r1 = 0xdead0000;
                if case == 0 {
                    b.write_u32(0xe000ed24, SHCSR_BUSFAULTENA).unwrap();
                } else if case == 2 {
                    cpu.set_active_exception(3);
                } else if case == 3 {
                    cpu.set_faults_enabled(false);
                }
                assert!(b.flash.write_u32(3 * 4, 0x201));
                assert!(b.flash.write_u32(5 * 4, 0x201));
            }
            let config = bus.config.clone();
            let a = actual.step_internal(&mut bus, &[], &config);
            let r = reference.step_internal(&mut reference_bus, &[], &config);
            assert_eq!(format!("{a:?}"), format!("{r:?}"));
            assert_eq!(a.is_ok(), case < 2);
            same(&actual, &reference, &bus, &reference_bus);
            assert_eq!(actual.pc, 0x100);
            assert_eq!(actual.scalar_word_dispatch_count, 0);
            if case < 3 {
                let faults = actual.live_fault_regs().unwrap();
                assert_eq!(faults.bfar, 0xdead0000);
                assert_eq!(faults.cfsr, CFSR_BFSR_PRECISERR | CFSR_BFSR_BFARVALID);
                assert_eq!(faults.hfsr, if case == 0 { 0 } else { HFSR_FORCED });
            }
            if case < 2 {
                let target = if case == 0 { 5 } else { 3 };
                assert_ne!(actual.pending_exceptions[0] & (1 << target), 0);
                actual.step_internal(&mut bus, &[], &config).unwrap();
                reference
                    .step_internal(&mut reference_bus, &[], &config)
                    .unwrap();
                same(&actual, &reference, &bus, &reference_bus);
                assert_eq!(actual.active_exception, target);
                assert_eq!(actual.pc, 0x200);
                assert_eq!(actual.fault_entry.unwrap().frame.pc, 0x100);
            } else if case == 2 {
                assert_eq!(actual.lockup.unwrap().pc, 0x100);
            } else {
                assert!(actual.fault_capture().is_none());
            }
        }
    }
}

#[test]
fn scalar_word_dispatch_wired_reset_ends_batch_on_aircr_store() {
    for batched in [false, true] {
        let (mut actual, mut bus) = wired_fixture(0x6008, true);
        let (mut reference, mut reference_bus) = wired_fixture(0x6008, false);
        for (cpu, b) in [
            (&mut actual, &mut bus),
            (&mut reference, &mut reference_bus),
        ] {
            cpu.r0 = 0x05fa0004;
            cpu.r1 = 0xe000ed0c;
            cpu.r2 = 0x48000014;
            assert!(b.flash.write_u16(0x102, 0x6010));
        }
        let mut config = bus.config.clone();
        config.batch_mode_enabled = batched;
        config.peripheral_tick_interval = 8;
        assert_eq!(actual.step_batch(&mut bus, &[], &config, 2).unwrap(), 1);
        assert_eq!(
            reference
                .step_batch(&mut reference_bus, &[], &config, 2)
                .unwrap(),
            1
        );
        same(&actual, &reference, &bus, &reference_bus);
        assert!(actual.sysreset_latched());
        assert_eq!(actual.pc, 0x102);
        assert_eq!(actual.scalar_word_dispatch_count, 1);
        assert_eq!(bus.read_u32(0x48000014).unwrap(), 0);
    }
}

#[cfg(feature = "event-scheduler")]
#[derive(Debug)]
struct WriteClock {
    clock: crate::CycleClock,
    events: std::sync::Mutex<Vec<(u64, u64, u8, u8)>>,
}
#[cfg(feature = "event-scheduler")]
impl SimulationObserver for WriteClock {
    fn on_memory_write(&self, addr: u64, old: u8, new: u8) {
        self.events
            .lock()
            .unwrap()
            .push((self.clock.now(), addr, old, new));
    }
}

#[cfg(feature = "event-scheduler")]
#[test]
fn scalar_word_dispatch_batch_gpio_bus_observers_see_live_cycles() {
    for batched in [false, true] {
        let (mut actual, mut bus) = fixture(0x6008, true);
        let (mut reference, mut reference_bus) = fixture(0x6008, false);
        for (cpu, b) in [
            (&mut actual, &mut bus),
            (&mut reference, &mut reference_bus),
        ] {
            cpu.r1 = 0x48000014;
            assert!(b.flash.write_u16(0x102, 0x680a)); // LDR r2,[r1]
            assert!(b.flash.write_u16(0x104, 0x6008)); // STR r0,[r1]
        }
        let observer_a = Arc::new(WriteClock {
            clock: bus.cycle_clock.clone(),
            events: Default::default(),
        });
        let observer_r = Arc::new(WriteClock {
            clock: reference_bus.cycle_clock.clone(),
            events: Default::default(),
        });
        bus.observers.push(observer_a.clone());
        reference_bus.observers.push(observer_r.clone());
        let mut config = bus.config.clone();
        config.batch_mode_enabled = batched;
        config.peripheral_tick_interval = 8;
        assert_eq!(actual.step_batch(&mut bus, &[], &config, 3).unwrap(), 3);
        assert_eq!(
            reference
                .step_batch(&mut reference_bus, &[], &config, 3)
                .unwrap(),
            3
        );
        same(&actual, &reference, &bus, &reference_bus);
        assert_eq!(actual.scalar_word_dispatch_count, 3);
        assert_eq!(actual.r2, 1);
        assert_eq!(bus.current_cycle, 40);
        let events = observer_a.events.lock().unwrap();
        assert_eq!(*events, *observer_r.events.lock().unwrap());
        assert_eq!(events.len(), 8);
        for (i, &(cycle, addr, old, new)) in events.iter().enumerate() {
            assert_eq!(cycle, if i < 4 { 37 } else { 39 });
            assert_eq!(addr, 0x48000014 + (i % 4) as u64);
            assert_eq!(old, 0);
            assert_eq!(new, u8::from(i % 4 == 0));
        }
    }
}
