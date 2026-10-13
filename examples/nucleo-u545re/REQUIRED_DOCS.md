# Required Source Documents (NUCLEO-U545RE-Q)

st.com was **not reachable** from the machine this port was written on, so ST's
PDFs (datasheet, RM0456, board user manual) were not read directly. Every fact
below is cited to an ST-published repository file instead. Where only a PDF could
settle a fact it is listed as unverified at the end.

## MCU

1. Register map, bases, IRQs, reset values - ST SVD `STM32U545.svd` v1.7, from the
   STM32_SVD pack (extracted from STM32CubeCLT, ST copyright, Apache-2.0):
   https://github.com/stm32duino/STM32_SVD. Committed as
   `tests/fixtures/real_world/stm32u545.svd`, sha256
   `080f9c21fa0e636ece8883e1d5707fc538f259b56e6c5ca46cfe02cce4245330`.
2. CMSIS device header `stm32u545xx.h` (STM32CubeU5 `12d19a5358da129dc74aecff1adee370218ca186`):
   SRAM1 192 KiB `0x20000000`, SRAM2 64 KiB `0x20030000`, SRAM4 16 KiB `0x28000000`,
   flash 512 KiB (`FLASH_SIZE_DEFAULT 0x80000`), `FLASH_BANK_SIZE = FLASH_SIZE >> 1`,
   8 KiB pages, DBGMCU base `0xE0044000`.
3. ST linker script `STM32U545xx_FLASH.ld` (STM32CubeU5): RAM 256K, SRAM4 16K, FLASH 512K.
4. ST open pin data `mcu/STM32U545RETxQ.xml` (commit `7d1f1514ed5583ec5007ad91236b4e1d377295b1`,
   the commit already vendored under `packages/api/vendor/st-open-pin-data`):
   package LQFP64, Flash 512 KB, RAM 274 KB (= 256 + 16 + 2 KiB BKPSRAM).
   https://github.com/STMicroelectronics/STM32_open_pin_data
5. Max CPU clock 160 MHz: Zephyr `boards/st/nucleo_u545re_q` (`rcc` `clock-frequency`).

## Board (NUCLEO-U545RE-Q, MB1841)

1. ST open pin data `boards/B56_Nucleo_NUCLEO-U545RE-Q_STM32U545RE_Board_AllConfig.ioc`
   (same commit): PA5 `LED_GREEN`, PC13 `USER_BUTTON` (pull-down), PA9/PA10
   `USART1_TX/RX`, PA11/PA12 USB.
2. ST `stm32u5xx-nucleo-bsp` `stm32u5xx_nucleo.h` (board name `NUCLEO-U545RE-Q`, id
   `MB1841A`): `LED2` = PA5, `BUTTON_USER` = PC13, `COM1` = USART1 PA9/PA10 AF7.
   https://github.com/STMicroelectronics/stm32u5xx-nucleo-bsp
3. Zephyr `boards/st/nucleo_u545re_q` (`nucleo_u545re_q.dts`, `arduino_r3_connector.dtsi`,
   `doc/index.rst`): `led0` PA5 active high, `sw0` PC13 active high + pull-down,
   console USART1, Arduino header map (A0 PA0, A1 PA1, A2 PA4, A3 PB0, A4 PC1, A5 PC0,
   D0 PA3, D1 PA2, D2 PC8, D3 PB3, D4 PB5, D5 PB4, D6 PB10, D7 PA8, D8 PC7, D9 PC6,
   D10 PC9, D11 PA7, D12 PA6, D13 PA5, D14 PB7, D15 PB6), I2C1 PB6/PB7, SPI1 PA5-PA7,
   LPUART1 PA2/PA3.
   https://github.com/zephyrproject-rtos/zephyr/tree/main/boards/st/nucleo_u545re_q
4. STM32CubeU5 `Projects/NUCLEO-U545RE-Q/Templates/TrustZoneDisabled` (clock config
   reproduced by `board_firmware/main.c`: MSI 4 MHz, PLL1 x80, SMPS, latency 4).

## Not verified from a primary source

- **Morpho connector (CN7/CN10) pin assignment**: only in the board user manual
  (UM3062), which was unreachable. The art and docs do not enumerate Morpho pins.
- **Datasheet electrical limits, RM0456 prose**: not read; register facts come from
  the SVD.
- **Board outline / component positions in `images/`**: ST's CAD files (board design
  project and manufacturing files on st.com) were unreachable; positions were
  measured from ST's product photograph. See `images/gen_images.py`.
- **TrustZone factory state**: assumed TZEN=0 as on the U575 board (UM2883); not
  re-checked for MB1841.

## Reference documents to consult when st.com is reachable

- RM0456 (STM32U5 reference manual), DS14086 (STM32U535/545 datasheet), UM3062
  (STM32U3/U5 Nucleo-64 boards, MB1841), MB1841 board design + manufacturing files.
