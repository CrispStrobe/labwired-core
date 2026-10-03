// SPDX-License-Identifier: MIT
use super::*;

#[test]
fn c3_irq_word_gate_truth_table_and_live_transitions() {
    let mut bus = SystemBus::new();
    for cached in [false, true, false] {
        for routing in [false, true, false] {
            for walk_deleted in [false, true, false] {
                bus.irq_fabric.esp32c3.intc = cached.then(Esp32c3IntcCache::default);
                bus.irq_fabric.esp32c3.routing = routing;
                bus.legacy_walk_disabled = walk_deleted;
                assert_eq!(
                    bus.c3_irq_word_write_hook_needed(),
                    cached || (cfg!(feature = "event-scheduler") && routing && walk_deleted),
                    "cached={cached} routing={routing} walk_deleted={walk_deleted}"
                );
            }
        }
    }
}

#[test]
fn c3_irq_word_gate_inactive_matches_original_read_only_hook() {
    let mut bus = SystemBus::new();
    bus.current_cycle = 37;
    for idx in [0, usize::MAX] {
        for offset in [0, 0x28, 0x90, u64::MAX] {
            let before = (
                format!("{:?}", bus.irq_fabric),
                bus.access_counts(),
                bus.current_cycle,
                bus.ram.data.clone(),
            );
            assert!(!bus.c3_irq_word_write_hook_needed());
            // The original hook itself is the oracle for the declined state.
            bus.sync_esp32c3_irq_cache_write(idx, offset);
            assert_eq!(
                before,
                (
                    format!("{:?}", bus.irq_fabric),
                    bus.access_counts(),
                    bus.current_cycle,
                    bus.ram.data.clone(),
                )
            );
        }
    }
}

#[cfg(feature = "event-scheduler")]
#[test]
fn c3_irq_word_gate_uncached_walk_free_refresh_is_not_skipped() {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    #[derive(Debug)]
    struct Source {
        asserted: Arc<AtomicBool>,
        polls: Arc<AtomicU32>,
    }
    impl crate::Peripheral for Source {
        fn read(&self, _offset: u64) -> SimResult<u8> {
            Ok(0)
        }
        fn write(&mut self, _offset: u64, _value: u8) -> SimResult<()> {
            Ok(())
        }
        fn uses_scheduler(&self) -> bool {
            true
        }
        fn needs_legacy_walk(&self) -> bool {
            false
        }
        fn matrix_irq_sources_into(&self, out: &mut Vec<u32>) {
            self.polls.fetch_add(1, Ordering::Relaxed);
            if self.asserted.load(Ordering::Relaxed) {
                out.push(16);
            }
        }
    }
    let asserted = Arc::new(AtomicBool::new(true));
    let polls = Arc::new(AtomicU32::new(0));
    let mut bus = SystemBus::empty();
    bus.add_peripheral(
        "source",
        0x40030000,
        0x100,
        None,
        Box::new(Source {
            asserted: asserted.clone(),
            polls: polls.clone(),
        }),
    );
    let idx = bus.find_peripheral_index_by_name("source").unwrap();
    bus.irq_fabric.esp32c3.intc = None;
    bus.irq_fabric.esp32c3.routing = true;
    bus.recompute_walk_deletable();
    assert!(bus.legacy_walk_disabled);
    let before = polls.load(Ordering::Relaxed);
    assert!(bus.c3_irq_word_write_hook_needed());
    crate::Bus::write_u32(&mut bus, 0x40030000, 0).unwrap();
    assert_eq!(bus.irq_fabric.esp32c3.sched_sources, [1 << 16, 0]);
    assert_eq!(polls.load(Ordering::Relaxed), before + 1);
    asserted.store(false, Ordering::Relaxed);
    crate::Bus::write_u32(&mut bus, 0x40030000, 0).unwrap();
    assert_eq!(bus.irq_fabric.esp32c3.sched_sources, [0, 0]);
    assert_eq!(polls.load(Ordering::Relaxed), before + 2);
    // This live policy transition must immediately disable the scheduler arm.
    bus.legacy_walk_disabled = false;
    assert!(!bus.c3_irq_word_write_hook_needed());
    bus.sync_esp32c3_irq_cache_write(idx, 0);
    assert_eq!(polls.load(Ordering::Relaxed), before + 2);
}

#[test]
fn c3_irq_word_gate_c3_c6_doorbells_update_at_the_store() {
    for c6 in [false, true] {
        let (name, base, offset, source) = if c6 {
            ("intpri", 0x600c5000, 0x90, 22)
        } else {
            ("system", 0x600c0000, 0x28, 50)
        };
        let mut descriptor = declarative_descriptor(None);
        descriptor.registers.truncate(1);
        descriptor.registers[0].address_offset = offset;
        let mut bus = SystemBus::new();
        bus.add_peripheral(
            name,
            base,
            0x100,
            None,
            Box::new(crate::peripherals::declarative::GenericPeripheral::new(
                descriptor,
            )),
        );
        let idx = bus.find_peripheral_index_by_name(name).unwrap();
        let mut cache = Esp32c3IntcCache::default();
        cache.from_cpu_source_base = source;
        cache.source_line[source as usize] = 3;
        cache.line_pri[3] = 1;
        cache.int_enable = 1 << 3;
        bus.irq_fabric.esp32c3.intc = Some(cache);
        bus.irq_fabric.esp32c3.routing = true;
        if c6 {
            bus.irq_fabric.esp32c3.intpri_idx = Some(idx);
        } else {
            bus.irq_fabric.esp32c3.system_idx = Some(idx);
        }
        for (value, lines) in [(1, 1 << 3), (0, 0), (1, 1 << 3), (0, 0)] {
            assert!(bus.c3_irq_word_write_hook_needed());
            crate::Bus::write_u32(&mut bus, base + offset, value).unwrap();
            assert_eq!(
                bus.irq_fabric
                    .esp32c3
                    .intc
                    .as_ref()
                    .unwrap()
                    .from_cpu_pending,
                value as u8
            );
            assert_eq!(bus.irq_fabric.esp32c3.irq_lines, lines, "c6={c6}");
        }
        // A populated cache must still be updated when routing is disabled.
        bus.irq_fabric.esp32c3.routing = false;
        crate::Bus::write_u32(&mut bus, base + offset, 1).unwrap();
        assert_eq!(
            bus.irq_fabric
                .esp32c3
                .intc
                .as_ref()
                .unwrap()
                .from_cpu_pending,
            1
        );
        assert_eq!(bus.irq_fabric.esp32c3.irq_lines, 0);
        bus.irq_fabric.esp32c3.routing = true;
        crate::Bus::write_u32(&mut bus, base + offset, 1).unwrap();
        assert_eq!(bus.irq_fabric.esp32c3.irq_lines, 1 << 3);
    }
}
