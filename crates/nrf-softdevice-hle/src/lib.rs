// SPDX-License-Identifier: MIT
//! nrf-softdevice-hle: an emulator-neutral, clean-room high-level emulation of
//! the Nordic nRF51 SoftDevice API (S110 v8 family), so that an application
//! built for it (micro:bit V1 and Calliope mini MakeCode images) runs in an
//! emulator with the SoftDevice region NEVER loaded, executed or disassembled.
//!
//! See SPEC.md (the contract every backend implements), PROVENANCE.md (where
//! each constant comes from) and AIR.md (the shared virtual air).
//!
//! A backend:
//!   1. maps only the application region (0x18000..0x3C000 on nRF51); reads of
//!      0x0..0x18000 answer erased flash (0xFF) and are never code;
//!   2. vectors exceptions through the application's table (VTOR = app base:
//!      what the MBR and SoftDevice forwarding amounts to);
//!   3. on SVCall entry, reads r0-r3 and the SVC number (the byte at stacked
//!      PC - 2) from the exception frame, calls [`SoftDevice::svc`], writes the
//!      result to the stacked r0 and returns from the exception;
//!   4. calls [`SoftDevice::poll`] every ~1 ms of emulated time.

pub mod aes;
pub mod air;
#[cfg(feature = "capi")]
pub mod capi;
pub mod facts;
pub mod gatt;
pub mod host;
pub mod sd;

pub use air::{Air, AirMsg, MemAirBus, TcpAir};
pub use host::{Host, NvicOp};
pub use sd::{Config, Family, SoftDevice, SvcRecord};
