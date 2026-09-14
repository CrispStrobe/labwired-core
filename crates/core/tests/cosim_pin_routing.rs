// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Co-simulation board routing, on a REAL chip.
//!
//! `crates/core/src/cosim/routing.rs` unit-tests the grammar and the unit
//! conversions in isolation. What this file covers is the part those cannot:
//! that a manifest path resolves against an actual `SystemBus` built from a
//! shipped chip descriptor, and that a routed value lands where the FIRMWARE
//! would look for it — the GPIO input register it samples, the ADC channel it
//! converts — rather than merely in the signal store.
//!
//! Both directions are asserted through the same seams the firmware uses:
//! `read_gpio_output` for an output pad, the GPIO input register for a driven
//! pin, and the ADC's own injected-channel readback for an analog level.

use labwired_config::{ChipDescriptor, CosimAdapter, CosimModelConfig, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::cosim::{CosimSession, CosimSignalValue, RoutingError};
use labwired_core::Bus;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn root(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// A bare NUCLEO-F401RE: the chip the `cosim-spice-rc` example targets, with
/// no external devices, so nothing but the co-simulation touches the ADC.
fn f401_bus() -> SystemBus {
    let chip_path = root("configs/chips/stm32f401.yaml");
    let chip = ChipDescriptor::from_file(&chip_path).expect("load stm32f401 chip");
    let manifest_yaml = format!(
        "name: \"cosim-routing\"\nchip: \"{}\"\nexternal_devices: []\n",
        chip_path.display()
    );
    let manifest: SystemManifest = serde_yaml::from_str(&manifest_yaml).expect("parse manifest");
    SystemBus::from_config(&chip, &manifest).expect("build bus")
}

/// A `mock` model: static outputs from `config.outputs`, routed through the
/// manifest's `outputs:` map like any other adapter's.
fn mock_model(
    step_ns: u64,
    inputs: &[(&str, &str)],
    outputs: &[(&str, &str)],
    static_outputs: &[(&str, serde_yaml::Value)],
) -> CosimModelConfig {
    let mut config = HashMap::new();
    let mapping: serde_yaml::Mapping = static_outputs
        .iter()
        .map(|(k, v)| (serde_yaml::Value::String((*k).to_string()), v.clone()))
        .collect();
    config.insert("outputs".to_string(), serde_yaml::Value::Mapping(mapping));
    CosimModelConfig {
        id: "routing_probe".to_string(),
        adapter: CosimAdapter::Mock,
        model: None,
        step_ns,
        inputs: inputs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        outputs: outputs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        config,
    }
}

fn build_session(bus: &SystemBus, models: &[CosimModelConfig]) -> CosimSession {
    let session = CosimSession::new(models, Path::new("."), bus)
        .expect("build session")
        .expect("models were declared, so there is a session");
    assert_eq!(
        session.binding_errors(),
        &[] as &[RoutingError],
        "every path in this fixture must resolve on an F401"
    );
    session
}

/// The injected 12-bit count the next conversion on `channel` would return.
/// `0xFFFF` means nothing has been injected.
fn adc_channel_count(bus: &mut SystemBus, channel: u8) -> u16 {
    let idx = bus
        .find_peripheral_index_by_name("adc1")
        .expect("F401 declares adc1");
    bus.peripherals[idx]
        .dev
        .as_any_mut()
        .expect("adc1 downcasts")
        .downcast_mut::<labwired_core::peripherals::adc::Adc>()
        .expect("adc1 is an Adc")
        .channel_input_count(channel)
}

/// Drive a GPIO output latch the way firmware does — an MMIO store to ODR.
fn write_odr_bit(bus: &mut SystemBus, pad: &str, level: bool) {
    let (addr, bit) = SystemBus::resolve_pin_odr_pub(bus, pad).expect("pad resolves to an ODR");
    let current = bus.read_u32(addr).expect("read ODR");
    let next = if level {
        current | (1 << bit)
    } else {
        current & !(1 << bit)
    };
    bus.write_u32(addr, next).expect("write ODR");
}

fn read_idr_bit(bus: &mut SystemBus, pad: &str) -> bool {
    let (addr, bit) = SystemBus::resolve_pin_idr_pub(bus, pad).expect("pad resolves to an IDR");
    bus.read_u32(addr).expect("read IDR") >> bit & 1 != 0
}

/// 84 MHz core, 100 us model period: the boundary is 8400 cycles, and both
/// edges of that are asserted — a machine at 0 may run the whole period, and a
/// machine already at the boundary must still be allowed to make progress
/// rather than be handed a zero budget and hang.
#[test]
fn the_cycle_budget_lands_the_machine_on_the_model_boundary() {
    let bus = f401_bus();
    assert_eq!(bus.cpu_hz, 84_000_000, "F401 descriptor declares 84 MHz");
    let session = build_session(&bus, &[mock_model(100_000, &[], &[], &[])]);

    assert_eq!(session.cpu_hz(), 84_000_000);
    assert_eq!(session.step_ns(), 100_000);
    assert_eq!(session.cycles_until_boundary(0), 8_400);
    assert_eq!(session.cycles_until_boundary(8_000), 400);
    assert_eq!(session.cycles_until_boundary(8_400), 1);
    assert_eq!(session.cycles_until_boundary(100_000), 1);
}

/// The finest declared period wins: stepping at anything coarser would let the
/// machine run past the faster model's boundary before that model saw the pin
/// levels that produced it.
#[test]
fn the_lockstep_granularity_is_the_finest_model_period() {
    let bus = f401_bus();
    let mut fast = mock_model(10_000, &[], &[], &[]);
    fast.id = "fast".to_string();
    let mut slow = mock_model(1_000_000, &[], &[], &[]);
    slow.id = "slow".to_string();
    let session = build_session(&bus, &[slow, fast]);
    assert_eq!(session.step_ns(), 10_000);
    assert_eq!(session.model_count(), 2);
}

/// Machine → model. The level is read through `read_gpio_output`, the same
/// accessor a `board_io` LED reads, so a pin the firmware drives and a pin a
/// model samples can never disagree.
#[test]
fn a_firmware_driven_pad_reaches_the_signal_store() {
    let mut bus = f401_bus();
    let mut session = build_session(
        &bus,
        &[mock_model(
            100_000,
            &[("gpio", "board.gpio.pa5")],
            &[],
            &[("unused", serde_yaml::Value::Bool(false))],
        )],
    );

    write_odr_bit(&mut bus, "PA5", true);
    session
        .advance_to(8_400, &mut bus)
        .expect("step at the boundary");
    assert_eq!(
        session.signals().get("board.gpio.pa5"),
        Some(&CosimSignalValue::Bool(true)),
        "PA5 driven high must reach the model as true"
    );

    write_odr_bit(&mut bus, "PA5", false);
    session
        .advance_to(16_800, &mut bus)
        .expect("step at the next boundary");
    assert_eq!(
        session.signals().get("board.gpio.pa5"),
        Some(&CosimSignalValue::Bool(false)),
        "PA5 driven low must reach the model as false"
    );
}

/// Model → machine, analog. 1.65 V is half of the 3.3 V reference, which the
/// ADC model converts to 2047 counts at 12 bits — the same count
/// `examples/ntc-thermistor-lab` asserts for its divider midpoint, because the
/// routing hands the ADC millivolts and lets it own the conversion instead of
/// doing the arithmetic a second time with a second rounding rule.
#[test]
fn a_model_voltage_lands_on_the_adc_channel_of_its_pad() {
    let mut bus = f401_bus();
    assert_eq!(
        adc_channel_count(&mut bus, 0),
        0xFFFF,
        "nothing is injected before the first co-sim step"
    );

    let mut session = build_session(
        &bus,
        &[mock_model(
            100_000,
            &[],
            &[("v_out", "board.analog.pa0_volts")],
            &[("v_out", serde_yaml::Value::from(1.65))],
        )],
    );
    let (routed, errors) = session
        .advance_to(8_400, &mut bus)
        .expect("step at the boundary");
    assert!(errors.is_empty(), "unexpected routing errors: {errors:?}");
    assert_eq!(routed.len(), 1, "one model, one boundary crossed");
    assert_eq!(adc_channel_count(&mut bus, 0), 2047);
}

/// The chip-neutral form addresses a controller and channel directly, for
/// parts whose pad → channel map LabWired does not model. It must land on the
/// same channel the pad form does.
#[test]
fn the_explicit_adc_channel_form_routes_to_the_same_place() {
    let mut bus = f401_bus();
    let mut session = build_session(
        &bus,
        &[mock_model(
            100_000,
            &[],
            &[("v_out", "adc.adc1.0_volts")],
            &[("v_out", serde_yaml::Value::from(1.65))],
        )],
    );
    session.advance_to(8_400, &mut bus).expect("step");
    assert_eq!(adc_channel_count(&mut bus, 0), 2047);
}

/// Model → machine, digital. The level goes through `set_gpio_input`, the seam
/// a `board_io` button and a sensor status line already use, so the firmware
/// samples it from the input register exactly as it would a real pin.
#[test]
fn a_model_output_drives_a_gpio_input_pin() {
    let mut bus = f401_bus();
    assert!(!read_idr_bit(&mut bus, "PC13"), "PC13 starts low");

    let mut session = build_session(
        &bus,
        &[mock_model(
            100_000,
            &[],
            &[("pressed", "board.gpio_in.pc13")],
            &[("pressed", serde_yaml::Value::Bool(true))],
        )],
    );
    let (_, errors) = session.advance_to(8_400, &mut bus).expect("step");
    assert!(errors.is_empty(), "unexpected routing errors: {errors:?}");
    assert!(
        read_idr_bit(&mut bus, "PC13"),
        "the model's true must be readable on PC13's input register"
    );
}

/// No boundary reached, no model stepped, nothing written. A co-simulation
/// must not run ahead of simulated time just because the loop called it.
#[test]
fn nothing_is_routed_before_the_first_boundary() {
    let mut bus = f401_bus();
    let mut session = build_session(
        &bus,
        &[mock_model(
            100_000,
            &[],
            &[("v_out", "board.analog.pa0_volts")],
            &[("v_out", serde_yaml::Value::from(1.65))],
        )],
    );
    let (routed, errors) = session
        .advance_to(8_399, &mut bus)
        .expect("step below the boundary");
    assert!(routed.is_empty(), "8399 cycles is 99.99 us, not 100 us");
    assert!(errors.is_empty());
    assert_eq!(adc_channel_count(&mut bus, 0), 0xFFFF);
}

/// A pad that does not exist on this chip is caught when the session is built,
/// not silently read as low for the whole run.
#[test]
fn an_unroutable_pad_is_reported_at_bind_time() {
    let bus = f401_bus();
    // The F401 descriptor declares gpioa/gpiob/gpioc only.
    let models = [mock_model(
        100_000,
        &[("gpio", "board.gpio.pz9")],
        &[],
        &[("unused", serde_yaml::Value::Bool(false))],
    )];
    let session = CosimSession::new(&models, Path::new("."), &bus)
        .expect("building the session is not itself an error")
        .expect("models were declared");
    assert_eq!(
        session.binding_errors(),
        &[RoutingError::UnknownPad {
            path: "board.gpio.pz9".to_string(),
            pad: "pz9".to_string(),
        }]
    );
}
