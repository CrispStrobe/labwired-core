/*
 * LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 */
/*
 * NUCLEO-U545RE-Q bare-metal blinky / io-smoke firmware (no HAL, no libc).
 *
 *   LD2  green  PA5   (active high)       BSP LED2
 *   B1   user   PC13  (active high)       BSP BUTTON_USER, internal pull-down
 *   VCP  USART1 PA9 TX / PA10 RX, AF7     BSP COM1
 *
 * Prints "OK\n" on USART1, toggles LD2 forever, reports "B1=<0|1>\n" on every
 * button edge and echoes any received byte. Registers are the RM0456 / ST SVD
 * (STM32U545) addresses; the sim clock is the 4 MHz MSI reset default.
 */
#include <stdint.h>

#define REG(a) (*(volatile uint32_t *)(a))

#define RCC_BASE     0x46020C00u
#define RCC_AHB2ENR1 REG(RCC_BASE + 0x8C)
#define RCC_APB2ENR  REG(RCC_BASE + 0xA4)
#define GPIOA        0x42020000u
#define GPIOC        0x42020800u
#define MODER(p)     REG((p) + 0x00)
#define PUPDR(p)     REG((p) + 0x0C)
#define IDR(p)       REG((p) + 0x10)
#define BSRR(p)      REG((p) + 0x18)
#define AFRH(p)      REG((p) + 0x24)
#define USART1       0x40013800u
#define U_CR1        REG(USART1 + 0x00)
#define U_BRR        REG(USART1 + 0x0C)
#define U_ISR        REG(USART1 + 0x1C)
#define U_RDR        REG(USART1 + 0x24)
#define U_TDR        REG(USART1 + 0x28)

static void putc_(char c)
{
    while (!(U_ISR & (1u << 7))) { /* TXE */ }
    U_TDR = (uint32_t)c;
}

static void puts_(const char *s)
{
    while (*s) putc_(*s++);
}

int main(void)
{
    RCC_AHB2ENR1 |= (1u << 0) | (1u << 2);          /* GPIOAEN, GPIOCEN */
    RCC_APB2ENR |= (1u << 14);                      /* USART1EN */

    MODER(GPIOA) = (MODER(GPIOA) & ~(3u << 10)) | (1u << 10);  /* PA5 output */
    /* PA9/PA10: alternate function 7 (USART1). */
    MODER(GPIOA) = (MODER(GPIOA) & ~((3u << 18) | (3u << 20))) | (2u << 18) | (2u << 20);
    AFRH(GPIOA) = (AFRH(GPIOA) & ~((0xFu << 4) | (0xFu << 8))) | (7u << 4) | (7u << 8);
    MODER(GPIOC) &= ~(3u << 26);                    /* PC13 input */
    PUPDR(GPIOC) = (PUPDR(GPIOC) & ~(3u << 26)) | (2u << 26);  /* pull-down */

    U_BRR = 4000000u / 115200u;                     /* 4 MHz MSI reset clock / 115200 */
    U_CR1 = (1u << 0) | (1u << 2) | (1u << 3);      /* UE | RE | TE */
    puts_("OK\n");

    uint32_t n = 0, last = 0;
    for (;;) {
        if (++n >= 100u) {
            n = 0;
            BSRR(GPIOA) = (IDR(GPIOA) & (1u << 5)) ? (1u << 21) : (1u << 5);
        }
        uint32_t b = (IDR(GPIOC) >> 13) & 1u;
        if (b != last) {
            last = b;
            puts_(b ? "B1=1\n" : "B1=0\n");
        }
        if (U_ISR & (1u << 5)) {                    /* RXNE */
            putc_((char)U_RDR);
        }
    }
}

extern uint32_t _estack;
void Reset_Handler(void) { main(); for (;;) {} }
void Default_Handler(void) { for (;;) {} }

__attribute__((section(".isr_vector"), used))
const void *const vectors[] = {
    &_estack, Reset_Handler,
    Default_Handler, Default_Handler, Default_Handler, Default_Handler,
    Default_Handler, 0, 0, 0, 0, Default_Handler, Default_Handler, 0,
    Default_Handler, Default_Handler,
};
