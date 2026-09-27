// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! GPIO observers for the `labwired run` CLI.
//!
//! - `TracingGpioObserver` — emits `tracing::info!(target: "gpio", ...)` on
//!   every pin transition. Always installed.
//! - `JsonGpioObserver` — streams one JSON-line record per transition to a
//!   file. Installed when `--gpio-trace <path>` is supplied.

use labwired_core::peripherals::esp32s3::gpio::GpioObserver;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Mutex;

/// Default GPIO observer: emits a `tracing::info!` event for every transition.
#[derive(Debug, Default)]
pub struct TracingGpioObserver;

impl TracingGpioObserver {
    pub fn new() -> Self {
        Self
    }
}

impl GpioObserver for TracingGpioObserver {
    fn on_pin_change(&self, pin: u8, from: bool, to: bool, sim_cycle: u64) {
        tracing::info!(
            target: "gpio",
            "GPIO{}: {}->{}  (cycle={})",
            pin,
            from as u8,
            to as u8,
            sim_cycle,
        );
    }
}

/// Streams `{"sim_cycle":N, "pin":P, "from":B, "to":B}` JSON lines to a file.
///
/// With [`GpioTraceFormat::FourState`] each line also carries
/// `"from_state"`/`"to_state"` (`"0"`, `"1"`, `"z"`, `"x"`), and a line is
/// written for every four-state change — including a pad that went high-Z or
/// into contention without its level moving. `from`/`to` stay the booleans
/// they always were, so a reader of the old format still parses every line.
pub struct JsonGpioObserver {
    writer: Mutex<BufWriter<File>>,
    format: GpioTraceFormat,
}

/// `--gpio-trace-format`: the boolean trace (default, unchanged) or the
/// four-state one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum GpioTraceFormat {
    /// `{"sim_cycle","pin","from","to"}` with boolean levels.
    #[default]
    Bool,
    /// The boolean fields plus `from_state`/`to_state` in `0`/`1`/`z`/`x`.
    #[value(name = "4state")]
    FourState,
}

impl JsonGpioObserver {
    pub fn new(path: &std::path::Path) -> std::io::Result<Self> {
        Self::with_format(path, GpioTraceFormat::Bool)
    }

    pub fn with_format(path: &std::path::Path, format: GpioTraceFormat) -> std::io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
            format,
        })
    }
}

impl std::fmt::Debug for JsonGpioObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JsonGpioObserver")
    }
}

impl GpioObserver for JsonGpioObserver {
    fn on_pin_state_change(
        &self,
        pin: u8,
        from: bool,
        to: bool,
        from_state: labwired_core::logic_capture::PadState,
        to_state: labwired_core::logic_capture::PadState,
        sim_cycle: u64,
    ) {
        if self.format != GpioTraceFormat::FourState {
            return;
        }
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(
                w,
                "{{\"sim_cycle\":{},\"pin\":{},\"from\":{},\"to\":{},\"from_state\":\"{}\",\"to_state\":\"{}\"}}",
                sim_cycle,
                pin,
                from,
                to,
                from_state.as_char(),
                to_state.as_char(),
            );
            let _ = w.flush();
        }
    }

    fn on_pin_change(&self, pin: u8, from: bool, to: bool, sim_cycle: u64) {
        // The four-state format writes from `on_pin_state_change`, which also
        // fires for every level change of a pad — one line per change.
        if self.format == GpioTraceFormat::FourState {
            return;
        }
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(
                w,
                "{{\"sim_cycle\":{},\"pin\":{},\"from\":{},\"to\":{}}}",
                sim_cycle, pin, from, to,
            );
            let _ = w.flush();
        }
    }
}
