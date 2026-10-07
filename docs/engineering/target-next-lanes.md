# Board completion lanes — 2026-10-07

These are proposed task contracts, not ownership claims or passing receipts.
Refresh the default branch and open PRs before starting; shared behavior lands
in the engine first, followed by an explicit qualified consumer pin.

## Current state

- micro:bit v2 matrix/buttons, bounded SAADC/EasyDMA, authored ARM scan guest
  and selected LSM303AGR motion are implemented. Read the
  [motion contract](microbit-lsm303agr.md) and
  [composed qualification](microbit-composed-qualification.md). Native selected
  workload and forty-chip spin receipts do not establish browser, audio,
  silicon timing or complete-board qualification.
- PyBadge buttons merged in [PR151](https://github.com/CrispStrobe/labwired-core/pull/151)
  as `f6d0cf8223de56d28dad09d57594f6febc6ab320`, after enabled exact-head checks
  passed in [run37240999321](https://github.com/CrispStrobe/labwired-core/actions/runs/37240999321).
  Tests cover all 256 masks, active-low/MSB-first shift, transparent latch,
  frozen snapshot, rising-only clock, zero fill and finite input validation.
  Five PA15 NeoPixels already have a model; do not recreate them.
- Blocking SAM SPI merged in [PR152](https://github.com/CrispStrobe/labwired-core/pull/152)
  as `67920fcf8d82c74f3cc63285181fe26db93cf720` (reviewed head
  `6b6b174a9aa69acca03cec086419cc4c21358bc5`).
  Seven actual-board tests, including an authored Cortex-M polling guest,
  passed on predecessor `2713c36c` in
  [run37267470830](https://github.com/CrispStrobe/labwired-core/actions/runs/37267470830).
  Merged-main [Rust Core CI37481145753](https://github.com/CrispStrobe/labwired-core/actions/runs/37481145753)
  passed. The panel guide was sanitized before landing.
- The same main's [Core Perf37481145682](https://github.com/CrispStrobe/labwired-core/actions/runs/37481145682)
  failed the absolute RTx floor: PyBadge was **0.05x**, batch **1**, while
  generic ATSAMD51 was **67.97x**, batch **1023.8**. This is a real measured
  regression, not merely missing relative-perf metadata; the relative stage
  did not execute after the RTx failure. Track [issue158](https://github.com/CrispStrobe/labwired-core/issues/158).
  This is the pre-repair baseline, not current main's rate.
- Idle-controller repair [PR159](https://github.com/CrispStrobe/labwired-core/pull/159)
  merged as `ae127c89b9f60ed8a239f82858859c31ef8255af`, reviewed/tested source
  `547da6da814afd0375199b192db7e3e2416042a0`; merge tree equals reviewed tree.
  All 20 enabled correctness checks passed, including all workspace shards;
  four declared full/warm/image jobs skipped. Two exact-source full runs passed
  all 42 native chip-spin RTx floors: PyBadge median **61.4713x** and
  **84.1056x**, batch **1023.8**. Only the second full run passed relative cost;
  the first's STM32F411 failure and a failed diagnostic harness are preserved.
  Valid isolated-build repeats also pass the unchanged selected baselines.
  Read the [source-bound receipts and limitations](../receipts/2026-10-07-sam-spi-p0/README.md).
  Idle SPI owns no scheduled work; active service remains one-cycle, with live
  clock-gate freeze. **This is not active display, full-board or WASM proof.**
  The full PyBadge GPIO resident still legitimately requires interval 1.
- Native selected micro:bit [qualification37481145769](https://github.com/CrispStrobe/labwired-core/actions/runs/37481145769)
  passed on that main: GPIO median **6.6944x**, motion median **1.2748x**
  (minimum **1.2697x**, all five motion runs above 1x). These are native active
  guest results, not browser RTx or whole-board qualification.
- ST7735 colour/parser/GPIO foundation landed in
  [PR161](https://github.com/CrispStrobe/labwired-core/pull/161), merge
  `f77110d4646cab67cb12be9a19fc787afe3a650b`, with 20 enabled checks passing.
  All four authored SAM guest tests passed with and without scheduler features.
  Fresh selected native micro:bit medians were **5.6307x** GPIO/display/buttons
  and **1.1656x** motion/display/buttons (minimum **1.1487x** motion).
  Read the [exact-source qualification and remaining P3 contract](st7735-color-foundation.md).
  These rates are not an A/B, PyBadge or WASM result; no app pin moved.
- The actual ST7735 production panel, DMA-driven native Arcade, QSPI, USB and audio are not qualified by
  buttons/controller tests. CP13 and CP14 remain open.

Consumer tasks: [Lite lanes](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/TARGET-NEXT-LANES.md).
Performance tasks and original results: [bw-board lanes](https://github.com/CrispStrobe/bw-board/blob/master/docs/TARGET-NEXT-LANES.md).

## Order and invariant acceptance

P0's bounded idle-controller repair and P1 have landed. P3 blocking
display work comes next, then P2 IRQ routing and P4 DMA as separate changes.
P5 needs P3/P4. Dynamic clock-rate derivation remains a separately qualified
part of P2, not a prerequisite for truthful nominal-clock blocking tests.
M1 is a separate micro:bit lane. Resolve shared file ownership before parallel
work. Every change needs source-bound positive and negative guest tests, exact
head/guest hashes and actual observations. All enabled final-head checks must
finish before implementation landing; an aggregate is insufficient while
workspace shards run. Preserve downcast ceilings, generated-doc checks,
hardware captures/acknowledgment expiry and unchanged performance floors.
No invented pixels/completions, waivers or automatic consumer pin movement.

## P0 — restore idle controller batching (landed; stability follow-up open)

**Files:** `crates/core/src/peripherals/sam/sercom_spi.rs`, scheduler delivery
in `crates/core/src/lib.rs`, `crates/core/tests/sam_sercom_spi.rs`.

PR159 moves idle SPI off the legacy walk through real scheduler ownership, not
a false inert declaration. Retain its forced-walk reference. Active service
remains one-cycle: do not advertise an active display speedup from an idle
spin improvement. Keep the central clock-gate resolver authoritative and freeze
the countdown while MCLK or GCLK is disabled. Preserve mux blocking, CS/DC,
FIFO, peek, W1C, disable/reset cancellation and guest TXC polling.

**Preserve:** scheduler/forced-walk and feature-off tests pass, authored guest
results match at intervals 1/64/1024, both clock gates freeze/resume without
early/duplicate bytes, and repeated exact-source hosted Core Perf runs recover
PyBadge >=1x without weakening any other chip's floor or relative baseline.
All enabled final-head CI checks must finish before merge. A later deadline
optimization must separately prove clock-pause and GPIO-observation semantics.

**Open measurement follow-up:** the identical final source produced F411 batch
1.9 Ir/step (failed) and 1.6 (passed) in full runs; valid focused candidate
repeats produced 1.8 and 1.6 (both passed). Do not erase the red receipt or claim
its cause resolved. Instrument raw low/high instruction counts and initialization
versus guest-loop contributions on pinned binaries; prove a stable measurement
without increasing tolerances, changing baselines or selecting only good runs.
The invalid shared-target diagnostic must never count as an A/B. This follow-up
does not qualify active devices or replace P3/P2/P4/P5.

## P1 — blocking SPI (landed; not native Arcade)

**Files:** PR152's `crates/core/src/peripherals/sam/sercom_spi.rs`,
`generic_factory.rs`, `bus/attach.rs`, `bus/tick.rs`,
`configs/chips/atsamd51-pybadge.yaml`, `crates/core/tests/sam_sercom_spi.rs`.
The files are now on main. Preserve the prior seven-test proof and the merge/CI
records above; do not repeat landing work. Retain mode/mux/clock/FIFO/peek/W1C/
reset and guest TXC-polling tests. Explicit exclusions remain DMA, pixels, edge-sampling slaves,
split IRQ delivery and dynamically derived kernel-clock timing. This is not
native Arcade qualification.

## P2 — SAM IRQ routing and clock semantics (after P1)

**Files:** SAM controller, generic peripheral/bus IRQ reporting and scheduler,
PyBadge chip descriptor, new integration tests. Read the pinned CMSIS
instance/component definitions linked in the [panel guide](../boards/pybadge-native.md).

SERCOM4 has four vectors beginning at IRQ62; derive actual DRE/TXC/RXC/ERROR
mapping, do not guess or collapse all onto one boolean line. DMAC has five
vectors. Extend the generic seam if necessary. Derive transfer timing from
enabled kernel and CPU clocks; test live rate changes and pending transfers.

**Done when:** authored guests enter correct independently masked/cleared
handlers, pending-on-enable/reset/clock-off behavior is tested, rate changes
preserve order/deadlines, and scheduler/feature-off regressions pass. Functional
countdowns do not establish wire or silicon-cycle accuracy.

## P3 — ST7735 stream, panel and GPIO observation (foundation landed; open)

PR161 completed the codec, opt-in parser/memory/inspection, write-driven GPIO
and bounded 2×2 authored blocking guest foundation. Preserve those controls;
do not recreate them or register their test-only fixture as a production kit.
Next execute the [rectangular production-stream and panel-binding contract](st7735-color-foundation.md#follow-on-lane-rectangular-production-stream-and-panel-binding):
deployed public build configuration, independent non-square address vectors,
actual driver bytes, module aperture/GM evidence and presentation semantics.
The requirements below describe the full P3 boundary, not already reached
physical-panel or native-runtime qualification.

**Files:** `crates/core/src/peripherals/components/declarative_display.rs`,
generic GPIO-observation seam, new `configs/devices/` ST7735 descriptor,
`configs/systems/pybadge.yaml`, actual-board tests.

Read the panel guide first. Implement RGB444 packed groups and 0x2D RGBSET
LUT required by the pinned stock driver, not a renamed fixed-RGB565 ST7789.
Qualify RGB565/RGB666 only if advertised. Test partial groups, format/window
changes, clipping, CS and D/C. Establish actual GM mode/visible offsets from
public board sources, with four uniquely colored corners in every orientation.
Implement hardware/software reset differences and RAM retention; observe PA0
reset and PA1 backlight independently of stored pixels, without shared-bus
controller downcasts. Add a permissive guest sending commands via real MMIO.

**Done when:** guest SPI produces expected visible pixels/checksum, LUT and
reset-retention tests pass, wrong CS/mux/reset/backlight behave as specified,
and existing displays remain green. Host-injected frames are not guest proof;
this lane alone does not qualify the native DMA driver.

## P4 — SAM DMAC and driver completion (after P1/P2)

**Files:** new SAM DMAC peripheral/factory/descriptor, bus request/IRQ seams,
PyBadge chip descriptor and authored descriptor guests.

Read exact CODAL DMA sources in the panel guide. DMAC base `0x4100a000` needs
byte descriptors, BASEADDR/WRBADDR tables, end-address increment convention,
BTCNT/writeback, channel windows, per-beat triggers and W1C completion/errors.
SERCOM4 RX/TX triggers are 12/13; channels 0..3 use IRQ 31..34 and 4..31 share 35.
Confirm stride/masks before implementation; unsupported modes stay explicit.

**Done when:** guest-created descriptors move real memory to SPI DATA, write
back progress and reach the correct handler. Disabled clocks/controller/channel,
wrong trigger, invalid descriptor/bus access, masked IRQ and abort/reset must
not fabricate completion. Execute the pinned ZSPI DMA -> TXC -> callback/fiber
sequence. Functional DMA is not arbitration/cycle-timing qualification.

## P5 — native PyBadge and remaining devices (after P3/P4)

**Files:** public guest/qualification workflow, PyBadge descriptors/tests;
consumer adoption is a separate Lite lane.

First execute an independently authored permissive guest through DMA, display,
buttons and existing NeoPixels. Then build/run the identified PXT/CODAL runtime
without patching completion paths. Retain compiler/runtime/guest hash,
boot/frames/input press-release and actual debugger pause/step/reset evidence.
Measure active RTx, frame rate and input latency, not terminal idle spin.
Split audio and QSPI into separate subsequent changes with public wiring and
active guests. USB needs independently qualified support or explicit isolation.

**Done when:** every advertised device has guest proof, active native RTx meets
the unchanged floor on the declared runner, and the app separately qualifies
the exact engine artifact. Display-only proof must not mark CP14 DONE.

## M1 — micro:bit shared IRQ and microphone/audio (separate lane)

**Files:** `configs/systems/microbit-v2.yaml`, selected LSM303AGR model,
Nordic GPIO/GPIOTE/SAADC scheduler, authored guests and
`.github/workflows/microbit-board-io.yml`.

Start from the selected LSM303AGR variant, not a silent FXOS8700 substitute.
Wire shared open-drain P0.25 IRQ and prove sensor assertion/deassertion,
mask/reset behavior through the guest. Subsequently qualify RUNMIC bias/control,
timed ADC sampling and speaker path with explicit units/cadence and bounded
observations; no host-forced data-ready flags.

**Done when:** guest interrupts and continuous sample/output tests pass,
negative cases pass and active hosted RTx is measured without weakening motion
or forty-chip gates. Physical noise/calibration/audio remain excluded without
independent capture. Browser adoption still requires the consumer lanes.
