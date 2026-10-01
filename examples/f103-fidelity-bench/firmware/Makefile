# Builds the fidelity-benchmark variants from one source.
#   build/control.elf    USART1 prints BENCH_UART_OK
#   build/clockbug.elf   same USART1 path, clock enabled
#   build/gpiobug.elf    GPIOA clock on, ODR readback prints BENCH_GPIO_OK
#   build/rambug.elf     in-SRAM store prints BENCH_RAM_OK
#   build/irqtime.elf    TIM2 UIF still clear prints BENCH_UIF_OK
#   build/nvicclear.elf  cleared NVIC pending prints BENCH_NVIC_OK
#   build/usartmux.elf   PA9 muxed and BRR set, prints BENCH_UART_OK
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
