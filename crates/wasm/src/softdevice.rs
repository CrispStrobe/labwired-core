// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! The nRF51 SoftDevice (S110 v8) emulated at API level, for the browser.
//!
//! `labwired_core::sd_hle::build_nrf51_s110` builds this machine from files,
//! which a browser does not have, and only the native examples called it. The
//! step that matters is `Machine::attach_sd_hle`; everything else is the
//! ordinary config-built machine. So a host builds the nRF51 the usual way
//! (`new_from_config` with the micro:bit V1 / Calliope system and chip text,
//! the APPLICATION region as the ELF -- never a Nordic byte) and then calls
//! `attach_softdevice_s110`. The browser's step path goes through
//! `Machine::advance`, which services the SoftDevice at every boundary.

use super::*;
use labwired_core::sd_hle::nrf_softdevice_hle::{Config, SoftDevice};

/// S110's application base on the nRF51822 (the vector table the HLE points VTOR at).
const S110_APP_BASE: u32 = 0x18000;

/// A random-static BLE address derived from the node name, so two boards on
/// one air have distinct addresses and a replay gets the same one.
fn node_address(node: &str) -> [u8; 6] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in node.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let mut a = [0u8; 6];
    for (i, byte) in a.iter_mut().enumerate() {
        *byte = (h >> (8 * i)) as u8;
    }
    a[5] |= 0xC0; // random static: the two top bits set
    a
}

#[wasm_bindgen]
impl WasmSimulator {
    /// Attach the emulated S110 SoftDevice to a built nRF51 whose firmware is
    /// an S110 APPLICATION region at 0x18000 (micro:bit V1, Calliope mini 1).
    /// Journaled. Also releases buttons A (P0.17) and B (P0.26), which the
    /// board pulls up: left floating low, the DAL reads both as held at reset
    /// and enters Bluetooth pairing mode, which never returns.
    #[wasm_bindgen]
    pub fn attach_softdevice_s110(&mut self, node: String) -> Result<(), JsValue> {
        self.record(lab_tools::Op::AttachSoftDeviceS110(node.clone()));
        let machine = self.machine_mut_or_err()?;
        let sd = SoftDevice::new(Config::microbit_v1(&node, node_address(&node)));
        machine
            .attach_sd_hle(sd, S110_APP_BASE)
            .map_err(|e| JsValue::from_str(&format!("SoftDevice attach failed: {e:?}")))?;
        for p in machine
            .bus
            .peripherals
            .iter_mut()
            .filter(|p| p.name == "gpio0")
        {
            p.dev.set_gpio_input(17, true);
            p.dev.set_gpio_input(26, true);
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::node_address;

    #[test]
    fn node_addresses_are_random_static_distinct_and_stable() {
        let a = node_address("microbit-a");
        let b = node_address("microbit-b");
        assert_ne!(a, b);
        assert_eq!(
            a,
            node_address("microbit-a"),
            "a replay gets the same address"
        );
        assert_eq!(a[5] & 0xC0, 0xC0, "random static address");
    }
}
