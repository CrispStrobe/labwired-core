# External components — NUCLEO-G071RB

The smoke/onboarding demo uses **no external components**. Everything it
touches is on-chip or on the Nucleo carrier board:

| Component | Where | Role in the demo |
|-----------|-------|------------------|
| LD4 (green LED) | on-board, PA5 | toggled by the demo (BSRR) |
| B1 (user button) | on-board, PC13 | declared in `board_io`, not driven by the demo |
| ST-LINK/V3 VCP | on-board debug MCU | carries USART2 TX bytes to the host |

`external_devices: []` in the system manifest reflects this — no sensors,
displays, or bus peripherals are wired.

## Adding external devices later

The chip yaml declares I2C1/I2C2, SPI1/SPI2 and USART1/3/4 for future labs:

- **I2C1** (`0x40005400`) — PB8/PB9 (Arduino I2C per Zephyr DTS).
- **SPI1** (`0x40013000`) — PB0/PA5/PA6/PA7 (Arduino SPI; PA5 is shared with
  LD4, so prefer SPI2 or remap if the LED is also used).
- **USART1** (`0x40013800`) — spare UART.

However, none of those buses has been validated end-to-end for G0 yet: the
I2C/SPI/ADC register models are reused from the L0/L4 families (see
`VALIDATION.md`). Re-validate a bus against `RM0444` (and ideally silicon)
before marking a new external-device lab as accurate.
