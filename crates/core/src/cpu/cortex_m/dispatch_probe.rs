// SPDX-License-Identifier: MIT
//! Finite diagnostic prefix, never enabled in an ordinary build. No bus reads,
//! logging, clock reads, synchronization or allocation during recording.
use super::{CortexM, Instruction};

pub const MAX_EVENTS: u32 = 1_000_000;
pub const PC_SLOTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    Dispatch,
    SelfBranch,
    Countdown,
    StoreSpin,
    RamFast,
    Block,
    CachedRun,
    Discovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Reason {
    Enter,
    Returned,
    CacheUnavailable,
    EntryGuard,
    Cold,
    TagMismatch,
    Wide,
    BranchShape,
    WindowLimit,
    BlockHit,
    BlockMiss,
    MemoizedMiss,
    InitialAdmissionRejected,
    InitialAdmissionPassed,
    SearchAdmissionRejected,
    DuplicateAdmissionPassed,
    DuplicateAdmissionRejected,
    SearchAdmissionPassed,
    ShapeNotFound,
    ShapeFound,
    RamRefusal,
    Unsupported,
    BranchExit,
}

const STAGES: usize = Stage::Discovery as usize + 1;
const REASONS: usize = Reason::BranchExit as usize + 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub events: u64,
    /// Only `Returned` events contribute. Stages nest; do not sum stages.
    pub retired: u64,
    pub zero_returns: u64,
    pub partial_stops: u64,
    pub stopped_prefix_retired: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rejection {
    pub stage: Stage,
    pub reason: Reason,
    /// PC at the observation point (the failing PC for cached runs).
    pub pc: u32,
    /// Raw opcode only when the existing cache tag matches PC. No fetch.
    pub opcode: Option<u32>,
    pub count: u64,
    pub prefix_retired: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchProbe {
    pub(crate) remaining: u32,
    limit: u32,
    recorded: u32,
    counts: [[Counts; REASONS]; STAGES],
    rejections: [Option<Rejection>; PC_SLOTS],
    untracked_rejections: u64,
}

impl DispatchProbe {
    fn new(limit: u32) -> Self {
        Self {
            remaining: limit,
            limit,
            recorded: 0,
            counts: [[Counts::default(); REASONS]; STAGES],
            rejections: [None; PC_SLOTS],
            untracked_rejections: 0,
        }
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }
    pub fn recorded(&self) -> u32 {
        self.recorded
    }
    /// At the cap, aggregates may end part way through an executor invocation.
    /// Absence of a final `Returned` is then truncation, not zero retirement.
    pub fn exhausted(&self) -> bool {
        self.remaining == 0
    }
    pub fn counts(&self, stage: Stage, reason: Reason) -> Counts {
        self.counts[stage as usize][reason as usize]
    }
    /// Exact first 64 distinct rejected keys, not a top-N estimate. Counts for
    /// retained keys stay exact in the prefix; overflow is explicit.
    pub fn rejections(&self) -> impl Iterator<Item = &Rejection> {
        self.rejections.iter().flatten()
    }
    pub fn untracked_rejections(&self) -> u64 {
        self.untracked_rejections
    }

    fn record(&mut self, stage: Stage, reason: Reason, pc: u32, opcode: Option<u32>, retired: u32) {
        if self.remaining == 0 {
            return;
        }
        self.remaining -= 1;
        self.recorded += 1;
        let counts = &mut self.counts[stage as usize][reason as usize];
        counts.events = counts.events.saturating_add(1);
        if reason == Reason::Returned {
            counts.retired = counts.retired.saturating_add(u64::from(retired));
            counts.zero_returns = counts.zero_returns.saturating_add(u64::from(retired == 0));
        }
        if !matches!(
            reason,
            Reason::CacheUnavailable
                | Reason::EntryGuard
                | Reason::Cold
                | Reason::TagMismatch
                | Reason::Wide
                | Reason::BranchShape
                | Reason::WindowLimit
                | Reason::MemoizedMiss
                | Reason::InitialAdmissionRejected
                | Reason::DuplicateAdmissionRejected
                | Reason::SearchAdmissionRejected
                | Reason::ShapeNotFound
                | Reason::RamRefusal
                | Reason::Unsupported
        ) {
            return;
        }
        counts.partial_stops = counts.partial_stops.saturating_add(u64::from(retired != 0));
        counts.stopped_prefix_retired = counts
            .stopped_prefix_retired
            .saturating_add(u64::from(retired));
        for slot in &mut self.rejections {
            match slot {
                Some(row)
                    if (row.stage, row.reason, row.pc, row.opcode)
                        == (stage, reason, pc, opcode) =>
                {
                    row.count = row.count.saturating_add(1);
                    row.prefix_retired = row.prefix_retired.saturating_add(u64::from(retired));
                    return;
                }
                None => {
                    *slot = Some(Rejection {
                        stage,
                        reason,
                        pc,
                        opcode,
                        count: 1,
                        prefix_retired: u64::from(retired),
                    });
                    return;
                }
                _ => {}
            }
        }
        self.untracked_rejections = self.untracked_rejections.saturating_add(1);
    }
}

impl CortexM {
    /// Explicit runtime opt-in, with a finite prefix cap. Returns false without
    /// changing an existing collection for invalid limits. Re-enable resets it.
    pub fn enable_dispatch_probe(&mut self, event_limit: u32) -> bool {
        if event_limit == 0 || event_limit > MAX_EVENTS {
            return false;
        }
        *self.dispatch_probe.get_mut() = Some(Box::new(DispatchProbe::new(event_limit)));
        true
    }

    /// Stop collection and transfer ownership; no guest or snapshot mutation.
    pub fn take_dispatch_probe(&mut self) -> Option<Box<DispatchProbe>> {
        self.dispatch_probe.get_mut().take()
    }

    fn record_dispatch_probe(&self, stage: Stage, reason: Reason, retired: u32) {
        self.record_dispatch_probe_at(stage, reason, self.pc, retired);
    }

    fn record_dispatch_probe_at(&self, stage: Stage, reason: Reason, pc: u32, retired: u32) {
        // No callback or bus operation occurs while the private cell is borrowed;
        // callers cannot hold its borrow. Single-CPU recording cannot reenter.
        let mut cell = self.dispatch_probe.borrow_mut();
        let Some(probe) = cell.as_mut() else {
            return;
        };
        if probe.remaining == 0 {
            return;
        }
        let opcode = self.decoded_entry(pc).map(|entry| entry.opcode);
        probe.record(stage, reason, pc, opcode, retired);
    }

    fn probe_discovery_entry(&self, pc: u32) {
        // Check collection first; no cache inspection after exhaustion.
        {
            let cell = self.dispatch_probe.borrow();
            if !cell.as_ref().is_some_and(|p| p.remaining != 0) {
                return;
            }
        }
        let reason = match self.decode_cache[((pc >> 1) & 0x0fff) as usize].as_ref() {
            None => Some(Reason::Cold),
            Some(entry) if entry.tag != pc => Some(Reason::TagMismatch),
            Some(entry) if entry.pc_increment != 2 => Some(Reason::Wide),
            _ => None,
        };
        if let Some(reason) = reason {
            self.record_dispatch_probe_at(Stage::Discovery, reason, pc, 0);
        }
    }

    fn probe_admission_rejection(&self, pc: u32) {
        self.probe_discovery_entry(pc);
        // Called only after the actual admission returned false. A tagged T16
        // entry here can have failed only the existing supported-op predicate.
        {
            let cell = self.dispatch_probe.borrow();
            if !cell.as_ref().is_some_and(|p| p.remaining != 0) {
                return;
            }
        }
        if self
            .decoded_entry(pc)
            .is_some_and(|entry| entry.pc_increment == 2)
        {
            self.record_dispatch_probe_at(Stage::Discovery, Reason::Unsupported, pc, 0);
        }
    }

    fn probe_op_refusal(op: Instruction) -> Reason {
        // Branches are separately handled and never return false. All other
        // supported ops can refuse only the existing live-RAM/write guard.
        // This deliberately does not infer MMIO vs unmapped vs protected RAM.
        if Self::t16_block_op_supported(op) {
            Reason::RamRefusal
        } else {
            Reason::Unsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_overflow_retained_keys_and_zero_returns_are_exact() {
        let mut p = DispatchProbe::new(1000);
        for pc in 0..65 {
            p.record(Stage::CachedRun, Reason::Unsupported, pc, Some(0xbe00), 0);
        }
        p.record(Stage::CachedRun, Reason::Unsupported, 0, Some(0xbe00), 0);
        p.record(Stage::CachedRun, Reason::Returned, 0, None, 0);
        p.record(Stage::CachedRun, Reason::Returned, 0, None, 7);
        assert_eq!(p.rejections().count(), 64);
        assert_eq!(p.rejections().next().unwrap().count, 2);
        assert_eq!(p.untracked_rejections(), 1);
        assert_eq!(
            p.counts(Stage::CachedRun, Reason::Returned),
            Counts {
                events: 2,
                retired: 7,
                zero_returns: 1,
                ..Counts::default()
            }
        );
        let mut capped = DispatchProbe::new(1);
        capped.record(Stage::Block, Reason::Enter, 0, None, 0);
        let frozen = capped.clone();
        capped.record(Stage::Block, Reason::Returned, 2, None, 500);
        assert_eq!(capped, frozen);
        assert!(capped.exhausted());
    }

    fn cache(cpu: &mut CortexM, pc: u32, opcode: u16) {
        cpu.insert_decoded_entry(super::super::DecodeCacheEntry {
            tag: pc,
            opcode: u32::from(opcode),
            pc_increment: 2,
            instruction: super::super::decode_thumb_16(opcode),
            cycles: 1,
        });
    }

    #[test]
    fn cached_run_records_partial_zero_tag_width_and_live_ram_refusal() {
        use crate::bus::SystemBus;
        for case in 0..7 {
            let mut cpu = CortexM::new();
            let mut bus = SystemBus::new();
            cpu.pc = 0x100;
            cpu.r1 = 0x40000000;
            cache(&mut cpu, 0x100, 0x3001); // ADDS then failing instruction
            cache(&mut cpu, 0x102, if case == 0 { 0x6808 } else { 0xbe00 });
            if case == 2 {
                cpu.decode_cache[0x81] = None;
            }
            if case == 3 {
                cpu.decode_cache[0x81].as_mut().unwrap().tag = 0x2102;
            }
            if case == 4 {
                cpu.decode_cache[0x81].as_mut().unwrap().pc_increment = 4;
            }
            if case == 5 {
                cpu.pc = 0x102;
            }
            let budget = if case == 6 { 0 } else { 16 };
            cpu.enable_dispatch_probe(100);
            let counts = bus.access_counts();
            let retired = cpu.run_t16_cached_run(&mut bus, budget);
            assert_eq!(retired, if case >= 5 { 0 } else { 1 });
            assert_eq!(bus.access_counts(), counts);
            let p = cpu.take_dispatch_probe().unwrap();
            let expected = match case {
                0 => Reason::RamRefusal,
                2 => Reason::Cold,
                3 => Reason::TagMismatch,
                4 => Reason::Wide,
                6 => Reason::EntryGuard,
                _ => Reason::Unsupported,
            };
            assert_eq!(
                p.counts(Stage::CachedRun, expected).events,
                1,
                "case={case}"
            );
            if case != 6 {
                assert_eq!(
                    p.counts(Stage::CachedRun, Reason::Returned).retired,
                    u64::from(retired)
                );
                assert_eq!(p.rejections().next().unwrap().pc, 0x102);
            }
        }
    }

    #[test]
    fn probe_does_not_change_cached_execution_or_bus_state_across_cap() {
        use crate::{bus::SystemBus, Cpu};
        for cap in [1, 2, 3, 7, 100] {
            for mmio in [false, true] {
                let mut observed = CortexM::new();
                let mut plain = CortexM::new();
                let mut a = SystemBus::new();
                let mut b = SystemBus::new();
                for cpu in [&mut observed, &mut plain] {
                    cpu.pc = 0x100;
                    cpu.r1 = if mmio { 0x40000000 } else { 0x20000100 };
                    for (i, op) in [0x3001, 0x6008, 0x680a, 0xe7fb].into_iter().enumerate() {
                        cache(cpu, 0x100 + i as u32 * 2, op);
                    }
                }
                assert!(observed.enable_dispatch_probe(cap));
                assert!(!observed.enable_dispatch_probe(0));
                assert!(!observed.enable_dispatch_probe(MAX_EVENTS + 1));
                for budget in [0, 1, 2, 3, 16, 33] {
                    assert_eq!(
                        observed.run_t16_cached_run(&mut a, budget),
                        plain.run_t16_cached_run(&mut b, budget)
                    );
                    assert_eq!(
                        serde_json::to_value(observed.snapshot()).unwrap(),
                        serde_json::to_value(plain.snapshot()).unwrap()
                    );
                    assert_eq!(a.ram.data, b.ram.data);
                    assert_eq!(a.access_counts(), b.access_counts());
                }
                let p = observed.take_dispatch_probe().unwrap();
                assert!(p.recorded() <= cap);
                assert!(observed.take_dispatch_probe().is_none());
            }
        }
    }

    #[test]
    fn block_discovery_memo_and_retirement_accounting() {
        use crate::bus::SystemBus;
        let mut cpu = CortexM::new();
        let mut bus = SystemBus::new();
        cpu.pc = 0x100;
        cache(&mut cpu, 0x100, 0x3001);
        cache(&mut cpu, 0x102, 0xe7fd);
        cpu.enable_dispatch_probe(200);
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 4), 4);
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
        let p = cpu.take_dispatch_probe().unwrap();
        assert_eq!(p.counts(Stage::Block, Reason::BlockMiss).events, 1);
        assert_eq!(p.counts(Stage::Block, Reason::BlockHit).events, 1);
        assert_eq!(p.counts(Stage::Block, Reason::ShapeFound).events, 1);
        assert_eq!(
            p.counts(Stage::Block, Reason::InitialAdmissionPassed)
                .events,
            1
        );
        assert_eq!(
            p.counts(Stage::Block, Reason::DuplicateAdmissionPassed)
                .events,
            1
        );
        assert_eq!(p.counts(Stage::Block, Reason::Returned).retired, 6);
        cache(&mut cpu, 0x100, 0xbe00);
        cpu.t16_fast_block = None;
        cpu.enable_dispatch_probe(200);
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 4), 0);
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 4), 0);
        let p = cpu.take_dispatch_probe().unwrap();
        assert_eq!(
            p.counts(Stage::Block, Reason::InitialAdmissionRejected)
                .events,
            1
        );
        assert_eq!(p.counts(Stage::Block, Reason::MemoizedMiss).events, 1);
    }
    #[test]
    fn discovery_failure_class_matches_actual_compile_boundary() {
        for case in 0..6 {
            let mut cpu = CortexM::new();
            cpu.pc = 0x100;
            cache(&mut cpu, 0x100, 0x3001);
            match case {
                0 => {} // Cold second entry
                1 => {
                    cache(&mut cpu, 0x102, 0xbf00);
                    cpu.decode_cache[0x81].as_mut().unwrap().tag = 0x2102;
                }
                2 => {
                    cache(&mut cpu, 0x102, 0xbf00);
                    cpu.decode_cache[0x81].as_mut().unwrap().pc_increment = 4;
                }
                3 => cache(&mut cpu, 0x102, 0xe000), // Wrong backward target
                4 => cache(&mut cpu, 0x102, 0xbe00), // Unsupported BKPT
                _ => {
                    for i in 1..super::super::T16_FAST_BLOCK_MAX {
                        cache(&mut cpu, 0x100 + i as u32 * 2, 0xbf00);
                    }
                }
            }
            cpu.enable_dispatch_probe(100);
            assert!(cpu.compile_t16_fast_block(0x100).is_none());
            let p = cpu.take_dispatch_probe().unwrap();
            let reason = match case {
                0 => Reason::Cold,
                1 => Reason::TagMismatch,
                2 => Reason::Wide,
                3 => Reason::BranchShape,
                4 => Reason::Unsupported,
                _ => Reason::WindowLimit,
            };
            assert_eq!(p.counts(Stage::Discovery, reason).events, 1, "case={case}");
            assert_eq!(
                p.rejections().next().unwrap().pc,
                if case == 5 { 0x100 } else { 0x102 }
            );
        }
    }
    #[test]
    fn full_dispatch_matches_probe_off_at_rotated_entries_and_all_executor_shapes() {
        use crate::{bus::SystemBus, Cpu};
        let cases: &[&[u16]] = &[
            &[0xe7fe],
            &[0x3801, 0xd1fd],
            &[0x3001, 0x6008, 0x680a, 0xe7fb],
            &[0x9000, 0x4669, 0x1c40, 0xe7fb],
            &[0x2001, 0x3001, 0xe7fc],
            &[0x6808, 0xe7fd],
            &[0xbf30, 0xe7fd],
        ];
        for &ops in cases {
            for phase in 0..ops.len() {
                for budget in [0, 1, 2, 3, 16, 65, 128] {
                    for mmio in [false, true] {
                        for cap in [1, 3, 1000] {
                            let mut observed = CortexM::new();
                            let mut plain = CortexM::new();
                            let mut a = SystemBus::new();
                            let mut b = SystemBus::new();
                            for cpu in [&mut observed, &mut plain] {
                                cpu.pc = 0x100 + phase as u32 * 2;
                                cpu.r0 = 50;
                                cpu.r1 = if mmio { 0x40000000 } else { 0x20000100 };
                                cpu.sp = 0x20000200;
                                for (j, &op) in ops.iter().enumerate() {
                                    cache(cpu, 0x100 + j as u32 * 2, op);
                                }
                            }
                            observed.enable_dispatch_probe(cap);
                            let retired = observed.run_t16_cached_fast_paths(&mut a, budget);
                            assert_eq!(
                                retired,
                                plain.run_t16_cached_fast_paths(&mut b, budget),
                                "ops={ops:x?} phase={phase} budget={budget} mmio={mmio} cap={cap}"
                            );
                            assert_eq!(
                                serde_json::to_value(observed.snapshot()).unwrap(),
                                serde_json::to_value(plain.snapshot()).unwrap()
                            );
                            assert_eq!(a.ram.data, b.ram.data);
                            assert_eq!(a.access_counts(), b.access_counts());
                            let p = observed.take_dispatch_probe().unwrap();
                            assert!(p.recorded() <= cap);
                            if cap == 1000 {
                                assert!(!p.exhausted());
                                let c = p.counts(Stage::Dispatch, Reason::Returned);
                                assert_eq!(c.events, 1);
                                assert_eq!(c.retired, u64::from(retired));
                                assert_eq!(c.zero_returns, u64::from(retired == 0));
                                if ops == [0xe7fe] {
                                    assert_eq!(retired, budget);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dispatcher_refusal_preserves_live_read_clear_irq_and_mmio_hooks() {
        use crate::{bus::SystemBus, peripherals::nvic::NvicState, Peripheral, SimResult};
        use std::sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        };
        #[derive(Debug, Default)]
        struct Hooks {
            read_count: AtomicUsize,
            capability_count: AtomicUsize,
            level_count: AtomicUsize,
            deassert_capability: AtomicBool,
            line: AtomicBool,
        }
        #[derive(Debug)]
        struct ReadClear(Arc<Hooks>);
        impl Peripheral for ReadClear {
            fn read(&self, _offset: u64) -> SimResult<u8> {
                panic!("word read must use word hook")
            }
            fn read_u32(&self, _offset: u64) -> SimResult<u32> {
                self.0.read_count.fetch_add(1, Ordering::SeqCst);
                self.0.line.store(false, Ordering::SeqCst);
                Ok(0x12345678)
            }
            fn write(&mut self, _offset: u64, _value: u8) -> SimResult<()> {
                panic!("no write expected")
            }
            fn reads_can_deassert_irq(&self) -> bool {
                self.0.capability_count.fetch_add(1, Ordering::SeqCst);
                self.0.deassert_capability.load(Ordering::SeqCst)
            }
            fn irq_line_level(&self) -> Option<bool> {
                self.0.level_count.fetch_add(1, Ordering::SeqCst);
                Some(self.0.line.load(Ordering::SeqCst))
            }
        }
        for enabled in [false, true] {
            for optimized in [false, true] {
                let mut cpu = CortexM::new();
                let mut bus = SystemBus::new();
                bus.config.optimized_bus_access = optimized;
                let hooks = Arc::new(Hooks::default());
                let nvic = Arc::new(NvicState::default());
                bus.nvic = Some(nvic.clone());
                bus.add_peripheral(
                    "read-clear-probe-control",
                    0x40000000,
                    0x100,
                    Some(5),
                    Box::new(ReadClear(hooks.clone())),
                );
                bus.refresh_peripheral_index();
                cpu.r1 = 0x40000000;
                cache(&mut cpu, 0x100, 0x6808);
                cache(&mut cpu, 0x102, 0xe7fd);
                assert!(bus.flash.write_u16(0x100, 0x6808));
                assert!(bus.flash.write_u16(0x102, 0xe7fd));
                if enabled {
                    cpu.enable_dispatch_probe(1000);
                }
                for (read_index, capability) in [true, false, true].into_iter().enumerate() {
                    cpu.pc = 0x100;
                    hooks
                        .deassert_capability
                        .store(capability, Ordering::SeqCst);
                    hooks.line.store(true, Ordering::SeqCst);
                    nvic.ispr[0].store(1 << 5, Ordering::SeqCst);
                    nvic.level_pended[0].store(1 << 5, Ordering::SeqCst);
                    let accesses = bus.access_counts();
                    let reads = hooks.read_count.load(Ordering::SeqCst);
                    let caps = hooks.capability_count.load(Ordering::SeqCst);
                    let levels = hooks.level_count.load(Ordering::SeqCst);
                    assert_eq!(cpu.run_t16_cached_fast_paths(&mut bus, 16), 0);
                    assert_eq!(bus.access_counts(), accesses);
                    assert_eq!(hooks.read_count.load(Ordering::SeqCst), reads);
                    assert_eq!(hooks.capability_count.load(Ordering::SeqCst), caps);
                    assert_eq!(hooks.level_count.load(Ordering::SeqCst), levels);
                    assert_eq!(nvic.ispr[0].load(Ordering::SeqCst), 1 << 5);
                    let config = bus.config.clone();
                    cpu.step_internal(&mut bus, &[], &config).unwrap();
                    assert_eq!(cpu.r0, 0x12345678);
                    assert_eq!(hooks.read_count.load(Ordering::SeqCst), read_index + 1);
                    assert_eq!(hooks.capability_count.load(Ordering::SeqCst), caps + 1);
                    assert_eq!(
                        hooks.level_count.load(Ordering::SeqCst),
                        levels + usize::from(capability)
                    );
                    assert_eq!(
                        nvic.ispr[0].load(Ordering::SeqCst),
                        if capability { 0 } else { 1 << 5 }
                    );
                    assert_eq!(
                        nvic.level_pended[0].load(Ordering::SeqCst),
                        if capability { 0 } else { 1 << 5 }
                    );
                    assert_eq!(bus.access_counts().2, accesses.2 + 1);
                }
            }
        }
    }
}
