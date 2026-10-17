// SPDX-License-Identifier: MIT
//! Opt-in SAMD51 main-array buffering and boundary commit. Other buses keep
//! their existing memory routing. No generic flash controller is substituted.
use super::*;
use crate::peripherals::sam::nvmctrl::{Operation, FLASH_SIZE};
use crate::SimulationError;

impl SystemBus {
    #[inline]
    pub(super) fn try_samd51_storage_store(
        &mut self,
        address: u64,
        width: u8,
        value: u32,
    ) -> Option<SimResult<()>> {
        let index = self.samd51_nvmctrl_idx?;
        self.flash.read_u8(address)?;
        if self.flash.base_addr != 0 || self.flash.data.len() != FLASH_SIZE as usize {
            return Some(Err(SimulationError::NotImplemented(
                "SAMD51 storage requires exact 512KiB zero-based backing".into(),
            )));
        }
        Some(
            self.peripherals[index]
                .dev
                .manual_flash_storage_mut()
                .unwrap()
                .store(address, width, value),
        )
    }

    /// One service at a retired CPU-window boundary. While pending, the bus
    /// pending-op predicate makes each such window end after one instruction.
    pub(crate) fn service_samd51_storage_boundary(&mut self) -> SimResult<()> {
        let Some(index) = self.samd51_nvmctrl_idx else {
            return Ok(());
        };
        let operation = self.peripherals[index]
            .dev
            .manual_flash_storage_mut()
            .unwrap()
            .boundary();
        let Some(operation) = operation else {
            return Ok(());
        };
        if self.flash.base_addr != 0 || self.flash.data.len() != FLASH_SIZE as usize {
            return Err(SimulationError::NotImplemented(
                "SAMD51 storage backing geometry differs".into(),
            ));
        }
        match operation {
            Operation::Clear => {}
            Operation::Erase { address } => {
                self.flash.data[address..address + 8192].fill(0xff);
                self.note_memory_write();
            }
            Operation::Program { address, bytes } => {
                for (old, new) in self.flash.data[address..address + 16].iter_mut().zip(bytes) {
                    *old &= new;
                }
                self.note_memory_write();
            }
        }
        self.peripherals[index]
            .dev
            .manual_flash_storage_mut()
            .unwrap()
            .finish();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::LinearMemory;
    use crate::peripherals::sam::nvmctrl::Samd51Nvmctrl;
    use crate::Bus;
    const NVM: u64 = 0x41004000;
    fn bus(optimized: bool) -> SystemBus {
        let mut bus = SystemBus::new();
        bus.flash = LinearMemory::new_erased(FLASH_SIZE as usize, 0);
        bus.config.optimized_bus_access = optimized;
        bus.add_peripheral(
            "manual-nvm",
            NVM,
            0x100,
            None,
            Box::new(Samd51Nvmctrl::new()),
        );
        bus
    }
    fn complete(bus: &mut SystemBus) {
        assert!(bus.models_flash_ops());
        assert!(bus.has_pending_flash_op());
        for _ in 0..3 {
            bus.service_samd51_storage_boundary().unwrap();
            assert!(bus.has_pending_flash_op());
        }
        bus.service_samd51_storage_boundary().unwrap();
        assert!(!bus.has_pending_flash_op());
    }
    #[test]
    fn actual_backing_changes_only_at_final_boundary_on_both_bus_paths() {
        for optimized in [false, true] {
            let mut b = bus(optimized);
            b.write_u32(0x78000, 0x12345678).unwrap();
            assert_eq!(b.read_u32(0x78000).unwrap(), 0xffffffff);
            b.write_u16(NVM + 4, 0xa504).unwrap();
            for _ in 0..3 {
                b.service_samd51_storage_boundary().unwrap();
                assert_eq!(b.read_u32(0x78000).unwrap(), 0xffffffff);
            }
            b.service_samd51_storage_boundary().unwrap();
            assert_eq!(b.read_u32(0x78000).unwrap(), 0x12345678);
            assert_eq!(b.read_u32(0x78004).unwrap(), 0xffffffff);
            b.flash.write_u8(0x77fff, 0x5a);
            b.flash.write_u8(0x7a000, 0xa5);
            b.write_u32(NVM + 0x14, 0x78123).unwrap();
            b.write_u16(NVM + 4, 0xa501).unwrap();
            complete(&mut b);
            assert!(b.flash.data[0x78000..0x7a000].iter().all(|v| *v == 0xff));
            assert_eq!(b.read_u8(0x77fff).unwrap(), 0x5a);
            assert_eq!(b.read_u8(0x7a000).unwrap(), 0xa5);
            assert!(b.write_u8(0x78000, 0).is_err());
            assert!(b.write_u16(0x78000, 0).is_err());
        }
    }
    #[test]
    fn absent_model_preserves_normal_flash_stores_and_wrong_geometry_refuses() {
        let mut b = SystemBus::new();
        b.write_u32(0x4000, 0x1234).unwrap();
        assert_eq!(b.read_u32(0x4000).unwrap(), 0x1234);
        b.add_peripheral("nvm", NVM, 0x100, None, Box::new(Samd51Nvmctrl::new()));
        assert!(b.write_u32(0x78000, 0).is_err()); // default backing is1MiB
    }
}
