// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Functional RGB bus boundary. The master supplies an active-area frame in
//! scan order; the receiver owns its control state and presentation. This is
//! deliberately not a pixel-clock or GPIO-matrix timing interface. No concrete
//! panel type, framebuffer getter, or engine object crosses this boundary.

pub trait RgbPanel: std::fmt::Debug + Send + Sync {
    /// Complete active-area frame. Each word is the RGB565 value present on
    /// the sixteen data wires after the master's byte/bit ordering controls.
    fn rgb_frame(&self, width: u16, height: u16, pixels: &[u16]);
}
