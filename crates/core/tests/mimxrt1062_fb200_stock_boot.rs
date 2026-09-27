// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT
//
// The unmodified FB200 stock firmware (FLAMMA/Mooer, V1.0.1) boots on the
// MIMXRT1062 twin.
//
// The image is vendor firmware and is NOT in this repository. Point
// `LABWIRED_FB200_STOCK_MR` at `fb200-stock.mr` to run this test. Without it
// the test skips — unless `LABWIRED_REQUIRE_FIRMWARE` names `fb200-stock` (or
// `all`), in which case a missing image is a failure, so a lane that claims
// to run it cannot pass without running it.
//
// Boot contract. The i.MX RT boot ROM and the FB200 bootloader at
// 0x6000_0000..0x6000_8000 are not in the `.mr` and are not simulated: the run
// starts where the bootloader hands over, at the application's own vector
// table in FlexSPI flash (SP/PC from 0x6001_0000, i.e. the chip's
// `reset_vector_offset`), exactly as `docs/FIRMWARE_BRINGUP.md` of fb200-tools
// describes. Block 0 is mapped at 0x6001_0000 (container START_PAGE 0x40 x 512
// above 0x6000_8000). Block 1 (the model library) is mapped at 0x600D_0000,
// the base the application's model table is built from (ITCM literal at
// 0x2918); the application reads no model data during boot, so that
// placement is not load-bearing for anything asserted here.
//
// Stages asserted, each against the silicon behaviour the firmware relies on:
//   1. the vendor loader walks its load table and enters ITCM 0x4D6, with the
//      ITCM image equal to the table's source bytes;
//   2. clock bring-up completes through the FSMs (DCDC STS_DC_OK, ARM PLL
//      LOCK, CCM handshake idle) and the core is off the bypass clock;
//   3. the vendor FlexSPI driver identifies the NOR (JEDEC read) and reads
//      its configuration sectors through IP commands;
//   4. LPI2C1 addresses the NAU88L21 at 0x54 and — no codec is attached yet —
//      sees a NACK;
//   5. the LED chain (FlexIO2 SPI, eDMA) receives a WS2812-encoded frame;
//   6. USB1 enumerates as 34DB:800F with the vendor HID interface 3;
//   7. no instruction was left undecoded and no MMIO access was unmapped.

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::bus_trace::{BusPayload, I2cSym};
use labwired_core::bus::SystemBus;
use labwired_core::memory::ProgramImage;
use labwired_core::peripherals::imxrt::flexio::ImxrtFlexio;
use labwired_core::peripherals::imxrt::flexspi::ImxrtFlexspi;
use labwired_core::peripherals::imxrt::usb::ImxrtUsb;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::{AdvanceRequest, BreakpointPolicy, Bus, Cpu, Machine};
use std::path::PathBuf;

const IMAGE_ENV: &str = "LABWIRED_FB200_STOCK_MR";

/// `.mr` container: 128-byte header, then per block a 512-byte tag
/// (`BLOCK_SIZE` at +17) and the payload (fb200-tools FIRMWARE_FORMAT.md).
fn parse_mr(bytes: &[u8]) -> Vec<Vec<u8>> {
    assert_eq!(&bytes[0..9], b"Mooer_TAG", "not a Mooer .mr container");
    let blocks = bytes[47] as usize;
    let mut off = 128;
    let mut out = Vec::new();
    for _ in 0..blocks {
        assert_eq!(&bytes[off..off + 9], b"Mooer_TAG", "block tag");
        let size = u32::from_le_bytes(bytes[off + 17..off + 21].try_into().unwrap()) as usize;
        off += 512;
        out.push(bytes[off..off + size].to_vec());
        off += size;
    }
    out
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn machine(blocks: &[Vec<u8>]) -> Machine<labwired_core::cpu::CortexM> {
    let sys = root().join("configs/systems/fb200.yaml");
    let mut manifest = SystemManifest::from_file(&sys).expect("fb200 system");
    let chip_path = sys.parent().unwrap().join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path).expect("mimxrt1062 chip");
    manifest.chip = chip_path.to_string_lossy().to_string();
    let mut bus = SystemBus::from_config(&chip, &manifest).expect("fb200 bus");
    // Board input the firmware waits on before it starts (see fb200.yaml):
    // 2.5 V on ADC1 channel 9, above the 2870/4095 threshold.
    let adc = bus.find_peripheral_index_by_name("adc1").unwrap();
    assert!(bus.peripherals[adc].dev.set_adc_channel_input(9, 2500));
    let (cpu, _) = configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    let mut img = ProgramImage::new(0x6001_0000, labwired_core::Arch::Arm);
    img.add_segment(0x6001_0000, blocks[0].clone());
    if let Some(models) = blocks.get(1) {
        img.add_segment(0x600D_0000, models.clone());
    }
    m.load_firmware(&img).expect("load");
    m
}

fn run_cycles(m: &mut Machine<labwired_core::cpu::CortexM>, cycles: u64) {
    let mut done = 0u64;
    while done < cycles {
        let req = AdvanceRequest::run(None)
            .with_cycle_limit((cycles - done).min(1_000_000))
            .with_batch_cap(std::num::NonZeroU32::new(1000).unwrap())
            .with_breakpoints(BreakpointPolicy::Ignore);
        let r = m.advance(req).expect("machine advance");
        assert!(r.elapsed_cycles > 0, "machine stopped advancing at pc {:#x}", m.cpu.get_pc());
        done += r.elapsed_cycles;
    }
}

fn dev<'a, T: 'static>(m: &'a mut Machine<labwired_core::cpu::CortexM>, name: &str) -> &'a mut T {
    let idx = m
        .bus
        .find_peripheral_index_by_name(name)
        .unwrap_or_else(|| panic!("no peripheral {name}"));
    m.bus.peripherals[idx]
        .dev
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<T>())
        .unwrap_or_else(|| panic!("{name} is not the expected model"))
}

#[test]
fn fb200_stock_firmware_boots_to_usb_enumeration() {
    let Some(path) = std::env::var_os(IMAGE_ENV) else {
        labwired_core::test_support::skip_or_fail_missing_firmware(
            "fb200-stock",
            "FB200 stock firmware image (fb200-stock.mr)",
            &format!("export {IMAGE_ENV}=/path/to/fb200-stock.mr (vendor image, not redistributable)"),
        );
        return;
    };
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{IMAGE_ENV}={path:?} is set but unreadable: {e}"));
    let blocks = parse_mr(&bytes);
    assert_eq!(blocks[0].len(), 200_704, "block 0 is the V1.0.1 application");
    labwired_core::fidelity::reset();
    let mut m = machine(&blocks);

    // Stage 1: the vendor stub and loader run from flash and enter ITCM.
    assert_eq!(m.cpu.get_pc(), 0x6001_04D8, "reset vector of block 0");
    let mut steps = 0u64;
    while m.cpu.get_pc() != 0x4D6 {
        m.step().expect("loader step");
        steps += 1;
        assert!(steps < 2_000_000, "loader never reached ITCM 0x4D6 (pc {:#x})", m.cpu.get_pc());
    }
    eprintln!("stage 1: ITCM entry 0x4D6 after {steps} instructions");
    // Load-table entry 0: flash 0x600107D4 -> ITCM 0x400, 0x1DBC8 bytes.
    for off in [0u64, 0x1000, 0x1DBC4] {
        let want = u32::from_le_bytes(blocks[0][0x7D4 + off as usize..0x7D8 + off as usize].try_into().unwrap());
        assert_eq!(m.bus.read_u32(0x400 + off).unwrap(), want, "ITCM image at {:#x}", 0x400 + off);
    }

    // Stages 2..6 all happen within the first ~70 M cycles of a cold boot
    // (USB enumeration completes at ~65 M; the first-boot preset format that
    // follows takes > 1 G cycles and is not needed here).
    run_cycles(&mut m, 90_000_000);
    eprintln!("stage 2..6 window ends at pc {:#x}", m.cpu.get_pc());

    // Stage 2: clocks.
    let dcdc_reg0 = m.bus.read_u32(0x4008_0000).unwrap();
    assert_ne!(dcdc_reg0 & (1 << 31), 0, "DCDC STS_DC_OK");
    let pll_arm = m.bus.read_u32(0x400D_8000).unwrap();
    assert_eq!(pll_arm & (1 << 12), 0, "ARM PLL powered");
    assert_ne!(pll_arm & (1 << 31), 0, "ARM PLL locked");
    assert_eq!(m.bus.read_u32(0x400F_C048).unwrap(), 0, "CCM handshake idle");
    let cbcdr = m.bus.read_u32(0x400F_C014).unwrap();
    assert_eq!(cbcdr & (1 << 25), 0, "periph_clk back on the PLL path (PERIPH_CLK_SEL=0)");
    eprintln!("stage 2: PLL_ARM={pll_arm:#010x} CBCDR={cbcdr:#010x} DCDC_REG0={dcdc_reg0:#010x}");

    // Stage 3: FlexSPI IP commands.
    let log = dev::<ImxrtFlexspi>(&mut m, "flexspi").ip_log().to_vec();
    assert!(log.iter().any(|e| e.1 == 0x9F), "JEDEC ID read: {:?}", &log[..log.len().min(8)]);
    assert!(
        log.iter().any(|e| e.1 == 0x6B && e.2 == 0xB_0000),
        "quad read of the 0xB0000 configuration sector"
    );
    eprintln!("stage 3: {} FlexSPI IP commands", log.len());

    // Stage 4: codec address phase on LPI2C1, NACKed.
    let trace = m.bus.bus_trace.snapshot();
    let nack = trace.iter().any(|e| {
        e.bus == "lpi2c1"
            && matches!(
                e.payload,
                BusPayload::I2c { kind: I2cSym::AddrWrite, byte, ack: false } if byte == 0x54 << 1
            )
    });
    assert!(nack, "LPI2C1 START+0x54(W) must be NACKed with no codec attached");

    // Stage 5: one WS2812 frame (40 RGB pixels x 24 bits, one SPI byte per bit).
    let words = dev::<ImxrtFlexio>(&mut m, "flexio2").wire_log();
    assert!(words.len() >= 960, "LED frame on FlexIO2: {} words", words.len());
    assert!(words.iter().all(|w| w.pin == 2 && w.bits == 8));
    assert!(
        words[..960].iter().all(|w| matches!(w.beats[0], 0xC0 | 0xFC)),
        "WS2812 bit encoding (0xC0 = 0, 0xFC = 1)"
    );

    // Stage 6: USB enumeration.
    let host = dev::<ImxrtUsb>(&mut m, "usb1").host_log().clone();
    let dd = &host.device_descriptor;
    assert_eq!(dd.len(), 18, "device descriptor: {host:?}");
    assert_eq!((u16::from_le_bytes([dd[8], dd[9]]), u16::from_le_bytes([dd[10], dd[11]])), (0x34DB, 0x800F));
    assert_eq!(host.address, Some(5));
    let cfg = &host.config_descriptor;
    let mut hid_if3 = false;
    let mut i = 0;
    while i + 1 < cfg.len() && cfg[i] > 0 {
        // Interface descriptor: bInterfaceNumber 3, class 3 (HID).
        if cfg[i + 1] == 4 && cfg[i + 2] == 3 && cfg[i + 5] == 3 {
            hid_if3 = true;
        }
        i += cfg[i] as usize;
    }
    assert!(hid_if3, "vendor HID on interface 3: {cfg:02x?}");
    assert!(host.strings.iter().any(|(_, s)| s == "FB200"), "product string: {:?}", host.strings);
    assert!(
        host.control.iter().any(|(s, _, ok)| s.b_request == 9 && *ok),
        "SET_CONFIGURATION accepted"
    );

    // Stage 7: fidelity.
    let gaps = labwired_core::fidelity::report();
    assert!(gaps.undecoded_instructions.is_empty(), "undecoded: {:?}", gaps.undecoded_instructions);
    assert!(gaps.unmapped_mmio.is_empty(), "unmapped MMIO: {:?}", gaps.unmapped_mmio);
}
