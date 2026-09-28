// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! `ScriptedCentral` — a **LabWired-defined** BLE central ("the phone") that
//! lives on a [`BleAirBus`] and drives a simulated peripheral through a real
//! connection: `CONNECT_IND`, connection events, GATT discovery, read, write,
//! subscribe.
//!
//! # What it is, and what it is not
//!
//! It is a test instrument, like the CAN UDS tester: not a model of any
//! silicon, and not pretending to be one. It exists so a user can test the
//! GATT server in their firmware without a second MCU — the way they would
//! with nRF Connect on a phone.
//!
//! Everything it puts on the air is a real BLE PDU built to the Core
//! specification (Vol 6 Part B link layer, Vol 3 Part A L2CAP, Vol 3 Part F
//! ATT, Vol 3 Part G GATT procedures), and everything it reads back is parsed
//! the same way, so the peripheral's firmware is exercised exactly as a phone
//! would exercise it.
//!
//! # Scope
//!
//! Link layer master: connect to the first connectable `ADV_IND` that matches
//! the target (name, service UUID or address), CSA #1, SN/NESN acknowledgement
//! with retransmission, MD, supervision timeout, and the control procedures a
//! peripheral starts (feature exchange, version exchange, data length,
//! connection parameter request — rejected — ping, terminate). L2CAP basic
//! mode with fragmentation/reassembly on the ATT and signalling channels.
//! ATT client: primary service, characteristic and descriptor discovery; read;
//! write request; subscribe via the CCCD; notifications and indications.
//!
//! Not modelled: encryption/pairing, MTU above 23, extended advertising, the
//! 2M/Coded PHYs, connection parameter updates.
//!
//! # Timing
//!
//! The central runs on world time: [`ScriptedCentral::advance_to`] is called
//! with the world's clock after every stepping quantum, and every frame it
//! sends is stamped at the exact air time the spec gives it (T_IFS after the
//! packet it answers, the anchor for a connection event). See
//! `peripherals/esp32c3/bt_link.rs` for the other side of that contract.

use crate::peripherals::ble_air::{airtime_ns, next_node_id, BleAirBus, BleAirFrame, T_IFS_NS};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const ADV_AA: u32 = 0x8E89_BED6;
const ADV_CRC: u32 = 0x0055_5555;
/// How late (after an `ADV_IND` ended) the central may still notice it and
/// answer at T_IFS. The world steps in quanta far below this; a larger gap
/// means the frame belongs to an advertising event already over.
const MAX_REACTION_NS: u64 = 120_000;
/// How long after the expected reply start the central waits before deciding
/// no reply came (the same role as the RW-BLE model's decision lag).
const RX_DECISION_LAG_NS: u64 = 60_000;
/// Packets per connection event the central will exchange before closing it.
const MAX_PACKETS_PER_EVENT: u32 = 12;
/// LL payload limit without data length extension.
const LL_MAX_PAYLOAD: usize = 27;
/// Default ATT MTU (no MTU exchange).
const ATT_MTU: usize = 23;
/// Transcript lines kept (oldest dropped first). A peripheral that notifies
/// every few ms would otherwise grow the log without bound in a long run.
const LOG_KEEP: usize = 4_000;
/// Notifications kept (oldest dropped first); `notification_count` has them all.
const NOTIFY_KEEP: usize = 256;

// L2CAP channels.
const CID_ATT: u16 = 0x0004;
const CID_SIGNALING: u16 = 0x0005;

/// One step of the central's script.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CentralStep {
    /// Scan for the target and connect. Completes when the first connection
    /// event has been acknowledged by the peripheral.
    Connect,
    /// Discover every primary service, characteristic and CCCD.
    Discover,
    /// Read the characteristic with this UUID.
    Read(String),
    /// Write request to the characteristic.
    Write(WriteStep),
    /// Enable notifications (or indications, if that is all it supports).
    Subscribe(String),
    /// Wait until at least `count` notifications/indications have arrived
    /// since the script started, or `timeout_ms` passed.
    WaitNotify(WaitNotifyStep),
    /// Do nothing for this long.
    WaitMs(u64),
    /// Send `LL_TERMINATE_IND`.
    Disconnect,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WriteStep {
    pub uuid: String,
    /// Value as hex (`"68656c6c6f"`)…
    #[serde(default)]
    pub hex: Option<String>,
    /// …or as text.
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WaitNotifyStep {
    pub count: usize,
    #[serde(default = "default_wait_timeout")]
    pub timeout_ms: u64,
}

fn default_wait_timeout() -> u64 {
    5_000
}

/// What the central connects to and what it then does.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CentralConfig {
    /// Match the advertised complete or shortened local name.
    #[serde(default)]
    pub target_name: Option<String>,
    /// Match a service UUID in the advertising data.
    #[serde(default)]
    pub target_service: Option<String>,
    /// Match the advertiser address, `aa:bb:cc:dd:ee:ff` (MSB first).
    #[serde(default)]
    pub target_address: Option<String>,
    /// Connection interval in 1.25 ms units (default 24 = 30 ms).
    #[serde(default)]
    pub interval: Option<u16>,
    /// Supervision timeout in 10 ms units (default 500 = 5 s).
    #[serde(default)]
    pub supervision_timeout: Option<u16>,
    #[serde(default)]
    pub script: Vec<CentralStep>,
}

/// Where the central is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CentralState {
    Idle,
    Scanning,
    Connected,
    Disconnected,
}

/// One line of the transcript.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CentralLogEntry {
    /// World time, µs.
    pub t_us: u64,
    /// `state`, `ll`, `att_tx`, `att_rx`, `gatt`, `script`, `error`.
    pub kind: String,
    pub text: String,
}

/// A discovered characteristic.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GattCharacteristic {
    pub service_uuid: String,
    pub uuid: String,
    pub properties: u8,
    pub decl_handle: u16,
    pub value_handle: u16,
    pub cccd_handle: Option<u16>,
}

/// A value the central read, or a notification it received.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GattValue {
    pub t_us: u64,
    pub uuid: String,
    pub handle: u16,
    pub value: Vec<u8>,
}

/// Everything a test, the CLI or the browser wants to know.
#[derive(Debug, Clone, Serialize)]
pub struct CentralReport {
    pub state: CentralState,
    pub peer_address: Option<String>,
    pub script_step: usize,
    pub script_done: bool,
    pub services: Vec<String>,
    pub characteristics: Vec<GattCharacteristic>,
    pub reads: Vec<GattValue>,
    pub writes_acked: Vec<GattValue>,
    /// The most recent notifications/indications (at most 256).
    pub notifications: Vec<GattValue>,
    /// Every notification/indication received, including dropped ones.
    pub notification_count: u64,
    pub connection_events: u64,
    /// The transcript (the most recent 4000 lines).
    pub log: Vec<CentralLogEntry>,
    /// Transcript lines dropped from the front of `log`.
    pub log_dropped: u64,
}

#[derive(Debug, Clone)]
struct Conn {
    aa: u32,
    crc_init: u32,
    interval_ns: u64,
    supervision_ns: u64,
    hop: u8,
    last_unmapped: u8,
    event_counter: u16,
    next_anchor_ns: u64,
    tx_sn: bool,
    rx_nesn: bool,
    /// The packet in flight (LLID, payload) — retransmitted until acked.
    inflight: Option<(u8, Vec<u8>)>,
    txq: VecDeque<(u8, Vec<u8>)>,
    rx_l2cap: Vec<u8>,
    last_rx_ns: u64,
    /// Current event, if one is open: data channel, expected reply start,
    /// packets exchanged, and whether it has seen a reply at all.
    ev: Option<ConnEvent>,
    established: bool,
    terminating: bool,
    peer: [u8; 6],
}

#[derive(Debug, Clone, Copy)]
struct ConnEvent {
    channel: u8,
    expect_ns: u64,
    packets: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum AttPending {
    Services { start: u16 },
    Chars { svc: usize, start: u16 },
    Descs { chr: usize, start: u16, end: u16 },
    Read { chr: usize },
    Write { chr: usize, value: Vec<u8> },
    Cccd { chr: usize },
}

#[derive(Debug, Clone)]
struct Service {
    uuid: Vec<u8>,
    start: u16,
    end: u16,
}

/// The scripted central. See the module docs.
#[derive(Debug)]
pub struct ScriptedCentral {
    air: BleAirBus,
    node_id: u64,
    cfg: CentralConfig,
    now_ns: u64,
    state: CentralState,
    scan_from_ns: u64,
    own_addr: [u8; 6],
    conn: Option<Conn>,
    step: usize,
    step_started_ns: u64,
    step_active: bool,
    att: Option<AttPending>,
    discovery_done: bool,
    services: Vec<Service>,
    chars: Vec<(usize, GattCharacteristic, Vec<u8>)>,
    reads: Vec<GattValue>,
    writes_acked: Vec<GattValue>,
    notifications: VecDeque<GattValue>,
    notification_count: u64,
    connection_events: u64,
    log: VecDeque<CentralLogEntry>,
    log_dropped: u64,
}

impl ScriptedCentral {
    pub fn new(air: BleAirBus, cfg: CentralConfig) -> Self {
        Self {
            air,
            node_id: next_node_id(),
            cfg,
            now_ns: 0,
            state: CentralState::Idle,
            scan_from_ns: 0,
            // A random static address (top two bits 11), LSB first as it
            // goes on the air: C0:4C:57:00:00:01 ("LW").
            own_addr: [0x01, 0x00, 0x00, 0x57, 0x4C, 0xC0],
            conn: None,
            step: 0,
            step_started_ns: 0,
            step_active: false,
            att: None,
            discovery_done: false,
            services: Vec::new(),
            chars: Vec::new(),
            reads: Vec::new(),
            writes_acked: Vec::new(),
            notifications: VecDeque::new(),
            notification_count: 0,
            connection_events: 0,
            log: VecDeque::new(),
            log_dropped: 0,
        }
    }

    pub fn state(&self) -> CentralState {
        self.state
    }

    pub fn log(&self) -> Vec<CentralLogEntry> {
        self.log.iter().cloned().collect()
    }

    pub fn reads(&self) -> &[GattValue] {
        &self.reads
    }

    pub fn notifications(&self) -> Vec<GattValue> {
        self.notifications.iter().cloned().collect()
    }

    /// Every notification/indication received so far.
    pub fn notification_count(&self) -> u64 {
        self.notification_count
    }

    pub fn script_done(&self) -> bool {
        self.step >= self.cfg.script.len()
    }

    pub fn report(&self) -> CentralReport {
        CentralReport {
            state: self.state,
            peer_address: self.conn.as_ref().map(|c| fmt_addr(&c.peer)),
            script_step: self.step,
            script_done: self.script_done(),
            services: self
                .services
                .iter()
                .map(|s| uuid_to_string(&s.uuid))
                .collect(),
            characteristics: self.chars.iter().map(|(_, c, _)| c.clone()).collect(),
            reads: self.reads.clone(),
            writes_acked: self.writes_acked.clone(),
            notifications: self.notifications(),
            notification_count: self.notification_count,
            connection_events: self.connection_events,
            log: self.log(),
            log_dropped: self.log_dropped,
        }
    }

    fn note(&mut self, kind: &str, text: impl Into<String>) {
        if self.log.len() >= LOG_KEEP {
            self.log.pop_front();
            self.log_dropped += 1;
        }
        self.log.push_back(CentralLogEntry {
            t_us: self.now_ns / 1_000,
            kind: kind.to_string(),
            text: text.into(),
        });
    }

    fn set_state(&mut self, s: CentralState) {
        if self.state != s {
            self.state = s;
            self.note("state", format!("{s:?}").to_lowercase());
        }
    }

    // ── Clock ───────────────────────────────────────────────────────────────

    /// Advance to world time `now_ns`, doing everything that falls due.
    pub fn advance_to(&mut self, now_ns: u64) {
        if now_ns < self.now_ns {
            return;
        }
        self.now_ns = now_ns;
        // Bounded: each pass either makes progress or returns.
        for _ in 0..64 {
            let progressed = match self.state {
                CentralState::Idle => {
                    self.run_script();
                    self.state != CentralState::Idle
                }
                CentralState::Scanning => self.scan(),
                CentralState::Connected => {
                    let p = self.connection();
                    self.run_script();
                    p
                }
                CentralState::Disconnected => {
                    self.run_script();
                    false
                }
            };
            if !progressed {
                break;
            }
        }
    }

    // ── Script ──────────────────────────────────────────────────────────────

    fn run_script(&mut self) {
        loop {
            let Some(step) = self.cfg.script.get(self.step).cloned() else {
                return;
            };
            if !self.step_active {
                self.step_active = true;
                self.step_started_ns = self.now_ns;
                self.note(
                    "script",
                    format!("step {}: {}", self.step, step_label(&step)),
                );
                if !self.begin_step(&step) {
                    // Could not start (e.g. unknown UUID): logged, skipped.
                    self.finish_step();
                    continue;
                }
            }
            if self.step_complete(&step) {
                self.finish_step();
                continue;
            }
            return;
        }
    }

    fn finish_step(&mut self) {
        self.step += 1;
        self.step_active = false;
        if self.script_done() {
            self.note("script", "done");
        }
    }

    /// Start `step`. Returns `false` if it cannot run.
    fn begin_step(&mut self, step: &CentralStep) -> bool {
        match step {
            CentralStep::Connect => {
                if self.state != CentralState::Connected {
                    self.scan_from_ns = self.now_ns;
                    self.set_state(CentralState::Scanning);
                }
                true
            }
            CentralStep::Discover => {
                if !self.connected() {
                    self.note("error", "discover: not connected");
                    return false;
                }
                self.services.clear();
                self.chars.clear();
                self.discovery_done = false;
                self.att_request(
                    AttPending::Services { start: 1 },
                    read_by_group_type(1, 0xFFFF, 0x2800),
                );
                true
            }
            CentralStep::Read(uuid) => match self.find_char(uuid) {
                Some(i) if self.connected() => {
                    let h = self.chars[i].1.value_handle;
                    self.att_request(
                        AttPending::Read { chr: i },
                        vec![0x0A, h as u8, (h >> 8) as u8],
                    );
                    true
                }
                _ => {
                    self.note(
                        "error",
                        format!("read: characteristic {uuid} not discovered"),
                    );
                    false
                }
            },
            CentralStep::Write(w) => {
                let value = match (&w.hex, &w.text) {
                    (Some(h), _) => match parse_hex(h) {
                        Some(v) => v,
                        None => {
                            self.note("error", format!("write: bad hex {h:?}"));
                            return false;
                        }
                    },
                    (None, Some(t)) => t.as_bytes().to_vec(),
                    (None, None) => Vec::new(),
                };
                match self.find_char(&w.uuid) {
                    Some(i) if self.connected() => {
                        let h = self.chars[i].1.value_handle;
                        let mut pdu = vec![0x12, h as u8, (h >> 8) as u8];
                        pdu.extend_from_slice(&value[..value.len().min(ATT_MTU - 3)]);
                        self.att_request(AttPending::Write { chr: i, value }, pdu);
                        true
                    }
                    _ => {
                        self.note(
                            "error",
                            format!("write: characteristic {} not discovered", w.uuid),
                        );
                        false
                    }
                }
            }
            CentralStep::Subscribe(uuid) => match self.find_char(uuid) {
                Some(i) if self.connected() => {
                    let c = &self.chars[i].1;
                    let Some(cccd) = c.cccd_handle else {
                        self.note("error", format!("subscribe: {uuid} has no CCCD"));
                        return false;
                    };
                    // Notify if supported, else indicate.
                    let v: u16 = if c.properties & 0x10 != 0 { 1 } else { 2 };
                    self.att_request(
                        AttPending::Cccd { chr: i },
                        vec![0x12, cccd as u8, (cccd >> 8) as u8, v as u8, 0],
                    );
                    true
                }
                _ => {
                    self.note(
                        "error",
                        format!("subscribe: characteristic {uuid} not discovered"),
                    );
                    false
                }
            },
            CentralStep::WaitNotify(_) | CentralStep::WaitMs(_) => true,
            CentralStep::Disconnect => {
                if let Some(c) = self.conn.as_mut() {
                    c.txq.push_back((0x3, vec![0x02, 0x13]));
                    c.terminating = true;
                    self.note("ll", "TX LL_TERMINATE_IND (0x13 remote user terminated)");
                    true
                } else {
                    false
                }
            }
        }
    }

    fn step_complete(&self, step: &CentralStep) -> bool {
        let elapsed = self.now_ns.saturating_sub(self.step_started_ns);
        match step {
            CentralStep::Connect => self.connected(),
            CentralStep::Discover => self.discovery_done || !self.connected(),
            CentralStep::Read(_) | CentralStep::Write(_) | CentralStep::Subscribe(_) => {
                self.att.is_none() || !self.connected()
            }
            CentralStep::WaitNotify(w) => {
                self.notification_count >= w.count as u64 || elapsed >= w.timeout_ms * 1_000_000
            }
            CentralStep::WaitMs(ms) => elapsed >= ms * 1_000_000,
            CentralStep::Disconnect => self.state == CentralState::Disconnected,
        }
    }

    fn connected(&self) -> bool {
        self.state == CentralState::Connected && self.conn.as_ref().is_some_and(|c| c.established)
    }

    fn find_char(&self, uuid: &str) -> Option<usize> {
        let want = parse_uuid(uuid)?;
        self.chars.iter().position(|(_, _, u)| uuid_eq(u, &want))
    }

    // ── Scanning / connecting ───────────────────────────────────────────────

    fn scan(&mut self) -> bool {
        let frames =
            self.air
                .frames_between(&[37, 38, 39], self.scan_from_ns, self.now_ns, self.node_id);
        for f in frames {
            let Some(end) = f.end_ns() else { continue };
            if end > self.now_ns {
                // Still on the air; look again next time.
                return false;
            }
            self.scan_from_ns = f.air_ns.unwrap_or(0) + 1;
            if f.access_address != ADV_AA || f.pdu.len() < 8 {
                continue;
            }
            let pdu_type = f.pdu[0] & 0x0F;
            if pdu_type != 0x0 {
                continue; // connectable undirected only
            }
            let adva: [u8; 6] = f.pdu[2..8].try_into().unwrap();
            let adv_data = &f.pdu[8..];
            if !self.matches_target(&adva, adv_data) {
                continue;
            }
            if self.now_ns > end + MAX_REACTION_NS {
                continue; // noticed too late to answer this one
            }
            self.connect(&f, adva, end);
            return true;
        }
        false
    }

    fn matches_target(&self, adva: &[u8; 6], data: &[u8]) -> bool {
        if let Some(a) = &self.cfg.target_address {
            if fmt_addr(adva).to_lowercase() != a.to_lowercase() {
                return false;
            }
        }
        let ads = parse_ad(data);
        if let Some(name) = &self.cfg.target_name {
            let found = ads
                .iter()
                .any(|(t, v)| (*t == 0x09 || *t == 0x08) && v.as_slice() == name.as_bytes());
            if !found {
                return false;
            }
        }
        if let Some(svc) = &self.cfg.target_service {
            let Some(want) = parse_uuid(svc) else {
                return false;
            };
            let found = ads.iter().any(|(t, v)| match *t {
                0x02 | 0x03 => v.chunks(2).any(|c| uuid_eq(c, &want)),
                0x06 | 0x07 => v.chunks(16).any(|c| uuid_eq(c, &want)),
                _ => false,
            });
            if !found {
                return false;
            }
        }
        true
    }

    fn connect(&mut self, adv: &BleAirFrame, adva: [u8; 6], adv_end: u64) {
        let interval = self.cfg.interval.unwrap_or(24).clamp(6, 3200);
        let timeout = self.cfg.supervision_timeout.unwrap_or(500).clamp(10, 3200);
        let aa: u32 = 0x5065_4C57; // a valid, fixed access address: deterministic
        let crc_init: u32 = 0x0012_3456;
        let win_size: u8 = 2;
        let win_offset: u16 = 0;
        let hop: u8 = 7;
        let sca: u8 = 5;
        let mut lldata = Vec::with_capacity(22);
        lldata.extend_from_slice(&aa.to_le_bytes());
        lldata.extend_from_slice(&crc_init.to_le_bytes()[..3]);
        lldata.push(win_size);
        lldata.extend_from_slice(&win_offset.to_le_bytes());
        lldata.extend_from_slice(&interval.to_le_bytes());
        lldata.extend_from_slice(&0u16.to_le_bytes()); // latency
        lldata.extend_from_slice(&timeout.to_le_bytes());
        lldata.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F]); // all 37 channels
        lldata.push(hop | (sca << 5));
        // Header: CONNECT_IND, ChSel 0 (CSA #1), TxAdd 1 (random),
        // RxAdd = the advertiser's TxAdd.
        let header = 0x05 | 0x40 | ((adv.pdu[0] & 0x40) << 1);
        let mut pdu = vec![header, 34];
        pdu.extend_from_slice(&self.own_addr);
        pdu.extend_from_slice(&adva);
        pdu.extend_from_slice(&lldata);
        let ci_at = adv_end + T_IFS_NS;
        let ci_end = ci_at + airtime_ns(pdu.len());
        self.tx(adv.channel, ADV_AA, ADV_CRC, pdu, ci_at);
        // First anchor inside the transmit window: 1.25 ms + WinOffset after
        // the CONNECT_IND ends, plus a quarter millisecond into the window.
        let anchor = ci_end + 1_250_000 + u64::from(win_offset) * 1_250_000 + 250_000;
        self.conn = Some(Conn {
            aa,
            crc_init,
            interval_ns: u64::from(interval) * 1_250_000,
            supervision_ns: u64::from(timeout) * 10_000_000,
            hop,
            last_unmapped: 0,
            event_counter: 0,
            next_anchor_ns: anchor,
            tx_sn: false,
            rx_nesn: false,
            inflight: None,
            txq: VecDeque::new(),
            rx_l2cap: Vec::new(),
            last_rx_ns: ci_end,
            ev: None,
            established: false,
            terminating: false,
            peer: adva,
        });
        self.note(
            "ll",
            format!(
                "TX CONNECT_IND to {} on ch{} (interval {} x 1.25 ms, AA {aa:#010x})",
                fmt_addr(&adva),
                adv.channel,
                interval
            ),
        );
        self.set_state(CentralState::Connected);
    }

    fn tx(&self, channel: u8, aa: u32, crc: u32, pdu: Vec<u8>, at_ns: u64) {
        self.air.transmit(BleAirFrame {
            seq: 0,
            source: self.node_id,
            channel,
            access_address: aa,
            crc_init: crc,
            pdu,
            air_ns: Some(at_ns),
        });
    }

    // ── Connection events ───────────────────────────────────────────────────

    fn connection(&mut self) -> bool {
        let Some(mut c) = self.conn.take() else {
            return false;
        };
        let now = self.now_ns;
        let mut progressed = false;
        if now.saturating_sub(c.last_rx_ns) > c.supervision_ns {
            self.conn = Some(c);
            self.note("ll", "supervision timeout");
            self.set_state(CentralState::Disconnected);
            return false;
        }
        match c.ev {
            None => {
                if now >= c.next_anchor_ns {
                    // Open the event: CSA #1.
                    let unmapped = (c.last_unmapped + c.hop) % 37;
                    c.last_unmapped = unmapped;
                    let ch = unmapped; // all channels used
                    let at = c.next_anchor_ns;
                    let end = self.conn_tx(&mut c, ch, at);
                    c.ev = Some(ConnEvent {
                        channel: ch,
                        expect_ns: end + T_IFS_NS,
                        packets: 1,
                    });
                    self.connection_events += 1;
                    progressed = true;
                }
            }
            Some(ev) => {
                let f = self.air.receive_window(
                    ev.channel,
                    c.aa,
                    ev.expect_ns - 2_000,
                    ev.expect_ns + 2_000,
                    self.node_id,
                );
                match f {
                    Some(f) if f.end_ns().is_some_and(|e| e <= now) => {
                        let end = f.end_ns().unwrap();
                        let peer_md = self.conn_rx(&mut c, &f);
                        c.last_rx_ns = end;
                        let more = peer_md || c.inflight.is_some() || !c.txq.is_empty();
                        let at = end + T_IFS_NS;
                        let next_event_start = c.next_anchor_ns + c.interval_ns;
                        if more
                            && ev.packets < MAX_PACKETS_PER_EVENT
                            && at + 1_000_000 < next_event_start
                            && self.state == CentralState::Connected
                        {
                            let tx_end = self.conn_tx(&mut c, ev.channel, at);
                            c.ev = Some(ConnEvent {
                                channel: ev.channel,
                                expect_ns: tx_end + T_IFS_NS,
                                packets: ev.packets + 1,
                            });
                        } else {
                            Self::close_event(&mut c);
                        }
                        progressed = true;
                    }
                    Some(_) => {}
                    None => {
                        if now >= ev.expect_ns + 2_000 + RX_DECISION_LAG_NS {
                            // No reply: the event is over. The packet in
                            // flight is retransmitted next event.
                            Self::close_event(&mut c);
                            progressed = true;
                        }
                    }
                }
            }
        }
        self.conn = Some(c);
        progressed
    }

    fn close_event(c: &mut Conn) {
        c.ev = None;
        c.event_counter = c.event_counter.wrapping_add(1);
        c.next_anchor_ns += c.interval_ns;
    }

    /// Send the packet in flight (or the next queued one, or an empty PDU) at
    /// `at`. Returns its end time.
    fn conn_tx(&mut self, c: &mut Conn, ch: u8, at: u64) -> u64 {
        if c.inflight.is_none() {
            c.inflight = Some(c.txq.pop_front().unwrap_or((0x1, Vec::new())));
        }
        let (llid, payload) = c.inflight.clone().unwrap();
        let md = !c.txq.is_empty();
        let mut h = llid & 0x3;
        if c.rx_nesn {
            h |= 1 << 2;
        }
        if c.tx_sn {
            h |= 1 << 3;
        }
        if md {
            h |= 1 << 4;
        }
        let mut pdu = vec![h, payload.len() as u8];
        pdu.extend_from_slice(&payload);
        let end = at + airtime_ns(pdu.len());
        self.tx(ch, c.aa, c.crc_init, pdu, at);
        end
    }

    /// Handle a packet from the peripheral. Returns its MD bit.
    fn conn_rx(&mut self, c: &mut Conn, f: &BleAirFrame) -> bool {
        let h = f.pdu[0];
        let llid = h & 0x3;
        let nesn = h & (1 << 2) != 0;
        let sn = h & (1 << 3) != 0;
        let md = h & (1 << 4) != 0;
        if !c.established {
            c.established = true;
            self.note("ll", "connection established (first packet acknowledged)");
        }
        if nesn != c.tx_sn {
            // Our packet was acknowledged.
            if let Some((l, p)) = c.inflight.take() {
                if c.terminating && l == 0x3 && p.first() == Some(&0x02) {
                    self.note("ll", "LL_TERMINATE_IND acknowledged — disconnected");
                    self.set_state(CentralState::Disconnected);
                }
            }
            c.tx_sn = !c.tx_sn;
        }
        if sn == c.rx_nesn {
            c.rx_nesn = !c.rx_nesn;
            let payload = f.pdu[2..].to_vec();
            if !payload.is_empty() {
                match llid {
                    0x3 => self.ll_control(c, &payload),
                    0x2 => {
                        c.rx_l2cap = payload;
                        self.l2cap_try(c);
                    }
                    0x1 => {
                        c.rx_l2cap.extend_from_slice(&payload);
                        self.l2cap_try(c);
                    }
                    _ => {}
                }
            }
        }
        md
    }

    fn ll_control(&mut self, c: &mut Conn, p: &[u8]) {
        let op = p[0];
        let reply: Option<Vec<u8>> = match op {
            0x02 => {
                self.note(
                    "ll",
                    format!(
                        "RX LL_TERMINATE_IND reason {:#04x}",
                        p.get(1).copied().unwrap_or(0)
                    ),
                );
                self.set_state(CentralState::Disconnected);
                None
            }
            // LL_FEATURE_REQ / LL_PERIPHERAL_FEATURE_REQ -> LL_FEATURE_RSP.
            0x08 | 0x0E => {
                self.note(
                    "ll",
                    "RX LL_FEATURE_REQ -> LL_FEATURE_RSP (no optional features)",
                );
                Some(vec![0x09, 0, 0, 0, 0, 0, 0, 0, 0])
            }
            // LL_VERSION_IND -> ours (5.0, company 0xFFFF).
            0x0C => {
                self.note("ll", "RX LL_VERSION_IND -> LL_VERSION_IND");
                Some(vec![0x0C, 0x09, 0xFF, 0xFF, 0x00, 0x00])
            }
            // LL_LENGTH_REQ -> LL_LENGTH_RSP with the minimum (27 / 328 µs).
            0x14 => {
                self.note("ll", "RX LL_LENGTH_REQ -> LL_LENGTH_RSP 27/328");
                Some(vec![0x15, 27, 0, 0x48, 0x01, 27, 0, 0x48, 0x01])
            }
            // LL_PING_REQ -> LL_PING_RSP.
            0x12 => Some(vec![0x13]),
            // LL_CONNECTION_PARAM_REQ -> LL_REJECT_EXT_IND (unsupported
            // remote feature): parameter updates are not modelled.
            0x0F => {
                self.note("ll", "RX LL_CONNECTION_PARAM_REQ -> LL_REJECT_EXT_IND 0x1A");
                Some(vec![0x11, 0x0F, 0x1A])
            }
            // Responses to requests we never send, and indications with no
            // reply: ignore.
            0x07 | 0x09 | 0x0D | 0x11 | 0x13 | 0x15 => None,
            _ => {
                self.note("ll", format!("RX LL control {op:#04x} -> LL_UNKNOWN_RSP"));
                Some(vec![0x07, op])
            }
        };
        if let Some(r) = reply {
            // Control PDUs go ahead of queued data.
            c.txq.push_front((0x3, r));
        }
    }

    fn l2cap_try(&mut self, c: &mut Conn) {
        if c.rx_l2cap.len() < 4 {
            return;
        }
        let len = u16::from_le_bytes([c.rx_l2cap[0], c.rx_l2cap[1]]) as usize;
        if c.rx_l2cap.len() < 4 + len {
            return;
        }
        let cid = u16::from_le_bytes([c.rx_l2cap[2], c.rx_l2cap[3]]);
        let sdu = c.rx_l2cap[4..4 + len].to_vec();
        c.rx_l2cap.clear();
        match cid {
            CID_ATT => self.att_rx(c, &sdu),
            CID_SIGNALING => self.signaling_rx(c, &sdu),
            _ => self.note("l2cap", format!("RX on CID {cid:#06x} ignored")),
        }
    }

    fn l2cap_send(c: &mut Conn, cid: u16, sdu: &[u8]) {
        let mut frame = Vec::with_capacity(4 + sdu.len());
        frame.extend_from_slice(&(sdu.len() as u16).to_le_bytes());
        frame.extend_from_slice(&cid.to_le_bytes());
        frame.extend_from_slice(sdu);
        for (i, chunk) in frame.chunks(LL_MAX_PAYLOAD).enumerate() {
            let llid = if i == 0 { 0x2 } else { 0x1 };
            c.txq.push_back((llid, chunk.to_vec()));
        }
    }

    fn signaling_rx(&mut self, c: &mut Conn, p: &[u8]) {
        if p.len() < 4 {
            return;
        }
        let code = p[0];
        let ident = p[1];
        match code {
            // Connection Parameter Update Request -> rejected (0x0001):
            // parameter updates are not modelled.
            0x12 => {
                self.note("l2cap", "RX conn param update request -> rejected");
                Self::l2cap_send(c, CID_SIGNALING, &[0x13, ident, 2, 0, 1, 0]);
            }
            0x13 | 0x01 => {}
            _ => {
                // Command reject, "command not understood".
                Self::l2cap_send(c, CID_SIGNALING, &[0x01, ident, 2, 0, 0, 0]);
            }
        }
    }

    // ── ATT ─────────────────────────────────────────────────────────────────

    fn att_request(&mut self, pending: AttPending, pdu: Vec<u8>) {
        self.note("att_tx", describe_att(&pdu));
        self.att = Some(pending);
        if let Some(c) = self.conn.as_mut() {
            Self::l2cap_send(c, CID_ATT, &pdu);
        }
    }

    /// `att_request` for use while the connection is borrowed out of `self`.
    fn att_request_on(&mut self, c: &mut Conn, pending: AttPending, pdu: Vec<u8>) {
        self.note("att_tx", describe_att(&pdu));
        self.att = Some(pending);
        Self::l2cap_send(c, CID_ATT, &pdu);
    }

    fn att_rx(&mut self, c: &mut Conn, p: &[u8]) {
        if p.is_empty() {
            return;
        }
        let op = p[0];
        self.note("att_rx", describe_att(p));
        let t_us = self.now_ns / 1_000;
        match op {
            // Server-initiated: MTU exchange -> keep 23.
            0x02 => Self::l2cap_send(c, CID_ATT, &[0x03, ATT_MTU as u8, 0]),
            // Notification / indication.
            0x1B | 0x1D if p.len() >= 3 => {
                let handle = u16::from_le_bytes([p[1], p[2]]);
                let uuid = self
                    .chars
                    .iter()
                    .find(|(_, ch, _)| ch.value_handle == handle)
                    .map(|(_, ch, _)| ch.uuid.clone())
                    .unwrap_or_default();
                if self.notifications.len() >= NOTIFY_KEEP {
                    self.notifications.pop_front();
                }
                self.notifications.push_back(GattValue {
                    t_us,
                    uuid,
                    handle,
                    value: p[3..].to_vec(),
                });
                self.notification_count += 1;
                if op == 0x1D {
                    Self::l2cap_send(c, CID_ATT, &[0x1E]);
                }
            }
            // Any other request from the server: this client has no GATT
            // database of its own.
            0x04 | 0x06 | 0x08 | 0x0A | 0x0C | 0x0E | 0x10 | 0x12 | 0x16 | 0x18 | 0x20 => {
                let err = if matches!(op, 0x04 | 0x06 | 0x08 | 0x10) {
                    0x0A
                } else {
                    0x06
                };
                Self::l2cap_send(c, CID_ATT, &[0x01, op, 0, 0, err]);
            }
            _ => self.att_response(c, p),
        }
    }

    fn att_response(&mut self, c: &mut Conn, p: &[u8]) {
        let Some(pending) = self.att.take() else {
            return;
        };
        let op = p[0];
        let t_us = self.now_ns / 1_000;
        let is_error = op == 0x01;
        match pending {
            AttPending::Services { .. } => {
                let mut last_end = 0xFFFF;
                if op == 0x11 && p.len() >= 2 {
                    let each = p[1] as usize;
                    if each >= 6 {
                        for e in p[2..].chunks(each) {
                            if e.len() < each {
                                break;
                            }
                            let start = u16::from_le_bytes([e[0], e[1]]);
                            let end = u16::from_le_bytes([e[2], e[3]]);
                            self.services.push(Service {
                                uuid: e[4..].to_vec(),
                                start,
                                end,
                            });
                            last_end = end;
                        }
                    }
                }
                if !is_error && last_end != 0xFFFF {
                    let s = last_end + 1;
                    self.att_request_on(
                        c,
                        AttPending::Services { start: s },
                        read_by_group_type(s, 0xFFFF, 0x2800),
                    );
                } else {
                    self.next_char_discovery(c, 0);
                }
            }
            AttPending::Chars { svc, .. } => {
                let mut last = None;
                if op == 0x09 && p.len() >= 2 {
                    let each = p[1] as usize;
                    if each >= 7 {
                        for e in p[2..].chunks(each) {
                            if e.len() < each {
                                break;
                            }
                            let decl = u16::from_le_bytes([e[0], e[1]]);
                            let props = e[2];
                            let value_handle = u16::from_le_bytes([e[3], e[4]]);
                            let uuid = e[5..].to_vec();
                            let svc_uuid = uuid_to_string(&self.services[svc].uuid);
                            self.chars.push((
                                svc,
                                GattCharacteristic {
                                    service_uuid: svc_uuid,
                                    uuid: uuid_to_string(&uuid),
                                    properties: props,
                                    decl_handle: decl,
                                    value_handle,
                                    cccd_handle: None,
                                },
                                uuid,
                            ));
                            last = Some(decl);
                        }
                    }
                }
                let svc_end = self.services[svc].end;
                match last {
                    Some(d) if !is_error && d < svc_end => {
                        let s = d + 1;
                        self.att_request_on(
                            c,
                            AttPending::Chars { svc, start: s },
                            read_by_type(s, svc_end, 0x2803),
                        );
                    }
                    _ => self.next_char_discovery(c, svc + 1),
                }
            }
            AttPending::Descs { chr, end, .. } => {
                let mut last = None;
                if op == 0x05 && p.len() >= 2 && p[1] == 1 {
                    for e in p[2..].chunks(4) {
                        if e.len() < 4 {
                            break;
                        }
                        let h = u16::from_le_bytes([e[0], e[1]]);
                        let t = u16::from_le_bytes([e[2], e[3]]);
                        if t == 0x2902 {
                            self.chars[chr].1.cccd_handle = Some(h);
                        }
                        last = Some(h);
                    }
                }
                match last {
                    Some(h) if !is_error && h < end => {
                        self.att_request_on(
                            c,
                            AttPending::Descs {
                                chr,
                                start: h + 1,
                                end,
                            },
                            find_information(h + 1, end),
                        );
                    }
                    _ => self.next_desc_discovery(c, chr + 1),
                }
            }
            AttPending::Read { chr } => {
                let (uuid, handle) = {
                    let ch = &self.chars[chr].1;
                    (ch.uuid.clone(), ch.value_handle)
                };
                if op == 0x0B {
                    let v = p[1..].to_vec();
                    self.note("gatt", format!("read {uuid} = {}", show_value(&v)));
                    self.reads.push(GattValue {
                        t_us,
                        uuid,
                        handle,
                        value: v,
                    });
                } else {
                    self.note("error", format!("read {uuid} failed: {}", describe_att(p)));
                }
            }
            AttPending::Write { chr, value } => {
                let (uuid, handle) = {
                    let ch = &self.chars[chr].1;
                    (ch.uuid.clone(), ch.value_handle)
                };
                if op == 0x13 {
                    self.note("gatt", format!("wrote {uuid} = {}", show_value(&value)));
                    self.writes_acked.push(GattValue {
                        t_us,
                        uuid,
                        handle,
                        value,
                    });
                } else {
                    self.note("error", format!("write {uuid} failed: {}", describe_att(p)));
                }
            }
            AttPending::Cccd { chr } => {
                let uuid = self.chars[chr].1.uuid.clone();
                if op == 0x13 {
                    self.note("gatt", format!("subscribed to {uuid}"));
                } else {
                    self.note(
                        "error",
                        format!("subscribe {uuid} failed: {}", describe_att(p)),
                    );
                }
            }
        }
    }

    fn next_char_discovery(&mut self, c: &mut Conn, svc: usize) {
        if svc < self.services.len() {
            let s = &self.services[svc];
            let (start, end) = (s.start, s.end);
            self.att_request_on(
                c,
                AttPending::Chars { svc, start },
                read_by_type(start, end, 0x2803),
            );
        } else {
            self.next_desc_discovery(c, 0);
        }
    }

    fn next_desc_discovery(&mut self, c: &mut Conn, mut chr: usize) {
        while chr < self.chars.len() {
            let (svc, ch, _) = &self.chars[chr];
            let start = ch.value_handle.saturating_add(1);
            let end = self
                .chars
                .get(chr + 1)
                .filter(|(s, _, _)| s == svc)
                .map(|(_, n, _)| n.decl_handle.saturating_sub(1))
                .unwrap_or(self.services[*svc].end);
            if start <= end {
                self.att_request_on(
                    c,
                    AttPending::Descs { chr, start, end },
                    find_information(start, end),
                );
                return;
            }
            chr += 1;
        }
        self.discovery_done = true;
        let summary: Vec<String> = self
            .chars
            .iter()
            .map(|(_, ch, _)| {
                format!(
                    "{} props={:#04x} value={:#06x}{}",
                    ch.uuid,
                    ch.properties,
                    ch.value_handle,
                    ch.cccd_handle
                        .map(|h| format!(" cccd={h:#06x}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        self.note(
            "gatt",
            format!(
                "discovered {} services, {} characteristics: {}",
                self.services.len(),
                self.chars.len(),
                summary.join("; ")
            ),
        );
    }
}

// ── PDU builders and helpers ─────────────────────────────────────────────────

fn read_by_group_type(start: u16, end: u16, ty: u16) -> Vec<u8> {
    let mut p = vec![0x10];
    p.extend_from_slice(&start.to_le_bytes());
    p.extend_from_slice(&end.to_le_bytes());
    p.extend_from_slice(&ty.to_le_bytes());
    p
}

fn read_by_type(start: u16, end: u16, ty: u16) -> Vec<u8> {
    let mut p = vec![0x08];
    p.extend_from_slice(&start.to_le_bytes());
    p.extend_from_slice(&end.to_le_bytes());
    p.extend_from_slice(&ty.to_le_bytes());
    p
}

fn find_information(start: u16, end: u16) -> Vec<u8> {
    let mut p = vec![0x04];
    p.extend_from_slice(&start.to_le_bytes());
    p.extend_from_slice(&end.to_le_bytes());
    p
}

/// Advertising data as `(type, value)` pairs.
pub fn parse_ad(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let len = data[i] as usize;
        if len == 0 || i + 1 + len > data.len() {
            break;
        }
        out.push((data[i + 1], data[i + 2..i + 1 + len].to_vec()));
        i += 1 + len;
    }
    out
}

/// `aa:bb:cc:dd:ee:ff` (MSB first) from an on-air (LSB first) address.
pub fn fmt_addr(a: &[u8; 6]) -> String {
    a.iter()
        .rev()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Parse `"180f"`, `"0x180F"` or a 128-bit UUID string into on-air
/// (little-endian) bytes.
pub fn parse_uuid(s: &str) -> Option<Vec<u8>> {
    let h: String = s
        .trim()
        .trim_start_matches("0x")
        .chars()
        .filter(|c| *c != '-')
        .collect();
    if h.len() != 4 && h.len() != 32 {
        return None;
    }
    let mut v = parse_hex(&h)?;
    v.reverse();
    Some(v)
}

fn expand_uuid(u: &[u8]) -> Option<[u8; 16]> {
    const BASE: [u8; 16] = [
        0xFB, 0x34, 0x9B, 0x5F, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0, 0, 0, 0,
    ];
    match u.len() {
        2 => {
            let mut b = BASE;
            b[12] = u[0];
            b[13] = u[1];
            Some(b)
        }
        16 => u.try_into().ok(),
        _ => None,
    }
}

fn uuid_eq(a: &[u8], b: &[u8]) -> bool {
    matches!((expand_uuid(a), expand_uuid(b)), (Some(x), Some(y)) if x == y)
}

/// Canonical string of an on-air UUID: `180f` for 16-bit, the dashed form for
/// 128-bit.
pub fn uuid_to_string(u: &[u8]) -> String {
    let be: Vec<u8> = u.iter().rev().copied().collect();
    let h: String = be.iter().map(|b| format!("{b:02x}")).collect();
    if be.len() == 16 {
        format!(
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    } else {
        h
    }
}

pub fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn show_value(v: &[u8]) -> String {
    let hex: String = v.iter().map(|b| format!("{b:02x}")).collect();
    if !v.is_empty() && v.iter().all(|b| (0x20..0x7F).contains(b)) {
        format!("\"{}\" ({hex})", String::from_utf8_lossy(v))
    } else {
        hex
    }
}

fn step_label(s: &CentralStep) -> String {
    match s {
        CentralStep::Connect => "connect".into(),
        CentralStep::Discover => "discover".into(),
        CentralStep::Read(u) => format!("read {u}"),
        CentralStep::Write(w) => format!("write {}", w.uuid),
        CentralStep::Subscribe(u) => format!("subscribe {u}"),
        CentralStep::WaitNotify(w) => format!("wait for {} notifications", w.count),
        CentralStep::WaitMs(ms) => format!("wait {ms} ms"),
        CentralStep::Disconnect => "disconnect".into(),
    }
}

/// One-line human description of an ATT PDU, for transcripts and the UI.
pub fn describe_att(p: &[u8]) -> String {
    let Some(&op) = p.first() else {
        return "empty ATT PDU".into();
    };
    let h = |i: usize| {
        p.get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .unwrap_or(0)
    };
    let rest = |i: usize| show_value(p.get(i..).unwrap_or(&[]));
    match op {
        0x01 => format!(
            "Error Response (req {:#04x}, handle {:#06x}, error {:#04x})",
            p.get(1).copied().unwrap_or(0),
            h(2),
            p.get(4).copied().unwrap_or(0)
        ),
        0x02 => format!("Exchange MTU Request ({})", h(1)),
        0x03 => format!("Exchange MTU Response ({})", h(1)),
        0x04 => format!("Find Information Request {:#06x}..{:#06x}", h(1), h(3)),
        0x05 => format!("Find Information Response ({} bytes)", p.len() - 1),
        0x08 => format!(
            "Read By Type Request {:#06x}..{:#06x} type {}",
            h(1),
            h(3),
            uuid_to_string(p.get(5..).unwrap_or(&[]))
        ),
        0x09 => format!("Read By Type Response ({} bytes)", p.len() - 1),
        0x0A => format!("Read Request handle {:#06x}", h(1)),
        0x0B => format!("Read Response {}", rest(1)),
        0x10 => format!(
            "Read By Group Type Request {:#06x}..{:#06x} type {}",
            h(1),
            h(3),
            uuid_to_string(p.get(5..).unwrap_or(&[]))
        ),
        0x11 => format!("Read By Group Type Response ({} bytes)", p.len() - 1),
        0x12 => format!("Write Request handle {:#06x} = {}", h(1), rest(3)),
        0x13 => "Write Response".into(),
        0x1B => format!("Handle Value Notification {:#06x} = {}", h(1), rest(3)),
        0x1D => format!("Handle Value Indication {:#06x} = {}", h(1), rest(3)),
        0x1E => "Handle Value Confirmation".into(),
        0x52 => format!("Write Command handle {:#06x} = {}", h(1), rest(3)),
        _ => format!("ATT opcode {op:#04x} ({} bytes)", p.len()),
    }
}

/// One-line description of any PDU on a BLE air, for the browser's BLE tab
/// and traces: advertising PDUs by type, data PDUs by LLID with the L2CAP/ATT
/// payload decoded when it starts a frame.
pub fn describe_pdu(f: &BleAirFrame) -> String {
    let Some(&h) = f.pdu.first() else {
        return "empty".into();
    };
    let payload = f.pdu.get(2..).unwrap_or(&[]);
    if f.access_address == ADV_AA {
        let name = match h & 0x0F {
            0x0 => "ADV_IND",
            0x1 => "ADV_DIRECT_IND",
            0x2 => "ADV_NONCONN_IND",
            0x3 => "SCAN_REQ",
            0x4 => "SCAN_RSP",
            0x5 => "CONNECT_IND",
            0x6 => "ADV_SCAN_IND",
            _ => "ADV (other)",
        };
        let adva = payload
            .get(0..6)
            .and_then(|a| <[u8; 6]>::try_from(a).ok())
            .map(|a| fmt_addr(&a))
            .unwrap_or_default();
        return format!("{name} {adva}");
    }
    let llid = h & 0x3;
    let flags = format!(
        "SN{} NESN{}{}",
        (h >> 3) & 1,
        (h >> 2) & 1,
        if h & 0x10 != 0 { " MD" } else { "" }
    );
    match llid {
        0x1 if payload.is_empty() => format!("LL empty ({flags})"),
        0x1 => format!("L2CAP continuation {} bytes ({flags})", payload.len()),
        0x2 if payload.len() >= 4 => {
            let cid = u16::from_le_bytes([payload[2], payload[3]]);
            match cid {
                CID_ATT => format!("ATT {} ({flags})", describe_att(&payload[4..])),
                CID_SIGNALING => format!(
                    "L2CAP signalling code {:#04x} ({flags})",
                    payload.get(4).copied().unwrap_or(0)
                ),
                _ => format!("L2CAP CID {cid:#06x} ({flags})"),
            }
        }
        0x3 => format!(
            "LL control {:#04x} {} ({flags})",
            payload.first().copied().unwrap_or(0),
            ll_control_name(payload.first().copied().unwrap_or(0xFF))
        ),
        _ => format!("LL data LLID {llid} ({flags})"),
    }
}

fn ll_control_name(op: u8) -> &'static str {
    match op {
        0x00 => "CONNECTION_UPDATE_IND",
        0x01 => "CHANNEL_MAP_IND",
        0x02 => "TERMINATE_IND",
        0x07 => "UNKNOWN_RSP",
        0x08 => "FEATURE_REQ",
        0x09 => "FEATURE_RSP",
        0x0C => "VERSION_IND",
        0x0E => "PERIPHERAL_FEATURE_REQ",
        0x0F => "CONNECTION_PARAM_REQ",
        0x11 => "REJECT_EXT_IND",
        0x12 => "PING_REQ",
        0x13 => "PING_RSP",
        0x14 => "LENGTH_REQ",
        0x15 => "LENGTH_RSP",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_parse_and_print_both_ways() {
        let u = parse_uuid("beb5483e-36e1-4688-b7f5-ea07361b26a8").unwrap();
        assert_eq!(u.len(), 16);
        assert_eq!(u[0], 0xa8, "little-endian on the air");
        assert_eq!(uuid_to_string(&u), "beb5483e-36e1-4688-b7f5-ea07361b26a8");
        let s = parse_uuid("0x2902").unwrap();
        assert_eq!(s, vec![0x02, 0x29]);
        // A 16-bit UUID equals its 128-bit expansion.
        let full = parse_uuid("00002902-0000-1000-8000-00805f9b34fb").unwrap();
        assert!(uuid_eq(&s, &full));
    }

    #[test]
    fn advertising_data_is_split_into_structures() {
        let ad = [0x02, 0x01, 0x06, 0x06, 0x09, b'E', b'S', b'P', b'3', b'2'];
        let v = parse_ad(&ad);
        assert_eq!(v[0], (0x01, vec![0x06]));
        assert_eq!(v[1], (0x09, b"ESP32".to_vec()));
    }

    /// A scripted central that finds nobody stays scanning and says so.
    #[test]
    fn with_no_advertiser_the_central_keeps_scanning() {
        let air = BleAirBus::new();
        let mut c = ScriptedCentral::new(
            air,
            CentralConfig {
                target_name: Some("nobody".into()),
                script: vec![CentralStep::Connect],
                ..Default::default()
            },
        );
        c.advance_to(10_000_000);
        assert_eq!(c.state(), CentralState::Scanning);
        assert!(!c.script_done());
    }

    /// The CONNECT_IND answers the ADV_IND at exactly T_IFS on the same
    /// channel, is addressed to the advertiser, and carries parameters a
    /// real peripheral accepts.
    #[test]
    fn a_matching_adv_ind_is_answered_with_a_connect_ind_at_t_ifs() {
        let air = BleAirBus::new();
        let mut c = ScriptedCentral::new(
            air.clone(),
            CentralConfig {
                target_name: Some("ESP32".into()),
                script: vec![CentralStep::Connect],
                ..Default::default()
            },
        );
        c.advance_to(1_000_000);
        let adva = [0x04, 0, 0, 0, 0, 0x02];
        let mut pdu = vec![0x20, 0];
        pdu.extend_from_slice(&adva);
        pdu.extend_from_slice(&[0x02, 0x01, 0x06, 0x06, 0x09, b'E', b'S', b'P', b'3', b'2']);
        pdu[1] = (pdu.len() - 2) as u8;
        let t0 = 2_000_000;
        let adv_end = t0 + airtime_ns(pdu.len());
        air.transmit(BleAirFrame {
            seq: 0,
            source: 99,
            channel: 39,
            access_address: ADV_AA,
            crc_init: ADV_CRC,
            pdu,
            air_ns: Some(t0),
        });
        c.advance_to(adv_end + 10_000);
        let ci = air
            .receive_window(39, ADV_AA, adv_end + T_IFS_NS, adv_end + T_IFS_NS, 99)
            .expect("CONNECT_IND at T_IFS");
        assert_eq!(ci.pdu[0] & 0x0F, 0x05);
        assert_eq!(ci.pdu[1], 34);
        assert_eq!(&ci.pdu[8..14], &adva, "AdvA is the advertiser");
        let interval = u16::from_le_bytes([ci.pdu[2 + 12 + 10], ci.pdu[2 + 12 + 11]]);
        assert_eq!(interval, 24);
        assert_eq!(c.state(), CentralState::Connected);
    }
}
