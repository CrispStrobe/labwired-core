// SPDX-License-Identifier: MIT
//! Authored instruction controls for inactive RAM-only chunks. These are not
//! the independently source-built PXT/native board qualification.
#![cfg(feature = "event-scheduler")]
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
fn run(m: &mut M, fuel: u64, strict: bool) {
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
fn equal_state(actual: &M, strict: &M) {
    assert_eq!(
        serde_json::to_value(actual.cpu.snapshot()).unwrap(),
        serde_json::to_value(strict.cpu.snapshot()).unwrap(),
        "all CPU registers/flags/IRQ state"
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
    let snapshots = |m: &M| {
        m.bus
            .peripherals
            .iter()
            .map(|p| (p.name.clone(), p.dev.snapshot()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        snapshots(actual),
        snapshots(strict),
        "every actual model snapshot"
    );
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
        (0x40003800, 2, 0x8008, 0x880a),      // TC0 CTRLA
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
fn unsupported_access_error_has_the_same_committed_prefix() {
    let mut actual = io_program(0xdead0000, 1, 0x6008, 0x680a);
    let mut strict = io_program(0xdead0000, 1, 0x6008, 0x680a);
    let a = actual.advance(AdvanceRequest::run(Some(200))).unwrap_err();
    let b = strict
        .advance(AdvanceRequest::run(Some(200)).with_batch_cap(NonZeroU32::new(1).unwrap()))
        .unwrap_err();
    assert_eq!(a.to_string(), b.to_string());
    equal_state(&actual, &strict);
}
