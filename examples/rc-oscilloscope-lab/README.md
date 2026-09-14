# RC Oscilloscope Lab

An STM32F401 drives a first-order RC low-pass (R1 = 10k, C1 = 100n, tau = 1 ms)
from PA5 and samples the filtered node on ADC1 channel 0 (PA0). PA5 steps
every 5 ms, the ADC is read every 500 us, and each sample is printed over
USART2 as `t=<us> adc=<code> v=<mV>`. The RC network is not a device model:
it is a SPICE netlist (`rc.cir`) run by the in-core analog engine through the
`cosim_models` block in `system.yaml`, and the playground opens the lab with
the oscilloscope instrument showing `v(out)` charging and discharging with
each PA5 edge.

## Files

- `system.yaml` — STM32F401 chip, `debug_uart: uart2`, `board_io` for the PA5
  drive (`led`) and the PA0 sample point (`adc_input`), and the `cosim_models`
  entry that binds `rc.cir` to those pins.
- `rc.cir` — the netlist. `Vgpio` is the source the engine drives from PA5.
- `src/main.rs` — bare-register firmware. TIM2 is the 1 MHz tick that
  schedules both the 5 ms toggle and the 500 us sample, so the timing is
  exact under simulation rather than busy-wait approximate.
- `test.yaml` — smoke test asserting the banner, the sample lines, and at
  least one reading in the 1.5–3.0 V band (the charge curve crossing
  mid-rail).

## Build the firmware

```
rustup target add thumbv7em-none-eabi
cargo build -p rc-oscilloscope-lab --release --target thumbv7em-none-eabi
```

The ELF lands at `target/thumbv7em-none-eabi/release/rc-oscilloscope-lab`.
Example ELFs are not committed; the playground fetches prebuilt demo firmware
from the `firmware-demos-v1` GitHub release (see
`packages/playground/scripts/fetch-demo-firmware.sh` in the labwired
monorepo), so this ELF needs to be added to that release when the lab ships.

## Run natively

```
labwired test --script examples/rc-oscilloscope-lab/test.yaml --analog-trace out.csv
```

`--analog-trace` writes the probed `v_out` samples to CSV for plotting or for
the validation matrix golden file.

## Branch dependencies

This example is ahead of two in-flight branches:

- `feat/analog-engine` adds the `analog` cosim adapter. Until it lands,
  `system.yaml` fails manifest parsing on `adapter: analog` (the parser only
  knows `external_process`, `fmi`, `mock`), and `--analog-trace` does not
  exist.
- `feat/cosim-pin-routing` adds the `board.gpio.<pin>` / `board.analog.<pin>_volts`
  routing grammar used in the `inputs` / `outputs` maps.

With the adapter temporarily switched to `mock`, the firmware runs, the
timing is exact (`t=` advances by exactly 500), and `test.yaml` passes 3 of 4
checks; the `uart_regex` band check fails because nothing drives PA0, so the
ADC returns its unseeded placeholder ramp instead of the RC curve. That is
the expected state until both branches merge.
