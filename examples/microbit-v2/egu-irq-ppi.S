/* SPDX-License-Identifier: MIT
 * Guest-owned EGU0 -> PPI channel 0 -> EGU1 task and late-enabled EGU0 IRQ.
 * RAM 0x20000100: PPI event seen, IRQ entries, completion marker,
 * and IRQ entries while the peripheral interrupt is still masked.
 */
.syntax unified
.cpu cortex-m4
.thumb
.section .vectors,"a"
.word 0x20020000
.word reset
.rept 34 /* remaining system exceptions and external IRQs 0..19 */
.word default_handler
.endr
.word egu0_irq
.section .text,"ax"
.thumb_func
.global reset
reset:
    ldr r7, =0x20000100
    movs r0, #0
    str r0, [r7, #0]
    str r0, [r7, #4]
    str r0, [r7, #8]
    ldr r4, =0x40014000 /* EGU0 */
    ldr r5, =0x40015100 /* EGU1 EVENTS_TRIGGERED[0] */
    ldr r6, =0x4001f000 /* PPI */
    ldr r0, =0x40014100 /* EGU0 EVENTS_TRIGGERED[0] */
    str r0, [r6, #0x510] /* CH0 EEP */
    ldr r0, =0x40015000 /* EGU1 TASKS_TRIGGER[0] */
    str r0, [r6, #0x514] /* CH0 TEP */
    movs r0, #1
    str r0, [r6, #0x504] /* CHENSET */
    str r0, [r4]         /* trigger while EGU0 interrupt is disabled */
wait_ppi:
    ldr r1, [r5]
    cmp r1, #0
    beq wait_ppi
    str r1, [r7, #0]
    ldr r1, =0xe000e100 /* NVIC ISER0 */
    ldr r2, =0x00100000 /* IRQ20 */
    str r2, [r1]
    movs r2, #128
masked_wait:
    subs r2, #1
    bne masked_wait
    ldr r1, [r7, #4]
    str r1, [r7, #12]
    str r0, [r4, #0x304] /* INTENSET after event already latched */
wait_irq:
    ldr r1, [r7, #4]
    cmp r1, #0
    beq wait_irq
    movs r2, #128
settle:
    subs r2, #1
    bne settle
    movs r1, #1
    str r1, [r7, #8]
done:
    b done
.thumb_func
egu0_irq:
    push {r0,r1}
    ldr r0, [r7, #4]
    adds r0, #1
    str r0, [r7, #4]
    movs r1, #0
    str r1, [r4, #0x100] /* clear EGU0 event, deassert its interrupt */
    pop {r0,r1}
    bx lr
.thumb_func
default_handler:
    b default_handler
