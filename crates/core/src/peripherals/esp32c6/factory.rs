// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Data-driven factory for ESP32-C6 peripheral models.
//!
//! Every arm here either parameterizes a shared model with the C6's values or
//! aliases the C3 model whose register map the C6 genuinely shares. The C6 has
//! no C3-only `system` / `rtc_cntl` / `apb_ctrl` blocks and no C6-specific
//! behavioral model yet; the clock/reset (PCR) and IO_MUX windows are
//! SVD-derived `declarative` descriptors wired directly in `esp32c6.yaml`.

use crate::Peripheral;
use labwired_config::PeripheralConfig;

pub fn try_build(canonical_type: &str, p_cfg: &PeripheralConfig) -> Option<Box<dyn Peripheral>> {
    let dev: Box<dyn Peripheral> = match canonical_type {
        // Real ESP-IDF UART register head + TX FIFO + interrupts, paced at the
        // C6's 160 MHz. UART0 is the DevKitC-1's USB-UART bridge console and
        // echoes to stdout; UART1 is capture-only. Source id comes from the
        // descriptor's `irq:` (43/44 on the C6) or the base-address default.
        "esp32c6_uart" => {
            let source_id = p_cfg
                .irq
                .unwrap_or_else(|| super::uart::default_source_id(p_cfg.base_address));
            let echo = p_cfg
                .config
                .get("echo_stdout")
                .and_then(|v| v.as_bool())
                .unwrap_or(source_id == super::uart::UART0_INTR_SOURCE_ID);
            Box::new(super::uart::new(echo, source_id))
        }
        // The C6 GPIO matrix shares the C3 model's register head (OUT/W1TS/W1TC
        // @0x04-0x0C, ENABLE @0x20, STRAP @0x38, IN @0x3C, FUNC0_OUT_SEL_CFG
        // @0x554, FUNC0_IN_SEL_CFG @0x154 — cross-checked against esp-idf v5.3
        // `soc/esp32c6/include/soc/gpio_reg.h`). It is sized for the C3's
        // 26-pin block, so C6 GPIO26..GPIO30 are not modelled. The C6's
        // I2C-matrix signal indices differ from the C3's (I2CEXT0_SCL is 45 vs
        // 53), so the C3 I2C pad wiring does not apply — UART0/UART1 TX indices
        // (6/9) happen to match and do.
        "esp32c6_gpio" => Box::new(crate::peripherals::esp32c3::gpio::Esp32c3Gpio::new()),
        _ => return None,
    };
    Some(dev)
}

pub const SUPPORTED_TYPES: &[&str] = &["esp32c6_uart", "esp32c6_gpio"];
