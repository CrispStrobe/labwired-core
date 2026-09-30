# nRF52840 comparison images. See main.c.
BUILD_DIR := build
CROSS ?= arm-none-eabi-
CC := $(CROSS)gcc

CPUFLAGS := -mcpu=cortex-m4 -mthumb -mfloat-abi=soft
CFLAGS := $(CPUFLAGS) -ffreestanding -fno-builtin -ffunction-sections \
          -fdata-sections -Os -g -Wall -Wextra -Werror
LDFLAGS := -nostdlib -T nrf.ld -Wl,--gc-sections

SRCS := startup.c main.c

ELVES := $(BUILD_DIR)/nrf-control.elf $(BUILD_DIR)/rtcclock.elf \
         $(BUILD_DIR)/flashbound.elf

all: $(ELVES)

$(BUILD_DIR):
	mkdir -p $@

$(BUILD_DIR)/nrf-control.elf: $(SRCS) nrf.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/rtcclock.elf: $(SRCS) nrf.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DRTC_CPU $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/flashbound.elf: $(SRCS) nrf.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DFLASH_BOUND $(LDFLAGS) $(SRCS) -o $@

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all clean
