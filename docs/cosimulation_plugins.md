# Co-Simulation Plugins

LabWired owns deterministic firmware execution, board topology, traces, and
fault orchestration. Physical plant models can be declared as co-simulation
models and stepped through an adapter.

The first supported manifest shape is:

```yaml
cosim_models:
  - id: "plant_model"
    adapter: "external_process" # external_process | fmi | mock
    model: "./models/mock-plant.py"
    step_ns: 10000
    inputs:
      controller_enable: "control.enable"
      load_torque_nm: "plant.load.torque_nm"
    outputs:
      shaft_speed_rpm: "observables.shaft_speed_rpm"
      plant_ready: "observables.plant_ready"
    config:
      protocol: "jsonl"
```

`external_process` and `fmi` adapters require `model`. `step_ns` must be
greater than zero. The `mock` adapter is intended for deterministic tests and
dry-runs; its static outputs are declared under `config.outputs` so the top-level
`outputs` map can stay dedicated to LabWired signal routing.

The core runtime now exposes a small co-sim registry and runner:

- `build_cosim_adapter(config)` constructs `mock` and `external_process`
  adapters from a `CosimModelConfig`.
- `adapter: fmi` intentionally returns a clear unsupported error until the FMI
  import path is selected.
- `CosimRunner::step_until(time_ns, inputs)` steps models only at their
  configured `step_ns` boundaries.
- `CosimRunner::step_until_with_signals(time_ns, signals)` applies the manifest
  `inputs` and `outputs` maps against a signal store. For example,
  `controller_enable: control.enable` feeds the model-local
  `controller_enable` input from the `control.enable` store path, and
  `shaft_speed_rpm: observables.shaft_speed_rpm` writes the model output back
  to `observables.shaft_speed_rpm`.
- `CosimRunner::from_configs_with_base(configs, base_dir)` resolves relative
  external model paths against the manifest directory, so example manifests can
  keep local `./models/...` references.
- `cosim::routing` maps store paths onto real machine state — see
  [Board signal paths](#board-signal-paths) — and `cosim::CosimSession` is the
  runner, the routing, and the cycle ↔ nanosecond time base bound to one
  machine, which is what `labwired test` steps.

This contract is intentionally domain-neutral. A model can represent a motor,
thermal plant, hydraulic system, sensor array, battery pack, power stage, or
any other external process as long as it consumes named inputs and returns named
outputs.

The `examples/cosim-plant-demo` manifest exercises this generic contract with a
reduced-order discrete plant: per-channel enabled/disabled states, a
scenario-driven `disabled_channels` list, and voltage/current/active-channel
observables. Higher-fidelity behavior (loss, thermal, semiconductor stress,
grid-compliance validation) belongs in a later adapter-backed model.

Drive any manifest-declared model from the command line with
`labwired cosim-step <system.yaml> --set <path>=<value>`, which builds the
runner from the manifest and prints the routed outputs after stepping.

## Mixed-signal: ngspice as a model

`tools/cosim/labwired_ngspice.py` turns any SPICE netlist into an
`external_process` model. It hosts libngspice in-process, and on every step
alters the mapped voltage sources to the requested values, places a breakpoint
at the step's end time and resumes the transient analysis to it; the probed
node voltages come back as outputs. There are no threads and no wall clock, so
the same input sequence always produces the same voltages.

- Inputs: `true`/`false` map to `vdd` / 0 V (a GPIO pin), numbers are volts.
- Outputs: last value of the probed vector (`v(node)`, `i(vsrc)`) at step end.
- The operating point is solved from the netlist's own defaults first; inputs
  are applied once time is running, like a real pin edge.
- Open-source and vendor device models work through the netlist's `.include`
  and `.lib` lines; check each library's licence before bundling it.
- One circuit per wrapper process (libngspice is process-global). A second
  circuit is a second `cosim_models` entry.

Worked example: [`examples/cosim-spice-rc`](../examples/cosim-spice-rc/README.md)
(GPIO into a 10 kΩ / 100 nF low-pass, τ = 1 ms). Requires `libngspice0`
(Debian/Ubuntu: `apt install libngspice0`). Tests:
`python3 -m pytest tools/cosim/test_labwired_ngspice.py`.

## Board signal paths

A model that only speaks abstract observables can be driven from the command
line but never by firmware. The `board.` / `adc.` path grammar is how a
`cosim_models:` entry names something real on the chip, so `labwired test`
steps the model against the pins the firmware is actually driving:

| Path | Direction | Type | What it is |
|------|-----------|------|------------|
| `board.gpio.<pad>` | machine → model | bool | The level the firmware is driving on an output pad. |
| `board.gpio_in.<pad>` | model → machine | bool | An externally held level on an input pad (also readable). |
| `board.analog.<pad>_volts` | model → machine | number | The analog level on the ADC channel belonging to `<pad>`. |
| `adc.<peripheral>.<channel>_volts` | model → machine | number | The analog level on an explicitly named ADC channel. |

`<pad>` is a pad label in whatever form the chip speaks — `pa5` / `PA5` on
STM32, `p0.13` on Nordic, `gpio5` or a bare `5` on ESP32. Labels resolve
through the same pin resolution every other pad-addressed feature uses: a
chip's declared `pins:` map first, then the standard STM32/Nordic parse, then
the ESP32 forms. Case does not matter.

Direction is enforced. `board.gpio.<pad>` is what the firmware drives, so a
model *output* routed to it is a config error rather than a write that silently
does nothing; drive a pin with `board.gpio_in.<pad>` instead, which goes
through the same seam a `board_io` button uses. Likewise an analog path is a
sink only and cannot be read back into a model input.

`board.analog.<pad>_volts` needs a pad → ADC-channel map. On the STM32 parts
in-tree that map is fixed silicon — `ADC1_IN0..IN7` are `PA0..PA7`, `IN8`/`IN9`
are `PB0`/`PB1`, `IN10..IN15` are `PC0..PC5` — and it is what
`examples/ntc-thermistor-lab` already wires its thermistor to. No chip
descriptor states it (`pins:` maps a pad to a GPIO block and bit, and carries
no analog function), so on any other family the pad form reports
"no ADC channel is modelled for pad …" at startup and the chip-neutral
`adc.<peripheral>.<channel>_volts` form is the one to use. Both write through
`SystemBus::seed_adc_channel`, the single choke point every analog stimulus
(thermistor, potentiometer, battery divider) already goes through; the ADC
model owns volts → counts, at 3.3 V full scale and 12 bits (1.65 V is 2047).

Paths outside this grammar — `control.enable`, `plant.output.voltage` — stay
plain signal-store keys routed between models, exactly as before.

## In the run loop

`labwired test` builds a `CosimSession` when, and only when, the manifest
declares `cosim_models`. A manifest without them runs the identical loop it ran
before this existed.

With models declared, each iteration of the run loop:

1. caps the advance request's **simulated-cycle** budget at the cycles left
   before the next model boundary, so the machine can never run past a boundary
   and hand a model pin levels from its future;
2. advances the machine to that boundary;
3. samples every routed `board.gpio*` path off the bus into the signal store;
4. steps every model whose `step_ns` boundary the machine has reached
   (`CosimRunner::step_until_with_signals`);
5. writes the routed outputs back — GPIO input levels onto pins, volts onto ADC
   channels — so the firmware's next instruction sees the model's answer.

The lockstep granularity is the finest declared `step_ns`. Simulated time comes
from the machine's own cycle counter and the bus's `cpu_hz`, so it is the same
clock every trace and assertion is expressed in. There are no threads and no
wall clock anywhere in this path: the same firmware produces the same model
inputs on every run.

A path that does not resolve fails the run at startup instead of degrading it —
a co-simulation whose pin never reached the firmware would otherwise still
print a verdict that is evidence of nothing. A model that errors mid-run ends
the run rather than letting the firmware keep executing against a plant that
stopped answering. Routed outputs are logged under the `cosim` target:

```bash
RUST_LOG=info,cosim=debug labwired test --script examples/cosim-spice-rc/rc-blink.yaml
```

`labwired cosim-step <system.yaml> --set <path>=<value>` still drives a model
directly, without firmware, for probing a manifest.
