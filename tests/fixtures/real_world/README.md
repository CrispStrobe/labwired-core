# Vendored vendor SVDs (`tests/fixtures/real_world/`)

These files are consumed by `crates/core/tests/register_coverage.rs`, which
enumerates every register the vendor declares and probes the simulator's bus for
it. A missing or incomplete SVD therefore does not merely reduce coverage — it
**silently disarms the gate for whatever it fails to declare**, because the scan
cannot fail on a register the SVD never mentions.

New entries record their provenance here, following the convention in
`tests/fixtures/svd/README.md`. Files that predate this README do not yet have a
record; add one when you next touch them.

## mimxrt1062.svd

**Source:** [`cmsis-svd/cmsis-svd-data`](https://github.com/cmsis-svd/cmsis-svd-data)
→ `data/NXP/MIMXRT1062.svd` at commit `c65f8551e57c770344d229dcaa0bf838fa29aff4`
(device `MIMXRT1062`, version 1.0, description `MIMXRT1062DVL6A`, NXP,
BSD-3-Clause; 10.9 MB, md5 `9cca40fe5b184050f15f75dbfc2fabd2`).
**Vendored:** 2026-09-27, for the MIMXRT1062 onboarding (FLAMMA FB200 board).
**Consumed by:** `svd_conformance.rs` (chip `mimxrt1062`) and the ingested
descriptors in `configs/peripherals/mimxrt1062/`.

### Known limits

- FlexIO2/FlexIO3 are `derivedFrom` FLEXIO1, so the SVD gives every FlexIO
  4 shifters / 4 timers (`PARAM` 0x0210_0404); the model follows the SVD.
- CCM_ANALOG, PMU, USB_ANALOG, XTALOSC24M and TEMPMON are five SVD
  peripherals at the same base 0x400D_8000; the chip wires one window for them.
- `USB2` is `derivedFrom` USB1 at 0x402E_0200 (fb200-tools HARDWARE.md called
  that address the USB1 PHY; USBPHY1 is at 0x400D_9000).

## stm32f411.svd

**Source:** [`modm-io/cmsis-svd-stm32`](https://github.com/modm-io/cmsis-svd-stm32)
→ `stm32f4/STM32F411.svd` (device `STM32F411`, version 1.9, 856 KB)
**Vendored:** 2026-07-27, for the STM32F411CEU6 (WeAct Black Pill) onboarding.
**Consumed by:** `register_coverage.rs` (chip `stm32f411ceu6`).

### Why modm-io and not cmsis-svd/cmsis-svd-data

Two public F411 SVDs exist and they are **not** equivalent. Measured on the
actual files:

| | modm-io (vendored) | cmsis-svd-data (rejected) |
|---|---|---|
| Peripherals | 47 | 55 |
| **Declared interrupts** | **56 (max 85)** | **34 (max 84)** |
| SPI5 peripheral | yes | yes |
| SPI5 interrupt | **yes (85)** | **absent** |
| USART1/2/6 interrupts | yes (37/38/71) | absent |
| TIM4 / TIM5 interrupts | yes (30/50) | absent |
| DMA stream interrupts | yes | absent |

The cmsis-svd-data file's NVIC table omits USART1/2/6, TIM4, TIM5, every DMA
stream **and SPI5** — precisely the peripheral this onboarding adds. Vendoring
it would install a gate that looks armed and verifies nothing about any of them.
For scale, the in-tree `stm32f401.svd` declares 54 interrupts up to 84, so the
modm file is of comparable quality and the cmsis-svd one is not.

### Residual gap — read this before trusting the gate

**Neither** F411 SVD declares `RCC.APB2ENR.SPI5EN`. The string `SPI5EN` does not
occur anywhere in the vendored file. That bit position (20) is taken from ST's
own CMSIS header
([`STMicroelectronics/cmsis_device_f4`](https://github.com/STMicroelectronics/cmsis_device_f4),
`Include/stm32f411xe.h`, `RCC_APB2ENR_SPI5EN_Pos`) and **cannot be cross-checked
against any SVD**. It is the one value in `configs/chips/stm32f411ceu6.yaml`
with a single source. The tier-1 fixture's `spi` check exercises it (gate off →
CR1 write dropped; gate on → transfer runs), so it is at least executable
evidence rather than an unchecked constant.

Everything else in the chip descriptor that the SVD *does* declare — SPI5's base
`0x40015000` and its IRQ 85, the RCC enable-register offsets, the GPIO port set
A/B/C/D/E/H — agrees between the SVD and the header.

Do not edit these files by hand. To refresh, re-download from the upstream repo
and re-check the interrupt count before committing.

## esp32c6.svd

**Source:** [`espressif/svd`](https://github.com/espressif/svd) → `svd/esp32c6.svd`
(device `ESP32-C6`, version 11, ~2.8 MB, Apache-2.0). Upstream raw URL:
`https://raw.githubusercontent.com/espressif/svd/main/svd/esp32c6.svd`
**Vendored:** 2026-09-19, for the ESP32-C6-DevKitC-1 onboarding.
**sha256:** `519b374c472483b170d6e5b479e48c07675c18af5bf2613c61eff888dc0fc8cd`
**Consumed by:** `svd_conformance.rs` (chip `esp32c6`) and `register_coverage.rs`
(chip `esp32c6`).

### What it arbitrates, and what it does not

The chip descriptor's declared bases and IRQs are checked against this file
with **zero** justified deviations: UART0 0x6000_0000 / IRQ 43, UART1
0x6000_1000 / IRQ 44, GPIO 0x6009_1000 / IRQ 30, IO_MUX 0x6009_0000, PCR
0x6009_6000, HP_SYS 0x6009_5000, INTERRUPT_CORE0 0x6001_0000.

Scope caveats, stated so a green `svd_conformance` run is not read as more
than it is:

* The SVD is Espressif's *register-map* publication. Memory sizes (512 KB HP
  SRAM, 320 KB ROM, 8 MB module flash) come from the ESP32-C6 datasheet /
  memory map, not from this file.
* It describes the **HP** core's register file plus the LP peripherals. The LP
  core itself is out of scope for this L1 target; the LP_* peripherals are
  deliberately not declared in `configs/chips/esp32c6.yaml`.
* `svd_conformance` checks declared windows agree with the SVD. It does not
  demand every SVD peripheral be declared: UART0/UART1/GPIO/IO_MUX/PCR/HP_SYS/
  INTERRUPT_CORE0 are wired; SPI, I2C, RMT, LEDC, DMA, ADC, crypto, USB and
  every radio are intentionally unmapped at L1.
