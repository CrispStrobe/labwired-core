# SAMD runtime replacement plan — 2026-10-08

Status: proposed R1 implementation boundary, not implemented or admitted.
Start with the [R0–R3 contracts](samd-permissive-runtime-lanes.md), then refresh
own-fork heads and open PRs. Do not repeat completed evidence collectors or
execute the historical chip-restricted images. Runtime source changes belong
in our own runtime forks; engine changes belong in LabWired. No upstream posts.

## Why not an ASF-compatible SPI shim?

The [original map](../receipts/2026-10-08-pybadge-link-map/README.md) retains
atomic, SPI, ADC, I2C and SERCOM handler/state contributions. Zero allocated
bytes from two named objects do not clear their headers or other builds.
The [dependency and notice evidence](../receipts/2026-10-08-pybadge-notices/README.md)
also needs header/inline and transitive-origin review. Deleting eight C files
while keeping their headers is not a justified replacement boundary.

Prefer a separately named simulation backend at the CODAL class boundary,
with narrow authored register operations and reviewed component origins.
Do not promise binary compatibility with ASF descriptor layouts. All consumers
of changed public headers must rebuild. Preserve CODAL attribution; this is
integration following source review, not whole-firmware clean-room work.

The eight original ASF compile inputs are `hal_atomic.c`, `hal_spi_m_sync.c`,
`hal_usart_async.c`, `hal_adc_sync.c`, `hal_i2c_m_sync.c`, `hal_io.c`,
`hpl_sercom.c` and `hpl_adc.c`. Their source selection and recursive header
exposure are explicit in the pinned [CMakeLists.txt](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/CMakeLists.txt).
The separate header-exposure review is recorded in
[PR184](https://github.com/CrispStrobe/labwired-core/pull/184#issuecomment-6065697868):
85 flagged headers reach 66 of 172 C/C++ units, including 58 non-ASF units.
These are dependency/comment flags, not licence classifications or proof of
retained inline bytes. Do not use the counts as an admission allowlist.

## Pinned caller observations

All sources below are at `codal-samd` commit
`5bd6b93c219c7e784e885ba2d6812809fb6289a8`. They were read as implementation
and interfaces for this plan; no isolated implementation process is claimed.

| Source | Constraint for the replacement |
| --- | --- |
| [ZSPI.h](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/inc/ZSPI.h) | Its public header includes ASF `hal_spi_m_sync.h` and embeds `spi_m_sync_descriptor`; replacing only linked functions leaves a header dependency. Keep the CODAL-facing API while replacing internal state. |
| [ZSPI.cpp](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/ZSPI.cpp) | Initialization uses HAL, HRI, pin/clock helpers and direct registers. Even `write()` enters `transfer()` and DMA. `transfer()` arms a fiber event and schedules; it is not a polling implementation. |
| [SAMDDMAC.h](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/inc/SAMDDMAC.h) | Exposes `sam.h`, `DmacDescriptor` references, eight software descriptors and callback types. Device headers and descriptor layout need their own origin/ABI review. Eight software slots do not limit the engine's hardware channel model. |
| [DmaInstance.cpp](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/DmaInstance.cpp) | Uses SAMD21/SAMD51 conditional register paths, end-address increments, beat-sized counts and writeback progress. Select SAMD51 explicitly; do not port the SAMD21 channel window by accident. |
| [DmaFactory.cpp](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/DmaFactory.cpp) | Aligns descriptor storage, programs BASEADDR/WRBADDR and enables five SAMD51 IRQs. Handler distinguishes transfer error from completion; preserve that distinction end to end. |

`ZSPI::dmaTransferComplete(DmaCode)` currently ignores its argument, waits for
TXC, drains RX, clears errors and invokes a callback or notification. This is
an observed risk to address in the new profile, not a qualified error behavior.
Do not implement DMA errors by calling the success path, or wait forever for
TXC after a failed transfer. Define error/cancellation reporting with the caller
before implementing it. The existing callback type has no status argument;
changing its semantics or adding state requires explicit consumer tests.

## Ordered implementation tasks

### R1a — bind the complete public API and build closure

Own the runtime build selector and public hardware-facing headers first.
Inventory every consumer of the eight inputs and exposed headers using the
already captured dependency graph; record symbols, types, calling conventions,
error codes and direct-register accesses. Review `ZPin`, serial, ADC/I2C,
timers, clocks/cache, startup/vector tables, CMSIS/device definitions, assembly,
linker scripts, generated settings and samd-peripherals independently.
Preserve exact component pins/notices and unresolved entries.

Deliver a machine-readable source/header/consumer manifest and a named
simulation-only build selection with no ASF include-directory fallback.
Compile every affected consumer from clean sources on hosted CI. A missing
header, unknown source or unsupported requested service must fail explicitly.
No stub may silently claim USB, QSPI, audio or sensor initialization succeeded.
Do not claim R1 complete from the five caller observations above.

### R1b — qualify startup, clocks, GPIO and polling SPI

Author minimal guests from reviewed permissive inputs and primary register
documentation. Prove reset/vector entry, initialized data/BSS, GPIO mux,
clock gating and real SPI DATA/TXC MMIO. This polling test backend must be
named separately from original ZSPI: it does not establish DMA/fiber behavior.

Require positive byte-order and transfer tests plus disabled MCLK/GCLK,
wrong mux, reset during transfer, invalid configuration and timeout controls.
Record exact source/toolchain/image/map hashes and actual engine results.
Retain the existing scheduler and feature-off comparisons. No vendor-header
fallback, host-forced completion or historical-image execution.

### R1c — bridge IRQ, descriptors and asynchronous CODAL semantics

Coordinate with [engine P2/P4](target-next-lanes.md#p2--sam-irq-routing-and-clock-semantics-after-p1).
Qualify per-vector mask/clear/reset behavior before DMA. Then prove descriptor
alignment, count/end-address rules, writeback and real SERCOM triggers through
authored guests; DMA completion alone is not final SPI TXC.

Test TX-only, RX-only, duplex, zero-length, size mismatch, buffer lifetime,
allocation exhaustion, abort, invalid descriptor, bus error and clock loss.
For admitted CODAL callers, prove exactly-once callback/event delivery, no
success on error, no notification from stale/reset transfers and correct fiber
wake-up. Define unsupported cases explicitly before claiming this API works.
Do not shrink the engine model to the runtime's eight software descriptors.

### R1d — close the remaining runtime services and admission

Replace or explicitly exclude atomic/critical-section, ADC, I2C, UART/IRQ and
other services found by R1a. An excluded service that the selected runtime
needs blocks whole-runtime qualification; exclusion is not successful emulation.
Rebuild all changed-header consumers, including assembly/startup review, and
reconcile allocated/discarded map sections with the full dependency manifest.

Only then implement [R3](samd-permissive-runtime-lanes.md#r3--qualify-admission-original-runtime-and-consumer-adoption)
for the new exact source/map/image, with changed-byte, unknown-object and
vendor-contribution rejection controls. Keep both historical image hashes
unchanged and rejected. Qualify actual loaded CF2/renderer selection, display,
inputs and debugger separately; active native/WASM timing and app pin adoption
follow successful execution, not notice capture or terminal-spin benchmarks.

## Toolchain evidence is separate

[PR185](https://github.com/CrispStrobe/labwired-core/pull/185#issuecomment-6066200720)
binds installed header-owner packages to exact source descriptors.
[PR186](https://github.com/CrispStrobe/labwired-core/pull/186#issuecomment-6066618120)
captures the three small packaging notice sets.
[PR187's first actual capture](https://github.com/CrispStrobe/labwired-core/pull/187#issuecomment-6067334329)
adds 12 GCC original-source notice candidates, including the root Runtime
Library Exception v3.1 and a distinct `gcc/m2` variant. At this plan's writing,
PR187 still awaits remaining enabled checks; do not infer it is merged.
Named-file presence is not header correspondence or exception applicability.
Distribution obligations and exact selected component review remain required.

No firmware admission, complete provenance, legal/store clearance, physical
module parity, active PyBadge RTx, browser result or app adoption is claimed.
