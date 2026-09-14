# Co-Simulation with ngspice — RC low-pass

Mixed-signal co-simulation through LabWired's `external_process` contract:
`tools/cosim/labwired_ngspice.py` hosts libngspice in-process and steps a
SPICE netlist in lockstep with the firmware clock. A firmware GPIO edge becomes
a voltage-source change; a probed node voltage comes back each step.

The netlist here is a 10 kΩ / 100 nF low-pass (τ = 1 ms). Any ngspice-compatible
model library works the same way — `.include` / `.lib` vendor or open-source
models in the `.cir` file and map the sources and nodes you care about.

## Run it

```bash
sudo apt install libngspice0          # once
labwired cosim-step examples/cosim-spice-rc/system.yaml \
    --set board.gpio.pa5=true
```

`cosim-step` builds the runner from the manifest, feeds `board.gpio.pa5` into the
model as the `gpio` source (booleans map to Vdd / 0 V), steps the circuit to the
model's `step_ns` boundary and prints `board.analog.pa0_volts`.

Standalone, without LabWired:

```bash
printf '{"time_ns":0,"dt_ns":1000000,"inputs":{"gpio":true}}\n' \
  | python3 examples/cosim-spice-rc/models/rc_lowpass.py
# {"outputs": {"v_out": 2.08...}}      (3.3 V × (1 − e⁻¹) after one τ)
```

## Contract details

- Inputs: `true`/`false` → `vdd` / 0 V; numbers → volts directly.
- Outputs: the probed vector's last value at the step's end time.
- Determinism: no threads or wall clock; the operating point is solved from
  the netlist's own defaults, then inputs are applied once time is running.
- One circuit per wrapper process (libngspice is process-global). Declare a
  second `cosim_models` entry for a second circuit.

Tests: `python3 -m pytest tools/cosim/test_labwired_ngspice.py`.
