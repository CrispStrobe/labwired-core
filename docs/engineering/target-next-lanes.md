# Board completion lanes — 2026-10-05

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
- Blocking SAM SPI is verified but unmerged [PR152](https://github.com/CrispStrobe/labwired-core/pull/152),
  head `d95bbb12985f0185cfa815d1fe350f42cccdcca9` at this snapshot.
  Seven actual-board tests, including an authored Cortex-M polling guest,
  passed on predecessor `2713c36c` in
  [run37267470830](https://github.com/CrispStrobe/labwired-core/actions/runs/37267470830).
  Final [run37269646155](https://github.com/CrispStrobe/labwired-core/actions/runs/37269646155)
  now passes all enabled exact-head checks, including all three workspace shards;
  the four intentionally disabled full/warm/image jobs are skipped. PR is CLEAN
  and OPEN at the final check. This documentation update does not merge it.
- ST7735, DMA-driven native Arcade, QSPI, USB and audio are not qualified by
  buttons/controller tests. CP13 and CP14 remain open.

Consumer tasks: [Lite lanes](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/TARGET-NEXT-LANES.md).
Performance tasks and original results: [bw-board lanes](https://github.com/CrispStrobe/bw-board/blob/master/docs/TARGET-NEXT-LANES.md).

## Order and invariant acceptance

P1 first; P2 and P3 can follow independently; P4 needs P1/P2; P5 needs P3/P4.
M1 is a separate micro:bit lane. Resolve shared file ownership before parallel
work. Every change needs source-bound positive and negative guest tests, exact
head/guest hashes and actual observations. All enabled final-head checks must
finish before implementation landing; an aggregate is insufficient while
workspace shards run. Preserve downcast ceilings, generated-doc checks,
hardware captures/acknowledgment expiry and unchanged performance floors.
No invented pixels/completions, waivers or automatic consumer pin movement.

## P1 — reconcile documentation and land verified blocking SPI (ready)

**Files:** PR152's `crates/core/src/peripherals/sam/sercom_spi.rs`,
`generic_factory.rs`, `bus/attach.rs`, `bus/tick.rs`,
`configs/chips/atsamd51-pybadge.yaml`, `crates/core/tests/sam_sercom_spi.rs`.
New SPI files are on the PR branch, not yet the default branch.

Refresh head/checks. Inspect all three workspace shards, feature-off and
default-members regeneration. Repair any actual failure on the existing
branch. Earlier failures were byte-MMIO API names, an extra concrete downcast
and stale generated rows; latest fixes preserve their guards. Keep the prior
seven-test proof distinct from fresh final-head results. Before publishing the
PR's draft board document, remove its private operational paragraph; preserve
the public panel contract accompanying this handoff.

**Done when:** all enabled exact-head checks pass, intentional skips are named,
and merge/run are recorded. Retain mode/mux/clock/FIFO/peek/W1C/reset and guest
TXC-polling tests. Explicit exclusions remain DMA, pixels, edge-sampling slaves,
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

## P3 — ST7735 stream, panel and GPIO observation (after P1)

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
