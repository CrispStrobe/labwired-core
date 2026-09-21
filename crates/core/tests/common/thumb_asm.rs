// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! A two-pass Thumb assembler for hand-built differential fixtures.
//!
//! Walk-differential gates need firmware, and pulling in a real toolchain for
//! a twenty-instruction poll loop is not worth it — but hand-writing
//! pc-relative `ldr` offsets and branch displacements by eye is exactly where
//! these fixtures go wrong, and a fixture that assembles to the wrong thing
//! still runs: it just stops exercising the peripheral, and the differential
//! then compares two identical do-nothing runs and reports success.
//!
//! So labels and literal-pool slots are resolved mechanically here, once, for
//! every fixture. Lifted verbatim from `nrf54l15_grtc_walk_differential.rs`,
//! which had the only copy.
//!
//! Only the handful of encodings the fixtures actually use are implemented;
//! add more as they are needed rather than speculatively.

#![allow(dead_code)]

#[derive(Clone)]
pub enum Ins {
    Raw(u16),
    /// `ldr rd, [pc, #imm]` loading pool word `key`.
    LdrPool(u8, &'static str),
    /// unconditional `b label`.
    B(&'static str),
    Label(&'static str),
}

pub struct Asm {
    base: u32,
    ins: Vec<Ins>,
    pool: Vec<(&'static str, u32)>,
}

impl Asm {
    pub fn new(base: u32) -> Self {
        Asm {
            base,
            ins: Vec::new(),
            pool: Vec::new(),
        }
    }
    pub fn movs(&mut self, rd: u8, imm: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x2000 | ((rd as u16) << 8) | imm as u16));
        self
    }
    pub fn lsls(&mut self, rd: u8, rm: u8, imm5: u8) -> &mut Self {
        self.ins.push(Ins::Raw(
            (imm5 as u16) << 6 | ((rm as u16) << 3) | rd as u16,
        ));
        self
    }
    pub fn adds(&mut self, rd: u8, imm: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x3000 | ((rd as u16) << 8) | imm as u16));
        self
    }
    /// `str rt, [rn, #0]`.
    pub fn str0(&mut self, rt: u8, rn: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x6000 | ((rn as u16) << 3) | rt as u16));
        self
    }
    /// `strb rt, [rn, #0]` — STRB (immediate) T1.
    pub fn strb0(&mut self, rt: u8, rn: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x7000 | ((rn as u16) << 3) | rt as u16));
        self
    }

    /// `adds rd, rd, rm` — ADD (register) T1. Low registers only.
    pub fn add_reg(&mut self, rd: u8, rm: u8) -> &mut Self {
        self.ins.push(Ins::Raw(
            0x1800 | ((rm as u16) << 6) | ((rd as u16) << 3) | rd as u16,
        ));
        self
    }

    /// `orrs rd, rm` — ORR (register) T1. Low registers only.
    pub fn orrs(&mut self, rd: u8, rm: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x4300 | ((rm as u16) << 3) | rd as u16));
        self
    }

    /// `ldr rt, [rn, #0]`.
    pub fn ldr0(&mut self, rt: u8, rn: u8) -> &mut Self {
        self.ins
            .push(Ins::Raw(0x6800 | ((rn as u16) << 3) | rt as u16));
        self
    }
    pub fn ldr_pool(&mut self, rd: u8, key: &'static str) -> &mut Self {
        self.ins.push(Ins::LdrPool(rd, key));
        self
    }
    pub fn wfi(&mut self) -> &mut Self {
        self.ins.push(Ins::Raw(0xBF30));
        self
    }
    pub fn bx(&mut self, rm: u8) -> &mut Self {
        self.ins.push(Ins::Raw(0x4700 | ((rm as u16) << 3)));
        self
    }
    pub fn b(&mut self, label: &'static str) -> &mut Self {
        self.ins.push(Ins::B(label));
        self
    }
    pub fn label(&mut self, name: &'static str) -> &mut Self {
        self.ins.push(Ins::Label(name));
        self
    }
    pub fn word(&mut self, key: &'static str, val: u32) -> &mut Self {
        self.pool.push((key, val));
        self
    }

    /// Assemble to (entry_addr, bytes). The literal pool is appended after the
    /// code, word-aligned.
    pub fn assemble(&self) -> (u32, Vec<u8>) {
        // Pass 1: assign an address to each instruction and every label.
        let mut addr = self.base;
        let mut labels: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        for i in &self.ins {
            match i {
                Ins::Label(n) => {
                    labels.insert(n, addr);
                }
                _ => addr += 2,
            }
        }
        // Pool starts word-aligned after the code.
        let pool_start = (addr + 3) & !3;
        let mut pool_addr: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        for (idx, (k, _)) in self.pool.iter().enumerate() {
            pool_addr.insert(k, pool_start + (idx as u32) * 4);
        }

        // Pass 2: emit.
        let mut out: Vec<u8> = Vec::new();
        let mut pc = self.base;
        let emit = |out: &mut Vec<u8>, hw: u16| out.extend_from_slice(&hw.to_le_bytes());
        for i in &self.ins {
            match i {
                Ins::Label(_) => {}
                Ins::Raw(hw) => {
                    emit(&mut out, *hw);
                    pc += 2;
                }
                Ins::LdrPool(rd, key) => {
                    let target = pool_addr[key];
                    let base = (pc + 4) & !3;
                    let imm = (target - base) / 4;
                    assert!(imm <= 255, "ldr literal out of range for {key}");
                    emit(&mut out, 0x4800 | ((*rd as u16) << 8) | imm as u16);
                    pc += 2;
                }
                Ins::B(label) => {
                    let target = labels[label];
                    let off = (target as i64 - (pc as i64 + 4)) / 2;
                    let imm11 = (off as i32 as u32) & 0x7FF;
                    emit(&mut out, 0xE000 | imm11 as u16);
                    pc += 2;
                }
            }
        }
        // Pad to the word-aligned pool start with nop halfwords (all emitted
        // code is 2-byte aligned, so this lands exactly on `pool_start`).
        while (self.base + out.len() as u32) < pool_start {
            emit(&mut out, 0xBF00); // nop
        }
        for (_, v) in &self.pool {
            out.extend_from_slice(&v.to_le_bytes());
        }
        (self.base, out)
    }
}
