# External Components (NUCLEO-U545RE-Q)

No external simulated components: the smoke tests use on-chip peripherals only
(RCC, GPIO PA5/PC13/PA9/PA10, USART1, SysTick, NVIC).

The Arduino matrix attaches a declarative kit (a model, not a hardware
requirement): `validation/arduino-matrix/systems/stm32u545.yaml` - INA219 at
`0x40` on `i2c1`, MAX31855 on `spi1` (CS `PA4`).

## CubeU5 checkout (external, not committed)

The vendor HAL firmware links against a stock STM32CubeU5 checkout, a sibling of
this repository (override with `STM32CUBE_U5_DIR=/path`):

```bash
git clone --depth 1 https://github.com/STMicroelectronics/STM32CubeU5 ../STM32CubeU5
# revision used: 12d19a5358da129dc74aecff1adee370218ca186
# HAL driver submodule 0e5fefb8dc2d6afa60816ebbf8b1672cfec4595b
```

Needs the `Drivers/STM32U5xx_HAL_Driver` and `Drivers/CMSIS/Device/ST/STM32U5xx`
submodules. The BSP submodule is not needed. Toolchain: `arm-none-eabi-gcc` 13.2.1.
