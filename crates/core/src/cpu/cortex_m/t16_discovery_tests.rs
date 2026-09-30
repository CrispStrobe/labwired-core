// SPDX-License-Identifier: MIT
//! Admission/discovery regressions for mixed-width and unsupported hot loops.
use super::*;

fn cache(cpu: &mut CortexM, pc: u32, opcode: u16) {
    cpu.insert_decoded_entry(DecodeCacheEntry {
        tag: pc,
        instruction: decode_thumb_16(opcode),
        opcode: u32::from(opcode),
        pc_increment: 2,
        cycles: 1,
    });
}

fn loop_fixture(start: u32) -> (CortexM, SystemBus) {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = start;
    cpu.r0 = 50;
    // SUBS r0,#1; BNE start. PC-relative displacement at BNE is -6.
    for (i, op) in [0x3801, 0xd1fd].into_iter().enumerate() {
        let pc = start + i as u32 * 2;
        cache(&mut cpu, pc, op);
        assert!(bus.flash.write_u16(u64::from(pc), op));
    }
    (cpu, bus)
}

#[test]
fn cold_miss_is_not_a_permanent_negative_cache() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0x100;
    cpu.r0 = 3;
    assert!(!cpu.t16_block_entry_admitted(cpu.pc));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
    cache(&mut cpu, 0x100, 0x3801);
    cache(&mut cpu, 0x102, 0xd1fd);
    assert!(cpu.t16_block_entry_admitted(cpu.pc));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 2));
}

#[test]
fn rotated_entry_and_branch_entry_match_reference_for_every_budget() {
    for phase in [0, 2] {
        for budget in 1..=15 {
            let (mut fast, mut fast_bus) = loop_fixture(0x100);
            let (mut reference, mut reference_bus) = loop_fixture(0x100);
            fast.pc += phase;
            reference.pc += phase;
            // The rotated branch starts with Z clear, so BNE is taken.
            fast.xpsr &= !(1 << 30);
            reference.xpsr &= !(1 << 30);
            assert!(fast.t16_block_entry_admitted(fast.pc));
            let retired = fast.run_t16_fast_block(&mut fast_bus, budget);
            assert_eq!(retired, budget, "phase={phase} budget={budget}");
            let config = reference_bus.config.clone();
            for _ in 0..retired {
                reference
                    .step_internal(&mut reference_bus, &[], &config)
                    .unwrap();
            }
            assert_eq!(fast.pc, reference.pc, "phase={phase} budget={budget}");
            assert_eq!(fast.r0, reference.r0);
            assert_eq!(fast.xpsr, reference.xpsr);
        }
    }
}

#[test]
fn cbnz_is_rejected_at_current_entry_and_cannot_be_spanned() {
    let (mut cpu, mut bus) = loop_fixture(0x100);
    cache(&mut cpu, 0x104, 0xb900); // CBNZ r0,+0
    assert!(matches!(
        cpu.decode_cache[0x82].unwrap().instruction,
        Instruction::Cbnz { .. }
    ));
    cpu.pc = 0x104;
    assert!(!cpu.t16_block_entry_admitted(cpu.pc));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert!(cpu.t16_fast_block.is_none());
    cache(&mut cpu, 0x106, 0x3801);
    cache(&mut cpu, 0x108, 0xd1fa); // BNE 0x100, across CBNZ
    cpu.pc = 0x106;
    assert!(cpu.t16_block_entry_admitted(cpu.pc));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert_eq!((cpu.pc, cpu.r0), (0x106, 50));
}

#[test]
fn thumb32_width_is_a_barrier_even_if_instruction_is_supported() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cache(&mut cpu, 0x100, 0x3801);
    cache(&mut cpu, 0x102, 0xbf00);
    cpu.decode_cache[0x81].as_mut().unwrap().pc_increment = 4;
    cpu.pc = 0x102;
    assert!(!cpu.t16_block_entry_admitted(cpu.pc));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    // A real Thumb32 occupies both halfwords; no entry is fabricated at +2.
    cache(&mut cpu, 0x106, 0x3801);
    cache(&mut cpu, 0x108, 0xd1fa);
    cpu.pc = 0x106;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert!(cpu.t16_fast_block.is_none());
}

#[test]
fn invalid_decode_tag_does_not_admit_aliasing_slot() {
    let (mut cpu, mut bus) = loop_fixture(0x100);
    cpu.decode_cache[0x80].as_mut().unwrap().tag = 0x2100;
    assert!(!cpu.t16_block_entry_admitted(0x100));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 50));
    // Repairing the collision permits discovery immediately.
    cache(&mut cpu, 0x100, 0x3801);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
}

#[test]
fn backward_search_at_zero_does_not_underflow_or_poison_future_discovery() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0;
    cache(&mut cpu, 0, 0xbf00);
    assert!(cpu.t16_block_entry_admitted(0));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert_eq!(cpu.pc, 0);
    let (mut cpu, mut bus) = loop_fixture(0);
    cpu.pc = 2;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 3), 3);
    assert_eq!((cpu.pc, cpu.r0), (0, 49));
}

#[test]
fn admitted_load_falls_back_for_mmio_without_side_effects_then_accepts_ram() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cache(&mut cpu, 0x100, 0x6808); // LDR r0,[r1,#0]
    cache(&mut cpu, 0x102, 0xe7fd); // B 0x100
    cpu.pc = 0x100;
    cpu.r0 = 0xdeadbeef;
    cpu.r1 = 0x40003104; // TWIM event MMIO, never direct RAM
    assert!(cpu.t16_block_entry_admitted(cpu.pc));
    let before = bus.access_counts();
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
    assert_eq!(bus.access_counts(), before);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 0xdeadbeef));
    cpu.r1 = 0x20000100;
    assert!(bus.ram.write_u32(u64::from(cpu.r1), 0x12345678));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 0x12345678));
    // A positive cache must still fall back when only the effective address
    // changes, and that execution failure must never become a discovery miss.
    cpu.r1 = 0x40003104;
    let before = (cpu.pc, cpu.r0, bus.access_counts());
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
    assert_eq!((cpu.pc, cpu.r0, bus.access_counts()), before);
    assert_ne!(cpu.t16_discovery_misses[0], (cpu.pc, cpu.decode_generation));
    cpu.r1 = 0x20000100;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
}

#[test]
fn cached_block_before_barrier_cannot_execute_unrelated_tail() {
    let (mut cpu, mut bus) = loop_fixture(0x100);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    assert!(cpu.t16_fast_block.is_some());
    cache(&mut cpu, 0x104, 0xb900);
    cache(&mut cpu, 0x106, 0x3801);
    cache(&mut cpu, 0x108, 0xd1fa);
    cpu.pc = 0x106;
    let before = cpu.r0;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert_eq!((cpu.pc, cpu.r0), (0x106, before));
}

#[test]
fn forward_and_wrong_target_terminal_branches_reject_without_retiring() {
    for terminal in [0xe001, 0xe7fc, 0xe7fe] {
        let (mut cpu, mut bus) = loop_fixture(0x100);
        cache(&mut cpu, 0x102, terminal);
        assert!(cpu.t16_block_entry_admitted(0x102));
        assert!(cpu.compile_t16_fast_block(0x100).is_none());
        let before = (cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts());
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
        assert_eq!((cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts()), before);
    }
}

#[test]
fn supported_window_longer_than_capacity_rejects_from_start_and_terminal() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    for i in 0..T16_FAST_BLOCK_MAX {
        cache(&mut cpu, 0x100 + i as u32 * 2, 0xbf00);
    }
    // Sixteen NOPs followed by B 0x100: seventeen operations cannot fit.
    let terminal_pc = 0x100 + T16_FAST_BLOCK_MAX as u32 * 2;
    let displacement = (0x100_i32 - terminal_pc as i32 - 4) / 2;
    cache(
        &mut cpu,
        terminal_pc,
        0xe000 | ((displacement as u16) & 0x7ff),
    );
    assert!(cpu.compile_t16_fast_block(0x100).is_none());
    for pc in [0x100, 0x102, terminal_pc] {
        cpu.pc = pc;
        let before = (cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts());
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
        assert_eq!((cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts()), before);
    }
}

#[test]
fn invalid_tag_inside_candidate_rejects_then_repaired_window_matches_interpreter() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0x100;
    cpu.r0 = 50;
    for (i, op) in [0x3801, 0xbf00, 0xd1fc].into_iter().enumerate() {
        let pc = 0x100 + i as u32 * 2;
        cache(&mut cpu, pc, op);
        assert!(bus.flash.write_u16(u64::from(pc), op));
    }
    cpu.decode_cache[0x81].as_mut().unwrap().tag = 0x2102;
    assert!(cpu.t16_block_entry_admitted(0x100));
    assert!(cpu.compile_t16_fast_block(0x100).is_none());
    let before = (cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts());
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 64), 0);
    assert_eq!((cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts()), before);
    cache(&mut cpu, 0x102, 0xbf00);
    let mut reference = CortexM::new();
    reference.pc = cpu.pc;
    reference.r0 = cpu.r0;
    reference.xpsr = cpu.xpsr;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 12), 12);
    let config = bus.config.clone();
    for _ in 0..12 {
        reference.step_internal(&mut bus, &[], &config).unwrap();
    }
    assert_eq!(
        (cpu.pc, cpu.r0, cpu.xpsr),
        (reference.pc, reference.r0, reference.xpsr)
    );
}

#[test]
fn discovery_memo_is_stable_until_real_decoder_warms_the_window() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0x100;
    cpu.r0 = 4;
    assert!(bus.flash.write_u16(0x100, 0x3801));
    assert!(bus.flash.write_u16(0x102, 0xd1fd));
    let generation = cpu.decode_generation;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 9), 0);
    assert_eq!(cpu.t16_discovery_misses[0], (0x100, generation));
    let before = (cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts());
    for _ in 0..4 {
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 9), 0);
    }
    assert_eq!((cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts()), before);
    let config = bus.config.clone();
    for _ in 0..2 {
        cpu.step_internal(&mut bus, &[], &config).unwrap();
    }
    assert!(cpu.decode_generation > generation);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 3));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 4), 4);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 1));
}

#[test]
fn fast_fetch_insertion_and_decode_tag_collision_invalidate_discovery_misses() {
    let (mut cpu, mut bus) = loop_fixture(0x100);
    cache(&mut cpu, 0x2100, 0xbf00); // same decode index, different PC tag
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
    let generation = cpu.decode_generation;
    assert_eq!(cpu.fetch_t16_fast(&mut bus, 0x100, true), Some(0x3801));
    assert!(cpu.decode_generation > generation);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
}

#[test]
fn memo_slot_aliases_require_exact_pc_and_generation() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0x100;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 1), 0);
    cpu.pc = 0x180; // same bounded memo slot, not same PC
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 1), 0);
    assert_eq!(cpu.t16_discovery_misses[0], (0x180, cpu.decode_generation));
    cpu.pc = 0x100;
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 1), 0);
    assert_eq!(cpu.t16_discovery_misses[0], (0x100, cpu.decode_generation));
}

#[test]
fn invalidate_reset_and_snapshot_restore_drop_positive_and_negative_state() {
    for path in 0..3 {
        let (mut cpu, mut bus) = loop_fixture(0x100);
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
        assert!(cpu.t16_fast_block.is_some());
        cpu.pc = 0x200;
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
        // Re-establish positive cache while retaining the unrelated miss.
        cpu.pc = 0x100;
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
        let generation = cpu.decode_generation;
        match path {
            0 => cpu.invalidate_code_caches(),
            1 => cpu.reset(&mut bus).unwrap(),
            _ => {
                let snapshot = cpu.snapshot();
                cpu.apply_snapshot(&snapshot);
            }
        }
        assert_ne!(cpu.decode_generation, generation);
        assert!(cpu.t16_fast_block.is_none());
        assert!(cpu.decode_cache.iter().all(Option::is_none));
        // Code can be patched into a supported loop after any flush.
        assert!(bus.flash.write_u16(0x100, 0x3802)); // patched SUBS r0,#2
        assert!(bus.flash.write_u16(0x102, 0xd1fd));
        cpu.pc = 0x100;
        cpu.r0 = 8;
        let config = bus.config.clone();
        for _ in 0..2 {
            cpu.step_internal(&mut bus, &[], &config).unwrap();
        }
        assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
        assert_eq!((cpu.pc, cpu.r0), (0x100, 4));
    }
}

#[test]
fn generation_wrap_clears_old_misses_before_reusing_generation_one() {
    let mut cpu = CortexM::new();
    cpu.t16_discovery_misses.fill((0x100, 1));
    cpu.decode_generation = u64::MAX;
    cache(&mut cpu, 0x100, 0xbf00);
    assert_eq!(cpu.decode_generation, 1);
    assert!(cpu
        .t16_discovery_misses
        .iter()
        .all(|entry| *entry == (0, 0)));
}

#[test]
fn positive_cache_is_considered_before_negative_memo_and_zero_budget_retires_nothing() {
    let (mut cpu, mut bus) = loop_fixture(0x100);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    cpu.memoize_t16_discovery_miss();
    let before = (cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts());
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 0), 0);
    assert_eq!((cpu.pc, cpu.r0, cpu.xpsr, bus.access_counts()), before);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 48));
}

#[test]
fn decoding_disabled_then_enabled_warms_cache_and_invalidates_old_miss() {
    let mut cpu = CortexM::new();
    let mut bus = SystemBus::new();
    cpu.pc = 0x100;
    cpu.r0 = 5;
    assert!(bus.flash.write_u16(0x100, 0x3801));
    assert!(bus.flash.write_u16(0x102, 0xd1fd));
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 0);
    let generation = cpu.decode_generation;
    let mut config = bus.config.clone();
    config.decode_cache_enabled = false;
    for _ in 0..2 {
        cpu.step_internal(&mut bus, &[], &config).unwrap();
    }
    assert_eq!((cpu.pc, cpu.r0), (0x100, 4));
    assert_eq!(cpu.decode_generation, generation);
    assert!(cpu.decoded_entry(0x100).is_none());
    config.decode_cache_enabled = true;
    for _ in 0..2 {
        cpu.step_internal(&mut bus, &[], &config).unwrap();
    }
    assert!(cpu.decode_generation > generation);
    assert_eq!(cpu.run_t16_fast_block(&mut bus, 2), 2);
    assert_eq!((cpu.pc, cpu.r0), (0x100, 2));
}
