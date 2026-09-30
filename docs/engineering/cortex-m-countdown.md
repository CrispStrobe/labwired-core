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
The [native hosted run 36720616418](https://github.com/CrispStrobe/labwired-core/actions/runs/36720616418)
passed all nine actual Rust countdown tests (0.61 seconds), the complete
153-test Cortex-M selection, GPIO/model/DMA/actual ARM functional proof and
WASM input-routing checks. Its [full motion receipt](../receipts/2026-09-30-microbit-countdown-native-motion.json)
records median **1.1112514028965554x**, minimum **1.102223808287991x**;
all five measured windows exceeded 1.0x. The separate
[GPIO-only receipt](../receipts/2026-09-30-microbit-countdown-native-active.json)
records median **4.805243504777581x**, not full motion throughput. The
[runner record](../receipts/2026-09-30-microbit-countdown-native-runner.txt)
identifies **AMD EPYC 7763**. These receipts retain tested PR merge-ref
`0d634f135970197d68e0eea9f4f00955367b21b3`, head
`068513485a90866d091cb0f2971541fff467dba5`, and the original source-bundle and
ELF hashes. The retained motion ELF SHA-256 is
`7ed400fb34f54fb7dac87882259c86741a41ada7fa22a9b6505dd580aeba9b12`.

The [same-runner A/B run 36720616379](https://github.com/CrispStrobe/labwired-core/actions/runs/36720616379)
also passed on **Intel Xeon Platinum 8573C**, alternating B/C/C/B after both
engines were built. Candidate medians were **1.474740510337552x** and
**1.470143579558601x**; baseline medians were **1.1617078117377073x** and
**1.1623196824899693x**. The ratio of median-of-medians was
**1.267146837638769**, with all ten candidate samples above 1.0x.
The [complete summary](../receipts/2026-09-30-microbit-countdown-ab-summary.json),
[source provenance](../receipts/2026-09-30-microbit-countdown-ab-source-provenance.json)
and [runner context](../receipts/2026-09-30-microbit-countdown-ab-runner-context.json)
retain the original tested merge-ref and identical guest/harness hashes. Each
full invocation receipt is retained separately:
[baseline 1](../receipts/2026-09-30-microbit-countdown-ab-01-baseline.json),
[candidate 2](../receipts/2026-09-30-microbit-countdown-ab-02-candidate.json),
[candidate 3](../receipts/2026-09-30-microbit-countdown-ab-03-candidate.json), and
[baseline 4](../receipts/2026-09-30-microbit-countdown-ab-04-baseline.json).

That baseline is main `3456c048894f194bbabc9c414932a752d89da999`, before
the combined discovery/GPIO/CAN changes. Thus this measures the **cumulative
candidate**, not an isolated countdown-only improvement; sequential same-runner
execution still does not isolate contention. Its Intel results cannot be
compared as an A/B with the separate AMD native job or prior AMD 9V45 runs.
An [isolated countdown comparison, run 36723407331](https://github.com/CrispStrobe/labwired-core/actions/runs/36723407331),
uses exact combined baseline `307541bdeff689115dd78fa96051a8ff6900d26c` and
the existing unchanged guest/gates. It **passed on AMD EPYC 7763** with
baseline medians **1.0170460311758822x** and **1.0384880332869062x**, and
candidate medians **1.0921114074772624x** and **1.1235781621159648x**.
The median-of-medians ratio was **1.077914303586253** (+7.79%); candidate
minima were **1.0875331105165045x** and **1.114310187944047x**, and all ten
candidate windows exceeded 1.0x. This isolates the countdown source delta
against the combined parent, within the sequential-run contention limitation.
The [raw summary](../receipts/2026-09-30-microbit-countdown-isolated-summary.json),
[source provenance](../receipts/2026-09-30-microbit-countdown-isolated-source-provenance.json)
and [runner context](../receipts/2026-09-30-microbit-countdown-isolated-runner-context.json)
retain the exact tested branch head, baseline and identical guest/harness
hashes. Full invocation receipts preserve every sample and original ELF hash:
[baseline 1](../receipts/2026-09-30-microbit-countdown-isolated-01-baseline.json),
[candidate 2](../receipts/2026-09-30-microbit-countdown-isolated-02-candidate.json),
[candidate 3](../receipts/2026-09-30-microbit-countdown-isolated-03-candidate.json), and
[baseline 4](../receipts/2026-09-30-microbit-countdown-isolated-04-baseline.json).

The source-identical pre-rebase candidate's
[full CorePerf run 36720189157](https://github.com/CrispStrobe/labwired-core/actions/runs/36720189157)
**failed** the per-board instruction-baseline gate. It tested original head
`5f2354fad09af2f0ccea27285e10f0b025082ab4`; the later base-only rebase changed
two validator test files, not runtime code. This failure is not waived or
rebaselined, and the narrow native/A/B passes are not an all-board qualification.
The failing step instruction counts were nRF52832 +3.2%, nRF52833 +3.7%,
nRF52840 +3.6%, nRF5340 +4.0%, nRF54L15 +4.4% and nRF54LM20A +4.6%
against the existing baseline; the nRF51822 count improved by 16.2%.
No linked full-host assembly result is recorded. The combined parent landed as
main `96b739c259a0c09bfa71a499a43d74a96ed2c37c`; the countdown branch initially was
rebased onto it without changing production code, configuration, guest, tests
or workflows from the qualified `06851348` source. The six pre-existing Nordic
relative instruction costs remain tracked in issue #120; no threshold or
baseline changed. The isolated comparison and native proof support incremental
review, not a claim that full CorePerf is green. This optional optimization is
not yet merged at that recording. It subsequently landed as main
`8736e1ff`; the original qualification commits below remain unchanged.

Subsequently, SAADC PR135 landed as main
`ede33fb4a4778f35cc3398190aaa3d2cf9beb4db`. The countdown branch now includes
that parent and retains both its actual ADC-scan and DMA fidelity test
selections alongside the explicit nine-test countdown selection. Countdown
CPU source and all original fixtures remain byte-identical to the qualified
`06851348` source, but **the whole runtime is not identical**: the parent adds
the new SAADC model. Fresh native and core qualification of this composition
is required before landing. Earlier receipts qualify their recorded original
commits only; no fixture was rebuilt or historic provenance rewritten.

The [fresh combined native run 36726287167](https://github.com/CrispStrobe/labwired-core/actions/runs/36726287167)
then **passed** on **AMD EPYC 9V74**, testing PR merge-ref
`63eb72d71e8f8aca24f0efcc83b9a0416ad07bd5` from branch head
`8f96601cefe1ae64e34c5723e5d6bf39dbf0a870`. The retained
[test-log excerpt](../receipts/2026-09-30-microbit-countdown-combined-test-excerpt.log)
records nine actual countdown tests (0.56 seconds), all 153 Cortex-M tests,
26 SAADC tests, the actual ARM ADC-scan integration test, four tick512 DMA
fidelity tests and three native WASM analog-routing tests, all passing.
The [motion receipt](../receipts/2026-09-30-microbit-countdown-combined-motion.json)
records median **1.0866680054778717x**, minimum **1.0710410932419758x**,
with all five windows above 1.0x. The separate
[GPIO-only receipt](../receipts/2026-09-30-microbit-countdown-combined-active.json)
records median **5.126428649797019x**; the
[runner context](../receipts/2026-09-30-microbit-countdown-combined-runner.txt)
retains original toolchains/run identity. Downloaded original ELF hashes were
verified: motion `56440443ca947fcdd0e305357935bb4ad7a49042f9cd758684bae42ceac099b1`,
ADC scan `53d8d448bf00dfc1aab7fec3968e7d3b6fd83f371486a719f249d456c8828f85`.
These native results qualify this composition narrowly, not browser runtime or
all-board CorePerf. Its broader PR/core checks remain pending at this recording;
the countdown change was unmerged at that recording and has since landed as
main `8736e1ff`. The later composed GPIO qualification is recorded separately
in [the composed proof](microbit-composed-qualification.md).

The separate CPU-discovery/GPIO qualification must not be delayed or relabeled
by this optional change. Existing validation acknowledgement dates are retained;
digest renewal is not a new silicon capture, browser qualification or app pin.
