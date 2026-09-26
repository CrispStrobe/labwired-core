// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT
//! Two emulated micro:bit V1s (S110 application regions, SoftDevice HLE) in
//! one process; their nRF RADIO peripherals share labwired's process-global
//! virtual air, so a MakeCode radio program on one is heard by the other.
//!
//!   cargo run --release -p labwired-core --example sd_hle_radio2 -- a.bin b.bin [ms]
use std::io::Write;

use labwired_core::machine::AdvanceRequest;
use labwired_core::sd_hle::nrf_softdevice_hle::{Config, SoftDevice};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ms: u64 = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(5000);
    let sys = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../configs/systems/microbit-v1.yaml");
    let mut nodes = Vec::new();
    for (i, f) in [&a[1], &a[2]].iter().enumerate() {
        let sd = SoftDevice::new(Config::microbit_v1(
            &format!("mb{i}"),
            [i as u8 + 1, 0xCC, 0xBB, 0xAA, 0xEE, 0xC0],
        ));
        nodes.push(labwired_core::sd_hle::build_nrf51_s110(
            &sys,
            std::fs::read(f)?,
            sd,
        )?);
    }
    let per_ms = nodes[0].0.bus.cpu_hz / 1000;
    let mut printed = [0usize; 2];
    let t0 = std::time::Instant::now();
    for _ in 0..ms {
        for (i, (m, sink)) in nodes.iter_mut().enumerate() {
            m.advance(AdvanceRequest::run(None).with_cycle_limit(per_ms))?;
            let out = sink.lock().unwrap();
            if out.len() > printed[i] {
                for line in String::from_utf8_lossy(&out[printed[i]..]).split_inclusive('\n') {
                    print!("[mb{i}] {line}");
                }
                std::io::stdout().flush()?;
                printed[i] = out.len();
            }
        }
    }
    eprintln!(
        "\n[sd_hle_radio2] {ms} ms emulated in {:.1} s",
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}
