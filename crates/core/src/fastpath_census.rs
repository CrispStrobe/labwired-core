//! Off-by-default occurrence diagnostics, NOT time attribution or qualification.
//!
//! Each thread has one aggregate window: callers must reset between workloads
//! and avoid interleaving machines if they need per-machine counts. No guest
//! state, clocks, routing or control flow is modified. Recording arguments are
//! not evaluated in feature-off builds, which contain neither state nor exports.
//! Enabling this feature adds overhead and changes code layout; its throughput
//! must never be compared with an uninstrumented build as optimization evidence.

#[cfg(feature = "fastpath-census")]
#[macro_export]
macro_rules! fastpath_count {
    ($counter:ident, $amount:expr) => {
        $crate::fastpath_census::record($crate::fastpath_census::Counter::$counter, $amount as u64)
    };
}

#[cfg(not(feature = "fastpath-census"))]
#[macro_export]
macro_rules! fastpath_count {
    ($counter:ident, $amount:expr) => {
        ()
    };
}

#[cfg(feature = "fastpath-census")]
mod enabled {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    macro_rules! counters {
        ($($name:ident),+ $(,)?) => {
            #[derive(Clone, Copy)]
            #[repr(usize)]
            pub enum Counter { $($name),+ }
            const NAMES: &[&str] = &[$(stringify!($name)),+];
        };
    }
    counters!(
        FastCalls,
        FastColdEntry,
        FastZero,
        FastRetired,
        BlockCalls,
        BlockCacheHits,
        BlockMemoizedMisses,
        BlockAdmissionRejects,
        BlockCompileAttempts,
        BlockDiscoveryMisses,
        BlockDiscovered,
        BlockExecutionRejects,
        BlockZero,
        BlockRetired,
        CachedRunCalls,
        CachedRunCacheStops,
        CachedRunExecutionRejects,
        CachedRunZero,
        CachedRunRetired,
        OrdinaryRetired,
        GpioColdCalls,
        GpioNoEdgeDevices,
        GpioDevicesVisited,
        GpioHostedServices,
    );
    thread_local! {
        static COUNTS: RefCell<[u64; NAMES.len()]> = const { RefCell::new([0; NAMES.len()]) };
    }

    #[inline]
    pub fn record(counter: Counter, amount: u64) {
        COUNTS.with(|counts| {
            let mut counts = counts.borrow_mut();
            let value = &mut counts[counter as usize];
            *value = value.saturating_add(amount);
        });
    }

    /// Clear this thread's aggregate diagnostic window.
    pub fn reset() {
        COUNTS.with(|counts| counts.borrow_mut().fill(0));
    }

    /// Copy without resetting; string-key map allocation is outside the hot path.
    pub fn snapshot() -> BTreeMap<&'static str, u64> {
        COUNTS.with(|counts| NAMES.iter().copied().zip(*counts.borrow()).collect())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reset_snapshot_saturation_and_thread_isolation() {
            reset();
            assert!(snapshot().values().all(|count| *count == 0));
            crate::fastpath_count!(FastCalls, 7);
            assert_eq!(snapshot()["FastCalls"], 7);
            assert_eq!(snapshot()["FastCalls"], 7); // Non-destructive.
            std::thread::spawn(|| {
                assert_eq!(snapshot()["FastCalls"], 0);
                crate::fastpath_count!(FastCalls, 3);
                assert_eq!(snapshot()["FastCalls"], 3);
            })
            .join()
            .unwrap();
            assert_eq!(snapshot()["FastCalls"], 7);
            crate::fastpath_count!(FastCalls, u64::MAX);
            assert_eq!(snapshot()["FastCalls"], u64::MAX);
            reset();
            assert!(snapshot().values().all(|count| *count == 0));
        }
    }
}

#[cfg(feature = "fastpath-census")]
pub use enabled::*;

#[cfg(all(test, not(feature = "fastpath-census")))]
mod tests {
    #[test]
    fn disabled_arguments_are_not_evaluated_or_resolved() {
        crate::fastpath_count!(NotEvenARealCounter, panic!("must not evaluate"));
    }
}
