# NUCLEO-U575ZI-Q Validation Runbook

Run all commands from the repository root. Captured evidence below is from
**2026-09-18** on branch `feat/onboard-stm32u575` unless a command output says
otherwise.

Tier: **sim-validated** — all values are SVD / RM0456 / DS13736-derived. There
is no bench part; no silicon capture and no Renode differential is claimed.

## Prerequisites

1. Rust toolchain (pinned by `rust-toolchain.toml`) with
   `thumbv8m.main-none-eabi` installed.
2. `arm-none-eabi-gcc` 13.2 for the CubeU5 HAL firmware.
3. STM32CubeU5 checkout — see [`EXTERNAL_COMPONENTS.md`](EXTERNAL_COMPONENTS.md).
4. Zephyr 3.7.2 workspace at `~/zephyrproject` with the `gnuarmemb` toolchain
   (`ZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb GNUARMEMB_TOOLCHAIN_PATH=/usr`) for the
   Zephyr matrix.
5. PlatformIO for the Arduino matrix.

## A. Rust smoke + io-smoke

```bash
cargo build -p firmware-stm32u575-demo --release --target thumbv8m.main-none-eabi
cargo run -q -p labwired-cli -- test --script examples/nucleo-u575zi/io-smoke.yaml
```

Captured output (UART line then the test verdict):

```
OK
PASS  2/2 checks · io-smoke · 64 steps · 0.00s
```

`strict_onboarding` runs this same script for the chip; the crate is built from
source (no prebuilt asset).

## B. STM32CubeU5 HAL firmware

Checkout/revision: `12d19a5358da129dc74aecff1adee370218ca186` (HAL driver
`0e5fefb8dc2d6afa60816ebbf8b1672cfec4595b`, CMSIS device
`624374fa1e21ca195d6f2102ac0caaa50d0ea4c8`). The firmware does the full
Cube HAL flow: `HAL_Init` → ICACHE → MSIS/PLL1 160 MHz + FLASH latency 4 +
VOS1/SMPS → GPIO PC7 → USART1 PA9/PA10 → banner + blink loop.

```bash
make -C examples/nucleo-u575zi/board_firmware
RUST_LOG=off target/release/labwired \
  --firmware examples/nucleo-u575zi/board_firmware/build/u575_hal_smoke.elf \
  --system examples/nucleo-u575zi/system.yaml --max-steps 20000000
```

Captured output (20M steps ≈ 0.14 s simulated; one blink = 250 ms ≈ 36M steps):

```
U575-HAL OK
BLINK 0 LD1=1
```

At `--max-steps 50000000`:

```
U575-HAL OK
BLINK 0 LD1=1
BLINK 1 LD1=0
```

## C. Determinism

Two runs, stdout only (`RUST_LOG=off`; stderr carries progress/timing logs), then
diff:

```bash
RUST_LOG=off target/release/labwired \
  --firmware examples/nucleo-u575zi/board_firmware/build/u575_hal_smoke.elf \
  --system examples/nucleo-u575zi/system.yaml --max-steps 20000000 > out/u575/hal-20m-run1.txt
RUST_LOG=off target/release/labwired \
  --firmware examples/nucleo-u575zi/board_firmware/build/u575_hal_smoke.elf \
  --system examples/nucleo-u575zi/system.yaml --max-steps 20000000 > out/u575/hal-20m-run2.txt
diff out/u575/hal-20m-run1.txt out/u575/hal-20m-run2.txt && echo DETERMINISTIC
```

Captured output: `DETERMINISTIC` (byte-identical UART stream).

## D. Unsupported-instruction audit

```bash
./scripts/unsupported_instruction_audit.sh \
  --firmware examples/nucleo-u575zi/board_firmware/build/u575_hal_smoke.elf \
  --system configs/systems/nucleo-u575zi.yaml \
  --max-steps 200000 \
  --out-dir out/unsupported-audit/nucleo-u575zi
```

Captured summary:

```
Audit summary:
  unknown_thumb16: 0
  unhandled_thumb32: 0
  unknown_riscv: 0
  unsupported_total: 0
  report: out/unsupported-audit/nucleo-u575zi/report.md
```

## E. Arduino matrix (fidelity engine)

```bash
cargo build -p labwired-cli --release
python3 validation/arduino-matrix/run_matrix.py --boards stm32u575
```

Captured result: **7 pass / 2 skip / 0 fail**, with cache-hit builds:

| Level | Result | Marker |
|-------|--------|--------|
| L0_serial_boot | ✅ pass | `LW_L0_OK` |
| L1_serial_loop | ✅ pass | `LW_L1_OK` |
| L2_blink_serial | ✅ pass | `LW_L2_OK` (GPIO edges on `gpioc:7`) |
| L3_i2c_sensor | ✅ pass | `LW_L3_OK` — INA219 exact tier, no `LW_L3_PARTIAL_NO_RX` |
| L4_spi_sensor | ✅ pass | `LW_L4_OK` — exact MAX31855 frame `0x01901600` |
| L5_adc | ⏭️ skip | ADC1 uses the closest (`stm32h7`) profile; U5 `RES[3:2]` delta documented, `analogRead` unproven |
| L6_pwm | ✅ pass | `LW_L6_OK` |
| L7_timer | ✅ pass | `LW_L7_OK` |
| L8_can | ⏭️ skip | FDCAN not declared in the U5 chip yaml (first pass) |

Per-level `uart.log`/`result.json` under
`validation/arduino-matrix/out/stm32u575/<level>/run/`.

## F. Zephyr matrix (fidelity engine)

Stock Zephyr 3.7.2 (`c66235fb7346bbe3dbedd1dd76ec5a37a8e8262b`), board
`nucleo_u575zi_q`:

```bash
PATH="$HOME/zephyrproject/.venv/bin:$PATH" \
  python3 validation/zephyr-matrix/run_matrix.py --boards stm32u575 --no-build
```

Captured result: **4/4 pass** — `LW_Z0_OK`, `LW_Z1_OK`, `LW_Z2_OK`, `LW_Z3_OK`,
each preceded by `*** Booting Zephyr OS build c66235fb7346 ***`. Per-level
outputs under `validation/zephyr-matrix/out/stm32u575/<level>/run/`.

## G. PR-gate survival fixtures

Committed fixtures (hard `assert!` on presence — absence fails, never skips):

| Fixture | sha256 | Source |
|---------|--------|--------|
| `tests/fixtures/stm32u575-zephyr-hello.elf` | `926bb5179504558c940bde30825030070775a20091a5cd9a287a125e4bbfe128` | stock Zephyr `samples/hello_world` @ `c66235fb7346` for `nucleo_u575zi_q` |
| `tests/fixtures/stm32u575-arduino-serial.elf` | `e4d620734c1b8bebd091cf17d6e87decdaaa6154f35fc5b64d2425a1419255a9` | Arduino matrix L0 (`PlatformIO`, `nucleo_u575zi_q`) |

```bash
cargo test -p labwired-core --test firmware_survival stm32u575 -- --nocapture
```

Captured output:

```
test test_stm32u575_arduino_serial_survival ... ok
test test_stm32u575_zephyr_survival ... ok
test result: ok. 2 passed; 0 failed
```

The Zephyr case asserts `Hello World! nucleo_u575zi_q` (the full stock line is
`Hello World! nucleo_u575zi_q/stm32u575xx`) on a real `attach_uart_tx_sink`
capture; the Arduino case asserts the `LW_L0_OK` marker and exercises the
Cube-startup `CRC->POL` write plus the U5 CRS register surface.

`validation/bus_proof_matrix.json` carries the U575 row (`uart`/`i2c`/`spi` all
`proven`), gated by `cargo test -p labwired-core --lib bus_proof_matrix`:

```
stm32u575        -       proven   proven   proven
```

## H. I2C L3 negative control (kit present vs absent)

Same stock Zephyr L3 ELF, same 15M step budget, only the system manifest
changes. The kit-free system (`examples/nucleo-u575zi/system.yaml`) declares
`external_devices: []`; the matrix system
(`validation/zephyr-matrix/systems/stm32u575.yaml`) attaches the INA219 at
`0x40` on `i2c1`.

```bash
# NEGATIVE — no device on the bus
target/release/labwired \
  --firmware validation/zephyr-matrix/out/stm32u575/L3_i2c_sensor/zephyr.elf \
  --system examples/nucleo-u575zi/system.yaml --max-steps 15000000
# → *** Booting Zephyr OS build c66235fb7346 ***
#   LW_Z3_BOOT
#   LW_Z3_FAIL err=-5        (Zephyr -EIO: the absent address NACKs)

# POSITIVE — INA219 @0x40 attached
target/release/labwired \
  --firmware validation/zephyr-matrix/out/stm32u575/L3_i2c_sensor/zephyr.elf \
  --system validation/zephyr-matrix/systems/stm32u575.yaml --max-steps 15000000
# → *** Booting Zephyr OS build c66235fb7346 ***
#   LW_Z3_BOOT
#   LW_Z3_OK
```

The negative control proves the L3 oracle is not vacuous: the identical
firmware/ELF fails when nothing answers the address and passes only with the
INA219 model attached.

## I. Manifest / generated docs

```bash
python3 scripts/generate_validation_status.py --check   # exit 0
```

`validation/manifest.yaml` carries the `stm32u575` entry (`tier: sim-validated`,
no `silicon:`) and the regenerated
[`docs/boards/VALIDATION_STATUS.md`](../../docs/boards/VALIDATION_STATUS.md)
renders it as "no silicon capture". `--check --drift` still reports the
**pre-existing** 7 silicon-board drifts (nrf52840, seeed-xiao-nrf52840-sense,
stm32h563, nucleo-l476rg, nucleo-l073rz, stm32f103, stm32f407) that predate this
change and are the maintainer's re-capture item; U575 is not among them.

## J. Scoreboards (partial-run policy)

Fleet scoreboards under `docs/coverage/` are published **only by full matrix
runs**. U575 was verified locally 2026-09-18; the fleet scoreboard refresh is
pending the next full run. The partial runs above wrote only
`validation/<matrix>/out/scoreboard.md` and left
`docs/coverage/arduino-scoreboard.md` / `docs/coverage/zephyr-scoreboard.md`
at their last full-run revisions (the Arduino runner printed
"Docs scoreboard unchanged (partial run; not full 18×9)").
