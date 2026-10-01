#![no_std]
#![no_main]
// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.
//
// STM32G0B1RE slice smoke (Cortex-M0+, thumbv6m-none-eabi).
//
// Bare registers from RM0444. The same image is what
// examples/stm32g0b1re/io-smoke.yaml runs. It is not the customer
// IO-Link ELF; that build stays in the iolinki reference workflow.

use core::ptr::{read_volatile, write_volatile};
use cortex_m_rt::entry;
use panic_halt as _;

// RCC @ 0x40021000. G0 offsets, not the L0 map.
const RCC_CR: *mut u32 = 0x4002_1000 as *mut u32;
const RCC_IOPENR: *mut u32 = 0x4002_1034 as *mut u32;
const RCC_APBENR1: *mut u32 = 0x4002_103C as *mut u32;
const RCC_APBENR2: *mut u32 = 0x4002_1040 as *mut u32;
const RCC_CR_HSION: u32 = 1 << 8;
const RCC_CR_HSIRDY: u32 = 1 << 10;
const IOPENR_GPIOAEN: u32 = 1 << 0;
const APBENR1_TIM2EN: u32 = 1 << 0;
const APBENR2_USART1EN: u32 = 1 << 14;

// GPIOA @ 0x50000000. PA9 is the USART1_TX pad (AF1).
const GPIOA_MODER: *mut u32 = 0x5000_0000 as *mut u32;
const GPIOA_AFRH: *mut u32 = 0x5000_0024 as *mut u32;
const PA9: u32 = 9;

// USART1 @ 0x40013800, stm32v2 register map.
const USART1_CR1: *mut u32 = 0x4001_3800 as *mut u32;
const USART1_BRR: *mut u32 = 0x4001_380C as *mut u32;
const USART1_ISR: *const u32 = 0x4001_381C as *const u32;
const USART1_TDR: *mut u32 = 0x4001_3828 as *mut u32;
const USART_ISR_TXE: u32 = 1 << 7;
const USART_CR1_UE: u32 = 1 << 0;
const USART_CR1_TE: u32 = 1 << 3;

// TIM2 @ 0x40000000. PSC @ 0x28. 16 MHz / (15 + 1) = 1 MHz.
const TIM2_PSC: *mut u32 = 0x4000_0028 as *mut u32;
const TIM2_PSC_1MHZ: u32 = 15;

// EXTI @ 0x40021800, G0 GPIO bank (RM0444). Not the F1/L4 map.
const EXTI_FTSR1: *mut u32 = 0x4002_1804 as *mut u32;
const EXTI_EXTICR1: *mut u32 = 0x4002_1860 as *mut u32;
const EXTI_IMR1: *mut u32 = 0x4002_1880 as *mut u32;
const EXTI_PORT_B: u32 = 1;

const PCLK_HZ: u32 = 16_000_000;
const BAUD: u32 = 115_200;
const SPIN_LIMIT: u32 = 20_000;

#[entry]
fn main() -> ! {
    unsafe {
        write_volatile(RCC_CR, read_volatile(RCC_CR) | RCC_CR_HSION);
        spin_until(|| read_volatile(RCC_CR) & RCC_CR_HSIRDY != 0);

        write_volatile(RCC_IOPENR, read_volatile(RCC_IOPENR) | IOPENR_GPIOAEN);
        write_volatile(RCC_APBENR1, read_volatile(RCC_APBENR1) | APBENR1_TIM2EN);
        write_volatile(RCC_APBENR2, read_volatile(RCC_APBENR2) | APBENR2_USART1EN);

        // Example 1 — USART1 TX on PA9, alternate function 1.
        let mut moder = read_volatile(GPIOA_MODER);
        moder &= !(0b11 << (PA9 * 2));
        moder |= 0b10 << (PA9 * 2);
        write_volatile(GPIOA_MODER, moder);
        let mut afrh = read_volatile(GPIOA_AFRH);
        afrh &= !(0xF << 4);
        afrh |= 1 << 4;
        write_volatile(GPIOA_AFRH, afrh);
        write_volatile(USART1_BRR, PCLK_HZ / BAUD);
        write_volatile(USART1_CR1, USART_CR1_UE | USART_CR1_TE);

        // Example 2 — TIM2 prescaler for a 1 MHz tick from 16 MHz HSI.
        write_volatile(TIM2_PSC, TIM2_PSC_1MHZ);

        // Example 3 — EXTI line 0 on PB0, falling edge, interrupt unmasked.
        // Port field is the low 3 bits of EXTICR1: 0 = PA, 1 = PB.
        write_volatile(EXTI_EXTICR1, EXTI_PORT_B);
        write_volatile(EXTI_FTSR1, 1);
        write_volatile(EXTI_IMR1, 1);
    }

    print_str("OK\n");
    print_str("usart1 pa9 af1\n");
    if unsafe { read_volatile(TIM2_PSC) } == TIM2_PSC_1MHZ {
        print_str("tim2 psc=15\n");
    } else {
        print_str("tim2 psc FAIL\n");
    }
    let exti_ok = unsafe {
        read_volatile(EXTI_EXTICR1) & 7 == EXTI_PORT_B
            && read_volatile(EXTI_FTSR1) & 1 == 1
            && read_volatile(EXTI_IMR1) & 1 == 1
    };
    if exti_ok {
        print_str("exti0 port=B falling imr=1\n");
    } else {
        print_str("exti0 FAIL\n");
    }

    loop {
        cortex_m::asm::nop();
    }
}

fn spin_until(mut cond: impl FnMut() -> bool) {
    let mut guard = 0u32;
    while !cond() && guard < SPIN_LIMIT {
        guard += 1;
    }
}

fn print_str(s: &str) {
    for b in s.bytes() {
        putc(b);
    }
}

fn putc(b: u8) {
    unsafe {
        let mut guard = 0u32;
        while (read_volatile(USART1_ISR) & USART_ISR_TXE) == 0 && guard < SPIN_LIMIT {
            guard += 1;
        }
        write_volatile(USART1_TDR, b as u32);
    }
}
