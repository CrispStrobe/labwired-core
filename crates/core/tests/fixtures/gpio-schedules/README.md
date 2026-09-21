These Cortex-M4 fixtures busy-poll real GPIO registers and record observations
in SRAM. They contain no expected sensor bytes or pulse widths. The same
fixtures are exercised through the app's Wasm runtime.

Rebuild each fixture from this directory (replace `dht-frame` with `echo-pulse`
for the second):

```sh
arm-none-eabi-gcc -mcpu=cortex-m4 -mthumb -Os -ffreestanding -fno-builtin -nostdlib -Wl,--build-id=none -T gpio-inputs.ld dht-frame.c -o dht-frame.elf
```
