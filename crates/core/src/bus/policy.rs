// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Bus policy: path resolve, cycle-accurate mode, safe tick interval, resident waveform policy.

use super::*;

impl SystemBus {
    pub(crate) fn resolve_peripheral_path(
        manifest: &SystemManifest,
        descriptor_path: &str,
    ) -> PathBuf {
        let raw = PathBuf::from(descriptor_path);
        if raw.is_absolute() {
            return raw;
        }

        let chip_path = Path::new(&manifest.chip);
        let chip_dir = chip_path.parent().unwrap_or_else(|| Path::new("."));
        let chip_relative = chip_dir.join(descriptor_path);
        if chip_relative.exists() {
            chip_relative
        } else {
            raw
        }
    }

    /// Exact residents and legacy service contracts require instruction boundaries.
    /// Grid-only residents may batch only when live cycle publication is available.
    ///
    /// H5 FLASH does not pin this predicate. The Cortex-M batch loop watches
    /// [`Self::has_pending_flash_op`] after each instruction and ends the batch
    /// on the write that records an erase or bank swap. A compiled block cannot
    /// do that probe, so the JIT hosts OR in [`Self::models_flash_ops`] themselves.
    ///
    /// HOT: called per batch plan, per interpreted step, and in the idle
    /// fast-forward check. Every clause stays a short scan of the resident list.
    #[inline]
    pub fn requires_cycle_accurate(&self) -> bool {
        self.gpio_devices
            .iter()
            .any(|d| d.needs_per_cycle_service())
            || (self.has_grid_gpio_schedules() && !self.resident_grid_batching_enabled())
    }

    pub(crate) fn has_grid_gpio_schedules(&self) -> bool {
        self.gpio_devices
            .iter()
            .any(|device| device.has_grid_schedules())
    }

    #[inline]
    pub(crate) fn resident_grid_batching_enabled(&self) -> bool {
        cfg!(feature = "event-scheduler")
            && self.legacy_walk_disabled
            && !self.resident_scheduling_disabled
            && self.config.peripheral_tick_interval > 1
    }

    /// Whether any FLASH on this bus records hardware operations as pending
    /// ops (H5 erase/bank-swap, U5 page erase). A cached bool — no scan.
    /// Callers on the hot path should prefer [`Self::has_pending_flash_op`],
    /// which short-circuits on this and answers the question they actually
    /// have.
    pub fn models_flash_ops(&self) -> bool {
        self.flash_models_ops
    }

    /// Non-consuming pending-op probe, called after EACH Cortex-M instruction
    /// on an H5/U5 bus. The cached `flash_models_ops` bool short-circuits it
    /// on every other bus, so the scan only runs where an op can exist.
    pub fn has_pending_flash_op(&self) -> bool {
        self.flash_models_ops
            && self
                .peripherals
                .iter()
                .any(|entry| entry.dev.has_pending_op())
    }

    /// Largest recommended peripheral interval; never changes the configured grid.
    ///
    /// H5 FLASH is not an arm: the CPU batch ends on the instruction that
    /// records an operation, independently of peripheral tick pacing.
    pub fn max_safe_tick_interval(&self) -> u32 {
        if self
            .gpio_devices
            .iter()
            .any(|d| d.needs_per_cycle_service())
            || (self.has_grid_gpio_schedules() && self.resident_scheduling_disabled)
        {
            return 1;
        }
        if cfg!(feature = "event-scheduler") && self.legacy_walk_disabled {
            RECOMMENDED_TICK_INTERVAL
        } else {
            1
        }
    }

    /// True when the per-cycle tick (`tick_peripherals_fully`) has no orchestration
    /// work beyond the NVIC scan: the legacy peripheral walk is deleted, no
    /// bus-aware peripheral needs a pre-tick pass, no Nordic GPIO/GPIOTE service
    /// is wired, no CAN synthetic testers are attached, and no resident needs
    /// ordinary tick service. On such a bus the tick early-outs to just the NVIC
    /// aggregation, avoiding the phase-1 orchestration and its allocations every
    /// cycle. Only meaningful under the `event-scheduler` feature (the walk is
    /// never deleted otherwise).
    ///
    /// Neither ESP32 interrupt-matrix fabric pins this to `false` any more.
    /// On a walk-DELETED bus there are no tick-produced peripheral sources
    /// (nothing walks — `irq_fabric.*.walk_sources` is rebuilt from an empty
    /// list every tick), and every remaining routing input is re-derived where
    /// it changes: at the MMIO write choke (`sync_esp32c3_irq_cache_write` /
    /// `sync_esp32s3_irq_write`) and on the event path
    /// (`deliver_scheduled_irq_levels`). So the per-cycle tick genuinely has
    /// nothing left to do. The C3 additionally needs its declarative INTC cache
    /// (hand-built buses without it fall back to a per-tick register read,
    /// which is then the only aggregation point); the S3 intmatrix is a native
    /// model that is always decoded, so it needs no such condition. See
    /// [`InterruptFabric::per_cycle_aggregation_free`](crate::bus::InterruptFabric::per_cycle_aggregation_free).
    ///
    /// `gpio_devices` counts as work. A bus-resident device (keypad, rotary
    /// encoder, DHT22) DRIVES pins the firmware samples, and
    /// `service_gpio_devices` — the pass that does the driving — lives inside
    /// the phase-1 body this predicate skips. Omitting the check made those
    /// devices silently inert on every walk-deleted bus: attach succeeded,
    /// `list_inputs` advertised the channel, `set_input` returned Ok, and the
    /// pin never moved. Walk deletion is a performance decision about
    /// peripheral ORCHESTRATION; a device driving a pin is not orchestration,
    /// and no fast path may decide it stops existing.
    ///
    /// The cost is bounded: only buses that actually host such a device give up
    /// the fast path, and `service_gpio_devices` early-outs on an empty list,
    /// so a bus without one is unaffected. Buttons deliberately do NOT rely on
    /// this — a contact level changes only when something drives it, so it is
    /// applied at the stimulus point (`sync_button_inputs`) and a button alone
    /// never costs a bus its fast path.
    #[cfg(feature = "event-scheduler")]
    #[inline]
    pub(crate) fn per_cycle_tick_is_trivial(&self) -> bool {
        self.legacy_walk_disabled
            && self.bus_tick_indices.is_empty()
            && !self.nordic_gpio_service
            && self
                .irq_fabric
                .per_cycle_aggregation_free(self.legacy_walk_disabled)
            && self.can_diagnostic_testers.is_empty()
            && self.can_uds_testers.is_empty()
            && self.can_log_players.is_empty()
            && self.no_gpio_device_needs_service()
            // A Tier-2 part with an `outputs:` pin needs the per-tick pass that
            // drains its queue onto the pad. Without this line the walk-free
            // fast path would silently un-wire every declarative INT line —
            // exactly the shape of bug `no_gpio_device_needs_service` exists
            // for, arriving through the other door.
            && self.device_pin_pads.is_empty()
    }

    /// Whether NO attached bus-resident device needs the per-cycle service pass
    /// — i.e. skipping [`service_gpio_devices`](Self::service_gpio_devices)
    /// changes nothing observable.
    ///
    /// A [`Button`](crate::peripherals::components::button::Button) is exempt:
    /// its level is applied when the contact is driven, not per cycle, so a
    /// canvas that adds a push button keeps the walk-free fast path. Every
    /// other device is scanned or sampled per tick and does need it.
    ///
    /// Vacuously true on an empty list, which is what preserves the fast path
    /// for the overwhelmingly common bus that hosts no such device at all.
    #[cfg(feature = "event-scheduler")]
    #[inline]
    fn no_gpio_device_needs_service(&self) -> bool {
        self.gpio_devices
            .iter()
            .all(|d| !d.needs_per_cycle_service())
    }
}
