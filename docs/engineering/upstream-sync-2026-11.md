# Upstream re-sync, 2026-11-07/08

The fork `CrispStrobe/labwired-core` was rebuilt on top of upstream
`w1ne/labwired-core` main. Before the rebuild, GitHub reported the fork as
"922 commits ahead, 891 behind". Those counts were misleading: upstream had
re-implemented or ported a large share of our work under its own commits
(`up/crisp-*`, `port/samd51-*`, `port/esp32c3-s3-ledc-pads`, ...), so patch-ids
and ahead/behind counts no longer matched content.

## Method

1. **Backup.** The old main is kept as
   [`backup/pre-upstream-sync-20261107`](https://github.com/CrispStrobe/labwired-core/tree/backup/pre-upstream-sync-20261107).
2. **Content scoring.** Every non-merge fork commit (641) was scored by the
   share of its still-surviving added lines that already exist in upstream's
   version of the same file. About 400 were fully upstream or superseded; about
   240 were partly or not upstream.
3. **Reference merge.** Upstream was merged into the old main once (103
   conflicted files) and every conflict resolved by hand. Rule: where upstream
   shipped the same feature, upstream's implementation wins; where only the
   fork has it, the fork's code is ported onto upstream's structure. The result
   was built, tested (core lib, integration tests, clippy `-D warnings`, fmt)
   and kept as the reference tree on
   [`sync/merge-20261107`](https://github.com/CrispStrobe/labwired-core/tree/sync/merge-20261107).
4. **Clean history.** `main` was rebuilt as upstream main plus only the
   not-yet-upstream fork commits, replayed in order, with conflicts resolved
   to the reference tree and one final reconcile commit, so the tree equals the
   tested reference. This needed one owner-approved force-push.
5. Later upstream commits are merged normally (no further force-push).

Result on 2026-11-08: 168 ahead, 0 behind upstream.

## Decisions

Upstream's implementation kept (the fork's version dropped):

- ESP32-C3/S3 LEDC duty/frequency on the routed pad (upstream ported it from
  us as PR #1509), ESP32-S3 GPIO bank 1, `esp32c3/ledc.rs`, the ESP32 classic
  system wiring, `bus/from_config.rs`.
- The CI job split in `core-ci.yml`, `core-perf.yml`, `core-qemu-arm.yml`
  (the fork's `samd-native-profile` dispatch job and our perf baselines for
  `samd51-no-asf-simulation` were re-added).
- SAMD51 TC/EIC/OSCCTRL/DAC/ADC/PAC/TCC, SERCOM I2C/USART/SPI timing, USB
  device and DMAC board models (upstream ported them from us as PRs
  #1510/#1512/#1516), including upstream's DMAC NOACT completion and the DAC
  reserved byte that reads as zero.
- `declarative_coverage.rs` stays deleted (upstream removed it on purpose).

Fork-only work kept: see "Where we still differ" in the [README](../../README.md).

## Fixes found after the sync

- ESP32-S3: `physical_pin()` so GPIO22-25 (not bonded out) are refused while
  bank 1 (GPIO32+) works.
- ESP32-S3: both the fork and upstream registered a `gpio_matrix_out` ROM
  thunk at `0x4000_1aa0` (debug-assert panic on boot). The fork's copy is gone.
- ESP32 classic LEDC: a `CONF0` write that keeps `TIMER_SEL` (ESP-IDF's
  `ledc_update_duty` after `DUTY_START`) no longer applies a latched duty
  mid-period. Caught by our `crates/cli/tests/esp32_ledc_pad.rs`.
- CI lists (`docs-runnable-chips.json`, `baselines.json`, `core-perf.yml`) had
  lost the fork-only `samd51-no-asf-simulation` chip.

## Performance

The fork's monomorphised SAMD checked-chunk split
(`advance_inner::<Ordinary>` / `advance_inner::<CheckedChunks>`) costs about
19 host instructions per single step on Cortex-M boards on top of upstream's
current loop (about 2 % in step mode; batch mode unchanged). Probed in CI
against upstream on the same runner:

| variant | stm32f103 Ir/step |
|---|---|
| upstream main | 932.9 |
| fork main after sync | ~952 |
| upstream machine loop, fork elsewhere | 935.7 |
| runtime checks instead of generics | 961.9 |
| generic split, `#[inline(never)]` loop | ~990 |

Only removing the SAMD checked chunks reaches upstream's cost, so the split is
kept as a deliberate trade-off. The parked-WFI test moved into a cold helper
(about 3 Ir/step). Step baselines of `r7fa6m5bh`, `r7fa8m1ah`, `stm32l552` were
re-locked from CI numbers; `stm32g0b1re` was already stale on upstream main
(+4.8 % there); `samd51-no-asf-simulation` locked in its faster numbers. See
[Core Perf fork rules](../performance/2026-10-15-core-perf-fork-rules.md).

## Drift acknowledgements

Nine hardware-captured boards watch model files the fork changes
(`nrf52840`, `seeed-xiao-nrf52840-sense`, `stm32h563`, `esp32c3`,
`nucleo-l476rg`, `nucleo-l073rz`, `stm32f103`, `stm32f407`, `esp32s3`). The
owner approved new acks dated 2026-11-08 (30-day TTL, expires 2026-12-08).
These are simulator-drift acknowledgements, not silicon captures; live
re-capture is still pending.

## How to repeat this

The procedure is recorded as the `fork-upstream-resync` agent skill. In short:
back up main, score fork commits by content, build and test one reference
merge, replay only the unmerged commits onto upstream with conflicts resolved
to the reference, check tree equality, then push. Keep the fork's own CI
entries (perf baselines, runnable-chip lists, bus-proof rows) in mind: they
are the usual casualties of taking upstream's copy of a shared file.
