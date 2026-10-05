# Cortex-M opcode-directed fast-path dispatch experiment

Status: unqualified candidate; no deployed artifact or pin changes.
Exact base: `4d944d2d9ec320f7acd25b54cc41a50b850d6788`.
This is independent of the block-payload candidate in PR 142.

The controlled PR 142 WASM comparison demonstrated no meaningful gain. Fresh
candidate profiling on Node 20.20.2 / shared Skylake VPS still attributed 7,142
of 13,744 samples to Cortex-M `step_batch`, 1,315 to `run_t16_fast_block`, and
1,050 to `SystemBus::read_u32`. Profiling is diagnostic, not qualification.

This experiment targets repeated admission work: one tagged, width-checked
cached opcode selects relevant self-branch, countdown, stack-store and RAM-loop
probes, in their original order. Every selected executor still checks the full
loop and live registers/addresses. General block discovery remains the final
probe. Cache reads borrow entries rather than materialize complete instructions.
Cold, collided and Thumb-32 entries decline to ordinary execution; no guest
instruction or peripheral observation is omitted. Decode-cache-off continues
to take the unchanged interpreter fallback.

Observer, trace, logic tap, IRQ, debug, IT-state and scheduler guards remain at
the caller. Cycle advancement and memory/MMIO accounting remain unchanged.
No stable-MMIO coalescer, peripheral promise, guest edit or timer shortcut is
introduced. A cached block whose current decode entry was evicted may now
decline to ordinary execution: this is a performance choice, not a stale-code
assumption or correctness shortcut.

New tests compare dispatch against the frozen original probe order at rotated
entries and varied budgets, including RAM/MMIO addresses, full CPU snapshots,
RAM contents and access counters. Cold/tag/width rejection is side-effect-free;
all 65,536 halfwords exercise the borrowed lookup and width/tag filter. Existing
single-step differential and scheduler/debug/IRQ/observer regressions remain
required; these dispatch tests do not replace them.

Qualification required before promotion: all Cortex-M tests, feature-off and
scheduler-observable CI, board/model checks, native CorePerf without waivers,
exact-base native A/B/B/A, deterministic fresh WASM builds, integration and
the unchanged functional motion guest. Controlled hosted WASM A/B/B/A must
retain all windows, including failures. Artifact promotion additionally requires
all five motion windows >=1x and separate browser/debugger acceptance. Hardware
capture drift is reported, not silently refreshed or waived.

No gain is claimed before measurements. Failed candidates must retain their
receipts and cannot be called real-time-qualified or silently replace pins.
