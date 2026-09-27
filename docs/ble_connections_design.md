# BLE connections and GATT between simulated nodes — design

Status: in progress (branch `feat/ble-gatt-connections`). This page is the
design; the "Measured" section at the end records what actually runs.

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
    nodes: [peripheral]
    config:
      target_name: ESP32
      script:
        - connect
        - discover
        - read: beb5483e-36e1-4688-b7f5-ea07361b26a8
        - write: { uuid: beb5483e-36e1-4688-b7f5-ea07361b26a8, hex: "68656c6c6f" }
        - subscribe: beb5483e-36e1-4688-b7f5-ea07361b26a8
        - wait_ms: 200
        - disconnect
```

It records a transcript (connection state, every ATT PDU in both directions)
that the CLI, the tests, the wasm API and the playground BLE tab all read.

## Browser

The multi-node world runs in wasm through `WasmWorld`. It gains a time-quantum
stepper and `ble_trace()` / `ble_central_log()` accessors, and a wasm-path test
drives the two-node GATT exchange through the same `WasmWorld` code the browser
uses. The playground change (a BLE tab showing connection state and GATT
traffic) lands in a follow-up app PR pinned to this core commit.

## Cross-family

nRF52 peripheral to ESP32-C3 central needs the nRF52 `RADIO` model to follow
real timing and to meet the RW-BLE air (the two airs carry different frame
types today; see `ble_air.rs`). Out of scope for this change; recorded as the
next step.

## Measured

(filled in as the work lands)
