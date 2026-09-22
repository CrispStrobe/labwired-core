//! Owns safe batch-width and idle-fast-forward planning.

use crate::{AdvanceRequest, BatchPolicy, BreakpointPolicy, Cpu, Machine};

/// Narrows the planned batch width, remembering which clause did it.
///
/// Every clamp in `plan_cpu_window` goes through this so the binding clause is
/// named rather than inferred. Under the default feature set `clause` is
/// unused and this is a plain `min` — see `machine::quantum_trace`.
macro_rules! clamp {
    ($count:ident, $binder:ident, $clause:expr, $limit:expr) => {{
        let limit = $limit;
        if limit < $count {
            $count = limit;
            // NOT feature-gated. `binder` is read unconditionally now --
            // `fill_to_cycles` below asks whether the tick boundary is what
            // bound this window, and that question has to have a real answer
            // in the build that ships, not only under `quantum-trace`.
            //
            // It was gated, and the first version of the window-filling fix
            // inherited the gate: `binder` stayed `UNBOUNDED` in a normal
            // build, the condition was always false, and the fix was a no-op
            // that compiled, passed and measured EXACTLY the old width (25.0).
            // One store on a branch that already assigns `$count` is not worth
            // a lever that only works in a build nobody ships.
            $binder = $clause;
        }
    }};
}

/// One planned CPU window: how many instructions it may retire, and how many
/// cycles it may still consume before the next peripheral tick boundary.
///
/// The second field exists because those two limits are not the same number on
/// every core. Where an instruction is one cycle they are interchangeable and
/// `fill_to_cycles` is `None`. Where instruction cycles are CLOCK TIME (AVR,
/// 1..=4), the step count is the window divided by the LONGEST instruction, so
/// a window of cheaper instructions stops short — and the executor needs the
/// cycle figure to finish filling it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CpuWindow {
    pub(crate) steps: u32,
    /// Cycles to the next tick boundary, when the tick boundary is what bound
    /// this window AND the core's cycles are clock time. `None` otherwise —
    /// including when some other clause bound it, because continuing past that
    /// clause's limit would violate it.
    pub(crate) fill_to_cycles: Option<u64>,
}

impl<C: Cpu> Machine<C> {
    pub(crate) fn plan_cpu_window(
        &mut self,
        request: AdvanceRequest,
        fuel_consumed: u64,
        elapsed_cycles: u64,
    ) -> CpuWindow {
        use crate::machine::quantum_trace::clause;

        self.service_resident_edges_at_boundary();
        let tick_interval = u64::from(self.config.peripheral_tick_interval.max(1));
        // Every clamp below that is measured in cycles is turned into a step
        // count. One step is one cycle on most cores; on a core whose step
        // cycles are clock time (AVR, up to 4) the budget is divided by the
        // longest step, so a window never runs more than one step past a
        // cycle limit, a tick boundary or a deadline. `1` for every other
        // concrete core, so this compiles to the identity there.
        let step_cycles = if self.cpu.instruction_cycles_are_time() {
            u64::from(self.cpu.max_step_cycles().max(1))
        } else {
            1
        };
        let steps_within = |cycles: u64| (cycles / step_cycles).max(1);
        let mut count = u64::from(u32::MAX);
        let mut binder = clause::UNBOUNDED;

        if let Some(limit) = request.limits().fuel {
            clamp!(
                count,
                binder,
                clause::FUEL_LIMIT,
                limit.saturating_sub(fuel_consumed)
            );
        }
        if let Some(limit) = request.limits().simulated_cycles {
            clamp!(
                count,
                binder,
                clause::CYCLE_LIMIT,
                steps_within(limit.saturating_sub(elapsed_cycles))
            );
        }
        if let BatchPolicy::AtMost(cap) = request.batch_policy() {
            clamp!(count, binder, clause::BATCH_POLICY, u64::from(cap.get()));
        }
        if let Some(deadline) = self.bus.next_motor_service_deadline_cycle() {
            clamp!(
                count,
                binder,
                clause::MOTOR_DEADLINE,
                steps_within(deadline.saturating_sub(self.total_cycles))
            );
        }

        // Dual-core: only lockstep while APP is active or still in reset-hold.
        // WAITI-parked APP (FreeRTOS idle) lets PRO batch.
        // A single-step request is already clamped to one instruction. Its
        // secondary classification cannot narrow the plan further, so avoid
        // charging that query to every reference-path instruction.
        let secondary_state = if request.is_single() {
            None
        } else {
            self.cpu_secondary
                .as_ref()
                .map(|sec| sec.secondary_execution_state())
        };
        let secondary_parked = secondary_state == Some(crate::SecondaryExecutionState::ParkedIdle);
        let secondary_reset_held =
            secondary_state == Some(crate::SecondaryExecutionState::ResetHeld);
        let secondary_lockstep =
            self.cpu_secondary.is_some() && !secondary_parked && !secondary_reset_held;

        // Reset fidelity is enforced by the party that can see the request,
        // not by pinning the quantum for the life of the bus:
        //   * SCB (Cortex-M SYSRESETREQ) — the CPU batch loop breaks on the
        //     instruction that writes AIRCR, via the latch shared by
        //     `configure_cortex_m` (`CortexM::sysreset_signal`), so the
        //     boundary drain lands exactly where quantum-1 put it.
        //   * RTC_CNTL (ESP SW_SYS_RST) — clamps only while a request is
        //     actually latched; `boundary.rs` steps the dual-core WAITI window
        //     one instruction at a time and breaks the moment it latches.
        // `scb_index.is_some()` used to sit here too, and since
        // `configure_cortex_m` installs an SCB on EVERY Cortex-M bus that
        // meant every ARM board ran at one instruction per batch forever —
        // discarding the whole walk-deletion batching win (measured ~16x on
        // NUCLEO-L476RG, ~9x on NUCLEO-F401RE).
        //
        // The other half of that clause's old rationale — "cycle-accurate push
        // capture" — was already carried elsewhere and needs nothing here:
        // push-mode capture advances the tap clock per RETIRED INSTRUCTION
        // inside the batch (`CortexM::step_batch` calls `tap.bump_clock()`
        // before each `step_internal`), and poll-mode capture has its own
        // `poll_sampling` arm below.
        let reset_fidelity = self.rtc_cntl_reset_pending();

        // Pending cycle-accurate bus cells and operations require a lifecycle
        // commit after every instruction.
        let cycle_accurate_bus = self.bus.requires_cycle_accurate();
        // Poll-mode capture must sample every committed instruction boundary.
        let poll_sampling = self.logic_capture.poll_active();
        // Honored breakpoints must be observed before executing past their PC.
        let honored_breakpoints =
            request.breakpoint_policy() == BreakpointPolicy::Honor && !self.breakpoints.is_empty();

        if reset_fidelity
            || secondary_lockstep
            || cycle_accurate_bus
            || poll_sampling
            || honored_breakpoints
        {
            // Attribute to the specific arm, not the disjunction: "something in
            // this `if` fired" is the answer that made #835 an elimination
            // exercise in the first place.
            let arm = if reset_fidelity {
                clause::RESET_FIDELITY
            } else if secondary_lockstep {
                clause::SECONDARY_LOCKSTEP
            } else if cycle_accurate_bus {
                clause::CYCLE_ACCURATE_BUS
            } else if poll_sampling {
                clause::POLL_SAMPLING
            } else {
                clause::HONORED_BREAKPOINTS
            };
            clamp!(count, binder, arm, 1);
        } else if secondary_parked || secondary_reset_held {
            // Coalesced dual-core idle batch: while the secondary core is
            // WAITI-parked the primary may retire several instructions per
            // machine boundary. Commit advances peripherals once with
            // elapsed = primary_steps (see boundary.rs).
            //
            // ⚠️ It may NOT run past the next peripheral tick boundary. This
            // clause used to clamp at a flat 1024 and skip the tick clamp
            // entirely — the comment even advertised "multi-instruction PRO
            // windows even when tick_interval is 1". That is not a batching
            // decision, it is a licence to stop observing peripherals: nothing
            // between the two boundaries re-derives an interrupt level, so an
            // IRQ raised at instruction 1 of the window is not seen by the CPU
            // until instruction 1024, whatever the caller set the tick interval
            // to. ESP-IDF's SMP FreeRTOS does not survive that — see the
            // `sync_esp32s3_irq_write` write-choke note in `bus/routing.rs` for
            // the deadlock it produces (`portYIELD_WITHIN_API` lands late,
            // `xQueueReceive` re-blocks an already-blocked task, `vListInsert`
            // links the event-list item to itself and spins forever with the
            // queue spinlock held). Both halves are needed: the write choke
            // makes an MMIO-raised level visible at the write, this clamp keeps
            // a *timed* one visible within one tick interval.
            //
            // The coalescing win survives wherever it was ever sound — at
            // `tick_interval > 1` (the browser's `RECOMMENDED_TICK_INTERVAL`)
            // the window is still hundreds of instructions wide. At interval 1
            // the caller asked for per-cycle peripheral service and now gets it.
            //
            // This flat 1024 is itself further narrowed below by the
            // `next_event_deadline()` clamp — previously only applied on the
            // non-parked path — so a scheduler-driven peripheral's interrupt
            // (e.g. the ESP32-S3 SYSTIMER TARGET0 alarm) can never be
            // delivered late just because the APP core happened to be
            // WAITI-parked when it came due. See
            // `docs/performance/2026-09-18-xtensa-batched.md` for the trace
            // that caught a real ~1000-cycle-late delivery here.
            clamp!(count, binder, clause::SECONDARY_PARKED, 1024);
            // A write can arm a grid waveform inside this window. Stop on the
            // next grid even when no edge was pending before the batch.
            if self.bus.has_grid_gpio_schedules() {
                let until_tick = tick_interval - (self.total_cycles % tick_interval);
                clamp!(
                    count,
                    binder,
                    clause::TICK_BOUNDARY,
                    steps_within(until_tick)
                );
            }
            // The parked core's own timer (Xtensa CCOMPARE0 — the FreeRTOS
            // tick source on the APP CPU) is not a scheduler event: the parked
            // path fast-forwards CCOUNT over the window and would only raise
            // the edge at its end, up to 1023 cycles after a quantum-1 run
            // raises it. End the window on the edge instead.
            if let Some(until) = self
                .cpu_secondary
                .as_ref()
                .and_then(|sec| sec.parked_wake_deadline_cycles())
            {
                clamp!(
                    count,
                    binder,
                    clause::SECONDARY_WAKE_DEADLINE,
                    steps_within(until)
                );
            }
        } else {
            // Normal path: batch only up to the next peripheral tick boundary.
            let until_tick = tick_interval - (self.total_cycles % tick_interval);
            clamp!(
                count,
                binder,
                clause::TICK_BOUNDARY,
                steps_within(until_tick)
            );
        }

        // Scheduler/resident waveform deadlines narrow the window on BOTH the
        // non-parked (tick-boundary) path and the secondary-parked
        // (coalesced dual-idle) path above: a pending event must never be
        // delivered later than its deadline just because the window's flat
        // cap (tick boundary, or 1024 while WAITI-parked) would otherwise run
        // past it. Previously gated on `!secondary_parked`, which let a
        // scheduler-driven peripheral's interrupt (e.g. ESP32-S3 SYSTIMER
        // TARGET0) fire up to ~1023 cycles late whenever the APP core was
        // idle-parked — see `docs/performance/2026-09-18-xtensa-batched.md`.
        if count > 1 {
            if let Some(deadline) = self.bus.next_resident_edge_deadline_cycle() {
                let until = deadline.saturating_sub(self.total_cycles);
                clamp!(
                    count,
                    binder,
                    clause::RESIDENT_EDGE_DEADLINE,
                    steps_within(until).min(u64::from(u32::MAX))
                );
            }
        }
        // The scheduler heap is always present (empty without the
        // `event-scheduler` feature, where nothing enqueues), so this clamp
        // needs no feature gate of its own.
        if tick_interval > 1 && count > 1 {
            if let Some(deadline) = self.sched.next_event_deadline() {
                let until = if deadline > self.total_cycles {
                    deadline - self.total_cycles
                } else {
                    1
                };
                clamp!(
                    count,
                    binder,
                    clause::SCHEDULER_DEADLINE,
                    steps_within(until)
                );
            }
        }

        let count = count.max(1);
        #[cfg(feature = "quantum-trace")]
        crate::machine::quantum_trace::record(binder, count);
        // On a core whose step cycles are CLOCK TIME, `steps_within` divided
        // the window by the LONGEST instruction, so a batch that retires
        // cheaper instructions stops short of the boundary. The leftover is
        // then re-divided on the next plan, and the widths decay: measured on
        // `atmega328p`, `tick_boundary` bound 42230 of 42231 windows and the
        // mean width was 23.68 where `512 / 4` is 128.
        //
        // Hand the executor the cycles this window may still consume, so it
        // can re-issue conservative sub-batches until the boundary is actually
        // reached instead of returning a half-empty window to the planner.
        // ONLY when the tick boundary is what bound it: looping past any other
        // clause's limit would violate that clause.
        let fill_to_cycles = (step_cycles > 1 && binder == clause::TICK_BOUNDARY)
            .then(|| tick_interval - (self.total_cycles % tick_interval));
        CpuWindow {
            steps: count as u32,
            fill_to_cycles,
        }
    }
}
