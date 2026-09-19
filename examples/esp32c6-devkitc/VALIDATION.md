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
