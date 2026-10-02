# Fast-path occurrence census (diagnostic only)

Build `labwired-wasm` with the explicit `fastpath-census` feature to expose
`fastpath_census_reset()` and `fastpath_census_snapshot_json()`. The feature is
off by default in both core and WASM. It is not implied by any production
feature. Feature-off recording macros do not evaluate their arguments.

Reset **after** startup/warm-up and external input changes, execute a fixed
counted guest window, and snapshot **before** reading guest receipts. The
snapshot is non-destructive, thread-local, and aggregates all machines on that
thread. Do not interleave machines when interpreting it as a single-workload
receipt. Overflow saturates rather than wrapping.

These are occurrence counts, not time attribution. Instrumentation changes
code layout and adds overhead; never use this artifact to report RTx, compare
throughput against an uninstrumented artifact, or qualify an optimization.
Only unchanged, uninstrumented builds can supply acceptance timings.

- `FastCalls`, `FastZero`, `FastColdEntry`, `FastRetired`: guarded cached T16
  dispatch attempts, zero-progress results, cold/collided/wide entries, and
  total instructions retired by all successful fast paths. Early cold returns
  are included in `FastZero`.
- `BlockCalls`, `BlockZero`, `BlockRetired`: block attempts and outcomes.
  `BlockCacheHits`, `BlockMemoizedMisses`, `BlockAdmissionRejects`,
  `BlockCompileAttempts`, `BlockDiscoveryMisses`, `BlockDiscovered` separate
  cache reuse from discovery work. `BlockCompileAttempts` counts individual
  candidate starts, not calls. `BlockExecutionRejects` counts live executor
  barriers, which may occur after positive progress. Such dynamic failures
  **must not** be treated as static discovery misses: an address register can
  change from MMIO to RAM without a decode-generation change.
- `CachedRunCalls`, `CachedRunZero`, `CachedRunRetired`,
  `CachedRunCacheStops`, `CachedRunExecutionRejects` describe the bounded
  straight-line fallback. Stops may follow positive progress. This primitive
  is selected by ordinary dispatch only on WASM; native tests can call it
  directly but native dispatch remains unchanged.
- `OrdinaryRetired` counts successful ordinary `step_internal` calls in the
  concrete SystemBus batch loop only; it is not a universal instruction census
  (non-batch, JIT, trait-bus and exception/idle behavior are outside its scope).
- `GpioColdCalls`, `GpioNoEdgeDevices`, `GpioDevicesVisited`,
  `GpioHostedServices` separate cold-hook calls that only scan an empty edge
  address inventory from those that inspect or actually service hosted devices.
  Counters do not alter routing or cache device membership.

The manual/experimental `fastpath-census.yml` workflow runs feature-off and
instrumented T16 tests plus instrumented bus tests, then retains an unpublished
WASM artifact with its exact source commit, compiler/bindgen versions and hashes.
It neither changes app pins nor updates physical hardware drift acknowledgements.
