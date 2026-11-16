// SPDX-License-Identifier: MIT
//! Opt-in countdown-path counters. They measure simulated instructions, not host time.
use super::CortexM;

/// One caller-defined window. Cycle progress must come from the actual driver,
/// not the requested amount. A snapshot stays incomplete until explicitly ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountdownWindow {
    pub requested_cycles: u64,
    pub completed_cycles: u64,
    pub attempts: u64,
    pub returns: u64,
    pub retired: u64,
    pub detail_limit: u64,
    pub detail_recorded: u64,
    pub overflow: bool,
    pub ended: bool,
}

impl CountdownWindow {
    fn increment(value: &mut u64, amount: u64, overflow: &mut bool) {
        match value.checked_add(amount) {
            Some(n) => *value = n,
            None => {
                *value = u64::MAX;
                *overflow = true;
            }
        }
    }

    fn detail(&mut self) {
        if self.detail_recorded < self.detail_limit {
            self.detail_recorded += 1;
        }
    }

    /// Record an attempted entry to the countdown fast path.
    pub fn record_attempt(&mut self) {
        Self::increment(&mut self.attempts, 1, &mut self.overflow);
        self.detail();
    }

    /// Record its actual returned instruction count, including zero.
    pub fn record_return(&mut self, retired: u32) {
        Self::increment(&mut self.returns, 1, &mut self.overflow);
        Self::increment(&mut self.retired, u64::from(retired), &mut self.overflow);
        self.detail();
    }

    /// The bounded event tally is full; aggregate counters continue afterward.
    /// Event payloads are not captured by this diagnostic.
    pub fn detail_exhausted(&self) -> bool {
        self.detail_recorded == self.detail_limit
    }

    /// An ended window is valid only with exact driver-reported progress and no overflow.
    pub fn complete(&self) -> bool {
        self.ended
            && !self.overflow
            && self.completed_cycles == self.requested_cycles
            && self.attempts == self.returns
    }
}

impl CortexM {
    /// Refuse zero-length windows, zero caps, or replacing a live window.
    pub fn begin_countdown_window(&mut self, requested_cycles: u64, detail_limit: u64) -> bool {
        if requested_cycles == 0 || detail_limit == 0 || self.countdown_probe.is_some() {
            return false;
        }
        self.countdown_probe = Some(CountdownWindow {
            requested_cycles,
            completed_cycles: 0,
            attempts: 0,
            returns: 0,
            retired: 0,
            detail_limit,
            detail_recorded: 0,
            overflow: false,
            ended: false,
        });
        true
    }

    /// Add cycles actually completed by the window driver, after each execution call.
    pub fn countdown_window_progress(&mut self, completed_cycles: u64) -> bool {
        let Some(p) = self.countdown_probe.as_mut() else {
            return false;
        };
        CountdownWindow::increment(&mut p.completed_cycles, completed_cycles, &mut p.overflow);
        true
    }

    /// Inspect without falsely marking an interrupted or still-running window complete.
    pub fn countdown_window(&self) -> Option<&CountdownWindow> {
        self.countdown_probe.as_ref()
    }

    /// End collection and return it even if incomplete, overrun, or overflowed.
    pub fn end_countdown_window(&mut self) -> Option<CountdownWindow> {
        self.countdown_probe.take().map(|mut p| {
            p.ended = true;
            p
        })
    }
}
