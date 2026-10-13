#![no_std]
#![no_main]

//! Master side of a toy binary protocol on USART1 at 19200 baud.
//!
//! Request:  `addr func reg_hi reg_lo count_hi count_lo crc_lo crc_hi`
//! Response: `addr func byte_count data... crc_lo crc_hi`
//!
//! The peer is a `uart_device` part declared in `system.yaml`. This is not
//! Modbus and the peer is not a Modbus slave; it only shares the frame shape
//! and the CRC-16/MODBUS checksum.

use cortex_m_rt::entry;
use panic_halt as _;

const RCC_BASE: u32 = 0x4002_1000;
const RCC_APB2ENR: *mut u32 = (RCC_BASE + 0x18) as *mut u32;
const RCC_APB1ENR: *mut u32 = (RCC_BASE + 0x1C) as *mut u32;
const GPIOA_CRL: *mut u32 = 0x4001_0800 as *mut u32;
const GPIOA_CRH: *mut u32 = 0x4001_0804 as *mut u32;

// USART1: the link to the peer. USART2: console.
const UART1_BASE: u32 = 0x4001_3800;
const UART1_SR: *const u32 = UART1_BASE as *const u32;
const UART1_DR: *mut u32 = (UART1_BASE + 0x04) as *mut u32;
const UART1_BRR: *mut u32 = (UART1_BASE + 0x08) as *mut u32;
const UART1_CR1: *mut u32 = (UART1_BASE + 0x0C) as *mut u32;
const UART2_BASE: u32 = 0x4000_4400;
const UART2_SR: *const u32 = UART2_BASE as *const u32;
const UART2_DR: *mut u32 = (UART2_BASE + 0x04) as *mut u32;
const UART2_BRR: *mut u32 = (UART2_BASE + 0x08) as *mut u32;
const UART2_CR1: *mut u32 = (UART2_BASE + 0x0C) as *mut u32;

const SR_RXNE: u32 = 1 << 5;
const SR_TXE: u32 = 1 << 7;

fn init() {
    unsafe {
        let apb2 = core::ptr::read_volatile(RCC_APB2ENR);
        core::ptr::write_volatile(RCC_APB2ENR, apb2 | (1 << 0) | (1 << 2) | (1 << 14));
        let apb1 = core::ptr::read_volatile(RCC_APB1ENR);
        core::ptr::write_volatile(RCC_APB1ENR, apb1 | (1 << 17));

        // PA2 = USART2_TX, PA9 = USART1_TX (AF push-pull), PA10 = USART1_RX.
        let crl = core::ptr::read_volatile(GPIOA_CRL);
        core::ptr::write_volatile(GPIOA_CRL, (crl & !(0xF << 8)) | (0xB << 8));
        let crh = core::ptr::read_volatile(GPIOA_CRH);
        core::ptr::write_volatile(GPIOA_CRH, (crh & !(0xFF << 4)) | (0xB << 4) | (0x4 << 8));

        // Core runs from the 8 MHz HSI and both APB prescalers are 1.
        // 8_000_000 / 19_200 = 416.67 -> 417 (0.08 % off 19200 baud).
        core::ptr::write_volatile(UART1_BRR, 417);
        core::ptr::write_volatile(UART1_CR1, (1 << 13) | (1 << 3) | (1 << 2));
        // Console: 8_000_000 / 115_200 = 69.4 -> 69.
        core::ptr::write_volatile(UART2_BRR, 69);
        core::ptr::write_volatile(UART2_CR1, (1 << 13) | (1 << 3));
    }
}

fn console_byte(b: u8) {
    unsafe {
        for _ in 0..256 {
            if core::ptr::read_volatile(UART2_SR) & SR_TXE != 0 {
                break;
            }
        }
        core::ptr::write_volatile(UART2_DR, b as u32);
    }
}

fn console(s: &str) {
    s.bytes().for_each(console_byte);
}

fn console_u32(mut v: u32) {
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
        console_byte(digits[n]);
    }
}

fn link_send(b: u8) {
    unsafe {
        while core::ptr::read_volatile(UART1_SR) & SR_TXE == 0 {}
        core::ptr::write_volatile(UART1_DR, b as u32);
    }
}

/// One received byte, or `None` after a generous busy-wait timeout.
fn link_recv() -> Option<u8> {
    for _ in 0..4_000_000u32 {
        unsafe {
            if core::ptr::read_volatile(UART1_SR) & SR_RXNE != 0 {
                return Some((core::ptr::read_volatile(UART1_DR) & 0xFF) as u8);
            }
        }
    }
    None
}

/// CRC-16/MODBUS: poly 0xA001 (reflected), init 0xFFFF. Check("123456789") = 0x4B37.
fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[entry]
fn main() -> ! {
    init();
    console("framed-peer lab\r\n");

    // Read two registers starting at 0 from device address 1.
    let mut req = [1u8, 3, 0, 0, 0, 2, 0, 0];
    let crc = crc16(&req[..6]);
    req[6] = crc as u8;
    req[7] = (crc >> 8) as u8;
    for &b in &req {
        link_send(b);
    }

    // addr, func, byte count, two 16-bit words, CRC.
    let mut resp = [0u8; 9];
    let mut got = 0;
    while got < resp.len() {
        match link_recv() {
            Some(b) => {
                resp[got] = b;
                got += 1;
            }
            None => break,
        }
    }
    if got == resp.len() {
        let want = crc16(&resp[..7]);
        let have = resp[7] as u16 | (resp[8] as u16) << 8;
        let r0 = (resp[3] as u32) << 8 | resp[4] as u32;
        let r1 = (resp[5] as u32) << 8 | resp[6] as u32;
        console("[peer] reg0=");
        console_u32(r0);
        console(" reg1=");
        console_u32(r1);
        console(if want == have {
            " crc=ok\r\n"
        } else {
            " crc=BAD\r\n"
        });
    } else {
        console("[peer] timeout\r\n");
    }
    loop {}
}
