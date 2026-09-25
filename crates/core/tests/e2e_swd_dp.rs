// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Read the stock nRF52840 SEGGER RTT control block through the SWD-DP.

use std::path::PathBuf;
use std::process::Command;

use labwired_core::bus::SystemBus;
use labwired_core::debug::{SwdAck, SwdTurn, SwdWdata};
use labwired_core::system::cortex_m::{attach_swd_dp, configure_cortex_m};
use labwired_core::{Bus, Machine};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn ensure_firmware_built(root: &std::path::Path) -> PathBuf {
    let bin = labwired_core::test_support::target_dir()
        .join("thumbv7em-none-eabi/release/firmware-nrf52840-rtt");
    let status = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "-p",
            "firmware-nrf52840-rtt",
            "--release",
            "--target",
            "thumbv7em-none-eabi",
        ])
        // See e2e_epaper_tricolor: clear coverage instrumentation flags so the
        // no_std firmware cross-build doesn't fail with E0463 under llvm-cov.
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("execute cargo build");
    assert!(
        status.success(),
        "failed to build firmware-nrf52840-rtt (needs gcc-arm-none-eabi)"
    );
    assert!(bin.exists(), "expected binary at {:?}", bin);
    bin
}

fn swd_header(ap: bool, read: bool, addr: u8) -> u8 {
    let mut h = 1u8;
    if ap {
        h |= 1 << 1;
    }
    if read {
        h |= 1 << 2;
    }
    if addr & 0x4 != 0 {
        h |= 1 << 3;
    }
    if addr & 0x8 != 0 {
        h |= 1 << 4;
    }
    if ((h >> 1) & 0xF).count_ones() & 1 == 1 {
        h |= 1 << 5;
    }
    h |= 1 << 7;
    h
}

#[test]
fn nrf52840_rtt_control_block_first_word_reads_over_swd() {
    let root = repo_root();
    let elf_path = ensure_firmware_built(&root);
    let elf_bytes = std::fs::read(&elf_path).expect("read ELF");

    let chip = labwired_config::ChipDescriptor::from_file(root.join("configs/chips/nrf52840.yaml"))
        .expect("nrf52840 chip");
    let manifest: labwired_config::SystemManifest =
        serde_yaml::from_str("name: swd-demo\nchip: ignored\n").expect("manifest");
    let mut bus = SystemBus::from_config(&chip, &manifest).expect("build bus");
    let (cpu, _nvic) = configure_cortex_m(&mut bus);
    let mut machine = Machine::new(cpu, bus);
    let image = labwired_loader::load_elf(&elf_path).expect("load ELF");
    machine.load_firmware(&image).expect("load firmware");

    let symbol = labwired_loader::resolve_symbol_in_elf(&elf_bytes, "_SEGGER_RTT")
        .expect("firmware ELF must export _SEGGER_RTT");
    let mut dp = attach_swd_dp(&mut machine.bus, &mut machine.cpu, 0x2BA0_1477);

    let mut saw_segg = false;
    for _ in 0..5_000_000u64 {
        machine.step().expect("simulator step");
        let word = machine
            .bus
            .read_u32(u64::from(symbol))
            .expect("read RTT control block");
        if word.to_le_bytes() == [0x53, 0x45, 0x47, 0x47] {
            saw_segg = true;
            break;
        }
    }
    assert!(
        saw_segg,
        "RTT control block at {symbol:#x} never read as 53 45 47 47 within 5_000_000 steps"
    );

    let ctrl = 0x5000_0000u32;
    let turned = dp
        .transact(
            &mut machine.bus,
            swd_header(false, false, 0x4),
            Some(SwdWdata {
                word: ctrl,
                parity: (ctrl.count_ones() & 1) as u8,
            }),
        )
        .expect("CTRL/STAT write");
    assert!(
        matches!(
            turned,
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                ..
            }
        ),
        "CTRL/STAT write {turned:?}"
    );

    let csw = 0x42u32;
    let turned = dp
        .transact(
            &mut machine.bus,
            swd_header(true, false, 0x0),
            Some(SwdWdata {
                word: csw,
                parity: (csw.count_ones() & 1) as u8,
            }),
        )
        .expect("CSW write");
    assert!(
        matches!(
            turned,
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                ..
            }
        ),
        "CSW write {turned:?}"
    );

    let turned = dp
        .transact(
            &mut machine.bus,
            swd_header(true, false, 0x4),
            Some(SwdWdata {
                word: symbol,
                parity: (symbol.count_ones() & 1) as u8,
            }),
        )
        .expect("TAR write");
    assert!(
        matches!(
            turned,
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                ..
            }
        ),
        "TAR write {turned:?}"
    );

    match dp
        .transact(&mut machine.bus, swd_header(true, true, 0xC), None)
        .expect("DRW read")
    {
        SwdTurn::Ack {
            ack: SwdAck::Ok,
            data: Some(0x4747_4553),
            ..
        } => {}
        other => panic!("DRW data phase {other:?}, expected 0x47474553"),
    }
}
