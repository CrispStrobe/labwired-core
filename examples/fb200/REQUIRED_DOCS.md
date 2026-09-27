# Required Source Documents (FB200 / i.MX RT1052)

## MCU Datasheet (authoritative)

1. NXP i.MX RT1050 Processor Reference Manual — **IMXRT1050RM** (memory map, CCM, IOMUXC, GPIO DR_TOGGLE, LPUART):
   https://www.nxp.com/docs/en/reference-manual/IMXRT1050RM.pdf
2. NXP SVD **MIMXRT1052** (vendored: `tests/fixtures/real_world/mimxrt1052.svd`) — peripheral bases and fields used by the smoke: CCM `0x400FC000` (CCGR3 CG1 = lpuart5, CG6 = gpio4), LPUART5 `0x40194000` (STAT `+0x14`, CTRL `+0x18`, DATA `+0x1C`), GPIO4 `0x401C4000` (GDIR `+0x04`, DR_TOGGLE `+0x8C`).

## Board Pinout / BSP

1. fb200-tools `docs/UI_AND_STORAGE.md`, `HARDWARE.md` — SoC marking MIMXRT1052DVL6B; Bluetooth module on LPUART5 (B1_12 TX / B1_13 RX, 115200 8N1); knob LEDs on GPIO4_IO0..15, active low. Summarised in `configs/systems/fb200.yaml`.

## Scope

1. SIM-DERIVED — not silicon-verified. The smoke firmware is open and minimal; it is not the FB200 stock image, which is vendor firmware and not redistributable (`crates/core/tests/mimxrt1052_fb200_stock_boot.rs`, `LABWIRED_FB200_STOCK_MR`).
