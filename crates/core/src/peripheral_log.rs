// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Named text logs that peripheral models record during a run.
//!
//! Some models keep a record of what happened: a simulated USB host logs the
//! enumeration it did, a flash controller logs the commands it executed, a
//! shifter logs the words it put on a pin. A model gives these records through
//! [`crate::Peripheral::logs`], one text line per entry. The bus adds one more
//! log to every peripheral: [`BUS_TRACE`], the lines of the universal bus
//! trace that carry the peripheral's name. See
//! [`crate::bus::SystemBus::peripheral_logs`].
//!
//! `labwired test` asserts on these lines with `peripheral_log`. The names a
//! model returns are the only list of valid names: a script that names a log
//! the model does not return is a config error.

/// The name of the bus-trace log that every peripheral has. Its lines are the
/// one-line payload summaries of [`crate::bus::bus_trace::BusPayload`], for
/// example `addr 0x54 W nack` or `mosi 0x9f miso 0xef`.
pub const BUS_TRACE: &str = "bus_trace";

/// One named log of a peripheral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralLog {
    /// The name a test script uses, e.g. `host`.
    pub name: &'static str,
    /// One line per recorded entry, oldest first.
    pub lines: Vec<String>,
}

impl PeripheralLog {
    pub fn new(name: &'static str, lines: Vec<String>) -> Self {
        Self { name, lines }
    }
}

impl crate::bus::SystemBus {
    /// Every named log of the peripheral `name`: the logs the model records
    /// ([`crate::Peripheral::logs`]), then [`BUS_TRACE`]. `None` when no
    /// peripheral and no attached device has that name.
    ///
    /// The bus trace is a ring. When it is full the oldest events go first,
    /// so a long run can lose early lines.
    /// [`crate::bus::bus_trace::BusTrace::evicted`] tells how many.
    ///
    /// When no peripheral has that name, `name` can be the
    /// `external_devices:` id of an external device, which has only the logs
    /// it records and no [`BUS_TRACE`] of its own: a bus-resident device (a
    /// GPIO part such as a segment display), or a device attached to a
    /// controller ([`crate::peripherals::i2c::I2cDevice::logs`]; its traffic is
    /// in the bus trace of that controller).
    pub fn peripheral_logs(&self, name: &str) -> Option<Vec<PeripheralLog>> {
        let Some(index) = self.find_peripheral_index_by_name(name) else {
            return self
                .gpio_devices
                .iter()
                .find(|d| d.id() == name)
                .map(|d| d.logs())
                .or_else(|| self.device_logs(name));
        };
        let mut logs = self.peripherals[index].dev.logs();
        let trace = self
            .bus_trace
            .snapshot()
            .into_iter()
            .filter(|e| e.bus == name)
            .map(|e| e.payload.to_string())
            .collect();
        logs.push(PeripheralLog::new(BUS_TRACE, trace));
        Some(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::bus_trace::{BusPayload, I2cSym};
    use crate::bus::SystemBus;
    use crate::peripherals::imxrt::flexspi::ImxrtFlexspi;

    #[test]
    fn model_logs_then_the_bus_trace_of_that_peripheral() {
        let mut bus = SystemBus::new();
        bus.add_peripheral(
            "flash",
            0x4000_0000,
            0x1000,
            None,
            Box::new(ImxrtFlexspi::default()),
        );
        let nack = BusPayload::I2c {
            kind: I2cSym::AddrWrite,
            byte: 0x54 << 1,
            ack: false,
        };
        bus.bus_trace.push("flash", nack.clone());
        bus.bus_trace.push("i2c9", nack);

        let logs = bus.peripheral_logs("flash").expect("known peripheral");
        let names: Vec<_> = logs.iter().map(|l| l.name).collect();
        assert_eq!(names, ["ip", BUS_TRACE], "empty logs are still listed");
        assert!(logs[0].lines.is_empty());
        assert_eq!(
            logs[1].lines,
            ["addr 0x54 W nack"],
            "other buses filtered out"
        );
        assert_eq!(bus.peripheral_logs("nope"), None);
    }
}
