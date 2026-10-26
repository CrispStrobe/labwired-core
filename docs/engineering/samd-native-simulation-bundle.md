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

The conformance estate probe uses the admitted control/config register for these
partial models: NVM CTRLA as 16 bits, DAC/USB CTRLA as 8 bits, and CMCC CFG at
`base+4` as 32 bits. Separate controls require reserved-crossing reads and the
unmodeled CMCC TYPE register to remain refused. Existing fleet probes, model
access rules, silicon comparisons and earlier conformance values are unchanged.
An L0 estate row grants no running-firmware or physical-accuracy coverage.

Current integration failures remain explicit: the authored profile's reset-spin
benchmark measured about 0.02× real time at 120MHz with one instruction per batch.
The 1× gate and all earlier baselines remain unchanged. The conservative legacy
walk prevents wide execution; any optimization must preserve active-model
service, MMIO arming boundaries and exact elapsed cycles, then pass differential
and affected native-guest qualification before new measurements are admitted.
The generic unselected SERCOM3 has the same unsupported wire-gate bring-up as
the original SAMD descriptors; the selected SAMD USART explicitly publishes
bytes without a PadLines wire. Byte acceptance does not grant UART edge coverage.
Bank-width controls must open the USB/DAC MCLK gates first: a clock-gated zero
read is not evidence that a reserved register was admitted by the model.

Provenance: the direct base is [CrispStrobe's PyBadge descriptor](https://github.com/CrispStrobe/labwired-core/blob/21ab8403c6b9d32d3e28152b4da7ad6b2e2908ce/configs/chips/atsamd51-pybadge.yaml).
CrispStrobe introduced that target in [8c894bd](https://github.com/CrispStrobe/labwired-core/commit/8c894bdbe591c74d135811d6cc33574f8b4755cf);
upstream later [imported CrispStrobe's work](https://github.com/w1ne/labwired-core/commit/94c57039d346a54bfb93c862e9ca834811b384ab).
The underlying [upstream SAMD51 foundation](https://github.com/w1ne/labwired-core/blob/fc463fcd17871d8ba9353999189aec112aee5f36/configs/chips/atsamd51.yaml)
carries Andrii Shylenko's notice, which is retained separately from the
CrispStrobe PyBadge adaptation and no-ASF assembly credit.

The performance candidate adds an explicit CPU capability for already checked
RAM/register-only chunks. It is admitted only for this reviewed memory/model
assembly with no attached devices, observers, capture, JIT, pending operation,
active legacy work or active bus service. Unknown CPUs return zero. A nonzero
prefix commits its actual elapsed cycles immediately; zero executes one original
strict instruction. MMIO and unsupported operations never follow an uncommitted
prefix. The configured tick/resident grid and the conservative public interval
recommendation remain unchanged. Active peripheral and full connected-game
execution remain on the strict path; this is not a claim of real-time games.
Differential controls compare CPU flags/registers/IRQs, complete SRAM, access
accounting, every model snapshot and exact clocks against one-instruction
execution, including odd budgets and transitions to gated, active, system and
storage accesses. Hosted source tests, the unchanged 1× gate and measured initial
Ir baselines must still pass before this candidate can qualify.

## Source-built CLI coverage

The scoped `samd-native-game-cli.yml` workflow builds the actual public game
from qualified Codal source using `scripts/ci/build_samd_native_game.py`. The
recipe runs only in owned hosted workflows, admits full source commits and
archive/payload hashes, retains selected source, dependency and archive-member
identities and component notices, and gives its newly compiled image a fresh
manifest. The earlier qualified game campaign is a reference, not a relabeling
of the new compiler output. No guest binary is committed here.

The real CLI executes `validation/samd-native-profile/native-game.yaml` in
both scheduler profiles with explicit authored calibration files. The two
assertions require the actual120×160 game area to be completely painted and
the8-pixel spare strip to stay black. These bounds match all51 retained
native snapshots from the qualified reference; they reject both an untouched
black display and a full-panel fill. They establish paint/layout coverage,
while the stronger source-bound game campaign retains exact colours, movement
and press/release requirements. The new CLI workflow remains unqualified until
its official source-built runs and independent original audits pass.
