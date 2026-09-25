// SPDX-License-Identifier: MIT
//! C ABI, for backends not written in Rust (Renode's C# peripheral uses it via
//! P/Invoke). One handle = one emulated SoftDevice.

use std::ffi::{c_char, c_void, CStr};

use crate::air::{Air, AirMsg, MemAir, MemAirBus, TcpAir};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Peer ports of SoftDevices created with air = "mem" (tests of the C ABI).
fn mem_peers() -> &'static Mutex<HashMap<usize, MemAir>> {
    static P: OnceLock<Mutex<HashMap<usize, MemAir>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}
use crate::host::{Host, NvicOp};
use crate::sd::{Config, SoftDevice};

/// Callbacks into the emulator. Every function gets `ctx` back.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SdHleHost {
    pub ctx: *mut c_void,
    /// Returns 1 on success.
    pub read: extern "C" fn(ctx: *mut c_void, addr: u32, buf: *mut u8, len: u32) -> i32,
    pub write: extern "C" fn(ctx: *mut c_void, addr: u32, buf: *const u8, len: u32) -> i32,
    /// op/a/b as NvicOp::encode. NULL: use the NVIC registers through read/write.
    pub nvic: Option<extern "C" fn(ctx: *mut c_void, op: u32, a: u32, b: u32) -> u32>,
    pub now_us: extern "C" fn(ctx: *mut c_void) -> u64,
}

struct CHost(SdHleHost);
impl Host for CHost {
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> bool {
        (self.0.read)(self.0.ctx, addr, buf.as_mut_ptr(), buf.len() as u32) == 1
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> bool {
        (self.0.write)(self.0.ctx, addr, data.as_ptr(), data.len() as u32) == 1
    }
    fn nvic(&mut self, op: NvicOp) -> u32 {
        match self.0.nvic {
            Some(f) => {
                let (o, a, b) = op.encode();
                f(self.0.ctx, o, a, b)
            }
            None => crate::host::scs_nvic(self, op),
        }
    }
    fn now_us(&mut self) -> u64 {
        (self.0.now_us)(self.0.ctx)
    }
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Create a SoftDevice. `node`: air node name; `addr_hex`: the device address
/// as "C0:EE:AA:BB:CC:DD" (most significant first); `air`: "host:port" of the
/// air hub or empty for none. Returns NULL on a bad address.
#[no_mangle]
pub extern "C" fn sdhle_new(
    node: *const c_char,
    addr_hex: *const c_char,
    air: *const c_char,
) -> *mut SoftDevice {
    let node = cstr(node);
    let parts: Vec<u8> = cstr(addr_hex)
        .split(':')
        .filter_map(|p| u8::from_str_radix(p, 16).ok())
        .collect();
    if parts.len() != 6 {
        return std::ptr::null_mut();
    }
    let mut addr = [0u8; 6];
    for i in 0..6 {
        addr[i] = parts[5 - i];
    }
    let mut sd = SoftDevice::new(Config::microbit_v1(&node, addr));
    let air = cstr(air);
    let mut peer = None;
    if air == "mem" {
        let bus = MemAirBus::default();
        peer = Some(bus.port());
        sd.attach_air(Box::new(bus.port()));
    } else if !air.is_empty() {
        if let Ok(a) = TcpAir::connect(&air, &node, "sd-hle") {
            sd.attach_air(Box::new(a));
        }
    }
    let p = Box::into_raw(Box::new(sd));
    if let Some(peer) = peer {
        mem_peers().lock().unwrap().insert(p as usize, peer);
    }
    p
}

#[no_mangle]
pub extern "C" fn sdhle_free(sd: *mut SoftDevice) {
    if !sd.is_null() {
        mem_peers().lock().unwrap().remove(&(sd as usize));
        drop(unsafe { Box::from_raw(sd) });
    }
}

/// Service one SVC; returns r0.
#[no_mangle]
pub extern "C" fn sdhle_svc(
    sd: *mut SoftDevice,
    num: u8,
    a0: u32,
    a1: u32,
    a2: u32,
    a3: u32,
    host: SdHleHost,
) -> u32 {
    let sd = unsafe { &mut *sd };
    sd.svc(num, [a0, a1, a2, a3], &mut CHost(host))
}

#[no_mangle]
pub extern "C" fn sdhle_poll(sd: *mut SoftDevice, host: SdHleHost) {
    let sd = unsafe { &mut *sd };
    sd.poll(&mut CHost(host))
}

/// The vector table base the application asked the SoftDevice to forward to (0: none yet).
#[no_mangle]
pub extern "C" fn sdhle_vector_base(sd: *const SoftDevice) -> u32 {
    unsafe { &*sd }.vector_base
}

/// Name of an SVC number ("" if unknown). Static string.
#[no_mangle]
pub extern "C" fn sdhle_svc_name(num: u8) -> *const c_char {
    match crate::facts::svc::name(num) {
        Some(n) => {
            // Names are 'static &str without NUL; keep a leaked NUL-terminated copy per name.
            use std::collections::HashMap;
            use std::sync::{Mutex, OnceLock};
            static NAMES: OnceLock<Mutex<HashMap<u8, std::ffi::CString>>> = OnceLock::new();
            let m = NAMES.get_or_init(|| Mutex::new(HashMap::new()));
            let mut g = m.lock().unwrap();
            g.entry(num)
                .or_insert_with(|| std::ffi::CString::new(n).unwrap())
                .as_ptr()
        }
        None => b"\0".as_ptr() as *const c_char,
    }
}

/// air = "mem" only: hand one bw-air/1 JSON message to the SoftDevice as if
/// another node had sent it. Returns 1 if accepted.
#[no_mangle]
pub extern "C" fn sdhle_air_inject(sd: *mut SoftDevice, json: *const c_char) -> i32 {
    let Some(m) = AirMsg::from_json(&cstr(json)) else {
        return 0;
    };
    match mem_peers().lock().unwrap().get_mut(&(sd as usize)) {
        Some(p) => {
            p.send(&m);
            1
        }
        None => 0,
    }
}

/// air = "mem" only: take the next message the SoftDevice sent, as JSON, into
/// `buf` (NUL-terminated). Returns its length, 0 when none, -1 if too small.
#[no_mangle]
pub extern "C" fn sdhle_air_take(sd: *mut SoftDevice, buf: *mut c_char, cap: u32) -> i32 {
    let mut g = mem_peers().lock().unwrap();
    let Some(p) = g.get_mut(&(sd as usize)) else {
        return 0;
    };
    let Some(m) = p.recv() else { return 0 };
    let s = m.to_json();
    if s.len() + 1 > cap as usize {
        return -1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, s.len());
        *buf.add(s.len()) = 0;
    }
    s.len() as i32
}
