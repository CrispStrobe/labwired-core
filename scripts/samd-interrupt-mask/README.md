# Authored interrupt-mask ARM guest control

This dedicated hosted qualification pins the unconnected MIT prototype in
[CODAL PR5](https://github.com/CrispStrobe/codal-samd/pull/5) at
`e15482fd6b537ffd208919c31d568c0bea14d47a`. It changes no production CPU,
peripheral, firmware admission or app dependency. Guest execution is pending.

The builder compiles two owned Cortex-M4 fixtures without standard libraries:
the reviewed header, and a temporary mutant that always restores PRIMASK to
zero. Both must link without unresolved symbols. The positive fixture checks
disabled entry and nested enabled-entry restoration; the mutant must reach
assertion3 (incorrect already-disabled restoration), not fault, time out or
fail compilation. Each test observes vectors, poisoned-BSS clearance, entry100
and the exact terminal status within100000 instructions, in both default and
event-scheduler profiles. Compilation alone is not execution evidence.

The receipt binds the actual PR merge commit/tree/parents, reviewed head,
fixture pin, run/attempt, logs and observed ELF hashes. Artifacts contain only
receipt, result logs, maps and symbols; no firmware binary is uploaded. An
independent receipt/log/source audit is still required before publishing a
pass. This initial workflow always executes; it does not claim docs-only reuse.
Do not push receipt-only prose while qualification is pending or repeat an
already successful run unnecessarily. A future reuse gate must bind all
engine/config/dependency/test/build/workflow inputs, not just this fixture pin.

This bounded control is not pending/active IRQ delivery, NMI/HardFault masking,
NVIC vector handling, DMA memory ordering, fiber/driver integration, hardware
behaviour, ASF removal or runtime-image admission. No new performance or
whole-component licence claim follows. A complete vector/NVIC positive and
negative fixture is the next separate qualification before atomic integration.
