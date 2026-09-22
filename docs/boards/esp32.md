# ESP32 (classic, dual-core Xtensa LX6)

The original ESP32 — dual-core Xtensa LX6, 240 MHz, no FPU. This profile
models the memory map probed against a real ESP32-WROOM-32 module (chip type
`esp32` revision v1.0, 4 MiB flash) per the comment in
`configs/chips/esp32.yaml`.

## Status at a glance

| Aspect      | Status                                                              |
|-------------|----------------------------------------------------------------------|
| Chip yaml   | [`configs/chips/esp32.yaml`](../../configs/chips/esp32.yaml)         |
| Validation  | Tier-1 fast-boot fixture (`tests/fixtures/tier1/esp32.elf`) + PR-run gate `firmware_survival::test_esp32_tier1_survival` |
| Tier        | **structural** — declared peripherals only, no silicon diff; the behaviour gate is the Tier-1 fast-boot self-test, passing with the documented dma gap (`esp32-no-mem2mem-dma`) |

## What is modeled (from the chip yaml)

- IRAM (0x4008_0000), DRAM (0x3FFB_0000), boot ROM (0x4000_0000), and the two
  flash XIP cache windows (I-cache 0x400D_0000, D-cache 0x3F40_0000)
- UART0 (`0x3FF4_0000`)
- GPIO (`gpio_esp32`, `0x3FF4_4000`)
- Timer group `timg0` (`0x3FF5_F000`)
- RTC_CNTL and the DPORT interrupt matrix (declared as stubs)
- SPI3 (`esp32_spi`), I2C0 (`esp32_i2c`), SAR ADC, LEDC

## What is NOT proven

- `firmware_survival::test_esp32_tier1_survival` runs the tier-1 fast-boot
  fixture, which is a raw-register self-test, not an application boot; it
  passes with the documented dma gap (`esp32-no-mem2mem-dma`).
- No silicon diff of any kind against a real ESP32.
- WiFi/BT/radio are not modeled.

## Sources

- ESP32 Technical Reference Manual v4.6 §1.3 (Memory Map), Table 1-2, as cited
  in `configs/chips/esp32.yaml`.
