# Opt-in SAMD51 USB device banks

This lane owns `crates/core/src/peripherals/sam/usb_device.rs`, its module export,
strict `samd51_usb_device_banks` factory registration, a separate opt-in fixture,
affected hosted guest controls and this contract. The default chip descriptor's
USB stub remains a distinct unqualified profile. No existing consumer pin moves
until the new source/model and affected actual guest evidence qualify together.

Independently implement factual register/descriptor behavior from Microchip
DS60001507F sections 10.2.2 and 38.6–38.8. Preserve two 16-byte SRAM descriptor
banks per endpoint, 32-byte endpoint stride, and the documented **four-byte**
alignment of descriptor tables and data buffers. Admit RAM-only DMA extents
through the Bus API before any data access. Never import ASF or vendor headers.

Separate controller state from a bounded typed external host transaction queue.
Host reset, setup, OUT and IN tokens must drive actual guest SRAM banks and
readiness/completion flags. Never synthesize USB descriptors, WebUSB replies or
HF2 responses. Refuse unsupported modes, endpoints, buffer extents and descriptor
fields explicitly. No absent-host transaction may produce transfer completion.

Implement the selected reset/enable/sync, device address, descriptor address,
calibration register, endpoint configuration/readiness, W1C interrupt flags and
W1S/W1C enable/status semantics. Route the documented IRQs separately:80 general,
81 SOF,82 OUT completion,83 IN completion. Clocks/pinmux admission and scheduler
wakes must remain observable; merely storing register writes is insufficient.

Before merge, freeze independent readers and qualify a clean source-built actual
SAMD USB driver/core guest in both engine scheduler profiles. Require real SRAM
DMA and NVIC delivery, actual enumeration bytes and source/image/host-event
joins. Missing IRQ/completion, malformed alignment/geometry, wrong DMA buffers,
disabled endpoints and corrupted descriptors must fail nonvacuously. Acquire
each official original once and retain failures. Host model tests alone do not
qualify this source change.

This bounded device-bank model does not claim physical USB signaling, packet
timing, isochronous/host modes, arbitrary class support, native Arcade startup,
complete peripheral reset or full-board behavior. The generated-board consumer
adds its own actual configuration, no-ASF source/header graph, real program,
CF2, WebUSB/HF2 and relocated startup requirements.
