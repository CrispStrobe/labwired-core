// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.
//
// Adafruit PyBadge (SAMD51J19A) io-smoke firmware, linked at 0x4000 behind
// the resident UF2 bootloader. Points VTOR at its own vector table, brings up
// the Feather UART (SERCOM1), prints "OK\n", then blinks the red D13 LED.
// Bare-register driver, no HAL.
//
// Board facts:
// * LED_BUILTIN / D13 = PA23 -- Adafruit ArduinoCore-samd
//   variants/pybadge_m4/variant.cpp ("13 (LED)": PORTA, 23) and the
//   uf2-samdx1 arcade_pybadge board_config.h CF2 table.
// * Application start 0x4000 -- uf2-samdx1 arcade_pybadge (16 KiB bootloader).
// * Console: SERCOM1 PAD0/PAD1 on PA16/PA17, the Feather UART as wired in
//   configs/systems/pybadge.yaml. (Adafruit's Arduino variant instead puts
//   Serial1 on SERCOM5 PB16/PB17; the twin descriptor does not model that.)

#![no_std]
#![no_main]

use core::ptr::{read_volatile, write_volatile};
use cortex_m_rt::entry;
use panic_halt as _;

// SCB.VTOR
const SCB_VTOR: *mut u32 = 0xE000_ED08 as *mut u32;
const APP_BASE: u32 = 0x0000_4000;

// MCLK APBAMASK @ 0x40000800 + 0x14; SERCOM1 is bit 13 (DS60001507).
const MCLK_APBAMASK: *mut u32 = 0x4000_0814 as *mut u32;
// GCLK PCHCTRL[8] @ 0x40001C00 + 0x80 + 4*8 (SERCOM1_CORE); CHEN = bit 6.
const GCLK_PCHCTRL8: *mut u32 = 0x4000_1CA0 as *mut u32;
const GCLK_PCHCTRL_CHEN: u32 = 1 << 6;
// SERCOM1 DATA @ 0x40003400 + 0x28.
const SERCOM1_DATA: *mut u32 = 0x4000_3428 as *mut u32;
// PORTA @ 0x41008000: DIRSET +0x08, OUTTGL +0x1C.
const PORTA_DIRSET: *mut u32 = 0x4100_8008 as *mut u32;
const PORTA_OUTTGL: *mut u32 = 0x4100_801C as *mut u32;
const LED_D13: u32 = 1 << 23; // PA23

#[entry]
fn main() -> ! {
    unsafe {
        write_volatile(SCB_VTOR, APP_BASE);
        write_volatile(MCLK_APBAMASK, 1 << 13);
        write_volatile(GCLK_PCHCTRL8, GCLK_PCHCTRL_CHEN);
        write_volatile(PORTA_DIRSET, LED_D13);
        for b in b"OK\n" {
            write_volatile(SERCOM1_DATA, *b as u32);
        }
        loop {
            write_volatile(PORTA_OUTTGL, LED_D13);
            for _ in 0..200 {
                // Keep the delay loop from being optimised away.
                let _ = read_volatile(PORTA_DIRSET);
            }
        }
    }
}
