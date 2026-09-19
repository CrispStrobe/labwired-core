# Validation — NUCLEO-G071RB

**Tier: L1 smoke (SIM-DERIVED).** This board was onboarded 2026-09-19 with no
STM32G0 silicon on the bench: every register number traces to RM0444 / DS12232
/ ST's CMSIS `stm32g071xx.h`, and the evidence below is simulator-side only.
Do not read this page as a hardware-validation claim.

Run all commands from the repository root.

## 0. Artifact identity

| Item | Value |
|------|-------|
| Fixture | `tests/fixtures/nucleo-g071rb-smoke.elf` |
| sha256 | `8870863f4721487f7e8eb26751b16cdccb9cf20976613dae056b2759ee1093c4` |
| Built from | `crates/firmware-stm32g0-demo` (thumbv6m-none-eabi, release) |
| Chip config | `configs/chips/stm32g071.yaml` |
| System config | `configs/systems/nucleo-g071rb.yaml` |

## 1. Build the firmware

```bash
export CARGO_TARGET_DIR=/home/andrii/src/labwired/core/target
export CARGO_BUILD_JOBS=4
cargo build -p firmware-stm32g0-demo --release --target thumbv6m-none-eabi -j 4
```

Observed: `Finished release profile [optimized + debuginfo] target(s) in 1.45s`.

## 2. RCC model unit tests (G0 layout)

The G0 register map is its own `stm32g0` layout in
`crates/core/src/peripherals/rcc.rs`; these tests pin offsets and behaviour.

```bash
cargo test -p labwired-core --lib "peripherals::rcc::tests::test_rcc_g0" -j 4 -- --nocapture
```

```
test peripherals::rcc::tests::test_rcc_g0_hsi_ready_bit_positions ... ok
test peripherals::rcc::tests::test_rcc_g0_layout_and_clock_switch ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 3541 filtered out; finished in 0.01s
```

This pins CR reset `0x0000_0500` (HSION bit8 | HSIRDY bit10), the 3-bit
CFGR.SW/SWS switch (HSISYS/HSE/PLLRCLK gated on CR ready, LSI/LSE on
CSR/BDCR ready), IOPENR@0x34, AHBENR@0x38, APBENR1@0x3C (USART2EN bit17),
APBENR2@0x40 (USART1EN bit14), and the resolver names the chip yaml uses.

## 3. Smoke survival gate

```bash
cargo test -p labwired-core --test firmware_survival test_nucleo_g071rb_smoke_survival -j 4 -- --nocapture
```

```
test test_nucleo_g071rb_smoke_survival ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 73 filtered out; finished in 11.08s
```

(Final run — re-run against the committed fixture `sha256 8870863f…` after the
last source-formatting rebuild, on a freshly compiled `labwired-core`.)

The case loads the committed ELF, runs it for 800 000 cycles, asserts the PC
stays in flash/RAM, and requires `OK` in the UART capture. A wrong G0 RCC
offset (IOPENR/APBENR1) leaves USART2 clock-gated and the assertion fails —
so this is a real gate on the G0 register map, not just a boot check.

## 4. Config-build gate

```bash
cargo test -p labwired-core --test stm32g071_config -j 4 -- --nocapture
```

```
test stm32g071_from_config_builds ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Asserts the system manifest builds, `rcc`/`gpioa`/`gpioc`/`usart2`/`usart1`/
`systick` are on the bus, flash = 128 KB @ `0x08000000`, SRAM = 36 KB @
`0x20000000`, `cpu_hz = 64 MHz`. `SystemBus::from_config` errors on any
unresolved `clock:` gate, so this also pins that every gate in the yaml
resolves through the `stm32g0` layout.

## 5. Config vs vendor SVD

The ST CMSIS-SVD is vendored at `tests/fixtures/real_world/stm32g071.svd` and
wired into `crates/core/tests/svd_conformance.rs`, which checks every declared
peripheral base and IRQ against the SVD (deviations must be allow-listed).

```bash
cargo test -p labwired-core --test svd_conformance -j 4 -- --nocapture
```

```
test every_vendored_svd_is_used_by_the_gate ... ok
test chip_configs_match_their_svd ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.54s
```

## 6. Bus-proof matrix + catalog

```bash
cargo test -p labwired-core --lib bus_proof_matrix -j 4 -- --nocapture
cargo test -p labwired-core --test session_catalog -j 4 -- --nocapture
```

```
test tests::bus_proof_matrix::bus_proof_matrix_is_complete_and_not_fabricated ... ok
test tests::bus_proof_matrix::bus_proof_matrix_never_regresses ... ok
(test result: ok. 10 passed; 0 failed; ...)

test every_catalog_chip_builds_or_is_listed_in_needs ... ok
(test result: ok. 1 passed; 0 failed; ...)
```

`session_catalog` now enumerates 39 chips (38 → 39 with stm32g071).

## 7. CLI run — UART evidence

```bash
cargo run -q -p labwired-cli -- \
  --firmware tests/fixtures/nucleo-g071rb-smoke.elf \
  --system configs/systems/nucleo-g071rb.yaml \
  --max-steps 200000
```

Observed stdout:

```
OK
```

and the trailing report (stderr) records `Final PC: 0x80002ca`, 200000
instructions / 239982 cycles, exit code 0. This is the L1 success criterion:
reset vector sane, USART2 TDR bytes leave the part, no critical unmapped
access on the smoke path.

Both example scripts also pass when driven through the CLI test runner:

```bash
# firmware must exist at the path the yaml declares
mkdir -p target/thumbv6m-none-eabi/release
cp "$CARGO_TARGET_DIR/thumbv6m-none-eabi/release/firmware-stm32g0-demo" \
   target/thumbv6m-none-eabi/release/
cargo run -q -p labwired-cli -- test --script examples/nucleo-g071rb/io-smoke.yaml \
  --output-dir out/nucleo-g071rb/io-smoke --no-uart-stdout
cargo run -q -p labwired-cli -- test --script examples/nucleo-g071rb/uart-smoke.yaml \
  --output-dir out/nucleo-g071rb/uart-smoke --no-uart-stdout
```

```
PASS  2/2 checks · io-smoke · 200000 steps · 0.55s
PASS  2/2 checks · uart-smoke · 200000 steps · 0.52s
```

## 8. Register-vs-SVD coverage (ratchet baseline regenerated)

```bash
UPDATE_COVERAGE_BASELINE=1 cargo test -p labwired-core --test register_coverage -j 4 -- --nocapture
```

Measured for `stm32g071`: **653 SVD registers, 571 mapped, 500 reset-matched,
343 modeled (52.5%)**. The conservative `modeled` count requires a register to
store state or reproduce its non-zero reset; write-only and read-only-reset-0
registers are under-counted by construction. Recorded in
`docs/coverage/register-modeling.json` (ratchet: may not regress).

Related regenerations in the same session:
- `docs/coverage/chip-conformance.{json,md}` — stm32g071 **L1**, estate ✓,
  40 peripherals, behavior gate `test_nucleo_g071rb_smoke_survival`.
- `docs/coverage/bus-visibility.{json,md}` — stm32g071 **✓ UART ✓ SPI ✓ I2C**
  (the family-reused SPI/I2C models on the G0 bases drive decodable edges;
  that is shallow evidence — see the matrix gaps).

## 9. Unsupported-instruction audit

```bash
./scripts/unsupported_instruction_audit.sh \
  --firmware tests/fixtures/nucleo-g071rb-smoke.elf \
  --system configs/systems/nucleo-g071rb.yaml \
  --max-steps 200000 \
  --out-dir out/unsupported-audit/nucleo-g071rb
```

Result (script exit 0):

```
Audit summary:
  unknown_thumb16: 0
  unhandled_thumb32: 0
  unknown_riscv: 0
  unsupported_total: 0
  report: .../out/unsupported-audit/nucleo-g071rb/report.md
AUDIT_EXIT=0
```

`report.md` records 200000 instructions executed, 0 unsupported observations,
**100% instruction support coverage** on the smoke path.

## What is actually modelled vs stubbed

| Block | State |
|-------|-------|
| Cortex-M0+ core + decoder | Modelled (full Thumb-2 decode; ARMv6-M subset not enforced — build target does that) |
| Flash 128 KB / SRAM 36 KB windows | Modelled |
| SysTick, NVIC | Modelled |
| RCC `stm32g0` layout | Modelled to the register/behaviour level above; **PLL frequency not modelled** |
| GPIO A–D, F (stm32v2 layout) | Modelled; LD4 (PA5) and B1 (PC13) declared in `board_io` |
| USART2 (and USART1/3/4, LPUART1) | Modelled on the `stm32v2` USART layout; only USART2 exercised end-to-end |
| Timers TIM1/2/3/6/7/14/15/16/17, LPTIM1/2 | Declared with real bases/IRQs; family timer model, not G0-diffed |
| DMA1, CRC, RTC, IWDG, WWDG, EXTI, SYSCFG, PWR, FLASH | Declared; family models |
| I2C1/2, SPI1/2 | Declared; L4/classic controller models; bus-visibility shallow proof only |
| ADC1, DAC1 | Register windows via the L4 ADC/DAC models; no G0 analog validation |
| DBGMCU | APB @ `0x40015800`; idcode `0x460` is ST's published DEV_ID, **not bench-read** |
| UCPD, CEC, VREFBUF, COMP, DMAMUX | Not declared / not modelled |

## Known fidelity limits (honest)

1. **No silicon validation.** No NUCLEO-G071RB has been connected. Everything
   is document-derived; the SVD/header checks catch wrong addresses, not wrong
   behaviour.
2. **No executing-fidelity differential** (no walk-vs-scheduler oracle) and no
   `walk_deleted` claim for this board.
3. The **engine does not enforce ARMv6-M**; the `thumbv6m-none-eabi` toolchain
   is the only ISA guardrail.
4. **Baud/timing are functional, not cycle-accurate.** The smoke's BRR value is
   computed for 16 MHz HSISYS but is not used for wire timing in a way that
   has been validated against a scope.
5. **Interrupts are not exercised.** The smoke is a polled bring-up; no IRQ
   delivery claim is made for any G0 vector.
6. **SPI/I2C are shallow**: bus-visibility proves decodable edges on the
   controller's own lines, not a device round-trip.
7. `dbgmcu.idcode` (`0x460`) and the PLL frequency are documented constants,
   not measured values.

## Follow-ups

- Bench a NUCLEO-G071RB over SWD and capture DBGMCU_IDCODE + reset values
  (this would promote the board to `silicon-smoke` and let `reset_oracle` be
  set in `chip_conformance.rs`).
- Add a walk-vs-scheduler differential if the G0 is ever made walk-deleted.
- Extend `svd_conformance` alias mapping to cover `dbgmcu` ↔ SVD `DBG` so the
  debug block's base is checked too.
