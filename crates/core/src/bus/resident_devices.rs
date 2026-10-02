// SPDX-License-Identifier: MIT
//! Vec-compatible ownership with conservative derived edge metadata.
use super::BusResidentDevice;
#[cfg(any(target_arch = "wasm32", test))]
use std::cell::Cell;
use std::ops::{Deref, DerefMut};

/// Resident devices. Every mutable collection/device access invalidates the
/// derived negative edge cache before lending the underlying Vec.
/// Explicit Vec assignments use `.into()`; push/index/iteration remain valid.
/// Cache state is not device state, an MMIO value or serialized evidence.
#[derive(Default)]
pub struct ResidentDevices {
    devices: Vec<Box<dyn BusResidentDevice>>,
    #[cfg(any(target_arch = "wasm32", test))]
    no_edges: Cell<bool>,
}

impl ResidentDevices {
    #[inline(always)]
    pub(crate) fn has_edge_devices(&self) -> bool {
        #[cfg(any(target_arch = "wasm32", test))]
        if self.no_edges.get() {
            return false;
        }
        let found = self
            .devices
            .iter()
            .any(|d| !d.edge_service_addrs().is_empty());
        #[cfg(any(target_arch = "wasm32", test))]
        if !found
            && self
                .devices
                .iter()
                .all(|d| d.edge_service_metadata_is_stable())
        {
            self.no_edges.set(true);
        }
        found
    }

    #[inline(always)]
    fn invalidate(&mut self) {
        #[cfg(any(target_arch = "wasm32", test))]
        self.no_edges.set(false);
    }
}

impl From<Vec<Box<dyn BusResidentDevice>>> for ResidentDevices {
    fn from(devices: Vec<Box<dyn BusResidentDevice>>) -> Self {
        Self {
            devices,
            #[cfg(any(target_arch = "wasm32", test))]
            no_edges: Cell::new(false),
        }
    }
}

impl Deref for ResidentDevices {
    type Target = Vec<Box<dyn BusResidentDevice>>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.devices
    }
}

impl DerefMut for ResidentDevices {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.invalidate();
        &mut self.devices
    }
}

impl<'a> IntoIterator for &'a ResidentDevices {
    type Item = &'a Box<dyn BusResidentDevice>;
    type IntoIter = std::slice::Iter<'a, Box<dyn BusResidentDevice>>;
    fn into_iter(self) -> Self::IntoIter {
        self.devices.iter()
    }
}

impl<'a> IntoIterator for &'a mut ResidentDevices {
    type Item = &'a mut Box<dyn BusResidentDevice>;
    type IntoIter = std::slice::IterMut<'a, Box<dyn BusResidentDevice>>;
    fn into_iter(self) -> Self::IntoIter {
        self.invalidate();
        self.devices.iter_mut()
    }
}

impl IntoIterator for ResidentDevices {
    type Item = Box<dyn BusResidentDevice>;
    type IntoIter = std::vec::IntoIter<Box<dyn BusResidentDevice>>;
    fn into_iter(self) -> Self::IntoIter {
        self.devices.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Debug)]
    struct Probe {
        stable: bool,
        edge: Arc<AtomicBool>,
        queries: Arc<AtomicUsize>,
    }
    impl crate::sim_input::SimInput for Probe {
        fn input_channels(&self) -> &[crate::sim_input::InputChannel] {
            &[]
        }
        fn set_input(
            &mut self,
            key: &str,
            value: f64,
        ) -> Result<(), crate::sim_input::SimInputError> {
            if key != "edge" {
                return Err(crate::sim_input::SimInputError::UnknownChannel(key.into()));
            }
            self.edge.store(value != 0.0, Ordering::Relaxed);
            Ok(())
        }
    }
    impl BusResidentDevice for Probe {
        fn service(&mut self, _: &mut dyn super::super::DevicePins, _: u64) {}
        fn as_sim_input(&mut self) -> &mut dyn crate::sim_input::SimInput {
            self
        }
        fn id(&self) -> &str {
            "edge-metadata-probe"
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn edge_service_addrs(&self) -> &[u64] {
            self.queries.fetch_add(1, Ordering::Relaxed);
            if self.edge.load(Ordering::Relaxed) {
                &[0x4800_0014]
            } else {
                &[]
            }
        }
        fn edge_service_metadata_is_stable(&self) -> bool {
            self.stable
        }
    }
    fn probe(
        stable: bool,
        edge: bool,
    ) -> (
        Box<dyn BusResidentDevice>,
        Arc<AtomicBool>,
        Arc<AtomicUsize>,
    ) {
        let edge = Arc::new(AtomicBool::new(edge));
        let queries = Arc::new(AtomicUsize::new(0));
        (
            Box::new(Probe {
                stable,
                edge: edge.clone(),
                queries: queries.clone(),
            }),
            edge,
            queries,
        )
    }

    #[test]
    fn stable_negative_edge_metadata_is_reused_without_rescanning() {
        let (device, _, queries) = probe(true, false);
        let devices: ResidentDevices = vec![device].into();
        assert!(!devices.has_edge_devices());
        assert!(!devices.has_edge_devices());
        assert_eq!(queries.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn untrusted_shared_metadata_is_never_negative_cached() {
        let (device, edge, queries) = probe(false, false);
        let devices: ResidentDevices = vec![device].into();
        assert!(!devices.has_edge_devices());
        edge.store(true, Ordering::Relaxed); // No mutable collection borrow.
        assert!(devices.has_edge_devices());
        assert_eq!(queries.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn insertion_and_same_length_replacement_invalidate_negative_metadata() {
        let (device, _, _) = probe(true, false);
        let mut devices: ResidentDevices = vec![device].into();
        assert!(!devices.has_edge_devices());
        devices[0] = probe(true, true).0; // Same allocation, length and capacity.
        assert!(devices.has_edge_devices());
        devices[0] = probe(true, false).0;
        assert!(!devices.has_edge_devices());
        devices.push(probe(true, true).0);
        assert!(devices.has_edge_devices());
        devices.pop();
        assert!(!devices.has_edge_devices());
        devices.clear();
        assert!(!devices.has_edge_devices());
    }

    #[test]
    fn mutable_downcast_iteration_and_raw_vec_borrows_invalidate() {
        let mut bus = super::super::SystemBus::new();
        bus.gpio_devices.push(probe(true, false).0);
        assert!(!bus.gpio_devices.has_edge_devices());
        for device in bus.gpio_devices_of_mut::<Probe>() {
            device.edge.store(true, Ordering::Relaxed);
        }
        assert!(bus.gpio_devices.has_edge_devices());
        for device in &mut bus.gpio_devices {
            device.as_sim_input().set_input("edge", 0.0).unwrap();
        }
        assert!(!bus.gpio_devices.has_edge_devices());
        let raw: &mut Vec<Box<dyn BusResidentDevice>> = &mut bus.gpio_devices;
        raw[0] = probe(true, true).0;
        assert!(bus.gpio_devices.has_edge_devices());
    }

    #[test]
    fn take_service_restore_does_not_reintroduce_stale_metadata() {
        let mut devices: ResidentDevices = vec![probe(true, false).0].into();
        assert!(!devices.has_edge_devices());
        let mut taken = std::mem::take(&mut devices);
        assert!(!devices.has_edge_devices());
        taken
            .iter_mut()
            .next()
            .unwrap()
            .as_sim_input()
            .set_input("edge", 1.0)
            .unwrap();
        devices = taken;
        assert!(devices.has_edge_devices());
    }
}
