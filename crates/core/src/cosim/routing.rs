// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Board signal routing for co-simulation.
//!
//! [`CosimRunner`](crate::cosim::CosimRunner) exchanges values with external
//! models through a flat signal store keyed by manifest paths. This module is
//! the half that fills that store from a running machine and writes routed
//! model outputs back into it, so a `cosim_models:` entry can name real
//! firmware pins instead of abstract observables:
//!
//! ```yaml
//! inputs:
//!   gpio: "board.gpio.pa5"            # firmware GPIO output level -> model
//!   drive: "board.gpio_output.pa5"    # is the firmware driving PA5 at all?
//! outputs:
//!   v_out: "board.analog.pa0_volts"   # model node voltage -> ADC channel
//! ```
//!
//! Paths outside the `board.` / `adc.` grammar are ordinary store paths and are
//! left untouched: a manifest that routes `control.enable` keeps behaving
//! exactly as it did before this module existed.
//!
//! Everything resolves ONCE, against the bus, in [`SignalRouter::bind`]. A pad
//! label cannot change owner mid-run, so the per-step work is an index and a
//! bit — and an unresolvable path is reported at bind time instead of silently
//! reading zero for the whole run.

use crate::bus::SystemBus;
use crate::cosim::{CosimRoutedModelStep, CosimRunner, CosimSignalValue, CosimSignals};
use crate::peripherals::gpio::{GpioMode, GpioPort};
use crate::{
    AdvanceReport, AdvanceRequest, AdvanceStop, Cpu, Machine, Peripheral, SimResult,
    SimulationError,
};
use labwired_config::CosimModelConfig;
use std::collections::BTreeSet;
use std::path::Path;

/// Default core clock assumed when a bus reports none, in Hz.
///
/// Every in-tree chip descriptor declares `cpu_hz` (a config gate enforces it),
/// so this only covers a hand-built bus in a test. Co-simulation needs a time
/// base to convert cycles into the `time_ns` an adapter is stepped to, and
/// dividing by zero is not an option; the session logs once when it falls back.
pub const FALLBACK_CPU_HZ: u64 = 16_000_000;

const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Simulated cycles → nanoseconds at `cpu_hz`, truncating.
pub fn cycles_to_ns(cycles: u64, cpu_hz: u64) -> u64 {
    if cpu_hz == 0 {
        return 0;
    }
    u64::try_from(u128::from(cycles) * NANOS_PER_SECOND / u128::from(cpu_hz)).unwrap_or(u64::MAX)
}

/// Nanoseconds → the FIRST cycle count whose [`cycles_to_ns`] is at or past
/// `ns` (ceiling division).
///
/// Ceiling, not truncation: this is used to decide how far the machine may run
/// before the next co-simulation boundary, and truncating would stop the
/// machine one cycle short of the boundary forever — the boundary would never
/// be reached and the run would crawl a cycle at a time.
pub fn ns_to_cycles(ns: u64, cpu_hz: u64) -> u64 {
    if cpu_hz == 0 {
        return 0;
    }
    let numerator = u128::from(ns) * u128::from(cpu_hz);
    let cycles = numerator.div_ceil(NANOS_PER_SECOND);
    u64::try_from(cycles).unwrap_or(u64::MAX)
}

/// Full-scale reference of the modelled STM32 ADC, in volts.
///
/// The ADC models hold injected stimuli in millivolts and own the
/// millivolt → count conversion — `Adc::set_channel_input` computes
/// `count = mV * 4095 / 3300`, i.e. 3.3 V is full scale at 12 bits. This
/// constant is here only so
/// a routed voltage can be clamped to something the pin could physically see;
/// the arithmetic deliberately stays in the ADC model, where every other
/// analog stimulus (thermistor, potentiometer, battery divider) already
/// converts.
pub const ADC_VREF_VOLTS: f64 = 3.3;

/// A manifest signal path that names something on the board.
///
/// Grammar (documented in `docs/cosimulation_plugins.md`):
///
/// | path | direction | type |
/// |------|-----------|------|
/// | `board.gpio.<pad>` | machine → model | `Bool` |
/// | `board.gpio_output.<pad>` | machine → model | `Bool` |
/// | `board.gpio_in.<pad>` | model → machine (also readable) | `Bool` |
/// | `board.analog.<pad>_volts` | model → machine | `F64` |
/// | `adc.<peripheral>.<channel>_volts` | model → machine | `F64` |
///
/// `<pad>` is a pad label in whatever form the chip speaks — `pa5` / `PA5` on
/// STM32, `p0.13` on Nordic, `gpio5` / `5` on ESP32 — resolved through the same
/// [`SystemBus`] pin resolution every other pad-addressed feature uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalPath {
    /// `board.gpio.<pad>` — the level the firmware is DRIVING on an output pad.
    GpioOutput { pad: String },
    /// `board.gpio_output.<pad>` — whether the firmware has made `<pad>` a
    /// general-purpose OUTPUT, read from the GPIO model's direction register
    /// (STM32 MODER / CRL-CRH, nRF DIR, ESP32 GPIO_ENABLE + output matrix, …).
    ///
    /// `board.gpio.<pad>` alone cannot tell a circuit whether the pin is
    /// driving: an input pad still has an output latch, and reading that latch
    /// as a source would clamp whatever the circuit puts on the pin.
    GpioDirection { pad: String },
    /// `board.gpio_in.<pad>` — the level an external driver holds on an input pad.
    GpioInput { pad: String },
    /// `board.analog.<pad>_volts` — the analog level on the ADC input the chip
    /// descriptor's `analog_pins:` assigns to `<pad>`.
    AnalogPad { pad: String },
    /// `adc.<peripheral>.<channel>_volts` — the analog level on an explicitly
    /// named ADC channel, for chips whose descriptor records no analog pads.
    AdcChannel { peripheral: String, channel: u8 },
}

impl SignalPath {
    /// Parse a manifest path. `None` means "not a board path" — an ordinary
    /// signal-store key, which the runner routes between models untouched.
    pub fn parse(path: &str) -> Option<Self> {
        if let Some(pad) = path.strip_prefix("board.gpio_output.") {
            return (!pad.is_empty()).then(|| Self::GpioDirection {
                pad: pad.to_string(),
            });
        }
        if let Some(pad) = path.strip_prefix("board.gpio_in.") {
            return (!pad.is_empty()).then(|| Self::GpioInput {
                pad: pad.to_string(),
            });
        }
        if let Some(pad) = path.strip_prefix("board.gpio.") {
            return (!pad.is_empty()).then(|| Self::GpioOutput {
                pad: pad.to_string(),
            });
        }
        if let Some(rest) = path.strip_prefix("board.analog.") {
            let pad = rest.strip_suffix("_volts")?;
            return (!pad.is_empty()).then(|| Self::AnalogPad {
                pad: pad.to_string(),
            });
        }
        if let Some(rest) = path.strip_prefix("adc.") {
            let rest = rest.strip_suffix("_volts")?;
            let (peripheral, channel) = rest.rsplit_once('.')?;
            if peripheral.is_empty() {
                return None;
            }
            let channel: u8 = channel.parse().ok()?;
            return Some(Self::AdcChannel {
                peripheral: peripheral.to_string(),
                channel,
            });
        }
        None
    }

    /// Can a model READ this path (machine → store)?
    pub fn is_readable(&self) -> bool {
        matches!(
            self,
            Self::GpioOutput { .. } | Self::GpioDirection { .. } | Self::GpioInput { .. }
        )
    }

    /// Can a model WRITE this path (store → machine)?
    pub fn is_writable(&self) -> bool {
        matches!(
            self,
            Self::GpioInput { .. } | Self::AnalogPad { .. } | Self::AdcChannel { .. }
        )
    }
}

/// Why one routed path could not be honoured.
///
/// Reported rather than swallowed: a co-simulation whose pin never reached the
/// firmware still runs to completion and still prints a verdict, so a silent
/// drop here is a run that proves nothing while claiming to.
///
/// No `Eq`: [`RoutingError::TypeMismatch`] carries the offending
/// [`CosimSignalValue`], whose `F64` arm makes equality partial.
#[derive(Debug, Clone, PartialEq)]
pub enum RoutingError {
    /// The pad label does not resolve on this chip.
    UnknownPad { path: String, pad: String },
    /// The pad resolves but its owning GPIO block is not on the bus.
    UnknownGpio { path: String, pad: String },
    /// The GPIO model that owns the pad cannot report the pad's direction, so
    /// `board.gpio_output.<pad>` has no honest answer on this chip.
    NoDirection { path: String, pad: String },
    /// A model output was routed to a path only the machine can drive.
    NotWritable { path: String },
    /// A model input was sourced from a path the machine cannot be read from.
    NotReadable { path: String },
    /// The chip descriptor's `analog_pins:` names no ADC input for this pad.
    /// Use the explicit `adc.<peripheral>.<channel>_volts` form instead.
    NoAdcChannel { path: String, pad: String },
    /// No ADC on the bus accepted the channel.
    AdcUnavailable { path: String, channel: u8 },
    /// The path names a peripheral this bus does not have.
    UnknownPeripheral { path: String, peripheral: String },
    /// The path names a peripheral that is not an ADC LabWired can drive.
    NotAnAdc { path: String, peripheral: String },
    /// The ADC has no such input. Its channels are `0..channels`.
    NoSuchAdcChannel {
        path: String,
        peripheral: String,
        channel: u8,
        channels: u8,
    },
    /// The owning GPIO block refused an externally driven level.
    DriveRejected { path: String },
    /// The store held a value of the wrong shape for this path.
    TypeMismatch {
        path: String,
        value: CosimSignalValue,
    },
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPad { path, pad } => {
                write!(
                    f,
                    "co-sim path '{path}': pad '{pad}' does not resolve on this chip"
                )
            }
            Self::UnknownGpio { path, pad } => write!(
                f,
                "co-sim path '{path}': pad '{pad}' resolves but its GPIO block is not on the bus"
            ),
            Self::NoDirection { path, pad } => write!(
                f,
                "co-sim path '{path}': the GPIO model that owns pad '{pad}' does not report pin \
                 direction, so whether the firmware drives the pad is unknown on this chip"
            ),
            Self::NotWritable { path } => write!(
                f,
                "co-sim path '{path}' is a model INPUT only; a model output cannot drive it \
                 (use board.gpio_in.<pad> to drive a pin)"
            ),
            Self::NotReadable { path } => write!(
                f,
                "co-sim path '{path}' is a model OUTPUT only; it cannot be read into a model input"
            ),
            Self::NoAdcChannel { path, pad } => write!(
                f,
                "co-sim path '{path}': the chip descriptor names no ADC input for pad '{pad}' \
                 (no `analog_pins:` entry); route adc.<peripheral>.<channel>_volts instead"
            ),
            Self::AdcUnavailable { path, channel } => write!(
                f,
                "co-sim path '{path}': no ADC on the bus accepted channel {channel}"
            ),
            Self::UnknownPeripheral { path, peripheral } => write!(
                f,
                "co-sim path '{path}': there is no peripheral named '{peripheral}' on this bus"
            ),
            Self::NotAnAdc { path, peripheral } => write!(
                f,
                "co-sim path '{path}': peripheral '{peripheral}' is not an ADC, so it takes no \
                 analog level"
            ),
            Self::NoSuchAdcChannel {
                path,
                peripheral,
                channel,
                channels,
            } => match channels.checked_sub(1) {
                Some(last) => write!(
                    f,
                    "co-sim path '{path}': ADC '{peripheral}' has channels 0..={last}; \
                     there is no channel {channel}"
                ),
                None => write!(
                    f,
                    "co-sim path '{path}': ADC '{peripheral}' has no analog input channels"
                ),
            },
            Self::DriveRejected { path } => write!(
                f,
                "co-sim path '{path}': the owning GPIO block refused an external level"
            ),
            Self::TypeMismatch { path, value } => write!(
                f,
                "co-sim path '{path}': cannot route signal value {value:?} (expected a number \
                 for an analog path, a boolean for a pin path)"
            ),
        }
    }
}

impl std::error::Error for RoutingError {}

/// A resolved machine → store source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadBinding {
    /// Owning peripheral index + bit, read through `Peripheral::read_gpio_output`.
    PadOutput { peripheral: usize, bit: u8 },
    /// Owning peripheral index + pad number, read through
    /// `Peripheral::gpio_routing`: true exactly when the mode is `Output`.
    PadDirection { peripheral: usize, pad: u8 },
    /// Owning peripheral index + bit, read through `Peripheral::read_gpio_input`.
    PadInput { peripheral: usize, bit: u8 },
}

/// A resolved store → machine sink.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WriteBinding {
    /// `(IDR address, bit)` driven through `SystemBus::drive_input_bit` — the
    /// same seam a `board_io` button and a sensor status line come through.
    PadInput { addr: u64, bit: u8 },
    /// An ADC channel seeded through `SystemBus::seed_adc_channel`, on the
    /// peripheral the manifest or the chip descriptor named.
    AdcChannel { connection: String, channel: u8 },
}

/// Routes manifest signal paths between a running machine and the co-sim
/// signal store.
#[derive(Debug, Default, Clone)]
pub struct SignalRouter {
    reads: Vec<(String, ReadBinding)>,
    writes: Vec<(String, WriteBinding)>,
}

impl SignalRouter {
    /// Resolve every board path named by `configs` against `bus`.
    ///
    /// Returns the router plus every path that could not be resolved. Binding
    /// is deterministic: the manifest maps are `HashMap`s, so paths are
    /// collected through a `BTreeSet` and the resulting order is the sorted
    /// path order, not a hash order that changes between processes.
    pub fn bind(configs: &[CosimModelConfig], bus: &SystemBus) -> (Self, Vec<RoutingError>) {
        let mut sources: BTreeSet<&str> = BTreeSet::new();
        let mut sinks: BTreeSet<&str> = BTreeSet::new();
        for config in configs {
            sources.extend(config.inputs.values().map(String::as_str));
            sinks.extend(config.outputs.values().map(String::as_str));
        }

        let mut router = Self::default();
        let mut errors = Vec::new();

        for path in &sources {
            let Some(parsed) = SignalPath::parse(path) else {
                continue; // Plain store path — the runner owns it.
            };
            if !parsed.is_readable() {
                errors.push(RoutingError::NotReadable {
                    path: (*path).to_string(),
                });
                continue;
            }
            match Self::resolve_read(&parsed, bus, path) {
                Ok(binding) => router.reads.push(((*path).to_string(), binding)),
                Err(err) => errors.push(err),
            }
        }

        for path in &sinks {
            let Some(parsed) = SignalPath::parse(path) else {
                continue;
            };
            if !parsed.is_writable() {
                errors.push(RoutingError::NotWritable {
                    path: (*path).to_string(),
                });
                continue;
            }
            match Self::resolve_write(&parsed, bus, path) {
                Ok(binding) => router.writes.push(((*path).to_string(), binding)),
                Err(err) => errors.push(err),
            }
        }

        (router, errors)
    }

    fn resolve_read(
        parsed: &SignalPath,
        bus: &SystemBus,
        path: &str,
    ) -> Result<ReadBinding, RoutingError> {
        match parsed {
            SignalPath::GpioOutput { pad } => {
                let (peripheral, bit) = resolve_pad_owner_odr(bus, pad, path)?;
                Ok(ReadBinding::PadOutput { peripheral, bit })
            }
            SignalPath::GpioDirection { pad } => {
                let (addr, bit) = SystemBus::resolve_pin_odr(bus, pad).ok_or_else(|| {
                    RoutingError::UnknownPad {
                        path: path.to_string(),
                        pad: pad.clone(),
                    }
                })?;
                let peripheral =
                    bus.find_peripheral_index(addr)
                        .ok_or_else(|| RoutingError::UnknownGpio {
                            path: path.to_string(),
                            pad: pad.clone(),
                        })?;
                let owner = &bus.peripherals[peripheral];
                let pad_number = pad_number(owner.dev.as_ref(), addr - owner.base, bit);
                direction_binding(owner.dev.as_ref(), peripheral, pad_number, path, pad)
            }
            SignalPath::GpioInput { pad } => {
                let (peripheral, bit) = resolve_pad_owner_idr(bus, pad, path)?;
                Ok(ReadBinding::PadInput { peripheral, bit })
            }
            // `is_readable` already rejected the analog forms.
            SignalPath::AnalogPad { .. } | SignalPath::AdcChannel { .. } => {
                Err(RoutingError::NotReadable {
                    path: path.to_string(),
                })
            }
        }
    }

    fn resolve_write(
        parsed: &SignalPath,
        bus: &SystemBus,
        path: &str,
    ) -> Result<WriteBinding, RoutingError> {
        match parsed {
            SignalPath::GpioInput { pad } => {
                let (addr, bit) = SystemBus::resolve_pin_idr(bus, pad).ok_or_else(|| {
                    RoutingError::UnknownPad {
                        path: path.to_string(),
                        pad: pad.clone(),
                    }
                })?;
                Ok(WriteBinding::PadInput { addr, bit })
            }
            SignalPath::AnalogPad { pad } => {
                // Descriptor data only. The pad → channel assignment differs
                // between families (PA0 is ADC1_IN0 on an F401, ADC1_IN5 on an
                // L476, ADC1_IN1 on a G474), so a chip that records nothing
                // gets an error, never a guess that reads the wrong channel.
                let (connection, channel) = bus
                    .analog_pin_map
                    .get(&pad.to_ascii_uppercase())
                    .cloned()
                    .ok_or_else(|| RoutingError::NoAdcChannel {
                        path: path.to_string(),
                        pad: pad.clone(),
                    })?;
                check_adc_channel(bus, path, &connection, channel)?;
                Ok(WriteBinding::AdcChannel {
                    connection,
                    channel,
                })
            }
            SignalPath::AdcChannel {
                peripheral,
                channel,
            } => {
                check_adc_channel(bus, path, peripheral, *channel)?;
                Ok(WriteBinding::AdcChannel {
                    connection: peripheral.clone(),
                    channel: *channel,
                })
            }
            SignalPath::GpioOutput { .. } | SignalPath::GpioDirection { .. } => {
                Err(RoutingError::NotWritable {
                    path: path.to_string(),
                })
            }
        }
    }

    /// No board path is routed in either direction.
    pub fn is_empty(&self) -> bool {
        self.reads.is_empty() && self.writes.is_empty()
    }

    /// The store paths this router fills from the machine, in bind order.
    pub fn read_paths(&self) -> impl Iterator<Item = &str> {
        self.reads.iter().map(|(path, _)| path.as_str())
    }

    /// Machine → store. Called immediately before stepping the models, so a
    /// model sees the pin levels as of the boundary it is stepped to.
    ///
    /// A pad the GPIO model cannot answer for (an alternate-function pin on a
    /// block that tracks direction, say) leaves its path ABSENT rather than
    /// inserting `false`: the runner then passes no value for that input, and
    /// the model keeps whatever it had, instead of being told the pin is low.
    pub fn sample(&self, bus: &SystemBus, signals: &mut CosimSignals) {
        for (path, binding) in &self.reads {
            let level = match *binding {
                ReadBinding::PadOutput { peripheral, bit } => bus
                    .peripherals
                    .get(peripheral)
                    .and_then(|p| p.dev.read_gpio_output(bit)),
                ReadBinding::PadInput { peripheral, bit } => bus
                    .peripherals
                    .get(peripheral)
                    .and_then(|p| p.dev.read_gpio_input(bit)),
                ReadBinding::PadDirection { peripheral, pad } => bus
                    .peripherals
                    .get(peripheral)
                    .and_then(|p| p.dev.gpio_routing(pad))
                    .map(|routing| routing.mode == GpioMode::Output),
            };
            if let Some(level) = level {
                signals.insert(path.clone(), CosimSignalValue::Bool(level));
            }
        }
    }

    /// Store → machine. Called immediately after stepping the models.
    pub fn apply(&self, bus: &mut SystemBus, signals: &CosimSignals) -> Vec<RoutingError> {
        let mut errors = Vec::new();
        for (path, binding) in &self.writes {
            let Some(value) = signals.get(path) else {
                continue; // No model produced this output on this step.
            };
            match binding {
                WriteBinding::PadInput { addr, bit } => {
                    let Some(level) = signal_as_bool(value) else {
                        errors.push(RoutingError::TypeMismatch {
                            path: path.clone(),
                            value: value.clone(),
                        });
                        continue;
                    };
                    if !bus.drive_input_bit(*addr, *bit, level) {
                        errors.push(RoutingError::DriveRejected { path: path.clone() });
                    }
                }
                WriteBinding::AdcChannel {
                    connection,
                    channel,
                } => {
                    let Some(volts) = signal_as_f64(value) else {
                        errors.push(RoutingError::TypeMismatch {
                            path: path.clone(),
                            value: value.clone(),
                        });
                        continue;
                    };
                    let millivolts = volts_to_millivolts(volts);
                    if !bus.seed_adc_channel(connection, *channel, millivolts) {
                        errors.push(RoutingError::AdcUnavailable {
                            path: path.clone(),
                            channel: *channel,
                        });
                    }
                }
            }
        }
        errors
    }
}

/// Refuse an ADC route the converter cannot take: a peripheral the bus does
/// not have, one that is not an ADC, or a channel that ADC does not have.
///
/// Checked at bind time against the named peripheral itself. The apply path
/// cannot catch these: an ADC model silently drops a channel it does not
/// have, so the route would step every period and land nowhere.
fn check_adc_channel(
    bus: &SystemBus,
    path: &str,
    peripheral: &str,
    channel: u8,
) -> Result<(), RoutingError> {
    let index = bus
        .find_peripheral_index_by_name(peripheral)
        .ok_or_else(|| RoutingError::UnknownPeripheral {
            path: path.to_string(),
            peripheral: peripheral.to_string(),
        })?;
    let channels = bus.peripherals[index]
        .dev
        .adc_channel_count()
        .ok_or_else(|| RoutingError::NotAnAdc {
            path: path.to_string(),
            peripheral: peripheral.to_string(),
        })?;
    if channel >= channels {
        return Err(RoutingError::NoSuchAdcChannel {
            path: path.to_string(),
            peripheral: peripheral.to_string(),
            channel,
            channels,
        });
    }
    Ok(())
}

/// Clamp a routed node voltage to the millivolt level an ADC model takes.
///
/// The ADC owns millivolts → counts (see [`ADC_VREF_VOLTS`]), so this is only
/// the unit change plus the clamp a real pin imposes: a SPICE node can ring
/// below ground or above the rail, and a pin cannot present either to the
/// converter. Rounds half away from zero, which is deterministic.
///
/// `NaN` is the one input with no honest answer — it is not high, low, or
/// anywhere between — so it reads as 0 rather than being fed to a comparison
/// that would quietly answer `false` in both directions.
pub fn volts_to_millivolts(volts: f64) -> u16 {
    if volts.is_nan() {
        return 0;
    }
    (volts.clamp(0.0, ADC_VREF_VOLTS) * 1000.0).round() as u16
}

fn signal_as_bool(value: &CosimSignalValue) -> Option<bool> {
    match value {
        CosimSignalValue::Bool(value) => Some(*value),
        CosimSignalValue::I64(value) => Some(*value != 0),
        CosimSignalValue::F64(value) => Some(*value != 0.0),
        CosimSignalValue::Text(_) => None,
    }
}

fn signal_as_f64(value: &CosimSignalValue) -> Option<f64> {
    match value {
        CosimSignalValue::F64(value) => Some(*value),
        CosimSignalValue::I64(value) => Some(*value as f64),
        // A boolean on an analog path is a pin level, not a voltage: it would
        // silently become 0 V / 0.001 V. Reject it so the manifest says what
        // it means.
        CosimSignalValue::Bool(_) | CosimSignalValue::Text(_) => None,
    }
}

/// Resolve a pad label to `(owning peripheral index, bit)` through its OUTPUT
/// register, through the same pin resolution MMIO routing and every other
/// pad-addressed feature already use: the chip's declared `pins:` map first,
/// then the STM32/Nordic label parse, then the ESP32 GPIO forms.
fn resolve_pad_owner_odr(
    bus: &SystemBus,
    pad: &str,
    path: &str,
) -> Result<(usize, u8), RoutingError> {
    let (addr, bit) =
        SystemBus::resolve_pin_odr(bus, pad).ok_or_else(|| RoutingError::UnknownPad {
            path: path.to_string(),
            pad: pad.to_string(),
        })?;
    let idx = bus
        .find_peripheral_index(addr)
        .ok_or_else(|| RoutingError::UnknownGpio {
            path: path.to_string(),
            pad: pad.to_string(),
        })?;
    Ok((idx, bit))
}

/// Register offset of the ESP32 family's second output bank (`GPIO_OUT1`),
/// where [`SystemBus::resolve_pin_odr`] places pads 32 and up as bank-relative
/// bits.
const ESP32_GPIO_OUT1_OFFSET: u64 = 0x10;

/// The pad NUMBER an `(ODR address, bit)` resolution names.
///
/// A GPIO port's bit is its pad. The ESP32 family's single `gpio` block is the
/// exception: its resolver splits pads 32.. into `GPIO_OUT1` as bank-relative
/// bits, while `gpio_routing` numbers pads absolutely. Handing it the bank bit
/// would report GPIO1's direction for GPIO33.
fn pad_number(owner: &dyn Peripheral, register_offset: u64, bit: u8) -> u8 {
    let is_port = owner.as_any().is_some_and(|any| any.is::<GpioPort>());
    if !is_port && register_offset == ESP32_GPIO_OUT1_OFFSET {
        bit.saturating_add(32)
    } else {
        bit
    }
}

/// Bind `board.gpio_output.<pad>` to its owner, or refuse a GPIO model that
/// cannot say which way the pad points.
///
/// Checked here, once, because the alternative is worse than an error: a pad
/// whose direction reads as absent would never close the circuit's driver, and
/// one read as `false` would silently disconnect the firmware from the net for
/// the whole run.
fn direction_binding(
    owner: &dyn Peripheral,
    peripheral: usize,
    pad_number: u8,
    path: &str,
    pad: &str,
) -> Result<ReadBinding, RoutingError> {
    if owner.gpio_routing(pad_number).is_none() {
        return Err(RoutingError::NoDirection {
            path: path.to_string(),
            pad: pad.to_string(),
        });
    }
    Ok(ReadBinding::PadDirection {
        peripheral,
        pad: pad_number,
    })
}

/// [`resolve_pad_owner_odr`]'s input-register twin.
fn resolve_pad_owner_idr(
    bus: &SystemBus,
    pad: &str,
    path: &str,
) -> Result<(usize, u8), RoutingError> {
    let (addr, bit) =
        SystemBus::resolve_pin_idr(bus, pad).ok_or_else(|| RoutingError::UnknownPad {
            path: path.to_string(),
            pad: pad.to_string(),
        })?;
    let idx = bus
        .find_peripheral_index(addr)
        .ok_or_else(|| RoutingError::UnknownGpio {
            path: path.to_string(),
            pad: pad.to_string(),
        })?;
    Ok((idx, bit))
}

/// A co-simulation bound to one machine: the runner, the pin routing, the
/// signal store, and the cycle ↔ nanosecond time base that keeps them in
/// lockstep.
///
/// The run loop that owns the machine only has to ask two things — how far may
/// I run ([`Self::cycles_until_boundary`]), and here is where I got to
/// ([`Self::advance_to`]) — so the lockstep rule lives in one place rather than
/// being re-derived by every caller.
pub struct CosimSession {
    runner: CosimRunner,
    router: SignalRouter,
    signals: CosimSignals,
    cpu_hz: u64,
    /// The finest model period: the granularity the machine is chopped at.
    step_ns: u64,
    /// Simulated time of the next boundary the machine must not run past.
    next_boundary_ns: u64,
    /// True when `cpu_hz` came from [`FALLBACK_CPU_HZ`] rather than the bus.
    fallback_clock: bool,
    /// Paths the manifest named that could not be resolved against this bus.
    binding_errors: Vec<RoutingError>,
    /// Every apply-time routing failure [`Self::advance`] has already handed
    /// back, by message, so each distinct one is reported once per run.
    reported_errors: BTreeSet<String>,
}

/// What one lockstep advance did: the machine's report, and what happened at
/// the model boundaries it reached.
#[derive(Debug, Clone)]
pub struct CosimAdvance {
    /// The machine's own accounting. For [`CosimSession::advance_budget`] it
    /// sums every chunk, and `stop` is the stop that ended the last one.
    pub report: AdvanceReport,
    /// Every model step taken at a boundary this advance reached, in order.
    /// Empty when no boundary was reached.
    pub routed: Vec<CosimRoutedModelStep>,
    /// Apply-time routing failures seen for the FIRST time. A failure an
    /// earlier advance already returned is not repeated: at a 100 us step a
    /// broken ADC route would otherwise be one identical line per period.
    pub new_routing_errors: Vec<RoutingError>,
}

impl From<AdvanceReport> for CosimAdvance {
    /// An advance that reached no model: the shape a caller with no session
    /// hands on, so one code path can handle both.
    fn from(report: AdvanceReport) -> Self {
        Self {
            report,
            routed: Vec::new(),
            new_routing_errors: Vec::new(),
        }
    }
}

/// Why a lockstep advance failed.
#[derive(Debug)]
pub enum CosimAdvanceError {
    /// The machine itself failed. No model was stepped for the failing chunk,
    /// and — as with [`Machine::advance`] — the CPU may already have retired
    /// part of its batch.
    Machine(SimulationError),
    /// The machine advanced, then a model failed at the boundary it reached.
    /// `report` accounts for the machine work that did commit.
    Model {
        report: AdvanceReport,
        error: SimulationError,
    },
}

impl std::fmt::Display for CosimAdvanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Machine(error) => write!(f, "{error}"),
            Self::Model { error, .. } => write!(f, "co-sim model step failed: {error}"),
        }
    }
}

impl std::error::Error for CosimAdvanceError {}

/// `total` followed by `next`: counters add, the stop is `next`'s.
fn chain_reports(total: AdvanceReport, next: AdvanceReport) -> AdvanceReport {
    AdvanceReport::new(
        next.stop,
        total.fuel_consumed + next.fuel_consumed,
        total.primary_steps + next.primary_steps,
        total.secondary_steps + next.secondary_steps,
        total.elapsed_cycles + next.elapsed_cycles,
        total.idle_cycles + next.idle_cycles,
        total.cpu_batches + next.cpu_batches,
    )
}

impl CosimSession {
    /// Build a session for `configs`, resolving relative `model:` paths against
    /// `base_dir` and every board path against `bus`.
    ///
    /// Returns `Ok(None)` when `configs` is empty — the caller then does
    /// nothing at all, which is what keeps a manifest without `cosim_models`
    /// byte-identical to a build without this feature.
    ///
    /// Unresolvable paths are NOT an error here: they are carried on the
    /// session as [`Self::binding_errors`] so the caller decides what an
    /// unroutable pin means for its run.
    pub fn new(
        configs: &[CosimModelConfig],
        base_dir: &Path,
        bus: &SystemBus,
    ) -> SimResult<Option<Self>> {
        if configs.is_empty() {
            return Ok(None);
        }
        let runner = CosimRunner::from_configs_with_base(configs, base_dir)?;
        let (router, binding_errors) = SignalRouter::bind(configs, bus);
        let fallback_clock = bus.cpu_hz == 0;
        let cpu_hz = if fallback_clock {
            FALLBACK_CPU_HZ
        } else {
            bus.cpu_hz
        };
        // The finest declared period is the lockstep granularity: stepping at
        // anything coarser would let the machine run past a faster model's
        // boundary before that model saw the pin levels that produced it.
        let step_ns = configs
            .iter()
            .map(|config| config.step_ns)
            .filter(|step| *step > 0)
            .min()
            .unwrap_or(1);
        Ok(Some(Self {
            runner,
            router,
            signals: CosimSignals::new(),
            cpu_hz,
            step_ns,
            next_boundary_ns: step_ns,
            fallback_clock,
            binding_errors,
            reported_errors: BTreeSet::new(),
        }))
    }

    /// Every manifest path that did not resolve against the machine's bus.
    pub fn binding_errors(&self) -> &[RoutingError] {
        &self.binding_errors
    }

    /// How many models this session steps.
    pub fn model_count(&self) -> usize {
        self.runner.model_count()
    }

    /// The runner's analog waveform ring. Every `adapter: analog` model writes
    /// its routed outputs and `config.trace` channels here as it steps; publish
    /// it with [`Machine::attach_analog_trace`](crate::Machine::attach_analog_trace)
    /// so `Machine::analog_trace_snapshot` and `--analog-trace` read the samples
    /// this session produces. A session with no analog model hands back a ring
    /// with no channels.
    pub fn analog_trace_registry(&self) -> crate::analog::AnalogTraceRegistry {
        self.runner.analog_trace_registry()
    }

    /// The effective core clock, in Hz.
    pub fn cpu_hz(&self) -> u64 {
        self.cpu_hz
    }

    /// Whether [`FALLBACK_CPU_HZ`] stood in for a bus that reported no clock.
    pub fn uses_fallback_clock(&self) -> bool {
        self.fallback_clock
    }

    /// The lockstep granularity, in nanoseconds (the finest model `step_ns`).
    pub fn step_ns(&self) -> u64 {
        self.step_ns
    }

    /// The signal store, for inspection after a step.
    pub fn signals(&self) -> &CosimSignals {
        &self.signals
    }

    /// The machine-sourced values the models were last handed, in bind order —
    /// the readout that answers "what did the model actually see on that pin?"
    /// without re-reading the bus. Empty before the first boundary.
    pub fn sampled_inputs(&self) -> Vec<(&str, &CosimSignalValue)> {
        self.router
            .read_paths()
            .filter_map(|path| self.signals.get_key_value(path))
            .map(|(path, value)| (path.as_str(), value))
            .collect()
    }

    /// How many more simulated cycles the machine may run before it would pass
    /// the next co-simulation boundary. Never zero, so a run always makes
    /// progress.
    pub fn cycles_until_boundary(&self, total_cycles: u64) -> u64 {
        let boundary = ns_to_cycles(self.next_boundary_ns, self.cpu_hz);
        boundary.saturating_sub(total_cycles).max(1)
    }

    /// Step every model whose boundary `total_cycles` has reached, sampling the
    /// machine's pins first and applying routed outputs back afterwards.
    ///
    /// Returns the routed steps (empty when no boundary was reached) and every
    /// routing error the apply pass hit.
    pub fn advance_to(
        &mut self,
        total_cycles: u64,
        bus: &mut SystemBus,
    ) -> SimResult<(Vec<CosimRoutedModelStep>, Vec<RoutingError>)> {
        let time_ns = cycles_to_ns(total_cycles, self.cpu_hz);
        if time_ns < self.next_boundary_ns {
            return Ok((Vec::new(), Vec::new()));
        }
        self.router.sample(bus, &mut self.signals);
        let routed = self
            .runner
            .step_until_with_signals(time_ns, &mut self.signals)?;
        let errors = self.router.apply(bus, &self.signals);
        // Land on the first boundary strictly after the time just reached, so
        // a long advance that crossed several periods does not replay them.
        self.next_boundary_ns = (time_ns / self.step_ns + 1).saturating_mul(self.step_ns);
        Ok((routed, errors))
    }

    /// One lockstep advance of `machine`: `request`, with its simulated-cycle
    /// budget capped at the next model boundary, then every model due at the
    /// point the machine reached.
    ///
    /// The cap is what keeps a model from being handed pin levels from its
    /// future. A fuel budget cannot express it — fuel counts scheduling quanta
    /// and idle skips, and one idle skip can cross milliseconds of device time
    /// — so the cap goes on `simulated_cycles`, the clock a boundary is defined
    /// in. A request that already carries a tighter cycle budget keeps it.
    ///
    /// Models are NOT stepped when the machine stopped for good or did not
    /// move: a firmware exit has no next instruction to hand a model's answer
    /// to, and a machine that made no progress reached no new time.
    ///
    /// This is the one place a machine is stepped in lockstep with its models.
    /// `labwired test` calls it once per run-loop iteration; a caller that
    /// wants a whole budget spent calls [`Self::advance_budget`], which is this
    /// in a loop.
    pub fn advance<C: Cpu>(
        &mut self,
        machine: &mut Machine<C>,
        request: AdvanceRequest,
    ) -> Result<CosimAdvance, CosimAdvanceError> {
        let to_boundary = self.cycles_until_boundary(machine.total_cycles);
        let cycle_limit = request
            .limits()
            .simulated_cycles
            .map_or(to_boundary, |limit| limit.min(to_boundary));
        let report = machine
            .advance(request.with_cycle_limit(cycle_limit))
            .map_err(CosimAdvanceError::Machine)?;

        let stopped_for_good = matches!(report.stop, AdvanceStop::FirmwareExit { .. });
        let made_no_progress = report.primary_steps == 0 && report.idle_cycles == 0;
        if stopped_for_good || made_no_progress {
            return Ok(report.into());
        }

        let (routed, errors) = self
            .advance_to(machine.total_cycles, &mut machine.bus)
            .map_err(|error| CosimAdvanceError::Model { report, error })?;
        let new_routing_errors = errors
            .into_iter()
            .filter(|err| self.reported_errors.insert(err.to_string()))
            .collect();
        Ok(CosimAdvance {
            report,
            routed,
            new_routing_errors,
        })
    }

    /// Spend `request`'s whole fuel and cycle budget in lockstep: repeated
    /// [`Self::advance`] calls, each stopped on a model boundary, until the
    /// budget is spent or the machine stops for any other reason (breakpoint,
    /// no progress, firmware exit).
    ///
    /// The returned report is what one [`Machine::advance`] with the same
    /// request would account — fuel, steps and cycles summed over the chunks —
    /// which is what lets a caller like the browser's `step_batch` keep its
    /// contract whether or not a session is attached. A request with neither
    /// budget runs until the machine stops on its own, exactly as
    /// [`Machine::advance`] would.
    pub fn advance_budget<C: Cpu>(
        &mut self,
        machine: &mut Machine<C>,
        request: AdvanceRequest,
    ) -> Result<CosimAdvance, CosimAdvanceError> {
        let limits = request.limits();
        let mut total: Option<CosimAdvance> = None;
        loop {
            let (fuel_spent, cycles_spent) = total.as_ref().map_or((0, 0), |advance| {
                (advance.report.fuel_consumed, advance.report.elapsed_cycles)
            });
            let mut chunk =
                request.with_fuel_limit(limits.fuel.map(|fuel| fuel.saturating_sub(fuel_spent)));
            let cycles_left = limits
                .simulated_cycles
                .map(|cycles| cycles.saturating_sub(cycles_spent));
            if let Some(cycles) = cycles_left {
                chunk = chunk.with_cycle_limit(cycles);
            }

            let step = match self.advance(machine, chunk) {
                Ok(step) => step,
                Err(CosimAdvanceError::Model { report, error }) => {
                    let report = match &total {
                        Some(advance) => chain_reports(advance.report, report),
                        None => report,
                    };
                    return Err(CosimAdvanceError::Model { report, error });
                }
                Err(machine_error) => return Err(machine_error),
            };

            let chunk_report = step.report;
            total = Some(match total {
                None => step,
                Some(mut advance) => {
                    advance.report = chain_reports(advance.report, step.report);
                    advance.routed.extend(step.routed);
                    advance.new_routing_errors.extend(step.new_routing_errors);
                    advance
                }
            });

            let budget_spent = match chunk_report.stop {
                // The chunk's fuel was everything that was left.
                AdvanceStop::FuelLimit => true,
                // Either the request's own cycle budget, or just a boundary.
                AdvanceStop::CycleLimit => {
                    cycles_left.is_some_and(|left| chunk_report.elapsed_cycles >= left)
                }
                AdvanceStop::Breakpoint(_)
                | AdvanceStop::NoProgress
                | AdvanceStop::FirmwareExit { .. } => true,
            };
            let made_no_progress = chunk_report.primary_steps == 0 && chunk_report.idle_cycles == 0;
            if budget_spent || made_no_progress {
                return Ok(total.expect("at least one chunk ran"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use labwired_config::{CosimAdapter, CosimModelConfig};
    use std::collections::HashMap;

    fn mock_model(
        id: &str,
        step_ns: u64,
        inputs: &[(&str, &str)],
        outputs: &[(&str, &str)],
        static_outputs: &[(&str, serde_yaml::Value)],
    ) -> CosimModelConfig {
        let mut config = HashMap::new();
        if !static_outputs.is_empty() {
            let mapping: serde_yaml::Mapping = static_outputs
                .iter()
                .map(|(k, v)| (serde_yaml::Value::String((*k).to_string()), v.clone()))
                .collect();
            config.insert("outputs".to_string(), serde_yaml::Value::Mapping(mapping));
        }
        CosimModelConfig {
            id: id.to_string(),
            adapter: CosimAdapter::Mock,
            model: None,
            step_ns,
            inputs: inputs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            outputs: outputs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            config,
        }
    }

    // ── Path grammar ────────────────────────────────────────────────────────

    #[test]
    fn parses_every_board_path_form() {
        assert_eq!(
            SignalPath::parse("board.gpio.pa5"),
            Some(SignalPath::GpioOutput {
                pad: "pa5".to_string()
            })
        );
        assert_eq!(
            SignalPath::parse("board.gpio_in.pc13"),
            Some(SignalPath::GpioInput {
                pad: "pc13".to_string()
            })
        );
        assert_eq!(
            SignalPath::parse("board.analog.pa0_volts"),
            Some(SignalPath::AnalogPad {
                pad: "pa0".to_string()
            })
        );
        assert_eq!(
            SignalPath::parse("adc.adc1.3_volts"),
            Some(SignalPath::AdcChannel {
                peripheral: "adc1".to_string(),
                channel: 3
            })
        );
    }

    #[test]
    fn parses_the_direction_path() {
        assert_eq!(
            SignalPath::parse("board.gpio_output.pa5"),
            Some(SignalPath::GpioDirection {
                pad: "pa5".to_string()
            })
        );
        assert_eq!(SignalPath::parse("board.gpio_output."), None);
        let direction = SignalPath::parse("board.gpio_output.p0.13").unwrap();
        assert_eq!(
            direction,
            SignalPath::GpioDirection {
                pad: "p0.13".to_string()
            }
        );
        assert!(direction.is_readable());
        assert!(!direction.is_writable());
    }

    /// A GPIO model with an output latch but no direction register. The
    /// direction path must refuse it when the session is built: answering
    /// `false` would disconnect the firmware from the circuit for the whole run
    /// and nothing would say so.
    #[derive(Debug, Default)]
    struct LatchOnlyGpio;

    impl Peripheral for LatchOnlyGpio {
        fn read(&self, _offset: u64) -> SimResult<u8> {
            Ok(0)
        }
        fn write(&mut self, _offset: u64, _value: u8) -> SimResult<()> {
            Ok(())
        }
        fn read_gpio_output(&self, pin: u8) -> Option<bool> {
            (pin < 32).then_some(false)
        }
    }

    #[test]
    fn a_gpio_model_without_a_direction_register_is_refused() {
        let err = direction_binding(&LatchOnlyGpio, 3, 5, "board.gpio_output.pa5", "pa5")
            .expect_err("no direction register, no binding");
        assert_eq!(
            err,
            RoutingError::NoDirection {
                path: "board.gpio_output.pa5".to_string(),
                pad: "pa5".to_string(),
            }
        );
        let message = err.to_string();
        assert!(message.contains("board.gpio_output.pa5"), "{message}");
        assert!(message.contains("direction"), "{message}");
    }

    /// Only the ESP32 family's second output bank shifts the bit. A SAM port's
    /// OUT register also sits at 0x10, and its bit IS the pad.
    #[test]
    fn the_second_esp32_output_bank_names_pads_from_32() {
        assert_eq!(pad_number(&LatchOnlyGpio, ESP32_GPIO_OUT1_OFFSET, 1), 33);
        assert_eq!(pad_number(&LatchOnlyGpio, 0x04, 1), 1);
        let port = GpioPort::new_with_layout(crate::peripherals::gpio::GpioRegisterLayout::SamPort);
        assert_eq!(pad_number(&port, 0x10, 1), 1);
    }

    #[test]
    fn a_direction_path_cannot_be_driven_by_a_model() {
        let bus = crate::bus::SystemBus::new();
        let configs = [mock_model(
            "m",
            1_000,
            &[],
            &[("out", "board.gpio_output.pa5")],
            &[("out", serde_yaml::Value::Bool(true))],
        )];
        let (_, errors) = SignalRouter::bind(&configs, &bus);
        assert_eq!(
            errors,
            vec![RoutingError::NotWritable {
                path: "board.gpio_output.pa5".to_string()
            }]
        );
    }

    /// A Nordic pad label carries a dot. Splitting the path on every dot would
    /// truncate `p0.13` to `p0`, which resolves to a DIFFERENT pin (bit 0 of
    /// the same port) rather than failing — so the grammar strips a fixed
    /// prefix and hands the whole remainder to pin resolution.
    #[test]
    fn keeps_dotted_nordic_pad_labels_intact() {
        assert_eq!(
            SignalPath::parse("board.gpio.p0.13"),
            Some(SignalPath::GpioOutput {
                pad: "p0.13".to_string()
            })
        );
    }

    #[test]
    fn plain_store_paths_are_not_board_paths() {
        // The `cosim-plant-demo` manifest shape must keep working untouched.
        assert_eq!(SignalPath::parse("control.enable"), None);
        assert_eq!(SignalPath::parse("plant.output.voltage"), None);
        assert_eq!(SignalPath::parse("observables.shaft_speed_rpm"), None);
        // Board-ish but not the grammar: no pad, no `_volts` suffix.
        assert_eq!(SignalPath::parse("board.gpio."), None);
        assert_eq!(SignalPath::parse("board.analog.pa0"), None);
        assert_eq!(SignalPath::parse("adc.adc1_volts"), None);
        assert_eq!(SignalPath::parse("adc.adc1.x_volts"), None);
    }

    #[test]
    fn directions_are_enforced_per_path_kind() {
        assert!(SignalPath::parse("board.gpio.pa5").unwrap().is_readable());
        assert!(!SignalPath::parse("board.gpio.pa5").unwrap().is_writable());
        assert!(SignalPath::parse("board.gpio_in.pc13")
            .unwrap()
            .is_writable());
        assert!(SignalPath::parse("board.analog.pa0_volts")
            .unwrap()
            .is_writable());
        assert!(!SignalPath::parse("board.analog.pa0_volts")
            .unwrap()
            .is_readable());
    }

    // ── Pad → ADC channel ───────────────────────────────────────────────────

    /// A bus built from no descriptor records no analog pads. The pad form must
    /// refuse, and say which form to use instead — reading channel 0 would be
    /// the wrong pin on most families and nothing would report it.
    #[test]
    fn a_pad_without_descriptor_analog_data_is_refused() {
        let bus = crate::bus::SystemBus::new();
        let configs = [mock_model(
            "m",
            1_000,
            &[],
            &[("v", "board.analog.pa0_volts")],
            &[("v", serde_yaml::Value::from(1.0))],
        )];
        let (router, errors) = SignalRouter::bind(&configs, &bus);
        assert!(router.is_empty());
        assert_eq!(
            errors,
            vec![RoutingError::NoAdcChannel {
                path: "board.analog.pa0_volts".to_string(),
                pad: "pa0".to_string(),
            }]
        );
        assert!(errors[0]
            .to_string()
            .contains("adc.<peripheral>.<channel>_volts"));
    }

    // ── Volts → the ADC's millivolts ────────────────────────────────────────

    /// Half of a 3.3 V reference. The ADC model turns 1650 mV into
    /// `1650 * 4095 / 3300` = 2047 counts at 12 bits — the same count
    /// `examples/ntc-thermistor-lab` asserts for its divider midpoint, which is
    /// the point of leaving the conversion in the ADC instead of redoing it
    /// here with a second rounding rule.
    #[test]
    fn converts_volts_to_the_adc_millivolt_unit() {
        assert_eq!(volts_to_millivolts(1.65), 1650);
        assert_eq!(volts_to_millivolts(0.0), 0);
        assert_eq!(volts_to_millivolts(3.3), 3300);
        assert_eq!(volts_to_millivolts(2.0805), 2081);
    }

    /// A SPICE node can ring below ground or above the rail; a pin cannot
    /// present that to the converter.
    #[test]
    fn clamps_voltages_a_pin_could_not_present() {
        assert_eq!(volts_to_millivolts(-1.0), 0);
        assert_eq!(volts_to_millivolts(12.0), 3300);
        assert_eq!(volts_to_millivolts(f64::NAN), 0);
        assert_eq!(volts_to_millivolts(f64::INFINITY), 3300);
    }

    // ── Time base ───────────────────────────────────────────────────────────

    #[test]
    fn converts_cycles_and_nanoseconds_at_the_core_clock() {
        // 84 MHz: 100 us is 8400 cycles.
        assert_eq!(cycles_to_ns(8_400, 84_000_000), 100_000);
        assert_eq!(ns_to_cycles(100_000, 84_000_000), 8_400);
        assert_eq!(cycles_to_ns(0, 84_000_000), 0);
    }

    /// Truncating here would put the boundary one cycle BELOW the time it
    /// represents, so `cycles_until_boundary` would return 1 forever and the
    /// run would crawl a cycle at a time without ever reaching the boundary.
    #[test]
    fn rounds_nanoseconds_up_to_a_reachable_cycle() {
        // 8 MHz: one cycle is 125 ns, so 100 ns must round UP to 1 cycle.
        assert_eq!(ns_to_cycles(100, 8_000_000), 1);
        assert_eq!(cycles_to_ns(1, 8_000_000), 125);
        assert!(cycles_to_ns(ns_to_cycles(100, 8_000_000), 8_000_000) >= 100);
    }

    #[test]
    fn a_zero_clock_cannot_divide_by_zero() {
        assert_eq!(cycles_to_ns(1_000, 0), 0);
        assert_eq!(ns_to_cycles(1_000, 0), 0);
    }

    // ── Binding ─────────────────────────────────────────────────────────────

    #[test]
    fn binding_rejects_a_model_output_routed_to_a_firmware_driven_pin() {
        let bus = crate::bus::SystemBus::new();
        let configs = [mock_model(
            "m",
            1_000,
            &[],
            &[("out", "board.gpio.pa5")],
            &[("out", serde_yaml::Value::Bool(true))],
        )];
        let (_, errors) = SignalRouter::bind(&configs, &bus);
        assert_eq!(
            errors,
            vec![RoutingError::NotWritable {
                path: "board.gpio.pa5".to_string()
            }]
        );
    }

    #[test]
    fn binding_rejects_a_model_input_sourced_from_an_analog_sink() {
        let bus = crate::bus::SystemBus::new();
        let configs = [mock_model(
            "m",
            1_000,
            &[("v", "board.analog.pa0_volts")],
            &[],
            &[],
        )];
        let (_, errors) = SignalRouter::bind(&configs, &bus);
        assert_eq!(
            errors,
            vec![RoutingError::NotReadable {
                path: "board.analog.pa0_volts".to_string()
            }]
        );
    }

    /// The generic plant demo routes only plain store paths. Binding must
    /// resolve nothing and complain about nothing, or adding pin routing would
    /// have broken every manifest that predates it.
    #[test]
    fn plain_store_manifests_bind_to_an_empty_router() {
        let bus = crate::bus::SystemBus::new();
        let configs = [mock_model(
            "plant",
            10_000,
            &[("enable", "control.enable")],
            &[("v_out", "plant.output.voltage")],
            &[("v_out", serde_yaml::Value::from(1.0))],
        )];
        let (router, errors) = SignalRouter::bind(&configs, &bus);
        assert!(router.is_empty());
        assert!(errors.is_empty(), "unexpected routing errors: {errors:?}");
    }

    #[test]
    fn an_empty_model_list_makes_no_session() {
        let bus = crate::bus::SystemBus::new();
        let session =
            CosimSession::new(&[], Path::new("."), &bus).expect("no models is not an error");
        assert!(session.is_none());
    }
}
