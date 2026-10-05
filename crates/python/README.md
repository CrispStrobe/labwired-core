# labwired (Python)

Run microcontroller firmware in the LabWired simulator and test it with pytest.
Time is virtual: execution advances only inside `run_for` and `expect`.

## Install

```sh
pip install labwired
```

Wheels are abi3: one wheel per platform covers CPython 3.9 and newer
(Linux x86_64/aarch64, macOS arm64/x86_64, Windows x64).

## Test firmware with pytest

The package installs a pytest plugin with a `sim` fixture:

```python
def test_boot_banner(sim):
    with sim("firmware.elf", chip="stm32f103") as s:
        s.expect(r"boot ok", timeout="10ms")
        s.send("ping\n")
        assert s.expect("pong", timeout="5ms").text == "pong"
```

Run it with `pytest`. Give `--labwired-chip <name>` to set a default chip for
every `sim(...)` call. On a failure, the report shows the UART transcript.

## Test a drawing

`Sim(elf, diagram="board.json")` runs the diagram you drew in the playground
(or an agent wrote): the parts, the wires, the analog circuit, the bench
instruments. The diagram is lowered by `labwired-lower`, the compiler the
playground and the hosted API use, so there is no second copy to drift.

```python
import labwired

with labwired.Sim("capacitive-touch-lab.elf", diagram="diagram.json") as board:
    board.watch("D13")                          # a pin the drawing wires
    board.run_for("0.4s")
    board.set_signal("ui.touch.pressed", True)  # a finger on the pad
    board.run_for("0.4s")
    print(board.read_uart(), board.edges())     # console, and the LED's level changes
    print(board.meters())                       # multimeter readings, as in the playground
```

A diagram with several MCUs returns a `World`; `elf` is then a mapping from part
id to ELF:

```python
with labwired.Sim({"mcu": "stm.elf", "avr": "avr.elf"}, diagram="two-boards.json") as world:
    world.run_for("30ms")
    world.machine("avr").uart_transcript()
    world.net("mcu.PB4")["edges"]               # every GPIO net between the chips
```

On a `Sim` and on each `world.machine(id)`: `watch(*pads)` and `edges()` (level
changes with the cycle and time), `read_uart`, `expect`, `set_input`, `set_pin`.
A `Sim` also has `set_signal(path, value)` (a signal of the analog island),
`analog_trace()` (the CSV `labwired test --analog-trace` writes) and `meters()`.
`world.nets()` reports every GPIO net: level, edge count, members, contention.
A pad is a pin the diagram wires (`"PB4"`), `"gpiob:4"`, or a `(peripheral, pin)` pair.

Needs Node.js 20 or newer for the lowering. `labwired-lower` is found on `PATH`,
else run as `npx -y --package=@labwired/board-config@<version> labwired-lower`. The wheel records the
version it was tested with, and the tool's major.minor must match. Without Node
the call fails with one message that says how to install it. It never falls back
to a hosted service: your design is not sent anywhere. Set `LABWIRED_LOWER` to
use a `labwired-lower` you already have.

One chip descriptor override is sometimes needed: a lab whose firmware was built
for silicon other than the drawn board (`gpio-net-two-boards` drawn as a G071
Nucleo, firmware for a G0B1) takes `chip_yaml={"mcu": "stm32g0b1re.yaml"}`.

Limits: an analog island needs one MCU (worlds do not run co-simulation models
yet), and a world's pads report edges by cycle on each node's own clock.

## API summary

- `Sim(elf, chip=... | system=..., uart=None)`: one ELF on one machine.
- `run_for(duration)`, `expect(pattern, timeout)`: advance virtual time.
- `read_uart()` returns text (invalid UTF-8 becomes U+FFFD).
  `read_uart_bytes()` returns the raw bytes.
- `send(text)`, `send_bytes(data)`, `read_u32`, `write_u32`, `read_memory`,
  `snapshot()`, `restore(snap)`, `set_input`, `set_pin`, `frames()`.
- `run_firmware(elf, chip=..., duration="1s", expect=...)`: one-shot helper.
