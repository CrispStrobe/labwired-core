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
        // Only an event-scheduler run drains `sched`; on the per-cycle walk a
        // timed USART services itself every tick and needs no wake.
        if !self.scheduler_bootstrapped {
            return;
        }
        let now = self.total_cycles;
        for idx in 0..self.bus.peripherals.len() {
            if let Some(target) = self.bus.peripherals[idx].dev.claim_timed_uart_wake(now) {
                self.sched.schedule(target, idx as u32, 0);
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

    /// Mark GPIO pads `(peripheral id, pin)` as members of a world `gpio_net`:
    /// from now on each reports only what this chip drives. Call before the
    /// pads are watched ([`Self::watch_marker_pins`]), which seeds each
    /// pad's drive. Refuses a pad whose model cannot take part, or whose
    /// drive is not known yet (a pad routed to a peripheral signal the model
    /// does not publish), naming it.
    pub fn isolate_net_pads(&mut self, pins: &[(String, u8)]) -> anyhow::Result<()> {
        for (name, pin) in pins {
            let idx = self
                .bus
                .find_peripheral_index_by_name(name)
                .ok_or_else(|| anyhow::anyhow!("no peripheral '{name}'"))?;
            let dev = &mut self.bus.peripherals[idx].dev;
            if !dev.set_gpio_net_isolated(*pin, true) {
                anyhow::bail!(
                    "peripheral '{name}' cannot take part in a GPIO net (pin {pin}): its GPIO model has no net support"
                );
            }
            if dev.read_gpio_pad_drive(*pin).is_none() {
                anyhow::bail!(
                    "pad {name}.{pin} cannot be on a GPIO net: its drive is not known (is it routed to a peripheral signal the model does not publish?)"
                );
            }
        }
        Ok(())
    }

    /// Four-state drive changes of the watched channels since `cursor`:
    /// `(channel, cycle, state)`, and the cursor to pass next time.
    pub fn net_pad_states(
        &mut self,
        cursor: u64,
    ) -> (Vec<(u32, u64, crate::logic_capture::PadState)>, u64) {
        let batch = self.logic_read_states(cursor);
        (
            batch
                .edges
                .iter()
                .map(|e| (e.ch, e.cycle, e.state))
                .collect(),
            batch.cursor,
        )
    }

    /// Four-state value of each watched channel when the watch was armed.
    pub fn net_pad_initial_states(&self) -> Vec<Option<crate::logic_capture::PadState>> {
        self.logic_initial_states().to_vec()
    }

    /// Hold `pin` of GPIO peripheral `name` at `level` as an external driver
    /// would, through the same seam a board button uses (EXTI edges and timer
    /// captures fire). `false` if the pad does not resolve or cannot be driven.
    pub fn drive_gpio_input(&mut self, name: &str, pin: u8, level: bool) -> bool {
        match self.bus.find_peripheral_index_by_name(name) {
            Some(idx) => self.bus.set_peripheral_gpio_input(idx, pin, level),
            None => false,
        }
    }
}
