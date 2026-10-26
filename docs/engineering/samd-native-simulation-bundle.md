# Explicit no-ASF SAMD51 simulation assembly

The existing PyBadge descriptor remains a historical peripheral subset. Its
stub USB and other blocks cannot stand in for the separately qualified no-ASF
native board. `configs/chips/samd51-no-asf-simulation.yaml` gives that software
assembly an explicit name and selects the existing qualified USB banks, TC/TCC,
DMA, NVMCTRL, ADC, UART, SPI and clock models. It starts at the authored reset
forwarder at address zero, without executing a UF2 bootloader.

The corresponding system selects the existing GPIO-clocked button model and a
registered ST7735R **controller-memory simulation** descriptor. That descriptor
keeps the foundation's 132×162 GM00 memory geometry, colour codec, reset and
unknown-pixel policy. Its explicitly authored 128×160 memory view starts at physical row 2 under
MADCTL=0x80. This software view does not identify a physical module or glass crop.
RGB444 remains unknown until the guest uploads a complete RGBSET table. The
owned runtime's explicitly selected RGB444 profile provides that upload; this
engine never seeds a presumed identity table.

Two additional named 1 KiB software memory windows expose separately authored
simulation inputs at `0x00800000` and `0x00806000`. Their explicit packets contain
the sixteen SW0 bytes at offset `0x80` and the 512 serial bytes at offset zero;
the remaining bytes are authored zero padding. These are software window sizes,
not assertions about physical calibration-page geometry.
Callers supply these under `LABWIRED_SAMD51_SIM_SW0` and
`LABWIRED_SAMD51_SIM_SERIAL`. They are synthetic software configuration, never
silicon captures. Omitted inputs retain the generic loader's zero-fill behavior;
that is not a qualified substitute for the admitted native input packet.

This source candidate registers and assembles existing models. Assembly tests
alone do not qualify native boot, active game performance, browser-WASM or
installed Lite adoption. Acceptance must bind the actual runtime/profile,
calibration, chip/system, engine and package bytes, exercise moving game frames
and press/release, retain the original faults, and pass every enabled exact-head
check before merge. Hardware timing, gamma, analogue colour and physical accuracy
remain outside the software assembly.
