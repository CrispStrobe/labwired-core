# ST7735 foundation: merged-main native results and rectangular guest

These are source-bound qualification records, not paired speedups or current
consumer-artifact measurements. No app pin, baseline, floor, capture or hardware
acknowledgement changed. Earlier failures and rates remain preserved in the
[P0 receipts](../2026-10-07-sam-spi-p0/README.md).

## Automatic native qualification of foundation main

Measured source: `f77110d4646cab67cb12be9a19fc787afe3a650b` (PR161 merge),
not the later rectangular-test or documentation commits.
[Core Perf37610602435](https://github.com/CrispStrobe/labwired-core/actions/runs/37610602435)
passed all **42** absolute native chip-spin floors and **82** matched relative
board/mode checks: zero regressions or contract failures. PyBadge's three-sample
median was **56.386390x**, minimum **53.081996x**, maximum **56.747708x**,
mean batch width **1023.8**. Relative batch cost was 1.4 Ir/step for PyBadge and
1.6 for F411. Two stale-baseline advisories remain (EFR32MG26 and nRF51822 step);
no baseline was changed. This does not resolve the earlier F411 failure's cause.

Original [artifact11479287934](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11479287934/zip),
reported ZIP size 6,983 bytes; preserved JSON member bytes, descriptive names:

- [Absolute native spin](native-spin-rtx.json), SHA256
  `04c70f72c73872d4113876c548b3300f5428666c61da09fddcf5788ad762890e`.
- [Relative cost](native-relative-cost.json), SHA256
  `a8e8c28d873764f8218a8a741aa275a2fcbbe4c7c6f115b26d36a2a682dfc48b`.

[Native micro:bit37610602523](https://github.com/CrispStrobe/labwired-core/actions/runs/37610602523)
also passed on that exact source. Five GPIO/display/buttons samples had median
**5.604150x**, minimum **5.102160x**. Five selected polled LSM303AGR motion/display/
buttons samples had median **1.179718x**, minimum **1.170654x**; all five exceeded
1x. Full samples, held poses, progressing guest outputs and firmware/source
hashes remain in the original JSON, not just these summaries.

Original [artifact11478971382](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11478971382/zip),
reported ZIP size 8,740 bytes; only the reviewed JSON members are published here:

- [GPIO active](microbit-active.json), SHA256
  `8602e63cd3f58d147a53d9dd38f1d18903365ad21b6e016bd6f21390278a0b65`.
- [Motion active](microbit-motion.json), SHA256
  `c0e8a7a64ff363d6f42fdbb88c1191d8acef5ffb9f7f696917b67745d17da80b`.

These are runner-specific native functional-model workloads. Chip spin is not
active display or full-board batching; selected micro:bit results are not shared
sensor IRQ/audio, complete-board, browser-WASM or physical timing qualification.
Cross-run rate differences are not controlled A/B evidence.

## Rectangular authored guest landed separately

[PR164](https://github.com/CrispStrobe/labwired-core/pull/164) merged as
`077055e133eb3db102adafd2bbf654e5054b578e`, reviewed source
`c5a2e4922dbff773bac4ae89798f0dc320b1080a`. Tested merge checkout
`cce49762c770fa9e0911f34e06d9a010a4476912` and landed main have identical tree
`db13dd3994d80de66325e81b2a706626fc375958`. All **19** enabled checks passed;
four declared full/warm/image checks skipped. There was no separate board job
enabled for this test-only diff; do not invent one.

[Core CI37613979061](https://github.com/CrispStrobe/labwired-core/actions/runs/37613979061)
executed all five SAM guest tests in both
[feature-off job112767769873](https://github.com/CrispStrobe/labwired-core/actions/runs/37613979061/job/112767769873)
and [feature-on job112767769955](https://github.com/CrispStrobe/labwired-core/actions/runs/37613979061/job/112767769955):
five passed, zero failed/ignored/filtered in each. Guest SHA256:
`92f2790f03dab5c16c12f2945f4f92abb33dc09f104c8e02a1c300430940ebf8`.
The nonzero 2×3 window and wrong-axis negative passed in ordinary and forced-
legacy controller paths. This is an authored literal-layout guest, not execution
of compiled CODAL, deployed panel binding, all orientations, DMA or an RTx test.
Continue the [rectangular contract](../../engineering/st7735-rectangular-controls.md)
and [remaining P3 lane](../../engineering/st7735-color-foundation.md#follow-on-lane-rectangular-production-stream-and-panel-binding).
