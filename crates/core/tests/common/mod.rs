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

/// A committed repository file: present in every checkout, so a missing one is
/// a broken checkout and fails the test instead of skipping it (a skipped gate
/// reads exactly like a passing one).
pub fn committed(rel: &str) -> Vec<u8> {
    let path = repo_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "committed fixture {} is unreadable ({e}); it is tracked in this repository, \
             so this is a broken checkout, not a missing build",
            path.display()
        )
    })
}

/// Chip and system manifest for a system file, with `chip` rewritten to the
/// absolute descriptor path as `build_system_bus` does.
pub fn system(
    sys_rel: &str,
) -> (
    labwired_config::ChipDescriptor,
    labwired_config::SystemManifest,
) {
    load_system(&repo_root().join(sys_rel))
}

fn load_system(
    sys_path: &Path,
) -> (
    labwired_config::ChipDescriptor,
    labwired_config::SystemManifest,
) {
    let mut manifest = labwired_config::SystemManifest::from_file(sys_path)
        .unwrap_or_else(|e| panic!("system {}: {e:#}", sys_path.display()));
    let chip_path = sys_path.parent().unwrap().join(&manifest.chip);
    manifest.chip = chip_path.to_string_lossy().into_owned();
    let chip = labwired_config::ChipDescriptor::from_file(&chip_path)
        .unwrap_or_else(|e| panic!("chip {}: {e:#}", chip_path.display()));
    (chip, manifest)
}

/// What a `labwired test` script declares: its firmware bytes, chip and system
/// manifest, and every `uart_contains` string, in order.
pub struct ScriptFixture {
    pub firmware: Vec<u8>,
    pub chip: labwired_config::ChipDescriptor,
    pub manifest: labwired_config::SystemManifest,
    pub uart_contains: Vec<String>,
}

/// The board half of a fixture script: chip, system manifest, and every
/// `uart_contains` string, without touching the firmware.
pub fn script_board(
    script_rel: &str,
) -> (
    labwired_config::ChipDescriptor,
    labwired_config::SystemManifest,
    Vec<String>,
) {
    let (script, dir) = read_script(script_rel);
    let sys_path = dir.join(
        script
            .inputs
            .system
            .as_deref()
            .expect("fixture declares a system"),
    );
    let (chip, manifest) = load_system(&sys_path);
    let uart_contains: Vec<String> = script
        .assertions
        .iter()
        .filter_map(|a| match a {
            labwired_config::TestAssertion::UartContains(u) => Some(u.uart_contains.clone()),
            _ => None,
        })
        .collect();
    assert!(
        !uart_contains.is_empty(),
        "{script_rel} has no uart_contains assertion"
    );
    (chip, manifest, uart_contains)
}

fn read_script(script_rel: &str) -> (labwired_config::TestScript, PathBuf) {
    let script_path = repo_root().join(script_rel);
    let script = labwired_config::TestScript::from_file(&script_path)
        .unwrap_or_else(|e| panic!("script {}: {e:#}", script_path.display()));
    (script, script_path.parent().unwrap().to_path_buf())
}

/// Load a fixture script (path relative to the repo root) so a test never
/// hard-codes the firmware, the board, or the expected console text.
///
/// A firmware path under `target/` is a build artifact: when it has not been
/// built, `skip_or_fail_missing_firmware` decides (skip locally, fail when
/// `LABWIRED_REQUIRE_FIRMWARE` names `firmware_key`) and this returns `None`.
/// Any other firmware path is a committed file and must exist.
pub fn script_fixture(
    script_rel: &str,
    firmware_key: &str,
    build_hint: &str,
) -> Option<ScriptFixture> {
    let (script, dir) = read_script(script_rel);
    let fw_path = resolve_firmware(&dir, &script.inputs.firmware);
    if !fw_path.exists() {
        assert!(
            script.inputs.firmware.contains("target/"),
            "{script_rel} names committed firmware {} that is not in this checkout",
            fw_path.display()
        );
        labwired_core::test_support::skip_or_fail_missing_firmware(
            firmware_key,
            &format!("{script_rel} firmware ({})", fw_path.display()),
            build_hint,
        );
        return None;
    }
    let firmware = std::fs::read(&fw_path).unwrap();
    let (chip, manifest, uart_contains) = script_board(script_rel);
    Some(ScriptFixture {
        firmware,
        chip,
        manifest,
        uart_contains,
    })
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
    let f = script_fixture(
        "examples/ci/uart-ok.yaml",
        "firmware-ci-fixture",
        "cargo build -p firmware-ci-fixture --release --target thumbv6m-none-eabi",
    )?;
    let expected = f.uart_contains[0].clone();
    Some((f.firmware, f.chip, f.manifest, expected))
}
