# Opt-in SAMD51 manual storage: source candidate

This lane follows the [qualified public source census](https://github.com/CrispStrobe/codal-samd/pull/71).
It adds an explicitly selected manual NVMCTRL slice, CMCC controls and a hosted
caller of the real public ZFlash implementation. It has not yet passed actual
guest qualification. Default chip descriptors and consumer pins are unchanged.

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
separate fresh batched run checks every checkpoint's backing state. The actual
IRQ helpers must execute three matched CPS pairs in thread context; their
original LR returns and actual entry/return PRIMASK transitions are observed
directly. Startup admits interrupts after data/BSS initialization, establishing
the explicit unmasked, nonnested precondition for these boot-only helpers.

Two isolated copied-model mutations omit erase commit or program commit. Each
must expose its exact actual wrong byte after a real successful driver return;
compiler/setup failures, unrelated errors and exhausted bounds cannot pass.
Both scheduler profiles are required. Initial source controls are not guest
qualification: freeze independent original readers, acquire admitted official
originals once, and require every enabled exact-head check and exact reviewed/
tested/predicted/landed composition before merging.

Later work includes actual RAFFS/settings execution, required board/USB/WebUSB
configuration, cache coherence outside this coherent backing slice, protected
regions, reset/persistence, native Arcade, performance and installed adoption.
No physical accuracy, release or broad storage-runtime claim follows here.
