Real Arduino-ESP32 images that drive two LEDC channels through five duty
phases, for `crates/cli/tests/esp32_ledc_pad.rs` (LEDC duty on the pad).

* `ledc_pwm.ino` — the sketch (GPIO2 `analogWrite`, GPIO5 `ledcSetup` +
  `ledcAttachPin` + `ledcWrite`, GPIO4 a plain phase marker).
* `<chip>/firmware.elf` — built from it for `esp32`, `esp32c3`, `esp32s3`,
  then `--strip-debug` (symbols kept: the boot paths resolve thunks from the
  symbol table).
* `<chip>/partitions.bin` — the same build's partition table; `labwired test`
  seeds it at flash 0x8000 when it sits beside the ELF.

Rebuild (PlatformIO Core 6.2.0, platform `espressif32@7.0.1`, i.e.
framework-arduinoespressif32 3.20017.241212 = arduino-esp32 2.0.17, toolchains
xtensa-esp32 / xtensa-esp32s3 / riscv32-esp 8.4.0+2021r2-patch5):

```ini
; platformio.ini
[env:esp32]
platform = espressif32@7.0.1
board = esp32dev
framework = arduino

[env:esp32c3]
platform = espressif32@7.0.1
board = esp32-c3-devkitm-1
framework = arduino

[env:esp32s3]
platform = espressif32@7.0.1
board = esp32-s3-devkitc-1
framework = arduino
```

```sh
cp ledc_pwm.ino <project>/src/main.ino
pio run
xtensa-esp32-elf-strip   --strip-debug -o esp32/firmware.elf   .pio/build/esp32/firmware.elf
riscv32-esp-elf-strip    --strip-debug -o esp32c3/firmware.elf .pio/build/esp32c3/firmware.elf
xtensa-esp32s3-elf-strip --strip-debug -o esp32s3/firmware.elf .pio/build/esp32s3/firmware.elf
cp .pio/build/<chip>/partitions.bin <chip>/
```

sha256 of the committed files:

```
b6a968e0d1601d1290ca2825e29873d27097fd41accd4f76c98fc2aea752776e  ledc_pwm.ino
cfe4dd59681c24fd2230f4a69e567828f29cb1fbe4545dcbaccc482d394bf1a6  esp32/firmware.elf
148b959cbff1c38aa8e1d5c0ba9d612c54997b945e56a63f41223eef650653a1  esp32/partitions.bin
b41faf9d6dd9e1a109b18a3a1b8717dfbf16dfe0b927135a0667d534515432a2  esp32c3/firmware.elf
148b959cbff1c38aa8e1d5c0ba9d612c54997b945e56a63f41223eef650653a1  esp32c3/partitions.bin
73df80e61464cda3ed13c7437edad3137291be81b5bca29769236ea0b1394f55  esp32s3/firmware.elf
1d9cca96de0fe07ad7fc0648b9878ddecd9ce565e38b589ad20fea698ed4c80c  esp32s3/partitions.bin
```

Unstripped ELFs of that build: esp32
`f0fd526f86d08b6db4da0cc8ae0be209c0c95a523b0670aff29d6aabbf4ef814`, esp32c3
`9e379e808f353ce1b8365d07a9cd42d06150a300f363ae7f9310a1172927062b`, esp32s3
`ea2ce51ff13fa8876316067e1425e19a909540e70211950c84c842e60e63c74f`. An
Arduino build embeds its build date and time, so a rebuild differs in bytes and
must reproduce the measured duties, not the hashes.
