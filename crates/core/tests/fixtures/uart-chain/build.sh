#!/usr/bin/env bash
# Rebuild the uart-chain fixture ELFs (arm-none-eabi-gcc; tested with GCC 16.1).
set -euo pipefail
cd "$(dirname "$0")"
# shellcheck disable=SC2054  # -Wl,--build-id=none is one argument
CF=(-mcpu=cortex-m4 -mthumb -Os -ffreestanding -fno-builtin -nostdlib -Wall -Wextra
    -Wl,--build-id=none -T f401.ld)
arm-none-eabi-gcc "${CF[@]}" -DROLE_SOURCE uart-chain.c -o uart-chain-source.elf
arm-none-eabi-gcc "${CF[@]}" -DROLE_RELAY uart-chain.c -o uart-chain-relay.elf
# An RX interrupt that spends 150 us per character: too slow for 115200 baud.
arm-none-eabi-gcc "${CF[@]}" -DROLE_RELAY -DRX_ISR_EXTRA_CYCLES=12600 uart-chain.c \
    -o uart-chain-relay-slow-rx.elf
# A relay programmed for 57600 baud (BRR 0x5B2) on a 115200 line.
arm-none-eabi-gcc "${CF[@]}" -DROLE_RELAY -DUSART_BRR=0x5B2 uart-chain.c \
    -o uart-chain-relay-57600.elf
