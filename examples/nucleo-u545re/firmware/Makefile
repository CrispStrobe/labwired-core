# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
# MIT License - see the LICENSE file in the project root.
#
# Builds the committed fixture tests/fixtures/nucleo-u545re-blinky.elf.
# Requires arm-none-eabi-gcc (13.2.1 used).
OUT := ../../../tests/fixtures/nucleo-u545re-blinky.elf
CFLAGS := -mcpu=cortex-m33 -mthumb -Os -g -ffreestanding -fno-builtin -Wall -Wextra \
          -ffunction-sections -fdata-sections
all: $(OUT)
$(OUT): blinky.c link.ld
	arm-none-eabi-gcc $(CFLAGS) -nostdlib -Wl,--gc-sections -Tlink.ld blinky.c -o $@
	arm-none-eabi-size $@
