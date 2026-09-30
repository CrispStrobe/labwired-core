// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT
//! Run an nRF51 S110 application region with the SoftDevice emulated at API
//! level (crates/nrf-softdevice-hle).
//!
//!   cargo run --release -p labwired-core --example sd_hle_run -- \
//!       --app app.bin [--system configs/systems/microbit-v1.yaml] \
//!       [--ms 3000] [--air 127.0.0.1:7461] [--node mb1] [--addr C0:EE:AA:BB:CC:01] \
//!       [--trace svc.jsonl] [--press-a-at-ms 1500] [--pair-ms 3000]
//!
//! Diagnostics (all print to stderr; none changes what the program does):
//!   --watch 0xA,0xB        log every arrival at these PCs (a breakpoint that
//!                          does not stop): emulated ms, IPSR, PRIMASK, r0-r2,
//!                          r4, SP, LR and the code-looking words on the stack.
//!                          E.g. the DAL's microbit_panic entry, whose r0 is
//!                          the panic code.
//!   --dump-ram PATH        write the 16 KB of RAM at the end of the run.
//!   --dump-ram-at 0xPC:PATH  write RAM (and r0-r7, SP, LR) the first time a
//!                          watched PC is reached; the PC must be in --watch.
//! The summary ends with the core state (PC, IPSR, PRIMASK, NVIC pending and
//! enabled) and the 96 words above SP, so a HardFault's stacked frame can be
//! read off it.
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
    // Hold A+B from reset for this many ms: the DAL's Bluetooth pairing mode.
    let pair_ms: Option<u64> = arg("--pair-ms").map(|s| s.parse().unwrap());

    let mut manifest = SystemManifest::from_file(&sys)?;
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path)?;
    manifest.chip = chip_path.to_str().unwrap().to_string();
    let mut bus = SystemBus::from_config(&chip, &manifest)?;
    let sink = Arc::new(Mutex::new(Vec::<u8>::new()));
    bus.attach_uart_tx_sink(sink.clone(), false);
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
    set_button(&mut m, 17, pair_ms.is_some());
    set_button(&mut m, 26, pair_ms.is_some());

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
    // --watch 0xA,0xB: log (not stop) every time execution reaches these PCs.
    let watch: Vec<u32> = arg("--watch")
        .map(|w| {
            w.split(',')
                .map(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap())
                .collect()
        })
        .unwrap_or_default();
    for w in &watch {
        m.breakpoints.insert(*w & !1);
    }
    let mut watch_hits: std::collections::BTreeMap<u32, u32> = Default::default();
    let hz = m.bus.cpu_hz;
    let per_ms = hz / 1000;
    let t0 = std::time::Instant::now();
    let mut printed = 0usize;
    let mut stop = None;
    for now_ms in 0..ms {
        if pair_ms == Some(now_ms) {
            set_button(&mut m, 17, false);
            set_button(&mut m, 26, false);
        }
        if Some(now_ms) == press_a {
            // Button A (P0.17) active low: drive the input pin low for 200 ms.
            set_button(&mut m, 17, true);
        }
        if press_a.map(|t| now_ms == t + 200).unwrap_or(false) {
            set_button(&mut m, 17, false);
        }
        let target = m.total_cycles + per_ms;
        let mut failed = false;
        while m.total_cycles < target {
            let left = target - m.total_cycles;
            let req = AdvanceRequest::run(None).with_cycle_limit(left);
            let req = if watch.is_empty() {
                req
            } else {
                req.with_breakpoints(labwired_core::machine::BreakpointPolicy::Honor)
            };
            match m.advance(req) {
                Err(e) => {
                    stop = Some(format!("{e}"));
                    failed = true;
                    break;
                }
                Ok(rep) => {
                    if let labwired_core::machine::AdvanceStop::Breakpoint(pc) = rep.stop {
                        let n = watch_hits.entry(pc).or_insert(0);
                        *n += 1;
                        if *n <= 20 {
                            let stack: Vec<String> = (0..24u64)
                                .filter_map(|k| {
                                    labwired_core::Bus::read_u32(&m.bus, m.cpu.sp as u64 + 4 * k)
                                        .ok()
                                })
                                .filter(|w| (0x18001..0x3C000).contains(w) && w & 1 == 1)
                                .map(|w| format!("{w:#x}"))
                                .collect();
                            eprintln!("[watch] {:#x} at {} ms (hit {}) ipsr={} primask={} r0={:#x} r1={:#x} r2={:#x} r4={:#x} sp={:#x} lr={:#x} stack-code-words {:?}", pc, now_ms, n, m.cpu.active_exception, m.cpu.primask, m.cpu.r0, m.cpu.r1, m.cpu.r2, m.cpu.r4, m.cpu.sp, m.cpu.lr, stack);
                        }
                        if let Some(spec) = arg("--dump-ram-at") {
                            let (at, path) = spec.split_once(':').unwrap();
                            if u32::from_str_radix(at.trim_start_matches("0x"), 16).unwrap() & !1
                                == pc
                                && *n == 1
                            {
                                let ram: Vec<u8> = (0..0x4000u64)
                                    .map(|k| {
                                        labwired_core::Bus::read_u8(&m.bus, 0x2000_0000 + k)
                                            .unwrap_or(0)
                                    })
                                    .collect();
                                std::fs::write(path, ram)?;
                                eprintln!("[dump-ram-at {pc:#x}] r0={:#x} r1={:#x} r2={:#x} r3={:#x} r4={:#x} r5={:#x} r6={:#x} r7={:#x} sp={:#x} lr={:#x}",
                                    m.cpu.r0, m.cpu.r1, m.cpu.r2, m.cpu.r3, m.cpu.r4, m.cpu.r5, m.cpu.r6, m.cpu.r7, m.cpu.sp, m.cpu.lr);
                            }
                        }
                        // Step past it so the breakpoint does not re-fire at once.
                        let _ = m.advance(AdvanceRequest::single());
                    } else {
                        break;
                    }
                }
            }
        }
        if failed {
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
    eprintln!(
        "  core: pc={:#x} ipsr={} primask={} lr={:#x} sp={:#x} ispr0={:#x} iser0={:#x}",
        m.cpu.get_pc(),
        m.cpu.active_exception,
        m.cpu.primask,
        m.cpu.lr,
        m.cpu.sp,
        labwired_core::Bus::read_u32(&m.bus, 0xE000_E200).unwrap_or(0),
        labwired_core::Bus::read_u32(&m.bus, 0xE000_E100).unwrap_or(0)
    );
    if let Some(path) = arg("--dump-ram") {
        let ram: Vec<u8> = (0..0x4000u64)
            .map(|k| labwired_core::Bus::read_u8(&m.bus, 0x2000_0000 + k).unwrap_or(0))
            .collect();
        std::fs::write(&path, ram)?;
    }
    let words: Vec<String> = (0..96u64)
        .map(|k| {
            format!(
                "{:08x}",
                labwired_core::Bus::read_u32(&m.bus, m.cpu.sp as u64 + 4 * k)
                    .unwrap_or(0xDEAD_BEEF)
            )
        })
        .collect();
    eprintln!("  stack @sp: {}", words.join(" "));
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
    for (pc, n) in &watch_hits {
        eprintln!("  watch {pc:#x}: {n} hits");
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
