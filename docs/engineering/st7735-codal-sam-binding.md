# Captured CODAL commands through SAM — bounded qualification passed

This extends the bounded [rectangular guest](st7735-rectangular-controls.md),
not the production panel descriptor or native DMA runtime.

## Actual source capture

The original pinned driver/header executed successfully in
[trace run37623161527](https://github.com/CrispStrobe/labwired-core/actions/runs/37623161527/job/112798202082)
at harness source `f426a724fd6924d8ae74585a44e0722e8503e437`.
Three cases cover byte, word and word-plus-tail packing; the deliberately corrupt
CASET capture failed as required. The harness landed in
[PR166](https://github.com/CrispStrobe/labwired-core/pull/166), merge
`b79fc643d8ff274966081ea55e01d989ad86fd40`, after all 15 enabled checks passed;
four declared image/full/warm checks skipped. Host trace execution is not ARM
driver or module qualification.

`crates/core/tests/fixtures/st7735-codal-host-trace.json` preserves the original
JSON member bytes from [artifact11483103038](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11483103038/zip).
Its SHA256 is `2e4d70928807baa1b3110a2a09ef6c32d9482ab6fa32ac0ce0cd8efe76b2d062`.
It records source/header hashes, compiler, all commands and scope exclusions.
The hosted trace job requires that exact fixture hash and compares freshly
executed driver commands against it. Compiler-version metadata is retained, not
silently normalized or required to match a future host's compiler string.

## Qualified authored guest proof

The sixth test in `crates/core/tests/st7735_sam_guest.rs` embeds those captured
CASET, RASET, RGBSET and RAMWR payloads into authored Thumb instructions. The
guest configures clocks, mux and SPI, drives CS/D/C and polls TXC. It writes the
actual production palette upload, not the earlier independently authored LUT.
The host attaches only the explicit fixture crop; it neither seeds controller
completion nor writes display RAM/MMIO configuration on the guest's behalf.

Three nonzero windows (2×3, 2×4, 2×5 physical controller-memory crops) run through
ordinary and forced-legacy controller paths. Literal expected RGB888 pixels
include distinct channels, a non-extreme colour and two **known black** pixels
from zero-valued palette entries. Expectations use no codec/addressing helper.
A named negative removes one RGBSET byte from the guest's transmitted stream:
SPI must finish but all visible pixels remain unknown and frame bytes withheld.

## Landed qualification and next tasks

[PR167](https://github.com/CrispStrobe/labwired-core/pull/167) merged as
`d102b42c0b8d01658b1bb23b7af4c28b14f9c845`, reviewed source
`0199e45c720468b340c2f3f84601d3776222e21a`. All 20 enabled checks passed;
the four declared image/full/warm checks skipped. The
[feature-off job](https://github.com/CrispStrobe/labwired-core/actions/runs/37626609246/job/112809882875)
and [scheduler job](https://github.com/CrispStrobe/labwired-core/actions/runs/37626609246/job/112809883171)
each executed all six guest tests: six passed, zero failed, ignored or filtered.
The [live driver/fixture comparison](https://github.com/CrispStrobe/labwired-core/actions/runs/37626609261/job/112809882579)
also passed on that source. Guest-file SHA256:
`9f956367e038b1e9dba75e7ecf85713eba7fe823e36f63812b43336dc21370af`.

The tested preview `4cd56056f58f314b454342ddcb0bf03d0b9aa67f` had tree
`3066988c5d228ca43f3896c76b3611a65070edbd`. The landed tree
`88e350915f214c70372932e55c2d3678eadd6166` equals the pre-merge expected
combined tree; its only difference from that tested tree is PR165's seven
documentation/receipt paths. Executable and workflow content did not change.
These full trees are not claimed identical.

The separate [all-orientation guest follow-up](st7735-orientation-controls.md)
adds a seventh test and remains pending its own exact-source qualification.

This executes captured driver bytes on ARM, **not the original CODAL ARM code**,
DMA driver, interrupt/fiber completion, deployed CF2/macros or actual module.
Complete independent all-orientation, odd-pixel interruption, reset/LUT retention
and panel aperture/GM evidence before closing P3. Review out-of-range window
semantics separately: the prototype currently clamps bounds, whereas the
[ST7735R datasheet](https://cdn-shop.adafruit.com/datasheets/ST7735R_V0.2.pdf)
sections 10.1.19–20 specify ignoring out-of-range data. Do not promote that
prototype policy to silicon fidelity. Glass/BGR, IRQ/DMA, active-frame RTx,
new-panel WASM rendering and consumer adoption remain unqualified.
