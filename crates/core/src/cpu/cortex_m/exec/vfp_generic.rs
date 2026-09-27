// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.
//
// The FPv5 (Cortex-M7 FPv5-D16) floating-point instructions that have no
// dedicated `Instruction` variant: the whole "other" data-processing group
// (VABS/VNEG/VSQRT/VCMP/VCVT*/VRINT*/VMOV imm .F64), the non-fused and
// negated multiply-accumulate forms, the double-precision fused forms,
// VMRS/VMSR, VMOV to/from a D-register half, and the ARMv8 additions FPv5
// carries (VSEL, VMAXNM/VMINNM, VRINT{A,N,P,M}, VCVT{A,N,P,M}).
//
// The decoder keeps the raw 32-bit opcode (`Instruction::Vfp { op }`) and the
// fields are decoded here, following the ARMv7-M ARM (DDI 0403E) section A7.9
// "Floating-point data-processing instructions" and the ARMv8-M ARM (DDI 0553)
// for the FPv5 additions. Encodings were cross-checked against the GNU
// assembler (`arm-none-eabi-as -mcpu=cortex-m7`); see the decoder unit tests.
//
// Arithmetic follows the ARM pseudocode rules that change results:
// FPSCR.FZ (flush denormal inputs and outputs), FPSCR.DN (default NaN), the
// NaN-propagation order (first signalling NaN, then first quiet NaN), and
// FPSCR.RMode for the instructions that honour it (VCVTR, VRINTR, VRINTX,
// VCVT from integer). Arithmetic rounding otherwise is round-to-nearest-even,
// the FPSCR reset mode. Cumulative exception flags are not accumulated.

use super::super::CortexM;
use super::super::PcAdvance;
use super::super::{FPSCR_DN, FPSCR_FZ};
use crate::{SimResult, SimulationError};

/// FPSCR bits a VMSR can write (ARMv7-M B1.4.8 / ARMv8-M): NZCV, AHP, DN, FZ,
/// RMode and the six cumulative exception flags.
const FPSCR_WRITABLE: u32 = 0xF7C0_009F;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Round {
    Nearest,
    PlusInf,
    MinusInf,
    Zero,
    TiesAway,
}

fn fpscr_round(fpscr: u32) -> Round {
    match (fpscr >> 22) & 3 {
        0 => Round::Nearest,
        1 => Round::PlusInf,
        2 => Round::MinusInf,
        _ => Round::Zero,
    }
}

fn round_integral(x: f64, r: Round) -> f64 {
    match r {
        Round::Nearest => x.round_ties_even(),
        Round::PlusInf => x.ceil(),
        Round::MinusInf => x.floor(),
        Round::Zero => x.trunc(),
        Round::TiesAway => x.round(),
    }
}

/// One IEEE format. Values travel as raw bits in a `u64` so both widths share
/// the instruction logic below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fmt {
    S,
    D,
}

impl Fmt {
    fn sign(self) -> u64 {
        match self {
            Fmt::S => 0x8000_0000,
            Fmt::D => 0x8000_0000_0000_0000,
        }
    }
    fn exp(self) -> u64 {
        match self {
            Fmt::S => 0x7F80_0000,
            Fmt::D => 0x7FF0_0000_0000_0000,
        }
    }
    fn mant(self) -> u64 {
        match self {
            Fmt::S => 0x007F_FFFF,
            Fmt::D => 0x000F_FFFF_FFFF_FFFF,
        }
    }
    fn quiet(self) -> u64 {
        match self {
            Fmt::S => 0x0040_0000,
            Fmt::D => 0x0008_0000_0000_0000,
        }
    }
    fn default_nan(self) -> u64 {
        match self {
            Fmt::S => 0x7FC0_0000,
            Fmt::D => 0x7FF8_0000_0000_0000,
        }
    }
    fn is_nan(self, b: u64) -> bool {
        b & self.exp() == self.exp() && b & self.mant() != 0
    }
    fn is_snan(self, b: u64) -> bool {
        self.is_nan(b) && b & self.quiet() == 0
    }
    fn flush(self, b: u64) -> u64 {
        if b & self.exp() == 0 && b & self.mant() != 0 {
            b & self.sign()
        } else {
            b
        }
    }
    /// The value as an `f64` (exact for both formats).
    fn to_f64(self, b: u64) -> f64 {
        match self {
            Fmt::S => f32::from_bits(b as u32) as f64,
            Fmt::D => f64::from_bits(b),
        }
    }
    /// Round an `f64` to this format (round-to-nearest-even).
    fn round_from_f64(self, v: f64) -> u64 {
        match self {
            Fmt::S => (v as f32).to_bits() as u64,
            Fmt::D => v.to_bits(),
        }
    }
}

struct FpEnv {
    fz: bool,
    dn: bool,
}

impl FpEnv {
    fn new(fpscr: u32) -> Self {
        Self {
            fz: fpscr & FPSCR_FZ != 0,
            dn: fpscr & FPSCR_DN != 0,
        }
    }
    fn input(&self, f: Fmt, b: u64) -> u64 {
        if self.fz {
            f.flush(b)
        } else {
            b
        }
    }
    fn output(&self, f: Fmt, b: u64) -> u64 {
        if f.is_nan(b) {
            // A NaN produced by the arithmetic itself (invalid operation).
            return f.default_nan();
        }
        if self.fz {
            f.flush(b)
        } else {
            b
        }
    }
    /// ARM `FPProcessNaNs`: the first signalling NaN wins (quietened), then
    /// the first quiet NaN; with DN set the result is the default NaN.
    fn process_nans(&self, f: Fmt, ops: &[u64]) -> Option<u64> {
        let pick = ops
            .iter()
            .copied()
            .find(|&b| f.is_snan(b))
            .or_else(|| ops.iter().copied().find(|&b| f.is_nan(b)))?;
        Some(if self.dn {
            f.default_nan()
        } else {
            pick | f.quiet()
        })
    }
    fn binop(&self, f: Fmt, a: u64, b: u64, op: fn(f64, f64) -> f64) -> u64 {
        let (a, b) = (self.input(f, a), self.input(f, b));
        if let Some(n) = self.process_nans(f, &[a, b]) {
            return n;
        }
        // For binary32 operands the f64 computation of +,-,*,/ followed by one
        // rounding to binary32 is correctly rounded (53 >= 2*24 + 2).
        self.output(f, f.round_from_f64(op(f.to_f64(a), f.to_f64(b))))
    }
    fn fused(&self, f: Fmt, addend: u64, x: u64, y: u64) -> u64 {
        let (c, a, b) = (self.input(f, addend), self.input(f, x), self.input(f, y));
        if let Some(n) = self.process_nans(f, &[c, a, b]) {
            return n;
        }
        let r = match f {
            Fmt::S => (f32::from_bits(a as u32))
                .mul_add(f32::from_bits(b as u32), f32::from_bits(c as u32))
                .to_bits() as u64,
            Fmt::D => f64::from_bits(a)
                .mul_add(f64::from_bits(b), f64::from_bits(c))
                .to_bits(),
        };
        self.output(f, r)
    }
    fn sqrt(&self, f: Fmt, a: u64) -> u64 {
        let a = self.input(f, a);
        if let Some(n) = self.process_nans(f, &[a]) {
            return n;
        }
        let r = match f {
            Fmt::S => (f32::from_bits(a as u32)).sqrt().to_bits() as u64,
            Fmt::D => f64::from_bits(a).sqrt().to_bits(),
        };
        self.output(f, r)
    }
    /// VCMP: returns FPSCR NZCV in bits [3:0].
    fn compare(&self, f: Fmt, a: u64, b: u64) -> u32 {
        let (a, b) = (self.input(f, a), self.input(f, b));
        if f.is_nan(a) || f.is_nan(b) {
            return 0b0011;
        }
        let (x, y) = (f.to_f64(a), f.to_f64(b));
        if x == y {
            0b0110
        } else if x < y {
            0b1000
        } else {
            0b0010
        }
    }
    fn round_int(&self, f: Fmt, a: u64, r: Round) -> u64 {
        let a = self.input(f, a);
        if let Some(n) = self.process_nans(f, &[a]) {
            return n;
        }
        let v = f.to_f64(a);
        if v.is_infinite() || v == 0.0 {
            return a;
        }
        let rounded = round_integral(v, r);
        // Keep the sign of a result that rounded to zero (-0.3 -> -0.0).
        let bits = f.round_from_f64(rounded);
        if rounded == 0.0 {
            (a & f.sign()) | (bits & !f.sign())
        } else {
            bits
        }
    }
    fn max_min_num(&self, f: Fmt, a: u64, b: u64, max: bool) -> u64 {
        let (a, b) = (self.input(f, a), self.input(f, b));
        // IEEE 754-2008 maxNum/minNum: a single quiet NaN loses to a number.
        let (qa, qb) = (
            f.is_nan(a) && !f.is_snan(a),
            f.is_nan(b) && !f.is_snan(b),
        );
        let (a, b) = match (qa, qb) {
            (true, false) if !f.is_nan(b) => (b, b),
            (false, true) if !f.is_nan(a) => (a, a),
            _ => (a, b),
        };
        if let Some(n) = self.process_nans(f, &[a, b]) {
            return n;
        }
        let (x, y) = (f.to_f64(a), f.to_f64(b));
        let pick_a = if x == y {
            // +0 > -0 for max; -0 < +0 for min.
            let a_neg = a & f.sign() != 0;
            if max {
                !a_neg
            } else {
                a_neg
            }
        } else if max {
            x > y
        } else {
            x < y
        };
        self.output(f, if pick_a { a } else { b })
    }
}

/// ARM `FPToFixed`: value * 2^fbits rounded with `r`, saturated to `bits`.
/// NaN converts to 0. Returns the result zero/sign-extended to 64 bits.
fn fp_to_fixed(f: Fmt, a: u64, fbits: u32, unsigned: bool, bits: u32, r: Round, fz: bool) -> u64 {
    let a = if fz { f.flush(a) } else { a };
    if f.is_nan(a) {
        return 0;
    }
    let v = f.to_f64(a) * 2f64.powi(fbits as i32);
    let v = round_integral(v, r);
    if unsigned {
        let max = ((1u128 << bits) - 1) as f64;
        let x = if v <= 0.0 {
            0.0
        } else if v >= max {
            max
        } else {
            v
        };
        x as u64
    } else {
        let max = ((1i128 << (bits - 1)) - 1) as f64;
        let min = -((1i128 << (bits - 1)) as f64);
        let x = v.clamp(min, max);
        (x as i64) as u64
    }
}

/// ARM `FixedToFP`: the low `bits` of `raw` as a (un)signed fixed-point value
/// with `fbits` fraction bits, rounded to `f` with `r`.
fn fixed_to_fp(f: Fmt, raw: u64, fbits: u32, unsigned: bool, bits: u32, r: Round) -> u64 {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let x = raw & mask;
    let int = if unsigned {
        x as f64
    } else {
        let shift = 64 - bits;
        (((x << shift) as i64) >> shift) as f64
    };
    let v = int / 2f64.powi(fbits as i32);
    match f {
        Fmt::D => v.to_bits(), // exact: at most 32 significant bits
        Fmt::S => {
            if r == Round::Nearest {
                return (v as f32).to_bits() as u64;
            }
            // Directed rounding to binary32: step the nearest value toward
            // the requested direction when it overshot.
            let n = v as f32;
            let nd = n as f64;
            let adj = match r {
                Round::PlusInf if nd < v => next_up_f32(n),
                Round::MinusInf if nd > v => next_down_f32(n),
                Round::Zero if nd.abs() > v.abs() => {
                    if n > 0.0 {
                        next_down_f32(n)
                    } else {
                        next_up_f32(n)
                    }
                }
                _ => n,
            };
            adj.to_bits() as u64
        }
    }
}

fn next_up_f32(x: f32) -> f32 {
    if x.is_nan() || x == f32::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f32::from_bits(1);
    }
    let b = x.to_bits();
    f32::from_bits(if x > 0.0 { b + 1 } else { b - 1 })
}

fn next_down_f32(x: f32) -> f32 {
    -next_up_f32(-x)
}

/// IEEE binary16 -> binary32 (exact). `ahp` selects the ARM alternative
/// half-precision format (no infinities or NaNs).
fn f16_to_f32(h: u16, ahp: bool) -> u32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as u32;
    if exp == 0x1F && !ahp {
        if mant == 0 {
            return sign | 0x7F80_0000;
        }
        return sign | 0x7FC0_0000 | (mant << 13);
    }
    if exp == 0 {
        if mant == 0 {
            return sign;
        }
        // Denormal: value = mant * 2^-24.
        let v = mant as f32 * 2f32.powi(-24);
        return sign | v.to_bits();
    }
    sign | (((exp - 15 + 127) as u32) << 23) | (mant << 13)
}

/// IEEE binary32 -> binary16, round-to-nearest-even.
fn f32_to_f16(bits: u32, ahp: bool) -> u16 {
    let sign = ((bits >> 16) & 0x8000) as u16;
    let v = f32::from_bits(bits);
    if v.is_nan() {
        if ahp {
            return sign;
        }
        return sign | 0x7E00 | ((bits >> 13) & 0x1FF) as u16;
    }
    let a = v.abs() as f64;
    let max = if ahp { 131_008.0 } else { 65_504.0 };
    if v.is_infinite() || a >= max + 16.0 {
        return if ahp { sign | 0x7FFF } else { sign | 0x7C00 };
    }
    if a == 0.0 {
        return sign;
    }
    // Scale into units of the smallest denormal (2^-24) and round to even.
    let exp = a.log2().floor() as i32;
    let e = exp.max(-14);
    let quantum = 2f64.powi(e - 10);
    let q = (a / quantum).round_ties_even();
    let mut m = q as u32; // includes the implicit bit when normal
    let mut e = e;
    if m >= 2048 {
        m >>= 1;
        e += 1;
    }
    if m < 1024 {
        // Denormal (e == -14).
        return sign | m as u16;
    }
    if e > 15 {
        return if ahp { sign | 0x7FFF } else { sign | 0x7C00 };
    }
    sign | (((e + 15) as u16) << 10) | (m as u16 & 0x3FF)
}

fn vfp_expand_imm(f: Fmt, imm8: u32) -> u64 {
    let sign = ((imm8 >> 7) & 1) as u64;
    let b6 = ((imm8 >> 6) & 1) as u64;
    let low = (imm8 & 0x3F) as u64;
    match f {
        Fmt::S => {
            (sign << 31) | ((b6 ^ 1) << 30) | (if b6 == 1 { 0x1F } else { 0 } << 25) | (low << 19)
        }
        Fmt::D => {
            (sign << 63) | ((b6 ^ 1) << 62) | (if b6 == 1 { 0xFF } else { 0 } << 54) | (low << 48)
        }
    }
}

impl CortexM {
    fn vfp_undef(&self) -> SimResult<PcAdvance> {
        Err(SimulationError::DecodeError(self.pc as u64))
    }

    fn vfp_read(&self, f: Fmt, idx: usize) -> u64 {
        match f {
            Fmt::S => self.fpu_s[idx] as u64,
            Fmt::D => (self.fpu_s[2 * idx] as u64) | ((self.fpu_s[2 * idx + 1] as u64) << 32),
        }
    }

    fn vfp_write(&mut self, f: Fmt, idx: usize, v: u64) {
        match f {
            Fmt::S => self.fpu_s[idx] = v as u32,
            Fmt::D => {
                self.fpu_s[2 * idx] = v as u32;
                self.fpu_s[2 * idx + 1] = (v >> 32) as u32;
            }
        }
    }

    /// Execute one FPv5 instruction kept raw by the decoder (see module doc).
    pub(in crate::cpu::cortex_m) fn exec_vfp_generic(&mut self, op: u32) -> SimResult<PcAdvance> {
        let bit = |n: u32| (op >> n) & 1;
        let vd = (op >> 12) & 0xF;
        let vn = (op >> 16) & 0xF;
        let vm = op & 0xF;
        let (d, n, m) = (bit(22), bit(7), bit(5));
        let sd = ((vd << 1) | d) as usize;
        let sn = ((vn << 1) | n) as usize;
        let sm = ((vm << 1) | m) as usize;
        let dd = ((d << 4) | vd) as usize;
        let dn = ((n << 4) | vn) as usize;
        let dm = ((m << 4) | vm) as usize;
        let fmt = if bit(8) == 1 { Fmt::D } else { Fmt::S };
        let (rd, rn, rm) = match fmt {
            Fmt::S => (sd, sn, sm),
            Fmt::D => (dd, dn, dm),
        };
        let env = FpEnv::new(self.fpscr);
        let neg = |b: u64| b ^ fmt.sign();

        // ---- Register transfers (op[4] == 1) ----
        if bit(4) == 1 {
            let rt = ((op >> 12) & 0xF) as u8;
            // VMRS / VMSR: 1110 1110 111L reg Rt 1010 0001 0000
            if (op >> 21) & 0x7FF == 0x777 && (op & 0x0FFF) == 0x0A10 {
                let reg = (op >> 16) & 0xF;
                if bit(20) == 1 {
                    let value = if reg == 1 { self.fpscr } else { 0 };
                    if rt == 15 {
                        // APSR_nzcv, FPSCR
                        self.xpsr = (self.xpsr & 0x0FFF_FFFF) | (self.fpscr & 0xF000_0000);
                    } else {
                        self.write_reg(rt, value);
                    }
                } else if reg == 1 {
                    self.fpscr = self.read_reg(rt) & FPSCR_WRITABLE;
                }
                return Ok(PcAdvance::Add4);
            }
            // VMOV Dd[x], Rt / VMOV Rt, Dn[x] (32-bit scalar):
            // 1110 1110 00xL Vd Rt 1011 D001 0000
            if (op >> 22) == 0x3B8 && (op & 0x0F7F) == 0x0B10 {
                let dreg = ((n << 4) | vn) as usize;
                if dreg > 15 {
                    return self.vfp_undef();
                }
                let s = 2 * dreg + bit(21) as usize;
                if bit(20) == 1 {
                    self.write_reg(rt, self.fpu_s[s]);
                } else {
                    self.fpu_s[s] = self.read_reg(rt);
                }
                return Ok(PcAdvance::Add4);
            }
            return self.vfp_undef();
        }

        // ---- FPv5 / ARMv8 unconditional group: 1111 1110 ... ----
        if op >> 24 == 0xFE {
            if fmt == Fmt::D && (dm > 15 || (dd > 15 && (op >> 18) & 3 != 0b11) || (bit(23) == 0 && dn > 15)) {
                return self.vfp_undef();
            }
            if bit(23) == 0 {
                // VSEL<cc>
                let cond = match (op >> 20) & 3 {
                    0 => 0x0, // EQ
                    1 => 0x6, // VS
                    2 => 0xA, // GE
                    _ => 0xC, // GT
                };
                let v = if self.check_condition(cond) {
                    self.vfp_read(fmt, rn)
                } else {
                    self.vfp_read(fmt, rm)
                };
                self.vfp_write(fmt, rd, v);
                return Ok(PcAdvance::Add4);
            }
            match (op >> 20) & 0xB {
                0x8 => {
                    // VMAXNM (op6=0) / VMINNM (op6=1)
                    let a = self.vfp_read(fmt, rn);
                    let b = self.vfp_read(fmt, rm);
                    let r = env.max_min_num(fmt, a, b, bit(6) == 0);
                    self.vfp_write(fmt, rd, r);
                    return Ok(PcAdvance::Add4);
                }
                0xB if bit(6) == 1 => {
                    let rmode = match (op >> 16) & 3 {
                        0 => Round::TiesAway,
                        1 => Round::Nearest,
                        2 => Round::PlusInf,
                        _ => Round::MinusInf,
                    };
                    match (op >> 18) & 3 {
                        0b10 if bit(7) == 0 => {
                            // VRINT{A,N,P,M}
                            let a = self.vfp_read(fmt, rm);
                            let r = env.round_int(fmt, a, rmode);
                            self.vfp_write(fmt, rd, r);
                            return Ok(PcAdvance::Add4);
                        }
                        0b11 => {
                            // VCVT{A,N,P,M}.{S32,U32}: Sd <- Sm/Dm; op7 = signed.
                            let a = self.vfp_read(fmt, rm);
                            let r = fp_to_fixed(fmt, a, 0, bit(7) == 0, 32, rmode, env.fz);
                            self.fpu_s[sd] = r as u32;
                            return Ok(PcAdvance::Add4);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            return self.vfp_undef();
        }

        let opc1 = (bit(23) << 2) | ((op >> 20) & 3);
        let op6 = bit(6);
        // FPv5-D16: D16..D31 do not exist. Every two/three-register form
        // below uses Dd/Dn/Dm when sz=1; the conversions in the "other"
        // group check their own D operand.
        if fmt == Fmt::D && opc1 != 0b111 && (dd > 15 || dn > 15 || dm > 15) {
            return self.vfp_undef();
        }
        match opc1 {
            0b000..=0b010 if !(opc1 == 0b010 && op6 == 0) => {
                let a = self.vfp_read(fmt, rn);
                let b = self.vfp_read(fmt, rm);
                let acc = self.vfp_read(fmt, rd);
                let prod = env.binop(fmt, a, b, |x, y| x * y);
                let r = match (opc1, op6) {
                    (0b000, 0) => env.binop(fmt, acc, prod, |x, y| x + y), // VMLA
                    (0b000, _) => env.binop(fmt, acc, neg(prod), |x, y| x + y), // VMLS
                    (0b001, 0) => env.binop(fmt, neg(acc), prod, |x, y| x + y), // VNMLS
                    (0b001, _) => env.binop(fmt, neg(acc), neg(prod), |x, y| x + y), // VNMLA
                    _ => neg(prod),                                        // VNMUL
                };
                self.vfp_write(fmt, rd, r);
            }
            0b010 => {
                let r = env.binop(fmt, self.vfp_read(fmt, rn), self.vfp_read(fmt, rm), |x, y| x * y);
                self.vfp_write(fmt, rd, r);
            }
            0b011 => {
                let (a, b) = (self.vfp_read(fmt, rn), self.vfp_read(fmt, rm));
                let r = if op6 == 0 {
                    env.binop(fmt, a, b, |x, y| x + y)
                } else {
                    env.binop(fmt, a, b, |x, y| x - y)
                };
                self.vfp_write(fmt, rd, r);
            }
            0b100 if op6 == 0 => {
                let r = env.binop(fmt, self.vfp_read(fmt, rn), self.vfp_read(fmt, rm), |x, y| x / y);
                self.vfp_write(fmt, rd, r);
            }
            0b101 | 0b110 => {
                // 1D01: VFNMS (op6=0) / VFNMA (op6=1); 1D10: VFMA / VFMS.
                let (a, b, acc) = (
                    self.vfp_read(fmt, rn),
                    self.vfp_read(fmt, rm),
                    self.vfp_read(fmt, rd),
                );
                let r = match (opc1, op6) {
                    (0b110, 0) => env.fused(fmt, acc, a, b),
                    (0b110, _) => env.fused(fmt, acc, neg(a), b),
                    (0b101, 0) => env.fused(fmt, neg(acc), a, b),
                    _ => env.fused(fmt, neg(acc), neg(a), b),
                };
                self.vfp_write(fmt, rd, r);
            }
            0b111 => {
                if op6 == 0 {
                    // VMOV (immediate)
                    let imm8 = (((op >> 16) & 0xF) << 4) | (op & 0xF);
                    self.vfp_write(fmt, rd, vfp_expand_imm(fmt, imm8));
                    return Ok(PcAdvance::Add4);
                }
                return self.exec_vfp_other(op, fmt, rd, rm, sd, sm, dd, dm, &env);
            }
            _ => return self.vfp_undef(),
        }
        Ok(PcAdvance::Add4)
    }

    /// The opc1 = 1x11, opc3 = x1 "other" group, selected by opc2 = op[19:16].
    #[allow(clippy::too_many_arguments)]
    fn exec_vfp_other(
        &mut self,
        op: u32,
        fmt: Fmt,
        rd: usize,
        rm: usize,
        sd: usize,
        sm: usize,
        dd: usize,
        dm: usize,
        env: &FpEnv,
    ) -> SimResult<PcAdvance> {
        let bit = |n: u32| (op >> n) & 1;
        let opc2 = (op >> 16) & 0xF;
        let op7 = bit(7);
        let rmode = fpscr_round(self.fpscr);
        if fmt == Fmt::D {
            // Which operands are D registers depends on the conversion.
            let d_dest = !(matches!(opc2, 0b0111 | 0b1100 | 0b1101)
                || (matches!(opc2, 0b0010 | 0b0011) && bit(16) == 1));
            // Fixed-point forms carry an immediate in the Vm field.
            let d_src = !(matches!(opc2, 0b1000 | 0b1010 | 0b1011 | 0b1110 | 0b1111)
                || (matches!(opc2, 0b0010 | 0b0011) && bit(16) == 0));
            if (d_dest && dd > 15) || (d_src && dm > 15) {
                return self.vfp_undef();
            }
        }
        match opc2 {
            0b0000 => {
                let a = self.vfp_read(fmt, rm);
                // VMOV reg (op7=0) / VABS (op7=1)
                let r = if op7 == 1 { a & !fmt.sign() } else { a };
                self.vfp_write(fmt, rd, r);
            }
            0b0001 => {
                let a = self.vfp_read(fmt, rm);
                let r = if op7 == 0 {
                    a ^ fmt.sign() // VNEG
                } else {
                    env.sqrt(fmt, a) // VSQRT
                };
                self.vfp_write(fmt, rd, r);
            }
            0b0010 | 0b0011 => {
                // VCVTB / VCVTT: op16 = 0 half->F32/F64, 1 F32/F64->half;
                // T (op7) selects the top half of the single register.
                let ahp = self.fpscr & (1 << 26) != 0;
                let top = op7 == 1;
                if bit(16) == 0 {
                    let h = self.fpu_s[sm];
                    let h = if top { (h >> 16) as u16 } else { h as u16 };
                    let s = f16_to_f32(h, ahp);
                    let r = match fmt {
                        Fmt::S => s as u64,
                        Fmt::D => (f32::from_bits(s) as f64).to_bits(),
                    };
                    self.vfp_write(fmt, rd, r);
                } else {
                    let a = self.vfp_read(fmt, rm);
                    let s = match fmt {
                        Fmt::S => a as u32,
                        Fmt::D => (f64::from_bits(a) as f32).to_bits(),
                    };
                    let h = f32_to_f16(s, ahp) as u32;
                    let old = self.fpu_s[sd];
                    self.fpu_s[sd] = if top {
                        (old & 0x0000_FFFF) | (h << 16)
                    } else {
                        (old & 0xFFFF_0000) | h
                    };
                }
            }
            0b0100 | 0b0101 => {
                // VCMP / VCMPE (op7 = E); opc2 bit0: compare with +0.0.
                let a = self.vfp_read(fmt, rd);
                let b = if bit(16) == 1 { 0 } else { self.vfp_read(fmt, rm) };
                let nzcv = env.compare(fmt, a, b);
                self.fpscr = (self.fpscr & 0x0FFF_FFFF) | (nzcv << 28);
            }
            0b0110 => {
                // VRINTR (op7=0, FPSCR mode) / VRINTZ (op7=1)
                let a = self.vfp_read(fmt, rm);
                let r = env.round_int(fmt, a, if op7 == 1 { Round::Zero } else { rmode });
                self.vfp_write(fmt, rd, r);
            }
            0b0111 => {
                if op7 == 0 {
                    // VRINTX
                    let a = self.vfp_read(fmt, rm);
                    let r = env.round_int(fmt, a, rmode);
                    self.vfp_write(fmt, rd, r);
                } else {
                    // VCVT between F32 and F64. sz=0: Dd <- Sm; sz=1: Sd <- Dm.
                    match fmt {
                        Fmt::S => {
                            if dd > 15 {
                                return self.vfp_undef();
                            }
                            let a = env.input(Fmt::S, self.fpu_s[sm] as u64);
                            let r = if Fmt::S.is_nan(a) {
                                if env.dn {
                                    Fmt::D.default_nan()
                                } else {
                                    ((a & 0x8000_0000) << 32)
                                        | 0x7FF8_0000_0000_0000
                                        | ((a & 0x003F_FFFF) << 29)
                                }
                            } else {
                                (f32::from_bits(a as u32) as f64).to_bits()
                            };
                            self.vfp_write(Fmt::D, dd, r);
                        }
                        Fmt::D => {
                            let a = env.input(Fmt::D, self.vfp_read(Fmt::D, dm));
                            let r = if Fmt::D.is_nan(a) {
                                if env.dn {
                                    VFP_S_DEFAULT_NAN
                                } else {
                                    (((a >> 32) & 0x8000_0000) as u32)
                                        | 0x7FC0_0000
                                        | (((a >> 29) & 0x003F_FFFF) as u32)
                                }
                            } else {
                                let s = (f64::from_bits(a) as f32).to_bits();
                                if env.fz {
                                    Fmt::S.flush(s as u64) as u32
                                } else {
                                    s
                                }
                            };
                            self.fpu_s[sd] = r;
                        }
                    }
                }
            }
            0b1000 => {
                // VCVT.F32/F64.{S32,U32} Sd/Dd <- Sm (op7 = signed)
                let raw = self.fpu_s[sm] as u64;
                let r = fixed_to_fp(fmt, raw, 0, op7 == 0, 32, rmode);
                self.vfp_write(fmt, rd, r);
            }
            0b1010 | 0b1011 | 0b1110 | 0b1111 => {
                // Fixed-point VCVT, in place on Sd/Dd.
                // op18 = to fixed; op16 = unsigned; op7 (sx) = 32-bit.
                let size = if op7 == 1 { 32 } else { 16 };
                let imm = ((op & 0xF) << 1) | bit(5);
                if imm > size {
                    return self.vfp_undef();
                }
                let fbits = size - imm;
                let unsigned = bit(16) == 1;
                if bit(18) == 1 {
                    let a = self.vfp_read(fmt, rd);
                    let r = fp_to_fixed(fmt, a, fbits, unsigned, size, Round::Zero, env.fz);
                    let r = match fmt {
                        Fmt::S => r & 0xFFFF_FFFF,
                        Fmt::D => r,
                    };
                    self.vfp_write(fmt, rd, r);
                } else {
                    let raw = self.vfp_read(fmt, rd);
                    let r = fixed_to_fp(fmt, raw, fbits, unsigned, size, rmode);
                    self.vfp_write(fmt, rd, r);
                }
            }
            0b1100 | 0b1101 => {
                // VCVT{R}.{U32,S32}.F32/F64 Sd <- Sm/Dm.
                // opc2 bit0 = signed; op7 = round toward zero (else FPSCR).
                let a = self.vfp_read(fmt, rm);
                let r = fp_to_fixed(
                    fmt,
                    a,
                    0,
                    bit(16) == 0,
                    32,
                    if op7 == 1 { Round::Zero } else { rmode },
                    env.fz,
                );
                self.fpu_s[sd] = r as u32;
            }
            _ => return self.vfp_undef(),
        }
        Ok(PcAdvance::Add4)
    }
}

const VFP_S_DEFAULT_NAN: u32 = 0x7FC0_0000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_precision_round_trips() {
        for (h, f) in [
            (0x3C00u16, 1.0f32),
            (0xC000, -2.0),
            (0x7BFF, 65504.0),
            (0x0001, 5.960_464_5e-8),
            (0x3555, 0.333_251_95),
        ] {
            assert_eq!(f32::from_bits(f16_to_f32(h, false)), f, "{h:#x}");
            assert_eq!(f32_to_f16(f.to_bits(), false), h, "{f}");
        }
        assert_eq!(f32_to_f16(1.0e6f32.to_bits(), false), 0x7C00);
    }

    #[test]
    fn fixed_point_conversions() {
        // 1.5 as s16.3 = 12
        assert_eq!(
            fp_to_fixed(Fmt::S, 1.5f32.to_bits() as u64, 3, false, 16, Round::Zero, false),
            12
        );
        // saturation
        assert_eq!(
            fp_to_fixed(Fmt::S, 1e10f32.to_bits() as u64, 0, false, 32, Round::Zero, false) as u32,
            i32::MAX as u32
        );
        assert_eq!(
            fp_to_fixed(Fmt::S, (-1.0f32).to_bits() as u64, 0, true, 32, Round::Zero, false),
            0
        );
        // u16 12 with 3 fraction bits -> 1.5
        assert_eq!(fixed_to_fp(Fmt::S, 12, 3, true, 16, Round::Nearest), 1.5f32.to_bits() as u64);
        // s16 0xFFF8 = -8 with 3 fraction bits -> -1.0
        assert_eq!(
            fixed_to_fp(Fmt::D, 0xFFF8, 3, false, 16, Round::Nearest),
            (-1.0f64).to_bits()
        );
    }

    #[test]
    fn expand_imm_matches_gas() {
        // vmov.f64 d0, #1.5 assembles to imm8 = 0x78 (eeb7 0b08).
        assert_eq!(vfp_expand_imm(Fmt::D, 0x78), 1.5f64.to_bits());
        assert_eq!(vfp_expand_imm(Fmt::S, 0x70), 1.0f32.to_bits() as u64);
    }

    use crate::decoder::arm::{decode_thumb_32, Instruction};

    /// Encodings produced by `arm-none-eabi-as -mcpu=cortex-m7` (see module
    /// doc): every one must reach the generic executor.
    const GAS: &[(u32, &str)] = &[
        (0xEE10_0AC1, "vnmla.f32 s0, s1, s2"),
        (0xEE10_0A81, "vnmls.f32 s0, s1, s2"),
        (0xEE01_0B02, "vmla.f64 d0, d1, d2"),
        (0xEE20_0AC1, "vnmul.f32 s0, s1, s2"),
        (0xEEBD_0A60, "vcvtr.s32.f32 s0, s1"),
        (0xEEBB_0A66, "vcvt.f32.u16 s0, s0, #3"),
        (0xEEBE_0B66, "vcvt.s16.f64 d0, d0, #3"),
        (0xEE21_3B10, "vmov.32 d1[1], r3"),
        (0xEE13_2B10, "vmov.32 r2, d3[0]"),
        (0xEEF1_FA10, "vmrs APSR_nzcv, fpscr"),
        (0xEEE1_0A10, "vmsr fpscr, r0"),
        (0xEEB2_0A60, "vcvtb.f32.f16 s0, s1"),
        (0xEEB3_0AE0, "vcvtt.f16.f32 s0, s1"),
        (0xEEB5_0BC0, "vcmpe.f64 d0, #0.0"),
        (0xFE20_0A81, "vselge.f32 s0, s1, s2"),
        (0xFE80_0A81, "vmaxnm.f32 s0, s1, s2"),
        (0xFE81_0B42, "vminnm.f64 d0, d1, d2"),
        (0xFEB8_0A60, "vrinta.f32 s0, s1"),
        (0xFEBB_0B41, "vrintm.f64 d0, d1"),
        (0xFEBC_0AE0, "vcvta.s32.f32 s0, s1"),
        (0xFEBF_0B41, "vcvtm.u32.f64 s0, d1"),
        (0xEEB6_0AE0, "vrintz.f32 s0, s1"),
        (0xEEB7_0B41, "vrintx.f64 d0, d1"),
        (0xEEB6_0A60, "vrintr.f32 s0, s1"),
        (0xEEB7_0B08, "vmov.f64 d0, #1.5"),
        (0xEEB7_0AE0, "vcvt.f64.f32 d0, s1"),
        (0xEEB7_0BC1, "vcvt.f32.f64 s0, d1"),
        (0xEEB8_0BE0, "vcvt.f64.s32 d0, s1"),
        (0xEEBC_0BC1, "vcvt.u32.f64 s0, d1"),
        (0xEEB1_0BC1, "vsqrt.f64 d0, d1"),
    ];

    #[test]
    fn gas_encodings_reach_the_generic_executor() {
        for &(op, text) in GAS {
            let i = decode_thumb_32((op >> 16) as u16, op as u16);
            assert!(matches!(i, Instruction::Vfp { op: o } if o == op), "{text}: {i:?}");
        }
    }

    fn run(cpu: &mut CortexM, op: u32) {
        cpu.exec_vfp_generic(op).unwrap_or_else(|_| panic!("{op:#x}"));
    }

    fn set_d(cpu: &mut CortexM, d: usize, v: f64) {
        cpu.vfp_write(Fmt::D, d, v.to_bits());
    }

    fn get_d(cpu: &CortexM, d: usize) -> f64 {
        f64::from_bits(cpu.vfp_read(Fmt::D, d))
    }

    #[test]
    fn executes_the_gas_forms() {
        let mut c = CortexM::default();
        // vmsr fpscr, r0 / vmrs r?, fpscr round trip (RMode = +inf).
        c.write_reg(0, 1 << 22);
        run(&mut c, 0xEEE1_0A10);
        assert_eq!(c.fpscr, 1 << 22);
        // vcvtr.s32.f32 s0, s1 under RMode +inf: 1.25 -> 2
        c.fpu_s[1] = 1.25f32.to_bits();
        run(&mut c, 0xEEBD_0A60);
        assert_eq!(c.fpu_s[0], 2);
        c.fpscr = 0;
        // vcvtr under RN: 2.5 -> 2 (ties to even)
        c.fpu_s[1] = 2.5f32.to_bits();
        run(&mut c, 0xEEBD_0A60);
        assert_eq!(c.fpu_s[0], 2);
        // vmla.f64 d0, d1, d2: 1 + 2*3 = 7
        set_d(&mut c, 0, 1.0);
        set_d(&mut c, 1, 2.0);
        set_d(&mut c, 2, 3.0);
        run(&mut c, 0xEE01_0B02);
        assert_eq!(get_d(&c, 0), 7.0);
        // vnmul.f32 s0, s1, s2 = -(s1*s2)
        c.fpu_s[1] = 2.0f32.to_bits();
        c.fpu_s[2] = 4.0f32.to_bits();
        run(&mut c, 0xEE20_0AC1);
        assert_eq!(f32::from_bits(c.fpu_s[0]), -8.0);
        // vnmla.f32 s0, s1, s2: s0 = -s0 - s1*s2 = -(-8) - 8 = 0
        run(&mut c, 0xEE10_0AC1);
        assert_eq!(f32::from_bits(c.fpu_s[0]), 0.0);
        // vsqrt.f64 d0, d1
        set_d(&mut c, 1, 2.25);
        run(&mut c, 0xEEB1_0BC1);
        assert_eq!(get_d(&c, 0), 1.5);
        // vcvt.f64.f32 d0, s1 / vcvt.f32.f64 s0, d1
        c.fpu_s[1] = 0.1f32.to_bits();
        run(&mut c, 0xEEB7_0AE0);
        assert_eq!(get_d(&c, 0), 0.1f32 as f64);
        set_d(&mut c, 1, 0.1);
        run(&mut c, 0xEEB7_0BC1);
        assert_eq!(f32::from_bits(c.fpu_s[0]), 0.1f32);
        // vmov.32 d1[1], r3 / vmov.32 r2, d3[0]
        c.write_reg(3, 0xDEAD_BEEF);
        run(&mut c, 0xEE21_3B10);
        assert_eq!(c.fpu_s[3], 0xDEAD_BEEF);
        c.fpu_s[6] = 0x1234_5678;
        run(&mut c, 0xEE13_2B10);
        assert_eq!(c.read_reg(2), 0x1234_5678);
        // vcmpe.f64 d0, #0 with d0 = -1 -> N; vmrs APSR_nzcv, fpscr
        set_d(&mut c, 0, -1.0);
        run(&mut c, 0xEEB5_0BC0);
        assert_eq!(c.fpscr >> 28, 0b1000);
        c.xpsr = 0x0100_0000;
        run(&mut c, 0xEEF1_FA10);
        assert_eq!(c.xpsr >> 28, 0b1000);
        // vselge.f32 s0, s1, s2 with N=1,V=0 (LT) -> s2
        c.fpu_s[1] = 1.0f32.to_bits();
        c.fpu_s[2] = 2.0f32.to_bits();
        run(&mut c, 0xFE20_0A81);
        assert_eq!(f32::from_bits(c.fpu_s[0]), 2.0);
        // vcvt.f32.u16 s0, s0, #3: 12 -> 1.5
        c.fpu_s[0] = 12;
        run(&mut c, 0xEEBB_0A66);
        assert_eq!(f32::from_bits(c.fpu_s[0]), 1.5);
        // vcvt.s16.f64 d0, d0, #3: -1.0 -> -8 sign-extended
        set_d(&mut c, 0, -1.0);
        run(&mut c, 0xEEBE_0B66);
        assert_eq!(c.vfp_read(Fmt::D, 0), (-8i64) as u64);
        // vmov.f64 d0, #1.5
        run(&mut c, 0xEEB7_0B08);
        assert_eq!(get_d(&c, 0), 1.5);
        // vrinta.f32 s0, s1: 2.5 -> 3 ; vrintm.f64 d0, d1: -1.5 -> -2
        c.fpu_s[1] = 2.5f32.to_bits();
        run(&mut c, 0xFEB8_0A60);
        assert_eq!(f32::from_bits(c.fpu_s[0]), 3.0);
        set_d(&mut c, 1, -1.5);
        run(&mut c, 0xFEBB_0B41);
        assert_eq!(get_d(&c, 0), -2.0);
        // vcvtm.u32.f64 s0, d1 (-1.5 -> saturates at 0)
        run(&mut c, 0xFEBF_0B41);
        assert_eq!(c.fpu_s[0], 0);
        // vcvtt.f16.f32 s0, s1 writes the top half only
        c.fpu_s[0] = 0x0000_1111;
        c.fpu_s[1] = 1.0f32.to_bits();
        run(&mut c, 0xEEB3_0AE0);
        assert_eq!(c.fpu_s[0], 0x3C00_1111);
    }
}

