// SPDX-License-Identifier: MIT
//! Runs every conformance vector (../conformance/*.json) against the Rust core
//! through the `Host` trait — the path the labwired backend links. The same
//! files run through the C ABI (Renode's path) and as guest firmware in both
//! emulators; see conformance/README.md.

use std::collections::HashMap;
use std::path::PathBuf;

use nrf_softdevice_hle::{Air, AirMsg, Config, Host, MemAirBus, SoftDevice};
use serde_json::Value;

/// Sparse guest memory with the Cortex-M NVIC set/clear register semantics.
pub struct MemHost {
    pub mem: HashMap<u32, u8>,
    iser: u32,
    ispr: u32,
    ipr: [u8; 32],
    pub now: u64,
}

impl MemHost {
    fn new() -> Self {
        MemHost {
            mem: HashMap::new(),
            iser: 0,
            ispr: 0,
            ipr: [0; 32],
            now: 0,
        }
    }
    fn mapped(a: u32) -> bool {
        (0x18000..0x3C000).contains(&a) || (0x2000_0000..0x2000_4000).contains(&a)
    }
    fn scs_read(&self, a: u32) -> Option<u32> {
        Some(match a {
            0xE000_E100 | 0xE000_E180 => self.iser,
            0xE000_E200 | 0xE000_E280 => self.ispr,
            0xE000_E400..=0xE000_E41C => u32::from_le_bytes(
                self.ipr[(a - 0xE000_E400) as usize..][..4]
                    .try_into()
                    .unwrap(),
            ),
            _ => return None,
        })
    }
}

impl Host for MemHost {
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> bool {
        if let Some(v) = self.scs_read(addr) {
            buf.copy_from_slice(&v.to_le_bytes()[..buf.len()]);
            return true;
        }
        for (i, b) in buf.iter_mut().enumerate() {
            let a = addr + i as u32;
            if !Self::mapped(a) {
                return false;
            }
            *b = *self.mem.get(&a).unwrap_or(&0);
        }
        true
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> bool {
        if (0xE000_E000..0xE000_F000).contains(&addr) {
            let v = u32::from_le_bytes(data.try_into().unwrap());
            match addr {
                0xE000_E100 => self.iser |= v,
                0xE000_E180 => self.iser &= !v,
                0xE000_E200 => self.ispr |= v,
                0xE000_E280 => self.ispr &= !v,
                0xE000_E400..=0xE000_E41C => {
                    self.ipr[(addr - 0xE000_E400) as usize..][..4].copy_from_slice(&v.to_le_bytes())
                }
                _ => {}
            }
            return true;
        }
        for (i, b) in data.iter().enumerate() {
            let a = addr + i as u32;
            if !Self::mapped(a) {
                return false;
            }
            self.mem.insert(a, *b);
        }
        true
    }
    fn now_us(&mut self) -> u64 {
        self.now
    }
}

fn num(v: &Value) -> u32 {
    match v {
        Value::Number(n) => n.as_u64().unwrap() as u32,
        Value::String(s) => u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap(),
        _ => panic!("not a number: {v}"),
    }
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// A subset match of a JSON object against an emitted air message.
fn matches(want: &Value, got: &AirMsg) -> bool {
    let g: Value = serde_json::from_str(&got.to_json()).unwrap();
    want.as_object()
        .unwrap()
        .iter()
        .all(|(k, v)| g.get(k) == Some(v))
}

fn run_vector(path: &PathBuf) -> Result<(), String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut h = MemHost::new();
    for m in v["memory"].as_array().unwrap() {
        let a = num(&m["addr"]);
        for (i, b) in unhex(m["hex"].as_str().unwrap()).iter().enumerate() {
            h.mem.insert(a + i as u32, *b);
        }
    }
    let bus = MemAirBus::default();
    let mut peer = bus.port();
    let mut sd = SoftDevice::new(Config::microbit_v1(
        "dut",
        [0xDD, 0xCC, 0xBB, 0xAA, 0xEE, 0xC0],
    ));
    sd.attach_air(Box::new(bus.port()));
    let mut out: Vec<AirMsg> = Vec::new();
    for (i, s) in v["steps"].as_array().unwrap().iter().enumerate() {
        let at = |what: String| format!("{} step {}: {}", v["name"], i, what);
        if let Some(w) = s.get("mem_write") {
            for m in w.as_array().unwrap() {
                h.write(num(&m["addr"]), &unhex(m["hex"].as_str().unwrap()));
            }
        }
        if let Some(m) = s.get("air_in") {
            peer.send(
                &AirMsg::from_json(&m.to_string()).ok_or_else(|| at(format!("bad air_in {m}")))?,
            );
        }
        if let Some(n) = s.get("svc") {
            let a: Vec<u32> = s["args"].as_array().unwrap().iter().map(num).collect();
            let r = sd.svc(num(n) as u8, [a[0], a[1], a[2], a[3]], &mut h);
            let want = num(&s["ret"]);
            if r != want {
                return Err(at(format!("svc {} returned {:#x}, want {:#x}", n, r, want)));
            }
        }
        if let Some(ms) = s.get("poll_ms") {
            for _ in 0..num(ms) {
                h.now += 1000;
                sd.poll(&mut h);
            }
        }
        while let Some(m) = peer.recv() {
            out.push(m);
        }
        if let Some(want) = s.get("air_out") {
            for w in want.as_array().unwrap() {
                match out.iter().position(|g| matches(w, g)) {
                    Some(p) => {
                        out.drain(..=p);
                    }
                    None => {
                        return Err(at(format!(
                            "no air message matching {w}; got {:?}",
                            out.iter().map(|m| m.to_json()).collect::<Vec<_>>()
                        )))
                    }
                }
            }
        }
        if let Some(mm) = s.get("mem") {
            for m in mm.as_array().unwrap() {
                let a = num(&m["addr"]);
                let want = unhex(m["hex"].as_str().unwrap());
                let mut got = vec![0u8; want.len()];
                h.read(a, &mut got);
                if got != want {
                    return Err(at(format!(
                        "memory at {:#x}: {:02x?}, want {:02x?}",
                        a, got, want
                    )));
                }
            }
        }
        if let Some(irq) = s.get("expect_irq_pending") {
            if h.ispr & (1 << num(irq)) == 0 {
                return Err(at(format!("IRQ {} not pending", irq)));
            }
        }
    }
    Ok(())
}

#[test]
fn conformance_vectors() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("conformance");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    files.sort();
    // Absence check first: an empty directory must not pass vacuously.
    assert!(
        files.len() >= 5,
        "expected the conformance vectors in {}, found {}",
        dir.display(),
        files.len()
    );
    let mut failures = Vec::new();
    for f in &files {
        if let Err(e) = run_vector(f) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} vectors failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
