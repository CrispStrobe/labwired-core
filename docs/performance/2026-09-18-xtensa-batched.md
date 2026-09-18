# Xtensa (ESP32-S3) batched CLI path

## What's there today

`run_firmware` (crates/cli/src/commands/run.rs) drives Xtensa through a
hand-written `while steps < limit { machine.step() }` loop instead of
`Machine::advance`. `--batched` is explicitly refused for Xtensa with a
message saying "the Xtensa path does not run through `Machine::advance`" —
that comment predates the `Machine::step() == advance(AdvanceRequest::single())`
refactor and is now stale: the step loop already calls `machine.step()`, so
it already goes through `advance`, just always at quantum 1.

## Tick-interval eligibility

`SystemBus::max_safe_tick_interval()` only looks at `legacy_walk_disabled`
(plus the `event-scheduler` feature and an HC-SR04 override) — it does not
depend on `InterruptFabric::per_cycle_aggregation_free`. `legacy_walk_disabled`
is derived by `bus.recompute_walk_deletable()`, which `configure_xtensa_esp32s3`
already calls at the end of peripheral registration. So on a build with
`--features event-scheduler`, an ESP32-S3 bus with an all-scheduler-driven
peripheral set can already report `RECOMMENDED_TICK_INTERVAL` (512) today,
*without* `perf/s3-irq-cache`.

`perf/s3-irq-cache` doesn't change tick-interval eligibility; it changes what
each per-cycle tick costs once batching is on. It removes the S3's hard
`per_cycle_aggregation_free` block, turning `per_cycle_tick_is_trivial` on for
walk-deleted S3 buses (the phase-1 orchestration and 29-peripheral re-poll
are skipped entirely, replaced by re-derivation at the MMIO write choke and
the event path). Its own differential gate (`esp32s3_irq_cache_differential`)
already validated 0 divergences over 40M Doom boundaries.

**Update after attempting the merge:** the walk-free S3 IRQ gate is *already
on `origin/main`* — `crates/core/tests/esp32s3_irq_cache_differential.rs` and
the `per_cycle_aggregation_free` S3 arm exist in this checkout under
different commit hashes (`836141a03`, `4e74bd4d1`) than the ones on
`perf/s3-irq-cache` (`104ac1f86`, `2e90b2278`) — same feature, landed
independently/re-based under a different history. That is exactly why the
raw `git merge origin/perf/s3-irq-cache` below produced conflicts spanning
~1,150 lines of `tick.rs` alone: it is not a small delta on top of main, it
is two copies of the same change with a month of unrelated `InterruptFabric`
refactor (`fec8268a7`, "eleven chip interrupt fields leave SystemBus")
sitting on top of one of them. **Decision: do NOT merge `perf/s3-irq-cache`.**
It brings nothing that is not already on main, and forcing the merge would
only risk re-deriving the same interrupt-routing logic incorrectly next to
its own already-passing differential gate. The `tick_interval=512` measured
below on the 40M Doom run confirms the walk-free gate is active without any
merge.

Original reasoning, still valid as the general principle (kept for the
record): the branch is dated Aug 22 and
`origin/main` has since landed an unrelated `InterruptFabric` extraction
("eleven chip interrupt fields leave SystemBus") that rewrites the same
files. Attempting the merge produced conflicts spanning ~1,150 lines of
`crates/core/src/bus/tick.rs` alone, plus `bus/routing.rs`, `cpu/riscv.rs`,
`docs/boards/VALIDATION_STATUS.md` and `validation/manifest.yaml` — i.e. two
independent structural rewrites of the same interrupt-routing code, not a
small textual conflict. Resolving that by hand, in the same change that also
introduces the batched CLI path, is exactly the situation the "byte-identity
is the hard bar" rule warns about: a wrong resolution would land silently
inside the interrupt-delivery code the differential gate exists to protect,
and it would be very hard to tell that divergence apart from a bug in the
new batched loop itself when the byte-identity oracle runs. Since the branch
is not required for `max_safe_tick_interval()` to report >1 (see above) —
only for the per-cycle constant factor once batching is already on — it is
safe to ship the batched path without it and land the rebase-and-merge of
`perf/s3-irq-cache` as its own follow-up PR, against current main, reviewed
on its own.

## Dual-core note

`Machine::advance` itself (not the CLI loop) already drains
`APPCPU_RESET_RELEASED` and unhalts `cpu_secondary` inside its own advance
loop (crates/core/src/machine/advance.rs), and `boundary.rs`'s
`coalesced_dual_idle` already accounts for a WAITI-parked secondary core
inside `Machine::advance`'s dual-core stepping. The CLI's step loop has its
own duplicate `APPCPU_RESET_RELEASED.take()` + `unhalt` purely for the
`eprintln!` message; `Machine::advance` performs the real unhalt regardless
of which frontend calls it. So the batched loop needs **no** extra dual-core
handling beyond calling `machine.advance` — the same as ARM's batched loop
needs none despite Cortex-M having no dual-core case at all; the generic
`Machine<C>::advance` carries this for every ISA.

## Approach

Add `run_xtensa_batched_loop`, mirroring `run_arm_batched_loop`: set
`peripheral_tick_interval` from `machine.bus.max_safe_tick_interval()` on
both `machine.config` and `machine.bus.config`, chunk at 4,000,000
instructions per `advance(AdvanceRequest::run(Some(fuel)))` call, stop on
`AdvanceStop::NoProgress` or zero forward progress, print the same
`[batched]` summary line. Dispatch it from `run_firmware` in place of the
current `--batched` refusal, but only for the plain rom-boot / fast-boot
path with no `--break-at` / `--watch-mem` / `--stop-on` (those need
per-instruction granularity the CLI loop currently provides via ring buffer
+ first-hit checks; `--batched` refuses that combination explicitly, same
shape as the existing RISC-V `--stimulus` refusal).

## 2026-09-18 follow-up: root-causing the 40M-step stdout divergence

### Identity evidence (unchanged by this investigation)

Doom flash `demo-esp32s3-doom-lab-flash.bin`, firmware
`tests/fixtures/tier1/esp32s3.elf`, `--rom-boot --allow-sim-error`:

| Run | stdout sha256 | bus-trace sha256 |
|---|---|---|
| step @ 5M | `c28194d680de69cd9d7d554…` | `ff44fe168fcb5b0888bebcd…` |
| batched @ 5M | `c28194d680de69cd9d7d554…` (match) | `ff44fe168fcb5b0888bebcd…` (match) |
| step @ 40M | `f63cd2557c5d3134d485e68…` | `ff44fe168fcb5b0888bebcd…` |
| batched @ 40M | `018430fc39fb65b572f366c…` (diverges) | `ff44fe168fcb5b0888bebcd…` (match) |

### Original hypothesis: WRONG

The task hypothesis was that a scheduler-driven ESP32-S3 timer peripheral
(SYSTIMER or TIMG) returns a stale free-running counter value when its
MMIO latch (`UNIT0_OP`/`TxUPDATE`) is read mid-batching-window, because the
counter is only advanced at `tick()`/window-boundary granularity.

This was checked directly:
- `Systimer::sync_counters_from_clock()` (called from the `UNIT0_OP`/`UNIT1_OP`
  write handlers) already implements exactly the lazy read-time catch-up
  pattern this task asked for, pulling "now" from the bus-published
  `CycleClock` before every latch — confirmed correct by inspection and by a
  temporary `eprintln!` on every `UNIT0_OP` write comparing step vs batched:
  **all `SYSTIMER_OP` events were byte-identical between step and batched
  through cycle 14,405,138** (1,469 consecutive identical lines), well past
  where the eventual stdout divergence originates.
- `Esp32s3TimerGroup`'s `TxUPDATE` (`0x0C`/`0x30`) latch handlers had NO
  equivalent catch-up — they copied `self.tX.counter` directly without first
  advancing it from the published clock. A `sync_counters_from_clock()`
  matching the systimer pattern was implemented and wired into both TxUPDATE
  handlers.
- **This fix had zero effect**: rebuilding with it in place and re-running
  the 40M-step batched case reproduced the *exact same* stdout sha256
  (`018430fc39fb65b572f366c…`) as the unfixed baseline. TIMG0/1 TxUPDATE is
  not on the path that produces the divergence in this firmware. The fix
  was reverted (`git checkout -- crates/core/src/peripherals/esp32s3/timer_group.rs`)
  since it doesn't address the real bug and would just be dead code sitting
  next to a differential gate.

### Actual root cause: found and localized

Instrumenting `Systimer::on_event` dispatch (`crate::lib.rs`
`drain_scheduler_events_inner`) with a temporary per-event
`eprintln!(cycle, deadline, idx, pc)` and diffing step vs batched showed the
systimer's TARGET0 alarm (`idx=22 name=systimer tok=0`) firing **on the
correct, identical schedule** in step mode (one event per cycle, `now`
incrementing by exactly 1 each time, matching `deadline` exactly) but
**telescoped/delayed in batched mode**: the alarm scheduled for
`deadline=14512815` was not actually delivered until `now=14513619` — 804
cycles late — after which several backlogged re-arms drained in the same
instant.

Instrumenting `Machine::plan_cpu_window` (`crates/core/src/machine/plan.rs`)
confirmed exactly why: the **coalesced-dual-idle window clamp** (the
`else if secondary_parked { clamp!(count, binder, clause::SECONDARY_PARKED,
1024); }` arm, taken whenever the APP core is WAITI-parked — the common case
for most of this firmware's runtime) clamps the batch window to a flat 1024
cycles and is **not** gated by `self.sched.next_event_deadline()` the way the
non-parked path is (see the `if count > 1 && !secondary_parked { … }` block
immediately below it, which applies the `SCHEDULER_DEADLINE` clamp only when
`secondary_parked` is false). Captured trace, cycles annotated:

```
PLAN now=14512595 secondary_parked=true count_before_sched_clamp=1024 sched_deadline=Some(14512815)
PLAN now=14513619 secondary_parked=true count_before_sched_clamp=1024 sched_deadline=Some(14513619)
```

The window starting at 14,512,595 had a known pending scheduler deadline
854 cycles ahead of the window minimum but ran the full 1024-cycle quantum
anyway, overshooting the deadline by 804 cycles before the event was
drained. In step mode (quantum 1 throughout) every alarm re-arm is observed
and re-scheduled at the exact cycle; in batched mode, whenever the APP core
is idle-parked, a scheduler-driven peripheral's interrupt can be delivered
up to ~1023 cycles late. This is an **interrupt-delivery timing gap in the
CPU batch-window planner**, not a peripheral register staleness issue — it
lives in `crates/core/src/machine/plan.rs`'s `secondary_parked` arm, i.e.
squarely in "the loop driving execution," which this task's scope
explicitly excludes fixing (`Do NOT special-case the batched loop… The fix
belongs in the peripheral's read-time logic, not in the loop driving
execution`). Fixing it correctly means adding the same
`sched.next_event_deadline()` clamp to the `secondary_parked` arm that the
non-parked arm already has — a `machine::plan` change, not a peripheral
change, and out of scope for this PR per the task brief. All debug
instrumentation used to find this (in `systimer.rs`, `timer_group.rs`,
`lib.rs`, `plan.rs`) was reverted; no functional changes are included in
this update. **PR left in draft**; see PR body for the recommended
follow-up.
