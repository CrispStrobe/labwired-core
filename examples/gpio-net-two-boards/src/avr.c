/* ATmega328P side of the gpio-net-two-boards example (avr-libc, no Arduino
 * core). Wires to the STM32 board:
 *
 *   irq    PD2  output, push-pull        -> STM32 PB0
 *   ready  PD3  input                    <- STM32 PB1
 *   alert  PD4  open-drain by DDR, shared, 10k pull-up  <-> STM32 PB4
 *
 * The model has no external-interrupt or pin-change interrupt for the
 * ATmega328P yet, so this side counts edges by polling PIND in a tight loop
 * (about 5 cycles per pass, far under the 20 us pulse width).
 *
 * Sequence:
 *   1. put 10 pulses on irq;
 *   2. count 7 rising edges on ready;
 *   3. count 5 falling and 5 rising edges on alert (the STM32 pulls it);
 *   4. pull alert low 3 times (DDR high = drive the PORT bit, 0; DDR low =
 *      release), the STM32 counts them;
 *   5. report over USART0.
 */
#define F_CPU 16000000UL
#include <avr/io.h>
#include <util/delay.h>
#include <stdint.h>

#define IRQ   (1u << PD2)
#define READY (1u << PD3)
#define ALERT (1u << PD4)

static void put(const char *s) {
    while (*s) {
        while (!(UCSR0A & (1u << UDRE0))) {}
        UDR0 = (uint8_t)*s++;
    }
}

static void put_num(uint8_t v) {
    char b[4];
    uint8_t n = 0;
    do { b[n++] = (char)('0' + v % 10u); v /= 10u; } while (v);
    while (n) {
        while (!(UCSR0A & (1u << UDRE0))) {}
        UDR0 = (uint8_t)b[--n];
    }
}

int main(void) {
    UBRR0 = 103;                         /* 16 MHz / 9600 */
    UCSR0B = (1u << TXEN0);
    PORTD &= (uint8_t)~(IRQ | ALERT);    /* irq low, alert PORT bit 0 */
    DDRD |= IRQ;                         /* alert (DDR bit 0) is released */
    _delay_ms(1);                        /* let the STM32 arm EXTI */

    for (uint8_t i = 0; i < 10; ++i) {
        PORTD |= IRQ;
        _delay_us(20);
        PORTD &= (uint8_t)~IRQ;
        _delay_us(20);
    }

    uint8_t ready = 0, prev = PIND & READY;
    while (ready < 7) {
        uint8_t cur = PIND & READY;
        if (cur && !prev) ++ready;
        prev = cur;
    }

    uint8_t a_fall = 0, a_rise = 0;
    prev = PIND & ALERT;
    while (a_fall < 5 || a_rise < 5) {
        uint8_t cur = PIND & ALERT;
        if (prev && !cur) ++a_fall;
        if (!prev && cur) ++a_rise;
        prev = cur;
    }

    _delay_us(100);
    for (uint8_t i = 0; i < 3; ++i) {
        DDRD |= ALERT;                   /* drive low */
        _delay_us(30);
        DDRD &= (uint8_t)~ALERT;         /* release */
        _delay_us(30);
    }

    put("AVR ready=");
    put_num(ready);
    put(" alert f=");
    put_num(a_fall);
    put(" r=");
    put_num(a_rise);
    put("\n");
    for (;;) {}
}
