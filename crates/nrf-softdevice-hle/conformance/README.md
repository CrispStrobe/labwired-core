# Conformance vectors

Each `*.json` is one vector: a starting memory image, then a sequence of
supervisor calls (and air traffic) with the results a SoftDevice must give.
Every backend runs the same files:

* `cargo test -p nrf-softdevice-hle` — the Rust core through the `Host` trait
  (what the labwired backend links).
* `tools/nrf-softdevice-hle/conformance/run_capi.py` (renode-spike-prime) —
  the same vectors through the C ABI `libnrf_softdevice_hle.so` that Renode's
  `SoftDeviceHle.cs` loads.
* `tools/nrf-softdevice-hle/conformance/guest/` — the same vectors compiled
  into a Cortex-M0 firmware that issues real `svc` instructions from the
  application region; it runs in Renode and in labwired and checks the
  emulator glue (exception frame, SVC number, stacked r0, exception return).

Format:

```json
{
  "name": "...",
  "source": "where the expected behaviour comes from (PROVENANCE.md ids)",
  "memory": [{"addr": "0x20001000", "hex": "0011"}],
  "steps": [
    {"svc": "0x12", "args": ["0x20001000", 0, 0, 0], "ret": "0x0",
     "mem": [{"addr": "0x20001000", "hex": "01"}]},
    {"air_in": {"t": "connect_ind", "...": "..."}},
    {"poll_ms": 5, "air_out": [{"t": "adv", "pdu": "adv_ind"}]},
    {"expect_irq_pending": 22}
  ]
}
```

`air_out` entries match when every given key equals the emitted message's key
(a subset match); `mem` compares bytes at the address after the step.
