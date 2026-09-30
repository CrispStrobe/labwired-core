// SPDX-License-Identifier: MIT
//! Original bounded, polled LSM303AGR I²C model, derived from ST DocID027765
//! Rev 11: https://www.st.com/resource/en/datasheet/lsm303agr.pdf.
//! Two independently attached slaves represent the two silicon address spaces.
//! Samples are driven only by physical SimInput values and simulated ODR time.
//! BDU holds each unread axis pair across transactions; after both bytes are
//! read that axis refreshes at the next ODR boundary (no hidden FIFO).
//! No noise, gravity, temperature, self-test, FIFO, digital filters, turn-on
//! analog settling, interrupt generation or shared open-drain GPIO is modeled.
//! HR mode uses the specified seven-period turn-on delay; other modes expose
//! their first sample after one period. Magnetometer single conversion uses
//! one configured ODR period, a bounded approximation of its conversion time.

use crate::peripherals::i2c::I2cDevice;
use crate::sim_input::{InputChannel, SimInput, SimInputError};
use std::borrow::Cow;

pub const ACCEL_ADDRESS: u8 = 0x19;
pub const MAG_ADDRESS: u8 = 0x1e;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Accel,
    Mag,
}

pub struct Lsm303agr {
    kind: Kind,
    registers: [u8; 128],
    input: [f64; 3],
    output: [i16; 3],
    ready: u8,
    overrun: u8,
    read_parts: [u8; 3],
    phase: u128,
    startup_periods: u8,
    pointer: u8,
    increment: bool,
    expect_pointer: bool,
    component_id: Option<String>,
}

impl Lsm303agr {
    pub fn new_accel() -> Self {
        Self::new(Kind::Accel)
    }
    pub fn new_mag() -> Self {
        Self::new(Kind::Mag)
    }
    fn new(kind: Kind) -> Self {
        let mut d = Self {
            kind,
            registers: [0; 128],
            input: [0.0; 3],
            output: [0; 3],
            ready: 0,
            overrun: 0,
            read_parts: [0; 3],
            phase: 0,
            startup_periods: 0,
            pointer: 0,
            increment: false,
            expect_pointer: true,
            component_id: None,
        };
        d.reset_registers();
        d
    }
    fn reset_registers(&mut self) {
        self.registers = [0; 128];
        if self.kind == Kind::Accel {
            self.registers[0x0f] = 0x33;
            self.registers[0x20] = 0x07;
            self.registers[0x2f] = 0x20; // FIFO empty, bypass only
        } else {
            self.registers[0x4f] = 0x40;
            self.registers[0x60] = 0x03;
            self.registers[0x63] = 0xe0;
        }
        self.output = [0; 3];
        self.ready = 0;
        self.overrun = 0;
        self.read_parts = [0; 3];
        self.phase = 0;
        self.startup_periods = 0;
    }
    fn rate(&self) -> u32 {
        if self.kind == Kind::Mag {
            if self.registers[0x60] & 3 >= 2 {
                return 0;
            }
            return [10, 20, 50, 100][((self.registers[0x60] >> 2) & 3) as usize];
        }
        let lp = self.registers[0x20] & 8 != 0;
        if lp && self.registers[0x23] & 8 != 0 {
            return 0;
        } // forbidden mode
        match self.registers[0x20] >> 4 {
            1 => 1,
            2 => 10,
            3 => 25,
            4 => 50,
            5 => 100,
            6 => 200,
            7 => 400,
            8 if lp => 1620,
            9 if lp => 5376,
            9 => 1344,
            _ => 0,
        }
    }
    fn bdu(&self) -> bool {
        if self.kind == Kind::Accel {
            self.registers[0x23] & 0x80 != 0
        } else {
            self.registers[0x62] & 0x10 != 0
        }
    }
    fn big_endian(&self) -> bool {
        if self.kind == Kind::Accel {
            self.registers[0x23] & 0x48 == 0x48
        } else {
            self.registers[0x62] & 8 != 0
        }
    }
    fn enabled_axes(&self) -> u8 {
        if self.kind == Kind::Accel {
            self.registers[0x20] & 7
        } else {
            7
        }
    }
    fn encode(&self, axis: usize) -> i16 {
        if self.kind == Kind::Mag {
            let offset = i16::from_le_bytes([
                self.registers[0x45 + axis * 2],
                self.registers[0x46 + axis * 2],
            ]) as f64;
            return (self.input[axis] / 0.15 - offset)
                .round()
                .clamp(-32768.0, 32767.0) as i16;
        }
        let fs = ((self.registers[0x23] >> 4) & 3) as usize;
        // Rev 11 Table 3 typical sensitivities in mg/LSB, not rounded app-note values.
        let (bits, sensitivity) = if self.registers[0x20] & 8 != 0 {
            (8, [15.63, 31.26, 62.52, 187.58][fs])
        } else if self.registers[0x23] & 8 != 0 {
            (12, [0.98, 1.95, 3.9, 11.72][fs])
        } else {
            (10, [3.9, 7.82, 15.63, 46.9][fs])
        };
        let limit = (1_i32 << (bits - 1)) as f64;
        let full_scale = [2.0, 4.0, 8.0, 16.0][fs];
        let raw = (self.input[axis].clamp(-full_scale, full_scale) * 1000.0 / sensitivity)
            .round()
            .clamp(-limit, limit - 1.0) as i16;
        raw.wrapping_shl(16 - bits)
    }
    fn sample(&mut self, periods: u128) {
        for axis in 0..3 {
            let bit = 1 << axis;
            if self.enabled_axes() & bit == 0 {
                continue;
            }
            if self.bdu() && self.ready & bit != 0 {
                continue;
            }
            if self.ready & bit != 0 || (!self.bdu() && periods > 1) {
                self.overrun |= bit;
            }
            self.output[axis] = self.encode(axis);
            self.ready |= bit;
            self.read_parts[axis] = 0;
        }
    }
    fn write_register(&mut self, address: u8, value: u8) {
        let allowed = if self.kind == Kind::Accel {
            matches!(address,0x1f..=0x26|0x2e|0x30|0x32..=0x34|0x36..=0x38|0x3a..=0x3f)
        } else {
            matches!(address,0x45..=0x4a|0x60..=0x63|0x65..=0x66)
        };
        if !allowed {
            return;
        }
        if self.kind == Kind::Mag && address == 0x60 && value & 0x20 != 0 {
            self.reset_registers();
            return;
        }
        let old = self.registers[address as usize];
        self.registers[address as usize] = value;
        let timing_change = if self.kind == Kind::Accel {
            (address == 0x20 && (old ^ value) & 0xf8 != 0)
                || (address == 0x23 && (old ^ value) & 8 != 0)
        } else {
            address == 0x60 && ((old ^ value) & 0x1f != 0 || value & 3 == 1)
        };
        if timing_change {
            self.phase = 0;
            self.startup_periods = if self.kind == Kind::Accel && self.registers[0x23] & 8 != 0 {
                6
            } else {
                0
            };
        }
        if self.kind == Kind::Accel && address == 0x20 {
            self.ready &= value & 7;
            self.overrun &= value & 7;
        }
    }
    fn next_pointer(&mut self) {
        if self.increment {
            self.pointer = self.pointer.wrapping_add(1) & 0x7f;
        }
    }
    fn status(&self) -> u8 {
        self.ready
            | if self.ready != 0 { 8 } else { 0 }
            | (self.overrun << 4)
            | if self.overrun != 0 { 0x80 } else { 0 }
    }
}

impl I2cDevice for Lsm303agr {
    fn address(&self) -> u8 {
        if self.kind == Kind::Accel {
            ACCEL_ADDRESS
        } else {
            MAG_ADDRESS
        }
    }
    fn claims_address(&self, addr: u8) -> bool {
        addr == self.address() && !(self.kind == Kind::Mag && self.registers[0x62] & 0x20 != 0)
    }
    fn start(&mut self) {
        self.expect_pointer = true;
    }
    fn stop(&mut self) {
        self.expect_pointer = true;
    } // pointer / BDU persist across STOP
    fn write(&mut self, data: u8) {
        if self.expect_pointer {
            self.pointer = data & 0x7f;
            self.increment = data & 0x80 != 0;
            self.expect_pointer = false;
        } else {
            self.write_register(self.pointer, data);
            self.next_pointer();
        }
    }
    fn read(&mut self) -> u8 {
        let output_base = if self.kind == Kind::Accel { 0x28 } else { 0x68 };
        let status_address = output_base - 1;
        let value = if self.pointer == status_address {
            self.status()
        } else if (output_base..output_base + 6).contains(&self.pointer) {
            let n = (self.pointer - output_base) as usize;
            let axis = n / 2;
            let bytes = if self.big_endian() {
                self.output[axis].to_be_bytes()
            } else {
                self.output[axis].to_le_bytes()
            };
            self.read_parts[axis] |= 1 << (n % 2);
            if self.read_parts[axis] == 3 {
                self.ready &= !(1 << axis);
                self.overrun &= !(1 << axis);
            }
            bytes[n % 2]
        } else {
            self.registers[self.pointer as usize]
        };
        self.next_pointer();
        value
    }
    fn advance_time_us(&mut self, us: u64) {
        let rate = self.rate();
        if rate == 0 {
            return;
        }
        let total = self.phase + u128::from(us) * u128::from(rate);
        let mut periods = total / 1_000_000;
        self.phase = total % 1_000_000;
        let skip = periods.min(u128::from(self.startup_periods));
        self.startup_periods -= skip as u8;
        periods -= skip;
        if periods == 0 {
            return;
        }
        if self.kind == Kind::Mag && self.registers[0x60] & 3 == 1 {
            self.sample(1);
            self.registers[0x60] = (self.registers[0x60] & !3) | 3;
            self.phase = 0;
        } else {
            self.sample(periods);
        } // O(axes), including u64::MAX time jumps
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn as_sim_input_mut(&mut self) -> Option<&mut dyn SimInput> {
        Some(self)
    }
}

macro_rules! channels {
    ($unit:literal,$min:expr,$max:expr) => {
        &[
            InputChannel {
                key: Cow::Borrowed("x"),
                label: Cow::Borrowed("X"),
                unit: Cow::Borrowed($unit),
                min: $min,
                max: $max,
            },
            InputChannel {
                key: Cow::Borrowed("y"),
                label: Cow::Borrowed("Y"),
                unit: Cow::Borrowed($unit),
                min: $min,
                max: $max,
            },
            InputChannel {
                key: Cow::Borrowed("z"),
                label: Cow::Borrowed("Z"),
                unit: Cow::Borrowed($unit),
                min: $min,
                max: $max,
            },
        ]
    };
}
pub const ACCEL_INPUTS: &[InputChannel] = channels!("g", -16.0, 16.0);
pub const MAG_INPUTS: &[InputChannel] = channels!("µT", -4915.2, 4915.2);
impl SimInput for Lsm303agr {
    fn input_channels(&self) -> &[InputChannel] {
        if self.kind == Kind::Accel {
            ACCEL_INPUTS
        } else {
            MAG_INPUTS
        }
    }
    fn set_input(&mut self, key: &str, value: f64) -> Result<(), SimInputError> {
        self.require_channel(key, value)?;
        if !value.is_finite() {
            return Err(SimInputError::Evaluation(
                "motion input must be finite".into(),
            ));
        }
        self.input[match key {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            _ => unreachable!(),
        }] = value;
        Ok(())
    }
    fn component_id(&self) -> Option<&str> {
        self.component_id.as_deref()
    }
    fn set_component_id(&mut self, id: String) {
        self.component_id = Some(id);
    }
}

use crate::peripherals::kit::{AttachCtx, Category, KitMetadata, PeripheralKit, Transport};
pub struct Lsm303agrKit {
    mag: bool,
}
pub static LSM303AGR_ACCEL_KIT: Lsm303agrKit = Lsm303agrKit { mag: false };
pub static LSM303AGR_MAG_KIT: Lsm303agrKit = Lsm303agrKit { mag: true };
macro_rules! metadata { ($name:literal,$label:literal,$inputs:expr) => { KitMetadata {
    device_type:Cow::Borrowed($name),label:Cow::Borrowed($label),
    summary:Cow::Borrowed("Polled LSM303AGR motion slave with physical inputs and simulated ODR."),
    detail:Cow::Borrowed("Fixed silicon address; byte-pointer I2C with SUB7 auto-increment, ODR-gated samples and persistent per-axis BDU. No FIFO, filters, temperature, self-test or interrupt wiring."),
    transport:Transport::I2c,category:Category::I2c,config_keys:Cow::Borrowed(&[]),labs:Cow::Borrowed(&[]),inputs:Cow::Borrowed($inputs),
} }; }
static ACCEL_METADATA: KitMetadata =
    metadata!("lsm303agr_accel", "LSM303AGR accelerometer", ACCEL_INPUTS);
static MAG_METADATA: KitMetadata = metadata!("lsm303agr_mag", "LSM303AGR magnetometer", MAG_INPUTS);
impl PeripheralKit for Lsm303agrKit {
    fn metadata(&self) -> &KitMetadata {
        if self.mag {
            &MAG_METADATA
        } else {
            &ACCEL_METADATA
        }
    }
    fn attach(&self, ctx: &mut AttachCtx<'_>) -> anyhow::Result<()> {
        let device = if self.mag {
            Lsm303agr::new_mag()
        } else {
            Lsm303agr::new_accel()
        };
        anyhow::ensure!(
            ctx.i2c_address_or(device.address())? == device.address(),
            "LSM303AGR address is fixed by silicon"
        );
        ctx.attach_i2c_device(Box::new(device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write(d: &mut Lsm303agr, address: u8, data: &[u8]) {
        d.start();
        d.write(address);
        for &v in data {
            d.write(v);
        }
        d.stop();
    }
    fn read(d: &mut Lsm303agr, address: u8, n: usize) -> Vec<u8> {
        d.start();
        d.write(address);
        d.start();
        let v = (0..n).map(|_| d.read()).collect();
        d.stop();
        v
    }
    fn word(d: &mut Lsm303agr, address: u8) -> i16 {
        let v = read(d, address | 0x80, 2);
        i16::from_le_bytes([v[0], v[1]])
    }
    #[test]
    fn reset_and_read_only_identity() {
        let mut a = Lsm303agr::new_accel();
        let mut m = Lsm303agr::new_mag();
        assert_eq!(a.address(), 0x19);
        assert_eq!(m.address(), 0x1e);
        write(&mut a, 0x0f, &[0]);
        write(&mut m, 0x4f, &[0]);
        assert_eq!(read(&mut a, 0x0f, 1), [0x33]);
        assert_eq!(read(&mut m, 0x4f, 1), [0x40]);
        a.advance_time_us(u64::MAX);
        m.advance_time_us(u64::MAX);
        assert_eq!(read(&mut a, 0x20, 1), [7]);
        assert_eq!(read(&mut m, 0x60, 1), [3]);
        assert_eq!(read(&mut a, 0x27, 1), [0]);
        assert_eq!(read(&mut m, 0x67, 1), [0]);
    }
    #[test]
    fn conditional_increment_read_and_write() {
        let mut d = Lsm303agr::new_accel();
        write(&mut d, 0x21, &[0x12, 0x34]);
        assert_eq!(read(&mut d, 0x21, 2), [0x34, 0x34]);
        write(&mut d, 0xa1, &[0x12, 0x34]);
        assert_eq!(read(&mut d, 0xa1, 2), [0x12, 0x34]);
    }
    #[test]
    fn input_validation_and_component_identity() {
        let mut d = Lsm303agr::new_accel();
        d.set_component_id("accel".into());
        assert_eq!(d.component_id(), Some("accel"));
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 16.01] {
            assert!(d.set_input("x", value).is_err());
        }
        assert!(d.set_input("invented", 0.0).is_err());
        assert_eq!(d.input, [0.0; 3]);
        d.set_input("x", -16.0).unwrap();
    }
    #[test]
    fn odr_gate_overrun_and_read_acknowledgement() {
        let mut d = Lsm303agr::new_accel();
        d.set_input("x", 1.0).unwrap();
        write(&mut d, 0x20, &[0x57]);
        d.advance_time_us(9999);
        assert_eq!(read(&mut d, 0x27, 1), [0]);
        d.advance_time_us(1);
        assert_eq!(read(&mut d, 0x27, 1), [0x0f]);
        assert_eq!(word(&mut d, 0x28), 16384);
        assert_eq!(read(&mut d, 0x27, 1), [0x0e]);
        d.advance_time_us(10000);
        assert_eq!(read(&mut d, 0x27, 1), [0xef]);
        read(&mut d, 0xa8, 6);
        assert_eq!(read(&mut d, 0x27, 1), [0]);
    }
    #[test]
    fn sensitivity_modes_and_full_scale_clamp() {
        for (ctrl, ctrl4, bits, sens) in [
            (0x5f, 0, 8, 15.63),
            (0x57, 0, 10, 3.9),
            (0x57, 8, 12, 0.98),
            (0x57, 0x38, 12, 11.72),
        ] {
            let mut d = Lsm303agr::new_accel();
            write(&mut d, 0x20, &[ctrl]);
            write(&mut d, 0x23, &[ctrl4]);
            d.set_input("x", -1.0).unwrap();
            d.advance_time_us(70000);
            assert_eq!(
                word(&mut d, 0x28),
                ((-1000.0_f64 / sens).round() as i16) << (16 - bits)
            );
        }
        let mut d = Lsm303agr::new_accel();
        write(&mut d, 0x20, &[0x57]);
        write(&mut d, 0x23, &[8]);
        d.set_input("x", 16.0).unwrap();
        d.advance_time_us(70000);
        assert_eq!(
            word(&mut d, 0x28),
            ((2000.0_f64 / 0.98).round() as i16) << 4
        );
    }
    #[test]
    fn forbidden_mode_and_disabled_axes_do_not_sample() {
        let mut d = Lsm303agr::new_accel();
        write(&mut d, 0x20, &[0x5f]);
        write(&mut d, 0x23, &[8]);
        d.advance_time_us(u64::MAX);
        assert_eq!(d.status(), 0);
        write(&mut d, 0x23, &[0]);
        write(&mut d, 0x20, &[0x51]);
        d.advance_time_us(10000);
        assert_eq!(d.status(), 9);
        assert_eq!(d.output[1..], [0, 0]);
    }
    #[test]
    fn bdu_survives_split_transactions_and_releases_per_axis() {
        let mut d = Lsm303agr::new_accel();
        write(&mut d, 0x20, &[0x57]);
        write(&mut d, 0x23, &[0x80]);
        d.set_input("x", 1.0).unwrap();
        d.advance_time_us(10000);
        let low = read(&mut d, 0x28, 1)[0];
        d.set_input("x", -1.0).unwrap();
        d.advance_time_us(20000);
        let high = read(&mut d, 0x29, 1)[0];
        assert_eq!(i16::from_le_bytes([low, high]), 16384);
        assert_eq!(d.status(), 0x0e); // BDU did not overwrite unread Y/Z
        d.advance_time_us(10000);
        assert_eq!(word(&mut d, 0x28), -16384);
    }
    #[test]
    fn magnetometer_single_shot_offset_and_reset() {
        let mut d = Lsm303agr::new_mag();
        d.set_input("x", 15.0).unwrap();
        write(&mut d, 0xc5, &[10, 0]);
        write(&mut d, 0x60, &[0x81]);
        d.advance_time_us(99999);
        assert_eq!(d.status(), 0);
        d.advance_time_us(1);
        assert_eq!(word(&mut d, 0x68), 90);
        assert_eq!(read(&mut d, 0x60, 1), [0x83]);
        d.set_input("x", 30.0).unwrap();
        d.advance_time_us(u64::MAX);
        assert_eq!(word(&mut d, 0x68), 90);
        write(&mut d, 0x60, &[0x20]);
        assert_eq!(read(&mut d, 0x60, 1), [3]);
        assert_eq!(d.status(), 0);
        assert_eq!(d.input[0], 30.0); // reset silicon registers, not environmental stimulus
    }
    #[test]
    fn large_time_jumps_are_bounded_and_endian_is_configured() {
        let mut d = Lsm303agr::new_mag();
        d.set_input("x", -15.0).unwrap();
        write(&mut d, 0x60, &[0x8c]);
        d.advance_time_us(u64::MAX);
        assert_eq!(d.status(), 0xff);
        assert_eq!(word(&mut d, 0x68), -100);
        write(&mut d, 0x62, &[8]);
        d.advance_time_us(10000);
        assert_eq!(read(&mut d, 0xe8, 2), (-100_i16).to_be_bytes());
        write(&mut d, 0x62, &[0x20]);
        assert!(!d.claims_address(0x1e));
    }

    #[test]
    fn all_accelerometer_ranges_use_rev11_typical_sensitivity() {
        for (ctrl1, hr, bits, sensitivities) in [
            (0x5f, 0, 8, [15.63, 31.26, 62.52, 187.58]),
            (0x57, 0, 10, [3.9, 7.82, 15.63, 46.9]),
            (0x57, 8, 12, [0.98, 1.95, 3.9, 11.72]),
        ] {
            for (fs, sensitivity) in sensitivities.into_iter().enumerate() {
                let mut d = Lsm303agr::new_accel();
                write(&mut d, 0x20, &[ctrl1]);
                write(&mut d, 0x23, &[hr | ((fs as u8) << 4)]);
                d.set_input("x", 1.0).unwrap();
                d.set_input("y", -1.0).unwrap();
                d.advance_time_us(70_000);
                let expected = (1000.0_f64 / sensitivity).round() as i16;
                assert_eq!(word(&mut d, 0x28), expected << (16 - bits));
                assert_eq!(word(&mut d, 0x2a), -expected << (16 - bits));
            }
        }
    }

    #[test]
    fn hr_startup_waits_seven_periods_and_endian_requires_hr() {
        let mut d = Lsm303agr::new_accel();
        write(&mut d, 0x20, &[0x57]);
        write(&mut d, 0x23, &[0x48]);
        d.set_input("x", 1.0).unwrap();
        d.advance_time_us(69_999);
        assert_eq!(d.status(), 0);
        d.advance_time_us(1);
        assert_eq!(d.status(), 15);
        assert_eq!(read(&mut d, 0xa8, 2), 16320_i16.to_be_bytes());
        write(&mut d, 0x23, &[0x40]);
        d.advance_time_us(10_000);
        assert_eq!(read(&mut d, 0xa8, 2), 16384_i16.to_le_bytes());
    }

    #[test]
    fn both_slaves_bdu_hold_high_first_until_low_is_read() {
        for mut d in [Lsm303agr::new_accel(), Lsm303agr::new_mag()] {
            let base = if d.kind == Kind::Accel {
                write(&mut d, 0x20, &[0x57]);
                write(&mut d, 0x23, &[0x80]);
                0x28
            } else {
                write(&mut d, 0x60, &[0x8c]);
                write(&mut d, 0x62, &[0x10]);
                0x68
            };
            d.set_input("x", 1.0).unwrap();
            d.advance_time_us(10_000);
            let original = d.output[0];
            let high = read(&mut d, base + 1, 1)[0];
            d.set_input("x", -1.0).unwrap();
            d.advance_time_us(u64::MAX);
            assert_eq!(d.output[0], original);
            assert_eq!(d.overrun, 0);
            let low = read(&mut d, base, 1)[0];
            assert_eq!(i16::from_le_bytes([low, high]), original);
            assert_eq!(d.ready & 1, 0);
            d.advance_time_us(10_000);
            assert!(d.output[0] < 0);
        }
    }

    #[test]
    fn every_odr_uses_fractional_central_time_without_transaction_progress() {
        for (ctrl1, rate) in [
            (0x17, 1),
            (0x27, 10),
            (0x37, 25),
            (0x47, 50),
            (0x57, 100),
            (0x67, 200),
            (0x77, 400),
            (0x8f, 1620),
            (0x97, 1344),
            (0x9f, 5376),
        ] {
            let mut d = Lsm303agr::new_accel();
            write(&mut d, 0x20, &[ctrl1]);
            let period = (1_000_000_u64 + rate - 1) / rate;
            d.advance_time_us(period - 1);
            for _ in 0..10 {
                assert_eq!(read(&mut d, 0x27, 1), [0]);
            }
            d.advance_time_us(1);
            assert_eq!(d.status(), 15);
        }
        for (code, rate) in [(0, 10), (1, 20), (2, 50), (3, 100)] {
            let mut d = Lsm303agr::new_mag();
            write(&mut d, 0x60, &[0x80 | (code << 2)]);
            d.advance_time_us(1_000_000 / rate - 1);
            assert_eq!(d.status(), 0);
            d.advance_time_us(1);
            assert_eq!(d.status(), 15);
        }
    }
}
