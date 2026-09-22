/* Busy-poll one DHT response. SRAM holds count followed by 83 pulse widths
 * measured in identical polling-loop iterations. No expected data is embedded.
 */
#include <stdint.h>
#define REG(address) (*(volatile uint32_t *)(address))
#define GPIOC 0x48000800u
#define RESULT ((volatile uint32_t *)0x20000100u)
void reset(void);
__attribute__((section(".vectors"), used))
const uintptr_t vectors[] = {0x20010000u, (uintptr_t)reset};
void reset(void) {
    REG(0x4002104cu) = 4;
    REG(GPIOC + 0x14) = 4;
    REG(GPIOC + 0x04) = 4; /* Open drain: ODR HIGH releases the wire. */
    REG(GPIOC) = 0x10;
    RESULT[0] = 0;
    REG(GPIOC + 0x14) = 0;
    for (volatile unsigned wait = 0; wait < 30000; ++wait) {
        __asm__ volatile ("nop");
    }
    REG(GPIOC + 0x14) = 4;
    while (REG(GPIOC + 0x10) & 4) {}
    unsigned previous = 0;
    for (unsigned pulse = 0; pulse < 83; ++pulse) {
        unsigned width = 0;
        while ((REG(GPIOC + 0x10) & 4) == previous) ++width;
        RESULT[pulse + 1] = width;
        RESULT[0] = pulse + 1;
        previous ^= 4;
    }
    for (;;) __asm__ volatile ("nop");
}
