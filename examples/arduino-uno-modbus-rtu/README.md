# Arduino Uno: Modbus RTU master, MAX485, two XY-MD02 sensors

An Uno polls two XY-MD02 Modbus RTU temperature and humidity sensors over one
RS-485 pair, with the ModbusMaster library (Apache-2.0). The firmware reads the
XY-MD02's input registers 0x0001 (temperature) and 0x0002 (humidity) and, once,
writes its temperature correction register 0x0103. A stimulus warms one sensor
mid-run; the master prints the new value on its next poll. The register map is
from the manufacturer manual; see [XY-MD02](../../docs/parts/xy-md02.md).

```
Uno D1 (TX) -> DI ┐              ┌─ A ── slave 1 (address 1) ── A ── 120 Ohm ─┐
Uno D0 (RX) <- RO ├─ MAX485 ─────┤                                              │
Uno D2 -> DE, /RE ┘              └─ B ── slave 2 (address 2) ── B ── 120 Ohm ─┘
```

The termination resistors sit at the two ends of the pair. They are parts in
the playground diagram. The simulator works at byte level, so they do not change
what is delivered.

| File | What |
|------|------|
| `src/main.cpp` | The sketch |
| `platformio.ini` | `atmelavr` / `uno` / `arduino`, `ModbusMaster@2.0.1` |
| `system.yaml` | The MAX485 and the two `xy-md02` parts on `usart0` |
| `io-smoke.yaml` | CLI smoke, a stimulus and a bus-log check |
| `REQUIRED_DOCS.md`, `EXTERNAL_COMPONENTS.md`, `VALIDATION.md` | Docs pack |

## Run

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/arduino-uno-modbus-rtu/io-smoke.yaml \
  --no-uart-stdout
```

The console shows what the firmware printed:

```
modbus-rtu master up
poll 1 s1 T=21.5 H=48.0
poll 1 s2 T=19.0 H=55.5
poll 2 s1 reg 0x0200 -> 0x2
poll 2 s2 write correction -> 0x0
poll 3 s2 T=19.5 H=55.5
poll 4 s1 T=25.0 H=48.0
```

The `rs485` log on `usart0` has every frame that crossed the pair:

```
t=0.583ms master: 01 04 00 01 00 02 20 0B
t=4.270ms slave s1: 01 04 04 00 D7 01 E0 4B A4
```

An independent decoder checks those frames: `scripts/ci/test_modbus_pymodbus.py`
(pymodbus RTU framer).

## Rebuild the fixture

```bash
pio run -d examples/arduino-uno-modbus-rtu
cp examples/arduino-uno-modbus-rtu/.pio/build/uno/firmware.elf tests/fixtures/avr/arduino-uno-modbus-rtu.elf
cp examples/arduino-uno-modbus-rtu/.pio/build/uno/firmware.hex tests/fixtures/avr/arduino-uno-modbus-rtu.hex
```
