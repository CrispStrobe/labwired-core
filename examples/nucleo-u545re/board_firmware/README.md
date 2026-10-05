# NUCLEO-U545RE-Q STM32CubeU5 HAL Smoke Firmware

Real-toolchain firmware using the STM32CubeU5 HAL (no BSP): MSI 4 MHz -> PLL1
160 MHz (`FLASH_LATENCY_4`, SMPS, VOS1; the sequence in CubeU5
`Projects/NUCLEO-U545RE-Q/Templates/TrustZoneDisabled`), ICACHE, USART1 VCP, LD2 on
PA5, B1 on PC13.

Output: `U545-HAL OK`, then every 250 ms `BLINK <n> LD2=<0|1> B1=<0|1>`.

```bash
make                    # needs ../../../../STM32CubeU5 (or STM32CUBE_U5_DIR=...)
# -> build/u545_hal_smoke.elf (committed copy: tests/fixtures/nucleo-u545re-cubehal.elf)
```

`stm32u5xx_hal_conf.h` is the U575 example's trimmed copy (HAL core, RCC, PWR, FLASH,
GPIO, UART, CORTEX, ICACHE). Run instructions: `../README.md`.
