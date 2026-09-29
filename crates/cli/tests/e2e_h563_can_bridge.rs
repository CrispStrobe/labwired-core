// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! The CAN bridge on real firmware: the committed STM32H563 FDCAN/UDS ECU
//! (`examples/h563-uds-ecu/firmware/h563_uds_ecu.elf`, UDSLib v2.0.0) and the
//! scripted UDS tester, through `labwired test` and through the machine API.
//!
//! * pause modes: DROP records what it dropped and the ECU never sees it;
//!   CAPTURE delivers after resume and the ECU answers; REPLAY ignores live
//!   frames;
//! * record -> replay: a recorded UDS session replays twice with identical
//!   results, and a tampered recording is caught (negative control);
//! * failure scenarios: node reset in the middle of a multi-frame ISO-TP
//!   transfer (timeout -> retry -> recovery), a missing reply, a delayed
//!   reply, and bus-off, each with the control that shows the check bites.

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::network::can_bridge::{
    CanBridge, CanBridgeConfig, CanOfferOutcome, CanOverflowPolicy, CanPauseMode,
};
use labwired_core::network::CanFrame;
use labwired_core::Machine;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

const ELF: &str = "examples/h563-uds-ecu/firmware/h563_uds_ecu.elf";
const EXAMPLE: &str = "examples/h563-can-replay";

fn out_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lw-canbridge-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Run `labwired test` on a committed script. Returns (exit code, result.json).
fn run_committed(script: &str, tag: &str) -> (i32, Value, PathBuf) {
    let out = out_dir(tag);
    let status = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .current_dir(root())
        .args(["test", "--script", script, "--output-dir"])
        .arg(&out)
        .output()
        .unwrap();
    let result: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("result.json")).unwrap()).unwrap();
    (status.status.code().unwrap_or(-1), result, out)
}

/// Run `labwired test` on a system manifest written from `system_yaml` (its
/// `chip:` rewritten to the absolute descriptor path) with `assertions`.
fn run_variant(tag: &str, system_yaml: &str, assertions: &str) -> (i32, Value) {
    let dir = out_dir(tag);
    let chip = root().join("configs/chips/stm32h563.yaml");
    let system = system_yaml.replace(
        "\"../../configs/chips/stm32h563.yaml\"",
        &format!("{:?}", chip.to_string_lossy()),
    );
    std::fs::write(dir.join("system.yaml"), system).unwrap();
    let script = format!(
        "schema_version: \"1.0\"\ninputs:\n  system: \"./system.yaml\"\n  firmware: {:?}\n\
         limits:\n  max_steps: 2000000\nassertions:\n{assertions}",
        root().join(ELF).to_string_lossy()
    );
    std::fs::write(dir.join("script.yaml"), script).unwrap();
    let out = dir.join("out");
    let status = Command::new(env!("CARGO_BIN_EXE_labwired"))
        .args(["test", "--script"])
        .arg(dir.join("script.yaml"))
        .arg("--output-dir")
        .arg(&out)
        .output()
        .unwrap();
    let result: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("result.json")).unwrap()).unwrap();
    (status.status.code().unwrap_or(-1), result)
}

fn example_text(file: &str) -> String {
    std::fs::read_to_string(root().join(EXAMPLE).join(file)).unwrap()
}

fn bridge(result: &Value) -> &Value {
    &result["can_bridges"][0]
}

fn timeline_kinds(result: &Value) -> Vec<String> {
    bridge(result)["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}:{}",
                e["lane"].as_str().unwrap(),
                e["kind"].as_str().unwrap()
            )
        })
        .collect()
}

fn first_index(kinds: &[String], want: &str) -> usize {
    kinds
        .iter()
        .position(|k| k == want)
        .unwrap_or_else(|| panic!("no {want} on the timeline: {kinds:?}"))
}

// ─── record -> replay ────────────────────────────────────────────────────────

#[test]
fn a_recorded_uds_session_replays_twice_with_identical_results() {
    // Record the full diagnostic session.
    let (code, rec, rec_dir) = run_committed(&format!("{EXAMPLE}/record.yaml"), "record");
    assert_eq!(code, 0, "record run failed: {rec:#}");
    let recorded = std::fs::read_to_string(rec_dir.join("can-bus.jsonl")).unwrap();
    assert!(bridge(&rec)["recorded_frames"].as_u64().unwrap() > 30);
    // The committed recording is exactly what the firmware produces today.
    assert_eq!(
        recorded,
        example_text("session.jsonl"),
        "examples/h563-can-replay/session.jsonl is stale; re-record it with record.yaml"
    );

    // Replay it twice: same verdict, same frames, same timeline.
    let (c1, r1, d1) = run_committed(&format!("{EXAMPLE}/replay.yaml"), "replay1");
    let (c2, r2, d2) = run_committed(&format!("{EXAMPLE}/replay.yaml"), "replay2");
    assert_eq!((c1, c2), (0, 0), "{r1:#}");
    let rep = &bridge(&r1)["replay"];
    assert_eq!(rep["verdict"], "match");
    assert_eq!(rep["rx_injected"], rep["rx_total"]);
    assert_eq!(rep["matched_tx"], rep["expected_tx"]);
    assert_eq!(rep["max_cycle_skew"], 0);
    assert_eq!(bridge(&r1), bridge(&r2), "two replays must be identical");
    assert_eq!(
        std::fs::read(d1.join("can-bus.jsonl")).unwrap(),
        std::fs::read(d2.join("can-bus.jsonl")).unwrap()
    );
    // Replay ran with no tester: the ECU's answers came from the recording's
    // requests alone, and equal the recorded answers frame for frame.
    let tx = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| l.contains("\"dir\":\"tx\""))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        tx(&std::fs::read_to_string(d1.join("can-bus.jsonl")).unwrap()),
        tx(&recorded)
    );
}

#[test]
fn negative_control_a_tampered_recording_fails_the_replay() {
    // Change one byte of the ECU's recorded VIN answer. The firmware still
    // answers with the real VIN, so the replay must report the difference.
    let session = example_text("session.jsonl");
    let tampered = session.replacen("62f1904c4142", "62f1904c4143", 1);
    assert_ne!(session, tampered, "the edit must hit the recording");
    let system = example_text("system-replay.yaml").replace(
        "      recording_path: \"session.jsonl\"\n",
        &format!("      recording: {:?}\n", tampered),
    );
    let (code, result) = run_variant(
        "tampered",
        &system,
        "  - can_bridge: { id: \"bus\", replay: match }\n",
    );
    assert_ne!(code, 0, "a mismatching replay must fail the run");
    let rep = &bridge(&result)["replay"];
    assert_eq!(rep["verdict"], "mismatch", "{rep:#}");
    assert_eq!(rep["first_mismatch"]["index"], 0);
    assert!(rep["first_mismatch"]["expected"]["data"]
        .as_str()
        .unwrap()
        .contains("4c4143"));
    assert!(rep["first_mismatch"]["observed"]["data"]
        .as_str()
        .unwrap()
        .contains("4c4142"));
}

// ─── failure scenarios ───────────────────────────────────────────────────────

#[test]
fn node_reset_during_a_multi_frame_transfer_times_out_retries_and_recovers() {
    let (code, result, _) = run_committed(&format!("{EXAMPLE}/reset-fault.yaml"), "reset-fault");
    assert_eq!(code, 0, "{result:#}");
    let b = bridge(&result);
    assert_eq!(b["faults"][0]["kind"], "node_reset");
    let kinds = timeline_kinds(&result);
    // One axis: the ECU's first consecutive frame, the reset, the reboot
    // banner, the tester's timeout, its retry, the complete answer.
    let fault = first_index(&kinds, "fault:node_reset");
    let reboot = fault
        + kinds[fault..]
            .iter()
            .position(|k| k == "console:console")
            .unwrap();
    let timeout = first_index(&kinds, "tester:timeout");
    let retry = first_index(&kinds, "tester:retry");
    assert!(
        fault < reboot && reboot < timeout && timeout < retry,
        "{kinds:?}"
    );
    let timeline = b["timeline"].as_array().unwrap();
    let resp_after_retry = timeline[retry..].iter().any(|e| {
        e["kind"] == "response"
            && e["detail"]
                .as_str()
                .unwrap()
                .contains("step 1: response 62 F1 A0")
    });
    assert!(resp_after_retry, "the retried 0xF1A0 read must complete");
    // The reset fired between the first CF and the rest of the transfer.
    let fault_ev = &timeline[fault];
    let cf = &timeline[fault - 1];
    assert_eq!(cf["data"].as_str().unwrap().get(..2), Some("21"));
    assert_eq!(cf["cycle"], fault_ev["cycle"]);
}

#[test]
fn negative_control_without_a_retry_the_reset_fails_the_tester() {
    let system = example_text("system-reset-fault.yaml").replace("retries: 1", "retries: 0");
    let (code, result) = run_variant(
        "reset-noretry",
        &system,
        "  - uds_tester: { id: \"uds-tester\", result: done }\n",
    );
    assert_ne!(code, 0, "the tester cannot finish without a retry");
    let kinds = timeline_kinds(&result);
    assert!(kinds.contains(&"tester:failed".to_string()), "{kinds:?}");
    assert!(!kinds.contains(&"tester:retry".to_string()));
}

/// The base session with one fault on the bridge and a tester timeout.
fn session_with_fault(fault_yaml: &str, retries: u32) -> String {
    let base = example_text("system-record.yaml");
    let base = base.replace(
        "      reply_id: 0x7E8\n",
        &format!(
            "      reply_id: 0x7E8\n      response_timeout_us: 100\n      retries: {retries}\n"
        ),
    );
    base.replace(
        "      pause_mode: drop\n",
        &format!("      pause_mode: drop\n      faults:\n{fault_yaml}"),
    )
}

#[test]
fn a_missing_reply_is_recorded_and_the_tester_recovers_by_retrying() {
    let fault = "        - { at_cycle: 0, kind: can_drop, frame: { direction: tx, id: 0x7E8 } }\n";
    let (code, result) = run_variant(
        "drop",
        &session_with_fault(fault, 1),
        "  - uds_tester: { id: \"uds-tester\", result: done }\n  - can_bridge: { id: \"bus\", faults_fired: 1 }\n",
    );
    assert_eq!(code, 0, "{result:#}");
    let b = bridge(&result);
    assert_eq!(b["dropped_total"], 1);
    assert_eq!(b["dropped"][0]["reason"], "fault");
    assert_eq!(b["dropped"][0]["id"], 0x7E8);
    let kinds = timeline_kinds(&result);
    assert!(first_index(&kinds, "fault:can_drop") < first_index(&kinds, "tester:timeout"));

    // Control: the same fault without a retry fails the session.
    let (code, _) = run_variant(
        "drop-noretry",
        &session_with_fault(fault, 0),
        "  - uds_tester: { id: \"uds-tester\", result: done }\n",
    );
    assert_ne!(code, 0);
}

#[test]
fn a_delayed_reply_within_the_timeout_passes_and_one_beyond_it_fails() {
    // Hold the ECU's first reply 50 us: inside the 100 us timeout.
    let short = "        - { at_cycle: 0, kind: can_delay, delay_us: 50, frame: { direction: tx, id: 0x7E8 } }\n";
    let (code, result) = run_variant(
        "delay-short",
        &session_with_fault(short, 0),
        "  - uds_tester: { id: \"uds-tester\", result: done }\n",
    );
    assert_eq!(code, 0, "{result:#}");
    let t = bridge(&result)["timeline"].as_array().unwrap().clone();
    let held = t.iter().find(|e| e["kind"] == "can_delay").unwrap();
    let resp = t
        .iter()
        .find(|e| e["kind"] == "response")
        .expect("the delayed reply arrives");
    let held_cycle = held["cycle"].as_u64().unwrap();
    // 50 us at 250 MHz.
    assert!(resp["cycle"].as_u64().unwrap() >= held_cycle + 12_500);

    // 150 us: beyond the timeout, no retry -> the tester fails.
    let long = short.replace("delay_us: 50", "delay_us: 150");
    let (code, result) = run_variant(
        "delay-long",
        &session_with_fault(&long, 0),
        "  - uds_tester: { id: \"uds-tester\", result: done }\n",
    );
    assert_ne!(code, 0);
    assert!(timeline_kinds(&result).contains(&"tester:failed".to_string()));
}

#[test]
fn bus_off_silences_the_ecu_because_this_firmware_never_recovers() {
    // Mid-session, after the VIN read (cycle 5513) and before the calibration
    // block completes.
    let fault = "        - { at_cycle: 10000, kind: can_bus_off }\n";
    let (code, result) = run_variant(
        "busoff",
        &session_with_fault(fault, 1),
        "  - uds_tester: { id: \"uds-tester\", result: done }\n",
    );
    assert_ne!(code, 0, "the ECU cannot answer from bus-off");
    let b = bridge(&result);
    assert_eq!(b["faults"][0]["kind"], "can_bus_off");
    let fired = b["faults"][0]["applied_cycle"].as_u64().unwrap();
    let t = b["timeline"].as_array().unwrap();
    assert!(
        !t.iter().any(|e| e["lane"] == "can"
            && e["dir"] == "tx"
            && e["cycle"].as_u64().unwrap() > fired),
        "no ECU frame after bus-off: the firmware never clears CCCR.INIT"
    );
    let kinds = timeline_kinds(&result);
    assert!(first_index(&kinds, "fault:can_bus_off") < first_index(&kinds, "tester:failed"));
}

// ─── pause modes on the machine API ──────────────────────────────────────────

fn ecu_machine(
    mode: CanPauseMode,
    cap: usize,
) -> (Machine<labwired_core::cpu::CortexM>, Arc<Mutex<Vec<u8>>>) {
    let example = root().join("examples/h563-uds-ecu");
    // The ECU alone: no tester, so every frame comes from the "live" source.
    let mut manifest = SystemManifest::from_file(example.join("system.yaml")).unwrap();
    manifest.external_devices.clear();
    let chip = ChipDescriptor::from_file(example.join(&manifest.chip)).unwrap();
    let mut bus = labwired_core::bus::SystemBus::from_config(&chip, &manifest).unwrap();
    let uart: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    bus.attach_uart_tx_sink(uart.clone(), false);
    let hz = bus.cpu_hz;
    bus.attach_can_bridge(
        CanBridge::new(
            "live",
            "fdcan1",
            hz,
            CanBridgeConfig {
                pause_mode: mode,
                capture_capacity: cap,
                overflow: CanOverflowPolicy::DropNewest,
                ..Default::default()
            },
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let (cpu, _) = labwired_core::system::cortex_m::configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    m.load_firmware(&labwired_loader::load_elf(Path::new(&root().join(ELF))).unwrap())
        .unwrap();
    (m, uart)
}

fn run(m: &mut Machine<labwired_core::cpu::CortexM>, steps: usize) {
    for _ in 0..steps {
        m.step().unwrap();
    }
}

fn read_vin() -> CanFrame {
    CanFrame::classic(0x7E0, vec![0x03, 0x22, 0xF1, 0x90])
}

fn answered_vin(m: &Machine<labwired_core::cpu::CortexM>) -> bool {
    m.bus
        .can_bridge("live")
        .unwrap()
        .recording()
        .entries
        .iter()
        .any(|e| {
            e.dir == labwired_core::network::can_bridge::CanDirection::Tx
                && e.frame.data.windows(3).any(|w| w == [0x62, 0xF1, 0x90])
        })
}

fn boot(m: &mut Machine<labwired_core::cpu::CortexM>, uart: &Arc<Mutex<Vec<u8>>>) {
    for _ in 0..200 {
        run(m, 500);
        if String::from_utf8_lossy(&uart.lock().unwrap()).contains("ECU_READY") {
            run(m, 2000);
            return;
        }
    }
    panic!("the ECU never printed ECU_READY");
}

#[test]
fn pause_drop_records_the_frame_and_the_ecu_never_answers() {
    let (mut m, uart) = ecu_machine(CanPauseMode::Drop, 8);
    boot(&mut m, &uart);
    m.bus.can_bridges_set_paused(true);
    let out = m
        .bus
        .can_bridge_offer("live", read_vin(), Some(1000.0))
        .unwrap();
    assert_eq!(out, CanOfferOutcome::Dropped);
    m.bus.can_bridges_set_paused(false);
    run(&mut m, 50_000);
    assert!(
        !answered_vin(&m),
        "a dropped frame must never reach the ECU"
    );
    let r = m.bus.can_bridge_report("live").unwrap();
    assert_eq!(r.dropped_total, 1);
    let d = &r.dropped[0];
    assert_eq!((d.id, d.dlc, d.data.as_str()), (0x7E0, 4, "0322f190"));
    assert_eq!(d.host_time_ms, Some(1000.0));

    // Control: the same frame offered while running is answered.
    m.bus.can_bridge_offer("live", read_vin(), None).unwrap();
    run(&mut m, 50_000);
    assert!(
        answered_vin(&m),
        "a frame offered while running reaches the ECU"
    );
}

#[test]
fn pause_capture_delivers_after_resume_and_records_overflow() {
    let (mut m, uart) = ecu_machine(CanPauseMode::Capture, 1);
    boot(&mut m, &uart);
    m.bus.can_bridges_set_paused(true);
    assert_eq!(
        m.bus
            .can_bridge_offer("live", read_vin(), Some(1.0))
            .unwrap(),
        CanOfferOutcome::Captured
    );
    // Capacity 1: the second frame overflows (drop_newest) and is recorded.
    assert_eq!(
        m.bus
            .can_bridge_offer(
                "live",
                CanFrame::classic(0x7E0, vec![0x02, 0x3E, 0x00]),
                Some(2.0)
            )
            .unwrap(),
        CanOfferOutcome::Dropped
    );
    // Nothing moves while paused (the host is not stepping).
    assert!(!answered_vin(&m));
    m.bus.can_bridges_set_paused(false);
    run(&mut m, 50_000);
    assert!(
        answered_vin(&m),
        "the captured request is delivered after resume"
    );
    let r = m.bus.can_bridge_report("live").unwrap();
    assert_eq!(r.capture.captured_total, 1);
    assert_eq!(r.capture.released_total, 1);
    assert_eq!(r.capture.overflowed_total, 1);
    assert_eq!(r.dropped[0].data, "023e00");
    assert!(r
        .pause_note
        .contains("does not preserve real-time interaction"));
}

#[test]
fn replay_mode_ignores_live_frames() {
    let (mut m, uart) = {
        // A replay bridge needs a recording: one VIN request at cycle 0.
        let example = root().join("examples/h563-uds-ecu");
        let mut manifest = SystemManifest::from_file(example.join("system.yaml")).unwrap();
        manifest.external_devices.clear();
        let chip = ChipDescriptor::from_file(example.join(&manifest.chip)).unwrap();
        let mut bus = labwired_core::bus::SystemBus::from_config(&chip, &manifest).unwrap();
        let uart: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        bus.attach_uart_tx_sink(uart.clone(), false);
        let hz = bus.cpu_hz;
        let rec = labwired_core::network::can_recording::CanRecording::parse(
            "(0.0002) fdcan1-rx 7E0#0322F190\n",
            hz,
        )
        .unwrap();
        bus.attach_can_bridge(
            CanBridge::new(
                "live",
                "fdcan1",
                hz,
                CanBridgeConfig {
                    pause_mode: CanPauseMode::Replay,
                    ..Default::default()
                },
                Some(rec),
            )
            .unwrap(),
        )
        .unwrap();
        let (cpu, _) = labwired_core::system::cortex_m::configure_cortex_m(&mut bus);
        let mut m = Machine::new(cpu, bus);
        m.load_firmware(&labwired_loader::load_elf(&root().join(ELF)).unwrap())
            .unwrap();
        (m, uart)
    };
    assert_eq!(
        m.bus
            .can_bridge_offer(
                "live",
                CanFrame::classic(0x7E0, vec![0x02, 0x10, 0x03]),
                None
            )
            .unwrap(),
        CanOfferOutcome::Dropped
    );
    boot(&mut m, &uart);
    run(&mut m, 50_000);
    assert!(answered_vin(&m), "the replayed request is answered");
    let r = m.bus.can_bridge_report("live").unwrap();
    assert_eq!(
        r.dropped[0].reason,
        labwired_core::network::can_bridge::CanDropReason::ReplayIgnoresLive
    );
    // The live 10 03 never reached the ECU: no 50 03 answer.
    assert!(!m
        .bus
        .can_bridge("live")
        .unwrap()
        .recording()
        .entries
        .iter()
        .any(|e| e.frame.data.windows(2).any(|w| w == [0x50, 0x03])));
}
