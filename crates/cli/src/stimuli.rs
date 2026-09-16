// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Command-line stimulus surface: parse `--stimulus` JSON arguments into the
//! shared `StimulusSpec` type and drive them through `Machine::set_input` at
//! `at_start` / `after_cycles`. Also used by `execute_test_loop`.

use labwired_config::{FaultTrigger, StimulusSpec, StimulusTarget};
use labwired_core::{Cpu, Machine};
use serde::Deserialize;
use tracing::{error, info};

/// One `--stimulus` argument. Field names are the agent-facing MCP schema
/// (`channel`, `value`, `after_cycles`, `component`); unknown fields are
/// rejected so typos fail loudly instead of silently resetting to a default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliStimulus {
    pub channel: String,
    pub value: f64,
    #[serde(default)]
    pub after_cycles: Option<u64>,
    #[serde(default)]
    pub component: Option<String>,
}

/// Parse one `--stimulus` argument into the shared `StimulusSpec` type.
/// `index` is the argument's position in `--stimulus` order and is included in
/// every error so a caller can fix the right entry.
pub fn parse_stimulus_arg(index: usize, raw: &str) -> Result<StimulusSpec, String> {
    let parsed: CliStimulus =
        serde_json::from_str(raw).map_err(|e| format!("--stimulus[{index}]: invalid JSON: {e}"))?;
    if parsed.channel.trim().is_empty() {
        return Err(format!("--stimulus[{index}]: channel cannot be empty"));
    }
    let trigger = match parsed.after_cycles {
        Some(cycles) if cycles > 0 => FaultTrigger::AfterCycles { cycles },
        _ => FaultTrigger::AtStart,
    };
    Ok(StimulusSpec {
        target: StimulusTarget {
            component: parsed.component,
            channel: parsed.channel,
        },
        trigger,
        value: parsed.value,
    })
}

/// Apply one stimulus through the generic `Machine::set_input` path. The log
/// strings are asserted by `e2e_kw41z_cow_stimulus.rs`; do not reword them.
pub fn apply_spec<C: Cpu>(machine: &mut Machine<C>, s: &StimulusSpec) {
    let result = match s.target.component.as_deref() {
        Some(component) => machine.set_input_on(component, &s.target.channel, s.value),
        None => machine.set_input(&s.target.channel, s.value),
    };
    match result {
        Ok(()) => info!("stimulus: {} = {} applied", s.target.channel, s.value),
        Err(e) => error!(
            "stimulus '{}' = {} could not be applied: {:?}",
            s.target.channel, s.value, e
        ),
    }
}

/// At-start application plus once-only `after_cycles` firing, shared by the
/// test runner and the `run --system` driver.
pub struct StimulusTrack {
    specs: Vec<StimulusSpec>,
    at_start_applied: bool,
    fired: Vec<bool>,
}

impl StimulusTrack {
    pub fn new(specs: &[StimulusSpec]) -> Self {
        Self {
            specs: specs.to_vec(),
            at_start_applied: false,
            fired: vec![false; specs.len()],
        }
    }

    /// Apply every `at_start` spec. Idempotent.
    pub fn apply_at_start<C: Cpu>(&mut self, machine: &mut Machine<C>) {
        if self.at_start_applied {
            return;
        }
        for s in &self.specs {
            if matches!(s.trigger, FaultTrigger::AtStart) {
                apply_spec(machine, s);
            }
        }
        self.at_start_applied = true;
    }

    /// Earliest unfired `after_cycles` threshold. Both loops clamp their batch
    /// to this deadline so a stimulus fires at its exact cycle.
    pub fn next_deadline(&self) -> Option<u64> {
        self.specs
            .iter()
            .zip(&self.fired)
            .filter_map(|(s, fired)| {
                if *fired {
                    return None;
                }
                match s.trigger {
                    FaultTrigger::AfterCycles { cycles } => Some(cycles),
                    _ => None,
                }
            })
            .min()
    }

    /// Apply every not-yet-fired `after_cycles` spec whose threshold `cycles`
    /// has reached.
    pub fn poll<C: Cpu>(&mut self, machine: &mut Machine<C>, cycles: u64) {
        if !self.has_pending() {
            return;
        }
        for i in self.due(cycles) {
            apply_spec(machine, &self.specs[i]);
        }
    }

    fn has_pending(&self) -> bool {
        self.specs
            .iter()
            .zip(&self.fired)
            .any(|(s, fired)| !*fired && matches!(s.trigger, FaultTrigger::AfterCycles { .. }))
    }

    /// Mark and return the indices newly due at `cycles` (test seam).
    fn due(&mut self, cycles: u64) -> Vec<usize> {
        let mut due = Vec::new();
        for (i, (s, fired)) in self.specs.iter().zip(self.fired.iter_mut()).enumerate() {
            if *fired {
                continue;
            }
            if let FaultTrigger::AfterCycles { cycles: threshold } = s.trigger {
                if cycles >= threshold {
                    *fired = true;
                    due.push(i);
                }
            }
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use labwired_config::FaultTrigger;

    #[test]
    fn parses_the_agent_wire_shape() {
        let s = parse_stimulus_arg(
            0,
            r#"{"component":"fxos8700","channel":"x","value":2.0,"after_cycles":3000000}"#,
        )
        .expect("parse stimulus");
        assert_eq!(s.target.component.as_deref(), Some("fxos8700"));
        assert_eq!(s.target.channel, "x");
        assert_eq!(s.value, 2.0);
        assert_eq!(s.trigger, FaultTrigger::AfterCycles { cycles: 3_000_000 });
    }

    #[test]
    fn omitted_or_zero_after_cycles_is_at_start() {
        let a = parse_stimulus_arg(0, r#"{"channel":"x","value":1.0}"#).expect("parse");
        assert_eq!(a.trigger, FaultTrigger::AtStart);
        let b = parse_stimulus_arg(0, r#"{"channel":"x","value":1.0,"after_cycles":0}"#)
            .expect("parse");
        assert_eq!(b.trigger, FaultTrigger::AtStart);
    }

    #[test]
    fn bad_json_reports_the_argument_index() {
        let err = parse_stimulus_arg(2, r#"{"channel":"x","value":1.0,"afterCycle":5}"#)
            .expect_err("unknown field must fail");
        assert!(err.starts_with("--stimulus[2]:"), "{err}");

        let err = parse_stimulus_arg(1, "not json").expect_err("malformed must fail");
        assert!(err.starts_with("--stimulus[1]:"), "{err}");

        let err = parse_stimulus_arg(0, r#"{"channel":"x","value":1.0,"after_cycles":-5}"#)
            .expect_err("negative cycles must fail");
        assert!(err.starts_with("--stimulus[0]:"), "{err}");
    }

    #[test]
    fn empty_channel_is_rejected() {
        let err = parse_stimulus_arg(0, r#"{"channel":"","value":1.0}"#).expect_err("empty");
        assert!(err.contains("channel"), "{err}");
    }

    fn spec(channel: &str, trigger: FaultTrigger) -> StimulusSpec {
        StimulusSpec {
            target: StimulusTarget {
                component: None,
                channel: channel.to_string(),
            },
            trigger,
            value: 1.0,
        }
    }

    #[test]
    fn track_fires_each_spec_once_and_reports_deadlines() {
        let specs = vec![
            spec("x", FaultTrigger::AfterCycles { cycles: 100 }),
            spec("y", FaultTrigger::AfterCycles { cycles: 50 }),
            spec("z", FaultTrigger::AtStart),
        ];
        let mut track = StimulusTrack::new(&specs);

        assert_eq!(track.next_deadline(), Some(50));
        assert_eq!(track.due(49), Vec::<usize>::new());
        assert_eq!(track.due(50), vec![1]);
        assert_eq!(track.next_deadline(), Some(100));
        assert_eq!(track.due(200), vec![0]);
        assert_eq!(track.next_deadline(), None, "fired specs never re-arm");
        assert_eq!(track.due(500), Vec::<usize>::new());
    }
}
