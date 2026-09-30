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

Old broad workspace checks were canceled at the 45-minute timeout (an apt
download in one shard and compilation in another), not proven source failures.
The original PR gate, browser and scheduler checks passed. Fresh broad CI on
the rebased composed branch remains required for the final integration review;
this record does not declare PR137 landed or every broad check green.
