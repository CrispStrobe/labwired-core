# SAM SPI idle-controller repair: source-bound native receipts

Bounded repair [PR159](https://github.com/CrispStrobe/labwired-core/pull/159)
merged as `ae127c89b9f60ed8a239f82858859c31ef8255af` from tested source
`547da6da814afd0375199b192db7e3e2416042a0`. Both trees are
`1a0b300a6b8b6e3cadfba27741e868c67a76ffd6`.
This does not move a consumer pin or qualify WASM, active frames or native Arcade.

## Correctness

[Rust Core CI37589460868](https://github.com/CrispStrobe/labwired-core/actions/runs/37589460868)
and every other enabled PR check passed: 20 enabled checks SUCCESS, four declared
full/warm/image jobs skipped. All three workspace shards completed. Feature-off
ran 4,111 library tests (zero failed, three ignored) and all 11 SPI integration
tests (zero failed/ignored). Authored guest/forced-walk comparisons cover
intervals 1/64/1024; both live clock gates freeze countdowns without early or
duplicate completion. Reset cancellation retires stale wakes, including after
clock removal. Downcast/feature-condition ceilings were not increased.

## Full native fleet: keep both outcomes

Both runs used the identical tested source, Ubuntu 24.04, Rust 1.95.0 and
`event-scheduler`, the existing spin fixtures and unchanged baselines/floors.
These are native executables with the browser feature set, **not WASM execution**.
The rates below belong to each hosted runner; they are not paired speedups.

| Run | Absolute native spin floor | PyBadge RTx median / minimum | Relative cost |
| --- | --- | --- | --- |
| [37589477850](https://github.com/CrispStrobe/labwired-core/actions/runs/37589477850) | All 42 chips pass | 61.4713x / 61.3440x | FAIL: F411 batch 1.9 vs baseline 1.4 Ir/step (+36.0%) |
| [37590764078](https://github.com/CrispStrobe/labwired-core/actions/runs/37590764078) | All 42 chips pass | 84.1056x / 80.9334x | PASS: all 82 board/modes, F411 batch 1.6 Ir/step |

PyBadge mean batch width is 1023.8 in both runs, versus the pre-repair main's
0.05x / batch 1 in
[37481145682](https://github.com/CrispStrobe/labwired-core/actions/runs/37481145682).
No tolerance, baseline, fixture, capture, acknowledgement or consumer pin changed.
The first F411 failure remains a real recorded gate failure; the repeat does
not erase it. Its causal explanation remains an open measurement-stability task
in [P0](../../engineering/target-next-lanes.md#p0--restore-idle-controller-batching-landed-stability-follow-up-open).

The JSON files are byte-preserved original artifact members, renamed only:

- [First full artifact 11469137787](https://github.com/CrispStrobe/labwired-core/actions/runs/37589477850/artifacts/11469137787):
  [RTx](full-first-rtx-status.json), [relative cost](full-first-perf-status.json).
- [Repeat artifact 11468892870](https://github.com/CrispStrobe/labwired-core/actions/runs/37590764078/artifacts/11468892870):
  [RTx](full-repeat-rtx-status.json), [relative cost](full-repeat-perf-status.json).

## Valid isolated-build comparison

[Diagnostic37591791573](https://github.com/CrispStrobe/labwired-core/actions/runs/37591791573),
[artifact11469528749](https://github.com/CrispStrobe/labwired-core/actions/runs/37591791573/artifacts/11469528749),
used one runner, two separate target directories and two repeats per source.
Base was `67920fcf8d82c74f3cc63285181fe26db93cf720`, executable SHA256
`e9b1006527e6d37f936bf85f0640aa5485740cabf7abc9ca31c8f47d7c07652b`.
Candidate was the tested source above, executable SHA256
`0ea8dd82a9936d8273e8ca1059b473793d08fcb82bb02c728029f9c589cd4d71`.
Order was base/base/candidate/candidate, not an alternating wall-time experiment.
It measures three selected batch-cost fixtures, not the whole fleet or active IO.

| Source | PyBadge Ir/step | F401 Ir/step | F411 Ir/step | Existing selected gate |
| --- | --- | --- | --- | --- |
| Base repeat 1 / 2 | 1829.5 / 1829.5 | 1.5 / 1.5 | 1.6 / 1.6 | FAIL / FAIL: PyBadge interval 1 |
| Candidate repeat 1 / 2 | 1.4 / 1.4 | 1.5 / 1.5 | 1.8 / 1.6 | PASS / PASS |

Preserved originals: [base 1](isolated-base-1.json), [base 2](isolated-base-2.json),
[candidate 1](isolated-candidate-1.json), [candidate 2](isolated-candidate-2.json).
No source-bound comparison is inferred from the invalid earlier
[37589818845](https://github.com/CrispStrobe/labwired-core/actions/runs/37589818845):
its shared Cargo directory reused the base binary for the candidate (0.20-second
candidate build, interval 1 retained). Its artifact and failure explanation
remain in the original run and PR discussion; collection SUCCESS is not a valid
performance result. The diagnostic workflow replacement was not merged.

## Automatic merged-main qualification

The automatic push run [37593590041](https://github.com/CrispStrobe/labwired-core/actions/runs/37593590041)
passed on merge `ae127c89b9f60ed8a239f82858859c31ef8255af`. All 42 absolute
chip-spin RTx checks and 82 matched relative board/mode checks passed, with no
regressions or contract failures. PyBadge median was **61.136537x**, minimum
**59.999848x**, maximum **61.397259x**, batch **1023.8**. PyBadge and F411
relative batch counts were both 1.4. Two stale-baseline advisories (EFR32MG26
step and nRF51822 step) remain in the original data; no baseline was updated.
The workflow automatically closed [issue158](https://github.com/CrispStrobe/labwired-core/issues/158).

Original [artifact11470453748](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11470453748/zip)
is 6,972 bytes, ZIP SHA256
`0e0c62558f2b5037c411a9f6c956f5358a3aefffda14bb6f987bb749520bceb7`.
The following files preserve the original JSON member bytes under descriptive
filenames; no runner log, normalization or baseline update is included:

- [Relative status](merged-main-perf-status.json): SHA256
  `040f38564d812fc0211cceef0324ef794f92dea7daa73bf39157292b4174e56d`.
- [Absolute RTx status](merged-main-rtx-status.json): SHA256
  `7f9a13b34201f6a230f1f8dcb695b1cd6a80ecb5790e0b17185be339fbddb4ea`.

This supplemental main pass does not erase the earlier F411 failure or establish
its cause. The runs are not a paired gain or active panel/WASM qualification.

## Boundary and next work

Idle SPI has no scheduled work. Active transfers retain conservative one-cycle
service; clock gates freeze work through the shared bus resolver. The complete
PyBadge manifest still contains a GPIO resident driving DATA through per-cycle
service, so its interval 1 is legitimate. Chip-only idle recovery must not be
advertised as full-board batching, active display speedup, silicon timing or
browser performance. Native display, split IRQ, DMAC and stock runtime proof
remain [P3, P2, P4 and P5](../../engineering/target-next-lanes.md).
