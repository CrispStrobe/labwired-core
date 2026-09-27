/* Frequency / duty meter in PWM-input mode (RM0368 §13.3.7), register-level.
 *
 * Signal on PA0 = TIM2_CH1 (AF1). TIM2 counts raw 84 MHz cycles (PSC=0).
 *   CH1: CC1S=01 (TI1), rising  → CCR1 = period
 *   CH2: CC2S=10 (TI1), falling → CCR2 = high time
 *   SMCR: TS=101 (TI1FP1), SMS=100 (reset mode) → CNT restarts every rise.
 * The main loop polls CC1IF, reads CCR2 then CCR1 and records a sample.
 *
 * SRAM 0x20000100: [0] samples taken, [1] last period (cycles),
 * [2] last high time (cycles), [3] frequency in Hz = 84e6 / period,
 * [4] duty in permille, [5] over-captures seen.
 */
#include "common.h"
void reset(void);
void default_handler(void) { for (;;) {} }

__attribute__((section(".vectors"), used))
const uintptr_t vectors[16] = {
    [0] = 0x20018000u,
    [1] = (uintptr_t)reset,
    [2] = (uintptr_t)default_handler,
    [3] = (uintptr_t)default_handler,
};

void reset(void) {
    for (int i = 0; i < 6; ++i) RESULT[i] = 0;
    pa0_as_tim2_ch1();
    TIM2_PSC = 0;
    TIM2_ARR = 0xFFFFFFFFu;
    TIM2_CCMR1 = (1u << 0) | (2u << 8);            /* CC1S=01, CC2S=10 */
    TIM2_CCER = (1u << 0) | (1u << 4) | (1u << 5); /* CC1E, CC2E, CC2P */
    TIM2_SMCR = (5u << 4) | 4u;                    /* TS=TI1FP1, SMS=reset */
    TIM2_EGR = 1;
    TIM2_SR = 0;
    TIM2_CR1 = 1;
    for (;;) {
        uint32_t sr = TIM2_SR;
        if (!(sr & SR_CC1IF)) continue;
        if (sr & SR_CC1OF) {
            RESULT[5] += 1;
            TIM2_SR = ~SR_CC1OF;
        }
        uint32_t high = TIM2_CCR2;
        uint32_t period = TIM2_CCR1;          /* clears CC1IF */
        if (period == 0) continue;
        RESULT[1] = period;
        RESULT[2] = high;
        RESULT[3] = 84000000u / period;
        RESULT[4] = (high * 1000u) / period;   /* high < 4.29M cycles */
        RESULT[0] += 1;
    }
}
