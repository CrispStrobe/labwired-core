// SPDX-License-Identifier: MIT
//! Admission/discovery regressions for mixed-width and unsupported hot loops.
use super::*;

fn cache(cpu: &mut CortexM, pc: u32, opcode: u16) {
    cpu.decode_cache[((pc >> 1) & 0x0fff) as usize] = Some(DecodeCacheEntry {
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
