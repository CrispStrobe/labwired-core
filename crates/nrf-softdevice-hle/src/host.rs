// SPDX-License-Identifier: MIT
//! What an emulator backend provides to the HLE. The HLE never touches an
//! emulator type: every backend (labwired, Renode via the C ABI, the replay
//! host of the conformance tests) implements this one trait.

/// NVIC and core operations the SoftDevice performs on the application's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvicOp {
    Enable(u32),
    Disable(u32),
    SetPending(u32),
    ClearPending(u32),
    /// Returns 1 when pending.
    GetPending(u32),
    SetPriority(u32, u32),
    /// Returns the priority (0..3 on Cortex-M0, as the app numbers them).
    GetPriority(u32),
    SystemReset,
    /// PRIMASK = 1 (true) or 0 (false).
    SetPrimask(bool),
    /// Returns PRIMASK.
    GetPrimask,
    /// Returns 1 when the IRQ is enabled.
    GetEnabled(u32),
}

impl NvicOp {
    /// Stable numeric encoding for the C ABI and the conformance vectors.
    pub fn encode(self) -> (u32, u32, u32) {
        match self {
            NvicOp::Enable(i) => (1, i, 0),
            NvicOp::Disable(i) => (2, i, 0),
            NvicOp::SetPending(i) => (3, i, 0),
            NvicOp::ClearPending(i) => (4, i, 0),
            NvicOp::GetPending(i) => (5, i, 0),
            NvicOp::SetPriority(i, p) => (6, i, p),
            NvicOp::GetPriority(i) => (7, i, 0),
            NvicOp::SystemReset => (8, 0, 0),
            NvicOp::SetPrimask(b) => (9, b as u32, 0),
            NvicOp::GetPrimask => (10, 0, 0),
            NvicOp::GetEnabled(i) => (11, i, 0),
        }
    }
    pub fn decode(op: u32, a: u32, b: u32) -> Option<NvicOp> {
        Some(match op {
            1 => NvicOp::Enable(a),
            2 => NvicOp::Disable(a),
            3 => NvicOp::SetPending(a),
            4 => NvicOp::ClearPending(a),
            5 => NvicOp::GetPending(a),
            6 => NvicOp::SetPriority(a, b),
            7 => NvicOp::GetPriority(a),
            8 => NvicOp::SystemReset,
            9 => NvicOp::SetPrimask(a != 0),
            10 => NvicOp::GetPrimask,
            11 => NvicOp::GetEnabled(a),
            _ => return None,
        })
    }
}

/// The emulator side of the HLE.
pub trait Host {
    /// Guest memory read. false: the address is not readable (the HLE then
    /// returns NRF_ERROR_INVALID_ADDR, as the SoftDevice does for a bad pointer).
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> bool;
    /// Guest memory write (RAM, and flash for sd_flash_write / page erase).
    fn write(&mut self, addr: u32, data: &[u8]) -> bool;
    /// Default: through the memory-mapped NVIC/SCB registers (ARMv6-M/ARMv7-M
    /// System Control Space), which every Cortex-M emulator models. Priority
    /// numbers are the application's (0..3 on nRF51: 2 implemented bits).
    fn nvic(&mut self, op: NvicOp) -> u32 {
        scs_nvic(self, op)
    }
    /// Emulated time, microseconds since reset.
    fn now_us(&mut self) -> u64;
    fn log(&mut self, _msg: &str) {}
}

const ISER: u32 = 0xE000_E100;
const ICER: u32 = 0xE000_E180;
const ISPR: u32 = 0xE000_E200;
const ICPR: u32 = 0xE000_E280;
const IPR: u32 = 0xE000_E400;
const AIRCR: u32 = 0xE000_ED0C;

/// NVIC operations through the System Control Space registers.
pub fn scs_nvic<H: Host + ?Sized>(h: &mut H, op: NvicOp) -> u32 {
    let bit = |i: u32| (1u32 << (i & 31)).to_le_bytes();
    let reg = |base: u32, i: u32| base + 4 * (i / 32);
    let rd = |h: &mut H, a: u32| {
        let mut b = [0u8; 4];
        h.read(a, &mut b);
        u32::from_le_bytes(b)
    };
    match op {
        NvicOp::Enable(i) => {
            h.write(reg(ISER, i), &bit(i));
            0
        }
        NvicOp::Disable(i) => {
            h.write(reg(ICER, i), &bit(i));
            0
        }
        NvicOp::SetPending(i) => {
            h.write(reg(ISPR, i), &bit(i));
            0
        }
        NvicOp::ClearPending(i) => {
            h.write(reg(ICPR, i), &bit(i));
            0
        }
        NvicOp::GetPending(i) => (rd(h, reg(ISPR, i)) >> (i & 31)) & 1,
        NvicOp::GetEnabled(i) => (rd(h, reg(ISER, i)) >> (i & 31)) & 1,
        NvicOp::SetPriority(i, p) => {
            // Word access (ARMv6-M allows only word access to IPR).
            let a = IPR + (i & !3);
            let sh = 8 * (i & 3);
            let v = (rd(h, a) & !(0xFF << sh)) | (((p & 3) << 6) << sh);
            h.write(a, &v.to_le_bytes());
            0
        }
        NvicOp::GetPriority(i) => (rd(h, IPR + (i & !3)) >> (8 * (i & 3) + 6)) & 3,
        NvicOp::SystemReset => {
            h.write(AIRCR, &0x05FA_0004u32.to_le_bytes());
            0
        }
        NvicOp::SetPrimask(_) | NvicOp::GetPrimask => 0,
    }
}

/// Little-endian helpers over a Host.
pub(crate) trait HostExt: Host {
    fn r8(&mut self, a: u32) -> Option<u8> {
        let mut b = [0u8; 1];
        self.read(a, &mut b).then_some(b[0])
    }
    fn r16(&mut self, a: u32) -> Option<u16> {
        let mut b = [0u8; 2];
        self.read(a, &mut b).then_some(u16::from_le_bytes(b))
    }
    fn r32(&mut self, a: u32) -> Option<u32> {
        let mut b = [0u8; 4];
        self.read(a, &mut b).then_some(u32::from_le_bytes(b))
    }
    fn rbuf(&mut self, a: u32, n: usize) -> Option<Vec<u8>> {
        let mut b = vec![0u8; n];
        self.read(a, &mut b).then_some(b)
    }
    fn w8(&mut self, a: u32, v: u8) -> bool {
        self.write(a, &[v])
    }
    fn w16(&mut self, a: u32, v: u16) -> bool {
        self.write(a, &v.to_le_bytes())
    }
    fn w32(&mut self, a: u32, v: u32) -> bool {
        self.write(a, &v.to_le_bytes())
    }
}
impl<T: Host + ?Sized> HostExt for T {}
