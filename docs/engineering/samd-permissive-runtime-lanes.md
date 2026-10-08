# SAMD simulation-runtime admission lanes — 2026-10-08

This is a prerequisite for original native PyBadge runtime execution and app
adoption, not a prerequisite for authored permissive MMIO guest tests. Keep
[P3 panel binding, P2 IRQ and P4 DMA](target-next-lanes.md) separate. Refresh
default branches, open PRs and consumer lane ownership before implementation.
Do not change the consumer's admission policy to make a historical image run.

## Observed boundary

The consumer's [classification at source 0441d5b](https://github.com/CrispStrobe/brickwright-lite/blob/0441d5b6247e3b3c6a41fac68fba36f0c7387927/overlay/scratch-gui/src/lib/bw-makecode/base-licences.js)
marks both PyBadge base images `chip-restricted`, naming ASF4:

| Build | HEX SHA256 |
| --- | --- |
| CDN | `842c30c5fc1db2346a949837c2e21acdb57937aa0a82f4d505725e995ff02b97` |
| Historical from-source | `9c2310bd5a65f0543c69a076e51a4067c803202a39228f3a7de41451be0da9ca` |

This records the existing registry/policy, not new legal clearance. A reproduced
build or passing preprocessor probe does not make either image emulator-admitted.
[PR174](https://github.com/CrispStrobe/labwired-core/pull/174)'s focused
[run37745531829](https://github.com/CrispStrobe/labwired-core/actions/runs/37745531829)
reproduced the historical build; it did not execute or upload the firmware.
PR174 and PR175 are merged. The qualified separate ARM preprocessing observed
`USE_RGB444=1`; the [original-map receipt](../receipts/2026-10-08-pybadge-link-map/README.md)
records its exact source/configuration boundary. Loaded CF2/constructor selection
remains separate.

## R0 — capture actual linked provenance

**Reached:** PR177 captured and independently audited the original map; see the
[source-bound receipt and six-member retained inventory](../receipts/2026-10-08-pybadge-link-map/README.md).
Do not repeat acquisition to obtain an already retained map. Remaining R0 work
includes complete component/header/inline/transitive-script provenance, not
reimplementing the collector. The zero-count members are not clearance claims.

**Dependency follow-up:** the [source-bound dependency receipt](../receipts/2026-10-08-pybadge-dependencies/README.md)
preserves two collector failures, the four surviving shared original rules and
the successful separate `-M` probes of all 172 observed C/C++ commands. The
diagnostic inventory has 542 unique dependencies; one assembly command remains
explicitly excluded. Do not mistake this for recovered original invocations,
retained-inline attribution, merged source, or completed component review.

Inputs: exact original build request, immutable dependency commits, compiler
identity and the clean hosted build. Retain the actual link command, linker map,
linker scripts, generated configuration, binary hash and dependency notices.
The first build-evidence artifact lacked a linker map; PR177 now retains the
original clean build's existing map without relinking. Do not manufacture maps
from source lists. If a future diagnostic relink is required, name it separately and
verify the output identity rather than calling it the original invocation.

The pinned [codal-samd build list](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/CMakeLists.txt)
selects `asf4/samd51` and lists eight ASF4 implementation inputs:
`hal_atomic.c`, `hal_spi_m_sync.c`, `hal_usart_async.c`, `hal_adc_sync.c`,
`hal_i2c_m_sync.c`, `hal_io.c`, `hpl_sercom.c`, `hpl_adc.c`.
These are compile inputs, **not** an inventory of retained image bytes.
Headers, inline code, startup, scripts, tables and transitive sources require
separate review even when a map contains no matching object name.

Files: extend the focused hosted evidence collector and add a source-bound
receipt, without running/disassembling the restricted whole firmware.

Acceptance: independently audited allocated input sections/object members,
discarded sections distinguished from retained ones, an exact source/component
and notice inventory, explicit unknowns and reproducible artifact hashes.
Unknown members fail admission; a filename allowlist alone is insufficient.

## R1 — choose the smallest justified permissive replacement boundary

Inputs: R0, exact API/ABI and register contracts. The pinned
[ZSPI](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/ZSPI.cpp)
uses `spi_m_sync_init/enable/disable/set_baudrate`, a SERCOM trigger seam and
DMA completion. The pinned
[DmaInstance](https://github.com/lancaster-university/codal-samd/blob/5bd6b93c219c7e784e885ba2d6812809fb6289a8/src/DmaInstance.cpp)
depends on `SAMDDMAC`. Those observations are clues, not a complete call graph.
Inventory clocks/cache, GPIO, startup/CMSIS, interrupts, timers, UART, ADC/I2C,
USB and storage contributions before selecting replacement slices.

Work only in our own runtime forks. Prefer exact reviewed permissive components
or attributed independent implementations with recorded origins. Review each
component/build and preserve notices/obligations; do not assume every header in
a vendor directory shares one licence, relabel copied code, or claim whole-tree
clean-room origin. MPL/LGPL components need their applicable obligations retained.

Acceptance: a reviewed source/build plan with immutable inputs, API scope,
excluded modes/devices, affected files, origin records and independently authored
positive/negative guests. The target is a simulation-only runtime, not firmware
approved for flashing or a silent substitution for the historical build.

## R2 — implement and qualify incremental driver slices

Land separately: startup/clock/GPIO foundations, polling SPI, then IRQ and DMA
completion, then other required services identified by R0/R1. Coordinate with
engine P2/P4 rather than fabricating hardware completion in the runtime.
Unsupported services must reject explicitly, not return invented success.

Acceptance per slice: clean hosted source build, exact image/map/source hashes,
authored permissive guest through real MMIO, meaningful disabled-clock/mux/
masked-interrupt/reset/error controls, all enabled final-head checks and retained
attribution. Add a separately named image/request profile; never overwrite the
historical restricted image's hash or claim binary equivalence after replacement.

## R3 — qualify admission, original runtime and consumer adoption

The consumer's current [firmware gate](https://github.com/CrispStrobe/brickwright-lite/blob/0441d5b6247e3b3c6a41fac68fba36f0c7387927/scripts/makecode/firmware-licence-gate.mjs)
is Nordic-oriented. Extend an explicitly SAMD-aware policy only after reviewed
component evidence exists. Tests must reject retained ASF4/vendor-restricted
contributions, unknown objects and changed/unbound image bytes. Passing a Nordic
gate does not certify this SAMD build. Preserve rejection of historical images.

Acceptance: an exact new source/map/binary-bound admission receipt and all
obligations/notices for distributed components; then original-runtime boot,
effective loaded CF2/path selection, frames, input and debugger evidence through
the qualified engine. Only afterward measure active native/WASM RTx and qualify
an explicit app artifact/pin. Do not infer store approval, physical safety,
module colour parity or complete-board support from source or map checks.

Until R3 passes, the existing PXT/translated Arcade route and authored permissive
guests remain distinct usable paths; neither is proof of native-runtime support.
