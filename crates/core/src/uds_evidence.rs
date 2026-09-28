// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! UDS evidence: what a scripted `uds-tester` run proved, as data and as a
//! human report.
//!
//! [`SystemBus::uds_evidence`] gathers the testers' own transcripts (the
//! executed assertions) and the CAN frames from the shared bus trace, decoded
//! as ISO-TP and UDS. [`render_markdown`] turns that into one self-contained
//! Markdown report. The CLI writes it next to `result.json`; the wasm engine
//! returns it to the browser, so both surfaces print the same report from the
//! same code.

use crate::bus::bus_trace::{BusDir, BusPayload};
use crate::bus::{CanUdsTesterState, SystemBus, UdsExchange};

/// Everything a UDS run produced, serializable into `result.json`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UdsEvidence {
    /// Core clock the cycle stamps convert with.
    pub cpu_hz: u64,
    pub testers: Vec<UdsTesterEvidence>,
    /// CAN frames on the testers' request/reply ids, oldest first.
    pub frames: Vec<CanTimelineFrame>,
    /// Lines the firmware printed on its UARTs, stamped with the cycle of
    /// their first byte. Interleaved with the frames they show when the ECU
    /// booted and rebooted relative to the exchange.
    pub console: Vec<ConsoleLine>,
    /// Bus-trace events evicted before this snapshot. Non-zero means the
    /// oldest frames of the timeline are missing (the transcript is not
    /// affected: the tester records it itself).
    pub trace_evicted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UdsTesterEvidence {
    pub id: String,
    pub connection: String,
    pub request_id: u32,
    pub reply_id: u32,
    /// `done`, `failed` or `running`.
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub steps_total: usize,
    pub steps_passed: usize,
    /// FlowControl bytes the tester answers a FirstFrame with (`""` = none).
    pub flow_control: String,
    pub exchanges: Vec<UdsExchange>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConsoleLine {
    pub cycle: u64,
    pub time_us: f64,
    pub peripheral: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CanTimelineFrame {
    pub seq: u64,
    pub cycle: u64,
    /// `cycle / cpu_hz`, in microseconds of simulated time.
    pub time_us: f64,
    pub peripheral: String,
    /// `tester->ecu` or `ecu->tester`.
    pub direction: String,
    pub id: u32,
    pub fd: bool,
    pub bitrate_switch: bool,
    pub data: Vec<u8>,
    /// ISO-TP frame decode, e.g. `SF len=3`, `FF len=603`, `CF SN=1`,
    /// `FC CTS BS=0 STmin=0`.
    pub isotp: String,
    /// UDS decode of a SingleFrame / FirstFrame payload; empty for CF / FC.
    pub uds: String,
}

impl SystemBus {
    /// UDS evidence for this run, or `None` when no `uds-tester` is attached.
    pub fn uds_evidence(&self) -> Option<UdsEvidence> {
        if self.can_uds_testers.is_empty() {
            return None;
        }
        let testers: Vec<UdsTesterEvidence> = self
            .can_uds_testers
            .iter()
            .map(|t| UdsTesterEvidence {
                id: t.id.clone(),
                connection: t.connection.clone(),
                request_id: t.request_id,
                reply_id: t.reply_id,
                result: match t.state {
                    CanUdsTesterState::Done => "done",
                    CanUdsTesterState::Failed => "failed",
                    _ => "running",
                }
                .to_string(),
                failure: t.failure.clone(),
                steps_total: t.script.len(),
                steps_passed: t.transcript.iter().filter(|e| e.passed).count(),
                flow_control: hex(&t.flow_control),
                exchanges: t.transcript.clone(),
            })
            .collect();

        let cpu_hz = self.cpu_hz.max(1);
        let events = self.bus_trace_snapshot();
        let to_us = |cycle: u64| cycle as f64 * 1_000_000.0 / cpu_hz as f64;

        // Firmware console lines, one per '\n', stamped at their first byte.
        let mut console: Vec<ConsoleLine> = Vec::new();
        let mut open: std::collections::BTreeMap<String, (u64, Vec<u8>)> = Default::default();
        for e in &events {
            let BusPayload::Uart {
                direction: BusDir::Tx,
                byte,
            } = e.payload
            else {
                continue;
            };
            let entry = open.entry(e.bus.clone()).or_insert((e.cycle, Vec::new()));
            if entry.1.is_empty() {
                entry.0 = e.cycle;
            }
            if byte == b'\n' {
                let text = String::from_utf8_lossy(&entry.1).trim_end().to_string();
                if !text.is_empty() {
                    console.push(ConsoleLine {
                        cycle: entry.0,
                        time_us: to_us(entry.0),
                        peripheral: e.bus.clone(),
                        text,
                    });
                }
                entry.1.clear();
            } else {
                entry.1.push(byte);
            }
        }
        for (bus, (cycle, bytes)) in open {
            let text = String::from_utf8_lossy(&bytes).trim_end().to_string();
            if !text.is_empty() {
                console.push(ConsoleLine {
                    cycle,
                    time_us: to_us(cycle),
                    peripheral: bus,
                    text,
                });
            }
        }
        console.sort_by_key(|l| l.cycle);

        let frames = events
            .into_iter()
            .filter_map(|e| {
                let BusPayload::Can {
                    direction,
                    id,
                    data,
                    fd,
                    bitrate_switch,
                    ..
                } = e.payload
                else {
                    return None;
                };
                let on_tester_ids = self
                    .can_uds_testers
                    .iter()
                    .any(|t| t.connection == e.bus && (t.request_id == id || t.reply_id == id));
                if !on_tester_ids {
                    return None;
                }
                // The trace direction is the MCU controller's view: `rx` is a
                // frame the ECU received, i.e. one the tester sent.
                let direction = match direction {
                    BusDir::Rx => "tester->ecu",
                    BusDir::Tx => "ecu->tester",
                };
                let (isotp, payload) = decode_isotp(&data);
                let uds = payload
                    .map(|p| describe_uds(p, direction == "tester->ecu"))
                    .unwrap_or_default();
                Some(CanTimelineFrame {
                    seq: e.seq,
                    cycle: e.cycle,
                    time_us: to_us(e.cycle),
                    peripheral: e.bus,
                    direction: direction.to_string(),
                    id,
                    fd,
                    bitrate_switch,
                    data,
                    isotp,
                    uds,
                })
            })
            .collect();

        Some(UdsEvidence {
            cpu_hz,
            testers,
            frames,
            console,
            trace_evicted: self.bus_trace.evicted(),
        })
    }
}

/// Space-separated upper-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn hex_short(bytes: &[u8], max: usize) -> String {
    if bytes.len() <= max {
        hex(bytes)
    } else {
        format!("{} … ({} B)", hex(&bytes[..max]), bytes.len())
    }
}

/// Decode one CAN frame's ISO-TP PCI. Returns the label and, for a
/// SingleFrame or FirstFrame, the UDS payload bytes it carries.
pub fn decode_isotp(data: &[u8]) -> (String, Option<&[u8]>) {
    let Some(&b0) = data.first() else {
        return ("empty".to_string(), None);
    };
    match b0 >> 4 {
        0 => {
            if b0 == 0 {
                // CAN-FD escape SingleFrame: length in byte 1.
                let len = data.get(1).copied().unwrap_or(0) as usize;
                let end = (2 + len).min(data.len());
                (format!("SF(FD) len={len}"), data.get(2..end))
            } else {
                let len = (b0 & 0x0F) as usize;
                let end = (1 + len).min(data.len());
                (format!("SF len={len}"), data.get(1..end))
            }
        }
        1 => {
            let len = (((b0 & 0x0F) as usize) << 8) | data.get(1).copied().unwrap_or(0) as usize;
            if len == 0 && data.len() >= 6 {
                let len = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
                (format!("FF len={len}"), data.get(6..))
            } else {
                (format!("FF len={len}"), data.get(2..))
            }
        }
        2 => (format!("CF SN={}", b0 & 0x0F), None),
        3 => {
            let fs = match b0 & 0x0F {
                0 => "CTS",
                1 => "WAIT",
                2 => "OVFLW",
                _ => "FS?",
            };
            (
                format!(
                    "FC {fs} BS={} STmin={}",
                    data.get(1).copied().unwrap_or(0),
                    data.get(2).copied().unwrap_or(0)
                ),
                None,
            )
        }
        _ => (format!("PCI {b0:02X}?"), None),
    }
}

/// UDS service name for a request SID.
pub fn service_name(sid: u8) -> &'static str {
    match sid {
        0x10 => "DiagnosticSessionControl",
        0x11 => "ECUReset",
        0x14 => "ClearDiagnosticInformation",
        0x19 => "ReadDTCInformation",
        0x22 => "ReadDataByIdentifier",
        0x23 => "ReadMemoryByAddress",
        0x27 => "SecurityAccess",
        0x28 => "CommunicationControl",
        0x2E => "WriteDataByIdentifier",
        0x2F => "InputOutputControlByIdentifier",
        0x31 => "RoutineControl",
        0x34 => "RequestDownload",
        0x36 => "TransferData",
        0x37 => "RequestTransferExit",
        0x3E => "TesterPresent",
        0x85 => "ControlDTCSetting",
        _ => "unknown service",
    }
}

/// ISO 14229-1 negative response code name.
pub fn nrc_name(nrc: u8) -> &'static str {
    match nrc {
        0x10 => "generalReject",
        0x11 => "serviceNotSupported",
        0x12 => "subFunctionNotSupported",
        0x13 => "incorrectMessageLengthOrInvalidFormat",
        0x14 => "responseTooLong",
        0x21 => "busyRepeatRequest",
        0x22 => "conditionsNotCorrect",
        0x24 => "requestSequenceError",
        0x31 => "requestOutOfRange",
        0x33 => "securityAccessDenied",
        0x35 => "invalidKey",
        0x36 => "exceedNumberOfAttempts",
        0x37 => "requiredTimeDelayNotExpired",
        0x72 => "generalProgrammingFailure",
        0x78 => "requestCorrectlyReceived-ResponsePending",
        0x7E => "subFunctionNotSupportedInActiveSession",
        0x7F => "serviceNotSupportedInActiveSession",
        _ => "unknown NRC",
    }
}

/// One-line UDS decode of a PDU (or its first bytes, for a FirstFrame).
pub fn describe_uds(pdu: &[u8], is_request: bool) -> String {
    let Some(&sid) = pdu.first() else {
        return String::new();
    };
    if sid == 0x7F {
        let req = pdu.get(1).copied().unwrap_or(0);
        let nrc = pdu.get(2).copied().unwrap_or(0);
        return format!(
            "NegativeResponse {} NRC 0x{nrc:02X} {}",
            service_name(req),
            nrc_name(nrc)
        );
    }
    if is_request {
        service_name(sid).to_string()
    } else if sid >= 0x40 {
        format!("{} positive response", service_name(sid - 0x40))
    } else {
        format!("0x{sid:02X}")
    }
}

/// One executed assertion line for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportAssertion {
    pub text: String,
    pub passed: bool,
}

/// What the caller knows about the run that the bus does not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportMeta {
    pub title: String,
    /// The run's own verdict (the report never re-derives it).
    pub passed: bool,
    pub firmware_path: Option<String>,
    pub firmware_sha256: Option<String>,
    pub system_path: Option<String>,
    pub chip: Option<String>,
    /// `(tool, version)` pairs.
    pub tool_versions: Vec<(String, String)>,
    pub assertions: Vec<ReportAssertion>,
    /// Commands that reproduce this run.
    pub reproduce: Vec<String>,
    /// Extra limits specific to this setup, printed after the generic ones.
    pub extra_limits: Vec<String>,
}

/// Modelling limits every UDS report states. Keep these honest: they are
/// what a reader needs to judge the timing and coverage claims.
pub const UDS_MODEL_LIMITS: &[&str] = &[
    "Times are simulated CPU time (engine cycle / core clock) at which a frame entered or left the CAN controller model. CAN bit timing, arbitration, ACK, error frames and bus load are not simulated: a frame takes zero wire time, so the latencies below measure firmware processing time, not bus time.",
    "The tester is a host-side scripted CAN node serviced once per engine tick, not a second MCU. It sends classic CAN frames (8 bytes max); it checks ISO-TP framing, ConsecutiveFrame sequence numbers and the UDS response bytes. It enforces a per-step tick budget, not the ISO 14229-2 P2/P2* or ISO 15765-2 N_Bs/N_Cr timers in real time.",
    "The ECU firmware, its UDS stack and its ISO-TP layer run unmodified as machine code on the simulated core. ECUReset is the firmware's own reset request (SCB AIRCR.SYSRESETREQ); the simulated core reboots through its vector table and the tester re-connects over the same bus.",
    "A system reset restarts the core only: peripheral registers (including the CAN controller and its RX FIFO) and RAM keep their state until the rebooted firmware re-initializes them. On silicon, SYSRESETREQ also resets the peripherals, so a frame queued at the moment of reset is lost. The scenario therefore waits before its first post-reset request, as a real tester does.",
    "Not covered by this scenario: the physical layer and transceiver, bus-off and error confinement, other bus participants, and security access.",
];

/// FDCAN-specific limits, added when a tester is wired to an `fdcan*` controller.
pub const FDCAN_MODEL_LIMITS: &[&str] = &[
    "STM32H5 FDCAN: acceptance filters are not modelled (frames follow RXGFC.ANFS into RX FIFO0), RX FIFO1 routing and dedicated RX buffers are not modelled, FDCAN2 and interrupt line 1 are not modelled. DBTP/NBTP are stored with their silicon reset values but do not change timing.",
];

fn ms(us: f64) -> String {
    format!("{:.3}", us / 1000.0)
}

/// Render the one human report for a run.
pub fn render_markdown(meta: &ReportMeta, ev: &UdsEvidence) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let us = |cycle: u64| cycle as f64 * 1_000_000.0 / ev.cpu_hz.max(1) as f64;
    let steps_total: usize = ev.testers.iter().map(|t| t.steps_total).sum();
    let steps_passed: usize = ev.testers.iter().map(|t| t.steps_passed).sum();
    let asserts_passed = meta.assertions.iter().filter(|a| a.passed).count();

    let _ = writeln!(o, "# UDS evidence report: {}\n", meta.title);
    let _ = writeln!(
        o,
        "**Verdict: {}** · {}/{} run assertions passed · {}/{} UDS steps passed\n",
        if meta.passed { "PASS" } else { "FAIL" },
        asserts_passed,
        meta.assertions.len(),
        steps_passed,
        steps_total
    );

    let _ = writeln!(o, "## Firmware and setup\n");
    let _ = writeln!(o, "| Item | Value |\n|---|---|");
    if let Some(p) = &meta.firmware_path {
        let _ = writeln!(o, "| Firmware | `{p}` |");
    }
    if let Some(h) = &meta.firmware_sha256 {
        let _ = writeln!(o, "| Firmware sha256 | `{h}` |");
    }
    if let Some(p) = &meta.system_path {
        let _ = writeln!(o, "| System | `{p}` |");
    }
    if let Some(c) = &meta.chip {
        let _ = writeln!(o, "| Chip | {c} |");
    }
    let _ = writeln!(o, "| Core clock | {} Hz |", ev.cpu_hz);
    for t in &ev.testers {
        let _ = writeln!(
            o,
            "| Tester `{}` | on `{}`, request 0x{:03X}, reply 0x{:03X}, FlowControl `{}` |",
            t.id,
            t.connection,
            t.request_id,
            t.reply_id,
            if t.flow_control.is_empty() {
                "(none)"
            } else {
                &t.flow_control
            }
        );
    }
    let _ = writeln!(o);

    if !meta.tool_versions.is_empty() {
        let _ = writeln!(o, "## Tool versions\n");
        let _ = writeln!(o, "| Tool | Version |\n|---|---|");
        for (k, v) in &meta.tool_versions {
            let _ = writeln!(o, "| {k} | {v} |");
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(o, "## Assertions (executed)\n");
    if meta.assertions.is_empty() {
        let _ = writeln!(o, "_No run-level assertions; see the UDS steps below._\n");
    } else {
        let _ = writeln!(o, "| # | Assertion | Result |\n|---|---|---|");
        for (i, a) in meta.assertions.iter().enumerate() {
            let _ = writeln!(
                o,
                "| {} | `{}` | {} |",
                i + 1,
                a.text.replace('|', "\\|"),
                if a.passed { "PASS" } else { "FAIL" }
            );
        }
        let _ = writeln!(o);
    }

    for t in &ev.testers {
        let _ = writeln!(
            o,
            "## UDS exchanges: tester `{}` ({}, {}/{} steps)\n",
            t.id, t.result, t.steps_passed, t.steps_total
        );
        if let Some(f) = &t.failure {
            let _ = writeln!(o, "**Failure:** {f}\n");
        }
        let _ = writeln!(
            o,
            "| Step | Request at (ms) | Service | Request | Expected | Response | ISO-TP frames | Latency (µs) | Result |\n|---|---|---|---|---|---|---|---|---|"
        );
        for e in &t.exchanges {
            let sid = e.request.first().copied().unwrap_or(0);
            let (resp, frames, lat) = match (&e.response, e.response_cycle) {
                (Some(r), Some(c)) => (
                    hex_short(r, 12),
                    e.response_frames.to_string(),
                    format!("{:.1}", us(c) - us(e.request_cycle)),
                ),
                _ => ("(none)".to_string(), "-".to_string(), "-".to_string()),
            };
            let _ = writeln!(
                o,
                "| {} | {} | {} | `{}` | `{}` | `{}` | {} | {} | {} |",
                e.step,
                ms(us(e.request_cycle)),
                service_name(sid),
                hex_short(&e.request, 12),
                e.expected,
                resp,
                frames,
                lat,
                if e.passed { "PASS" } else { "FAIL" }
            );
        }
        let started = t.exchanges.len();
        if started < t.steps_total {
            let _ = writeln!(
                o,
                "\n{} of {} scripted steps never started.",
                t.steps_total - started,
                t.steps_total
            );
        }
        let _ = writeln!(o);
    }

    let _ = writeln!(o, "## CAN frame timeline (ISO-TP and UDS decoded)\n");
    if ev.trace_evicted > 0 {
        let _ = writeln!(
            o,
            "_The bus trace evicted {} older events; the earliest frames may be missing._\n",
            ev.trace_evicted
        );
    }
    let _ = writeln!(
        o,
        "| # | t (ms) | Cycle | Direction | CAN ID | Type | ISO-TP | UDS | Data |\n|---|---|---|---|---|---|---|---|---|"
    );
    let mut console = ev.console.iter().peekable();
    let print_console_until =
        |o: &mut String,
         console: &mut std::iter::Peekable<std::slice::Iter<'_, ConsoleLine>>,
         cycle: u64| {
            while let Some(l) = console.next_if(|l| l.cycle <= cycle) {
                let _ = writeln!(
                    o,
                    "| - | {} | {} | firmware {} | | | | console | `{}` |",
                    ms(l.time_us),
                    l.cycle,
                    l.peripheral,
                    l.text.replace('|', "\\|").replace('`', "'")
                );
            }
        };
    for (i, f) in ev.frames.iter().enumerate() {
        print_console_until(&mut o, &mut console, f.cycle);
        let kind = match (f.fd, f.bitrate_switch) {
            (true, true) => "FD+BRS",
            (true, false) => "FD",
            _ => "classic",
        };
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | 0x{:03X} | {} | {} | {} | `{}` |",
            i + 1,
            ms(f.time_us),
            f.cycle,
            f.direction,
            f.id,
            kind,
            f.isotp,
            f.uds,
            hex_short(&f.data, 16)
        );
    }
    print_console_until(&mut o, &mut console, u64::MAX);
    let _ = writeln!(o);

    let _ = writeln!(o, "## What is modelled and what is not\n");
    for l in UDS_MODEL_LIMITS {
        let _ = writeln!(o, "- {l}");
    }
    if ev.testers.iter().any(|t| t.connection.starts_with("fdcan")) {
        for l in FDCAN_MODEL_LIMITS {
            let _ = writeln!(o, "- {l}");
        }
    }
    for l in &meta.extra_limits {
        let _ = writeln!(o, "- {l}");
    }
    let _ = writeln!(o);

    if !meta.reproduce.is_empty() {
        let _ = writeln!(o, "## Reproduce\n\n```sh");
        for r in &meta.reproduce {
            let _ = writeln!(o, "{r}");
        }
        let _ = writeln!(o, "```");
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isotp_decode_covers_every_pci() {
        assert_eq!(decode_isotp(&[0x03, 0x22, 0xF1, 0x90]).0, "SF len=3");
        assert_eq!(
            decode_isotp(&[0x03, 0x22, 0xF1, 0x90]).1,
            Some(&[0x22, 0xF1, 0x90][..])
        );
        let (l, p) = decode_isotp(&[0x00, 0x02, 0x50, 0x03]);
        assert_eq!((l.as_str(), p), ("SF(FD) len=2", Some(&[0x50, 0x03][..])));
        let (l, p) = decode_isotp(&[0x12, 0x5B, 0x62, 0xF1, 0xA0]);
        assert_eq!(
            (l.as_str(), p),
            ("FF len=603", Some(&[0x62, 0xF1, 0xA0][..]))
        );
        assert_eq!(decode_isotp(&[0x21, 0, 1]).0, "CF SN=1");
        assert_eq!(decode_isotp(&[0x30, 0, 0]).0, "FC CTS BS=0 STmin=0");
        assert_eq!(decode_isotp(&[0x32, 0, 0]).0, "FC OVFLW BS=0 STmin=0");
    }

    #[test]
    fn uds_decode_names_services_and_nrcs() {
        assert_eq!(
            describe_uds(&[0x10, 0x03], true),
            "DiagnosticSessionControl"
        );
        assert_eq!(
            describe_uds(&[0x62, 0xF1, 0x90], false),
            "ReadDataByIdentifier positive response"
        );
        assert_eq!(
            describe_uds(&[0x7F, 0x2E, 0x31], false),
            "NegativeResponse WriteDataByIdentifier NRC 0x31 requestOutOfRange"
        );
    }
}
