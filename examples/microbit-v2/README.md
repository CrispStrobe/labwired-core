# micro:bit v2 Onboarding Example

Run all commands from `core/`.

## Purpose

This example provides deterministic bring-up for the BBC micro:bit v2
(Nordic nRF52833) using the minimal supported subset:

1. `uart0` — UARTE0 EasyDMA console bridged to the interface MCU (TX = P0.06)
2. `gpio0` / `gpio1` — buttons A (P0.14) and B (P0.23) are declared as inputs
3. `clock` — HFCLK/LFCLK behavioural model

The 5x5 LED matrix (charlieplexed), radio/BLE, USB, speaker and microphone are
intentionally omitted.

**SIM-DERIVED.** Not silicon-verified. No executing-fidelity differential
exists for this part yet; the smoke proves the UARTE EasyDMA console path
end-to-end and nothing more.

## Quick Run

```bash
cargo build -p firmware-nrf52833-demo --release --target thumbv7em-none-eabi
cargo run -q -p labwired-cli -- test --script examples/microbit-v2/uart-smoke.yaml --output-dir out/microbit-v2/uart-smoke --no-uart-stdout
```

Expected result:

1. smoke test passes
2. UART contains `OK`

## Files

1. `system.yaml`: local board mapping for simulation runs.
2. `uart-smoke.yaml`: deterministic UART smoke assertion.
3. `REQUIRED_DOCS.md`: source-grounding references (nRF52833 PS, micro:bit docs).
4. `EXTERNAL_COMPONENTS.md`: external component declaration.
5. `VALIDATION.md`: reproducible validation/audit commands.
