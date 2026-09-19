// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! ESP32-C6 (RISC-V HP core) peripheral wiring.
//!
//! The C6 is the C3's successor with a different memory map and a different
//! clock/reset block, but a lot of shared IP: the UART head register map and
//! the GPIO matrix are offset-identical to the C3's. So this module is
//! deliberately thin — it owns the C6-specific *values* (interrupt-matrix
//! source ids, core clock, which model serves each window) and wires the
//! existing models instead of cloning them:
//!
//! | C6 type             | served by                                       |
//! |---------------------|-------------------------------------------------|
//! | `esp32c6_uart`      | [`crate::peripherals::esp_uart::EspUart`] (head map shared with C3/S3) |
//! | `esp32c6_gpio`      | [`crate::peripherals::esp32c3::gpio::Esp32c3Gpio`] (register head shared) |
//! | `declarative` pcr / io_mux / interrupt_core0 / hp_sys | SVD-derived descriptors in `configs/peripherals/esp32c6/` |
//!
//! What is C6-only and NOT modelled at L1 (see `docs/boards/esp32c6-devkitc.md`):
//! the LP core, the C6 UART register tail, IO_MUX electrical enforcement, the
//! PCR clock/reset gates, the interrupt-matrix routing fabric, and every radio.

pub mod factory;
pub mod uart;
