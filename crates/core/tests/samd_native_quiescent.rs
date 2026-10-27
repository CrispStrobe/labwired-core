// SPDX-License-Identifier: MIT
//! Authored instruction controls for inactive RAM-only chunks. These are not
//! the independently source-built PXT/native board qualification.
// Cargo requires event-scheduler for this target. Do not turn a feature-off
// invocation into an empty, successful test binary with a crate-level cfg.
use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::{
    bus::SystemBus,
    cpu::cortex_m::CortexM,
    memory::{ProgramImage, Segment},
    system::cortex_m::configure_cortex_m,
    AdvanceRequest, Arch, Bus, Cpu, Machine, SimulationObserver,
};
use std::{num::NonZeroU32, sync::Arc};
type M = Machine<CortexM>;
fn machine(code: &[u16], literals: &[(usize, u32)], attached: bool) -> M {
    let chip: ChipDescriptor = serde_yaml::from_str(include_str!(
        "../../../configs/chips/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    let mut manifest: SystemManifest = serde_yaml::from_str(include_str!(
        "../../../configs/systems/samd51-no-asf-simulation.yaml"
    ))
    .unwrap();
    if !attached {
        manifest.external_devices.clear();
    }
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    let mut raw = vec![0; 0x300];
    raw[..4].copy_from_slice(&0x20030000u32.to_le_bytes());
    raw[4..8].copy_from_slice(&0x101u32.to_le_bytes());
    for (i, instruction) in code.iter().enumerate() {
        raw[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&instruction.to_le_bytes());
    }
    for &(address, value) in literals {
        raw[address..address + 4].copy_from_slice(&value.to_le_bytes());
    }
    // Valid handlers for authored pending-IRQ/SysTick controls.
    raw[15 * 4..16 * 4].copy_from_slice(&0x201u32.to_le_bytes());
    raw[16 * 4..17 * 4].copy_from_slice(&0x201u32.to_le_bytes());
    raw[0x200..0x202].copy_from_slice(&0xe7feu16.to_le_bytes());
    let mut image = ProgramImage::new(0, Arch::Arm);
    image.segments.push(Segment {
        start_addr: 0,
        data: raw,
    });
    m.load_firmware(&image).unwrap();
    m
}
fn spin(attached: bool) -> M {
    // LDR r1, literal; MOVS r0,0; loop: ADDS r0,1; STR r0,[r1]; B loop.
    machine(
        &[0x4903, 0x2000, 0x3001, 0x6008, 0xe7fc, 0xbf00],
        &[(0x110, 0x20000040)],
        attached,
    )
}
fn run<C: Cpu>(m: &mut Machine<C>, fuel: u64, strict: bool) {
    let mut request = AdvanceRequest::run(Some(fuel));
    if strict {
        request = request.with_batch_cap(NonZeroU32::new(1).unwrap());
    }
    let report = m.advance(request).unwrap();
    assert_eq!(report.fuel_consumed, fuel);
    assert_eq!(report.primary_steps, fuel);
    assert_eq!(
        m.config.peripheral_tick_interval, 1,
        "configured grid is never widened"
    );
    assert_eq!(
        m.bus.config.peripheral_tick_interval, 1,
        "bus grid is never widened"
    );
}
fn equal_state<C: Cpu, D: Cpu>(actual: &Machine<C>, strict: &Machine<D>) {
    assert_eq!(
        serde_json::to_value(actual.cpu.snapshot()).unwrap(),
        serde_json::to_value(strict.cpu.snapshot()).unwrap(),
        "published CPU register/flag/IRQ snapshot"
    );
    assert_eq!(actual.bus.ram.data, strict.bus.ram.data, "all SRAM bytes");
    assert_eq!(
        actual.bus.access_counts(),
        strict.bus.access_counts(),
        "architectural memory/peripheral accounting"
    );
    assert_eq!(
        actual.total_cycles, strict.total_cycles,
        "exact elapsed cycles"
    );
    assert_eq!(
        actual.bus.current_cycle, strict.bus.current_cycle,
        "published bus cycle"
    );
    let snapshots = |bus: &SystemBus| {
        bus.peripherals
            .iter()
            .map(|p| (p.name.clone(), p.dev.snapshot()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        snapshots(&actual.bus),
        snapshots(&strict.bus),
        "every actual model snapshot"
    );
}
#[test]
fn host_armed_systick_is_enqueued_before_a_warm_chunk() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    run(&mut actual, 257, false);
    run(&mut strict, 257, true);
    assert!(
        actual.step_profile().cpu_batches * 4 < actual.step_profile().cpu_instructions,
        "the loop must be warm and genuinely coalescing before host arming"
    );
    for m in [&mut actual, &mut strict] {
        m.bus.write_u32(0xe000e014, 3).unwrap();
        m.bus.write_u32(0xe000e018, 0).unwrap();
        m.bus.write_u32(0xe000e010, 7).unwrap();
        assert!(!m.bus.pending_schedule.is_empty(), "actual pre-heap event");
    }
    // A short first request would itself flush the pending event and make an
    // omitted guard invisible. Start with a real wide eligible window.
    for fuel in [1031, 1, 2, 17] {
        run(&mut actual, fuel, false);
        run(&mut strict, fuel, true);
        equal_state(&actual, &strict);
    }
    assert_eq!(
        actual.cpu.active_exception, 15,
        "actual SysTick handler taken"
    );
    assert_eq!(strict.cpu.active_exception, 15);
}
#[test]
fn enabled_pending_nvic_interrupt_is_taken_before_a_warm_chunk() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    run(&mut actual, 257, false);
    run(&mut strict, 257, true);
    for m in [&mut actual, &mut strict] {
        m.bus.write_u32(0xe000e100, 1).unwrap();
        m.bus.write_u32(0xe000e200, 1).unwrap();
    }
    run(&mut actual, 1031, false);
    run(&mut strict, 1031, true);
    equal_state(&actual, &strict);
    assert_eq!(actual.cpu.active_exception, 16, "enabled IRQ0 really taken");
    assert_eq!(strict.cpu.active_exception, 16);
}
#[test]
fn wfi_sleeps_and_enabled_irq_really_wakes_the_cpu() {
    let mut actual = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
    let mut strict = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
    for m in [&mut actual, &mut strict] {
        m.config.idle_fast_forward_enabled = true;
    }
    run(&mut actual, 1, false);
    run(&mut strict, 1, true);
    equal_state(&actual, &strict);
    assert_eq!(actual.cpu.pc, 0x102);
    for (m, strict_mode) in [(&mut actual, false), (&mut strict, true)] {
        let mut request = AdvanceRequest::run(Some(37));
        if strict_mode {
            request = request.with_batch_cap(NonZeroU32::new(1).unwrap());
        }
        let report = m.advance(request).unwrap();
        assert_eq!(report.fuel_consumed, 37);
        assert_eq!(
            report.primary_steps, 0,
            "sleep retires no instructions: strict={strict_mode}"
        );
        assert_eq!(report.idle_cycles, 37, "actual idle accounting");
    }
    equal_state(&actual, &strict);
    assert_eq!(actual.cpu.r0, 0, "MOVS after WFI has not executed");
    assert_eq!(actual.cpu.pc, 0x102);
    for m in [&mut actual, &mut strict] {
        m.bus.write_u32(0xe000e100, 1).unwrap();
        m.bus.write_u32(0xe000e200, 1).unwrap();
    }
    run(&mut actual, 17, false);
    run(&mut strict, 17, true);
    equal_state(&actual, &strict);
    assert_eq!(
        actual.cpu.active_exception, 16,
        "actual enabled IRQ wakes WFI"
    );
    assert_eq!(strict.cpu.active_exception, 16);
}
#[test]
fn shared_nvic_sleep_wakes_preserve_wfi_wfe_mask_contracts() {
    for (opcode, primask, basepri, faultmask, wakes) in [
        (0xbf30, false, 0, false, true),
        (0xbf20, false, 0, false, true),
        (0xbf30, true, 0, false, true),
        (0xbf20, true, 0, false, false),
        (0xbf30, false, 0x40, false, false),
        (0xbf20, false, 0x40, false, false),
        (0xbf30, false, 0, true, false),
        (0xbf20, false, 0, true, false),
    ] {
        let mut actual = machine(&[opcode, 0x205a, 0xe7fe], &[], false);
        let mut strict = machine(&[opcode, 0x205a, 0xe7fe], &[], false);
        for m in [&mut actual, &mut strict] {
            m.config.idle_fast_forward_enabled = true;
            m.cpu.primask = primask;
            m.cpu.basepri = basepri;
            m.cpu.faultmask = faultmask;
        }
        run(&mut actual, 1, false);
        run(&mut strict, 1, true);
        equal_state(&actual, &strict);
        for m in [&mut actual, &mut strict] {
            // IRQ0 priority0x80 is blocked by BASEPRI0x40, not by BASEPRI0.
            m.bus.write_u32(0xe000e400, 0x80).unwrap();
            m.bus.write_u32(0xe000e100, 1).unwrap();
            m.bus.write_u32(0xe000e200, 1).unwrap();
        }
        if wakes {
            run(&mut actual, 17, false);
            run(&mut strict, 17, true);
            assert_eq!(actual.cpu.active_exception, if primask { 0 } else { 16 });
        } else {
            for (m, strict_mode) in [(&mut actual, false), (&mut strict, true)] {
                let mut request = AdvanceRequest::run(Some(37));
                if strict_mode {
                    request = request.with_batch_cap(NonZeroU32::new(1).unwrap());
                }
                let report = m.advance(request).unwrap();
                assert_eq!(report.fuel_consumed, 37);
                assert_eq!(report.primary_steps, 0, "masked wake stays asleep: opcode={opcode:x}, primask={primask}, basepri={basepri:x}, faultmask={faultmask}, strict={strict_mode}");
                assert_eq!(report.idle_cycles, 37);
                assert_eq!(m.cpu.pc, 0x102);
                assert_eq!(m.cpu.r0, 0);
                // Clearing the mask must make the same real pending IRQ live.
                m.cpu.primask = false;
                m.cpu.basepri = 0;
                m.cpu.faultmask = false;
            }
            run(&mut actual, 17, false);
            run(&mut strict, 17, true);
            assert_eq!(actual.cpu.active_exception, 16);
        }
        equal_state(&actual, &strict);
    }
}
#[test]
fn active_bus_work_keeps_wfi_parked_and_serviced_without_skipping() {
    for device in ["oscctrl", "sercom2", "usb", "dmac"] {
        let mut actual = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
        let mut strict = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
        for m in [&mut actual, &mut strict] {
            m.config.idle_fast_forward_enabled = true;
        }
        run(&mut actual, 1, false);
        run(&mut strict, 1, true);
        for m in [&mut actual, &mut strict] {
            match device {
                // DPLL0 waits for its disabled reference channel.
                "oscctrl" => m.bus.write_u8(0x40001030, 2).unwrap(),
                // A real pending address cannot complete with clocks off.
                "sercom2" => {
                    m.bus.write_u32(0x41012000, 0x16).unwrap();
                    m.bus.write_u16(0x4101201a, 0x10).unwrap();
                    m.bus.write_u32(0x41012024, 0xa0).unwrap();
                }
                // An enabled USB controller must still poll for host tokens.
                "usb" => {
                    m.bus.write_u32(0x40000810, 1 << 10).unwrap();
                    m.bus.write_u32(0x40000818, 1).unwrap();
                    m.bus.write_u8(0x41000000, 2).unwrap();
                    m.bus.write_u8(0x41000008, 0).unwrap();
                }
                // A genuine half-word DAC channel waits for TC3 overflow.
                "dmac" => {
                    m.bus.write_u32(0x40000810, 1 << 9).unwrap();
                    m.bus.write_u16(0x20001000, 0x0501).unwrap();
                    m.bus.write_u16(0x20001002, 1).unwrap();
                    m.bus.write_u32(0x20001004, 0x20002002).unwrap();
                    m.bus.write_u32(0x20001008, 0x43002410).unwrap();
                    m.bus.write_u32(0x2000100c, 0).unwrap();
                    m.bus.write_u32(0x4100a034, 0x20001000).unwrap();
                    m.bus.write_u32(0x4100a038, 0x20001200).unwrap();
                    m.bus
                        .write_u32(0x4100a040, (0x35 << 8) | (2 << 20) | 2)
                        .unwrap();
                    m.bus.write_u32(0x4100a000, 0x102).unwrap();
                }
                _ => unreachable!(),
            }
            let p = m.bus.peripherals.iter().find(|p| p.name == device).unwrap();
            assert!(p.dev.needs_bus_tick(), "{device} really needs service");
            assert!(
                !p.dev.legacy_tick_active(),
                "{device} has no IRQ to veto skipping"
            );
        }
        for (m, strict_mode) in [(&mut actual, false), (&mut strict, true)] {
            let mut request = AdvanceRequest::run(Some(37));
            if strict_mode {
                request = request.with_batch_cap(NonZeroU32::new(1).unwrap());
            }
            let report = m.advance(request).unwrap();
            assert_eq!(report.fuel_consumed, 37);
            assert_eq!(report.idle_cycles, 0, "{device} must not lose service");
            // Primary quanta service the bus; WFI still fetches no MOVS.
            assert_eq!(report.primary_steps, 37);
            assert_eq!(m.cpu.pc, 0x102, "{device}: CPU stays parked");
            assert_eq!(m.cpu.r0, 0);
            assert_eq!(m.cpu.active_exception, 0);
            let p = m.bus.peripherals.iter().find(|p| p.name == device).unwrap();
            assert!(
                p.dev.needs_bus_tick(),
                "{device} remains live without a refusal"
            );
        }
        equal_state(&actual, &strict);
    }
}
#[test]
fn default_idle_mode_preserves_wfi_as_spin() {
    let mut actual = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
    let mut strict = machine(&[0xbf30, 0x205a, 0xe7fe], &[], false);
    assert!(!actual.config.idle_fast_forward_enabled);
    run(&mut actual, 38, false);
    run(&mut strict, 38, true);
    equal_state(&actual, &strict);
    assert_eq!(actual.cpu.r0, 0x5a);
}
#[test]
fn aircr_reset_is_consumed_before_a_warm_chunk() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    run(&mut actual, 257, false);
    run(&mut strict, 257, true);
    for m in [&mut actual, &mut strict] {
        m.bus.write_u32(0xe000e100, 2).unwrap();
        m.bus.write_u32(0xe000ed0c, 0x05fa0004).unwrap();
    }
    run(&mut actual, 1031, false);
    run(&mut strict, 1031, true);
    equal_state(&actual, &strict);
    assert_eq!(
        actual.bus.read_u32(0xe000e100).unwrap(),
        0,
        "reset cleared ISER"
    );
    assert_eq!(strict.bus.read_u32(0xe000e100).unwrap(), 0);
}
#[test]
fn boxed_cpu_forwards_the_real_capability_and_keeps_strict_state() {
    let m = spin(false);
    let mut actual = Machine::new(Box::new(m.cpu) as Box<dyn Cpu>, m.bus);
    let mut strict = spin(false);
    for fuel in [37, 1031, 2049] {
        run(&mut actual, fuel, false);
        run(&mut strict, fuel, true);
        equal_state(&actual, &strict);
    }
    assert!(
        actual.step_profile().cpu_batches * 4 < actual.step_profile().cpu_instructions,
        "boxed production CPU must actually forward checked chunks"
    );
}
#[test]
fn retirement_witness_records_actual_full_chunks_and_resets_with_profile() {
    let mut actual = machine(&[0xe7fe], &[], false);
    let mut strict = machine(&[0xe7fe], &[], false);
    run(&mut actual, 7, false);
    run(&mut strict, 7, true);
    assert_eq!(
        actual.max_retired_quiescent_chunk(),
        0,
        "a planned window below admission size retires no checked chunk"
    );
    run(&mut actual, 2049, false);
    run(&mut strict, 2049, true);
    equal_state(&actual, &strict);
    assert_eq!(actual.max_retired_quiescent_chunk(), 1024);
    assert_eq!(strict.max_retired_quiescent_chunk(), 0);
    actual.reset_step_profile();
    assert_eq!(actual.max_retired_quiescent_chunk(), 0);
}
#[test]
fn inactive_checked_chunks_match_rotated_strict_prefixes_and_really_coalesce() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    for fuel in [37, 1031, 2049, 4097, 53] {
        run(&mut actual, fuel, false);
        run(&mut strict, fuel, true);
        equal_state(&actual, &strict);
    }
    assert!(
        actual.step_profile().cpu_batches * 4 < actual.step_profile().cpu_instructions,
        "nonvacuous actual checked chunks"
    );
    assert_eq!(
        strict.step_profile().cpu_batches,
        strict.step_profile().cpu_instructions
    );
    assert_eq!(
        actual.max_safe_tick_interval(),
        1,
        "active-model recommendation stays conservative"
    );
}
#[derive(Debug)]
struct Observe;
impl SimulationObserver for Observe {}
#[test]
fn attached_observed_and_unsupported_modes_keep_one_instruction_commits() {
    for mode in [
        "attached",
        "observer",
        "bus-observer",
        "decode-off",
        "batch-off",
        "jit",
        "breakpoint",
    ] {
        // An uncompiled backend cannot run, even when the CLI's request flag
        // is set. The separate control below must actually coalesce there.
        if mode == "jit" && !cfg!(feature = "jit") {
            continue;
        }
        let mut m = spin(mode == "attached");
        match mode {
            "observer" => m.observers.push(Arc::new(Observe)),
            "bus-observer" => m.bus.observers.push(Arc::new(Observe)),
            "decode-off" => m.config.decode_cache_enabled = false,
            "batch-off" => m.config.batch_mode_enabled = false,
            "jit" => m.config.cortex_m_jit_enabled = true,
            "breakpoint" => {
                m.breakpoints.insert(0x234);
            }
            _ => {}
        }
        run(&mut m, 257, false);
        assert_eq!(
            m.step_profile().cpu_batches,
            257,
            "{mode} must not enter this capability"
        );
    }
}
#[cfg(not(feature = "jit"))]
#[test]
fn uncompiled_jit_request_matches_the_real_cli_default_and_still_coalesces() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    for m in [&mut actual, &mut strict] {
        m.config.cortex_m_jit_enabled = true;
        m.bus.config.cortex_m_jit_enabled = true;
    }
    for fuel in [37, 1031, 2049] {
        run(&mut actual, fuel, false);
        run(&mut strict, fuel, true);
        equal_state(&actual, &strict);
    }
    assert!(
        actual.step_profile().cpu_batches * 4 < actual.step_profile().cpu_instructions,
        "an inert request must not disable the production interpreter capability"
    );
}
#[test]
fn trace_clock_probe_matches_the_last_strict_instruction_boundary() {
    use labwired_core::bus::bus_trace::{BusDir, BusPayload};
    let mut actual = spin(false);
    let mut strict = spin(false);
    for fuel in [37, 1031, 2049] {
        run(&mut actual, fuel, false);
        run(&mut strict, fuel, true);
        for m in [&mut actual, &mut strict] {
            // Authored trace-clock probe, not a claimed guest UART transfer.
            m.bus.bus_trace.push(
                "authored-clock-probe",
                BusPayload::Uart {
                    direction: BusDir::Tx,
                    byte: 0x41,
                },
            );
        }
        assert_eq!(
            actual.bus.bus_trace.snapshot(),
            strict.bus.bus_trace.snapshot()
        );
        assert_eq!(
            actual.bus.bus_trace.snapshot().last().unwrap().cycle,
            actual.total_cycles - 1
        );
    }
}
#[test]
fn real_scheduled_uart_after_coalescence_keeps_bytes_and_complete_trace() {
    use labwired_core::bus::bus_trace::{BusDir, BusPayload};
    use labwired_core::peripherals::uart::UartStreamDevice;
    struct OneByte(Option<u8>);
    impl UartStreamDevice for OneByte {
        fn poll(&mut self, _elapsed_us: u32) -> Option<u8> {
            self.0.take()
        }
    }
    let mut actual = spin(false);
    let mut strict = spin(false);
    run(&mut actual, 257, false);
    run(&mut strict, 257, true);
    assert!(actual.step_profile().cpu_batches * 4 < actual.step_profile().cpu_instructions);
    for m in [&mut actual, &mut strict] {
        m.bus.write_u32(0x40000818, 1 << 10).unwrap();
        // SERCOM3 also requires GCLK PCHCTRL24.CHEN (generator zero).
        m.bus.write_u32(0x40001ce0, 1 << 6).unwrap();
        m.bus
            .attach_uart_stream_by_id("sercom3", Box::new(OneByte(Some(0x5a))))
            .unwrap();
        m.bus.write_u32(0x41014028, 0x33).unwrap();
    }
    // The real stream cadence is one cycle on this configuration. Do not
    // invent a wider UART deadline to make a synthetic wide-event claim.
    run(&mut actual, 17, false);
    run(&mut strict, 17, true);
    assert_eq!(actual.bus.read_u32(0x41014028).unwrap() & 0xff, 0x5a);
    assert_eq!(strict.bus.read_u32(0x41014028).unwrap() & 0xff, 0x5a);
    equal_state(&actual, &strict);
    let events = actual.bus.bus_trace.snapshot();
    assert_eq!(events, strict.bus.bus_trace.snapshot());
    for (direction, byte) in [(BusDir::Tx, 0x33), (BusDir::Rx, 0x5a)] {
        assert!(events.iter().any(|event| event.bus == "sercom3" &&
            matches!(&event.payload, BusPayload::Uart { direction: d, byte: b } if *d == direction && *b == byte)),
            "real generic UART byte and direction observed");
    }
}
fn io_program(address: u32, value: u32, store: u16, load: u16) -> M {
    // Warm a finite countdown, then leave its nonzero prefix BEFORE any IO.
    // LDR r1; MOVS r0,37; SUBS r0,1; BNE; LDR r0,value; STORE; LOAD; B .
    machine(
        &[0x4903, 0x2025, 0x3801, 0xd1fd, 0x4802, store, load, 0xe7fe],
        &[(0x110, address), (0x114, value)],
        false,
    )
}
#[test]
fn prefix_closes_before_gated_and_active_model_system_and_storage_accesses() {
    for (index, (address, value, store, load)) in [
        (0x41000000, 2, 0x7008, 0x780a),      // USB CTRLA, clock-gated
        (0x41000000, 2, 0x7008, 0x780a),      // same model with banks opened below
        (0x40003800, 2, 0x6008, 0x680a),      // TC0 CTRLA is a word register
        (0x41016000, 2, 0x6008, 0x680a),      // TCC0 CTRLA
        (0x43001c00, 2, 0x8008, 0x880a),      // ADC0 CTRLA
        (0x4100a000, 2, 0x8008, 0x880a),      // DMAC CTRL
        (0x41004004, 0xa515, 0x8008, 0xbf00), // NVM page-buffer clear
        (0x78000, 0x41, 0x6008, 0x680a),      // actual manual-storage intercept
        (0xe000e010, 7, 0x6008, 0x680a),      // SysTick enable/read
        (0xe000e200, 1, 0x6008, 0x680a),      // NVIC pend/read
        (0xe000ed10, 4, 0x6008, 0x680a),      // SCB SCR
    ]
    .into_iter()
    .enumerate()
    {
        let mut actual = io_program(address, value, store, load);
        let mut strict = io_program(address, value, store, load);
        if index == 1 {
            for m in [&mut actual, &mut strict] {
                m.bus.write_u32(0x40000810, 1 << 10).unwrap();
                m.bus.write_u32(0x40000818, 1).unwrap();
            }
        }
        if address == 0xe000e010 {
            for m in [&mut actual, &mut strict] {
                m.bus.write_u32(0xe000e014, 3).unwrap();
            }
        }
        for fuel in [17, 31, 29, 1, 2, 51] {
            run(&mut actual, fuel, false);
            run(&mut strict, fuel, true);
            equal_state(&actual, &strict);
        }
    }
}
#[test]
fn unmapped_access_has_the_same_committed_prefix_and_bus_policy() {
    let mut actual = io_program(0xdead0000, 1, 0x6008, 0x680a);
    let mut strict = io_program(0xdead0000, 1, 0x6008, 0x680a);
    // The existing bus permits unmapped stores and returns its default value
    // for unmapped reads. Chunk refusal must preserve that original policy.
    run(&mut actual, 200, false);
    run(&mut strict, 200, true);
    equal_state(&actual, &strict);
}
#[test]
fn refused_register_error_has_the_same_committed_prefix() {
    // TC CTRLBCLR is byte-wide. Unlike an unmapped address, this actual model
    // rejects a word store. Preserve both the error and all committed state.
    let mut actual = io_program(0x40003804, 1, 0x6008, 0x680a);
    let mut strict = io_program(0x40003804, 1, 0x6008, 0x680a);
    let a = actual.advance(AdvanceRequest::run(Some(200))).unwrap_err();
    let b = strict
        .advance(AdvanceRequest::run(Some(200)).with_batch_cap(NonZeroU32::new(1).unwrap()))
        .unwrap_err();
    assert_eq!(a.to_string(), b.to_string());
    assert!(a.to_string().contains("store outside the 8/16-bit slice"));
    equal_state(&actual, &strict);
}

#[test]
fn external_window_runner_keeps_the_strict_service_grid() {
    let mut actual = spin(false);
    let mut strict = spin(false);
    let mut windows = Vec::new();
    let report = actual
        .advance_with_window_runner(
            AdvanceRequest::run(Some(1031)),
            |cpu, bus, observers, config, count| {
                windows.push(count);
                cpu.step_batch(bus, observers, config, count)
            },
        )
        .unwrap();
    assert_eq!(report.primary_steps, 1031);
    assert_eq!(report.fuel_consumed, 1031);
    assert_eq!(windows.len(), 1031);
    assert!(windows.iter().all(|count| *count == 1));
    assert_eq!(actual.max_retired_quiescent_chunk(), 0);
    run(&mut strict, 1031, true);
    equal_state(&actual, &strict);

    // The same warm machine still admits the in-tree checked primitive.
    run(&mut actual, 1031, false);
    run(&mut strict, 1031, true);
    equal_state(&actual, &strict);
    assert!(actual.max_retired_quiescent_chunk() >= 8);
    assert_eq!(actual.config.peripheral_tick_interval, 1);
    assert_eq!(actual.bus.config.peripheral_tick_interval, 1);
}
