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

Current candidate: the register/bank implementation and strict opt-in factory
are present, with eight pure model controls. Preliminary local controls execute
the real module through a small mock Bus/API harness; the full engine API and
JSON snapshot remain unqualified there. Eight source mutation cuts detect table
alignment/RAM admission, buffer RAM admission, missing completion/IRQ, incorrect
packet progress, bypassed readiness and bypassed clock/pin admission. A mapped
MMIO test bank deliberately accepts ordinary reads/stores so a downstream bus
error cannot hide a missing DMA admission check.

The modeled host IN operation includes successful host acknowledgement. Ordinary
full-size IN completion without AUTO_ZLP follows section38.6.2.9; applying its
final-ACK rule to an exact multiple in a multi-packet transfer is an inference
combined with section38.6.2.10, consistent with the actual driver's manual-ZLP
sequence. AUTO_ZLP remains explicitly unsupported in this slice. Do not claim
wire-level or arbitrary multi-packet conformance from the preliminary controls.
The actual source-built driver guest, full engine integration and both scheduler
qualifications are still required. This candidate must remain unmerged until
those pass and independently audited originals bind the tested source and image.
