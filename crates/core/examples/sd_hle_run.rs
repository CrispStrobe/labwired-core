// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT
//! Run an nRF51 S110 application region with the SoftDevice emulated at API
//! level (crates/nrf-softdevice-hle).
//!
//!   cargo run --release -p labwired-core --example sd_hle_run -- \
//!       --app app.bin [--system configs/systems/microbit-v1.yaml] \
//!       [--ms 3000] [--air 127.0.0.1:7461] [--node mb1] [--addr C0:EE:AA:BB:CC:01] \
//!       [--trace svc.jsonl] [--press-a-at-ms 1500]
//!
//! The app .bin starts at 0x18000 (tools/nrf-softdevice-hle/appimage.py of
//! renode-spike-prime extracts it from an official .hex, dropping every
//! Nordic byte unread).

use std::io::Write;
use std::sync::{Arc, Mutex};

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::cpu::cortex_m::CortexM;
use labwired_core::machine::AdvanceRequest;
use labwired_core::memory::ProgramImage;
use labwired_core::peripherals::nrf52::uarte::Nrf52Uarte;
use labwired_core::sd_hle::nrf_softdevice_hle::{facts, Config, SoftDevice, TcpAir};
use labwired_core::{Arch, Bus, Cpu, Machine};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

fn main() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sys = arg("--system")
        .map(std::path::PathBuf::from)
        .unwrap_or(root.join("configs/systems/microbit-v1.yaml"));
    let app = std::fs::read(arg("--app").expect("--app app.bin"))?;
    let ms: u64 = arg("--ms").map(|s| s.parse().unwrap()).unwrap_or(2000);
    let node = arg("--node").unwrap_or("microbit".into());
    let addr_s = arg("--addr").unwrap_or("C0:EE:AA:BB:CC:DD".into());
    let press_a: Option<u64> = arg("--press-a-at-ms").map(|s| s.parse().unwrap());

    let mut manifest = SystemManifest::from_file(&sys)?;
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path)?;
    manifest.chip = chip_path.to_str().unwrap().to_string();
    let mut bus = SystemBus::from_config(&chip, &manifest)?;
    let sink = Arc::new(Mutex::new(Vec::<u8>::new()));
    if let Some(p) = bus.peripherals.iter_mut().find(|p| p.name == "uart0") {
        if let Some(u) = p
            .dev
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<Nrf52Uarte>())
        {
            u.set_sink(Some(sink.clone()), false);
        }
    }
    // The Cortex-M system block (NVIC, SCB, SysTick, shared VTOR), as the CLI installs it.
    let (cpu, _nvic) = labwired_core::system::cortex_m::configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    let mut img = ProgramImage::new(0x18000, Arch::Arm);
    img.add_segment(0x18000, app);
    m.load_firmware(&img)?;

    let parts: Vec<u8> = addr_s
        .split(':')
        .map(|p| u8::from_str_radix(p, 16).unwrap())
        .collect();
    let mut a6 = [0u8; 6];
    for i in 0..6 {
        a6[i] = parts[5 - i];
    }
    let mut sd = SoftDevice::new(Config::microbit_v1(&node, a6));
    if let Some(air) = arg("--air") {
        sd.attach_air(Box::new(TcpAir::connect(&air, &node, "labwired-sd-hle")?));
    }
    m.attach_sd_hle(sd, 0x18000)?;
    // Buttons A (P0.17) and B (P0.26) released: the board pulls them up. A low
    // level at reset would put the DAL in Bluetooth pairing mode.
    set_button(&mut m, 17, false);
    set_button(&mut m, 26, false);

    if arg("--trap-hole").is_some() {
        // Single-step until the core fetches below the application base (the
        // Nordic range is never code); print the last 64 PCs and the core state.
        let mut ring = std::collections::VecDeque::new();
        let limit: u64 = arg("--trap-hole").unwrap().parse().unwrap_or(50_000_000);
        let trap_pc: u32 = arg("--trap-pc")
            .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
            .unwrap_or(u32::MAX);
        for _ in 0..limit {
            let pc = m.cpu.get_pc();
            if pc < 0x18000 || pc == trap_pc {
                let sp = m.cpu.sp;
                let mut fr = Vec::new();
                for i in 0..8u64 {
                    fr.push(m.bus.read_u32(sp as u64 + 4 * i).unwrap_or(0));
                }
                eprintln!("[trap] frame at sp: r0 {:#x} r1 {:#x} r2 {:#x} r3 {:#x} r12 {:#x} lr {:#x} pc {:#x} xpsr {:#x}", fr[0], fr[1], fr[2], fr[3], fr[4], fr[5], fr[6], fr[7]);
                eprintln!("[trap-hole] fetch at {pc:#x}; last PCs:");
                for p in &ring {
                    eprint!(" {p:#x}");
                }
                eprintln!(
                    "\n lr={:#x} sp={:#x} ipsr={} r0={:#x} r1={:#x} r2={:#x} r3={:#x}",
                    m.cpu.lr,
                    m.cpu.sp,
                    m.cpu.active_exception,
                    m.cpu.r0,
                    m.cpu.r1,
                    m.cpu.r2,
                    m.cpu.r3
                );
                return Ok(());
            }
            ring.push_back(pc);
            if ring.len() > 64 {
                ring.pop_front();
            }
            m.advance(AdvanceRequest::single())?;
        }
        eprintln!("[trap-hole] no fetch below the app base in {limit} steps");
        return Ok(());
    }
    let hz = m.bus.cpu_hz;
    let per_ms = hz / 1000;
    let t0 = std::time::Instant::now();
    let mut printed = 0usize;
    let mut stop = None;
    for now_ms in 0..ms {
        if Some(now_ms) == press_a {
            // Button A (P0.17) active low: drive the input pin low for 200 ms.
            set_button(&mut m, 17, true);
        }
        if press_a.map(|t| now_ms == t + 200).unwrap_or(false) {
            set_button(&mut m, 17, false);
        }
        let r = m.advance(AdvanceRequest::run(None).with_cycle_limit(per_ms));
        if let Err(e) = r {
            stop = Some(format!("{e}"));
            break;
        }
        let out = sink.lock().unwrap();
        if out.len() > printed {
            std::io::stdout().write_all(&out[printed..])?;
            std::io::stdout().flush()?;
            printed = out.len();
        }
    }
    let wall = t0.elapsed().as_secs_f64();
    let slot = m.sd_hle.as_ref().unwrap();
    eprintln!(
        "\n[sd_hle_run] {} ms emulated in {:.2} s wall ({:.1}x realtime); pc={:#x}; {} SVCs; stop={:?}",
        ms,
        wall,
        ms as f64 / 1000.0 / wall,
        labwired_core::Cpu::get_pc(&m.cpu),
        slot.svc_count,
        stop
    );
    let mut counts = std::collections::BTreeMap::new();
    for r in &slot.sd.trace {
        *counts.entry(r.svc).or_insert(0u32) += 1;
    }
    for (n, c) in &counts {
        eprintln!(
            "  svc {:#04x} {:<40} x{}",
            n,
            facts::svc::name(*n).unwrap_or("?"),
            c
        );
    }
    if let Some(p) = arg("--trace") {
        let mut f = std::fs::File::create(p)?;
        for r in &slot.sd.trace {
            writeln!(
                f,
                r#"{{"n":{},"t_us":{},"svc":{},"name":"{}","r0":{},"r1":{},"r2":{},"r3":{},"ret":{}}}"#,
                r.n,
                r.time_us,
                r.svc,
                facts::svc::name(r.svc).unwrap_or("?"),
                r.args[0],
                r.args[1],
                r.args[2],
                r.args[3],
                r.ret
            )?;
        }
    }
    for l in &slot.sd.log {
        eprintln!("  hle: {l}");
    }
    Ok(())
}

fn set_button(m: &mut Machine<CortexM>, pin: u8, pressed: bool) {
    for p in m.bus.peripherals.iter_mut().filter(|p| p.name == "gpio0") {
        p.dev.set_gpio_input(pin, !pressed);
    }
}
