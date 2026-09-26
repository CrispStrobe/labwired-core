# nrf-softdevice-hle — the backend contract

This crate is the one, emulator-neutral implementation of the SoftDevice API
(S110 v8 family, nRF51: micro:bit V1, Calliope mini 1). An emulator becomes a
backend by meeting the five points below; the conformance suite
(`conformance/*.json`) is run by every backend.

Backends today:

| backend | where | links the core via |
|---------|-------|--------------------|
| labwired | `crates/core/src/sd_hle.rs` (`Machine::attach_sd_hle`), chip `configs/chips/nrf51822.yaml`, runner `crates/core/examples/sd_hle_run.rs` | Rust (`Host` trait) |
| Renode | renode-spike-prime `tools/nrf-softdevice-hle/renode/SoftDeviceHle.cs` (loaded with `include`), platform `nrf51822-app.repl`, runner `run.py` | C ABI (`--features capi`, `libnrf_softdevice_hle.so`) |

## 1. Load only the application region

The image loader keeps bytes in `[APP_BASE, APP_END)` and drops every other
record unread (renode-spike-prime `tools/nrf-softdevice-hle/appimage.py`):

| image | APP_BASE | APP_END | Nordic ranges never loaded |
|-------|----------|---------|----------------------------|
| micro:bit V1 (nRF51822, S110 v8) | 0x18000 | 0x3C000 | MBR+SD 0x0-0x17FFF, bootloader 0x3C000- |
| Calliope mini 1 (nRF51822, S110 v8) | 0x18000 | 0x3C000 | same |
| micro:bit V2 (nRF52833, S113) | 0x1C000 | 0x77000 | MBR 0x0-0xFFF, SD 0x1000-0x1BFFF, bootloader 0x77000- — **not supported, see "V2" below** |

Reads below APP_BASE return erased flash or zero, never code; fetching there
is a fault (nothing is executable). A debugger reading the range sees 0xFF/0.

## 2. Forward exceptions like the MBR + SoftDevice

On silicon, reset enters the MBR, which forwards to the SoftDevice, which (not
enabled) forwards reset and every interrupt to the vector table at APP_BASE.
The backend sets **VTOR = APP_BASE** (Cortex-M0 has no architectural VTOR;
both emulators honour one anyway) and resets from that table. The
application's `SD_EVT_IRQHandler` (SWI2, IRQ 22) is where SoftDevice events
arrive; the HLE pends IRQ 22 through the NVIC registers.

## 3. Service supervisor calls

When the core has taken SVCall (IPSR = 11) and is at the application's
SVCall vector (vector 11 of the APP_BASE table), before executing it:

1. frame = PSP if EXC_RETURN (LR) bit 2 else MSP;
2. r0-r3 = frame[0..4]; number = byte at (frame[6] - 2) — the `svc #imm8`;
3. `ret = SoftDevice::svc(number, [r0, r1, r2, r3], host)`;
4. frame[0] = ret;
5. return from the exception (branch to the EXC_RETURN value in LR).

The application's own SVCall handler (a `b .` default handler in the micro:bit
images) never runs.

## 4. Provide the Host

`Host::read/write` over the system bus, refusing addresses below APP_BASE;
`Host::nvic` (default: through the NVIC ISER/ICER/ISPR/ICPR/IPR and AIRCR
registers — every Cortex-M model has them); `Host::now_us` = emulated time.

## 5. Poll

Call `SoftDevice::poll` about every millisecond of emulated time: it sends
advertising events and takes air traffic (connections, ATT, SMP, LL control).

## The air

`SoftDevice::attach_air` takes any `Air`: `TcpAir` to the bw-air/1 hub,
`MemAirBus` in process. The hub and the contract have one home:
renode-spike-prime `tools/bw-air/` (`AIR.md`, `airhub.py`). The same air
carries the emulated SPIKE Prime hub (its HCI host through
`tools/bw-air/hci_node.py`), bumble peers and Scratch Link clients such as
Brickwright lite, so an emulated micro:bit and a SPIKE hub see each other.
This crate does not carry its own air; message types it does not handle
(`lmp`, `acl_br`, `enc_br` for BR/EDR) are ignored.

## Coverage

Implemented (S110 v8 numbering): SDM enable/disable/is_enabled/vector base;
MBR command (vector table base); SoC: NVIC wrappers, critical region, rand,
power (mode/reset reason/RAM/GPREGRET/DCDC), clock HFCLK, ECB (AES-128),
temperature, flash page erase/write (+ flash events), `sd_evt_get`,
`sd_app_evt_wait`, PPI and radio notification accepted; BLE: enable,
`sd_ble_evt_get` (with peek), TX buffer count, vendor UUIDs (add, encode,
decode), version, opt set; GAP: address get/set, advertising data, start,
stop, connection parameter update, disconnect, TX power, appearance, PPCP,
device name, security parameters reply (legacy Just Works pairing with key
distribution into the app's keyset), security info reply (re-encryption with
a bonded LTK), connection security get; GATTS: service/characteristic/
descriptor add (stack- and user-located values, CCCDs), value set/get,
notifications and indications (+ `BLE_EVT_TX_COMPLETE` / `BLE_GATTS_EVT_HVC`),
system attributes, authorize reply accepted.

Not implemented (return `NRF_ERROR_NOT_SUPPORTED` and are visible in the
trace): GAP central/observer role (scan, connect), GATTC, L2CAP CID
registration, radio timeslot sessions, mutexes, passkey/OOB pairing, LE
Secure Connections (S110 has none), read/write authorization requests.

## V2 (S113) — out of scope, and why

The official micro:bit V2 images are built for S113 (nRF5 SDK 17 API). The
only sources for its SVC numbers and structures are Nordic's 5-clause-licensed
headers (clause 4: only with a Nordic IC — the clause covers source); no
permissive source exists. More decisively, the V2 **application region itself**
links 37 nRF5 SDK modules (29,310 bytes: peer manager, fds, nrf_sdh, …) under
that licence even for radio-only programs (measured from a rebuild of pxt's
exact request; see the PR), so executing the official V2 application region
in an emulator is what the licence forbids, SoftDevice or not. V2 emulation
uses the Bluetooth-free bases built from source (lite PR #332).
