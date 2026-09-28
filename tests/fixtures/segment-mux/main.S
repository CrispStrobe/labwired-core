/* segment-mux-thumbv7m: multiplexes "1.23" on a 3-digit 7-segment display
 * (STM32F103, GPIOA). Fixture for the segment_display primitive
 * (crates/cli/tests/e2e_segment_display.rs, system.yaml in this directory).
 *
 *   PA0..PA7  segments a b c d e f g dp, active high
 *   PA8..PA10 digit selects 0..2 (leftmost first), active high
 *
 * Each digit is on for about 0.25 ms. The digit change writes the NEXT
 * digit's segments while the OLD digit is still selected, for one store:
 * a real ghost, which the persistence threshold must reject.
 * Build: ./build.sh (arm-none-eabi-gcc). */
    .syntax unified
    .cpu cortex-m3
    .thumb

    .section .vectors, "a"
    .word 0x20001000
    .word reset + 1

    .text
    .thumb_func
reset:
    ldr   r0, =0x40021018      /* RCC_APB2ENR */
    ldr   r1, [r0]
    orr   r1, r1, #(1 << 2)    /* IOPAEN */
    str   r1, [r0]
    ldr   r0, =0x40010800      /* GPIOA */
    ldr   r1, =0x33333333      /* CRL: PA0..7 push-pull outputs */
    str   r1, [r0, #0x00]
    ldr   r1, =0x44444333      /* CRH: PA8..10 outputs */
    str   r1, [r0, #0x04]
    adr   r4, patterns         /* segment + select word per digit */
    movs  r5, #0               /* current digit */
scan:
    adds  r6, r5, #1           /* next digit */
    cmp   r6, #3
    it    eq
    moveq r6, #0
    ldr   r2, [r4, r6, lsl #2] /* next word: segments | its select */
    ldr   r3, [r4, r5, lsl #2]
    and   r7, r2, #0xFF        /* next segments ... */
    and   r3, r3, #0x700       /* ... with the old select: the ghost */
    orr   r7, r7, r3
    str   r7, [r0, #0x0C]      /* ODR */
    str   r2, [r0, #0x0C]      /* next digit, its own select */
    mov   r5, r6
    ldr   r1, =6000            /* ~0.25 ms at 72 MHz */
delay:
    subs  r1, r1, #1
    bne   delay
    b     scan

    .align 2
patterns:
    .word 0x100 | 0x86         /* digit 0: '1' + dp */
    .word 0x200 | 0x5B         /* digit 1: '2' */
    .word 0x400 | 0x4F         /* digit 2: '3' */
    .ltorg
