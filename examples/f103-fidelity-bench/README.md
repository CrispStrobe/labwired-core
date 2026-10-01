# F103 images

One firmware (`firmware/main.c`). Each image enables the clocks it uses, keeps its store inside the 20 KB SRAM, and prints a marker. A case passes when that marker reaches the UART. LabWired must print it.

| case | marker |
| --- | --- |
| `control` | `BENCH_UART_OK` |
| `clockbug` | `BENCH_UART_OK` |
| `gpiobug` | `BENCH_GPIO_OK` |
| `rambug` | `BENCH_RAM_OK` |
| `irqtime` | `BENCH_UIF_OK` |
| `nvicclear` | `BENCH_NVIC_OK` |
| `usartmux` | `BENCH_UART_OK` |

`irqtime` arms TIM2 (`ARR` 1000), spins a few dozen cycles, and prints when `SR.UIF` is still clear. `nvicclear` sets and clears the NVIC pending bit for IRQ0 while PRIMASK is set, and prints when that ISR does not run. `usartmux` muxes PA9 and programs BRR before enabling the transmitter.

The nRF52840 images live in `examples/nrf52840-fidelity-bench`. `scripts/perf/compare_renode_cases.py` runs every case on LabWired and, when a Renode binary is passed, on Renode. LabWired has to match the silicon verdict. Renode's verdict is the one that run prints.

## Run

```bash
./run-benchmark.sh
```

Exits non-zero unless LabWired prints every marker, and writes
`benchmark-results.json`.

Needs `arm-none-eabi-gcc` and a built `labwired` (`cargo build -p labwired-cli`).
