# BBC micro:bit v2 (Nordic nRF52833) — UART smoke

The **micro:bit v2** target MCU: Nordic **nRF52833** — Cortex-M4F at 64 MHz,
512 KB flash, 128 KB RAM, 42 GPIOs, 2.4 GHz radio, USB and NFC. LabWired
models the nRF52 peripheral family shared with the nRF52840; this page covers
the **smoke-manual** slice (core, GPIO P0/P1, CLOCK, and the UARTE0 EasyDMA
console bridged to the interface MCU) plus the **tier-1 peripheral-depth**
fixture, which raw-register-exercises ten peripheral classes on this silicon
row.

!!! tip "Live status"
    - [Chip conformance](../coverage/chip-conformance.md)
    - [Bus visibility](../coverage/bus-visibility.md)
    - [Target support rubric](../target_support_rubric.md)

---

## Status at a glance

| Aspect | Status |
|--------|--------|
| Chip descriptor | [`configs/chips/nrf52833.yaml`](../../configs/chips/nrf52833.yaml) |
| System | [`configs/systems/microbit-v2.yaml`](../../configs/systems/microbit-v2.yaml) |
| Example | [`examples/microbit-v2/`](../../examples/microbit-v2/README.md) |
| Committed ELF | `tests/fixtures/microbit-v2-smoke.elf` |
| Survival gate | `firmware_survival::test_nrf52833_microbit_v2_smoke_survival` |
| Tier-1 fixture | [`examples/tier1-fixture/nrf52833/`](../../examples/tier1-fixture/nrf52833/) → `tests/fixtures/tier1/nrf52833.elf` |
| Tier-1 result | clock/gpio/timer/rtc/i2c/spi/adc/wdt/pwm **PASS** + UART via `TIER1 done`; `dma`/`irq` `na` (not declared) |
| Tier | **smoke-manual (L1)** — boots and prints `OK`, tier-1 depth recorded, **no silicon diff** |

---

## Flash / firmware artifact

| Use | Artifact | Notes |
|-----|----------|-------|
| Bare-metal / Zephyr | **ELF** | Vector table at flash base `0x00000000`; 512 KB / 128 KB per nRF52833 PS |
| micro:bit runtime (codal) | — | not exercised in this repo |
| UF2 / SoftDevice | — | not modelled |

---

## Pins (micro:bit v2)

Nordic pins are **P0.xx / P1.xx**. Values below are from the micro:bit v2
hardware docs (`tech.microbit.org/hardware/schematic/`); the schematic's
`UART_INT_*` labels are from the interface MCU's perspective.

| Signal | nRF52833 pin | Notes |
|--------|--------------|-------|
| UART TX (target → interface MCU) | **P0.06** | `MICROBIT_PIN_UART_TX` in codal-microbit-v2 |
| UART RX (interface MCU → target) | **P1.08** | labels are swapped on the schematic; see microbit-foundation/microbit-v2-hardware#5 |
| Button A | **P0.14** | active-low; declared as an `input` stub in `board_io` |
| Button B | **P0.23** | active-low; declared as an `input` stub in `board_io` |
| 5×5 LED matrix rows/cols | P0.19/P0.21/P0.22/P0.15/P0.24 + P0.28/P0.11/P0.31/P1.05/P0.30 | **charlieplexed — not modelled** |

P1 is P1.00–P1.09 on this part (the family total is 42 GPIOs). The sim remaps
the P1 window to `0x50001000` to avoid GPIO0's 4 KB window, the same simulator
map the nRF52840 descriptor uses (silicon places it at `0x50000300`).

---

## Support matrix

| Block | Status | Notes |
|-------|--------|-------|
| Cortex-M4F + FPU | ✅ | Thumb-2 decoder shared with nRF52840 |
| GPIO P0/P1 | ✅ | `profile: nrf52`; buttons declared as inputs |
| UARTE0 EasyDMA console | ✅ smoke-proven | `OK\n` on P0.06 at 115200, `ENABLE=8`, ENDTX poll |
| CLOCK / NVMC / FICR / UICR | ✅ declared | behavioural models shared with nRF52840 |
| TIMER0–4 / RTC0–2 / PWM0–3 / SPIM / TWIM / PPI / GPIOTE / SAADC / ECB / AAR / RNG / TEMP / EGU / QDEC / COMP / PDM / I2S | ✅ declared | same-model reuse; not smoke-exercised on this part |
| RADIO / BLE | ❌ not modelled | RADIO is a register window; no BLE stack or air model on this target |
| USB (USBD) | ⚠️ window only | register window declared; no USB device model |
| NFC (NFCT) | ⚠️ window only | register window declared; no tag model |
| 5×5 LED matrix | ❌ not modelled | charlieplexed; no matrix driver in the engine |
| Speaker / microphone / motion sensor / touch logo | ❌ not attached | require external component models |
| Silicon diff / executing-fidelity differential | ❌ none | no bench part captured; L1 smoke is the ceiling today |

---

## What is proven

`crates/firmware-nrf52833-demo` is a bare-metal `no_std` image: it programs
`PSEL.TXD = P0.06`, `PSEL.RXD = P1.08`, `BAUDRATE = 0x01D6_0000` (115200),
`ENABLE = 8` (UARTE), then pushes a RAM-resident `OK\n` through
`TXD.PTR`/`TXD.MAXCNT`/`TASKS_STARTTX` and waits for `EVENTS_ENDTX`. The
committed ELF is booted in-process by the survival gate and the byte stream is
asserted at the UART sink; the same binary runs on silicon-target toolchains
(`thumbv7em-none-eabi`).

The chip descriptor and system manifest are also gated by
`nrf52833_from_config_builds` (`SystemBus::from_config` must expose `uart0`,
`gpio0`, `gpio1`).

## What is not proven

No silicon capture, no register sweep, no executing-fidelity differential. The
radio/BLE, USB protocol, NFC, the charlieplexed display and every on-board
sensor are **not** modelled. The chip descriptor mirrors the nRF52840 family's
peripheral types — the shared blocks are the same silicon IP; the tier-1
fixture (below) now exercises ten of those blocks end-to-end, but the
per-instance sweep, timing and the radio/DMA/interrupt paths stay unproven.

---

## Tier-1 peripheral depth

[`examples/tier1-fixture/nrf52833/`](../../examples/tier1-fixture/nrf52833/) is
a standalone bare-metal `no_std` image (own `[workspace]`, thumbv7em) that
pokes raw MMIO and prints the TIER1 protocol over the UARTE0 EasyDMA console
configured exactly as the micro:bit v2 wiring (`PSEL.TXD = P0.06`,
`PSEL.RXD = P1.08`, 115200, `ENABLE = 8`):

```text
TIER1 gpio PASS      # P0.13 and P1.05 DIRSET/OUTSET/OUT/OUTCLR round-trips
TIER1 clock PASS     # TASKS_HFCLKSTART -> EVENTS_HFCLKSTARTED + HFCLKRUN
TIER1 timer PASS     # TIMER0 32-bit counter advances between two CAPTURE0s
TIER1 rtc PASS       # RTC0 COUNTER advances from TASKS_START
TIER1 i2c PASS       # TWIM1 EasyDMA TX with no slave -> ANACK + LASTTX/ERROR
TIER1 spi PASS       # SPIM2 EasyDMA TXD/RXD round-trip -> END + AMOUNTs
TIER1 adc PASS       # SAADC EasyDMA conversion read back by value (12/10 bit)
TIER1 wdt PASS       # WDT CRV/RREN + TASKS_START -> RUNSTATUS + TIMEOUT
TIER1 pwm PASS       # PWM0 SEQ[0] EasyDMA playback -> SEQEND0 + PWMPERIODEND
TIER1 done           # UART is implicit: the transcript itself is the proof
```

Run it (the step budget is generous: the whole sequence completes under
500 000 steps):

```bash
labwired run --chip configs/chips/nrf52833.yaml \
  --firmware tests/fixtures/tier1/nrf52833.elf --max-steps 8000000
```

**Honest limitations.** `dma` and `irq` are not attempted and render `na` in
the [tier-1 matrix](../coverage/tier1-scoreboard.md): the family declares no
DMA/NVIC peripheral type and neither nrf52832 nor nrf52840 records one either.
Only the first instance of each class is exercised (TIMER0, RTC0, PWM0, TWIM1,
SPIM2 — SPIM3/PWM1-3/RTC1-2 are declared but unswept). The `i2c` proof is a
no-slave address-NACK, not a data transfer against a modeled slave; SAADC
reads the model's fixed internal source, not a pin voltage. GPIO P1 is tested
at the simulator's remapped window (`0x50001000`), not the raw-silicon base.
There is still no silicon diff for any of these paths.

---

## How to run

```bash
cargo build -p firmware-nrf52833-demo --release --target thumbv7em-none-eabi
cargo run -q -p labwired-cli -- \
  --firmware tests/fixtures/microbit-v2-smoke.elf \
  --system configs/systems/microbit-v2.yaml \
  --max-steps 200000
```

Expected: the run finishes and prints `OK` to stdout; the deterministic
scripted assertion lives in
[`examples/microbit-v2/uart-smoke.yaml`](../../examples/microbit-v2/uart-smoke.yaml).

---

## Related

- [nRF52840](nrf52840.md) — the silicon-verified sibling this descriptor borrows its blocks from
- [nRF52832](nrf52832.md) — single-port sibling
- [micro:bit v2 example](../../examples/microbit-v2/README.md)
