# F103 fidelity benchmark

A faithful emulator must **fail the firmware that real hardware fails**. One that
passes a known-bad firmware gives a *false pass* — a green CI run hiding a bug.
This suite runs deliberately-broken firmware on LabWired and checks its verdict
against the STM32F103C8 datasheet, so false-pass prevention is measured, not
asserted. It doubles as a CI fidelity regression guard.

## Result

```
case       real-HW   LabWired
control    PASS      PASS
clockbug   FAIL      FAIL
gpiobug    FAIL      FAIL
rambug     FAIL      FAIL
                     4/4
```

LabWired reproduces the real silicon on every case because it models RCC clock
gating and the real 20 KB SRAM — the behaviour a passing test never exercises is
exactly where a false pass would otherwise hide.

## Cases

One firmware (`firmware/main.c`), one line changed each:

- `control` — correct; enables the USART1 clock → **PASS**
- `clockbug` — forgets `RCC_APB2ENR.USART1EN`; TXE never asserts → **FAIL**
- `gpiobug` — drives GPIOA without `IOPAEN`; writes dropped → **FAIL**
- `rambug` — stores 4 KB past the 20 KB SRAM; faults → **FAIL**

A case passes iff its marker (`BENCH_*_OK`) reaches the UART.

`irqtime` arms TIM2 (`ARR` 1000) and spins a few dozen cycles. `BENCH_UIF_OK` prints only if the update flag is already set. Silicon leaves it clear.

`nvicclear` sets and then clears the NVIC pending bit for IRQ0 while PRIMASK is set. `BENCH_NVIC_OK` prints only if that ISR still runs. Silicon does not enter it.

`usartmux` clocks USART1, leaves PA9 at reset, and leaves BRR at 0. `BENCH_UART_OK` is what it tries to send. Silicon sends nothing without the pad mux and a baud divisor.

The nRF52840 images live in `examples/nrf52840-fidelity-bench`. `scripts/perf/compare_renode_cases.py` runs every case on LabWired and, when a Renode binary is passed, on Renode. LabWired has to match the silicon verdict. Renode's verdict is the one that run prints.

## Run

```bash
./run-benchmark.sh
```

Exits non-zero if LabWired ever disagrees with silicon, and writes
`benchmark-results.json`. `system-nogate.yaml` runs the same firmware on a
clock-gating-stripped chip: it false-passes there, showing the gates are what
catch the bug.

Needs `arm-none-eabi-gcc` and a built `labwired` (`cargo build -p labwired-cli`).
