# PyBadge native panel/runtime contract — 2026-10-07

Buttons are merged in [PR151](https://github.com/CrispStrobe/labwired-core/pull/151).
Blocking SAM SPI landed in
[PR152](https://github.com/CrispStrobe/labwired-core/pull/152).
Idle-controller batching and clock-freeze repair landed in
[PR159](https://github.com/CrispStrobe/labwired-core/pull/159).
Its [native chip-spin receipts](../receipts/2026-10-07-sam-spi-p0/README.md)
do not qualify active display, full-board batching or WASM. The full board's
DATA-driving GPIO resident still requires per-cycle service. Active SPI retains
one-cycle service; no active-display performance gain is claimed.
The [ST7735 colour/GPIO foundation](../engineering/st7735-color-foundation.md)
landed in [PR161](https://github.com/CrispStrobe/labwired-core/pull/161), with
four authored blocking SAM guest tests passing in both feature configurations.
Its test-only 2×2 crop is not the deployed panel. Production panel binding,
DMA and native Arcade are not yet qualified. Work contracts
and pass criteria: [engine lanes](../engineering/target-next-lanes.md).

## Wiring and primary definitions

SERCOM4 is `0x43000000`, MCLK APBDMASK bit 0 and GCLK core channel 34.
PB13/PAD1 SCK and PB15/PAD3 MOSI need mux C and DOPO=2.
Display CS PB7, D/C PB5, reset PA0, backlight PA1.
Buttons latch PB0, clock PB31, DATA PB30; host 1 is pressed, raw bits active-low.
The existing five NeoPixels are PA15.

- [SERCOM4 instance map](https://github.com/arduino/ArduinoModule-CMSIS-Atmel/blob/46ab1021146152a64caf1ddbb837d8181b8faa35/CMSIS-Atmel/CMSIS/Device/ATMEL/samd51/include/instance/sercom4.h)
- [Pinned PAD/DOPO mapping](https://github.com/lancaster-university/samd-peripherals/blob/96563308fc7b97646cbe429953e79cb3405846f0/samd/samd51/sercom.c)
- [PyBadge display wiring/init](https://github.com/adafruit/Adafruit_Arcada/blob/19f707488a21981819a10e83ae785bcddea82ad4/Boards/Adafruit_Arcada_PyBadge.h)
- [ST7735R datasheet](https://cdn-shop.adafruit.com/datasheets/ST7735R_V0.2.pdf)

## ST7735 is not a renamed ST7789

`configs/devices/st7789.yaml` is 240x320 fixed RGB565; consuming a COLMOD
argument is not ST7735 format support. Generic display reset/backlight GPIO
observation now has bounded write-driven guest qualification; external net-only
notifications and physical timing remain excluded. ST7735R RAM is 132x162; GM=11 uses 128x160
address bounds, GM=00 full bounds. Verify actual module mode/visible offsets
before mirror/crop formulas, with four corners tested in all orientations.

Datasheet section 9.15 specifies RAM retention across hardware/software reset.
Software reset preserves MADCTL/COLMOD; hardware restores defaults. COLMOD
low bits select RGB444 (3), RGB565 (5), RGB666 (6), hardware default 6.
Do not clear RAM or uniformly reset all variables.

Pinned stock CODAL initializes COLMOD=3 and uploads command 0x2D RGBSET with
128 LUT bytes (R32/G64/B32); its ordinary indexed-image path sends packed
RGB444, three bytes for two pixels. RGB565-only decoding cannot qualify this
production stream. Use independent datasheet tests and exact driver inputs:

- [Pinned CODAL ST7735](https://github.com/lancaster-university/codal-core/blob/312ae57e0b31f5b9df07a81e9d846945828e3c5a/source/drivers/ST7735.cpp)
- [Pinned PXT screen](https://github.com/microsoft/pxt-common-packages/blob/31abf23d118f35010fb75122e60a1eb2b6dffe9f/libs/screen---st7735/screen.cpp)

Conditional `USE_RGB444` and doubled-image source paths do not establish the
actual macro configuration/CF2 board ID; verify the deployed build first.

### Pinned PXT package configuration — source review, not guest proof

The consumer's `scripts/sync-makecode-runtime.mjs` pins
[pxt-arcade 4.2.1](https://registry.npmjs.org/pxt-arcade/-/pxt-arcade-4.2.1.tgz)
archive SHA256 `d403926da1c96dd71a696f0182dbdd82b97babb9d0de9281b8e2a6b67c57cf32`.
The archive's `package/built/target.json` and inspected served target match:
SHA256 `6f35aadf1456436b3318d2f2339cc3d1add6e6c3b0bf8566f86d7f7bb4f7722e`.
Its bundled `hw---samd51adafruit/pxt.json` sets
`yotta.config.USE_RGB444 = 1`; the bundled `config.ts` supplies no display
constants and explicitly takes configuration from the bootloader.

The bundled `screen---st7735/screen.cpp` has SHA256
`dea9ea175d65d885275eb0715d56353674feae88d64d29a5eb2809f899a0d958`,
matching the pinned PXT screen source linked above. With SPI and a recognized
Adafruit board ID, its non-doubled path calls `sendIndexedImage444`: direct
palette-to-RGB444 bytes, rather than CODAL's RGBSET-indexed image path. Recognized
IDs have high 16 bits `0x239A`, or equal one of `0x18591AB9`, `0x75FDEB5F`,
`0x3F05BA69`, `0x2DD7A88C`, `0x2B9E3D05`, `0x7A236324`. Unknown or absent
IDs retain the stock RGBSET path. Both paths therefore need qualification;
passing the CODAL RGBSET capture alone cannot qualify known-Adafruit rendering.

This verifies the package's macro request and branch source, **not** generated
compiler definitions, effective CF2 settings, selected runtime branch, module
aperture or ARM execution. Next bind the actual generated build request and
effective board ID/display settings, capture the exact-source direct renderer
with selected/unknown/absent-ID controls, then prove its bytes through guest SPI.
Preserve odd-width/height packing, transfer batching and CS/D/C behavior; do not
replace the production method with an authored equivalent and call that original
source execution. Full original ARM/DMA completion remains a later requirement.

There is also an unresolved model boundary: the current ST7735R prototype's
RGB444 decoder requires a valid RGBSET table, while the selected direct-renderer
branch is designed not to upload that table. Capturing its wire bytes alone will
not establish visible colours in this prototype. Establish the actual module's
colour-transfer behavior and qualify an explicit supported profile; do not seed
an invented identity LUT or weaken the existing unknown-pixel controls to obtain
a passing frame.

### Public bootloader candidate — not yet adopted

[Adafruit's PyBadge CF2 source at d4dc9288](https://github.com/adafruit/uf2-samdx1/blob/d4dc92889759c0c551683420e24c5ef535ac303e/boards/arcade_pybadge/board_config.h)
provides a reproducible candidate input: board ID `0x239A0033`, width 160,
height 128, `DISPLAY_CFG0=0x80`, `DISPLAY_CFG1=0x12C2D` and
`DISPLAY_CFG2=0x18`, with reset PA0, backlight PA1 and the SPI wiring above.
That board ID satisfies the pinned PXT renderer's Adafruit predicate. These
are source defaults, not evidence that an emulator or installed application
loaded those values. Qualify their effective selection explicitly; do not inject
them silently to make an existing guest progress.

The pinned Arcada board source linked above instead calls
`initR(INITR_BLACKTAB)` and rotation 1 for a 160×128 display. The independently
[pinned Adafruit driver](https://github.com/adafruit/Adafruit-ST7735-Library/blob/62112b90eddcb2ecc51f474e9fe98b68eb26cb2a/Adafruit_ST7735.cpp)
uses zero start offsets for BLACKTAB and MY|MV|RGB for rotation 1. This is
another software initialization convention, not proof of the module's GM straps
or equivalence to CODAL's swapped coordinate convention. Keep GM00's 132×162
memory fixture separate from the module aperture/mirroring decision until that
decision has primary module evidence and independent corner tests.

## Runtime provenance and DMA dependency

[Lite run36567239929](https://github.com/CrispStrobe/brickwright-lite/actions/runs/36567239929)
records samd51adafruit request SHA256
`19efcdc73769fdfdeb51aa215c528bebad59782cbc538f72f4a194326f1f42b1`,
HEX SHA256 `9c2310bd5a65f0543c69a076e51a4067c803202a39228f3a7de41451be0da9ca`.
Its codal-core pin is `312ae57e0b31f5b9df07a81e9d846945828e3c5a`,
codal-itsybitsy-m4 `6ecd80ccf1abcc126d06653f178ca03a4d1c6421`,
codal-samd `5bd6b93c219c7e784e885ba2d6812809fb6289a8`,
asf4 `6664673f70d9170b4374a726c5322e9be6b3f237`,
samd-peripherals `96563308fc7b97646cbe429953e79cb3405846f0`.
Historical manifest provenance is not current app pin-adoption proof.

Even a one-byte write uses DMA, waits for TXC and wakes a callback/fiber:

- [Pinned ZSPI](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/ZSPI.cpp)
- [DMA descriptors](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/DmaInstance.cpp)
- [DMA initialization/IRQ dispatch](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/DmaFactory.cpp)

SERCOM4 RX/TX triggers 12/13; DMAC channels 0..3 IRQ 31..34, 4..31 shared 35.
End-address descriptors, writeback, clock/reset/error/mask semantics need guest
proof, not host-completed transfers. Keep QSPI/USB/audio and controller IRQ/
dynamic-clock limits explicit until separately qualified. Nominal BAUD delays,
terminal-spin RTx and wall-paced PXT do not prove native frames or silicon timing.
