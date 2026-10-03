/* ATmega328P half of the contention demo: PD2 is driven low for good. */
#include <avr/io.h>
int main(void) {
    PORTD &= (uint8_t)~(1u << PD2);
    DDRD |= (1u << PD2);
    for (;;) {}
}
