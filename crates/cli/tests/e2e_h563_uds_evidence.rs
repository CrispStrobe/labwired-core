// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.
//
// The H563 UDS evidence gate, as executed assertions on the machine-readable
// result. Runs `labwired test --script examples/h563-uds-ecu/uds-evidence.yaml`
// exactly as a prospect would (scripts/uds_evidence.sh), then checks what the
// run PROVED from `result.json`'s `uds` block and `uds-report.md`:
//
// - DiagnosticSessionControl 10 03 -> 50 03;
// - ReadDataByIdentifier F1A0 -> a 603-byte response over multi-frame ISO-TP
//   (FirstFrame, tester FlowControl, ConsecutiveFrames in sequence);
// - a negative response 7F 2E 31 where the script expects one;
// - ECUReset 11 01 -> 51 01, a real reboot (the banner prints again), and a
//   reconnection whose first request goes out AFTER the reboot;
//
// and, as negative controls, that breaking the ISO-TP flow control (an
// Overflow FlowControl, or none at all) turns the same gate red.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn example() -> PathBuf {
    repo_root().join("examples/h563-uds-ecu")
}

fn run(script: &Path, tag: &str) -> (Output, PathBuf) {
    let out = labwired_cli::test_support::unique_temp_dir(tag);
    let _ = std::fs::remove_dir_all(&out);
    let output = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .args(["test", "--script"])
        .arg(script)
        .args(["--no-uart-stdout", "--output-dir"])
        .arg(&out)
        .output()
        .expect("run labwired test");
    (output, out)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read result.json"))
        .expect("parse result.json")
}

fn bytes(v: &Value) -> Vec<u8> {
    v.as_array()
        .map(|a| a.iter().map(|b| b.as_u64().unwrap() as u8).collect())
        .unwrap_or_default()
}

fn exchange<'a>(exchanges: &'a [Value], request: &[u8], nth: usize) -> &'a Value {
    exchanges
        .iter()
        .filter(|e| bytes(&e["request"]) == request)
        .nth(nth)
        .unwrap_or_else(|| panic!("no exchange #{nth} with request {request:02X?}"))
}

#[test]
fn h563_uds_evidence_gate_proves_the_scenario() {
    let (output, out) = run(
        &example().join("uds-evidence.yaml"),
        "labwired-uds-evidence-pass",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the evidence gate must pass\n{stderr}"
    );

    let result = read_json(&out.join("result.json"));
    assert_eq!(result["status"], "pass");
    let tester = &result["uds"]["testers"][0];
    assert_eq!(tester["result"], "done", "tester: {tester:#}");
    let total = tester["steps_total"].as_u64().unwrap();
    assert!(total >= 16, "scenario shrank to {total} steps");
    assert_eq!(tester["steps_passed"].as_u64().unwrap(), total);
    let exchanges = tester["exchanges"].as_array().unwrap();
    assert_eq!(exchanges.len() as u64, total);
    assert!(exchanges.iter().all(|e| e["passed"] == true));

    // DiagnosticSessionControl -> extended.
    let dsc = exchange(exchanges, &[0x10, 0x03], 0);
    assert_eq!(bytes(&dsc["response"])[..2], [0x50, 0x03]);

    // ReadDataByIdentifier F1A0: 603-byte multi-frame response.
    let rdbi = exchange(exchanges, &[0x22, 0xF1, 0xA0], 0);
    let resp = bytes(&rdbi["response"]);
    assert_eq!(resp.len(), 603, "62 F1 A0 + 600 data bytes");
    assert!(
        resp[3..]
            .iter()
            .enumerate()
            .all(|(i, b)| *b == (i & 0xFF) as u8),
        "the 600-byte block reassembled byte-exact"
    );
    assert!(rdbi["response_frames"].as_u64().unwrap() >= 2);

    // The ISO-TP flow on the wire: FF len=603, the tester's CTS FlowControl,
    // then CFs with SN 1, 2, 3, ... in order.
    let frames = result["uds"]["frames"].as_array().unwrap();
    let isotp: Vec<(&str, &str)> = frames
        .iter()
        .map(|f| {
            (
                f["direction"].as_str().unwrap(),
                f["isotp"].as_str().unwrap(),
            )
        })
        .collect();
    let ff = isotp
        .iter()
        .position(|(d, i)| *d == "ecu->tester" && *i == "FF len=603")
        .expect("ECU FirstFrame len=603 on the bus");
    assert_eq!(isotp[ff + 1], ("tester->ecu", "FC CTS BS=0 STmin=0"));
    let cfs: Vec<&str> = isotp[ff + 2..]
        .iter()
        .take_while(|(d, i)| *d == "ecu->tester" && i.starts_with("CF"))
        .map(|(_, i)| *i)
        .collect();
    let want: Vec<String> = (1..=cfs.len())
        .map(|n| format!("CF SN={}", n & 0xF))
        .collect();
    assert!(cfs.len() >= 2, "ConsecutiveFrames after the FC: {cfs:?}");
    assert_eq!(cfs, want);

    // Negative response case: the default-session write is refused.
    let nrc = exchange(exchanges, &[0x2E, 0x01, 0x23, 0xDE, 0xAD, 0xBE, 0xEF], 0);
    assert_eq!(nrc["expected"], "NRC 7F 2E 31");
    assert_eq!(bytes(&nrc["response"]), [0x7F, 0x2E, 0x31]);

    // ECUReset, the reboot, and the reconnection after it.
    let reset = exchange(exchanges, &[0x11, 0x01], 0);
    assert_eq!(bytes(&reset["response"]), [0x51, 0x01]);
    let reset_cycle = reset["response_cycle"].as_u64().unwrap();
    let console = result["uds"]["console"].as_array().unwrap();
    let reboot_ready = console
        .iter()
        .filter(|l| l["text"] == "ECU_READY")
        .map(|l| l["cycle"].as_u64().unwrap())
        .find(|c| *c > reset_cycle)
        .expect("ECU_READY printed again after the 51 01 response (a real reboot)");
    let reconnect = exchange(exchanges, &[0x22, 0xF1, 0x90], 1);
    assert!(
        reconnect["request_cycle"].as_u64().unwrap() > reboot_ready,
        "the post-reset request must go to the REBOOTED ECU"
    );
    assert_eq!(bytes(&reconnect["response"])[..3], [0x62, 0xF1, 0x90]);
    // Back in the default session: the extended-only write is refused again.
    let nrc_again = exchange(exchanges, &[0x2E, 0x01, 0x23, 0xDE, 0xAD, 0xBE, 0xEF], 2);
    assert_eq!(bytes(&nrc_again["response"]), [0x7F, 0x2E, 0x31]);

    // The human report: same verdict, firmware hash, limits.
    let report = std::fs::read_to_string(out.join("uds-report.md")).expect("uds-report.md");
    assert!(report.contains("**Verdict: PASS**"), "{report}");
    let sha = result["firmware_hash"].as_str().unwrap();
    assert!(report.contains(sha), "report names the firmware sha256");
    assert!(report.contains("FF len=603"));
    assert!(report.contains("NegativeResponse WriteDataByIdentifier NRC 0x31"));
    assert!(report.contains("## What is modelled and what is not"));
}

/// Write a copy of the example whose tester answers FirstFrames with
/// `flow_control`, and return the test script that runs it.
fn broken_flow_control_script(flow_control: &str, tag: &str) -> PathBuf {
    let ex = example();
    let dir = labwired_cli::test_support::unique_temp_dir(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let system = std::fs::read_to_string(ex.join("system.yaml")).unwrap();
    let chip = ex
        .join("../../configs/chips/stm32h563.yaml")
        .canonicalize()
        .unwrap();
    let system = system
        .replace(
            "chip: \"../../configs/chips/stm32h563.yaml\"",
            &format!("chip: \"{}\"", chip.display()),
        )
        .replace(
            "      reply_id: 0x7E8\n",
            &format!("      reply_id: 0x7E8\n      flow_control: \"{flow_control}\"\n"),
        );
    assert!(system.contains("flow_control:"), "fault was not injected");
    std::fs::write(dir.join("system.yaml"), system).unwrap();
    let script = std::fs::read_to_string(ex.join("uds-evidence.yaml"))
        .unwrap()
        .replace(
            "./system.yaml",
            &dir.join("system.yaml").display().to_string(),
        )
        .replace(
            "./firmware/h563_uds_ecu.elf",
            &ex.join("firmware/h563_uds_ecu.elf").display().to_string(),
        );
    let path = dir.join("uds-evidence.yaml");
    std::fs::write(&path, script).unwrap();
    path
}

/// Negative controls: break the ISO-TP flow control and the same gate fails,
/// at the multi-frame step, for the right reason, in both outputs.
#[test]
fn h563_uds_evidence_gate_fails_when_flow_control_is_broken() {
    for (fc, tag) in [("32 00 00", "overflow"), ("", "none")] {
        let script = broken_flow_control_script(fc, &format!("labwired-uds-negctl-{tag}"));
        let (output, out) = run(&script, &format!("labwired-uds-negctl-out-{tag}"));
        assert!(
            !output.status.success(),
            "flow_control {fc:?}: the gate must FAIL"
        );
        let result = read_json(&out.join("result.json"));
        assert_eq!(result["status"], "fail", "flow_control {fc:?}");
        let tester = &result["uds"]["testers"][0];
        assert_eq!(tester["result"], "failed");
        let failure = tester["failure"].as_str().unwrap();
        assert!(
            failure.starts_with("step 1:")
                && failure.contains("waiting for the ECU's ConsecutiveFrames"),
            "flow_control {fc:?}: {failure}"
        );
        let step1 = &tester["exchanges"][1];
        assert_eq!(bytes(&step1["request"]), [0x22, 0xF1, 0xA0]);
        assert_eq!(step1["passed"], false);
        let report = std::fs::read_to_string(out.join("uds-report.md")).unwrap();
        assert!(report.contains("**Verdict: FAIL**"), "{report}");
        assert!(report.contains(failure));
    }
}
