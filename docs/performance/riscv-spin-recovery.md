# RISC-V spin-loop recovery

Recover the pure-Rust ADDI/back-branch batch idea from CrispStrobe/labwired-core
(`5d03bc55a5`, with the once-per-batch refinement in `315e6f5cb1`). The original
persistent promotion cache could outlive code stores and snapshot restoration.
It is deliberately not imported.

The implementation recognizes two instructions in the existing, permission-vetted
256-byte fetch window on each batch. It adds no persistent compiled-code cache.
Cold windows and blocks spanning a window boundary use the interpreter. Guest
stores already invalidate the fetch window; reset and both snapshot restore paths
must invalidate it too. This also repairs stale interpreted fetches after restore.

Only ADDI/C.ADDI with identical nonzero source/destination followed by JAL x0/C.J
back to the first instruction qualify. Retire whole iterations only. Preserve
register wrapping, mtime/mip, odd remaining budgets, scheduler deadlines and the
published pre-instruction clock. Disable aggregation with interrupts enabled,
external IRQ lines, waiting CPUs, cycle-accurate devices, observers or logic taps.
The existing native JIT retains its dispatch priority. Setting
`decode_cache_enabled=false` also opts out of loop aggregation.

Verification: compare against individual interpreter steps; exercise guest code
stores, reset, both snapshot formats, compressed negative increments, observable
execution, and scheduler deadlines. Measure a warmed loop against repeated steps;
report that synthetic measurement separately from real firmware performance.

Deferred fork work: broader Cortex-M caching, Xtensa execution/JIT changes,
persistent RISC-V promotion caches, and fork-wide performance baselines. None
is implied by this recovery. ARM DSP extensions are separate recovery work requiring
instruction-level semantic review. The nRF RF-medium delivery path was already
merged in upstream #800, including its path-loss/RSSI regression test.

## Local evidence

Rust 1.95.0, repository development profile (`opt-level=1`, debug assertions and
overflow checks enabled): all 37 RISC-V unit tests in the initial recovery run
passed. Before the repair, reset and both snapshot-restore regressions each
produced 64 instead of 96, proving stale ADDI +1 execution after code became +2.

One synthetic warmed-loop measurement (640,000 guest instructions) took
470 microseconds aggregated versus 92.3 milliseconds using individual interpreter
steps with the same configuration and full final CPU-state equality. This is a
trivial arithmetic-loop measurement, not an end-to-end firmware throughput or
release-build claim; interrupts/observability and nonmatching code retain the
interpreter. No physical hardware was recaptured.
