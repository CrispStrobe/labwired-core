use super::*;
use crate::bus::{BusResidentDevice, DevicePins};
use crate::peripherals::gpio::{GpioPort, GpioRegisterLayout};
use crate::sim_input::{InputChannel, SimInput, SimInputError};
use crate::Bus;
use std::sync::{Arc, Mutex};

const GPIOA: u64 = 0x4800_0000;
const GPIOB: u64 = 0x4800_0400;
const ODR: u64 = GPIOA + 0x14;
type Trace = Arc<Mutex<Vec<(String, u64, Option<bool>)>>>;

#[derive(Debug)]
struct Probe {
    id: String,
    addresses: Vec<u64>,
    trace: Trace,
}
impl SimInput for Probe {
    fn input_channels(&self) -> &[InputChannel] {
        &[]
    }
    fn set_input(&mut self, key: &str, value: f64) -> Result<(), SimInputError> {
        if key != "address" {
            return Err(SimInputError::UnknownChannel(key.into()));
        }
        self.addresses = if value == 0.0 {
            vec![]
        } else {
            vec![value as u64]
        };
        Ok(())
    }
}
impl BusResidentDevice for Probe {
    fn service(&mut self, _pins: &mut dyn DevicePins, _now: u64) {
        panic!("edge proof must not use the tick service");
    }
    fn service_edge(&mut self, pins: &mut dyn DevicePins, now: u64) {
        self.trace
            .lock()
            .unwrap()
            .push((self.id.clone(), now, pins.output_bit(ODR, 0)));
    }
    fn edge_service_addrs(&self) -> &[u64] {
        &self.addresses
    }
    fn as_sim_input(&mut self) -> &mut dyn SimInput {
        self
    }
    fn id(&self) -> &str {
        &self.id
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
fn bus() -> SystemBus {
    let mut bus = SystemBus::empty();
    for (name, base) in [("gpioa", GPIOA), ("gpiob", GPIOB)] {
        bus.add_peripheral(
            name,
            base,
            0x400,
            None,
            Box::new(GpioPort::new_with_layout(GpioRegisterLayout::Stm32V2)),
        );
    }
    bus
}
fn attach(bus: &mut SystemBus, id: &str, addresses: Vec<u64>, trace: &Trace) {
    bus.gpio_devices.push(Box::new(Probe {
        id: id.into(),
        addresses,
        trace: trace.clone(),
    }));
}

#[test]
fn edge_eligibility_bool_default_tracks_slice_and_real_contact() {
    use crate::peripherals::components::button::Button;
    let contact = Button::new("contact".into(), (GPIOA + 0x10, 0), false);
    assert!(contact.edge_service_addrs().is_empty());
    assert!(!contact.has_edge_service_addrs());
    let mut probe = Probe {
        id: "mutable".into(),
        addresses: vec![],
        trace: Trace::default(),
    };
    for addresses in [vec![], vec![ODR], vec![0x6000_0000], vec![]] {
        probe.addresses = addresses;
        let device: &dyn BusResidentDevice = &probe;
        assert_eq!(
            device.has_edge_service_addrs(),
            !device.edge_service_addrs().is_empty()
        );
    }
}

#[test]
fn edge_eligibility_bool_empty_inventory_and_nonedge_devices_preserve_real_writes() {
    let mut bus = bus();
    let trace = Trace::default();
    bus.write_u32(ODR, 1).unwrap();
    attach(&mut bus, "level", vec![], &trace);
    bus.write_u32(ODR, 0).unwrap();
    assert_eq!(bus.read_u32(ODR).unwrap(), 0);
    assert!(trace.lock().unwrap().is_empty());
    assert_eq!(bus.gpio_devices.len(), 1);
}

#[test]
fn edge_eligibility_bool_public_attachment_removal_and_replacement_are_observed() {
    let mut bus = bus();
    let trace = Trace::default();
    attach(&mut bus, "nonedge", vec![], &trace);
    bus.write_u32(ODR, 0).unwrap();
    attach(&mut bus, "edge", vec![ODR], &trace);
    bus.set_current_cycle(17);
    bus.write_u32(ODR, 1).unwrap();
    let edge = bus.gpio_devices.pop().unwrap();
    bus.write_u32(ODR, 0).unwrap();
    bus.gpio_devices.push(edge);
    bus.set_current_cycle(23);
    bus.write_u32(ODR, 0).unwrap();
    assert_eq!(
        *trace.lock().unwrap(),
        vec![
            ("edge".into(), 17, Some(true)),
            ("edge".into(), 23, Some(false))
        ]
    );
    assert_eq!(bus.gpio_devices.len(), 2);
}

#[test]
fn edge_eligibility_bool_mutated_addresses_and_registration_order_remain_live() {
    let mut bus = bus();
    let trace = Trace::default();
    attach(&mut bus, "first", vec![], &trace);
    attach(&mut bus, "second", vec![ODR], &trace);
    bus.gpio_devices[0]
        .as_sim_input()
        .set_input("address", ODR as f64)
        .unwrap();
    bus.set_current_cycle(31);
    bus.write_u32(ODR, 1).unwrap();
    bus.gpio_devices[0]
        .as_sim_input()
        .set_input("address", (GPIOB + 0x14) as f64)
        .unwrap();
    bus.gpio_devices[1]
        .as_sim_input()
        .set_input("address", 0.0)
        .unwrap();
    bus.write_u32(ODR, 0).unwrap();
    bus.set_current_cycle(37);
    bus.write_u32(GPIOB + 0x14, 1).unwrap();
    assert_eq!(
        *trace.lock().unwrap(),
        vec![
            ("first".into(), 31, Some(true)),
            ("second".into(), 31, Some(true)),
            ("first".into(), 37, Some(false))
        ]
    );
}

#[test]
fn edge_eligibility_bool_respects_narrower_routing_and_ignores_unmapped_addresses() {
    let mut bus = bus();
    let trace = Trace::default();
    bus.add_peripheral(
        "narrow",
        ODR,
        4,
        None,
        Box::new(GpioPort::new_with_layout(GpioRegisterLayout::Stm32V2)),
    );
    attach(&mut bus, "outside", vec![0x6000_0000], &trace);
    attach(&mut bus, "overlap", vec![ODR], &trace);
    assert_eq!(bus.find_peripheral_index(ODR), Some(2));
    bus.maybe_service_edge_driven_gpio_devices(0);
    assert!(trace.lock().unwrap().is_empty());
    bus.set_current_cycle(41);
    bus.write_u32(ODR, 1).unwrap();
    assert_eq!(
        *trace.lock().unwrap(),
        vec![("overlap".into(), 41, Some(true))]
    );
    assert_eq!(bus.gpio_devices.len(), 2);
}
