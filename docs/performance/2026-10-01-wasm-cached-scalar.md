# WASM cached scalar experiment

Exact base: `4deee6f04266a73d0d2696cacfb2eac0b29aed84`.
This does not stack the negatively measured outlining experiment in PR 144.

Profiling finds the interpreter/batch loop, block execution and bus reads are
hot. Static block shape and discovery-miss caching already exist. Instead of
adding a duplicate cache, WASM may reuse the existing checked T16 block executor
to retire one decoded instruction when no loop fast path retires anything.

Native production dispatch remains unchanged. The scalar primitive is also
compiled in host unit tests for exhaustive reference comparison. It checks
cache tag and width, budget and sleep state, uses current register/RAM values,
retains the existing caller observer/debug/IRQ/IT/trace/tap and scheduler guards,
and declines unsupported, MMIO or unmapped operations to the interpreter.
No MMIO read is coalesced or cached. Existing per-retirement accounting stays
at the same caller boundary. No decoder/executor instruction logic is copied.

Added tests compare every halfword under three flag states with the ordinary
interpreter, architectural snapshots, RAM and access counters; declined cases
must have no side effects. Both new tests passed in the feature-off core suite;
all execution CI checks passed at `273e683e`, including three workspace shards,
native board qualification, browser layer and scheduler-observable checks.
Actual WASM build run 36867515370 passed determinism and all 101 existing
integration tests, zero skipped. Fresh qualification still failed the 1x floor:
0.768439x median / 0.752785x minimum on EPYC 7763.

Three independent exact-artifact hosted A/B/B/A runs observed median gains
of +6.94%, +1.23% and +4.85% (runs 36869551919, 36870103563, 36871226338).
Candidate medians were 0.924740x, 1.099840x and 0.928345x, respectively. The
middle run passed every window for both engines, but its candidate minimum
was worse than baseline; neither universal 1x nor every-window gains are claimed.
All cycle-indexed guest observations matched. Raw receipts are retained in
the bw-board companion report. Default traces show the helper reaches TurboFan;
sampling/forced-tier timings are diagnostic, not qualification.

The user explicitly approved updating the seven existing content-bound hardware
acknowledgements after verification. Their dates/expiry remain 2026-10-01 /
2026-10-31; capture dates/results/digests are unchanged and live re-capture is
still owed. No floor or deployed pin changes. Final-head CI remains required
before merge; this acknowledgement does not turn failed WASM RTx into a pass.
