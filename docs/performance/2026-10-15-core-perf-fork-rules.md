# Core Perf in the fork: declared-only chips, sync PRs, an open AVR question

This note follows [2026-09-26 all-chip real time](2026-09-26-all-chip-realtime.md). That note said "no coverage
waivers". Three fork changes since then alter the rules.

## Declared-only chips are reported, not refused (fork #215)
`board_rtx.py` refused to run while any chip carried a waiver. The 2026-10-12 upstream sync added `rv1103`, a
generic descriptor with nothing to execute, so the gate went red on main.

`declared_only(chip)` now marks a chip as unmeasurable when it has:
- no peripherals;
- flash size 0;
- no perf fixture for its memory map.

Such a chip is listed under `not_measured` in the status JSON and printed as "not measured (declared only…)".

A waiver on any **runnable** chip is still a refusal, because a waiver in `board_perf.WAIVED` wins over a matching
fixture and could hide a chip the gate is able to run. `test_board_rtx.py` covers both sides.

## Upstream-sync PRs run Core Perf before they merge (fork #219)
`core-perf.yml` has a `pull_request` trigger. A job-level `if` limits it to this repository and to `merge/upstream-*`
head branches. Without it, a sync merged with every PR check green and turned main red here, with its Ir/step
regressions hidden for a day. Other PRs still run the lane by hand with `workflow_dispatch`. Fork #221 was the
first sync PR to run it.

## Re-locked baselines
Each baseline below was re-locked from CI numbers, never a local run.

| PR | board | mode | Ir/step |
|---|---|---|---|
| #215 | atmega328p | step | 922.6 |
| #215 | pybadge | step | 871.3 |
| #215 | efr32mg26 | step | 1480.7 |
| #215 | nrf51822 | step | 1298.7 |
| #221 | atmega328p | step | 987.7 |
| #221 | atmega328p | batch | 7.2 |

## Open: AVR step cost moves with code layout
#221 merged four upstream commits:
- the opt-in Cortex-A7/tlib backend;
- RV1103;
- interrupt and metadata ordering;
- snapshot placeholders.

None of them touches the AVR core, yet `atmega328p` step rose 922.6 → 987.7 Ir/step (+7 %). A cold-path helper that
lowered the cost on upstream's tree raised it on the fork's (1002.7), so it was reverted and the baseline re-locked.

**Next work (unclaimed):**
1. Compare the fork head with its pre-sync parent per function, under the same callgrind setup as CI
   (`cg_annotate`, jump collection).
2. Check the inlining and codegen of the AVR decode/execute path: `#[inline]`/`#[cold]`, codegen units and LTO in the
   bench profile.
3. Try one change at a time on **both** trees.
4. Keep a change only if it lowers AVR Ir/step on both trees and every other `baselines.json` row stays within
   tolerance. Otherwise record here why layout noise of this size is accepted.

A per-chip waiver is not an option: runnable chips refuse waivers.
