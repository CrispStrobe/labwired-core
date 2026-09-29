// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! A CAN interaction recorded on the virtual time axis, and its two file
//! formats.
//!
//! A [`CanRecording`] is every frame one CAN controller exchanged with the
//! outside of the simulation, in order, each stamped with the machine cycle it
//! crossed the controller boundary at. `rx` frames went INTO the controller
//! (what a tester, a log, or a live bus sent); `tx` frames came OUT of it (what
//! the firmware answered). A `can-bridge` in `replay` mode injects the `rx`
//! frames at their cycles and compares what the firmware transmits with the
//! recorded `tx` frames, which turns one recorded interaction into a
//! repeatable regression test.
//!
//! Two text formats, both line oriented:
//!
//! * **JSON lines** (lossless, the native format). The first line is a header
//!   `{"labwired_can_recording":1,"clock_hz":250000000,"controller":"fdcan1"}`;
//!   every other line is one frame:
//!   `{"cycle":1234,"dir":"rx","id":2016,"ext":false,"fd":false,"brs":false,"rtr":false,"data":"0322f190"}`.
//!   `cycle` is exact, so a replay injects on the same cycle the frame was
//!   recorded on.
//! * **candump log** (`candump -l` style), for interchange with can-utils:
//!   `(<seconds>) <iface> <ID>#<DATA>`, CAN-FD as `<ID>##<flags><DATA>`.
//!   candump has no direction column, so the interface name carries it: an
//!   interface named `tx` or ending in `-tx` holds frames the controller
//!   transmitted; every other interface holds frames sent TO the controller.
//!   Timestamps are virtual seconds since power-on (`cycle / clock_hz`). A log
//!   captured on a real bus carries wall-clock epoch timestamps; any log whose
//!   first timestamp is at or above 10^6 s is treated as one and rebased so its
//!   first frame is at virtual time 0.

use super::CanFrame;
use serde::{Deserialize, Serialize};

/// Which way a frame crossed the controller boundary, from the controller's
/// point of view (the same convention as the bus trace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanDirection {
    /// Into the controller: sent by the outside (tester, log, live bus).
    Rx,
    /// Out of the controller: transmitted by the firmware.
    Tx,
}

impl CanDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            CanDirection::Rx => "rx",
            CanDirection::Tx => "tx",
        }
    }
}

/// One recorded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanRecordEntry {
    /// Machine cycle (since power-on) the frame crossed the boundary at.
    pub cycle: u64,
    pub dir: CanDirection,
    pub frame: CanFrame,
}

/// A recorded CAN interaction. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CanRecording {
    /// Core clock the cycles count, in Hz. `0` when unknown.
    pub clock_hz: u64,
    /// The controller the frames were recorded on (informational).
    pub controller: String,
    pub entries: Vec<CanRecordEntry>,
}

/// The CAN-FD data length code for a payload length.
pub fn can_dlc(len: usize) -> u8 {
    match len {
        0..=8 => len as u8,
        9..=12 => 9,
        13..=16 => 10,
        17..=20 => 11,
        21..=24 => 12,
        25..=32 => 13,
        33..=48 => 14,
        _ => 15,
    }
}

/// Lower-case hex, no separators.
pub fn hex_bytes(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Parse hex bytes. Accepts `"0322f190"`, `"03 22 F1 90"`, `"03:22"`.
pub fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != ':' && *c != ',')
        .collect();
    if !compact.is_ascii() {
        return Err(format!("'{text}' is not hex"));
    }
    if compact.len() % 2 != 0 {
        return Err(format!("'{text}' has an odd number of hex digits"));
    }
    (0..compact.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&compact[i..i + 2], 16)
                .map_err(|e| format!("'{text}' is not hex: {e}"))
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct HeaderLine {
    labwired_can_recording: u32,
    #[serde(default)]
    clock_hz: u64,
    #[serde(default)]
    controller: String,
}

#[derive(Serialize, Deserialize)]
struct FrameLine {
    cycle: u64,
    dir: CanDirection,
    id: u32,
    #[serde(default)]
    ext: bool,
    #[serde(default)]
    fd: bool,
    #[serde(default)]
    brs: bool,
    #[serde(default)]
    rtr: bool,
    #[serde(default)]
    data: String,
}

/// First timestamp at or above this many seconds marks a wall-clock log.
const EPOCH_THRESHOLD_S: f64 = 1.0e6;

impl CanRecording {
    pub fn new(clock_hz: u64, controller: impl Into<String>) -> Self {
        Self {
            clock_hz,
            controller: controller.into(),
            entries: Vec::new(),
        }
    }

    /// Entries in one direction, in order.
    pub fn direction(&self, dir: CanDirection) -> impl Iterator<Item = &CanRecordEntry> {
        self.entries.iter().filter(move |e| e.dir == dir)
    }

    /// Serialize as JSON lines (header first).
    pub fn to_jsonl(&self) -> String {
        let mut out = serde_json::to_string(&HeaderLine {
            labwired_can_recording: 1,
            clock_hz: self.clock_hz,
            controller: self.controller.clone(),
        })
        .unwrap_or_default();
        out.push('\n');
        for e in &self.entries {
            let line = FrameLine {
                cycle: e.cycle,
                dir: e.dir,
                id: e.frame.id,
                ext: e.frame.extended,
                fd: e.frame.fd,
                brs: e.frame.bitrate_switch,
                rtr: e.frame.remote,
                data: hex_bytes(&e.frame.data),
            };
            out.push_str(&serde_json::to_string(&line).unwrap_or_default());
            out.push('\n');
        }
        out
    }

    /// Serialize as a candump log. Direction goes in the interface name
    /// (`<controller>-rx` / `<controller>-tx`); see the module docs.
    pub fn to_candump(&self) -> String {
        let iface = if self.controller.is_empty() {
            "can"
        } else {
            self.controller.as_str()
        };
        let hz = self.clock_hz.max(1) as f64;
        let mut out = String::new();
        for e in &self.entries {
            let t = e.cycle as f64 / hz;
            let id = if e.frame.extended {
                format!("{:08X}", e.frame.id)
            } else {
                format!("{:03X}", e.frame.id)
            };
            let body = if e.frame.remote {
                "R".to_string()
            } else if e.frame.fd {
                let flags = u8::from(e.frame.bitrate_switch);
                format!("#{flags:X}{}", hex_bytes(&e.frame.data).to_uppercase())
            } else {
                hex_bytes(&e.frame.data).to_uppercase()
            };
            out.push_str(&format!(
                "({t:.9}) {iface}-{} {id}#{body}\n",
                e.dir.as_str()
            ));
        }
        out
    }

    /// Parse either format (auto-detected from the first non-blank line).
    /// `clock_hz` converts candump seconds to cycles; a JSON-lines header
    /// carries its own clock and wins.
    pub fn parse(text: &str, clock_hz: u64) -> Result<Self, String> {
        let first = text.lines().map(str::trim).find(|l| !l.is_empty());
        match first {
            None => Err("the recording is empty".into()),
            Some(l) if l.starts_with('{') => Self::parse_jsonl(text),
            Some(_) => Self::parse_candump(text, clock_hz),
        }
    }

    /// Parse the JSON-lines format.
    pub fn parse_jsonl(text: &str) -> Result<Self, String> {
        let mut rec = CanRecording::default();
        let mut saw_header = false;
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if !saw_header {
                let h: HeaderLine = serde_json::from_str(line).map_err(|e| {
                    format!(
                        "recording line {n}: expected the header \
                         {{\"labwired_can_recording\":1,...}}: {e}"
                    )
                })?;
                if h.labwired_can_recording != 1 {
                    return Err(format!(
                        "recording line {n}: format version {} is not supported (only 1)",
                        h.labwired_can_recording
                    ));
                }
                rec.clock_hz = h.clock_hz;
                rec.controller = h.controller;
                saw_header = true;
                continue;
            }
            let f: FrameLine =
                serde_json::from_str(line).map_err(|e| format!("recording line {n}: {e}"))?;
            let data = parse_hex_bytes(&f.data).map_err(|e| format!("recording line {n}: {e}"))?;
            let frame = CanFrame {
                id: f.id,
                data,
                extended: f.ext,
                fd: f.fd,
                bitrate_switch: f.brs,
                remote: f.rtr,
            };
            validate_frame(&frame).map_err(|e| format!("recording line {n}: {e}"))?;
            rec.entries.push(CanRecordEntry {
                cycle: f.cycle,
                dir: f.dir,
                frame,
            });
        }
        if !saw_header {
            return Err("the recording is empty".into());
        }
        rec.entries.sort_by_key(|e| e.cycle);
        Ok(rec)
    }

    /// Parse a candump log (classic and CAN-FD lines). See the module docs for
    /// the direction and timestamp rules.
    pub fn parse_candump(text: &str, clock_hz: u64) -> Result<Self, String> {
        if clock_hz == 0 {
            return Err(
                "a candump log needs the core clock (clock_hz) to place frames in cycles".into(),
            );
        }
        let mut parsed: Vec<(f64, CanDirection, CanFrame, String)> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(ts), Some(iface), Some(body)) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(format!(
                    "candump line {n}: expected '(<ts>) <iface> <ID>#<DATA>'"
                ));
            };
            let ts: f64 = ts
                .strip_prefix('(')
                .and_then(|t| t.strip_suffix(')'))
                .ok_or_else(|| format!("candump line {n}: timestamp must be '(seconds)'"))?
                .parse()
                .map_err(|e| format!("candump line {n}: bad timestamp: {e}"))?;
            let dir = if iface == "tx" || iface.ends_with("-tx") {
                CanDirection::Tx
            } else {
                CanDirection::Rx
            };
            let frame = parse_candump_frame(body).map_err(|e| format!("candump line {n}: {e}"))?;
            parsed.push((ts, dir, frame, iface.to_string()));
        }
        if parsed.is_empty() {
            return Err("the candump log contains no frames".into());
        }
        let t0 = if parsed[0].0 >= EPOCH_THRESHOLD_S {
            parsed[0].0
        } else {
            0.0
        };
        let controller = parsed[0]
            .3
            .trim_end_matches("-rx")
            .trim_end_matches("-tx")
            .to_string();
        let mut entries: Vec<CanRecordEntry> = parsed
            .into_iter()
            .map(|(t, dir, frame, _)| CanRecordEntry {
                cycle: ((t - t0).max(0.0) * clock_hz as f64).round() as u64,
                dir,
                frame,
            })
            .collect();
        entries.sort_by_key(|e| e.cycle);
        Ok(CanRecording {
            clock_hz,
            controller,
            entries,
        })
    }
}

/// Reject a frame no controller could carry.
pub fn validate_frame(frame: &CanFrame) -> Result<(), String> {
    if frame.extended {
        if frame.id > 0x1FFF_FFFF {
            return Err(format!("extended CAN id {:#x} is out of range", frame.id));
        }
    } else if frame.id > 0x7FF {
        return Err(format!("standard CAN id {:#x} is out of range", frame.id));
    }
    let max = if frame.fd { 64 } else { 8 };
    if frame.data.len() > max {
        return Err(format!(
            "{} payload bytes exceed the {} maximum of {max}",
            frame.data.len(),
            if frame.fd { "CAN-FD" } else { "classic CAN" }
        ));
    }
    if frame.fd {
        let len = frame.data.len();
        let dlc = can_dlc(len);
        let exact = [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
        if exact[dlc as usize] != len {
            return Err(format!(
                "{len} bytes is not a CAN-FD frame length (use 0-8, 12, 16, 20, 24, 32, 48 or 64)"
            ));
        }
    }
    Ok(())
}

/// Parse the `<ID>#<DATA>` / `<ID>##<flags><DATA>` / `<ID>#R` part.
fn parse_candump_frame(body: &str) -> Result<CanFrame, String> {
    let (id_str, rest) = body
        .split_once('#')
        .ok_or_else(|| "expected '<ID>#<DATA>'".to_string())?;
    let id = u32::from_str_radix(id_str, 16).map_err(|e| format!("bad CAN id '{id_str}': {e}"))?;
    let extended = id_str.len() > 3;
    let mut frame = CanFrame {
        id,
        data: Vec::new(),
        extended,
        fd: false,
        bitrate_switch: false,
        remote: false,
    };
    if let Some(fd_rest) = rest.strip_prefix('#') {
        let mut chars = fd_rest.chars();
        let flags = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| "CAN-FD line needs a flags nibble after '##'".to_string())?;
        frame.fd = true;
        frame.bitrate_switch = flags & 1 != 0;
        frame.data = parse_hex_bytes(chars.as_str())?;
    } else if rest.starts_with('R') {
        frame.remote = true;
    } else {
        frame.data = parse_hex_bytes(rest)?;
    }
    validate_frame(&frame)?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CanRecording {
        let mut r = CanRecording::new(250_000_000, "fdcan1");
        r.entries.push(CanRecordEntry {
            cycle: 1000,
            dir: CanDirection::Rx,
            frame: CanFrame::classic(0x7E0, vec![0x03, 0x22, 0xF1, 0x90]),
        });
        r.entries.push(CanRecordEntry {
            cycle: 2500,
            dir: CanDirection::Tx,
            frame: CanFrame {
                id: 0x7E8,
                data: (0..12).collect(),
                extended: false,
                fd: true,
                bitrate_switch: true,
                remote: false,
            },
        });
        r.entries.push(CanRecordEntry {
            cycle: 4000,
            dir: CanDirection::Rx,
            frame: CanFrame {
                id: 0x18DA_F110,
                data: vec![0xAA],
                extended: true,
                fd: false,
                bitrate_switch: false,
                remote: false,
            },
        });
        r
    }

    #[test]
    fn jsonl_round_trips_exactly() {
        let r = sample();
        let text = r.to_jsonl();
        assert!(text.starts_with("{\"labwired_can_recording\":1"));
        assert_eq!(CanRecording::parse(&text, 0).unwrap(), r);
    }

    #[test]
    fn candump_round_trips_frames_direction_and_cycles() {
        let r = sample();
        let text = r.to_candump();
        assert!(text.contains("fdcan1-rx 7E0#0322F190"), "{text}");
        assert!(
            text.contains("fdcan1-tx 7E8##1000102030405060708090A0B"),
            "{text}"
        );
        let back = CanRecording::parse(&text, 250_000_000).unwrap();
        assert_eq!(back.entries, r.entries);
        assert_eq!(back.controller, "fdcan1");
    }

    #[test]
    fn a_wall_clock_candump_is_rebased_to_its_first_frame() {
        let log = "(1578925458.824500) can0 123#11\n(1578925458.825500) can0 124#22\n";
        let r = CanRecording::parse(log, 1_000_000).unwrap();
        assert_eq!(r.entries[0].cycle, 0);
        assert_eq!(r.entries[1].cycle, 1000);
        assert!(r.entries.iter().all(|e| e.dir == CanDirection::Rx));
    }

    #[test]
    fn bad_lines_name_the_line() {
        let err = CanRecording::parse("(1.0) can0 123#ABC\n", 1).unwrap_err();
        assert!(err.contains("line 1"), "{err}");
        let fd = CanRecording::parse("(1.0) can0 123##1", 1).unwrap();
        assert!(fd.entries[0].frame.fd && fd.entries[0].frame.bitrate_switch);
        let err = CanRecording::parse("(1.0) can0 123#00112233445566778899\n", 1).unwrap_err();
        assert!(err.contains("classic CAN"), "{err}");
        let err = CanRecording::parse("{\"labwired_can_recording\":2}\n", 0).unwrap_err();
        assert!(err.contains("version 2"), "{err}");
        let err = CanRecording::parse("(1.0) can0 123##10011223344556677889900\n", 1).unwrap_err();
        assert!(err.contains("not a CAN-FD frame length"), "{err}");
    }
}
