# Prospective strict paired-IRQ guest harness

Preparation only: no source is pinned or qualified here yet, no workflow is
installed, and these controls have not compiled or executed. Do not interpret
the presence of the test as runtime admission. Keep historical IRQ fixtures
unchanged.

After every enabled exact-head CODAL integration check passes, select that
immutable source explicitly, review the build and test, and add a dedicated
hosted workflow. `build.sh` requires `LABWIRED_PAIRED_QUALIFIED_SOURCE`, a clean
source checkout, and builds five controls. This explicit parameter checks
identity, not GitHub qualification; the workflow/reviewer must verify the latter.

Execute the ignored `samd_paired_irq_guest` test twice, with ordinary scheduling
and `--features event-scheduler`, providing `LABWIRED_PAIRED_FIXTURE_DIR`.
Bind source files, compiler, linked ELF hashes, final qualification head/tree,
official run identity, and both execution logs in a reviewed receipt. Add
manufactured positive/refusal controls for that receipt reader before publishing
a success claim. No timeout, compiler failure, missing result, or unexpected
exception may count as a successful refusal.

The prospective test validates all vector words, startup BSS clearing and .data
copy, positive IRQ0 delivery only after the outer pair restores the mask, and
four exact faults: underflow3, changed diagnostic identity5, depth limit4, and
lost mask6. Each refusal must continue for 64 observed instructions, masked in
thread mode, with unchanged diagnostics and zero IRQ witnesses, inside the
actual compiled terminal function. This is a bounded liveness observation, not
proof of indefinite behavior. The diagnostic identity is not a real fiber.

Still owed: hosted compile/format/lint checks, actual source-bound guest results,
source/receipt adversaries, real CODAL caller and scheduler integration,
peripheral IRQ/DMA, notices and consumer adoption. No ASF-free runtime, native
Arcade boot, physical safety or performance result is established by these
authored controls.
