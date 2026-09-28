#!/bin/sh
# Rebuild the committed boot-logger ELF. Needs arm-none-eabi binutils.
set -eu
cd "$(dirname "$0")"
arm-none-eabi-as -mcpu=cortex-m4 -mthumb -o bootlog.o bootlog.S
arm-none-eabi-ld -T link.ld -o ../stm32f401-supply-bootlog.elf bootlog.o
rm -f bootlog.o
