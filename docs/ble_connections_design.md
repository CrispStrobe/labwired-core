# BLE connections and GATT between simulated nodes — design

Status: ESP32-C3 connections + GATT land with this change (two C3s, and a C3
with a scripted central, in native and in the browser's wasm build). nRF52 is
the next step. The "Measured" section at the end records what runs.

## The problem

Every BLE radio in LabWired was advertising-only. `ble_air.rs`,
`virtual_ble.rs`, `nrf52/radio.rs` and `esp32c3/bt.rs` all said so: no
connection state machine, no data channels, no acknowledgement. So a user could
test a manufacturer-data broadcast, but not the thing a real BLE product does:
a central connects, discovers a GATT service, reads, writes and subscribes.

## Where the link layer lives, per family

The layer to model is the one the firmware does NOT contain.

| Family | Who runs the link layer | What LabWired must model |
|---|---|---|
| ESP32-C3 (Arduino / ESP-IDF, Bluedroid or NimBLE) | The mask-ROM RW-BLE link layer (`r_lld_*`, `r_llc_*`) plus ESP-IDF's IRAM patches. The host stack talks to it over VHCI, entirely in firmware. | The **RW-BLE baseband core** under the ROM: exchange table, control structures, TX/RX descriptors, and the radio event. The ROM already runs the connection state machine; it only needs the hardware to do what hardware does. |
| nRF52 (SoftDevice / Zephyr controller) | Firmware drives `RADIO` directly. | A `RADIO` with real timing (T_IFS, anchors). Not in this change; see "Cross-family" below. |

For the ESP32-C3 we deliberately do **not** model the controller at the HCI
level. The twin already boots the genuine mask ROM and the genuine ESP-IDF
controller library, and the advertising/scanning path already runs through the
real ROM link layer against a register-level model of the RW-BLE core (see
`esp32c3/bt.rs`). An HCI-level stand-in would replace real code with a
LabWired-written controller, i.e. a thunk. Extending the baseband model keeps
every byte of link-layer and host behaviour the firmware's own.

## The RW-BLE hardware contract we implement

Field layouts come from three independent sources that agree with each other:
the RW-BLE register headers for the same IP generation (90-byte control
structure, 14-byte TX descriptor, 20-byte RX descriptor — the exact sizes the
C3 ROM uses), the reference RW-BLE link-layer source (`lld_con.c`,
`lld_adv.c`, `lld_init.c`) for what software expects hardware to write, and the
C3 mask ROM disassembly (`esp32c3_rev3_rom.elf`) wherever the two could differ.

Control-structure formats (`CS+0x00` bits[4:0]):

| format | activity | hardware behaviour modelled |
|---|---|---|
| `0x02` | master connection event | TX at anchor, RX response at T_IFS, continue while MD |
| `0x03` | slave connection event | RX within the window, TX response at T_IFS, continue while MD |
| `0x04` | connectable undirected advertising | TX `ADV_IND`, then listen T_IFS for `SCAN_REQ` / `CONNECT_IND` |
| `0x08` | passive scan | unchanged |
| `0x09` | active scan | as `0x08`, plus `SCAN_REQ` / `SCAN_RSP` |
| `0x0E` | initiator | RX `ADV_IND` from the target, TX `CONNECT_IND` at T_IFS |

Connection-event hardware duties (all of them are what `lld_con_rx` /
`lld_con_tx` read back):

* data channel: `CS+0x16` `CH_IDX` is the *unmapped* channel software computed
  (CSA #1), remapped through `LLCHMAP` (`CS+0x22..0x27`); with `HOP_SEL`
  set the core runs CSA #2 from `EVTCNT` (`CS+0x50`) and the access address;
* acknowledgement: `SN`/`NESN` live in `CS+0x18` (`TXRXCNTL`), the core
  updates them; a TX descriptor (`TXDONE` clear) is sent until the peer's NESN
  acknowledges it, then `TXDONE` is set and `CS+0x1C` moves to `NEXTPTR`; with
  nothing queued the core sends an empty PDU;
* every received packet — empty ones included, which is what reloads the
  supervision timer — goes into an RX descriptor with the connection status
  word (`SYNC/CRC/LEN/MIC/SN/NESN` errors), the header, the RX sync time
  (`+0x08` CLKN, `+0x0C` fine count) and the link label;
* `CS+0x56` (`TXRXDESCCNT`) counts the descriptors used per event and
  `TXRXCNTL.LASTEMPTY` says whether the last packet sent was empty — the
  ESP-IDF build of `lld_con_frm_isr` reads both.

## Time on the air

The old air was timeless: a frame was "on the air" until a receiver's cursor
passed it. A connection needs a request and a response 150 µs apart, so frames
now carry the transmitter's **air time** (ns since the world started) and a
receiver accepts only frames that start inside its receive window. A node
decides "nothing came" only once its own clock is past the window plus a lag
margin, and the world steps its nodes in **time quanta** (not instruction
counts), so no node can run further ahead of another than one quantum. A
responder stamps its reply at `request_end + 150 µs` exactly, even if it
notices the request a few µs late; the exchange is therefore
deterministic and independent of how the host interleaves the nodes.

Advertising and passive scanning keep their existing (backlog) semantics, so
BLE Pong and the other advertising gates see no change.

## The scripted central ("phone")

`ScriptedCentral` is a LabWired-defined BLE central that lives on the same air
— the same kind of device as the CAN UDS tester: not silicon, a test
instrument. It implements a real link-layer master (CONNECT_IND, CSA #1,
SN/NESN, the LL control procedures a peripheral starts), L2CAP, and an ATT
client (discover services, characteristics and descriptors, read, write,
subscribe, notifications/indications). A world config declares it with a
script:

```yaml
interconnects:
  - type: ble_central
    nodes: [peripheral]          # the nodes whose air it joins
    config:
      id: phone                  # optional; default phone0, phone1, ...
      target_name: ESP32         # or target_service / target_address
      # interval: 24             # 1.25 ms units (default 30 ms)
      # supervision_timeout: 500 # 10 ms units (default 5 s)
      script:
        - connect
        - discover
        - read: beb5483e-36e1-4688-b7f5-ea07361b26a8
        - write: { uuid: beb5483e-36e1-4688-b7f5-ea07361b26a8, hex: "68656c6c6f" }  # or text:
        - subscribe: beb5483e-36e1-4688-b7f5-ea07361b26a8
        - wait_notify: { count: 3, timeout_ms: 2000 }
        - wait_ms: 200
        - disconnect
```

Two MCUs with no phone use `type: ble_air`, `nodes: [a, b]` (no config).
Both interconnects give the listed nodes ONE private BLE medium (no leak to
another world in the same process) and switch the world to time lockstep.

It records a transcript (connection state, every ATT PDU in both directions)
that the CLI, the tests, the wasm API and the playground BLE tab all read.

## Browser

The multi-node world runs in wasm through `WasmWorld`. New bindings:

| binding | what |
|---|---|
| `WasmWorld.register_esp32c3_rom(irom, drom)` | the page hands over the C3 mask ROM it fetched (`/wasm/esp32c3_rom.bin`, `esp32c3_drom.bin`); a world builds C3 flash-image nodes from it |
| `ble_centrals()` | `[{ id, report }]`: state, peer, discovered services/characteristics, reads, acknowledged writes, the last 256 notifications + total count, connection events, transcript (last 4000 lines) |
| `ble_air_trace()` | the world's BLE air, most recent first (200 frames), each with `air_ns`, channel, access address and a decode (`ATT Read Request handle 0x002a`, `LL control 0x0c VERSION_IND`, ...) |
| `time_ns()` | the slowest node's simulated time |

`step_batch(n)` is unchanged: in a BLE world one round steps only the nodes
that are not ahead of the slowest by more than 10 µs, so the page's stepping
loop needs no change. `scripts/ci/wasm-ble-gatt-smoke.mjs` runs both scenarios
through a `wasm-pack --target nodejs` build of `crates/wasm`.

The playground change (a BLE tab showing connection state and GATT traffic)
lands in a follow-up app PR pinned to this core commit. Note for that PR: the
app vendors its own copy of `esp32c3_rom.bin`; it must take the fixed image
(see "ROM `.data_btdm`" below) or every ACL packet leaks a buffer in the
browser.

## Cross-family

nRF52 peripheral to ESP32-C3 central needs the nRF52 `RADIO` model to follow
real timing and to meet the RW-BLE air (the two airs carry different frame
types today; see `ble_air.rs`). Out of scope for this change; recorded as the
next step.

## Stepping: time lockstep

`World::step_all` used to step every node once per round. With idle
fast-forward a node can jump milliseconds in one step, so its peer would
decide "nothing came" before the node had transmitted. A world with a BLE
medium steps only the nodes whose simulated time (cycles / `cpu_hz`) is
within `BLE_LOCKSTEP_QUANTUM_NS` (10 µs) of the slowest node, then advances
the scripted centrals to that time. Worlds without a `ble_air` /
`ble_central` interconnect step exactly as before.

Each world also has its own eFuse-MAC fab (`FactoryMacAllocator`): its C3
dice are numbered in manifest order, so the same world built twice in one
process gets the same BLE addresses, and a transcript replays byte for byte.
The first two dice still get `...:04` and `...:05`, as from the process-wide
fab in a fresh process, so the existing two-node gate is unchanged.

## Three faults found on the way

1. **Acknowledgement across events.** A slave's reply is acknowledged by the
   master's first packet of the NEXT connection event. The model kept "what I
   sent last" per event, lost it at the event end, and toggled SN anyway: the
   same `VERSION_IND` went out every event with a fresh SN. The core now reads
   it where the hardware keeps it: `TXRXCNTL.LASTEMPTY` clear means the
   descriptor `CS+0x1C` still names is the unacknowledged one (the pointer
   moves only on an acknowledgement).
2. **ROM `.data_btdm`.** `btdm_controller_rom_data_init` copies 12 bytes from
   `*_data_start_btdm_rom` (0x4005966C) to `.data_btdm`; the vendored IROM had
   zeros there (the ELF gives the section only at its DRAM address). It holds
   `misc_msg_handler_tab`, the only handler of `MISC_FREE_EM_BUF` (0x0901),
   which the link layer sends for every acknowledged ACL packet. Symptom:
   `assert ke_task.c 157, param 00000901 00000004` on both nodes and GATT
   discovery stalling once the exchange-memory buffers ran out. The extractor
   now reconstructs the load image; the vendored `esp32c3_rom.bin` changes in
   those 12 bytes only, and the extraction cache key moved to `v2`.
3. **CSA #1 hop.** With `FH_EN` set, `HOPCNTL.CH_IDX` is the previous event's
   unmapped channel and the core adds `HOP_INT`. Measured from the ROM's own
   programming (`0x8707` at `EVTCNT` 1). Two C3s never hit it (they negotiate
   CSA #2); the phone uses CSA #1, like many real centrals.

## Measured

All on the stock Arduino-ESP32 examples (ESP-IDF v4.4.7), unmodified:
`BLE_notify` (GATT server, notifies a counter) and `BLE_client` (central).

| scenario | where | result |
|---|---|---|
| `BLE_client` C3 ↔ `BLE_notify` C3 | `World`, native | scan, connect, discover, read, write, subscribe, ≥ 5 notifications; identical serial on two runs |
| phone ↔ `BLE_notify` C3 | `World`, native | connect, discover (3 services, 5 characteristics, CCCDs), read, write-with-response, subscribe, 3 notifications, `LL_TERMINATE_IND`; identical transcript on two runs |
| phone ↔ `BLE_notify` C3 | `WasmWorld`, host | same, read back through the page's bindings |
| phone ↔ `BLE_notify` C3 | real wasm (Node 22) | same transcript as native, to the µs; 0.96 s simulated in ~65 s |
| `BLE_client` ↔ `BLE_notify` | real wasm (Node 22) | as native; 1.45 s simulated in ~300 s |

`BLE_notify` notifies every 3 ms (the sketch's own `delay(3)`), faster than a
30 ms connection interval can carry, so its serial shows Bluedroid's
`esp_ble_gatts_send_ notify: rc=-1` congestion errors. The sketch's comment
says the same happens on silicon.

Gates: `crates/core/tests/world_esp32c3_ble_gatt.rs` (both scenarios, run
twice for determinism), `crates/wasm/src/world.rs`
(`a_wasm_world_runs_a_scripted_phone_against_a_c3_gatt_server`),
`scripts/ci/wasm-ble-gatt-smoke.mjs`; all in the nightly
`esp32c3-ble-e2e` job with `LABWIRED_REQUIRE_C3_BLE=1`.

Not modelled: encryption/pairing (a link that starts `LL_ENC_REQ` is not
supported), 2M/Coded PHY, extended advertising, and on the phone's side:
data length extension and MTU exchange (it keeps 27-byte PDUs and a 23-byte
ATT MTU) and connection parameter updates.
