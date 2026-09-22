// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Checked exact arithmetic for opt-in derived producers. Outer slots stay f64;
//! local bindings and inverse candidates never cross that precision boundary.
use anyhow::{anyhow, bail, Context, Result};
use labwired_config::IntegerBinding;
use std::collections::HashMap;

pub(super) const EXACT_LIMIT: f64 = 9007199254740992.0;
const MAX_NODES: usize = 512;
const MAX_DEPTH: usize = 64;
const MAX_BINDINGS: usize = 128;

pub(super) fn import(value: f64) -> Result<i64> {
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > EXACT_LIMIT {
        bail!("integer import must be finite, integral and within [-2^53, 2^53], got {value}");
    }
    Ok(value as i64)
}
pub(super) fn export(value: i64) -> Result<f64> {
    if !(-(1_i64 << 53)..=(1_i64 << 53)).contains(&value) {
        bail!("integer export {value} is outside [-2^53, 2^53]");
    }
    Ok(value as f64)
}

#[derive(Debug, Clone)]
enum Expr {
    Const(i64),
    Outer(String),
    Local(usize),
    Neg(Box<Self>),
    Abs(Box<Self>),
    Bin(Op, Box<Self>, Box<Self>),
}
#[derive(Debug, Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Shl,
    Shr,
}
impl Expr {
    fn eval(&self, outer: &HashMap<String, f64>, local: &[i64]) -> Result<i64> {
        let overflow = || anyhow!("checked integer arithmetic overflow");
        match self {
            Self::Const(v) => Ok(*v),
            Self::Outer(n) => import(
                *outer
                    .get(n)
                    .ok_or_else(|| anyhow!("missing integer input '{n}'"))?,
            )
            .with_context(|| format!("input '{n}'")),
            Self::Local(i) => local
                .get(*i)
                .copied()
                .ok_or_else(|| anyhow!("missing integer local {i}")),
            Self::Neg(a) => a.eval(outer, local)?.checked_neg().ok_or_else(overflow),
            Self::Abs(a) => a.eval(outer, local)?.checked_abs().ok_or_else(overflow),
            Self::Bin(op, a, b) => {
                let (a, b) = (a.eval(outer, local)?, b.eval(outer, local)?);
                match op {
                    Op::Add => a.checked_add(b).ok_or_else(overflow),
                    Op::Sub => a.checked_sub(b).ok_or_else(overflow),
                    Op::Mul => a.checked_mul(b).ok_or_else(overflow),
                    Op::Div => a
                        .checked_div(b)
                        .ok_or_else(|| anyhow!("integer division by zero or overflow")),
                    Op::Min => Ok(a.min(b)),
                    Op::Max => Ok(a.max(b)),
                    Op::Shl | Op::Shr => {
                        if !(0..=63).contains(&b) {
                            bail!("integer shift count {b} outside [0, 63]");
                        }
                        if matches!(op, Op::Shr) {
                            Ok(a >> b)
                        } else {
                            i64::try_from((a as i128) << b).map_err(|_| overflow())
                        }
                    }
                }
            }
        }
    }
    fn depth(&self) -> usize {
        match self {
            Self::Neg(a) | Self::Abs(a) => 1 + a.depth(),
            Self::Bin(_, a, b) => 1 + a.depth().max(b.depth()),
            _ => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct IntExpr(Expr);
impl IntExpr {
    pub(super) fn compile(src: &str, known: &[String]) -> Result<Self> {
        Ok(Self(Parser::new(src, known, &[])?.expression()?))
    }
    pub(super) fn eval(&self, slots: &HashMap<String, f64>) -> Result<i64> {
        self.0.eval(slots, &[])
    }
}
#[derive(Debug, Clone)]
pub(super) struct Guard {
    a: Expr,
    cmp: String,
    b: Expr,
}
impl Guard {
    pub(super) fn compile(src: &str, known: &[String]) -> Result<Self> {
        Self::parse(src, known, &[])
    }
    fn parse(src: &str, known: &[String], local: &[String]) -> Result<Self> {
        let mut p = Parser::new(src, known, local)?;
        let a = p.sum(0)?;
        let cmp = p
            .next()
            .ok_or_else(|| anyhow!("integer guard requires comparison"))?
            .to_string();
        if ![">", ">=", "<", "<=", "==", "!="].contains(&cmp.as_str()) {
            bail!("integer guard requires comparison");
        }
        let b = p.sum(0)?;
        p.finish(&a)?;
        p.finish(&b)?;
        Ok(Self { a, cmp, b })
    }
    fn holds(&self, outer: &HashMap<String, f64>, local: &[i64]) -> Result<bool> {
        let (a, b) = (self.a.eval(outer, local)?, self.b.eval(outer, local)?);
        Ok(match self.cmp.as_str() {
            ">" => a > b,
            ">=" => a >= b,
            "<" => a < b,
            "<=" => a <= b,
            "==" => a == b,
            _ => a != b,
        })
    }
    pub(super) fn holds_outer(&self, outer: &HashMap<String, f64>) -> Result<bool> {
        self.holds(outer, &[])
    }
}

#[derive(Debug, Clone)]
struct Binding {
    name: String,
    expr: Expr,
    guard: Option<Guard>,
}
#[derive(Debug, Clone)]
pub(super) struct Program {
    bindings: Vec<Binding>,
    of: Expr,
    guard: Option<Guard>,
}
impl Program {
    pub(super) fn compile(
        bindings: &[IntegerBinding],
        of: &str,
        when: Option<&str>,
        variable: Option<&str>,
        known: &[String],
    ) -> Result<Self> {
        if bindings.len() > MAX_BINDINGS {
            bail!("integer program exceeds {MAX_BINDINGS} bindings");
        }
        let mut local = Vec::new();
        if let Some(v) = variable {
            validate_name(v, known, &local)?;
            local.push(v.to_string());
        }
        let mut compiled = Vec::new();
        for b in bindings {
            validate_name(&b.name, known, &local)?;
            let expr = Parser::new(&b.expr, known, &local)?
                .expression()
                .with_context(|| format!("binding '{}'", b.name))?;
            let guard = b
                .when
                .as_deref()
                .map(|g| Guard::parse(g, known, &local))
                .transpose()
                .with_context(|| format!("binding '{}' guard", b.name))?;
            compiled.push(Binding {
                name: b.name.clone(),
                expr,
                guard,
            });
            local.push(b.name.clone());
        }
        Ok(Self {
            bindings: compiled,
            of: Parser::new(of, known, &local)?.expression()?,
            guard: when.map(|g| Guard::parse(g, known, &local)).transpose()?,
        })
    }
    pub(super) fn eval(&self, outer: &HashMap<String, f64>, candidate: Option<i64>) -> Result<i64> {
        let mut local = Vec::with_capacity(self.bindings.len() + 1);
        if let Some(v) = candidate {
            local.push(v);
        }
        for b in &self.bindings {
            let value = (|| {
                if let Some(g) = &b.guard {
                    if !g.holds(outer, &local)? {
                        return Ok(0);
                    }
                }
                b.expr.eval(outer, &local)
            })()
            .with_context(|| format!("binding '{}'", b.name))?;
            local.push(value);
        }
        if let Some(g) = &self.guard {
            if !g.holds(outer, &local)? {
                return Ok(0);
            }
        }
        self.of.eval(outer, &local).context("integer output")
    }
}
fn validate_name(name: &str, known: &[String], local: &[String]) -> Result<()> {
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        bail!("invalid integer binding name '{name}'");
    }
    if known.iter().chain(local).any(|n| n == name) {
        bail!("integer binding '{name}' shadows an existing name");
    }
    Ok(())
}

/// Lower-bound nearest inversion. The author guarantees nondecreasing output.
/// There is deliberately no endpoint-based monotonicity claim or exact-match exit.
pub(super) fn inverse(
    program: &Program,
    slots: &HashMap<String, f64>,
    bounds: [i64; 2],
    target: i64,
) -> Result<i64> {
    let [lower, upper] = bounds;
    let (mut lo, mut hi) = (lower, upper);
    while lo < hi {
        let mid = (lo as i128 + (hi as i128 - lo as i128) / 2) as i64;
        if program.eval(slots, Some(mid))? < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    // Validate the chosen candidate even when the domain has one point and
    // neither the search loop nor the predecessor comparison runs.
    let here = (program.eval(slots, Some(lo))? as i128 - target as i128).abs();
    if lo > lower {
        let prev = (program.eval(slots, Some(lo - 1))? as i128 - target as i128).abs();
        if prev <= here {
            return Ok(lo - 1);
        }
    }
    Ok(lo)
}

/// Tokens retain decimal text: integer literals are never parsed through f64.
struct Parser<'a> {
    tokens: Vec<&'a str>,
    pos: usize,
    nodes: usize,
    known: &'a [String],
    local: &'a [String],
}
impl<'a> Parser<'a> {
    fn new(src: &'a str, known: &'a [String], local: &'a [String]) -> Result<Self> {
        if src.len() > 8192 {
            bail!("integer expression exceeds source limit");
        }
        let bytes = src.as_bytes();
        let mut i = 0;
        let mut tokens = Vec::new();
        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            let start = i;
            let c = bytes[i];
            i += 1;
            if c.is_ascii_digit() {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            } else if c.is_ascii_alphabetic() || c == b'_' {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            } else if b"<>!=".contains(&c) {
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                }
            } else if !b"+-*/(),".contains(&c) {
                bail!("unexpected character in integer expression");
            }
            tokens.push(&src[start..i]);
            if tokens.len() > 2048 {
                bail!("integer expression exceeds token limit");
            }
        }
        Ok(Self {
            tokens,
            pos: 0,
            nodes: 0,
            known,
            local,
        })
    }
    fn next(&mut self) -> Option<&'a str> {
        let t = self.tokens.get(self.pos).copied();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &str) -> bool {
        if self.tokens.get(self.pos) == Some(&t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &str) -> Result<()> {
        if self.eat(t) {
            Ok(())
        } else {
            bail!("expected '{t}' in integer expression")
        }
    }
    fn node(&mut self, e: Expr) -> Result<Expr> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            bail!("integer expression exceeds {MAX_NODES} nodes");
        }
        Ok(e)
    }
    fn expression(mut self) -> Result<Expr> {
        let e = self.sum(0)?;
        self.finish(&e)?;
        Ok(e)
    }
    fn finish(&self, e: &Expr) -> Result<()> {
        if self.pos != self.tokens.len() {
            bail!("trailing input in integer expression");
        }
        if e.depth() > MAX_DEPTH {
            bail!("integer expression exceeds depth {MAX_DEPTH}");
        }
        Ok(())
    }
    fn sum(&mut self, depth: usize) -> Result<Expr> {
        let mut a = self.product(depth)?;
        loop {
            let op = if self.eat("+") {
                Op::Add
            } else if self.eat("-") {
                Op::Sub
            } else {
                break;
            };
            let b = self.product(depth)?;
            a = self.node(Expr::Bin(op, Box::new(a), Box::new(b)))?;
        }
        Ok(a)
    }
    fn product(&mut self, depth: usize) -> Result<Expr> {
        let mut a = self.atom(depth)?;
        loop {
            let op = if self.eat("*") {
                Op::Mul
            } else if self.eat("/") {
                Op::Div
            } else {
                break;
            };
            let b = self.atom(depth)?;
            a = self.node(Expr::Bin(op, Box::new(a), Box::new(b)))?;
        }
        Ok(a)
    }
    fn atom(&mut self, depth: usize) -> Result<Expr> {
        if depth >= MAX_DEPTH {
            bail!("integer expression exceeds depth {MAX_DEPTH}");
        }
        if self.eat("-") {
            // Permit the minimum signed literal without admitting positive 2^63.
            if self.tokens.get(self.pos) == Some(&"9223372036854775808") {
                self.pos += 1;
                return self.node(Expr::Const(i64::MIN));
            }
            let a = self.atom(depth + 1)?;
            return self.node(Expr::Neg(Box::new(a)));
        }
        if self.eat("(") {
            let a = self.sum(depth + 1)?;
            self.expect(")")?;
            return Ok(a);
        }
        let t = self
            .next()
            .ok_or_else(|| anyhow!("integer expression ended early"))?;
        if t.as_bytes()[0].is_ascii_digit() {
            return self.node(Expr::Const(
                t.parse::<i64>()
                    .context("integer literal outside i64 range")?,
            ));
        }
        if self.eat("(") {
            let a = self.sum(depth + 1)?;
            if t == "abs" {
                self.expect(")")?;
                return self.node(Expr::Abs(Box::new(a)));
            }
            let op = match t {
                "min" => Op::Min,
                "max" => Op::Max,
                "shl" => Op::Shl,
                "shr" => Op::Shr,
                _ => bail!("unknown integer function '{t}'"),
            };
            self.expect(",")?;
            let b = self.sum(depth + 1)?;
            self.expect(")")?;
            return self.node(Expr::Bin(op, Box::new(a), Box::new(b)));
        }
        if let Some(i) = self.local.iter().position(|n| n == t) {
            self.node(Expr::Local(i))
        } else if self.known.iter().any(|n| n == t) {
            self.node(Expr::Outer(t.to_string()))
        } else {
            bail!("unknown integer name '{t}': names must be declared before use")
        }
    }
}
