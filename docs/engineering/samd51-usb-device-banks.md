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

Initial candidate controls: the register/bank implementation and strict opt-in factory
are present, with ten pure model controls. Preliminary local controls execute
the real module through a small mock Bus/API harness; the full engine API and
JSON snapshot remain unqualified there. Ten source mutation cuts detect table
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
Those preliminary controls alone did not qualify the full engine. The later
source-built actual driver qualification below supplies its separate finite
engine/guest boundary; it does not broaden multi-packet or physical conformance.

SETUP reception follows section38.6.2.6: it ignores BK0RDY while RXSTP is clear,
overwrites the next setup packet, clears both stall requests and BK1RDY, and
sets DTGLOUT/DTGLIN/CURBK/BK0RDY. It preserves an existing TRCPT0 flag. An unread
RXSTP still owns its buffer. A separate regression covers a preceding unread
status OUT, stall recovery, and protection of an unread setup packet; its
bank-readiness mutation is detected. This behavior is required for successive
real control requests and is separate from ordinary OUT bank readiness.

The first full-source candidate retained a deterministic nRF52832 step-mode
result of 1320.4 host instructions per step against the unchanged 1279.2
baseline, exceeding the existing 3% limit. It is not merge qualification. A
bounded hosted parent/candidate diagnostic retains both binaries and the
original low/high callgrind attribution using one shared frozen fixture. This
diagnostic changes no baseline, threshold or production performance gate.
The generated ESP32-C6 validation digest follows the shared factory change;
silicon ACKs, captures and watch lists remain unchanged.

The retained same-toolchain comparison measured current main at 1311.406307
and the USB candidate at 1320.406252 host instructions per nRF52832 step.
Independent original readers account for the approximately nine added
instructions entirely in unchanged peripheral tick orchestration. No USB method
is selected by this board; changed generated code is observed, while the exact
optimizer decision remains unestablished. The construction-only isolation
candidate outlines validation, allocation and trait-object conversion into one
cold helper. It changes no transaction behavior or shared tick/GPIO source.
Its performance and affected guest qualification remain required. The frozen
diagnostic is admitted only at its original head, avoiding automatic replay of
those successful original measurements on later source revisions.

An enabled controller remains in the bus-aware polling set after its host queue
drains. This admits later external host submissions through the normal cached
bus pass. Empty polls emit no reply, flag or transaction. A specific control
checks an empty poll followed by a real queued reset and renewed emptiness,
then controller disable. The actual driver guest releases enumeration only
after the real reset handler returns, exercising delayed submission naturally.
This fixes the cached-eligibility lifetime; it does not add physical wire timing.

## Qualified finite source and remaining boundaries — 2026-10-24

Core PR256 and paired Codal PR82 qualified actual source-built core/SAMD USB
execution in both scheduler profiles. Frozen standalone readers bind the actual
source/header/dependency/library members, six distinct ELF images, official
checkouts, host input plans versus actual queue arrival, and complete response
bytes. Bulk/interrupt echo cases return the actual device/configuration/BOS/
MS OS descriptors, preserve the unsupported landing-page stall and recovery,
and compare all 64 echo bytes. Bulk sends its real manual ZLP; interrupt mode
suppresses it. Actual constructor counts are `[1, 2, 2]` and IRQ80/82 counts
are `[9, 1]`. Missing NVIC80 enable, missing IN-bank arm, misaligned descriptor
address and hidden WebUSB interface require specific observed refusals/stalls.
A generic timeout or unrelated architectural failure is never a positive result.

The source-built leaf preserves USB1/WebUSB1, the generated 120 MHz/192 KiB/
10 KiB configuration and relocated vectors at `0x4000`, with an explicitly
declared O0 ARM leaf build. Its reset-first host plan releases enumeration only
after the real reset handler returns with initialized control buffers. All
12 case/profile originals passed independent audit. Earlier observer startup,
ABI-name selection, caller-status and wait-loop failures remain retained under
their original source identities; their fixes do not change protocol bytes or
turn a timeout into success. The missing-IRQ control requires 128 actual WFI
entries and its blocked-state invariants at every intervening instruction.

Cold construction alone did not restore the unchanged nRF52832 performance
gate. Current-source instruction originals reconcile tick-phase self cost and
show repeated stack copies of the cached GPIO-port array. The final source
borrows that cache during its read-only sampling loop, preserving cache-miss
resolution, baseline adoption, port order and edge delivery. The unchanged full
42-chip/82-mode gate and exact-head functional/guest checks qualify the source.
The final nRF52832 step result is 1298.4 host instructions per step, compared
with the retained failing 1320.4 and unchanged 1279.2 baseline, passing the
existing 3% limit. The existing RV1103 nonmeasurement reason remains unchanged;
no new waiver is added. This control workload is not active USB or native-PXT
performance evidence.

The actual driver exercises general IRQ80 and OUT IRQ82. SOF generation and
natural IN IRQ83 delivery are not qualified by this leaf. Its authored fuse and
clock/pin inputs are nominal test inputs, not silicon captures or a demonstrated
48 MHz USB clock/tolerance model. This is a diagnostic echo interface, not HF2
or native Arcade. Repeated reset/re-enumeration, endpoint heap reuse, complete
peripheral reset, physical signaling/timing and installed adoption remain open.
The [remaining consumer lanes](https://github.com/CrispStrobe/codal-samd/blob/master/docs/PXT-NO-ASF-NEXT-LANES.md)
include actual CF2/PXT/HF2 integration and the source findings in endpoint
initialization and the core descriptor buffer's matching deallocator.

## Retained diagnostic maintenance

The frozen one-run `samd-usb-codegen-diagnostic.yml` and
`scripts/perf/samd_usb_codegen_diagnostic.py` retain the original parent/current
attribution source. Their literal admitted-head condition prevents replay on
later heads. Retiring these inactive producers is separate workflow maintenance:
first preserve the exact original source identities and audited originals, check
all references, then remove only the obsolete producer paths and update their
documentation. Honor the unchanged source/performance path policy and every
check it selects; do not rerun the successful original diagnostic or alter a
baseline to simplify cleanup. The later instruction-position diagnostic stayed
on its separate temporary PR rather than changing the engine source.
