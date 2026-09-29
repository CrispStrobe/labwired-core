/* A UART relay chain node for the STM32F401 (NUCLEO-F401RE), register-level,
 * interrupt-driven. One source file, built per role (see README.md):
 *
 *   ROLE_SOURCE  every PERIOD_US, SysTick builds a message and queues it on
 *                USART2 (TX to the next node).
 *   ROLE_RELAY   USART1 receives from the previous node (RXNE interrupt into
 *                a ring buffer); the main loop parses messages, increments
 *                the hop count and forwards them on USART2 (TXE interrupt
 *                drains a ring buffer).
 *
 * Message, 5 bytes: A5, seq_lo, seq_hi, hops, seq_lo ^ seq_hi ^ hops.
 * The source sends hops = 0; each relay forwards hops + 1.
 *
 * PA5 (LD2) toggles every time the application handles a message: the source
 * when it queues one, a relay when it accepts one. The network timeline shows
 * those edges as application markers.
 *
 * Both USARTs: 115200 8N1 from an 84 MHz clock (BRR = 45.5625 -> 0x2D9, RM0368
 * §19.3.4 Table 75), so one bit is 729 cycles.
 *
 * Build options:
 *   RX_ISR_EXTRA_CYCLES  busy cycles added to the RX interrupt: an RX path too
 *                        slow for the line, which must show overrun (ORE).
 *   USART_BRR            the BRR value for both USARTs (default 0x2D9).
 *
 * SRAM 0x20000100 (RESULT), all counters since the last reset:
 *   [0] messages accepted          [1] seq of the last one
 *   [2] hops of the last one       [3] checksum failures
 *   [4] ORE seen by the RX ISR     [5] FE seen by the RX ISR
 *   [6] RX ring high-water mark    [7] TX ring high-water mark
 *   [8] messages queued for TX     [9] messages dropped: TX ring full
 *   [10] sequence gaps (a message id jumped past last + 1)
 *   [11] RX ISR entries
 * SRAM 0x200000F0: boot count (not cleared by reset).
 */
#include <stdint.h>

#define REG(a) (*(volatile uint32_t *)(a))
#define RCC_AHB1ENR REG(0x40023830u)
#define RCC_APB1ENR REG(0x40023840u)
#define RCC_APB2ENR REG(0x40023844u)
#define GPIOA_MODER REG(0x40020000u)
#define GPIOA_ODR   REG(0x40020014u)
#define GPIOA_AFRL  REG(0x40020020u)
#define GPIOA_AFRH  REG(0x40020024u)
#define USART1_BASE 0x40011000u
#define USART2_BASE 0x40004400u
#define U_SR(b)  REG((b) + 0x00u)
#define U_DR(b)  REG((b) + 0x04u)
#define U_BRR(b) REG((b) + 0x08u)
#define U_CR1(b) REG((b) + 0x0Cu)
#define U_CR2(b) REG((b) + 0x10u)
#define U_CR3(b) REG((b) + 0x14u)
#define SR_PE   (1u << 0)
#define SR_FE   (1u << 1)
#define SR_ORE  (1u << 3)
#define SR_RXNE (1u << 5)
#define SR_TXE  (1u << 7)
#define CR1_RE     (1u << 2)
#define CR1_TE     (1u << 3)
#define CR1_RXNEIE (1u << 5)
#define CR1_TXEIE  (1u << 7)
#define CR1_UE     (1u << 13)
#define NVIC_ISER1 REG(0xE000E104u)
#define SYST_CSR REG(0xE000E010u)
#define SYST_RVR REG(0xE000E014u)
#define SYST_CVR REG(0xE000E018u)
#define RESULT ((volatile uint32_t *)0x20000100u)
#define BOOTS  (*(volatile uint32_t *)0x200000F0u)

#define CPU_HZ 84000000u
#ifndef PERIOD_US
#define PERIOD_US 1000u
#endif
#ifndef USART_BRR
#define USART_BRR 0x2D9u
#endif
#ifndef RX_ISR_EXTRA_CYCLES
#define RX_ISR_EXTRA_CYCLES 0u
#endif

#define SYNC 0xA5u
#define MSG_LEN 5u
#define RING 64u

struct ring {
    volatile uint8_t buf[RING];
    volatile uint32_t head, tail;
};
static struct ring rx, tx;
#ifdef ROLE_SOURCE
static uint16_t next_seq;
#endif

static inline void irq_off(void) { __asm__ volatile("cpsid i" ::: "memory"); }
static inline void irq_on(void) { __asm__ volatile("cpsie i" ::: "memory"); }
static inline uint32_t used(struct ring *r) { return (r->head - r->tail) & 0xFFFFu; }

static int push(struct ring *r, uint8_t b, uint32_t hwm_slot) {
    if (used(r) >= RING) return 0;
    r->buf[r->head % RING] = b;
    r->head = r->head + 1;
    if (used(r) > RESULT[hwm_slot]) RESULT[hwm_slot] = used(r);
    return 1;
}

static int pop(struct ring *r, uint8_t *b) {
    if (r->head == r->tail) return 0;
    *b = r->buf[r->tail % RING];
    r->tail = r->tail + 1;
    return 1;
}

static void marker(void) { GPIOA_ODR ^= 1u << 5; }

/* Queue one message for USART2 (called with interrupts off). */
static void send(uint16_t seq, uint8_t hops) {
    uint8_t m[MSG_LEN] = {SYNC, (uint8_t)seq, (uint8_t)(seq >> 8), hops, 0};
    m[4] = m[1] ^ m[2] ^ m[3];
    if (RING - used(&tx) < MSG_LEN) {
        RESULT[9] += 1;
        return;
    }
    for (uint32_t i = 0; i < MSG_LEN; ++i) push(&tx, m[i], 7);
    RESULT[8] += 1;
    U_CR1(USART2_BASE) |= CR1_TXEIE;
}

void reset(void);
void default_handler(void) { for (;;) {} }
void systick_handler(void);
void usart1_irq(void);
void usart2_irq(void);

__attribute__((section(".vectors"), used))
const uintptr_t vectors[16 + 39] = {
    [0] = 0x20018000u,
    [1] = (uintptr_t)reset,
    [2] = (uintptr_t)default_handler,
    [3] = (uintptr_t)default_handler,
    [15] = (uintptr_t)systick_handler,
    [16 + 37] = (uintptr_t)usart1_irq,
    [16 + 38] = (uintptr_t)usart2_irq,
};

void systick_handler(void) {
#ifdef ROLE_SOURCE
    send(next_seq, 0);
    next_seq += 1;
    marker();
#endif
}

/* RX from the previous node. Reading SR then DR clears RXNE and ORE/FE/PE. */
void usart1_irq(void) {
    uint32_t sr = U_SR(USART1_BASE);
    RESULT[11] += 1;
    if (sr & (SR_RXNE | SR_ORE)) {
        uint8_t b = (uint8_t)U_DR(USART1_BASE);
        if (sr & SR_ORE) RESULT[4] += 1;
        if (sr & SR_FE) RESULT[5] += 1;
        else push(&rx, b, 6);
    }
#if RX_ISR_EXTRA_CYCLES > 0
    for (volatile uint32_t i = 0; i < RX_ISR_EXTRA_CYCLES / 4u; ++i) {
    }
#endif
}

/* TX to the next node: refill DR while TXE, stop the interrupt when empty. */
void usart2_irq(void) {
    if (U_SR(USART2_BASE) & SR_TXE) {
        uint8_t b;
        if (pop(&tx, &b)) U_DR(USART2_BASE) = b;
        else U_CR1(USART2_BASE) &= ~CR1_TXEIE;
    }
}

static void usart_init(uint32_t base, uint32_t cr1) {
    U_CR1(base) = 0;
    U_BRR(base) = USART_BRR;
    U_CR2(base) = 0;          /* 1 stop bit */
    U_CR3(base) = 0;
    U_CR1(base) = cr1 | CR1_UE; /* 8 data bits, no parity */
}

#ifndef ROLE_SOURCE
/* The relay's parser: collect MSG_LEN bytes from a SYNC, check, forward. */
static uint8_t frame[MSG_LEN];
static uint32_t fill;
static uint32_t have_last;

static void relay_byte(uint8_t b) {
    if (fill == 0 && b != SYNC) return;
    frame[fill++] = b;
    if (fill < MSG_LEN) return;
    fill = 0;
    if ((uint8_t)(frame[1] ^ frame[2] ^ frame[3]) != frame[4]) {
        RESULT[3] += 1;
        return;
    }
    uint16_t seq = (uint16_t)(frame[1] | (frame[2] << 8));
    if (have_last && seq != (uint16_t)(RESULT[1] + 1)) RESULT[10] += 1;
    have_last = 1;
    RESULT[0] += 1;
    RESULT[1] = seq;
    RESULT[2] = frame[3];
    marker();
    irq_off();
    send(seq, (uint8_t)(frame[3] + 1));
    irq_on();
}
#endif

void reset(void) {
    extern uint8_t __bss_start, __bss_end;
    for (uint8_t *p = &__bss_start; p < &__bss_end; ++p) *p = 0;
    for (int i = 0; i < 16; ++i) RESULT[i] = 0;
    BOOTS += 1;

    RCC_AHB1ENR |= 1u;           /* GPIOA */
    RCC_APB2ENR |= 1u << 4;      /* USART1 */
    RCC_APB1ENR |= 1u << 17;     /* USART2 */
    /* PA2/PA3 = USART2 TX/RX (AF7), PA9/PA10 = USART1 TX/RX (AF7), PA5 out. */
    GPIOA_MODER = (GPIOA_MODER & ~((3u << 4) | (3u << 6) | (3u << 10) | (3u << 18) | (3u << 20)))
                | (2u << 4) | (2u << 6) | (1u << 10) | (2u << 18) | (2u << 20);
    GPIOA_AFRL = (GPIOA_AFRL & ~((0xFu << 8) | (0xFu << 12))) | (7u << 8) | (7u << 12);
    GPIOA_AFRH = (GPIOA_AFRH & ~((0xFu << 4) | (0xFu << 8))) | (7u << 4) | (7u << 8);

    usart_init(USART2_BASE, CR1_TE);
#ifdef ROLE_SOURCE
    SYST_RVR = CPU_HZ / 1000000u * PERIOD_US - 1u;
    SYST_CVR = 0;
    SYST_CSR = 7u;               /* core clock, interrupt, enable */
#else
    usart_init(USART1_BASE, CR1_RE | CR1_RXNEIE);
    NVIC_ISER1 = 1u << (37 - 32);
#endif
    NVIC_ISER1 = 1u << (38 - 32);

    for (;;) {
        uint8_t b;
        irq_off();
        int got = pop(&rx, &b);
        if (!got) {
            __asm__ volatile("wfi");
            irq_on();
            continue;
        }
        irq_on();
#ifndef ROLE_SOURCE
        relay_byte(b);
#endif
    }
}
