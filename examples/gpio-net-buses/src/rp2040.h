/* RP2040 registers the bus-over-nets firmware uses (RP2040 datasheet). */
#ifndef RP2040_H
#define RP2040_H
#include <stdint.h>

#define REG(a) (*(volatile uint32_t *)(a))
#define IO_CTRL(n)  REG(0x40014004u + 8u * (n))    /* GPIOn_CTRL */
#define SSPCR0      REG(0x4003C000u)
#define SSPCR1      REG(0x4003C004u)
#define SSPDR       REG(0x4003C008u)
#define SSPSR       REG(0x4003C00Cu)
#define SSPCPSR     REG(0x4003C010u)

#define FUNC_SPI 1u
#define SR_TFE (1u << 0)
#define SR_RNE (1u << 2)
#define SR_BSY (1u << 4)
#define CR1_SSE (1u << 1)
#define CR1_MS  (1u << 2)
#define CR0_SPO (1u << 6)
#define CR0_SPH (1u << 7)

/* Results the tests read, by address. */
#define RESULT ((volatile uint32_t *)0x20000100u)

static inline void delay(uint32_t loops) {
    for (volatile uint32_t i = 0; i < loops; ++i) {}
}

void reset(void);
void default_handler(void) { for (;;) {} }

__attribute__((section(".vectors"), used))
const uintptr_t vectors[16 + 26] = {
    [0] = 0x20040000u,
    [1] = (uintptr_t)reset,
    [2] = (uintptr_t)default_handler,
    [3] = (uintptr_t)default_handler,
};
#endif
