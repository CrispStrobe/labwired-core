# STM32H563 CAN record / replay, pause modes and failure scenarios

A `can-bridge` sits between FDCAN1 of the H563 UDS ECU
(`../h563-uds-ecu/firmware/h563_uds_ecu.elf`, UDSLib v2.0.0) and everything
outside the simulation: the scripted UDS tester, a replayed recording, or live
frames a host feeds in (a SocketCAN/WebSocket bridge, the browser playground).
It records every frame on the virtual time axis, says exactly what happens to
live frames while the simulation is paused, replays a recording against
virtual time, and injects faults on the CAN path.

## Files

| File | What it does |
| --- | --- |
| `system-record.yaml`, `record.yaml` | The full UDS session from `h563-uds-ecu`, with a bridge that records it. |
| `session.jsonl` | That recording (JSON lines, exact cycles). |
| `system-replay.yaml`, `replay.yaml` | No tester: the bridge injects the recorded requests at their cycles and compares every ECU answer with the recording. |
| `system-reset-fault.yaml`, `reset-fault.yaml` | Node reset in the middle of the 603-byte DID `0xF1A0` ISO-TP transfer; the tester times out, retries and recovers. |

```bash
labwired test --script examples/h563-can-replay/record.yaml --output-dir out
# out/can-bus.jsonl  (replay format)   out/can-bus.candump.log  (can-utils)
labwired test --script examples/h563-can-replay/replay.yaml --output-dir out
labwired test --script examples/h563-can-replay/reset-fault.yaml --output-dir out
```

`result.json` gets a `can_bridges` block per bridge: frames delivered and
transmitted, **every dropped frame** (cycle, host time, direction, ID, DLC,
data, reason), the capture queue, the replay verdict (`match` / `mismatch` /
`incomplete`, first mismatch, cycle skew), the faults that fired, and the
**failure timeline**: CAN frames, faults, the firmware's console lines and the
tester's progress (request, response, timeout, retry, done, failed) on one
cycle axis. The `can_bridge` assertion checks it:

```yaml
- can_bridge: { id: "bus", replay: match, max_dropped: 0, faults_fired: 1 }
```

## Pause modes (`pause_mode`)

The host tells the bridge when the simulation stops advancing virtual time
(paused, halted at a breakpoint) and when it resumes.

| Mode | Live frame while paused | Recorded |
| --- | --- | --- |
| `drop` (default) | discarded | each frame: cycle, host time, ID, DLC, data |
| `capture` | queued (bounded by `capture_capacity`, default 256); released in order after resume, one frame time apart at `bitrate` (default 500 kbit/s) | each captured frame; each overflow with the `overflow` policy (`drop_newest` / `drop_oldest`) that decided it |
| `replay` | ignored (live frames are ignored while running too) | each ignored frame |

**Buffering during a pause does not preserve real-time interaction with a
physical device.** A captured frame is delivered when the simulation resumes,
not when it arrived; the physical peer saw no reply during the pause and may
already have timed out or retried. CAPTURE keeps the frames, not the timing.
To make an interaction repeatable, record it and replay it: a replay runs on
virtual time, which does not move while paused, so it is identical every run.

## Recording formats

* JSON lines (native, exact): a header
  `{"labwired_can_recording":1,"clock_hz":250000000,"controller":"fdcan1"}`,
  then one frame per line
  `{"cycle":1601,"dir":"rx","id":2016,"ext":false,"fd":false,"brs":false,"rtr":false,"data":"0322f190"}`.
  `rx` = into the controller, `tx` = transmitted by the firmware.
* candump log: `(<seconds>) <iface> <ID>#<DATA>`, CAN-FD as
  `<ID>##<flags><DATA>`. The interface name carries the direction
  (`fdcan1-tx` or `tx` = transmitted by the firmware; anything else = into the
  controller). Seconds are virtual time since power-on; a log whose first
  timestamp is at or above 10^6 s (a real-bus capture with epoch times) is
  rebased to start at 0.

Give a replay bridge the recording inline (`recording:`) or as a file next to
the manifest (`recording_path:`). Recorded cycles count from power-on
(`replay_start: boot`, the default); `replay_start: attach` injects the first
recorded frame when the bridge is attached and keeps the spacing, for
replaying into a machine that is already running (the browser). The replay
verdict compares frame content and order; the cycle skew is reported, not
judged.

## Faults (`faults:`)

The same schema as the lockstep fault engine (`labwired fault-inject`):

| `kind` | Effect |
| --- | --- |
| `can_drop` | the next `count` frames matching `frame` never arrive |
| `can_delay` | matching frames arrive `delay_us` later |
| `node_reset` | reset the node (the SYSRESETREQ path) at `at_cycle`, or when a frame matching `frame` crosses the bridge |
| `can_bus_off` | force FDCAN into bus-off at `at_cycle` (PSR.BO, CCCR.INIT); firmware recovers by clearing CCCR.INIT |

`frame` matches on `direction` (`rx`/`tx`), `id`, `data_prefix` (hex) and
`skip` (let N matches pass first). Faults fire at or after `at_cycle`.

The UDS tester gains `response_timeout_us` and `retries` so a scenario can
show timeout and recovery. The regression tests in
`crates/cli/tests/e2e_h563_can_bridge.rs` run each scenario with a control:
a missing reply (recovers with a retry, fails without), a delayed reply
(passes inside the timeout, fails beyond it), a node reset mid-transfer, and
bus-off (this firmware never clears CCCR.INIT, so the ECU stays silent: that
is the firmware's reaction, reported, not hidden).

## Limits

* The bridge applies to traffic between one controller and the outside of
  the simulation (testers, log players, replay, live host frames). It does not
  sit on a multi-node `CanBus` interconnect between simulated MCUs.
* There is no SocketCAN reader in core. A host bridge (SocketCAN, WebSocket)
  calls the bridge's `offer` API (`SystemBus::can_bridge_offer`, wasm
  `can_bridge_offer`) with each frame.
* Bus-off recovery completes on the CCCR.INIT write; the 129 x 11 recessive
  bit wait is not modeled. Bit errors are not simulated; bus-off is entered
  only through the fault.
