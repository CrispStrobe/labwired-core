# Experimental WASM resident-edge metadata

Baseline: `43b2d62f5a0fa24ae0b38a645069f5aaa78af685`. Do not stack the rejected
literal-load variants. No speedup or qualification is claimed before testing.

Whole-process active F0 profiles found repeated resident-device edge-address
scanning on GPIO writes, even for a bus containing only input contacts. The
list was publicly mutable, so a naive cached absence bit would miss in-place
replacement, insertion or device mutation. This experiment introduces a
Vec-like `ResidentDevices` owner that invalidates derived absence on every
mutable borrow (including indexing, iterators, downcasts and Vec access).

Only WASM production and native unit tests enable negative caching. Native
production retains the original scan policy. Custom devices remain untrusted
by default: their edge metadata is queried on every write. Only Button opts
in, because its trait edge-address slice is permanently empty. The new opt-in
contract also requires metadata queries to have no guest-visible effects.
Nonempty metadata is never cached. No MMIO value, clock gate, IRQ, waveform,
device service or GPIO transition is suppressed. Analog mux/timer capture
hooks retain their position before edge-device filtering.

Rust source compatibility: `gpio_devices` now has a Vec-like wrapper type;
push/index/iteration remain valid, explicit Vec assignment requires `.into()`
and struct initializers use `ResidentDevices::default()`. This is an explicit
Rust API change, not a WASM export, debugger or snapshot-format change. Cache
state is derived and is not serialized as physical/guest evidence.

Tests cover cached absence, untrusted interior metadata changes, same-length
replacement, insertion/removal, mutable downcasts/iteration/raw Vec borrows,
and take/service/restore. Existing generic resident-device/bit-banged display
tests remain required; their source anti-vacuity check tracks the new generic
field type without relaxing the ban on device-specific bus fields.

Before landing: full core library and generic device tests, deterministic
actual WASM integration, repeated same-runner ordinary A/B/B/A for both F0
workloads and selected motion, and all final enabled CI checks. Keep failed
floor windows failed. No artifact publication, app pin update, hardware capture
rewrite or acknowledgement re-stamp is authorized by this experiment.
