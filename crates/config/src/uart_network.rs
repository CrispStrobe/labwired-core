// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Schema of the `uart_network` environment interconnect: a timed serial
//! network between world nodes (see `labwired_core::network::timed_uart`).
//!
//! One typed struct is the single source of truth: the manifest validator
//! deserializes it to reject bad input before a world is built, and the world
//! builder deserializes the same struct to build the links.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How the listed nodes are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UartTopology {
    /// `nodes[i].uart_out` ↔ `nodes[i+1].uart_in`, for every consecutive pair.
    #[default]
    Chain,
    /// `hub` ↔ every other node: the hub's `hub_uarts[k]` ↔ spoke k's
    /// `spoke_uart`, spokes in manifest order.
    Star,
}

/// Where a firmware's messages are on the wire, so the network can report
/// end-to-end latency per message id without knowing the protocol otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UartMessageTagging {
    /// First byte of every message.
    pub sync: u8,
    /// Whole message length in bytes, sync included.
    pub length: usize,
    /// Offset of the message id (little-endian).
    pub id_offset: usize,
    /// Width of the message id: 1 or 2 bytes.
    #[serde(default = "default_id_bytes")]
    pub id_bytes: usize,
    /// Offset of a hop counter byte, if the firmware carries one.
    #[serde(default)]
    pub hop_offset: Option<usize>,
    /// When true, the last byte is the XOR of bytes `1..length-1`; a message
    /// that fails it is not counted.
    #[serde(default)]
    pub checksum_xor: bool,
}

fn default_id_bytes() -> usize {
    1
}

/// One scripted change to the network, applied at `at_us` of world time.
/// Exactly one action field is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UartNetworkEvent {
    /// World time, microseconds.
    pub at_us: f64,
    /// Set link `n`'s extra propagation delay to `delay_us` (and its jitter to
    /// `jitter_us`, when given).
    #[serde(default)]
    pub slow_link: Option<u32>,
    #[serde(default)]
    pub delay_us: Option<f64>,
    #[serde(default)]
    pub jitter_us: Option<f64>,
    /// Reset node `id` (CPU and its USARTs) through its reset vector.
    #[serde(default)]
    pub reset_node: Option<String>,
    /// Cut link `n`: the line goes idle and nothing crosses until restored.
    #[serde(default)]
    pub cut_link: Option<u32>,
    #[serde(default)]
    pub restore_link: Option<u32>,
}

/// A GPIO pin whose edges appear on the network timeline, for an
/// application-level marker (the firmware toggles it when it acts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UartNetworkMarker {
    pub node: String,
    /// GPIO peripheral id in the node's chip, e.g. `gpioa`.
    pub peripheral: String,
    pub pin: u8,
    /// Label shown on the timeline; defaults to `<peripheral>.<pin>`.
    #[serde(default)]
    pub name: Option<String>,
}

/// `config:` of a `uart_network` interconnect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UartNetworkConfig {
    #[serde(default)]
    pub topology: UartTopology,
    /// Chain: the UART a node receives from its upstream neighbour on.
    #[serde(default = "default_uart_in")]
    pub uart_in: String,
    /// Chain: the UART a node transmits to its downstream neighbour on.
    #[serde(default = "default_uart_out")]
    pub uart_out: String,
    /// Star: the hub node (default: the first listed node).
    #[serde(default)]
    pub hub: Option<String>,
    /// Star: the hub's UART for each spoke, in spoke order.
    #[serde(default)]
    pub hub_uarts: Vec<String>,
    /// Star: the UART every spoke uses.
    #[serde(default = "default_uart_in")]
    pub spoke_uart: String,
    /// Star: per-spoke UARTs, in spoke order (overrides `spoke_uart`).
    #[serde(default)]
    pub spoke_uarts: Vec<String>,
    /// Extra propagation delay on every link, microseconds.
    #[serde(default)]
    pub delay_us: f64,
    /// Uniform random extra delay in `[0, jitter_us]` per character, drawn from
    /// a per-link generator seeded by `seed`.
    #[serde(default)]
    pub jitter_us: f64,
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Upper bound of one synchronisation round, microseconds. The round is
    /// also bounded by the network's lookahead (the shortest time a character
    /// can take from one node to another), so results do not depend on it.
    #[serde(default)]
    pub max_quantum_us: Option<f64>,
    #[serde(default)]
    pub messages: Option<UartMessageTagging>,
    #[serde(default)]
    pub events: Vec<UartNetworkEvent>,
    #[serde(default)]
    pub markers: Vec<UartNetworkMarker>,
}

fn default_uart_in() -> String {
    "uart1".to_string()
}
fn default_uart_out() -> String {
    "uart2".to_string()
}
fn default_seed() -> u64 {
    1
}

impl UartNetworkConfig {
    /// Parse an interconnect's `config:` map.
    pub fn from_interconnect_config(config: &HashMap<String, serde_yaml::Value>) -> Result<Self> {
        let mut map = serde_yaml::Mapping::new();
        for (k, v) in config {
            map.insert(serde_yaml::Value::String(k.clone()), v.clone());
        }
        let parsed: Self = serde_yaml::from_value(serde_yaml::Value::Mapping(map))
            .context("uart_network: config")?;
        parsed.validate()?;
        Ok(parsed)
    }

    /// Checks that need no node list.
    pub fn validate(&self) -> Result<()> {
        let finite_nonneg = |v: f64, what: &str| -> Result<()> {
            if !v.is_finite() || v < 0.0 {
                anyhow::bail!("uart_network: {what} must be a finite number >= 0");
            }
            Ok(())
        };
        finite_nonneg(self.delay_us, "delay_us")?;
        finite_nonneg(self.jitter_us, "jitter_us")?;
        if let Some(q) = self.max_quantum_us {
            if !q.is_finite() || q <= 0.0 {
                anyhow::bail!("uart_network: max_quantum_us must be a positive number");
            }
        }
        if let Some(m) = &self.messages {
            if m.length < 2 {
                anyhow::bail!("uart_network: messages.length must be at least 2");
            }
            if !matches!(m.id_bytes, 1 | 2) {
                anyhow::bail!("uart_network: messages.id_bytes must be 1 or 2");
            }
            if m.id_offset == 0 || m.id_offset + m.id_bytes > m.length {
                anyhow::bail!("uart_network: messages.id_offset is outside the message");
            }
            if let Some(h) = m.hop_offset {
                if h == 0 || h >= m.length {
                    anyhow::bail!("uart_network: messages.hop_offset is outside the message");
                }
            }
        }
        for (i, e) in self.events.iter().enumerate() {
            finite_nonneg(e.at_us, &format!("events[{i}].at_us"))?;
            let actions = [
                e.slow_link.is_some(),
                e.reset_node.is_some(),
                e.cut_link.is_some(),
                e.restore_link.is_some(),
            ]
            .iter()
            .filter(|x| **x)
            .count();
            if actions != 1 {
                anyhow::bail!(
                    "uart_network: events[{i}] needs exactly one of slow_link, reset_node, cut_link, restore_link"
                );
            }
            if e.slow_link.is_some() && e.delay_us.is_none() && e.jitter_us.is_none() {
                anyhow::bail!("uart_network: events[{i}].slow_link needs delay_us or jitter_us");
            }
            if e.slow_link.is_none() && (e.delay_us.is_some() || e.jitter_us.is_some()) {
                anyhow::bail!("uart_network: events[{i}]: delay_us/jitter_us belong to slow_link");
            }
            if let Some(d) = e.delay_us {
                finite_nonneg(d, &format!("events[{i}].delay_us"))?;
            }
            if let Some(j) = e.jitter_us {
                finite_nonneg(j, &format!("events[{i}].jitter_us"))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<UartNetworkConfig> {
        let map: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        UartNetworkConfig::from_interconnect_config(&map)
    }

    #[test]
    fn defaults_are_a_chain_on_uart1_in_uart2_out() {
        let c = parse("{}").unwrap();
        assert_eq!(c.topology, UartTopology::Chain);
        assert_eq!(c.uart_in, "uart1");
        assert_eq!(c.uart_out, "uart2");
        assert_eq!(c.seed, 1);
    }

    #[test]
    fn an_event_needs_exactly_one_action() {
        assert!(parse("events: [{ at_us: 1 }]").is_err());
        assert!(parse("events: [{ at_us: 1, cut_link: 0, restore_link: 0 }]").is_err());
        assert!(parse("events: [{ at_us: 1, slow_link: 0 }]").is_err());
        assert!(parse("events: [{ at_us: 1, slow_link: 0, delay_us: 5 }]").is_ok());
        assert!(parse("events: [{ at_us: 1, reset_node: n1 }]").is_ok());
    }

    #[test]
    fn unknown_keys_and_bad_numbers_are_rejected() {
        assert!(parse("delay: 5").is_err());
        assert!(parse("delay_us: -1").is_err());
        assert!(parse("max_quantum_us: 0").is_err());
        assert!(parse("messages: { sync: 165, length: 5, id_offset: 4, id_bytes: 2 }").is_err());
    }
}
