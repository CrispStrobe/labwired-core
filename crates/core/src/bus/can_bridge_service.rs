// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! `SystemBus` side of the CAN bridge ([`crate::network::can_bridge`]): the
//! per-tick service, the path testers and log players take when a bridge
//! sits on their controller, and the host-facing API (offer, pause, report).

use super::SystemBus;
use crate::network::can_bridge::{
    CanBridge, CanBridgeAction, CanBridgeReport, CanFrameSource, CanOfferOutcome, CanPathVerdict,
    CanTimelineEvent, CanTimelineLane,
};
use crate::network::{CanFrame, CanRxRejection};
use crate::peripherals::{bxcan::BxCan, fdcan::Fdcan};

/// The two CAN controller models, reached through one downcast site.
enum CanCtl<'a> {
    Fd(&'a mut Fdcan),
    Bx(&'a mut BxCan),
}

impl SystemBus {
    /// Index of the bridge on `controller`, if one is attached.
    pub(crate) fn can_bridge_index(&self, controller: &str) -> Option<usize> {
        if self.can_bridges.is_empty() {
            return None;
        }
        self.can_bridges
            .iter()
            .position(|b| b.controller == controller)
    }

    /// Attach a bridge. Its controller must be a CAN controller on this bus,
    /// and a controller takes at most one bridge (two would each see half the
    /// traffic).
    pub fn attach_can_bridge(&mut self, bridge: CanBridge) -> Result<(), String> {
        let Some(idx) = self.find_peripheral_index_by_name(&bridge.controller) else {
            return Err(format!(
                "can-bridge '{}': controller '{}' was not found",
                bridge.id, bridge.controller
            ));
        };
        let is_can = self.can_ctl(idx).is_some();
        if !is_can {
            return Err(format!(
                "can-bridge '{}': '{}' is not a CAN controller (FDCAN or bxCAN)",
                bridge.id, bridge.controller
            ));
        }
        if self.can_bridge_index(&bridge.controller).is_some() {
            return Err(format!(
                "can-bridge '{}': controller '{}' already has a bridge",
                bridge.id, bridge.controller
            ));
        }
        if self.can_bridges.iter().any(|b| b.id == bridge.id) {
            return Err(format!(
                "can-bridge '{}': the id is already used",
                bridge.id
            ));
        }
        let mut bridge = bridge;
        bridge.console_seq = self.bus_trace.latest_seq();
        if bridge.config().replay_start == crate::network::can_bridge::CanReplayStart::Attach {
            bridge.start_replay_at(self.current_cycle);
        }
        self.can_bridges.push(bridge);
        Ok(())
    }

    /// Remove the bridge named `id`; its controller is then reached directly
    /// again. Returns the bridge (its recording and report stay readable).
    pub fn detach_can_bridge(&mut self, id: &str) -> Result<CanBridge, String> {
        let i = self
            .can_bridges
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| format!("no can-bridge named '{id}'"))?;
        Ok(self.can_bridges.remove(i))
    }

    /// Names of the CAN controllers (FDCAN, bxCAN) on this bus, in bus order.
    pub fn can_controller_names(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        for i in 0..self.peripherals.len() {
            if self.can_ctl(i).is_some() {
                names.push(self.peripherals[i].name.clone());
            }
        }
        names
    }

    /// The bridge named `id`.
    pub fn can_bridge(&self, id: &str) -> Option<&CanBridge> {
        self.can_bridges.iter().find(|b| b.id == id)
    }

    /// Tell every bridge the simulation stopped (`true`) or resumed (`false`)
    /// advancing virtual time. A host calls this when its run loop pauses, a
    /// debugger halts, or it resumes.
    pub fn can_bridges_set_paused(&mut self, paused: bool) {
        let now = self.current_cycle;
        for b in &mut self.can_bridges {
            b.set_paused(paused, now);
        }
    }

    /// A live frame from outside the simulation for the bridge `id`.
    pub fn can_bridge_offer(
        &mut self,
        id: &str,
        frame: CanFrame,
        host_time_ms: Option<f64>,
    ) -> Result<CanOfferOutcome, String> {
        let now = self.current_cycle;
        let b = self
            .can_bridges
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or_else(|| format!("no can-bridge named '{id}'"))?;
        b.offer(frame, now, host_time_ms)
    }

    /// The run-result block of the bridge `id`, with the progress of every
    /// tester on the same controller merged into its timeline.
    pub fn can_bridge_report(&self, id: &str) -> Option<CanBridgeReport> {
        let b = self.can_bridge(id)?;
        let mut extra: Vec<CanTimelineEvent> = Vec::new();
        for t in &self.can_uds_testers {
            if t.connection == b.controller {
                extra.extend(t.events.iter().cloned());
            }
        }
        if let Some((cycle, bytes)) = &b.console_partial {
            extra.push(console_event(*cycle, bytes));
        }
        Some(b.report(&extra))
    }

    /// Every bridge's report, in attach order.
    pub fn can_bridge_reports(&self) -> Vec<CanBridgeReport> {
        self.can_bridges
            .iter()
            .filter_map(|b| self.can_bridge_report(&b.id))
            .collect()
    }

    /// Put a frame on a controller's receive path, clock- and state-gated as
    /// [`Self::inject_can_frame`] is, and arm the RX wake-ups.
    fn deliver_to_controller(&mut self, idx: usize, frame: CanFrame) -> Result<(), CanRxRejection> {
        let clocked = self.is_peripheral_clocked(idx);
        let result = match self.can_ctl(idx) {
            None => Err(CanRxRejection::NotRunning),
            Some(_) if !clocked => Err(CanRxRejection::Unclocked),
            Some(CanCtl::Bx(_)) if frame.fd => Err(CanRxRejection::FdOnClassicController),
            Some(CanCtl::Bx(bx)) => bx.try_deliver_rx(frame),
            Some(CanCtl::Fd(fd)) => fd.try_receive_frame(frame),
        };
        self.collect_scheduled_events(idx);
        result
    }

    /// A tester or log player on a bridged controller sends through here, so
    /// its frames are recorded and pass the CAN-path faults. Returns `true`
    /// when the frame left the sender (delivered, held by a delay, or lost to
    /// a drop fault) and `false` when the controller refused it (the sender
    /// retries, as before).
    pub(crate) fn deliver_can_external(
        &mut self,
        idx: usize,
        bridge: usize,
        frame: CanFrame,
        source: CanFrameSource,
    ) -> bool {
        let now = self.current_cycle;
        match self.can_bridges[bridge].pass_rx(frame, source, now) {
            CanPathVerdict::Deliver(frame) => {
                let result = self.deliver_to_controller(idx, frame.clone());
                let ok = result.is_ok();
                self.can_bridges[bridge].note_rx_result(
                    &frame,
                    source,
                    now,
                    result.map_err(|e| e.to_string()),
                );
                ok
            }
            CanPathVerdict::Held | CanPathVerdict::Dropped => true,
        }
    }

    /// Frames the bridged controller transmitted this round, after the
    /// CAN-path faults (the tester's view of the wire).
    pub(crate) fn take_bridged_tx(&mut self, bridge: usize) -> Vec<CanFrame> {
        std::mem::take(&mut self.can_bridges[bridge].tx_ready)
    }

    fn can_ctl(&mut self, idx: usize) -> Option<CanCtl<'_>> {
        let any = self.peripherals.get_mut(idx)?.dev.as_any_mut()?;
        if any.is::<Fdcan>() {
            any.downcast_mut::<Fdcan>().map(CanCtl::Fd)
        } else {
            any.downcast_mut::<BxCan>().map(CanCtl::Bx)
        }
    }

    fn drain_controller_tx(&mut self, idx: usize) -> Vec<CanFrame> {
        match self.can_ctl(idx) {
            Some(CanCtl::Fd(fd)) => fd.tx_frames.drain(..).collect(),
            Some(CanCtl::Bx(bx)) => bx.tx_frames.drain(..).collect(),
            None => Vec::new(),
        }
    }

    fn apply_bridge_action(&mut self, bridge: usize, idx: usize, action: CanBridgeAction) {
        let now = self.current_cycle;
        let (fault, detail) = match action {
            CanBridgeAction::NodeReset(i) => {
                // What firmware does to reset itself: a word write to AIRCR
                // with VECTKEY and SYSRESETREQ (PRIGROUP kept). The machine
                // drains the latch at the next instruction boundary.
                const AIRCR: u64 = 0xE000_ED0C;
                let detail = if self.find_peripheral_index_by_name("scb").is_some() {
                    let prigroup = crate::Bus::read_u32(self, AIRCR).map_or(0, |v| v & 0x0700);
                    match crate::Bus::write_u32(self, AIRCR, 0x05FA_0004 | prigroup) {
                        Ok(()) => "node reset: SYSRESETREQ written to AIRCR; the core restarts \
                                   at the next instruction boundary (peripheral registers keep \
                                   their state, as on that path)"
                            .to_string(),
                        Err(e) => format!("NOT APPLIED: writing AIRCR failed: {e}"),
                    }
                } else {
                    "NOT APPLIED: node reset needs a Cortex-M SCB on this bus".to_string()
                };
                (i, detail)
            }
            CanBridgeAction::BusOff(i) => {
                let detail = match self.can_ctl(idx) {
                    Some(CanCtl::Fd(fd)) => {
                        fd.enter_bus_off();
                        "controller forced bus-off: PSR.BO set, CCCR.INIT set, IR.BO raised; \
                         it stays off the bus until firmware clears CCCR.INIT"
                            .to_string()
                    }
                    _ => "NOT APPLIED: bus-off is modeled on FDCAN only".to_string(),
                };
                self.collect_scheduled_events(idx);
                (i, detail)
            }
        };
        self.can_bridges[bridge].note_fault(fault, now, detail);
    }

    /// Read console bytes the firmware printed since the last round and put
    /// each finished line on the bridge's timeline (the firmware's reaction).
    fn poll_bridge_console(&mut self, bridge: usize) {
        let after = self.can_bridges[bridge].console_seq;
        let bytes = self.bus_trace.uart_bytes_since(after);
        if bytes.is_empty() {
            return;
        }
        for (seq, cycle, _bus, dir, byte) in bytes {
            let b = &mut self.can_bridges[bridge];
            b.console_seq = seq;
            if dir != crate::bus::bus_trace::BusDir::Tx {
                continue;
            }
            if byte == b'\n' {
                if let Some((start, line)) = b.console_partial.take() {
                    let ev = console_event(start, &line);
                    b.push_event(ev);
                }
                continue;
            }
            if byte == b'\r' {
                continue;
            }
            let partial = b.console_partial.get_or_insert_with(|| (cycle, Vec::new()));
            if partial.1.len() < 240 {
                partial.1.push(byte);
            }
        }
    }

    /// Per-tick service: faults due by cycle, frames the controller sent, and
    /// frames due into it (live, captured, replayed, delayed).
    pub(crate) fn service_can_bridges(&mut self) {
        if self.can_bridges.is_empty() {
            return;
        }
        let now = self.current_cycle;
        for bi in 0..self.can_bridges.len() {
            let controller = self.can_bridges[bi].controller.clone();
            let Some(idx) = self.find_peripheral_index_by_name(&controller) else {
                continue;
            };
            self.poll_bridge_console(bi);

            for action in self.can_bridges[bi].take_actions(now) {
                self.apply_bridge_action(bi, idx, action);
            }

            // Out of the controller. What a tester did not take last round is
            // not kept: `tx_ready` holds one round.
            self.can_bridges[bi].tx_ready.clear();
            for frame in self.drain_controller_tx(idx) {
                if let CanPathVerdict::Deliver(f) = self.can_bridges[bi].pass_tx(frame, now) {
                    self.can_bridges[bi].tx_ready.push(f);
                }
            }
            let due_tx = self.can_bridges[bi].due_tx(now);
            self.can_bridges[bi].tx_ready.extend(due_tx);

            // Into the controller.
            for (frame, source, filtered) in self.can_bridges[bi].due_rx(now) {
                let frame = if filtered {
                    frame
                } else {
                    match self.can_bridges[bi].pass_rx(frame, source, now) {
                        CanPathVerdict::Deliver(f) => f,
                        _ => continue,
                    }
                };
                let result = self.deliver_to_controller(idx, frame.clone());
                self.can_bridges[bi].note_rx_result(
                    &frame,
                    source,
                    now,
                    result.map_err(|e| e.to_string()),
                );
            }

            // Frame-triggered faults (node reset on a frame) fire this round.
            for action in self.can_bridges[bi].take_actions(now) {
                self.apply_bridge_action(bi, idx, action);
            }
        }
    }
}

fn console_event(cycle: u64, bytes: &[u8]) -> CanTimelineEvent {
    CanTimelineEvent {
        cycle,
        t_us: 0.0,
        lane: CanTimelineLane::Console,
        kind: "console".into(),
        dir: None,
        source: None,
        id: None,
        dlc: None,
        data: None,
        detail: String::from_utf8_lossy(bytes).into_owned(),
    }
}
