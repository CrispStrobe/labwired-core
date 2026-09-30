# nRF52840 fidelity cases

Three images from `firmware/main.c`, scored by
`scripts/perf/compare_renode_cases.py` on LabWired and on Renode's
`platforms/cpus/nrf52840.repl`. A case passes when its marker reaches the UART.
The silicon column is the expected LabWired verdict. Renode's verdict is
whatever that run prints.

| case | silicon | marker | why |
| --- | --- | --- | --- |
| `nrf-control` | PASS | `BENCH_NRF_OK` | legacy UART0 TXD prints the marker |
| `rtcclock` | FAIL | `BENCH_RTC_CPU` | RTC0 must not count in 32 CPU nops; it runs from 32.768 kHz |
| `flashgate` | FAIL | `BENCH_FLASH_OK` | a flash byte store without NVMC `CONFIG.WEN` must not stick |

Needs `arm-none-eabi-gcc`. The comparison script builds the images.
