// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! GT911 polling touch controller, functional single-contact model.
//!
//! Register protocol follows Espressif's `esp_lcd_touch_gt911` driver: a
//! big-endian 16-bit register pointer, little-endian coordinates, ready/count
//! at 0x814e, eight bytes per contact, and a firmware write of zero to acknowledge.
//! Once read by the guest, a report is immutable through acknowledgement;
//! newer host movement coalesces into the next report. There is no
//! interrupt/reset pin on the S040 C_I polling profile.
//! This is not a capacitive sensing, gesture, multi-contact or flash model.

use std::borrow::Cow;

use crate::peripherals::device::I2cDevice;
use crate::peripherals::kit::{
    AttachCtx, Category, ConfigKey, ConfigType, KitMetadata, PeripheralKit, Transport,
};
use crate::sim_input::{InputChannel, SimInput, SimInputError};

const CONFIG_START: u16 = 0x8047;
const CONFIG_END: u16 = 0x8100;
const STATUS: u16 = 0x814e;
const PRODUCT_ID: u16 = 0x8140;
const COMMAND: u16 = 0x8040;
const REPORT_US: u64 = 10_000;

pub const GT911_INPUTS: &[InputChannel] = &[
    InputChannel {
        key: Cow::Borrowed("x"),
        label: Cow::Borrowed("Touch X"),
        unit: Cow::Borrowed("px"),
        min: 0.0,
        max: 479.0,
        default: Some(0.0),
    },
    InputChannel {
        key: Cow::Borrowed("y"),
        label: Cow::Borrowed("Touch Y"),
        unit: Cow::Borrowed("px"),
        min: 0.0,
        max: 479.0,
        default: Some(0.0),
    },
    InputChannel {
        key: Cow::Borrowed("pressed"),
        label: Cow::Borrowed("Touch pressed"),
        unit: Cow::Borrowed(""),
        min: 0.0,
        max: 1.0,
        default: Some(0.0),
    },
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Contact {
    x: u16,
    y: u16,
    pressed: bool,
}

#[derive(Debug)]
pub struct Gt911 {
    address: u8,
    id: Option<String>,
    pointer: u16,
    pointer_bytes: u8,
    config: [u8; (CONFIG_END - CONFIG_START + 1) as usize],
    contact: Contact,
    report: Option<Contact>,
    report_read: bool,
    queued: Option<Contact>,
    sleeping: bool,
    elapsed_us: u64,
}

impl Gt911 {
    pub fn new(address: u8) -> Self {
        let mut config = [0; (CONFIG_END - CONFIG_START + 1) as usize];
        // Model configuration, not a claim of a specific module's factory
        // firmware: 480x480, up to one reported contact, a 10ms sample period.
        config[0] = 0x41;
        config[1..3].copy_from_slice(&480u16.to_le_bytes());
        config[3..5].copy_from_slice(&480u16.to_le_bytes());
        config[5] = 1;
        Self {
            address,
            id: None,
            pointer: 0,
            pointer_bytes: 0,
            config,
            contact: Contact::default(),
            report: None,
            report_read: false,
            queued: None,
            sleeping: false,
            elapsed_us: 0,
        }
    }

    fn publish(&mut self) {
        if self.sleeping {
            return;
        }
        if self.report.is_none() || !self.report_read {
            self.report = Some(self.contact);
        } else {
            self.queued = Some(self.contact);
        }
    }

    fn register(&self, reg: u16) -> u8 {
        match reg {
            PRODUCT_ID..=0x8143 => b"911\0"[(reg - PRODUCT_ID) as usize],
            // Resolution readback, as used by common Arduino GT911 drivers.
            0x8146 | 0x8148 => 0xe0,
            0x8147 | 0x8149 => 0x01,
            STATUS => self.report.map_or(0, |c| 0x80 | u8::from(c.pressed)),
            0x814f..=0x8156 => {
                let Some(c) = self.report.filter(|c| c.pressed) else {
                    return 0;
                };
                let x = c.x.to_le_bytes();
                let y = c.y.to_le_bytes();
                [0, x[0], x[1], y[0], y[1], 1, 0, 0][(reg - 0x814f) as usize]
            }
            CONFIG_START..=CONFIG_END => self.config[(reg - CONFIG_START) as usize],
            _ => 0,
        }
    }

    fn write_register(&mut self, reg: u16, value: u8) {
        match reg {
            STATUS if value == 0 => {
                self.report = self.queued.take();
                self.report_read = false;
            }
            COMMAND if value == 5 => {
                self.sleeping = true;
                self.report = None;
                self.queued = None;
                self.report_read = false;
            }
            COMMAND if value == 0 => {
                self.sleeping = false;
            }
            CONFIG_START..=CONFIG_END => self.config[(reg - CONFIG_START) as usize] = value,
            _ => {} // Identification and contact data are read-only.
        }
    }
}

impl I2cDevice for Gt911 {
    fn address(&self) -> u8 {
        self.address
    }
    fn start(&mut self) {
        self.pointer_bytes = 0;
    }
    fn stop(&mut self) {
        self.pointer_bytes = 0;
    }
    fn read(&mut self) -> u8 {
        // Freeze at the first guest observation, after an atomic host x/y/
        // pressed update has finished. Later host changes cannot tear a report
        // between the driver's status read and coordinate burst.
        if (STATUS..=0x8156).contains(&self.pointer) && self.report.is_some() {
            self.report_read = true;
        }
        let value = self.register(self.pointer);
        self.pointer = self.pointer.wrapping_add(1);
        value
    }
    fn write(&mut self, data: u8) {
        match self.pointer_bytes {
            0 => {
                self.pointer = u16::from(data) << 8;
                self.pointer_bytes = 1;
            }
            1 => {
                self.pointer |= u16::from(data);
                self.pointer_bytes = 2;
            }
            _ => {
                self.write_register(self.pointer, data);
                self.pointer = self.pointer.wrapping_add(1);
            }
        }
    }
    fn advance_time_us(&mut self, us: u64) {
        self.elapsed_us = self.elapsed_us.saturating_add(us);
        if self.elapsed_us >= REPORT_US {
            self.elapsed_us %= REPORT_US;
            // Repeated held-contact samples are driven by simulation time,
            // never manufactured by reading the status register.
            if self.contact.pressed {
                self.publish();
            }
        }
    }
    fn as_sim_input_mut(&mut self) -> Option<&mut dyn SimInput> {
        Some(self)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl SimInput for Gt911 {
    fn input_channels(&self) -> &[InputChannel] {
        GT911_INPUTS
    }
    fn component_id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    fn set_component_id(&mut self, id: String) {
        self.id = Some(id);
    }
    fn set_input(&mut self, key: &str, value: f64) -> Result<(), SimInputError> {
        self.require_channel(key, value)?;
        let old = self.contact;
        match key {
            "x" => self.contact.x = value.round() as u16,
            "y" => self.contact.y = value.round() as u16,
            "pressed" => self.contact.pressed = value >= 0.5,
            _ => unreachable!("require_channel checked the key"),
        }
        if self.contact != old && (self.contact.pressed || old.pressed) {
            self.publish();
        }
        Ok(())
    }
}

pub struct Gt911Kit;
pub static GT911_KIT: Gt911Kit = Gt911Kit;
static METADATA: KitMetadata = KitMetadata {
    inputs: Cow::Borrowed(GT911_INPUTS), device_type: Cow::Borrowed("gt911"),
    label: Cow::Borrowed("GT911 touch (polling)"),
    summary: Cow::Borrowed("480x480 single-contact GT911 register and ready/ack model."),
    detail: Cow::Borrowed("16-bit I2C pointer, product ID, configuration storage and one contact. Set x/y then pressed. No reset-address strap, IRQ, gestures or capacitive physics."),
    transport: Transport::I2c, category: Category::I2c,
    config_keys: Cow::Borrowed(&[ConfigKey { name: Cow::Borrowed("i2c_address"), ty: ConfigType::Int, doc: Cow::Borrowed("7-bit address: 0x5d (default) or 0x14.") }]),
    labs: Cow::Borrowed(&[]),
};
impl PeripheralKit for Gt911Kit {
    fn metadata(&self) -> &'static KitMetadata {
        &METADATA
    }
    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        let address = ctx.i2c_address_or(0x5d)?;
        anyhow::ensure!(
            matches!(address, 0x5d | 0x14),
            "GT911 supports address 0x5d or 0x14"
        );
        ctx.attach_i2c_device(Box::new(Gt911::new(address)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(d: &mut Gt911, reg: u16, n: usize) -> Vec<u8> {
        d.start();
        for b in reg.to_be_bytes() {
            d.write(b);
        }
        d.start();
        let data = (0..n).map(|_| d.read()).collect();
        d.stop();
        data
    }
    fn write(d: &mut Gt911, reg: u16, bytes: &[u8]) {
        d.start();
        for b in reg.to_be_bytes().into_iter().chain(bytes.iter().copied()) {
            d.write(b);
        }
        d.stop();
    }
    #[test]
    fn identity_pointer_config_and_read_only_registers() {
        let mut d = Gt911::new(0x5d);
        assert_eq!(read(&mut d, PRODUCT_ID, 4), b"911\0");
        write(&mut d, PRODUCT_ID, b"bad!");
        assert_eq!(read(&mut d, PRODUCT_ID, 4), b"911\0");
        assert_eq!(read(&mut d, CONFIG_START + 1, 4), [0xe0, 1, 0xe0, 1]);
        write(&mut d, CONFIG_START + 8, &[7, 9]);
        assert_eq!(read(&mut d, CONFIG_START + 8, 2), [7, 9]);
        assert_eq!(read(&mut d, STATUS, 1), [0]);
    }
    #[test]
    fn down_move_up_are_coherent_until_acknowledged() {
        let mut d = Gt911::new(0x5d);
        d.set_input("x", 123.0).unwrap();
        d.set_input("y", 456.0).unwrap();
        assert_eq!(read(&mut d, STATUS, 1), [0]);
        d.set_input("pressed", 1.0).unwrap();
        assert_eq!(read(&mut d, STATUS, 9), [0x81, 0, 123, 0, 200, 1, 1, 0, 0]);
        d.set_input("x", 321.0).unwrap();
        d.set_input("y", 47.0).unwrap();
        assert_eq!(read(&mut d, STATUS, 9), [0x81, 0, 123, 0, 200, 1, 1, 0, 0]);
        write(&mut d, STATUS, &[0]);
        assert_eq!(read(&mut d, STATUS, 9), [0x81, 0, 65, 1, 47, 0, 1, 0, 0]);
        d.set_input("pressed", 0.0).unwrap();
        write(&mut d, STATUS, &[0]);
        assert_eq!(read(&mut d, STATUS, 1), [0x80]);
        write(&mut d, STATUS, &[0]);
        assert_eq!(read(&mut d, STATUS, 1), [0]);
    }
    #[test]
    fn held_contact_is_time_driven_and_input_ranges_are_enforced() {
        let mut d = Gt911::new(0x14);
        d.set_input("pressed", 1.0).unwrap();
        write(&mut d, STATUS, &[0]);
        assert_eq!(read(&mut d, STATUS, 1), [0]);
        d.advance_time_us(REPORT_US - 1);
        assert_eq!(read(&mut d, STATUS, 1), [0]);
        d.advance_time_us(1);
        assert_eq!(read(&mut d, STATUS, 1), [0x81]);
        assert!(d.set_input("x", 480.0).is_err());
        assert!(d.set_input("y", f64::NAN).is_err());
        write(&mut d, COMMAND, &[5]);
        d.advance_time_us(REPORT_US);
        assert_eq!(read(&mut d, STATUS, 1), [0]);
    }
}
