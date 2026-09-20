// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Xtensa LX7 JIT — runtime-agnostic walker + emit core (#124 Phase 4.1).
//!
//! This module is **always** compiled (no `jit` feature gate, no wasmtime
//! dependency). It owns the parts of the JIT pipeline that don't care
//! whether the resulting wasm bytes end up in `wasmtime::Module::new`
//! (native) or `js_sys::WebAssembly::Module::new` (browser):
//!
//!   1. The basic-block walker ([`walk_bb`]) — decodes Xtensa instructions
//!      forward from a given PC until a terminator / unsupported opcode.
//!   2. The opcode allowlist ([`is_supported`]) + terminator predicate
//!      ([`is_terminator`]).
//!   3. The end-to-end entry point ([`walk_and_emit`]) — given a flat
//!      slice of the bus that contains the candidate PC, produces an
//!      [`EmittedBlock`] containing the wasm bytes that both backends
//!      consume.
//!
//! ## Why this lives outside the `jit` feature gate
//!
//! The browser-side prototype in `labwired-wasm` cannot enable the `jit`
//! feature (wasmtime doesn't build for `wasm32-unknown-unknown`). But it
//! still needs the walker + the emitted wasm bytes. Phase 4.0 (PR #131)
//! already established this split by baking the canonical hot-block
//! bytes at crate build time into [`crate::cpu::xtensa_jit_bytes::HOT_BB_WASM`].
//! Phase 4.1 generalises that: `walk_and_emit` accepts an arbitrary PC,
//! walks the bus to decode the BB, and returns the bytes both backends
//! consume.
//!
//! ## Current emit scope (4.1)
//!
//! Phase 4.1 is a *refactor*: the actual byte-stream-emit-per-opcode work
//! lives in the canonical [`crate::cpu::xtensa_jit::hot_bb.wat`] source
//! and is compiled to wasm bytes by `crates/core/build.rs`. The walker +
//! [`walk_and_emit`] recognises the eight-instruction polling-block shape at
//! any linked PC and with any logical-register allocation, then reuses the
//! pre-baked [`HOT_BB_WASM`] bytes with an explicit host ABI manifest. Per-opcode
//! runtime emit functions are stubbed below ([`emit_or`], [`emit_l8ui`],
//! …) so Phase 4.2/4.3 can fill them in without further surgery on the
//! native / browser adapters.
//!
//! [`HOT_BB_PC`]: crate::cpu::xtensa_jit_bytes::HOT_BB_PC
//! [`HOT_BB_WASM`]: crate::cpu::xtensa_jit_bytes::HOT_BB_WASM

use crate::cpu::xtensa_jit_bytes::{
    EXIT_FALL_THROUGH, EXIT_HOST_BUS_ERROR, HOT_BB_INSTR_COUNT, HOT_BB_WASM,
};
use crate::decoder::xtensa::{self, Instruction};
use crate::decoder::{xtensa_length, xtensa_narrow};

// ── Public side-exit / shape vocabulary ───────────────────────────────

/// Reason an emitted block can side-exit early. The actual `i32` code in
/// the wasm body comes from [`crate::cpu::xtensa_jit_bytes`] so native +
/// browser agree on the wire values; this enum is the runtime-agnostic
/// view for diagnostics + Phase 4.2 control-flow emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideExitReason {
    /// Block executed cleanly to the terminator. Wire code:
    /// [`EXIT_FALL_THROUGH`].
    FallThrough,
    /// A host import (e.g. `read_u8`) reported a bus error. Wire code:
    /// [`EXIT_HOST_BUS_ERROR`].
    HostBusError,
}

impl SideExitReason {
    /// Wire side-exit code emitted into the wasm body. Native +
    /// browser dispatch on these identical i32 values.
    #[inline]
    pub fn wire_code(self) -> i32 {
        match self {
            SideExitReason::FallThrough => EXIT_FALL_THROUGH,
            SideExitReason::HostBusError => EXIT_HOST_BUS_ERROR,
        }
    }
}

/// Failure reasons from [`walk_and_emit`]. None of these are bugs — they
/// just mean "this PC isn't JIT-able yet"; the caller falls back to the
/// interpreter and the BB walks back through the regular dispatch path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitError {
    /// The walked BB doesn't match any currently-supported shape.
    UnsupportedShape,
    /// The walker refused (unsupported opcode mid-block, or the PC
    /// pointed outside the supplied `bus_slice`).
    WalkRefused,
    /// The walked block was empty (only a terminator at `pc`).
    BlockTooShort,
    /// PC outside the supplied bus slice — caller passed a slice that
    /// doesn't cover the block under consideration.
    PcOutOfRange,
}

impl core::fmt::Display for EmitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmitError::UnsupportedShape => f.write_str("BB shape not yet supported by emit core"),
            EmitError::WalkRefused => f.write_str("BB walker refused (unsupported opcode or OOB)"),
            EmitError::BlockTooShort => f.write_str("BB walker returned no non-terminator ops"),
            EmitError::PcOutOfRange => f.write_str("PC outside the supplied bus slice"),
        }
    }
}

impl std::error::Error for EmitError {}

/// Subset of PS that affects JIT validity. Currently informational —
/// Phase 4.1 only emits straight-line arithmetic that doesn't depend on
/// PS. Phase 4.4 (CALL8/RETW) will read CALLINC/WOE from here. Carried
/// in [`walk_and_emit`]'s signature now so adding consumers later is
/// not an API break.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PsBits {
    /// Raw PS register value (see [`crate::cpu::xtensa_regs::Ps`]).
    pub raw: u32,
}

impl PsBits {
    /// Construct from a raw PS value (typically `cpu.ps.as_raw()`).
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self { raw }
    }
}

/// One emit pass output: the wasm bytes plus the metadata both backends
/// need to commit register state, advance PC, and bump CCOUNT after the
/// block runs.
///
/// `wasm_bytes` is a `Vec<u8>` rather than a `&'static [u8]` so future
/// runtime-emit code (Phase 4.2+) can produce bytes per-BB without
/// requiring a static lifetime. Cloning a baked block reuses the static
/// bytes (one allocation, no per-call cost on the hot path because the
/// JitCache holds the compiled `Module` long-term).
#[derive(Debug, Clone)]
pub struct EmittedBlock {
    /// Wasm module bytes. Magic+version validated by the backend at
    /// `Module::new` time, not here.
    pub wasm_bytes: Vec<u8>,
    /// Number of Xtensa instructions the wasm body executes. Both
    /// backends advance CCOUNT by `length_in_instrs - 1` after a clean
    /// fall-through (the outer step already counted one).
    pub length_in_instrs: u32,
    /// First PC after the JIT'd range — the interpreter resumes here
    /// when the block fall-throughs.
    pub end_pc: u32,
    /// Side-exit codes the emitted body can produce, paired with their
    /// reason. Backends use this to map a returned i32 to "commit
    /// state" vs "refuse + fall back to interp".
    pub side_exit_reasons: Vec<(i32, SideExitReason)>,
    /// Register/constant mapping for the emitted multi-op ABI. The wasm
    /// module's two register parameters and four register results are
    /// positional; this manifest maps them back to arbitrary Xtensa logical
    /// registers so the same compiled body is not tied to one linked PC.
    pub abi: MultiOpAbi,
}

/// Host marshalling manifest for the currently emitted eight-op polling
/// block. `input_regs` are `(load_base, move_source)` and `output_regs` are
/// `(and_result, first_load, literal_result, move_result)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiOpAbi {
    pub input_regs: [u8; 2],
    pub output_regs: [u8; 4],
    pub l32r_addr: u32,
}

impl EmittedBlock {
    /// Look up the [`SideExitReason`] for a wire exit code emitted by
    /// `self.wasm_bytes`. Returns `None` if the code isn't in
    /// `side_exit_reasons` — the backend treats that as a sim-level
    /// "unknown side-exit" error.
    #[inline]
    pub fn reason_for(&self, code: i32) -> Option<SideExitReason> {
        self.side_exit_reasons
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, r)| *r)
    }
}

// ── Walker — decoded ops + control predicates ─────────────────────────

/// One decoded Xtensa op + its byte length. Used by the BB walker.
#[derive(Debug, Clone)]
pub struct DecodedOp {
    pub pc: u32,
    pub len: u32,
    pub ins: Instruction,
}

/// Walk forward from `start_pc`, decoding instructions out of `text`
/// (a flat slice mapping PC → byte). Stops when:
///   * a terminator (any control transfer) is reached — terminator is
///     **excluded** from the returned vec.
///   * an unsupported opcode is hit — returns `None` (refuse the whole BB).
///   * `max_ops` instructions have been collected — returns what we have.
///
/// `pc_to_offset` converts a PC to an index into `text`; returns `None`
/// if the PC is outside `text`.
pub fn walk_bb<F>(
    start_pc: u32,
    mut pc_to_offset: F,
    text: &[u8],
    max_ops: usize,
) -> Option<Vec<DecodedOp>>
where
    F: FnMut(u32) -> Option<usize>,
{
    let mut ops = Vec::with_capacity(max_ops);
    let mut pc = start_pc;
    while ops.len() < max_ops {
        let off = pc_to_offset(pc)?;
        if off >= text.len() {
            return None;
        }
        let b0 = text[off];
        let len: u32 = xtensa_length::instruction_length(b0);
        // Verify the full instruction fits inside `text`.
        if off + (len as usize) > text.len() {
            return None;
        }
        let ins = if len == 2 {
            let hw = u16::from_le_bytes([text[off], text[off + 1]]);
            xtensa_narrow::decode_narrow(hw)
        } else if len == 3 {
            let w = u32::from_le_bytes([text[off], text[off + 1], text[off + 2], 0]);
            xtensa::decode(w)
        } else {
            return None;
        };
        if is_terminator(&ins) {
            return Some(ops);
        }
        if !is_supported(&ins) {
            return None;
        }
        ops.push(DecodedOp { pc, len, ins });
        pc = pc.wrapping_add(len);
    }
    Some(ops)
}

/// Is this opcode a basic-block terminator (control transfer)?
pub fn is_terminator(ins: &Instruction) -> bool {
    use Instruction::*;
    matches!(
        ins,
        Call0 { .. }
            | Call4 { .. }
            | Call8 { .. }
            | Call12 { .. }
            | Callx0 { .. }
            | Callx4 { .. }
            | Callx8 { .. }
            | Callx12 { .. }
            | Ret
            | Retw
            | Jx { .. }
            | Beq { .. }
            | Bne { .. }
            | Blt { .. }
            | Bge { .. }
            | Bltu { .. }
            | Bgeu { .. }
            | Beqz { .. }
            | Bnez { .. }
            | Bltz { .. }
            | Bgez { .. }
            | Bt { .. }
            | Bf { .. }
            | Beqi { .. }
            | Bnei { .. }
            | Blti { .. }
            | Bgei { .. }
            | Bltui { .. }
            | Bgeui { .. }
            | Bany { .. }
            | Ball { .. }
            | Bnone { .. }
            | Bnall { .. }
            | Bbc { .. }
            | Bbs { .. }
            | Bbci { .. }
            | Bbsi { .. }
            | Entry { .. }
            | Rfe
            | Rfde
            | Rfi { .. }
            | Rfwo
            | Rfwu
            | Ill
    )
}

/// Is this opcode in the Phase 4.1 supported set?
///
/// Keep this list narrow: any new opcode here needs corresponding
/// per-opcode emit code below (`emit_*`). Adding more is Phase 4.2+
/// work and must come paired with lockstep coverage.
pub fn is_supported(ins: &Instruction) -> bool {
    use Instruction::*;
    matches!(
        ins,
        // Pure arithmetic / bitwise
        Add { .. }
            | Sub { .. }
            | And { .. }
            | Or { .. }
            | Xor { .. }
            | Addi { .. }
            | Movi { .. }
            | Extui { .. }
            // Loads
            | L8ui { .. }
            | L32r { .. }
            // Barriers — semantic no-ops in sim
            | Memw
            | Nop
    )
}

// ── walk_and_emit — runtime-agnostic entry point ──────────────────────

/// Walk the BB starting at `pc` (using `bus_slice` indexed by
/// `pc_to_offset`) and produce an [`EmittedBlock`] of wasm bytes both
/// backends can consume.
///
/// `bus_slice` is the flat slice of host memory the BB is decoded out
/// of; `pc_to_offset` maps a PC to an index inside that slice. The
/// canonical caller is `try_jit_multi_op` which extracts an IRAM slice
/// covering `pc` and supplies the offset closure.
///
/// `ps_bits` is currently informational — see [`PsBits`].
///
/// The BB walker is fully position-independent. The currently emitted shape
/// is the common eight-op byte-polling block; its PC, register allocation and
/// L32R literal address are decoded into [`MultiOpAbi`] rather than hard-coded.
pub fn walk_and_emit<F>(
    bus_slice: &[u8],
    pc: u32,
    pc_to_offset: F,
    _ps_bits: PsBits,
) -> Result<EmittedBlock, EmitError>
where
    F: FnMut(u32) -> Option<usize>,
{
    // Cap the walk at `HOT_BB_INSTR_COUNT + 2` — Phase 4.1 only emits
    // the 8-op hot block; budget two slack ops so a shape mismatch
    // surfaces as UnsupportedShape rather than a silent truncation.
    let max_ops = (HOT_BB_INSTR_COUNT as usize) + 2;
    let ops = walk_bb(pc, pc_to_offset, bus_slice, max_ops).ok_or(EmitError::WalkRefused)?;
    if ops.is_empty() {
        return Err(EmitError::BlockTooShort);
    }

    if let Some(abi) = polling_block_abi(&ops) {
        Ok(EmittedBlock {
            wasm_bytes: HOT_BB_WASM.to_vec(),
            length_in_instrs: HOT_BB_INSTR_COUNT,
            end_pc: ops.last().unwrap().pc.wrapping_add(ops.last().unwrap().len),
            side_exit_reasons: vec![
                (EXIT_FALL_THROUGH, SideExitReason::FallThrough),
                (EXIT_HOST_BUS_ERROR, SideExitReason::HostBusError),
            ],
            abi,
        })
    } else {
        Err(EmitError::UnsupportedShape)
    }
}

/// Does this decoded op sequence match the canonical hot-BB shape?
///
/// `0x400829cc`:
/// ```text
///   or    a10,a5,a5
///   memw
///   l8ui  a6,a3,0
///   memw
///   l8ui  a2,a3,1
///   extui a2,a2,0,8
///   and   a2,a2,a6
///   l32r  a8,0x40080534
/// ```
fn polling_block_abi(ops: &[DecodedOp]) -> Option<MultiOpAbi> {
    use Instruction::*;
    if ops.len() != HOT_BB_INSTR_COUNT as usize {
        return None;
    }
    let Or {
        ar: move_dst,
        as_: move_src,
        at: move_src_2,
    } = ops[0].ins
    else {
        return None;
    };
    let L8ui {
        at: load0,
        as_: base,
        imm: 0,
    } = ops[2].ins
    else {
        return None;
    };
    let L8ui {
        at: load1,
        as_: base_2,
        imm: 1,
    } = ops[4].ins
    else {
        return None;
    };
    let L32r {
        at: literal_dst,
        pc_rel_byte_offset,
    } = ops[7].ins
    else {
        return None;
    };
    if move_src != move_src_2
        || base != base_2
        || !matches!(ops[1].ins, Memw)
        || !matches!(ops[3].ins, Memw)
        || !matches!(ops[5].ins, Extui { ar, at, shift: 0, bits: 8 } if ar == load1 && at == load1)
        || !matches!(ops[6].ins, And { ar, as_, at } if ar == load1 && as_ == load1 && at == load0)
    {
        return None;
    }
    let l32r_base = (ops[7].pc.wrapping_add(3)) & !3;
    Some(MultiOpAbi {
        input_regs: [base, move_src],
        output_regs: [load1, load0, literal_dst, move_dst],
        l32r_addr: l32r_base.wrapping_add(pc_rel_byte_offset as u32),
    })
}

// ── Per-opcode emit stubs ─────────────────────────────────────────────
//
// Phase 4.1 keeps the canonical wasm body in `hot_bb.wat`; each per-op
// emit function below currently delegates to that pre-baked artifact
// (effectively a no-op for the runtime path). Phase 4.2 will replace
// these stubs with real wasm-byte emission so `walk_and_emit` can build
// a block byte stream for any sequence of supported ops, not just the
// canonical shape.
//
// The signatures live here today so the Phase 4.2 wiring is a fill-in
// rather than a rewrite. Each stub returns a `Result` so a future
// "unsupported operand" path can plumb through without an API break.
//
// NOTE: these are deliberately not exposed via `pub use` until Phase
// 4.2 fills them in — exposing empty stubs would risk callers binding
// to incorrect output.

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_or(_ar: u8, _as_: u8, _at: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_memw(_out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_nop(_out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_l8ui(_at: u8, _as_: u8, _imm: u32, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_l32r(_at: u8, _literal: u32, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_extui(_ar: u8, _at: u8, _shift: u8, _bits: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_and(_ar: u8, _as_: u8, _at: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_add(_ar: u8, _as_: u8, _at: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_sub(_ar: u8, _as_: u8, _at: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_xor(_ar: u8, _as_: u8, _at: u8, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_addi(_at: u8, _as_: u8, _imm8: i32, _out: &mut Vec<u8>) {}

#[allow(dead_code, reason = "Phase 4.2 stub — fills in real emit logic")]
pub(crate) fn emit_movi(_at: u8, _imm: i32, _out: &mut Vec<u8>) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walker stops at a terminator and excludes it from the returned vec.
    #[test]
    fn walker_stops_at_terminator() {
        // 4 bytes: two NOP.N (0x3d 0xf0) then a RET.N (0x0d 0xf0).
        let text: Vec<u8> = vec![0x3d, 0xf0, 0x3d, 0xf0, 0x0d, 0xf0];
        let ops = walk_bb(0, |pc| Some(pc as usize), &text, 16).unwrap();
        assert_eq!(ops.len(), 2, "should collect 2 NOP.Ns then stop at RET.N");
        for op in &ops {
            assert!(matches!(op.ins, Instruction::Nop));
        }
    }

    /// Walker refuses unsupported opcodes (returns None).
    #[test]
    fn walker_refuses_unsupported() {
        // SSL a3 — not in `is_supported`.
        let text: Vec<u8> = vec![0x40, 0x13, 0x40, 0x00, 0x00, 0x00];
        let ops = walk_bb(0, |pc| Some(pc as usize), &text, 16);
        assert!(ops.is_none(), "must refuse unsupported opcode");
    }

    /// `walk_and_emit` returns `UnsupportedShape` for non-hot-BB PCs.
    #[test]
    fn walk_and_emit_unknown_pc_unsupported() {
        let text: Vec<u8> = vec![0x3d, 0xf0, 0x0d, 0xf0];
        let err = walk_and_emit(&text, 0, |pc| Some(pc as usize), PsBits::default()).unwrap_err();
        assert_eq!(err, EmitError::UnsupportedShape);
    }

    #[test]
    fn polling_block_is_position_independent() {
        const BYTES: &[u8] = &[
            0x50, 0xa5, 0x20, 0xc0, 0x20, 0x00, 0x62, 0x03, 0x00, 0xc0, 0x20, 0x00, 0x22, 0x03,
            0x01, 0x20, 0x20, 0x74, 0x60, 0x22, 0x10, 0x81, 0xd4, 0xf6, 0xe0, 0x08, 0x00,
        ];
        let relocated_pc = 0x4200_1000;
        let block = walk_and_emit(
            BYTES,
            relocated_pc,
            |pc| Some(pc.wrapping_sub(relocated_pc) as usize),
            PsBits::default(),
        )
        .unwrap();

        assert_eq!(block.end_pc, relocated_pc + 24);
        assert_eq!(block.abi.input_regs, [3, 5]);
        assert_eq!(block.abi.output_regs, [2, 6, 8, 10]);
        assert_eq!(
            block.abi.l32r_addr,
            crate::cpu::xtensa_jit_bytes::HOT_BB_L32R_ADDR
                .wrapping_add(relocated_pc.wrapping_sub(crate::cpu::xtensa_jit_bytes::HOT_BB_PC),)
        );
    }

    #[test]
    fn polling_block_uses_decoded_register_manifest() {
        const BYTES: &[u8] = &[
            0x70, 0xb7, 0x20, // or a11,a7,a7
            0xc0, 0x20, 0x00, // memw
            0x92, 0x04, 0x00, // l8ui a9,a4,0
            0xc0, 0x20, 0x00, // memw
            0x32, 0x04, 0x01, // l8ui a3,a4,1
            0x30, 0x30, 0x74, // extui a3,a3,0,8
            0x90, 0x33, 0x10, // and a3,a3,a9
            0xc1, 0xd4, 0xf6, // l32r a12,...
            0xe0, 0x08, 0x00, // callx8 terminator
        ];
        let pc = 0x4200_1000;
        let block = walk_and_emit(
            BYTES,
            pc,
            |candidate| Some(candidate.wrapping_sub(pc) as usize),
            PsBits::default(),
        )
        .unwrap();

        assert_eq!(block.abi.input_regs, [4, 7]);
        assert_eq!(block.abi.output_regs, [3, 9, 12, 11]);
    }

    /// `walk_and_emit` propagates walker failures as `WalkRefused`.
    #[test]
    fn walk_and_emit_walker_refused_propagates() {
        // SSL a3 — refused by walker.
        let text: Vec<u8> = vec![0x40, 0x13, 0x40];
        let err = walk_and_emit(&text, 0, |pc| Some(pc as usize), PsBits::default()).unwrap_err();
        assert_eq!(err, EmitError::WalkRefused);
    }

    /// `SideExitReason::wire_code` round-trips through
    /// `EmittedBlock::reason_for` (sanity for backends that index by
    /// the i32 returned from wasm).
    #[test]
    fn side_exit_reason_round_trips() {
        let block = EmittedBlock {
            wasm_bytes: vec![0, b'a', b's', b'm', 1, 0, 0, 0],
            length_in_instrs: 1,
            end_pc: 0,
            side_exit_reasons: vec![
                (EXIT_FALL_THROUGH, SideExitReason::FallThrough),
                (EXIT_HOST_BUS_ERROR, SideExitReason::HostBusError),
            ],
            abi: MultiOpAbi {
                input_regs: [3, 5],
                output_regs: [2, 6, 8, 10],
                l32r_addr: crate::cpu::xtensa_jit_bytes::HOT_BB_L32R_ADDR,
            },
        };
        assert_eq!(
            block.reason_for(EXIT_FALL_THROUGH),
            Some(SideExitReason::FallThrough)
        );
        assert_eq!(
            block.reason_for(EXIT_HOST_BUS_ERROR),
            Some(SideExitReason::HostBusError)
        );
        assert_eq!(block.reason_for(42), None);
    }
}
