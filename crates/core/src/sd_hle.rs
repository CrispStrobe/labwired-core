// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT
//! labwired backend of `nrf-softdevice-hle`: runs an application built for a
//! Nordic SoftDevice with the SoftDevice ABSENT.
//!
//! Contract (crates/nrf-softdevice-hle/SPEC.md), as this backend meets it:
//!   * only the application region is loaded; the chip descriptor maps no
//!     flash below the application base. The `nordic_hole` memory region
//!     there (erased-flash 0xFF, see configs/chips/nrf51822.yaml) is data,
//!     never Nordic code: nothing is loaded into it;
//!   * VTOR is the application base (the MBR/SoftDevice forwarding);
//!   * at an instruction boundary where the core is in the SVCall handler
//!     (IPSR 11, PC = the application's SVCall vector) the frame's r0-r3 and
//!     the SVC number (byte at stacked PC - 2) go to [`SoftDevice::svc`], the
//!     result is written to the stacked r0 and the exception returns;
//!   * [`SoftDevice::poll`] runs every millisecond of emulated time.

use nrf_softdevice_hle::{Host, SoftDevice};

use crate::bus::SystemBus;
use crate::cpu::cortex_m::CortexM;
use crate::{Bus, Cpu, Machine, SimResult};

pub use nrf_softdevice_hle;

/// The SoftDevice attached to a machine.
pub struct SdHleSlot {
    pub sd: SoftDevice,
    /// Application base (vector table): 0x18000 on nRF51 + S110.
    pub app_base: u32,
    /// The application's SVCall handler (its vector 11), resolved at attach.
    pub svc_handler: u32,
    pub svc_count: u64,
    last_poll_us: u64,
}

struct BusHost<'a> {
    bus: &'a mut SystemBus,
    now_us: u64,
    app_base: u32,
}

impl Host for BusHost<'_> {
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> bool {
        if addr < self.app_base {
            return false;
        }
        for (i, b) in buf.iter_mut().enumerate() {
            match self.bus.read_u8(addr as u64 + i as u64) {
                Ok(v) => *b = v,
                Err(_) => return false,
            }
        }
        true
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> bool {
        if addr < self.app_base {
            return false;
        }
        if (0xE000_E000..0xE000_F000).contains(&addr) && data.len() == 4 {
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            return self.bus.write_u32(addr as u64, v).is_ok();
        }
        for (i, b) in data.iter().enumerate() {
            if self.bus.write_u8(addr as u64 + i as u64, *b).is_err() {
                // Flash is not CPU-writable on this bus; fall back to the image store.
                if !self.bus.flash.write_u8(addr as u64 + i as u64, *b) {
                    return false;
                }
            }
        }
        true
    }
    fn now_us(&mut self) -> u64 {
        self.now_us
    }
}

impl<C: Cpu> Machine<C> {
    /// Attach a SoftDevice HLE: set VTOR to the application base and resolve
    /// the SVCall vector. Call after the image is loaded, before running.
    pub fn attach_sd_hle(&mut self, sd: SoftDevice, app_base: u32) -> SimResult<()> {
        let handler = self.bus.read_u32(app_base as u64 + 0x2C)? & !1;
        let Some(cpu) = self
            .cpu
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<CortexM>())
        else {
            return Err(crate::SimulationError::Other(
                "the SoftDevice HLE needs a Cortex-M core".into(),
            ));
        };
        cpu.set_vtor(app_base);
        self.sd_hle = Some(Box::new(SdHleSlot {
            sd,
            app_base,
            svc_handler: handler,
            svc_count: 0,
            last_poll_us: 0,
        }));
        Ok(())
    }

    fn sd_hle_now_us(&self) -> u64 {
        let hz = self.bus.cpu_hz.max(1_000_000);
        self.total_cycles / (hz / 1_000_000)
    }

    /// Service a pending SVCall and poll. Called at every advance boundary;
    /// one `Option` test when no SoftDevice is attached.
    pub(crate) fn service_sd_hle(&mut self) -> SimResult<()> {
        let Some(mut slot) = self.sd_hle.take() else {
            return Ok(());
        };
        let now_us = self.sd_hle_now_us();
        let r = self.service_sd_hle_inner(&mut slot, now_us);
        self.sd_hle = Some(slot);
        r
    }

    fn service_sd_hle_inner(&mut self, slot: &mut SdHleSlot, now_us: u64) -> SimResult<()> {
        let Some(cpu) = self
            .cpu
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<CortexM>())
        else {
            return Ok(());
        };
        if cpu.active_exception == 11 && cpu.get_pc() & !1 == slot.svc_handler {
            let lr = cpu.lr;
            // The frame is on the stack the SVC was issued from (EXC_RETURN bit 2: PSP).
            let frame = if lr & 4 != 0 { cpu.psp } else { cpu.sp };
            let mut a = [0u32; 4];
            for (i, v) in a.iter_mut().enumerate() {
                *v = self.bus.read_u32(frame as u64 + 4 * i as u64)?;
            }
            let ret_pc = self.bus.read_u32(frame as u64 + 24)?;
            let num = self.bus.read_u8(ret_pc as u64 - 2)?;
            let r = {
                let mut h = BusHost {
                    bus: &mut self.bus,
                    now_us,
                    app_base: slot.app_base,
                };
                slot.sd.svc(num, a, &mut h)
            };
            self.bus.write_u32(frame as u64, r)?;
            slot.svc_count += 1;
            let cpu = self
                .cpu
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<CortexM>())
                .unwrap();
            cpu.hle_exception_return(&mut self.bus)?;
        }
        if now_us >= slot.last_poll_us + 1000 {
            slot.last_poll_us = now_us;
            let mut h = BusHost {
                bus: &mut self.bus,
                now_us,
                app_base: slot.app_base,
            };
            slot.sd.poll(&mut h);
        }
        Ok(())
    }
}

/// Build a micro:bit V1 / Calliope mini machine for an S110 application
/// region: the system YAML's bus, the Cortex-M system block, the app at
/// 0x18000, and a SoftDevice HLE. Returns the machine and the UART0 byte sink.
pub fn build_nrf51_s110(
    system_yaml: &std::path::Path,
    app: Vec<u8>,
    sd: SoftDevice,
) -> anyhow::Result<(Machine<CortexM>, std::sync::Arc<std::sync::Mutex<Vec<u8>>>)> {
    use labwired_config::{ChipDescriptor, SystemManifest};
    let mut manifest = SystemManifest::from_file(system_yaml)?;
    let chip_path = system_yaml
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(&manifest.chip);
    let chip = ChipDescriptor::from_file(&chip_path)?;
    manifest.chip = chip_path.to_string_lossy().into_owned();
    let mut bus = SystemBus::from_config(&chip, &manifest)?;
    let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    if let Some(p) = bus.peripherals.iter_mut().find(|p| p.name == "uart0") {
        if let Some(u) = p
            .dev
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<crate::peripherals::nrf52::uarte::Nrf52Uarte>())
        {
            u.set_sink(Some(sink.clone()), false);
        }
    }
    let (cpu, _nvic) = crate::system::cortex_m::configure_cortex_m(&mut bus);
    let mut m = Machine::new(cpu, bus);
    let mut img = crate::memory::ProgramImage::new(0x18000, crate::Arch::Arm);
    img.add_segment(0x18000, app);
    m.load_firmware(&img)?;
    m.attach_sd_hle(sd, 0x18000)?;
    // Buttons A (P0.17) and B (P0.26) released (pulled up on the board).
    for p in m.bus.peripherals.iter_mut().filter(|p| p.name == "gpio0") {
        p.dev.set_gpio_input(17, true);
        p.dev.set_gpio_input(26, true);
    }
    Ok((m, sink))
}
