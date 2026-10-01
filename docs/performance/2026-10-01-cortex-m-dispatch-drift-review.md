# Cortex-M dispatch: temporary hardware-drift review, 2026-10-01

The user approved a dated, expiring, exact-content-bound acknowledgement after
reviewing the tradeoffs. This is **not a silicon capture**. The physical capture
dates, results and silicon model digests remain unchanged. Hardware re-capture
is still owed; these acknowledgements expire on **2026-10-31**. The normal
30-day policy is unchanged, and changed model content invalidates the digest.

Affected manifest entries: nrf52840, seeed-xiao-nrf52840-sense, stm32h563,
nucleo-l476rg, nucleo-l073rz, stm32f103 and stm32f407. The two nRF entries
share a physical reference; they are not independent hardware evidence.

## Reviewed implementation and evidence

Reviewed CPU revision: `13ace46fe9ea26ae489a07bae57d67aac95fe166`.
Exact comparison base: `4d944d2d9ec320f7acd25b54cc41a50b850d6788`.

The optimization borrows the cached decode entry and selects existing fast-path
probes using a tagged, width-checked opcode. Existing probe ordering, shape and
live-address checks, caller observer/debug/IRQ/IT/trace/tap/budget guards,
cycle accounting and access accounting remain in place. Cold, collided and
wide entries decline without side effects. This does not introduce a new MMIO
coalescer or peripheral model. Simulation tests cannot prove every interaction
with physical hardware.

- [Rust CI](https://github.com/CrispStrobe/labwired-core/actions/runs/36835509065):
  execution suites passed; the default-member gate failed on these seven
  unacknowledged content drifts. Feature-off unit suite: 4,056 passed, three
  existing ignored tests. This is not a zero-skip claim for the whole job.
- [Native board qualification](https://github.com/CrispStrobe/labwired-core/actions/runs/36835509055):
  156 Cortex-M tests passed, zero ignored, including 21 discovery tests.
  The whole job retains two physical-only integration ignores.
- [Exact-base native A/B/B/A](https://github.com/CrispStrobe/labwired-core/actions/runs/36835539860):
  median-of-medians 1.3267339211x to 1.4996376974x (+13.03%);
  all ten candidate motion windows passed 1x, minimum 1.4661383953x.
- [Native CorePerf](https://github.com/CrispStrobe/labwired-core/actions/runs/36835542979):
  all 40 spin targets passed median and minimum 1x; no instruction-performance
  regressions or waivers. This does not qualify realistic firmware on all chips.
- [Fresh WASM qualification](https://github.com/CrispStrobe/bw-board/actions/runs/36835546181):
  deterministic builds and functional motion passed, but motion realtime failed
  (median 0.7389873307x, minimum 0.7163933137x); publication was skipped.
- [Hosted WASM A/B](https://github.com/CrispStrobe/bw-board/actions/runs/36836511572)
  and [independent repeat](https://github.com/CrispStrobe/bw-board/actions/runs/36837055740):
  pooled median improvements +6.60% and +7.38%, respectively. All 40 windows
  remained below 1x, and the repeat's candidate minimum was worse than baseline.
  These are diagnostic results, not floor qualification or universal speedups.
- Actual Lite WEB input-routing tests: four passed, zero skipped, at Lite
  `d0dd61a5abfaf027273fabf27eb464eacd387354`.

[Immutable full receipts and provenance](https://github.com/CrispStrobe/bw-board/tree/b15cdfb20a223750154556f606eaee33fdf6af7d/docs/receipts/2026-10-01-cortex-m-dispatch)
preserve actual tested sources, runner identities, artifacts and raw outcomes.
Do not relabel these measurements as tests of a later acknowledgement commit.

## Landing and remaining obligations

This acknowledgement changes review metadata and documentation only; CPU source
and tests remain byte-identical to the reviewed revision. Final-head CI must
pass before merging the CPU change. Hardware re-capture remains owed before
expiry; a new model change requires a new content-bound review or capture.

No deployed WASM pin changes or artifact promotion are authorized by this
acknowledgement. CP13 remains open: all five motion windows must meet 1x and
browser worker/debugger qualification must pass before promoting an artifact.

