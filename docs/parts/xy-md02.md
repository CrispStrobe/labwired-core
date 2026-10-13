# XY-MD02

A real product: the XY-MD02 "Modbus RTU RS485 SHT20 Temperature Humidity
Transmitter", a DIN-rail housing (65 x 46 x 28.5 mm) with a 4-way screw
terminal. The twin is a data pack on the `uart_device` primitive
(`configs/devices/xy-md02.yaml`), so it runs in the browser, in hosted runs, in
CI and from Python the same way.

## Source

The register map, defaults and frame examples come from the manufacturer manual
"XY-MD02" (copy retrieved 2026-10-04):

- <https://www.hestore.hu/prod_getfile.php?id=18062>
- <https://knowledgebase.grenton.com/hubfs/xy-md02-manual.pdf>

A second revision of the manual (60 x 30 x 18 mm, same register map) is at
<https://iot-kmutnb.github.io/blogs/sensors/xy-md02/xy-md02_manual-2.pdf>. The
terminal order comes from the label on the housing, `B-  A+  RS485` and
`-  +  DC 5~30V`, read in the same order as the 4-way terminal.

## Status at a glance

| Aspect | Status |
|--------|--------|
| Catalog id | `xy-md02` |
| Bus | UART behind a [MAX485](max485.md) (terminals A+ and B-) |
| Descriptor | `configs/devices/xy-md02.yaml` |
| Settings | `address` (1..247, default 1), `baud` (9600, 14400 or 19200, default 9600), `temp_correction`, `hum_correction` |
| Inputs | `temperature` (C), `humidity` (%RH) |
| Power | DC 5 to 30 V |
| Terminal | `B-`, `A+`, `GND`, `V+`, in that order |
| Example | [`examples/arduino-uno-modbus-rtu`](../../examples/arduino-uno-modbus-rtu/README.md) |
| Tier | modeled (protocol level) |

## Register map

Function codes: 03 read holding, 04 read input, 06 write one holding, 10 (16)
write several holding.

| Address | Kind | Content | Encoding |
|---------|------|---------|----------|
| 0x0001 | input (04) | temperature | signed, tenths of a degree C (0x0131 = 30.5, 0xFF33 = -20.5) |
| 0x0002 | input (04) | humidity | tenths of %RH (0x0222 = 54.6) |
| 0x0101 | holding (03, 06, 10) | device address | 1..247, default 1 |
| 0x0102 | holding | baud rate | the bit rate itself: 9600 = 0x2580 (default), 14400, 19200 |
| 0x0103 | holding | temperature correction | signed tenths, -100..100 (-10.0..10.0 C), default 0 |
| 0x0104 | holding | humidity correction | signed tenths, -100..100, default 0 |

The measured value reads `input + correction`. Writing 0x0101 changes the
address at once; the answer to that request still carries the old address.

Factory defaults: address 1, 9600 baud, 8 data bits, 1 stop bit, no parity.

Every frame example in the manual is a test (`rs485_modbus.rs`,
`manual_examples`). Two CRCs the manual prints are wrong and are not copied: the
humidity response (`D1 BA`; the CRC of that frame is `38 49`) and the function-06
response (`D4 0F`; a function-06 answer echoes the request, `D8 30`).

## Where the manual is silent or contradicts itself

- **Baud register.** The table says `0:9600 1:14400 2:19200`; the function-10
  example writes `0x2580` into it. The twin stores and reads back the bit rate
  and accepts 9600, 14400 and 19200. The codes 0 to 2 answer exception 03. A
  written rate is stored but the line keeps the rate it was placed with; a real
  part applies it at the next power-up.
- **Exceptions.** The manual lists none. The twin answers the standard Modbus
  ones.
- **Wrong kind of register.** A read of 0x0001 with function 03, or of 0x0101
  with function 04, answers exception 02.
- **Broadcast.** The manual gives addresses 1..247 and says nothing about 0, so
  address 0 is ignored.
- **Ranges.** The manual gives temperature -40..60 C and humidity 0..80 %RH in
  one revision and humidity 0..100 in the other; the simulation inputs allow
  -40..60 C and 0..100 %RH.

## Behaviour on the wire

- A frame ends after 3.5 character times of silence at the part's baud
  (10 bits per character).
- A bad CRC-16/MODBUS, a frame for another address, a frame split by a longer
  gap and a frame shorter than address + function + CRC get no answer.
- Exception 01 for an unsupported function code, 02 for a register or range
  outside the map or a write to an input register, 03 for a count outside
  1..125 (123 for function 10), a byte count that does not match the register
  count, an address outside 1..247, a baud that is not 9600, 14400 or 19200, or
  a correction outside -10.0..10.0.

## Limits

- The manual's "ordinary UART protocol" (text commands `READ`, `AUTO`, `BR:`,
  `TC:`, `HC:`, `HZ:`, `PARAM`) is not modelled.
- No parity setting (8N1).
- A function-10 write checks the range of its first two values only.
- The fixed 1.75 ms gap Modbus asks for above 19200 baud is not applied.

## Related

- [MAX485 transceiver](max485.md)
- [Part packs](../part-packs.md)
- [Parts index](index.md)
