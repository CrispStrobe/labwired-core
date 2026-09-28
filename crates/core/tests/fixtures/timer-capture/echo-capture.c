/* HC-SR04 ranging with TIM2 input capture (the CubeMX "TIM2 CH1 direct +
 * CH2 indirect" recipe), register-level.
 *
 * TRIG = PA1 (output). ECHO = PA0 = TIM2_CH1 (AF1).
 * TIM2: PSC = 83 → one tick per microsecond at 84 MHz.
 *   CH1: CC1S=01 (TI1), rising  → CCR1 = echo start
 *   CH2: CC2S=10 (TI1), falling → CCR2 = echo end, CC2IE → TIM2 IRQ (28)
 * Echo width = CCR2 - CCR1 microseconds, computed in the ISR.
 *
 * SRAM 0x20000100: [0] phase (0 idle, 1 triggered, 2 captured),
 * [1] rise tick, [2] fall tick, [3] width (µs), [4] ISR count,
 * [5] SR as the ISR found it.
 * The firmware holds no expected distance: the test compares [3] with the
 * distance it injected.
 */
#include "common.h"
void reset(void);
void tim2_irq(void);
void default_handler(void) { for (;;) {} }

__attribute__((section(".vectors"), used))
const uintptr_t vectors[16 + 29] = {
    [0] = 0x20018000u,
    [1] = (uintptr_t)reset,
    [2] = (uintptr_t)default_handler,
    [3] = (uintptr_t)default_handler,
    [16 + 28] = (uintptr_t)tim2_irq,
};

void tim2_irq(void) {
    uint32_t sr = TIM2_SR;
    if (sr & SR_CC2IF) {
        uint32_t fall = TIM2_CCR2;   /* reading CCR2 clears CC2IF */
        uint32_t rise = TIM2_CCR1;   /* and CCR1 clears CC1IF */
        RESULT[1] = rise;
        RESULT[2] = fall;
        RESULT[3] = fall - rise;
        RESULT[5] = sr;
        RESULT[0] = 2;
    }
    RESULT[4] += 1;
}

void reset(void) {
    for (int i = 0; i < 6; ++i) RESULT[i] = 0;
    pa0_as_tim2_ch1();
    GPIOA_MODER = (GPIOA_MODER & ~(3u << 2)) | (1u << 2);   /* PA1 output */

    TIM2_PSC = 83;
    TIM2_ARR = 0xFFFFFFFFu;
    TIM2_CCMR1 = (1u << 0) | (2u << 8);    /* CC1S=01 TI1, CC2S=10 TI1 */
    TIM2_CCER = (1u << 0) | (1u << 4) | (1u << 5); /* CC1E, CC2E, CC2P=falling */
    TIM2_EGR = 1;                          /* load PSC */
    TIM2_SR = 0;
    TIM2_DIER = SR_CC2IF;                  /* CC2IE */
    NVIC_ISER0 = 1u << 28;
    TIM2_CR1 = 1;

    GPIOA_BSRR = 1u << 1;                  /* TRIG high ... */
    for (volatile unsigned wait = 0; wait < 300; ++wait) __asm__ volatile ("nop");
    GPIOA_BSRR = 1u << (1 + 16);           /* ... ≥10 µs, then low */
    RESULT[0] = 1;
    for (;;) __asm__ volatile ("wfi");
}
