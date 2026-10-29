/* SPDX-License-Identifier: MIT
 * Source-built electrical-path probe. Samples P0.25 into RAM and counts reads.
 */
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
    movs r3, #0
loop:
    ldr r2, [r0, #0x510]
    lsrs r2, r2, #25
    and r2, r2, #1
    str r2, [r1]
    adds r3, #1
    str r3, [r1, #4]
    b loop
