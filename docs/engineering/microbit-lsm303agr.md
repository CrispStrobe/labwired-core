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
and stops backward discovery at structural barriers. It does not cache misses,
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

The lazy candidate is **hosted-qualified, not yet landed** at this documentation
checkpoint. Its subsequent rebase includes only the main branch's SAADC unit
test comment formatting repair plus documentation/digest changes; CPU/runtime
and guest sources remain identical to the qualified candidate. This is not an
actual browser-WASM performance measurement, a package pin update, sensor IRQ
or ADC/audio qualification, or wider CP13 completion.
