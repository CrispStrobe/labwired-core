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
