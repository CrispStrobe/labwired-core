/* Register-level STM32F401 definitions shared by the timer-capture fixtures.
 * Addresses: RM0368 Rev 5 (memory map §2.3, RCC §6.3, GPIO §8.4, TIM2-5 §13.4).
 */
#include <stdint.h>
#define REG(address) (*(volatile uint32_t *)(address))
#define RCC_AHB1ENR REG(0x40023830u)
#define RCC_APB1ENR REG(0x40023840u)
#define GPIOA_MODER REG(0x40020000u)
#define GPIOA_AFRL  REG(0x40020020u)
#define GPIOA_BSRR  REG(0x40020018u)
#define TIM2_CR1   REG(0x40000000u)
#define TIM2_SMCR  REG(0x40000008u)
#define TIM2_DIER  REG(0x4000000Cu)
#define TIM2_SR    REG(0x40000010u)
#define TIM2_EGR   REG(0x40000014u)
#define TIM2_CCMR1 REG(0x40000018u)
#define TIM2_CCER  REG(0x40000020u)
#define TIM2_CNT   REG(0x40000024u)
#define TIM2_PSC   REG(0x40000028u)
#define TIM2_ARR   REG(0x4000002Cu)
#define TIM2_CCR1  REG(0x40000034u)
#define TIM2_CCR2  REG(0x40000038u)
#define NVIC_ISER0 REG(0xE000E100u)
#define RESULT ((volatile uint32_t *)0x20000100u)
#define SR_CC1IF (1u << 1)
#define SR_CC2IF (1u << 2)
#define SR_CC1OF (1u << 9)

/* PA0 = TIM2_CH1 (AF1, DS9716 Table 9). */
static inline void pa0_as_tim2_ch1(void) {
    RCC_AHB1ENR |= 1u;           /* GPIOAEN */
    RCC_APB1ENR |= 1u;           /* TIM2EN */
    GPIOA_MODER = (GPIOA_MODER & ~3u) | 2u;      /* PA0 alternate function */
    GPIOA_AFRL = (GPIOA_AFRL & ~0xFu) | 1u;      /* AF1 */
}
