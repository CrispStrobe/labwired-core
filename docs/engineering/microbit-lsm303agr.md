# Selected micro:bit v2 LSM303AGR motion model

This is an original native LabWired functional model, not copied firmware or a
generic descriptor that returns sensor identity without implementing samples.
It selects the LSM303AGR-equipped micro:bit v2 variant. The
[Foundation I²C inventory](https://tech.microbit.org/hardware/i2c/) also lists
an FXOS8700 variant; that is not selected or silently emulated here.

Both production and example systems attach `accelerometer` (`lsm303agr_accel`)
and `magnetometer` (`lsm303agr_mag`) to `i2c0`. Addresses are fixed at 7-bit
`0x19` and `0x1e`, respectively, with no address override. The native model is
[`lsm303agr.rs`](../../crates/core/src/peripherals/components/lsm303agr.rs).
The validation manifest watches that actual source so later changes cannot
silently preserve the board's recorded validation status.

## Input and sampling contract

Each component exposes live `x`, `y`, `z` channels: acceleration in g and
magnetic field in µT. Values default to zero; no gravity, rotation, heading or
motion is invented. Inputs are held physical values, not a generated trajectory.
Firmware configures output data rate and operating mode over I²C. The central
machine's elapsed simulation time advances conversions; host calls or register
reads do not substitute for sampling time or force an always-ready result.
Accelerometer high-resolution startup waits seven configured sample periods;
other modes expose the first sample after one period. Magnetometer single-mode
conversion is approximated by one configured ODR period, not a measured analog
conversion delay.

The intended bounded contract includes accelerometer low-power / normal /
high-resolution data formats, per-axis block-data-update retention, and
magnetometer continuous / single / idle modes. Register behavior and scaling
are derived from the [ST LSM303AGR datasheet, Rev 11](https://www.st.com/resource/en/datasheet/lsm303agr.pdf),
not a physical sensor capture. Implementation tests and the source-built ARM
guest must establish the actually supported subset before a pass is recorded.

## Deliberate boundaries

The shared open-drain sensor interrupt on P0.25 is not wired or qualified.
FIFO, gesture/click/orientation detection, filters, self-test, temperature,
physical calibration, noise and real-world motion dynamics are unsupported.
Full CODAL/MakeCode sensor firmware, browser-WASM sensor performance, and
continuous microphone/speaker integration are not qualified by this slice.

Full hosted functional qualification passed at `dd4e5754` in
[run 36683869753](https://github.com/CrispStrobe/labwired-core/actions/runs/36683869753),
including guest transactions and physical input conversions. The active motion
throughput gate **failed**: median 0.326908x, minimum 0.322135x, versus 2.340300x
for the GPIO-only workload in that same job. The
[complete failing receipt](../receipts/2026-09-30-microbit-motion-hosted-baseline.json)
retains all five observations and source/executable provenance. Real-time
qualification was not met by that baseline; identities or
passing functional tests alone do not establish it.
The historical tier-1 I²C test remains an absent-address NACK scenario, not a
motion sensor proof. No silicon bench comparison is claimed, and CP13 remains
incomplete until its remaining sensor/audio/browser checkpoints are met.

## Executable qualification

The opt-in `microbit_v2_motion_io_guest` test builds `board-io.S` with
`MICROBIT_MOTION_IO`, including `motion-polled.inc` and `board-io.ld`, using
`arm-none-eabi-gcc`. No downloaded vendor image is needed:

```sh
cargo test --release -p labwired-core --features microbit-board-io-test --test microbit_v2_motion_io_guest
```

The guest uses TWIM0's real DMA buffers in its own RAM, reads both identities,
configures normal-mode acceleration and continuous magnetic sampling at
100 Hz, and polls data-ready before six-byte XYZ bursts. Two held physical
poses must yield the expected signed samples through `Machine::set_inputs`.
Guest errors, stale sample counts, incorrect DMA amounts, incorrect button
masks and any missing/extra matrix pixel fail the proof. The magnetometer
setup enables the datasheet-required temperature-compensation bit; thermal
behavior itself remains outside the functional model.

The ignored release benchmark warms up for 8 million steps and measures five
64-million-step windows, recording actual simulated cycles and wall time.
Each window changes the physical pose/buttons and checks continued sensor
sampling and matrix scanning. `LABWIRED_REQUIRE_REALTIME=1` makes a median
below 1.0x fail. `.github/workflows/microbit-board-io.yml` runs this gate and
uploads the raw log, validated receipt and the exact source-built ELF.
`scripts/perf/microbit_motion_report.py` checks the observations and hashes
all three guest sources plus compiler flags; it never invents measurements.

Profiling the actual four-million-step functional test with Callgrind attributed
over 60% of host instruction work to `run_t16_fast_block` and its inlined
operations: repeated discovery around T32 loads and unsupported CBNZ poll
instructions. The candidate optimization rejects impossible current entries
and stops backward discovery at structural barriers. That first version did not cache misses,
skip guest instructions, alter TWIM wire latency, lower polling frequency or
weaken the benchmark. Mixed-width, cold-cache and rotated-entry regressions and
a fresh hosted active-motion measurement qualifies only this bounded native
workload, as recorded below, before merge.

### First optimization: instruction work versus host throughput

The native four-million-step functional proof was profiled before and after
the first discovery optimization. Callgrind reported **2,558,205,555** host
instructions (`Ir`) for the baseline and **1,375,692,554** for the candidate,
a **46.2% reduction** in total recorded instruction work. These counts include
configuration/YAML setup and guest startup, not just the steady-state loop.
They describe host execution work under instrumentation, not simulated ARM
instruction counts, wall-clock RTx, or a silicon timing improvement. Both
runs used the same functional-test invocation and preserved its assertions.

The first candidate runtime `e32b4a3557da3a1ac5c1391c3eff11ad544e655a`
also completed the full five-window benchmark on the shared VPS. Its
[complete optimized VPS receipt](../receipts/2026-09-30-microbit-motion-vps-optimized.json)
records median **0.31243952566997163x**, minimum **0.288592214098303x**, and
`realtimeTargetMet: false`. All sensor conversions, DMA amounts, sample/scan
progress, matrix pixels and alternating button assertions still passed. GNU
time measured 16.07 seconds elapsed, 12.63 seconds user, 0.41 seconds system,
and 81% CPU for the complete invocation, including setup/warmup; it is not the
benchmark's measured-window timing alone.

The [VPS baseline receipt](../receipts/2026-09-30-microbit-motion-vps-baseline.json)
at `2a916120` and this candidate share guest source-bundle SHA-256
`e6b8c239dc7ee1aca736671350cda8101f4bf8d6c89d91e2a9e787c85b553939`
and the same output assertions. Host contention was not controlled, so these
VPS wall times are **not a controlled performance A/B** and do not establish
stable >=1.0x throughput. In particular, lower Callgrind work must not be
reported as a proportional wall-clock speedup.

The first candidate hosted run was cancelled by a subsequent documentation
push; cancellation is neither a pass nor a measured failure. The replacement
[hosted run 36687935898](https://github.com/CrispStrobe/labwired-core/actions/runs/36687935898)
**passed** its functional and real-time gates. Five active-motion samples
measured median **1.7582140503340078x**, minimum **1.7339540271462885x**;
every sample exceeded 1.0x. The separate GPIO-only guest in that same job
measured **5.531283760017577x median**, not a full sensor-workload result.
Both [motion](../receipts/2026-09-30-microbit-motion-hosted-optimized.json) and
[GPIO-only](../receipts/2026-09-30-microbit-active-hosted-motion-optimization.json)
receipts preserve all observations and original hashes.

Their recorded commit `199af713794f9b2135f931bced3835203882bd76` is GitHub's
tested pull-request merge-ref, not a main-branch merge. PR head `3c831043`
contains documentation-only changes after runtime source `e32b4a35`.
This first optimization is **hosted-qualified and landed upstream** in
[PR 129](https://github.com/CrispStrobe/labwired-core/pull/129), main merge
`ce60a49941f9fa94d83aca6859bc27ae1c5b9e0b`. Documentation-only landing head
`d05043dd` changed no runtime code after `e32b4a35`. Historical receipt
annotations retain the pre-merge status at measurement time; no measurement
is relabeled as a post-merge run. This landing does not qualify sensor IRQs,
ADC/audio, browser WASM, or arbitrary applications, and does not itself update
Brickwright Lite's WASM package pin.
CP13 remains incomplete.

### Second optimization: lazy payload candidate, hosted-qualified before landing

The follow-up borrows a valid cached block until execution actually needs its
payload and validates a proposed instruction window before constructing its
operation array. Invalid discovery attempts therefore avoid copying or filling
large `Option<T16Block>` payloads. The cached-block-first behavior, instruction
admission, rotated entries, MMIO fallbacks and scheduler boundaries remain
unchanged. Eleven focused CPU regressions cover these paths, including forward
or wrong-target terminal branches, oversized windows and invalid cache tags.

The actual four-million-step ARM motion guest passed its functional assertions
locally with candidate source `dbc248c4a6f2e812fe293051250ef0b7a628654a`.
Callgrind recorded **1,300,205,720 Ir**, versus **1,375,692,554 Ir** for the
first optimization and **2,558,205,555 Ir** before either optimization. This is
approximately **5.5% less total host instruction work** than the first
optimization, including configuration/YAML setup and startup. It is not a
controlled wall-clock speedup or a new real-time qualification.

The [complete lazy-candidate VPS receipt](../receipts/2026-09-30-microbit-motion-vps-lazy.json)
preserves that original candidate commit, rather than relabeling it with the
follow-up branch's cherry-picked commit. The unchanged guest source-bundle hash
is `e6b8c239dc7ee1aca736671350cda8101f4bf8d6c89d91e2a9e787c85b553939`.
All five windows passed the functional checks, but median throughput was
**0.3197440597674396x**, minimum **0.27542579185194027x**, and
`realtimeTargetMet` is **false**. GNU time recorded 17.45 seconds elapsed,
10.81 seconds user, 0.36 seconds system and 64% CPU for the complete invocation,
including setup/warmup; that CPU figure is not steady-window utilization.
These shared-VPS wall times are not a controlled A/B against the earlier runs.

The independent [hosted lazy-candidate run 36691941435](https://github.com/CrispStrobe/labwired-core/actions/runs/36691941435)
passed the functional/model/DMA checks, **all eleven CPU regressions**, WASM
routing checks and both native throughput gates. Its complete
[motion receipt](../receipts/2026-09-30-microbit-motion-hosted-lazy.json) records
median **1.1265547370697286x**, minimum **1.1230014365962186x**, with all five
samples >=1.0x. Its separate [GPIO-only receipt](../receipts/2026-09-30-microbit-active-hosted-lazy.json)
records median **3.7836838863482463x**. Both retain tested merge-ref
`9e4e5f83586f7c94bc989a44399821401078ab37`, PR head
`143402d6c69a501647d71111597bb00d408dcda6` with runtime source
`ab501cdf140e51fd129245939b322b1177f0de0e`. The eleven-test qualification is
hosted, not a claim that all eleven unit tests were executed locally.

For context, [main qualification run 36690708740](https://github.com/CrispStrobe/labwired-core/actions/runs/36690708740)
tested the first optimization's actual main commit
`ce60a49941f9fa94d83aca6859bc27ae1c5b9e0b` and passed its eight CPU tests,
functional/WASM checks and native gates. Its complete
[motion receipt](../receipts/2026-09-30-microbit-motion-hosted-main-qualified.json)
records median **1.0306447307567719x**, minimum **1.0270104540205454x**;
the [GPIO-only receipt](../receipts/2026-09-30-microbit-active-hosted-main-qualified.json)
records median **3.355509706840291x**. These different hosted runs are **not a
controlled wall-clock A/B**; their ratios do not establish a proportional
speedup from the second change.

The lazy optimization subsequently landed in [PR 131](https://github.com/CrispStrobe/labwired-core/pull/131)
at main `3456c048894f194bbabc9c414932a752d89da999`. Its rebase preserved
qualified CPU/guest sources; only the main branch's SAADC unit-test comment
formatting repair and documentation/digests changed. However, the exact-main
[qualification run 36694881019](https://github.com/CrispStrobe/labwired-core/actions/runs/36694881019)
**failed the unchanged motion real-time gate**: the [complete failed-main receipt](../receipts/2026-09-30-microbit-motion-main-lazy-failure.json)
records median **0.8898152593409774x**, minimum **0.8871067576500745x**,
with all five windows below 1.0x despite passing functional
checks. The earlier successful PR result is not a stable main-branch >=1.0x
claim. Different runner timings remain incomparable without controlled A/B.

### Third optimization: generation-scoped structural discovery misses

The new source candidate memoizes only unsuccessful **structural block
discovery**, in a bounded 64-slot PC/generation table (1 KiB). It is not a
permanent cold-cache rejection: every ordinary or fast-path decode insertion,
including a tag collision, advances the generation. Reset, explicit code-cache
invalidation and snapshot restore invalidate the memo too; generation wrap
physically clears all slots before generation one is reused. Exact PC tags
prevent memo-slot aliases from matching unrelated instructions.

The existing positive block cache is considered first. RAM/MMIO execution
failures, register-dependent addresses and guest-visible work are never cached
as discovery misses. Guest instruction budgets, MMIO latency, polling frequency,
model clock, scheduler and observer routing remain unchanged. Raw mutable
decode-cache access is now private, with public read-only `decoded_entry(pc)`
inspection; external code edits use the existing `Cpu::invalidate_code_caches`
contract. That invalidation now also drops a positive fast-block cache, fixing
stale blocks after debugger edits.

Eighteen focused discovery regressions cover cold-cache warm-up through the real
decoder, decoding-disabled/enabled transitions, collision and malformed-window
repair, reset/invalidation/snapshot code patches, epoch wrap, positive-cache-first
handling, exact budgets, and MMIO-to-RAM-to-MMIO effective-address changes.
Formatting, generated validation/drift and report-parser checks are local;
the Rust tests and unchanged actual ARM motion benchmark await hosted
qualification. The full CorePerf gate also remains required; Nordic step-cost
regressions in main are not fixed or waived by this batch-discovery change.

The first [same-runner B/C/C/B run](https://github.com/CrispStrobe/labwired-core/actions/runs/36712453588)
retained complete raw logs, but its receipt parser rejected a Rust pretty-test
header prefix on the payload hash. The raw candidate medians were 0.997368x
and 1.014610x, versus baseline 0.879165x and 0.890832x. The first candidate
still failed the strict >=1x gate: neither an aggregate median nor fixing the
output parser makes this a passing qualification. Future invocations use the
terse test format; recovered historical receipts must disclose normalization.
The [original failed workflow summary](../receipts/2026-09-30-microbit-motion-ab-36712453588/summary.json)
and [recovered full 20-sample receipts](../receipts/2026-09-30-microbit-motion-ab-36712453588/recovered/summary-recovered.json)
retain raw logs, fingerprint/source provenance and explicit normalization;
[qualification context](../receipts/2026-09-30-microbit-motion-ab-36712453588/qualification-context.json)
keeps tested merge-ref `b7bdb4f8` and old candidate head `088f89a5` distinct.

The same old candidate passed the separate
[native qualification run 36712453524](https://github.com/CrispStrobe/labwired-core/actions/runs/36712453524):
motion median 1.3132537051929918x, minimum 1.2985379190213056x, with all
18 CPU and guest/model/DMA/input-routing checks passing. Its
[full native receipt](../receipts/2026-09-30-microbit-discovery-native-36712453524/microbit-motion-throughput.json)
and [runner context](../receipts/2026-09-30-microbit-discovery-native-36712453524/microbit-runner-context.txt)
record an Intel Xeon Platinum 8573C, whereas the
[paired A/B fingerprint](../receipts/2026-09-30-microbit-motion-ab-36712453588/runner-context.json)
records AMD EPYC 7763. These are separate runner observations, not a controlled
cross-machine speedup or proof hardware alone caused the difference. The old
candidate still fails the AMD paired strict gate; neither result promotes
current main or qualifies the later combined GPIO candidate.

The next combined candidate also caches Nordic pull-configuration masks at
valid PIN_CNF writes (eight derived bytes per port). IN reads retain the same
direction/external-drive/latch decisions without a repeated per-pin scan.
Seven additional loop-reference, bank-size, subword-write and snapshot-schema
regressions cover this optimization. The unchanged motion guest and all-chip
performance gates must qualify the combined revision before landing; no new
real-time result is claimed for it yet.

None of these results is an actual browser-WASM performance measurement, a
package pin update, sensor IRQ or ADC/audio qualification, or wider CP13
completion.
