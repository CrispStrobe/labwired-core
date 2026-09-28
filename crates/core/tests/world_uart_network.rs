// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! A timed UART network of real firmware (Renode issue #948's question):
//! "when this link slows down and one node restarts, does the network recover
//! without overflowing its buffers?"
//!
//! The nodes are STM32F401 (NUCLEO-F401RE) running `uart-chain.c`, an
//! interrupt-driven relay: USART1 RXNE interrupt into a ring buffer, a main
//! loop that parses 5-byte messages and forwards them with the hop count
//! incremented, USART2 TXE interrupt draining a TX ring buffer. Node 0 is the
//! source (one message per millisecond from SysTick).
//!
//! Every golden number is computed from first principles, not measured:
//!
//! * 115200 baud from 84 MHz: BRR = 0x2D9 = 729 cycles per bit.
//! * 8N1: a character is 10 bits on the wire; the receiver sets RXNE in the
//!   middle of the stop bit, 9.5 bits after the start edge.
//! * A 5-byte message leaves back to back (the TXE interrupt refills the data
//!   register while the shifter works), so its last character starts 4 frames
//!   after the first and completes 4 × 10 + 9.5 = 49.5 bit times after the
//!   first start bit. That is one hop, `HOP`.
//! * Across `k` links: `k × HOP + (k − 1) × p`, where `p` is the relay's own
//!   store-and-forward time (interrupt entry, parse, queue, TX interrupt).
//!   `p` is the firmware's, so the test measures it and bounds it.

use labwired_config::{ChipDescriptor, EnvironmentManifest, SystemManifest};
use labwired_core::network::timed_uart::{NetEventKind, NetReport};
use labwired_core::system::node::NodeFirmware;
use labwired_core::world::{ResolvedWorldNode, World};
use std::path::PathBuf;

const HZ: u64 = 84_000_000;
const BRR: u64 = 729;
const MSG_LEN: u64 = 5;
const PERIOD_PS: u64 = 1_000_000_000; // 1 ms
const RESULT: u32 = 0x2000_0100;
const BOOTS: u32 = 0x2000_00F0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One bit at 115200 baud from 84 MHz, ps.
fn bit_ps() -> u64 {
    labwired_core::network::timed_uart::cycles_to_ps(BRR, HZ)
}

/// One cycle, ps (the model's resolution).
fn cycle_ps() -> u64 {
    labwired_core::network::timed_uart::cycles_to_ps(1, HZ) + 1
}

/// One hop of one message: (MSG_LEN − 1) whole frames + 9.5 bits, ps.
fn hop_ps() -> u64 {
    (MSG_LEN - 1) * 10 * bit_ps() + bit_ps() * 19 / 2
}

fn elf(name: &str) -> Vec<u8> {
    std::fs::read(
        root()
            .join("crates/core/tests/fixtures/uart-chain")
            .join(name),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A world of `firmwares.len()` NUCLEO-F401RE nodes `n0, n1, …` joined by a
/// `uart_network` with the given extra config lines.
fn world(firmwares: &[&str], config: &str) -> World {
    let chip = ChipDescriptor::from_file(root().join("configs/chips/stm32f401.yaml")).unwrap();
    let system = SystemManifest::from_file(root().join("configs/systems/nucleo-f401re.yaml")).unwrap();
    let ids: Vec<String> = (0..firmwares.len()).map(|i| format!("n{i}")).collect();
    let nodes_yaml: String = ids
        .iter()
        .map(|id| format!("  - {{ id: {id}, system: s.yaml, firmware: f.elf }}\n"))
        .collect();
    let markers: String = ids
        .iter()
        .map(|id| format!("      - {{ node: {id}, peripheral: gpioa, pin: 5, name: app }}\n"))
        .collect();
    let yaml = format!(
        r#"schema_version: "1.0"
name: uart-chain
nodes:
{nodes_yaml}interconnects:
  - type: uart_network
    nodes: [{list}]
    config:
      messages: {{ sync: 0xA5, length: 5, id_offset: 1, id_bytes: 2, hop_offset: 3, checksum_xor: true }}
      markers:
{markers}{config}"#,
        list = ids.join(", "),
    );
    let manifest: EnvironmentManifest = serde_yaml::from_str(&yaml).unwrap();
    let resolved = ids
        .iter()
        .zip(firmwares)
        .map(|(id, fw)| ResolvedWorldNode {
            id: id.clone(),
            system: system.clone(),
            chip: chip.clone(),
            firmware: NodeFirmware::from_bytes(elf(fw)),
        })
        .collect();
    World::from_resolved(manifest, resolved).expect("world")
}

fn run_to(world: &mut World, t_ps: u64) {
    let mut rounds = 0u64;
    while world.uart_network_now_ps().unwrap() < t_ps {
        for (id, r) in world.step_all() {
            r.unwrap_or_else(|e| panic!("node {id}: {e:?}"));
        }
        rounds += 1;
        assert!(rounds < 10_000_000, "runaway");
    }
}

fn result_words(world: &World, node: &str) -> Vec<u32> {
    let bytes = world.machines[node].read_memory(RESULT, 48).unwrap();
    bytes
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn boots(world: &World, node: &str) -> u32 {
    let b = world.machines[node].read_memory(BOOTS, 4).unwrap();
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn report(world: &World) -> NetReport {
    world.uart_network_report(0).unwrap()
}

/// Latency of message `id` to `node`, ps.
fn latency(r: &NetReport, id: u32, node: &str) -> Option<u64> {
    r.messages
        .iter()
        .find(|m| m.id == id)?
        .deliveries
        .iter()
        .find(|d| d.node == node)?
        .latency_ps
}

#[test]
fn smoke_three_node_chain_delivers() {
    let mut w = world(
        &["uart-chain-source.elf", "uart-chain-relay.elf", "uart-chain-relay.elf"],
        "",
    );
    run_to(&mut w, 6 * PERIOD_PS);
    let r = report(&w);
    eprintln!("bit_ps={} hop_ps={}", bit_ps(), hop_ps());
    for m in r.messages.iter().take(4) {
        eprintln!("{m:?}");
    }
    for l in &r.links {
        eprintln!("{:?}", l.directions[0]);
    }
    eprintln!("n1 {:?} n2 {:?}", result_words(&w, "n1"), result_words(&w, "n2"));
    let l1 = latency(&r, 0, "n1").expect("msg 0 reached n1");
    let l2 = latency(&r, 0, "n2").expect("msg 0 reached n2");
    eprintln!("L1={l1} L2={l2} hop={} p={}", hop_ps(), l2 as i64 - 2 * hop_ps() as i64);
    let _ = (cycle_ps(), boots(&w, "n1"), NetEventKind::Deliver);
}
