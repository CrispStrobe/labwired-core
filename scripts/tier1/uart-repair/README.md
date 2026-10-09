# Tier1 UART fixture initialization repair — pending

[Diagnostic run37941472606](https://github.com/CrispStrobe/labwired-core/actions/runs/37941472606)
retains twenty silent cases: two source pins, two chips, five execution modes.
Forced single-step is also silent. Batched modes retire the full8M fuel with
zero idle skips; JIT is unavailable in the default standalone CLI build.
This does not support blaming PR190, JIT or idle fast-forward for the silence.

Two source-level mismatches explain why the old fixtures cannot exercise their
current console paths:

- nRF52832 only enabled legacy UART and wrote TXD. The model now requires
  STARTTX and finishes a shift before accepting the next byte. The repair
  sets the baud/pad, starts TX, clears and polls TXDRDY with a finite bound.
- STM32F103 never configured PA9 alternate-function output or a nonzero BRR.
  The current F1 console sink follows that pad/divisor gate. The repair enables
  the console clocks, configures PA9, sets BRR and enables USART/transmitter.

No peripheral model, ratchet, snapshot or acknowledgement changes. The hosted
workflow rebuilds only these two public authored fixtures and qualifies all
nine existing reported classes plus `TIER1 done` in both ordinary and forced
single-step runs. Exit zero alone is not success. Original streams are kept
on failure; artifacts contain logs/hashes, not binaries.

This branch initially changes fixture **source only**. Committed ELF blobs and
their manifest remain unchanged until the source-built guest result is audited.
Do not merge this source-only checkpoint as a repaired production matrix.
After qualification, adopt exactly the tested generated fixtures with their
manifest/source provenance, retain the original failed runs, and require the
unchanged full matrix/immutable-baseline ratchet and all enabled CI gates.
Then requalify PR190 against the landed repair before considering its merge;
PR191 remains a separately qualified, bounded software-pended IRQ fixture.
No calibrated baud, hardware, application adoption or performance claim follows.
