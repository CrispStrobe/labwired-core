/* STM32G0B1 half of the contention demo: PB1 is a push-pull output that goes
 * high 50 us after reset and low again 50 us later, while the AVR holds the
 * same wire low (avr_fight.c). Two drivers, opposite levels. */
#include <stdint.h>
#define REG(a) (*(volatile uint32_t *)(a))
#define RCC_IOPENR  REG(0x40021034u)
#define GPIOB_MODER REG(0x50000400u)
#define GPIOB_BSRR  REG(0x50000418u)

static void delay(uint32_t n) { for (volatile uint32_t i = 0; i < n; ++i) {} }

void reset(void);
void default_handler(void) { for (;;) {} }
__attribute__((section(".vectors"), used))
const uintptr_t vectors[4] = {0x20009000u, (uintptr_t)reset, (uintptr_t)default_handler,
                              (uintptr_t)default_handler};

void reset(void) {
    RCC_IOPENR |= 3u;
    GPIOB_MODER = (GPIOB_MODER & ~(3u << 2)) | (1u << 2);
    delay(150);
    GPIOB_BSRR = 1u << 1;
    delay(150);
    GPIOB_BSRR = 1u << 17;
    for (;;) {}
}
