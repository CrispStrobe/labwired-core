// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! **MAX485 RS-485 transceiver** — the kit that wires an
//! [`Rs485Gate`](crate::peripherals::rs485::Rs485Gate) into the UART its DI and
//! RO pins reach.
//!
//! `config:` names the pads on DE and /RE. A pad label (`PD2`, `PA8`) is read
//! through a level cell the GPIO model keeps current. `high` and `low` mean the
//! pin is tied to a rail on the board. A key left out is tied low: DE low is a
//! receive-only node, /RE low is a receiver that always listens.

use std::borrow::Cow;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::peripherals::kit::{
    AttachCtx, Category, ConfigKey, ConfigType, KitMetadata, PeripheralKit, Transport,
};
use crate::peripherals::rs485::{PinSense, Rs485Gate};

pub struct Max485Kit;
pub static MAX485_KIT: Max485Kit = Max485Kit;

static MAX485_METADATA: KitMetadata = KitMetadata {
    inputs: Cow::Borrowed(&[]),
    device_type: Cow::Borrowed("max485"),
    label: Cow::Borrowed("MAX485 RS-485 transceiver"),
    summary: Cow::Borrowed("Half-duplex RS-485 transceiver between a UART and a shared A/B pair."),
    detail: Cow::Borrowed(
        "UART pass-through gated by DE and /RE. A byte the MCU sends reaches the bus only \
         while DE is high; with DE high and /RE low the MCU hears its own frame, as on \
         hardware. A slave byte reaches the MCU only while /RE is low. A slave answering \
         while DE is still high, or two slaves answering together, is reported as bus \
         contention and the colliding bytes are not delivered. Every slave on the A/B pair \
         is one more peer on the UART behind the transceiver. NOT modelled: line levels, \
         termination, bias, reflections and driver turn-around time.",
    ),
    transport: Transport::Uart,
    category: Category::Uart,
    config_keys: Cow::Borrowed(&[
        ConfigKey {
            name: Cow::Borrowed("de"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed(
                "Pad on the driver-enable pin (`PD2`), or `high` / `low` when tied to a rail.",
            ),
        },
        ConfigKey {
            name: Cow::Borrowed("re"),
            ty: ConfigType::Str,
            doc: Cow::Borrowed(
                "Pad on the active-low receiver-enable pin /RE, or `high` / `low` when tied to a rail.",
            ),
        },
    ]),
    labs: Cow::Borrowed(&[]),
};

fn pin_sense(ctx: &mut AttachCtx<'_>, key: &str) -> Result<PinSense> {
    let Some(label) = ctx.config_str(key).map(str::to_string) else {
        return Ok(PinSense::Const(false));
    };
    match label.to_ascii_lowercase().as_str() {
        "high" | "vcc" | "1" => return Ok(PinSense::Const(true)),
        "low" | "gnd" | "0" => return Ok(PinSense::Const(false)),
        _ => {}
    }
    let cell = Arc::new(AtomicBool::new(false));
    let (id, ty) = (ctx.device_id().to_string(), ctx.device_type().to_string());
    let (addr, bit) = crate::bus::SystemBus::resolve_pin_idr_pub(ctx.bus, &label)
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

impl PeripheralKit for Max485Kit {
    fn metadata(&self) -> &KitMetadata {
        &MAX485_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> Result<()> {
        let de = pin_sense(ctx, "de")?;
        let re_n = pin_sense(ctx, "re")?;
        let id = ctx.device_id().to_string();
        ctx.uart()?.set_rs485_gate(Rs485Gate::new(id, de, re_n));
        Ok(())
    }
}
