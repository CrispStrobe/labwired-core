// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! The UART console as a stream with a read cursor.
//!
//! The sink keeps every byte since construction; the cursor marks how far
//! `expect` and `read_uart` have consumed. Bytes produced past a match (an
//! advance batch overshoots it) stay unread, so the next `expect` sees them
//! first — pexpect's stream semantics.

use crate::system::builder::UartWires;

pub struct UartStream {
    wires: UartWires,
    cursor: usize,
}

impl UartStream {
    pub fn new(wires: UartWires) -> Self {
        Self { wires, cursor: 0 }
    }

    /// Every console byte since construction.
    pub fn all(&self) -> Vec<u8> {
        self.wires
            .sink
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Bytes past the cursor.
    pub fn unread(&self) -> Vec<u8> {
        self.wires
            .sink
            .lock()
            .map(|g| g[self.cursor.min(g.len())..].to_vec())
            .unwrap_or_default()
    }

    /// Mark `n` more bytes as read.
    pub fn consume(&mut self, n: usize) {
        self.cursor += n;
    }

    /// Queue bytes on every UART RX feeder, as the browser's
    /// `feed_uart_input` does; firmware sees them as time advances.
    pub fn send(&self, bytes: &[u8]) {
        for b in &self.wires.rx {
            if let Ok(mut q) = b.lock() {
                q.extend(bytes.iter().copied());
            }
        }
    }
}
