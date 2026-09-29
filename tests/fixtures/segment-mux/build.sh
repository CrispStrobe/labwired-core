#!/bin/sh
# Rebuild segment-mux-thumbv7m.elf from main.S. Needs arm-none-eabi-gcc.
set -e
cd "$(dirname "$0")"
CROSS="${CROSS:-arm-none-eabi-}"
"${CROSS}gcc" -mcpu=cortex-m3 -mthumb -nostdlib -T link.ld main.S -o segment-mux-thumbv7m.elf
