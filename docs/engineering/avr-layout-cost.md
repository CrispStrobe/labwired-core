# AVR instruction-cost layout investigation (C9)

Status: diagnostic source **merged** in [PR248](https://github.com/CrispStrobe/labwired-core/pull/248)
as `5b470d61cef312aefcbb2bd3d311f4c03ffaff16`, after all 16 enabled exact-head
checks passed (six declared skips). The landed parents and tree match the
reviewed prediction. **Original intake, bounded native-code explanation and the no-implementation decision are complete.**

The owner-selected C9 task reports a change in AVR step cost from 922.6 to
987.7 Ir/step after the upstream sync, and 1002.7 for a subsequently reverted
cold-helper attempt. These are historical author-reported observations until
their original receipts are independently admitted. They are not reproduced
measurements from this diagnostic.

## Completed campaign — 2026-10-17 06:30 UTC

The existing [campaign 39001605287](https://github.com/CrispStrobe/labwired-core/actions/runs/39001605287),
attempt 1, completed successfully on controller
`2201716dbb8461629c73beaa2e2db0e0386b45fe` / `lane/c9-avr-layout-20261017`.
All four official archives and job logs were acquired once. The frozen independent
reader admitted every source and the complete cohort: exact event/source joins,
archive and inventory hashes, compiler/runner identity, common firmware/descriptor,
original command journals, exact retirement witnesses, all 32 raw profiles and
their instruction totals. No producer import, guest replay or repeated download
was used. **C9.1 is complete: reuse retained originals; do not download or dispatch again.**
The executable hash remains producer-declared because the executable itself is
not packaged; do not present it as an independently hashed retained binary.

### Native instruction costs

Step uses one low/high pair; batch reports all three pair slopes and their median.
All four builds used the same declared Rust 1.95.0 toolchain, runner image and
unchanged authored input/bounds. These are finite native spin results.

| Snapshot | Step Ir/step | Batch pair slopes | Batch median Ir/step |
| --- | ---: | --- | ---: |
| `before-sync` / `f1ca219d` | 913.558531 | 6.837941 / 6.837941 / 6.837991 | 6.837941 |
| `sync-merge` / `6e03dd12` | 987.746057 | 7.188549 / 7.188549 / 7.188549 | 7.188549 |
| `current` / `715e1272` | 1004.746098 | 7.223695 / 7.223745 / 7.223695 | 7.223695 |
| `upstream-source` / `49f656f5` | 995.746146 | 7.223695 / 7.223614 / 7.223695 | 7.223695 |

The new pre-sync result is not the historical author-reported 922.6 measurement;
this campaign does not relabel an earlier build or reproduce its exact receipt.

### Observed step-cost attribution

The following are exclusive function-cost slope differences, in Ir/step, from
the admitted original profiles. Group `Machine::advance` and `advance_inner`
when comparing them: outlining moves costs between those function names.
Original names and records remain available. The batch per-function medians
are separate observations and are not assumed to sum to the median total.

| Comparison | Total step difference | Main observed function differences |
| --- | ---: | --- |
| Pre-sync → sync | +74.187526 | `advance_inner` +412; `advance` −384 (combined +28); `advance_central_i2c_time` +42.187500; `run_firmware_avr` +5; `tick_peripherals_at_boundary` −1. Remaining functions sum to +0.000026. |
| Sync → current | +17.000041 | `advance` +425; `advance_inner` −412 (combined +13); `Avr::tick_io_timers` +9; `run_firmware_avr` −5; `__strncmp_avx2` +0.000041. |
| Retained upstream → sync | −8.000089 | `advance` −423; `advance_inner` +412 (combined −11); `run_firmware_avr` +5; `Avr as Cpu::step` −2. Remaining functions sum to −0.000089. |

AVR decoder/scheduler sources and release profiles are identical across the
four selected inputs. This locates observed cost in shared machine/time advance
and compiler-dependent function boundaries; it does **not** establish which
source or inlining decision causes the net change. In particular, do not treat
+412 and −384 as independent regressions or call the whole +74.187526 layout noise.
No optimization, baseline change, waiver, browser-throughput or physical-timing
conclusion follows from this table.

The separate normal [Core Perf run 39001582632](https://github.com/CrispStrobe/labwired-core/actions/runs/39001582632)
on merged source `5b470d61` reports 82 measured board/mode rows across 42 chips,
no regression or stale rows, and the existing unrelated RV1103 waiver. It reports
ATmega328P step **1004.7 Ir/step** against baseline **987.7** (+1.7%), and batch
**7.2** against **7.2** (+0.3% before display rounding). This is authentic
reported context: that gate discards raw profiles, so its instruction counters
were not independently reconstructed. Its different build is not the selected
`715e1272` campaign binary. It supplies no causal or browser-performance proof.

## Causal native-code review and implementation decision — 2026-10-19

Read-only analysis of the retained campaign's native disassembly and low/high
Callgrind instruction/jump records explains the following executed mechanisms.
The observations concern the actual profiled monomorphs, identified by their
recorded addresses; a same-named generic instantiation is not interchangeable.
No new build, guest execution, download or measurement was needed.

| Region | Source and native-code finding | Observed implication |
| --- | --- | --- |
| AVR timer dispatch | AVR, Timer1, decoder, scheduler and release-profile bytes are identical in all four inputs. The stopped-prescaler path runs 1,200,000 times in each high sample. Only `715e1272` has four extra entry pushes, four matching return pops and one register move on that path. | Its timer self-cost increases by exactly 9 Ir/step; the unchanged Timer2 tail remains 22 Ir/step. |
| Central I2C time | The method is byte-identical in all four inputs. Before sync its executed body is inline; the other three builds outline it. Both forms retain 1,200,000 division calls, 1,087,500 equal-time skips and 112,499 changed-time fan-outs in the high sample. | The outlined 42.1875 self-cost row is not 42 new semantic instructions per step. Relative to the inline 27, frame entry/return adds 15, two pointer spills add 0.1875, and the caller adds 2: combined overhead 17.1875 Ir/step. |
| Advance entry/setup | The executed prefixes contain 46, 77, 84 and 82 instructions in pre-sync, sync, task-current and retained-upstream order. Sync eagerly materializes more machine-field addresses with address calculations and stack stores. | The pre-sync-to-sync prefix contributes 31 Ir/step. The sync-to-current prefix contributes 7, including five facade guards moved from the CLI caller. |
| Remaining advance body | Instruction/debug-region accounting retains offsetting additions, removals and moved attribution. One unchanged boundary-dispatch region grows from a conditional branch to an AND, two stores and the branch. | This identifies another 3 executed instructions within the sync-to-current residual 6; it does not uniquely assign every net difference to a source expression. |

For the timer witness, `715e1272` executes the extra pushes at
`0x2227310–0x222731a`, the extra pops at `0x22276d6–0x22276dc`, and a register
move at `0x22273ae`. Its active Timer1 path is inlined, enlarging the frame even
though that active path is not taken by this spin guest; the sync build calls
`apply_ticks` out of line. The exact unrelated source perturbation that caused
LLVM to choose that layout is **not established**. Identical timer source does
not imply identical executed native code.

The pre-sync-to-sync combined advance/central-clock ledger is
`31 - 27 + 2 + 22 + 42.1875 = 70.1875` Ir/step. The CLI adds five and the
boundary region removes one, giving 74.1875 apart from the small fixed-startup
term. The 22 residual advance instructions are accounted for in the retained
PC totals but are not individually assigned to source expressions. For sync
to task-current, advance adds 13 (seven in entry/setup and six elsewhere),
the CLI loses five moved guards, and the timer adds nine. The remaining net
three instructions within that six-instruction body residual likewise have no
unique source-expression attribution. Raw counters and disassembly establish
executed mechanisms; they do not supply missing compiler optimization records.

The central-clock calls must remain: they service the selected opt-in parked
I2C/SPI controllers and the 16 MHz time conversion. Removing them as unused
would change semantics. The exact call counts also refute attributing the
outlined self-cost increase to new clock fan-out work.

**C9.3 decision: do not add another runtime optimization.** Earlier broad inlining/cold-helper attempts have not supplied a qualified
improvement across both prescribed source families. The old task-current timer frame is also no longer the current-main
shape: static inspection of the already retained issue-251 fixed candidate
shows the compact frame and out-of-line Timer1 call. That supplementary static
observation has no new raw Callgrind counts and is not a fifth source in this
campaign. The separately qualified [issue-251 correction](https://github.com/CrispStrobe/labwired-core/pull/252)
reports AVR step 995.7 in its unchanged full-baseline campaign; matching rounded
totals alone would not establish causal identity with an older build.

The current accepted gate has no throughput regressions across its unchanged
82 board/mode rows. Retain its existing tolerances: there is no evidence for an
AVR waiver, wider baseline or skipped clocks. An optimization against obsolete
`715e1272` would not demonstrate an improvement on current main. Any future
proposal needs a precise current-source mechanism, separately claimed files,
affected actual guests, both prescribed source families, every unchanged
baseline row and all enabled exact-head checks. Compiler-trigger attribution
would need separately scoped optimization records or controlled compilation;
it is an explicit remaining diagnostic limit, not a claimed result.

<a id="pending-tasks-and-acceptance"></a>

## Completion and remaining boundaries

C9.1's original/cohort admission, C9.2's bounded executed-code explanation and
C9.3's no-implementation decision are recorded above. This closes the current
investigation with the residual source-expression/compiler-trigger limits
preserved. It does not claim a complete compiler-decision proof, browser
throughput, physical timing or a new runtime improvement. Retain and reuse the
four original packets and failed candidates; do not redispatch, download again
or replay a guest to reread them. Engine release and installed-consumer adoption
remain separately scoped work with explicit pins and affected qualification.

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
