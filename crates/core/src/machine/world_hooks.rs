// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! What a multi-node [`World`](crate::world::World) needs from one machine to
//! run a timed network: advance to a cycle, wake its timed USARTs, reset the
//! node, and report GPIO marker edges.

use crate::{AdvanceRequest, AdvanceStop, Cpu, Machine, SimResult};

impl<C: Cpu> Machine<C> {
    /// Run until `total_cycles >= target` (or the CPU stops making progress).
    /// Batched, with idle fast-forward, as a continuous run.
    pub fn advance_to_cycle(&mut self, target: u64) -> SimResult<()> {
        let mut guard = 0u32;
        while self.total_cycles < target {
            let report = self
                .advance(AdvanceRequest::run(None).with_cycle_limit(target - self.total_cycles))?;
            if matches!(
                report.stop,
                AdvanceStop::NoProgress | AdvanceStop::FirmwareExit { .. }
            ) {
                break;
            }
            guard += 1;
            if guard > 1_000_000 {
                break;
            }
        }
        Ok(())
    }

    /// Hand every timed USART a chance to schedule a wake for a character a
    /// peer put on the wire since it last looked. A world calls this at the
    /// start of each synchronisation round.
    pub fn timed_uart_sync(&mut self) {
        #[cfg(feature = "event-scheduler")]
        {
            let now = self.total_cycles;
            for idx in 0..self.bus.peripherals.len() {
                let Some(uart) = self.bus.peripherals[idx]
                    .dev
                    .as_any()
                    .and_then(|a| a.downcast_ref::<crate::peripherals::uart::Uart>())
                else {
                    continue;
                };
                if !uart.is_timed() {
                    continue;
                }
                if let Some(target) = uart.timed_claim_wake(now) {
                    self.sched.schedule(target, idx as u32, 0);
                }
            }
        }
    }

    /// Reset the node the way its reset pin does: the core restarts through
    /// its reset vector, the NVIC and SysTick return to their reset state, and
    /// every peripheral gets [`crate::Peripheral::on_node_reset`]. SRAM keeps
    /// its contents, as on silicon.
    pub fn reset_node(&mut self) -> SimResult<()> {
        if let Some(nvic) = &self.bus.nvic {
            use std::sync::atomic::Ordering;
            for w in 0..8 {
                nvic.iser[w].store(0, Ordering::SeqCst);
                nvic.ispr[w].store(0, Ordering::SeqCst);
                nvic.iabr[w].store(0, Ordering::SeqCst);
                nvic.level_pended[w].store(0, Ordering::SeqCst);
            }
            for p in nvic.ipr.iter() {
                p.store(0, Ordering::SeqCst);
            }
        }
        for p in self.bus.peripherals.iter_mut() {
            p.dev.on_node_reset();
        }
        self.reset()
    }

    /// Watch GPIO pads for marker edges: `(gpio peripheral id, pin)` each.
    pub fn watch_marker_pins(&mut self, pins: &[(String, u8)]) -> anyhow::Result<()> {
        let mut sources = Vec::with_capacity(pins.len());
        for (name, pin) in pins {
            let idx = self
                .bus
                .find_peripheral_index_by_name(name)
                .ok_or_else(|| anyhow::anyhow!("no peripheral '{name}'"))?;
            sources.push(Some(crate::logic_capture::LogicSource::pad(idx, *pin)));
        }
        self.logic_watch(&sources);
        Ok(())
    }

    /// Marker edges since `cursor`: `(channel, cycle, level)`, and the cursor
    /// to pass next time.
    pub fn marker_edges(&mut self, cursor: u64) -> (Vec<(u32, u64, bool)>, u64) {
        let batch = self.logic_read_edges(cursor);
        (
            batch
                .edges
                .iter()
                .map(|e| (e.ch, e.cycle, e.value))
                .collect(),
            batch.cursor,
        )
    }
}
