// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! CAN bridge in the browser: pause modes, live frames, record/replay and the
//! failure timeline. See `labwired_core::network::can_bridge`.
//!
//! The playground's run loop is the "live source" and the pause switch: it
//! calls [`WasmSimulator::can_bridge_set_paused`] when the user pauses or a
//! breakpoint halts the run, and [`WasmSimulator::can_bridge_offer`] for a
//! frame typed in or arriving from a host bridge (WebSocket, WebSerial). A
//! bridge declared in the system manifest (`type: can-bridge`) is here too.
//!
//! Every call that changes the machine is journaled, so snapshot/restore and
//! fault experiments replay it exactly.

use crate::lab_tools::Op;
use crate::WasmSimulator;
use labwired_core::network::can_recording::{hex_bytes, parse_hex_bytes, CanRecording};
use labwired_core::network::CanFrame;
use wasm_bindgen::prelude::*;

fn js(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// A frame as JS passes it: `{"id": 2016 | "0x7E0", "data": "0322f190",
/// "ext": false, "fd": false, "brs": false}`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JsFrame {
    id: serde_json::Value,
    #[serde(default)]
    data: String,
    #[serde(default)]
    ext: bool,
    #[serde(default)]
    fd: bool,
    #[serde(default)]
    brs: bool,
}

pub(crate) fn parse_js_frame(json: &str) -> Result<CanFrame, String> {
    let f: JsFrame = serde_json::from_str(json).map_err(|e| format!("CAN frame: {e}"))?;
    let id = match &f.id {
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| format!("CAN id {n} is out of range"))?,
        serde_json::Value::String(s) => {
            let t = s.trim();
            match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(h) => u32::from_str_radix(h, 16),
                None => u32::from_str_radix(t, 16),
            }
            .map_err(|_| format!("'{s}' is not a CAN id"))?
        }
        other => {
            return Err(format!(
                "CAN id must be a number or a hex string, got {other}"
            ))
        }
    };
    let frame = CanFrame {
        id,
        data: parse_hex_bytes(&f.data)?,
        extended: f.ext,
        fd: f.fd,
        bitrate_switch: f.brs,
        remote: false,
    };
    labwired_core::network::can_recording::validate_frame(&frame)?;
    Ok(frame)
}

impl WasmSimulator {
    fn bus_mut(&mut self) -> Result<&mut labwired_core::bus::SystemBus, String> {
        self.machine
            .as_mut()
            .map(|m| &mut m.bus)
            .ok_or_else(|| "no machine".to_string())
    }

    fn bus_ref(&self) -> Result<&labwired_core::bus::SystemBus, String> {
        self.machine
            .as_ref()
            .map(|m| &m.bus)
            .ok_or_else(|| "no machine".to_string())
    }

    pub(crate) fn can_bridge_attach_s(
        &mut self,
        id: &str,
        controller: &str,
        config_json: &str,
    ) -> Result<(), String> {
        let config: serde_json::Value =
            serde_json::from_str(config_json).map_err(|e| format!("can-bridge config: {e}"))?;
        let bus = self.bus_mut()?;
        let bridge = labwired_core::peripherals::components::can_testers::can_bridge_from_config(
            id, controller, bus.cpu_hz, &config,
        )
        .map_err(|e| format!("{e:#}"))?;
        bus.attach_can_bridge(bridge)?;
        self.record(Op::CanBridgeAttach(
            id.to_string(),
            controller.to_string(),
            config_json.to_string(),
        ));
        Ok(())
    }

    pub(crate) fn can_bridge_offer_s(
        &mut self,
        id: &str,
        frame_json: &str,
        host_time_ms: Option<f64>,
    ) -> Result<String, String> {
        let frame = parse_js_frame(frame_json)?;
        let out = self.bus_mut()?.can_bridge_offer(id, frame, host_time_ms)?;
        self.record(Op::CanBridgeOffer(
            id.to_string(),
            frame_json.to_string(),
            host_time_ms,
        ));
        Ok(serde_json::to_value(out)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default())
    }
}

#[wasm_bindgen]
impl WasmSimulator {
    /// Attach a CAN bridge to controller `controller` (e.g. `"fdcan1"`).
    /// `config_json` takes the keys of a `can-bridge` in a system manifest:
    /// `pause_mode` (`drop` | `capture` | `replay`), `capture_capacity`,
    /// `overflow` (`drop_newest` | `drop_oldest`), `bitrate`, `faults`
    /// (the fault-injection schema) and `recording` (JSON-lines or candump
    /// text; replay mode).
    #[wasm_bindgen]
    pub fn can_bridge_attach(
        &mut self,
        id: &str,
        controller: &str,
        config_json: &str,
    ) -> Result<(), JsValue> {
        self.can_bridge_attach_s(id, controller, config_json)
            .map_err(js)
    }

    /// Ids and controllers of the attached bridges, as JSON
    /// `[{"id","controller","pause_mode","paused"}]`.
    #[wasm_bindgen]
    pub fn can_bridge_list(&self) -> Result<String, JsValue> {
        let bus = self.bus_ref().map_err(js)?;
        Ok(serde_json::Value::Array(
            bus.can_bridges
                .iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id,
                        "controller": b.controller,
                        "pause_mode": b.pause_mode().as_str(),
                        "paused": b.is_paused(),
                    })
                })
                .collect(),
        )
        .to_string())
    }

    /// Tell every bridge the run stopped (`true`: paused, halted at a
    /// breakpoint) or resumed (`false`). What happens to live frames while
    /// paused is each bridge's `pause_mode`.
    #[wasm_bindgen]
    pub fn can_bridge_set_paused(&mut self, paused: bool) -> Result<(), JsValue> {
        self.bus_mut().map_err(js)?.can_bridges_set_paused(paused);
        self.record(Op::CanBridgeSetPaused(paused));
        Ok(())
    }

    /// A live frame from outside the simulation for bridge `id`. `frame_json`
    /// is `{"id": 2016 | "0x7E0", "data": "0322f190", "ext", "fd", "brs"}`;
    /// `host_time_ms` is the wall-clock time it arrived (recorded only).
    /// Returns `"queued"`, `"captured"` or `"dropped"`.
    #[wasm_bindgen]
    pub fn can_bridge_offer(
        &mut self,
        id: &str,
        frame_json: &str,
        host_time_ms: Option<f64>,
    ) -> Result<String, JsValue> {
        self.can_bridge_offer_s(id, frame_json, host_time_ms)
            .map_err(js)
    }

    /// The bridge's report as JSON: delivered/dropped frames (every dropped
    /// frame, exactly), capture queue, replay verdict, faults fired, and the
    /// failure timeline (frames, faults, console lines, tester progress on one
    /// cycle axis). The same block `labwired test` writes into `result.json`.
    #[wasm_bindgen]
    pub fn can_bridge_report(&self, id: &str) -> Result<String, JsValue> {
        let bus = self.bus_ref().map_err(js)?;
        let report = bus
            .can_bridge_report(id)
            .ok_or_else(|| js(format!("no can-bridge named '{id}'")))?;
        serde_json::to_string(&report).map_err(js)
    }

    /// Everything the bridge recorded, as `"jsonl"` (the replay format) or
    /// `"candump"` text.
    #[wasm_bindgen]
    pub fn can_bridge_recording(&self, id: &str, format: &str) -> Result<String, JsValue> {
        let bus = self.bus_ref().map_err(js)?;
        let b = bus
            .can_bridge(id)
            .ok_or_else(|| js(format!("no can-bridge named '{id}'")))?;
        match format {
            "jsonl" => Ok(b.recording().to_jsonl()),
            "candump" => Ok(b.recording().to_candump()),
            other => Err(js(format!(
                "unknown recording format '{other}' (jsonl or candump)"
            ))),
        }
    }
}

/// Parse a recording (JSON lines or candump) and summarize it as JSON:
/// `{"frames","rx","tx","first_cycle","last_cycle","clock_hz","controller"}`,
/// or throw with the line that failed. For a UI to check a file before
/// replaying it.
#[wasm_bindgen]
pub fn can_recording_summary(text: &str, clock_hz: f64) -> Result<String, JsValue> {
    let rec = CanRecording::parse(text, clock_hz.max(0.0) as u64).map_err(js)?;
    let rx = rec
        .entries
        .iter()
        .filter(|e| e.dir == labwired_core::network::can_recording::CanDirection::Rx)
        .count();
    Ok(serde_json::json!({
        "frames": rec.entries.len(),
        "rx": rx,
        "tx": rec.entries.len() - rx,
        "first_cycle": rec.entries.first().map(|e| e.cycle),
        "last_cycle": rec.entries.last().map(|e| e.cycle),
        "clock_hz": rec.clock_hz,
        "controller": rec.controller,
        "first_rx_data": rec
            .entries
            .iter()
            .find(|e| e.dir == labwired_core::network::can_recording::CanDirection::Rx)
            .map(|e| hex_bytes(&e.frame.data)),
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_frames_parse_numbers_and_hex_ids() {
        let f = parse_js_frame(r#"{"id":"0x7E0","data":"03 22 F1 90"}"#).unwrap();
        assert_eq!(
            (f.id, f.data.clone()),
            (0x7E0, vec![0x03, 0x22, 0xF1, 0x90])
        );
        assert_eq!(parse_js_frame(r#"{"id":2016}"#).unwrap().id, 0x7E0);
        assert!(parse_js_frame(r#"{"id":"0x800","data":"00"}"#).is_err());
        assert!(parse_js_frame(r#"{"id":1,"data":"00","bogus":1}"#).is_err());
    }
}
