# CPU-driven SERCOM4 TX DMA — qualification and landing boundary

[PR216](https://github.com/CrispStrobe/labwired-core/pull/216) implemented the
explicit `sam_dmac_sercom4_tx_byte` profile and CPU-authored hardware-request
guests. This is not native Arcade, CODAL events/fibers, final TXC/CS/callback
ordering, physical timing, whole-runtime provenance or a new RTx measurement.

## Reviewed source qualification

Source `6b66c69d5548b54cfb6dca26f2eb50490d5cd454`, base
`d65931517d96e30bedb1508044103f95f99be4ec`, tree
`45f08c3190b0a5d2065d2c58817802f389459dc7` passed all 22 enabled checks and
four declared conditional skips in
[Rust run38379682539](https://github.com/CrispStrobe/labwired-core/actions/runs/38379682539).
Both original guest logs were independently audited without importing engine
code or replaying the guests. Actual checkout
`dd886f42c43a71eedecebf32634e71f080c7dc67` had the reviewed tree and parents.

| Profile / official job | Cases | DMA entries / returns | Wire bytes | Original log SHA256 |
| --- | ---: | ---: | ---: | --- |
| [Feature-off115236706908](https://github.com/CrispStrobe/labwired-core/actions/runs/38379682539/job/115236706908) | 228 | 84 / 84 | 456 | `a141f48a7f96ed6ad3c4d353495189802546030099b5f2ead5f1a57527fef64e` |
| [Feature-on115236706877](https://github.com/CrispStrobe/labwired-core/actions/runs/38379682539/job/115236706877) | 456 | 168 / 168 | 912 | `3c8761bc67b320505b9936e86275ac1fc45d5929920026a098441b902408aaac` |

Original log sizes are 699978 and 207219 bytes respectively. Neither audit found
missing or duplicate cases. Both covered three named TX tests, preserved the
software DMA90-case controls, controller-local endpoint controls, bus acceptance
controls and split SPI IRQ guests. The frozen original reader SHA256 is
`c9a175e3c1b432a523b44f9f80868bb0da5df74fc983ab3e174565fadc71d461`;
the final admission reader is
`8cdf00fa8767ed09b4376f908f6c69dd0ebf9cb7d0485b9403dae34f8a25899c`.
They bind source/base/tree/PR/run/job/attempt/step and actual checkout separately.

The guests program their own descriptors, source bytes, clocks, pads, NVIC and
DMAC registers without software-trigger writes or host-injected requests/IRQs.
Six channels and19 modes exercise exact transmission, masks, disabled gates,
partial-transfer clock pauses, reset/restart and real bus faults. Feature-on
adds batch64/1024 to forced-walk/batch1 and scheduler/batch1 controls. Host
configuration and invalid descriptor controls remain separately labelled.

## Preserved failures and concurrent landing drift

The first candidate's generated C6 model digest was stale; its failed
[run38364110610](https://github.com/CrispStrobe/labwired-core/actions/runs/38364110610)
is preserved. The digest-only repair passed at sourcef96f1519 in
[run38368419566](https://github.com/CrispStrobe/labwired-core/actions/runs/38368419566).
Main then advanced through PR142's CPU block-payload optimization, so that old
pass was not reused as new-base qualification. The normally merged candidate
above passed both fresh profiles and every enabled check.

PR216 landed as
[`709eb97f259434b736623744d7fa18f7fde1bd77`](https://github.com/CrispStrobe/labwired-core/commit/709eb97f259434b736623744d7fa18f7fde1bd77).
However, PR215's performance-harness repair advanced main immediately before
the merge. Actual landed parents are
`80b4623403e810940ea035af46ca47587da1c81d` and the reviewed source above;
actual landed tree is `5952e5a7783d6bd16f5b81f787d7e7bc6453baa8`.
The frozen landed admission **failed tree/parent equality**. That failure is
retained; this receipt does not assert reviewed/tested/landed tree equality.
The difference is limited to `scripts/perf/baselines.json`,
`scripts/perf/board_rtx.py` and `scripts/perf/test_board_rtx.py`; the DMA
implementation, guest tests and workflow are unchanged. This identity check
does not substitute for qualification of the actual refreshed source.

## Required follow-up before completion-order integration

Qualify the actual refreshed base through both existing PR guest profiles and
the complete enabled check census. Preserve the old originals; bind fresh
results to their own source/base/tree and actual checkout. Record any further
merge drift explicitly, rather than weakening an admission reader or erasing
failures. The [DMA engineering contract](../../engineering/samd-dmac-byte-foundation.md)
remains authoritative for final DATA/count/writeback/TC, wire/TXC, CS and
callback ordering. No app pin, hardware capture, threshold or admission policy
was changed by the TX DMA lane.
