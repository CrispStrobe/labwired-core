# Prospective actual CODAL cancellation and recovery

This separate fixture selects caller mode 9 from runtime source
`08ee445a5d1c9a54125b6011ec4b40ff0cc79646`, integrating the current runtime
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
