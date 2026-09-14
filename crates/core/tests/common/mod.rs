// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Fixture loading shared by the `session_*` integration tests.

// Each test binary compiles this module separately and uses a subset of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
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
/// the text its `uart_contains` assertion expects.
///
/// The fixture is the one `examples/ci-fixture-arm/ci/test.sh` runs,
/// `examples/ci/uart-ok.yaml`; firmware path, system and expected UART text all
/// come from that script. Its ELF is built by the core-ci "Build test firmware
/// fixture" step. `None` when it has not been built and the lane does not
/// require it (`LABWIRED_REQUIRE_FIRMWARE`), in which case a notice is printed.
pub fn arm_fixture() -> Option<(
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
