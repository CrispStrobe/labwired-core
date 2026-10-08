# ST7735 invalid-address guard — authored guest qualified

This P3 model/fixture/guest slice follows
[retention controls](st7735-retention-controls.md).
The bounded model and authored guest controls passed. Keep this separate from
deployed panel binding, Image layout, IRQ/DMA and consumer adoption.

## Landed qualification — 2026-10-08

[PR171](https://github.com/CrispStrobe/labwired-core/pull/171) merged as
`98732ec2452fd6d9f5302154ac207e68ae817b89`, from reviewed source
`53187fb82082b93c54c22081ecf087e460920890`. The tested merge checkout
`ed99670021db47c83a5f84ccf463f9b5a6aeb573` and landed main have identical
tree `102a3facb4963f2997a3a4edb7d2d12bd5145ed2`.
All 21 enabled checks passed; four declared full/warm/image jobs skipped.

[Core run37702186754](https://github.com/CrispStrobe/labwired-core/actions/runs/37702186754)
passed the new full-RAM/validity unit control and all nine SAM guest tests,
zero failed/ignored/filtered, in both
[feature-off](https://github.com/CrispStrobe/labwired-core/actions/runs/37702186754/job/113068021050)
and [feature-on](https://github.com/CrispStrobe/labwired-core/actions/runs/37702186754/job/113068020846).
Guest SHA256:
`588690ff109ab9cb553960172ce7e7cba4c9507a5435963900e151d954610c6f`.
Fixture SHA256:
`63dde9c59d4ebaa0472cb272ce00d8e56b827f6ccc70e77fc26a54c4757f85e6`.

The [native micro:bit board job](https://github.com/CrispStrobe/labwired-core/actions/runs/37702186807/job/113068005657)
also passed selected active display/button and motion/display/button workloads.
It is not a fresh 42-chip spin sweep, a PyBadge active-panel rate, a controlled
speedup or a new WASM RTx result. Live CODAL, existing displays, workspace and
browser gates passed. No app pin, floor, baseline or hardware evidence changed.

## Contract and narrow change

[ST7735R v0.2](https://cdn-shop.adafruit.com/datasheets/ST7735R_V0.2.pdf),
sections 10.1.19–20 (printed pages 102 and 104), describes ignoring pixel data
outside the configured GM memory extent. GM00 has 132 columns and 162 rows;
MV interchanges the logical extents. Start must not exceed end.

The test fixture previously clamped CASET/RASET to the last legal address.
That aliases invalid writes onto border RAM. Removing that clamp alone is
insufficient: the shared saturating mirror transform can alias an oversized
coordinate onto physical zero.

The fixture now preserves the complete 16-bit window parameters. The opt-in
serial-colour write path checks logical bounds before mirroring. Complete
decoded pixels still advance the counters even when their destination is
ignored; ignored writes change neither RAM bytes nor validity bits.
The generic mapping helper and non-serial display paths remain unchanged.

Inverted windows violate the documented start/end restriction. This emulator
conservatively suppresses their writes while consuming the stream. That is an
explicit API policy, not qualification of unspecified silicon behavior.

## Prepared controls

A model-level test uses literal logical extents and physical corners for all
eight GM00 orientations. A mixed 2×2 window spans the last legal and first
invalid coordinate in each axis; only its first pixel may be stored. It compares
the complete RAM and validity map and verifies all four decoded positions were
consumed. Invalid-only columns/rows, both `0xffff` axes and inverted bounds
must leave those complete snapshots unchanged.

The ninth SAM guest test runs all eight orientations in ordinary and
forced-legacy SPI paths, both with a known blue corner and an unknown corner:
32 scenarios, each with seven guest-written checkpoints. It checks invalid-only
column suppression, mixed-window green preservation, `0xffff` and inverted
column suppression, incomplete RGB444 input and subsequent RGB666 repaint
recovery to literal `[69,117,174]`. The host inspects a fixed one-pixel physical
crop and injects no panel MMIO or pixels. Expected addresses and colours do not
call model mapping/decoder helpers. Model-level full-RAM checks cover writes
outside that guest crop; the crop alone is not a full-frame integrity assertion.

## Acceptance

Require the new model test and all nine SAM guest tests to execute with zero
failures, ignored or filtered tests in both feature configurations, plus every
enabled workspace/display/browser/live-driver gate. Bind exact source and
fixture/guest hashes to the tested checkout and landed tree. Preserve failures;
formatting and static review are not execution. Do not waive a cancelled or
missing workspace shard.

This does not establish a deployed module's GM/aperture/BGR, full PXT Image
compatibility, real firmware driver/DMA/IRQ behavior, physical invalid-input
behavior, active-frame RTx, browser-WASM speed or installed-app adoption.
P3/CP14 remain open.
