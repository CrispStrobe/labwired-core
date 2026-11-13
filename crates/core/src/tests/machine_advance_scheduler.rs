// SPDX-License-Identifier: MIT
//! Scheduler-only Machine boundary tests share one feature gate.

use super::*;

#[test]
fn idle_fast_forward_is_reconciled_in_report_and_cpu() {
    let mut bus = SystemBus::new();
    bus.peripherals.clear();
    let mut machine = Machine::new(
        CountingCpu {
            idle_budget: Some(5),
            ..Default::default()
        },
        bus,
    );
    machine.config.idle_fast_forward_enabled = true;

    let report = machine.advance(AdvanceRequest::run(Some(5))).unwrap();

    assert_eq!(report.stop, AdvanceStop::FuelLimit);
    assert_eq!(report.fuel_consumed, 5);
    assert_eq!(report.idle_cycles, 5);
    assert_eq!(report.primary_steps, 0);
    assert_eq!(report.cpu_batches, 0);
    assert_eq!(report.elapsed_cycles, 5);
    assert_eq!(machine.cpu.idle_skipped, 5);
    assert_eq!(machine.idle_fast_forward_cycles_skipped, 5);
}

mod deadline_boundaries {
    use crate::cpu::cortex_m::t16_countdown_tests::{fixture, state};
    use crate::cpu::CortexM;
    use crate::peripheral::Peripheral;
    use crate::sched::{EventResult, EventScheduler};
    use crate::Bus;
    use crate::{DebugControl, Machine, SimResult};
    use std::sync::{Arc, Mutex};

    const ARM_REGISTER: u64 = 0x4000_0000;
    const TOKEN: u32 = 37;
    type Deliveries = Arc<Mutex<Vec<(u64, u32)>>>;

    /// A real bus write harvests this device's relative wake into
    /// pending_schedule. Only Machine's normal scheduler drain delivers it.
    #[derive(Debug)]
    struct DeadlineIrq {
        delay: u64,
        armed: bool,
        deliveries: Deliveries,
    }

    impl Peripheral for DeadlineIrq {
        fn read(&self, _offset: u64) -> SimResult<u8> {
            Ok(0)
        }

        fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
            assert_eq!((offset, value), (0, 1));
            self.armed = true;
            Ok(())
        }

        fn uses_scheduler(&self) -> bool {
            true
        }

        fn take_scheduled_events(&mut self) -> Vec<(u64, u32)> {
            if std::mem::take(&mut self.armed) {
                vec![(self.delay, TOKEN)]
            } else {
                vec![]
            }
        }

        fn on_event(
            &mut self,
            token: u32,
            sched: &mut EventScheduler,
            _bus: &mut dyn Bus,
        ) -> EventResult {
            self.deliveries.lock().unwrap().push((sched.now(), token));
            EventResult {
                system_exception: Some(15),
                ..EventResult::default()
            }
        }
    }

    fn machine(
        branch_entry: bool,
        arm_in_batch: bool,
        offset: u32,
    ) -> (Machine<CortexM>, Deliveries) {
        let (mut cpu, mut bus) = fixture(0, 2048, 0);
        cpu.pc = if branch_entry { 0x102 } else { 0x100 };
        cpu.sp = 0x2000_1000;
        assert!(bus.flash.write_u32(15 * 4, 0x181));
        // Stay in the handler and visibly retire instructions after IRQ entry.
        assert!(bus.flash.write_u16(0x180, 0x3701)); // ADDS r7,#1
        assert!(bus.flash.write_u16(0x182, 0xe7fd)); // B 0x180
        if arm_in_batch {
            // Two-instruction prefix. STRB executes at cycle 0; the countdown
            // enters SUBS or BNE at cycle 2 with the requested remaining budget.
            cpu.pc = 0xfc;
            cpu.r1 = ARM_REGISTER as u32;
            cpu.r2 = 1;
            assert!(bus.flash.write_u16(0xfc, 0x700a)); // STRB r2,[r1]
            assert!(bus
                .flash
                .write_u16(0xfe, if branch_entry { 0xe000 } else { 0xbf00 }));
        }
        let deliveries = Arc::new(Mutex::new(vec![]));
        let deadline_idx = bus.peripherals.len();
        bus.add_peripheral(
            "countdown-deadline",
            ARM_REGISTER,
            4,
            None,
            Box::new(DeadlineIrq {
                // The write executes at cycle 0. Bus collection anchors
                // relative delays at current_cycle + 1, so offset + 1
                // places the wake at countdown entry (cycle 2) + offset.
                delay: u64::from(offset) + 1,
                armed: false,
                deliveries: deliveries.clone(),
            }),
        );
        let mut machine = Machine::new(cpu, bus);
        machine.config.peripheral_tick_interval = 64;
        if !arm_in_batch {
            machine
                .sched
                .schedule(u64::from(offset), deadline_idx as u32, TOKEN);
            assert_eq!(machine.sched.next_event_deadline(), Some(u64::from(offset)));
        }
        assert!(machine.bus.pending_schedule.is_empty());
        (machine, deliveries)
    }

    fn assert_equivalent(fast: &mut Machine<CortexM>, reference: &mut Machine<CortexM>) {
        assert_eq!(state(&fast.cpu), state(&reference.cpu));
        // The public snapshot omits these architectural exception/stack fields.
        assert_eq!(
            (
                fast.cpu.msp,
                fast.cpu.psp,
                fast.cpu.control,
                fast.cpu.basepri,
                fast.cpu.faultmask,
                fast.cpu.it_state,
                fast.cpu.active_exception
            ),
            (
                reference.cpu.msp,
                reference.cpu.psp,
                reference.cpu.control,
                reference.cpu.basepri,
                reference.cpu.faultmask,
                reference.cpu.it_state,
                reference.cpu.active_exception
            )
        );
        assert_eq!(fast.total_cycles, reference.total_cycles);
        assert_eq!(fast.bus.current_cycle, reference.bus.current_cycle);
        assert_eq!(fast.sched.now(), reference.sched.now());
        assert_eq!(
            fast.step_profile().cpu_instructions,
            reference.step_profile().cpu_instructions
        );
        assert_eq!(
            fast.sched.next_event_deadline(),
            reference.sched.next_event_deadline()
        );
        assert_eq!(fast.bus.pending_schedule, reference.bus.pending_schedule);
        // Include the hardware exception frame, especially pre-IRQ R0 and PC.
        for addr in (0x2000_0fe0..0x2000_1000).step_by(4) {
            assert_eq!(
                fast.bus.read_u32(addr).unwrap(),
                reference.bus.read_u32(addr).unwrap()
            );
        }
    }

    fn differential_deadlines(arm_in_batch: bool) {
        for branch_entry in [false, true] {
            for remaining in [7, 8, 9, 15, 16, 17] {
                for offset in [0, 1, 7, 8, 9] {
                    let (mut fast, actual) = machine(branch_entry, arm_in_batch, offset);
                    let (mut reference, expected) = machine(branch_entry, arm_in_batch, offset);
                    // A genuinely ordinary per-instruction oracle: disabling
                    // batching alone at interval 64 does not drain MMIO-armed
                    // events mid-batch, so would reproduce the same lateness.
                    reference.config.batch_mode_enabled = false;
                    reference.config.peripheral_tick_interval = 1;
                    let prefix = if arm_in_batch { 2 } else { 0 };
                    let budget = remaining + prefix;
                    let deadline = (offset + prefix).max(1);
                    fast.run(Some(budget)).unwrap();
                    reference.run(Some(budget)).unwrap();
                    assert_equivalent(&mut fast, &mut reference);
                    let delivered = if deadline <= budget {
                        vec![(u64::from(deadline), TOKEN)]
                    } else {
                        vec![]
                    };
                    assert_eq!(*actual.lock().unwrap(), delivered,
                        "branch={branch_entry} armed={arm_in_batch} remaining={remaining} offset={offset}");
                    assert_eq!(*expected.lock().unwrap(), delivered);
                    assert_eq!(fast.total_cycles, u64::from(budget));
                    assert_eq!(fast.step_profile().cpu_instructions, u64::from(budget));
                    // Exception entry itself consumes one modeled step; allow
                    // a second step for the handler's ADDS, including when the
                    // deadline lay outside the initial request.
                    let finish = budget.max(deadline + 2);
                    if finish > budget {
                        fast.run(Some(finish - budget)).unwrap();
                        reference.run(Some(finish - budget)).unwrap();
                    }
                    assert_equivalent(&mut fast, &mut reference);
                    assert_eq!(*actual.lock().unwrap(), vec![(u64::from(deadline), TOKEN)]);
                    assert_eq!(*expected.lock().unwrap(), *actual.lock().unwrap());
                    assert_eq!(fast.cpu.active_exception, 15);
                    assert!(
                        fast.cpu.r7 > 0,
                        "the delivered IRQ must execute its handler"
                    );
                    let countdown_steps = deadline - prefix;
                    let subs = if branch_entry {
                        countdown_steps / 2
                    } else {
                        (countdown_steps + 1) / 2
                    };
                    assert_eq!(fast.bus.read_u32(0x2000_0fe0).unwrap(), 2048 - subs);
                    assert_eq!(fast.total_cycles, u64::from(finish));
                    assert_eq!(fast.step_profile().cpu_instructions, u64::from(finish));
                }
            }
        }
    }

    #[test]
    fn countdown_respects_actual_scheduler_heap_deadlines() {
        differential_deadlines(false);
    }

    #[test]
    fn countdown_respects_newly_mmio_armed_pending_event_deadlines() {
        differential_deadlines(true);
    }
}
