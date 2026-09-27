# FB200 (i.MX RT1052) Validation Runbook

Run all commands from `core/`. The system is `configs/systems/fb200.yaml`
(chip `configs/chips/mimxrt1052.yaml`); the host console is LPUART5.

## 1) Optional: ensure soft-float target installed

```bash
rustup target add thumbv7em-none-eabi
```

Do **not** use `thumbv7em-none-eabihf` for this smoke.

## 2) Build smoke firmware

```bash
cargo build -p firmware-imxrt1052-demo --release --target thumbv7em-none-eabi
```

The image links its vector table at FlexSPI `0x60010000`
(`flash.base` + `reset_vector_offset` in the chip yaml). The boot ROM, the
FlexSPI config block and the IVT are not modelled, so the image has none.

## 3) Run deterministic UART smoke

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/fb200/uart-smoke.yaml \
  --output-dir out/fb200/uart-smoke \
  --no-uart-stdout
```

Pass criteria:
1. exit code is `0`
2. UART contains `RT1052 SMOKE OK`

## 4) Run IO smoke (the CI coverage-matrix cell `fb200`)

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/fb200/io-smoke.yaml \
  --output-dir out/fb200/io-smoke \
  --no-uart-stdout
```

Pass criteria:
1. exit code is `0`
2. UART contains `RT1052 SMOKE OK`
3. GPIO4 `GDIR` bit 0 = 1 and GPIO4 `DR` bit 0 = 1 (DR_TOGGLE applied)

## 5) Run firmware survival gate

```bash
cargo test -p labwired-core --test firmware_survival test_mimxrt1052_fb200_smoke_survival -- --nocapture
```

Pass criteria:
1. test passes
2. UART sink contains `RT1052 SMOKE OK` (fixture: `tests/fixtures/fb200-rt1052-smoke.elf`)

## 6) Run with the chip alone (docs-runnable-chips gate)

```bash
cargo run -q -p labwired-cli -- run \
  --chip configs/chips/mimxrt1052.yaml \
  --firmware tests/fixtures/fb200-rt1052-smoke.elf \
  --max-steps 6000000
```
