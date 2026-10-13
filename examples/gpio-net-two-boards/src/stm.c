/* STM32G0B1 side of the gpio-net-two-boards example. Register level, no
 * libraries. Three wires go to the ATmega328P board:
 *
 *   irq    PB0  input, EXTI line 0, both edges     <- AVR PD2 (push-pull)
 *   ready  PB1  output, push-pull                  -> AVR PD3
 *   alert  PB4  open-drain, shared, 10k pull-up    <-> AVR PD4
 *
 * Sequence (the AVR runs the other half):
 *   1. count the 10 pulses the AVR puts on irq (EXTI0, RPR/FPR flags);
 *   2. put 7 pulses on ready;
 *   3. pull alert low 5 times (the AVR counts them);
 *   4. enable EXTI on alert and count the 3 pulses the AVR pulls;
 *   5. report over USART1.
 *
 * SRAM 0x20000100 (the test reads it):
 *   [0] irq rising   [1] irq falling   [2] alert rising   [3] alert falling
 *   [4] 1 when the report has been sent
 */
#include <stdint.h>

#define REG(a) (*(volatile uint32_t *)(a))
#define RCC_IOPENR  REG(0x40021034u)
#define RCC_APBENR2 REG(0x40021040u)
#define GPIOA_MODER REG(0x50000000u)
#define GPIOA_AFRH  REG(0x50000024u)
#define GPIOB_MODER REG(0x50000400u)
#define GPIOB_OTYPER REG(0x50000404u)
#define GPIOB_ODR   REG(0x50000414u)
#define GPIOB_BSRR  REG(0x50000418u)
#define EXTI_RTSR1  REG(0x40021800u)
#define EXTI_FTSR1  REG(0x40021804u)
#define EXTI_RPR1   REG(0x4002180Cu)
#define EXTI_FPR1   REG(0x40021810u)
#define EXTI_EXTICR1 REG(0x40021860u)
#define EXTI_EXTICR2 REG(0x40021864u)
#define EXTI_IMR1   REG(0x40021880u)
#define NVIC_ISER   REG(0xE000E100u)
#define USART1_CR1  REG(0x40013800u)
#define USART1_BRR  REG(0x4001380Cu)
#define USART1_ISR  REG(0x4001381Cu)
#define USART1_TDR  REG(0x40013828u)
#define RESULT ((volatile uint32_t *)0x20000100u)

#define ONE_US_LOOPS 3u /* ~16 MHz, a busy loop iteration is about 5 cycles */
/* The browser lab runs the same firmware 20x slower (TIME_SCALE=20, see
 * build.sh) so a person can watch the pulses; every count is unchanged. */
#ifndef TIME_SCALE
#define TIME_SCALE 1u
#endif
static void delay_us(uint32_t us) {
    for (volatile uint32_t i = 0; i < us * ONE_US_LOOPS * TIME_SCALE; ++i) {}
}

static void put(const char *s) {
    while (*s) {
        while (!(USART1_ISR & (1u << 7))) {}
        USART1_TDR = (uint32_t)*s++;
    }
}

/* Counts here are below 100; the M0+ has no divide instruction. */
static void put_num(uint32_t v) {
    uint32_t tens = 0;
    while (v >= 10u) { v -= 10u; ++tens; }
    if (tens) {
        while (!(USART1_ISR & (1u << 7))) {}
        USART1_TDR = '0' + tens;
    }
    while (!(USART1_ISR & (1u << 7))) {}
    USART1_TDR = '0' + v;
}

void reset(void);
void default_handler(void) { for (;;) {} }
void exti0_1_handler(void);
void exti4_15_handler(void);

__attribute__((section(".vectors"), used))
const uintptr_t vectors[16 + 8] = {
    [0] = 0x20009000u,
    [1] = (uintptr_t)reset,
    [2] = (uintptr_t)default_handler,
    [3] = (uintptr_t)default_handler,
    [16 + 5] = (uintptr_t)exti0_1_handler,
    [16 + 7] = (uintptr_t)exti4_15_handler,
};

void exti0_1_handler(void) {
    uint32_t r = EXTI_RPR1, f = EXTI_FPR1;
    if (r & 1u) { EXTI_RPR1 = 1u; RESULT[0] += 1; }
    if (f & 1u) { EXTI_FPR1 = 1u; RESULT[1] += 1; }
}

void exti4_15_handler(void) {
    uint32_t r = EXTI_RPR1, f = EXTI_FPR1;
    if (r & (1u << 4)) { EXTI_RPR1 = 1u << 4; RESULT[2] += 1; }
    if (f & (1u << 4)) { EXTI_FPR1 = 1u << 4; RESULT[3] += 1; }
}

int main(void);
void reset(void) { (void)main(); for (;;) {} }

int main(void) {
    RCC_IOPENR |= 3u;               /* GPIOA, GPIOB */
    RCC_APBENR2 |= (1u << 14);      /* USART1 */
    /* PA9 = USART1_TX (AF1). */
    GPIOA_MODER = (GPIOA_MODER & ~(3u << 18)) | (2u << 18);
    GPIOA_AFRH = (GPIOA_AFRH & ~(0xFu << 4)) | (1u << 4);
    USART1_BRR = 139u;              /* 16 MHz / 115200 */
    USART1_CR1 = (1u << 3) | 1u;    /* TE, UE */

    /* PB1 ready (push-pull, low); PB4 alert (open-drain, released). */
    GPIOB_ODR = 1u << 4;
    GPIOB_OTYPER = 1u << 4;
    GPIOB_MODER = (GPIOB_MODER & ~((3u << 2) | (3u << 8))) | (1u << 2) | (1u << 8);
    /* PB0 irq input: EXTI line 0 on port B, both edges. */
    GPIOB_MODER &= ~3u;
    EXTI_EXTICR1 = 1u;
    EXTI_RTSR1 = 1u;
    EXTI_FTSR1 = 1u;
    EXTI_IMR1 = 1u;
    NVIC_ISER = 1u << 5;
    __asm__ volatile("cpsie i" ::: "memory");

    while (RESULT[0] < 10u || RESULT[1] < 10u) {}

    delay_us(200);                  /* the AVR is polling by then */
    for (int i = 0; i < 7; ++i) {   /* ready pulses */
        GPIOB_BSRR = 1u << 1;
        delay_us(20);
        GPIOB_BSRR = 1u << 17;
        delay_us(20);
    }
    delay_us(100);
    for (int i = 0; i < 5; ++i) {   /* alert low, alone */
        GPIOB_BSRR = 1u << 20;
        delay_us(30);
        GPIOB_BSRR = 1u << 4;
        delay_us(30);
    }

    /* The AVR pulls alert 3 times; count them (the pad is released). */
    delay_us(20);
    EXTI_EXTICR2 = 1u;              /* line 4 on port B */
    EXTI_RTSR1 |= 1u << 4;
    EXTI_FTSR1 |= 1u << 4;
    EXTI_IMR1 |= 1u << 4;
    NVIC_ISER = 1u << 7;
    while (RESULT[3] < 3u || RESULT[2] < 3u) {}

    put("STM irq r=");
    put_num(RESULT[0]);
    put(" f=");
    put_num(RESULT[1]);
    put(" alert r=");
    put_num(RESULT[2]);
    put(" f=");
    put_num(RESULT[3]);
    put("\n");
    RESULT[4] = 1u;
    for (;;) {}
}
