# Exact Thumb countdown candidate

This optional CPU candidate admits only current-PC `SUBS Rd,#1` followed by
`BNE` back to that SUBS, with both decoded-cache tags valid and widths exactly
two bytes. It does not recognize board identities or guest addresses, change
loop constants, skip MMIO polling, or alter scheduler/observer/debug/IRQ/IT
guards. A branch-rotated entry follows the existing execution paths.

The helper retires complete subtraction/branch pairs within the supplied
instruction budget, stopping after the not-taken branch at zero. Initial zero
is a wrapping countdown, not a zero-trip loop: it requires 2^32 pairs to exit.
An odd remaining budget retires just the last SUBS and leaves PC on BNE.
Final NZCV comes from the last actual subtraction through `sub_with_flags`,
including signed overflow at 0x80000000 and borrow when zero wraps. Budget and
PC arithmetic explicitly handle u32 boundaries.

Both opcodes use one modeled cycle per ordinary Thumb16 instruction in the
current interpreter/batch accounting. A hot decode cache incurs no instruction
fetch memory reads, and the helper accesses no bus. The existing batch advances
its cycle clock and instruction retirement count by the returned count; this
is not a claim of silicon-cycle accuracy.

Nine full-crate regressions compare ordinary `step_internal` execution across
all eight low registers, flags, boundary initial values and budgets; reject
cold/tag-collided/wrong-opcode/wrong-target/T32/rotated entries; cover zero,
overflow, maximum budgets and PC wrap; verify code-patch invalidation, observer
callbacks, Machine cycle/retirement budgets and pending-interrupt priority.
The native qualification workflow explicitly selects the new test module.

A standalone rustc harness extracted the actual helper, tagged-cache lookup,
NZCV update and subtraction helper, with register/cache infrastructure stubbed.
It passed 7,296 comparisons against an independent arithmetic loop. This is a
limited local arithmetic proof, not execution of the nine full-crate tests.
No candidate throughput or linked full-host assembly result is recorded yet.
Fresh hosted correctness, unchanged strict native-motion >=1.0x measurement,
same-runner A/B and full CorePerf review remain required before landing.

The separate CPU-discovery/GPIO qualification must not be delayed or relabeled
by this optional change. Existing validation acknowledgement dates are retained;
digest renewal is not a new silicon capture, browser qualification or app pin.
