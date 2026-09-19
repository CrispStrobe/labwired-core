# NUCLEO-G071RB (ST STM32G071RB) — UART/LED smoke

An ST **STM32G071RB** Nucleo-64 board — single-core **Cortex-M0+ (ARMv6-M)**,
128 KB flash, 36 KB SRAM, up to 64 MHz. This page covers the modelled slice:
core + SysTick, the **G0 RCC** layout, GPIO on the `0x50000000` IOPORT bus,
and USART2 (the ST-LINK VCP console). Built for `thumbv6m-none-eabi`.

**Tier: L1 smoke (SIM-DERIVED).** No STM32G0 silicon has ever been probed
over SWD for this port; every register number traces to RM0444 / DS12232 /
ST's CMSIS `stm32g071xx.h`. Read the limitations below before trusting any
peripheral beyond the smoke path.

!!! tip "Live status"
    Authoritative automation:

    - [Chip conformance scoreboard](../coverage/chip-conformance.md) — level · modelled peripherals · register-match
    - [Bus visibility](../coverage/bus-visibility.md) — which buses produce decodable edges
    - [Target support rubric](../target_support_rubric.md) — modeled / stub / silicon-verified

---

## Status at a glance

| Aspect | Status |
|--------|--------|
| Chip descriptor | [`configs/chips/stm32g071.yaml`](../../configs/chips/stm32g071.yaml) |
| Example system | [`configs/systems/nucleo-g071rb.yaml`](../../configs/systems/nucleo-g071rb.yaml) |
| Reference firmware | [`crates/firmware-stm32g0-demo/`](../../crates/firmware-stm32g0-demo/) |
| Committed ELF | `tests/fixtures/nucleo-g071rb-smoke.elf` |
| Example / evidence | [`examples/nucleo-g071rb/`](../../examples/nucleo-g071rb/) |
| Validation | `firmware_survival::test_nucleo_g071rb_smoke_survival` · `stm32g071_from_config_builds` |
| Tier | **L1 smoke** — boots firmware and prints `OK`; **no silicon diff** |
| Playground board id | none yet (not in the bundled catalog) |

---

## Flash / firmware artifact

| Use | Artifact | Notes |
|-----|----------|-------|
| CLI / survival fixture | `tests/fixtures/nucleo-g071rb-smoke.elf` | Built `--release --target thumbv6m-none-eabi` from `crates/firmware-stm32g0-demo` |
| Rebuild | `cargo build -p firmware-stm32g0-demo --release --target thumbv6m-none-eabi` | Build.rs passes its own `-Tlink.x`; no `.cargo/config.toml` needed |
| Real board | `objcopy -O binary` then flash at `0x08000000` | The same binary is intended to run on silicon |

---

## Pins (NUCLEO-G071RB, UM2505 / Zephyr `nucleo_g071rb` DTS)

| Board label | MCU pin | Function | Notes |
|-------------|---------|----------|-------|
| LD4 (green) | PA5 | GPIO output | active high, toggled by the smoke |
| B1 (user) | PC13 | GPIO input | active low (pull-up to VDD) |
| USART2_TX | PA2 | AF1 | ST-LINK V3 Virtual COM Port |
| USART2_RX | PA3 | AF1 | ST-LINK V3 Virtual COM Port |
| SWDIO / SWCLK | PA13 / PA14 | SWD | Cortex-M0+ has **no JTAG TAP** |
| Arduino I2C | PB8 / PB9 | AF | declared in the chip yaml, unproven |
| Arduino SPI | PB0/PA5/PA6/PA7 | AF | PA5 shared with LD4, unproven |

---

## Support matrix

| Mark | Meaning |
|------|---------|
| ✅ | Modeled well enough for the firmware this repo ships |
| ⚠️ | Present but partial / family-reused / not G0-tuned |
| ❌ | Not simulated — use the bench |

### Core & boot

| Block | Status | Notes |
|-------|--------|-------|
| Cortex-M0+ CPU | ⚠️ | Decoder executes the full Thumb-2 set; the ARMv6-M subset is enforced only by the `thumbv6m` toolchain |
| Flash / SRAM windows | ✅ | 128 KB @ `0x08000000` / 36 KB @ `0x20000000` (DS12232) |
| SysTick | ✅ | |
| NVIC | ✅ | |
| DBGMCU | ⚠️ | APB @ `0x40015800`; DEV_ID `0x460` is ST's published value, **not read off a die** |

### Clock & reset

| Block | Status | Notes |
|-------|--------|-------|
| RCC `stm32g0` layout | ✅ | CR/ICSCR/CFGR/PLLCFGR @0x00–0x0C, 3-bit SW/SWS, IOPENR@0x34 / AHBENR@0x38 / APBENR1@0x3C / APBENR2@0x40, BDCR@0x5C / CSR@0x60 — verified against RM0444 §5.4 + `stm32g071xx.h`, pinned by unit tests |
| HSI16 ready / SYSCLK switch | ✅ | HSION bit8 → HSIRDY bit10; SW→SWS completes only when the source is ready |
| LSI / LSE ack | ✅ | CSR bit0→bit1, BDCR bit0→bit1 |
| PLL frequency | ❌ | PLLON→PLLRDY is modelled; the programmed frequency is not |

### Console, GPIO, timers

| Block | Status | Notes |
|-------|--------|-------|
| USART2 (VCP) | ✅ | `stm32v2` USART layout, clocked via APBENR1 bit17; byte path proven by the smoke |
| USART1/3/4, LPUART1 | ⚠️ | Same model, declared; not exercised end-to-end |
| GPIO A–D, F | ✅ | `stm32v2` layout on the IOPORT bus; LD4/PA5 + B1/PC13 declared in `board_io` |
| TIM1/2/3/6/7/14/15/16/17 | ⚠️ | Family timer model; G0 widths declared (TIM2 32-bit, rest 16-bit) but not timed against silicon |
| LPTIM1/2 | ⚠️ | Declared; family model, unproven |

### Buses & analog

| Block | Status | Notes |
|-------|--------|-------|
| I2C1 / I2C2 | ⚠️ | `stm32l4` controller model reused — not G0-diffed |
| SPI1 / SPI2 | ⚠️ | Classic (`stm32`) SPI model reused — not G0-diffed |
| ADC1 / DAC1 | ❌ | Register windows only; analog conversion is not modelled for this part |
| DMA1 | ⚠️ | Family DMA model, declared; DMAMUX not declared |
| RTC / IWDG / WWDG / CRC | ⚠️ | Family models, declared; CRC is 32-bit IDR |
| EXTI / SYSCFG | ⚠️ | Single-bank EXTI (`stm32f1` profile); SYSCFG model present |
| UCPD / CEC / VREFBUF / COMP | ❌ | Not declared / not modelled |

### What the smoke actually proves

1. The G0 RCC map: firmware writes IOPENR (`0x34`) and APBENR1 (`0x3C`);
   a wrong (L0/L4) offset leaves the UART silent.
2. `HSIRDY` at CR bit 10 — the G0 ready-bit layout.
3. USART2 TDR writes leave the part as UART bytes (`OK`).
4. LD4 (PA5) toggles through GPIOA BSRR under the G0 IOPENR gate.

### What it does **not** prove

- Anything about real silicon: there is no NUCLEO-G071RB capture in this
  repo, and no executing-fidelity differential (no walk-vs-scheduler oracle).
- Clock frequencies, baud timing, DMA/DMAMUX, analog, or the non-console
  buses.
- Interrupt delivery for any IRQ (the smoke is polled, not interrupt-driven).

---

## How to run

```bash
# Build the smoke firmware (Cortex-M0+)
cargo build -p firmware-stm32g0-demo --release --target thumbv6m-none-eabi

# Direct run — expect "OK" on the UART
cargo run -q -p labwired-cli -- \
  --firmware tests/fixtures/nucleo-g071rb-smoke.elf \
  --system configs/systems/nucleo-g071rb.yaml \
  --max-steps 200000

# Asserted smoke
cargo run -q -p labwired-cli -- test \
  --script examples/nucleo-g071rb/uart-smoke.yaml \
  --output-dir out/nucleo-g071rb/uart-smoke --no-uart-stdout
```

**Playground:** not registered in the bundled catalog — this board is CLI /
library only today. **MCP:** the board is reachable through the same
`Session`/bus APIs as any other chip; no board-specific MCP wiring exists.

---

## Related

- [NUCLEO-L073RZ](nucleo-l073rz.md) — the closest cheap STM32 (M0+, IOPORT
  bus, modern USART); the L0 RCC map differs from G0 (CRRCR@0x08, IOPENR@0x2C)
- [NUCLEO-G474RE / STM32G474RE](stm32g474re.md) — the G4 sibling, different
  RCC map and GPIO bus (`0x48000000`)
- [`examples/nucleo-g071rb/VALIDATION.md`](../../examples/nucleo-g071rb/VALIDATION.md) — exact commands + evidence
