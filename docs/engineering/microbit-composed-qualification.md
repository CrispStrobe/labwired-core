# Composed countdown, SAADC and GPIO qualification

The exact composed source `1e7ba0a3575c7df43b2f29a09ec9561ca9fd3b2b`
passed both [native qualification 36733437798](https://github.com/CrispStrobe/labwired-core/actions/runs/36733437798)
and [CorePerf 36733427626](https://github.com/CrispStrobe/labwired-core/actions/runs/36733427626).
The rebased PR137 source `f84a57cd30fcc416068005d2bb2076200ef92e36`
has byte-identical engines, configuration, guests, tests, workflows, validation
manifest and performance scripts; only engineering/receipt documentation differs.
Original evidence identifies the tested commits, not the later documentation head.

Native testing on AMD EPYC 9V74 recorded full polled motion/display/buttons
median **1.3963423432295103x**, minimum **1.369084261691459x**, all five
windows above 1.0x. Separate GPIO-only median was **7.3078141345753265x**.
The unchanged motion guest/harness was used. The selected functional invocations
passed 349 tests **including overlapping filters**, plus two benchmark assertion
tests: countdown9, full Cortex-M153, SAADC26, actual ARM ADC-scan1,
DMA fidelity4, pull8, mask16 and WASM analog-routing3 are included. This is not
349 unique tests or a browser motion/audio qualification.

CorePerf passed both the unchanged absolute **all40 chips >=1.0x** gate and the
strict relative instruction gate across **78 measured board-modes/11 maps**.
There were zero regressions, contract failures, never-measured targets or waivers.
The six earlier Nordic step regressions no longer trip this receipt. A stale
*improvement* warning remains for nRF51822 (-17.76%); it is retained without
baseline or threshold changes. CorePerf ran on Ubuntu24.04 image20260920.314.1,
Rust1.95; that job did not record its CPU model, so the native CPU must not be
attributed to it. These distinct jobs are not a controlled wall-time A/B.

The [receipt directory](../receipts/2026-09-30-microbit-composed-1e7/qualification-context.json)
retains the original native JSON/logs/runner context, four complete CorePerf
reports/status files, test-log excerpt and exact run/artifact/source provenance.
Downloaded motion and actual ADC ELF SHA256s were independently checked;
original hashes remain in context, without committing ELF binaries.

Old broad workspace checks were canceled at the 45-minute timeout after slow
apt downloads in both affected shards: shard 1 spent 44m44s in the compiler
installer and never began tests; shard 3 spent 43m15s there and had only 78s of
compilation/testing before cancellation. Both installers pulled an unnecessary
463MB C++ newlib archive. Their logs reveal no source error; this is not a
claim that the uncompleted tests passed.
The original PR gate, browser and scheduler checks passed. Fresh broad CI on
the rebased composed branch remains required for the final integration review;
this record does not declare PR137 landed or every broad check green.

## Controlled composed-source A/B

[Same-runner A/B 36731892881](https://github.com/CrispStrobe/labwired-core/actions/runs/36731892881)
compared baseline `ede33fb4` with composed candidate `464bd0ed` on one AMD EPYC
9V74 runner, using separate source/target directories in baseline/candidate/
candidate/baseline order. Guest assembly, linker script, motion include and
benchmark harness were byte-identical. The [original seven JSONs, raw measurement logs and audit context](../receipts/2026-09-30-microbit-composed-isolated-ab-36731892881/qualification-context.json)
retain exact source/runner provenance and verified hashes of all four actual
hosted ELF binaries. Revalidation reproduced all twenty timing/formula and
functional sample assertions, both strict candidate median gates, and the
summary. All ten candidate individual samples also exceeded 1.0x.

Baseline median-of-medians was **1.0035145315080594x**, candidate
**1.0877771001141803x**: ratio **1.083967462314166**, or **+8.396746%** on this
paired host/workload. Candidate invocation medians/minima were
1.0880796209661583x/1.057997003606482x and
1.0874745792622023x/1.0772277767659197x. The first baseline median
0.9987930115815333x remains visible; baseline real-time enforcement was
intentionally off while candidate enforcement stayed on. Separate native
runs on other hosts, including the preserved 0.995903x failure, are not a
substitute for this A/B or erased by it. These observations establish bounded
native held-input motion/display/button performance, not browser/audio,
silicon timing, all-host speed or completion of broad workspace CI.

## Fresh rebased native proof

[Native run 36772744347](https://github.com/CrispStrobe/labwired-core/actions/runs/36772744347)
passed on **AMD EPYC 7763** from rebased branch
`8c745a68d3f33086245905c951ba73a5fed8ff59`, testing merge-ref
`3afa20885006f4c36a2c9f19b3f35759841cc595`. Full motion median was
**1.1161352077261557x**, minimum **1.0514338861805301x**; all five windows
were above 1.0x with zero guest errors. Separate GPIO-only median was
**5.536380578241844x**. This is a fresh native result on a different CPU,
not a controlled comparison with the earlier 9V74 result.

The original logs again record **349 selected functional passes including
overlapping filters**, plus two benchmark assertion tests: countdown9,
Cortex-M153, SAADC26, actual ADC-scan1, DMA fidelity4, pull8, mask16 and WASM
analog-routing3 are included. The
[new receipt context](../receipts/2026-09-30-microbit-rebased-8c/qualification-context.json)
links the original five JSON/log/context files, exact test-log excerpt, original
artifact digest and three independently verified motion/ADC ELF hashes.

Runtime/configuration/guest/test/validation/performance source is byte-identical
to qualified `1e7ba0a3`. The only source comparison exceptions are the broad
CI cross-compiler installer (`--no-install-recommends`) and its regression:
15 Python tests passed, and the actual freestanding ECU C sources compiled and
linked with `-nostdlib` into a fresh temporary directory. This is not a fixture
rebuild or local Cargo/firmware execution claim. The CI change does not alter
the native benchmark, simulator or performance thresholds. Fresh broad checks
were still running at the original receipt recording; the completed results
and subsequent CI-only split are distinguished below.

## Completed 8c broad checks and pending split qualification

The completed [8c Core CI run 36772744309](https://github.com/CrispStrobe/labwired-core/actions/runs/36772744309)
passed all three workspace shards and their aggregate, browser and scheduler
checks, Clippy and default-member library tests. Its monolithic `pr-gate`
nevertheless hit the unchanged 20-minute job limit while feature-off core was
still compiling, after 4m30s in that step. The following AVR, walk-derivation,
firmware-survival, BLE-provenance and SoftDevice commands were skipped in that
job. This is a separate compilation-budget timeout, not the earlier apt
download timeout and not a full broad-CI success or observed source failure.

CI-only head `bd05656f9af2f4fa536db8834bccb9aa08b7b400` splits the original
commands between `pr-default-members` and `pr-feature-off`, each retaining a
20-minute limit. The original `pr-gate` context now always evaluates both
results and passes only if both children succeed. Cargo commands, feature
flags, nonvacuity checks, compiler/interpreter pins and read-only PR caches are
unchanged; the existing structural CI-contract test is adapted to count only
those explicitly enforced children. Production engines, guests and simulator
configuration remain unchanged from the qualified 8c runtime.

[Split Core CI 36778919591](https://github.com/CrispStrobe/labwired-core/actions/runs/36778919591)
was still pending at this docs-only successor's preparation. No new complete
CI verdict or PR137/main landing is inferred from the earlier runtime proofs;
the original timed-out run and its skipped commands remain part of the record.
