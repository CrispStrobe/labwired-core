# Original PyBadge link-map capture — 2026-10-08

[PR177](https://github.com/CrispStrobe/labwired-core/pull/177) source
`ce9de6d264366f9f5f9b7144a6186f31cb8c774c` passed all fifteen enabled checks;
four declared full/warm/image jobs skipped. Merge
`a37a755a3a514c17d5a389f2c6890112c64d9b35`, actual tested checkout
`2895f228919a8544343483b63822f997f44e5f92` and reviewed merge prediction share
tree `4d7bd69fc9d567bda48865c2b832682bd112228a`.

The [focused run37774892577/job113303158804](https://github.com/CrispStrobe/labwired-core/actions/runs/37774892577/job/113303158804)
reproduced the historical original worker request and ARM build, verified
effective `USE_RGB444=1` using separate original-source preprocessing, and
captured the original clean build's existing map. No diagnostic relink or
firmware execution/disassembly occurred. The original ELF was hashed and checked
unchanged, not uploaded; its identity remains a hosted observation, not an
independent ELF replay.

## Raw evidence identity

[Artifact11549798815](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11549798815/zip),
`pybadge-original-build-evidence`, ZIP 485,630 bytes, SHA256
`b6d75a531b077f3ca9fb4b69358129d401f5ead0dfa2a5fe04e9691e0d9b9730`.
Keep raw members unchanged; Actions retention may expire. This compact index
does not replace the raw map. The artifact contains no firmware binaries.

| Member | Bytes | SHA256 |
| --- | ---: | --- |
| `original-link.txt` | 3,240 | `8163427829709d67f4667c35c5ed7e8e8f31cdd701b96cca357246781c90d264` |
| `original-link.map` | 713,168 | `2322e427353ba623b3efadbccfcef1a2ee50c09322d8016dc05b1b782bdb1f64` |
| `original-linker-script.ld` | 4,110 | `611417cfb9164497e4e6bd069ce14050d20070f33a10c11f35ba13fda0b761ae` |

A separate standard-library ZIP/JSON/hash audit checked these members, all 52
captured configuration/notice file hashes and binary exclusion, without importing
source helpers. The earlier [run37759168804](https://github.com/CrispStrobe/labwired-core/actions/runs/37759168804)
failed admission because the original recipe already generated a map; no
diagnostic linker ran. Its raw recipe and failure remain preserved, not converted
into a passing map result. The revised capture uses that observed original path.

## Bounded retained-object inventory

Two separate read-only parsing approaches agreed on the following eight named
CMake inputs in `libcodal-samd.a`. One tracked output/input section state after
`Linker script and memory map`; the other scanned the explicit output ranges.
The second checked address bounds and non-overlap. Discarded/debug entries were
not counted as retained image bytes.

| Named input member | Bytes in `.text` output | Separate `.bss` bytes |
| --- | ---: | ---: |
| `hal_atomic.c.o` | 28 | 0 |
| `hal_spi_m_sync.c.o` | 108 | 0 |
| `hal_usart_async.c.o` | 0 | 0 |
| `hal_adc_sync.c.o` | 140 | 0 |
| `hal_i2c_m_sync.c.o` | 144 | 0 |
| `hal_io.c.o` | 0 | 0 |
| `hpl_sercom.c.o` | 2,116 | 24 |
| `hpl_adc.c.o` | 400 | 0 |
| Total for these named members | 2,936 | 24 |

The `.text` output includes 48 bytes of SERCOM `.rodata`; therefore 2,936 is
not a pure instruction-byte count. The 24 `.bss` bytes are zero-initialized RAM,
not bytes stored in the HEX. No contribution from these named members was found
in `.relocate`. Zero entries apply only to the audited output ranges, not to
headers/inline code, other objects, other builds or complete licence provenance.

## Next replacement boundary

Use the [R0–R3 admission contracts](../../engineering/samd-permissive-runtime-lanes.md).
The original-map acquisition part of R0 is reached; complete component, header,
inline and transitive script review before claiming R0 closed. R1 should scope
atomic/SPI, I2C, ADC and retained SERCOM interrupt/state contributions rather
than merely deleting eight source files or replacing the SPI wrapper alone.
Do not infer that the small byte count makes replacement ABI/behavior trivial.

Source access/provenance and preserved notices remain required; no whole-firmware
clean-room claim. The historical image remains chip-restricted in the consumer.
No new runtime admission, actual loaded CF2/constructor/renderer, module LUT,
guest boot, active PyBadge/WASM performance, app adoption or store clearance is
established here. Engine/source/runtime/app pins, ACKs and baselines did not move.
