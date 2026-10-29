# Board completion lanes — 2026-10-08

These are proposed task contracts, not ownership claims or passing receipts.
Refresh the default branch and open PRs before starting; shared behavior lands
in the engine first, followed by an explicit qualified consumer pin.

## PR performance guard — qualified and merged — 2026-10-19

[PR253](https://github.com/CrispStrobe/labwired-core/pull/253) merged as
`080d0c35908c9ca5887ec7aafe5ea23b07b6d8a7` after all 17 enabled exact-head
checks succeeded; five source-declared conditional jobs were skipped. The
landed tree and ordered parents match the tested composition. This lane is
complete and its source ownership is released.

Owned-fork PRs affecting runtime, chip descriptors, build settings, performance
inputs or the gate now automatically run the unchanged full performance budget.
Selection compares the actual tested merge with its admitted first parent;
docs/test-only branches, including branches behind main, skip the build.
Runtime renames/deletions remain selected and Git/source errors fail closed.
Six policy/history controls and four detected selector mutations passed.
The frozen issue251 campaign, main/manual/scheduled policy, every baseline,
tolerance, fixture/profile and issue-write boundary remain unchanged.

The automatically selected [PR campaign 39254133590](https://github.com/CrispStrobe/labwired-core/actions/runs/39254133590)
and its once-retained independent reported-context audit passed all 82 modes
across 42 chips, with no regressions or stale rows and the existing RV1103
waiver unchanged. Its four reports do not contain raw profiles; the audit does
not claim independently reconstructed instruction counters, causal attribution
or new browser/physical-timing evidence. No runtime/model/guest or pin changed.

## C9 AVR layout-cost measurement — 2026-10-17

Measurement source delivered by the Codex AVR layout investigation lane;
C9.1–C9.3 now have a bounded explanation and no-implementation decision, following
[the Lite task contract](https://github.com/CrispStrobe/brickwright-lite/blob/main/lanes/2026-10-17-c9-avr-instruction-cost-layout-investigation.md).
Own a diagnostic profile producer and pure controls in
`scripts/perf/avr_layout_profile.py` and `scripts/perf/test_avr_layout_profile.py`,
a separate manual `.github/workflows/avr-layout-profile.yml`, and
`docs/engineering/avr-layout-cost.md`. No existing gate or runtime change.
Pure producer controls may run on matching pull requests; the actual profiling
job remains manual-only and requires those controls first.

Measure exact owned-fork snapshots `f1ca219d08080f4d93a73a3a4c5630e0824db799`
(pre-sync), `6e03dd12e22ceb534ec1bc494c360b72209ca94f` (sync merge),
`715e1272d639aff3d12c0c73ff5f00e9a94d3371` (current), and the already retained
upstream source `49f656f5233d109a8d357e6c5e0cec9e788decd5`. Use hosted CI,
the existing authored AVR spin input and measurement bounds; retain build and
input identities, step/batch counts, raw Callgrind profiles and per-function
annotations. Freeze the independent original reader before dispatch, acquire
originals once, and keep all failures. The reader never executes a guest.

Do not change baselines, tolerances, release profiles, decoder/engine behavior,
fixtures, pins, captures or ACKs. No local heavy build, upstream workflow,
automated issue write, layout-performance or physical timing claim follows
from source inspection. A later optimization needs a separately precise scope
and unchanged full-baseline qualification on both prescribed source trees.

Producer delivery is complete: [PR248](https://github.com/CrispStrobe/labwired-core/pull/248)
merged as `5b470d61` after all 16 enabled source checks passed. The existing [run 39001605287](https://github.com/CrispStrobe/labwired-core/actions/runs/39001605287)
and its once-only independent original/cohort audits passed. C9.1 is complete;
reuse retained originals, with no redispatch or new download. The
[causal native-code review and decision](avr-layout-cost.md#causal-native-code-review-and-implementation-decision--2026-10-19)
records the exact extra timer frame instructions, central-clock outlining cost
and eager advance setup, together with residual source-expression and LLVM
trigger limits. C9.3 declines another runtime optimization: the obsolete timer
shape is absent from the retained issue-251 fixed candidate and prior broad
inlining attempts supplied no qualified improvement across both prescribed
source families. No baseline, tolerance, clock or waiver
changes are justified. The current investigation is complete within those
explicit boundaries. A future compiler-trigger diagnostic or optimization needs
its own precise scope and unchanged qualification. Preserve other owners' shared
machine lanes; this measurement claim grants no runtime or consumer-pin change.

## Issue 251 — native throughput regression repaired — 2026-10-19

[PR252](https://github.com/CrispStrobe/labwired-core/pull/252) merged as
`79071f69cd6035a820cbdad93da7343d7d501e4d` after all 28 enabled exact-head
checks succeeded across 11 events; nine source-declared conditional checks were
skipped. The landed tree and ordered parents exactly match the reviewed and
qualified composition. This lane is complete; its source ownership is released.

The fix in `crates/core/src/bus/samd51_storage.rs` inlines the cached
capability-presence check and outlines the unchanged controller-present body.
It preserves service order, four-boundary commit/READY behavior, store routing,
geometry/refusals and active model semantics. No pin, ACK, baseline, tolerance,
fixture or release profile changed.

The sole [paired native campaign](https://github.com/CrispStrobe/labwired-core/actions/runs/39238796436)
compared frozen pre-fix `1c580dcb4e906b197778caa9dabe2ddb2db614c4` with reviewed
candidate `945e80351b34780d78ddba061fd5b7c22cf9a679`. The control reproduced all
30 step-mode regression flags; the candidate passed all 82 modes across 42 chips
with zero flags. Both passed the existing native real-time floor. Reported host
instructions per simulated step included:

| Board | Pre-fix | Fixed |
|---|---:|---:|
| ATmega328P | 1019.7 | 995.7 |
| SAMD51 PyBadge | 916.3 | 893.3 |
| nRF52833 | 1371.8 | 1348.8 |

Originals were acquired once. Independent bounded metric-report and full Git
commit-object audits verified exact source/tree/ordered parents, unchanged
inputs, matching native tools/runner image, coverage, table/JSON joins and the
unchanged gate predicates. Preserve the original intake limits: the full
auxiliary view was refused because optional native disassembly exceeded its
materialization envelope. The producer's shallow parent list was not accepted
as complete ancestry; immutable full commit objects supplied that proof
independently. Only the bounded metric reports are qualified. No raw-profile,
per-call causal, comparative wall-time, browser or physical-timing claim follows.

Fresh [actual ZFlash qualification](https://github.com/CrispStrobe/labwired-core/actions/runs/39238771258)
passed default and event-scheduler profiles. Independent once-retained
source/image/stream, matching guest ELF and four exact model-patch audits passed:
12 phases, ten original method returns, six IRQ-helper returns and both specific
erase/program mutants per profile. The new absent-controller control and
existing active cadence/neighbor controls passed on both bus paths. Broader
reset/earlier-caller and all other enabled correctness checks also passed.

The existing [shipped-main performance run](https://github.com/CrispStrobe/labwired-core/actions/runs/39244959337)
completed successfully and [issue251](https://github.com/CrispStrobe/labwired-core/issues/251)
closed on October19. The later full guard campaign above also passed. This
observation is complete; do not dispatch a replacement or reacquire its originals.
A future failure needs its own exact source/tool/runner and measurement review,
with every gate predicate preserved.

The paired workflow is a frozen one-campaign diagnostic. Do not repeat it or
reacquire successful originals. Any future reuse first needs a fresh scoped
admission, full ancestry capture and a bounded metric artifact separate from
optional native listings. C9 mechanism/optimization work remains separately
scoped; this fix neither repeats its completed campaign nor advances a consumer
engine pin. Subsequent Codal engine/fixture adoption and finite RAFFS/settings
composition landed in PR72/PR74 below; full-board closure remains separate.

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
- P3 follow-ups PR164/167/168/170/171 now qualify non-square layout, captured
  stock-CODAL input, all eight fixture GM00 orientations, RAM/LUT/reset
  retention and invalid ranges through authored blocking SAM guests. PR169/172
  add actual host execution of original PXT direct-method/caller fragments.
  [PR172's exact-source receipt](../receipts/2026-10-08-st7735-pxt-caller/README.md)
  records both eleven-case caller builds, sixteen admission controls and the
  independent 93,360-byte audit. All fifteen enabled checks passed; merge
  `ef6e085b3163200f2e6b49356dc2cc8d5a602a6b` matches the tested tree.
  The latest archived native spin snapshot is still source `d102b42`, with
  PyBadge median **61.036527x**, minimum **60.630180x** and 42/82 passing gates;
  it is not a new current-main, active PyBadge, controlled A/B or WASM result.
  Read the [production evidence lanes S1–S4](st7735-production-next-lanes.md).
- Actual selected CODAL service/heap and SPI callers now cover success,
  cancellation, gated timeout, DMA error, software-pended idle IRQs and bounded
  blocking-result ownership. Runtime34/Core239 additionally qualify deliberate
  active/delivering/pending-result destruction refusal and settled idle
  reconstruction: [all ten actual cases and exact limits](samd-real-spi-lifetime-qualification.md).
  The [current admission checkpoint and next lanes](samd-permissive-runtime-lanes.md#actual-service-integration-checkpoint--2026-10-15)
  link final source/images, both engine profiles and preserved failures. These
  are bounded callers, not an admitted board image, RTx result or consumer pin.
- The actual ST7735 production panel, DMA-driven native Arcade, QSPI, USB and audio are not qualified by
  buttons/controller tests. CP13 and CP14 remain open.

Consumer tasks: [Lite lanes](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/TARGET-NEXT-LANES.md).
Performance tasks and original results: [bw-board lanes](https://github.com/CrispStrobe/bw-board/blob/master/docs/TARGET-NEXT-LANES.md).

## Order and invariant acceptance

P0's bounded idle-controller repair and P1 have landed. The original P3/P2/P4
ordering below is historical: finite actual service/SPI/DMA callers subsequently
qualified through Core220–249 and Codal21–81. Preserve those results instead of
repeating their completed prerequisites. Production-display/native-board
integration remains separate; follow the [current SAMD checkpoint](#samd51-selected-zflash-manual-storage--2026-10-18)
and its linked no-ASF implementation lanes. P5 still requires its real P3/P4
production integration, rather than only the finite control fixtures. Dynamic clock-rate derivation remains a separately qualified
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
Later bounded stream/orientation/retention/range/caller controls are recorded
above; do not implement them again. Next execute the
[production evidence lanes S1–S4](st7735-production-next-lanes.md): deployed
public build configuration, actual module aperture/GM/LUT evidence, supported
image/status dimensions and blocking integration. Preserve the
[full panel-binding contract](st7735-color-foundation.md#follow-on-lane-rectangular-production-stream-and-panel-binding).
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

**Bounded prerequisite reached:** [PR210](https://github.com/CrispStrobe/labwired-core/pull/210)
landed the opt-in software-triggered BYTE/RAM foundation. Both final-source
profiles passed90 guest cases/36 interrupt entries and returns each, after all 22
enabled checks passed. Read its [receipt](../receipts/2026-10-12-samd-dmac-byte/README.md)
and historical [live-TX contract](samd-dmac-byte-foundation.md#next-bounded-lane--live-sercom4-tx-byte-requests).
Later actual service/SPI/DMA qualification is recorded in the [current SAMD
checkpoint](#samd51-selected-zflash-manual-storage--2026-10-18); do not repeat its
completed finite prerequisites. Production display/native-runtime integration
still has its own selection and affected-guest requirements.

**Files:** new SAM DMAC peripheral/factory/descriptor, bus request/IRQ seams,
PyBadge chip descriptor and authored descriptor guests.

Read exact CODAL DMA sources in the panel guide. DMAC base `0x4100a000` needs
byte descriptors, BASEADDR/WRBADDR tables, end-address increment convention,
BTCNT/writeback, channel windows, per-beat triggers and W1C completion/errors.
SERCOM4 RX/TX triggers are 12/13; channels 0..3 use IRQ 31..34 and 4..31 share 35.
Confirm stride/masks before implementation; unsupported modes stay explicit.

The generic `DmaRequest` queue is not a per-channel transfer-result protocol:
`SystemBus::execute_dma` returns bus failures, but the scheduler drain currently
logs them without reporting them to the originating DMA channel. A new SAM
controller must observe each beat's bus result before committing count,
writeback or completion, using a reviewed bus-aware service or result seam.
Do not decrement a failed beat or turn an invalid access into success. Follow
the primary distinction between bus transfer error (TERR/channel disable) and
invalid descriptor suspension (SUSP/FERR). Add actual guest controls for both.
SERCOM DRE/RXC requests need their live request-level behavior, not just an
enable-time pulse. A DMA completion must remain distinct from final SPI TXC.

**Done when:** guest-created descriptors move real memory to SPI DATA, write
back progress and reach the correct handler. Disabled clocks/controller/channel,
wrong trigger, invalid descriptor/bus access, masked IRQ and abort/reset must
not fabricate completion. Execute the pinned ZSPI DMA -> TXC -> callback/fiber
sequence. Functional DMA is not arbitration/cycle-timing qualification.

## P5 — native PyBadge and remaining devices (after P3/P4)

**Files:** public guest/qualification workflow, PyBadge descriptors/tests;
consumer adoption is a separate Lite lane.

First execute an independently authored permissive guest through DMA, display,
buttons and existing NeoPixels. Before whole PXT/CODAL runtime execution,
complete the [SAMD permissive-runtime admission lanes R0–R3](samd-permissive-runtime-lanes.md):
the historical PyBadge bases are classified chip-restricted/ASF4 by the consumer.
Reproducible build/configuration evidence is not execution admission. Preserve
historical classifications; qualify a separate permissible simulation build.
Then build/run the identified admitted PXT/CODAL runtime
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
This remains unimplemented: the system explicitly leaves P0.25 unconnected and
the selected sensor descriptors exclude interrupts. Do not close it by attaching two ordinary Boolean outputs to one pad: legacy
routes apply queued outputs directly, so the last writer can incorrectly
release another asserted source. Explicitly selected pulled-up open-drain
routes retain state per device instance and role; ordinary routes keep their
existing behavior. The electrical candidate requires the qualification below
before it supports any sensor interrupt claim.

**First bounded implementation:** an opt-in shared open-drain resolver for
resident-device outputs, plus only the required attachment/configuration seam.
Retain state by device instance and output role; either asserted driver holds
the line low, releasing one cannot override the other, and both released yield
the explicitly declared pulled-high level. Preserve existing output behavior
for every unselected route. Require reversed-device-order, release/reset,
unwired-output and unrelated-pin controls, reject an unsupported/undeclared
new-mode configuration, and detect a meaningful last-writer-wins mutation.
Qualify affected bus behavior with source-bound hosted controls and an authored
guest before claiming a newly selected device boundary. This electrical stage
alone does not implement sensor interrupts or establish hardware behavior.

Placements opt in by listing output role names under `config.open_drain_pullups`;
the list declares a pull-up on each connected pad. Unknown or duplicate
roles and mixed legacy/open-drain output bindings on one pad are rejected.
An unconnected output remains unconnected. The current resolver is digital;
it does not model floating nets, analog behavior, delay or contention.
CPU reset does not itself reset external devices; device reset and output
release must be qualified separately.

**Then, separately:** freeze the selected sensor's register/output polarity,
pin, routing, masking, clearing and clock ownership from its published source.
TWIM advances attached-device time, but `declarative_i2c.rs` currently promotes
data-ready state before I2C reads. An IRQ requires autonomous state promotion
and output delivery while the I2C caller is idle, not host-forced data-ready or
an extra read that manufactures an edge. A declarative rule can subscribe with
`on: { data_ready: NAME }` to a uniquely named I2C conversion; the device clock
promotes that subscribed conversion at its deadline and fires the rule once.
Conversions with no such listener retain the existing read-driven behavior.
The event only reports conversion completion; a device descriptor must still
model the documented output routing, polarity, masking, clearing and reset.
A disabled in-flight conversion is cancelled, while an already-ready result
continues to follow its declared clear rules. Combining these events with device
timers is rejected until cross-clock ordering is implemented and qualified.
Review selected sensor cancellation and clear semantics before using the event. Wire P0.25 only after those contracts
are implemented and prove sensor assertion/deassertion and mask/reset through
real GPIO/GPIOTE guest observations in both scheduler profiles. Subsequently
qualify RUNMIC bias/control,
timed ADC sampling and speaker path with explicit units/cadence and bounded
observations; no host-forced data-ready flags.

**Done when:** guest interrupts and continuous sample/output tests pass,
negative cases pass and active hosted RTx is measured without weakening motion
or forty-chip gates. Physical noise/calibration/audio remain excluded without
independent capture. Browser adoption still requires the consumer lanes.

## A10: nRF52 UARTE transmit-buffer lifetime — 2026-10-15

DONE (source) — [PR244](https://github.com/CrispStrobe/labwired-core/pull/244), merged as `6a769a59694ec8920de559f9e3fffa2f5375ac6b`.
Capture the active EasyDMA transmit descriptor and bytes before the producer
can reuse its stack buffer. Preserve the existing stop-bit completion deadline,
UART publication and completion events, legacy UART personality, receive path,
and bounded scheduler ownership. No generic bus/scheduler behavior change.
Envelope: `crates/core/src/peripherals/nrf52/uarte.rs`, focused peripheral controls,
new `crates/core/tests/microbit_v2_uarte_stack_guest.rs`, an original source-built
`examples/microbit-v2/uarte-stack-buffer.S` using the existing board linker,
its provenance/validation notes, this contract, and an added ARM GCC setup plus
nonvacuous guest command in the existing `.github/workflows/core-ci.yml`
feature-off job, and the two explicitly approved model-digest bindings for
`nrf52840` and `seeed-xiao-nrf52840-sense` plus affected generated status rows.
Acknowledgement dates, expiry, capture records and watch lists stay unchanged.
Workspace integration shards already provide ARM GCC and run
the scheduler-enabled target; the feature-off lib suite cannot reach guests.
If an engine-wide hook is necessary, expand the envelope explicitly before
editing that shared boundary.

Qualification: host controls must detect late reads and wrong descriptors while
retaining delayed completion; a real compiled Cortex-M4 guest must overwrite
its stack transmit buffer and still produce the original ordered serial bytes.
Run affected tests in both scheduler configurations and all enabled exact-head
correctness/performance checks. Do not weaken baselines, hardware ACKs, captured
results or thresholds. The later deterministic WASM release and explicit
bw-board/Lite adoption are separate consumer boundaries; native source tests
do not qualify the released artifact, physical timing or the whole board.

The claim and its qualification extension landed at `72a2d6a82` and
`b6cb3b632`. The candidate latches PTR/MAXCNT at STARTTX, captures bytes at
the first scheduler boundary, and retains the original frame-count completion
deadline. At most one capture/completion pair is armed per pending transfer;
zero-length transfers retain one completion wake. Host controls cover changed
staging descriptors, stack reuse, fresh rearm and absence of early completion.
The original guest has 168 loadable bytes (5272-byte ELF). Hosted run
`38727112401`, workspace shard 1, passed its source-built guest at scheduler
intervals 1 and 64, asserting all 19 original serial bytes and every reused
stack slot. Source-bound library controls passed except for the new test
adding one conditional-compilation site above the existing policy ceiling;
that failure is retained and repaired with an explicit CI interval selection.
The operator approved the two digest updates after guest qualification; their
existing October 13 acknowledgement and November 12 expiry remain unchanged.
The final head integrated qualified reset changes from PR242 and passed all
27 enabled checks before the matched-head merge; seven optional checks were
explicitly skipped. The feature-off guest and scheduler intervals 1 and 64
passed all 19 ordered serial bytes and stack-reuse assertions. Aggregate
workspace results were 7,860 passes and no failures across 473 test targets;
105 existing ignored cases remain ignored. The landed tree and parents matched
the reviewed merge prediction.

The separate deterministic release `labwired-wasm-6a769a59` is now published
through [bw-board PR621](https://github.com/CrispStrobe/bw-board/pull/621):
its retained-byte hosted qualification passed all 123 tests without skips,
and original-byte lineage plus release digests were independently checked.
[Lite PR988](https://github.com/CrispStrobe/brickwright-lite/pull/988) still
requires actual CLI/browser qualification before consumer integration.
No physical re-capture, full-board or physical-timing result is claimed.

### A10 follow-up: interrupt-chained transmit completion — 2026-10-15

IMPLEMENTED candidate — Codex, branch `lane/a10-uarte-completion-irq-20261015`.
The first source-built stack guest and deterministic release remain passing
bounded evidence. Lite PR988's actual compiled MakeCode CLI guest nevertheless
failed both complete-line checks: serial begins `hl bw` rather than `hello bw`.
Preserve that failure; consumer integration remains held.

Review found that UARTE completion unconditionally raises TXSTOPPED alongside
ENDTX, whereas the nRF52833 specification associates TXSTOPPED with STOPTX.
An interrupt handler that starts its next byte on ENDTX can then process the
spurious stop event and clear the new transfer's busy state. Correct this event
separation in `crates/core/src/peripherals/nrf52/uarte.rs`, retain STARTTX buffer
capture and framed completion, and add nonvacuous interrupt-chain controls.
Extend the original assembly guest and native guest test to cover that sequence
at exact and recommended intervals. Update affected UARTE completion tests in
`crates/core/tests/nrf52_easydma_tick512_fidelity.rs` to request STOPTX explicitly
when asserting TXSTOPPED; keep completion-cycle and byte assertions intact.
The envelope includes the guest's provenance notes, existing core CI guest
selection if necessary, and this contract. No generic bus, CPU, scheduler,
MakeCode base, compiler, dependency or unrelated peripheral change is claimed.

Require affected feature-off and scheduler clean guest qualification before
new model-digest acknowledgement bindings for the same two nRF52840 entries;
keep acknowledgement dates, expiry, physical captures and watch lists intact.
Then qualify and publish a distinct deterministic engine release and requalify
Lite's original complete serial assertions. The published first engine and all
original failures stay retained; no weakening or relabelling of their results.

### A10 shared nRF5340 UART regression boundary — 2026-10-15

The first follow-up's feature-off run passed both original ARM UART guests
and 4,368 library tests, but the unchanged nRF5340 Zephyr survival case then
failed its full greeting (only `*` was emitted). Keep the original failure.
Zephyr's enhanced polling routes ENDTX to STOPTX through DPPI; the nRF5340
DPPIC is currently a zero-returning stub. The old unconditional stop event
masked this missing configured route. Do not restore an unsolicited stop for
unconfigured nRF52 transfers or weaken the Zephyr greeting assertion.

Expand this lane before implementation to a bounded UART-local DPPI subset:
`crates/core/src/peripherals/nrf52/dppic.rs` for CHEN/CHENSET/CHENCLR state;
its declaration in `nrf52/mod.rs` and construction in `nrf52/factory.rs`;
its required canonical type entry in `peripherals/generic_factory.rs`;
`configs/chips/nrf5340.yaml` promotes that controller to the native subset;
remove its obsolete zero-stub entry from `crates/core/src/bus/known_stubs.rs`.
The UARTE resolves the sibling base from the existing ChipMap and triggers
its own STOPTX only when PUBLISH_ENDTX and SUBSCRIBE_STOPTX enable the same
valid channel and DPPIC.CHEN enables it. Preserve ordinary explicit STOPTX.
No generic bus/CPU/scheduler algorithm, cross-instance DPPI routing or full
DPPI support is included. Document that limited route and remaining gaps.

Require disabled-publisher/subscriber/channel, mismatched/out-of-range channel,
remapped-controller, channel-enable alias, and positive configured-route
controls. Both feature-off and scheduler guest paths, the original complete
nRF5340 Zephyr greeting and all enabled final checks remain mandatory.
New physical results or weakened assertions/ACK dates are not authorized.

The first candidate removed the unsolicited completion-time TXSTOPPED assignment;
explicit STOPTX stayed live. Its feature-off original stack-reuse and new IRQ
guest passed, including exact 19-byte and 17-byte streams. The unchanged nRF5340
Zephyr greeting then failed. The scoped DPPI follow-up above remains pending;
no replacement engine, updated model ACK or consumer completion is claimed.

The DPPI candidate now promotes the DPPIC channel-enable subset and resolves
its base through ChipMap. UARTE's local event/task route checks both endpoint
enable bits, a matching channel below 32, and live CHEN at ENDTX. Explicit and
configured STOPTX share one task action. Controls cover byte/word controller
aliases, disabled or mismatched routes, channels 31/32, missing DPPIC,
remapped controller addresses, and channel changes between capture/completion
in both scheduler and bus-tick modes. No generic scheduling algorithm changed.
Fresh clean guest and nRF5340 Zephyr qualification is pending; earlier passing
native streams and the original later survival failure stay retained.

### A10 classic PPI and differential regression boundary — 2026-10-15

The DPPI candidate's feature-off job now passes both compiled UART guests and
the unchanged nRF5340 Zephyr greeting. Scheduler-observable qualification still
fails two nRF52840 Zephyr JIT/interpreter comparisons at the existing RTC live-
event ceiling. Preserve that failure, the ceiling of eight, the instruction
budget and all architectural, UART, cycle and memory comparisons.

Expand the existing local-route envelope before implementation to classic
nRF52 PPI: resolve the PPI controller and this UART's base through the factory,
and recognize an enabled configurable channel only when its EEP is this
UART's ENDTX and its TEP or FORK names this UART's STOPTX. No unconditional
stop, general event fabric, cross-instance route or RTC/scheduler change.
Own `nrf52/uarte.rs` and `nrf52/factory.rs` for this subset and controls, plus
`crates/core/tests/cortex_m_jit_nrf_differential.rs` for failure-only UART/PPI
register observations that rethrow the original panic. Successful execution
must not acquire extra observations or altered comparisons. Keep the full
Zephyr markers and configured-cycle/JIT assertions mandatory.

Require disabled-channel, wrong-event/task, last configurable channel, remapped
addresses and enabled primary/fork route controls, both bus-tick and event
paths, then all affected clean guests and exact-head checks. This extension
permits no threshold, baseline, firmware-byte or ACK/date relaxation.

The classic-PPI candidate now resolves both bases and admits a local stop only
through an enabled configurable channel's exact EEP and primary/fork endpoint.
Controls cover disabled and mismatched routes, channel 19 versus fixed channel
20, missing controller, remapped addresses and address overflow, in scheduler
and bus-tick modes. UART buffer capture/publication deadlines stay intact.
Failure-only Zephyr diagnostics rethrow the original payload; wrapper expansion
preserves the original normal test source and all comparisons/budgets exactly.
This is a candidate pending fresh clean qualification, not a resolved RTC or
full PPI-fabric claim. All earlier passing and failing originals remain retained.

The model-path gate additionally requires the new DPPIC source to appear
explicitly in the uncaptured `nrf5340` board's `models:` list in
`validation/manifest.yaml`. Add that coverage entry without changing the gate,
any captured board's watch list, ACK field or silicon record. The original
workspace failure remains retained; no allowlist exception is introduced.

### A10 current guest qualification and digest binding — 2026-10-16

The combined UART-local completion/PPI/DPPI candidate at `769847de` passed
affected clean hosted guests. Native feature-off passed 4,372 library tests,
the exact 19-byte stack-reuse stream, the exact 17-byte IRQ-chain stream with
zero unsolicited stops and one final explicit stop, and the unchanged nRF5340
Zephyr greeting. Scheduler guests passed stack reuse at intervals 1/64 and
IRQ chaining at 1/64/1024. Both previously failing nRF52840 Zephyr JIT comparisons
and the CLI stop-condition test passed with their original budgets, markers
and event ceiling. All three workspace shards passed: 7,879 passed, zero
failed, 105 existing ignored tests; model-path coverage passed. The actual
CI checkout tree equalled the reviewed candidate. Earlier failures remain retained.

The operator approved rebinding only the two existing nRF52840/XIAO drift-ACK
digests to this qualified model content. The October 13 ACK dates, November 12
expiry, silicon captures and captured-board watch lists stay unchanged. This
is an acknowledgement of model drift, not a physical re-capture.

Source [PR247](https://github.com/CrispStrobe/labwired-core/pull/247) is merged
at `1ea67f9c742c22e8ea5d73ea642956b4f8907bd6`, retaining reviewed engine source
`de14721e4ae1d34a428f91d232e4520d50f1ceba`. All 28 enabled final-head checks
passed; seven conditional checks had declared skips. The landed tree and both
parents matched the reviewed merge composition.

Producer [PR629](https://github.com/CrispStrobe/bw-board/pull/629) is also
merged. Distinct engine run
[38855838615](https://github.com/CrispStrobe/bw-board/actions/runs/38855838615)
passed two independent matching builds, determinism and all 125 release tests
with zero failures or skips, including both exact UART streams. Published
`labwired-wasm-de14721e` assets match the original qualified build bytes.
Lite [PR988](https://github.com/CrispStrobe/brickwright-lite/pull/988) merged
as `3753d3b4` after all 12 enabled final checks passed (two declared conditional
skips). Actual compiled V2 JSON/text CLI greeting/counter, matrix/fault/time
bounds and a fresh browser V2 ELF's complete Debug-console greeting passed.
A10 is complete across source, rebuilt engine and consumer. The first published
engine and earlier failures remain retained; no hardware or broader physical accuracy claim is added.

## SAMD51 selected ZFlash manual storage — 2026-10-18

Source [PR249](https://github.com/CrispStrobe/labwired-core/pull/249) merged as
`7da375fc8ae4f61c191502a542a2d92e6ab9f948` after all 29 enabled exact-head
checks passed, with seven source-declared conditional skips. Fresh paired guest
[run39113938039](https://github.com/CrispStrobe/labwired-core/actions/runs/39113938039)
and independent source/image/stream/four-patch audits passed; the landed tree
and ordered parents match the qualified composition. Read the [qualified boundary and follow-up contracts](samd51-zflash-manual-storage.md)
for the actual paired ZFlash guest, original-method/CPS observations, exact
model mutants and independent audits. The selected opt-in NVMCTRL/CMCC slice
preserves default descriptors, consumer pins, existing limits and failures.
It does not close full board/runtime/storage. That source merge itself advanced
no consumer pin. Subsequent [Codal PR72](https://github.com/CrispStrobe/codal-samd/pull/72)
explicitly adopted engine79071f69 with all 22 enabled checks and 21 independent
case audits. [PR74](https://github.com/CrispStrobe/codal-samd/pull/74) separately
qualified actual RAFFS/settings plus GC/services: both scheduler profiles,
1,871 storage checks, object remount and eight specific mutation/profile cases.
Those two tasks are complete; preserve their original evidence and limits.

Subsequent [write compaction](https://github.com/CrispStrobe/codal-samd/pull/76),
[source census](https://github.com/CrispStrobe/codal-samd/pull/77), and
[modeled retained-backing reset](https://github.com/CrispStrobe/codal-samd/pull/78)
also completed their finite qualifications. The reset leaves peripheral models
live and retains its original one-Bus scope. [PR81](https://github.com/CrispStrobe/codal-samd/pull/81)
separately qualifies completed-operation backing reopen across a fresh engine
process and startup, with a specific lost-backing control in both profiles.
That completed-operation receipt does not qualify interrupted writes. Subsequent
[Codal PR94](https://github.com/CrispStrobe/codal-samd/pull/94) separately qualified
three actual NVM boundaries with strict old/old/new recovery across fresh
processes in both profiles. Its [interruption contract](https://github.com/CrispStrobe/codal-samd/blob/master/docs/PXT-STORAGE-INTERRUPTION.md)
retains that finite scope. Later native cases cover separately declared modeled
deletion, compaction, clearing and torn-operation faults; physical power loss and
complete peripheral reset remain unqualified. Do not backdate later evidence to
the earlier reopen or one-Bus reset receipts.

[PR80](https://github.com/CrispStrobe/codal-samd/pull/80) admits the reproducible
actual generated USB1/WebUSB1 configuration. [PR83](https://github.com/CrispStrobe/codal-samd/pull/83)
admits authored CF2 source/data metadata, not actual runtime CF2 reads or a
bootloader. [Core256's explicit USB banks](samd51-usb-device-banks.md) and
[Codal82's real driver/core leaf](https://github.com/CrispStrobe/codal-samd/pull/82)
have their own finite source-built USB qualification; the default stub and
existing production consumer selections retain their prior scope.

The [selected no-ASF lanes A–G](https://github.com/CrispStrobe/codal-samd/blob/master/docs/PXT-NO-ASF-NEXT-LANES.md)
now record implemented compiler/link closure, pin/EIC/PWM and SERCOM/DMA/timer/
audio adapters, generated native PXT programs, actual authored CF2 reads,
USB/HF2/startup and endpoint lifetime cases. Required features remain enabled;
this is the selected software profile, not blanket admission of other sources
or physical-board behavior. Guessed layouts and successful return stubs still
cannot close a source/runtime lane. Do not repeat completed implementation lanes.

[Codal PR103](https://github.com/CrispStrobe/codal-samd/pull/103) separately landed
the selected display-clock and RAFFS v2 work after all four hosted fullboard
profile/shard jobs passed. Its [storage status](https://github.com/CrispStrobe/codal-samd/blob/master/docs/PXT-STORAGE-NEXT-STEPS.md)
and [CRC/migration contract](https://github.com/CrispStrobe/codal-samd/blob/master/docs/RAFFS-V2-CHECKSUM.md)
preserve modeled-fault limits, the legacy negative control and format/downgrade
constraints. Older finite fixtures keep their own engine/source selections.
These later results do not silently update this repository's published package
or an installed consumer. Normal project compile/import, debugger/GUI acceptance,
representative active native/WASM performance and physical observations remain
separate. Keep inventories, notices, original failures, captures, ACKs and pins.


<a id="explicit-native-simulation-assembly-candidate--2026-10-27"></a>

## Explicit native simulation assembly — qualified supplier baseline, 2026-10-29

The [no-ASF SAMD51 assembly](samd-native-simulation-bundle.md) gives the existing
qualified model subset a separate descriptor and registers the controller-memory
colour model for config-driven consumers. The real CLI passed both scheduler
profiles, nineteen architectural/assembly controls passed, and the initial
reset-inactive Ir baselines and unchanged 1× gate have source-bound measurements
in that contract. These results do not establish active-game real-time speed.

The supplier stages are complete for the frozen published baseline:

- [Core PR262](https://github.com/CrispStrobe/labwired-core/pull/262) merged as
  `3b655aef4085082152129c84849d4e2ad5004a7f` after its affected source, fleet,
  selected guest and CLI qualification.
- [BW PR711](https://github.com/CrispStrobe/bw-board/pull/711) merged as
  `1f92a1aa85e808827a4c28b219bf11a572f07882` after its enabled checks and
  independent original audit. [Producer40699886470](https://github.com/CrispStrobe/bw-board/actions/runs/40699886470)
  passed two byte-identical production builds, the existing 125-test legacy suite
  and three source-built Node game samples.
- The [locked release](https://github.com/CrispStrobe/bw-board/releases/tag/labwired-wasm-3b655aef-locked)
  is API-immutable and its nine asset digests match qualified originals. Preserve
  its component notices, Node 22 workaround and the
  [producer's stated limits](https://github.com/CrispStrobe/bw-board/blob/master/docs/SAMD-ENGINE-RELEASE-HANDOFF.md).
  Motion was deferred in that campaign; publication is not motion qualification.

Do not repeat those completed supplier stages or republish their originals.
Later Core/Codal source changes do not automatically enter this frozen release.
The active consumer qualification belongs to
[Lite PR1533](https://github.com/CrispStrobe/brickwright-lite/pull/1533); consult its
[current handoff](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/SAMD-NATIVE-GAME-HANDOFF.md)
and [adoption contract](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/SAMD-NATIVE-GAME-ADOPTION.md)
before any pin, dispatch or artifact acquisition. Do not create a duplicate lane
or infer Chromium/installed acceptance merely from native/Node supplier results.
Normal project compile/import/debugger/GUI-route adoption and representative
active-game native/WASM performance remain separately scoped follow-ups. Deferred
motion needs its own scoped evidence; the successful original campaign is not
replayed to inspect it. The historical PyBadge module/glass contract remains
unresolved: this authored software assembly does not establish physical accuracy.
