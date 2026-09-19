// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! ESP32-C6 interrupt-matrix routing through the split INTPRI block.
//!
//! The C6 is the first part in the tree whose matrix enable/priority/threshold
//! registers and `CPU_INTR_FROM_CPU_n` doorbells live OUTSIDE the
//! `INTERRUPT_CORE0` MAP bank (C6: INTPRI @0x600C_5000 + `interrupt_core0`
//! @0x6001_0000; C3: everything in one 0x600C_2000 bank). These tests pin the
//! C6 layout the bus derives from the chip descriptor:
//!
//!   * a C6 bus built from `esp32c6.yaml` arms matrix routing by itself — the
//!     presence of the C6-only INTPRI block is the gate (`routing == true`);
//!   * `CPU_INTR_FROM_CPU_0` (matrix source 22, NOT the C3's 50) routes through
//!     its MAP register to the enabled CPU line, level-sensitively at the MMIO
//!     write choke;
//!   * the enable gate and the priority/threshold gate both really mask, so a
//!     line is not asserted merely because a source is pending.
//!
//! The end-to-end trap (fixture firmware takes the RISC-V trap from the doorbell
//! and clears it in the handler) is proven by
//! `examples/tier1-fixture/esp32c6`; this module pins the bus state it depends
//! on without building a CPU.

use crate::bus::SystemBus;
use crate::Bus;
use labwired_config::{ChipDescriptor, SystemManifest};
use std::path::PathBuf;

const INTERRUPT_CORE0: u64 = 0x6001_0000;
const INTPRI: u64 = 0x600C_5000;

/// `CPU_INTR_FROM_CPU_0` matrix source (`ETS_FROM_CPU_INTR0_SOURCE` in the
/// ESP32-C6 SVD; the C3 numbers the same doorbell 50).
const SOURCE_FROM_CPU_0: u64 = 22;
const LINE: u32 = 9;

const CPU_INT_ENABLE: u64 = 0x00;
const CPU_INT_PRI_BASE: u64 = 0x0C; // CPU_INT_PRI_n @ 0x0C + n*4
const CPU_INT_THRESH: u64 = 0x8C;
const CPU_INTR_FROM_CPU_0: u64 = 0x90;

fn c6_bus() -> SystemBus {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let chip_path = root.join("../../configs/chips/esp32c6.yaml");
    let system_path = root.join("../../configs/systems/esp32c6-devkitc.yaml");
    let chip = ChipDescriptor::from_file(&chip_path).expect("load esp32c6 chip yaml");
    let mut manifest = SystemManifest::from_file(&system_path).expect("load c6 system yaml");
    // Anchor the manifest's chip path to the manifest's directory so
    // `resolve_peripheral_path` finds the chip-relative descriptor files.
    manifest.chip = system_path
        .parent()
        .unwrap()
        .join(&manifest.chip)
        .to_string_lossy()
        .into_owned();
    SystemBus::from_config(&chip, &manifest).expect("build c6 devkit bus")
}

/// Arm the doorbell path: MAP source 22 → `LINE`, enable `LINE`, priority 1.
fn arm_line(bus: &mut SystemBus) {
    bus.write_u32(INTERRUPT_CORE0 + SOURCE_FROM_CPU_0 * 4, LINE)
        .unwrap();
    bus.write_u32(INTPRI + CPU_INT_ENABLE, 1 << LINE).unwrap();
    bus.write_u32(INTPRI + CPU_INT_PRI_BASE + (LINE as u64) * 4, 1)
        .unwrap();
}

#[test]
fn c6_descriptor_arms_matrix_routing_and_routes_the_doorbell() {
    let mut bus = c6_bus();
    assert!(
        bus.irq_fabric.esp32c3.routing,
        "the C6-only INTPRI block must arm matrix routing from the descriptor"
    );

    arm_line(&mut bus);
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines, 0,
        "an idle doorbell must not assert a line"
    );

    // Ring CPU_INTR_FROM_CPU_0: the write choke re-routes source 22 → line 9.
    bus.write_u32(INTPRI + CPU_INTR_FROM_CPU_0, 1).unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines,
        1 << LINE,
        "the doorbell's matrix source (22 on the C6, not 50) must reach the routed line"
    );

    // Acknowledge: clearing the doorbell de-asserts the level.
    bus.write_u32(INTPRI + CPU_INTR_FROM_CPU_0, 0).unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines, 0,
        "clearing the doorbell must de-assert the routed line"
    );
}

#[test]
fn c6_enable_gate_masks_a_pending_doorbell() {
    let mut bus = c6_bus();
    arm_line(&mut bus);

    // Disable the line, then ring the doorbell: nothing may be routed.
    bus.write_u32(INTPRI + CPU_INT_ENABLE, 0).unwrap();
    bus.write_u32(INTPRI + CPU_INTR_FROM_CPU_0, 1).unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines, 0,
        "CPU_INT_ENABLE=0 must mask the pending source"
    );

    // Enabling the line with the doorbell still pending delivers it.
    bus.write_u32(INTPRI + CPU_INT_ENABLE, 1 << LINE).unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines,
        1 << LINE,
        "enabling the line must deliver the already-pending doorbell"
    );
}

#[test]
fn c6_priority_below_threshold_is_masked() {
    let mut bus = c6_bus();
    arm_line(&mut bus);

    // Raise the threshold above the line's priority: the level is held back.
    bus.write_u32(INTPRI + CPU_INT_THRESH, 5).unwrap();
    bus.write_u32(INTPRI + CPU_INTR_FROM_CPU_0, 1).unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines, 0,
        "priority 1 < threshold 5 must mask the line"
    );

    // Raise the priority above the threshold: the same pending source passes.
    bus.write_u32(INTPRI + CPU_INT_PRI_BASE + (LINE as u64) * 4, 6)
        .unwrap();
    assert_eq!(
        bus.irq_fabric.esp32c3.irq_lines,
        1 << LINE,
        "priority 6 >= threshold 5 must route the pending source"
    );
}
