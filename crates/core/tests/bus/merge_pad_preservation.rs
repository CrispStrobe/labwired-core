// SPDX-License-Identifier: MIT
//! Production API controls for the composed fork/upstream pad behavior.
use labwired_config::ExternalDevice;
use labwired_core::bus::{PeripheralEntry, SystemBus};
use labwired_core::peripherals::avr_gpio::AvrGpioPort;
use labwired_core::peripherals::gpio::{GpioPort, GpioRegisterLayout};
use labwired_core::peripherals::kit::AttachCtx;
use labwired_core::peripherals::pad_lines::PadLines;
use labwired_core::peripherals::pad_routing::{PadRoutes, PinRoute};
use labwired_core::Peripheral;
use std::sync::Arc;

fn external() -> ExternalDevice {
    ExternalDevice {
        id: "pad-boundary".into(),
        r#type: "neopixel".into(),
        connection: "gpio".into(),
        channel: None,
        route: Default::default(),
        config: Default::default(),
    }
}

fn empty_bus() -> SystemBus {
    let mut bus = SystemBus::new();
    bus.peripherals.clear();
    bus
}

fn mapped_empty_bus(pins: &[(&str, &str, u8)]) -> SystemBus {
    use labwired_config::{ChipDescriptor, PinLoc, SystemManifest};
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut chip = ChipDescriptor::from_file(root.join("configs/chips/atsamd21g18a.yaml")).unwrap();
    chip.peripherals.clear();
    chip.pins.clear();
    chip.analog_pins.clear();
    for &(label, gpio, bit) in pins {
        chip.pins.insert(
            label.into(),
            PinLoc {
                gpio: gpio.into(),
                bit,
            },
        );
    }
    let manifest: SystemManifest =
        serde_yaml::from_str("name: declared-pads\nchip: unused\n").unwrap();
    SystemBus::from_config(&chip, &manifest).unwrap()
}

fn add(bus: &mut SystemBus, name: &str, dev: Box<dyn Peripheral>) {
    bus.peripherals.push(PeripheralEntry {
        name: name.into(),
        base: 0x4000_0000 + bus.peripherals.len() as u64 * 0x1000,
        size: 128,
        irq: None,
        dev,
        ticks_remaining: 0,
        clock_gate: None,
    });
}

#[test]
fn concrete_gpio_width_bonding_ambiguity_and_wrong_family_refusals() {
    let ext = external();
    for (layout, prefix, width) in [
        (GpioRegisterLayout::SamPort, "port", 32),
        (GpioRegisterLayout::Stm32V2, "gpio", 16),
    ] {
        let mut bus = empty_bus();
        for port in ['a', 'b'] {
            add(
                &mut bus,
                &format!("{prefix}{port}"),
                Box::new(GpioPort::new_with_layout(layout)),
            );
        }
        let ctx = AttachCtx::new(&mut bus, &ext);
        assert_eq!(ctx.parse_gpio_pin("PA15"), Some(15));
        assert_eq!(ctx.parse_gpio_pin("PB0"), Some(width));
        assert_eq!(ctx.parse_gpio_pin("PB15"), Some(width + 15));
        assert_eq!(ctx.parse_gpio_pin("PA31"), (width == 32).then_some(31));
        for invalid in ["PA32", "PC0", "PA-1", "PA+1", "PA 1"] {
            assert_eq!(ctx.parse_gpio_pin(invalid), None, "{layout:?} {invalid}");
        }
        for (label, value) in [("GPIO48", 48), ("IO17", 17), ("15", 15)] {
            assert_eq!(ctx.parse_gpio_pin(label), Some(value));
        }
        assert_eq!(ctx.parse_gpio_pin("GPIO49"), None);
    }
    for count in [6, 8] {
        let mut bus = empty_bus();
        add(
            &mut bus,
            "portd",
            Box::new(AvrGpioPort::new_with_pin_count(count)),
        );
        let ctx = AttachCtx::new(&mut bus, &ext);
        assert_eq!(ctx.parse_gpio_pin("PD5"), Some(29));
        assert_eq!(ctx.parse_gpio_pin("PD6"), (count == 8).then_some(30));
        assert_eq!(ctx.parse_gpio_pin("PD8"), None);
    }
    let mut bus = empty_bus();
    add(
        &mut bus,
        "gpioa",
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::Stm32V2)),
    );
    add(
        &mut bus,
        "porta",
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::SamPort)),
    );
    assert_eq!(AttachCtx::new(&mut bus, &ext).parse_gpio_pin("PA15"), None);
    for (name, layout, label) in [
        ("gpioa", GpioRegisterLayout::SamPort, "PA15"),
        ("portd", GpioRegisterLayout::Stm32V2, "PD6"),
    ] {
        let mut bus = empty_bus();
        add(&mut bus, name, Box::new(GpioPort::new_with_layout(layout)));
        assert_eq!(AttachCtx::new(&mut bus, &ext).parse_gpio_pin(label), None);
        // An explicit map must not relabel a known concrete family to bypass
        // the same canonical-name refusal.
        let mut mapped = mapped_empty_bus(&[(label, name, if label == "PA15" { 15 } else { 6 })]);
        add(
            &mut mapped,
            name,
            Box::new(GpioPort::new_with_layout(layout)),
        );
        assert_eq!(
            AttachCtx::new(&mut mapped, &ext).parse_gpio_pin(label),
            None
        );
    }
}

#[test]
fn other_port_aliases_need_explicit_board_declarations_and_valid_labels() {
    let ext = external();
    let mut bus = empty_bus();
    add(
        &mut bus,
        "declared-sam-port",
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::SamPort)),
    );
    assert_eq!(AttachCtx::new(&mut bus, &ext).parse_gpio_pin("PA15"), None);
    let mut bus = mapped_empty_bus(&[
        ("PA15", "declared-sam-port", 15),
        ("PB0", "declared-sam-port", 0),
        ("PA31", "declared-sam-port", 31),
        ("PA-1", "declared-sam-port", 1),
        ("PA+1", "declared-sam-port", 1),
        ("PA256", "declared-sam-port", 1),
    ]);
    add(
        &mut bus,
        "declared-sam-port",
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::SamPort)),
    );
    let ctx = AttachCtx::new(&mut bus, &ext);
    assert_eq!(ctx.parse_gpio_pin("PA15"), Some(15));
    assert_eq!(ctx.parse_gpio_pin("PB0"), Some(32));
    assert_eq!(ctx.parse_gpio_pin("PA31"), Some(31));
    for invalid in ["PC0", "PA32", "PA-1", "PA+1", "PA256"] {
        assert_eq!(ctx.parse_gpio_pin(invalid), None, "{invalid}");
    }
}

#[test]
fn span_listener_selection_matches_pad_reads_and_preserves_max_prefix() {
    const PWM: &[&str] = &["CH0", "CH1", "CH2"];
    let lines = Arc::new(PadLines::new(PWM, &[false, true, false]));
    let mut routes = PadRoutes::new();
    routes.bind_span(&lines, 5, 71, 0, PWM);
    let listeners = routes.pin_routes(5);
    for offset in 0..3 {
        let selector = 71 + offset;
        let index = PinRoute::select(&listeners, Some(selector)).unwrap();
        assert_eq!(listeners[index].line, offset as usize);
        assert_eq!(
            routes.level(5, |_| Some(selector)),
            Some(lines.level(offset as usize))
        );
        assert_eq!(
            routes.func(5, |_| Some(selector)),
            Some(PWM[offset as usize])
        );
    }
    for selector in [70, 74] {
        assert_eq!(routes.level(5, |_| Some(selector)), None);
        assert_eq!(PinRoute::select(&listeners, Some(selector)), None);
    }
    let mut routes = PadRoutes::new();
    routes.bind_span(&lines, 5, u32::MAX, 0, PWM);
    let listeners = routes.pin_routes(5);
    assert_eq!(listeners.len(), 1);
    assert_eq!(routes.func(5, |_| Some(u32::MAX)), Some("CH0"));
    assert_eq!(PinRoute::select(&listeners, Some(u32::MAX)), Some(0));
    for selector in [0, u32::MAX - 1] {
        assert_eq!(routes.level(5, |_| Some(selector)), None);
        assert_eq!(PinRoute::select(&listeners, Some(selector)), None);
    }
}

#[test]
fn fork_cmcc_and_upstream_samd21_factory_arms_keep_configuration_refusals() {
    use labwired_config::{PeripheralConfig, SystemManifest};
    use labwired_core::bus::bus_trace::BusTrace;
    use labwired_core::peripherals::generic_factory::{is_canonical_model_type, try_build};
    use labwired_core::peripherals::sam::sercom_i2cm::SAMD21_SERCOM_INSTANCES;
    use labwired_core::peripherals::sam::tcc::SAMD21_TCC_INSTANCES;
    let manifest: SystemManifest =
        serde_yaml::from_str("name: merge-factory\nchip: unused\n").unwrap();
    let trace = BusTrace::new();
    for (model, base) in [
        ("samd51_cmcc_board", 0x4100_6000),
        ("samd21_adc", 0x4200_4000),
        ("samd21_tcc", SAMD21_TCC_INSTANCES[0].base),
        ("samd21_sercom_i2cm", SAMD21_SERCOM_INSTANCES[3].base),
    ] {
        let cfg: PeripheralConfig = serde_yaml::from_str(&format!(
            "id: probe\ntype: {model}\nbase_address: {base:#x}\nsize: 256B\n"
        ))
        .unwrap();
        assert!(is_canonical_model_type(model), "{model}");
        assert!(
            try_build(model, &cfg, &manifest, &trace).unwrap().is_some(),
            "{model}"
        );
        let mut invalid = cfg.clone();
        invalid.base_address = 0;
        assert!(
            try_build(model, &invalid, &manifest, &trace).is_err(),
            "{model} invalid base"
        );
        let mut invalid = cfg.clone();
        invalid.irq = Some(255);
        assert!(
            try_build(model, &invalid, &manifest, &trace).is_err(),
            "{model} invalid IRQ"
        );
        let mut invalid = cfg;
        invalid
            .config
            .insert("unexpected".into(), serde_yaml::Value::Bool(true));
        assert!(
            try_build(model, &invalid, &manifest, &trace).is_err(),
            "{model} unexpected config"
        );
    }
}
