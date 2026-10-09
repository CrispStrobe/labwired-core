# Tier1 UART fixture initialization repair — full ratchet pending

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
on failure; qualification artifacts contain logs/hashes, not binaries.

## Qualified guest inputs and adoption

[Original guest run37943735344](https://github.com/CrispStrobe/labwired-core/actions/runs/37943735344)
passed all four cases. Each chip emitted its nine existing class `PASS` lines
and exactly one `TIER1 done` in ordinary and single-step execution, exit zero,
without timeout. Independent raw-stream hash/size/result auditing passed.
Tested merge `bf7452f0ae6f52effef85abd83c09815f5acbcda` has tree
`b477c9530e36188e7bedb8ea0dd5904cdb587c36`, equal to reviewed source
`f8626c03c7bdb57ff0d983168baeb258e91659c1`.

[Export run37945705313](https://github.com/CrispStrobe/labwired-core/actions/runs/37945705313)
rebuilt that immutable tested source without rerunning an engine. Both
generated ELF hashes exactly matched the original qualified guest inputs:

| Fixture | SHA256 |
| --- | --- |
| nRF52832 | `d754f4e9a76a39b4b12df2cf9c6fa54e59047a196d2c06899000588d943b26c4` |
| STM32F103 | `4e8f7755b014e02c12d099d6d063be0d761cc604ed7778981388b79b321dd82c` |

The branch now adopts these two exact blobs and updates only their manifest
digest/source-provenance entries; unrelated fixtures and manifest entries are
unchanged. The export contains only these public authored guest inputs, not an
engine or private firmware. This bounded qualification is not yet a full-matrix
pass: retain the original failures and require the unchanged matrix,
immutable-baseline ratchet and every enabled final-head CI gate before merge.
Then requalify PR190 against the landed repair before considering its merge;
PR191 remains a separately qualified, bounded software-pended IRQ fixture.
No calibrated baud, hardware, application adoption or performance claim follows.
