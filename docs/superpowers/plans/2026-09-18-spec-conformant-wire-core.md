# Spec-conformant wire, labwired-core slice — implementation plan

> **For agentic workers:** execute task by task, TDD, one commit per task. Steps use `- [ ]`.

**Goal:** the LabWired native IO-Link master model, the iolink examples and the on-wire CI speak the
IO-Link V1.1.5 wire (spec checksum, reply layout, ISDU framing) so that the simulated station certifies
a conformant protocol instead of the co-designed one.

**Architecture:** `iolink_master.rs` (pure-Rust master model used by `iolink-dido` and the multiport
tests) gets the spec checksum and reply decoding; ISDU/event encoders in that file follow C3/C4. The
`iolink-native` feature wraps the real `iolinki-master` C code, so it only needs the refreshed vendored
sources and the submodule pin. Firmware examples are rebuilt from the new device/master trees.

**Spec:** `/home/andrii/projects/iolinki-wt-wire/docs/superpowers/specs/2026-09-18-spec-conformant-wire-design.md`
(C1..C6 normative). Extracts: `/tmp/claude-1000/-home-andrii/7d9ab1e8-21ab-4fb5-bb9c-6cc409879441/scratchpad/spec_extract.md`, `.../sec_736.md`, `.../sec_738.md`.

## Global constraints

- Shared 14 GB machine: `export CARGO_TARGET_DIR=/home/andrii/projects/labwired-wt/cargo-iolink-wire
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` before every cargo command, `-j 2`, one cargo
  command at a time. A first test build can take 40 minutes.
- No engine-path change; `Machine::advance` untouched. No new dependencies.
- `cargo clippy -p labwired-core --lib --tests --no-deps -j 2 -- -D warnings`, `cargo fmt --all`.
- Test vectors from the oracle below, never from the Rust output.
- Commit messages: conventional, no AI/assistant mention, no trailers, no emoji.

## Oracle

```python
def ck6(octets):
    c = 0x52
    for o in octets: c ^= o
    b = [(c >> i) & 1 for i in range(8)]
    return ((b[7]^b[5]^b[3]^b[1])<<5)|((b[6]^b[4]^b[2]^b[0])<<4)|((b[7]^b[6])<<3)|((b[5]^b[4])<<2)|((b[3]^b[2])<<1)|(b[1]^b[0])
```
Master frames: `[0x00,0x00]`→CKT `0x2D`; `[0xA2,0x00]`→`0x00`; `[0x20,0x00,0x99]`→`0x06`;
TYPE_1 write `[0x00,0x40,0xA5,0x5A]`→`0x75`; TYPE_2 read `[0x80,0x80]`→`0xAD`.
Replies `[data...] CKS`: OD `0x10` flags 0 → `10 39`; PD `0xA5` valid, no event → `A5 22`;
PD `0xA5` + Event flag → `A5 8A`; PD `0xA5` invalid → `A5 62`.

---

### Task 1: checksum and reply layout in the pure-Rust master model

**Files:** `crates/core/src/peripherals/components/iolink_master.rs` (`crc6` → `checksum6`,
`encode_type0`, `encode_type1_cycle`, `decode_operate`, tests at the bottom), any other caller of
`crc6` (`grep -rn crc6 crates`).
- [ ] Tests: `checksum6` vectors above; `encode_type0(0xA2) == [0xA2, 0x00]`; `encode_type0(0x00) ==
  [0x00, 0x2D]`; the DeviceOperate write `[0x20, 0x06, 0x99]`; `decode_operate(&[0xA5, 0x22], 1, 0)`
  → pd `[0xA5]`, `pd_valid`, `checksum_ok`, no event; `[0xA5, 0x8A]` → event; `[0xA5, 0x62]` → invalid.
  `decode_operate` takes the reply as `[PD][OD] CKS` (no status octet): `pd_valid = cks & 0x40 == 0`,
  `event_present = cks & 0x80 != 0`, checksum over the reply with CKS bits 0-5 zeroed.
- [ ] Implement; `cargo test -p labwired-core --lib iolink -j 2` green. Commit
  `fix(iolink): spec A.1.6 checksum and A.1.5 reply layout in the native master model`.

### Task 2: ISDU and event encoders in the pure-Rust model

**Files:** `crates/core/src/peripherals/components/iolink_master.rs` (wherever the model reads
vendor name / events / detailed status; `grep -n "0x0010\|isdu\|0x1C\|event" ` to find them).
- [ ] If the model issues ISDU reads: encode per C3 (I-Service 0x9/0xA/0xB by Table A.15, Length =
  total octets, CHKPDU; FlowCTRL START then COUNT in the MC address on channel 0x60; poll with read
  START, accept `0x01` Busy; IDLE at the end). Test: read of index 0x10 emits ISDU octets `93 10 83`
  spread over TYPE_0 OD messages with MCs `0x70, 0x61` (W, ISDU, START / COUNT 1) — derive the exact
  MC sequence from A.1.2 (RW bit 7 = 0 write, channel 3 = 0x60, address = FlowCTRL) and assert it.
- [ ] Events: on the CKS Event flag read the event memory via `MC = 0xC0` (R, DIAGNOSIS, addr 0), then
  slots, then write `MC = 0x40` addr 0 to confirm (Table 59). Keep the existing `MASTER EVENT` log line.
- [ ] `cargo test -p labwired-core --lib iolink -j 2` green. Commit
  `fix(iolink): spec ISDU framing and diagnosis-channel events in the native master model`.

### Task 3: submodule pin, vendored master, firmware rebuild, on-wire harness

**Files:** `third_party/iolinki` (submodule → the merged device commit on `develop`; until it merges,
point at the branch head of `/home/andrii/projects/iolinki-wt-wire` and say so in the commit body),
`third_party/iolinki-master/` (rsync from `/home/andrii/projects/iolinki-master-wt-wire` excluding
`.git`, `build*`, `docs/superpowers`), `examples/iolink-dido/firmware`, `examples/iolink-station/*`
(rebuild ELFs with `arm-none-eabi-gcc`; `STM32CUBE_L4_DIR=$HOME/projects/STM32CubeL4`), the
`phy_labwired.c` adapters only if the PHY API changed.
- [ ] `cargo test --release -p labwired-core --test world_multichip -- --nocapture` and
  `--test world_station_services` reach OPERATE, read VendorName, DS round trip; `labwired test --script
  examples/iolink-dido/test.yaml` passes; `cargo test -p labwired-core --features iolink-native --test
  iolink_native_master -j 2` green.
- [ ] Commit `chore(iolink): pin iolinki and iolinki-master to the spec-conformant wire`.

### Task 4: gates

- [ ] `python3 scripts/generate_validation_status.py --check --drift` exits 0 (if it exists);
  clippy + fmt; `scripts/example_smokes.sh` for the iolink examples. Write "## Status" into this plan
  file (per task: sha, tests, result) and commit `docs(plan): status of the spec-conformant wire slice`.

## Status

Run 2026-09-18: Task 1, Task 2 and the clippy/fmt part of Task 4 only. Task 3
(submodule pin, vendored master, firmware rebuild, on-wire harness) is deferred
to a later run as instructed.

### Task 1 — checksum and reply layout

- Commit: `16eb756b6` `fix(iolink): spec A.1.6 checksum and A.1.5 reply layout in the native master model`.
- `crates/core/src/peripherals/components/iolink_master.rs`: `crc6` deleted and
  replaced by `checksum6` (A.1.6 XOR seed `0x52` + equations A.1); `encode_type0`
  / `encode_type1_cycle` build CKT; `decode_operate` reads `[PD_in..., OD..., CKS]`
  with no status octet, `pd_valid = CKS&0x40 == 0`, `event_present = CKS&0x80 != 0`,
  checksum over the reply with CKS bits 0-5 zeroed.
- Vectors from the plan's Python oracle (computed with `python3`): `(00,00)->2D`,
  `(A2,00)->00`, `(20,00,99)->06`, TYPE_1 `(00,40,A5,5A)->35`, TYPE_2 `(80,80)->2D`;
  replies `10 39`, `A5 22`, `A5 8A`, `A5 7A`.
- **Deviation from the plan text:** the plan lists the invalid-PD reply as
  `A5 62`. That value is inconsistent with the normative C1 (it omits CKS bit 6
  from the checked message). C1 says only bits 0-5 are zeroed, so
  `CKS = 0x40 | ck6([0xA5, 0x40]) = 0x7A`; the Event vector `A5 8A` only works
  the same way. The tests use the C1-consistent `A5 7A` and document the
  derivation.
- Test: `cargo test -p labwired-core --lib iolink -j 2` → 18 passed, 0 failed.

### Task 2 — ISDU framing and diagnosis-channel events

- Commit: `d20bc8574` `fix(iolink): spec ISDU framing and diagnosis-channel events in the native master model`.
- Added A.1.2 MC builder (`mc`), channel/FlowCTRL constants (Table A.1/A.52),
  `isdu_read_request` (Table A.13/A.15 index formats, CHKPDU A.5.6),
  `isdu_flowctrl_segments` (START then COUNT, wrap 15→0),
  `encode_type0_write` (`MC CKT OD CK`), and the Table 58/59 diagnosis-channel
  event readout (`diagnosis_read_mc` 0xC0, `diagnosis_write_mc` 0x40,
  `event_readout_plan`/`event_readout_mcs`).
- The pure model does not issue ISDU reads, so the ISDU encoders are
  unit-tested now and marked `#[allow(dead_code)]` for the follow-on
  ISDU/parameter-exchange scheduling task. The event readout **is** wired: a
  rising CKS Event flag on a cyclic reply queues the Table 59 readout
  (StatusCode 0xC0, slots 0xC1..=0xD2, confirmation write 0x40) as
  `IolinkFrameKind::EventReadout` type-0 frames before cyclic traffic resumes;
  the `MASTER EVENT` log line is kept. Decoding of cyclic replies is now gated
  on the in-flight frame being `Cyclic`, so diagnosis replies are not misparsed.
- Vectors: read index 0x10 → `93 10 83`; index 0x10 sub 1 → `A4 10 01 B5`;
  index 0x0123 sub 4 → `B5 01 23 04 93`; ISDU write MCs `0x70, 0x61, 0x62`;
  read/IDLE/ABORT MCs `0xF0, 0xF1, 0xFF`; event readout MCs `0xC0..0xD2, 0x40`.
- Test: `cargo test -p labwired-core --lib iolink -j 2` → 23 passed, 0 failed.

### Task 4 — gates (clippy/fmt part only)

- `cargo clippy -p labwired-core --lib --tests --no-deps -j 2 -- -D warnings`:
  clean (`Finished` with no warnings/errors).
- `cargo fmt --all -- --check`: exit 0.
- `python3 scripts/generate_validation_status.py --check --drift` and
  `scripts/example_smokes.sh` were not run: Task 4's remaining gates are
  coupled to the on-wire harness (Task 3, deferred).

### Environment / assumptions

- All cargo commands run with `CARGO_TARGET_DIR=/home/andrii/projects/labwired-wt/cargo-iolink-wire`,
  `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `-j 2`.
- On-wire and `iolink-native` tests are expected red until Task 3 lands the
  device submodule bump and the vendored master; none were run.

