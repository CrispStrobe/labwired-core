// SPDX-License-Identifier: MIT
//! The shared virtual air ("bw-air/1"). The contract (AIR.md) and the hub
//! (airhub.py) live in one place: renode-spike-prime `tools/bw-air/`.
//!
//! One newline-delimited JSON object per message, over TCP to the air hub. BLE travels at the level of
//! Google bumble's LocalLink (advertising PDUs, LL control PDUs, L2CAP
//! frames between addresses) so a bumble controller — and through it any
//! HCI host, e.g. the SPIKE hub's stack — shares the air with an emulated
//! SoftDevice. Raw 2.4 GHz frames (nRF RADIO, MakeCode radio) travel beside
//! them with the logical address and the RAM packet image.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum AirMsg {
    /// A node announces itself (first message on a connection).
    Hello { node: String, kind: String },
    /// Legacy advertising PDU. `pdu`: "adv_ind" | "adv_nonconn_ind" | "adv_scan_ind".
    Adv {
        addr: String,
        pdu: String,
        data: Vec<u8>,
        scan_rsp: Vec<u8>,
    },
    /// CONNECT_IND from `initiator` to `advertiser`.
    ConnectInd {
        initiator: String,
        advertiser: String,
        interval: u16,
        latency: u16,
        timeout: u16,
    },
    /// An L2CAP basic frame (length, CID, payload) from `src` to `dst`.
    Acl {
        src: String,
        dst: String,
        data: Vec<u8>,
    },
    /// LL control PDU. op: "terminate_ind" (error_code), "enc_req" (rand, ediv, ltk),
    /// "start_enc_rsp", "reject_ext_ind" (reject_opcode, error_code), "feature_req", "feature_rsp".
    Ll {
        src: String,
        dst: String,
        op: String,
        error_code: u8,
        rand: Vec<u8>,
        ediv: u16,
        ltk: Vec<u8>,
    },
    /// Raw radio frame: nRF RADIO FREQUENCY, MODE, the on-air logical address
    /// (BASE | PREFIX << (8*BALEN)) and the packet image as it sits in RAM at PACKETPTR.
    Raw {
        src: String,
        freq: u8,
        mode: u8,
        address: u32,
        balen: u8,
        packet: Vec<u8>,
        tx_power: i8,
    },
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

impl AirMsg {
    pub fn to_json(&self) -> String {
        match self {
            AirMsg::Hello { node, kind } => format!(
                r#"{{"t":"hello","node":"{}","kind":"{}","proto":"bw-air/1"}}"#,
                esc(node),
                esc(kind)
            ),
            AirMsg::Adv {
                addr,
                pdu,
                data,
                scan_rsp,
            } => format!(
                r#"{{"t":"adv","addr":"{}","pdu":"{}","data":"{}","scan_rsp":"{}"}}"#,
                addr,
                pdu,
                hex(data),
                hex(scan_rsp)
            ),
            AirMsg::ConnectInd {
                initiator,
                advertiser,
                interval,
                latency,
                timeout,
            } => format!(
                r#"{{"t":"connect_ind","initiator":"{}","advertiser":"{}","interval":{},"latency":{},"timeout":{}}}"#,
                initiator, advertiser, interval, latency, timeout
            ),
            AirMsg::Acl { src, dst, data } => format!(
                r#"{{"t":"acl","src":"{}","dst":"{}","data":"{}"}}"#,
                src,
                dst,
                hex(data)
            ),
            AirMsg::Ll {
                src,
                dst,
                op,
                error_code,
                rand,
                ediv,
                ltk,
            } => format!(
                r#"{{"t":"ll","src":"{}","dst":"{}","op":"{}","error_code":{},"rand":"{}","ediv":{},"ltk":"{}"}}"#,
                src,
                dst,
                op,
                error_code,
                hex(rand),
                ediv,
                hex(ltk)
            ),
            AirMsg::Raw {
                src,
                freq,
                mode,
                address,
                balen,
                packet,
                tx_power,
            } => format!(
                r#"{{"t":"raw","src":"{}","freq":{},"mode":{},"address":{},"balen":{},"packet":"{}","tx_power":{}}}"#,
                esc(src),
                freq,
                mode,
                address,
                balen,
                hex(packet),
                tx_power
            ),
        }
    }

    pub fn from_json(line: &str) -> Option<AirMsg> {
        let o = flat_json(line)?;
        let s = |k: &str| {
            o.iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let n = |k: &str| s(k).parse::<i64>().unwrap_or(0);
        Some(match s("t").as_str() {
            "hello" => AirMsg::Hello {
                node: s("node"),
                kind: s("kind"),
            },
            "adv" => AirMsg::Adv {
                addr: s("addr"),
                pdu: s("pdu"),
                data: unhex(&s("data")),
                scan_rsp: unhex(&s("scan_rsp")),
            },
            "connect_ind" => AirMsg::ConnectInd {
                initiator: s("initiator"),
                advertiser: s("advertiser"),
                interval: n("interval") as u16,
                latency: n("latency") as u16,
                timeout: n("timeout") as u16,
            },
            "acl" => AirMsg::Acl {
                src: s("src"),
                dst: s("dst"),
                data: unhex(&s("data")),
            },
            "ll" => AirMsg::Ll {
                src: s("src"),
                dst: s("dst"),
                op: s("op"),
                error_code: n("error_code") as u8,
                rand: unhex(&s("rand")),
                ediv: n("ediv") as u16,
                ltk: unhex(&s("ltk")),
            },
            "raw" => AirMsg::Raw {
                src: s("src"),
                freq: n("freq") as u8,
                mode: n("mode") as u8,
                address: n("address") as u32,
                balen: n("balen") as u8,
                packet: unhex(&s("packet")),
                tx_power: n("tx_power") as i8,
            },
            _ => return None,
        })
    }
}

/// Parse one flat JSON object of string / number / bool / null values.
pub fn flat_json(line: &str) -> Option<Vec<(String, String)>> {
    let b = line.trim().as_bytes();
    if b.first() != Some(&b'{') {
        return None;
    }
    let mut i = 1;
    let mut out = Vec::new();
    let ws = |i: &mut usize| {
        while *i < b.len() && (b[*i] as char).is_whitespace() {
            *i += 1
        }
    };
    let string = |i: &mut usize| -> Option<String> {
        if b.get(*i) != Some(&b'"') {
            return None;
        }
        *i += 1;
        let mut s = String::new();
        while *i < b.len() && b[*i] != b'"' {
            if b[*i] == b'\\' && *i + 1 < b.len() {
                *i += 1;
            }
            s.push(b[*i] as char);
            *i += 1;
        }
        *i += 1;
        Some(s)
    };
    loop {
        ws(&mut i);
        if b.get(i) == Some(&b'}') {
            break;
        }
        let k = string(&mut i)?;
        ws(&mut i);
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        ws(&mut i);
        let v = if b.get(i) == Some(&b'"') {
            string(&mut i)?
        } else {
            let st = i;
            while i < b.len() && b[i] != b',' && b[i] != b'}' {
                i += 1
            }
            String::from_utf8_lossy(&b[st..i]).trim().to_string()
        };
        out.push((k, v));
        ws(&mut i);
        match b.get(i) {
            Some(b',') => i += 1,
            Some(b'}') => break,
            _ => return None,
        }
    }
    Some(out)
}

/// A node's port onto the air.
pub trait Air: Send {
    fn send(&mut self, m: &AirMsg);
    fn recv(&mut self) -> Option<AirMsg>;
}

/// TCP client to the air hub. Non-blocking receive; a dead hub drops messages
/// (the node keeps running, as a radio would with nobody listening).
pub struct TcpAir {
    stream: Option<TcpStream>,
    reader: Option<BufReader<TcpStream>>,
    pending: String,
}

impl TcpAir {
    pub fn connect(addr: &str, node: &str, kind: &str) -> std::io::Result<TcpAir> {
        let s = TcpStream::connect(addr)?;
        s.set_nodelay(true).ok();
        let r = s.try_clone()?;
        r.set_nonblocking(true)?;
        let mut a = TcpAir {
            stream: Some(s),
            reader: Some(BufReader::new(r)),
            pending: String::new(),
        };
        a.send(&AirMsg::Hello {
            node: node.to_string(),
            kind: kind.to_string(),
        });
        Ok(a)
    }
}

impl Air for TcpAir {
    fn send(&mut self, m: &AirMsg) {
        if let Some(s) = self.stream.as_mut() {
            let mut line = m.to_json();
            line.push('\n');
            if s.write_all(line.as_bytes()).is_err() {
                self.stream = None;
            }
        }
    }
    fn recv(&mut self) -> Option<AirMsg> {
        let r = self.reader.as_mut()?;
        loop {
            let mut chunk = String::new();
            match r.read_line(&mut chunk) {
                Ok(0) => return None,
                Ok(_) => {
                    self.pending.push_str(&chunk);
                    if self.pending.ends_with('\n') {
                        let line = std::mem::take(&mut self.pending);
                        if let Some(m) = AirMsg::from_json(&line) {
                            return Some(m);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return None,
                Err(_) => return None,
            }
        }
    }
}

/// In-process air for tests and single-process multi-node setups: every
/// message a port sends is delivered to every OTHER port.
#[derive(Clone, Default)]
pub struct MemAirBus {
    inner: Arc<Mutex<Vec<(usize, VecDeque<AirMsg>)>>>,
}
pub struct MemAir {
    bus: MemAirBus,
    id: usize,
}
impl MemAirBus {
    pub fn port(&self) -> MemAir {
        let mut g = self.inner.lock().unwrap();
        let id = g.len();
        g.push((id, VecDeque::new()));
        MemAir {
            bus: self.clone(),
            id,
        }
    }
}
impl Air for MemAir {
    fn send(&mut self, m: &AirMsg) {
        let mut g = self.bus.inner.lock().unwrap();
        for (id, q) in g.iter_mut() {
            if *id != self.id {
                q.push_back(m.clone());
            }
        }
    }
    fn recv(&mut self) -> Option<AirMsg> {
        self.bus.inner.lock().unwrap()[self.id].1.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_round_trip() {
        let ms = vec![
            AirMsg::Adv {
                addr: "C0:EE:AA:BB:CC:DD".into(),
                pdu: "adv_ind".into(),
                data: vec![2, 1, 6],
                scan_rsp: vec![],
            },
            AirMsg::ConnectInd {
                initiator: "F0:00:00:00:00:01".into(),
                advertiser: "C0:EE:AA:BB:CC:DD".into(),
                interval: 24,
                latency: 0,
                timeout: 400,
            },
            AirMsg::Acl {
                src: "a".into(),
                dst: "b".into(),
                data: vec![3, 0, 4, 0, 2, 0x17, 0],
            },
            AirMsg::Ll {
                src: "a".into(),
                dst: "b".into(),
                op: "enc_req".into(),
                error_code: 0,
                rand: vec![0; 8],
                ediv: 7,
                ltk: vec![1; 16],
            },
            AirMsg::Raw {
                src: "mb1".into(),
                freq: 7,
                mode: 0,
                address: 0x7575_7575,
                balen: 4,
                packet: vec![5, 1, 2],
                tx_power: 0,
            },
        ];
        for m in ms {
            assert_eq!(AirMsg::from_json(&m.to_json()), Some(m));
        }
    }
}
