// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! The `segment_display` primitive through `labwired test`.
//!
//! Real firmware (`tests/fixtures/segment-mux/main.S`) multiplexes "1.23" on
//! a 3-digit 7-segment display on STM32F103 GPIOA, with a one-store ghost at
//! each digit change. The display's `text` log must hold "1.23". Each
//! negative control changes one thing and must fail: a wrong text, a zero
//! persistence threshold (the ghost becomes visible), and the wrong digit
//! select level (the wrong digits light together).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "../../tests/fixtures/segment-mux";

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("labwired-tests")
        .join(labwired_cli::test_support::unique_name(name));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The fixture system with `extra` config lines appended to the display.
fn system(dir: &Path, extra: &str) -> PathBuf {
    let fixture = std::fs::canonicalize(FIXTURE).unwrap();
    let chip = std::fs::canonicalize("../../configs/chips/stm32f103.yaml").unwrap();
    let text = std::fs::read_to_string(fixture.join("system.yaml"))
        .unwrap()
        .replace(
            "../../../configs/chips/stm32f103.yaml",
            &chip.display().to_string(),
        );
    let path = dir.join("system.yaml");
    std::fs::write(&path, format!("{text}{extra}")).unwrap();
    path
}

fn run(dir: &Path, system: &Path, contains: &str) -> Output {
    let firmware = std::fs::canonicalize(format!("{FIXTURE}/segment-mux-thumbv7m.elf")).unwrap();
    let script = dir.join("script.yaml");
    std::fs::write(
        &script,
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: \"{}\"\nlimits:\n  max_steps: 3000000\n\
             assertions:\n  - peripheral_log: {{peripheral: display, log: text, contains: {contains:?}}}\n",
            firmware.display()
        ),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_labwired"))
        .args(["test", "--system"])
        .arg(system)
        .arg("--script")
        .arg(&script)
        .args(["--no-uart-stdout", "--output-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn multiplexed_text_is_read_from_the_display_log() {
    let dir = work_dir("segment-display-pass");
    let out = run(&dir, &system(&dir, ""), "\"1.23\"");
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn a_wrong_text_fails() {
    let dir = work_dir("segment-display-wrong-text");
    let out = run(&dir, &system(&dir, ""), "\"1.24\"");
    assert!(!out.status.success());
    let err = stderr(&out);
    // The failure shows what the display did show.
    assert!(err.contains("has 0 line(s)"), "{err}");
    assert!(err.contains("last line(s):") && err.contains("1.23"), "{err}");
}

#[test]
fn without_the_persistence_threshold_the_ghost_shows() {
    let dir = work_dir("segment-display-no-threshold");
    let sys = system(&dir, "      threshold_pct: 0\n      min_duty_pct: 0\n");
    let out = run(&dir, &sys, "\"1.23\"");
    assert!(!out.status.success(), "the ghost must spoil the text");
    assert!(stderr(&out).contains("has 0 line(s)"), "{}", stderr(&out));
}

#[test]
fn the_wrong_select_level_lights_nothing() {
    let dir = work_dir("segment-display-select-low");
    let text = std::fs::read_to_string(system(&dir, ""))
        .unwrap()
        .replace("digit_active_high: true", "digit_active_high: false");
    let sys = dir.join("select-low.yaml");
    std::fs::write(&sys, text).unwrap();
    let out = run(&dir, &sys, "\"1.23\"");
    assert!(!out.status.success());
    // Inverted selects light the two digits that should be dark, together,
    // as a real display would: the text is garbled, never "1.23".
    assert!(stderr(&out).contains("has 0 line(s)"), "{}", stderr(&out));
}
