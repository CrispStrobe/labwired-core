// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.
//
// FB200 / i.MX RT1052 smoke: ungate CCM clocks, print "RT1052 SMOKE OK\n" on
// LPUART5 (the FB200 Bluetooth UART, which fb200.yaml routes to the host
// console) at the reset BAUD (OSR 16, SBR 4: 1.25 Mbaud from the 80 MHz UART
// clock), toggle GPIO4_IO00 (knob LED 1) via DR_TOGGLE.
// Bare-register — no HAL. Soft-float thumbv7em-none-eabi.
// Addresses and fields: tests/fixtures/real_world/mimxrt1052.svd.

#![no_std]
#![no_main]

use core::ptr::{read_volatile, write_volatile};
use cortex_m_rt::entry;
use panic_halt as _;

// CCM @ 0x400FC000 — CCGR3@0x74: CG1 lpuart5 (bits 3:2), CG6 gpio4 (bits 13:12).
const CCM_CCGR3: *mut u32 = 0x400F_C074 as *mut u32;

// LPUART5 @ 0x40194000 — RT layout: STAT@+0x14, CTRL@+0x18,
// DATA@+0x1C (VERID/PARAM/GLOBAL/PINCFG sit in front).
const LPUART5_BASE: u32 = 0x4019_4000;
const LPUART5_STAT: *mut u32 = (LPUART5_BASE + 0x14) as *mut u32;
const LPUART5_CTRL: *mut u32 = (LPUART5_BASE + 0x18) as *mut u32;
const LPUART5_DATA: *mut u32 = (LPUART5_BASE + 0x1C) as *mut u32;

const STAT_TDRE: u32 = 1 << 23;
const CTRL_TE: u32 = 1 << 19;
const CTRL_RE: u32 = 1 << 18;

// GPIO4 @ 0x401C4000 — GDIR@+0x04, DR_TOGGLE@+0x8C.
const GPIO4_GDIR: *mut u32 = 0x401C_4004 as *mut u32;
const GPIO4_DR_TOGGLE: *mut u32 = 0x401C_408C as *mut u32;

const KNOB_LED1: u32 = 1 << 0; // GPIO4_IO00 — FB200 knob LED, active low

#[entry]
fn main() -> ! {
    unsafe {
        let ccgr3 = read_volatile(CCM_CCGR3);
        write_volatile(CCM_CCGR3, ccgr3 | (0b11 << 2) | (0b11 << 12));

        write_volatile(LPUART5_CTRL, CTRL_TE | CTRL_RE);

        let gdir = read_volatile(GPIO4_GDIR);
        write_volatile(GPIO4_GDIR, gdir | KNOB_LED1);

        for b in b"RT1052 SMOKE OK\n" {
            while read_volatile(LPUART5_STAT) & STAT_TDRE == 0 {}
            write_volatile(LPUART5_DATA, *b as u32);
        }

        write_volatile(GPIO4_DR_TOGGLE, KNOB_LED1);
    }
    loop {}
}
