// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! `build_machine` on a Cortex-M chip boots the ARM CI fixture and its console
//! reaches the UART sink the builder hands back. See `common::arm_fixture` for
//! where the fixture comes from and when this test skips.

mod common;

use labwired_core::machine::AdvanceRequest;
use labwired_core::system::builder::{
    build_machine, BlobMap, BootMode, BuildOptions, BuildRequest, FirmwareSource,
};

#[test]
fn build_machine_arm_boots_fixture_and_prints_expected_uart() {
    let Some((fw, chip, manifest, expected)) = common::arm_fixture() else {
        return;
    };
    let blobs = BlobMap::new();
    let mut built = build_machine(BuildRequest {
        chip: &chip,
        system: &manifest,
        firmware: FirmwareSource::Elf(&fw),
        boot: BootMode::FastBoot,
        blobs: &blobs,
        options: BuildOptions::default(),
    })
    .unwrap();
    for _ in 0..200 {
        built
            .machine
            .advance(AdvanceRequest::run(Some(50_000)))
            .unwrap();
        let text = String::from_utf8_lossy(&built.uart.sink.lock().unwrap()).into_owned();
        if text.contains(&expected) {
            return;
        }
    }
    panic!("fixture never printed {expected:?}");
}
