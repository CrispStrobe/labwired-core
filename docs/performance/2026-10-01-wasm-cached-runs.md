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
Execution tests and exact-artifact A/B are pending; no speedup is claimed yet.

No hardware acknowledgements/capture evidence, floor or app pins changed.
Content-bound drift gates remain applicable. Do not merge a regression or treat
a paired median gain as every-window realtime qualification.
