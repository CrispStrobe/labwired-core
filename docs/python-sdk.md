# Python firmware SDK

`labwired.Sim` drives the Rust `Session` engine directly. It loads one ELF using the same machine builder as the native simulator. No server, subprocess simulator, or network connection is required. The older `labwired.Machine` and `labwired.StopReason` imports remain available.

This SDK depends on the `feat/core-session` implementation; it is not a claim that these changes have reached a published PyPI release.

## Build and install

From the repository root, with Rust and Python 3.9+ available:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install maturin pytest
maturin build --release --manifest-path crates/python/Cargo.toml
pip install target/wheels/labwired-*.whl
```

The wheel includes the config catalog and its device/peripheral descriptors. Named-chip lookup uses those installed files, so it works outside the checkout. It does not change `LABWIRED_CONFIG_DIR`. Building copies the repository's config tree into the wheel; use `system=` for a custom manifest, whose `chip` reference may be a packaged catalog name or a file path relative to that manifest. Named chips are catalog names, not an assurance that arbitrary firmware or every physical chip feature is supported.

## Run firmware

This example uses a committed ARM ELF that prints `OK`:

```python
from labwired import Sim

with Sim("tests/fixtures/uart-ok-thumbv7m.elf",
         chip="ci-fixture-cortex-m3-uart1", uart="uart1") as sim:
    match = sim.expect(r"(O)K", timeout="1ms")
    assert match.text == "OK"
    assert match.captures == ["O"]
    print(sim.uart_transcript())
```

Provide exactly one of `chip=` or `system=`. A custom board can be opened with `Sim("firmware.elf", system="board.yaml")`. `uart=` names the actual peripheral ID, not a pin or host serial device. It selects both console capture and receive routing. If omitted, the manifest's `debug_uart` is used; if neither is declared the existing board-wide console capture/RX behavior applies. Invalid UART names fail; unsupported receive routing raises `NotSupported`.

`run_for(0.001)` and `run_for("1ms")` both request a millisecond of **virtual** time. Units are `s`, `ms`, `us`, and `ns`. Positive durations round up to a nanosecond, then to engine cycles; instructions can overshoot a cycle budget. Zero `run_for` is a no-op. Negative, non-finite, malformed, or excessive durations are errors. `expect` requires a positive timeout. Host execution time depends on firmware and machine speed.

`run_for` returns a `StopReason` with `kind` (`reached`, `halted`, `breakpoint`, or `error`) and optional breakpoint `pc`. Check that result if reaching the full duration matters. `sim.time` is elapsed virtual seconds and `sim.cycles` is the actual cycle count. No execution happens between calls.

`expect(regex, timeout=...)` uses Rust byte-regex syntax, consumes through the matched bytes, and preserves later output for the next call. It checks buffered bytes before advancing. `Match.at` is the virtual observation time; batches can run past the exact transmit cycle. `read_uart()` drains the same unread stream as UTF-8 text with replacement for invalid bytes; `uart_transcript()` returns all output without consuming it. `ExpectTimeout` is an `AssertionError` subclass with the pattern, timeout, and last output in its message. A halt without a match also produces this error. A breakpoint without a match raises an execution error.

## Inputs and observations

```python
sim.send("status\n")                  # UTF-8, no automatic newline
sim.send_bytes(b"\x01\x02")
channels = sim.list_inputs()          # device, key, label, unit, min, max
sim.set_input("pressed", 1.0)         # example: board with one input button
sim.set_inputs({"x": 0.0, "y": 0.0}) # atomic: all channels apply or none do
sim.set_pin("user_button", True)     # board_io input binding ID, logical active
word = sim.read_u32(0x20000000)
sim.write_u32(0x20000000, word + 1)
data = sim.read_memory(0x20000000, 4) # bytes
address = sim.symbol("Reset")         # integer address or None
word = sim.read_u32("Reset")         # symbol-aware, clears Thumb function bit
frames = sim.frames()                 # list of dictionaries; drains trace cursor
```

Channel names and binding IDs depend on the board. `list_inputs()` reports the channels the runtime actually exposes; it does not assume every modeled sensor has an input control. Multiple devices exposing the same key make that key ambiguous. Unknown names and invalid values raise errors. Reads and writes use the actual bus, including MMIO side effects. `frames()` returns `seq`, `cycle`, `at` (seconds), `bus`, `summary`, and the native serialized `payload`. The core trace ring is bounded; sequence gaps reveal evicted events.

`inject_can(bus, id, data, extended=False, fd=False, bitrate_switch=False, remote=False)` delivers through the modeled CAN controller's receive path. Clock, initialization, filters and queue capacity must permit reception; rejection is an error.

`snapshot()` returns an opaque token owned by the originating Sim. `restore(token)` reconstructs and deterministically replays that session to restore memory, device state, time and stream cursors. It may be expensive and refuses another Sim's token. Tokens are not portable save files.

Use a context manager or call `close()` to release the machine. Close is idempotent. Subsequent operations fail except `closed` and the retained `uart_transcript()`, which remain useful for diagnostics.

For a one-call run:

```python
from labwired import run_firmware
result = run_firmware("firmware.elf", chip="stm32f103", duration="10ms")
print(result.uart, result.stop_reason.kind)
```

The returned `Run` contains `uart`, `stop_reason`, actual elapsed `time` and `cycles`, and `matches`. Optional `expect=["ready", "done"]` waits for patterns in order with `timeout=` per pattern, then runs the requested `duration`. A breakpoint or early halt is preserved in `stop_reason`.

## pytest

Installing the package registers its pytest plugin. The optional `test` extra declares pytest as a dependency. Ordinary pytest discovery does not open firmware.

```python
def test_boot(sim):
    machine = sim("tests/fixtures/uart-ok-thumbv7m.elf")
    machine.expect("OK", timeout="1ms")
```

```sh
pytest --labwired-chip ci-fixture-cortex-m3-uart1 --junitxml=result.xml
```

The `sim` fixture is a factory accepting the same arguments as `Sim`; it closes every created instance at teardown. Failures include full UART transcripts in terminal sections and JUnit properties, including machines already closed by a context manager. An explicit `NotSupported` exception becomes a skipped test with its reason. Other errors remain failures.

There is no YAML test collection yet. This interface does not add radio networking, RTOS task inspection, logic-analyzer control, ROM/flash-image boot, or multi-machine orchestration. The Python surface exposes only implemented operations; the shared builder reports unsupported targets explicitly. Simulation checks firmware behavior against modeled peripherals; it does not establish electrical behavior or physical-device parity.

## Source archives

For a source distribution, first build a wheel from the full repository to stage the catalog, then run `maturin sdist --manifest-path crates/python/Cargo.toml`. The sdist includes that generated catalog; its wheel rebuild does not need the original checkout. A source build with neither the repository catalog nor the bundled catalog fails explicitly. This wheel-first sequence is the supported packaging path for this SDK slice.
