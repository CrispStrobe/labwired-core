/* SPDX-License-Identifier: MIT; SAADC END, late-enabled IRQ7. */
.syntax unified
.cpu cortex-m4
.thumb
.section .vectors,"a"
.word 0x20020000
.word reset
.rept 21
.word default_handler
.endr
.word saadc_irq
.section .text,"ax"
.thumb_func
.global reset
reset:
 ldr r7, =0x20000100
 movs r0, #0
 str r0, [r7, #0]
 str r0, [r7, #4]
 str r0, [r7, #8]
 ldr r4, =0x40007000
 movs r1, #1
 str r1, [r4, #0x500]
 movs r2, #4
 str r2, [r4, #0x510]
 ldr r2, =0x20000200
 str r2, [r4, #0x62c]
 str r1, [r4, #0x630]
 str r1, [r4, #0]
 str r1, [r4, #4]
wait_end:
 ldr r2, [r4, #0x104]
 cmp r2, #0
 beq wait_end
 ldr r2, [r4, #0x634]
 str r2, [r7, #12]
 ldr r2, =0xe000e100
 movs r3, #128
 str r3, [r2]
 movs r2, #128
masked_wait:
 subs r2, #1
 bne masked_wait
 ldr r2, [r7, #4]
 str r2, [r7, #0]
 movs r2, #2
 str r2, [r4, #0x304]
wait_irq:
 ldr r2, [r7, #4]
 cmp r2, #0
 beq wait_irq
 movs r2, #128
settle:
 subs r2, #1
 bne settle
 str r1, [r7, #8]
done:
 b done
.thumb_func
saadc_irq:
 push {r0,r1}
 ldr r0, [r7, #4]
 adds r0, #1
 str r0, [r7, #4]
 movs r1, #0
 str r1, [r4, #0x104]
 pop {r0,r1}
 bx lr
.thumb_func
default_handler:
 b default_handler
