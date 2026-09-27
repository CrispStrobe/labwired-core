STM32F401 (Cortex-M4) fixtures for TIM2 input capture. Register-level C, no
HAL, no expected values: each firmware measures what arrives on PA0
(`TIM2_CH1`, AF1) and records it in SRAM at 0x20000100. The tests inject the
pulses and compare.

* `echo-capture.c` — HC-SR04 ranging: CH1 captures the rising edge, CH2 (TI1,
  falling) the falling edge, CC2IE interrupt computes the width in µs.
* `freq-meter.c` — PWM-input mode: slave reset mode on TI1FP1, CCR1 = period,
  CCR2 = high time, polled.

Rebuild from this directory:

```sh
for f in echo-capture freq-meter; do
  arm-none-eabi-gcc -mcpu=cortex-m4 -mthumb -Os -ffreestanding -fno-builtin -nostdlib \
    -Wl,--build-id=none -T f401.ld $f.c -o $f.elf
done
```
