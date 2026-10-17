# AVR instruction-cost layout investigation (C9)

Status: diagnostic source candidate; **no new hosted result yet**.

The owner-selected C9 task reports a change in AVR step cost from 922.6 to
987.7 Ir/step after the upstream sync, and 1002.7 for a subsequently reverted
cold-helper attempt. These are historical author-reported observations until
their original receipts are independently admitted. They are not reproduced
measurements from this diagnostic.

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
