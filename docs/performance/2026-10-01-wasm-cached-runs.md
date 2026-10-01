# WASM bounded cached-run experiment

Exact baseline: core main `c05e8de37f8837148c3066c5e5f1d715a6dbd0f8`.
Extend its verified scalar fallback to at most 16 cached T16 retirements,
also bounded by the caller's scheduler budget. Each retirement validates the
live PC/tag/width; supported ALU/branch/RAM execution reuses the existing checked
executor. Stop before cold/collided/T32/unsupported/MMIO/unmapped entries.
The caller retains debug/observer/tap/trace/IRQ/IT exclusion and aggregate cycle
accounting. Native production dispatch is unchanged.

The original all-halfword scalar differential now exercises this same primitive
with budget one. New mixed-run tests cover all budgets 0–33 plus large limits,
live RAM, taken/fall-through branches, bounded cycles and decline barriers.
The rebuilt full event-scheduler library suite at `2f5d9355` passed 4,234 tests,
zero failed, with three existing ignored tests. Twelve targeted tests passed
with zero ignored at `2dcf61b5`. A test-only mid-file wrapper initially confused
the existing source audit; removing the wrapper fixed it without changing the
audit, allowlist or production dispatch.

Two ordinary exact-artifact A/B/B/A comparisons against the landed scalar
baseline measured +8.24% (EPYC 9V45) and +7.15% (EPYC 7763) pooled median gains.
All cycle-indexed guest observations matched. The first host passed every
window; the repeat failed every window. These are same-host comparisons, not
universal realtime qualification. The measured candidate source is `8562db36`;
later changes only strengthen tests/remove a test-only wrapper. The first
fixed-head rebuild has identical executable WASM sections and candidate glue,
but 26 data bytes differ, consistent with shifted logging/source locations.
It is not byte-identical to the measured module. Both independent fixed-head
builds succeeded; local byte comparison confirms their NODEJS/web modules and
glue agree exactly (WASM SHA256
`7bd66fe4e926fbf14322621499f3fbddefefae763f61742c4c8c7312113b7a3d`).
Hosted determinism, fresh runtime qualification, an exact-new-artifact A/B and
final-head CI remain pending. Each engine in the
measured comparisons used its own original, different glue under the explicitly
enabled paired-glue policy.

The measured candidate's fresh build passed determinism and all 101 actual WASM
integration tests with zero skips, but the unchanged floor failed all five
windows: 0.979751x median / 0.951531x minimum. Publication was skipped.
Raw receipts and exact provenance are being retained in bw-board's
`docs/receipts/2026-10-01-wasm-cached-runs/`.
The README/receipt report and glue-provenance correction are merged through
bw-board PRs 202/203. The stored module comparison records every differing byte;
its source/log-location explanation is an inference, not blanket equivalence.

- Primary comparison: https://github.com/CrispStrobe/bw-board/actions/runs/36885715368
- Independent repeat: https://github.com/CrispStrobe/bw-board/actions/runs/36886507617
- Measured build/qualification: https://github.com/CrispStrobe/bw-board/actions/runs/36883581882
- Fixed-head fresh verification: https://github.com/CrispStrobe/bw-board/actions/runs/36888339538
- Exact fixed-artifact ordinary A/B: https://github.com/CrispStrobe/bw-board/actions/runs/36891599325

No hardware acknowledgements/capture evidence, floor or app pins changed.
Content-bound drift gates remain applicable. Do not merge a regression or treat
a paired median gain as every-window realtime qualification.
