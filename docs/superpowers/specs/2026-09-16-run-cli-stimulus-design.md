# `run` CLI Stimulus Design

**Date:** 2026-09-16

## Purpose

Declarative input stimuli (test-script schema 1.2) exist only behind
`labwired test --script`: the `stimuli:` YAML block. Agents and scripts that
invoke the CLI directly must author or synthesize a YAML file to drive a sensor
mid-run. This design adds a first-class CLI surface on `labwired run` that
accepts stimuli as command-line arguments with the same wire fields the MCP
`run` tool exposes.

The surface is not a straight flag addition because `labwired run` today builds
its bus without a system manifest: the ARM and RISC-V paths synthesize
`external_devices: []` (crates/cli/src/commands/run.rs:1114), and the Xtensa
paths never read one. External SimInput devices — the devices stimuli target —
are therefore absent from a `run` bus. So `run` gains an optional `--system`
manifest handled by a system-aware driver, and the stimulus flag lives on that
driver.

## Goals

1. `labwired run --system <manifest>` selects a system-aware driver that
   attaches external devices; the chip descriptor is resolved from the
   manifest, so `--chip` is optional when `--system` is present.
2. `--stimulus <JSON>` is repeatable and spells exactly the MCP wire shape:
   `{"channel": "...", "value": 2.0, "after_cycles": 3000000, "component": "..."}`.
   `after_cycles` omitted or `0` means `at_start`.
3. Stimuli are validated against the live bus before stepping. Malformed JSON,
   unknown channels, ambiguous channels, and out-of-range values fail with
   `EXIT_CONFIG_ERROR`; under `--json` the error payload includes the available
   `{component, channel}` inventory so an agent can self-correct.
4. `run` and `test` share one stimulus runtime helper, so trigger semantics
   (`at_start`, `after_cycles`, once-only firing, batch deadline limiting)
   cannot drift between them.
5. `labwired run` behavior is unchanged when `--system` is absent; the existing
   per-arch fast-boot paths are not touched.

## Non-goals

- Live or interactive driving (REPL, pause/step/set-input). CLI flags inject
  timed stimuli; they do not make `run` a server.
- Any change to `labwired test`, the test-script schema, or the MCP temp-script
  synthesis flow.
- `--rom-boot`, `--break-at`, and `--watch-mem` support on the system-aware
  driver; those remain exclusive to the existing chip paths.
- `--stimulus` support without `--system` (hard error, see below).
- New SimInput devices, channels, or trigger kinds. `on_write` / `on_read`
  remain rejected for stimuli, matching test-script validation.

## CLI Surface

```console
$ labwired run \
    --system configs/systems/frdm-kw41z-lcd.yaml \
    --firmware tests/fixtures/kw41z-lcd-activity.elf \
    --max-steps 6000000 \
    --stimulus '{"component":"fxos8700","channel":"x","value":2.0,"after_cycles":3000000}'
```

New `RunArgs` fields:

- `--system <PATH>`: board manifest (SystemManifest YAML). Selects the
  system-aware driver.
- `--stimulus <JSON>`: repeatable, order preserved. Each argument deserializes
  into a struct with `deny_unknown_fields`; missing required fields, unknown
  fields, non-finite values, and negative cycle counts are configuration
  errors reported with the argument index (e.g. `--stimulus[2]: ...`).

Rules:

- `--chip` is `required_unless_present("system")`. With `--system`, the chip
  YAML is resolved from the manifest exactly as `build_system_bus` does.
- `--chip` and `--system` conflict (clap `conflicts_with`); a mixed invocation
  is rejected rather than silently preferring one.
- `--stimulus` without `--system` exits `EXIT_CONFIG_ERROR` with the message
  that stimuli target devices declared in a system manifest.
- A bare `channel` exposed by more than one device is an error until
  `component` disambiguates it, mirroring the bus resolution rule.

## Architecture

Dispatch change (crates/cli/src/main.rs):

```rust
Some(Commands::Run(args)) => commands::run::run_firmware(args, cli.json),
```

`run_firmware` gains a `json: bool` parameter for structured diagnostics, then
branches:

- `args.system.is_some()` → `commands::run_system::run_firmware_with_system(&args, json)`.
- `args.system.is_none()` → existing per-arch dispatch, byte-for-byte behavior.

New module `crates/cli/src/commands/run_system.rs` drives:

1. Parse each `--stimulus` argument into `labwired_config::StimulusSpec`
   (absent/zero `after_cycles` → `FaultTrigger::AtStart`; non-zero →
   `FaultTrigger::AfterCycles`).
2. Build the bus via `labwired_core::system::builder::build_system_bus(Some(path))`.
3. Load the firmware ELF and pick the arch from the manifest's chip descriptor,
   reusing the `Machine` construction from `run_interactive_{arm,riscv,xtensa}`
   (`configure_cortex_m` / `configure_riscv` / `configure_xtensa` +
   `Machine::load_firmware`).
4. Validate every spec against `bus.list_inputs()` before stepping (see Error
   Handling). Validation resolves the owner and range-checks against the
   channel's `InputChannel::{min,max}`; it never applies a value early, so
   `after_cycles` timing is unaffected.
5. Apply `at_start` specs, then run the loop with `StimulusTrack::poll` once per
   step, using `Metrics::get_cycles()` for `after_cycles` thresholds and
   `args.max_steps` as the step limit (unlimited when not given, matching
   today's `run`). UART streaming, stop-reason mapping to exit codes, and the
   final stderr progress line follow the existing `run` paths; `--json` affects
   error payloads only (see Error Handling), not the success output.
6. Export `--bus-trace-out` / `--gpio-trace` as the current `run` paths do.

New shared module `crates/cli/src/stimuli.rs`:

```rust
/// Deserialized `--stimulus` argument (MCP wire shape).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliStimulus {
    pub channel: String,
    pub value: f64,
    #[serde(default)]
    pub after_cycles: Option<u64>,
    #[serde(default)]
    pub component: Option<String>,
}

pub fn parse_stimulus_arg(index: usize, raw: &str) -> Result<StimulusSpec, StimulusArgError>;
pub fn validate_specs(specs: &[StimulusSpec], bus: &mut SystemBus) -> Result<(), StimulusValidationError>;

/// At-start application plus once-only `after_cycles` firing.
pub struct StimulusTrack { /* specs, fired flags */ }

impl StimulusTrack {
    pub fn new(specs: &[StimulusSpec]) -> Self;
    pub fn apply_at_start<C: Cpu>(&mut self, machine: &mut Machine<C>);
    pub fn next_deadline(&self) -> Option<u64>;
    pub fn poll<C: Cpu>(&mut self, machine: &mut Machine<C>, cycles: u64);
}
```

`execute_test_loop` (crates/cli/src/main.rs:1487-1509, 1601-1615, 1656-1666) is
refactored onto `StimulusTrack`: the inline `apply_stimulus` closure, the
`pending_stimuli` vector, the per-iteration firing check, and the batch limiting
to the next deadline are replaced by `apply_at_start` / `poll` / `next_deadline`
calls. This is behavior-preserving, including log-and-continue when a set fails
at fire time.

## Data Flow

```
argv
  → clap parses --system / --stimulus (raw strings)
  → parse_stimulus_arg per entry → Vec<StimulusSpec>
  → build_system_bus(manifest) → manifest.chip resolved to chip YAML
  → load ELF → arch from chip descriptor → Machine
  → validate_specs(list_inputs)        [fail fast, exit config error]
  → StimulusTrack::apply_at_start
  → loop: Machine::step; StimulusTrack::poll(machine, metrics.cycles)
  → existing stop-reason/exit-code mapping, UART streamed as today
```

## Error Handling

| Failure | Exit | Message / payload |
| --- | --- | --- |
| Malformed JSON, unknown field, missing `channel`/`value`, non-finite value | `EXIT_CONFIG_ERROR` | `--stimulus[i]: <detail>`; JSON payload under `--json` |
| Unknown channel | `EXIT_CONFIG_ERROR` | human message + `available: [{component, channel}]` inventory under `--json` |
| Ambiguous bare channel | `EXIT_CONFIG_ERROR` | same, plus candidate components for that channel |
| Out-of-range value | `EXIT_CONFIG_ERROR` | channel, accepted range from `InputChannel` metadata |
| `--stimulus` without `--system` | `EXIT_CONFIG_ERROR` | "stimuli target devices from a system manifest; pass --system" |
| Set fails at fire time (should not pass pre-validation) | run continues | `error!` log line, matching current test-loop semantics |

## Testing

Unit tests (`crates/cli/src/stimuli.rs`):

- Argument parsing: valid entity, missing fields, unknown field, non-finite
  value, `after_cycles: 0` → `AtStart`, negative cycles rejected, index in
  error messages.
- Validation against a fixture bus: unique channel accepted,
  component-disambiguated accepted, ambiguous rejected with candidates,
  unknown rejected with inventory.

Integration test (`crates/cli/tests/e2e_run_stimulus.rs`):

- KW41Z cow activity: `run --system configs/systems/frdm-kw41z-lcd.yaml
  --firmware tests/fixtures/kw41z-lcd-activity.elf
  --stimulus '{"component":"fxos8700","channel":"x","value":2.0,"after_cycles":3000000}'`
  produces `MOOD=CALM` then `MOOD=ACTIVE` on stdout — the same observable as
  the existing `e2e_kw41z_cow_stimulus` test-script run.
- Fail-fast: unknown channel exits non-zero; with `--json` the error lists
  `fxos8700:x` in the inventory.
- Regression: `run` without `--system` is unchanged (existing suite).

Existing coverage for the `execute_test_loop` refactor: `e2e_kw41z_cow_stimulus`
plus the other stimulus-bearing test-script e2e tests must pass unchanged.

## Validation Plan

From `core/`:

```console
cargo test -p labwired-cli
cargo test -p labwired-cli --test e2e_run_stimulus -- --nocapture
cargo clippy -p labwired-cli -- -D warnings
cargo fmt --all -- --check
```

## Documentation

- `--help` text for `--system` and `--stimulus` states the JSON shape and the
  `--stimulus` requires `--system` rule.
- The KW41Z e2e command is the canonical example for agents; no marketing or
  website docs are in scope.
