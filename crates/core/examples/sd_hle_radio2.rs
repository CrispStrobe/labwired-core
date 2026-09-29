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
    let air = labwired_core::peripherals::nrf52::radio::VirtualAirBus::new();
    let ble_air = labwired_core::peripherals::ble_air::BleAirBus::default();
    let fabric = labwired_core::network::SimMqttFabric::new();
    let mut nodes = Vec::new();
    for (i, f) in [&a[1], &a[2]].iter().enumerate() {
        let sd = SoftDevice::new(Config::microbit_v1(
            &format!("mb{i}"),
            [i as u8 + 1, 0xCC, 0xBB, 0xAA, 0xEE, 0xC0],
        ));
        let (mut m, sink) = labwired_core::sd_hle::build_nrf51_s110(&sys, std::fs::read(f)?, sd)?;
        // One RADIO air for both nodes, each with its own node id: a radio
        // never hears its own frames, so the ids must differ.
        m.bus.attach_lab_air(
            &format!("mb{i}"),
            air.clone(),
            ble_air.clone(),
            fabric.clone(),
        );
        nodes.push((m, sink));
    }
    if std::env::var("SD_HLE_NO_IDLE_FF").is_ok() {
        for (m, _) in nodes.iter_mut() {
            m.config.idle_fast_forward_enabled = false;
        }
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
    let trace = air.trace_snapshot();
    eprintln!(
        "[sd_hle_radio2] {} frames on the air (most recent first):",
        trace.len()
    );
    for f in trace.iter().take(4) {
        eprintln!(
            "  ch {} mode {} base {:#x} prefix {:#x} iv {:#x} bytes {:02x?}",
            f.channel,
            f.mode,
            f.addr_base,
            f.addr_prefix,
            f.whitening_iv,
            &f.bytes[..f.bytes.len().min(24)]
        );
    }
    for (i, (m, _)) in nodes.iter_mut().enumerate() {
        let r = |m: &mut labwired_core::Machine<labwired_core::cpu::cortex_m::CortexM>, o: u64| {
            labwired_core::Bus::read_u32(&m.bus, 0x4000_1000 + o).unwrap_or(0xDEAD)
        };
        eprintln!(
            "[mb{i}] RADIO STATE {} FREQ {} MODE {} BASE0 {:#x} PREFIX0 {:#x} RXADDR {:#x} PCNF0 {:#x} PCNF1 {:#x} CRCCNF {:#x} SHORTS {:#x} INTEN {:#x}",
            r(m, 0x550), r(m, 0x508), r(m, 0x510), r(m, 0x51C), r(m, 0x524), r(m, 0x530), r(m, 0x514), r(m, 0x518), r(m, 0x534), r(m, 0x200), r(m, 0x304)
        );
        let pp = r(m, 0x504);
        let buf: Vec<u8> = (0..16)
            .map(|k| labwired_core::Bus::read_u8(&m.bus, pp as u64 + k).unwrap_or(0))
            .collect();
        eprintln!(
            "[mb{i}] PACKETPTR {:#x} CRCSTATUS {} EVENTS_END {} RXMATCH {} buf {:02x?}",
            pp,
            r(m, 0x400),
            r(m, 0x10C),
            r(m, 0x408),
            buf
        );
    }
    Ok(())
}
