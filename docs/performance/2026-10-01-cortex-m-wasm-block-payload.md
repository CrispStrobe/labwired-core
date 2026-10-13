# Cortex-M WASM block payload experiment

Status: candidate only; qualification pending. No shipped engine pin changes.

Base: `4d944d2d9ec320f7acd25b54cc41a50b850d6788` (current fork main).
The unchanged selected-motion guest and fresh NODEJS engine were profiled on
2026-10-01 with Node 20.20.2 on the shared Skylake VPS. The original WASM SHA256
is `105966355233d96e813b745874730894d992ce4f0b8e6522ce189707e41cf9c9`.
This is sampled CPU evidence, not a throughput qualification. Of 12,339 samples,
6,498 hit Cortex-M `step_batch` (aggregating its two stack nodes), 1,231 hit
`run_t16_fast_block`, and 866 hit `SystemBus::read_u32`.

The separate Binaryen A/B/B/A experiment in bw-board PR #187 retained all twenty
failed RTx windows. Its approximately 0.56% median difference on the shared VPS
did not justify a speedup claim or artifact promotion. This candidate therefore
targets a concrete executor cost instead of promoting that binary transform.

## Change and safety boundary

`run_t16_fast_block` previously copied the complete 16-instruction block from
the positive cache into a local value on every cache-hit invocation. The
candidate keeps that payload in its cache slot, copies start/length scalars,
and copies only the current instruction before mutating CPU state.

Discovery, decode-generation invalidation, observer/debug/IRQ/IT admission,
scheduler budgets, execution order, memory accounting and MMIO fallback are
unchanged. An execution rejection still clears the positive block; it is not
memoized as a permanent negative discovery result. There is no unsafe aliasing,
guest-loop substitution, skipped peripheral work or relaxed real-time floor.

The new regression exercises a full-capacity block at every rotated entry,
resumes it through several budgets, and compares registers, flags and PC with
individual interpreter steps through the final not-taken branch. Existing
discovery tests cover MMIO rejection followed by RAM acceptance, cache collisions,
generation changes, code edits, cold misses and snapshot/restore invalidation.

## Required evidence before landing

- Cortex-M and discovery regressions, including event-scheduler builds.
- Native board/model and all-chip Core Perf gates, without waivers.
- Same-runner native A/B against the exact `4d944d2d` baseline.
- Two deterministic fresh WASM builds, existing integration, and unchanged
  selected-motion functionality plus all five >=1x windows.
- Controlled NODEJS baseline/candidate comparison; browser/debugger acceptance
  remains separate from NODEJS measurement and from package/artifact promotion.

No performance gain is claimed until candidate measurements exist. A green
native gate alone cannot qualify the WASM artifact; the currently retained
hosted motion WASM result remains below 1x.
