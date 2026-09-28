// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Shared BLE "virtual air" — the medium two link-layer controllers in one
//! world exchange packets over.
//!
//! This is the [`nrf52::radio::VirtualAirBus`](crate::peripherals::nrf52::radio::VirtualAirBus)
//! pattern, deliberately and literally: a shared in-process registry keyed by
//! the physical channel, `Arc<Mutex<…>>` so instances stay `Send` inside a
//! `Machine`, per-bus isolation so two labs in one process do not hear each
//! other, and a bounded TX ring the UI can poll. It is a **second registry, not
//! a second mechanism**, because the two radios trade different objects: the
//! nRF52 RADIO is a raw-buffer machine (whitened bytes + nRF logical
//! BASE/PREFIX address matching + a MODE tag), while the ESP32-C3's
//! RivieraWaves RW-BLE core is a *PDU* machine — firmware never sees a
//! whitened buffer, it stages a BLE PDU in exchange memory and the core does
//! the PHY. Forcing one frame type over both would mean inventing a bit layout
//! for whichever side did not measure it. A bridge between the two (BLE
//! channel index ↔ nRF `FREQUENCY`, access address ↔ `BASE0`/`PREFIX0`) is a
//! follow-up, and is the reason the frame here carries the access address and
//! CRC init explicitly rather than folding them into the bytes.
//!
//! ## Faithfully modelled
//!
//! * **The PDU itself.** [`BleAirFrame::pdu`] is the real on-air BLE PDU —
//!   header byte, length byte, then exactly the payload bytes the transmitting
//!   controller staged in its own exchange memory. Nothing is synthesised.
//! * **Channel selectivity.** A receiver only sees frames pushed on the RF
//!   channel index it is tuned to.
//! * **Access-address selectivity.** A receiver only accepts frames whose
//!   access address matches the one its control structure programmed, so an
//!   advertising scanner (`0x8E89BED6`) does not decode connection traffic.
//! * **Broadcast, not point-to-point.** Every receiver tuned to the channel
//!   sees the frame, which is what advertising *is*. (The nRF52 air consumes,
//!   i.e. one receiver per frame — correct for its 1:1 test topologies, wrong
//!   for advertising.) Implemented with a monotonic sequence number per frame
//!   and a per-receiver cursor.
//!
//! ## Idealised — present, but not physical
//!
//! * **The channel is lossless and collision-free.** No bit errors, no
//!   interference, no path loss, no two-transmitter collision, no capture
//!   effect. A frame that is pushed is a frame that is received.
//! * **Propagation is instantaneous** and there is no notion of distance or
//!   RSSI. A frame is visible to receivers the moment it is pushed.
//! * **No PHY.** No GFSK, no preamble, no access-address bit sync, no
//!   whitening and no CRC *computation* — the access address and CRC init
//!   travel as metadata so a receiver can filter on them, but the CRC is never
//!   generated or checked, and a receiver is told the frame is good.
//! * **The backlog is bounded** ([`AIR_DEPTH`] frames per channel). A receiver
//!   that stops draining silently misses the oldest frames rather than
//!   stalling the transmitter — which is closer to a real radio than an
//!   unbounded queue, but it is a choice, not a measurement.
//!
//! Not modelled at all: timing of the transfer (a frame is atomic), channel
//! hopping, encryption, and any form of medium arbitration.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

/// Frames retained per channel. A receiver reading the air at a slower cadence
/// than the advertiser transmits drops the oldest, exactly as a real receiver
/// that was not listening would have missed them.
pub const AIR_DEPTH: usize = 64;

/// Frames retained in the inspection trace (the UI/test view), independent of
/// what receivers have consumed.
const TX_HISTORY_CAP: usize = 200;

/// One BLE PDU on the air.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BleAirFrame {
    /// Monotonic sequence number within the bus. Receivers keep a cursor over
    /// this so a frame is broadcast to every listener rather than consumed by
    /// the first one.
    pub seq: u64,
    /// Opaque identity of the transmitting controller. A receiver never
    /// decodes its own transmission: a radio is not a full-duplex device and
    /// the RW-BLE core does not hand firmware back what it just sent.
    pub source: u64,
    /// RF channel index, 0..=39 in BLE numbering (37/38/39 are the primary
    /// advertising channels). Taken from the transmitter's programmed hop
    /// control word, not chosen here.
    pub channel: u8,
    /// Access address the transmitter's control structure programmed
    /// (`0x8E89BED6` for the advertising channels).
    pub access_address: u32,
    /// CRC init the transmitter's control structure programmed (`0x555555`
    /// for the advertising channels). Carried so a receiver can filter and a
    /// sniffer can reproduce the on-air bytes; never used to compute a CRC.
    pub crc_init: u32,
    /// The BLE PDU: `[header, length, payload…]`. `payload.len() == length`.
    pub pdu: Vec<u8>,
    /// When the packet's first bit (the preamble) went on the air, in ns of
    /// **world time** (the transmitting node's cycle count converted with its
    /// own clock; every node of a world starts at 0). `None` for a transmitter
    /// that does not know its time — such a frame is invisible to the timed
    /// receive path ([`BleAirBus::receive_window`]) and only reaches the
    /// legacy backlog path ([`BleAirBus::receive_from`]).
    ///
    /// This is what makes a connection possible at all: a request and its
    /// response are 150 µs apart (T_IFS), so a receiver must be able to ask
    /// "did a packet START inside my receive window", not merely "is there
    /// anything on this channel I have not seen".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_ns: Option<u64>,
}

/// The inter-frame space, T_IFS: 150 µs between the end of one packet and the
/// start of the next within an exchange (Core spec Vol 6 Part B 4.1.1).
pub const T_IFS_NS: u64 = 150_000;

/// On-air duration of `pdu` (header + length + payload bytes) on the LE 1M
/// PHY: 1 byte preamble + 4 bytes access address + the PDU + 3 bytes CRC, at
/// 1 µs per bit. Only the 1M PHY is modelled.
pub fn airtime_ns(pdu_len: usize) -> u64 {
    ((1 + 4 + pdu_len + 3) as u64) * 8 * 1_000
}

/// Time from a packet's first bit to the end of its access address, where a
/// receiver achieves sync (preamble + AA on 1M = 40 µs). The RW-BLE core
/// timestamps receptions here.
pub const SYNC_OFFSET_NS: u64 = 40_000;

impl BleAirFrame {
    /// Air time of the packet's last bit, when it carries a timestamp.
    pub fn end_ns(&self) -> Option<u64> {
        self.air_ns.map(|t| t + airtime_ns(self.pdu.len()))
    }
}

#[derive(Debug, Default)]
struct BleAir {
    /// Per-channel ring of frames still inside the retention window.
    channels: HashMap<u8, VecDeque<BleAirFrame>>,
    /// Next sequence number to hand out.
    next_seq: u64,
    /// Bounded transmit trace, oldest first. Never drained by receivers.
    tx_history: VecDeque<BleAirFrame>,
}

/// A shared BLE air. Controllers minted with the same bus hear each other;
/// controllers on different buses are fully isolated, so two BLE labs (or two
/// worker threads) can coexist in one process.
#[derive(Debug, Clone, Default)]
pub struct BleAirBus {
    inner: Arc<Mutex<BleAir>>,
}

impl BleAirBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a PDU onto `channel`. Returns the sequence number assigned.
    pub fn transmit(&self, frame: BleAirFrame) -> u64 {
        let Ok(mut air) = self.inner.lock() else {
            return 0;
        };
        let seq = air.next_seq;
        air.next_seq += 1;
        let mut frame = frame;
        frame.seq = seq;
        let ch = frame.channel;
        let q = air.channels.entry(ch).or_default();
        q.push_back(frame.clone());
        while q.len() > AIR_DEPTH {
            q.pop_front();
        }
        air.tx_history.push_back(frame);
        while air.tx_history.len() > TX_HISTORY_CAP {
            air.tx_history.pop_front();
        }
        seq
    }

    /// The oldest frame on `channel` with `seq >= cursor` whose access address
    /// matches and which `listener` did not itself transmit, or `None`.
    /// Broadcast: the frame stays on the air for other receivers. The caller
    /// advances its cursor past `frame.seq`.
    pub fn receive_from(
        &self,
        channel: u8,
        access_address: u32,
        cursor: u64,
        listener: u64,
    ) -> Option<BleAirFrame> {
        let air = self.inner.lock().ok()?;
        air.channels
            .get(&channel)?
            .iter()
            .find(|f| f.seq >= cursor && f.access_address == access_address && f.source != listener)
            .cloned()
    }

    /// The earliest timed frame on `channel` with access address
    /// `access_address`, not sent by `listener`, whose first bit went on the
    /// air inside `[from_ns, to_ns]`. This is a radio with its receiver open
    /// for exactly that window: anything that started earlier or later was
    /// not heard. Broadcast, like [`Self::receive_from`] — nothing is consumed.
    pub fn receive_window(
        &self,
        channel: u8,
        access_address: u32,
        from_ns: u64,
        to_ns: u64,
        listener: u64,
    ) -> Option<BleAirFrame> {
        let air = self.inner.lock().ok()?;
        air.channels
            .get(&channel)?
            .iter()
            .filter(|f| {
                f.access_address == access_address
                    && f.source != listener
                    && f.air_ns.is_some_and(|t| t >= from_ns && t <= to_ns)
            })
            .min_by_key(|f| (f.air_ns, f.seq))
            .cloned()
    }

    /// Every timed frame on any of `channels` that started inside
    /// `[from_ns, to_ns]` and was not sent by `listener`, oldest first. The
    /// scripted central's scanner uses this to see advertising on all three
    /// primary channels at once.
    pub fn frames_between(
        &self,
        channels: &[u8],
        from_ns: u64,
        to_ns: u64,
        listener: u64,
    ) -> Vec<BleAirFrame> {
        let Ok(air) = self.inner.lock() else {
            return Vec::new();
        };
        let mut out: Vec<BleAirFrame> = channels
            .iter()
            .filter_map(|ch| air.channels.get(ch))
            .flat_map(|q| q.iter())
            .filter(|f| {
                f.source != listener && f.air_ns.is_some_and(|t| t >= from_ns && t <= to_ns)
            })
            .cloned()
            .collect();
        out.sort_by_key(|f| (f.air_ns, f.seq));
        out
    }

    /// Most-recent-first snapshot of what has been transmitted, for tests and
    /// the air visualisation. Mirrors `VirtualAirBus::trace_snapshot`.
    pub fn trace_snapshot(&self) -> Vec<BleAirFrame> {
        match self.inner.lock() {
            Ok(a) => a.tx_history.iter().rev().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Drop every channel ring. Keeps the trace, exactly as
    /// `VirtualAirBus::clear` does.
    /// The sequence number the NEXT transmission will carry.
    ///
    /// A controller binding to this air starts its cursor here, so it hears
    /// only what is sent from that moment on. A radio has no history buffer:
    /// frames that crossed the air before it powered on are simply gone. This
    /// matters because a lab REUSES its air across a simulation restart (the
    /// playground mints a new `AirBus` only when the source/diagram hash
    /// changes), so without this a restarted node would replay the previous
    /// run's backlog as live peer traffic.
    pub fn current_seq(&self) -> u64 {
        self.inner.lock().map(|a| a.next_seq).unwrap_or(0)
    }

    pub fn clear(&self) {
        if let Ok(mut a) = self.inner.lock() {
            a.channels.clear();
        }
    }
}

/// Hand every controller on a BLE air a distinct identity, so no controller
/// ever decodes its own transmission.
///
/// ONE counter for the whole process, deliberately. Identities are compared
/// across controllers that share an air, and an air is shared by whatever ends
/// up in one lab — an ESP32-C3 RW-BLE core and a [`crate::peripherals::
/// virtual_ble::VirtualBle`] can be two nodes of the same world. Two
/// per-model counters would both start at 1, so the first controller of each
/// kind would collide and each would silently swallow the other's frames as
/// "its own transmission". That failure looks exactly like a dead radio.
pub fn next_node_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Process-global air, so two `Esp32c3Bt` models built by the ordinary factory
/// (which has no lab identity to thread through) share one medium — the same
/// transitional arrangement `nrf52::radio` uses for `Nrf52Radio::new()`.
pub fn default_ble_air_bus() -> &'static BleAirBus {
    static BUS: OnceLock<BleAirBus> = OnceLock::new();
    BUS.get_or_init(BleAirBus::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ch: u8, aa: u32, payload: &[u8]) -> BleAirFrame {
        BleAirFrame {
            seq: 0,
            air_ns: None,
            source: 1,
            channel: ch,
            access_address: aa,
            crc_init: 0x0055_5555,
            pdu: payload.to_vec(),
        }
    }

    /// Broadcast: two receivers with independent cursors both see one frame.
    #[test]
    fn a_frame_reaches_every_listener_on_the_channel() {
        let bus = BleAirBus::new();
        let seq = bus.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x0f, 1, 2, 3]));
        for cursor in [0u64, 0u64] {
            let got = bus
                .receive_from(37, 0x8E89_BED6, cursor, 2)
                .expect("delivered");
            assert_eq!(got.seq, seq);
            assert_eq!(got.pdu, vec![0x20, 0x0f, 1, 2, 3]);
        }
        // Advancing past it stops delivery.
        assert!(bus.receive_from(37, 0x8E89_BED6, seq + 1, 2).is_none());
    }

    /// Channel and access address both select.
    #[test]
    fn channel_and_access_address_select() {
        let bus = BleAirBus::new();
        bus.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x00]));
        assert!(
            bus.receive_from(38, 0x8E89_BED6, 0, 2).is_none(),
            "wrong channel"
        );
        assert!(
            bus.receive_from(37, 0x1234_5678, 0, 2).is_none(),
            "wrong AA"
        );
        assert!(bus.receive_from(37, 0x8E89_BED6, 0, 2).is_some());
    }

    /// A controller never decodes its own transmission.
    #[test]
    fn a_node_does_not_hear_itself() {
        let bus = BleAirBus::new();
        bus.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x00]));
        assert!(
            bus.receive_from(37, 0x8E89_BED6, 0, 1).is_none(),
            "own frame"
        );
        assert!(
            bus.receive_from(37, 0x8E89_BED6, 0, 2).is_some(),
            "someone else's"
        );
    }

    /// Buses are isolated: two labs in one process do not hear each other.
    #[test]
    fn buses_are_isolated() {
        let a = BleAirBus::new();
        let b = BleAirBus::new();
        a.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x00]));
        assert!(b.receive_from(37, 0x8E89_BED6, 0, 2).is_none());
    }

    /// `current_seq` is the join point a fresh controller uses to skip a
    /// backlog it was not present for.
    #[test]
    fn current_seq_tracks_what_has_already_crossed_the_air() {
        let bus = BleAirBus::new();
        assert_eq!(bus.current_seq(), 0, "nothing sent yet");
        for _ in 0..10 {
            bus.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x01]));
        }
        assert_eq!(bus.current_seq(), 10);
        assert!(
            bus.receive_from(37, 0x8E89_BED6, bus.current_seq(), 3)
                .is_none(),
            "a listener joining now hears none of the backlog",
        );
        assert!(
            bus.receive_from(37, 0x8E89_BED6, 0, 3).is_some(),
            "but the backlog is still there for cursor 0 — which is why the \
             cursor, not the bus, is what has to be fixed",
        );
    }

    /// The retention window is bounded and drops the oldest.
    #[test]
    fn the_backlog_is_bounded() {
        let bus = BleAirBus::new();
        for _ in 0..(AIR_DEPTH + 8) {
            bus.transmit(frame(37, 0x8E89_BED6, &[0x20, 0x00]));
        }
        let oldest = bus
            .receive_from(37, 0x8E89_BED6, 0, 2)
            .expect("something survives");
        assert_eq!(oldest.seq, 8, "the first 8 frames aged out");
    }
}
