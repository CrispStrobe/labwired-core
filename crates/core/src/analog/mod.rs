// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! In-core analog engine: a small deterministic MNA transient solver for
//! linear elements and ideal switches, plus the [`crate::cosim::CosimAdapter`]
//! that steps it in lockstep with firmware.
//!
//! It exists because the browser cannot spawn a process, so the ngspice
//! `external_process` adapter has no browser counterpart. This engine compiles
//! into the same WASM the playground ships, runs natively too, and adds no
//! dependency to either. What it does not do — diodes, transistors,
//! subcircuits, model libraries, AC/DC sweeps — it refuses by name and points
//! at `tools/cosim/labwired_ngspice.py`, which does them all.
//!
//! * [`netlist`] — the accepted SPICE subset and its errors.

pub mod netlist;

pub use netlist::{
    parse_netlist, parse_spice_value, AnalogError, Capacitor, Circuit, CurrentSource, Inductor,
    NodeRef, Resistor, Switch, VoltageSource,
};
