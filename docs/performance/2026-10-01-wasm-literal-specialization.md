# Compile-time isolation of cached literal loads

This variant starts from experimental source
`7e1c2553c0ceace31cb390d31b629fc37b54990a`, not a landed optimization.
That source passed 4,236 local core tests (three existing ignores), both
independent WASM builds, determinism and 101 actual-WASM integration tests
with zero skips. Its selected fresh qualification passed on one Xeon 6973P-C:
1.585183x median / 1.580895x minimum. This is not an all-host/chip speed claim.

Two ordinary VPS comparisons against freshly rebuilt landed core `43b2d62f`
observed +0.19% and +5.69% pooled median changes. Shared-host drift was large;
neither establishes a reliable speedup. All cycle-indexed guest observations
matched, and every VPS window remained below 1x. The original candidate's
hosted comparisons are pending; preserve their actual source and module hash.

Inspection of exact WASM function bodies found the existing `run_t16_fast_block`
grew from 32,735 to 33,116 bytes, despite its unchanged admission rules excluding
literal loads. This is byte-size evidence, not proof of a timing regression;
raw-body hashes also contain relocated function/data references.

Use a compile-time `ALLOW_LITERAL` parameter on the same checked executor:
the existing fast block selects `false`, and only the WASM bounded cached run
selects `true`. No runtime policy branch, duplicate executor, mapping cache,
new snapshot field, widened admission or native production instruction is added.
A direct regression verifies the original executor declines literal loads with
unchanged CPU state and access counters. The literal/reference and exhaustive
halfword/flag-state tests still exercise the bounded variant.

This is a separate unverified candidate. Require fresh reference tests,
independent deterministic WASM builds, non-skipped integration and repeated
exact-artifact paired comparisons before selecting it. Re-inspect function
body sizes; do not assume specialization restored the old body or improved
performance. Publication, app pins, hardware captures and acknowledgement
expiry are unchanged. No hardware acknowledgement re-stamp is authorized here.
