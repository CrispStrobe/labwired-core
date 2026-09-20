# ESP32-C6-DevKitC-1 Validation Runbook

Run all commands from `core/`.

## 1) Ensure target installed

```bash
rustup target add riscv32imc-unknown-none-elf
```

The C6 HP core is RV32IMAC on silicon; this L1 smoke links for `riscv32imc`
because the image uses no A-extension instructions and the engine's
interpreter is exercised against that subset. The A extension decodes but is
not part of this image.

## 2) Build smoke firmware

```bash
cargo build -p firmware-esp32c6-demo --release --target riscv32imc-unknown-none-elf
```

Committed copy of the same artifact: `tests/fixtures/esp32c6-demo.elf`.

## 3) Run deterministic UART smoke

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/esp32c6-devkitc/uart-smoke.yaml \
  --output-dir out/esp32c6-devkitc/uart-smoke \
  --no-uart-stdout
```

Pass criteria:

1. exit code is `0`
2. UART contains `OK`

## 4) Run firmware survival gate

```bash
cargo test -p labwired-core --test firmware_survival test_esp32c6_demo_survival -- --nocapture
```

Pass criteria:

1. test passes
2. UART sink contains `OK` (fixture: `tests/fixtures/esp32c6-demo.elf`)

## 5) Run direct simulation for PC/SP evidence

```bash
cargo run -q -p labwired-cli -- \
  --firmware tests/fixtures/esp32c6-demo.elf \
  --system configs/systems/esp32c6-devkitc.yaml \
  --max-steps 200000
```

Pass criteria: the console shows `OK` and the run ends at the step limit with
the PC still inside the C6 flash window (0x4200_0000..0x42FF_FFFF).

## 6) Run unsupported-instruction audit

```bash
./scripts/unsupported_instruction_audit.sh \
  --firmware tests/fixtures/esp32c6-demo.elf \
  --system configs/systems/esp32c6-devkitc.yaml \
  --max-steps 200000 \
  --out-dir out/unsupported-audit/esp32c6
```

Pass criteria:

1. script exits `0`
2. audit report exists at `out/unsupported-audit/esp32c6/report.md`

## 7) Tier-1 fixture (L3: clock + gpio + timer + dma + irq)

```bash
scripts/tier1/build_esp32c6.sh

./target/debug/labwired run \
  --chip configs/chips/esp32c6.yaml \
  --firmware tests/fixtures/tier1/esp32c6.elf \
  --max-steps 8000000 2>&1 | grep -a TIER1
```

Observed (2026-09-20, `feat/l3-esp32c6`):

```text
TIER1 clock PASS
TIER1 gpio PASS
TIER1 timer PASS
TIER1 dma PASS
TIER1 irq PASS
TIER1 done
```

What each line proves:

- `clock` — `pcr` is a native register file (full SVD map): `UART0_SCLK_CONF`
  and `SYSCLK_CONF` round-trip documented values; `UART0_CONF.CLK_EN=0` makes
  UART0 reads return 0 and drops writes (the pre-gate value survives), and
  reopening the gate restores it. The same `clock:` gate is declared for
  UART1/TIMG0/TIMG1/GDMA. `RST_EN` is recorded, not enforced.
- `gpio` — `OUT`/`ENABLE` stores plus the `W1TS`/`W1TC` set/clear aliases read
  back through `OUT`/`ENABLE`; `FUNC4_OUT_SEL_CFG` and `FUNC6_IN_SEL_CFG`
  round-trip; `IN` does not follow the output latch.
- `timer` — TIMG0 (shared `esp32_timg`): `T0CONFIG.EN` set, `T0UPDATE`-latched
  `T0LO/T0HI` advances across a bounded spin; clearing `EN` freezes it.
- `dma` — GDMA (3 channels): real in-RAM linked-list mem→mem transfer;
  descriptors walked, bytes land in the destination, `IN_SUC_EOF`/`IN_DONE`
  and `OUT_TOTAL_EOF`/`OUT_DONE` latch, owners write back.
- `irq` — a real RISC-V trap: `CPU_INTR_FROM_CPU_0` (matrix source 22, INTPRI
  doorbell `@0x600C_5090`) is mapped to line 9, line 9 is enabled with a
  passing priority, and the fixture's `mtvec` entry observes
  `mcause=0x8000_0009` and acknowledges it. A second doorbell with the line
  disabled must not trap (enable-gate proof). A register round-trip of the MAP
  word is checked first.
- `uart` — implicit: the transcript arrived over UART0.

Classes without a declared peripheral type (`i2c`, `spi`, `adc`, `pwm`, `wdt`,
`rtc`) render `na` in the matrix; the fixture deliberately does not attempt
them. The six L3 rubric classes (clock, gpio, uart, timer, dma, irq) all pass.

## 8) Unsupported-instruction audit (L2 evidence)

```bash
./scripts/unsupported_instruction_audit.sh \
  --firmware tests/fixtures/tier1/esp32c6.elf \
  --system configs/systems/esp32c6-devkitc.yaml \
  --max-steps 200000 \
  --out-dir out/unsupported-audit/esp32c6-devkitc
```

Observed: `unknown_riscv: 0`, `unsupported_total: 0` (report in
`out/unsupported-audit/esp32c6-devkitc/report.md`).

