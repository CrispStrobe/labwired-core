// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! ST7701 RGB panel for the GUITION ESP32-4848S040 C_I assembly.
//!
//! A GPIO observer receives the real 3-wire, MSB-first 9-bit control stream
//! (first bit D/C, then eight payload bits, rising SCK with CS low). LCD_CAM
//! supplies complete RGB565 active-area frames through the independent RGB
//! panel port. DCS sleep/display commands and the physical backlight gate the
//! visible image. Extended command-bank parameters are accepted and retained,
//! but analog gamma/voltage, porch edges, RGB pin mux and scan-clock timing are
//! not simulated. Rotation is performed by guest framebuffer software, as in
//! Arduino_GFX's Arduino_RGB_Display, not invented from ILI9341 commands.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use crate::peripherals::components::RgbPanel;
use crate::peripherals::device::GpioObserver;
use crate::peripherals::kit::{
    AttachCtx, Category, ConfigKey, ConfigType, KitMetadata, PeripheralKit, Transport,
};

pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 480;

#[derive(Debug, Clone, Copy)]
pub struct ControlPins {
    pub cs: u8,
    pub sck: u8,
    pub mosi: u8,
    pub backlight: u8,
    pub reset: Option<u8>,
}

impl Default for ControlPins {
    fn default() -> Self {
        Self {
            cs: 39,
            sck: 48,
            mosi: 47,
            backlight: 38,
            reset: None,
        }
    }
}

#[derive(Debug)]
struct State {
    cs: bool,
    sck: bool,
    mosi: bool,
    reset: bool,
    backlight: bool,
    serial: u16,
    bits: u8,
    command: u8,
    params: Vec<u8>,
    bank: u8,
    sleeping: bool,
    display_on: bool,
    colmod: u8,
    framebuffer: Vec<u8>,
    frames_received: u64,
}

impl State {
    fn new() -> Self {
        Self {
            cs: true,
            sck: false,
            mosi: false,
            reset: true,
            backlight: false,
            serial: 0,
            bits: 0,
            command: 0,
            params: Vec::new(),
            bank: 0,
            sleeping: true,
            display_on: false,
            colmod: 0x60,
            framebuffer: vec![0; WIDTH * HEIGHT * 2],
            frames_received: 0,
        }
    }

    fn reset_controller(&mut self) {
        self.serial = 0;
        self.bits = 0;
        self.command = 0;
        self.params.clear();
        self.bank = 0;
        self.sleeping = true;
        self.display_on = false;
        self.colmod = 0x60;
        self.framebuffer.fill(0);
        self.frames_received = 0;
    }

    fn serial_word(&mut self, word: u16) {
        if word & 0x100 == 0 {
            self.command = word as u8;
            self.params.clear();
            if self.bank == 0 {
                match self.command {
                    0x01 => self.reset_controller(),
                    0x10 => self.sleeping = true,
                    0x11 => self.sleeping = false,
                    0x28 => self.display_on = false,
                    0x29 => self.display_on = true,
                    _ => {}
                }
            }
        } else {
            // Bound malformed streams: the largest type9 parameter block is
            // 16 bytes; retain up to 32 rather than grow on an endless CS.
            if self.params.len() < 32 {
                self.params.push(word as u8);
            }
            if self.command == 0xff && self.params.len() == 5 && self.params[..4] == [0x77, 1, 0, 0]
            {
                self.bank = self.params[4];
            } else if self.bank == 0 && self.command == 0x3a && self.params.len() == 1 {
                self.colmod = word as u8;
            }
        }
    }
}

#[derive(Debug)]
pub struct St7701 {
    id: String,
    pins: ControlPins,
    powered: bool,
    state: Mutex<State>,
}

impl St7701 {
    pub fn new(id: impl Into<String>, pins: ControlPins, powered: bool) -> Self {
        Self {
            id: id.into(),
            pins,
            powered,
            state: Mutex::new(State::new()),
        }
    }
}

impl GpioObserver for St7701 {
    fn on_pin_change(&self, pin: u8, _from: bool, to: bool, _sim_cycle: u64) {
        if !self.powered {
            return;
        }
        if pin != self.pins.cs
            && pin != self.pins.sck
            && pin != self.pins.mosi
            && pin != self.pins.backlight
            && Some(pin) != self.pins.reset
        {
            return;
        }
        let mut s = self.state.lock().unwrap();
        if pin == self.pins.backlight {
            s.backlight = to;
        }
        if Some(pin) == self.pins.reset {
            s.reset = to;
            if !to {
                s.reset_controller();
            }
            return;
        }
        if pin == self.pins.cs {
            s.cs = to;
            s.bits = 0;
            s.serial = 0;
        }
        if pin == self.pins.mosi {
            s.mosi = to;
        }
        if pin == self.pins.sck {
            let rising = !s.sck && to;
            s.sck = to;
            if rising && !s.cs && s.reset {
                s.serial = (s.serial << 1) | u16::from(s.mosi);
                s.bits += 1;
                if s.bits == 9 {
                    let word = s.serial;
                    s.bits = 0;
                    s.serial = 0;
                    s.serial_word(word);
                }
            }
        }
    }
}

impl RgbPanel for St7701 {
    fn rgb_frame(&self, width: u16, height: u16, pixels: &[u16]) {
        if !self.powered
            || usize::from(width) != WIDTH
            || usize::from(height) != HEIGHT
            || pixels.len() != WIDTH * HEIGHT
        {
            return;
        }
        let mut s = self.state.lock().unwrap();
        if !s.reset {
            return;
        }
        // The S040's physical RGB565 wiring is used with type9 COLMOD=0x60
        // (RGB666). Missing R/B low bits are not imaginary extra DMA bytes.
        // Both documented 565 and that board's 666 init yield a 565 artifact.
        if !matches!(s.colmod & 0x70, 0x50 | 0x60) {
            return;
        }
        for (bytes, pixel) in s.framebuffer.chunks_exact_mut(2).zip(pixels) {
            bytes.copy_from_slice(&pixel.to_be_bytes());
        }
        s.frames_received = s.frames_received.wrapping_add(1);
    }
}

impl crate::inspect::DeviceEvidence for St7701 {
    fn artifacts(
        &self,
        id: &str,
        opts: &crate::inspect::InspectOpts,
    ) -> Vec<crate::inspect::Artifact> {
        let s = self.state.lock().unwrap();
        let visible = self.powered && s.reset && !s.sleeping && s.display_on && s.backlight;
        let dark;
        let bytes = if visible {
            &s.framebuffer
        } else {
            dark = vec![0; s.framebuffer.len()];
            &dark
        };
        vec![crate::inspect::Artifact {
            kind: "framebuffer".into(),
            id: id.into(),
            meta: serde_json::json!({
                "w": WIDTH, "h": HEIGHT, "format": crate::inspect::artifact_format::RGB565_BE,
                "generation": crate::inspect::artifact_generation(bytes),
                "display_on": s.display_on, "sleeping": s.sleeping,
                "backlight_on": s.backlight, "powered": self.powered,
                "frames_received": s.frames_received, "colmod": s.colmod,
                "painted_bytes": bytes.iter().filter(|&&b| b != 0).count(),
                "total_bytes": bytes.len(), "bus": "rgb-16bit-functional"
            }),
            bytes: crate::inspect::artifact_bytes(bytes, opts),
        }]
    }
}

impl crate::bus::ObservedDevice for St7701 {
    fn manifest_id(&self) -> &str {
        &self.id
    }
    fn evidence(&self) -> Option<&dyn crate::inspect::DeviceEvidence> {
        Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_arc_any(self: Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync> {
        self
    }
}

pub struct St7701Kit;
pub static ST7701_KIT: St7701Kit = St7701Kit;
static METADATA: KitMetadata = KitMetadata {
    inputs: Cow::Borrowed(&[]), device_type: Cow::Borrowed("st7701-rgb"),
    label: Cow::Borrowed("ST7701 480x480 RGB panel"),
    summary: Cow::Borrowed("S040 16-bit RGB frame receiver and 9-bit GPIO control."),
    detail: Cow::Borrowed("Functional LCD_CAM RGB565 frames, sleep/display commands, type9 bank selection and backlight. RGB pad timing/mux and analog panel behavior are not modeled."),
    transport: Transport::GpioGroup, category: Category::Gpio,
    config_keys: Cow::Borrowed(&[
        ConfigKey { name: Cow::Borrowed("cs_pin"), ty: ConfigType::Str, doc: Cow::Borrowed("Active-low control CS, default GPIO39.") },
        ConfigKey { name: Cow::Borrowed("sck_pin"), ty: ConfigType::Str, doc: Cow::Borrowed("Rising-edge 9-bit control clock, default GPIO48.") },
        ConfigKey { name: Cow::Borrowed("mosi_pin"), ty: ConfigType::Str, doc: Cow::Borrowed("Control data, default GPIO47.") },
        ConfigKey { name: Cow::Borrowed("backlight_pin"), ty: ConfigType::Str, doc: Cow::Borrowed("Active-high backlight, default GPIO38.") },
        ConfigKey { name: Cow::Borrowed("reset_pin"), ty: ConfigType::Str, doc: Cow::Borrowed("Optional active-low reset. S040 C_I has no MCU reset connection.") },
        crate::peripherals::components::supply::POWERED_CONFIG_KEY,
    ]), labs: Cow::Borrowed(&[]),
};

impl PeripheralKit for St7701Kit {
    fn metadata(&self) -> &'static KitMetadata {
        &METADATA
    }
    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        let reset = ctx
            .config_str("reset_pin")
            .map(|pin| {
                ctx.parse_gpio_pin(pin)
                    .ok_or_else(|| anyhow::anyhow!("invalid ST7701 reset pin {pin}"))
            })
            .transpose()?;
        let pins = ControlPins {
            cs: ctx.config_gpio_pin("cs_pin", "CS", "GPIO39")?,
            sck: ctx.config_gpio_pin("sck_pin", "SCK", "GPIO48")?,
            mosi: ctx.config_gpio_pin("mosi_pin", "MOSI", "GPIO47")?,
            backlight: ctx.config_gpio_pin("backlight_pin", "BL", "GPIO38")?,
            reset,
        };
        let mut distinct = vec![pins.cs, pins.sck, pins.mosi, pins.backlight];
        distinct.extend(reset);
        let n = distinct.len();
        distinct.sort();
        distinct.dedup();
        anyhow::ensure!(distinct.len() == n, "ST7701 control pins must be distinct");
        let panel = Arc::new(St7701::new(
            ctx.device_id(),
            pins,
            crate::peripherals::components::supply::powered_from_config(ctx),
        ));
        let idx = ctx
            .bus
            .find_peripheral_index_by_name("lcd_cam")
            .ok_or_else(|| anyhow::anyhow!("st7701-rgb needs an ESP32-S3 LCD_CAM RGB master"))?;
        anyhow::ensure!(
            ctx.bus.peripherals[idx].dev.attach_rgb_panel(panel.clone()),
            "st7701-rgb requires an RGB-capable LCD master"
        );
        anyhow::ensure!(
            ctx.install_gpio_observer(panel.clone()),
            "st7701-rgb needs GPIO control-edge observation"
        );
        ctx.bus.observe_device(panel);
        Ok(())
    }
}
