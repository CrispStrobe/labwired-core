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
must have no side effects. Execution and exact-artifact A/B are pending.

No hardware capture, acknowledgement digest, performance floor or deployed
pin changes. Newly changed source remains subject to the content drift gate.
