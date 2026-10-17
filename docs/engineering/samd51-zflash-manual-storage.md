# Opt-in SAMD51 manual storage: qualified source boundary

This lane follows the [qualified public source census](https://github.com/CrispStrobe/codal-samd/pull/71).
It adds an explicitly selected manual NVMCTRL slice, CMCC controls and a hosted
caller of the real public ZFlash implementation. The selected actual guest qualification is recorded below. Engine integration
is tracked in [PR249](https://github.com/CrispStrobe/labwired-core/pull/249) and
remains gated on every enabled exact-head check.
Default chip descriptors and consumer pins are unchanged.

The new factory names are `samd51_nvmctrl_manual_storage` at `0x41004000` and
`samd51_cmcc_control` at `0x41006000`; both refuse IRQ/config overrides. The
profile requires exactly 512 KiB of zero-based flash, 512-byte pages and 8 KiB
erase blocks. Only the final 32 KiB storage window admits array stores and
addressed operations. Other regions and unsupported modes refuse explicitly;
that simulator boundary is not a claim that silicon produces the same error.

Register facts are reviewed against Microchip's
[SAM D5x/E5x datasheet DS60001507F](https://ww1.microchip.com/downloads/en/DeviceDoc/SAM_D5xE5x_Family_Data_Sheet_DS60001507F.pdf),
sections 11.6/11.10 and 25.6.6/25.8. No vendor implementation or register header
is copied. Cache controls do not model cache contents or physical cache timing.
This slice excludes fuses, boot protection, region locks, automatic writes,
ECC, NVM interrupts, auxiliary rows, physical timing and reset persistence.

Manual aligned 32-bit stores populate a page buffer with the documented 64-bit
holding-register transitions and address updates. Main flash stays unchanged
until an admitted WQW command commits its sixteen-byte window. EB erases one
8 KiB block; PBC clears buffering. The key and busy admission, READY, DONE,
PROGE/ADDRE and their supported W1C operations stay distinct. Unsupported
commands refuse rather than being mislabeled as invalid hardware commands.

Four instruction-boundary services provide a finite deterministic busy interval,
not measured silicon latency. Pending operations end Cortex-M batches; the
existing flash-operation predicate also excludes JIT execution. Commit and
READY publication follow in the same boundary, against the actual bus backing.
Default buses take the absent-controller path. Unsupported concurrent array
writes refuse; this does not implement an AHB stall protocol.

## Real-driver guest and mutation qualification

The hosted builder selects codal-samd `62a3a408ac544a4e34bf0beb3932683a2c114e26`,
CODAL core `2ed19d64ec3359143409a4b0a9758b4aeef3ee7d` and framework
`31abf23d118f35010fb75122e60a1eb2b6dffe9f`. It reuses the qualified include-only
ZFlash compiler producer and links that actual object with an authored boot-only
caller, real CPS instructions and freestanding byte-copy/clear routines. It
supplies no invented Flash implementation. This is not a complete CODAL/PXT
runtime or its paired fiber/IRQ service implementation.

The linker reserves the complete storage window outside executable/load data.
The observer declares its initial dirty test block and neighboring canaries
before execution. Only actual guest-driver calls and model operations may
subsequently change them. Six ordered checkpoints require geometry, real erase,
32-byte programming, an eight-byte partial window, actual -1/-2/-3 driver
refusals and re-erase, preserving all neighbors. Single-step observation follows
all four original method entries and LR returns with actual r0 results; a
separate fresh advance-API run checks every checkpoint's backing state. Both
use the declared default peripheral interval of one; no widened-batch result
is inferred. The actual
IRQ helpers must execute three matched CPS pairs in thread context; their
original LR returns and actual entry/return PRIMASK transitions are observed
directly. Startup admits interrupts after data/BSS initialization, establishing
the explicit unmasked, nonnested precondition for these boot-only helpers.

Two isolated copied-model mutations omit erase commit or program commit. Each
must expose its exact actual wrong byte after a real successful driver return;
compiler/setup failures, unrelated errors and exhausted bounds cannot pass.
Both scheduler profiles passed in
[run39007999563](https://github.com/CrispStrobe/labwired-core/actions/runs/39007999563)
at reviewed source `69368aff0cfa07bc4ba6dbbee2967cfceff0010b`, tested merge
`7cec74a855d8b4625e21f0907f04426a0d9f6e9c` and tree
`07462ad5bc05da64610d541866b7c6909cc37098`. Before integrating source, verify the landed tree and ordered parents against
the tested and predicted composition.

Each profile produced twelve checkpoint observations across two fresh execution
modes, ten original ZFlash method returns and six actual CPS helper returns.
Each also detected both exact erase/program commit mutations at their required
wrong bytes after real successful driver returns. Independent readers audited
source/image/commands, sixteen dependency identities, component notices, the
original streams and exact mutation patches. Both profiles used the same ELF
`95a68be3d3e8f4875def40cea1914479b2c36c9ce2bd8226dc64ee0f8500d8dc`. These are finite selected-driver results.
Earlier broader CI failures were retained; the final source uses a peripheral
capability without increasing any downcast ratchet ceiling, and its generated
ignored-test inventory explicitly records the hosted fixture.

## Scoped follow-up contracts

The existing DMA API still borrows caller-owned buffers. Preserve the original
masked async terminal interval, the newer finite unmasked intervals and the
expected premature-release witness; none establishes enforced ownership or
unlimited lifetime. Keep the [qualified DMA boundaries](https://github.com/CrispStrobe/codal-samd/blob/master/docs/PXT-GC-FAILURE-LIFETIME-LANES.md)
when requalifying a new engine.

These are pending contracts, not ownership claims. Refresh repository heads,
open PRs and lane ownership before starting. Keep the admitted public sources,
notices, original failures, GC/DMA regressions and all stated bounds.

1. **Consumer engine adoption.** After the engine source merge, claim a separate
   codal-samd consumer-integration lane. Update only its explicitly reviewed
   engine binding and the selected storage fixture's opt-in NVMCTRL/CMCC
   entries. The historical census pin remains historical evidence. Requalify
   the existing GC/OOM, synchronous/asynchronous and allocated-DMA cases against
   that engine, plus both actual ZFlash profiles and their specific model
   mutants. Admit exact source/descriptor/image/artifact identities before
   changing a consumer pin. Do not enable the models in unrelated/default
   descriptors or infer browser adoption from native results.
   Enumerate checkout/admission pins in codal-samd's runtime workflows
   (`.github/workflows/pxt-gc-*.yml`, `pxt-public-*.yml`,
   `pxt-native-real-oom.yml` and `pxt-allocated-dma*.yml`) and their matching
   `pxt-gc-runtime/guest*.rs` and `check_*mutants.py` assertions. Review the
   complete selected binding set together; keep historical reports and
   `profiles/pxt-storage-link-census.json` immutable. Create new audit frames
   for the new engine rather than relabeling older successful originals.
2. **Actual RAFFS/settings execution.** Start only after qualified consumer
   adoption and real GC/root/allocation-service composition. Link the admitted
   actual filesystem/settings sources and real source-bound hash/configuration/
   scheduler services; do not replace filesystem methods with fixtures. Select
   one explicit storage size within the admitted final 32 KiB. Require real
   format/mount/write/read/remove/list, refusal cases, live GC payload retention
   and neighboring/code/data canaries in both profiles. Define one persistence
   boundary in the contract before implementation; this model currently proves
   neither reset nor power-loss persistence. Add specific nonvacuous mutants,
   freeze independent readers and qualify clean hosted guests before merging.
3. **Full board and IRQ/service configuration.** Separately census the remaining
   shared-header dependencies and the historical USB/WebUSB-enabled board
   configuration. Trace required functions to admitted public implementations;
   model and qualify each missing behavior in an owned lane. The boot-only CPS
   helpers here do not qualify full CODAL paired fiber/IRQ semantics. The held
   USB/QSPI stubs and feature-disabled service fixture do not establish the
   original board configuration. Preserve that gap until actual enabled
   configuration and affected guests pass; disabling required features cannot
   close it.
4. **Additional controller semantics or downstream adoption.** Fuses/protection,
   automatic writes, ECC, cache contents/timing, NVM interrupts and reset/
   persistence each need a separate precise model contract and affected guest
   qualification. Native Arcade, WASM/installed Lite and performance require
   their own exact engine/package/consumer evidence after source integration.
   No physical, release or complete storage-runtime conclusion follows from
   the selected flash case.

All source/model changes require affected clean guest qualification and every
enabled exact-head check. Acquire official originals once after metadata
admission and audit retained bytes with standalone readers, without replay.
Documentation-only follow-ups use the applicable source-policy/docs checks;
do not manually dispatch another unchanged ARM guest merely to update prose.
