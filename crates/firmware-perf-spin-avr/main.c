/* LabWired AVR8 throughput fixture: linked for ATmega328P at flash address 0. */
__attribute__((used, naked, noreturn)) void main(void) {
    __asm__ __volatile__(
        "ldi r18, 0\n"
        "1: inc r18\n"
        "rjmp 1b\n");
}
