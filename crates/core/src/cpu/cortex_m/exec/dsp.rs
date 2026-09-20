// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// MIT License; see LICENSE.
// Multiply arithmetic adapted from CrispStrobe/labwired-core f1d2705d16
// by Claude <noreply@anthropic.com>. Saturation and hint changes are not ported.

use super::super::{CortexM, PcAdvance};
use crate::decoder::ArmInstruction as Instruction;
use crate::SimResult;

impl CortexM {
    pub(in crate::cpu::cortex_m) fn exec_dsp_multiply(
        &mut self,
        instruction: Instruction,
    ) -> SimResult<PcAdvance> {
        match instruction {
            Instruction::WordHalfwordMul {
                rd,
                rn,
                rm,
                ra,
                m_high,
            } => {
                let m = half(self.read_reg(rm), m_high);
                // SMULW/SMLAW use the signed 32x16 product's bits [47:16].
                let product = (((self.read_reg(rn) as i32 as i64) * (m as i64)) >> 16) as i32;
                // Ra == 0b1111 is the plain multiply form (SMUL*/SMULW*),
                // which has no accumulate and cannot set Q.
                let value = if ra == 0xF {
                    product
                } else {
                    let acc = self.read_reg(ra) as i32;
                    let (sum, overflow) = acc.overflowing_add(product);
                    if overflow {
                        self.xpsr |= 1 << 27;
                    }
                    sum
                };
                self.write_reg(rd, value as u32);
            }
            Instruction::DualMul {
                rd,
                rn,
                rm,
                ra,
                swap,
                sub,
            } => {
                let (p1, p2) = dual_products(self.read_reg(rn), self.read_reg(rm), swap);
                // The two products are combined in 64-bit precision; only the
                // final result can overflow into Q (A7.7.146/.154).
                let combined = if sub { p1 - p2 } else { p1 + p2 };
                let value = if ra == 0xF {
                    combined
                } else {
                    combined + (self.read_reg(ra) as i32 as i64)
                };
                if value != (value as i32) as i64 {
                    self.xpsr |= 1 << 27;
                }
                self.write_reg(rd, value as u32);
            }
            Instruction::TopWordMul {
                rd,
                rn,
                rm,
                ra,
                round,
                sub,
            } => {
                let product =
                    (self.read_reg(rn) as i32 as i64).wrapping_mul(self.read_reg(rm) as i32 as i64);
                // SMMUL has no accumulator (Ra == 0b1111); SMMLA adds it and
                // SMMLS subtracts the product from it, both at bit 32.
                let acc = if ra == 0xF {
                    0i64
                } else {
                    (self.read_reg(ra) as i32 as i64) << 32
                };
                let mut total = if sub {
                    acc.wrapping_sub(product)
                } else {
                    acc.wrapping_add(product)
                };
                if round {
                    total = total.wrapping_add(0x8000_0000);
                }
                self.write_reg(rd, (total >> 32) as u32);
            }
            Instruction::SmlalXy {
                rd_lo,
                rd_hi,
                rn,
                rm,
                n_high,
                m_high,
            } => {
                let acc = ((self.read_reg(rd_hi) as u64) << 32) | (self.read_reg(rd_lo) as u64);
                let n = half(self.read_reg(rn), n_high) as i64;
                let m = half(self.read_reg(rm), m_high) as i64;
                let new = (acc as i64).wrapping_add(n.wrapping_mul(m)) as u64;
                self.write_reg(rd_lo, new as u32);
                self.write_reg(rd_hi, (new >> 32) as u32);
            }
            Instruction::SmlaldSld {
                rd_lo,
                rd_hi,
                rn,
                rm,
                swap,
                sub,
            } => {
                let acc = ((self.read_reg(rd_hi) as u64) << 32) | (self.read_reg(rd_lo) as u64);
                let (p1, p2) = dual_products(self.read_reg(rn), self.read_reg(rm), swap);
                let combined = if sub { p1 - p2 } else { p1 + p2 };
                let new = (acc as i64).wrapping_add(combined) as u64;
                self.write_reg(rd_lo, new as u32);
                self.write_reg(rd_hi, (new >> 32) as u32);
            }
            _ => unreachable!("DSP dispatch only accepts multiply variants"),
        }
        Ok(PcAdvance::Add4)
    }
}

/// Sign-extend one halfword of `value` to i32. `high` picks bits [31:16]
/// (the `T` in SMLAT*/SMLA*T) over bits [15:0].
fn half(value: u32, high: bool) -> i32 {
    let h = if high {
        (value >> 16) as u16
    } else {
        value as u16
    };
    h as i16 as i32
}

/// The two halfword products shared by SMUAD/SMUSD/SMLAD/SMLSD/SMLALD/SMLSLD.
///
/// Returned as i64 so the caller can combine them without wrapping and then
/// decide for itself whether the 32-bit result overflowed — the dual multiplies
/// only set Q on the final result, not on the intermediate sum.
/// `swap` is the `X` suffix: exchange Rm's halfwords first.
fn dual_products(rn: u32, rm: u32, swap: bool) -> (i64, i64) {
    let m_lo = half(rm, swap);
    let m_hi = half(rm, !swap);
    (
        (half(rn, false) as i64) * (m_lo as i64),
        (half(rn, true) as i64) * (m_hi as i64),
    )
}
