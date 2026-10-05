// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT
//
//! Arduino Zero (ATSAMD21G18A) demo firmware: the real SAM D21 bring-up
//! (NVMCTRL wait states, OSC8M ready poll, GCLK generator 0 -> SERCOM0_CORE,
//! PM.APBCMASK, PORT.WRCONFIG mux of PA10/PA11 to function C, SERCOM0 USART),
//! then a blink of LED_BUILTIN (D13 = PA17, active high) with a heartbeat line
//! on Serial1 (SERCOM0, D1/D0 = PA10/PA11).
//!
//! The bring-up is the same register traffic as `firmware-samd21-demo`, which
//! is the Nano 33 IoT smoke; this crate is the board demo: it blinks at a
//! visible rate and reports each beat with the LED state READ BACK from
//! PORT.OUT, so the console text reports the pad the firmware actually drove.
//! Every address and field is from `ATSAMD21G18A.svd` (Microchip, Apache-2.0).

#![no_std]
#![no_main]

use core::ptr::{read_volatile, write_volatile};
use cortex_m_rt::entry;
use panic_halt as _;

// ── Peripheral bases ─────────────────────────────────────────────────────────
const PM_BASE: u32 = 0x4000_0400;
const SYSCTRL_BASE: u32 = 0x4000_0800;
const GCLK_BASE: u32 = 0x4000_0C00;
const NVMCTRL_BASE: u32 = 0x4100_4000;
const PORTA_BASE: u32 = 0x4100_4400;
const SERCOM0_BASE: u32 = 0x4200_0800;

// ── NVMCTRL ──────────────────────────────────────────────────────────────────
const NVMCTRL_CTRLB: *mut u32 = (NVMCTRL_BASE + 0x04) as *mut u32;
/// `CTRLB.RWS` is bits [4:1]. One wait state is what the datasheet requires
/// once the core runs above 24 MHz.
const NVMCTRL_CTRLB_RWS_HALF: u32 = 1 << 1;

// ── SYSCTRL ──────────────────────────────────────────────────────────────────
const SYSCTRL_PCLKSR: *const u32 = (SYSCTRL_BASE + 0x0C) as *const u32;
const PCLKSR_OSC8MRDY: u32 = 1 << 3;

// ── GCLK ─────────────────────────────────────────────────────────────────────
const GCLK_STATUS: *const u8 = (GCLK_BASE + 0x01) as *const u8;
const GCLK_CLKCTRL: *mut u16 = (GCLK_BASE + 0x02) as *mut u16;
const GCLK_GENCTRL: *mut u32 = (GCLK_BASE + 0x04) as *mut u32;
const GCLK_GENDIV: *mut u32 = (GCLK_BASE + 0x08) as *mut u32;
const GCLK_STATUS_SYNCBUSY: u8 = 1 << 7;
/// `GENCTRL.SRC` 0x06 = OSC8M (SVD `GCLK_GENCTRL.SRC` enum).
const GCLK_SRC_OSC8M: u32 = 0x06;
const GCLK_GENCTRL_GENEN: u32 = 1 << 16;
/// `CLKCTRL.ID` 0x14 = SERCOM0_CORE (SVD `GCLK_CLKCTRL.ID` enum).
const GCLK_ID_SERCOM0_CORE: u16 = 0x14;
const GCLK_CLKCTRL_CLKEN: u16 = 1 << 14;

// ── PM ───────────────────────────────────────────────────────────────────────
const PM_APBCMASK: *mut u32 = (PM_BASE + 0x20) as *mut u32;
/// `APBCMASK.SERCOM0_` is bit 2. APBCMASK resets to 0x0001_0000 (ADC only), so
/// this bit is genuinely off until firmware sets it.
const PM_APBCMASK_SERCOM0: u32 = 1 << 2;

// ── PORT (GROUP 0 = PA) ──────────────────────────────────────────────────────
const PORTA_DIRSET: *mut u32 = (PORTA_BASE + 0x08) as *mut u32;
const PORTA_OUT: *const u32 = (PORTA_BASE + 0x10) as *const u32;
const PORTA_OUTCLR: *mut u32 = (PORTA_BASE + 0x14) as *mut u32;
const PORTA_OUTSET: *mut u32 = (PORTA_BASE + 0x18) as *mut u32;
const PORTA_WRCONFIG: *mut u32 = (PORTA_BASE + 0x28) as *mut u32;
const WRCONFIG_PMUXEN: u32 = 1 << 16;
const WRCONFIG_INEN: u32 = 1 << 17;
const WRCONFIG_WRPMUX: u32 = 1 << 28;
const WRCONFIG_WRPINCFG: u32 = 1 << 30;
/// `WRCONFIG.PMUX` 0x2 = peripheral function **C**, which is where SERCOM0's
/// pads live on PA10/PA11 (SAM D21 datasheet, "I/O Multiplexing and
/// Considerations").
const PMUX_FUNCTION_C: u32 = 0x2;
/// The Arduino Zero / Feather M0 user LED.
const LED_PIN: u32 = 17;

// ── SERCOM0 (USART mode) ─────────────────────────────────────────────────────
const SERCOM0_CTRLA: *mut u32 = SERCOM0_BASE as *mut u32;
const SERCOM0_CTRLB: *mut u32 = (SERCOM0_BASE + 0x04) as *mut u32;
const SERCOM0_BAUD: *mut u16 = (SERCOM0_BASE + 0x0C) as *mut u16;
const SERCOM0_INTFLAG: *const u8 = (SERCOM0_BASE + 0x18) as *const u8;
const SERCOM0_SYNCBUSY: *const u32 = (SERCOM0_BASE + 0x1C) as *const u32;
const SERCOM0_DATA: *mut u16 = (SERCOM0_BASE + 0x28) as *mut u16;

/// `CTRLA.MODE` 0x1 = USART with the internal clock.
const CTRLA_MODE_USART_INT_CLK: u32 = 0x1 << 2;
/// `CTRLA.RXPO` [21:20] = 3 → RX on PAD[3] (PA11).
const CTRLA_RXPO_PAD3: u32 = 0x3 << 20;
/// `CTRLA.TXPO` [17:16] = 1 → TX on PAD[2] (PA10).
const CTRLA_TXPO_PAD2: u32 = 0x1 << 16;
/// `CTRLA.DORD` bit 30 = 1 → LSB first, which is what a UART frame is.
const CTRLA_DORD_LSB: u32 = 1 << 30;
const CTRLA_ENABLE: u32 = 1 << 1;
const CTRLB_TXEN: u32 = 1 << 16;
const CTRLB_RXEN: u32 = 1 << 17;
const INTFLAG_DRE: u8 = 1 << 0;
/// 115200 baud from an 8 MHz core clock, 16× oversampling:
/// `65536 * (1 - 16 * 115200 / 8_000_000)` = 50438.
const BAUD_115200_AT_8MHZ: u16 = 50438;

/// Spin until GCLK has finished synchronising. Bounded, so a twin that never
/// clears SYNCBUSY fails the smoke run instead of hanging it.
fn gclk_sync() {
    for _ in 0..10_000 {
        if unsafe { read_volatile(GCLK_STATUS) } & GCLK_STATUS_SYNCBUSY == 0 {
            return;
        }
    }
}

fn sercom_sync() {
    for _ in 0..10_000 {
        if unsafe { read_volatile(SERCOM0_SYNCBUSY) } == 0 {
            return;
        }
    }
}

fn putc(byte: u8) {
    // The DRE spin every SAM D21 serial driver performs.
    for _ in 0..10_000 {
        if unsafe { read_volatile(SERCOM0_INTFLAG) } & INTFLAG_DRE != 0 {
            break;
        }
    }
    unsafe { write_volatile(SERCOM0_DATA, u16::from(byte)) };
}

fn puts(s: &str) {
    for byte in s.as_bytes() {
        putc(*byte);
    }
}

#[entry]
fn main() -> ! {
    unsafe {
        // 1. Flash wait states before the clock rises.
        write_volatile(NVMCTRL_CTRLB, NVMCTRL_CTRLB_RWS_HALF);

        // 2. Wait for the 8 MHz RC oscillator. On silicon this is a handful of
        //    cycles; on a twin whose PCLKSR reads 0 it never returns.
        for _ in 0..10_000 {
            if read_volatile(SYSCTRL_PCLKSR) & PCLKSR_OSC8MRDY != 0 {
                break;
            }
        }

        // 3. GCLK generator 0 = OSC8M, undivided; then SERCOM0_CORE on gen 0.
        write_volatile(GCLK_GENDIV, 0); // ID 0, DIV 0
        gclk_sync();
        write_volatile(GCLK_GENCTRL, (GCLK_SRC_OSC8M << 8) | GCLK_GENCTRL_GENEN);
        gclk_sync();
        write_volatile(GCLK_CLKCTRL, GCLK_ID_SERCOM0_CORE | GCLK_CLKCTRL_CLKEN);
        gclk_sync();

        // 4. SERCOM0's APB clock.
        let mask = read_volatile(PM_APBCMASK) | PM_APBCMASK_SERCOM0;
        write_volatile(PM_APBCMASK, mask);

        // 5. PA10 (TX) and PA11 (RX) to peripheral function C, in one store —
        //    PINMASK bits 10 and 11, low half of the port.
        write_volatile(
            PORTA_WRCONFIG,
            (1 << 10)
                | (1 << 11)
                | WRCONFIG_PMUXEN
                | WRCONFIG_INEN
                | (PMUX_FUNCTION_C << 24)
                | WRCONFIG_WRPMUX
                | WRCONFIG_WRPINCFG,
        );
        //    PA17: plain output, LED off.
        write_volatile(PORTA_DIRSET, 1 << LED_PIN);

        // 6. SERCOM0 as a USART. CTRLB before CTRLA.ENABLE, as the datasheet
        //    requires — CTRLB is enable-protected.
        write_volatile(
            SERCOM0_CTRLA,
            CTRLA_MODE_USART_INT_CLK | CTRLA_RXPO_PAD3 | CTRLA_TXPO_PAD2 | CTRLA_DORD_LSB,
        );
        write_volatile(SERCOM0_CTRLB, CTRLB_TXEN | CTRLB_RXEN);
        sercom_sync();
        write_volatile(SERCOM0_BAUD, BAUD_115200_AT_8MHZ);
        let ctrla = read_volatile(SERCOM0_CTRLA) | CTRLA_ENABLE;
        write_volatile(SERCOM0_CTRLA, ctrla);
        sercom_sync();
    }

    puts("Arduino Zero ready\r\n");

    let mut beat: u32 = 0;
    loop {
        unsafe {
            if beat & 1 == 0 {
                write_volatile(PORTA_OUTSET, 1 << LED_PIN);
            } else {
                write_volatile(PORTA_OUTCLR, 1 << LED_PIN);
            }
        }
        // PORT.OUT read back: the LED is lit while PA17 reads high.
        let lit = unsafe { read_volatile(PORTA_OUT) } & (1 << LED_PIN) != 0;
        puts("hb=");
        put_dec(beat);
        puts(if lit { " led=on\r\n" } else { " led=off\r\n" });
        beat = beat.wrapping_add(1);
        for _ in 0..200_000u32 {
            unsafe {
                let _ = read_volatile(PORTA_OUT);
            }
        }
    }
}

fn put_dec(mut v: u32) {
    let mut digits = [0u8; 10];
    let mut n = 0;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        putc(digits[n]);
    }
}
