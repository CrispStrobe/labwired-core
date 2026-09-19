# micro:bit v2 Validation Runbook

Run all commands from `core/`.

## 1) Optional: ensure target installed

```bash
rustup target add thumbv7em-none-eabi
```

## 2) Build smoke firmware

```bash
cargo build -p firmware-nrf52833-demo --release --target thumbv7em-none-eabi
```

## 3) Run deterministic UART smoke

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/microbit-v2/uart-smoke.yaml \
  --output-dir out/microbit-v2/uart-smoke \
  --no-uart-stdout
```

Pass criteria:

1. exit code is `0`
2. UART contains `OK`

## 4) Run firmware survival gate

```bash
cargo test -p labwired-core --test firmware_survival test_nrf52833_microbit_v2_smoke_survival -- --nocapture
```

Pass criteria:

1. test passes
2. UART sink contains `OK` (fixture: `tests/fixtures/microbit-v2-smoke.elf`)

## 5) Run direct simulation for PC/SP evidence

```bash
cargo run -q -p labwired-cli -- \
  --firmware target/thumbv7em-none-eabi/release/firmware-nrf52833-demo \
  --system configs/systems/microbit-v2.yaml \
  --max-steps 32 \
  --json
```

## 6) Run unsupported-instruction audit

```bash
./scripts/unsupported_instruction_audit.sh \
  --firmware target/thumbv7em-none-eabi/release/firmware-nrf52833-demo \
  --system configs/systems/microbit-v2.yaml \
  --max-steps 200000 \
  --out-dir out/unsupported-audit/microbit-v2
```

Pass criteria:

1. script exits `0`
2. audit report exists at `out/unsupported-audit/microbit-v2/report.md`

## 7) Tier-1 peripheral self-tests (raw-register depth)

The fixture is a standalone crate (own `[workspace]`), so it builds with its
own target dir rather than the workspace root's.

```bash
cd examples/tier1-fixture/nrf52833
cargo build --release --target thumbv7em-none-eabi
cd ../../..

# Only when re-cutting the committed blob:
cp examples/tier1-fixture/nrf52833/target/thumbv7em-none-eabi/release/tier1-fixture-nrf52833 \
   tests/fixtures/tier1/nrf52833.elf

labwired run --chip configs/chips/nrf52833.yaml \
  --firmware tests/fixtures/tier1/nrf52833.elf --max-steps 8000000 \
  2>&1 | grep -a TIER1
```

Observed transcript (verbatim):

```text
TIER1 gpio PASS
TIER1 clock PASS
TIER1 timer PASS
TIER1 rtc PASS
TIER1 i2c PASS
TIER1 spi PASS
TIER1 adc PASS
TIER1 wdt PASS
TIER1 pwm PASS
TIER1 done
```

Pass criteria:

1. all nine printed classes report `PASS` (UART is implicit via `TIER1 done`)
2. `TIER1 done` is present — the fixture completed its whole sequence
3. `dma`/`irq` stay `na` (no DMA/NVIC peripheral type is declared in the chip
   yaml), same as the nrf52832/nrf52840 rows
