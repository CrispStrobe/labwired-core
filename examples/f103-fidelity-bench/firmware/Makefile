# Builds the fidelity-benchmark variants from one source.
#   build/control.elf    correct firmware (positive control)
#   build/clockbug.elf   forgets RCC_APB2ENR.USART1EN
#   build/gpiobug.elf    drives GPIOA without IOPAEN
#   build/rambug.elf     stores past the end of the 20 KB SRAM
#   build/irqtime.elf    TIM2 update must not latch on the enable tick
#   build/nvicclear.elf  cleared NVIC pending must not enter the ISR
#   build/usartmux.elf   USART1 with no pad mux and no baud divisor
BUILD_DIR := build
CROSS ?= arm-none-eabi-
CC := $(CROSS)gcc

CPUFLAGS := -mcpu=cortex-m3 -mthumb
CFLAGS := $(CPUFLAGS) -ffreestanding -fno-builtin -ffunction-sections \
          -fdata-sections -Os -g -Wall -Wextra -Werror
LDFLAGS := -nostdlib -T bench.ld -Wl,--gc-sections

SRCS := startup.c main.c

ELVES := $(BUILD_DIR)/control.elf $(BUILD_DIR)/clockbug.elf \
         $(BUILD_DIR)/gpiobug.elf $(BUILD_DIR)/rambug.elf \
         $(BUILD_DIR)/irqtime.elf $(BUILD_DIR)/nvicclear.elf \
         $(BUILD_DIR)/usartmux.elf

all: $(ELVES)

$(BUILD_DIR):
	mkdir -p $@

$(BUILD_DIR)/control.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/clockbug.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DSKIP_UART_CLOCK $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/gpiobug.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DGPIO_CLOCK_BUG $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/rambug.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DRAM_OVERFLOW $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/irqtime.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DIRQ_TIME $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/nvicclear.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DNVIC_CLEAR $(LDFLAGS) $(SRCS) -o $@

$(BUILD_DIR)/usartmux.elf: $(SRCS) bench.ld | $(BUILD_DIR)
	$(CC) $(CFLAGS) -DUSART_MUX_BUG $(LDFLAGS) $(SRCS) -o $@

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all clean
