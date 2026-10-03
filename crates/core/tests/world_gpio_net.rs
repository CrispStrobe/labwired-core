// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! GPIO nets between machines: an STM32G0B1 and an ATmega328P joined by an
//! interrupt line, a ready line and a shared open-drain alert line with a
//! pull-up (`examples/gpio-net-two-boards`).
//!
//! The firmware counts edges (EXTI on the STM32, a polled PIND on the AVR) and
//! reports over UART. Every count is a hand-derived number from the firmware's
//! loops, not a measurement: 10 irq pulses, 7 ready pulses, 5 alert pulses the
//! STM32 pulls and 3 the AVR pulls.

use labwired_config::EnvironmentManifest;
use labwired_core::network::gpio_net::{GPIO_NET_CONTENTION, GPIO_NET_FLOATING};
use labwired_core::world::World;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gpio-net-two-boards")
}

type Sinks = (Arc<Mutex<Vec<u8>>>, Arc<Mutex<Vec<u8>>>);

fn build(env_file: &str, rewrite: impl Fn(String) -> String) -> (World, Sinks) {
    let yaml = rewrite(std::fs::read_to_string(example().join(env_file)).unwrap());
    let manifest: EnvironmentManifest = serde_yaml::from_str(&yaml).unwrap();
    let mut world = World::from_manifest(manifest, &example()).expect("world");
    let stm = Arc::new(Mutex::new(Vec::new()));
    let avr = Arc::new(Mutex::new(Vec::new()));
    for (id, sink) in [("stm", &stm), ("avr", &avr)] {
        // Renamed nodes keep their role in the id's tail.
        let key = world
            .machines
            .keys()
            .find(|k| k.ends_with(id))
            .cloned()
            .unwrap();
        world
            .machines
            .get_mut(&key)
            .unwrap()
            .attach_uart_tx_sink(sink.clone(), false)
            .unwrap();
    }
    (world, (stm, avr))
}

fn run_ms(world: &mut World, ms: u64) {
    let end = ms * 1_000_000_000;
    let mut calls = 0u64;
    while world.round_now_ps().unwrap() < end {
        for (id, r) in world.step_all() {
            r.unwrap_or_else(|e| panic!("node {id}: {e:?}"));
        }
        calls += 1;
        assert!(calls < 50_000_000, "runaway");
    }
}

fn text(sink: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&sink.lock().unwrap()).into_owned()
}

fn stm_result(world: &World, id: &str) -> Vec<u32> {
    let b = world.machines[id].read_memory(0x2000_0100, 20).unwrap();
    b.chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

const STM_LINE: &str = "STM irq r=10 f=10 alert r=3 f=3\n";
const AVR_LINE: &str = "AVR ready=7 alert f=5 r=5\n";

/// Everything one run can show that must not depend on how it was scheduled.
#[derive(Debug, PartialEq)]
struct Fingerprint {
    stm_uart: String,
    avr_uart: String,
    stm_ram: Vec<u32>,
    applied: Vec<(String, u8, u64, u64, bool)>,
    nets: Vec<(String, u64, bool, u64)>,
}

fn fingerprint(world: &World, stm_id: &str, sinks: &Sinks) -> Fingerprint {
    Fingerprint {
        stm_uart: text(&sinks.0),
        avr_uart: text(&sinks.1),
        stm_ram: stm_result(world, stm_id),
        applied: world
            .gpio_net_applied()
            .iter()
            .map(|a| (a.node.clone(), a.pin, a.cycle, a.due_ps, a.level))
            .collect(),
        nets: world
            .gpio_net_reports()
            .iter()
            .map(|n| (n.name.clone(), n.edges, n.level, n.contention_events))
            .collect(),
    }
}

#[test]
fn two_boards_count_each_others_edges_exactly() {
    let (mut world, sinks) = build("env.yaml", |s| s);
    run_ms(&mut world, 30);
    assert_eq!(text(&sinks.0), STM_LINE, "STM32 report");
    assert_eq!(text(&sinks.1), AVR_LINE, "AVR report");
    assert_eq!(stm_result(&world, "stm"), vec![10, 10, 3, 3, 1]);

    let reports = world.gpio_net_reports();
    let by = |n: &str| reports.iter().find(|r| r.name == n).unwrap();
    // irq: 10 pulses = 20 wire edges; ready: 7 pulses = 14; alert: 5 + 3
    // pulses = 16 (open-drain wired-AND, pull-up).
    assert_eq!(by("irq").edges, 20);
    assert_eq!(by("ready").edges, 14);
    assert_eq!(by("alert").edges, 16);
    for r in &reports {
        assert_eq!(r.contention_events, 0, "{}", r.name);
        // Nothing drives a pulled net, and the ready line has no driver
        // until the STM32 firmware configures it: only that is flagged.
        assert!(r.diagnostics.iter().all(|d| d.code != GPIO_NET_CONTENTION));
    }
    assert_eq!(
        by("alert").floating_events,
        0,
        "the pull-up holds alert high"
    );
}

/// Same run, scheduled differently: node ids that sort the other way round
/// (so the world steps the AVR first), a different manifest order for nets
/// and nodes, and rounds of several lengths. Every observable is identical.
#[test]
fn results_do_not_depend_on_node_order_or_round_size() {
    let baseline = {
        let (mut w, s) = build("env.yaml", |s| s);
        run_ms(&mut w, 30);
        fingerprint(&w, "stm", &s)
    };
    assert_eq!(baseline.stm_uart, STM_LINE);

    // Node order: the world steps nodes in id order. `avr` < `stm`, so the
    // baseline steps the AVR first; `a_stm` < `z_avr` steps the STM32 first.
    let renamed = |s: String| {
        s.replace("id: stm", "id: a_stm")
            .replace("id: avr", "id: z_avr")
            .replace("node: stm", "node: a_stm")
            .replace("node: avr", "node: z_avr")
            .replace("[avr, stm]", "[a_stm, z_avr]")
    };
    let (mut w, s) = build("env.yaml", renamed);
    run_ms(&mut w, 30);
    let f = fingerprint(&w, "a_stm", &s);
    assert_eq!(f.stm_uart, baseline.stm_uart);
    assert_eq!(f.avr_uart, baseline.avr_uart);
    assert_eq!(f.stm_ram, baseline.stm_ram);
    // The applied log names nodes; compare with the names mapped back.
    let norm = |v: &[(String, u8, u64, u64, bool)]| {
        let mut v: Vec<_> = v
            .iter()
            .map(|(n, p, c, d, l)| {
                (
                    n.trim_start_matches("a_")
                        .trim_start_matches("z_")
                        .to_string(),
                    *p,
                    *c,
                    *d,
                    *l,
                )
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(norm(&f.applied), norm(&baseline.applied), "node order");
    assert_eq!(f.nets, baseline.nets);

    // Round size: one tenth, a prime fraction, and the full latency.
    for round_ps in [10_000u64, 33_333, 70_001, 100_000] {
        let (mut w, s) = build("env.yaml", |s| s);
        w.set_gpio_round_ps(round_ps).unwrap();
        run_ms(&mut w, 30);
        let f = fingerprint(&w, "stm", &s);
        assert_eq!(f, baseline, "round {round_ps} ps");
    }

    // The same net with a different latency is a different run (the delay is
    // real), but it still counts the same edges.
    let (mut w, s) = build("env.yaml", |s| {
        s.replace("latency_ns: 100", "latency_ns: 1000")
    });
    run_ms(&mut w, 30);
    assert_eq!(text(&s.0), STM_LINE);
    assert_eq!(text(&s.1), AVR_LINE);
}

#[test]
fn both_boards_driving_one_push_pull_wire_reports_contention() {
    let (mut world, _s) = build("env-contention.yaml", |s| s);
    run_ms(&mut world, 1);
    let reports = world.gpio_net_reports();
    assert_eq!(reports.len(), 1);
    let net = &reports[0];
    assert_eq!(net.name, "fight");
    assert_eq!(net.contention_events, 1, "{net:#?}");
    let d = net
        .diagnostics
        .iter()
        .find(|d| d.code == GPIO_NET_CONTENTION)
        .expect("GPIO_NET_CONTENTION");
    // The STM32 drives high for about 150 loop iterations (a few tens of us)
    // after boot; the AVR has held the wire low since its first instructions.
    assert!(
        d.t_ps > 1_000_000 && d.t_ps < 200_000_000,
        "begins at {} ps",
        d.t_ps
    );
    let end = d.end_ps.expect("the STM32 releases the wire");
    assert!(end > d.t_ps);
    let drives: Vec<_> = d.members.iter().map(|m| (m.node.as_str(), m.pin)).collect();
    assert_eq!(drives, vec![("avr", 2), ("stm", 1)]);
    // The low side wins: the wire never rises, so nothing was delivered.
    assert!(!net.level);
    assert_eq!(net.edges, 0);
    assert!(world.gpio_net_applied().iter().all(|a| !a.level));
}

#[test]
fn a_net_nobody_drives_or_pulls_floats_and_is_flagged() {
    let (mut world, _s) = build("env.yaml", |s| s);
    run_ms(&mut world, 0);
    // The ready wire has a pull-down, so it is not floating; remove it.
    let (mut world2, _s2) = build("env.yaml", |s| {
        s.replacen(
            "      name: ready\n      pull: down\n",
            "      name: ready\n",
            1,
        )
    });
    run_ms(&mut world2, 0);
    let ready = world2
        .gpio_net_reports()
        .into_iter()
        .find(|r| r.name == "ready")
        .unwrap();
    assert_eq!(ready.floating_events, 1);
    assert!(ready
        .diagnostics
        .iter()
        .any(|d| d.code == GPIO_NET_FLOATING));
    assert!(!ready.level, "a floating wire reads 0");
    let ready = world
        .gpio_net_reports()
        .into_iter()
        .find(|r| r.name == "ready")
        .unwrap();
    assert_eq!(ready.floating_events, 0);
}

fn build_err(env_file: &str, rewrite: impl Fn(String) -> String) -> String {
    let yaml = rewrite(std::fs::read_to_string(example().join(env_file)).unwrap());
    match serde_yaml::from_str::<EnvironmentManifest>(&yaml)
        .map_err(anyhow::Error::from)
        .and_then(|m| World::from_manifest(m, &example()))
    {
        Ok(_) => panic!("expected the world to be refused"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn zero_latency_and_sub_cycle_latency_are_refused() {
    let e = build_err("env.yaml", |s| {
        s.replace("latency_ns: 100", "latency_ns: 0")
    });
    assert!(e.contains("zero-delay"), "{e}");
    // One 16 MHz cycle is 62.5 ns.
    let e = build_err("env.yaml", |s| {
        s.replace("latency_ns: 100", "latency_ns: 40")
    });
    assert!(e.contains("below one cycle"), "{e}");
}

#[test]
fn a_pad_on_two_nets_and_an_unknown_pad_are_refused() {
    let e = build_err("env.yaml", |s| {
        s.replace(
            "{ node: avr, peripheral: portd, pin: 3 }",
            "{ node: avr, peripheral: portd, pin: 2 }",
        )
    });
    assert!(e.contains("on two nets"), "{e}");
    let e = build_err("env.yaml", |s| {
        s.replace("peripheral: portd, pin: 3", "peripheral: portx, pin: 3")
    });
    assert!(e.contains("portx"), "{e}");
    let e = build_err("env.yaml", |s| s.replace("pin: 3 }", "pin: 9 }"));
    assert!(
        e.to_lowercase().contains("pin") || e.contains("net support"),
        "{e}"
    );
}
