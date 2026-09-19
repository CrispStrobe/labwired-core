# ESP32-C6-DevKitC-1

Espressif **ESP32-C6** on the **ESP32-C6-DevKitC-1** (v1.2) — single HP core **RISC-V RV32IMAC @ 160 MHz**, 512 KB HP SRAM, 320 KB ROM, 8 MB external SPI flash on the WROOM-1 module, plus an LP core and Wi-Fi 6 / Bluetooth 5 / IEEE 802.15.4 radios on silicon. LabWired's C6 target runs a bare-metal ELF whose console reaches the capture sink.

This is an **L1 (smoke-supported)** target: UART0 bring-up through the C6's own clock/reset block (PCR), no silicon capture yet.

!!! tip "Live status"
    The tables below are a maintained snapshot. Authoritative automation:

    - [Chip conformance scoreboard](../coverage/chip-conformance.md) — level · modelled peripherals · register-match
    - [Bus visibility scoreboard](../coverage/bus-visibility.md) — which buses produce decodable edges
    - [Target support rubric](../target_support_rubric.md) — modeled / stub / silicon-verified

---

## Status at a glance

| Aspect | Status |
|--------|--------|
| Chip descriptor | [`configs/chips/esp32c6.yaml`](../../configs/chips/esp32c6.yaml) |
| Example system | [`configs/systems/esp32c6-devkitc.yaml`](../../configs/systems/esp32c6-devkitc.yaml) |
| Reference firmware | [`crates/firmware-esp32c6-demo/`](../../crates/firmware-esp32c6-demo/) |
| Example | [`examples/esp32c6-devkitc/`](../../examples/esp32c6-devkitc/) |
| Committed artifact | `tests/fixtures/esp32c6-demo.elf` (`OK\n`) |
| Register/interrupt source | Vendored `tests/fixtures/real_world/esp32c6.svd` (espressif/svd) |
| Tier (snapshot) | **L1 smoke** — SIM-DERIVED, no silicon diff |

---

## Flash / firmware artifact

| Use | Artifact | Notes |
|-----|----------|--------|
| **Bare / demo crate** | ELF linked into the C6's unified flash window | `cargo build -p firmware-esp32c6-demo --release --target riscv32imc-unknown-none-elf` |
| **ESP-IDF / esptool image** | Merged flash `.bin` (bootloader + partition table + app) | **Not modelled end-to-end.** The bootloader/ROM path and partition parsing are out of scope at L1 |

Unlike the C3, the C6 maps **both** instructions and constant data through the
same external-memory window (`0x4200_0000..0x42FF_FFFF`; esp-idf v5.3
`soc/esp32c6/include/soc/soc.h` sets `SOC_IROM_LOW = SOC_DROM_LOW = 0x42000000`).
The 8 MB DevKitC-1 flash is declared as that window and behaves as plain memory:
there is **no SPI flash controller** behind it, so a real IDF image's ROM boot
sequence cannot run. The smoke ELF is loaded straight at its link addresses.

**ROM region (optional):** 320 KB at `0x4000_0000`, loadable via
`LABWIRED_ESP32C6_ROM`. Nothing on the L1 smoke path needs it; without the env
var the window stays zero.

---

## Pins (ESP32-C6-DevKitC-1 v1.2)

| Board label | GPIO | Typical default | Notes |
|-------------|------|-----------------|--------|
| TX (J3-2) | GPIO16 | **U0TXD** | USB-to-UART console |
| RX (J3-3) | GPIO17 | **U0RXD** | USB-to-UART console |
| — | GPIO8 | **Addressable RGB LED** (WS2812) | Needs RMT frame; pin stub only |
| BOOT (J3-11) | GPIO9 | BOOT button (active low) | Strapping pin; download mode not modelled |
| D0… | GPIO0–GPIO7 | LP-capable / ADC1 / strapping (MTMS/MTDI) | Advertised via the shared GPIO matrix model |
| D1… | GPIO10, GPIO11, GPIO15, GPIO18–GPIO23 | Header pins | Not modelled beyond the GPIO window |
| 3V3 / 5V / GND | — | Power | Digital levels in sim — not SPICE |

The GPIO matrix reuses the engine's C3-sized register block (pins 0–25), so
**GPIO26–GPIO30 are not modelled**. C6 GPIO has 31 pins
(`SOC_GPIO_PIN_COUNT` = 31 in `soc_caps.h`).

---

## Support matrix

| Mark | Meaning |
|------|---------|
| ✅ | Modeled well enough for the documented L1 smoke |
| ⚠️ | Present but partial, stubbed, or easy to misuse |
| ❌ | Not simulated — use the bench |

### Core & boot

| Block | Status | Notes |
|-------|--------|-------|
| RV32IMAC HP core | ✅ / ⚠️ | Interpreter executes the smoke; the A-extension decodes but is not exercised by the `imc` fixture |
| Memory map (flash window, 512 KB HP SRAM, 320 KB ROM region, 16 KB LP SRAM) | ✅ | Bases match the C6 memory map and `soc.h` |
| Mask ROM | ⚠️ | Optional image via `LABWIRED_ESP32C6_ROM`; not the reset path of this ELF |
| Reset / clocks | ⚠️ | PCR is a register-backed **stub**: gates are recorded, never enforced |
| LP core (RV32IMC) | ❌ | Not modelled |

### Console, GPIO, IO_MUX

| Block | Status | Notes |
|-------|--------|-------|
| UART0 / UART1 | ✅ / ⚠️ | Espressif UART IP + 128-byte FIFOs, clocked at 160 MHz (`esp32c6_uart`). Head map only: the C6 register tail (`SLEEP_CONF*`, `CLK_CONF` @0x88, `DATE` @0x8C, `ID` @0x9C) is unmodelled and reads as the C3's old offsets |
| GPIO matrix | ⚠️ | Shared C3-sized model (26 pins); `board_io` LED/button declared but the RGB LED needs RMT |
| IO_MUX | ⚠️ | Register-backed stub: pad function writes are recorded, not electrically enforced — the console shifts bytes regardless of routing |
| USB Serial/JTAG | ❌ | `USB_DEVICE` window deliberately unmapped |
| TIMG0/TIMG1, SYSTIMER, RMT, LEDC, I2C, SPI, I2S, GDMA, ADC, TWAI, crypto, eFuse, PMU, LP_* | ❌ | Deliberately **not declared**; their windows fault loudly instead of pretending to work |

### Interrupts

| Block | Status | Notes |
|-------|--------|-------|
| Interrupt matrix (`INTERRUPT_CORE0`, base 0x6001_0000) | ⚠️ | SVD-derived `declarative` window: routing registers are storage only. UART0=43 / UART1=44 are declared, but **no interrupt is delivered** — the L1 smoke is polled and the UART model drains its FIFO on cycle ticks |
| GPIO IRQ 30 | ⚠️ | Declared in the source SVD; not routed |

### Radio & network

| Block | Status | Notes |
|-------|--------|-------|
| Wi-Fi 6 (2.4 GHz), Bluetooth 5 (LE), IEEE 802.15.4 (Zigbee/Thread) | ❌ | **Not modelled at all** — no MAC, no PHY, no packet path. Do not run networking firmware here |
| LP peripherals / deep sleep | ❌ | Not modelled |

---

## What it catches vs what needs a bench

**Sim is strong for:** CPU/decode regressions on a second RISC-V memory map; UART
bring-up sequencing (PCR gate → IO_MUX route → CLKDIV → FIFO); register-map and
IRQ cross-checks against the vendor SVD; deterministic CI of a C6 image.

**Still use silicon for:** the ROM bootloader and flash/partition images, LP core
and power management, interrupts, Wi-Fi/BT/802.15.4, USB Serial/JTAG, analog,
power, antenna/EMI, production sign-off.

---

## How to run

### CLI (oracle / CI)

```bash
cargo build -p firmware-esp32c6-demo --release --target riscv32imc-unknown-none-elf

cargo run -q -p labwired-cli -- \
  --firmware tests/fixtures/esp32c6-demo.elf \
  --system configs/systems/esp32c6-devkitc.yaml \
  --max-steps 200000
```

Expected: `OK` on the console, PC ending inside `0x4200_0000..0x42FF_FFFF`.

The runnable example (and its assertions) is
[`examples/esp32c6-devkitc/`](../../examples/esp32c6-devkitc/README.md).

### Playground

Not yet a catalog board — the L1 target is CLI/CI only until a differential and
a playground registration land.

### Agent (MCP)

Not yet registered.

---

## Related systems & examples

| Path | What |
|------|------|
| `configs/chips/esp32c6.yaml` | Chip descriptor (bases/IRQs cited) |
| `configs/systems/esp32c6-devkitc.yaml` | Baseline system |
| `examples/esp32c6-devkitc/` | Smoke example + validation runbook |
| `tests/fixtures/real_world/esp32c6.svd` | Vendored vendor SVD used by the gates |
| [ESP32-C3 board page](esp32c3.md) | Sibling Espressif RISC-V target (different map, deeper model) |
