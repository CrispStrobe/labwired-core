#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Build an original, independently authored Xtensa MMIO guest (stdlib only).

No Arduino/ROM calls or emulator hooks: real instructions bit-bang ST7701,
program LCD_CAM/GDMA, poll VSYNC, read GT911 over I2C, acknowledge each report
and redraw. This is a focused register probe, not a vendor/LVGL boot image.
"""
from pathlib import Path
import hashlib
import struct

BASE = 0x40378000
ENTRY = BASE + 0x1000
DESC = 0x3FC90000
RESULT = 0x3FC91800
FRAME = 0x3FC92000
GPIO = 0x60004000
LCD = 0x60041000
DMA = 0x6003F000
I2C = 0x60013000
code = bytearray()
literals = []
labels = {}
fixups = []


def insn(word):
    code.extend(word.to_bytes(3, "little"))


def imm(reg, value):
    value &= 0xffffffff
    if value not in literals:
        literals.append(value)
    literal = BASE + literals.index(value) * 4
    pc = ENTRY + len(code)
    relative = (literal - ((pc + 3) & ~3)) // 4
    assert -65536 <= relative < 0
    insn(1 | reg << 4 | (relative & 0xffff) << 8)


def store(reg, base, offset=0, size=4):
    insn(2 | reg << 4 | base << 8 | {1: 4, 2: 5, 4: 6}[size] << 12 | (offset // size) << 16)


def load(reg, base, offset=0, size=4):
    insn(2 | reg << 4 | base << 8 | {1: 0, 2: 1, 4: 2}[size] << 12 | (offset // size) << 16)


def write(address, value):
    imm(2, address); imm(3, value); store(3, 2)


def label(name):
    labels[name] = len(code)


def jump(name, register=None, nonzero=False):
    fixups.append((len(code), name, register, nonzero))
    insn(0)


def and_reg(dst, left, right):
    insn(0x100000 | dst << 12 | left << 8 | right << 4)


def pin(number, high):
    assert number >= 32
    write(GPIO + (0x14 if high else 0x18), 1 << (number - 32))


def serial(word):
    pin(39, False)
    for bit in range(8, -1, -1):
        pin(48, False); pin(47, bool(word & (1 << bit))); pin(48, True)
    pin(48, False); pin(39, True)


def uart(message):
    for byte in message.encode():
        write(0x60000000, byte)


def start_frame():
    write(LCD + 0x70, 3)
    write(DMA + 0x80, (DESC & 0xfffff) | (1 << 21))
    write(LCD + 0x14, (1 << 23) | (1 << 24) | (1 << 27) | (1 << 13))


def await_frame(tag):
    label(tag)
    imm(2, LCD + 0x68); load(4, 2); imm(5, 1); and_reg(4, 4, 5)
    jump(tag, 4)


def i2c_command(tx, commands):
    write(I2C + 0x18, (1 << 12) | (1 << 13))
    for byte in tx:
        write(I2C + 0x1c, byte)
    for i in range(8):
        write(I2C + 0x58 + 4 * i, commands[i] if i < len(commands) else (4 << 11))
    write(I2C + 4, 0x20b | (1 << 5))


# FUNC1 is GPIO, including CS39 whose reset FUNC0 is MTCK. Configure the
# physical pads before enabling the actual control bank and releasing CS.
for pin_number in (38, 39, 47, 48):
    write(0x60009004 + pin_number * 4, 0x1b00)
write(GPIO + 0x30, sum(1 << (p - 32) for p in (38, 39, 47, 48)))
pin(39, True); pin(38, True)
# Exercise the type9 extended-bank select, return to DCS, then sleep-out/on.
for word in (0xff, 0x177, 0x101, 0x100, 0x100, 0x110,
             0xff, 0x177, 0x101, 0x100, 0x100, 0x100,
             0x3a, 0x160, 0x11, 0x29):
    serial(word)
# Known S040 totals and active area. Clock is stored, not timing-qualified.
write(LCD + 0x00, (1 << 31) | (3 << 29) | (20 << 9) | (1 << 6))
write(LCD + 0x1c, (1 << 31) | (479 << 11) | (517 << 21) | 57)
write(LCD + 0x20, (547 << 20) | (479 << 8) | 27)
write(LCD + 0x24, 7 | (1 << 7) | (7 << 16) | (1 << 23))
write(LCD + 0x64, 1)
write(DMA + 0xa8, 5)
# Matrix route for touch: I2C0 SDA=19, SCL=45, both input and output.
for pin_number, signal in ((19, 90), (45, 89)):
    write(0x60009004 + pin_number * 4, 0x1b00)
    write(GPIO + 0x554 + pin_number * 4, signal)
    write(GPIO + 0x154 + signal * 4, (1 << 7) | pin_number)
start_frame(); await_frame("first_frame")
write(RESULT, 1); uart("S040 READY\n")

label("poll")
i2c_command([0xba, 0x81, 0x4e, 0xbb],
            [6 << 11, (1 << 11) | 3, 6 << 11, (1 << 11) | 1,
             (3 << 11) | 9, 2 << 11])
imm(2, I2C + 0x1c); imm(6, RESULT + 8)
for offset in range(9):
    load(4, 2); store(4, 6, offset, 1)
load(4, 6, 0, 1); imm(5, 0x80); and_reg(4, 4, 5)
jump("poll", 4)
# A held contact paints pixel 0 with its actual X coordinate; release is red.
load(4, 6, 0, 1); imm(5, 1); and_reg(4, 4, 5)
jump("released", 4)
load(7, 6, 2, 2); jump("paint")
label("released"); imm(7, 0xf800)
label("paint"); imm(2, FRAME); store(7, 2, 0, 2)
i2c_command([0xba, 0x81, 0x4e, 0], [6 << 11, (1 << 11) | 4, 2 << 11])
start_frame(); await_frame("redraw")
imm(2, RESULT); load(4, 2); insn(2 | 4 << 4 | 4 << 8 | 0xc << 12 | 1 << 16); store(4, 2)
uart("S040 TOUCH\n"); jump("poll")

for offset, name, register, nonzero in fixups:
    delta = labels[name] - offset - 4
    if register is None:
        word = 6 | (delta & 0x3ffff) << 6
    else:
        assert -2048 <= delta < 2048
        word = 0x16 | int(nonzero) << 6 | register << 8 | (delta & 0xfff) << 12
    code[offset:offset + 3] = word.to_bytes(3, "little")
pool = b"".join(struct.pack("<I", n) for n in literals)
assert len(pool) <= 0x1000
text = pool.ljust(0x1000, b"\0") + code
# Three actual row buffers produce independent expected color quadrants. The
# first row is separate so touch changes only pixel zero. The remaining top
# and bottom rows reuse their respective buffers through real descriptors.
# This keeps code/data in SRAM1, away from cache-owned SRAM0/SRAM2 and the
# ROM stack. S3 DRAM->IRAM alias delta is +0x6f0000 (ESP-IDF soc.h).
row_bytes = 480 * 2
top = b"".join(struct.pack("<H", 0xf800 if x < 240 else 0x07e0) for x in range(480))
bottom = b"".join(struct.pack("<H", 0x001f if x < 240 else 0xffff) for x in range(480))
pixels = top + top + bottom
descriptors = bytearray()
for i in range(480):
    length = row_bytes
    last = i == 479
    row = 0 if i == 0 else 1 if i < 240 else 2
    descriptors += struct.pack("<III", (1 << 31) | (int(last) << 30) | length << 12 | length,
                               FRAME + row * row_bytes, 0 if last else DESC + (i + 1) * 12)
# SRAM1 [3fc88000,3fcf0000) aliases IRAM [40378000,403e0000).
# Guard the physical layout independently of the emulator's RAM allowance.
assert 0x40378000 <= BASE < ENTRY < 0x403e0000
assert BASE - 0x6f0000 + len(text) <= DESC
assert DESC + len(descriptors) <= RESULT
assert RESULT + 32 <= FRAME
assert FRAME + len(pixels) <= 0x3fcf0000
segments = [(BASE, text, 5), (DESC, descriptors, 6), (FRAME, pixels, 6)]
header_size = 52 + len(segments) * 32
header = bytearray(b"\x7fELF\x01\x01\x01" + bytes(9))
header += struct.pack("<HHIIIIIHHHHHH", 2, 94, 1, ENTRY, 52, 0, 0, 52, 32, len(segments), 0, 0, 0)
body = bytearray()
for address, data, flags in segments:
    header += struct.pack("<IIIIIIII", 1, header_size + len(body), address, address, len(data), len(data), flags, 4)
    body += data
elf = bytes(header + body)
destination = Path(__file__).with_name("probe.elf")
destination.write_bytes(elf)
print(f"{hashlib.sha256(elf).hexdigest()}  {destination.name} ({len(elf)} bytes)")
