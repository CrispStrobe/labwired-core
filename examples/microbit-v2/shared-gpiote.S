/* SPDX-License-Identifier: MIT
 * Sample P0.25 and count falling-edge GPIOTE channel events in RAM.
 */
/* Defaults preserve the original micro:bit observer. A separate nRF52840
 * regression opts into a free fixture pad and guest-owned TWIM writes. */
#ifndef GPIOTE_TEST_PIN
#define GPIOTE_TEST_PIN 25
#endif
#ifndef GPIOTE_TEST_POLARITY
#define GPIOTE_TEST_POLARITY 2
#endif
.syntax unified
.cpu cortex-m4
.thumb
.section .vectors,"a"
.word 0x20020000
.word reset
.section .text,"ax"
.thumb_func
.global reset
reset:
    ldr r0, =0x50000000
    ldr r1, =0x20000100
    ldr r4, =0x40006100    /* EVENTS_IN[0] */
    ldr r6, =0x40006510    /* CONFIG[0] */
    movs r2, #0
    str r2, [r4]
    ldr r2, =((GPIOTE_TEST_POLARITY << 16) | (GPIOTE_TEST_PIN << 8) | 1)
    str r2, [r6]
    movs r3, #0
    movs r5, #0
    str r5, [r1, #8]
#ifdef GPIOTE_DRIVE_PCF
    bl fixture_init
#endif
loop:
    ldr r2, [r0, #0x510]
    lsrs r2, r2, #GPIOTE_TEST_PIN
    and r2, r2, #1
    str r2, [r1]
    adds r3, #1
    str r3, [r1, #4]
    ldr r2, [r4]
    cbz r2, no_event
    adds r5, #1
    str r5, [r1, #8]
    movs r2, #0
    str r2, [r4]
no_event:
#ifdef GPIOTE_DRIVE_PCF
    bl fixture_command
#endif
    b loop

#ifdef GPIOTE_DRIVE_PCF
/* Explicit synthetic PCF8574 at 0x20 on logical TWIM0, not onboard hardware.
 * RAM +0c errors, +10 intent(1=low,2=release), +14 completed, +1c TX amount.
 * The guest owns every TWIM register access; host changes RAM intent only.
 */
.thumb_func
fixture_init:
    push {r0-r2,lr}
    ldr r8, =0x40003000
    ldr r9, =0x20000200
    ldr r10, =0x20000100
    movs r2, #8
    str r2, [r8, #0x508]
    movs r2, #16
    str r2, [r8, #0x50c]
    ldr r2, =0x06400000
    str r2, [r8, #0x524]
    movs r2, #6
    str r2, [r8, #0x500]
    pop {r0-r2,pc}

.thumb_func
fixture_command:
    push {r0-r7,lr}
    sub sp, #4
    ldr r7, [r10, #0x10]
    cmp r7, #0
    beq fixture_done
    cmp r7, #1
    beq fixture_low
    cmp r7, #2
    beq fixture_release
    ldr r6, [r10, #0x0c]
    orr r6, r6, #0x40000000
    str r6, [r10, #0x0c]
    b fixture_complete
fixture_low:
    movs r2, #0xfe
    b fixture_write
fixture_release:
    movs r2, #0xff
fixture_write:
    strb r2, [r9]
    movs r2, #0x20
    str r2, [r8, #0x588]
    str r9, [r8, #0x544]
    movs r2, #1
    str r2, [r8, #0x548]
    movw r2, #0x200 /* LASTTX -> STOP */
    str r2, [r8, #0x200]
    movs r2, #0
    str r2, [r8, #0x104]
    str r2, [r8, #0x124]
    movs r2, #7
    str r2, [r8, #0x4c4]
    movs r2, #1
    str r2, [r8, #8]
    movw r4, #60000
fixture_wait:
    ldr r5, [r8, #0x124]
    cbnz r5, fixture_error
    ldr r5, [r8, #0x104]
    cbnz r5, fixture_amount
    subs r4, #1
    bne fixture_wait
    ldr r6, [r10, #0x0c]
    orr r6, r6, #0x80000000
    str r6, [r10, #0x0c]
    b fixture_complete
fixture_error:
    ldr r5, [r8, #0x4c4]
    ldr r6, [r10, #0x0c]
    orrs r6, r5
    orr r6, r6, #0x10000
    str r6, [r10, #0x0c]
fixture_amount:
    ldr r2, [r8, #0x54c]
    str r2, [r10, #0x1c]
fixture_complete:
    str r7, [r10, #0x14]
    movs r2, #0
    str r2, [r10, #0x10]
fixture_done:
    add sp, #4
    pop {r0-r7,pc}
.ltorg
#endif
