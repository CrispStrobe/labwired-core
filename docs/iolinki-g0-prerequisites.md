# STM32G0 IO-Link firmware prerequisites

The customer STM32G0B1RE/TIOL112 reference ELF boots on the G0 descriptor,
configures its 1 MHz TIM2 timebase, and executes its own EXTI0/1 wake handler.
The tests assert that firmware clears the falling-edge flag and programs
USART1 for COM2. GPIO PA9 uses the G0 AF1 UART output route rather than the
L0 AF4 route; mux, polarity, split pending flags and selective W1C behavior
have separate regressions.

This is a simulator prerequisite. It does not yet exercise an electrical C/Q
link, master exchange, cable loading, supply faults or a physical board.
The descriptor includes only the peripherals used by this reference. Its
GPIO EXTI path excludes software triggers and internal event lines. The
128 KiB parity-protected SRAM region is exposed; additional parity storage
is excluded. The general Cortex-M interpreter does not enforce every M0+
instruction restriction; the actual firmware is compiled for Cortex-M0+.

Sources: [ST STM32G0B1 datasheet](https://www.st.com/resource/en/datasheet/stm32g0b1re.pdf),
[RM0444](https://www.st.com/resource/en/reference_manual/rm0444-stm32g0x1-advanced-armbased-32bit-mcus-stmicroelectronics.pdf)
and ST CMSIS device headers pinned by the workflow.

Build the customer reference as documented in
[iolinki](https://github.com/w1ne/iolinki/tree/5572a628b8befa6a9c74496311da2359868f96d9/examples/stm32g0_tiol112),
then run from this repository:

```sh
export IOLINKI_G0_ELF=/absolute/path/to/reference-device.elf
cargo test -p labwired-core --test g0_uart_pad --test g0_exti --test g0_reference_firmware -- --include-ignored
cargo test -p labwired-core --features event-scheduler --test g0_uart_pad --test g0_exti --test g0_reference_firmware -- --include-ignored
```

The firmware test is explicitly ignored without an external build; the CI
workflow builds the pinned customer sources and requires this test in both
execution modes. Missing firmware is an error in that test. The workflow
records the ELF SHA-256 and archives the ELF/map/HEX artifacts.

Local witness on 2026-10-01: ARM GCC 13.2.1 produced ELF SHA-256
`44b97d0d7b2dd6d9e9800626728a2b034b2cd856033e8fbeab25a2d521eb00a7`.
All five focused tests passed in both modes. The ELF wake test executes
500,000 startup instructions and 100,000 after the external edge; its
assertions inspect registers configured or cleared by the actual firmware.
The SHA identifies that local build, not every compiler's future output.
