// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Focused tests for the ESP32-C6 GDMA model.
//!
//! These pin the C6 register layout (SVD-sourced, deliberately different from
//! the S3's flat `0xC0` block), the memory-to-memory descriptor walk the
//! tier-1 fixture exercises, owner-bit / `CHECK_OWNER` semantics, and the
//! documented stall-vs-fallback split for peripheral-coupled starts.

use super::*;
use crate::bus::SystemBus;
use crate::Bus;

const IN_CH0_SRC: u32 = 66;

/// Test-only flat RAM window. The C6 model reconstructs descriptor and buffer
/// addresses with the `0x4080_0000` prefix, so tests place everything there.
#[derive(Debug)]
struct TestRam {
    bytes: Vec<u8>,
}

impl TestRam {
    fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }
}

impl crate::Peripheral for TestRam {
    fn read(&self, offset: u64) -> crate::SimResult<u8> {
        self.bytes
            .get(offset as usize)
            .copied()
            .ok_or(crate::SimulationError::MemoryViolation(offset))
    }
    fn write(&mut self, offset: u64, value: u8) -> crate::SimResult<()> {
        match self.bytes.get_mut(offset as usize) {
            Some(b) => {
                *b = value;
                Ok(())
            }
            None => Err(crate::SimulationError::MemoryViolation(offset)),
        }
    }
}

const RAM_BASE: u64 = 0x4080_0000;
/// The C6's full HP SRAM (512 KB), so descriptor/buffer addresses used by the
/// tests all fall inside the test bus window.
const RAM_SIZE: usize = 512 * 1024;

fn bus_with_ram() -> SystemBus {
    let mut bus = SystemBus::new();
    bus.add_peripheral(
        "ram_test",
        RAM_BASE,
        RAM_SIZE as u64,
        None,
        Box::new(TestRam::new(RAM_SIZE)),
    );
    bus
}

// ── offset helpers ────────────────────────────────────────────────────────

fn in_base(ch: u64) -> u64 {
    IN_BLOCK_BASE + ch * CHANNEL_STRIDE
}
fn in_conf0(ch: u64) -> u64 {
    in_base(ch) + 0x00
}
fn in_link(ch: u64) -> u64 {
    in_base(ch) + 0x10
}
fn in_peri_sel(ch: u64) -> u64 {
    in_base(ch) + 0x30
}
fn out_conf1(ch: u64) -> u64 {
    OUT_CONF1_BASE + ch * CHANNEL_STRIDE
}
fn out_link(ch: u64) -> u64 {
    out_conf1(ch) + 0x0C
}
fn out_peri_sel(ch: u64) -> u64 {
    out_conf1(ch) + 0x2C
}
fn out_conf0(ch: u64) -> u64 {
    OUT_CONF0_BASE + ch * CHANNEL_STRIDE
}
fn in_int(ch: u64, reg: u64) -> u64 {
    IN_INT_BASE + ch * INT_CHANNEL_STRIDE + reg
}
fn out_int(ch: u64, reg: u64) -> u64 {
    OUT_INT_BASE + ch * INT_CHANNEL_STRIDE + reg
}

fn write_word(g: &mut Esp32c6Gdma, off: u64, value: u32) {
    g.write_u32(off, value).unwrap();
}

fn read_word(g: &Esp32c6Gdma, off: u64) -> u32 {
    g.read_u32(off).unwrap()
}

/// Write a 3-word descriptor through the bus.
fn write_desc(bus: &mut SystemBus, addr: u64, dw0: u32, buffer: u32, next: u32) {
    bus.write_u32(addr, dw0).unwrap();
    bus.write_u32(addr + 4, buffer).unwrap();
    bus.write_u32(addr + 8, next).unwrap();
}

// ── register map ──────────────────────────────────────────────────────────

#[test]
fn defaults_match_the_svd_reset_values() {
    let g = Esp32c6Gdma::new(IN_CH0_SRC);
    for ch in 0..NUM_CHANNELS as u64 {
        assert_eq!(read_word(&g, in_conf0(ch)), 0, "IN_CONF0 resets 0");
        assert_eq!(
            read_word(&g, in_link(ch)) & LINK_ADDR_MASK,
            0,
            "INLINK_ADDR resets 0"
        );
        assert_ne!(
            read_word(&g, in_link(ch)) & IN_LINK_PARK_BIT,
            0,
            "idle IN channel parks"
        );
        assert_ne!(
            read_word(&g, in_link(ch)) & IN_LINK_AUTO_RET_BIT,
            0,
            "IN_LINK resets with AUTO_RET set (SVD 0x0110_0000)"
        );
        assert_eq!(
            read_word(&g, in_peri_sel(ch)) & PERI_SEL_MASK,
            PERI_SEL_RESET,
            "IN_PERI_SEL resets unbound"
        );
        assert_ne!(
            read_word(&g, out_link(ch)) & OUT_LINK_PARK_BIT,
            0,
            "idle OUT channel parks"
        );
        assert_eq!(
            read_word(&g, out_peri_sel(ch)) & PERI_SEL_MASK,
            PERI_SEL_RESET,
            "OUT_PERI_SEL resets unbound"
        );
        assert_eq!(
            read_word(&g, out_conf0(ch)),
            0x8,
            "OUT_CONF0 resets with OUT_EOF_MODE (SVD 0x8)"
        );
        assert_eq!(read_word(&g, in_int(ch, 0x0)), 0, "IN_INT_RAW resets 0");
        assert_eq!(read_word(&g, out_int(ch, 0x0)), 0, "OUT_INT_RAW resets 0");
    }
    assert_eq!(read_word(&g, MISC_CONF_OFFSET), 0);
}

/// The C6 register map is NOT the S3's uniform `0xC0` stride:
/// `OUT_CONF0_CHn` lives at `0x190 + n*0xC0` while `OUT_CONF1_CHn` lives at
/// `0xD4 + n*0xC0` — so `OUT_CONF0_CH0` (0x190) sits right before
/// `OUT_CONF1_CH1` (0x194). Pin both so a future "simplification" to the S3
/// layout fails loudly.
#[test]
fn out_conf0_and_out_conf1_offsets_are_the_c6_layout() {
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    write_word(&mut g, out_conf1(0), 0x1111_0000);
    write_word(&mut g, out_conf0(0), 0x2222_0000);
    write_word(&mut g, out_conf1(1), 0x3333_0000);
    write_word(&mut g, out_conf0(1), 0x4444_0000);
    write_word(&mut g, out_conf1(2), 0x5555_0000);
    write_word(&mut g, out_conf0(2), 0x6666_0000);
    assert_eq!(read_word(&g, out_conf1(0)), 0x1111_0000);
    assert_eq!(read_word(&g, out_conf0(0)), 0x2222_0000);
    assert_eq!(read_word(&g, out_conf1(1)), 0x3333_0000, "OUT_CONF1_CH1");
    assert_eq!(read_word(&g, out_conf0(1)), 0x4444_0000, "OUT_CONF0_CH1");
    assert_eq!(read_word(&g, out_conf1(2)), 0x5555_0000, "OUT_CONF1_CH2");
    assert_eq!(read_word(&g, out_conf0(2)), 0x6666_0000, "OUT_CONF0_CH2");
    // IN_CONF0/IN_LINK/IN_PERI_SEL stride 0xC0 from 0x70.
    write_word(&mut g, in_conf0(2), 0x7777_0000);
    assert_eq!(read_word(&g, in_conf0(2)), 0x7777_0000);
}

/// The C6 INT registers are flat with a 0x10 stride (not per-channel blocks):
/// IN_INT_* at 0x00/0x04/0x08/0x0C + n*0x10 and OUT_INT_* at
/// 0x30/0x34/0x38/0x3C + n*0x10.
#[test]
fn flat_int_registers_are_per_direction_with_a_0x10_stride() {
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    write_word(&mut g, in_int(0, 0x8), 0xAA);
    write_word(&mut g, in_int(1, 0x8), 0xBB);
    write_word(&mut g, out_int(2, 0x8), 0xCC);
    assert_eq!(read_word(&g, in_int(0, 0x8)), 0xAA, "IN_INT_ENA_CH0");
    assert_eq!(read_word(&g, in_int(1, 0x8)), 0xBB, "IN_INT_ENA_CH1");
    assert_eq!(read_word(&g, out_int(2, 0x8)), 0xCC, "OUT_INT_ENA_CH2");
}

#[test]
fn int_st_is_raw_and_ena_and_int_clr_is_w1c() {
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    // Direct RAW writes are ignored.
    write_word(&mut g, in_int(0, 0x0), 0xFFFF_FFFF);
    assert_eq!(
        read_word(&g, in_int(0, 0x0)),
        0,
        "RAW not directly writable"
    );
    write_word(&mut g, in_int(0, 0x8), 0b11); // ENA
    let mut bus = bus_with_ram();
    arm_m2m(
        &mut g,
        &mut bus,
        0,
        RAM_BASE + 0x1_0000,
        RAM_BASE + 0x2_0000,
        RAM_BASE + 0x3_0000,
        RAM_BASE + 0x4_0000,
        b"z",
    );
    g.tick_with_bus(&mut bus);
    assert_eq!(read_word(&g, in_int(0, 0x4)), 0b11, "IN_INT_ST = RAW & ENA");
    write_word(&mut g, in_int(0, 0xC), 0b01); // CLR
    assert_eq!(read_word(&g, in_int(0, 0x4)), 0b10);
}

// ── M2M descriptor walk ───────────────────────────────────────────────────

/// Arm a single-descriptor M2M transfer on `ch`: `src_data` is written to
/// `src`, the TX descriptor goes to `tx_desc`, the RX descriptor to `rx_desc`,
/// and both links are started with `MEM_TRANS_EN` set and PERI_SEL=Dummy-1.
#[allow(clippy::too_many_arguments)]
fn arm_m2m(
    g: &mut Esp32c6Gdma,
    bus: &mut SystemBus,
    ch: usize,
    src: u64,
    dst: u64,
    tx_desc: u64,
    rx_desc: u64,
    src_data: &[u8],
) {
    for (i, &b) in src_data.iter().enumerate() {
        bus.write_u8(src + i as u64, b).unwrap();
    }
    let len = src_data.len() as u32;
    write_desc(
        bus,
        tx_desc,
        (1 << 31) | (1 << 30) | (len << 12) | len,
        src as u32,
        0,
    );
    write_desc(bus, rx_desc, (1 << 31) | len, dst as u32, 0);

    let ch = ch as u64;
    write_word(g, in_conf0(ch), MEM_TRANS_EN_BIT);
    write_word(g, in_peri_sel(ch), 1); // Dummy-1
    write_word(g, out_peri_sel(ch), 1); // Dummy-1
    write_word(g, in_int(ch, 0xC), 0xFFFF_FFFF); // clear stale
    write_word(g, out_int(ch, 0xC), 0xFFFF_FFFF);
    // Kick both: IN_LINK gets the RX descriptor, OUT_LINK the TX descriptor.
    write_word(g, in_link(ch), (rx_desc as u32) & LINK_ADDR_MASK);
    write_word(g, out_link(ch), (tx_desc as u32) & LINK_ADDR_MASK);
    write_word(
        g,
        in_link(ch),
        ((rx_desc as u32) & LINK_ADDR_MASK) | IN_LINK_START_BIT,
    );
    write_word(
        g,
        out_link(ch),
        ((tx_desc as u32) & LINK_ADDR_MASK) | OUT_LINK_START_BIT,
    );
}

#[test]
fn m2m_single_descriptor_moves_bytes_and_latches_flags() {
    let mut bus = bus_with_ram();
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let src = RAM_BASE + 0x1_0000;
    let dst = RAM_BASE + 0x2_0000;
    let tx_desc = RAM_BASE + 0x3_0000;
    let rx_desc = RAM_BASE + 0x4_0000;
    let data = b"TIER1-GDMA-M2M!\0";

    arm_m2m(&mut g, &mut bus, 0, src, dst, tx_desc, rx_desc, data);

    // The transfer is deferred to tick_with_bus: no EOF yet, and the bus is
    // asked to keep ticking.
    assert_eq!(read_word(&g, in_int(0, 0x0)) & IN_SUC_EOF_BIT, 0);
    assert!(g.needs_bus_tick());

    g.tick_with_bus(&mut bus);

    assert_ne!(
        read_word(&g, in_int(0, 0x0)) & IN_SUC_EOF_BIT,
        0,
        "IN_SUC_EOF"
    );
    assert_ne!(read_word(&g, in_int(0, 0x0)) & IN_DONE_BIT, 0, "IN_DONE");
    assert_ne!(read_word(&g, out_int(0, 0x0)) & OUT_EOF_BIT, 0, "OUT_EOF");
    assert_ne!(
        read_word(&g, out_int(0, 0x0)) & OUT_TOTAL_EOF_BIT,
        0,
        "OUT_TOTAL_EOF"
    );
    assert_ne!(read_word(&g, out_int(0, 0x0)) & OUT_DONE_BIT, 0, "OUT_DONE");
    assert!(!g.needs_bus_tick(), "walk must clear pending");

    for (i, &expected) in data.iter().enumerate() {
        assert_eq!(
            bus.read_u8(dst + i as u64).unwrap(),
            expected,
            "dst[{i}] mismatch"
        );
    }

    // EOF descriptor addresses are recorded.
    assert_eq!(
        read_word(&g, in_base(0) + 0x18),
        rx_desc as u32,
        "IN_SUC_EOF_DES_ADDR"
    );
    assert_eq!(
        read_word(&g, out_conf1(0) + 0x14),
        tx_desc as u32,
        "OUT_EOF_DES_ADDR"
    );

    // RX writeback: owner cleared, length = bytes written.
    let rx_dw0 = bus.read_u32(rx_desc).unwrap();
    assert_eq!(rx_dw0 & (1 << 31), 0, "RX owner must be returned to CPU");
    assert_eq!(
        (rx_dw0 >> 12) & 0xFFF,
        data.len() as u32,
        "RX length field must hold the received byte count"
    );
    // OUT writeback is gated on OUT_AUTO_WRBACK, which resets 0.
    let tx_dw0 = bus.read_u32(tx_desc).unwrap();
    assert_ne!(
        tx_dw0 & (1 << 31),
        0,
        "TX owner stays DMA-owned without OUT_AUTO_WRBACK"
    );
}

#[test]
fn out_auto_wrback_returns_out_descriptors_to_the_cpu() {
    let mut bus = bus_with_ram();
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let src = RAM_BASE + 0x1_0000;
    let dst = RAM_BASE + 0x2_0000;
    let tx_desc = RAM_BASE + 0x3_0000;
    let rx_desc = RAM_BASE + 0x4_0000;
    arm_m2m(&mut g, &mut bus, 0, src, dst, tx_desc, rx_desc, b"auto");
    write_word(&mut g, out_conf0(0), 0x8 | OUT_AUTO_WRBACK_BIT);
    g.tick_with_bus(&mut bus);
    assert_eq!(
        bus.read_u32(tx_desc).unwrap() & (1 << 31),
        0,
        "OUT_AUTO_WRBACK must clear the TX owner"
    );
}

#[test]
fn m2m_two_descriptor_chain_moves_everything() {
    let mut bus = bus_with_ram();
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let src = RAM_BASE + 0x1_0000;
    let dst = RAM_BASE + 0x2_0000;
    let tx1 = RAM_BASE + 0x3_0000;
    let tx2 = RAM_BASE + 0x3_1000;
    let rx = RAM_BASE + 0x4_0000;
    let first: &[u8] = b"AAAA";
    let second: &[u8] = b"BBBBBB";

    for (i, &b) in first.iter().enumerate() {
        bus.write_u8(src + i as u64, b).unwrap();
    }
    for (i, &b) in second.iter().enumerate() {
        bus.write_u8(src + 0x100 + i as u64, b).unwrap();
    }
    // TX chain: tx1 -> tx2 (suc_eof on the last).
    write_desc(
        &mut bus,
        tx1,
        (1 << 31) | (4 << 12) | 4,
        src as u32,
        tx2 as u32,
    );
    write_desc(
        &mut bus,
        tx2,
        (1 << 31) | (1 << 30) | (6 << 12) | 6,
        (src + 0x100) as u32,
        0,
    );
    // RX chain: two descriptors, capacities 4 and 6.
    let rx2 = RAM_BASE + 0x4_1000;
    write_desc(&mut bus, rx, (1 << 31) | 4, dst as u32, rx2 as u32);
    write_desc(&mut bus, rx2, (1 << 31) | 6, (dst + 0x100) as u32, 0);

    let ch = 0u64;
    write_word(&mut g, in_conf0(ch), MEM_TRANS_EN_BIT);
    write_word(&mut g, in_peri_sel(ch), 1);
    write_word(&mut g, out_peri_sel(ch), 1);
    write_word(&mut g, in_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(&mut g, out_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(
        &mut g,
        in_link(ch),
        ((rx as u32) & LINK_ADDR_MASK) | IN_LINK_START_BIT,
    );
    write_word(
        &mut g,
        out_link(ch),
        ((tx1 as u32) & LINK_ADDR_MASK) | OUT_LINK_START_BIT,
    );
    g.tick_with_bus(&mut bus);

    let mut got = Vec::new();
    for i in 0..4u64 {
        got.push(bus.read_u8(dst + i).unwrap());
    }
    for i in 0..6u64 {
        got.push(bus.read_u8(dst + 0x100 + i).unwrap());
    }
    let mut want = first.to_vec();
    want.extend_from_slice(second);
    assert_eq!(got, want);
    // Both RX descriptors were filled and returned to the CPU.
    assert_eq!(bus.read_u32(rx).unwrap() & (1 << 31), 0);
    assert_eq!(bus.read_u32(rx2).unwrap() & (1 << 31), 0);
    assert_eq!(
        read_word(&g, in_base(0) + 0x18),
        rx2 as u32,
        "IN_SUC_EOF_DES_ADDR must be the last RX descriptor"
    );

    // ── second transfer on the same (now CPU-owned) chain with
    // CHECK_OWNER enabled must refuse to walk it.
    write_word(&mut g, in_conf0(ch), MEM_TRANS_EN_BIT | 0);
    write_word(&mut g, in_base(ch) + 0x04, CHECK_OWNER_BIT); // IN_CONF1
    write_word(&mut g, out_conf1(ch), CHECK_OWNER_BIT); // OUT_CONF1 (base 0xD4)
    write_word(&mut g, in_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(&mut g, out_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(
        &mut g,
        in_link(ch),
        ((rx as u32) & LINK_ADDR_MASK) | IN_LINK_START_BIT,
    );
    write_word(
        &mut g,
        out_link(ch),
        ((tx1 as u32) & LINK_ADDR_MASK) | OUT_LINK_START_BIT,
    );
    g.tick_with_bus(&mut bus);
    assert_ne!(
        read_word(&g, in_int(ch, 0x0)) & IN_SUC_EOF_BIT,
        0,
        "completion still latches"
    );
    assert_eq!(bus.read_u32(rx).unwrap() & (1 << 31), 0, "still CPU-owned");
}

/// `CHECK_OWNER` semantics (C6 TRM §3.4.5): with the check enabled, an
/// owner=0 descriptor halts the chain; with it disabled, the walk ignores the
/// owner bit. The S3 twin's walker always stops at owner=0 — this test pins
/// that the C6 model follows the C6 TRM instead.
#[test]
fn check_owner_gates_the_owner_bit_check() {
    let mut bus = bus_with_ram();
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let src = RAM_BASE + 0x1_0000;
    let dst = RAM_BASE + 0x2_0000;
    let tx_desc = RAM_BASE + 0x3_0000;
    let rx_desc = RAM_BASE + 0x4_0000;
    let data = b"ownerbit";
    for (i, &b) in data.iter().enumerate() {
        bus.write_u8(src + i as u64, b).unwrap();
    }
    // TX descriptor owner=0 (CPU-owned), everything else valid.
    write_desc(&mut bus, tx_desc, (1 << 30) | (8 << 12) | 8, src as u32, 0);
    write_desc(&mut bus, rx_desc, (1 << 31) | 8, dst as u32, 0);

    let ch = 0u64;
    write_word(&mut g, in_conf0(ch), MEM_TRANS_EN_BIT);
    write_word(&mut g, in_peri_sel(ch), 1);
    write_word(&mut g, out_peri_sel(ch), 1);
    write_word(&mut g, out_conf1(ch), CHECK_OWNER_BIT); // OUT_CHECK_OWNER
    write_word(
        &mut g,
        in_link(ch),
        (rx_desc as u32 & LINK_ADDR_MASK) | IN_LINK_START_BIT,
    );
    write_word(
        &mut g,
        out_link(ch),
        (tx_desc as u32 & LINK_ADDR_MASK) | OUT_LINK_START_BIT,
    );
    g.tick_with_bus(&mut bus);
    assert_eq!(
        bus.read_u8(dst).unwrap(),
        0,
        "owner=0 + OUT_CHECK_OWNER must halt the TX walk"
    );
    assert_ne!(read_word(&g, in_int(ch, 0x0)) & IN_SUC_EOF_BIT, 0);

    // Now clear CHECK_OWNER: the same owner=0 chain walks on C6 silicon.
    write_word(&mut g, out_conf1(ch), 0);
    // Re-own the RX descriptor (previous completion returned it).
    write_desc(&mut bus, rx_desc, (1 << 31) | 8, dst as u32, 0);
    write_word(&mut g, in_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(&mut g, out_int(ch, 0xC), 0xFFFF_FFFF);
    write_word(
        &mut g,
        in_link(ch),
        (rx_desc as u32 & LINK_ADDR_MASK) | IN_LINK_START_BIT,
    );
    write_word(
        &mut g,
        out_link(ch),
        (tx_desc as u32 & LINK_ADDR_MASK) | OUT_LINK_START_BIT,
    );
    g.tick_with_bus(&mut bus);
    for (i, &expected) in data.iter().enumerate() {
        assert_eq!(
            bus.read_u8(dst + i as u64).unwrap(),
            expected,
            "owner check disabled: byte {i} must move"
        );
    }
}

// ── coupled-start policy ──────────────────────────────────────────────────

/// A START with `MEM_TRANS_EN` clear and a real peripheral bound must STALL
/// visibly: no EOF latches, `needs_bus_tick()` stays true. Auto-completing
/// here would claim byte movement that never happened.
#[test]
fn bound_peripheral_start_stalls_without_a_pump() {
    let mut bus = bus_with_ram();
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let ch = 0u64;
    write_word(&mut g, in_peri_sel(ch), 2); // UHCI0
    write_word(&mut g, out_peri_sel(ch), 2);
    write_word(&mut g, in_link(ch), 0x1234 | IN_LINK_START_BIT);
    write_word(&mut g, out_link(ch), 0x2345 | OUT_LINK_START_BIT);
    assert_eq!(
        read_word(&g, in_int(ch, 0x0)) & IN_SUC_EOF_BIT,
        0,
        "bound coupled start must not auto-complete"
    );
    assert_eq!(read_word(&g, out_int(ch, 0x0)) & OUT_EOF_BIT, 0);
    assert!(
        g.needs_bus_tick(),
        "stalled direction keeps the bus visiting"
    );
    g.tick_with_bus(&mut bus);
    assert_eq!(
        read_word(&g, in_int(ch, 0x0)) & IN_SUC_EOF_BIT,
        0,
        "still stalled after a bus tick"
    );
}

/// A START with an unbound (or Dummy) PERI_SEL takes the fallback
/// auto-complete path: completion flags latch with zero bytes moved, so a
/// firmware that never binds a peripheral cannot hang. The bounded stall
/// test above is what keeps this from becoming a blanket "everything
/// completes" lie.
#[test]
fn unbound_start_auto_completes_with_zero_bytes() {
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    let ch = 0u64;
    // PERI_SEL stays at the unbound reset value.
    write_word(&mut g, in_link(ch), 0x1234 | IN_LINK_START_BIT);
    write_word(&mut g, out_link(ch), 0x2345 | OUT_LINK_START_BIT);
    assert_ne!(read_word(&g, in_int(ch, 0x0)) & IN_SUC_EOF_BIT, 0);
    assert_ne!(read_word(&g, out_int(ch, 0x0)) & OUT_EOF_BIT, 0);
    assert!(!g.needs_bus_tick());
}

// ── interrupt sources ─────────────────────────────────────────────────────

#[test]
fn asserted_interrupts_export_the_c6_matrix_sources() {
    let mut g = Esp32c6Gdma::new(IN_CH0_SRC);
    // No enables → no sources.
    assert!(g.matrix_irq_sources().is_empty());

    // IN channel 1 pending + enabled → source 67.
    write_word(&mut g, in_int(1, 0x8), IN_SUC_EOF_BIT); // ENA
    g.write_u32(in_int(1, 0x0), 0).unwrap(); // RAW write ignored
                                             // Latch raw via a fallback auto-complete on channel 1's IN direction.
    g.write_u32(in_link(1), IN_LINK_START_BIT).unwrap();
    assert_eq!(g.matrix_irq_sources(), vec![IN_CH0_SRC + 1]);

    // OUT channel 2 pending + enabled → source 66 + 3 + 2 = 71.
    write_word(&mut g, out_int(2, 0x8), OUT_EOF_BIT);
    g.write_u32(out_link(2), OUT_LINK_START_BIT).unwrap();
    assert_eq!(g.matrix_irq_sources(), vec![IN_CH0_SRC + 1, IN_CH0_SRC + 5]);
}

#[test]
fn link_addresses_reconstruct_with_the_sram_prefix() {
    // The 20-bit link field composes with 0x4080_0000 (HP SRAM).
    assert_eq!(Esp32c6Gdma::full_desc_addr(0x0001_2340), 0x4081_2340);
    assert_eq!(Esp32c6Gdma::full_desc_addr(0x000F_FFFF), 0x408F_FFFF);
}
