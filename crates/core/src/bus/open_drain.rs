// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Retained drivers for an explicitly pulled-up, open-drain software net.
//!
//! This resolver is not connected to device pads yet. It does not model a
//! floating net, analog voltages, propagation delay, or push-pull contention.

use std::collections::BTreeMap;

/// Identity of one output on one resident device instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OpenDrainDriver {
    pub device_id: String,
    pub role: String,
}

/// A net with an explicitly declared pull-up and retained driver states.
/// Each registered driver initially releases the line. `false` drives low;
/// `true` releases it, matching the existing Boolean output-level convention.
#[derive(Debug, Default)]
pub struct PulledUpOpenDrain {
    drivers: BTreeMap<OpenDrainDriver, bool>,
}

impl PulledUpOpenDrain {
    /// Declare a pulled-up net; floating nets require a separate model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a released driver. Duplicate identities are rejected.
    pub fn register(&mut self, driver: OpenDrainDriver) -> Result<(), &'static str> {
        if self.drivers.contains_key(&driver) {
            return Err("duplicate open-drain driver");
        }
        self.drivers.insert(driver, true);
        Ok(())
    }

    /// Update only the named driver and return the resolved pad level.
    pub fn drive(&mut self, driver: &OpenDrainDriver, level: bool) -> Result<bool, &'static str> {
        let state = self
            .drivers
            .get_mut(driver)
            .ok_or("unknown open-drain driver")?;
        *state = level;
        Ok(self.level())
    }

    /// Release one driver without changing the other drivers.
    pub fn release(&mut self, driver: &OpenDrainDriver) -> Result<bool, &'static str> {
        self.drive(driver, true)
    }

    /// Resolve the net against its declared pull-up.
    pub fn level(&self) -> bool {
        self.drivers.values().all(|released| *released)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver(device: &str, role: &str) -> OpenDrainDriver {
        OpenDrainDriver {
            device_id: device.into(),
            role: role.into(),
        }
    }

    #[test]
    fn either_retained_driver_keeps_the_pad_low_in_both_orders() {
        for reverse in [false, true] {
            let (a, b) = (driver("sensor-a", "irq"), driver("sensor-b", "irq"));
            let mut net = PulledUpOpenDrain::new();
            net.register(a.clone()).unwrap();
            net.register(b.clone()).unwrap();
            assert!(net.level());
            let (first, second) = if reverse { (&b, &a) } else { (&a, &b) };
            assert!(!net.drive(first, false).unwrap());
            assert!(!net.drive(second, false).unwrap());
            assert!(!net.release(first).unwrap());
            assert!(net.release(second).unwrap());
        }
    }

    #[test]
    fn duplicate_and_unknown_drivers_cannot_change_the_net() {
        let a = driver("sensor", "irq");
        let unknown = driver("sensor", "other");
        let mut net = PulledUpOpenDrain::new();
        net.register(a.clone()).unwrap();
        net.drive(&a, false).unwrap();
        assert!(net.register(a.clone()).is_err());
        assert!(net.drive(&unknown, true).is_err());
        assert!(!net.level());
        assert!(net.release(&a).unwrap());
    }

    #[test]
    fn separate_nets_do_not_affect_one_another() {
        let a = driver("part", "irq");
        let mut first = PulledUpOpenDrain::new();
        let mut second = PulledUpOpenDrain::new();
        first.register(a.clone()).unwrap();
        second.register(a.clone()).unwrap();
        first.drive(&a, false).unwrap();
        assert!(!first.level());
        assert!(second.level());
    }
}
