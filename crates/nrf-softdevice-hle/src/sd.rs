// SPDX-License-Identifier: MIT
//! High-level emulation of the nRF51 SoftDevice API (S110 v8 family), clean
//! room: behaviour from the API documentation in the BSD-3 nrf51-sdk headers
//! [H], the Bluetooth Core Specification [S] and what the application
//! observably expects [O]. No Nordic code is executed, read or reproduced.

use std::collections::VecDeque;

use crate::aes;
use crate::air::{Air, AirMsg};
use crate::facts::{err::*, evt::*, k::*, svc::*};
use crate::gatt::{AttEffect, Attr, GattDb, Store, Uuid, UUID_CCCD, UUID_CHAR, UUID_PRIMARY};
use crate::host::{Host, HostExt, NvicOp};

/// Which SoftDevice API the application was built against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// S110 v8 on nRF51822 (micro:bit V1, Calliope mini 1): app at 0x18000.
    S110v8,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub family: Family,
    /// Node name on the air.
    pub node: String,
    /// Device address, little endian (as in ble_gap_addr_t.addr). The real
    /// SoftDevice derives a random static address from FICR.DEVICEADDR.
    pub addr: [u8; 6],
    pub flash_page: u32,
    pub rng_seed: u64,
}

impl Config {
    pub fn microbit_v1(node: &str, addr: [u8; 6]) -> Config {
        Config {
            family: Family::S110v8,
            node: node.to_string(),
            addr,
            flash_page: 1024,
            rng_seed: 0x5EED_0001,
        }
    }
}

/// One recorded supervisor call, for traces and conformance vectors.
#[derive(Debug, Clone, PartialEq)]
pub struct SvcRecord {
    pub n: u64,
    pub svc: u8,
    pub args: [u32; 4],
    pub ret: u32,
    pub time_us: u64,
}

#[derive(Default)]
struct Smp {
    preq: Vec<u8>,
    pres: Vec<u8>,
    mconfirm: [u8; 16],
    srand: [u8; 16],
    stk: [u8; 16],
    keyset: u32,
    expect_central_keys: u8, // bitmask: 1 enc, 2 id, 4 sign
    got_central_keys: u8,
    resp_kdist: u8,
    init_kdist: u8,
    bond: bool,
    waiting_app_reply: bool,
    ltk: [u8; 16],
    ediv: u16,
    rand: [u8; 8],
}

struct Conn {
    peer: String,
    peer_addr: [u8; 6],
    /// SMP address type of the initiator: 0 public, 1 random.
    peer_type: u8,
    encrypted: bool,
    mtu: u16,
    smp: Smp,
    pending_enc: Option<([u8; 8], u16, [u8; 16])>,
}

pub struct SoftDevice {
    pub cfg: Config,
    pub enabled: bool,
    pub ble_enabled: bool,
    pub vector_base: u32,
    soc_evts: VecDeque<u32>,
    ble_evts: VecDeque<Vec<u8>>,
    pub gatt: GattDb,
    vs_uuids: Vec<[u8; 16]>,
    adv_data: Vec<u8>,
    sr_data: Vec<u8>,
    pub advertising: bool,
    adv_type: u8,
    adv_interval_us: u64,
    last_adv_us: Option<u64>,
    device_name: Vec<u8>,
    appearance: u16,
    ppcp: [u8; 8],
    conn: Option<Conn>,
    air: Option<Box<dyn Air>>,
    rng: u64,
    gpregret: u32,
    crit_masked: Vec<u32>,
    crit_depth: u32,
    pub trace: Vec<SvcRecord>,
    ncalls: u64,
    pub log: Vec<String>,
}

fn addr_str(a: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        a[5], a[4], a[3], a[2], a[1], a[0]
    )
}
fn parse_addr(s: &str) -> [u8; 6] {
    let mut a = [0u8; 6];
    let parts: Vec<u8> = s
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .filter_map(|p| u8::from_str_radix(p, 16).ok())
        .collect();
    if parts.len() == 6 {
        for i in 0..6 {
            a[i] = parts[5 - i];
        }
    }
    a
}

/// Bluetooth security function e (AES-128, little-endian octet order as carried in SMP PDUs).
fn e_le(key: &[u8; 16], data: &[u8; 16]) -> [u8; 16] {
    let mut k = *key;
    k.reverse();
    let mut d = *data;
    d.reverse();
    let mut r = aes::encrypt(&k, &d);
    r.reverse();
    r
}
fn xor16(a: &[u8; 16], b: &[u8; 16]) -> [u8; 16] {
    let mut r = [0u8; 16];
    for i in 0..16 {
        r[i] = a[i] ^ b[i];
    }
    r
}
/// Legacy pairing confirm value c1 [S Vol 3 Part H 2.2.3].
fn c1(
    k: &[u8; 16],
    r: &[u8; 16],
    preq: &[u8],
    pres: &[u8],
    iat: u8,
    rat: u8,
    ia: &[u8; 6],
    ra: &[u8; 6],
) -> [u8; 16] {
    let mut p1 = [0u8; 16];
    p1[0] = iat;
    p1[1] = rat;
    p1[2..9].copy_from_slice(&preq[..7]);
    p1[9..16].copy_from_slice(&pres[..7]);
    let mut p2 = [0u8; 16];
    p2[0..6].copy_from_slice(ra);
    p2[6..12].copy_from_slice(ia);
    e_le(k, &xor16(&e_le(k, &xor16(r, &p1)), &p2))
}
/// Legacy STK generation s1 [S Vol 3 Part H 2.2.4]: r1 = Srand, r2 = Mrand.
fn s1(k: &[u8; 16], r1: &[u8; 16], r2: &[u8; 16]) -> [u8; 16] {
    let mut r = [0u8; 16];
    r[0..8].copy_from_slice(&r2[0..8]);
    r[8..16].copy_from_slice(&r1[0..8]);
    e_le(k, &r)
}

impl SoftDevice {
    pub fn new(cfg: Config) -> SoftDevice {
        let rng = cfg.rng_seed | 1;
        SoftDevice {
            cfg,
            enabled: false,
            ble_enabled: false,
            vector_base: 0,
            soc_evts: VecDeque::new(),
            ble_evts: VecDeque::new(),
            gatt: GattDb::default(),
            vs_uuids: Vec::new(),
            adv_data: Vec::new(),
            sr_data: Vec::new(),
            advertising: false,
            adv_type: 0,
            adv_interval_us: 100_000,
            last_adv_us: None,
            device_name: b"nRF5x".to_vec(),
            appearance: 0,
            ppcp: [0; 8],
            conn: None,
            air: None,
            rng,
            gpregret: 0,
            crit_masked: Vec::new(),
            crit_depth: 0,
            trace: Vec::new(),
            ncalls: 0,
            log: Vec::new(),
        }
    }

    pub fn attach_air(&mut self, air: Box<dyn Air>) {
        self.air = Some(air);
    }

    pub fn address(&self) -> String {
        addr_str(&self.cfg.addr)
    }

    pub fn connected(&self) -> bool {
        self.conn.is_some()
    }

    fn rand8(&mut self) -> u8 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 24) as u8
    }
    fn rand_block(&mut self) -> [u8; 16] {
        let mut b = [0u8; 16];
        for x in b.iter_mut() {
            *x = self.rand8();
        }
        b
    }

    fn send(&mut self, m: AirMsg) {
        if let Some(a) = self.air.as_mut() {
            a.send(&m);
        }
    }

    fn pend_evt_irq(&mut self, h: &mut dyn Host) {
        h.nvic(NvicOp::SetPending(SD_EVT_IRQN));
    }

    fn push_ble_evt(&mut self, id: u16, body: Vec<u8>, h: &mut dyn Host) {
        let mut e = Vec::with_capacity(4 + body.len());
        e.extend(id.to_le_bytes());
        e.extend((body.len() as u16).to_le_bytes());
        e.extend(body);
        self.ble_evts.push_back(e);
        self.pend_evt_irq(h);
    }
    fn push_soc_evt(&mut self, id: u32, h: &mut dyn Host) {
        self.soc_evts.push_back(id);
        self.pend_evt_irq(h);
    }

    /// Service one supervisor call. `args` are r0..r3 at the SVC; the return
    /// value goes to r0.
    pub fn svc(&mut self, num: u8, args: [u32; 4], h: &mut dyn Host) -> u32 {
        let ret = self.dispatch(num, args, h);
        self.ncalls += 1;
        let t = h.now_us();
        self.trace.push(SvcRecord {
            n: self.ncalls,
            svc: num,
            args,
            ret,
            time_us: t,
        });
        if self.trace.len() > 4096 {
            self.trace.drain(..1024);
        }
        ret
    }

    fn dispatch(&mut self, num: u8, a: [u32; 4], h: &mut dyn Host) -> u32 {
        match num {
            // ---- SoftDevice manager / MBR ----
            SD_SOFTDEVICE_ENABLE => {
                if self.enabled {
                    return NRF_ERROR_INVALID_STATE;
                }
                self.enabled = true;
                NRF_SUCCESS
            }
            SD_SOFTDEVICE_DISABLE => {
                self.enabled = false;
                self.ble_enabled = false;
                self.advertising = false;
                NRF_SUCCESS
            }
            SD_SOFTDEVICE_IS_ENABLED => {
                if !h.w8(a[0], self.enabled as u8) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_SOFTDEVICE_VECTOR_TABLE_BASE_SET => {
                self.vector_base = a[0];
                NRF_SUCCESS
            }
            SD_MBR_COMMAND => {
                // sd_mbr_command_t: command u32 @0, params @4. Only the vector
                // table base is meaningful without an MBR.
                match h.r32(a[0]) {
                    Some(4) => {
                        self.vector_base = h.r32(a[0] + 4).unwrap_or(0);
                        NRF_SUCCESS
                    }
                    Some(_) => NRF_ERROR_NOT_SUPPORTED,
                    None => NRF_ERROR_INVALID_ADDR,
                }
            }
            // ---- SoC library ----
            SD_NVIC_ENABLEIRQ => {
                // Application interrupts run at NRF_APP_PRIORITY_HIGH (1) or
                // LOW (3) [H nrf_soc.h]; 0 and 2 belong to the SoftDevice. The
                // SoftDevice event IRQ is where the app calls back into the
                // SoftDevice (sd_ble_evt_get), so it must sit below SVCall (0):
                // at 0 the SVC would escalate to HardFault. The app never sets
                // it (measured: only sd_nvic_EnableIRQ(22) is called); the HLE
                // gives it APP_LOW when it is still at the reset value 0 [X].
                if a[0] == SD_EVT_IRQN && h.nvic(NvicOp::GetPriority(a[0])) == 0 {
                    h.nvic(NvicOp::SetPriority(a[0], 3));
                }
                h.nvic(NvicOp::Enable(a[0]));
                NRF_SUCCESS
            }
            SD_NVIC_DISABLEIRQ => {
                h.nvic(NvicOp::Disable(a[0]));
                NRF_SUCCESS
            }
            SD_NVIC_GETPENDINGIRQ => {
                let v = h.nvic(NvicOp::GetPending(a[0]));
                if !h.w32(a[1], v) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_NVIC_SETPENDINGIRQ => {
                h.nvic(NvicOp::SetPending(a[0]));
                NRF_SUCCESS
            }
            SD_NVIC_CLEARPENDINGIRQ => {
                h.nvic(NvicOp::ClearPending(a[0]));
                NRF_SUCCESS
            }
            SD_NVIC_SETPRIORITY => {
                h.nvic(NvicOp::SetPriority(a[0], a[1]));
                NRF_SUCCESS
            }
            SD_NVIC_GETPRIORITY => {
                let v = h.nvic(NvicOp::GetPriority(a[0]));
                if !h.w32(a[1], v) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_NVIC_SYSTEMRESET => {
                h.nvic(NvicOp::SystemReset);
                NRF_SUCCESS
            }
            SD_NVIC_CRITICAL_REGION_ENTER => {
                // The SoftDevice masks the application's interrupts (its own
                // keep running); PRIMASK would forbid the SVC that exits.
                let nested = self.crit_depth > 0;
                if !nested {
                    self.crit_masked.clear();
                    for irq in 0..32 {
                        if irq != SD_EVT_IRQN && h.nvic(NvicOp::GetEnabled(irq)) != 0 {
                            h.nvic(NvicOp::Disable(irq));
                            self.crit_masked.push(irq);
                        }
                    }
                }
                self.crit_depth += 1;
                h.w8(a[0], nested as u8);
                NRF_SUCCESS
            }
            SD_NVIC_CRITICAL_REGION_EXIT => {
                // Argument: is_nested_critical_region (by value).
                if a[0] & 0xFF == 0 || self.crit_depth <= 1 {
                    for irq in std::mem::take(&mut self.crit_masked) {
                        h.nvic(NvicOp::Enable(irq));
                    }
                    self.crit_depth = 0;
                } else {
                    self.crit_depth -= 1;
                }
                NRF_SUCCESS
            }
            SD_RAND_APPLICATION_POOL_CAPACITY | SD_RAND_APPLICATION_BYTES_AVAILABLE => {
                if !h.w8(a[0], 64) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_RAND_APPLICATION_GET_VECTOR => {
                let n = (a[1] & 0xFF) as usize;
                let v: Vec<u8> = (0..n).map(|_| self.rand8()).collect();
                if !h.write(a[0], &v) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_POWER_MODE_SET
            | SD_POWER_POF_ENABLE
            | SD_POWER_POF_THRESHOLD_SET
            | SD_POWER_RAMON_SET
            | SD_POWER_RAMON_CLR
            | SD_POWER_DCDC_MODE_SET
            | SD_POWER_RESET_REASON_CLR
            | SD_CLOCK_HFCLK_RELEASE
            | SD_PPI_CHANNEL_ENABLE_SET
            | SD_PPI_CHANNEL_ENABLE_CLR
            | SD_PPI_CHANNEL_ASSIGN
            | SD_RADIO_NOTIFICATION_CFG_SET
            | SD_FLASH_PROTECT => NRF_SUCCESS,
            SD_POWER_SYSTEM_OFF => NRF_SUCCESS,
            SD_POWER_RESET_REASON_GET | SD_POWER_RAMON_GET | SD_PPI_CHANNEL_ENABLE_GET => {
                if !h.w32(a[0], 0) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_POWER_GPREGRET_SET => {
                self.gpregret |= a[0];
                NRF_SUCCESS
            }
            SD_POWER_GPREGRET_CLR => {
                self.gpregret &= !a[0];
                NRF_SUCCESS
            }
            SD_POWER_GPREGRET_GET => {
                if !h.w32(a[0], self.gpregret) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_APP_EVT_WAIT => NRF_SUCCESS,
            SD_CLOCK_HFCLK_REQUEST => NRF_SUCCESS,
            SD_CLOCK_HFCLK_IS_RUNNING => {
                if !h.w32(a[0], 1) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_ECB_BLOCK_ENCRYPT => {
                // nrf_ecb_hal_data_t: key[16] @0, cleartext[16] @16, ciphertext[16] @32.
                let (Some(k), Some(p)) = (h.rbuf(a[0], 16), h.rbuf(a[0] + 16, 16)) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let c = aes::encrypt(&k.try_into().unwrap(), &p.try_into().unwrap());
                h.write(a[0] + 32, &c);
                NRF_SUCCESS
            }
            SD_EVT_GET => match self.soc_evts.pop_front() {
                Some(e) => {
                    h.w32(a[0], e);
                    NRF_SUCCESS
                }
                None => NRF_ERROR_NOT_FOUND,
            },
            SD_TEMP_GET => {
                // 0.25 degC units: 21.00 degC.
                h.w32(a[0], 84);
                NRF_SUCCESS
            }
            SD_FLASH_PAGE_ERASE => {
                let base = a[0] * self.cfg.flash_page;
                h.write(base, &vec![0xFF; self.cfg.flash_page as usize]);
                self.push_soc_evt(NRF_EVT_FLASH_OPERATION_SUCCESS, h);
                NRF_SUCCESS
            }
            SD_FLASH_WRITE => {
                let Some(data) = h.rbuf(a[1], (a[2] * 4) as usize) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                // NOR semantics: a write can only clear bits.
                let old = h
                    .rbuf(a[0], data.len())
                    .unwrap_or_else(|| vec![0xFF; data.len()]);
                let merged: Vec<u8> = old.iter().zip(&data).map(|(o, n)| o & n).collect();
                h.write(a[0], &merged);
                self.push_soc_evt(NRF_EVT_FLASH_OPERATION_SUCCESS, h);
                NRF_SUCCESS
            }
            // ---- BLE common ----
            SD_BLE_ENABLE => {
                if !self.enabled {
                    return NRF_ERROR_INVALID_STATE;
                }
                self.ble_enabled = true;
                self.add_gap_gatt_services();
                NRF_SUCCESS
            }
            SD_BLE_EVT_GET => {
                let Some(e) = self.ble_evts.front().cloned() else {
                    return NRF_ERROR_NOT_FOUND;
                };
                let len = e.len() as u16;
                if a[0] == 0 {
                    h.w16(a[1], len);
                    return NRF_SUCCESS;
                }
                let cap = h.r16(a[1]).unwrap_or(0);
                if cap < len {
                    h.w16(a[1], len);
                    return NRF_ERROR_DATA_SIZE;
                }
                h.write(a[0], &e);
                h.w16(a[1], len);
                self.ble_evts.pop_front();
                NRF_SUCCESS
            }
            SD_BLE_TX_BUFFER_COUNT_GET => {
                h.w8(a[0], 6);
                NRF_SUCCESS
            }
            SD_BLE_UUID_VS_ADD => {
                let Some(b) = h.rbuf(a[0], 16) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let mut base: [u8; 16] = b.try_into().unwrap();
                base[12] = 0;
                base[13] = 0;
                let idx = match self.vs_uuids.iter().position(|u| *u == base) {
                    Some(i) => i,
                    None => {
                        self.vs_uuids.push(base);
                        self.vs_uuids.len() - 1
                    }
                };
                h.w8(a[1], BLE_UUID_TYPE_VENDOR_BEGIN + idx as u8);
                NRF_SUCCESS
            }
            SD_BLE_UUID_DECODE => {
                let len = a[0] & 0xFF;
                let Some(b) = h.rbuf(a[1], len as usize) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                match len {
                    2 => {
                        h.w16(a[2], u16::from_le_bytes([b[0], b[1]]));
                        h.w8(a[2] + 2, BLE_UUID_TYPE_BLE);
                        NRF_SUCCESS
                    }
                    16 => {
                        let mut base: [u8; 16] = b.clone().try_into().unwrap();
                        base[12] = 0;
                        base[13] = 0;
                        match self.vs_uuids.iter().position(|u| *u == base) {
                            Some(i) => {
                                h.w16(a[2], u16::from_le_bytes([b[12], b[13]]));
                                h.w8(a[2] + 2, BLE_UUID_TYPE_VENDOR_BEGIN + i as u8);
                                NRF_SUCCESS
                            }
                            None => NRF_ERROR_NOT_FOUND,
                        }
                    }
                    _ => NRF_ERROR_INVALID_LENGTH,
                }
            }
            SD_BLE_UUID_ENCODE => {
                let (Some(u), Some(t)) = (h.r16(a[0]), h.r8(a[0] + 2)) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let bytes = self.full_uuid(u, t).bytes();
                h.w8(a[1], bytes.len() as u8);
                if a[2] != 0 {
                    h.write(a[2], &bytes);
                }
                NRF_SUCCESS
            }
            SD_BLE_VERSION_GET => {
                // ble_version_t: version_number u8 @0, company_id u16 @2, subversion_number u16 @4.
                // LL version 7 (Core 4.1), Nordic company id 0x0059, FWID of S110 v8.0.0 (0x64).
                h.w8(a[0], 7);
                h.w16(a[0] + 2, 0x0059);
                h.w16(a[0] + 4, 0x0064);
                NRF_SUCCESS
            }
            SD_BLE_OPT_SET => NRF_SUCCESS,
            SD_BLE_OPT_GET => NRF_ERROR_NOT_SUPPORTED,
            SD_BLE_USER_MEM_REPLY => NRF_SUCCESS,
            // ---- GAP ----
            SD_BLE_GAP_ADDRESS_SET => {
                // S110 v8: (addr_cycle_mode, const ble_gap_addr_t *).
                if let Some(b) = h.rbuf(a[1], 7) {
                    self.cfg.addr.copy_from_slice(&b[1..7]);
                }
                NRF_SUCCESS
            }
            SD_BLE_GAP_ADDRESS_GET => {
                let mut b = vec![BLE_GAP_ADDR_TYPE_RANDOM_STATIC];
                b.extend(self.cfg.addr);
                if !h.write(a[0], &b) {
                    return NRF_ERROR_INVALID_ADDR;
                }
                NRF_SUCCESS
            }
            SD_BLE_GAP_ADV_DATA_SET => {
                let d = if a[0] != 0 {
                    h.rbuf(a[0], (a[1] & 0xFF) as usize)
                } else {
                    Some(vec![])
                };
                let s = if a[2] != 0 {
                    h.rbuf(a[2], (a[3] & 0xFF) as usize)
                } else {
                    Some(vec![])
                };
                let (Some(d), Some(s)) = (d, s) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                if d.len() > 31 || s.len() > 31 {
                    return NRF_ERROR_INVALID_LENGTH;
                }
                if a[0] != 0 {
                    self.adv_data = d;
                }
                if a[2] != 0 {
                    self.sr_data = s;
                }
                NRF_SUCCESS
            }
            SD_BLE_GAP_ADV_START => {
                if self.advertising {
                    return NRF_ERROR_INVALID_STATE;
                }
                // ble_gap_adv_params_t: type @0, interval @16 (0.625 ms units).
                self.adv_type = h.r8(a[0]).unwrap_or(0);
                let iv = h.r16(a[0] + 16).unwrap_or(160).max(32) as u64;
                self.adv_interval_us = iv * 625;
                self.advertising = true;
                self.last_adv_us = None;
                self.log.push(format!(
                    "adv start type {} interval {} us",
                    self.adv_type, self.adv_interval_us
                ));
                NRF_SUCCESS
            }
            SD_BLE_GAP_ADV_STOP => {
                if !self.advertising {
                    return NRF_ERROR_INVALID_STATE;
                }
                self.advertising = false;
                NRF_SUCCESS
            }
            SD_BLE_GAP_CONN_PARAM_UPDATE => {
                if self.conn.is_none() {
                    return BLE_ERROR_INVALID_CONN_HANDLE;
                }
                NRF_SUCCESS
            }
            SD_BLE_GAP_DISCONNECT => {
                let Some(c) = self.conn.take() else {
                    return BLE_ERROR_INVALID_CONN_HANDLE;
                };
                let reason = (a[1] & 0xFF) as u8;
                let me = self.address();
                self.send(AirMsg::Ll {
                    src: me,
                    dst: c.peer.clone(),
                    op: "terminate_ind".into(),
                    error_code: reason,
                    rand: vec![],
                    ediv: 0,
                    ltk: vec![],
                });
                let mut body = 0u16.to_le_bytes().to_vec();
                body.push(BLE_HCI_LOCAL_HOST_TERMINATED_CONNECTION);
                self.push_ble_evt(BLE_GAP_EVT_DISCONNECTED, body, h);
                NRF_SUCCESS
            }
            SD_BLE_GAP_TX_POWER_SET => NRF_SUCCESS,
            SD_BLE_GAP_APPEARANCE_SET => {
                self.appearance = a[0] as u16;
                self.set_gap_value(0x2A01, &(a[0] as u16).to_le_bytes(), h);
                NRF_SUCCESS
            }
            SD_BLE_GAP_APPEARANCE_GET => {
                h.w16(a[0], self.appearance);
                NRF_SUCCESS
            }
            SD_BLE_GAP_PPCP_SET => {
                let Some(b) = h.rbuf(a[0], 8) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                self.ppcp.copy_from_slice(&b);
                self.set_gap_value(0x2A04, &b, h);
                NRF_SUCCESS
            }
            SD_BLE_GAP_PPCP_GET => {
                h.write(a[0], &self.ppcp.clone());
                NRF_SUCCESS
            }
            SD_BLE_GAP_DEVICE_NAME_SET => {
                let Some(n) = h.rbuf(a[1], (a[2] & 0xFFFF) as usize) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                self.device_name = n.clone();
                self.set_gap_value(0x2A00, &n, h);
                NRF_SUCCESS
            }
            SD_BLE_GAP_DEVICE_NAME_GET => {
                let n = self.device_name.clone();
                if a[0] != 0 {
                    h.write(a[0], &n);
                }
                h.w16(a[1], n.len() as u16);
                NRF_SUCCESS
            }
            SD_BLE_GAP_SEC_PARAMS_REPLY => self.sec_params_reply(a, h),
            SD_BLE_GAP_SEC_INFO_REPLY => self.sec_info_reply(a, h),
            SD_BLE_GAP_CONN_SEC_GET => {
                let enc = self.conn.as_ref().map(|c| c.encrypted).unwrap_or(false);
                h.w8(a[1], if enc { 0x21 } else { 0x11 });
                h.w8(a[1] + 1, if enc { 16 } else { 0 });
                NRF_SUCCESS
            }
            SD_BLE_GAP_AUTHENTICATE
            | SD_BLE_GAP_AUTH_KEY_REPLY
            | SD_BLE_GAP_RSSI_START
            | SD_BLE_GAP_RSSI_STOP => NRF_SUCCESS,
            // ---- GATT server ----
            SD_BLE_GATTS_SERVICE_ADD => {
                let (Some(u), Some(t)) = (h.r16(a[1]), h.r8(a[1] + 2)) else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let uuid = self.full_uuid(u, t);
                let hnd = self.gatt.next_handle();
                self.gatt.services.push(hnd);
                self.gatt.attrs.push(Attr {
                    handle: hnd,
                    uuid: Uuid::U16(UUID_PRIMARY),
                    app_uuid: (u, t),
                    kind: 1,
                    value: Store::Stack(uuid.bytes()),
                    max_len: 16,
                    vlen: false,
                    read_perm: 0x11,
                    write_perm: 0,
                    rd_auth: false,
                    wr_auth: false,
                    srvc_handle: hnd,
                    char_value_handle: 0,
                    srvc_uuid: (u, t),
                    char_uuid: (0, 0),
                });
                h.w16(a[2], hnd);
                NRF_SUCCESS
            }
            SD_BLE_GATTS_CHARACTERISTIC_ADD => self.characteristic_add(a, h),
            SD_BLE_GATTS_DESCRIPTOR_ADD => {
                let svc = self.gatt.services.last().copied().unwrap_or(0);
                let hnd = self.gatt.next_handle();
                match self.attr_from_app(a[1], hnd, svc, 6, h) {
                    Some(at) => {
                        self.gatt.attrs.push(at);
                        h.w16(a[2], hnd);
                        NRF_SUCCESS
                    }
                    None => NRF_ERROR_INVALID_ADDR,
                }
            }
            SD_BLE_GATTS_VALUE_SET => {
                // (conn_handle, handle, ble_gatts_value_t*): len @0, offset @2, p_value @4.
                let hnd = a[1] as u16;
                let (Some(len), Some(off), Some(p)) =
                    (h.r16(a[2]), h.r16(a[2] + 2), h.r32(a[2] + 4))
                else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let data = if p != 0 {
                    h.rbuf(p, len as usize).unwrap_or_default()
                } else {
                    vec![]
                };
                if self.gatt.get(hnd).is_none() {
                    return BLE_ERROR_INVALID_ATTR_HANDLE;
                }
                match self.gatt.write_value(hnd, off, &data, h) {
                    Some(nl) => {
                        h.w16(a[2], nl.min(len));
                        NRF_SUCCESS
                    }
                    None => NRF_ERROR_INVALID_PARAM,
                }
            }
            SD_BLE_GATTS_VALUE_GET => {
                let hnd = a[1] as u16;
                let Some(v) = self.gatt.read_value(hnd, h) else {
                    return BLE_ERROR_INVALID_ATTR_HANDLE;
                };
                let (Some(len), Some(off), Some(p)) =
                    (h.r16(a[2]), h.r16(a[2] + 2), h.r32(a[2] + 4))
                else {
                    return NRF_ERROR_INVALID_ADDR;
                };
                let off = off as usize;
                if off > v.len() {
                    return NRF_ERROR_INVALID_PARAM;
                }
                let n = (v.len() - off).min(len as usize);
                if p != 0 {
                    h.write(p, &v[off..off + n]);
                }
                h.w16(
                    a[2],
                    if p != 0 {
                        n as u16
                    } else {
                        (v.len() - off) as u16
                    },
                );
                NRF_SUCCESS
            }
            SD_BLE_GATTS_HVX => self.hvx(a, h),
            SD_BLE_GATTS_SERVICE_CHANGED => NRF_SUCCESS,
            SD_BLE_GATTS_RW_AUTHORIZE_REPLY => NRF_SUCCESS,
            SD_BLE_GATTS_SYS_ATTR_SET => NRF_SUCCESS,
            SD_BLE_GATTS_SYS_ATTR_GET => {
                if a[2] != 0 {
                    h.w16(a[2], 0);
                }
                NRF_SUCCESS
            }
            _ => NRF_ERROR_NOT_SUPPORTED,
        }
    }

    fn full_uuid(&self, u: u16, t: u8) -> Uuid {
        if t >= BLE_UUID_TYPE_VENDOR_BEGIN {
            if let Some(base) = self.vs_uuids.get((t - BLE_UUID_TYPE_VENDOR_BEGIN) as usize) {
                let mut b = *base;
                b[12..14].copy_from_slice(&u.to_le_bytes());
                return Uuid::U128(b);
            }
        }
        Uuid::U16(u)
    }

    fn add_gap_gatt_services(&mut self) {
        if !self.gatt.attrs.is_empty() {
            return;
        }
        let add = |db: &mut GattDb, uuid: u16, val: Vec<u8>, perm: u8, svc: &mut u16, kind: u8| {
            let h = db.next_handle();
            if uuid == UUID_PRIMARY {
                *svc = h;
                db.services.push(h);
            }
            db.attrs.push(Attr {
                handle: h,
                uuid: Uuid::U16(uuid),
                app_uuid: (uuid, 1),
                kind,
                max_len: 64,
                value: Store::Stack(val),
                vlen: true,
                read_perm: perm,
                write_perm: 0,
                rd_auth: false,
                wr_auth: false,
                srvc_handle: *svc,
                char_value_handle: h,
                srvc_uuid: (0, 1),
                char_uuid: (uuid, 1),
            });
            h
        };
        let db = &mut self.gatt;
        let mut svc = 0;
        add(
            db,
            UUID_PRIMARY,
            0x1800u16.to_le_bytes().to_vec(),
            0x11,
            &mut svc,
            1,
        );
        for (uuid, val) in [
            (0x2A00u16, self.device_name.clone()),
            (0x2A01, vec![0, 0]),
            (0x2A04, vec![0; 8]),
        ] {
            let vh = db.next_handle() + 1;
            let mut decl = vec![0x02];
            decl.extend(vh.to_le_bytes());
            decl.extend(uuid.to_le_bytes());
            add(db, UUID_CHAR, decl, 0x11, &mut svc, 4);
            add(db, uuid, val, 0x11, &mut svc, 5);
        }
        add(
            db,
            UUID_PRIMARY,
            0x1801u16.to_le_bytes().to_vec(),
            0x11,
            &mut svc,
            1,
        );
    }

    fn set_gap_value(&mut self, uuid: u16, v: &[u8], _h: &mut dyn Host) {
        if let Some(a) = self
            .gatt
            .attrs
            .iter_mut()
            .find(|a| a.uuid == Uuid::U16(uuid) && a.kind == 5)
        {
            a.value = Store::Stack(v.to_vec());
        }
    }

    /// Build an attribute from the app's ble_gatts_attr_t.
    fn attr_from_app(
        &self,
        p: u32,
        hnd: u16,
        svc: u16,
        kind: u8,
        h: &mut dyn Host,
    ) -> Option<Attr> {
        let p_uuid = h.r32(p)?;
        let p_md = h.r32(p + 4)?;
        let init_len = h.r16(p + 8)?;
        let init_offs = h.r16(p + 10)?;
        let max_len = h.r16(p + 12)?;
        let p_value = h.r32(p + 16)?;
        let u = h.r16(p_uuid)?;
        let t = h.r8(p_uuid + 2)?;
        let rp = h.r8(p_md)?;
        let wp = h.r8(p_md + 1)?;
        let fl = h.r8(p_md + 2)?;
        let vloc = (fl >> 1) & 3;
        let value = if vloc == 2 {
            Store::User {
                addr: p_value,
                len: init_len,
            }
        } else {
            let mut v = vec![0u8; init_len as usize];
            if p_value != 0 {
                v = h.rbuf(p_value + init_offs as u32, init_len as usize)?;
            }
            Store::Stack(v)
        };
        let srvc_uuid = self.gatt.get(svc).map(|s| s.app_uuid).unwrap_or((0, 0));
        Some(Attr {
            handle: hnd,
            uuid: self.full_uuid(u, t),
            app_uuid: (u, t),
            kind,
            value,
            max_len,
            vlen: fl & 1 != 0,
            read_perm: rp,
            write_perm: wp,
            rd_auth: fl & 0x08 != 0,
            wr_auth: fl & 0x10 != 0,
            srvc_handle: svc,
            char_value_handle: hnd,
            srvc_uuid,
            char_uuid: (u, t),
        })
    }

    fn characteristic_add(&mut self, a: [u32; 4], h: &mut dyn Host) -> u32 {
        let svc = a[0] as u16;
        // ble_gatts_char_md_t: char_props @0, p_cccd_md @20.
        let Some(props) = h.r8(a[1]) else {
            return NRF_ERROR_INVALID_ADDR;
        };
        let p_cccd_md = h.r32(a[1] + 20).unwrap_or(0);
        let decl_h = self.gatt.next_handle();
        let val_h = decl_h + 1;
        let Some(mut val) = self.attr_from_app(a[2], val_h, svc, 5, h) else {
            return NRF_ERROR_INVALID_ADDR;
        };
        let mut decl = vec![props];
        decl.extend(val_h.to_le_bytes());
        decl.extend(val.uuid.bytes());
        let srvc_uuid = val.srvc_uuid;
        let char_uuid = val.app_uuid;
        self.gatt.attrs.push(Attr {
            handle: decl_h,
            uuid: Uuid::U16(UUID_CHAR),
            app_uuid: (UUID_CHAR, 1),
            kind: 4,
            value: Store::Stack(decl),
            max_len: 19,
            vlen: false,
            read_perm: 0x11,
            write_perm: 0,
            rd_auth: false,
            wr_auth: false,
            srvc_handle: svc,
            char_value_handle: val_h,
            srvc_uuid,
            char_uuid,
        });
        val.char_value_handle = val_h;
        self.gatt.attrs.push(val);
        let mut cccd_h = 0u16;
        if props & 0x30 != 0 {
            // Notify/indicate: the stack adds a CCCD; its permissions come from p_cccd_md.
            cccd_h = self.gatt.next_handle();
            let (rp, wp) = if p_cccd_md != 0 {
                (
                    h.r8(p_cccd_md).unwrap_or(0x11),
                    h.r8(p_cccd_md + 1).unwrap_or(0x11),
                )
            } else {
                (0x11, 0x11)
            };
            self.gatt.attrs.push(Attr {
                handle: cccd_h,
                uuid: Uuid::U16(UUID_CCCD),
                app_uuid: (UUID_CCCD, 1),
                kind: 6,
                value: Store::Stack(vec![0, 0]),
                max_len: 2,
                vlen: false,
                read_perm: rp,
                write_perm: wp,
                rd_auth: false,
                wr_auth: false,
                srvc_handle: svc,
                char_value_handle: val_h,
                srvc_uuid,
                char_uuid,
            });
        }
        // ble_gatts_char_handles_t: value @0, user_desc @2, cccd @4, sccd @6.
        h.w16(a[3], val_h);
        h.w16(a[3] + 2, 0);
        h.w16(a[3] + 4, cccd_h);
        h.w16(a[3] + 6, 0);
        NRF_SUCCESS
    }

    fn hvx(&mut self, a: [u32; 4], h: &mut dyn Host) -> u32 {
        if self.conn.is_none() || a[0] as u16 != 0 {
            return BLE_ERROR_INVALID_CONN_HANDLE;
        }
        // ble_gatts_hvx_params_t: handle @0, type @2, offset @4, p_len @8, p_data @12.
        let (Some(hnd), Some(ty), Some(off), Some(p_len), Some(p_data)) = (
            h.r16(a[1]),
            h.r8(a[1] + 2),
            h.r16(a[1] + 4),
            h.r32(a[1] + 8),
            h.r32(a[1] + 12),
        ) else {
            return NRF_ERROR_INVALID_ADDR;
        };
        if self.gatt.get(hnd).is_none() {
            return BLE_ERROR_INVALID_ATTR_HANDLE;
        }
        let len = if p_len != 0 {
            h.r16(p_len).unwrap_or(0)
        } else {
            0
        };
        if p_data != 0 && len > 0 {
            let d = h.rbuf(p_data, len as usize).unwrap_or_default();
            self.gatt.write_value(hnd, off, &d, h);
        }
        let cccd = self
            .gatt
            .cccd_of(hnd)
            .and_then(|c| self.gatt.read_value(c, h))
            .unwrap_or_default();
        let bits = cccd.first().copied().unwrap_or(0);
        let want = if ty == BLE_GATT_HVX_INDICATION { 2 } else { 1 };
        if bits & want == 0 {
            return NRF_ERROR_INVALID_STATE;
        }
        let v = self.gatt.read_value(hnd, h).unwrap_or_default();
        let n = v.len().min(20);
        let mut att = vec![if ty == BLE_GATT_HVX_INDICATION {
            0x1D
        } else {
            0x1B
        }];
        att.extend(hnd.to_le_bytes());
        att.extend(&v[..n]);
        self.send_att(att);
        if p_len != 0 {
            h.w16(p_len, n as u16);
        }
        if ty != BLE_GATT_HVX_INDICATION {
            // ble_common_evt_t: conn_handle @0, params.tx_complete.count @4.
            let mut body = vec![0u8; 5];
            body[4] = 1;
            self.push_ble_evt(BLE_EVT_TX_COMPLETE, body, h);
        }
        NRF_SUCCESS
    }

    fn send_l2cap(&mut self, cid: u16, payload: Vec<u8>) {
        let Some(c) = self.conn.as_ref() else { return };
        let mut f = (payload.len() as u16).to_le_bytes().to_vec();
        f.extend(cid.to_le_bytes());
        f.extend(payload);
        let m = AirMsg::Acl {
            src: addr_str(&self.cfg.addr),
            dst: c.peer.clone(),
            data: f,
        };
        self.send(m);
    }
    fn send_att(&mut self, pdu: Vec<u8>) {
        self.send_l2cap(4, pdu);
    }
    fn send_smp(&mut self, pdu: Vec<u8>) {
        self.send_l2cap(6, pdu);
    }

    fn sec_params_reply(&mut self, a: [u32; 4], h: &mut dyn Host) -> u32 {
        let Some(c) = self.conn.as_mut() else {
            return BLE_ERROR_INVALID_CONN_HANDLE;
        };
        if !c.smp.waiting_app_reply {
            return NRF_ERROR_INVALID_STATE;
        }
        c.smp.waiting_app_reply = false;
        let status = (a[1] & 0xFF) as u8;
        if status != 0 {
            self.send_smp(vec![0x05, 0x05]); // Pairing Failed: Pairing Not Supported
            return NRF_SUCCESS;
        }
        // ble_gap_sec_params_t: b0 bond|mitm<<1|io_caps<<2|oob<<5, min, max, kdist_periph, kdist_central.
        let sp = if a[2] != 0 {
            h.rbuf(a[2], 5).unwrap_or(vec![1, 16, 16, 1, 0])
        } else {
            vec![1, 16, 16, 1, 0]
        };
        let c = self.conn.as_mut().unwrap();
        c.smp.keyset = a[3];
        let preq = c.smp.preq.clone();
        let bond = sp[0] & 1 != 0 && preq[3] & 1 != 0;
        let io = (sp[0] >> 2) & 7;
        let oob = (sp[0] >> 5) & 1;
        let auth = (bond as u8) | (((sp[0] >> 1) & 1) << 2);
        let resp_kd = preq[6] & sp[3] & 0x07;
        let init_kd = preq[5] & sp[4] & 0x07;
        let pres = vec![0x02, io, oob, auth, sp[2].clamp(7, 16), init_kd, resp_kd];
        c.smp.pres = pres.clone();
        c.smp.bond = bond;
        c.smp.resp_kdist = resp_kd;
        c.smp.init_kdist = init_kd;
        c.smp.expect_central_keys = init_kd;
        self.send_smp(pres);
        NRF_SUCCESS
    }

    fn sec_info_reply(&mut self, a: [u32; 4], h: &mut dyn Host) -> u32 {
        let Some(c) = self.conn.as_mut() else {
            return BLE_ERROR_INVALID_CONN_HANDLE;
        };
        let Some((_rand, _ediv, want)) = c.pending_enc.take() else {
            return NRF_ERROR_INVALID_STATE;
        };
        let peer = c.peer.clone();
        let me = addr_str(&self.cfg.addr);
        // p_enc_info: ble_gap_enc_info_t: ltk[16] @0.
        let ltk = if a[1] != 0 { h.rbuf(a[1], 16) } else { None };
        if ltk.as_deref() == Some(&want[..]) {
            self.send(AirMsg::Ll {
                src: me,
                dst: peer,
                op: "start_enc_rsp".into(),
                error_code: 0,
                rand: vec![],
                ediv: 0,
                ltk: vec![],
            });
            self.conn.as_mut().unwrap().encrypted = true;
            self.push_ble_evt(BLE_GAP_EVT_CONN_SEC_UPDATE, vec![0, 0, 0x21, 16], h);
        } else {
            self.send(AirMsg::Ll {
                src: me,
                dst: peer,
                op: "reject_ext_ind".into(),
                error_code: 0x06,
                rand: vec![],
                ediv: 0,
                ltk: vec![],
            });
        }
        NRF_SUCCESS
    }

    /// Advance: take air traffic, advertise. Call often (every ms or so of
    /// emulated time); cheap when idle.
    pub fn poll(&mut self, h: &mut dyn Host) {
        let now = h.now_us();
        if self.advertising && self.conn.is_none() {
            let due = match self.last_adv_us {
                None => true,
                Some(t) => now >= t + self.adv_interval_us.max(100_000),
            };
            if due {
                self.last_adv_us = Some(now);
                let pdu = match self.adv_type {
                    0 => "adv_ind",
                    2 => "adv_scan_ind",
                    _ => "adv_nonconn_ind",
                };
                let m = AirMsg::Adv {
                    addr: self.address(),
                    pdu: pdu.into(),
                    data: self.adv_data.clone(),
                    scan_rsp: self.sr_data.clone(),
                };
                self.send(m);
            }
        }
        for _ in 0..64 {
            let m = match self.air.as_mut().and_then(|a| a.recv()) {
                Some(m) => m,
                None => break,
            };
            self.on_air(m, h);
        }
    }

    fn on_air(&mut self, m: AirMsg, h: &mut dyn Host) {
        let me = self.address();
        match m {
            AirMsg::ConnectInd {
                initiator,
                advertiser,
                interval,
                latency,
                timeout,
            } => {
                if advertiser != me
                    || !self.advertising
                    || self.conn.is_some()
                    || self.adv_type != BLE_GAP_ADV_TYPE_ADV_IND
                {
                    return;
                }
                self.advertising = false;
                let peer_addr = parse_addr(&initiator);
                let peer_type = if initiator.ends_with("/P") { 0 } else { 1 };
                self.conn = Some(Conn {
                    peer: initiator,
                    peer_addr,
                    peer_type,
                    encrypted: false,
                    mtu: 23,
                    smp: Smp::default(),
                    pending_enc: None,
                });
                // ble_gap_evt_t{conn_handle}, ble_gap_evt_connected_t: peer_addr(7) @0, own_addr(7) @7,
                // irk byte @14, conn_params @16 (min, max, latency, timeout).
                let mut body = 0u16.to_le_bytes().to_vec();
                let mut p = vec![0u8; 24];
                p[0] = if peer_type == 0 {
                    BLE_GAP_ADDR_TYPE_PUBLIC
                } else {
                    BLE_GAP_ADDR_TYPE_RANDOM_STATIC
                };
                p[1..7].copy_from_slice(&peer_addr);
                p[7] = BLE_GAP_ADDR_TYPE_RANDOM_STATIC;
                p[8..14].copy_from_slice(&self.cfg.addr);
                p[16..18].copy_from_slice(&interval.to_le_bytes());
                p[18..20].copy_from_slice(&interval.to_le_bytes());
                p[20..22].copy_from_slice(&latency.to_le_bytes());
                p[22..24].copy_from_slice(&timeout.to_le_bytes());
                body.extend(p);
                self.push_ble_evt(BLE_GAP_EVT_CONNECTED, body, h);
                self.log.push("connected".into());
            }
            AirMsg::Ll {
                src,
                dst,
                op,
                error_code,
                rand,
                ediv,
                ltk,
            } => {
                if dst != me || self.conn.as_ref().map(|c| c.peer != src).unwrap_or(true) {
                    return;
                }
                match op.as_str() {
                    "terminate_ind" => {
                        self.conn = None;
                        let mut body = 0u16.to_le_bytes().to_vec();
                        body.push(error_code);
                        self.push_ble_evt(BLE_GAP_EVT_DISCONNECTED, body, h);
                    }
                    "enc_req" => self.on_enc_req(rand, ediv, ltk, h),
                    "feature_req" => {
                        self.send(AirMsg::Ll {
                            src: me,
                            dst: src,
                            op: "feature_rsp".into(),
                            error_code: 0,
                            rand: vec![],
                            ediv: 0,
                            ltk: vec![],
                        });
                    }
                    _ => {}
                }
            }
            AirMsg::Acl { src, dst, data } => {
                if dst != me
                    || self.conn.as_ref().map(|c| c.peer != src).unwrap_or(true)
                    || data.len() < 4
                {
                    return;
                }
                let cid = u16::from_le_bytes([data[2], data[3]]);
                let payload = data[4..].to_vec();
                match cid {
                    4 => self.on_att(payload, h),
                    6 => self.on_smp(payload, h),
                    5 => {
                        // LE signalling: answer requests with Command Reject (not understood).
                        if payload
                            .first()
                            .map(|c| c & 1 == 0 && *c != 0x13)
                            .unwrap_or(false)
                            && payload.len() >= 2
                        {
                            self.send_l2cap(5, vec![0x01, payload[1], 2, 0, 0, 0]);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn on_att(&mut self, pdu: Vec<u8>, h: &mut dyn Host) {
        let (enc, mut mtu) = match self.conn.as_ref() {
            Some(c) => (c.encrypted, c.mtu),
            None => return,
        };
        let (rsp, effects) = self.gatt.att(&pdu, &mut mtu, enc, h);
        if let Some(c) = self.conn.as_mut() {
            c.mtu = mtu;
        }
        if let Some(r) = rsp {
            self.send_att(r);
        }
        for e in effects {
            match e {
                AttEffect::Write {
                    handle,
                    op,
                    offset,
                    data,
                } => {
                    let Some(at) = self.gatt.get(handle).cloned() else {
                        continue;
                    };
                    // ble_gatts_evt_t{conn_handle @0}, ble_gatts_evt_write_t @2: handle @0, op @2,
                    // context @4 {srvc_uuid @0, char_uuid @4, desc_uuid @8, srvc_handle @12,
                    // value_handle @14, type @16}, offset @22, len @24, data @26.
                    let mut w = vec![0u8; 26];
                    w[0..2].copy_from_slice(&handle.to_le_bytes());
                    w[2] = op;
                    w[4..6].copy_from_slice(&at.srvc_uuid.0.to_le_bytes());
                    w[6] = at.srvc_uuid.1;
                    w[8..10].copy_from_slice(&at.char_uuid.0.to_le_bytes());
                    w[10] = at.char_uuid.1;
                    if at.kind == 6 {
                        w[12..14].copy_from_slice(&at.app_uuid.0.to_le_bytes());
                        w[14] = at.app_uuid.1;
                    }
                    w[16..18].copy_from_slice(&at.srvc_handle.to_le_bytes());
                    w[18..20].copy_from_slice(&at.char_value_handle.to_le_bytes());
                    w[20] = at.kind;
                    w[22..24].copy_from_slice(&offset.to_le_bytes());
                    w[24..26].copy_from_slice(&(data.len() as u16).to_le_bytes());
                    w.extend(&data);
                    let mut body = 0u16.to_le_bytes().to_vec();
                    body.extend(w);
                    self.push_ble_evt(BLE_GATTS_EVT_WRITE, body, h);
                }
                AttEffect::Confirm { .. } => {
                    let mut body = 0u16.to_le_bytes().to_vec();
                    body.extend(0u16.to_le_bytes());
                    self.push_ble_evt(BLE_GATTS_EVT_HVC, body, h);
                }
            }
        }
    }

    fn on_smp(&mut self, pdu: Vec<u8>, h: &mut dyn Host) {
        let Some(code) = pdu.first().copied() else {
            return;
        };
        match code {
            0x01 if pdu.len() >= 7 => {
                let c = self.conn.as_mut().unwrap();
                c.smp = Smp {
                    preq: pdu[..7].to_vec(),
                    waiting_app_reply: true,
                    ..Smp::default()
                };
                // ble_gap_evt_sec_params_request_t: peer_params (ble_gap_sec_params_t).
                let auth = pdu[3];
                let b0 = (auth & 1)
                    | (((auth >> 2) & 1) << 1)
                    | ((pdu[1] & 7) << 2)
                    | ((pdu[2] & 1) << 5);
                let mut body = 0u16.to_le_bytes().to_vec();
                body.extend([b0, 7, pdu[4], pdu[6] & 7, pdu[5] & 7]);
                self.push_ble_evt(BLE_GAP_EVT_SEC_PARAMS_REQUEST, body, h);
            }
            0x03 if pdu.len() >= 17 => {
                let srand = self.rand_block();
                let me = self.cfg.addr;
                let c = self.conn.as_mut().unwrap();
                c.smp.mconfirm.copy_from_slice(&pdu[1..17]);
                c.smp.srand = srand;
                let tk = [0u8; 16];
                let sconfirm = c1(
                    &tk,
                    &srand,
                    &c.smp.preq,
                    &c.smp.pres,
                    c.peer_type,
                    1,
                    &c.peer_addr,
                    &me,
                );
                let mut out = vec![0x03];
                out.extend(sconfirm);
                self.send_smp(out);
            }
            0x04 if pdu.len() >= 17 => {
                let me = self.cfg.addr;
                let c = self.conn.as_mut().unwrap();
                let mut mrand = [0u8; 16];
                mrand.copy_from_slice(&pdu[1..17]);
                let tk = [0u8; 16];
                let check = c1(
                    &tk,
                    &mrand,
                    &c.smp.preq,
                    &c.smp.pres,
                    c.peer_type,
                    1,
                    &c.peer_addr,
                    &me,
                );
                if check != c.smp.mconfirm {
                    self.send_smp(vec![0x05, 0x04]);
                    self.auth_status(0x84, h);
                    return;
                }
                c.smp.stk = s1(&tk, &c.smp.srand, &mrand);
                let mut out = vec![0x04];
                out.extend(c.smp.srand);
                self.send_smp(out);
            }
            0x05 => {
                let reason = pdu.get(1).copied().unwrap_or(8);
                self.auth_status(0x80 | reason, h);
            }
            0x06 | 0x07 | 0x08 | 0x09 | 0x0A => {
                // Keys from the central: Encryption Info, Master Id, Identity Info, Identity Address, Signing Info.
                let ks = self.conn.as_ref().unwrap().smp.keyset;
                // keys_central @12: p_enc_key @12, p_id_key @16, p_sign_key @20.
                if ks != 0 {
                    match code {
                        0x06 => {
                            if let Some(p) = h.r32(ks + 12).filter(|p| *p != 0) {
                                h.write(p, &pdu[1..17]);
                                h.w8(p + 16, 16 << 1);
                            }
                        }
                        0x07 => {
                            if let Some(p) = h.r32(ks + 12).filter(|p| *p != 0) {
                                h.write(p + 18, &pdu[1..11]);
                            }
                        }
                        0x08 => {
                            if let Some(p) = h.r32(ks + 16).filter(|p| *p != 0) {
                                h.write(p, &pdu[1..17]);
                            }
                        }
                        0x09 => {
                            if let Some(p) = h.r32(ks + 16).filter(|p| *p != 0) {
                                h.write(p + 16, &pdu[1..8]);
                            }
                        }
                        _ => {}
                    }
                }
                let c = self.conn.as_mut().unwrap();
                let bit = match code {
                    0x07 => 1,
                    0x09 => 2,
                    0x0A => 4,
                    _ => 0,
                };
                c.smp.got_central_keys |= bit;
                if c.smp.got_central_keys & c.smp.expect_central_keys == c.smp.expect_central_keys
                    && bit != 0
                {
                    self.auth_status(0, h);
                }
            }
            _ => {
                self.send_smp(vec![0x05, 0x07]); // Command Not Supported
            }
        }
    }

    fn on_enc_req(&mut self, rand: Vec<u8>, ediv: u16, ltk: Vec<u8>, h: &mut dyn Host) {
        let me = self.address();
        let c = self.conn.as_mut().unwrap();
        let peer = c.peer.clone();
        let mut want = [0u8; 16];
        if ltk.len() == 16 {
            want.copy_from_slice(&ltk);
        }
        if ediv == 0 && rand.iter().all(|b| *b == 0) && c.smp.stk != [0u8; 16] {
            // Encryption with the STK of a pairing that just ran.
            if want != c.smp.stk {
                self.send(AirMsg::Ll {
                    src: me,
                    dst: peer,
                    op: "reject_ext_ind".into(),
                    error_code: 0x06,
                    rand: vec![],
                    ediv: 0,
                    ltk: vec![],
                });
                return;
            }
            c.encrypted = true;
            self.send(AirMsg::Ll {
                src: me.clone(),
                dst: peer,
                op: "start_enc_rsp".into(),
                error_code: 0,
                rand: vec![],
                ediv: 0,
                ltk: vec![],
            });
            // ble_gap_evt_conn_sec_update_t: conn_sec {sec_mode sm|lv<<4, encr_key_size}.
            self.push_ble_evt(BLE_GAP_EVT_CONN_SEC_UPDATE, vec![0, 0, 0x21, 16], h);
            self.distribute_keys(h);
        } else {
            // Re-encryption with a bonded LTK: ask the app (BLE_GAP_EVT_SEC_INFO_REQUEST).
            let mut r8 = [0u8; 8];
            if rand.len() == 8 {
                r8.copy_from_slice(&rand);
            }
            c.pending_enc = Some((r8, ediv, want));
            // ble_gap_evt_sec_info_request_t: peer_addr(7) @0, master_id {ediv @8, rand @10}, flags @18.
            let mut p = vec![0u8; 20];
            p[0] = BLE_GAP_ADDR_TYPE_RANDOM_STATIC;
            p[1..7].copy_from_slice(&c.peer_addr);
            p[8..10].copy_from_slice(&ediv.to_le_bytes());
            p[10..18].copy_from_slice(&r8);
            p[18] = 1;
            let mut body = 0u16.to_le_bytes().to_vec();
            body.extend(p);
            self.push_ble_evt(BLE_GAP_EVT_SEC_INFO_REQUEST, body, h);
        }
    }

    fn distribute_keys(&mut self, h: &mut dyn Host) {
        let ltk = self.rand_block();
        let ediv = u16::from_le_bytes([self.rand8(), self.rand8()]);
        let mut rand = [0u8; 8];
        for x in rand.iter_mut() {
            *x = self.rand8();
        }
        let irk = self.rand_block();
        let me = self.cfg.addr;
        let c = self.conn.as_mut().unwrap();
        let (kd, ks) = (c.smp.resp_kdist, c.smp.keyset);
        c.smp.ltk = ltk;
        c.smp.ediv = ediv;
        c.smp.rand = rand;
        if kd & 1 != 0 {
            let mut m = vec![0x06];
            m.extend(ltk);
            self.send_smp(m);
            let mut m = vec![0x07];
            m.extend(ediv.to_le_bytes());
            m.extend(rand);
            self.send_smp(m);
            // keys_periph.p_enc_key @0: ble_gap_enc_key_t {ltk @0, auth|len<<1 @16, ediv @18, rand @20}.
            if ks != 0 {
                if let Some(p) = h.r32(ks).filter(|p| *p != 0) {
                    h.write(p, &ltk);
                    h.w8(p + 16, 16 << 1);
                    h.w16(p + 18, ediv);
                    h.write(p + 20, &rand);
                }
            }
        }
        if kd & 2 != 0 {
            let mut m = vec![0x08];
            m.extend(irk);
            self.send_smp(m);
            let mut m = vec![0x09, BLE_GAP_ADDR_TYPE_RANDOM_STATIC];
            m.extend(me);
            self.send_smp(m);
        }
        let c = self.conn.as_ref().unwrap();
        if c.smp.expect_central_keys == 0 {
            self.auth_status(0, h);
        }
    }

    fn auth_status(&mut self, status: u8, h: &mut dyn Host) {
        let Some(c) = self.conn.as_ref() else { return };
        // ble_gap_evt_auth_status_t: auth_status @0, error_src|bonded<<2 @1, sm1_levels @2,
        // sm2_levels @3, kdist_periph @4, kdist_central @5.
        let ok = status == 0;
        let b1 = if ok { (c.smp.bond as u8) << 2 } else { 1 };
        let body = vec![
            0,
            0,
            status,
            b1,
            if ok { 0x03 } else { 0x01 },
            0,
            if ok { c.smp.resp_kdist } else { 0 },
            if ok { c.smp.init_kdist } else { 0 },
        ];
        self.push_ble_evt(BLE_GAP_EVT_AUTH_STATUS, body, h);
    }

    /// Bytes the application reads from the (unmapped) SoftDevice/MBR range.
    /// None: the backend answers with erased flash (0xFF).
    pub fn hole_read(&self, _addr: u32) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn c1_matches_core_spec_sample() {
        // Core Spec Vol 3 Part H 2.2.3 example (values given MSB first there).
        let k = [0u8; 16];
        let r = hexrev("5783D52156AD6F0E6388274EC6702EE0");
        let preq = hexrev("07071000000101");
        let pres = hexrev("05000800000302");
        let ia = hexrev("A1A2A3A4A5A6");
        let ra = hexrev("B1B2B3B4B5B6");
        let want = hexrev("1e1e3fef878988ead2a74dc5bef13b86");
        let got = c1(
            &k,
            r.as_slice().try_into().unwrap(),
            &preq,
            &pres,
            1,
            0,
            ia.as_slice().try_into().unwrap(),
            ra.as_slice().try_into().unwrap(),
        );
        assert_eq!(got.to_vec(), want);
    }
    #[test]
    fn s1_matches_core_spec_sample() {
        let k = [0u8; 16];
        let r1 = hexrev("000F0E0D0C0B0A091122334455667788");
        let r2 = hexrev("010203040506070899AABBCCDDEEFF00");
        let want = hexrev("9a1fe1f0e8b0f49b5b4216ae796da062");
        assert_eq!(
            s1(
                &k,
                r1.as_slice().try_into().unwrap(),
                r2.as_slice().try_into().unwrap()
            )
            .to_vec(),
            want
        );
    }
    fn hexrev(s: &str) -> Vec<u8> {
        let mut v: Vec<u8> = (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        v.reverse();
        v
    }
}
