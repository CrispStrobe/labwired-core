# ST7735 colour foundation — qualified; production panel still pending

`crates/core/src/peripherals/components/st7735_color.rs` is a pure byte codec.
The qualified generic-display integration opts into it using `serial_color` and
`load_rgb_lut`; it is not a shipped PyBadge display or
performance result. Existing fixed-format panels do not opt in, and the
ST7789 descriptor and PyBadge configuration remain unchanged.
The full acceptance boundary remains [P3](target-next-lanes.md) and the
[PyBadge panel contract](../boards/pybadge-native.md).

The implementation uses the [ST7735R v0.2 datasheet](https://cdn-shop.adafruit.com/datasheets/ST7735R_V0.2.pdf),
particularly sections 9.8.20–22, 9.15, 9.18 and 10.1.22. It decodes serial
RGB444/RGB565/RGB666 into six-bit memory channels. RGBSET has 128 entries;
undefined lookup values produce an explicit unknown pixel, not invented black.
Complete pixels remain distinguishable from incomplete bytes. Software reset
preserves depth/table; hardware reset restores depth and invalidates the table.
Reserved COLMOD rejection is an emulator API policy, not silicon qualification.

Unit controls cover all 4,096 RGB444 and 65,536 RGB565 inputs with a non-linear,
channel-distinct table, RGB666 low-bit exclusion, delivery chunk splits, partial
pixel discard, table invalidation/replacement and reset/depth changes. These
controls are not evidence of a working production screen. Exact-source hosted
qualification is recorded below; consumer adoption remains separate.

## Landed qualification — 2026-10-07

[PR161](https://github.com/CrispStrobe/labwired-core/pull/161) merged as
`f77110d4646cab67cb12be9a19fc787afe3a650b`, from reviewed source
`90943b8d72957f497d6794913ef43a066b302711`. The actual tested merge checkout
`4e99e37cd4ded9cb932c333418d5935dc1481477` and landed main have identical tree
`f1c3af7b4753ba5648ce12d7e63f3e1d5ce1e135`.
All 20 enabled checks passed; the four declared image/full/warm jobs skipped.

- [Core CI37605866790](https://github.com/CrispStrobe/labwired-core/actions/runs/37605866790)
  passed workspace shards, existing displays, standalone eleven-control codec,
  browser/WASM regressions and the scheduler/feature-off paths.
- [Scheduler job112741824558](https://github.com/CrispStrobe/labwired-core/actions/runs/37605866790/job/112741824558)
  executed all four authored SAM guest tests, with zero failures or ignores.
- [Feature-off job112741824415](https://github.com/CrispStrobe/labwired-core/actions/runs/37605866790/job/112741824415)
  passed 4,151 core unit tests (three existing ignores), including all sixteen
  stream and four GPIO controls, and all four SAM guest tests.
- [Native micro:bit run37605866679](https://github.com/CrispStrobe/labwired-core/actions/runs/37605866679)
  passed the previously failing catalogue gate and selected active workloads.
  Five GPIO/display/buttons samples had median **5.6307x**, minimum **5.3289x**;
  five motion/display/buttons samples had median **1.1656x**, minimum **1.1487x**.
  The [original artifact11476321255](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11476321255/zip)
  records the tested merge checkout above. These are runner-specific native
  64 MHz functional-model results, not an A/B gain, silicon timing, PyBadge,
  complete-board or WASM RTx measurement. Earlier source-bound rates remain
  historical evidence, not replaced measurements.

Guest SHA256: `7150263c7ecd391783621ca8f9639890dd2779e0ac4c0c74ec1b33a4d9f1ce16`.
Test-fixture SHA256: `ed34fd56bfcf22c21c2cce00a56cecc7f2e71f6ac043d69c4d89e4fdc283f797`.
The browser check does not qualify this new panel's guest rendering or speed.
P3 remains open for the rectangular production stream and actual panel binding;
no production descriptor, consumer pin, capture or performance baseline moved.

`.github/workflows/st7735-color-controls.yml` compiles this dependency-free
module directly with Rust 1.95.0 and requires all eleven controls to be listed
before execution. This focused hosted check supplements, not replaces, the
normal crate/workspace checks; it does not execute a guest or measure RTx.

## Qualified parser and memory integration

`crates/core/tests/fixtures/st7735r-memory.yaml` is an **unregistered test
prototype** using GM00's
132×162 controller extent, not a claim about PyBadge module GM/crop. It must be
loaded explicitly. It is deliberately outside the production device catalogue:
every `configs/devices` descriptor must be a registered kit, and this fixture
must not become a shipped panel before qualification. The production YAML
baseline stays at main's 89; no gate exemption or model allowance is added.
The original location failed `every_device_descriptor_is_a_kit` at source
`8a810a86b1958a071de109128b139fd274a61dec` in
[board run 37603425496](https://github.com/CrispStrobe/labwired-core/actions/runs/37603425496).
Preserve that failure; the relocation passed the final exact-head board check
above. That does not turn the earlier failed run into a passing result.
RGB888 artifact bytes contain bit-expanded RGB666 memory
channels (`(v << 2) | (v >> 4)`), not original wire bytes or analogue glass output.
BGR, gamma, scan order and inversion rendering remain unimplemented. MADCTL
address mapping reuses the generic engine; physical module parity is still owed.

The opt-in parser collects the complete 128-byte RGBSET payload, invalidates
the previous table at upload start, and leaves an interrupted upload undefined.
Whole-byte CS pauses preserve both parameter and pixel progress. A new command
discards an incomplete pixel; each completed pixel consumes one address step,
including a pixel whose lookup value is unknown. Later LUT changes do not
recolour stored RAM. Partial parameter-register updates on silicon are not
claimed by this all-or-nothing command parser.

RAM starts unknown. A validity map follows the same orientation/crop as frame
bytes; artifacts publish `known_pixels` and `unknown_pixels` and withhold bytes
if any visible pixel is unknown, even when bytes were requested. `painted_bytes`
is then null. Raw `framebuffer()` users must consult `known_pixels()` rather than
interpret storage placeholders as valid black. Generation includes validity,
so unknown-to-known black is observable. This policy avoids a fabricated frame.
The shared inspection decoder now accepts RGB888 for region ink measurements
and explicitly rejects unknown-pixel metadata, even if a caller supplies black
placeholders. Short payloads, overflowing RGB geometry/regions and malformed
unknown counts are errors. This is not a physical-colour or browser-rendering
claim; downstream RGB888 rendering still needs integration.

Software reset retains COLMOD, MADCTL, LUT, RAM and pixel validity; hardware
reset restores COLMOD/MADCTL defaults and invalidates the LUT, retaining RAM
and validity. Reset cancels parser fragments and restores orientation-aware
full windows. Optional reset/backlight GPIO attachment has bounded guest
qualification below, not physical timing or deployed-panel qualification.
Reserved COLMOD values are rejected without changing the selected format and
counted in artifact metadata. Clear-RAM/refresh and incompatible descriptor
formats are rejected for this profile.

Colour snapshots use a separate tagged shape containing RAM and validity.
Existing fixed-format v1 snapshots are unchanged. Cross-profile/wrong-size
restores are refused before mutation. As with existing display snapshots,
control/parser state is rebuilt by bus replay, not restored by this RAM snapshot.

Sixteen `st7735_stream_*` controls in
`crates/core/src/peripherals/components/declarative_display.rs` exercise this
prototype through the SPI door, plus descriptor/validity/snapshot checks.
They run in the normal hosted crate/workspace checks, not the standalone codec
job. They passed the exact-source checks above. Unit controls alone imply no
guest or performance result.

## Authored SAM SPI guest — bounded hosted qualification passed

`crates/core/tests/st7735_sam_guest.rs` loads authored Thumb instructions, vector
bytes and literals into the actual Cortex-M machine with the PyBadge chip/system
descriptors. The host attaches the prototype and sets a **fixture-only** visible
2×2 crop; it does not configure guest MMIO, write pixel RAM, replace the guest's
SPI implementation or seed completion. The guest enables MCLK/GCLK, configures
SERCOM4 and PB13/PB15 mux, controls PB7 CS/PB5 D/C, uploads a non-linear RGBSET,
selects RGB444 and a 2×2 window, sends four packed pixels and polls TXC before
releasing CS after each byte. An SRAM marker distinguishes guest completion.

The original two integration tests cover exact memory bytes/region measurement
and separate
MCLK/GCLK/mux/CS/D/C negatives in both ordinary and forced-legacy controller
paths. Clock/mux negatives must stall completion; CS/D/C negatives must complete
controller transmission without painting. The normal CI feature-off and
scheduler jobs invoke this target through the nonvacuous wrapper. Both tests
passed in the feature-on step of [run 37602485369](https://github.com/CrispStrobe/labwired-core/actions/runs/37602485369/job/112730721587)
at source `214473596c13ffd74cf1683367569c99b46dc258`; the superseded overall
run was cancelled. This is a completed two-test step, not a successful aggregate
or qualification of the later GPIO source. The final four-test qualification
is recorded above. The original tests prove no real module crop, full native frame,
reset/backlight, IRQ, DMA or RTx result.

## Qualified write-driven reset/backlight attachment

The optional `gpio_control` descriptor binds reset and backlight config keys
with explicit polarity. When configured, the generic kit shares one actual
display RAM between the SPI handle and a GPIO observer. It resolves the port
address and bit together, samples both inputs after stores to their GPIO ports,
and adds no per-cycle polling or controller-specific downcast. Missing optional
config keys remain unwired; a production board contract must require its actual
wiring. Existing descriptors without this option keep their previous behavior.

Known pad level requires an actively driven, uncontended pad; a latch value
alone is insufficient. Unknown reset blocks transfers and discards fragments
across state changes. Asserted reset restores control defaults and invalidates
the LUT, retaining RAM/validity. Held reset rejects writes. Unknown or inactive
backlight makes `lit` false but does not suppress RAM writes. Artifacts expose
`reset_asserted` and `backlight_on`, with null for an unknown connected wire.
These are conservative emulator policies, not analogue/timing qualification.

Four added narrow-port unit controls and two added SAM integration controls
passed in both feature-on and feature-off checks. The SAM fixture uses the actual generic kit/AttachCtx path
with an explicitly enabled fixture-only crop. Authored instructions drive PA0
reset and PA1 backlight, including held/undriven/muxed reset, a wrong-port bit,
dark-but-painted RAM, and reset after disabling SPI's MCLK. Separate host-MMIO
diagnostics check backlight changes without a tick and contended-pad rejection.
External input/net changes alone do not notify this store-driven observer;
their propagation remains a separate task. No full-frame, physical module,
timing, native Arcade, DMA, IRQ, browser, app adoption or performance claim is
made by this bounded foundation.

## Reached follow-ups and next implementation slice

The first nonzero rectangular literal-layout guest and wrong-axis negative
subsequently landed in PR164. Read its [bounded qualification](st7735-rectangular-controls.md)
and the [automatic foundation-main native results](../receipts/2026-10-07-st7735-main/README.md).
The later [PR167 merged-main native snapshot](../receipts/2026-10-07-st7735-main-d102/README.md)
also passed 42 chip-spin floors and 82 relative checks; PyBadge spin median was
61.036527x, minimum 60.630180x. These are native spin results, not active-panel
or browser-WASM rates or a controlled gain over the earlier snapshot.
Subsequent bounded proofs also landed: [stock-CODAL bytes through SAM](st7735-codal-sam-binding.md)
(PR167), [all eight GM00 orientations](st7735-orientation-controls.md) (PR168),
original PXT direct-method capture (PR169), [retention](st7735-retention-controls.md)
(PR170), [invalid ranges](st7735-address-range-controls.md) (PR171), and
[original PXT caller fragments](../receipts/2026-10-08-st7735-pxt-caller/README.md)
(PR172). Do not repeat those implementations. They do not identify a deployed
module or qualify a complete original ARM runtime.

1. Preserve the landed codec, parser, inspection, GPIO and actual-guest controls;
   do not repeat their implementation. Partial-table silicon contents and
   external net-only GPIO notifications remain outside this boundary.
2. Follow the [production evidence lanes S1–S4](st7735-production-next-lanes.md):
   actual build/configuration, module/LUT binding, supported image dimensions,
   then blocking integration. The older full contract below remains the
   acceptance boundary, not a list of wholly unimplemented controls.
3. Only then complete P3 qualification, followed by the separate IRQ/DMA and
   production-runtime lanes. No app pin, hardware acknowledgement or benchmark
   baseline should move on codec unit tests alone.

The pinned [CODAL stream](https://github.com/lancaster-university/codal-core/blob/312ae57e0b31f5b9df07a81e9d846945828e3c5a/source/drivers/ST7735.cpp)
is a captured integration input. Independent datasheet vectors and actual driver
bytes must agree before a production-stream claim.
[PR167's captured-stream guest](st7735-codal-sam-binding.md) now qualifies
the small pinned stock-CODAL palette/RAMWR inputs through authored blocking SAM
MMIO. Later PR168/169/172 add fixture orientation and host direct-path/caller
proofs, not deployed PXT configuration, production DMA/IRQ or the physical module.

## Follow-on lane: rectangular production-stream and panel binding

The GPIO prerequisite and bounded stream/orientation/retention/range/caller
follow-ups have landed; start from refreshed main. Use S1–S4 above to avoid
repeating reached work. Keep this full contract separate from P2/P4's IRQ/DMA
implementation and consumer package adoption.

1. Record the deployed public runtime build's macro configuration, CF2 board ID,
   `DISPLAY_CFG0`, width and height. The pinned PXT screen source derives MADCTL
   from CFG0 bits 0..7, offX from 8..15 and offY from 16..23; defaults are not
   deployed-board evidence. `USE_RGB444`, board-ID gating and doubled images
   choose distinct paths. Retain an explicit unknown if build evidence is absent.
2. Extract a small, reviewable input vector from the exact pinned selected
   CODAL/PXT path, preserving its notices. Stock CODAL's `setAddrWindow` sends
   CASET from its **y** arguments and RASET from **x**; its palette upload fills
   the first sixteen entries of each channel block and zeroes the remainder.
   Match those bytes against independent datasheet expectations before running
   them in an authored blocking-MMIO guest. A host driver/vector check alone
   does not qualify the native DMA driver.
3. Use a non-square, at least 3×2 fixture with distinct corner/adjacent colours,
   odd pixel and CS chunk boundaries, and every advertised MADCTL orientation.
   Calculate expected addresses independently of the implementation's mapping
   helper. The existing symmetric 2×2 proof cannot distinguish an x/y swap.
   Test retained RAM versus current orientation, LUT replacement and reset.
4. Establish the public module's aperture/offsets separately from its GM straps.
   The pinned Arcada PyBadge header specifies INITR_BLACKTAB, rotation 1 and
   160×128 application dimensions; that does **not** establish physical GM mode
   or the deployed PXT CF2 values. Do not silently attach the GM00 prototype to
   PyBadge. Keep controller-memory RGB666/expanded RGB888 distinct from glass
   BGR/inversion/gamma presentation; do not relabel memory evidence as glass.
5. Publish exact source/vector hashes, hosted positive/negative results and
   remaining exclusions. Only qualify the bounded blocking panel path after
   existing displays and all enabled checks pass; no DMA, physical capture,
   active-frame RTx, WASM or app adoption claim follows automatically.

Primary inputs are the pinned CODAL and PXT sources in the
[panel contract](../boards/pybadge-native.md#st7735-is-not-a-renamed-st7789),
plus the pinned Arcada board header under its wiring definitions. No inferred
default replaces an observed deployed configuration.
