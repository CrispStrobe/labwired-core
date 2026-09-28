// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! 74HC4051 8-channel analog multiplexer (NXP 74HC4051 / TI CD74HC4051).
//!
//! Datasheet (NXP 74HC4051 rev. 12, table 3 "Function table"): the select
//! inputs S2 S1 S0 pick ONE of Y0..Y7 and connect it to the common pin Z.
//! Channel number = S2·4 + S1·2 + S0. A HIGH on the enable input E (active
//! LOW) opens every switch.
//!
//! The part sits between analog sources and an ADC channel:
//!
//! ```text
//!  pot k0 ──Y0┐
//!  pot k1 ──Y1┤ 74HC4051 ──Z── ADC channel (connection / channel)
//!    ...      │   ▲ ▲ ▲ ▲
//!  pot k7 ──Y7┘  S0 S1 S2 E  ◀── MCU GPIO outputs
//! ```
//!
//! What this model does:
//! * An analog source (a potentiometer, a thermistor, ...) that names this
//!   mux as its `connection:` and a mux input as its `channel:` drives Yn.
//! * The select and enable pads are read from the GPIO model (the pad level,
//!   [`crate::Peripheral::read_gpio_pad`]). The bus re-routes synchronously
//!   inside every MMIO write to a GPIO peripheral that hosts one of these
//!   pads, so a conversion that the firmware starts right after it moved the
//!   select lines converts the NEW channel.
//! * Z drives the downstream ADC channel with the routed Yn voltage.
//!
//! What it does not model: the ON resistance (70 Ω typ. at 4.5 V, ignored:
//! an ADC input draws no DC current), the switch time (t_on ≈ 20 ns, far
//! shorter than one ADC sample time), charge injection, and bidirectional
//! use (Z as an input). With E HIGH, Z is open; this model then drives
//! 0 mV onto the ADC channel, as a board with a pull-down on Z does. A
//! floating ADC input on silicon holds whatever charge the sample capacitor
//! had, which a firmware must not depend on.

use crate::peripherals::kit::{
    AttachCtx, Category, ConfigKey, ConfigType, KitMetadata, PeripheralKit, Transport,
};
use anyhow::{anyhow, bail, Result};
use std::borrow::Cow;

/// Number of select inputs of a 74HC4051 (S0, S1, S2).
pub const SELECT_PINS: usize = 3;
/// Number of Yn inputs.
pub const CHANNELS: usize = 1 << SELECT_PINS;

/// One MCU pad the mux observes, resolved at attach time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxPad {
    /// The pad label as written in system.yaml (for diagnostics).
    pub label: String,
    /// Bus index of the GPIO peripheral that owns the pad.
    pub peripheral: usize,
    /// Pin number inside that GPIO peripheral.
    pub bit: u8,
}

/// The select decode of the datasheet function table.
///
/// `select` holds the S0, S1, S2 levels in that order. `enable_high` is the
/// level on E; `None` means E is tied to GND (always enabled). Returns the
/// routed Yn, or `None` when E is HIGH and every switch is open.
pub fn decode(select: [bool; SELECT_PINS], enable_high: Option<bool>) -> Option<usize> {
    if enable_high == Some(true) {
        return None;
    }
    Some(
        select
            .iter()
            .enumerate()
            .map(|(i, &s)| (s as usize) << i)
            .sum(),
    )
}

/// True for a system.yaml `type:` that builds an [`AnalogMux`]. Parts that
/// name such a device as their `connection:` are its analog inputs, not
/// devices behind an I²C bus switch.
pub fn is_analog_mux_type(type_str: &str) -> bool {
    matches!(
        type_str.to_ascii_lowercase().as_str(),
        "74hc4051" | "cd74hc4051" | "cd4051"
    )
}

/// A 74HC4051 on the bus: its wiring and the level on each Yn input.
#[derive(Debug, Clone)]
pub struct AnalogMux {
    /// system.yaml `external_devices` id. Analog sources name it as their
    /// `connection:`.
    pub id: String,
    /// Where Z goes: an ADC peripheral id, or the id of another mux.
    pub connection: String,
    /// The channel of `connection` that Z drives.
    pub channel: u8,
    /// S0, S1, S2.
    pub select: [MuxPad; SELECT_PINS],
    /// E (active LOW). `None` = E tied to GND.
    pub enable: Option<MuxPad>,
    /// Level on each Yn in millivolts. An input that nothing drives is 0 mV.
    inputs: [u16; CHANNELS],
    /// The last routing pushed downstream (`None` = all switches open).
    /// `Some(..)` in the outer option once the first routing happened.
    routed: Option<Option<usize>>,
    /// Number of times the routed channel changed (evidence for tests).
    switches: u64,
}

impl AnalogMux {
    pub fn new(
        id: impl Into<String>,
        connection: impl Into<String>,
        channel: u8,
        select: [MuxPad; SELECT_PINS],
        enable: Option<MuxPad>,
    ) -> Self {
        Self {
            id: id.into(),
            connection: connection.into(),
            channel,
            select,
            enable,
            inputs: [0; CHANNELS],
            routed: None,
            switches: 0,
        }
    }

    /// Set the level on Yn. Returns false for a channel the part does not have.
    pub fn set_input_mv(&mut self, channel: u8, millivolts: u16) -> bool {
        match self.inputs.get_mut(channel as usize) {
            Some(slot) => {
                *slot = millivolts;
                true
            }
            None => false,
        }
    }

    /// Level on Yn in millivolts.
    pub fn input_mv(&self, channel: usize) -> Option<u16> {
        self.inputs.get(channel).copied()
    }

    /// The channel currently routed to Z (`None`: E is HIGH, or the mux has
    /// not been routed yet).
    pub fn selected(&self) -> Option<usize> {
        self.routed.flatten()
    }

    /// How many times the routed channel changed since attach.
    pub fn switch_count(&self) -> u64 {
        self.switches
    }

    /// Record a routing. Returns the level Z now drives, and whether the
    /// routing differs from the previous one (true for the first routing).
    pub fn route(&mut self, routed: Option<usize>) -> (u16, bool) {
        let changed = self.routed != Some(routed);
        if changed && self.routed.is_some() {
            self.switches += 1;
        }
        self.routed = Some(routed);
        (self.z_mv(), changed)
    }

    /// Level on Z: the routed Yn, or 0 mV with all switches open.
    pub fn z_mv(&self) -> u16 {
        self.selected().map(|ch| self.inputs[ch]).unwrap_or(0)
    }

    /// True when `peripheral` hosts a select or enable pad.
    pub fn watches(&self, peripheral: usize) -> bool {
        self.select.iter().any(|p| p.peripheral == peripheral)
            || self.enable.as_ref().is_some_and(|p| p.peripheral == peripheral)
    }
}

// ─── PeripheralKit registration ────────────────────────────────────────────

/// 74HC4051 kit. Registered as `74hc4051`; `cd74hc4051` and `cd4051` resolve
/// here through the registry alias table (same pinout and function table).
pub struct Hc4051Kit;
pub static HC4051_KIT: Hc4051Kit = Hc4051Kit;

static HC4051_METADATA: KitMetadata = KitMetadata {
    inputs: Cow::Borrowed(&[]),
    device_type: Cow::Borrowed("74hc4051"),
    label: Cow::Borrowed("74HC4051 8-channel analog multiplexer"),
    summary: Cow::Borrowed(
        "Routes one of eight analog inputs (Y0..Y7) to an ADC channel, selected by three GPIO pins.",
    ),
    detail: Cow::Borrowed(
        "NXP/TI 74HC4051 single 8-channel analog multiplexer. S2 S1 S0 (MCU GPIO outputs) \
         select Yn = S2*4 + S1*2 + S0 and connect it to Z; E (active LOW, optional: absent \
         means tied to GND) opens all switches. Z drives the ADC channel named by \
         `connection:` and `channel`. Analog sources (for example a potentiometer) name this \
         mux as their `connection:` and a Y input as their `channel`. The select pads are \
         re-read on every GPIO write, so a conversion started after a select change converts \
         the new channel. Not modelled: ON resistance, switch time, charge injection, Z used \
         as an input. With E HIGH the ADC channel reads 0 mV (Z open, pull-down assumed).",
    ),
    transport: Transport::Analog,
    category: Category::Analog,
    config_keys: Cow::Borrowed(&[
        ConfigKey {
            name: Cow::Borrowed("channel"),
            ty: ConfigType::Int,
            doc: Cow::Borrowed("Channel of the `connection:` ADC (or mux) that Z drives."),
        },
        ConfigKey {
            name: Cow::Borrowed("s0_pin"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed("S0 select pad (chip pad label, for example GPIO_B1_01 or PA0)."),
        },
        ConfigKey {
            name: Cow::Borrowed("s1_pin"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed("S1 select pad."),
        },
        ConfigKey {
            name: Cow::Borrowed("s2_pin"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed("S2 select pad."),
        },
        ConfigKey {
            name: Cow::Borrowed("e_pin"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed(
                "E enable pad, active LOW. Leave it out when E is tied to GND on the board.",
            ),
        },
    ]),
    labs: Cow::Borrowed(&[]),
};

impl PeripheralKit for Hc4051Kit {
    fn metadata(&self) -> &KitMetadata {
        &HC4051_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> Result<()> {
        let id = ctx.device_id().to_string();
        let channel = match ctx.config_i64("channel") {
            Some(c) if (0..=255).contains(&c) => c as u8,
            Some(c) => bail!("74hc4051 '{id}': channel {c} is outside 0..255"),
            None => bail!("74hc4051 '{id}': config `channel` (the ADC channel Z drives) is required"),
        };
        let pad = |ctx: &AttachCtx<'_>, key: &str| -> Result<MuxPad> {
            let label = ctx
                .config_str(key)
                .ok_or_else(|| anyhow!("74hc4051 '{id}': config `{key}` is required"))?;
            resolve_pad(ctx, &id, key, label)
        };
        let select = [pad(ctx, "s0_pin")?, pad(ctx, "s1_pin")?, pad(ctx, "s2_pin")?];
        let enable = match ctx.config_str("e_pin") {
            Some(label) => Some(resolve_pad(ctx, &id, "e_pin", label)?),
            None => None,
        };
        let connection = ctx.connection().to_string();
        let mux = AnalogMux::new(id, connection, channel, select, enable);
        ctx.bus.attach_analog_mux(mux)
    }
}

/// Resolve a pad label to its GPIO peripheral and pin, through the same chip
/// pin map every pin-bound part uses.
fn resolve_pad(ctx: &AttachCtx<'_>, id: &str, key: &str, label: &str) -> Result<MuxPad> {
    let (addr, bit) = ctx.resolve_pin_odr(label).ok_or_else(|| {
        anyhow!("74hc4051 '{id}': {key} '{label}' is not a GPIO pad of this chip")
    })?;
    let peripheral = ctx
        .bus
        .find_peripheral_index(addr)
        .ok_or_else(|| anyhow!("74hc4051 '{id}': {key} '{label}' has no GPIO peripheral"))?;
    if ctx.bus.peripherals[peripheral]
        .dev
        .read_gpio_pad(bit)
        .is_none()
    {
        bail!(
            "74hc4051 '{id}': {key} '{label}': the GPIO model cannot report this pad's level, \
             so the mux could not follow it"
        );
    }
    Ok(MuxPad {
        label: label.to_string(),
        peripheral,
        bit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(bit: u8) -> MuxPad {
        MuxPad {
            label: format!("P{bit}"),
            peripheral: 0,
            bit,
        }
    }

    fn mux(enable: bool) -> AnalogMux {
        AnalogMux::new(
            "m",
            "adc1",
            3,
            [pad(0), pad(1), pad(2)],
            enable.then(|| pad(3)),
        )
    }

    /// Datasheet function table: every S2 S1 S0 combination selects
    /// Yn = S2*4 + S1*2 + S0, with E LOW or E tied to GND.
    #[test]
    fn every_select_code_routes_its_channel() {
        for n in 0..CHANNELS {
            let s = [n & 1 != 0, n & 2 != 0, n & 4 != 0];
            assert_eq!(decode(s, None), Some(n), "E tied low, code {n}");
            assert_eq!(decode(s, Some(false)), Some(n), "E low, code {n}");
        }
    }

    /// E HIGH opens every switch, whatever the select code.
    #[test]
    fn enable_high_opens_every_switch() {
        for n in 0..CHANNELS {
            let s = [n & 1 != 0, n & 2 != 0, n & 4 != 0];
            assert_eq!(decode(s, Some(true)), None, "code {n}");
        }
    }

    /// Z carries exactly the routed input: distinct levels on all eight
    /// inputs, each code reads its own level and no other.
    #[test]
    fn z_follows_the_routed_input() {
        let mut m = mux(true);
        for ch in 0..CHANNELS as u8 {
            assert!(m.set_input_mv(ch, 100 + 300 * ch as u16));
        }
        for n in 0..CHANNELS {
            assert_eq!(m.route(Some(n)).0, 100 + 300 * n as u16);
            assert_eq!(m.selected(), Some(n));
        }
        assert_eq!(m.route(None), (0, true), "E high: Z open reads 0 mV");
        assert_eq!(m.selected(), None);
        assert!(!m.set_input_mv(8, 1), "the part has no Y8");
    }

    #[test]
    fn switch_count_counts_changes_only() {
        let mut m = mux(false);
        assert!(m.route(Some(0)).1, "the first routing is a change");
        assert!(!m.route(Some(0)).1);
        assert_eq!(m.switch_count(), 0);
        m.route(Some(5));
        m.route(None);
        assert_eq!(m.switch_count(), 2);
    }

    #[test]
    fn watches_only_its_own_gpio_peripherals() {
        let mut m = mux(true);
        m.enable.as_mut().unwrap().peripheral = 4;
        assert!(m.watches(0));
        assert!(m.watches(4));
        assert!(!m.watches(1));
    }
}
