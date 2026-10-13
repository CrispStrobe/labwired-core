// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! The **`uart_transceiver` primitive** — a part that sits between a UART and a
//! shared line and passes bytes only as its enable pins allow.
//!
//! An RS-485 transceiver (MAX485) is the first user, and any half-duplex
//! line driver with a driver-enable and a receiver-enable pin is the same
//! shape: an RS-422/485 or LIN transceiver, a tri-state bus buffer. The
//! descriptor names the two `config:` keys that carry the pads and the level
//! at which each pin is active; this kit reads those pads, attaches the gate
//! to the hosting UART, and
//! [`crate::peripherals::rs485::Rs485Gate`] does the rest (see that module for
//! the byte-level behaviour and its limits).
//!
//! A `config:` value is a pad label (`PD2`), or `high` / `low` for a pin tied
//! to a rail on the board. A key the placement leaves out is a pin tied to its
//! inactive level for the driver (never drives) and its active level for the
//! receiver (always listens).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use labwired_config::{DeviceDescriptor, EnableLevel, EnablePin};

use crate::peripherals::kit::{AttachCtx, KitMetadata, PeripheralKit};
use crate::peripherals::rs485::{PinSense, Rs485Gate};

/// Static checks for a `uart_transceiver` descriptor.
pub(crate) fn validate_descriptor(desc: &DeviceDescriptor) -> Result<()> {
    anyhow::ensure!(
        desc.behavior.primitive == "uart_transceiver",
        "declarative transceiver kit requires behavior.primitive: uart_transceiver, got '{}'",
        desc.behavior.primitive
    );
    let spec = desc.behavior.transceiver.as_ref().ok_or_else(|| {
        anyhow!(
            "uart_transceiver '{}' declares no `transceiver:` block, so it would gate nothing",
            desc.r#type
        )
    })?;
    anyhow::ensure!(
        spec.driver_enable.is_some() || spec.receiver_enable.is_some(),
        "uart_transceiver '{}' names neither `driver_enable` nor `receiver_enable`",
        desc.r#type
    );
    Ok(())
}

/// A `uart_transceiver` descriptor as a [`PeripheralKit`].
pub struct DeclarativeTransceiverKit {
    descriptor: DeviceDescriptor,
    metadata: KitMetadata,
}

impl std::fmt::Debug for DeclarativeTransceiverKit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclarativeTransceiverKit")
            .field("type", &self.descriptor.r#type)
            .finish()
    }
}

impl DeclarativeTransceiverKit {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let descriptor = DeviceDescriptor::from_yaml(yaml)?;
        validate_descriptor(&descriptor)?;
        let channels = super::declarative_i2c::owned_channels(&descriptor);
        let metadata = super::declarative_i2c::owned_uart_metadata(&descriptor, channels);
        Ok(Self {
            descriptor,
            metadata,
        })
    }
}

/// The sense for one enable pin. `default_enabled` is what an unwired pin reads.
fn sense(
    ctx: &mut AttachCtx<'_>,
    pin: Option<&EnablePin>,
    default_enabled: bool,
) -> Result<PinSense> {
    let Some(pin) = pin else {
        return Ok(PinSense::Const(default_enabled));
    };
    let active_high = pin.active == EnableLevel::High;
    let level = match ctx.config_str(&pin.config).map(str::to_string) {
        // Left out of the placement: tied to the default (enabled/disabled).
        None => return Ok(PinSense::Const(default_enabled)),
        Some(label) => match label.to_ascii_lowercase().as_str() {
            "high" | "vcc" | "1" => PinSense::Const(true),
            "low" | "gnd" | "0" => PinSense::Const(false),
            _ => pad_cell(ctx, &pin.config, &label)?,
        },
    };
    // `PinSense` reads the electrical level; the gate wants "enabled".
    Ok(if active_high {
        level
    } else {
        PinSense::Not(Box::new(level))
    })
}

fn pad_cell(ctx: &mut AttachCtx<'_>, key: &str, label: &str) -> Result<PinSense> {
    let cell = Arc::new(AtomicBool::new(false));
    let (id, ty) = (ctx.device_id().to_string(), ctx.device_type().to_string());
    let (addr, bit) = crate::bus::SystemBus::resolve_pin_idr_pub(ctx.bus, label)
        .ok_or_else(|| anyhow!("{ty} '{id}': {key} pin '{label}' is not a pad on this chip"))?;
    let idx = ctx
        .bus
        .find_peripheral_index(addr)
        .ok_or_else(|| anyhow!("{ty} '{id}': {key} pin '{label}' has no GPIO block"))?;
    if !ctx.bus.peripherals[idx]
        .dev
        .watch_pad_level(bit, cell.clone())
    {
        return Err(anyhow!(
            "{ty} '{id}': this chip's GPIO model cannot report the level of pad '{label}' at \
             the moment of a UART write, which the transceiver needs for {key}"
        ));
    }
    Ok(PinSense::Cell(cell))
}

impl PeripheralKit for DeclarativeTransceiverKit {
    fn metadata(&self) -> &KitMetadata {
        &self.metadata
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> Result<()> {
        let spec = self
            .descriptor
            .behavior
            .transceiver
            .as_ref()
            .ok_or_else(|| anyhow!("uart_transceiver lost its `transceiver:` block"))?;
        // Driver: never drives unless a pad enables it. Receiver: always
        // listens unless a pad disables it.
        let driver = sense(ctx, spec.driver_enable.as_ref(), false)?;
        let receiver = sense(ctx, spec.receiver_enable.as_ref(), true)?;
        let id = ctx.device_id().to_string();
        // The gate stores the receiver as an active-low /RE level.
        ctx.uart()?.set_rs485_gate(Rs485Gate::new(
            id,
            driver,
            PinSense::Not(Box::new(receiver)),
        ));
        Ok(())
    }
}

/// Same bridge the other declarative kits use for the `&'static` registry.
impl PeripheralKit for std::sync::LazyLock<DeclarativeTransceiverKit> {
    fn metadata(&self) -> &KitMetadata {
        std::sync::LazyLock::force(self).metadata()
    }
    fn attach(&self, ctx: &mut AttachCtx<'_>) -> Result<()> {
        std::sync::LazyLock::force(self).attach(ctx)
    }
}
