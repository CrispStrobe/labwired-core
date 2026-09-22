use labwired_config::DeviceDescriptor;

#[test]
fn scalar_and_arbitrary_length_list_pin_roles_deserialize() {
    let desc = DeviceDescriptor::from_yaml(
        r#"
type: pins-test
behavior:
  primitive: gpio_device
  pins:
    clock: clock_pin
    rows: [r0_pin, r1_pin, r2_pin]
    cols: { config: column_pins, count: 5 }
"#,
    )
    .expect("scalar, per-pin keys and config lists must coexist");
    assert_eq!(desc.behavior.pins.len(), 3);
}

#[test]
fn list_bindings_round_trip_without_changing_scalar_shape() {
    let desc = DeviceDescriptor::from_yaml(
        r#"
type: pins-test
behavior:
  primitive: gpio_device
  pins: { clock: clock_pin, rows: [r0_pin, r1_pin] }
"#,
    )
    .unwrap();
    let serialized = serde_yaml::to_value(&desc).unwrap();
    assert_eq!(
        serialized["behavior"]["pins"]["clock"].as_str(),
        Some("clock_pin")
    );
    assert_eq!(
        serialized["behavior"]["pins"]["rows"]
            .as_sequence()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn oversized_pin_groups_are_rejected_before_expansion() {
    let yaml = format!("type: test\nbehavior:\n  primitive: gpio_device\n  pins:\n    rows: {{config: rows, count: {}}}\n", usize::MAX);
    assert!(DeviceDescriptor::from_yaml(&yaml).is_err());
    let list = std::iter::repeat_n("pad", 4097)
        .collect::<Vec<_>>()
        .join(", ");
    assert!(DeviceDescriptor::from_yaml(&format!(
        "type: test\nbehavior:\n  primitive: gpio_device\n  pins:\n    rows: [{list}]\n"
    ))
    .is_err());
}
