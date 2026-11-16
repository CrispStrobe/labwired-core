// SPDX-License-Identifier: MIT
//! Focused controls for the actual opt-in production counter implementation.
#![cfg(feature = "cortex-m-countdown-probe")]
use labwired_core::cpu::cortex_m::countdown_probe::CountdownWindow;
use labwired_core::cpu::CortexM;

fn window(requested_cycles: u64, detail_limit: u64) -> CountdownWindow {
    CountdownWindow {
        requested_cycles,
        completed_cycles: 0,
        attempts: 0,
        returns: 0,
        retired: 0,
        detail_limit,
        detail_recorded: 0,
        overflow: false,
        ended: false,
    }
}

#[test]
fn aggregate_survives_cap_and_counts_zero_returns() {
    let mut p = window(4, 1);
    p.record_attempt();
    p.record_return(37);
    p.record_attempt();
    p.record_return(0);
    p.completed_cycles = 4;
    p.ended = true;
    assert_eq!((p.attempts, p.returns, p.retired), (2, 2, 37));
    assert_eq!(p.detail_recorded, 1);
    assert!(p.detail_exhausted());
    assert!(p.complete());
}

#[test]
fn unmatched_entry_and_counter_overflow_are_not_complete() {
    let mut p = window(1, 1);
    p.record_attempt();
    p.completed_cycles = 1;
    p.ended = true;
    assert!(!p.complete());
    let mut p = window(1, 1);
    p.attempts = u64::MAX;
    p.record_attempt();
    p.completed_cycles = 1;
    p.ended = true;
    assert!(p.overflow);
    assert!(!p.complete());
}

#[test]
fn unfinished_overrun_and_overflow_are_not_complete() {
    let mut cpu = CortexM::new();
    assert!(!cpu.begin_countdown_window(0, 1));
    assert!(!cpu.begin_countdown_window(1, 0));
    assert!(cpu.begin_countdown_window(2, 2));
    assert!(!cpu.begin_countdown_window(2, 2));
    assert!(cpu.countdown_window_progress(1));
    assert!(!cpu.countdown_window().unwrap().complete());
    assert!(!cpu.end_countdown_window().unwrap().complete());
    assert!(cpu.begin_countdown_window(2, 2));
    assert!(cpu.countdown_window_progress(3));
    assert!(!cpu.end_countdown_window().unwrap().complete());
    assert!(cpu.begin_countdown_window(u64::MAX, 2));
    assert!(cpu.countdown_window_progress(u64::MAX));
    assert!(cpu.countdown_window_progress(1));
    let p = cpu.end_countdown_window().unwrap();
    assert!(p.overflow);
    assert!(!p.complete());
    assert!(!cpu.countdown_window_progress(1));
}

#[test]
fn real_dispatch_counts_retirement_without_changing_cpu_state() {
    use labwired_core::{bus::SystemBus, Cpu, SimulationConfig};
    fn fixture() -> (CortexM, SystemBus) {
        let mut cpu = CortexM::new();
        let mut bus = SystemBus::new();
        cpu.pc = 0x100;
        cpu.xpsr = 1 << 24;
        cpu.r0 = 40;
        assert!(bus.flash.write_u16(0x100, 0x3801)); // SUBS r0, #1
        assert!(bus.flash.write_u16(0x102, 0xd1fd)); // BNE 0x100
        let config = SimulationConfig::default();
        cpu.step(&mut bus, &[], &config).unwrap();
        cpu.step(&mut bus, &[], &config).unwrap();
        (cpu, bus)
    }
    let (mut observed, mut observed_bus) = fixture();
    let (mut control, mut control_bus) = fixture();
    let config = SimulationConfig::default();
    assert!(observed.begin_countdown_window(16, 1));
    let retired = observed
        .step_batch(&mut observed_bus, &[], &config, 16)
        .unwrap();
    let expected = control
        .step_batch(&mut control_bus, &[], &config, 16)
        .unwrap();
    assert_eq!(retired, expected);
    assert_eq!(
        serde_json::to_value(observed.snapshot()).unwrap(),
        serde_json::to_value(control.snapshot()).unwrap()
    );
    assert!(observed.countdown_window_progress(u64::from(retired)));
    let window = observed.end_countdown_window().unwrap();
    assert_eq!(window.attempts, 1, "actual countdown fast path must run");
    assert_eq!(window.returns, 1);
    assert_eq!(window.retired, u64::from(retired));
    assert!(window.detail_exhausted());
    assert!(window.complete());
}
