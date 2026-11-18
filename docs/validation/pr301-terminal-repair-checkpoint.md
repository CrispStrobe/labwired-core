# PR301 terminal repair checkpoint

Snapshot: 2026-11-18 17:45 UTC. [PR301](https://github.com/CrispStrobe/labwired-core/pull/301)
is open and unmerged on `merge/upstream-20261116-next`. The latest implementation
commit is `253485416801474b021115c9d9f12b244c05e301`. This checkpoint adds
documentation only. Checks must be refreshed against the resulting PR head;
the implementation commit's results do not qualify a later head automatically.

## Implementation retained

- ARM run fuel is dispatched work plus skipped cycles, charged once. Total
  instruction work retains both dispatched and represented instructions.
- C3 timer/GPIO coalescing requires the supported decoded loop and a live proof
  of code, register, device, observer, interrupt and deadline conditions.
  Ordinary `JAL -12` loops retain wide batches. The counterexample starting at
  cycle 4 requires two windows, 1020 and 4 instructions, on the unchanged
  1024-cycle peripheral grid. Its architectural assertions remain intact.
- S3 coverage retains the original write probe, RO/W1C semantics and 3333 floor.
  Additional coverage requires real endpoint-event/FIFO behavioral witnesses
  and negative controls; it does not credit generic register storage.
- The current performance candidate shares the same advance loop and cleanup
  for report consumers and the single-step compatibility adapter. The latter
  avoids collecting a discarded per-call report. Public reports retain complete
  checked accounting; live profiles, fuel, boundary effects and overflow checks
  remain enabled. No chip feature, dependency pin, performance baseline or gate
  threshold was removed or changed.

Relevant follow-up commits are `5f2a5a771` (Clippy fixture initialization),
`7c8fe141e` (supported C3 batch hints), `2c253db7f` (correct tick-grid control),
and `253485416` (discarded-report cost candidate and equivalence controls).

## Actual qualification evidence

At the snapshot, implementation head `253485416` has **33 successful checks,
zero failures, 11 conditional skips and one running check**. CoreCI and the
affected runtime controls have completed successfully:

- Default core library: 6144 passed, zero failed, three ignored. This includes
  both report-discard equivalence controls, profile-overflow atomicity, the
  corrected C3 counterexample and cache-on/cache-off shape controls.
- Workspace shard 1: 1188 passed, 37 ignored, no new or known failures. ARM
  batched-path tests passed 8/8; register-coverage tests passed 3/3.
- All three workspace shards, feature-off tests and their aggregate verdicts,
  Cargo check, Clippy, browser and scheduler-observable checks succeeded.

Evidence: [normal CoreCI run](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080232),
[default-member log](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080232/job/134984416441),
[shard-1 log](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080232/job/134984416385),
and [source gate log](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080232/job/134984416519).

The [normal full CorePerf run](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080361)
has passed its RTx/RTF step. Its [board-throughput job](https://github.com/CrispStrobe/labwired-core/actions/runs/44356080361/job/134984667697)
is still measuring Ir/step. No terminal full report or successful dependent
perf verdict is available. Runtime equivalence does not establish a performance
repair or identify the first historical offending commit.

## Failures that must remain visible

Head `5f2a5a771` failed C3 workload RTF: index 0.0009058 versus baseline
0.001212, at the unchanged 25% tolerance. Its Ir/step step was skipped.

Head `7c8fe141e` failed the newly added C3 batch-count expectation of one;
the actual two batches followed the mandatory tick grid. Commit `2c253db7f`
corrected that control, and subsequent exact-head tests passed it.

The [full CorePerf run for `2c253db7f`](https://github.com/CrispStrobe/labwired-core/actions/runs/44332136292)
passed RTx/RTF but failed ten step-mode measurements. Its dependent perf verdict
also failed. These remain unclosed until the candidate's complete full gate
passes, including every other board and mode.

| Board | Baseline Ir/step | Measured Ir/step |
| --- | ---: | ---: |
| atmega328p | 1044.8 | 1080.0 |
| atmega328pb | 1026.4 | 1080.0 |
| atmega32u4 | 1020.0 | 1080.0 |
| esp32 | 955.5 | 1033.0 |
| esp32-devkitc-32e-gpio-smoke | 977.8 | 1033.0 |
| esp32-devkitc-32e-uart1 | 977.8 | 1033.0 |
| esp32s3 | 991.9 | 1048.7 |
| esp32s3-devkitc-1-n16r8-adc | 991.9 | 1048.7 |
| esp32s3-zero | 991.9 | 1048.7 |
| mimxrt595s | 892.0 | 921.8 |

The relative regression gate remains 3%; step mode has no absolute noise
allowance, and batch mode retains its 0.5 Ir/step noise floor. No batch regression
or scheduler-contract failure appeared in that report. C3 RTF index was 0.001188
against 0.001212. Its three Pico samples each retired 1,000,000,000 instructions
over 1,000,000,000 cycles and showed 79 meaningful frames. The Pico 0.5 floor is
**retired instructions divided by cycles**, not RTF. These are measurements of
that failed head and cannot qualify the current head or establish full green.

The earlier ESP32 955.5-to-1005.0 regression and its first causal offender also
remain unresolved. The shared report-cost change is a repair candidate, not a
measured causal closure. Original job logs and reports have been retained;
successful originals must not be downloaded or executed again just to read them.

## Blocker and continuation

The precise blocker is the unfinished full Ir/step board measurement and its
dependent verdict. No merge has occurred. Issue #300 remains open. Main was last
recorded red at `0aaf0bf57552e7c2457372b2ca3337885c3da744`; no fixed-main claim is made.

1. Refresh the PR's exact head and all enabled checks after this documentation
   push. Retain conditional skips explicitly; use no older-head pass credit.
2. At completion of the existing full perf job, admit its metadata, acquire its
   original log/report once, then read the originals independently. Preserve
   failure or supersession as observed. Do not dispatch a duplicate measurement.
3. Compare all ten regressions and all other boards/modes against unchanged
   baselines. Check RTF and actual Pico retirement above 0.5 with frame progress.
   If red, establish source/binary/workload-bound instruction or callgraph
   attribution before another minimal general repair. Do not rebaseline, waive
   gates, remove effects or retry the unchanged resource-failed local build.
4. Only the PR owner may normally merge with `--match-head-commit` after every
   enabled required exact-head check and the full CorePerf verdict succeed.
   Verify the actual merge parents, tree and resulting main qualification.
   Keep #300 open until fixed exact full CorePerf is green. Any later full
   upstream integration follows qualified PR301 and verified main.

Work is stopped at this checkpoint. No new experiments, model runs, heavy builds,
manual CI dispatches or background watchers were started for the handoff.

## Manual continuation — 2026-11-18 19:55 UTC

The owner resumed this checkpoint under direct user instruction. The full
`ae88ac6c0` gate failed twelve measurements: the ten historical step failures
became nine (MIMXRT595S passed at 885.8), while the three AVR batch measurements
also regressed. AVR step measured 1094.5, ESP32 1044.0 and ESP32-S3 1051.7;
AVR batch measured 8.4. These are failed results, not performance closure.

Diagnostic head `04e5783da47431bc231279fa2ffee4c68113de3c` retained original
Callgrind samples from the same full required gate. Its terminal checks are
32 success, three failure and eleven conditional skips. The failures are the
frozen producer guard, board throughput and its dependent verdict. The
[full report](https://github.com/CrispStrobe/labwired-core/actions/runs/44376788330/job/135058993854)
reproduces all twelve regressions exactly; there are no scheduler-contract
failures. The original artifact is `board-throughput-report`, ID 14465090675,
SHA-256 `5866966877f71b018cf0c42908c9de1d58498a82372ce61b5abd7e422d2f18a0`.

An independent reader verified its 24 sample manifests, seven content-addressed
assets and every retained output hash. Official `callgrind_annotate` readings
of the paired 200,000/1,200,000-step originals attribute 127 host instructions
per AVR step and 133 per Xtensa step to `advance_output` itself. Planning,
execution and boundary helpers remain outlined with dynamic request/mode
arguments despite the adapter's fixed single-step policy. This identifies a
current recurring cost; it does not establish the first historical regression
commit or qualify a repair.

The next source candidate inlines those four shared seams so fixed policies
can propagate through the existing lifecycle. It adds no alternate dispatcher
and changes no accounting, overflow checks, boundaries, reports or thresholds.
The diagnostic filter violated the frozen producer guard, so the candidate also
restores the held producer and its original tests byte-for-byte. Original
sample retention uses that producer's existing unfiltered interface; no guard
pin or assertion is changed. All 142 affected producer/diagnostic/raw-profile
host controls pass, and the Rust files pass formatting. These host checks do
not qualify runtime or performance.

The next normal source push must pass all exact-head runtime checks and the
complete unchanged performance gate. The twelve failed measurements remain
open until that happens. PR301 is unmerged and #300 remains open; no fixed Pico,
full-performance or main-green claim is made.
