# Prospective actual CODAL cancellation and recovery

This separate fixture selects caller mode 9 from runtime source
`846601f69005d2cbd1c82f33e350301882ac4f80`, integrating the current runtime
documentation base without changing the previously built cancellation caller.
The prior source `9bdd9177a3d3b2200bbfd5bcdb1a3650b0c439a7` compiled; all nine
earlier images remained identical. Final-source build and actual guest outcomes
are required before qualification. This is not complete TX-service, native
Arcade, physical behavior, installed-app or performance admission.

The unchanged selected driver runs a 32-byte RAM transfer. A distinct real
fiber's competing start must refuse busy; its real cancel call must produce
exactly one typed failure, correct argument/identity and busy callback reentry,
without success delivery. The real failure callback emits an actual Event to
the enrolled main fiber. No host callback/Event injection is used.

The observer preserves the entire cancelled phase, not a filtered prefix:
CS, every partial DATA/wire byte, actual failure entry/return, cancellation
return and resumed main. It binds accepted-byte count to the real DMA writeback
remainder after cancellation, before recovery resets the descriptor. Released
CS must precede failure delivery; return/wake ordering must match. Blocked
attempts must not replace the caller's buffer, callback or terminal ownership.

Recovery then requires the same two four-byte asynchronous/blocking transfers
and unchanged 28-row DMA/wire/TXC/CS/callback/fiber ordering as the positive
caller. Final callback/failure witnesses, actual exception entries/returns,
assembly swaps, vector/startup/data/BSS and immutable source/image/map must bind.
The four-million-step bound is unchanged. Manufactured missing-row, byte and
premature-release controls are not actual guest evidence.

Freeze a separate independent reader with source/image/metadata bindings before
acquiring actual originals. Both scheduler profiles and all enabled exact-head
checks are required. Runtime [PR22](https://github.com/CrispStrobe/codal-samd/pull/22)
and this fixture remain prospective until those gates pass. Preserve the prior
positive guest and all failures; no consumer pin, threshold, acknowledgement or
physical capture changes are part of this slice. Error, gated timeout, stale
IRQ and blocking-terminal lease cases remain separate subsequent requirements.

The first actual source-08ee cancellation guest delivered one typed failure,
resumed main and retained writeback count 30 (two accepted beats), but failed
recovery: DMAC reset was requested while global enable remained set. Its original
failure is preserved. The corrected driver explicitly globally disables the
exclusively owned controller before reset. The model's reset guard is unchanged.
Because shared driver code changed, the workflow rebuilds both mode 8 and mode 9
from the same immutable corrected source using separate clean core checkouts,
then requires both positive and cancellation cases in both scheduler profiles.
Step errors now preserve observed raw rows before failing; no source budget,
ordering predicate or failure is weakened or relabelled as success.
