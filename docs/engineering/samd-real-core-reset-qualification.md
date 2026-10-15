# Actual CODAL core-reset qualification lane

## Landed core-local checkpoint — 2026-10-15

[PR242](https://github.com/CrispStrobe/labwired-core/pull/242) landed as
`73f8475f8e406b1ff4475bc9886e22479881d67a` after all 27 enabled final-head
checks passed; seven declared optional jobs skipped. Final reviewed head was
`7da58a8d43347e490209710267f95aca3bdda087`. Its
[actual reset guest run38723902862](https://github.com/CrispStrobe/labwired-core/actions/runs/38723902862)
also passed. Executable and guest/workflow inputs are unchanged from the
separately audited source and original described below; the final-head changes
only bind the approved acknowledgements and update documentation.

Reviewed merge `7c2abfe700d8a4a5f7e8f26dfe5f370ceb388c99` had tree
`61b880705eea66ade30c54d446fa0d3a7fd512fc`. Main advanced during landing;
actual parents are `b6cb3b632d2b297f84fcbbde3f8e87b20a660972` and the final
reviewed head, with landed tree `cf8251039ffe7fbaa07179df014e2cfc82447e23`.
The reviewed-to-landed tree delta is solely 26 added lines in
`docs/engineering/target-next-lanes.md`, another worker's lane documentation.
There is no executable or workflow difference. Preserve both tree identities;
do not claim whole-tree equality or overwrite that concurrent lane update.

The [actual run38709086550](https://github.com/CrispStrobe/labwired-core/actions/runs/38709086550)
passed all42 cases at engine source
`7478a36e118f3fad87e2d0b3f1415a96a7f4d7e3`: four reset cases and38 earlier
caller cases. A separately frozen original-log audit passed. Each reset case
retired1720 instructions, with terminal1656 plus64 post-terminal instructions,
two boot epochs and314 raw event/RAM rows. Quantum1 used1720 CPU batches;
quantum64 used28, in both scheduler profiles. This is real bounded batching,
not an absolute RTx measurement or a fully wired board result.

The tested merge was `629e06e14108a277a956928a78f76b881a6ae2f1`, tree
`686e059ec762ade5de81a637b8503f4c25a05f7c`. Compared with the source head,
it adds only `docs/performance/2026-10-15-core-perf-fork-rules.md` from a
concurrent main update; executable and workflow inputs match. Preserve this
distinction rather than claiming source-head/tested-tree equality.

The [twenty-mode build38706839486](https://github.com/CrispStrobe/codal-samd/actions/runs/38706839486)
passed; modes0–18 were byte-identical to the separately retained baseline.
Runtime source `12d4759af0bf2b29726cfd79c487474a282cc5b0` landed through
[PR36](https://github.com/CrispStrobe/codal-samd/pull/36). See the
[Runtime results](https://github.com/CrispStrobe/codal-samd/blob/master/docs/REAL-CORE-RESET-RESULTS.md).

After the actual guest and all three workspace shards passed, the seven
previously acknowledged captured-board content bindings were refreshed with
operator approval. Their acknowledgement age remains2026-10-13 and their
expiry is explicitly2026-11-12; physical captures and results are unchanged.
Physical re-capture remains owed. These are not new hardware qualifications.
The matched-head integration is complete; no consumer package pin is advanced
by this checkpoint. Whole-board reset and the remaining native-image work are
still separate qualifications.

## Original contract and retained failures

The following records the pre-qualification contract, not complete board reset.
The actual caller is pinned to Runtime
`12d4759af0bf2b29726cfd79c487474a282cc5b0`. Its new twenty-mode build and
actual guest have now passed as recorded above; earlier predecessor builds
alone did not qualify the revised image or engine correction.

The first actual [run 38698483581](https://github.com/CrispStrobe/labwired-core/actions/runs/38698483581)
failed before any reset request: the probe incorrectly expected the Thumb bit
from `MRS xPSR`. Arm's table 2-3 specifies EPSR reads zero through this
instruction. The revised caller checks that software-visible value separately
from the unchanged full debugger xPSR check. Keep the original failure.

That original also exposed missing RAM halfword/word callbacks on the fast bus
path. The revised probe explicitly enables
`SystemBus.config.observe_wide_ram_stores` (default false). Only committed RAM
stores report actual pre-store and post-store bytes; flash/MMIO and existing
default observer behavior are unchanged. Permanent unit controls check opt-in,
same-valued stores, existing partial byte-fallback faults and opt-out. Do not substitute invented
byte events or read the bus through an aliased observer reference. This is
diagnostic instrumentation, not a measured performance optimization.

The retained first run also had one CPU batch per instruction at quantum64:
the board's declarative button-shift-register resident drives DATA and the
current resident predicate requires per-cycle service. This is not a NeoPixel
resident: that separate model uses the GPIO-observer attachment path.
The new reset-only fixture explicitly clears external board devices in both
quantums, retains the chip/core models, and reports that fixture identity and
zero external-device count. It still requires actual multi-instruction windows;
it does not claim batching or reset of the fully wired board. All nineteen
earlier caller fixtures keep their unchanged board setup. Full-board resident
timing and reset qualification remain separate work, not a reason to relax the
reset-only fixture's batching assertion.

The focused `CortexM::reset` correction clears prior masks, IT/PSR and LR
according to [Arm's Cortex-M4 register summary](https://documentation-service.arm.com/static/5f2ac4ab60a93e65927bbdbf).
This was a source-review finding, not a claimed earlier guest failure. Permanent
unit controls deliberately poison the prior state. The actual target_reset
caller separately requires pre-CPSID startup register capture, real invalid-key
return versus valid AIRCR store/reset cut, two startup/data/BSS epochs, retained
NOLOAD witness, pre-reset core-state readbacks and closed post-terminal bounds.
Use actual retired store opcode/registers, not invented SCS write callbacks.
Previous-retirement registers at reset entry are OLD context; only the actual
post-first-LDR snapshot and startup's own register stores describe the new boot.

Because image loading also invokes CPU reset, the hosted workflow clean-builds
and repeats all nineteen earlier service/heap/SPI modes at unchanged Runtime
`932d476344107386d56dfd2d595d74d713780563`, under both scheduler profiles.
The new caller also runs single-step and real bounded-batch paths in both.
Every profile/caller executes despite another failure; retain all originals.
Do not merge on unit or build evidence alone. Freeze independent source/image/
provider predicates and refusal controls before sole original acquisition;
then require every enabled final-head check and exact tested/landed tree proof.

The selected engine reset is core-local: peripheral models, external pending
sources, flash commands/persistence, USB/QSPI, physical reset behavior, complete
board/API/build/image admission, native Arcade, active RTx and installed consumer
qualification remain separate. The [board/API continuation](samd-runtime-replacement-plan.md#current-continuation-after-actual-spi-callers)
and [landed lifetime checkpoint](samd-real-spi-lifetime-qualification.md) remain
authoritative. No consumer pin, physical capture or threshold change. Only the
seven approved content-bound acknowledgements described above were refreshed.
