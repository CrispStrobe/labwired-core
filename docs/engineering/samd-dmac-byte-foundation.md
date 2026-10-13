# SAMD51 DMAC byte foundation

Status: bounded software-triggered byte foundation landed in
[PR210](https://github.com/CrispStrobe/labwired-core/pull/210), after the qualified
[split-IRQ checkpoint](../receipts/2026-10-12-samd-sercom-split-irq/README.md).
Both final-source guest profiles passed90 cases and36 interrupt entries/returns
each; all22 enabled checks passed. Read the
[source-bound receipt](../receipts/2026-10-12-samd-dmac-byte/README.md).
Live peripheral DMA, production clock binding and native Arcade remain unqualified.

## Selected first boundary

The authored MIT, explicitly opt-in `sam_dmac_byte` model uses primary register
facts; no ASF4 implementation or vendor-restricted header was copied/imported.
Do not change the production PyBadge descriptor, application pins or captures.
Reserve `sam_dmac` with an explicit not-admitted refusal; it must not silently
fall through the generic name heuristic to an unrelated STM32 DMA register map.

Primary input: Microchip
[DS60001507F](https://ww1.microchip.com/downloads/en/DeviceDoc/SAM_D5xE5x_Family_Data_Sheet_DS60001507F.pdf),
sections22.6.2.3/22.6.2.7/22.6.2.9/22.7/22.8/22.10. For SAMD51, channel
windows begin at0x40 with0x10 stride; BASEADDR/WRBADDR are0x34/0x38 and
16-byte aligned. Descriptors are16 bytes with BTCTRL/BTCNT/SRCADDR/DSTADDR/
DESCADDR. Channels0..3 use IRQ31..34; channels4..31 share IRQ35.

The first slice permits software-triggered BLOCK action, single-byte beats,
single unlinked descriptors, fixed or unit-increment addresses, and NOACT/INT
block actions. Fetch descriptors through the real bus. Observe each byte's
read/write result before decrementing the count. On transfer bus error, retain
the failed beat's count, write back progress, disable the channel and expose
TERR without newly raising TCMPL. Clear VALID in final/error writeback. Completion interrupt
depends on the selected block action and enabled interrupt, not host injection.

Mapped descriptor, data and writeback targets are limited to primary SystemBus
RAM. Flash, extra mappings, MMIO (including self/stub/clock-gated windows) and
nonsecure peripheral aliases explicitly refuse before the affected beat;
unmapped targets still exercise actual bus faults. Check all16 bytes of each
table extent before data transfer. A generic bus `Ok` from a dropped MMIO store
cannot establish RAM writeback or DMA completion. Peripheral DMA admission is
a separate follow-on, not implied by this profile.

Require actual authored CPU guests to create their own descriptors, program
DMAC/NVIC, transfer memory, acknowledge the correct handler and return. Test
all five IRQ routes and channel31; masks, no trigger, disabled controller/channel,
reset, read/write bus failures and no success after a failed beat. Compare both
feature configurations and preserve existing SPI/scheduler regressions.

`crates/core/tests/sam_dmac_byte.rs` declares90 authored guest cases per feature
configuration. Reset must interrupt actual partial progress, not just precede
all work. The second-beat read/write-error controls require63 beats remaining;
the failed beat must not decrement. A separate failed-writeback control cannot
produce TCMPL. Handler-written flag/vector/count witnesses and architectural
entry/return are checked, separately from host-MMIO/refusal/shared-vector tests.
The model is bus-assisted and not event-migrated or performance-qualified.

The reset guest disables the controller before SWRST. Enabled global reset
returns an access error, enabled channel reset is ignored, and LVLEN fields are
enable-protected. Separate host controls assert those boundaries and refusal
of excluded CRC control rather than silently accepting it.

Unsupported descriptor modes latch an explicit simulator refusal exposed by
subsequent register reads and inspection; no guessed hardware flags are used.
Writes that request unsupported trigger/command/priority/event modes refuse
immediately. External data/writeback side effects already performed before a
bus failure are not rolled back or claimed atomic. Wider transfers, physical
arbitration, CPU batch quanta and full production clock behavior are excluded.

Unsupported requests must fail explicitly, not silently degrade into byte
transfers or completion. The first slice excludes peripheral triggers, linked
descriptors, halfword/word beats, flexible steps, suspend/resume, CRC/events,
hardware arbitration/timing and production clock binding.

## Primary ambiguity retained

Section22.6.2.9 describes invalid VALID as SUSP/FERR; section22.8.22 also says
an invalid fetched descriptor sets TERR. Section22.10.1 describes suspension.
Do not invent a resolution or use invalid VALID as proof of one interrupt
classification. This first slice must refuse that case before a data beat or
success. Resolve the primary/errata boundary separately before advertising it.

The printed addressing formula uses BEATSIZE+1, while the field encoding and
width description need care for wider beats. The byte-only slice has an
unambiguous unit increment; wider beat/address math remains a separate review.

## Follow-on, not reached

### Next bounded lane — live SERCOM4 TX byte requests

The bus acceptance prerequisite landed in
[PR214](https://github.com/CrispStrobe/labwired-core/pull/214), after both
final-source profiles and all22 enabled checks passed. Read the
[bounded bus receipt](../receipts/2026-10-13-samd-spi-bus/README.md).
It shares byte-MMIO bookkeeping, denies unsupported/shadowed/aliased routes,
honors live clocks, and distinguishes accepted from not-ready. These are host
bus controls with CPU time advancement, not a CPU-programmed peripheral DMA
guest. No hardware-triggered profile or production descriptor is admitted yet.

**Qualified and landed:** an explicitly opt-in SAMD51 SERCOM4 TX byte
profile, using the landed bus acceptance seam. Preserve the software-only
profile and its90 guest cases. CPU-authored descriptors must exercise actual
hardware-request polling, count/writeback progress, clock stalls, reset and IRQ
return without host-injected requests or completion. See the
[original qualification and concurrent landing drift](../receipts/2026-10-13-samd-sercom4-tx/README.md).
PR216's concurrent landing drift is preserved; PR217's fresh both-profile
qualification and successful exact-tree landed audit close that gap.
Final wire delivery/TXC,
CS release and callback/fiber integration remain separately observed boundaries;
no native Arcade or performance result follows from this claim.

The controller-local endpoint prerequisite landed in
[PR212](https://github.com/CrispStrobe/labwired-core/pull/212):
`Peripheral::dma_tx_request` and `try_dma_write_tx_u8` expose readiness and
explicit acceptance, with default refusal for other peripherals. Both profile
host controls and existing SPI/byte-DMA guests passed; all22 enabled checks
succeeded. Read the [bounded receipt](../receipts/2026-10-13-samd-spi-endpoint/README.md).
These methods are not connected to Bus/DMAC. Bus clock/alias/observer/scheduler
integration remains a separate admission step, not a reached DMA capability.

**Files:** `crates/core/src/peripherals/sam/dmac_byte.rs`,
`crates/core/src/peripherals/sam/sercom_spi.rs`, `crates/core/src/lib.rs`,
`crates/core/src/bus/accessors.rs`, bus scheduling/IRQ reconciliation, and a
separate authored integration guest. Preserve the software-only profile and its
refusal tests; add explicitly selected peripheral admission, not arbitrary MMIO.

Primary facts: DS60001507F section22.8.16 assigns SERCOM4 TX trigger0x0d,
RX0x0c and BURST action2; section35.6.4.1 makes TX requests depend on an empty
TX DATA buffer, independently of CPU interrupt enable. The selected board's
SERCOM4 DATA is0x43000028. DMAC's MCLK AHBMASK gate is bit9; SERCOM4's current
APBDMASK bit0 and GCLK channel34 are separate gates. Qualify the selected clock
bindings without claiming nominal SPI delay is a derived CPU/kernel frequency.

Integrate the landed readiness/accepted-beat capability through the bus seam. Existing SPI
register stores return `Ok` even when an occupied/disabled controller ignores
DATA: bus success alone must not decrement BTCNT or establish completion.
Not-ready, accepted and actual transfer faults must remain distinct. Retain
descriptor/writeback RAM checks and scheduler wake harvesting after accepted
DATA writes. No host-injected request or IRQ may substitute for guest execution.

**Next bus prerequisite:** reuse the landed controller hooks through a
default-refusing bus operation. Share the existing byte-MMIO write choke's
clock checks, non-consuming observer sample, scheduler synchronization/wakes,
IRQ reconciliation and dynamic pump-index updates. Preserve ordinary CPU store
behavior. Reject aliases, memory-shadowed routes and unsupported endpoints
before transfer; do not use a raw peripheral call or concrete-type downcast to
bypass those guards. No observer write or DMA progress may be reported for a
not-ready result. Qualify accepted/rejected writes, paused clocks, reset,
observer notifications and both feature profiles before wiring hardware DMA.

**Done when:** a CPU-authored descriptor feeds an owned SPI device exact ordered
bytes, stalls without count/writeback progress while not ready, and resumes
after each admitted gate reopens. Test disabled/full endpoints, wrong trigger/
width/direction, reset and IRQ masks. Observe last DATA acceptance, final DMA
writeback/TC flag, last device frame and SPI TXC separately; do not release CS
or report a CODAL success callback at DMA completion alone. Compare both feature
profiles and relevant forced-walk/batch1/64/1024 controls. Preserve original
failures, source/job/tree bindings and every enabled check before landing.
RX, links, abort/suspend, physical arbitration and CODAL fibers remain separate.

**Claimed completion-order lane:** add focused CPU-authored TX guests that
separately observe final accepted DATA, zero-count writeback/TC, last slave
byte, TXC, GPIO CS release and an actual guest driver callback call/return.
Use existing read-only bus/instruction observers and owned slave hooks, not
injected completion. Require a genuine DMA-complete/TXC-low window and an
exactly caught premature-release/callback mutant. Exercise paused clocks,
masked interrupts and reset/error outcomes under both profiles and admitted
walk/batch controls. An authored driver callback is not actual CODAL
events/fibers; that integration and selected no-ASF4 runtime qualification
remain the next separate boundary. No native Arcade, performance, physical
timing or app-adoption claim follows from these ordering fixtures.

Add live SERCOM DRE/RXC triggers, production AHB clock gating, descriptor
chaining/abort/suspend/error distinctions and final SPI TXC controls. Then
qualify actual CODAL callback/event/fiber behavior and the selected no-ASF build
closure. Only admitted execution can support active native/WASM Arcade RTx
and app/debugger adoption. A passing byte foundation does not complete P4.
