// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! PeripheralKits for host-side CAN tools formerly hand-wired in `from_config`.
//!
//! These are not SoC models — they are second-node injectors / log players that
//! hang off a named CAN controller (`connection:` → bxCAN/FDCAN id). Migrating
//! them off residual `from_config` arms keeps that match empty for product
//! devices and puts attach next to the types they construct.

use anyhow::anyhow;

use crate::bus::{CanDiagnosticTester, CanLogPlayer, CanUdsTester, SystemBus, UdsStep};
use crate::peripherals::kit::{
    AttachCtx, Category, ConfigKey, ConfigType, KitMetadata, PeripheralKit, Transport,
};

fn require_can_connection(ctx: &AttachCtx<'_>, label: &str) -> anyhow::Result<()> {
    if ctx
        .bus
        .find_peripheral_index_by_name(ctx.connection())
        .is_none()
    {
        // Preserve legacy from_config wording so tests/logs still match
        // (`can-player 'p' connection 'nope' was not found`).
        return Err(anyhow!(
            "{label} '{}' connection '{}' was not found",
            ctx.device_id(),
            ctx.connection()
        ));
    }
    Ok(())
}

// ─── can-diagnostic-tester ───────────────────────────────────────────────────

pub struct CanDiagnosticTesterKit;
pub static CAN_DIAGNOSTIC_TESTER_KIT: CanDiagnosticTesterKit = CanDiagnosticTesterKit;

static CAN_DIAGNOSTIC_METADATA: KitMetadata = KitMetadata {
    inputs: std::borrow::Cow::Borrowed(&[]),
    device_type: std::borrow::Cow::Borrowed("can-diagnostic-tester"),
    label: std::borrow::Cow::Borrowed("CAN diagnostic tester"),
    summary: std::borrow::Cow::Borrowed(
        "One-shot single-frame UDS-style request injector on bxCAN/FDCAN.",
    ),
    detail: std::borrow::Cow::Borrowed(
        "Injects a single diagnostic request frame (default 0x7E0 / ReadDataByIdentifier) \
             once the connected CAN controller is up. Alias: uds-diagnostic-tester.",
    ),
    transport: Transport::Can,
    category: Category::Misc,
    config_keys: std::borrow::Cow::Borrowed(&[
        ConfigKey {
            name: std::borrow::Cow::Borrowed("request_id"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed("CAN id for the request (default 0x7E0)."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("request_data"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed("Hex/bytes payload (default 03 22 F1 90)."),
        },
    ]),
    labs: std::borrow::Cow::Borrowed(&[]),
};

impl PeripheralKit for CanDiagnosticTesterKit {
    fn metadata(&self) -> &'static KitMetadata {
        &CAN_DIAGNOSTIC_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        require_can_connection(ctx, "CAN diagnostic tester")?;
        let request_id = SystemBus::yaml_u32(ctx.ext.config.get("request_id"), 0x7E0);
        let request_data = SystemBus::yaml_bytes(
            ctx.ext.config.get("request_data"),
            &[0x03, 0x22, 0xF1, 0x90],
        );
        ctx.bus.can_diagnostic_testers.push(CanDiagnosticTester {
            id: ctx.device_id().to_string(),
            connection: ctx.connection().to_string(),
            request_id,
            request_data,
            sent: false,
        });
        Ok(())
    }
}

// ─── uds-tester ──────────────────────────────────────────────────────────────

pub struct CanUdsTesterKit;
pub static CAN_UDS_TESTER_KIT: CanUdsTesterKit = CanUdsTesterKit;

static CAN_UDS_METADATA: KitMetadata = KitMetadata {
    inputs: std::borrow::Cow::Borrowed(&[]),
    device_type: std::borrow::Cow::Borrowed("uds-tester"),
    label: std::borrow::Cow::Borrowed("UDS / ISO-TP tester"),
    summary: std::borrow::Cow::Borrowed(
        "Stateful multi-frame UDS tester (SecurityAccess-class handshakes).",
    ),
    detail: std::borrow::Cow::Borrowed(
        "Second CAN node that drives ISO-TP First/Consecutive frames and \
             observes ECU responses via the public bxCAN/FDCAN inject API. \
             Optional `script:` steps; legacy first_frame/consecutive_frame still work.",
    ),
    transport: Transport::Can,
    category: Category::Misc,
    config_keys: std::borrow::Cow::Borrowed(&[
        ConfigKey {
            name: std::borrow::Cow::Borrowed("request_id"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed("Tester → ECU id (default 0x111)."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("reply_id"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed("ECU → tester id (default 0x222)."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("first_frame"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "Legacy ISO-TP FirstFrame bytes when script is omitted.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("consecutive_frame"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "Legacy ISO-TP ConsecutiveFrame bytes when script is omitted.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("script"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "YAML list of {send, expect, expect_nrc, delay_us} steps.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("flow_control"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "ISO-TP FlowControl sent after an ECU FirstFrame (default 30 00 00). \
                 Set 32 00 00 (overflow) or \"\" (none) to inject a fault.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("response_timeout_us"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed(
                "Per-request response timeout in microseconds of virtual time \
                 (P2 plus the whole transfer). Off by default.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("retries"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed(
                "Times a timed-out request is sent again before the tester fails (default 0).",
            ),
        },
    ]),
    labs: std::borrow::Cow::Borrowed(&[]),
};

impl PeripheralKit for CanUdsTesterKit {
    fn metadata(&self) -> &'static KitMetadata {
        &CAN_UDS_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        require_can_connection(ctx, "UDS tester")?;
        let mut tester =
            CanUdsTester::new(ctx.device_id().to_string(), ctx.connection().to_string());
        tester.request_id = SystemBus::yaml_u32(
            ctx.ext.config.get("request_id"),
            CanUdsTester::DEFAULT_REQUEST_ID,
        );
        tester.reply_id = SystemBus::yaml_u32(
            ctx.ext.config.get("reply_id"),
            CanUdsTester::DEFAULT_REPLY_ID,
        );
        tester.first_frame = SystemBus::yaml_bytes(
            ctx.ext.config.get("first_frame"),
            &CanUdsTester::DEFAULT_FIRST_FRAME,
        );
        tester.consecutive_frame = SystemBus::yaml_bytes(
            ctx.ext.config.get("consecutive_frame"),
            &CanUdsTester::DEFAULT_CONSECUTIVE_FRAME,
        );
        tester.script = SystemBus::parse_script(ctx.ext.config.get("script"));
        if let Some(us) = ctx.config_i64("response_timeout_us") {
            if us <= 0 {
                return Err(anyhow!(
                    "UDS tester '{}': response_timeout_us must be above 0",
                    ctx.device_id()
                ));
            }
            let hz = ctx.bus.cpu_hz;
            tester.response_timeout_cycles =
                Some(((us as u128 * hz as u128) / 1_000_000).max(1) as u64);
        }
        if let Some(r) = ctx.config_i64("retries") {
            tester.retries = u32::try_from(r).map_err(|_| {
                anyhow!(
                    "UDS tester '{}': retries must be 0 or more",
                    ctx.device_id()
                )
            })?;
        }
        tester.flow_control = SystemBus::yaml_bytes(
            ctx.ext.config.get("flow_control"),
            &CanUdsTester::DEFAULT_FLOW_CONTROL,
        );
        // When no `script:` key is present, synthesize a single step from the
        // legacy first_frame / consecutive_frame fields.
        if !ctx.ext.config.contains_key("script") {
            let ff = &tester.first_frame;
            let pdu_len = if ff.len() >= 2 {
                (((ff[0] & 0x0F) as usize) << 8) | (ff[1] as usize)
            } else {
                0
            };
            if ctx.ext.config.contains_key("first_frame") && (ff.len() < 2 || pdu_len == 0) {
                tracing::warn!(
                    "[uds-tester] '{}': first_frame is too short or decodes pdu_len=0 \
                     — synthesized send will be empty",
                    ctx.device_id()
                );
            }
            let ff_payload: &[u8] = if ff.len() >= 2 { &ff[2..] } else { &[] };
            let cf_payload: &[u8] = if !tester.consecutive_frame.is_empty() {
                &tester.consecutive_frame[1..]
            } else {
                &[]
            };
            let raw: Vec<u8> = ff_payload
                .iter()
                .chain(cf_payload.iter())
                .copied()
                .take(pdu_len)
                .collect();
            if raw.is_empty() && ctx.ext.config.contains_key("first_frame") {
                tracing::warn!(
                    "[uds-tester] '{}': reassembled send payload is empty \
                     — check first_frame / consecutive_frame config",
                    ctx.device_id()
                );
            }
            tester.script = vec![UdsStep {
                send: raw,
                expect: vec![Some(0x06), Some(0x67)],
                expect_nrc: None,
                delay_us: 0,
            }];
        }
        ctx.bus.can_uds_testers.push(tester);
        Ok(())
    }
}

// ─── can-player ──────────────────────────────────────────────────────────────

pub struct CanLogPlayerKit;
pub static CAN_LOG_PLAYER_KIT: CanLogPlayerKit = CanLogPlayerKit;

static CAN_LOG_PLAYER_METADATA: KitMetadata = KitMetadata {
    inputs: std::borrow::Cow::Borrowed(&[]),
    device_type: std::borrow::Cow::Borrowed("can-player"),
    label: std::borrow::Cow::Borrowed("CAN log player"),
    summary: std::borrow::Cow::Borrowed(
        "Replays candump-format traffic into a bxCAN/FDCAN controller.",
    ),
    detail: std::borrow::Cow::Borrowed(
        "Host-side log player for J1939 / bus-monitor labs. Requires inline \
             `data:` (candump text) and optional ticks_per_second.",
    ),
    transport: Transport::Can,
    category: Category::Misc,
    config_keys: std::borrow::Cow::Borrowed(&[
        ConfigKey {
            name: std::borrow::Cow::Borrowed("data"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed("Inline candump .log text (required)."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("ticks_per_second"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed("Sim ticks per log second (default 1_000_000)."),
        },
    ]),
    labs: std::borrow::Cow::Borrowed(&[]),
};

impl PeripheralKit for CanLogPlayerKit {
    fn metadata(&self) -> &'static KitMetadata {
        &CAN_LOG_PLAYER_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        require_can_connection(ctx, "can-player")?;
        let Some(data) = ctx.config_str("data") else {
            return Err(anyhow!(
                "can-player '{}': set 'path' (a candump .log file) or inline 'data'",
                ctx.device_id()
            ));
        };
        let tps = SystemBus::yaml_u32(ctx.ext.config.get("ticks_per_second"), 1_000_000) as u64;
        let player = CanLogPlayer::from_candump(
            ctx.device_id().to_string(),
            ctx.connection().to_string(),
            data,
            tps,
        )
        .map_err(|e| anyhow!(e))?;
        ctx.bus.can_log_players.push(player);
        Ok(())
    }
}

// ─── can-bridge ──────────────────────────────────────────────────────────────

pub struct CanBridgeKit;
pub static CAN_BRIDGE_KIT: CanBridgeKit = CanBridgeKit;

static CAN_BRIDGE_METADATA: KitMetadata = KitMetadata {
    inputs: std::borrow::Cow::Borrowed(&[]),
    device_type: std::borrow::Cow::Borrowed("can-bridge"),
    label: std::borrow::Cow::Borrowed("CAN bridge (pause modes, record/replay, faults)"),
    summary: std::borrow::Cow::Borrowed(
        "Boundary between a CAN controller and traffic from outside the simulation.",
    ),
    detail: std::borrow::Cow::Borrowed(
        "Records every frame the controller exchanges (virtual time), says exactly what \
         happens to live frames while the simulation is paused (drop / capture / replay), \
         replays a recording on virtual time and compares the firmware's answers, and \
         injects CAN-path faults (can_drop, can_delay, can_bus_off, node_reset). \
         Buffering during a pause does not preserve real-time interaction with a physical device.",
    ),
    transport: Transport::Can,
    category: Category::Misc,
    config_keys: std::borrow::Cow::Borrowed(&[
        ConfigKey {
            name: std::borrow::Cow::Borrowed("pause_mode"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed("drop (default) | capture | replay."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("capture_capacity"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed("Capture queue bound (default 256)."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("overflow"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed("drop_newest (default) | drop_oldest."),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("bitrate"),
            ty: ConfigType::Int,
            doc: std::borrow::Cow::Borrowed(
                "Nominal bit rate; spaces frames released after a pause (default 500000).",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("recording"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "Recording to replay: JSON lines or a candump log (replay mode).",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("recording_path"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "File holding the recording, relative to the system manifest.",
            ),
        },
        ConfigKey {
            name: std::borrow::Cow::Borrowed("faults"),
            ty: ConfigType::Str,
            doc: std::borrow::Cow::Borrowed(
                "List of {at_cycle, kind, ...} faults (the fault-injection schema).",
            ),
        },
    ]),
    labs: std::borrow::Cow::Borrowed(&[]),
};

/// Parse a bridge from an external-device `config:` map. Shared with the
/// wasm API, which takes the same keys as JSON.
pub fn can_bridge_from_config(
    id: &str,
    controller: &str,
    clock_hz: u64,
    config: &serde_json::Value,
) -> anyhow::Result<crate::network::can_bridge::CanBridge> {
    use crate::network::can_bridge::{CanBridge, CanBridgeConfig};
    use crate::network::can_recording::CanRecording;
    let mut map = config.as_object().cloned().unwrap_or_default();
    let recording_text = match map.remove("recording") {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Null) | None => None,
        Some(_) => {
            return Err(anyhow!(
                "can-bridge '{id}': recording must be text (JSON lines or a candump log)"
            ))
        }
    };
    if map.remove("recording_path").is_some() && recording_text.is_none() {
        return Err(anyhow!(
            "can-bridge '{id}': recording_path is resolved when the system manifest is loaded \
             from a file; pass the recording text as `recording` instead"
        ));
    }
    let cfg: CanBridgeConfig = serde_json::from_value(serde_json::Value::Object(map))
        .map_err(|e| anyhow!("can-bridge '{id}': {e}"))?;
    let recording = match recording_text {
        Some(text) => Some(
            CanRecording::parse(&text, clock_hz).map_err(|e| anyhow!("can-bridge '{id}': {e}"))?,
        ),
        None => None,
    };
    CanBridge::new(id, controller, clock_hz, cfg, recording).map_err(|e| anyhow!(e))
}

impl PeripheralKit for CanBridgeKit {
    fn metadata(&self) -> &'static KitMetadata {
        &CAN_BRIDGE_METADATA
    }

    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        require_can_connection(ctx, "can-bridge")?;
        let yaml = serde_yaml::Value::Mapping(
            ctx.ext
                .config
                .iter()
                .map(|(k, v)| (serde_yaml::Value::String(k.clone()), v.clone()))
                .collect(),
        );
        let json: serde_json::Value = serde_json::to_value(&yaml)
            .map_err(|e| anyhow!("can-bridge '{}': config: {e}", ctx.device_id()))?;
        let bridge =
            can_bridge_from_config(ctx.device_id(), ctx.connection(), ctx.bus.cpu_hz, &json)?;
        ctx.bus.attach_can_bridge(bridge).map_err(|e| anyhow!(e))
    }
}
