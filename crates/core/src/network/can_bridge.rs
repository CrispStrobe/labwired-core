// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! A CAN bridge: the boundary between one simulated CAN controller and CAN
//! traffic from outside the simulation, with explicit behaviour while the
//! simulation is paused, record/replay on the virtual time axis, and faults on
//! the CAN path.
//!
//! ## What "outside" and "paused" mean
//!
//! Frames reach a controller from a scripted tester (`uds-tester`), a log
//! player, a replayed recording, or a *live* source the host feeds through
//! [`CanBridge::offer`] (a SocketCAN or network bridge, a browser tab, a test
//! harness). A live source runs on wall-clock time; the simulation runs on
//! virtual time and stops advancing whenever it is paused or halted under a
//! debugger. The host tells the bridge when that happens
//! ([`CanBridge::set_paused`]). While paused, the bridge does exactly what its
//! [`CanPauseMode`] says, and records it:
//!
//! * [`CanPauseMode::Drop`]: live frames are discarded. Each one is recorded
//!   (cycle, host time, direction, ID, DLC, data) and appears in the report and
//!   on the timeline, so a lost frame is never silent.
//! * [`CanPauseMode::Capture`]: live frames go into a bounded, timestamped
//!   queue (`capture_capacity`). When it is full the configured
//!   [`CanOverflowPolicy`] discards either the new frame or the oldest queued
//!   one, and that is recorded too. On resume the queued frames are released
//!   in arrival order, spaced by one frame time at the configured `bitrate`
//!   so the controller's receive FIFO is not flooded in one instant.
//! * [`CanPauseMode::Replay`]: live frames are ignored (and recorded as
//!   ignored). The bridge instead injects the `rx` frames of a
//!   [`CanRecording`] at their recorded cycles, so the run is independent of
//!   any live source and of pauses: virtual time does not move while paused,
//!   so a replayed frame is never early or late.
//!
//! **Buffering during a pause does not preserve real-time interaction with a
//! physical device.** A captured frame is delivered at the virtual time the
//! simulation resumes, not at the time it arrived. The physical peer saw no
//! reply while the simulation was paused and may already have timed out,
//! retried or given up. CAPTURE keeps the frames; it cannot keep the timing.
//! Use REPLAY to make an interaction repeatable, and keep the simulation
//! running (not paused) when a physical device needs timely replies.
//!
//! ## Faults on the CAN path
//!
//! A bridge takes the same [`ScheduledFault`] list as the lockstep fault
//! engine ([`crate::vfi`]). The CAN kinds (`can_drop`, `can_delay`,
//! `can_bus_off`, and `node_reset` with a `frame` trigger) act here; a plain
//! `node_reset` fires at its cycle. Every fault that fires is recorded with
//! the cycle it fired at and what it did.

use super::can_recording::{can_dlc, hex_bytes, parse_hex_bytes, validate_frame};
pub use super::can_recording::{CanDirection, CanRecordEntry, CanRecording};
use super::CanFrame;
use crate::vfi::{FaultAction, ScheduledFault};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// What a bridge does with live frames while the simulation is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanPauseMode {
    #[default]
    Drop,
    Capture,
    Replay,
}

impl CanPauseMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CanPauseMode::Drop => "drop",
            CanPauseMode::Capture => "capture",
            CanPauseMode::Replay => "replay",
        }
    }
}

/// Which frame a full capture queue discards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanOverflowPolicy {
    /// Keep the queue, discard the frame that did not fit.
    #[default]
    DropNewest,
    /// Discard the oldest queued frame to make room.
    DropOldest,
}

/// Selects frames for a CAN-path fault. Every field that is set must match.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanFrameMatch {
    /// `rx` (into the controller) or `tx` (out of it). Both when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<CanDirection>,
    /// CAN identifier: a number or a `"0x7E8"` string.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_opt_u32"
    )]
    pub id: Option<u32>,
    /// Hex bytes the payload must start with, e.g. `"21"` for the first
    /// ISO-TP consecutive frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_prefix: Option<String>,
    /// Let this many matching frames pass before the fault acts.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub skip: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

fn de_opt_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
    }
    match Option::<NumOrStr>::deserialize(d)? {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => u32::try_from(n)
            .map(Some)
            .map_err(|_| serde::de::Error::custom(format!("CAN id {n} is out of range"))),
        Some(NumOrStr::Str(s)) => {
            let t = s.trim();
            let parsed = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u32::from_str_radix(h, 16)
            } else {
                t.parse::<u32>()
            };
            parsed
                .map(Some)
                .map_err(|_| serde::de::Error::custom(format!("'{s}' is not a CAN id")))
        }
    }
}

/// Bridge configuration (the `config:` of a `can-bridge` external device).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanBridgeConfig {
    #[serde(default)]
    pub pause_mode: CanPauseMode,
    /// Capture queue bound (CAPTURE mode).
    #[serde(default = "default_capacity")]
    pub capture_capacity: usize,
    #[serde(default)]
    pub overflow: CanOverflowPolicy,
    /// Nominal bit rate, used only to space frames released after a pause.
    #[serde(default = "default_bitrate")]
    pub bitrate: u32,
    /// Faults on this bridge's CAN path, in the lockstep engine's schema.
    #[serde(default)]
    pub faults: Vec<ScheduledFault>,
    /// Most frames kept in the recording; older ones are counted, not kept.
    #[serde(default = "default_record_limit")]
    pub record_limit: usize,
}

fn default_capacity() -> usize {
    256
}
fn default_bitrate() -> u32 {
    500_000
}
fn default_record_limit() -> usize {
    100_000
}

impl Default for CanBridgeConfig {
    fn default() -> Self {
        Self {
            pause_mode: CanPauseMode::Drop,
            capture_capacity: default_capacity(),
            overflow: CanOverflowPolicy::DropNewest,
            bitrate: default_bitrate(),
            faults: Vec::new(),
            record_limit: default_record_limit(),
        }
    }
}

/// Where a frame came from, as the timeline names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanFrameSource {
    /// A live frame the host offered while the simulation was running.
    Live,
    /// A live frame captured during a pause, released after resume.
    Captured,
    /// A frame from the replayed recording.
    Replay,
    /// A scripted tester on the same controller (`uds-tester`).
    Tester,
    /// A log player on the same controller (`can-player`).
    Player,
    /// The controller itself (a `tx` frame).
    Controller,
}

impl CanFrameSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CanFrameSource::Live => "live",
            CanFrameSource::Captured => "captured",
            CanFrameSource::Replay => "replay",
            CanFrameSource::Tester => "tester",
            CanFrameSource::Player => "player",
            CanFrameSource::Controller => "controller",
        }
    }
}

/// Why a frame did not arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanDropReason {
    /// DROP mode: the frame arrived while the simulation was paused.
    PausedDrop,
    /// CAPTURE mode: the queue was full.
    CaptureOverflow,
    /// REPLAY mode: live frames are ignored.
    ReplayIgnoresLive,
    /// The controller refused it (not clocked, in INIT, FIFO full, ...).
    ControllerRejected,
    /// A `can_drop` fault removed it.
    Fault,
}

/// One frame that did not arrive, recorded exactly.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DroppedCanFrame {
    pub cycle: u64,
    pub t_us: f64,
    /// Wall-clock time the host gave with the frame, when it gave one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_time_ms: Option<f64>,
    pub dir: CanDirection,
    pub source: CanFrameSource,
    pub reason: CanDropReason,
    pub detail: String,
    pub id: u32,
    pub extended: bool,
    pub fd: bool,
    pub dlc: u8,
    pub data: String,
}

/// Which lane of the timeline an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanTimelineLane {
    /// A frame crossed the controller boundary.
    Can,
    /// Pause/resume, capture, drop, overflow: what the bridge did.
    Bridge,
    /// A fault fired.
    Fault,
    /// A console line the firmware printed.
    Console,
    /// A scripted tester's progress (request sent, reply, timeout, retry).
    Tester,
}

/// One event on the failure timeline. Everything shares one time axis:
/// `cycle` (exact) and `t_us` (cycle / core clock).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanTimelineEvent {
    pub cycle: u64,
    pub t_us: f64,
    pub lane: CanTimelineLane,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<CanDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CanFrameSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlc: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    pub detail: String,
}

/// A fault that fired on the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanFaultFired {
    /// Index into the bridge's `faults`.
    pub index: usize,
    pub kind: String,
    pub at_cycle: u64,
    pub applied_cycle: u64,
    pub detail: String,
}

/// What a fault asks the machine around the bridge to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanBridgeAction {
    /// Reset the node (fault index).
    NodeReset(usize),
    /// Force the controller into bus-off (fault index).
    BusOff(usize),
}

/// What happened to a frame on the CAN path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanPathVerdict {
    /// Pass it on now.
    Deliver(CanFrame),
    /// A delay fault holds it; it comes back from `due_rx` / `due_tx`.
    Held,
    /// A drop fault removed it.
    Dropped,
}

/// What [`CanBridge::offer`] did with a live frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanOfferOutcome {
    /// Delivered on the next bridge service (the simulation is running).
    Queued,
    /// Held in the capture queue until the simulation resumes.
    Captured,
    /// Discarded; recorded in `dropped`.
    Dropped,
}

/// A frame one side of the bridge still has to deliver.
#[derive(Debug, Clone)]
struct Pending {
    frame: CanFrame,
    host_time_ms: Option<f64>,
    offered_cycle: u64,
}

#[derive(Debug, Clone)]
struct Held {
    due_cycle: u64,
    dir: CanDirection,
    source: CanFrameSource,
    frame: CanFrame,
}

#[derive(Debug, Clone)]
struct ArmedFault {
    fault: ScheduledFault,
    prefix: Vec<u8>,
    skip_left: u32,
    remaining: u32,
    done: bool,
}

#[derive(Debug, Clone)]
struct ReplayState {
    rx: Vec<(u64, CanFrame)>,
    next_rx: usize,
    expected_tx: Vec<(u64, CanFrame)>,
    observed_tx: Vec<(u64, CanFrame)>,
    injected: u64,
    rejected: u64,
}

/// Capture queue accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanCaptureReport {
    pub capacity: usize,
    pub overflow: CanOverflowPolicy,
    pub queued_now: usize,
    pub captured_total: u64,
    pub released_total: u64,
    pub overflowed_total: u64,
}

/// A frame as the replay comparison shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanFrameView {
    pub cycle: u64,
    pub id: u32,
    pub extended: bool,
    pub fd: bool,
    pub dlc: u8,
    pub data: String,
}

impl CanFrameView {
    fn of(cycle: u64, f: &CanFrame) -> Self {
        Self {
            cycle,
            id: f.id,
            extended: f.extended,
            fd: f.fd,
            dlc: can_dlc(f.data.len()),
            data: hex_bytes(&f.data),
        }
    }
}

/// The first place a replay's output differed from the recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanReplayMismatch {
    /// Index among the `tx` frames.
    pub index: usize,
    pub expected: Option<CanFrameView>,
    pub observed: Option<CanFrameView>,
}

/// How a replay went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CanReplayVerdict {
    /// Every recorded frame was injected and the firmware transmitted exactly
    /// the recorded frames, in order.
    Match,
    /// The firmware transmitted a different frame, or an extra one.
    Mismatch,
    /// No difference so far, but the run ended before every recorded frame
    /// was injected or answered.
    Incomplete,
}

/// Replay result: what was injected and how the firmware's output compares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanReplayReport {
    pub verdict: CanReplayVerdict,
    pub rx_total: usize,
    pub rx_injected: u64,
    pub rx_rejected: u64,
    pub expected_tx: usize,
    pub observed_tx: usize,
    /// Leading `tx` frames equal to the recording (content and order).
    pub matched_tx: usize,
    /// Largest cycle difference between a matched frame and its recording.
    pub max_cycle_skew: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_mismatch: Option<CanReplayMismatch>,
}

/// Everything a bridge did during a run. Part of the run result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanBridgeReport {
    pub id: String,
    pub controller: String,
    pub pause_mode: CanPauseMode,
    pub paused: bool,
    pub clock_hz: u64,
    /// Frames the controller accepted from outside, by source.
    pub delivered_rx: u64,
    /// Frames the controller transmitted.
    pub observed_tx: u64,
    pub dropped_total: u64,
    /// Every dropped frame (the most recent `record_limit`).
    pub dropped: Vec<DroppedCanFrame>,
    pub capture: CanCaptureReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<CanReplayReport>,
    pub faults: Vec<CanFaultFired>,
    /// Faults that never fired (index, kind, why).
    pub faults_not_fired: Vec<String>,
    pub recorded_frames: usize,
    pub recording_truncated: u64,
    pub timeline: Vec<CanTimelineEvent>,
    pub timeline_truncated: u64,
    /// The one-paragraph honesty note about pauses, repeated in every report.
    pub pause_note: &'static str,
}

/// See the module docs.
pub const PAUSE_NOTE: &str =
    "Buffering during a pause does not preserve real-time interaction with \
a physical device: captured frames are delivered when the simulation resumes, not when they \
arrived, and the physical peer saw no reply while the simulation was paused.";

const TIMELINE_LIMIT: usize = 50_000;

/// See the module docs.
#[derive(Debug, Clone)]
pub struct CanBridge {
    pub id: String,
    /// Name of the bridged controller (`fdcan1`, `bxcan1`, ...).
    pub controller: String,
    config: CanBridgeConfig,
    clock_hz: u64,
    paused: bool,
    live_inbox: VecDeque<Pending>,
    capture: VecDeque<Pending>,
    release_next_cycle: u64,
    captured_total: u64,
    released_total: u64,
    overflowed_total: u64,
    dropped: VecDeque<DroppedCanFrame>,
    dropped_total: u64,
    delivered_rx: u64,
    observed_tx: u64,
    replay: Option<ReplayState>,
    armed: Vec<ArmedFault>,
    fired: Vec<CanFaultFired>,
    actions: Vec<CanBridgeAction>,
    held: Vec<Held>,
    /// Frames transmitted by the controller this service round, after the
    /// CAN-path faults, for a tester on the same controller.
    pub(crate) tx_ready: Vec<CanFrame>,
    recording: CanRecording,
    recording_truncated: u64,
    timeline: Vec<CanTimelineEvent>,
    timeline_truncated: u64,
    /// Bus-trace sequence number up to which console bytes were read.
    pub(crate) console_seq: u64,
    /// Console line being assembled: (cycle of its first byte, bytes).
    pub(crate) console_partial: Option<(u64, Vec<u8>)>,
}

impl CanBridge {
    /// Build a bridge. `replay` is required in REPLAY mode and refused
    /// otherwise, so a recording is never silently ignored.
    pub fn new(
        id: impl Into<String>,
        controller: impl Into<String>,
        clock_hz: u64,
        config: CanBridgeConfig,
        replay: Option<CanRecording>,
    ) -> Result<Self, String> {
        let id = id.into();
        let controller = controller.into();
        if config.capture_capacity == 0 {
            return Err(format!(
                "can-bridge '{id}': capture_capacity must be at least 1"
            ));
        }
        if config.bitrate == 0 {
            return Err(format!("can-bridge '{id}': bitrate must be above 0"));
        }
        let replay = match (config.pause_mode, replay) {
            (CanPauseMode::Replay, None) => {
                return Err(format!(
                    "can-bridge '{id}': pause_mode replay needs a recording \
                     (`recording:` inline text or `recording_path:`)"
                ))
            }
            (CanPauseMode::Replay, Some(rec)) => {
                let mut rx = Vec::new();
                let mut expected_tx = Vec::new();
                for e in rec.entries {
                    // A recording made on another clock is rescaled to this one.
                    let cycle = if rec.clock_hz != 0 && clock_hz != 0 && rec.clock_hz != clock_hz {
                        ((e.cycle as u128 * clock_hz as u128) / rec.clock_hz as u128) as u64
                    } else {
                        e.cycle
                    };
                    match e.dir {
                        CanDirection::Rx => rx.push((cycle, e.frame)),
                        CanDirection::Tx => expected_tx.push((cycle, e.frame)),
                    }
                }
                if rx.is_empty() {
                    return Err(format!(
                        "can-bridge '{id}': the recording has no rx frames to replay"
                    ));
                }
                Some(ReplayState {
                    rx,
                    next_rx: 0,
                    expected_tx,
                    observed_tx: Vec::new(),
                    injected: 0,
                    rejected: 0,
                })
            }
            (mode, Some(_)) => {
                return Err(format!(
                    "can-bridge '{id}': a recording is only used by pause_mode replay \
                     (this bridge is '{}')",
                    mode.as_str()
                ))
            }
            (_, None) => None,
        };
        let mut armed = Vec::new();
        for (i, f) in config.faults.iter().enumerate() {
            let frame = match &f.action {
                FaultAction::CanDrop { frame, count }
                | FaultAction::CanDelay { frame, count, .. } => {
                    if *count == 0 {
                        return Err(format!(
                            "can-bridge '{id}': fault {i}: count must be at least 1"
                        ));
                    }
                    Some(frame)
                }
                FaultAction::NodeReset { frame } => frame.as_ref(),
                FaultAction::CanBusOff => None,
                other => {
                    return Err(format!(
                        "can-bridge '{id}': fault {i}: '{}' acts on the CPU, not the CAN path; \
                         run it as a lockstep fault experiment",
                        other.kind()
                    ))
                }
            };
            let prefix = match frame.and_then(|m| m.data_prefix.as_deref()) {
                Some(p) => parse_hex_bytes(p)
                    .map_err(|e| format!("can-bridge '{id}': fault {i}: data_prefix {e}"))?,
                None => Vec::new(),
            };
            let remaining = match &f.action {
                FaultAction::CanDrop { count, .. } | FaultAction::CanDelay { count, .. } => *count,
                _ => 1,
            };
            armed.push(ArmedFault {
                fault: f.clone(),
                prefix,
                skip_left: frame.map_or(0, |m| m.skip),
                remaining,
                done: false,
            });
        }
        let recording = CanRecording::new(clock_hz, controller.clone());
        Ok(Self {
            id,
            controller,
            config,
            clock_hz,
            paused: false,
            live_inbox: VecDeque::new(),
            capture: VecDeque::new(),
            release_next_cycle: 0,
            captured_total: 0,
            released_total: 0,
            overflowed_total: 0,
            dropped: VecDeque::new(),
            dropped_total: 0,
            delivered_rx: 0,
            observed_tx: 0,
            replay,
            armed,
            fired: Vec::new(),
            actions: Vec::new(),
            held: Vec::new(),
            tx_ready: Vec::new(),
            recording,
            recording_truncated: 0,
            timeline: Vec::new(),
            timeline_truncated: 0,
            console_seq: 0,
            console_partial: None,
        })
    }

    pub fn config(&self) -> &CanBridgeConfig {
        &self.config
    }

    pub fn pause_mode(&self) -> CanPauseMode {
        self.config.pause_mode
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn clock_hz(&self) -> u64 {
        self.clock_hz
    }

    fn t_us(&self, cycle: u64) -> f64 {
        if self.clock_hz == 0 {
            0.0
        } else {
            cycle as f64 * 1.0e6 / self.clock_hz as f64
        }
    }

    fn us_to_cycles(&self, us: u64) -> u64 {
        ((us as u128 * self.clock_hz as u128) / 1_000_000) as u64
    }

    /// Air time of one frame at the configured bit rate, in cycles (no bit
    /// stuffing; a lower bound, which is what spacing needs).
    fn frame_cycles(&self, f: &CanFrame) -> u64 {
        let bits = if f.extended { 67 } else { 47 } + 8 * f.data.len() as u64;
        ((bits as u128 * self.clock_hz as u128) / self.config.bitrate as u128).max(1) as u64
    }

    /// Add an event to the timeline (bounded; the overflow is counted).
    pub fn push_event(&mut self, mut ev: CanTimelineEvent) {
        ev.t_us = self.t_us(ev.cycle);
        if self.timeline.len() >= TIMELINE_LIMIT {
            self.timeline_truncated += 1;
            return;
        }
        self.timeline.push(ev);
    }

    fn event(&mut self, cycle: u64, lane: CanTimelineLane, kind: &str, detail: String) {
        self.push_event(CanTimelineEvent {
            cycle,
            t_us: 0.0,
            lane,
            kind: kind.to_string(),
            dir: None,
            source: None,
            id: None,
            dlc: None,
            data: None,
            detail,
        });
    }

    fn frame_event(
        &mut self,
        cycle: u64,
        lane: CanTimelineLane,
        kind: &str,
        dir: CanDirection,
        source: CanFrameSource,
        frame: &CanFrame,
        detail: String,
    ) {
        self.push_event(CanTimelineEvent {
            cycle,
            t_us: 0.0,
            lane,
            kind: kind.to_string(),
            dir: Some(dir),
            source: Some(source),
            id: Some(frame.id),
            dlc: Some(can_dlc(frame.data.len())),
            data: Some(hex_bytes(&frame.data)),
            detail,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn record_drop(
        &mut self,
        cycle: u64,
        host_time_ms: Option<f64>,
        dir: CanDirection,
        source: CanFrameSource,
        reason: CanDropReason,
        detail: String,
        frame: &CanFrame,
    ) {
        self.dropped_total += 1;
        if self.dropped.len() >= self.config.record_limit.max(1) {
            self.dropped.pop_front();
        }
        self.dropped.push_back(DroppedCanFrame {
            cycle,
            t_us: self.t_us(cycle),
            host_time_ms,
            dir,
            source,
            reason,
            detail: detail.clone(),
            id: frame.id,
            extended: frame.extended,
            fd: frame.fd,
            dlc: can_dlc(frame.data.len()),
            data: hex_bytes(&frame.data),
        });
        self.frame_event(
            cycle,
            CanTimelineLane::Bridge,
            "dropped",
            dir,
            source,
            frame,
            detail,
        );
    }

    fn record_frame(&mut self, cycle: u64, dir: CanDirection, frame: &CanFrame) {
        if self.recording.entries.len() >= self.config.record_limit {
            self.recording_truncated += 1;
            return;
        }
        self.recording.entries.push(CanRecordEntry {
            cycle,
            dir,
            frame: frame.clone(),
        });
    }

    /// The host says the simulation stopped (`true`) or resumed (`false`)
    /// advancing virtual time. `cycle` is the machine cycle at that moment.
    pub fn set_paused(&mut self, paused: bool, cycle: u64) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        if paused {
            self.event(
                cycle,
                CanTimelineLane::Bridge,
                "paused",
                format!(
                    "simulation paused; live frames are now {}",
                    match self.config.pause_mode {
                        CanPauseMode::Drop => "dropped (and recorded)",
                        CanPauseMode::Capture => "captured into the bounded queue",
                        CanPauseMode::Replay => "ignored (replay runs on virtual time)",
                    }
                ),
            );
        } else {
            self.release_next_cycle = cycle;
            let queued = self.capture.len();
            self.event(
                cycle,
                CanTimelineLane::Bridge,
                "resumed",
                if queued > 0 {
                    format!("simulation resumed; releasing {queued} captured frame(s) from this cycle on")
                } else {
                    "simulation resumed".to_string()
                },
            );
        }
    }

    /// A live frame from outside the simulation. `cycle` is the machine cycle
    /// now; `host_time_ms` is the wall-clock time the host received it, if it
    /// knows it (recorded, never used for timing).
    pub fn offer(
        &mut self,
        frame: CanFrame,
        cycle: u64,
        host_time_ms: Option<f64>,
    ) -> Result<CanOfferOutcome, String> {
        validate_frame(&frame)?;
        let src = CanFrameSource::Live;
        if self.config.pause_mode == CanPauseMode::Replay {
            self.record_drop(
                cycle,
                host_time_ms,
                CanDirection::Rx,
                src,
                CanDropReason::ReplayIgnoresLive,
                "replay mode: live frames are ignored".into(),
                &frame,
            );
            return Ok(CanOfferOutcome::Dropped);
        }
        let pending = Pending {
            frame,
            host_time_ms,
            offered_cycle: cycle,
        };
        if !self.paused {
            // Keep arrival order behind frames still being released.
            if self.capture.is_empty() {
                self.live_inbox.push_back(pending);
                return Ok(CanOfferOutcome::Queued);
            }
        } else if self.config.pause_mode == CanPauseMode::Drop {
            self.record_drop(
                cycle,
                host_time_ms,
                CanDirection::Rx,
                src,
                CanDropReason::PausedDrop,
                "arrived while the simulation was paused (drop mode)".into(),
                &pending.frame,
            );
            return Ok(CanOfferOutcome::Dropped);
        }
        // CAPTURE (paused), or running with captured frames still queued.
        if self.capture.len() >= self.config.capture_capacity {
            self.overflowed_total += 1;
            match self.config.overflow {
                CanOverflowPolicy::DropNewest => {
                    let detail = format!(
                        "capture queue full ({} frames); overflow policy drop_newest discarded this frame",
                        self.config.capture_capacity
                    );
                    self.record_drop(
                        cycle,
                        host_time_ms,
                        CanDirection::Rx,
                        src,
                        CanDropReason::CaptureOverflow,
                        detail,
                        &pending.frame,
                    );
                    return Ok(CanOfferOutcome::Dropped);
                }
                CanOverflowPolicy::DropOldest => {
                    if let Some(old) = self.capture.pop_front() {
                        let detail = format!(
                            "capture queue full ({} frames); overflow policy drop_oldest discarded the frame captured at cycle {}",
                            self.config.capture_capacity, old.offered_cycle
                        );
                        self.record_drop(
                            old.offered_cycle,
                            old.host_time_ms,
                            CanDirection::Rx,
                            CanFrameSource::Captured,
                            CanDropReason::CaptureOverflow,
                            detail,
                            &old.frame,
                        );
                    }
                }
            }
        }
        self.captured_total += 1;
        let depth = self.capture.len() + 1;
        self.frame_event(
            cycle,
            CanTimelineLane::Bridge,
            "captured",
            CanDirection::Rx,
            src,
            &pending.frame,
            format!(
                "queued ({depth}/{}){}",
                self.config.capture_capacity,
                host_time_ms
                    .map(|t| format!(", host time {t:.3} ms"))
                    .unwrap_or_default()
            ),
        );
        self.capture.push_back(pending);
        Ok(CanOfferOutcome::Captured)
    }

    /// Cycle-scheduled faults now due (`node_reset` without a frame trigger,
    /// `can_bus_off`). Also returns actions frame triggers queued.
    pub fn take_actions(&mut self, cycle: u64) -> Vec<CanBridgeAction> {
        for i in 0..self.armed.len() {
            let a = &self.armed[i];
            if a.done || a.fault.at_cycle > cycle {
                continue;
            }
            let action = match &a.fault.action {
                FaultAction::NodeReset { frame: None } => CanBridgeAction::NodeReset(i),
                FaultAction::CanBusOff => CanBridgeAction::BusOff(i),
                _ => continue,
            };
            self.armed[i].done = true;
            self.actions.push(action);
        }
        std::mem::take(&mut self.actions)
    }

    /// Record that a fault fired.
    pub fn note_fault(&mut self, index: usize, cycle: u64, detail: String) {
        let Some(a) = self.armed.get(index) else {
            return;
        };
        let kind = a.fault.action.kind().to_string();
        let at_cycle = a.fault.at_cycle;
        self.fired.push(CanFaultFired {
            index,
            kind: kind.clone(),
            at_cycle,
            applied_cycle: cycle,
            detail: detail.clone(),
        });
        self.event(cycle, CanTimelineLane::Fault, &kind, detail);
    }

    fn matches(a: &ArmedFault, m: &CanFrameMatch, dir: CanDirection, f: &CanFrame) -> bool {
        m.direction.is_none_or(|d| d == dir)
            && m.id.is_none_or(|id| id == f.id)
            && f.data.starts_with(&a.prefix)
    }

    /// Apply the CAN-path faults to a frame crossing the bridge.
    fn pass(
        &mut self,
        frame: CanFrame,
        dir: CanDirection,
        source: CanFrameSource,
        cycle: u64,
    ) -> CanPathVerdict {
        for i in 0..self.armed.len() {
            if self.armed[i].done || self.armed[i].fault.at_cycle > cycle {
                continue;
            }
            let m = match &self.armed[i].fault.action {
                FaultAction::CanDrop { frame, .. } | FaultAction::CanDelay { frame, .. } => {
                    frame.clone()
                }
                FaultAction::NodeReset { frame: Some(frame) } => frame.clone(),
                _ => continue,
            };
            if !Self::matches(&self.armed[i], &m, dir, &frame) {
                continue;
            }
            if self.armed[i].skip_left > 0 {
                self.armed[i].skip_left -= 1;
                continue;
            }
            let action = self.armed[i].fault.action.clone();
            match action {
                FaultAction::CanDrop { .. } => {
                    self.armed[i].remaining -= 1;
                    self.armed[i].done = self.armed[i].remaining == 0;
                    let detail = format!(
                        "dropped {} id={:#x} [{}]",
                        dir.as_str(),
                        frame.id,
                        hex_bytes(&frame.data)
                    );
                    self.note_fault(i, cycle, detail.clone());
                    self.record_drop(
                        cycle,
                        None,
                        dir,
                        source,
                        CanDropReason::Fault,
                        format!("fault {i} (can_drop): {detail}"),
                        &frame,
                    );
                    return CanPathVerdict::Dropped;
                }
                FaultAction::CanDelay { delay_us, .. } => {
                    self.armed[i].remaining -= 1;
                    self.armed[i].done = self.armed[i].remaining == 0;
                    let due = cycle + self.us_to_cycles(delay_us);
                    let detail = format!(
                        "held {} id={:#x} [{}] for {delay_us} us (until cycle {due})",
                        dir.as_str(),
                        frame.id,
                        hex_bytes(&frame.data)
                    );
                    self.note_fault(i, cycle, detail);
                    self.held.push(Held {
                        due_cycle: due,
                        dir,
                        source,
                        frame,
                    });
                    return CanPathVerdict::Held;
                }
                FaultAction::NodeReset { .. } => {
                    // The frame still arrives; the node resets right after it.
                    self.armed[i].done = true;
                    self.actions.push(CanBridgeAction::NodeReset(i));
                }
                _ => {}
            }
        }
        CanPathVerdict::Deliver(frame)
    }

    /// A frame on its way INTO the controller (from a tester, player, the
    /// live inbox, the capture queue or the replay). Applies the faults.
    pub fn pass_rx(
        &mut self,
        frame: CanFrame,
        source: CanFrameSource,
        cycle: u64,
    ) -> CanPathVerdict {
        self.pass(frame, CanDirection::Rx, source, cycle)
    }

    /// A frame the controller transmitted. It is recorded as transmitted
    /// (before faults: the firmware did send it) and compared with the
    /// recording in REPLAY mode, then the faults decide whether it reaches
    /// the tester side.
    pub fn pass_tx(&mut self, frame: CanFrame, cycle: u64) -> CanPathVerdict {
        self.observed_tx += 1;
        self.record_frame(cycle, CanDirection::Tx, &frame);
        self.frame_event(
            cycle,
            CanTimelineLane::Can,
            "frame",
            CanDirection::Tx,
            CanFrameSource::Controller,
            &frame,
            String::new(),
        );
        if let Some(r) = self.replay.as_mut() {
            r.observed_tx.push((cycle, frame.clone()));
        }
        self.pass(frame, CanDirection::Tx, CanFrameSource::Controller, cycle)
    }

    /// Record the controller's answer to an rx frame the bridge (or a tester
    /// through it) delivered.
    pub fn note_rx_result(
        &mut self,
        frame: &CanFrame,
        source: CanFrameSource,
        cycle: u64,
        result: Result<(), String>,
    ) {
        if source == CanFrameSource::Replay {
            if let Some(r) = self.replay.as_mut() {
                match result {
                    Ok(()) => r.injected += 1,
                    Err(_) => r.rejected += 1,
                }
            }
        }
        match result {
            Ok(()) => {
                self.delivered_rx += 1;
                self.record_frame(cycle, CanDirection::Rx, frame);
                self.frame_event(
                    cycle,
                    CanTimelineLane::Can,
                    "frame",
                    CanDirection::Rx,
                    source,
                    frame,
                    String::new(),
                );
            }
            Err(why) => {
                // A tester retries a refused frame every service round; only
                // frames nobody retries are losses worth recording.
                if matches!(source, CanFrameSource::Tester) {
                    return;
                }
                self.record_drop(
                    cycle,
                    None,
                    CanDirection::Rx,
                    source,
                    CanDropReason::ControllerRejected,
                    format!("the controller refused it: {why}"),
                    frame,
                );
            }
        }
    }

    /// Frames due INTO the controller at `cycle`: held (delayed) frames now
    /// due, then live frames, captured frames (spaced after a resume) and
    /// replayed frames. The bool is `true` when the frame already went through
    /// the faults (a held frame) and must not pass them again.
    pub fn due_rx(&mut self, cycle: u64) -> Vec<(CanFrame, CanFrameSource, bool)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.held.len() {
            if self.held[i].dir == CanDirection::Rx && self.held[i].due_cycle <= cycle {
                let h = self.held.remove(i);
                out.push((h.frame, h.source, true));
            } else {
                i += 1;
            }
        }
        if self.paused {
            return out;
        }
        while let Some(p) = self.live_inbox.pop_front() {
            out.push((p.frame, CanFrameSource::Live, false));
        }
        while self.release_next_cycle <= cycle {
            let Some(p) = self.capture.pop_front() else {
                break;
            };
            self.released_total += 1;
            self.release_next_cycle =
                self.release_next_cycle.max(cycle) + self.frame_cycles(&p.frame);
            out.push((p.frame, CanFrameSource::Captured, false));
        }
        if let Some(r) = self.replay.as_mut() {
            while r.next_rx < r.rx.len() && r.rx[r.next_rx].0 <= cycle {
                out.push((r.rx[r.next_rx].1.clone(), CanFrameSource::Replay, false));
                r.next_rx += 1;
            }
        }
        out
    }

    /// Held `tx` frames now due for the tester side.
    pub fn due_tx(&mut self, cycle: u64) -> Vec<CanFrame> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.held.len() {
            if self.held[i].dir == CanDirection::Tx && self.held[i].due_cycle <= cycle {
                out.push(self.held.remove(i).frame);
            } else {
                i += 1;
            }
        }
        out
    }

    /// The earliest cycle the bridge has work at, if it waits for one (a held
    /// frame, a captured frame to release, the next replayed frame).
    pub fn next_due_cycle(&self) -> Option<u64> {
        let held = self.held.iter().map(|h| h.due_cycle).min();
        let cap = (!self.paused && !self.capture.is_empty()).then_some(self.release_next_cycle);
        let rep = self
            .replay
            .as_ref()
            .and_then(|r| r.rx.get(r.next_rx).map(|(c, _)| *c));
        [held, cap, rep].into_iter().flatten().min()
    }

    /// Everything recorded so far, as a [`CanRecording`].
    pub fn recording(&self) -> &CanRecording {
        &self.recording
    }

    fn replay_report(&self) -> Option<CanReplayReport> {
        let r = self.replay.as_ref()?;
        let mut matched = 0usize;
        let mut skew = 0u64;
        let mut first_mismatch = None;
        let n = r.expected_tx.len().max(r.observed_tx.len());
        for i in 0..n {
            match (r.expected_tx.get(i), r.observed_tx.get(i)) {
                (Some((ec, ef)), Some((oc, of))) if ef == of => {
                    matched += 1;
                    skew = skew.max(ec.abs_diff(*oc));
                }
                (Some(_), None) => break,
                (e, o) => {
                    first_mismatch = Some(CanReplayMismatch {
                        index: i,
                        expected: e.map(|(c, f)| CanFrameView::of(*c, f)),
                        observed: o.map(|(c, f)| CanFrameView::of(*c, f)),
                    });
                    break;
                }
            }
        }
        let verdict = if first_mismatch.is_some() {
            CanReplayVerdict::Mismatch
        } else if r.next_rx < r.rx.len() || r.observed_tx.len() < r.expected_tx.len() {
            CanReplayVerdict::Incomplete
        } else {
            CanReplayVerdict::Match
        };
        Some(CanReplayReport {
            verdict,
            rx_total: r.rx.len(),
            rx_injected: r.injected,
            rx_rejected: r.rejected,
            expected_tx: r.expected_tx.len(),
            observed_tx: r.observed_tx.len(),
            matched_tx: matched,
            max_cycle_skew: skew,
            first_mismatch,
        })
    }

    /// The run result block. `extra` events (console lines, tester progress)
    /// are merged into the timeline in cycle order.
    pub fn report(&self, extra: &[CanTimelineEvent]) -> CanBridgeReport {
        let mut timeline = self.timeline.clone();
        for e in extra {
            let mut e = e.clone();
            e.t_us = self.t_us(e.cycle);
            timeline.push(e);
        }
        // Stable: events at one cycle keep their recording order.
        timeline.sort_by_key(|e| e.cycle);
        let faults_not_fired = self
            .armed
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.fired.iter().any(|f| f.index == *i))
            .map(|(i, a)| {
                format!(
                    "fault {i} ({}): {}",
                    a.fault.action.kind(),
                    if a.fault.action.needs_can_bridge()
                        && !matches!(a.fault.action, FaultAction::CanBusOff)
                    {
                        "no matching frame crossed the bridge at or after its at_cycle"
                    } else {
                        "the run ended before its at_cycle"
                    }
                )
            })
            .collect();
        CanBridgeReport {
            id: self.id.clone(),
            controller: self.controller.clone(),
            pause_mode: self.config.pause_mode,
            paused: self.paused,
            clock_hz: self.clock_hz,
            delivered_rx: self.delivered_rx,
            observed_tx: self.observed_tx,
            dropped_total: self.dropped_total,
            dropped: self.dropped.iter().cloned().collect(),
            capture: CanCaptureReport {
                capacity: self.config.capture_capacity,
                overflow: self.config.overflow,
                queued_now: self.capture.len(),
                captured_total: self.captured_total,
                released_total: self.released_total,
                overflowed_total: self.overflowed_total,
            },
            replay: self.replay_report(),
            faults: self.fired.clone(),
            faults_not_fired,
            recorded_frames: self.recording.entries.len(),
            recording_truncated: self.recording_truncated,
            timeline,
            timeline_truncated: self.timeline_truncated,
            pause_note: PAUSE_NOTE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ: u64 = 1_000_000; // 1 cycle = 1 us

    fn frame(id: u32, b: u8) -> CanFrame {
        CanFrame::classic(id, vec![b])
    }

    fn bridge(mode: CanPauseMode, cap: usize, overflow: CanOverflowPolicy) -> CanBridge {
        CanBridge::new(
            "b",
            "fdcan1",
            HZ,
            CanBridgeConfig {
                pause_mode: mode,
                capture_capacity: cap,
                overflow,
                ..Default::default()
            },
            None,
        )
        .unwrap()
    }

    #[test]
    fn running_frames_are_delivered_in_every_mode_but_replay() {
        for mode in [CanPauseMode::Drop, CanPauseMode::Capture] {
            let mut b = bridge(mode, 4, CanOverflowPolicy::DropNewest);
            assert_eq!(
                b.offer(frame(1, 1), 10, None).unwrap(),
                CanOfferOutcome::Queued
            );
            let due = b.due_rx(10);
            assert_eq!(due.len(), 1);
            assert_eq!(due[0].1, CanFrameSource::Live);
            assert_eq!(b.report(&[]).dropped_total, 0);
        }
    }

    #[test]
    fn drop_mode_records_exactly_what_it_dropped() {
        let mut b = bridge(CanPauseMode::Drop, 4, CanOverflowPolicy::DropNewest);
        b.set_paused(true, 100);
        let f = CanFrame::classic(0x7E0, vec![0x02, 0x3E, 0x00]);
        assert_eq!(
            b.offer(f, 100, Some(1234.5)).unwrap(),
            CanOfferOutcome::Dropped
        );
        b.set_paused(false, 100);
        assert!(
            b.due_rx(200).is_empty(),
            "a dropped frame must never arrive"
        );
        let r = b.report(&[]);
        assert_eq!(r.dropped_total, 1);
        let d = &r.dropped[0];
        assert_eq!(
            (d.cycle, d.id, d.dlc, d.data.as_str()),
            (100, 0x7E0, 3, "023e00")
        );
        assert_eq!(d.host_time_ms, Some(1234.5));
        assert_eq!(d.reason, CanDropReason::PausedDrop);
        assert!(r.timeline.iter().any(|e| e.kind == "dropped"));
        assert!(r.timeline.iter().any(|e| e.kind == "paused"));
    }

    #[test]
    fn capture_mode_queues_bounded_and_releases_spaced_after_resume() {
        let mut b = bridge(CanPauseMode::Capture, 2, CanOverflowPolicy::DropNewest);
        b.set_paused(true, 50);
        for i in 0..3 {
            b.offer(frame(0x100 + i, i as u8), 50, Some(i as f64))
                .unwrap();
        }
        assert!(
            b.due_rx(1_000_000).is_empty(),
            "nothing is released while paused"
        );
        let r = b.report(&[]);
        assert_eq!(r.capture.queued_now, 2);
        assert_eq!(r.capture.overflowed_total, 1);
        assert_eq!(r.dropped[0].reason, CanDropReason::CaptureOverflow);
        assert_eq!(
            r.dropped[0].id, 0x102,
            "drop_newest discards the frame that did not fit"
        );
        b.set_paused(false, 1000);
        let first = b.due_rx(1000);
        assert_eq!(first.len(), 1, "one frame per frame time, not a burst");
        assert_eq!(first[0].0.id, 0x100);
        // 47 + 8 bits at 500 kbit/s = 110 us = 110 cycles at 1 MHz.
        assert!(b.due_rx(1109).is_empty());
        let second = b.due_rx(1110);
        assert_eq!(second[0].0.id, 0x101);
        assert_eq!(second[0].1, CanFrameSource::Captured);
        assert_eq!(b.report(&[]).capture.released_total, 2);
    }

    #[test]
    fn capture_drop_oldest_discards_the_oldest_and_says_so() {
        let mut b = bridge(CanPauseMode::Capture, 2, CanOverflowPolicy::DropOldest);
        b.set_paused(true, 0);
        for i in 0..3 {
            b.offer(frame(0x100 + i, 0), 7, None).unwrap();
        }
        let r = b.report(&[]);
        assert_eq!(r.dropped[0].id, 0x100);
        assert!(r.dropped[0].detail.contains("drop_oldest"));
        b.set_paused(false, 10);
        let ids: Vec<u32> = (0..10_000)
            .flat_map(|c| b.due_rx(10 + c))
            .map(|f| f.0.id)
            .collect();
        assert_eq!(ids, vec![0x101, 0x102]);
    }

    #[test]
    fn replay_mode_ignores_live_and_injects_on_virtual_time() {
        let mut rec = CanRecording::new(HZ, "fdcan1");
        rec.entries.push(CanRecordEntry {
            cycle: 500,
            dir: CanDirection::Rx,
            frame: frame(0x7E0, 1),
        });
        rec.entries.push(CanRecordEntry {
            cycle: 700,
            dir: CanDirection::Tx,
            frame: frame(0x7E8, 2),
        });
        let mut b = CanBridge::new(
            "b",
            "fdcan1",
            HZ,
            CanBridgeConfig {
                pause_mode: CanPauseMode::Replay,
                ..Default::default()
            },
            Some(rec),
        )
        .unwrap();
        assert_eq!(
            b.offer(frame(1, 1), 0, None).unwrap(),
            CanOfferOutcome::Dropped
        );
        assert!(b.due_rx(499).is_empty());
        // A pause does not move virtual time, so it does not change the replay.
        b.set_paused(true, 499);
        b.set_paused(false, 499);
        let due = b.due_rx(500);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, CanFrameSource::Replay);
        b.note_rx_result(&due[0].0, CanFrameSource::Replay, 500, Ok(()));
        assert_eq!(
            b.report(&[]).replay.unwrap().verdict,
            CanReplayVerdict::Incomplete
        );
        b.pass_tx(frame(0x7E8, 2), 700);
        let r = b.report(&[]).replay.unwrap();
        assert_eq!(r.verdict, CanReplayVerdict::Match);
        assert_eq!(r.max_cycle_skew, 0);
        b.pass_tx(frame(0x7E8, 3), 800);
        let r = b.report(&[]).replay.unwrap();
        assert_eq!(r.verdict, CanReplayVerdict::Mismatch);
        assert_eq!(r.first_mismatch.unwrap().index, 1);
    }

    #[test]
    fn a_recording_is_refused_outside_replay_and_required_in_it() {
        let cfg = CanBridgeConfig {
            pause_mode: CanPauseMode::Replay,
            ..Default::default()
        };
        assert!(CanBridge::new("b", "c", HZ, cfg, None).is_err());
        let rec = CanRecording::new(HZ, "c");
        assert!(CanBridge::new("b", "c", HZ, CanBridgeConfig::default(), Some(rec)).is_err());
    }

    fn with_faults(json: &str) -> CanBridge {
        let faults: Vec<ScheduledFault> = serde_json::from_str(json).unwrap();
        CanBridge::new(
            "b",
            "fdcan1",
            HZ,
            CanBridgeConfig {
                faults,
                ..Default::default()
            },
            None,
        )
        .unwrap()
    }

    #[test]
    fn can_drop_removes_the_matching_frame_after_skip() {
        let mut b = with_faults(
            r#"[{"at_cycle":10,"kind":"can_drop","frame":{"direction":"tx","id":"0x7E8","data_prefix":"21","skip":1}}]"#,
        );
        let cf = CanFrame::classic(0x7E8, vec![0x21, 0xAA]);
        assert!(
            matches!(b.pass_tx(cf.clone(), 5), CanPathVerdict::Deliver(_)),
            "before at_cycle"
        );
        assert!(matches!(
            b.pass_tx(CanFrame::classic(0x7E8, vec![0x22]), 20),
            CanPathVerdict::Deliver(_)
        ));
        assert!(
            matches!(b.pass_tx(cf.clone(), 20), CanPathVerdict::Deliver(_)),
            "skipped once"
        );
        assert_eq!(b.pass_tx(cf.clone(), 30), CanPathVerdict::Dropped);
        assert!(
            matches!(b.pass_tx(cf, 40), CanPathVerdict::Deliver(_)),
            "count 1 is spent"
        );
        let r = b.report(&[]);
        assert_eq!(r.faults.len(), 1);
        assert_eq!(r.faults[0].applied_cycle, 30);
        assert_eq!(r.dropped[0].reason, CanDropReason::Fault);
        assert_eq!(r.observed_tx, 5, "a dropped tx frame was still transmitted");
    }

    #[test]
    fn can_delay_holds_then_releases() {
        let mut b = with_faults(
            r#"[{"at_cycle":0,"kind":"can_delay","frame":{"direction":"rx"},"delay_us":100}]"#,
        );
        assert_eq!(
            b.pass_rx(frame(1, 1), CanFrameSource::Tester, 50),
            CanPathVerdict::Held
        );
        assert!(b.due_rx(149).is_empty());
        let due = b.due_rx(150);
        assert_eq!(due.len(), 1);
        assert!(due[0].2, "a held frame must not pass the faults twice");
    }

    #[test]
    fn node_reset_and_bus_off_become_actions() {
        let mut b = with_faults(
            r#"[{"at_cycle":100,"kind":"can_bus_off"},
                {"at_cycle":0,"kind":"node_reset","frame":{"direction":"tx","data_prefix":"21"}}]"#,
        );
        assert!(b.take_actions(99).is_empty());
        assert_eq!(b.take_actions(100), vec![CanBridgeAction::BusOff(0)]);
        assert!(b.take_actions(200).is_empty(), "fires once");
        let _ = b.pass_tx(CanFrame::classic(0x7E8, vec![0x21]), 300);
        assert_eq!(b.take_actions(300), vec![CanBridgeAction::NodeReset(1)]);
    }

    #[test]
    fn cpu_faults_are_refused_on_a_bridge() {
        let faults: Vec<ScheduledFault> =
            serde_json::from_str(r#"[{"at_cycle":1,"kind":"instruction_skip"}]"#).unwrap();
        let err = CanBridge::new(
            "b",
            "c",
            HZ,
            CanBridgeConfig {
                faults,
                ..Default::default()
            },
            None,
        )
        .unwrap_err();
        assert!(err.contains("lockstep"), "{err}");
    }
}
