# SAMD software-profile handoff — updated 2026-10-28

**Core work for this lane is complete and merged.** The downstream engine
release (bw-board) and Lite adoption are still open; see the
[producer handoff](https://github.com/CrispStrobe/bw-board/blob/master/docs/SAMD-ENGINE-RELEASE-HANDOFF.md)
and the [Lite handoff](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/SAMD-NATIVE-GAME-HANDOFF.md).
Do not open a new Core runtime PR for this lane unless one of the open tasks
below proves a Core defect.

## What landed

[PR262](https://github.com/CrispStrobe/labwired-core/pull/262) merged normally on
its matched head `1f1beefcdf5e3720e98db8be75b0d64d6efe0bec` as
`3b655aef4085082152129c84849d4e2ad5004a7f`. The landed tree equals the tree that
CI tested. Every enabled exact-head check passed; only declared jobs were skipped.
This commit is the Core pin for the downstream engine.

- **Hot-path isolation.** Boards without the SAMD software profile run their own
  copy of the advance loop with no SAMD hooks (`advance_inner::<Ordinary>`); the
  SAMD profile runs `advance_inner::<CheckedChunks>`. The choice is fixed when the
  bus is built and read once per advance. Earlier variants (cached flags, forced
  inlining removal, const-specialised helpers) all cost about 4–8% per step on the
  fleet and were rejected by measurement.
- **Fleet throughput** ([run](https://github.com/CrispStrobe/labwired-core/actions/runs/40578811706)):
  no regression rows; for example STM32F407 929.3 vs 915.7 Ir/step (+1.5%),
  ATSAMD21 +1.6%, PyBadge +2.5%, ESP32-S3 +0.9%, all within the unchanged 3% bound.
- **Selected SAMD profile** ([run](https://github.com/CrispStrobe/labwired-core/actions/runs/40599699118)):
  20 controls and the external-runner mutation passed; 1644.5 Ir/step, 9.3 Ir per
  batched step (three batch slopes ≈9.308), native inactive-fixture median 7.59×
  real time. Independently re-read from the original samples.
- **Source-built CLI game** ([run](https://github.com/CrispStrobe/labwired-core/actions/runs/40578811875)):
  both scheduler profiles passed at 70 million steps.
- **Drift ACK.** The seven entries `nrf52840`, `seeed-xiao-nrf52840-sense`,
  `stm32h563`, `nucleo-l476rg`, `nucleo-l073rz`, `stm32f103`, `stm32f407` were
  re-acknowledged by the operator on 2026-10-27 for the digests at the landed
  source. Expiry **2026-11-12**, captures and watch lists unchanged. This is
  simulator-drift acknowledgement only, not a silicon re-capture.

These results qualify the landed commit only. They are not active-game,
browser, installed-app or physical-hardware performance claims.

## Retained failures (do not delete or relabel)

- Fleet throughput failed for every earlier runtime variant, e.g.
  [first](https://github.com/CrispStrobe/labwired-core/actions/runs/40443757411),
  [const-specialised](https://github.com/CrispStrobe/labwired-core/actions/runs/40451364784),
  [a92](https://github.com/CrispStrobe/labwired-core/actions/runs/40459382058),
  [99404e4](https://github.com/CrispStrobe/labwired-core/actions/runs/40471913258).
- Rust Core CI failed on the drift check at `99404e4` until the ACK was applied.
  That was the intended human gate, not a test failure.

## Open tasks

1. **Drift ACK expiry — due before 2026-11-12.** The ACK above expires on
   2026-11-12. Before then either (a) re-capture the seven boards on real
   hardware (a separately authorized hardware task; never flash without explicit
   operator approval), or (b) ask the operator for an explicit new dated ACK of the
   then-current digests (`python3 scripts/generate_validation_status.py --digests`).
   Never bulk-stamp, never extend the expiry without that approval. Any later edit
   to a watched model file (for example `crates/core/src/cpu/cortex_m.rs`) changes
   the digests again and needs a new approval.
2. **Only if a downstream task proves a Core defect**, fix it on a new branch from
   `main` with a new PR, and require the same gates: fleet throughput, selected
   profile dispatch of `core-perf.yml` with `samd_profile_only=true` (once, on the
   qualification carrier branch advanced to the exact head), CLI game, all enabled
   exact-head checks, then a matched-head merge.
   The WASM motion and radio-pair problems seen downstream were measured and are
   **not** Core defects (same-runner A/B: old engine 1.363× vs new 1.340× median).

Separate scope, not part of this lane: silicon re-capture, normal native SAMD GUI
routing, active-game real-time speed, physical panel/electrical accuracy.
Retain the PyBadge adaptation credit (owned by CrispStrobe) separately from the
upstream SAMD51 foundation notice.
