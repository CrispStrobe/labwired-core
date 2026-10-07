# ST7735 captured-stream main: native performance snapshot

Measured source: `d102b42c0b8d01658b1bb23b7af4c28b14f9c845`,
the [PR167](https://github.com/CrispStrobe/labwired-core/pull/167) merge.
[Core Perf37639289675](https://github.com/CrispStrobe/labwired-core/actions/runs/37639289675)
passed all **42 absolute native chip-spin floors** (minimum required RTx 1)
and **82 matched relative board/mode checks** across 42 chips and 11 maps.
There were zero regressions, contract failures, waivers, skips or unmeasured modes.

PyBadge's three native spin samples had median **61.036527x**, minimum
**60.630180x**, maximum **62.264244x**, and mean batch width **1023.8**.
The relative report records 1.3 Ir/step for its batched mode. Stale-baseline
advisories remain for EFR32MG26 and nRF51822 step; no baseline was changed.
This does not establish the cause of the earlier F411 failure.

The original [artifact11492758164](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11492758164/zip)
reported a 6,944-byte ZIP. These JSON members preserve its bytes under descriptive
names:

- [Absolute native spin](native-spin-rtx.json), SHA256
  `9a46617d019f234ba1428add25db42074d3169b789264d80e2f566224b51cf20`.
- [Relative cost](native-relative-cost.json), SHA256
  `6bba21e254aa98a6b27a016ce7807dc993dfa6bc468f28c963aae506ac7d05fb`.

This is a runner-specific native functional-model spin snapshot, not a paired
speedup, active-display/full-board workload, native Arcade, motion, physical
timing, browser-WASM or installed-app result. The historical
[foundation-main and micro:bit receipts](../2026-10-07-st7735-main/README.md)
remain unchanged; no fresh micro:bit motion measurement is claimed here.
No consumer pin, floor, baseline, capture or hardware acknowledgement changed.
P3 panel binding and the [remaining target lanes](../../engineering/target-next-lanes.md)
remain separate acceptance gates.
