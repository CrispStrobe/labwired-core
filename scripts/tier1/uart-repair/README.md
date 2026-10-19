# Tier1 UART fixture initialization repair — qualified and landed

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

[PR193](https://github.com/CrispStrobe/labwired-core/pull/193) adopted these two
exact blobs and their manifest digest/source-provenance entries, then merged as
`80823eb479e20327d8ecb3f35b4d0aa4912a1a2a`. Unrelated fixtures and manifest
entries were unchanged. Its final
[guest run37946576833](https://github.com/CrispStrobe/labwired-core/actions/runs/37946576833)
passed all four cases again, with the same adopted ELF hashes. The unchanged
[full Tier1 matrix and immutable-baseline ratchet37946567305](https://github.com/CrispStrobe/labwired-core/actions/runs/37946567305)
and every enabled final-head check passed before merge.

[PR190](https://github.com/CrispStrobe/labwired-core/pull/190) subsequently landed
after requalification on the repair; the separate software-pended IRQ fixture
[PR191](https://github.com/CrispStrobe/labwired-core/pull/191) also landed after
fresh qualification. Their bounded results and remaining runtime work are in
[the SAMD runtime lanes](../../../docs/engineering/samd-permissive-runtime-lanes.md).

The superseded diagnostic [PR192](https://github.com/CrispStrobe/labwired-core/pull/192)
is closed without merging its experiment workflow. Its two original diagnostic
runs remain historical evidence: the old fixtures were silent on both source
pins and in forced single-step, which did not establish a PR190 or JIT
regression. No further implementation is pending for that original silence.
Any new console failure needs a fresh source-bound reproduction and its own
scope; do not rerun the successful historical experiments merely to read them.
No calibrated baud, hardware, application adoption or performance claim follows.
