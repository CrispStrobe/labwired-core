//! Fixed-size, per-CPU diagnostic counters. No cache or guest state lives here.
//! Instrumented timings are not ordinary RTx or removable-cost estimates.

#[derive(Clone, Copy)]
pub(super) enum Event {
    Calls,
    PositiveReuse,
    NoPositiveSpan,
    MemoHit,
    MemoEmpty,
    MemoCollision,
    MemoSamePcStale,
    MemoDifferentPcStale,
    AdmissionRefused,
    SearchIterations,
    CompileAttempts,
    Discovered,
    SearchExhausted,
    RuntimeRejected,
    ExecutedOps,
    BranchExit,
    BudgetExit,
    DecodeInsert,
    DecodeClear,
    GenerationWrap,
}

const NAMES: [&str; 20] = [
    "calls",
    "positive_reuse",
    "no_positive_span",
    "memo_hit",
    "memo_empty",
    "memo_same_generation_collision",
    "memo_same_pc_stale_generation",
    "memo_different_pc_stale_generation",
    "admission_refused",
    "search_iterations",
    "compile_attempts",
    "discovered",
    "search_exhausted",
    "runtime_rejected",
    "executed_ops",
    "branch_exit",
    "budget_exit",
    "decode_insert",
    "decode_clear",
    "generation_wrap",
];

#[derive(Default, Debug)]
pub(super) struct Census {
    active: bool,
    counts: [u64; 20],
    overflow: bool,
}

impl Census {
    pub(super) fn begin(&mut self) {
        *self = Self {
            active: true,
            ..Self::default()
        };
    }

    pub(super) fn record(&mut self, event: Event) {
        if !self.active {
            return;
        }
        let count = &mut self.counts[event as usize];
        match count.checked_add(1) {
            Some(value) => *count = value,
            None => self.overflow = true,
        }
    }

    pub(super) fn end(&mut self) -> serde_json::Value {
        self.active = false;
        // Decimal strings prevent silently rounding large u64 values in JS.
        let counts: serde_json::Map<String, serde_json::Value> = NAMES
            .iter()
            .zip(self.counts.iter())
            .map(|(name, count)| {
                (
                    name.to_string(),
                    serde_json::Value::String(count.to_string()),
                )
            })
            .collect();
        serde_json::json!({"schema": "labwired.t16-discovery-census.v1",
            "diagnostic_only": true, "overflow": self.overflow, "counts": counts})
    }
}

pub(super) fn classify(slot: (u32, u64), pc: u32, generation: u64) -> Event {
    if slot.1 == 0 {
        Event::MemoEmpty
    } else if slot.1 == generation {
        if slot.0 == pc {
            Event::MemoHit
        } else {
            Event::MemoCollision
        }
    } else if slot.0 == pc {
        Event::MemoSamePcStale
    } else {
        Event::MemoDifferentPcStale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_classification_distinguishes_collision_and_stale_generation() {
        for (slot, expected) in [
            ((0, 0), Event::MemoEmpty),
            ((8, 3), Event::MemoHit),
            ((136, 3), Event::MemoCollision),
            ((8, 2), Event::MemoSamePcStale),
            ((136, 2), Event::MemoDifferentPcStale),
        ] {
            assert_eq!(classify(slot, 8, 3) as usize, expected as usize);
        }
    }

    #[test]
    fn census_runtime_activation_reset_stop_and_decimal_counts() {
        let mut census = Census::default();
        census.record(Event::Calls);
        assert_eq!(census.end()["counts"]["calls"], "0");
        census.begin();
        for event in [Event::Calls, Event::Calls, Event::MemoHit] {
            census.record(event);
        }
        assert_eq!(census.end()["counts"]["calls"], "2");
        census.record(Event::Calls);
        assert_eq!(census.end()["counts"]["calls"], "2");
        census.begin();
        assert_eq!(census.end()["counts"]["memo_hit"], "0");
    }

    #[test]
    fn census_overflow_is_flagged_without_wrapping() {
        let mut census = Census::default();
        census.begin();
        census.counts[Event::Calls as usize] = u64::MAX;
        census.record(Event::Calls);
        let result = census.end();
        assert_eq!(result["overflow"], true);
        assert_eq!(result["counts"]["calls"], u64::MAX.to_string());
    }
}
