// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! `build_machine` on a Cortex-M chip boots the ARM CI fixture and its console
//! reaches the UART sink the builder hands back.
//!
//! The fixture is the one `examples/ci-fixture-arm/ci/test.sh` runs:
//! `examples/ci/uart-ok.yaml`. The firmware path, system manifest and expected
//! UART text all come from that script, so this test never hard-codes them.
//!
//! The ELF is built by the core-ci "Build test firmware fixture" step:
//! `cargo build -p firmware-ci-fixture --release --target thumbv6m-none-eabi`.
//! Without it the test skips with a notice, unless `LABWIRED_REQUIRE_FIRMWARE`
//! names `firmware-ci-fixture`, in which case it fails.

use labwired_core::machine::AdvanceRequest;
use labwired_core::system::builder::{
    build_machine, BlobMap, BootMode, BuildOptions, BuildRequest, FirmwareSource,
};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A script's `inputs.firmware` is relative to the script and names
/// `<workspace>/target/...`. The target dir can be relocated
/// (`CARGO_TARGET_DIR`, `[build] target-dir`), so re-root that tail on the real
/// one when the literal path does not exist.
fn resolve_firmware(script_dir: &Path, rel: &str) -> PathBuf {
    let direct = script_dir.join(rel);
    if direct.exists() {
        return direct;
    }
    match rel.split_once("target/") {
        Some((_, tail)) => labwired_core::test_support::target_dir().join(tail),
        None => direct,
    }
}

/// Load the ARM CI fixture: firmware bytes, chip, system manifest (with `chip`
/// rewritten to the absolute descriptor path, as `build_system_bus` does), and
/// the text its `uart_contains` assertion expects. `None` when the firmware has
/// not been built and the lane does not require it.
fn arm_fixture() -> Option<(
    Vec<u8>,
    labwired_config::ChipDescriptor,
    labwired_config::SystemManifest,
    String,
)> {
    let script_path = repo_root().join("examples/ci/uart-ok.yaml");
    let dir = script_path.parent().unwrap();
    let script = labwired_config::TestScript::from_file(&script_path).unwrap();
    let fw_path = resolve_firmware(dir, &script.inputs.firmware);
    if !fw_path.exists() {
        labwired_core::test_support::skip_or_fail_missing_firmware(
            "firmware-ci-fixture",
            &format!("ARM CI fixture ({})", fw_path.display()),
            "cargo build -p firmware-ci-fixture --release --target thumbv6m-none-eabi",
        );
        return None;
    }
    let fw = std::fs::read(&fw_path).unwrap();
    let sys_path = dir.join(
        script
            .inputs
            .system
            .as_deref()
            .expect("fixture declares a system"),
    );
    let mut manifest = labwired_config::SystemManifest::from_file(&sys_path).unwrap();
    let chip_path = sys_path.parent().unwrap().join(&manifest.chip);
    manifest.chip = chip_path.to_string_lossy().into_owned();
    let chip = labwired_config::ChipDescriptor::from_file(&chip_path).unwrap();
    let expected = script
        .assertions
        .iter()
        .find_map(|a| match a {
            labwired_config::TestAssertion::UartContains(u) => Some(u.uart_contains.clone()),
            _ => None,
        })
        .expect("fixture has a uart_contains assertion");
    Some((fw, chip, manifest, expected))
}

#[test]
fn build_machine_arm_boots_fixture_and_prints_expected_uart() {
    let Some((fw, chip, manifest, expected)) = arm_fixture() else {
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
