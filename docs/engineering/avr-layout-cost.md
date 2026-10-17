# AVR instruction-cost layout investigation (C9)

Status: diagnostic source **merged** in [PR248](https://github.com/CrispStrobe/labwired-core/pull/248)
as `5b470d61cef312aefcbb2bd3d311f4c03ffaff16`, after all 16 enabled exact-head
checks passed (six declared skips). The landed parents and tree match the
reviewed prediction. **The four-source comparison remains pending.**

The owner-selected C9 task reports a change in AVR step cost from 922.6 to
987.7 Ir/step after the upstream sync, and 1002.7 for a subsequently reverted
cold-helper attempt. These are historical author-reported observations until
their original receipts are independently admitted. They are not reproduced
measurements from this diagnostic.

## Continuation checkpoint — 2026-10-17 06:19 UTC

Continue the **existing [campaign 39001605287](https://github.com/CrispStrobe/labwired-core/actions/runs/39001605287)**, attempt 1,
controller `2201716dbb8461629c73beaa2e2db0e0386b45fe` on
`lane/c9-avr-layout-20261017`. Do not dispatch another campaign or rerun a
successful original. At this checkpoint, producer controls, `before-sync` and
`sync-merge` succeeded; `current` was running and `upstream-source` was queued. Refresh
all children and the whole event before acting. These are job-status
observations, not an admitted four-source performance result.

The separate normal [Core Perf run 39001582632](https://github.com/CrispStrobe/labwired-core/actions/runs/39001582632)
on merged source `5b470d61` reports 82 measured board/mode rows across 42 chips,
no regression or stale rows, and the existing unrelated RV1103 waiver. It reports
ATmega328P step **1004.7 Ir/step** against baseline **987.7** (+1.7%), and batch
**7.2** against **7.2** (+0.3% before display rounding). This is authentic
reported context: that gate discards raw profiles, so its instruction counters
were not independently reconstructed. Its different build is not the selected
`715e1272` campaign binary. It supplies no causal or browser-performance proof.

## Pending tasks and acceptance

1. **C9.1 — admit the existing originals.** Wait for the entire event and all
   children to finish. Check retained-original and acquisition state first;
   reuse existing copies and the frozen standalone reader. Admit exact event,
   attempt, controller, source, job, artifact, digest and size before acquiring
   each original once. Artifacts have the workflow's 14-day retention. Keep
   failed/partial originals and refusals; do not retry downloads or replay a
   guest to repair missing evidence. If access to retained evidence is missing,
   obtain it before proceeding. A successful sibling cannot qualify a failed
   or incomplete cohort.
2. **C9.2 — explain the complete four-source result.** Independently bind all
   four snapshots, actual toolchain/runner versions, unchanged firmware and
   descriptor, build settings, exact low/high retirement witnesses and raw
   profile totals. Read originals without importing the producer or executing
   a guest. Report step slopes and all three batch pairs with their median;
   per-function medians need not sum to the median total. Compare pre-sync to
   sync, sync to current, and retained upstream to sync. Join changed function
   self-costs to native disassembly and exact source witnesses. Static review
   found identical AVR decoder/scheduler sources and release profiles; shared
   retirement/advance changes are candidates, not established causes. Publish
   source-bound CI numbers, limits and preserved failures here. If admission
   fails, document that refusal and scope a new diagnostic separately; never
   silently replace or rerun this campaign.
3. **C9.3 — decide whether an implementation is justified.** After C9.2, scope
   any optimization separately, naming exact files, source families and affected
   guest tests; coordinate shared-machine ownership before implementation.
   Work only in the owned fork. Require qualified hosted results on both
   prescribed source families and every unchanged baseline row, with all
   enabled exact-head checks passing. Preserve original failures, baselines,
   tolerances, fixtures and profiles. If no robust improvement is justified,
   explain the measured tradeoff and why the existing tolerance is acceptable;
   do not introduce an AVR waiver or label an unmeasured cause as layout noise.
   Engine release and consumer adoption would be separately scoped follow-ups,
   with explicit pins and affected guest qualification.

C9 is complete only when its CI-backed explanation and the resulting decision
are recorded. Producer delivery alone does not close the investigation.

## Fixed first campaign

The manual profiling job in `avr-layout-profile.yml` builds four exact source
snapshots from the owned fork: pre-sync `f1ca219d`, sync merge `6e03dd12`, current
task input `715e1272`, and already retained upstream source `49f656f5`. It uses
Ubuntu 24.04, Rust 1.95.0 and each snapshot's unchanged release profile, with
`event-scheduler` enabled. It records actual tool versions and the original
verbose build commands; no cache or compiler/profile override is introduced.
Its pure producer controls also run on matching pull requests; they execute
no compiler, profiler or guest. Actual profiling remains manual-only.

Every source must contain the same authored AVR spin program and chip descriptor
as the controller. The AVR firmware build command matches `board_perf.py`.
Each source runs the existing 200,000/1,200,000-step pair: once in step mode
and three times in batch mode. Batch retirement, width and tick-interval
witnesses remain required. Exact finite terminal witnesses and matching
stderr/profile instruction totals are also required.

The gate actually uses **Callgrind**, so this diagnostic uses
[`callgrind_annotate`](https://valgrind.org/docs/manual/cl-manual.html),
its matching function-summary reader, rather than changing the tool to fit
Cachegrind's `cg_annotate`. It retains full exclusive-cost annotations, raw
instruction/jump profiles, symbol addresses, native disassembly, build/input
identities and the original stdout/stderr. `--threshold=100` retains the full
ranking. Low/high differences separate fixed startup from per-step cost.
Raw jump records are retained for separate analysis; the command-line
annotation does not itself display instruction-level jump arrows.

The producer writes an inventory even after a caught failure. A failed or
partial snapshot remains a failure, and a successful sibling cannot qualify
the four-source comparison. Independent readers must verify originals and
source identities without importing the producer or replaying any guest.
The original intake and independent reader are frozen before dispatch.

## Boundaries

This is a diagnostic, not a change to AVR decoding, a performance fix or a
release. It neither modifies nor re-locks any baseline, tolerance, fixture,
profile, engine pin, model capture or ACK. Native authored-spin instruction
counts do not establish browser throughput, physical timing, full-board
behavior or results for the other baseline rows. A later proposed optimization
requires its own precise scope and unchanged qualification on both prescribed
source trees and every existing baseline row.
