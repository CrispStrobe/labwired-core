# Experimental scalar WASM edge preflight

Start from landed `43b2d62f5a0fa24ae0b38a645069f5aaa78af685`, not the
cache-wrapper experiment. That experiment improved hosted F0 GPIO but its
ordinary VPS motion comparisons were negative; it is not accepted for landing.

Return a live boolean from the resident trait's preflight query, rather than
returning a slice only to inspect its length across the WASM virtual-call ABI.
The default helper calls the existing getter once and can inline its concrete
implementation before crossing that boundary. No model-specific override or
opt-in is needed. This is a code-generation hypothesis, not yet a measured gain.

There is no cache or collection wrapper: the public Vec and native production
scan policy stay unchanged. Interior metadata changes remain visible, getter
side effects are retained, and real edge-address routing/service remains
unchanged. No MMIO values, GPIO transitions, clock gates, IRQs, scheduling or
debugger guards are suppressed. Tests cover dynamic shared metadata and exact
getter query effects, plus a default-empty Button through dynamic dispatch.

Before acceptance: exact-source deterministic WASM, actual integration tests,
repeated ordinary motion and F0 comparisons on hosted and VPS Node, and all
final enabled core checks. Keep failed realtime windows failed; do not publish
artifacts, update app pins or rewrite hardware captures/acknowledgements.
