// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! End-to-end gates for the motor oracle and the `gpio_edges` early stop, on
//! the real `labwired` binary.
//!
//! The H-bridge gate runs `examples/f401-hbridge-dc-motor`: bare-metal STM32F401
//! firmware drives a two-terminal DC motor through an H-bridge channel (IN1/IN2
//! on PA0/PA1, ENA as TIM3 CH1 hardware PWM on PA6), forward then reverse. The
//! motor plant reads only the pads and the timer, so the `motor_speed_reached`
//! assertions pass only if the firmware really commanded both directions.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn require(path: PathBuf) -> PathBuf {
    assert!(path.exists(), "missing fixture: {}", path.display());
    path
}

fn run(root: &Path, script: &Path, out: &Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec![
        "test",
        "--script",
        script.to_str().unwrap(),
        "--no-uart-stdout",
        "--no-key",
        "--output-dir",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    let output = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .current_dir(root)
        .args(&args)
        .output()
        .expect("spawn labwired");
    let result = std::fs::read_to_string(out.join("result.json")).unwrap_or_else(|e| {
        panic!(
            "no result.json ({e}); stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    serde_json::from_str(&result).expect("result.json parses")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lw-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

#[test]
fn hbridge_example_proves_forward_and_reverse_and_stops_early() {
    let root = repo_root();
    let script = require(root.join("examples/f401-hbridge-dc-motor/hbridge.yaml"));
    require(root.join("tests/fixtures/stm32f401-hbridge-motor.elf"));
    let tmp = temp_dir("hbridge");
    let parsed = run(&root, &script, &tmp.join("out"), &[]);

    assert_eq!(parsed["status"], "pass", "{parsed:#}");
    assert_eq!(parsed["stop_reason"], "assertions_passed", "{parsed:#}");
    let steps = parsed["steps_executed"].as_u64().unwrap();
    assert!(steps < 20_000_000, "ran the whole budget: {steps}");

    let motors = parsed["motors"].as_array().expect("motors block");
    assert_eq!(motors.len(), 1);
    let motor = &motors[0];
    assert_eq!(motor["id"], "motor");
    assert_eq!(motor["kind"], "dc");
    assert_eq!(motor["control_state"], "reverse");
    let max = motor["speed_rpm_max"].as_f64().unwrap();
    let min = motor["speed_rpm_min"].as_f64().unwrap();
    let peak = motor["speed_rpm_peak_abs"].as_f64().unwrap();
    assert!(max >= 500.0, "forward leg never reached 500 rpm: {max}");
    assert!(min <= -500.0, "reverse leg never reached -500 rpm: {min}");
    assert_eq!(peak, max.max(-min));
    // 70 % of 12 V on a 0.08 V/(rad/s) motor: no-load speed ~1003 rpm. The
    // forward leg runs long enough to settle there, so the timer duty (not the
    // pad latch, which never moves in AF mode) is what drove it.
    assert!((900.0..1100.0).contains(&max), "forward speed {max}");

    // The unbounded upper bound is not echoed back as 1.8e308.
    let echoed = parsed["assertions"].to_string();
    assert!(!echoed.contains("max_abs_rpm"), "{echoed}");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_run_without_motors_omits_the_motors_block() {
    let root = repo_root();
    let firmware = require(root.join("tests/fixtures/stm32f401-hbridge-motor.elf"));
    let tmp = temp_dir("no-motors");
    let system = tmp.join("system.yaml");
    std::fs::write(
        &system,
        format!(
            "name: no-motors\nchip: \"{}\"\n",
            root.join("configs/chips/stm32f401.yaml").display()
        ),
    )
    .unwrap();
    let script = tmp.join("script.yaml");
    std::fs::write(
        &script,
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: \"{}\"\n  system: \"{}\"\nlimits:\n  max_steps: 10000\nassertions:\n  - expected_stop_reason: max_steps\n",
            firmware.display(),
            system.display()
        ),
    )
    .unwrap();
    let parsed = run(&root, &script, &tmp.join("out"), &[]);
    assert_eq!(parsed["status"], "pass", "{parsed:#}");
    assert!(parsed.get("motors").is_none(), "{parsed:#}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The ESP32-S3 prove shape: serial is not observable there, so `gpio_edges` is
/// the whole oracle, and without a stop condition the run spends its full
/// budget on a verdict already decided. No `--watch-gpio` flag is passed: the
/// assertion must arm its own channel.
#[test]
fn gpio_edges_assertion_stops_an_s3_rom_boot_run() {
    let root = repo_root();
    let flash = require(root.join("tests/fixtures/tier1/esp32s3-flash.bin"));
    let system = require(root.join("configs/systems/esp32s3-zero.yaml"));
    let tmp = temp_dir("gpio-stop");
    let script = tmp.join("gpio_edges_stop.yaml");
    std::fs::write(
        &script,
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: \"\"\n  system: \"{}\"\nlimits:\n  max_steps: 200000000\n  stop_when_assertions_pass: true\n  stop_when_assertions_pass_settle_steps: 100000\n  stop_when_assertions_pass_min_steps: 0\nassertions:\n  - gpio_edges:\n      pin: \"gpio:4\"\n      min_edges: 2\n",
            system.display(),
        ),
    )
    .unwrap();
    let out = tmp.join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .current_dir(&root)
        .env("LABWIRED_ESP32S3_FLASH", &flash)
        .args([
            "test",
            "--script",
            script.to_str().unwrap(),
            "--rom-boot",
            "--no-uart-stdout",
            "--no-key",
            "--output-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn labwired");
    assert!(
        output.status.success(),
        "run failed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("result.json")).unwrap()).unwrap();
    assert_eq!(parsed["stop_reason"], "assertions_passed", "{parsed:#}");
    let steps = parsed["steps_executed"].as_u64().unwrap();
    assert!(steps < 30_000_000, "no early stop: {steps} steps");
    assert!(steps > 100_000, "settle window skipped: {steps} steps");
    let edges: usize = parsed["logic_edges"]["channels"]
        .as_array()
        .expect("auto-armed channel")
        .iter()
        .map(|c| c["transitions"].as_array().map_or(0, Vec::len))
        .sum();
    assert!(
        edges >= 2,
        "expected >= 2 captured transitions, got {edges}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A `gpio_edges` clause that is never satisfied must not pass: the H-bridge
/// firmware moves IN1 (PA0) exactly three times (forward, reverse, brake) and
/// then idles, so asking for four edges runs to the budget and fails.
#[test]
fn gpio_edges_counts_real_transitions_only() {
    let root = repo_root();
    let firmware = require(root.join("tests/fixtures/stm32f401-hbridge-motor.elf"));
    let system = require(root.join("examples/f401-hbridge-dc-motor/system.yaml"));
    let tmp = temp_dir("gpio-count");
    let script = |min: u32| {
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: \"{}\"\n  system: \"{}\"\nlimits:\n  max_steps: 20000000\n  stop_when_assertions_pass: true\nassertions:\n  - gpio_edges:\n      pin: \"gpioa:0\"\n      min_edges: {min}\n",
            firmware.display(),
            system.display()
        )
    };
    let two = tmp.join("two.yaml");
    std::fs::write(&two, script(2)).unwrap();
    let parsed = run(&root, &two, &tmp.join("out-two"), &[]);
    assert_eq!(parsed["status"], "pass", "{parsed:#}");
    assert_eq!(parsed["stop_reason"], "assertions_passed");

    let four = tmp.join("four.yaml");
    std::fs::write(&four, script(4)).unwrap();
    let parsed = run(&root, &four, &tmp.join("out-four"), &[]);
    assert_eq!(parsed["status"], "fail", "{parsed:#}");
    assert_eq!(parsed["stop_reason"], "max_steps");
    let _ = std::fs::remove_dir_all(&tmp);
}
