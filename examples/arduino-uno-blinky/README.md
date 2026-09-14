# Arduino Uno R3 golden blink + Serial

PlatformIO `uno` sketch — AVR sim-smoke survival golden for the Uno R3 board
manifest (`configs/systems/arduino-uno.yaml`). Same ATmega328P core as the
Nano golden; the board differs in pinout and form factor, not silicon.

```bash
pio run -d examples/arduino-uno-blinky
cp examples/arduino-uno-blinky/.pio/build/uno/firmware.elf tests/fixtures/avr/arduino-uno-blinky.elf
```
