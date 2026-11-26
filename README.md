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

Run unmodified ARM, RISC-V, and Xtensa firmware against modeled silicon, circuits, and boards — in your browser, terminal, CI, or AI coding agent.

LabWired Core loads a firmware ELF and executes it against modeled silicon: CPU, buses,
peripherals, sensors, displays, and protocol devices. You get UART output, GPIO and bus
traces, register state, and a pass/fail exit code. Runs are deterministic: the same ELF and
the same board manifest give the same trace on every machine.

Supported cores: ARM Cortex-M0+, M3, M4, M7, M33; RISC-V; Xtensa LX6 and LX7 (selected
ESP32 paths). The hosted browser [Playground](https://app.labwired.com/) and the MCP
connector run the same models.

Two design choices separate it from a bare CPU emulator:

1. **The board is modeled, not only the chip.** Sensors, displays, analog RC networks, and bus devices live in declarative YAML manifests (`configs/chips/`, `configs/systems/`). You assert on I²C traffic, SPI transfers, GPIO pin state, or framebuffer output.
2. **Thin spots are published.** Every target sits in a named validation tier (`Modeled`, `Smoke-tested`, `Hardware-compared`). Shortcuts are documented in [`FIDELITY.md`](FIDELITY.md).

For analog circuits and KiCad boards, see the [online circuit simulator guide](https://labwired.com/circuit-simulator/) (browser linear RLC + native `ngspice` co-simulation) and the [KiCad PCB import guide](https://labwired.com/kicad-pcb-simulation/).

## Performance Benchmarks

Measured on Linux x86_64 with `cargo test -p labwired-core --release --features "jit-framework,jit"` (see [`docs/esp32sim-audit.md`](docs/esp32sim-audit.md) for methodology, differential tests, and MIT attribution to [`mikroverk/esp32sim`](https://github.com/mikroverk/esp32sim)):

| Workload | Baseline interpreter | `Rv32BlockCache` (native & browser WASM32) | `RiscvWasmJit` (shared `Store`+`Memory`) |
| --- | ---: | ---: | ---: |
| ESP32 512 KiB SRAM ALU + mem + branch (2M instrs) | 35.8 Minsn/s (55.9 ms) | **193.8 Minsn/s** (10.3 ms, **5.42×**) | **85.8 Minsn/s** (23.3 ms, **2.40×**) |
| RV32IM pure ALU self-loop (2M instrs) | 69.8 Minsn/s (28.6 ms) | **402.9 Minsn/s** (5.0 ms, **5.77×**) | 59.8 Minsn/s (33.5 ms, 0.86×) |
| Short 256 B SRAM `step_unit` vs `Machine::step` (1M instrs) | 5.32 Minsn/s (187.8 ms) | — | **153.6 Minsn/s** (6.5 ms, **28.85×**) |

Reproduce locally:

```sh
cargo test -p labwired-core --release --features "jit-framework,jit" \
  --test riscv_block_cache_and_shared_jit_bench -- --include-ignored --nocapture
```

## Quickstart (CLI)

<img alt="labwired test booting an nRF54L15 and gating on the result" src="docs/assets/labwired-demo.gif" width="100%">

<sub>Recorded from [`docs/assets/demo.tape`](docs/assets/demo.tape) against a real binary. The second half breaks an assertion on purpose so you can see the CI gate fail.</sub>

Clone the repository, install the CLI, and run a committed bare-metal ELF — no cross-toolchain needed:

```sh
git clone https://github.com/w1ne/labwired-core && cd labwired-core
curl -fsSL https://labwired.com/install.sh | LABWIRED_VERSION=v0.26.0 sh
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

## GitHub Action (`uses: w1ne/labwired-core@v1`)

Run LabWired hardware & firmware verification on every pull request in public repositories at zero cost — no API keys or secrets required:

```yaml
name: LabWired Verify
on: [pull_request, push]

permissions:
  contents: read
  pull-requests: write
  id-token: write

jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: w1ne/labwired-core@v1
```

Auto-detects unmodified Wokwi `diagram.json` projects, KiCad `.kicad_pcb`/`.kicad_sch` boards, and LabWired test scripts (`tests/labwired.yml`), runs ERC + PCB DRC + deterministic firmware assertions, and posts a PR check summary (`ERC 0/0 · DRC 0/0 · X/X firmware assertions passed`) with a shareable replay link and `Verified by LabWired` badge. See [`action/README.md`](action/README.md) and the drop-in samples in [`examples/ci-wokwi-dropin`](examples/ci-wokwi-dropin) and [`examples/ci-kicad-dropin`](examples/ci-kicad-dropin).

## Installation

The install script covers Linux, macOS, and Windows via WSL2.

```sh
curl -fsSL https://labwired.com/install.sh | LABWIRED_VERSION=v0.26.0 sh
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

### Windows

The native Windows archive contains `labwired.exe` and `labwired-dap.exe`.
Install the current [x64 Microsoft Visual C++ v14 Redistributable](https://aka.ms/vc14/vc_redist.x64.exe)
first; both executables require `VCRUNTIME140.dll`. See Microsoft's
[runtime requirements](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170)
for supported Windows versions.

Download and extract the archive in PowerShell with `tar` available:

```powershell
$v = "v0.26.0"
Invoke-WebRequest "https://github.com/w1ne/labwired-core/releases/download/$v/labwired-$v-windows-x86_64.tar.gz" -OutFile labwired.tar.gz
mkdir $env:LOCALAPPDATA\LabWired -Force
tar -xzf labwired.tar.gz -C $env:LOCALAPPDATA\LabWired
$env:PATH += ";$env:LOCALAPPDATA\LabWired"
labwired --version
```

The VS Code extension does this for you on Windows — it downloads the same
archive on first use. WSL2 also works and takes the Linux instructions above.

Or build it yourself:

```sh
cargo build --release -p labwired-cli
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

### From Python

Install the SDK from PyPI with Python 3.9 or later:

```sh
python -m pip install 'labwired[test]==0.26.0'
```

Use `labwired.Sim` to run an ELF, wait for UART output, and write pytest checks:

```python
from labwired import Sim

with Sim("tests/fixtures/uart-ok-thumbv7m.elf",
         chip="ci-fixture-cortex-m3-uart1", uart="uart1") as sim:
    sim.expect("OK", timeout="1ms")
```

The SDK runs the Rust engine in your Python process. See the
[Python SDK quickstart](docs/python-sdk.md) for source-build instructions and a
runnable pytest test using the repository's firmware fixture.

### In CI

The same YAML scripts are the merge gate, with no HIL bench to maintain:

```yaml
- run: labwired test --script examples/ci/uart-ok.yaml --junit report.xml
```

See [CI integration](docs/ci_integration.md) and
[labwired.com/ci](https://labwired.com/ci.html).

### From an AI coding agent (MCP + Agent Skills)

LabWired speaks MCP and ships official Agent Skills + a Claude Code plugin directly inside this repository (`w1ne/labwired-core`). An agent can assemble a board, compile and simulate firmware, verify behavior, price a BOM, route a KiCad PCB, and read the evidence back — without touching a local toolchain.

For 1-click setup across Claude Code, Codex, Cursor, ChatGPT, VS Code Copilot, and Gemini CLI, visit **[app.labwired.com/tools/mcp](https://app.labwired.com/tools/mcp)** — or run directly:

```sh
# 1. Connect hosted MCP (OAuth 2.1, free daily tier)
claude mcp add --transport http labwired https://api.labwired.com/mcp
codex  mcp add labwired --url https://api.labwired.com/mcp

# 2. Install the official Claude Code plugin + Agent Skills from w1ne/labwired-core
claude plugin marketplace add w1ne/labwired-core
claude plugin install labwired@labwired

# Or install the Agent Skills into any agent via the universal CLI
npx skills add w1ne/labwired-core
```

Bundled skills in [`plugins/labwired/`](plugins/labwired/README.md) (with live proof in [`plugins/labwired/proof/`](plugins/labwired/proof/README.md)):

- [`electronics-lab`](plugins/labwired/skills/electronics-lab/SKILL.md) — default turnkey circuit, firmware, and PCB workflow
- [`build-and-verify-hardware`](plugins/labwired/skills/build-and-verify-hardware/SKILL.md) — prompt to verified circuit, firmware, BOM pricing, PCB, and 3D enclosure
- [`debug-firmware-in-sim`](plugins/labwired/skills/debug-firmware-in-sim/SKILL.md) — deterministic firmware debugging via UART, GPIO, bus traces, and interactive stepping
- [`verify-ci-regression`](plugins/labwired/skills/verify-ci-regression/SKILL.md) — deterministic pass/fail hardware verification oracles for GitHub Actions CI
- [`export-kicad-and-bom`](plugins/labwired/skills/export-kicad-and-bom/SKILL.md) — KiCad schematics/PCBs, Gerbers/CPL, priced BOMs, and STEP enclosures
- [`audit-erc-drc-and-safety`](plugins/labwired/skills/audit-erc-drc-and-safety/SKILL.md) — pre-fab ERC/DRC, signal integrity, thermal/pin-limit, and fault-injection audits

Your client opens a browser to sign in on first use. Other MCP clients take the standard
block:

```json
{ "mcpServers": { "labwired": { "type": "http", "url": "https://api.labwired.com/mcp" } } }
```

Then ask for what you want:

> *"Connect LabWired over MCP. Load a virtual STM32 LED + UART board, run the firmware,
> check the UART output, and give me the Playground URL."*

Agents working inside this repository should read [docs/agents.md](docs/agents.md).

## How a board is described

A board is data, not code.

A chip descriptor in [`configs/chips`](configs/chips/) declares memory geometry, cores, and
peripheral bases. A system manifest in [`configs/systems`](configs/systems/) wires up pins,
buses, and the devices on them. Firmware writes to modeled registers. The modeled hardware
drives the firmware back.

To add a board you write those two files and an example. You do not patch the engine. See
the [board onboarding playbook](docs/board_onboarding_playbook.md) and the
[configuration reference](docs/configuration_reference.md).

## What we validate

Every capability sits in one of three tiers, and we say which:

* **Modeled** — simulator logic exists and firmware can execute against it.
* **Smoke-tested** — a committed test or example exercises the model and checks output.
* **Hardware-compared** — captured silicon behavior is diffed against simulator behavior for
  a documented scope.

The NUCLEO-H563ZI example is the reference case. The same firmware runs on a physical board
and on the simulator, and the artifacts are committed. See
[`VALIDATION.md`](examples/nucleo-h563zi/VALIDATION.md),
[`determinism_report_h563.json`](examples/nucleo-h563zi/golden-reference/determinism_report_h563.json),
and the [golden reference method](docs/golden_reference.md).

Each report covers the scope it describes. None of them claim that every instruction and
timing path matches silicon. The [Fidelity Ledger](FIDELITY.md) has both sides: where we
short-circuit hardware, and where we hold up. One e-paper panel matches silicon on 19033 of
19033 SPI transfers.

## Boards and examples

ARM Cortex-M and RISC-V have the deepest coverage. Some ESP32/Xtensa paths exist for
specific examples. The optional native [QEMU-derived Cortex-A7 backend](docs/qemu-arm-core.md)
runs bounded instruction fixtures through the same CPU and bus interfaces. The
[Luckfox Pico Mini A dossier](docs/boards/luckfox-pico-mini-a.md) includes vendor assets,
pins and declared memory geometry; RV1103 peripherals and Linux boot remain unimplemented.

Check the per-board status before you assume a peripheral is modeled. It is in
[docs/boards](docs/boards/), the
[validation status matrix](https://docs.labwired.com/boards/VALIDATION_STATUS/), and the browsable catalog at
[app.labwired.com/validation](https://app.labwired.com/validation).

| To see | Run |
| --- | --- |
| A deterministic pass/fail gate | [CI UART smoke](examples/ci/README.md) |
| Firmware driving a sensor over I2C | [Blinky + TMP102](examples/demo-blinky/README.md) |
| Simulator compared against a physical board | [NUCLEO-H563ZI](examples/nucleo-h563zi/README.md) |
| CAN/UDS diagnostics | [UDS on STM32H563](examples/h563-uds-ecu/README.md) |
| An IO-Link device | [IO-Link DIDO](examples/iolink-dido/README.md) |
| GDB or VS Code debugging without a probe | [Debugging](docs/debugging.md), [GDB](docs/gdb_integration.md) |

The full list is in [docs/demos.md](docs/demos.md). Each example's README and validation file
is the source of truth for what that example actually models.

## The LabWired ecosystem

This repository is the engine. Everything below runs on it, and all of it is public.

**Put it in your workflow**

| Project | What it gives you |
| --- | --- |
| [firmware-test](https://github.com/LabWired/firmware-test) | GitHub Action. Run a LabWired test script as a merge gate — no hardware, no cross-toolchain on the runner. |
| [firmware-ci-starter](https://github.com/LabWired/firmware-ci-starter) | Template repo. Firmware, test script, and workflow already wired — [generate](https://github.com/LabWired/firmware-ci-starter/generate) it and your first push runs green. |
| [labwired-zephyr](https://github.com/w1ne/labwired-zephyr) | `west labwired-simulate` on Zephyr 3.7 and 4.5. The optional native runner on 4.5 also supports `west simulate -r labwired` and `west flash -r labwired`; see the setup instructions. |
| [labwired-vscode](https://github.com/w1ne/labwired-vscode) | VS Code extension. Run and debug firmware from the editor without a probe. |
| [labwired-lab-template](https://github.com/w1ne/labwired-lab-template) | Bare template for a repo whose merges gate on a simulation run. |

**Drive it from an agent**

| Project | What it gives you |
| --- | --- |
| [agent](https://github.com/LabWired/agent) | The LabWired Firmware Agent — writes firmware and checks it on a virtual board. |
| [LabWired Agent Skills & Plugin](plugins/labwired/README.md) | Official Claude Code plugin and 6 Agent Skills (`npx skills add w1ne/labwired-core`) bundled right in this repository. |
| [`docs/agents.md`](docs/agents.md) | The MCP surface and the rules an agent working in this repo should follow. |

**See it doing real work**

| Project | What it shows |
| --- | --- |
| [labwired-cra-evidence](https://github.com/LabWired/labwired-cra-evidence) | CRA-style secure-boot and signed-OTA evidence, regenerated in CI on a virtual nRF52840 + ATECC608A. Evidence, not a certificate. |
| [smart-ring-digital-twin](https://github.com/w1ne/smart-ring-digital-twin) | An nRF54L15 smart ring, register-level sensor models, and an honest BLE-contention demonstration. |
| [labwired-nokia-ci-demo](https://github.com/w1ne/labwired-nokia-ci-demo) | STM32L476 driving a Nokia 5110 (PCD8544) and an HC-SR04, gated in CI. |
| [labwired-demo-stm32f1xx](https://github.com/w1ne/labwired-demo-stm32f1xx) | Rust `embedded-hal` on STM32F1 under the simulator. |

**Firmware stacks that use LabWired as their test bench**

[iolinki](https://github.com/w1ne/iolinki) (IO-Link device stack for Zephyr) ·
[iolinki-master](https://github.com/w1ne/iolinki-master) (IO-Link master stack) ·
[udslib](https://github.com/w1ne/udslib) (ISO 14229 UDS for embedded ECUs) ·
[thermal-io-link-condition-sensor](https://github.com/w1ne/thermal-io-link-condition-sensor) (MLX90640 + ESP32-C3 condition monitoring)

Using LabWired for something public? [Open an issue][new-issue] and it goes on this list.

## Missing a chip? Peripheral behaving wrong?

[Open an issue][new-issue] for a wrong register, an unsupported instruction, a board you
need, or a peripheral you would model differently. Attach a firmware ELF and a system
manifest if you can — those usually become regression tests.

Adding a board is the easiest way to contribute: follow the
[board onboarding playbook](docs/board_onboarding_playbook.md), mirror an existing example,
and check the result against the [target support rubric](docs/target_support_rubric.md).

See [ROADMAP.md](ROADMAP.md) for what is planned, [CONTRIBUTING.md](CONTRIBUTING.md) for
repository workflow, and [SECURITY.md](SECURITY.md) for security issues.

[new-issue]: https://github.com/w1ne/labwired-core/issues/new

## Repository scope

This repository owns the core simulator and its validation assets: CPU, bus, memory,
peripheral, and external device execution; chip and system descriptors; CLI, test runner,
debug adapters, and snapshot/trace tooling; and hardware-target validation metadata.
Application UI, hosted Playground behavior, and product surfaces live outside this package.

The merge gate is [`core-ci.yml`](.github/workflows/core-ci.yml). Narrower signals come from
board smoke coverage ([`core-board-ci.yml`](.github/workflows/core-board-ci.yml)), coverage
([matrix smoke](.github/workflows/core-coverage-matrix-smoke.yml),
[weekly](.github/workflows/core-coverage-weekly.yml)),
[unsupported instruction audits](.github/workflows/core-unsupported-audit.yml),
[nightly validation](.github/workflows/core-nightly.yml),
[hardware target sweeps](.github/workflows/core-validate-hw-targets.yml), and per-board
throughput ([`core-perf.yml`](.github/workflows/core-perf.yml)). Release mechanics are in
[RELEASE_PROCESS.md](RELEASE_PROCESS.md) and
[RELEASE_READINESS_CHECKLIST.md](RELEASE_READINESS_CHECKLIST.md).

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
