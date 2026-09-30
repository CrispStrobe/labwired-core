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

Current qualification is pending: attaching two components does not itself
prove guest transactions, timing, conversion fidelity, or real-time throughput.
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
