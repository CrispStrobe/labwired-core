#!/usr/bin/env bash
# Rebuild the example's firmware (arm-none-eabi-gcc, avr-gcc). The ELFs are
# committed so the tests run without a toolchain.
set -euo pipefail
cd "$(dirname "$0")"
arm-none-eabi-gcc -mcpu=cortex-m0plus -mthumb -Os -ffreestanding -fno-builtin -nostdlib \
    -Wall -Wextra -Wl,--build-id=none -T src/stm.ld src/stm.c -o firmware/stm.elf
avr-gcc -mmcu=atmega328p -Os -Wall -Wextra src/avr.c -o firmware/avr.elf
# Contention demo: both boards drive one push-pull wire.
arm-none-eabi-gcc -mcpu=cortex-m0plus -mthumb -Os -ffreestanding -fno-builtin -nostdlib \
    -Wall -Wextra -Wl,--build-id=none -T src/stm.ld src/stm_fight.c -o firmware/stm-fight.elf
avr-gcc -mmcu=atmega328p -Os -Wall -Wextra src/avr_fight.c -o firmware/avr-fight.elf
