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
already validated 0 divergences over 40M Doom boundaries. Since it is a pure
perf win with its own byte-identity gate and no interaction with the CLI
loop's structure, we merge it in: it is exactly the win that makes a wide
batch interval on the S3 cheap per cycle rather than merely legal.

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
