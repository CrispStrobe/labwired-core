// SPDX-License-Identifier: MIT
//! The GATT server the SoftDevice keeps for the application: the attribute
//! table built by sd_ble_gatts_*_add, and the ATT protocol served to a peer
//! (Bluetooth Core Spec Vol 3 Part F/G) [S].

use crate::host::{Host, HostExt};

pub const UUID_PRIMARY: u16 = 0x2800;
pub const UUID_CHAR: u16 = 0x2803;
pub const UUID_CCCD: u16 = 0x2902;

/// An attribute type: 16-bit SIG UUID or a full 128-bit UUID (little endian).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uuid {
    U16(u16),
    U128([u8; 16]),
}
impl Uuid {
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Uuid::U16(u) => u.to_le_bytes().to_vec(),
            Uuid::U128(b) => b.to_vec(),
        }
    }
    pub fn from_bytes(b: &[u8]) -> Option<Uuid> {
        match b.len() {
            2 => Some(Uuid::U16(u16::from_le_bytes([b[0], b[1]]))),
            16 => {
                let mut a = [0u8; 16];
                a.copy_from_slice(b);
                // A 128-bit form of a SIG UUID compares equal to its 16-bit form.
                const BASE: [u8; 12] = [
                    0xFB, 0x34, 0x9B, 0x5F, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00,
                ];
                if a[..12] == BASE && a[14] == 0 && a[15] == 0 {
                    return Some(Uuid::U16(u16::from_le_bytes([a[12], a[13]])));
                }
                Some(Uuid::U128(a))
            }
            _ => None,
        }
    }
}

/// Where an attribute value lives: in the SoftDevice (copied) or in app RAM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    Stack(Vec<u8>),
    /// BLE_GATTS_VLOC_USER: the value is the app's memory at `addr`; its
    /// current length is kept here.
    User {
        addr: u32,
        len: u16,
    },
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub handle: u16,
    pub uuid: Uuid,
    /// The app's ble_uuid_t (uuid16, type) for events.
    pub app_uuid: (u16, u8),
    pub kind: u8, // BLE_GATTS_ATTR_TYPE_*
    pub value: Store,
    pub max_len: u16,
    pub vlen: bool,
    /// ble_gap_conn_sec_mode_t bytes (sm | lv << 4).
    pub read_perm: u8,
    pub write_perm: u8,
    pub rd_auth: bool,
    pub wr_auth: bool,
    pub srvc_handle: u16,
    pub char_value_handle: u16,
    pub srvc_uuid: (u16, u8),
    pub char_uuid: (u16, u8),
}

#[derive(Default)]
pub struct GattDb {
    pub attrs: Vec<Attr>,
    /// Handles of the service declarations, in order.
    pub services: Vec<u16>,
}

/// What an ATT request did, for the SoftDevice to turn into app events.
#[derive(Debug, Clone, PartialEq)]
pub enum AttEffect {
    Write {
        handle: u16,
        op: u8,
        offset: u16,
        data: Vec<u8>,
    },
    Confirm {
        handle: u16,
    },
}

fn perm_ok(perm: u8, encrypted: bool) -> Result<(), u8> {
    let sm = perm & 0x0F;
    let lv = perm >> 4;
    if sm == 0 && lv == 0 {
        return Err(0x02); // no access: Read/Write Not Permitted (caller maps)
    }
    if lv >= 2 && !encrypted {
        return Err(0x05); // Insufficient Authentication
    }
    Ok(())
}

impl GattDb {
    pub fn next_handle(&self) -> u16 {
        self.attrs.last().map(|a| a.handle + 1).unwrap_or(1)
    }
    pub fn get(&self, h: u16) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.handle == h)
    }
    pub fn get_mut(&mut self, h: u16) -> Option<&mut Attr> {
        self.attrs.iter_mut().find(|a| a.handle == h)
    }
    pub fn read_value(&self, h: u16, host: &mut dyn Host) -> Option<Vec<u8>> {
        let a = self.get(h)?;
        Some(match &a.value {
            Store::Stack(v) => v.clone(),
            Store::User { addr, len } => host.rbuf(*addr, *len as usize).unwrap_or_default(),
        })
    }
    /// Set a value (app side, sd_ble_gatts_value_set, or a peer write).
    pub fn write_value(
        &mut self,
        h: u16,
        offset: u16,
        data: &[u8],
        host: &mut dyn Host,
    ) -> Option<u16> {
        let a = self.get_mut(h)?;
        let end = offset as usize + data.len();
        if end > a.max_len as usize {
            return None;
        }
        match &mut a.value {
            Store::Stack(v) => {
                if v.len() < end {
                    v.resize(end, 0);
                }
                v[offset as usize..end].copy_from_slice(data);
                if a.vlen {
                    v.truncate(end);
                }
                Some(v.len() as u16)
            }
            Store::User { addr, len } => {
                host.write(*addr + offset as u32, data);
                if a.vlen || end as u16 > *len {
                    *len = end as u16;
                }
                Some(*len)
            }
        }
    }

    fn end_of_group(&self, svc: u16) -> u16 {
        let i = self.services.iter().position(|&s| s == svc).unwrap_or(0);
        match self.services.get(i + 1) {
            Some(n) => n - 1,
            None => self.attrs.last().map(|a| a.handle).unwrap_or(svc),
        }
    }

    /// Serve one ATT PDU from the peer. Returns (response PDU, effects).
    pub fn att(
        &mut self,
        pdu: &[u8],
        mtu: &mut u16,
        encrypted: bool,
        host: &mut dyn Host,
    ) -> (Option<Vec<u8>>, Vec<AttEffect>) {
        let err = |op: u8, h: u16, code: u8| Some(vec![0x01, op, h as u8, (h >> 8) as u8, code]);
        let op = match pdu.first() {
            Some(&o) => o,
            None => return (None, vec![]),
        };
        let u16at = |i: usize| -> u16 {
            pdu.get(i..i + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .unwrap_or(0)
        };
        match op {
            0x02 => {
                // Exchange MTU Request; the S110 serves ATT_MTU 23.
                *mtu = 23;
                (Some(vec![0x03, 23, 0]), vec![])
            }
            0x04 => {
                // Find Information
                let (s, e) = (u16at(1), u16at(3));
                let mut out = vec![0x05, 0];
                let mut fmt = 0;
                for a in self.attrs.iter().filter(|a| a.handle >= s && a.handle <= e) {
                    let f = if matches!(a.uuid, Uuid::U16(_)) { 1 } else { 2 };
                    if fmt == 0 {
                        fmt = f;
                    }
                    if f != fmt || out.len() + 2 + a.uuid.bytes().len() > *mtu as usize {
                        break;
                    }
                    out.extend(a.handle.to_le_bytes());
                    out.extend(a.uuid.bytes());
                }
                if fmt == 0 {
                    return (err(op, s, 0x0A), vec![]);
                }
                out[1] = fmt;
                (Some(out), vec![])
            }
            0x06 => {
                // Find By Type Value (primary service by UUID)
                let (s, e, ty) = (u16at(1), u16at(3), u16at(5));
                let val = &pdu[7.min(pdu.len())..];
                let mut out = vec![0x07];
                for a in self
                    .attrs
                    .iter()
                    .filter(|a| a.handle >= s && a.handle <= e && a.uuid == Uuid::U16(ty))
                {
                    if self.read_value(a.handle, host).as_deref() == Some(val) {
                        let end = if ty == UUID_PRIMARY {
                            self.end_of_group(a.handle)
                        } else {
                            a.handle
                        };
                        out.extend(a.handle.to_le_bytes());
                        out.extend(end.to_le_bytes());
                        if out.len() + 4 > *mtu as usize {
                            break;
                        }
                    }
                }
                if out.len() == 1 {
                    return (err(op, s, 0x0A), vec![]);
                }
                (Some(out), vec![])
            }
            0x08 | 0x10 => {
                // Read By Type / Read By Group Type
                let (s, e) = (u16at(1), u16at(3));
                let ty = match Uuid::from_bytes(&pdu[5.min(pdu.len())..]) {
                    Some(u) => u,
                    None => return (err(op, s, 0x04), vec![]),
                };
                let mut out = vec![op + 1, 0];
                let mut item_len = 0usize;
                let handles: Vec<u16> = self
                    .attrs
                    .iter()
                    .filter(|a| a.handle >= s && a.handle <= e && a.uuid == ty)
                    .map(|a| a.handle)
                    .collect();
                for h in handles {
                    let a = self.get(h).unwrap().clone();
                    if let Err(c) = perm_ok(a.read_perm, encrypted) {
                        if out.len() == 2 {
                            return (err(op, h, if c == 0x02 { 0x02 } else { c }), vec![]);
                        }
                        break;
                    }
                    let v = self.read_value(h, host).unwrap_or_default();
                    let mut item = h.to_le_bytes().to_vec();
                    if op == 0x10 {
                        item.extend(self.end_of_group(h).to_le_bytes());
                    }
                    let room = (*mtu as usize).saturating_sub(2 + item.len()).min(253);
                    item.extend(&v[..v.len().min(room)]);
                    if item_len == 0 {
                        item_len = item.len();
                    }
                    if item.len() != item_len || out.len() + item.len() > *mtu as usize {
                        break;
                    }
                    out.extend(item);
                }
                if item_len == 0 {
                    return (err(op, s, 0x0A), vec![]);
                }
                out[1] = item_len as u8;
                (Some(out), vec![])
            }
            0x0A | 0x0C => {
                // Read / Read Blob
                let h = u16at(1);
                let off = if op == 0x0C { u16at(3) as usize } else { 0 };
                let a = match self.get(h) {
                    Some(a) => a.clone(),
                    None => return (err(op, h, 0x01), vec![]),
                };
                if let Err(c) = perm_ok(a.read_perm, encrypted) {
                    return (err(op, h, c), vec![]);
                }
                let v = self.read_value(h, host).unwrap_or_default();
                if off > v.len() {
                    return (err(op, h, 0x07), vec![]);
                }
                let mut out = vec![op + 1];
                out.extend(&v[off..v.len().min(off + *mtu as usize - 1)]);
                (Some(out), vec![])
            }
            0x12 | 0x52 => {
                // Write Request / Write Command
                let h = u16at(1);
                let data = pdu[3.min(pdu.len())..].to_vec();
                let a = match self.get(h) {
                    Some(a) => a.clone(),
                    None => return (if op == 0x12 { err(op, h, 0x01) } else { None }, vec![]),
                };
                if let Err(c) = perm_ok(a.write_perm, encrypted) {
                    return (
                        if op == 0x12 {
                            err(op, h, if c == 0x02 { 0x03 } else { c })
                        } else {
                            None
                        },
                        vec![],
                    );
                }
                if self.write_value(h, 0, &data, host).is_none() {
                    return (if op == 0x12 { err(op, h, 0x0D) } else { None }, vec![]);
                }
                let eff = vec![AttEffect::Write {
                    handle: h,
                    op: if op == 0x12 { 1 } else { 2 },
                    offset: 0,
                    data,
                }];
                (if op == 0x12 { Some(vec![0x13]) } else { None }, eff)
            }
            0x1E => (None, vec![AttEffect::Confirm { handle: 0 }]),
            _ => {
                if op & 0x40 != 0 {
                    (None, vec![]) // unknown command: ignored
                } else {
                    (err(op, 0, 0x06), vec![])
                }
            }
        }
    }

    /// The CCCD of the characteristic whose value handle is `vh`.
    pub fn cccd_of(&self, vh: u16) -> Option<u16> {
        let a = self.get(vh)?;
        let svc = a.srvc_handle;
        let end = self.end_of_group(svc);
        let next_char = self
            .attrs
            .iter()
            .filter(|x| x.handle > vh && x.handle <= end && x.uuid == Uuid::U16(UUID_CHAR))
            .map(|x| x.handle)
            .min()
            .unwrap_or(end + 1);
        self.attrs
            .iter()
            .find(|x| x.handle > vh && x.handle < next_char && x.uuid == Uuid::U16(UUID_CCCD))
            .map(|x| x.handle)
    }
}
