# WASM-only interpreter outlining experiment

Exact base: `4deee6f04266a73d0d2696cacfb2eac0b29aed84`.

The previous artifact's CortexM::step_batch body was 77,465 wire bytes, the
largest function in that WASM. Both step_internal and step_execute were forced
inline and had no separate named bodies. Size alone does not establish a
performance bottleneck or promise a speedup.

Experiment: keep step_internal out of line only on wasm32. Its interpreter and
fault fallback remain unchanged; native retains inline(always), and
step_execute keeps its existing attribute. No instruction, bus, guest-cycle,
observer, debugger, IRQ, scheduler or runtime validation semantics change.

Qualification is pending: compare exact-base original artifacts using the
unchanged motion harness, retain every failed realtime window, and gather
separate sampling/tiering diagnostics rather than treating profiled timings as
normal qualification. Retain native/feature-off/differential and debugger
checks. Do not promote artifacts or deployed pins without the existing gates.

Existing content-bound hardware acknowledgements do not silently cover this
new source content. No capture dates/digests or acknowledgement digests are
changed by this experiment; drift must remain visible pending a new review.
