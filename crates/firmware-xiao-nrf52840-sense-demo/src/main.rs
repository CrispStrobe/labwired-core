#![no_std]
#![no_main]

//! Seeed XIAO nRF52840 Sense demo firmware: a running light over the board's user LEDs plus a
//! heartbeat line on the console UARTE, in bare registers (no HAL).
//!
//! The RGB LED is red P0.26, green P0.30, blue P0.06, all active low. The console is UARTE0 on D6 = P1.11 (the XIAO Serial1 TX).
//!
//! Each beat lights ONE LED, prints `hb=<n> <led>=on`, then switches it off and
//! prints `... =off`. The state in the printed line is READ BACK from the GPIO
//! OUT register after the write, so the text reports the pad the firmware
//! actually drove and not what it intended to drive.

use core::ptr::{read_volatile, write_volatile};
use cortex_m_rt::entry;
use panic_halt as _;

// UARTE0 — EasyDMA console. DMA reads RAM only, so every line is built on the
// stack. There is no byte-at-a-time TXD in this personality (ENABLE = 8).
const UARTE: u32 = 0x40002000;
const UARTE_TASKS_STARTTX: *mut u32 = (UARTE + 0x008) as *mut u32;
const UARTE_EVENTS_ENDTX: *mut u32 = (UARTE + 0x120) as *mut u32;
const UARTE_ENABLE: *mut u32 = (UARTE + 0x500) as *mut u32;
const UARTE_PSEL_TXD: *mut u32 = (UARTE + 0x50C) as *mut u32;
const UARTE_BAUDRATE: *mut u32 = (UARTE + 0x524) as *mut u32;
const UARTE_TXD_PTR: *mut u32 = (UARTE + 0x544) as *mut u32;
const UARTE_TXD_MAXCNT: *mut u32 = (UARTE + 0x548) as *mut u32;
/// PSEL.TXD: P1.11 (port 1 => +32), the XIAO D6 / Serial1 TX.
const TXD_PIN: u32 = 43;
const BAUD_115200: u32 = 0x01D6_0000;

// GPIO port 0 (nRF GPIO block; register offsets are relative to 0x50000000).
const GPIO: u32 = 0x50000000;
const GPIO_OUT: *mut u32 = (GPIO + 0x504) as *mut u32;
const GPIO_OUTSET: *mut u32 = (GPIO + 0x508) as *mut u32;
const GPIO_OUTCLR: *mut u32 = (GPIO + 0x50c) as *mut u32;
const GPIO_DIRSET: *mut u32 = (GPIO + 0x518) as *mut u32;

/// User LEDs, in running-light order: (console name, P0 bit). XIAO nRF52840 Sense RGB: red P0.26, green P0.30, blue P0.06.
const LEDS: [(&[u8], u32); 3] = [(b"red", 1 << 26), (b"green", 1 << 30), (b"blue", 1 << 6)];

fn all_leds() -> u32 {
    let mut m = 0;
    let mut i = 0;
    while i < LEDS.len() {
        m |= LEDS[i].1;
        i += 1;
    }
    m
}

fn send(buf: &[u8]) {
    unsafe {
        write_volatile(UARTE_EVENTS_ENDTX, 0);
        write_volatile(UARTE_TXD_PTR, buf.as_ptr() as u32);
        write_volatile(UARTE_TXD_MAXCNT, buf.len() as u32);
        write_volatile(UARTE_TASKS_STARTTX, 1);
        while read_volatile(UARTE_EVENTS_ENDTX) == 0 {}
    }
}

/// Append `s` to `buf` at `*at`.
fn put(buf: &mut [u8; 48], at: &mut usize, s: &[u8]) {
    for b in s {
        buf[*at] = *b;
        *at += 1;
    }
}

fn put_dec(buf: &mut [u8; 48], at: &mut usize, mut v: u32) {
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
        buf[*at] = digits[n];
        *at += 1;
    }
}

fn report(beat: u32, name: &[u8], mask: u32) {
    // Active low: the LED is lit while its OUT bit reads 0.
    let lit = unsafe { read_volatile(GPIO_OUT) } & mask == 0;
    let mut line = [0u8; 48];
    let mut at = 0;
    put(&mut line, &mut at, b"hb=");
    put_dec(&mut line, &mut at, beat);
    put(&mut line, &mut at, b" ");
    put(&mut line, &mut at, name);
    put(
        &mut line,
        &mut at,
        if lit { b"=on\r\n" } else { b"=off\r\n" },
    );
    send(&line[..at]);
}

fn delay() {
    for _ in 0..200_000u32 {
        unsafe {
            let _ = read_volatile(GPIO_OUT);
        }
    }
}

#[entry]
fn main() -> ! {
    unsafe {
        write_volatile(UARTE_PSEL_TXD, TXD_PIN);
        write_volatile(UARTE_BAUDRATE, BAUD_115200);
        write_volatile(UARTE_ENABLE, 8);
        // LEDs are active low: drive them high (off) before making them outputs.
        write_volatile(GPIO_OUTSET, all_leds());
        write_volatile(GPIO_DIRSET, all_leds());
    }
    send(b"XIAO nRF52840 Sense ready\r\n");

    let mut beat: u32 = 0;
    loop {
        let (name, mask) = LEDS[((beat / 2) as usize) % LEDS.len()];
        unsafe {
            if beat & 1 == 0 {
                write_volatile(GPIO_OUTCLR, mask);
            } else {
                write_volatile(GPIO_OUTSET, mask);
            }
        }
        report(beat, name, mask);
        beat = beat.wrapping_add(1);
        delay();
    }
}
