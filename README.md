# LabWired Core — CrispStrobe fork

This is the [CrispStrobe](https://github.com/CrispStrobe) fork of
[w1ne/labwired-core](https://github.com/w1ne/labwired-core), the deterministic
firmware simulator behind [labwired.com](https://labwired.com/). We use it as
one "real silicon" engine of [Brickwright-lite](https://github.com/CrispStrobe/brickwright-lite),
compiled to WebAssembly and driven from a circuit designer and a debugger in
the browser and in the Tauri app.

The upstream project and its documentation are the reference for LabWired
itself; a shortened copy of the upstream README follows [below](#upstream-labwired-core).
This first part covers only the fork: what we did, how Brickwright uses it,
and where we still differ from upstream.

**Status (2026-11-08):** `main` is upstream `main` plus the fork's not yet
upstreamed work ([compare](https://github.com/w1ne/labwired-core/compare/main...CrispStrobe:labwired-core:main)),
0 commits behind. Fork CI (Rust Core CI, Core Perf, Board CI, coverage,
Arduino matrix, QEMU ARM, micro:bit qualification, IO-Link, docs) is green.

- [How Brickwright uses LabWired](#how-brickwright-uses-labwired)
- [What we did, from start to today](#what-we-did-from-start-to-today)
- [What upstream has taken over](#what-upstream-has-taken-over)
- [Where we still differ from upstream](#where-we-still-differ-from-upstream)
- [Keeping in sync with upstream](#keeping-in-sync-with-upstream)
- [Working on the fork](#working-on-the-fork)
- [Fork documents](#fork-documents)

## How Brickwright uses LabWired

```
labwired-core (this fork, Rust) --cargo build -p labwired-wasm--> labwired-wasm release (sha256-pinned)
        |                                  built and published by bw-board
        v
bw-board        board layer: adapter, netlist -> manifest bridge, chip/board catalog, debug targets
        |       npm dependency, pinned
        v
brickwright-lite  Scratch/blocks <-> code studio (web + Tauri); lazy-loads the engine; debugger UI
        |
bw-circuit-ui   circuit designer faces: LED matrix, framebuffer, touch, from device state
```

**Build.** [bw-board/scripts/build-labwired-wasm.mjs](https://github.com/CrispStrobe/bw-board/blob/master/scripts/build-labwired-wasm.mjs)
clones this fork at a pinned SHA and runs
`cargo build --release --target wasm32-unknown-unknown -p labwired-wasm`
(`crates/wasm`) plus `wasm-bindgen`, for a `web/` and a `nodejs/` glue. The
build is reproducible: bw-board's CI builds twice and compares the bytes.
Releases are published on bw-board, for example
[labwired-wasm-3b655aef-locked](https://github.com/CrispStrobe/bw-board/releases/tag/labwired-wasm-3b655aef-locked)
and [labwired-wasm-9c420649](https://github.com/CrispStrobe/bw-board/releases/tag/labwired-wasm-9c420649).

**Loading.** Brickwright-lite fetches the engine with
[scripts/sync-labwired-wasm.mjs](https://github.com/CrispStrobe/brickwright-lite/blob/main/scripts/sync-labwired-wasm.mjs),
which writes nothing unless every sha256 matches. The ~20 MB engine is never
committed and never in the eager bundle:
[labwired-engine.js](https://github.com/CrispStrobe/brickwright-lite/blob/main/packages/scratch-gui/src/lib/labwired-engine.js)
loads it on demand through a dynamic import, and a bundle gate checks that.
A build without the engine still compiles and simply offers no heavy tier.

**Tiers.** Brickwright has a light tier (bw-board's own JavaScript Cortex-M0
machine for its STM32F030 blocks-to-C target) and a heavy tier, LabWired, for
foreign firmware and real boards: MakeCode, CODAL, Arduino and ESP-IDF images,
STM32F1/F4, nRF5x, SAMD51, RISC-V and Xtensa. Other targets in Brickwright use
their own engines (avr8js, rp2040js, MicroPython WASM, the PXT simulators,
a Renode-derived boundary for LEGO SPIKE Prime).

**The pad is the boundary.** bw-board generates the LabWired system manifest
from the circuit netlist, never the other way round
([LABWIRED-BRIDGE.md](https://github.com/CrispStrobe/bw-board/blob/master/LABWIRED-BRIDGE.md)).
LabWired owns the silicon (registers, peripherals, pad direction and drive);
bw-board's circuit solver owns everything off-chip (resistors, LEDs,
dividers, rails). Every header pin becomes a `board_io` binding, no
`external_devices` are emitted, outputs come back as cycle-stamped edges
(`watch_logic_signals` / `read_logic_edges`), inputs go in through
`set_board_io_input` and the engineering-input API (pots to ADC channels,
motion poses). Pads the engine cannot carry end up in a refusal list that
Brickwright shows to the user instead of silently ignoring them.

**Boards and features used through LabWired**

| Board / chip | What it does in Brickwright |
|---|---|
| STM32F030 circuit benches | optional heavy-tier engine beside the light tier; same nets, UART and edges on both |
| BBC micro:bit V2 (nRF52833), V1 / Calliope mini (nRF51822) | run MakeCode `.hex` in the debugger; 5x5 LED matrix on the circuit face; buttons; SAADC inputs; LSM303AGR motion; serial |
| Adafruit PyBadge (ATSAMD51J19) | debug Arcade UF2/ELF on the exact board; a native CODAL game on the explicit no-ASF SAMD profile with ST7735 display and buttons |
| ESP32 / ESP32-C3 / ESP32-S3 | Arduino-ESP32 sketches with LEDC PWM on real pads; firmware-only ELF debugging |
| ESP32-S3 4848S040 | 480x480 RGB panel and GT911 touch on a display dock (engine done, Lite adoption in progress) |
| Arduino Uno (ATmega328P) | comparison against avr8js, which stays the shipped AVR path |
| Firmware-only catalog | STM32F103/F401/F407/L476/G474, nRF51/52, RP2040/RP2350, SAMD21/51, RA4M1, ESP32/C3/C6/S3 ELF images in the debugger |

**Pins in use.** bw-board builds from `9c420649` (ESP32-4848S040), Brickwright-lite
`main` from `3b655aef` (SAMD native game). Both commits come from the fork's
history before the 2026-11 re-sync; they stay reachable through the tags
`brickwright/bw-board-pin-9c420649` and `brickwright/lite-pin-3b655aef` and the
branch `backup/pre-upstream-sync-20261107`. Do not delete those refs while a
consumer still pins them. The next engine release should be cut from the
current `main`.

## What we did, from start to today

| When (2026) | What |
|---|---|
| Aug 25 | fork created; first WASM spike builds unchanged (2.1 MB brotli); bw-board adapter and STM32 path |
| Aug 27 | reproducible WASM build in bw-board; Brickwright-lite loads LabWired as a lazy heavy tier |
| Aug 30 | level-sensitive NVIC pending follows the line down, ADC held-channel injection exported to the WASM bridge (upstreamed as w1ne#1073) |
| Sep 20-28 | performance programme for real time in the browser: hot Thumb loop caching, walk-free scheduling, bus dispatch caching, Xtensa and RISC-V fast paths, dual-core batch boundaries; AVR and PyBadge targets |
| Sep 29-30 | micro:bit V2: GPIOTE owns its pad, `led_matrix` primitive, SAADC scan, LSM303AGR motion through a source-built TWIM guest, Nordic GPIO and countdown fast paths |
| Oct 1 | WASM-specific dispatch: cached T16 runs, opcode-directed fast paths, no block payload copies |
| Oct 5-6 | nRF52 PWM drives exact-cycle edges onto pads; PC-relative `VLDR`/`LDRD` alignment fix found in a real MakeCode image (upstreamed as w1ne#1324) |
| Oct 7-13 | PyBadge: ST7735 panel controls, SAMD SERCOM SPI with split IRQs, byte DMA; ESP32 LEDC outputs drive their pads through the GPIO matrix |
| Oct 15-19 | nRF52 UARTE buffer lifetime, Tier1 UART fixture repair, SAMD51 manual flash backing, PR performance guard |
| Oct 24-27 | SAMD51 USB device banks, TCC/ADC models, I2C timing, DFLL USB clock recovery; the explicit no-ASF SAMD simulation assembly with checked memory-only chunks (from 0.02x to about 4.4x real time for the native game) |
| Oct 29 | micro:bit shared sensor lines: opt-in open-drain outputs, autonomous data-ready events, first-edge GPIOTE |
| Nov 1 | ESP32-S3 / ESP32-4848S040: LCD_CAM RGB scan-out through GDMA, GT911 touch |
| Nov 7-8 | re-sync onto upstream main (see below); fixes found by the merge: S3 GPIO bank-1 pads, duplicate ROM thunk, ESP32 LEDC duty latching |

Each change landed through a pull request with the fork's CI as the gate.
The detailed records are in the [fork documents](#fork-documents) and under
`docs/receipts/`.

## What upstream has taken over

Upstream merged 13 of our pull requests directly (w1ne#1073, #1224, #1225,
#1226, #1241, #1246, #1260, #1261, #1265, #1266, #1281, #1324, #1328: NVIC level
latch and ADC injection, ESP32/S3/RISC-V performance, browser ROM images,
nRF GPIOTE and the `led_matrix` primitive, the `VLDR`/`LDRD` fix, batch end on
due events). It also re-implemented or ported a larger share under its own
commits, among them the SAMD51 board models, SERCOM timing, USB device and
DMAC (w1ne#1510, #1512, #1516), ESP32-C3/S3 LEDC pads (w1ne#1509), the Tier1
UART fixtures (w1ne#1508) and the SAMD51 flash and PR perf gate (w1ne#1487).
Of 641 fork commits before the re-sync, about 400 were fully present upstream
by content.

## Where we still differ from upstream

Kept in this fork because upstream does not have it (yet):

- **SAMD native game profile**: the explicit no-ASF SAMD simulation assembly
  (`samd51-no-asf-simulation`) and the checked quiescent memory-only chunks that
  let it run in real time, with their own tests and perf baselines. Costs about
  2 % in single-step mode on other Cortex-M boards; see
  [the re-sync note](docs/engineering/upstream-sync-2026-11.md#performance).
- **SAMD51 extras**: DMAC latency, DFLL USB clock recovery, SPI MISO pads, CMCC
  board probe, USB bank details, flash and reset qualification guests.
- **micro:bit board I/O**: opt-in open-drain device outputs with retained
  pull-ups, autonomous sensor data-ready events, shared-line TWIM controls,
  first GPIOTE event detection, and their guest qualifications.
- **ESP32-4848S040**: LCD_CAM RGB scan-out over GDMA and GT911 touch
  ([board doc](docs/boards/esp32-4848s040.md)).
- **WFI parking**: a WFI core stays parked while SAMD bus service refuses an
  idle skip, and checks shared NVIC wake eligibility first.
- **Validation state**: owner-approved drift acknowledgements (2026-11-08,
  expire 2026-12-08) for nine hardware-captured boards whose watched model
  files the fork changes. These are not silicon captures.
- **Perf baselines** re-locked for the fork (see
  [Core Perf fork rules](docs/performance/2026-10-15-core-perf-fork-rules.md)).
- **Records**: engineering notes, qualification receipts and lane handoffs
  under `docs/engineering/`, `docs/performance/` and `docs/receipts/`.

## Keeping in sync with upstream

Upstream regularly takes our work and re-lands it in its own form, so GitHub's
"ahead/behind" counts do not show what is really missing on either side.
In November 2026 we therefore rebuilt `main` once as upstream `main` plus only
the fork commits whose content upstream does not have, verified against a
fully tested reference merge
([method and decisions](docs/engineering/upstream-sync-2026-11.md)).

From now on:

- New upstream commits are merged into `main` with a normal merge commit.
- Where upstream ships a feature we also have, upstream's implementation wins
  and ours is dropped, unless ours is proven better by a test.
- Fork-only entries in shared files (perf baselines, runnable-chip lists,
  bus-proof rows, drift acks) must survive every sync; they are the usual
  casualties of taking upstream's copy of a file.
- Work meant for upstream goes as a focused pull request to w1ne/labwired-core.

## Working on the fork

Build, test and lint as upstream does (see [docs/agents.md](docs/agents.md)
and [CONTRIBUTING.md](CONTRIBUTING.md)). Before pushing to `main`, run what
fork CI's `core-integrity` job runs:

```sh
python3 scripts/generate_validation_status.py --check --drift
python3 scripts/generate_firmware_exercise_matrix.py --check
python3 scripts/generate_model_ratings.py --check
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy -p labwired-core --all-targets --features event-scheduler -- -D warnings
```

`scripts/regen-all.sh` refreshes all generated docs (needs Python with PyYAML).
A change to a watched model file moves the drift digests of hardware-captured
boards; only the owner may acknowledge that (dated `drift_ack`, then
`--write-ack-digests`). Perf baselines are only ever re-locked from CI numbers.

## Fork documents

- [Upstream re-sync, 2026-11](docs/engineering/upstream-sync-2026-11.md)
- [Core Perf fork rules](docs/performance/2026-10-15-core-perf-fork-rules.md)
- [SAMD native simulation bundle](docs/engineering/samd-native-simulation-bundle.md) and
  [SAMD software-profile handoff](docs/engineering/samd-software-profile-handoff.md)
- [SAMD51 board models](docs/engineering/samd51-board-models.md),
  [SAMD51 USB device banks](docs/engineering/samd51-usb-device-banks.md)
- [micro:bit composed qualification](docs/engineering/microbit-composed-qualification.md),
  [micro:bit LSM303AGR](docs/engineering/microbit-lsm303agr.md)
- [ESP32-4848S040](docs/boards/esp32-4848s040.md),
  [PyBadge native](docs/boards/pybadge-native.md)
- [Board completion lanes (historical)](docs/engineering/target-next-lanes.md)
- Brickwright side: [LABWIRED-BRIDGE.md](https://github.com/CrispStrobe/bw-board/blob/master/LABWIRED-BRIDGE.md),
  [SAMD native game adoption](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/SAMD-NATIVE-GAME-ADOPTION.md),
  [target emulator performance](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/TARGET-EMULATOR-PERFORMANCE.md)

---

# Upstream LabWired Core

<a href="https://labwired.com">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/w1ne/labwired-core/main/docs/assets/brand/labwired-logo-dark.png">
    <img alt="LabWired" src="https://raw.githubusercontent.com/w1ne/labwired-core/main/docs/assets/brand/labwired-logo.png" width="300">
  </picture>
</a>

Shortened from the [upstream README](https://github.com/w1ne/labwired-core#readme),
which is the current and complete version.

Run your firmware on a virtual instance of a real chip, from your terminal, your CI, or your
AI coding agent. No board on your desk.

LabWired Core loads a firmware ELF and executes it against modeled silicon: CPU, buses,
peripherals, sensors, displays, and protocol devices. You get UART output, GPIO and bus
traces, register state, and a pass/fail exit code. Runs are deterministic: the same ELF and
the same board manifest give the same trace on every machine.

Supported cores: ARM Cortex-M0+, M3, M4, M7, M33; RISC-V; Xtensa LX6 and LX7 (selected
ESP32 paths). The hosted browser [Playground](https://app.labwired.com/) and the MCP
connector run the same models.


## Quickstart

Clone the repository, install the CLI, run a firmware. No cross-toolchain needed.

```sh
git clone https://github.com/w1ne/labwired-core && cd labwired-core
curl -fsSL https://labwired.com/install.sh | LABWIRED_VERSION=v0.25.0 sh
labwired test --script examples/nrf54l15-dk/io-smoke.yaml
```

```
nRF54L15 boot OK
core=cortex-m33 rram=1524K ram=256K
uarte20@0x500C6000 gpio2@0x50050400
regs from MDK/SVD, not nRF52
PASS  4/4 checks · io-smoke · 200000 steps · 0.04s
```

That is a committed bare-metal ELF booting on an nRF54L15-DK profile in under a second.
Nothing is compiled on your machine.

The four checks are the three UART lines plus the stop reason.

The firmware prints those lines from string literals. The text proves nothing by itself.
What it proves is that the bytes arrived. To send them, the firmware had to boot from RRAM
at `0x0`, set up RAM at `0x20000000`, and drive UARTE20 over EasyDMA. Get the UARTE base
wrong in the chip model and you get no output at all.

GPIO is checked at the pin, not in the banner, by
`firmware_survival::test_nrf54l15_lights_dk_led0`. The
[example README](examples/nrf54l15-dk/README.md) says why both tests are needed.

The verdict goes to stderr. The exit code is the gate: `0` passes, anything else fails.
UART output and `--json` stay on stdout, so pipes keep working.

## Installation

The install script covers Linux, macOS, and Windows via WSL2.

```sh
curl -fsSL https://labwired.com/install.sh | LABWIRED_VERSION=v0.25.0 sh
```

| Variable | Effect |
| --- | --- |
| `LABWIRED_VERSION=` | pin a release |
| `LABWIRED_INSTALL_DIR=` | choose the install directory |
| `LABWIRED_FROM_SOURCE=1` | build from source instead of downloading |

To read the installer before running it:

```sh
curl -fsSL https://labwired.com/install.sh -o install.sh   # review it, then:
sh install.sh
```

## Running LabWired

### From your terminal

```sh
labwired run  --chip configs/chips/<chip>.yaml --firmware path/to/firmware.elf
labwired test --script  path/to/test.yaml --junit report.xml
```

`run` is interactive: UART, GPIO, traces, snapshots.

`test` is the gate. It writes `result.json`, `uart.log`, and JUnit, and returns an exit code.
Assertions cover UART content, memory and register values, stop reasons, and step and
wall-time limits.

See the [CLI reference](docs/cli_reference.md) and the
[test runner reference](docs/ci_test_runner.md).

## Documentation

[Docs index](docs/index.md) ·
[Architecture overview](docs/architecture_overview.md) ·
[Engine architecture](docs/architecture.md) ·
[CLI reference](docs/cli_reference.md) ·
[Python SDK](docs/python-sdk.md) ·
[CI test runner](docs/ci_test_runner.md) ·
[Configuration reference](docs/configuration_reference.md) ·
[Board onboarding playbook](docs/board_onboarding_playbook.md) ·
[Target support rubric](docs/target_support_rubric.md) ·
[Debugging](docs/debugging.md) ·
[PlatformIO integration](docs/platformio_integration.md) ·
[Agents manual](docs/agents.md)

## Project links

[labwired.com](https://labwired.com/) ·
[Playground](https://app.labwired.com/) ·
[Docs](https://docs.labwired.com/) ·
[Validation](https://app.labwired.com/validation) ·
[For CI](https://labwired.com/ci.html) ·
[Blog](https://labwired.com/blog/) ·
[Pricing](https://labwired.com/pricing.html)

## License

MIT, see [LICENSE](LICENSE). LabWired Core is by Andrii Shylenko
([w1ne](https://github.com/w1ne)); the fork's additions are under the same licence.
