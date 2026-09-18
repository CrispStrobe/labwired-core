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
