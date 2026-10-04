# STM32F401 H-bridge DC motor

Bare-metal STM32F401 firmware drives a two-terminal brushed DC motor through
one L298N-style H-bridge channel, forward and then reverse. The test script
proves both directions from the motor plant itself.

| MCU pad | Bridge input | Use |
|---|---|---|
| PA0 | IN1 | direction |
| PA1 | IN2 | direction |
| PA6 | ENA | TIM3 CH1 hardware PWM, 70 % duty |

The driver chip is not a simulated device. Its truth table is the plant's
terminal drive (`in1_pin` / `in2_pin`): `10` forward, `01` reverse, `11`
brake, `00` coast. `pwm_pin` plus `timer_name` / `timer_channel` give the
speed input, and the plant reads the timer's duty while that channel is in
PWM mode (an alternate-function pad's output latch never moves).

```bash
cargo run -q -p labwired-cli -- test \
  --script examples/f401-hbridge-dc-motor/hbridge.yaml \
  --output-dir out/f401-hbridge
```

The run stops with `assertions_passed` once the UART shows `HBRIDGE READY`,
`FORWARD`, `REVERSE` and the motor has passed 500 rpm in each direction.
`result.json` carries a `motors` block with the final `speed_rpm` and the
extremes the plant saw (`speed_rpm_max`, `speed_rpm_min`,
`speed_rpm_peak_abs`). With 12 V, 70 % duty and a 0.08 V/(rad/s) motor the
forward leg settles near 1000 rpm.

Rebuild the committed firmware fixture with:

```bash
cargo build --release -p firmware-f401-demo --bin firmware-f401-hbridge \
  --target thumbv7em-none-eabihf
arm-none-eabi-strip -o tests/fixtures/stm32f401-hbridge-motor.elf \
  target/thumbv7em-none-eabihf/release/firmware-f401-hbridge
```

PWM on an IN pin (DRV8833 / L9110 style) is the same config with `pwm_pin`
naming the same pad as `in1_pin` or `in2_pin`.
