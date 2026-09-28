// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! The Cortex-M fault verdict, end to end through `labwired test`.
//!
//! Each fixture in `tests/fixtures/fault-verdict/` is real firmware (C, built
//! by `build.sh` for the NUCLEO-L476RG) that faults on purpose inside
//! `sensor_read`, called from `main`. The run must put a `fault_verdict` block
//! in `result.json` whose sentence names the fault, the address (only when
//! valid), the PC, the function, the source line and the caller — and print
//! the same sentence on stderr. A clean firmware must produce no block.

use std::path::{Path, PathBuf};
use std::process::Command;

const SYSTEM: &str = "../../configs/systems/nucleo-l476rg.yaml";
const FIXTURES: &str = "../../tests/fixtures/fault-verdict";

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("labwired-tests")
        .join(labwired_cli::test_support::unique_name(name));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run `labwired test` on `firmware`; return (result.json, stderr).
fn run(name: &str, firmware: &Path, stop: &str) -> (serde_json::Value, String) {
    let dir = work_dir(name);
    let firmware = std::fs::canonicalize(firmware).unwrap();
    let script = dir.join("script.yaml");
    std::fs::write(
        &script,
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: \"{}\"\nlimits:\n  max_steps: 5000\n\
             assertions:\n  - expected_stop_reason: {stop}\n",
            firmware.display()
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .args(["test", "--system"])
        .arg(std::fs::canonicalize(SYSTEM).unwrap())
        .arg("--script")
        .arg(&script)
        .args(["--no-uart-stdout", "--output-dir"])
        .arg(dir.join("out"))
        .env_remove("LABWIRED_CORTEXM_FAULTS")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let text = std::fs::read_to_string(dir.join("out/result.json"))
        .unwrap_or_else(|e| panic!("no result.json ({e}); stderr:\n{stderr}"));
    (serde_json::from_str(&text).unwrap(), stderr)
}

fn fixture(kind: &str) -> PathBuf {
    Path::new(FIXTURES).join(format!("fault-{kind}.elf"))
}

/// Run a fixture and check the golden sentence, both in result.json and on
/// stderr. Returns the verdict block for further checks.
fn golden(kind: &str, stop: &str, sentence: &str) -> serde_json::Value {
    let (result, stderr) = run(kind, &fixture(kind), stop);
    let v = result
        .get("fault_verdict")
        .unwrap_or_else(|| panic!("{kind}: no fault_verdict in result.json; stderr:\n{stderr}"))
        .clone();
    assert_eq!(v["summary"], sentence, "{kind}: verdict sentence");
    assert!(
        stderr.contains(&format!("FAULT  {sentence}")),
        "{kind}: stderr must print the verdict; got:\n{stderr}"
    );
    v
}

#[test]
fn forced_hardfault_from_a_precise_busfault() {
    let v = golden(
        "hardfault-forced",
        "max_steps",
        "HardFault escalated from a precise BusFault: data access at 0x3000_0004 (BFAR valid), \
         at PC 0x0800_006C in `sensor_read` (fault_fixture.c:55), called from `main`.",
    );
    assert_eq!(v["kind"], "hard_fault");
    assert_eq!(v["escalated"], true);
    assert_eq!(v["origin"], "bus_fault");
    assert_eq!(v["precise"], true);
    assert_eq!(v["fault_address"], "0x30000004");
    assert_eq!(v["fault_address_source"], "BFAR");
    assert_eq!(v["registers"]["hfsr"], "0x40000000");
    assert_eq!(v["registers"]["cfsr"], "0x00008200");
    assert_eq!(v["registers"]["bfsr"], "0x82");
    assert_eq!(v["location"]["function"], "sensor_read");
    assert_eq!(v["location"]["line"], 55);
    assert_eq!(v["caller"]["function"], "main");
    // The frame was stacked on MSP from Thread mode.
    assert_eq!(v["exc_return"]["value"], "0xFFFFFFF9");
    assert_eq!(v["exc_return"]["return_to"], "thread");
    assert_eq!(v["exc_return"]["stack"], "msp");
    assert_eq!(v["frame"]["pc"], "0x0800006C");
}

#[test]
fn precise_busfault_taken_directly() {
    let v = golden(
        "busfault",
        "max_steps",
        "Precise BusFault: data access at 0x3000_0004 (BFAR valid), at PC 0x0800_006C in \
         `sensor_read` (fault_fixture.c:55), called from `main`.",
    );
    assert_eq!(v["kind"], "bus_fault");
    assert_eq!(v["escalated"], false);
}

#[test]
fn undefined_instruction_escalates_to_hardfault() {
    let v = golden(
        "undef",
        "max_steps",
        "HardFault escalated from a UsageFault: undefined instruction, at PC 0x0800_006E in \
         `sensor_read` (fault_fixture.c:46), called from `main`.",
    );
    assert_eq!(v["origin"], "usage_fault");
    assert!(
        v.get("fault_address").is_none(),
        "UNDEFINSTR has no address"
    );
}

#[test]
fn divide_by_zero_with_div_0_trp() {
    let v = golden(
        "div0",
        "max_steps",
        "UsageFault: divide by zero (CCR.DIV_0_TRP is set), at PC 0x0800_0070 in `sensor_read` \
         (fault_fixture.c:50), called from `main`.",
    );
    assert_eq!(v["kind"], "usage_fault");
    assert_eq!(v["registers"]["ufsr"], "0x0200");
}

#[test]
fn fault_inside_hardfault_handler_is_lockup() {
    let v = golden(
        "lockup",
        "memory_violation",
        "Core LOCKUP: a precise BusFault could not be taken while the HardFault handler was \
         running (double fault): data access at 0x3000_0004 (BFAR valid), at PC 0x0800_0056 in \
         `HardFault_Handler` (fault_fixture.c:85). The core stopped; the first fault (HardFault) \
         was at PC 0x0800_007A in `sensor_read` (fault_fixture.c:55).",
    );
    assert_eq!(v["kind"], "lockup");
}

#[test]
fn clean_firmware_has_no_fault_verdict() {
    let (result, stderr) = run(
        "clean",
        Path::new("../../tests/fixtures/nucleo-l476rg-smoke.elf"),
        "max_steps",
    );
    assert!(
        result.get("fault_verdict").is_none(),
        "a clean run must not report a fault: {}",
        result["fault_verdict"]
    );
    assert!(!stderr.contains("FAULT  "), "{stderr}");
}
