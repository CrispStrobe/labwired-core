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
