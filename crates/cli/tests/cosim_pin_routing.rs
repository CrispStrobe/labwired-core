// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! `labwired test` steps a manifest's `cosim_models` in lockstep with the
//! firmware, routing pins in both directions.
//!
//! The unit tests in `cosim::routing` cover the grammar, and
//! `crates/core/tests/cosim_pin_routing.rs` covers the routing against a real
//! bus. Neither can catch the thing this file exists for: that the run loop
//! actually CALLS any of it. Before the hook existed, every one of those tests
//! passed while `labwired test` stepped no model at all.
//!
//! The firmware is the committed NUCLEO-F401RE Arduino blink image, which
//! drives PA5 (LD2) — so the sampled input is a level the firmware really
//! produced, not one the test poked in.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Run {
    status: Option<i32>,
    stderr: String,
    result: serde_json::Value,
}

/// Run `labwired test` over a temp-dir manifest + script, with the co-sim
/// routing log turned on.
fn run(name: &str, system_yaml: &str, script_yaml: &str) -> Run {
    let temp_dir = labwired_cli::test_support::unique_temp_dir(&format!("labwired-cosim-{name}"));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let firmware = workspace_root().join("tests/fixtures/stm32f401-blinky.elf");
    assert!(
        firmware.exists(),
        "firmware fixture not found at {firmware:?}"
    );

    std::fs::write(temp_dir.join("system.yaml"), system_yaml).unwrap();
    let script_path = temp_dir.join("script.yaml");
    std::fs::write(
        &script_path,
        script_yaml.replace("__FIRMWARE__", &firmware.display().to_string()),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .arg("test")
        .arg("--script")
        .arg(&script_path)
        .arg("--output-dir")
        .arg(&temp_dir)
        .arg("--no-uart-stdout")
        // The routed values are the evidence; without the `cosim` directive
        // they are filtered out before they reach stderr. `info` keeps the
        // runner's own default level, which a bare `cosim=debug` would drop.
        .env("RUST_LOG", "info,cosim=debug")
        .output()
        .expect("failed to run labwired");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let result_path = temp_dir.join("result.json");
    let result: serde_json::Value = if result_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&result_path).unwrap())
            .expect("parse result.json")
    } else {
        serde_json::Value::Null
    };
    let run = Run {
        status: output.status.code(),
        stderr,
        result,
    };
    let _ = std::fs::remove_dir_all(&temp_dir);
    run
}

/// A model whose static output is routed onto PC13's input register, and whose
/// input is sourced from the LED pad the firmware drives.
const ROUTED_SYSTEM: &str = r#"
name: "cosim-pin-routing"
chip: "stm32f401"
cosim_models:
  - id: "pin_probe"
    adapter: "mock"
    step_ns: 100000
    inputs:
      led: "board.gpio.pa5"
    outputs:
      contact: "board.gpio_in.pc13"
      node_volts: "board.analog.pa0_volts"
    config:
      outputs:
        contact: true
        node_volts: 1.65
external_devices: []
"#;

/// GPIOC IDR on an F401 is base 0x40020800 + 0x10; bit 13 is PC13.
const ROUTED_SCRIPT: &str = r#"
schema_version: "1.2"
inputs:
  firmware: "__FIRMWARE__"
  system: "./system.yaml"
limits:
  max_steps: 1500000
assertions:
  - uart_contains: "LED ON"
  - memory_value:
      address: 0x40020810
      expected_value: 0x2000
      mask: 0x2000
"#;

/// Both directions, through the real `labwired test` loop: the model's output
/// reaches the GPIO input register the firmware would sample, and the pad the
/// firmware drives reaches the model.
#[test]
fn labwired_test_routes_pins_in_both_directions() {
    let run = run("both-ways", ROUTED_SYSTEM, ROUTED_SCRIPT);

    assert_eq!(
        run.result["status"], "pass",
        "run did not pass.\nresult: {}\nstderr: {}",
        run.result, run.stderr
    );
    assert_eq!(run.status, Some(0));

    // Machine → model: the Arduino sketch drives LD2 high in loop(), and that
    // is the level the model was handed.
    assert!(
        run.stderr.contains("board.gpio.pa5 = true"),
        "PA5 high never reached the model.\nstderr: {}",
        run.stderr
    );
    // Model → machine, analog: the routed volts were applied with no error.
    assert!(
        run.stderr.contains("board.analog.pa0_volts = 1.65"),
        "the analog route never fired.\nstderr: {}",
        run.stderr
    );
    assert!(
        !run.stderr.contains("co-sim path"),
        "a routed path failed at runtime.\nstderr: {}",
        run.stderr
    );
}

/// A pad that does not exist on the chip fails the run at startup. A co-sim
/// whose pin never reached the firmware must not be able to print a pass.
#[test]
fn an_unroutable_pad_fails_the_run_as_a_config_error() {
    const BAD_SYSTEM: &str = r#"
name: "cosim-bad-pad"
chip: "stm32f401"
cosim_models:
  - id: "pin_probe"
    adapter: "mock"
    step_ns: 100000
    inputs:
      led: "board.gpio.pz9"
    outputs: {}
    config:
      outputs: {}
external_devices: []
"#;
    const SCRIPT: &str = r#"
schema_version: "1.2"
inputs:
  firmware: "__FIRMWARE__"
  system: "./system.yaml"
limits:
  max_steps: 50000
assertions:
  - uart_contains: "LabWired"
"#;
    let run = run("bad-pad", BAD_SYSTEM, SCRIPT);
    assert_eq!(
        run.status,
        Some(2),
        "expected the config-error exit.\nstderr: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains("pad 'pz9' does not resolve"),
        "the failure must name the pad.\nstderr: {}",
        run.stderr
    );
}

/// A model output routed onto a pad the FIRMWARE drives is a config error, not
/// a write that silently does nothing — the manifest author meant
/// `board.gpio_in.<pad>`.
#[test]
fn driving_a_firmware_output_pad_is_rejected() {
    const BAD_SYSTEM: &str = r#"
name: "cosim-bad-direction"
chip: "stm32f401"
cosim_models:
  - id: "pin_probe"
    adapter: "mock"
    step_ns: 100000
    inputs: {}
    outputs:
      contact: "board.gpio.pc13"
    config:
      outputs:
        contact: true
external_devices: []
"#;
    const SCRIPT: &str = r#"
schema_version: "1.2"
inputs:
  firmware: "__FIRMWARE__"
  system: "./system.yaml"
limits:
  max_steps: 50000
assertions:
  - uart_contains: "LabWired"
"#;
    let run = run("bad-direction", BAD_SYSTEM, SCRIPT);
    assert_eq!(run.status, Some(2), "stderr: {}", run.stderr);
    assert!(
        run.stderr.contains("board.gpio_in.<pad>"),
        "the failure must point at the writable form.\nstderr: {}",
        run.stderr
    );
}

/// The zero-change guarantee: the same firmware and limits, with no
/// `cosim_models:` in the manifest, must retire exactly the same instructions
/// and cycles. Co-simulation is opt-in, and a manifest that does not ask for it
/// must not pay for it — in speed OR in scheduling.
#[test]
fn a_manifest_without_cosim_models_is_unchanged() {
    const PLAIN_SYSTEM: &str = r#"
name: "no-cosim"
chip: "stm32f401"
external_devices: []
"#;
    const SCRIPT: &str = r#"
schema_version: "1.2"
inputs:
  firmware: "__FIRMWARE__"
  system: "./system.yaml"
limits:
  max_steps: 200000
assertions:
  - uart_contains: "LabWired"
"#;
    let first = run("plain-a", PLAIN_SYSTEM, SCRIPT);
    let second = run("plain-b", PLAIN_SYSTEM, SCRIPT);
    assert_eq!(first.result["status"], "pass", "stderr: {}", first.stderr);
    assert_eq!(
        first.result["metrics"]["cycles"], second.result["metrics"]["cycles"],
        "a run with no co-sim models must stay deterministic"
    );
    assert!(
        !first.stderr.contains("co-sim:"),
        "no co-sim machinery should start.\nstderr: {}",
        first.stderr
    );
}

/// `--analog-trace` on `labwired test` records the waveform the co-simulation
/// session produces. The in-core `adapter: analog` model fills a ring that the
/// session publishes on the machine; before that, `test` had no runner attached
/// and wrote a header with no rows.
///
/// The circuit is `examples/cosim-spice-rc/system-analog.yaml`: the blink
/// sketch holds PA5 high (it drives LD2 at cycle 6072 and the blink delay is
/// 500 ms), so across this run the RC node must charge monotonically toward the
/// rail. `v(in)` is a trace channel of that manifest, so the CSV itself says
/// when the pin was high.
#[test]
fn analog_trace_records_the_session_waveform() {
    let temp_dir = labwired_cli::test_support::unique_temp_dir("labwired-cosim-analog-trace");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let root = workspace_root();
    let firmware = root.join("tests/fixtures/stm32f401-blinky.elf");
    let system = root.join("examples/cosim-spice-rc/system-analog.yaml");
    let script_path = temp_dir.join("script.yaml");
    std::fs::write(
        &script_path,
        format!(
            "schema_version: \"1.2\"\ninputs:\n  firmware: \"{}\"\n  system: \"{}\"\n\
             limits:\n  max_steps: 1500000\nassertions:\n  - uart_contains: \"LED ON\"\n",
            firmware.display(),
            system.display()
        ),
    )
    .unwrap();
    let csv_path = temp_dir.join("rc.csv");

    let output = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .arg("test")
        .arg("--script")
        .arg(&script_path)
        .arg("--output-dir")
        .arg(&temp_dir)
        .arg("--no-uart-stdout")
        .arg("--analog-trace")
        .arg(&csv_path)
        .output()
        .expect("failed to run labwired");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        !stderr.contains("so the trace has no channels"),
        "the session's ring was not attached.\nstderr: {stderr}"
    );

    let csv = std::fs::read_to_string(&csv_path).expect("read analog trace CSV");
    let _ = std::fs::remove_dir_all(&temp_dir);
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().expect("CSV header").split(',').collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("no `{name}` column in {header:?}"))
    };
    let (v_out_col, v_in_col) = (column("v_out"), column("v(in)"));
    let rows: Vec<Vec<f64>> = lines
        .map(|line| {
            line.split(',')
                .map(|field| field.parse::<f64>().expect("numeric CSV field"))
                .collect()
        })
        .collect();
    assert!(
        rows.len() > 100,
        "expected more than 100 samples, got {}",
        rows.len()
    );

    // Row 0 is the t = 0 operating point, solved before any routed input
    // applies, so the source still sits at 0 V. From the first model boundary
    // on, the sketch has PA5 high for the rest of this run.
    assert_eq!(rows[0][0], 0.0, "first row is the t = 0 operating point");
    assert_eq!(rows[0][v_in_col], 0.0);
    let high: Vec<&Vec<f64>> = rows.iter().filter(|row| row[v_in_col] > 1.65).collect();
    assert_eq!(
        high.len(),
        rows.len() - 1,
        "PA5 should be high at every boundary after t = 0"
    );
    for pair in high.windows(2) {
        // Backward Euler approaches the rail from below; 1 nV absorbs the last
        // ulp of rounding once the node has settled.
        assert!(
            pair[1][v_out_col] >= pair[0][v_out_col] - 1e-9,
            "v_out fell while PA5 was high: {} -> {}",
            pair[0][v_out_col],
            pair[1][v_out_col]
        );
    }
    let (first, last) = (high[0][v_out_col], high[high.len() - 1][v_out_col]);
    assert!(
        first > 0.0 && first < 1.0,
        "the first high sample should be early in the charge: {first}"
    );
    assert!(
        last > 3.2,
        "the node should have settled near 3.3 V: {last}"
    );
}
