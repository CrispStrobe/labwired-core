// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Synchronous ADIv5 SWD. A successful read's data phase is the value this
//! transaction just produced. Posted reads are not implemented.

use crate::bus::SystemBus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdAck {
    Ok,
    Wait,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwdWdata {
    pub word: u32,
    pub parity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdHostError {
    MissingWriteData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdTurn {
    NoAck,
    Ack {
        ack: SwdAck,
        data: Option<u32>,
        parity: Option<u8>,
    },
}

#[derive(Debug)]
pub struct SwdDp {
    idcode: u32,
    select: u32,
    req_dbg: bool,
    req_sys: bool,
    sticky_err: bool,
    wdata_err: bool,
    last_read: Option<u32>,
    last_ap_read: Option<u32>,
    // MEM-AP CSW and TAR. The AP path does not read them until it is implemented.
    #[allow(dead_code)]
    csw: u32,
    #[allow(dead_code)]
    tar: u32,
}

impl SwdDp {
    pub fn new(idcode: u32) -> Self {
        Self {
            idcode,
            select: 0,
            req_dbg: false,
            req_sys: false,
            sticky_err: false,
            wdata_err: false,
            last_read: None,
            last_ap_read: None,
            csw: 0x0000_0040,
            tar: 0,
        }
    }

    pub fn transact(
        &mut self,
        bus: &mut SystemBus,
        header: u8,
        wdata: Option<SwdWdata>,
    ) -> Result<SwdTurn, SwdHostError> {
        if !header_ok(header) {
            return Ok(SwdTurn::NoAck);
        }
        let ap = header & (1 << 1) != 0;
        let read = header & (1 << 2) != 0;
        let addr = (((header >> 3) & 1) << 2) | (((header >> 4) & 1) << 3);
        if !read && wdata.is_none() {
            return Err(SwdHostError::MissingWriteData);
        }
        if ap {
            self.ap_access(bus, read, addr, wdata)
        } else {
            self.dp_access(read, addr, wdata)
        }
    }

    fn dp_access(
        &mut self,
        read: bool,
        addr: u8,
        wdata: Option<SwdWdata>,
    ) -> Result<SwdTurn, SwdHostError> {
        let bank = self.select & 0xF;
        match (addr, read) {
            (0x0, true) => Ok(self.ok_read(self.idcode)),
            (0x0, false) => {
                let data = wdata.unwrap();
                if !data_parity_ok(data) {
                    self.wdata_err = true;
                    return Ok(ok_write());
                }
                if data.word & (1 << 2) != 0 {
                    self.sticky_err = false;
                }
                if data.word & (1 << 3) != 0 {
                    self.wdata_err = false;
                }
                Ok(ok_write())
            }
            (0x4, true) if bank == 0 => Ok(self.ok_read(self.ctrl_stat())),
            (0x4, false) if bank == 0 => {
                let data = wdata.unwrap();
                if !data_parity_ok(data) {
                    self.wdata_err = true;
                    return Ok(ok_write());
                }
                self.req_dbg = data.word & (1 << 28) != 0;
                self.req_sys = data.word & (1 << 30) != 0;
                Ok(ok_write())
            }
            (0x4, _) => Ok(if read { self.ok_read(0) } else { ok_write() }),
            (0x8, true) => Ok(self.ok_read(self.last_read.unwrap_or(0))),
            (0x8, false) => {
                let data = wdata.unwrap();
                if !data_parity_ok(data) {
                    self.wdata_err = true;
                    return Ok(ok_write());
                }
                self.select = data.word;
                Ok(ok_write())
            }
            (0xC, true) => Ok(self.ok_read(self.last_ap_read.unwrap_or(0))),
            (0xC, false) => Ok(ok_write()),
            _ => Ok(SwdTurn::NoAck),
        }
    }

    fn ap_access(
        &mut self,
        _bus: &mut SystemBus,
        _read: bool,
        _addr: u8,
        _wdata: Option<SwdWdata>,
    ) -> Result<SwdTurn, SwdHostError> {
        Ok(SwdTurn::Ack {
            ack: SwdAck::Fault,
            data: None,
            parity: None,
        })
    }

    fn ctrl_stat(&self) -> u32 {
        let mut w = 0u32;
        if self.req_sys {
            w |= (1 << 30) | (1 << 31);
        }
        if self.req_dbg {
            w |= (1 << 28) | (1 << 29);
        }
        if self.wdata_err {
            w |= 1 << 7;
        }
        if self.sticky_err {
            w |= 1 << 5;
        }
        w
    }

    fn ok_read(&mut self, word: u32) -> SwdTurn {
        self.last_read = Some(word);
        SwdTurn::Ack {
            ack: SwdAck::Ok,
            data: Some(word),
            parity: Some((word.count_ones() & 1) as u8),
        }
    }
}

fn data_parity_ok(data: SwdWdata) -> bool {
    (data.word.count_ones() & 1) as u8 == data.parity
}

fn ok_write() -> SwdTurn {
    SwdTurn::Ack {
        ack: SwdAck::Ok,
        data: None,
        parity: None,
    }
}

fn header_ok(header: u8) -> bool {
    let start = header & 1 == 1;
    let stop = (header >> 6) & 1 == 0;
    let park = (header >> 7) & 1 == 1;
    let body = (header >> 1) & 0xF;
    let parity = (header >> 5) & 1;
    start && stop && park && parity == (body.count_ones() & 1) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::SystemBus;
    use crate::system::cortex_m::{attach_swd_dp, configure_cortex_m};
    use crate::Machine;

    fn port() -> (SwdDp, Machine<crate::cpu::CortexM>) {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let chip =
            labwired_config::ChipDescriptor::from_file(root.join("configs/chips/nrf52840.yaml"))
                .expect("chip");
        let manifest: labwired_config::SystemManifest =
            serde_yaml::from_str("name: swd-gate\nchip: ignored\n").expect("manifest");
        let mut bus = SystemBus::from_config(&chip, &manifest).expect("bus");
        let (cpu, _) = configure_cortex_m(&mut bus);
        let mut machine = Machine::new(cpu, bus);
        let dp = attach_swd_dp(&mut machine.bus, &mut machine.cpu, 0x2BA0_1477);
        (dp, machine)
    }

    #[test]
    fn bad_header_does_not_stick() {
        let (mut dp, mut m) = port();
        let bad = 0xA5 ^ (1 << 5); // flipped parity
        assert!(matches!(
            dp.transact(&mut m.bus, bad, None).unwrap(),
            SwdTurn::NoAck
        ));
        match dp.transact(&mut m.bus, 0xA5, None).unwrap() {
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                data: Some(0x2BA0_1477),
                parity: Some(p),
            } => {
                assert_eq!(p, (0x2BA0_1477u32.count_ones() & 1) as u8);
            }
            other => panic!("expected IDCODE, got {other:?}"),
        }
    }

    #[test]
    fn bad_park_and_bad_stop_are_no_ack() {
        let (mut dp, mut m) = port();
        assert!(matches!(
            dp.transact(&mut m.bus, 0xA5 & !(1 << 7), None).unwrap(),
            SwdTurn::NoAck
        ));
        assert!(matches!(
            dp.transact(&mut m.bus, 0xA5 | (1 << 6), None).unwrap(),
            SwdTurn::NoAck
        ));
    }

    #[test]
    fn resend_repeats_idcode_and_rdbuff_stays_zero() {
        let (mut dp, mut m) = port();
        dp.transact(&mut m.bus, 0xA5, None).unwrap();
        // DP read of address 0x8: A[2]=0, A[3]=1, RnW=1, APnDP=0.
        let resend = swd_header(false, true, 0x8);
        match dp.transact(&mut m.bus, resend, None).unwrap() {
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                data: Some(word),
                ..
            } => {
                assert_eq!(word, 0x2BA0_1477);
            }
            other => panic!("RESEND {other:?}"),
        }
        let rdbuff = swd_header(false, true, 0xC);
        match dp.transact(&mut m.bus, rdbuff, None).unwrap() {
            SwdTurn::Ack {
                ack: SwdAck::Ok,
                data: Some(0),
                ..
            } => {}
            other => panic!("RDBUFF {other:?}"),
        }
    }

    pub(crate) fn swd_header(ap: bool, read: bool, addr: u8) -> u8 {
        let mut h = 1u8;
        if ap {
            h |= 1 << 1;
        }
        if read {
            h |= 1 << 2;
        }
        if addr & 0x4 != 0 {
            h |= 1 << 3;
        }
        if addr & 0x8 != 0 {
            h |= 1 << 4;
        }
        if ((h >> 1) & 0xF).count_ones() & 1 == 1 {
            h |= 1 << 5;
        }
        h |= 1 << 7;
        h
    }
}
