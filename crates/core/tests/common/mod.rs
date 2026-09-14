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

/// Everything a fixture script declares: firmware bytes, chip, system
/// manifest (with `chip` rewritten to the absolute descriptor path, as
/// `build_system_bus` does), and the text of its first `uart_contains`.
pub struct Fixture {
    pub fw: Vec<u8>,
    pub chip: labwired_config::ChipDescriptor,
    pub manifest: labwired_config::SystemManifest,
    pub expected: String,
}

/// Load a fixture from its test script (path relative to the repo root).
///
/// Firmware under `target/` is a build output: when it is missing the test
/// skips through the repo's skip helper (or fails, when the lane requires
/// firmware). Firmware anywhere else is a committed blob, so missing it is a
/// broken checkout and fails outright.
pub fn script_fixture(script_rel: &str, firmware_key: &str, build_hint: &str) -> Option<Fixture> {
    let script_path = repo_root().join(script_rel);
    let dir = script_path.parent().unwrap();
    let script = labwired_config::TestScript::from_file(&script_path).unwrap();
    let fw_path = resolve_firmware(dir, &script.inputs.firmware);
    if !fw_path.exists() {
        assert!(
            script.inputs.firmware.contains("target/"),
            "committed fixture firmware {} is missing",
            fw_path.display()
        );
        labwired_core::test_support::skip_or_fail_missing_firmware(
            firmware_key,
            &format!("{script_rel} firmware ({})", fw_path.display()),
            build_hint,
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
    Some(Fixture {
        fw,
        chip,
        manifest,
        expected,
    })
}

/// Load the ARM CI fixture: firmware bytes, chip, system manifest, and the
/// text its `uart_contains` assertion expects.
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
    Some((f.fw, f.chip, f.manifest, f.expected))
}

/// The nRF54L15 smart-ring I²C probe (`examples/nrf54l15-smart-ring/io-smoke.yaml`):
/// a Cortex-M33 firmware that reads the WHO_AM_I of four real I²C device
/// models on TWIM21 and prints each answer. Its ELF is committed
/// (`tests/fixtures/nrf54l15-smart-ring.elf`), so this fixture never skips.
pub fn smart_ring_fixture() -> Fixture {
    script_fixture(
        "examples/nrf54l15-smart-ring/io-smoke.yaml",
        "nrf54l15-smart-ring",
        "committed blob; regenerate with `make publish` in examples/nrf54l15-smart-ring",
    )
    .expect("the smart-ring firmware is committed")
}

/// A chip with no board around it: `configs/chips/<chip>.yaml`, an empty
/// system manifest, and a committed firmware image (`elf`, relative to the
/// repo root) to load. For tests that drive peripherals from the session
/// rather than from firmware.
pub fn bare_chip_fixture(chip: &str, elf: &str) -> Fixture {
    let chip_path = repo_root().join(format!("configs/chips/{chip}.yaml"));
    let manifest: labwired_config::SystemManifest = serde_yaml::from_str(&format!(
        "name: \"{chip}-bare\"\nchip: \"{}\"\nexternal_devices: []\n",
        chip_path.display()
    ))
    .unwrap();
    let chip = labwired_config::ChipDescriptor::from_file(&chip_path).unwrap();
    let fw_path = repo_root().join(elf);
    let fw = std::fs::read(&fw_path)
        .unwrap_or_else(|e| panic!("committed firmware {} is missing: {e}", fw_path.display()));
    Fixture {
        fw,
        chip,
        manifest,
        expected: String::new(),
    }
}
