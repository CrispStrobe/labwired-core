# SAMD software-profile handoff — 2026-10-27

**Unfinished: do not merge or release the current runtime candidate.** Continue
[PR262](https://github.com/CrispStrobe/labwired-core/pull/262); do not start a
second implementation. Runtime candidate `a92a399fdd861e30e58ca316137b54687629eaa4`
restores the ordinary CPU-window/progress structures, keeps one authoritative
advance loop and isolates checked SAMD work behind cached eligibility and
private per-window markers. Refresh the PR head: later documentation integration
may advance it. This candidate has static review/format/workflow checks only.

## Completed evidence and its boundary

[Core PR263](https://github.com/CrispStrobe/labwired-core/pull/263) and CODAL PRs91–98
landed after qualification. CODAL had no open PRs at this checkpoint.

Earlier runtime `7f47d2fb16eba0d31548f4ae52a13756b09f9364` passed
[20 controls, the external-runner mutation and selected measurements](https://github.com/CrispStrobe/labwired-core/actions/runs/40449039908).
An independent reader reconstructed eight original Callgrind samples: 1667.2
Ir/step, 9.2 Ir/batched step. Three native inactive-fixture runs had a 4.64×
real-time median. This is not active-game, browser or physical performance.
Its [actual source-built CLI game](https://github.com/CrispStrobe/labwired-core/actions/runs/40448925872)
passed both scheduler profiles at 70 million steps; the tested merge tree equalled
the reviewed tree. Package/source/compiler/report joins were independently read.
These receipts qualify their original source, **not the new candidate**.

CrispStrobe authored the [PyBadge adaptation](https://github.com/CrispStrobe/labwired-core/commit/8c894bdbe591c74d135811d6cc33574f8b4755cf).
Retain its credit separately from the upstream SAMD51 foundation's notice.
Do not attribute the owned adaptation to its later upstream import.

## Retained failures

The [first fleet run](https://github.com/CrispStrobe/labwired-core/actions/runs/40443757411)
reported about 4% extra step cost on existing boards. The
[const-specialized integration](https://github.com/CrispStrobe/labwired-core/actions/runs/40451364784)
also failed, including 7.9% on ATSAMD21. Passing selected-profile measurements do
not establish fleet recovery. Original raw profiles show changed compiler call
boundaries and about 42 extra Ir/step; an exact spill/prologue cause is unproven.
Never raise old baselines to accept this regression.

Three direct unit-test callers also failed compilation after the const API
change. Their assertions were preserved; the new candidate restores plain calls.
Search tracked Git objects as well as present files in sparse checkouts.

## Ordered next steps

1. Fetch and inspect PR262's exact current head and all enabled checks. Preserve
   other owners' main changes. Read the
   [candidate contract](https://github.com/CrispStrobe/labwired-core/blob/a92a399fdd861e30e58ca316137b54687629eaa4/docs/engineering/samd-native-simulation-bundle.md).
   Review the mailbox lifecycle, strict zero-prefix fallback, external-runner
   refusal and unchanged admission guards before final qualification.
2. Require fresh hosted 20 controls, detected-and-restored external-runner mutation,
   actual CLI game in both profiles, scheduler/WFI/SysTick, firmware survival,
   actual XIAO source guest and full-fleet recovery. A green no-op or empty test
   does not qualify. Engine builds and guest runs belong in hosted CI.
3. No selected manual campaign has been dispatched for this latest runtime.
   Once final, normally advance the existing qualification carrier to that
   exact head and dispatch `core-perf.yml` **once**, `samd_profile_only=true`.
   Bind a new independent reader to source/run. Require eight original samples,
   service grid1 and actual retired chunk>=1024 in every low/high batch repeat.
   Preserve 1×, 3% relative cost, 0.5 Ir noise and every earlier baseline.
   Admit metadata before acquiring originals once; consult the ignored local
   handoff before downloading anything already retained.
4. Recompute `python3 scripts/generate_validation_status.py --digests`.
   Explicit human ACK is still needed for seven manifest entries: `nrf52840`,
   `seeed-xiao-nrf52840-sense`, `stm32h563`, `nucleo-l476rg`, `nucleo-l073rz`,
   `stm32f103`, `stm32f407`. After affected qualification, obtain approval of
   exact digests, record the actual review date, preserve the explicit
   **2026-11-12** expiry/captures/watch lists, and regenerate status. No
   unrestricted bulk stamping. No ACK was applied in this session.
5. Pass every enabled final-head check; perform the normal matched-head merge
   and verify parents/tree. Then follow the
   [producer handoff](https://github.com/CrispStrobe/bw-board/blob/master/docs/SAMD-ENGINE-RELEASE-HANDOFF.md)
   and [Lite handoff](https://github.com/CrispStrobe/brickwright-lite/blob/main/docs/SAMD-NATIVE-GAME-HANDOFF.md).

Silicon re-capture remains a separately authorized hardware task. The nRF/XIAO
entries share historical evidence; do not claim seven independent physical
validations. Normal native SAMD GUI routing, active-game real-time speed and
physical panel/electrical accuracy remain separate work. Source-built firmware
survival defaults idle handling off; direct WFI controls cover the changed branch.
