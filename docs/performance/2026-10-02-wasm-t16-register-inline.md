# Restricted T16 register specialization experiment

Exact base: `43b2d62f5a0fa24ae0b38a645069f5aaa78af685` (core main).
This does not stack PR 148 or the whole-register inlining in PR 149.

PR 149 gained all three selected workload medians on three hosted Node 22
comparisons, but lost RAM/GPIO on both clean VPS comparisons. A hosted Node 20
reverse repeat also lost 22.03% RAM. The ordinary interpreter's `step_batch`
WASM body expanded from 77,390 to 217,959 bytes. Those results do not qualify
that variant for landing or justify updating content-bound drift acknowledgements.
Original evidence: bw-board PR 226, `docs/receipts/2026-10-02-wasm-register-inline/`.

This experiment specializes only register selectors 0..7 at call sites inside
the existing admitted `execute_t16_fast_op` executor. High, special and invalid
selectors delegate to unchanged ordinary helpers; the T16 high-register operand
helper preserves wrapping PC+4. Other cached paths and ordinary interpreter
call sites are untouched. Native production wrappers forward to original helpers.
Native unit tests deliberately compile and exercise the WASM selector bodies.
No ISA implementation, memory access, cycles, flags, discovery/admission gate,
debugger behavior, register layout, snapshot or public export changes.

The hypothesis is that fast-executor specialization retains useful gains without
the large ordinary-interpreter expansion. Neither reduced code size nor speed
is established yet. Independently inspect exact-source emitted bodies and run
all actual integration tests, differential instruction/budget tests, repeated
ordinary artifact A/B with reverse order on Node 20 and 22 and clean VPS checks.
Preserve negative medians and every failed realtime window. Existing hardware
captures, acknowledgement expiry, app pins and publication stay unchanged until
the appropriate independent gates pass. Do not treat diagnostic completion as
qualification or waive the every-window ≥1× checkpoint.
