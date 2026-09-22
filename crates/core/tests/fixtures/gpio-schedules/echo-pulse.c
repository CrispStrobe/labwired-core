/* Trigger HC-SR04 on PA0 and measure PB0 echo in polling-loop iterations.
 * SRAM: phase (0 waiting, 1 high, 2 finished), measured high width.
 */
#include <stdint.h>
#define REG(address) (*(volatile uint32_t *)(address))
#define RESULT ((volatile uint32_t *)0x20000100u)
void reset(void);
__attribute__((section(".vectors"), used))
const uintptr_t vectors[] = {0x20010000u, (uintptr_t)reset};
void reset(void) {
    REG(0x4002104cu) = 3;
    REG(0x48000000u) = 1;
    REG(0x48000400u) = 0;
    RESULT[0] = 0;
    RESULT[1] = 0;
    REG(0x48000014u) = 1;
    for (volatile unsigned wait = 0; wait < 200; ++wait) {
        __asm__ volatile ("nop");
    }
    REG(0x48000014u) = 0;
    while (!(REG(0x48000410u) & 1)) {}
    RESULT[0] = 1;
    unsigned width = 0;
    while (REG(0x48000410u) & 1) ++width;
    RESULT[1] = width;
    RESULT[0] = 2;
    for (;;) __asm__ volatile ("nop");
}
