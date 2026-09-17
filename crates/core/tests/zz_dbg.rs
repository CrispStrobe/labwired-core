#[test]
fn dbg() {
    let yaml = labwired_config::embedded_device_yaml("ds3231").unwrap();
    let d = labwired_config::DeviceDescriptor::from_yaml(yaml).unwrap();
    for r in &d.behavior.i2c.as_ref().unwrap().registers {
        if r.name.starts_with("ALARM1_SEC") {
            println!("{} encode={:?} write_mask={:?}", r.name, r.encode, r.write_mask);
        }
    }
}
