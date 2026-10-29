# micro:bit V2 LSM303AGR motion sensor

The micro:bit V2 carries an ST LSM303AGR on the internal I²C bus (TWIM0,
SCL P0.08 / SDA P0.16). It is two I²C slaves, and LabWired models each as a
declarative device:

| Component id    | Type              | Address | Descriptor |
|-----------------|-------------------|---------|------------|
| `accelerometer` | `lsm303agr_accel` | `0x19`  | [`lsm303agr_accel.yaml`](../../configs/devices/lsm303agr_accel.yaml) |
| `magnetometer`  | `lsm303agr_mag`   | `0x1E`  | [`lsm303agr_mag.yaml`](../../configs/devices/lsm303agr_mag.yaml) |

Both are attached in [`configs/systems/microbit-v2.yaml`](../../configs/systems/microbit-v2.yaml)
and [`examples/microbit-v2/system.yaml`](../../examples/microbit-v2/system.yaml).
The [Foundation I²C inventory](https://tech.microbit.org/hardware/i2c/) also
lists an FXOS8700 board variant; that variant is not selected here.

## Inputs

Each component takes live `x`, `y`, `z` stimulus: acceleration in g and
magnetic field in µT. They default to zero; no gravity or heading is invented.

## What is modelled

- Register maps, reset values and WHO_AM_I (`0x33` / `0x40`) per the
  [ST LSM303AGR datasheet, Rev 11](https://www.st.com/resource/en/datasheet/lsm303agr.pdf).
- Sub-address bit 7 (auto-increment) on both halves.
- Accelerometer output: left-justified 16-bit two's complement at the full
  scale `CTRL_REG4_A` FS[1:0] selects.
- Magnetometer output: 16-bit two's complement at 0.15 µT/LSB.

- Data-ready: each half raises its XYZ data-ready bits 10 ms (100 Hz) after
  it starts measuring or after its last output read, through the engine's
  `data_ready` rule. The accelerometer measures while `CTRL_REG1_A` ODR is
  non-zero; the magnetometer while `CFG_REG_A_M` MD[1] is clear (continuous or
  single). Polling firmware therefore reads about 100 samples a second, as on
  silicon, instead of one per poll.

## Not modelled

The configured ODR value (every rate paces at 100 Hz), a fixed sampling grid
(the period restarts from each output read), single-shot drop to idle,
overrun bits, block-data-update latching, resolution truncation of the low
bits, FIFO, acceleration and threshold interrupts,
click/orientation detection, filters, self-test, temperature and hard-iron
offset subtraction. Each descriptor header lists these.

## Executable proof

`microbit_v2_motion_io_guest` assembles an original polled driver
([`motion-polled.inc`](../../examples/microbit-v2/motion-polled.inc)) with
`arm-none-eabi-gcc`, runs it on the simulated nRF52833, and drives both halves
through TWIM0 EasyDMA. Two held poses must produce exact raw samples:

```sh
cargo test -p labwired-core --features microbit-board-io-test --test microbit_v2_motion_io_guest
```

## Magnetometer DRDY candidate

The optional `DRDY` output models native active-high data-ready. `CFG_REG_C_M`
bit 0 routes it, including enable after a completed conversion; clearing that
bit masks the output without acknowledging the sample. Status, threshold-source
and output low-byte reads leave DRDY asserted. Any output high-byte read
(`0x69`, `0x6B`, `0x6D`), including a standalone read, acknowledges it. This is
from [ST AN4825 Rev 1, section 3.3.3, page 15](https://www.st.com/resource/en/application_note/an4825-ultracompact-highperformance-ecompass-module-based-on-the-lsm303agr-stmicroelectronics.pdf).
`INT_CTRL_REG_M.IEA` configures the separate threshold signal, not DRDY.
The [datasheet Rev 11, section 8.39, Table 94](https://www.st.com/resource/en/datasheet/lsm303agr.pdf)
distinguishes continuous data-ready from single mode; section 8.41 distinguishes
bit 0 from the threshold route at bit 6.

The selected board routes this native output through the explicit inverting
collector to pulled-up P0.25. It follows the
[Foundation V2.0 schematic, sheet 2, U6 pin 7 and T7](https://github.com/microbit-foundation/microbit-v2-hardware/blob/main/V2.00/MicroBit_V2.0.0_S_schematic.PDF).
Acceleration and KL27 participants are still absent. The native sensor output
is not open drain. Digital inversion does not model analog transistor behavior.

This candidate is bounded to continuous 100 Hz, threshold routing off, and a
bus with an advancing device clock. Existing read-restarted 10 ms timing,
live output data, single-mode approximation and missing software reset/reboot
remain limitations. An idle master write cancels an unfinished conversion;
completed data remains until acknowledged under the existing engine contract.
This is not evidence for silicon power-down transients. CPU reset does not
reset the external sensor. Constructing a fresh part restores reset idle and
route-off state; no software-reset claim follows.

`microbit_v2_mag_drdy_guest` builds an original ARM driver that configures the
real TWIM0/EasyDMA model, performs route and acknowledgement transactions, and
observes P0.25 plus falling GPIOTE events. The host sends only RAM commands.
It covers masked completion, enable-after-ready, read-clear, remasking,
recurrence and idle cancellation in scheduler intervals 1 and 512. Hosted
execution is required before qualification; source presence alone is not proof.

Acceleration requires a separate acknowledgement prerequisite: AN4825 section
4.2.1, page 26 requires the high bytes of **all enabled axes** before clearing
DRDY. The current acceleration descriptor's any-axis status clear is an
approximation and must not be reused for its IRQ.
