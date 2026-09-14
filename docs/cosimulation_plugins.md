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

## In-core analog engine (`adapter: analog`)

The browser cannot spawn a process, so `external_process` — and with it
ngspice — has no counterpart there. `labwired_core::analog` is a small
deterministic MNA transient solver compiled into the engine itself: it runs in
the browser, runs natively, and adds no dependency to either.

A manifest switches engines by changing one line. The `netlist`, `vdd`,
`probes` and `sources` config keys are spelled exactly as the ngspice wrapper
spells them:

```yaml
cosim_models:
  - id: rc_lowpass
    adapter: analog            # was: external_process + model: ./models/rc_lowpass.py
    step_ns: 100000
    inputs:  { gpio: board.gpio.pa5 }
    outputs: { v_out: board.analog.pa0_volts }
    config:
      netlist: ./rc.cir        # or inline: netlist_text: |
      vdd: 3.3                 # volts a boolean input maps to (default 3.3)
      substeps: 10             # internal solver steps per co-sim step
      integration: be          # be (default) | trap
      probes:  { v_out: "v(out)" }   # output name <- node / branch current
      sources: { gpio: Vgpio }       # input name  -> V, I or S element
      trace: ["v(in)", "i(Vgpio)"]   # extra oscilloscope channels
      trace_samples: 20000           # ring depth (default 20000)
```

Worked example: [`examples/cosim-spice-rc/system-analog.yaml`](../examples/cosim-spice-rc/system-analog.yaml),
the same circuit and the same routing as `system.yaml`.

### Netlist subset

| Element | Line |
|---|---|
| Resistor | `R<name> n1 n2 <value>` |
| Capacitor | `C<name> n1 n2 <value> [ic=<v>]` |
| Inductor | `L<name> n1 n2 <value> [ic=<i>]` |
| Voltage source | `V<name> n+ n- dc <value>` |
| Current source | `I<name> n+ n- dc <value>` |
| Switch | `S<name> n1 n2 <ctrl> ron=<r> roff=<r>` |

Plus `*` comment lines, `;` / `$` trailing comments, `.end`, and
`.ic V(node)=<v>`. Node `0` and `gnd` are ground. Values take the usual SPICE
suffixes (`k`, `meg`, `m`, `u`, `n`, `p`, `f`, `g`, `t`), and trailing unit
letters are ignored, so `100nF` and `10kohm` read as written. Unlike a classic
SPICE deck, line 1 is NOT a title — put a `*` on it, because silently dropping
an element line is the worst thing a netlist parser can do.

A switch's `<ctrl>` is the name of a routed boolean input, not a circuit node,
so it needs no `sources:` entry.

### Solver

- Modified nodal analysis with companion models. Backward Euler by default;
  `integration: trap` selects trapezoidal, about two orders of magnitude closer
  to the closed form at the same step (0.29 % vs 0.002 % at one tau on the RC
  example) but able to ring on a hard edge. The first internal step after any
  source or switch change is taken with backward Euler, as SPICE does at a
  breakpoint, so an edge does not leave trapezoidal a half-step behind.
- Fixed internal step `h = step_ns / substeps`. Dense LU, own implementation.
  `N` nodes + `M` branch currents is capped at 64; a bigger circuit is an
  error naming the ngspice adapter.
- The operating point is solved at t = 0 from the netlist's own DC values,
  capacitors open and inductors shorted, then `.ic` / `ic=` override it. Routed
  inputs apply only once time runs — the same ordering the ngspice wrapper gets
  by pausing its transient just after t = 0, so a pull-up sits at Vdd and a
  GPIO at 0 before the firmware has done anything.
- `CosimStep::time_ns` is the END of the interval being simulated, matching
  `tools/cosim/labwired_ngspice.py`.
- Deterministic: `f64` only, `Vec` indices in the hot path, `BTreeMap` for
  names, no threads and no wall clock.

### Waveform trace

Every routed output is an oscilloscope channel; `config.trace` adds more.
Samples go into a bounded ring (`config.trace_samples`, default 20 000 — two
seconds at a 100 µs step), read by cursor like `logic_read_edges`:

- core: `Machine::analog_trace_snapshot(cursor)` and `Machine::analog_channels()`,
  after `Machine::attach_analog_trace(runner.analog_trace_registry())`.
- WASM: `WasmSimulator::analog_channels()` and
  `WasmSimulator::analog_trace_snapshot(cursor)`.
- CLI: `--analog-trace <path>` on `run`, `test` and `cosim-step`. A `.csv`
  extension writes `time_ns,<channel>...`; anything else writes a VCD with one
  `real` variable per channel, so the analog curve opens in GTKWave / PulseView
  beside the digital logic capture.

All analog models on one runner share one ring, each owning a block of
channels; a model that steps writes a full row and carries the other models'
channels forward, which is what a scope shows between updates. Channel names
are the plain manifest names for a single analog model, and `<model id>.<name>`
when more than one is declared.

### The boundary

The in-core engine is linear elements and ideal switches, and nothing else. It
does not model diodes, transistors, subcircuits, `.include` / `.lib` device
libraries, or AC/DC sweeps, and it does not approximate them: a netlist line
outside the subset fails **manifest validation** with

```
element `D1 a b diode` needs ngspice; use `adapter: external_process` with `tools/cosim/labwired_ngspice.py`
```

That is the whole boundary. Native runs that need real device physics use the
ngspice adapter above, which has none of these limits; the browser runs the
in-core engine, which needs no process.
