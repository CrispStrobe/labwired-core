// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT
//! GDB server over a micro:bit V1 / Calliope mini S110 application region
//! with the SoftDevice emulated at API level.
//!
//!   cargo run --release -p labwired-gdbstub --example sd_hle_gdb -- app.bin 3334
use labwired_core::sd_hle::nrf_softdevice_hle::{Config, SoftDevice};
use labwired_gdbstub::GdbServer;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let app = std::fs::read(&a[1])?;
    let port: u16 = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(3334);
    let sys = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../configs/systems/microbit-v1.yaml");
    let sd = SoftDevice::new(Config::microbit_v1(
        "mb-gdb",
        [0x01, 0xCC, 0xBB, 0xAA, 0xEE, 0xC0],
    ));
    let (m, _sink) = labwired_core::sd_hle::build_nrf51_s110(&sys, app, sd)?;
    eprintln!("GDB server on 127.0.0.1:{port} (application region 0x18000-0x3FFFF; nothing below 0x18000 is loaded)");
    GdbServer::new(port).run(m)
}
