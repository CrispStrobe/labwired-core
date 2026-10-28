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

An [earlier throughput campaign](https://github.com/CrispStrobe/labwired-core/actions/runs/40375660666)
measured about 0.02× real time at the authored 120 MHz clock with one instruction
per batch and failed the unchanged 1× gate. Preserve that failure alongside the
later checked-execution controls and measurements below. All earlier baselines
remain unchanged. Generic SERCOM3 byte-stream controls open its MCLK and GCLK
gates; the selected SAMD USART also publishes bytes without a PadLines wire.
Byte acceptance does not grant UART edge coverage.
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
assembly with no attached devices, observers, capture, compiled JIT backend,
pending operation, active legacy work or active bus service. Unknown CPUs return zero. A nonzero
prefix commits its actual elapsed cycles immediately; zero executes one original
strict instruction. MMIO and unsupported operations never follow an uncommitted
prefix. The configured tick/resident grid and the conservative public interval
recommendation remain unchanged. Active peripheral and full connected-game
execution remain on the strict path; this is not a claim of real-time games.
Differential controls compare CPU flags/registers/IRQs, complete SRAM, access
accounting, every model snapshot and exact clocks against one-instruction
execution, including odd budgets and transitions to gated, active, system and
storage accesses. Admission also reads enabled pending bits directly from the
shared NVIC: a host-pended interrupt must reach the ordinary CPU import/dispatch
before any cached chunk. The WFI controls exercise both configured idle sleep
and the existing default spin behavior. WFI/WFE idle admission also checks the
shared controller before skipping its import boundary, retaining their existing
PRIMASK distinction and BASEPRI/FAULTMASK/active-priority rules. An eight-case
sleep/mask control requires both masked sleep and wake after clearing the mask.
For this assembly, idle skipping reuses the all-inactive admission predicate:
inert clock banks do not veto a skip, while pending operations, bus service,
residents and observers still do. Other assemblies keep their existing idle
admission policy. With idle fast-forward enabled, refused skips leave WFI
parked while one-cycle bus service continues; the default disabled mode keeps
its existing spin behaviour. Real DPLL lock-wait, gated I2C, enabled USB and
DMA waiting for TC3 controls require this distinction without inventing an IRQ.
The real generic UART stream control
opens both the MCLK mask and SERCOM3's GCLK channel before checking TX/RX bytes.
The [selected hosted campaign](https://github.com/CrispStrobe/labwired-core/actions/runs/40432630606)
passed all seventeen architectural controls and both assembly controls on
source `76b0b7b1ab8f7b8642c831ccafadf6ef833e199d`, with no skips.
All enabled checks on the final baseline/documentation head must still pass
before landing.
The selected measurement workflow retains each original low/high Callgrind
output, stdout/stderr, exact argument vector, step-mode selection and hashed
CLI/fixture/chip bytes. The packet roster lets an independent reader reconstruct
the step slope and three-sample batch median without rerunning a guest or
importing the measurement implementation. A failed measurement process cannot
publish a throughput number. Existing threshold and baseline gates are unchanged.

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
and press/release requirements.

The [source-built CLI run](https://github.com/CrispStrobe/labwired-core/actions/runs/40432611281)
passed both profiles at [candidate source](https://github.com/CrispStrobe/labwired-core/commit/76b0b7b1ab8f7b8642c831ccafadf6ef833e199d).
Its tested merge `fca4e434ecc35f25c9128a63455eee91d9acb5b8` has the same
tree. The retained packet binds the fresh build to pinned sources, recorded
compiler inputs and both 70-million-step results. The selected architectural
and reset-inactive throughput campaign is recorded below; production WASM and
Lite acceptance remain separate gates.

## Performance floor and strict service

The existing 1024-instruction dispatcher floor and 1× real-time requirement
remain. Existing boards retain their original recommendation check. This
software profile keeps its peripheral recommendation and resident service grid
at one; the interpreter may separately retire a checked memory-only CPU chunk.
The CLI reports the largest chunk actually retired, independently of its
configured horizon, with an explicit `service_grid=1` witness. It never reports
a widened peripheral interval for this path.

For this execution mode the performance reader requires an actual retired
chunk of at least1024 in every low and high run of every repeat. Missing,
duplicate, wrong-grid or impossible witnesses fail. The retirement counter stays
zero for refused or merely planned chunks and resets with the step profile.
The original real-time threshold, Ir regression budget, all earlier baseline
values, and existing-board checks remain unchanged. This distinguishes CPU
dispatch from peripheral service without adding a board waiver or claiming
active-game real-time performance.

## Initial reset-inactive measurements — 2026-10-27

The [original measurement packet](https://github.com/CrispStrobe/labwired-core/actions/runs/40432630606/artifacts/12704442724)
binds the selected source, compiler and Valgrind identities, actual CLI,
descriptor and fixture bytes, all eight low/high invocations, and their raw
Callgrind output. Independent reading of those retained bytes reconstructed:

| Mode | Observed Ir per simulated instruction | Initial baseline |
| --- | ---: | ---: |
| Step | 1625.199951 | 1625.2 |
| Batch | 9.129052 median of three repeats | 9.1 |

The batch slopes were 9.129052, 9.128690 and 9.129068. Every low/high batch
invocation reported an actually retired chunk of 1024 with `service_grid=1`.
Only this new board's two baseline values are added; all earlier entries,
the 3% regression budget, absolute noise allowance and loop-distinction check
retain their existing values.

CI's three 120-million-step real-time windows at the authored 120 MHz clock
reported 4.3633–4.4128×, median 4.4017×. This passes the unchanged 1× gate for
the reset-inactive fixture. Active peripherals and the connected game retain
their strict execution paths; these numbers do not qualify game frame rate,
input latency, browser/installed execution or physical timing.

## Supplier and consumer follow-up

The [engine producer change](https://github.com/CrispStrobe/bw-board/pull/711)
must carry the qualified Core source through two matching builds, the legacy
suite, selected motion windows and the actual source-built Node game before
immutable publication. The
[Lite qualification infrastructure](https://github.com/CrispStrobe/brickwright-lite/pull/1456)
registers the dispatch-only acceptance path. After its checks and landing,
advance the exact engine/profile pins and bind official frontend/native inputs
to actual Chromium and installed Tauri/WebKit game observations. Keep the normal
native compile/import/GUI route and physical captures as separately scoped work.

Shared-runtime review also leaves the existing silicon re-capture task open for
`nrf52840`, `seeed-xiao-nrf52840-sense`, `stm32h563`, `nucleo-l476rg`,
`nucleo-l073rz`, `stm32f103` and `stm32f407`. Their existing ACK expiry remains
2026-11-12. A separately authorized hardware session must follow each board's
manifest suites, retain raw comparisons and update capture/digest evidence only
from that new capture. Simulated guest success does not complete this task.

The [full-fleet gate](https://github.com/CrispStrobe/labwired-core/actions/runs/40443757411)
retained a step-mode instruction-cost regression on existing boards. A later
[const-specialized candidate](https://github.com/CrispStrobe/labwired-core/actions/runs/40451364784)
also failed the unchanged gate; do not treat its passing selected-profile
measurements as fleet recovery. Retained raw Callgrind samples show changed
compiler call boundaries and extra instructions, rather than a wall-clock
outlier.

The next candidate restores the qualified main's CPU-window and progress
structures, keeps one authoritative advance loop and holds checked-window
markers privately on the machine. Eligibility is cached once per public
advance; the full dynamic admission and checked executor are separate helpers.
External runners retain their original service grid. Planning overwrites each
window's markers, execution consumes admission and marks only a positive
retired prefix, and commit consumes that marker once. Ordinary execution keeps
its existing return structures and strict execution call site.

That candidate (exact-head fleet run 40459382058) still reported about 4% extra
step cost on existing ARM and Xtensa boards. The follow-up keeps every marker
store on the admitted path only: each public advance and external-runner entry
clears the markers, the admitted planner clears them before planning, and the
ordinary executor, planner and commit each read one marker without a store. The
executor wrapper is no longer forced inline and the checked helpers are cold.
This is an unmeasured hypothesis until the unchanged fleet gate passes.

That follow-up (exact-head fleet run 40471913258) did not recover either. The
current candidate therefore separates the loop by type, not by per-window flags.
The public advance reads the construction-time SAMD bus flag once and enters
`advance_inner::<Ordinary>` or `advance_inner::<CheckedChunks>`. In the ordinary
monomorphisation the planner, executor and commit contain no checked-chunk hook.
The checked loop widens only a multi-step window the one-cycle grid would cut,
applies the resident-edge and scheduler deadlines on the cold admitted path,
executes a planned chunk inside the batch arm and commits its retired prefix
through the existing coalesced-tick branch. External window runners use the
checked loop with eligibility forced off. A Cortex-M awake step tests `sleeping`
once, as before. An offline diagnostic Callgrind build of the same fleet
harness measured this candidate at or below the main baseline on STM32F407,
ATSAMD21 and RP2040 step mode. That diagnostic is not qualification: the
unchanged hosted fleet gate, controls, mutation and guests still decide.

All twenty controls, the external-runner mutation, fresh actual guest checks and
full-fleet/selected measurements must pass on the final runtime before landing.
Old baselines, noise limits and the real-time floor remain unchanged. This
candidate has no qualified performance result yet.
