# Modbus RTU sensor

A generic Modbus RTU slave: temperature, humidity and a raw 16-bit value on an
RS-485 pair. It is a data pack on the `uart_device` primitive
(`configs/devices/modbus-rtu-sensor.yaml`), so it runs in the browser, in hosted
runs, in CI and from Python the same way.

## Status at a glance

| Aspect | Status |
|--------|--------|
| Catalog id | `modbus-rtu-sensor` |
| Bus | UART behind a [MAX485](max485.md) (pins A and B) |
| Descriptor | `configs/devices/modbus-rtu-sensor.yaml` |
| Settings | `address` (1..247, default 1), `baud` (default 9600), `temp_offset` |
| Inputs | `temperature` (C), `humidity` (%RH), `raw` (0..65535) |
| Example | [`examples/arduino-uno-modbus-rtu`](../../examples/arduino-uno-modbus-rtu/README.md) |
| Tier | modeled (protocol level) |

## Register map

| Register | Input (04) | Holding (03, 06, 16) |
|----------|------------|----------------------|
| 0 | temperature, 0.1 C, signed | same value, read only |
| 1 | humidity, 0.1 %RH | same value, read only |
| 2 | raw 16-bit value | same value, read only |
| 256 (0x0100) | | slave address, 1..247, writable |
| 257 (0x0101) | | temperature offset, 0.1 C, writable |

The temperature register reads `temperature + offset`. Writing register 256
changes the address at once; the answer to that request still carries the old
address.

## Behaviour on the wire

- A frame ends after 3.5 character times of silence at the part's baud
  (10 bits per character).
- A bad CRC-16/MODBUS, a frame for another address, a frame split by a longer
  gap and a frame shorter than address + function + CRC get no answer.
- Exception 01 for an unsupported function code, 02 for a register or range
  outside the map or a write to a read-only register, 03 for a count outside
  1..125 (123 for function 16), a byte count that does not match the register
  count, or an address outside 1..247.
- Address 0 is a broadcast write to register 257 and is never answered.

## Limits

- No parity setting (8N1).
- No broadcast for function 16.
- The fixed 1.75 ms gap Modbus asks for above 19200 baud is not applied.
- Function codes other than 03, 04, 06 and 16 answer exception 01.

## Related

- [MAX485 transceiver](max485.md)
- [Part packs](../part-packs.md)
- [Parts index](index.md)
