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

## 2026-09-18 follow-up: root-causing and fixing the 40M-step divergence

### Where the 40M-step divergence actually came from

The initial hypothesis — a scheduler-driven S3 timer returning a stale
free-running counter on a mid-window read — was checked first and was
**wrong**: `Systimer` already catches its counter up from the bus-published
`CycleClock` on every `UNITx_OP` latch, and tracing every `UNIT0_OP` write
showed step and batched byte-identical reads for the first ~1,470 latches. A
matching catch-up added to the TIMG `TxUPDATE` latch changed nothing (same
40M-step hash with and without it) and was dropped. `Machine::plan_cpu_window`,
the scheduler drain, the S3 interrupt-matrix routing, the Xtensa
`dispatch_irq`/WAITI park points and the systimer latch were instrumented
instead and the two runs diffed at the first disagreement, three times over —
each fix exposed the next, smaller one. Four independent defects, all on the
batched side (the per-instruction path's output never moved):

1. **Coalesced dual-idle windows ignored scheduler deadlines.** With the APP
   core WAITI-parked, `plan_cpu_window` clamped the window to a flat 1024 and
   the `next_event_deadline()` clamp was gated on `!secondary_parked`. The
   SYSTIMER TARGET0 alarm due at cycle 14,512,815 was drained at 14,513,619
   (804 cycles late). Fix: the scheduler-deadline clamp applies on both
   arms (`crates/core/src/machine/plan.rs`).
2. **The parked core's own CCOMPARE0 edge was not a window boundary.** The
   parked path fast-forwards APP's CCOUNT over the window and raises timer-0
   only at its end. Fix: `Cpu::parked_wake_deadline_cycles` (Xtensa:
   cycles until CCOUNT reaches CCOMPARE0, `None` elsewhere) and a
   `SECONDARY_WAKE_DEADLINE` clamp. It binds nothing on this firmware (APP's
   CCOMPARE0 is 0 here) but closes the same class of hole.
3. **The bus clock was published one cycle apart on the two paths.**
   `SingleDirect`/`RunDual` charged the cycle first and published
   `total_cycles + 1`; `RunBatch` published `total_cycles` and let
   `step_batch` republish `+1` per retired instruction (`batch_start + i`,
   the convention #842 documented and every walk/scheduler differential
   pins). A lazily-clocked read under `Machine::step` was therefore one
   cycle AHEAD of the identical read under a batched `advance`; the S3
   SYSTIMER divides the CPU clock by 15, so one read in ~15 flipped a tick
   (`976897` vs `976896` at cycle 14,653,455). Fix: the quantum-1 arms
   publish BEFORE charging, like `RunBatch` (`boundary.rs`). Two unit tests
   that pinned `bus.current_cycle == 1` after a failed single step now pin
   `0`; nothing else in `--lib` (both feature sets) moved, and the step-path
   CLI hashes are identical before and after.
4. **The RMT counted `tick_with_bus` calls as cycles.** The 2,000-cycle
   post-TX_START IRQ holdoff and the WS2812 pad playback both advanced by
   one per call; the bus-tick pass runs once per *boundary*, so on the
   batched path the RGB-LED TX-done interrupt that wakes the PRO core out of
   WAITI landed 94,928 cycles late instead of 412 (source 40, `S3_ROUTE`
   trace) — that is the `I (271)` vs `I (280)` esp_log timestamp. The
   `Peripheral::idle_poll_bus_tick` docs already state the contract this
   broke ("key their internal cadence on device cycles — NOT on
   tick_with_bus call count"). Fix (`peripherals/esp32s3/rmt.rs`): holdoff
   and playback advance by cycles elapsed since the last bus tick (read from
   the attached `CycleClock`; one per call when the clock does not move, so
   hand-built test buses and the forced-walk oracle are unchanged), and the
   holdoff expiry is armed as a scheduler event so the window ends on it and
   the level is delivered on the same cycle the per-cycle tick delivers it.

### Identity evidence after the fix

Same fixture, flash and flags as above (`--rom-boot --allow-sim-error`,
CLI built with `--features jit-core,event-scheduler`):

| Run | stdout sha256 | bus-trace sha256 |
|---|---|---|
| step @ 5M | `c28194d680de69cd9d7d5546…` | `ff44fe168fcb5b0888bebcd1…` |
| batched @ 5M | `c28194d680de69cd9d7d5546…` (match) | `ff44fe168fcb5b0888bebcd1…` (match) |
| step @ 40M | `f63cd2557c5d3134d485e683…` | `ff44fe168fcb5b0888bebcd1…` |
| batched @ 40M | `f63cd2557c5d3134d485e683…` (**match**) | `ff44fe168fcb5b0888bebcd1…` (match) |

The step-path hashes are the ones the draft PR reported before any change.
Step-vs-batched at 5M steps on `tests/fixtures/tier1/{esp32c3,stm32l476,nrf52840}.elf`
is byte-identical as well (stdout and bus trace), so the shared planner and
clock changes did not move the single-core paths.

### Ir/step after the fix (callgrind, `--cache-sim=no --branch-sim=no`)

Baseline = this branch merged with `origin/main` at `ddeee6a89`, measured in
the same build (`jit-core,event-scheduler`) on the same machine.

| Steps | Path | Baseline Ir/step | Fixed Ir/step | Delta |
|---|---|---|---|---|
| 4M | step | 1114.7 | 1115.7 | +0.09% |
| 4M | batched | 841.8 | 844.9 | +0.37% |
| 12M | step | 1221.0 | 1221.8 | +0.07% |
| 12M | batched | 948.0 | 950.7 | +0.28% |

Batched stays 24–22% below step. `python3 scripts/perf/board_perf.py`
(CLI built with `--features event-scheduler`, the configuration
`core-perf.yml` uses): all 52 baselined board-modes within ±0.4% of
`baselines.json`, no REGRESSION, no stale baseline, exit 0; `esp32s3` /
`esp32s3-zero` / `esp32` step rows report `(new)` — no baseline exists for
them yet. (A `jit-core` build reports every Cortex-M `batch` row 2–3x above
baseline on the unmodified tree too — the baselines are recorded without the
JIT, per the workflow.)

### Gates

- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`
- `cargo test -p labwired-core --lib` (3547) and `--features event-scheduler` (3677)
- `cargo test -p labwired-cli` — every target passes except `demo_blinky`,
  which needs the `demo-blinky` firmware built first (fresh worktree)
- S3: `esp32s3_doom_frame_oracle`, `esp32s3_irq_cache_differential`,
  `esp32s3_walk_differential`, `esp32s3_ws2812_rmt`, `led_strip_migration_parity`
- Other walk/scheduler differentials: `esp32c3_walk_differential`,
  `stm32_timer_walk_differential`, `nrf52_timer_walk_differential`,
  `stm32l073_walk_differential`, `stm32_dma_walk_differential`,
  `nrf52_easydma_tick512_fidelity`, `board_batch_width`, `event_scheduler`
- `scripts/generate_validation_status.py --check --drift`: esp32s3 drifted
  on `rmt.rs` + `xtensa_lx7.rs`; acked 2026-09-18 in `validation/manifest.yaml`
  (ACK, not a capture — no register or reset value moved).
