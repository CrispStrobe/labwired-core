# WASM-only register-helper inlining experiment

Exact base: core main `43b2d62f5a0fa24ae0b38a645069f5aaa78af685`.
This does not stack the unresolved scalar boolean-preflight experiment in PR 148.

The main-artifact hosted motion profile (bw-board run 36960285662) attributes
135,794 microseconds of whole-process self samples to `read_reg` and 59,890 to
`write_reg`, out of 7,318,567 sampled microseconds (about 2.67% combined).
Whole-process self samples include initialization and warmup; this share is not
removable cost or a predicted speedup. The profile only motivates an experiment.

Add WASM-only inline(always) attributes to the existing register helpers. No
register fields, bank rules, PC/SP/xPSR behavior, instruction execution,
memory accesses, scheduling, debugger gates, snapshots or exports change.
Native production attributes stay unchanged. The hypothesis is that eliminating
helper calls and allowing call-site register specialization beats the potential
larger generated functions and instruction-cache pressure. It may regress.

Before acceptance: exact-source independent deterministic WASM builds, all
actual integration tests, repeated ordinary original-artifact motion and F0
comparisons (including reverse order), and exact-head core checks. Keep every
failed realtime window failed. Do not publish artifacts, change app pins,
rewrite physical captures, or silently re-stamp drift acknowledgements.
