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
  configured `step_ns` boundaries. It is deliberately independent from the bus
  for now; the next integration layer should map real firmware/topology signals
  into the runner inputs and route returned outputs into traces/UI observables.
- `CosimRunner::step_until_with_signals(time_ns, signals)` applies the manifest
  `inputs` and `outputs` maps against a signal store. For example,
  `controller_enable: control.enable` feeds the model-local
  `controller_enable` input from the `control.enable` store path, and
  `shaft_speed_rpm: observables.shaft_speed_rpm` writes the model output back
  to `observables.shaft_speed_rpm`.
- `CosimRunner::from_configs_with_base(configs, base_dir)` resolves relative
  external model paths against the manifest directory, so example manifests can
  keep local `./models/...` references.

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

The co-sim runner is not yet wired into the machine tick (see
`CosimRunner::step_until` above), so today the circuit is driven through
`labwired cosim-step` and the runner API; routing firmware pins to sources and
node voltages to ADC channels is the next integration step.
