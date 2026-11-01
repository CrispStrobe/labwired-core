# Original S040 Xtensa register probe

`build_probe.py` is the complete MIT-licensed source and minimal instruction/ELF
encoder for `probe.elf`. Run it with Python 3; it requires no SDK or third-party
package. Rebuilding must reproduce SHA-256
`fc5563c16da88dc609c5ada36b17c72a611cc4998675d502572318c1657c419f`
(18,032 bytes).

The ELF is little-endian EM_XTENSA (94), with entry `0x40379000` and an IRAM
literal/code segment at `0x40378000`. The 480 row descriptors start at
`0x3fc90000`; three 960-byte RGB565 row buffers start at `0x3fc92000`. The first
row is mutable, rows 1…239 repeat the second buffer, and rows 240…479 repeat
the third. These real descriptors produce the complete 480×480 frame without
a cache-dependent full-frame SRAM allocation. It has no operating system,
stack use, ROM calls, library hooks or external binary dependency. Load it as
an S3 **bare ELF entry**, not a bootable merged flash image.

The generator checks non-overlap using the physical SRAM1 DRAM↔IRAM alias
delta `0x006f0000`. Its code, descriptors, mailbox and row buffers stay in
SRAM1 and avoid cache-owned SRAM0/SRAM2 and the ROM stack. The source is
[Espressif's S3 memory definitions](https://github.com/espressif/esp-idf/blob/fcae32885b0296b32044cb99ecbdc50d98dddb83/components/soc/esp32s3/include/soc/soc.h)
and the same revision's `components/heap/port/esp32s3/memory_layout.c`.

Firmware bit-bangs ST7701 control on CS39/SCK48/MOSI47, enables BL38, programs
RGB mode and one-shot GDMA, and polls LCD VSYNC. It then polls the GT911 on I2C0
at `0x5d`, acknowledges each report, modifies pixel zero using its X coordinate
(release = red), and submits a new frame. Touch reports are copied at
`0x3fc91808`; a completed-frame/report counter is at `0x3fc91800`. UART0 emits
`S040 READY` for the initial frame and `S040 TOUCH` for each redraw.

This probe establishes the finite functional register path and guest redraw.
It is not Arduino/LVGL, a physical-board capture, a PSRAM allocator, a timing
benchmark, or evidence for arbitrary vendor firmware compatibility. The
separate integration test covers MMU-mapped PSRAM frame reads. See
`docs/boards/esp32-4848s040.md` for the model boundaries and primary references.

`node tests/fixtures/esp32-4848s040/check_wasm.cjs /path/to/nodejs-engine`
executes the guest through `WasmSimulator.new_from_config`, checks the actual
display/touch path and UART drain, then qualifies the named GPIO external-drive
API, malformed/unbonded refusals, true release, open drain and journal restore.
The engine directory must contain matching `labwired_wasm.js` and
`labwired_wasm_bg.wasm`; the JSON result records both hashes. This local test is
not a substitute for the immutable two-build release process.
