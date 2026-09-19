# Required Source Documents (NUCLEO-G071RB)

Every base address, IRQ number, and pin assignment in this board's chip yaml
and firmware traces to one of the documents below. They are the ground truth
for any future fidelity work (no silicon capture exists for this part yet, so
the documents are the oracle).

| Doc | ID | Used for | Notes |
|-----|----|----------|-------|
| STM32G0x1 Reference Manual | **RM0444** | Memory map, RCC (G0 layout), GPIO/USART/I2C/SPI/timer register maps, DBGMCU (§42.5), EXTI cascades | Primary register source. ST DocID024196. |
| STM32G071x8/xB Datasheet | **DS12232** | 128KB flash / 36KB SRAM, 64 MHz max clock, AF table (USART2 = AF1 on PA2/PA3; LD4 = PA5) | Memory boundaries + AF mapping. |
| ST CMSIS `stm32g071xx.h` | `cmsis-device-g0` | Peripheral `*_BASE` addresses, IRQn numbers, RCC_TypeDef offsets and bits, IOPENR/AHBENR/APBENR1/APBENR2 bit positions | Header is the machine-readable cross-check of RM0444; fetched 2026-09-19. |
| STM32 Nucleo-64 User Manual | **UM2505** | LD4 (PA5), B1 (PC13 active-low), USART2 → ST-LINK VCP wiring, SWD-only debug | Board-level wiring. |
| Cortex-M0+ Technical Reference Manual | ARM **DDI 0484** | Core, SysTick, NVIC, no-JTAG/SWD-only debug, no bit-band region | Explains ARMv6-M caveats. |
| ARMv6-M Architecture Reference Manual | ARM **DDI 0419** | Legal Thumb instruction subset (toolchain target `thumbv6m-none-eabi`) | Why the firmware builds for thumbv6m. |
| ST CMSIS-SVD `STM32G071.svd` | cmsis-svd-data (Apache-2.0) | Register-coverage measurement (`tests/fixtures/real_world/stm32g071.svd`) | ST's published SVD; not an independent oracle. |

## Open items

- No NUCLEO-G071RB has been probed over SWD. The chip yaml's `dbgmcu.idcode`
  (`0x460`, ST's published G07x/G08x DEV_ID) is **not** silicon-verified.
- Per-peripheral fidelity beyond the RCC/GPIO/USART2 smoke path has not been
  diffed against silicon — see `VALIDATION.md` "Known fidelity limits".
