//! Finite, owned GPIO edge schedules. All operands are latched before replacement.
use anyhow::{anyhow, ensure, Result};
use labwired_config::{
    GpioScheduleSpec, InputPrecision, ScheduleBitOrder, ScheduleDuration, ScheduleSegment,
    ScheduleTiming,
};
use std::collections::BTreeMap;

pub const MAX_EDGES: usize = 4096;

fn duration_cycles(d: &ScheduleDuration, inputs: &BTreeMap<String, f64>, hz: u64) -> Result<u64> {
    let (us, minimum) = match d {
        ScheduleDuration::Fixed(us) => (*us, 0),
        ScheduleDuration::InputLinear {
            input,
            scale,
            arithmetic,
            min_cycles,
        } => {
            let value = *inputs
                .get(input)
                .ok_or_else(|| anyhow!("missing schedule input '{input}'"))?;
            let us = match arithmetic {
                InputPrecision::F32 => ((value as f32) * (*scale as f32)) as f64,
                InputPrecision::F64 => value * scale,
            };
            (us, *min_cycles)
        }
    };
    ensure!(
        us.is_finite() && us >= 0.0,
        "schedule duration must be finite and nonnegative"
    );
    let cycles = us * (hz as f64 / 1_000_000.0);
    ensure!(
        cycles.is_finite() && cycles < u64::MAX as f64,
        "schedule duration overflows cycles"
    );
    Ok((cycles as u64).max(minimum))
}

/// Validate the maximum expansion before any request can allocate its edges.
pub fn validate(spec: &GpioScheduleSpec) -> Result<()> {
    let mut count = 2usize;
    for segment in &spec.segments {
        let holds: Vec<&labwired_config::ScheduleHold> = match segment {
            ScheduleSegment::Hold { hold } => {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("schedule too large"))?;
                vec![hold]
            }
            ScheduleSegment::Bits { bits } => {
                ensure!(
                    (1..=64).contains(&bits.count),
                    "schedule bit count must be 1..64"
                );
                ensure!(
                    !bits.zero.is_empty() && !bits.one.is_empty(),
                    "schedule bit holds must be nonempty"
                );
                count = count
                    .checked_add(usize::from(bits.count) * bits.zero.len().max(bits.one.len()))
                    .ok_or_else(|| anyhow!("schedule too large"))?;
                bits.zero.iter().chain(&bits.one).collect()
            }
        };
        ensure!(
            count <= MAX_EDGES,
            "schedule expands beyond {MAX_EDGES} edges"
        );
        for hold in holds {
            match &hold.us {
                ScheduleDuration::Fixed(us) => {
                    ensure!(us.is_finite() && *us >= 0.0, "invalid schedule duration")
                }
                ScheduleDuration::InputLinear { input, scale, .. } => ensure!(
                    !input.is_empty() && scale.is_finite() && *scale >= 0.0,
                    "invalid schedule input duration"
                ),
            }
        }
    }
    Ok(())
}

/// Preserve each segment's individual float-to-cycle flooring, including zero.
pub fn expand(
    spec: &GpioScheduleSpec,
    values: &[i64],
    inputs: &BTreeMap<String, f64>,
    hz: u64,
    now: u64,
) -> Result<Vec<(u64, bool)>> {
    validate(spec)?;
    let mut edges = Vec::new();
    let mut at = now;
    let mut level = spec.idle;
    let mut values = values.iter();
    let mut append = |hold: &labwired_config::ScheduleHold| -> Result<()> {
        if hold.level != level {
            edges.push((at, hold.level));
            level = hold.level;
        }
        at = at
            .checked_add(duration_cycles(&hold.us, inputs, hz)?)
            .ok_or_else(|| anyhow!("schedule cumulative deadline overflow"))?;
        Ok(())
    };
    for segment in &spec.segments {
        match segment {
            ScheduleSegment::Hold { hold } => append(hold)?,
            ScheduleSegment::Bits { bits } => {
                let value = *values
                    .next()
                    .ok_or_else(|| anyhow!("missing latched schedule bits"))?
                    as u64;
                for index in 0..bits.count {
                    let shift = match bits.order {
                        ScheduleBitOrder::MsbFirst => bits.count - 1 - index,
                        ScheduleBitOrder::LsbFirst => index,
                    };
                    for hold in if (value >> shift) & 1 == 0 {
                        &bits.zero
                    } else {
                        &bits.one
                    } {
                        append(hold)?;
                    }
                }
            }
        }
    }
    if level != spec.final_level {
        edges.push((at, spec.final_level));
    }
    Ok(edges)
}

#[derive(Debug)]
struct Active {
    output: String,
    timing: ScheduleTiming,
    edges: Vec<(u64, bool)>,
    cursor: usize,
    generation: u64,
}

#[derive(Debug, Default)]
pub struct ScheduleBank {
    active: BTreeMap<String, Active>,
    revision: u64,
}
impl ScheduleBank {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn emit(
        &mut self,
        name: &str,
        spec: &GpioScheduleSpec,
        values: &[i64],
        inputs: &BTreeMap<String, f64>,
        hz: u64,
        now: u64,
    ) -> Result<()> {
        let mut edges = expand(spec, values, inputs, hz, now)?;
        // A replacement's initial idle participates in its own timing policy.
        edges.insert(0, (now, spec.idle));
        self.revision = self.revision.wrapping_add(1);
        self.active.insert(
            name.into(),
            Active {
                output: spec.output.clone(),
                timing: spec.timing,
                edges,
                cursor: 0,
                generation: self.revision,
            },
        );
        Ok(())
    }
    pub fn cancel(&mut self, name: &str, spec: &GpioScheduleSpec, now: u64) {
        self.revision = self.revision.wrapping_add(1);
        self.active.insert(
            name.into(),
            Active {
                output: spec.output.clone(),
                timing: ScheduleTiming::Exact,
                edges: vec![(now, spec.idle)],
                cursor: 0,
                generation: self.revision,
            },
        );
    }
    pub fn next_deadline(&self, interval: u64) -> Option<u64> {
        self.active
            .values()
            .filter_map(|a| {
                let at = a.edges.get(a.cursor)?.0;
                match a.timing {
                    ScheduleTiming::Exact => Some(at),
                    ScheduleTiming::PeripheralTickGrid => {
                        let i = interval.max(1);
                        at.checked_add((i - at % i) % i)
                    }
                }
            })
            .min()
    }
    /// Return only the waveform level visible at the serviced boundary.
    pub fn service(&mut self, now: u64, interval: u64) -> Vec<(String, bool)> {
        let mut due = Vec::new();
        for a in self.active.values_mut() {
            let limit = match a.timing {
                ScheduleTiming::Exact => now,
                ScheduleTiming::PeripheralTickGrid => now / interval.max(1) * interval.max(1),
            };
            while let Some(&(at, level)) = a.edges.get(a.cursor) {
                if at > limit {
                    break;
                }
                due.push((at, a.generation, a.cursor, a.output.clone(), level));
                a.cursor += 1;
            }
        }
        due.sort_by_key(|e| (e.0, e.1, e.2));
        let mut outputs = BTreeMap::new();
        for (_, _, _, output, level) in due {
            outputs.insert(output, level);
        }
        outputs.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(segments: &str) -> GpioScheduleSpec {
        serde_yaml::from_str(&format!(
            "output: DATA\nidle: true\nfinal: true\ntiming: exact\nsegments:\n{segments}"
        ))
        .unwrap()
    }
    #[test]
    fn offgrid_replacement_keeps_visible_level_until_grid_and_cancel_is_immediate() {
        let mut s = spec("  - hold: {level: true, us: 3}\n  - hold: {level: false, us: 40}");
        s.timing = ScheduleTiming::PeripheralTickGrid;
        let mut bank = ScheduleBank::default();
        bank.emit("a", &s, &[], &BTreeMap::new(), 1_000_000, 0)
            .unwrap();
        assert_eq!(bank.service(10, 10), vec![("DATA".into(), false)]);
        bank.emit("a", &s, &[], &BTreeMap::new(), 1_000_000, 11)
            .unwrap();
        assert!(bank.service(11, 10).is_empty());
        assert_eq!(bank.next_deadline(10), Some(20));
        bank.cancel("a", &s, 12);
        assert_eq!(bank.service(12, 10), vec![("DATA".into(), true)]);
    }
    #[test]
    fn input_linear_f32_width_is_floored_after_promotion_and_minimum_is_applied() {
        let s = spec("  - hold: {level: true, us: 200}\n  - hold: {level: false, us: {input: distance, scale: 58, arithmetic: f32, min_cycles: 1}}");
        let inputs = BTreeMap::from([("distance".into(), 50.125)]);
        let edges = expand(&s, &[], &inputs, 1_500_001, 7).unwrap();
        assert_eq!(edges[0].0, 7 + 300);
        assert_eq!(
            edges[1].0 - edges[0].0,
            ((50.125_f32 * 58.0) as f64 * (1_500_001.0 / 1e6)) as u64
        );
        let edges = expand(&s, &[], &inputs, 1, 7).unwrap();
        assert_eq!(edges, vec![(7, false), (8, true)]);
    }
    #[test]
    fn replacement_failure_is_atomic_and_cancel_invalidates_edges() {
        let s = spec("  - hold: {level: true, us: 3}\n  - hold: {level: false, us: 4}");
        let mut bank = ScheduleBank::default();
        bank.emit("a", &s, &[], &BTreeMap::new(), 1_000_000, 10)
            .unwrap();
        let revision = bank.revision();
        assert!(bank
            .emit("a", &s, &[], &BTreeMap::new(), 1_000_000, u64::MAX - 1)
            .is_err());
        assert_eq!(bank.revision(), revision);
        assert_eq!(bank.service(13, 1), vec![("DATA".into(), false)]);
        bank.cancel("a", &s, 14);
        assert_eq!(bank.service(14, 1), vec![("DATA".into(), true)]);
        assert_eq!(bank.next_deadline(1), None);
    }
    #[test]
    fn grid_collapses_pulses_and_recomputes_deadlines_on_interval_change() {
        let mut s = spec("  - hold: {level: true, us: 3}\n  - hold: {level: false, us: 4}");
        s.timing = ScheduleTiming::PeripheralTickGrid;
        let mut bank = ScheduleBank::default();
        bank.emit("a", &s, &[], &BTreeMap::new(), 1_000_000, 11)
            .unwrap();
        assert_eq!(bank.next_deadline(10), Some(20));
        assert_eq!(bank.next_deadline(4), Some(12));
        assert!(bank.service(19, 10).is_empty());
        assert_eq!(bank.service(20, 10), vec![("DATA".into(), true)]);
        assert_eq!(bank.next_deadline(10), None);
    }
    #[test]
    fn fractional_and_low_clocks_floor_each_segment_with_last_writer_winning() {
        let s = spec("  - hold: {level: true, us: 3}\n  - hold: {level: false, us: 4}");
        assert_eq!(
            expand(&s, &[], &BTreeMap::new(), 1_500_001, 0).unwrap(),
            vec![(4, false), (10, true)]
        );
        let mut bank = ScheduleBank::default();
        bank.emit("a", &s, &[], &BTreeMap::new(), 1, 10).unwrap();
        assert_eq!(bank.service(10, 1), vec![("DATA".into(), true)]);
    }
    #[test]
    fn equal_deadlines_respect_latest_generation_across_two_schedules() {
        let a = spec("  - hold: {level: true, us: 3}\n  - hold: {level: false, us: 4}");
        let b = spec("  - hold: {level: true, us: 30}");
        let mut bank = ScheduleBank::default();
        bank.emit("z", &a, &[], &BTreeMap::new(), 1_000_000, 0)
            .unwrap();
        bank.emit("a", &b, &[], &BTreeMap::new(), 1_000_000, 3)
            .unwrap();
        assert_eq!(bank.service(3, 1), vec![("DATA".into(), true)]);
    }
    #[test]
    fn bounds_and_nonfinite_or_negative_durations_are_rejected() {
        for duration in ["-1", ".nan", ".inf"] {
            let s = spec(&format!("  - hold: {{level: true, us: {duration}}}"));
            assert!(expand(&s, &[], &BTreeMap::new(), 1_000_000, 0).is_err());
        }
        for count in [0, 65] {
            let s = spec(&format!("  - bits: {{value: '0', count: {count}, order: msb_first, zero: [{{level: false, us: 1}}], one: [{{level: true, us: 1}}]}}"));
            assert!(validate(&s).is_err());
        }
        let mut s = spec("  - hold: {level: true, us: 1}");
        s.segments = vec![s.segments[0].clone(); MAX_EDGES];
        assert!(validate(&s).is_err());
    }
    #[test]
    fn dht_has_84_transitions_and_last_bit_keeps_its_width() {
        for value in [0, 1] {
            let s = spec("  - hold: {level: true, us: 30}\n  - hold: {level: false, us: 80}\n  - hold: {level: true, us: 80}\n  - bits:\n      value: '0'\n      count: 40\n      order: msb_first\n      zero: [{level: false, us: 50}, {level: true, us: 27}]\n      one: [{level: false, us: 50}, {level: true, us: 70}]\n  - hold: {level: false, us: 50}");
            let edges = expand(&s, &[value], &BTreeMap::new(), 1_000_000, 100).unwrap();
            assert_eq!(edges.len(), 84);
            assert_eq!(edges[0], (130, false));
            assert_eq!(edges[1], (210, true));
            let n = edges.len();
            assert_eq!(
                edges[n - 2].0 - edges[n - 3].0,
                if value == 0 { 27 } else { 70 }
            );
            assert_eq!(edges[n - 1].0 - edges[n - 2].0, 50);
            assert!(edges[n - 1].1);
        }
    }
}
