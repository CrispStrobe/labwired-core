// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! The arithmetic expression behind [`labwired_config::DerivedChannel`].
//!
//! A derived channel is a named value a part COMPUTES from the values it
//! senses — the INA219's POWER register is `bus_mV × |I_mA| / 1000` (§8.5.4),
//! a product of two stimulus channels. The legacy float language uses a
//! recursive-descent parser over `+ - * /`, unary minus, parentheses, decimal
//! literals, names, and `abs` / `min` / `max`.
//!
//! **Parsed once, at load.** [`CompiledExpr`] is the AST; the engine builds one
//! per declared channel when the descriptor is built and evaluates it on every
//! read, so a read never touches the expression text. A name that is not a
//! declared input or an EARLIER derived channel is rejected at parse time by
//! [`compile_derived`], which is what makes a cycle unwritable — see
//! [`labwired_config::DerivedChannel`] for why that is the rule rather than a
//! dependency sort.
//!
//! **The legacy float grammar remains unchanged.** No rounding or comparisons
//! inside arithmetic expressions. Rounding to a register count is [`labwired_config::Encode`]'s
//! single rule for every part, and a value that depends on what the part is
//! currently doing is a state machine. Either one grown into this grammar
//! would be a second place the engine decides those things.
//!
//! Exact producers are separately declared: `quantize` marks the explicit
//! rounding boundary, while `integer` and `invert` use the checked i64 parser
//! in `declarative_integer`. Their local intermediates never pass through f64.

use std::collections::HashMap;

use super::declarative_integer as exact;
use anyhow::{bail, Result};
use labwired_config::DerivedChannel;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Const(f64),
    /// Index into the evaluation's name list — resolved at compile time so a
    /// read is an array index, not a string hash.
    Name(String),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Abs(Box<Expr>),
    Min(Box<Expr>, Box<Expr>),
    Max(Box<Expr>, Box<Expr>),
    /// `pow(base, exponent)` — the CdS photoresistor's `R = R10 * (lux/10)^-γ`.
    Pow(Box<Expr>, Box<Expr>),
    /// `exp(x)` — the NTC's beta equation, `R = R0 * e^(B(1/T - 1/T0))`.
    Exp(Box<Expr>),
}

/// The comparison a [`DerivedChannel::when`] guard makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CmpOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

/// A parsed `when:` guard: exactly one comparison between two expressions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pred {
    lhs: Expr,
    op: CmpOp,
    rhs: Expr,
}

impl Pred {
    fn holds(&self, slots: &HashMap<String, f64>) -> bool {
        let (a, b) = (self.lhs.eval(slots), self.rhs.eval(slots));
        match self.op {
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// One legacy float expression and its optional float comparison guard.
#[derive(Debug, Clone)]
pub(crate) struct CompiledExpr {
    expr: Expr,
    /// `Some` ⇒ the channel is `expr` while the guard holds and 0 when it does
    /// not. See [`DerivedChannel::when`].
    guard: Option<Pred>,
}

impl CompiledExpr {
    /// Evaluate against a slot map, honouring the guard. Used directly by the
    /// `analog_source` primitive, whose `formula:` is one expression rather
    /// than a named channel.
    pub(crate) fn eval_with(&self, slots: &HashMap<String, f64>) -> f64 {
        match &self.guard {
            Some(g) if !g.holds(slots) => 0.0,
            _ => self.expr.eval(slots),
        }
    }
}

impl Expr {
    fn eval(&self, slots: &HashMap<String, f64>) -> f64 {
        match self {
            Expr::Const(v) => *v,
            // A name that reached evaluation was checked at load, so a miss
            // cannot happen; 0.0 rather than a panic keeps a read on the bus
            // from aborting the whole simulation if it ever did.
            Expr::Name(n) => slots.get(n).copied().unwrap_or(0.0),
            Expr::Neg(a) => -a.eval(slots),
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(slots), b.eval(slots));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                }
            }
            Expr::Abs(a) => a.eval(slots).abs(),
            Expr::Min(a, b) => a.eval(slots).min(b.eval(slots)),
            Expr::Max(a, b) => a.eval(slots).max(b.eval(slots)),
            Expr::Pow(a, b) => a.eval(slots).powf(b.eval(slots)),
            Expr::Exp(a) => a.eval(slots).exp(),
        }
    }
}

// ─── tokenizer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    Cmp(CmpOp),
}

fn lex(src: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            // Comparison operators. They appear ONLY in a `when:` guard —
            // `parse` (the arithmetic entry point) refuses a leftover token,
            // so `a > b` inside an `expr:` is a load error naming the channel
            // rather than a silently-true condition.
            '>' | '<' | '=' | '!' => {
                let two = chars.get(i + 1) == Some(&'=');
                let op = match (c, two) {
                    ('>', false) => CmpOp::Gt,
                    ('>', true) => CmpOp::Ge,
                    ('<', false) => CmpOp::Lt,
                    ('<', true) => CmpOp::Le,
                    ('=', true) => CmpOp::Eq,
                    ('!', true) => CmpOp::Ne,
                    ('=', false) => bail!("'=' is not an operator — write '==' to compare"),
                    _ => bail!("'!' is not an operator — write '!=' to compare"),
                };
                i += if two { 2 } else { 1 };
                out.push(Tok::Cmp(op));
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // Exponent form (`1e3`, `2.5e-4`): consumed here so it is a
                // number rather than a number followed by an unknown name.
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let mark = i;
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    if i < chars.len() && chars[i].is_ascii_digit() {
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    } else {
                        i = mark;
                    }
                }
                let text: String = chars[start..i].iter().collect();
                match text.parse::<f64>() {
                    Ok(v) => out.push(Tok::Num(v)),
                    Err(_) => bail!("'{text}' is not a number"),
                }
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            _ => bail!("unexpected character '{c}'"),
        }
    }
    Ok(out)
}

// ─── parser ────────────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
    bounded: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, want: &Tok, what: &str) -> Result<()> {
        if self.eat(want) {
            Ok(())
        } else {
            bail!("expected {what}")
        }
    }

    /// `sum := product (('+' | '-') product)*`
    fn sum(&mut self) -> Result<Expr> {
        let mut lhs = self.product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.product()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `product := unary (('*' | '/') unary)*`
    fn product(&mut self) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `unary := '-' unary | atom`
    fn unary(&mut self) -> Result<Expr> {
        self.depth += 1;
        if self.bounded && self.depth > 64 {
            bail!("quantize expression exceeds depth 64");
        }
        let result = if self.eat(&Tok::Minus) {
            self.unary().map(|e| Expr::Neg(Box::new(e)))
        } else {
            self.atom()
        };
        self.depth -= 1;
        result
    }

    /// `atom := number | name | call | '(' sum ')'`
    fn atom(&mut self) -> Result<Expr> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(Expr::Const(v)),
            Some(Tok::LParen) => {
                let inner = self.sum()?;
                self.expect(&Tok::RParen, "')'")?;
                Ok(inner)
            }
            Some(Tok::Ident(name)) => {
                if !self.eat(&Tok::LParen) {
                    return Ok(Expr::Name(name));
                }
                match name.as_str() {
                    "abs" => {
                        let a = self.sum()?;
                        self.expect(&Tok::RParen, "')' after abs(…)")?;
                        Ok(Expr::Abs(Box::new(a)))
                    }
                    "pow" => {
                        let a = self.sum()?;
                        self.expect(&Tok::Comma, "',' between the base and the exponent")?;
                        let b = self.sum()?;
                        self.expect(&Tok::RParen, "')'")?;
                        Ok(Expr::Pow(Box::new(a), Box::new(b)))
                    }
                    "exp" => {
                        let a = self.sum()?;
                        self.expect(&Tok::RParen, "')' after exp(…)")?;
                        Ok(Expr::Exp(Box::new(a)))
                    }
                    "min" | "max" => {
                        let a = self.sum()?;
                        self.expect(&Tok::Comma, "',' between the two arguments")?;
                        let b = self.sum()?;
                        self.expect(&Tok::RParen, "')'")?;
                        Ok(if name == "min" {
                            Expr::Min(Box::new(a), Box::new(b))
                        } else {
                            Expr::Max(Box::new(a), Box::new(b))
                        })
                    }
                    // Named loudly: a descriptor that reaches for `round(` or
                    // `sqrt(` is reaching for a decision this language does not
                    // make, and a silent "unknown name" would look like a typo.
                    other => bail!(
                        "unknown function '{other}(' — this language has abs(), min(), max(), \
                         pow() and exp() and nothing else"
                    ),
                }
            }
            Some(t) => bail!("unexpected token {t:?}"),
            None => bail!("expression ended early"),
        }
    }
}

/// Parse a `when:` guard: `<expr> <cmp> <expr>`, exactly one comparison.
fn parse_pred(src: &str) -> Result<Pred> {
    let mut p = Parser {
        toks: lex(src)?,
        pos: 0,
        depth: 0,
        bounded: false,
    };
    if p.toks.is_empty() {
        bail!("the condition is empty");
    }
    let lhs = p.sum()?;
    let op = match p.next() {
        Some(Tok::Cmp(op)) => op,
        Some(t) => bail!("expected a comparison (>, >=, <, <=, ==, !=), got {t:?}"),
        None => bail!("a `when:` is a COMPARISON — '{src}' is only the left-hand side"),
    };
    let rhs = p.sum()?;
    if p.pos != p.toks.len() {
        bail!(
            "trailing input after the comparison — a `when:` holds exactly one, and a \
             compound condition belongs in a second derived channel"
        );
    }
    Ok(Pred { lhs, op, rhs })
}

fn parse(src: &str) -> Result<Expr> {
    let mut p = Parser {
        toks: lex(src)?,
        pos: 0,
        depth: 0,
        bounded: false,
    };
    if p.toks.is_empty() {
        bail!("the expression is empty");
    }
    let e = p.sum()?;
    if p.pos != p.toks.len() {
        bail!("trailing input after a complete expression");
    }
    Ok(e)
}

/// Quantize opts into the same resource bounds as integer expressions without
/// changing the established float grammar or imposing new limits on legacy expr.
fn validate_quantize_limits(src: &str, guard: bool) -> Result<()> {
    if src.len() > 8192 {
        bail!("quantize expression exceeds source limit");
    }
    let toks = lex(src)?;
    if toks.len() > 2048 {
        bail!("quantize expression exceeds token limit");
    }
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
        bounded: true,
    };
    let a = p.sum()?;
    let b = if guard {
        if !matches!(p.next(), Some(Tok::Cmp(_))) {
            bail!("quantize guard requires comparison");
        }
        Some(p.sum()?)
    } else {
        None
    };
    fn size(e: &Expr) -> (usize, usize) {
        match e {
            Expr::Const(_) | Expr::Name(_) => (1, 1),
            Expr::Neg(a) | Expr::Abs(a) | Expr::Exp(a) => {
                let (n, d) = size(a);
                (n + 1, d + 1)
            }
            Expr::Bin(_, a, b) | Expr::Min(a, b) | Expr::Max(a, b) | Expr::Pow(a, b) => {
                let (n, d) = size(a);
                let (m, f) = size(b);
                (n + m + 1, d.max(f) + 1)
            }
        }
    }
    let (n, d) = size(&a);
    let (m, f) = b.as_ref().map(size).unwrap_or((0, 0));
    if n + m > 512 || d.max(f) > 64 {
        bail!("quantize expression exceeds 512 nodes or depth 64");
    }
    if p.pos != p.toks.len() {
        bail!("trailing input in quantize expression");
    }
    Ok(())
}

/// Collect every name an expression reads.
fn names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Const(_) => {}
        Expr::Name(n) => out.push(n.clone()),
        Expr::Neg(a) | Expr::Abs(a) | Expr::Exp(a) => names(a, out),
        Expr::Bin(_, a, b) | Expr::Min(a, b) | Expr::Max(a, b) | Expr::Pow(a, b) => {
            names(a, out);
            names(b, out);
        }
    }
}

// ─── the load-time contract ────────────────────────────────────────────────

/// Compile a descriptor's `behavior.derived` list against its declared input
/// channel keys.
///
/// Enforced here, so a broken descriptor fails at load with the channel and the
/// name in the message rather than reading 0.0 forever on the bus:
///   * a derived name may not collide with a stimulus channel key, or with an
///     earlier derived name — a `source:` naming both would be ambiguous;
///   * every name an expression reads must be a declared input or a derived
///     channel declared ABOVE it. That is also why no cycle can be written.
#[derive(Debug, Clone)]
pub(crate) struct CompiledDerived {
    pub(crate) name: String,
    producer: Producer,
}
#[derive(Debug, Clone)]
enum Producer {
    Float(CompiledExpr),
    Quantize(CompiledExpr),
    Integer {
        program: exact::Program,
        guard: Option<exact::Guard>,
    },
    Invert {
        program: exact::Program,
        guard: Option<exact::Guard>,
        target: exact::IntExpr,
        bounds: [i64; 2],
    },
}
impl CompiledDerived {
    pub(crate) fn is_exact(&self) -> bool {
        !matches!(self.producer, Producer::Float(_))
    }
    fn eval(&self, slots: &HashMap<String, f64>) -> Result<f64> {
        match &self.producer {
            Producer::Float(expr) => Ok(expr.eval_with(slots)),
            Producer::Quantize(expr) => {
                exact::export(exact::import(expr.eval_with(slots).round())?)
            }
            Producer::Integer { program, guard } => {
                if let Some(g) = guard {
                    if !g.holds_outer(slots)? {
                        return Ok(0.0);
                    }
                }
                exact::export(program.eval(slots, None)?)
            }
            Producer::Invert {
                program,
                guard,
                target,
                bounds,
            } => {
                if let Some(g) = guard {
                    if !g.holds_outer(slots)? {
                        return Ok(0.0);
                    }
                }
                exact::export(exact::inverse(
                    program,
                    slots,
                    *bounds,
                    target.eval(slots)?,
                )?)
            }
        }
    }
}

pub(crate) fn compile_derived(
    derived: &[DerivedChannel],
    input_keys: &[String],
) -> Result<Vec<CompiledDerived>> {
    let mut out: Vec<CompiledDerived> = Vec::with_capacity(derived.len());
    let mut known = input_keys.to_vec();
    for d in derived {
        if d.name.trim().is_empty() {
            bail!("a derived channel has an empty name");
        }
        if input_keys.contains(&d.name) {
            bail!(
                "derived channel '{}' has the same name as a stimulus input channel",
                d.name
            );
        }
        if out.iter().any(|c| c.name == d.name) {
            bail!("derived channel '{}' is declared twice", d.name);
        }
        let producer = (|| -> Result<Producer> {
            if [
                d.expr.is_some(),
                d.integer.is_some(),
                d.quantize.is_some(),
                d.invert.is_some(),
            ]
            .into_iter()
            .filter(|v| *v)
            .count()
                != 1
            {
                bail!("exactly one of expr, integer, quantize or invert is required");
            }
            if let Some(src) = &d.expr {
                return Ok(Producer::Float(compile_float_derived(
                    &d.name,
                    src,
                    d.when.as_deref(),
                    &known,
                )?));
            }
            if let Some(q) = &d.quantize {
                validate_quantize_limits(&q.expr, false)?;
                if let Some(g) = &d.when {
                    validate_quantize_limits(g, true)?;
                }
                return Ok(Producer::Quantize(compile_float_derived(
                    &d.name,
                    &q.expr,
                    d.when.as_deref(),
                    &known,
                )?));
            }
            let guard = d
                .when
                .as_deref()
                .map(|g| exact::Guard::compile(g, &known))
                .transpose()?;
            if let Some(p) = &d.integer {
                return Ok(Producer::Integer {
                    program: exact::Program::compile(
                        &p.bindings,
                        &p.of,
                        p.when.as_deref(),
                        None,
                        &known,
                    )?,
                    guard,
                });
            }
            if let Some(p) = &d.invert {
                let [lower, upper] = p.over;
                if upper < lower || upper as i128 - lower as i128 + 1 > 1_048_576 {
                    bail!("inverse domain must contain 1 to 1048576 candidates");
                }
                return Ok(Producer::Invert {
                    program: exact::Program::compile(
                        &p.bindings,
                        &p.of,
                        p.when.as_deref(),
                        Some(&p.variable),
                        &known,
                    )?,
                    guard,
                    target: exact::IntExpr::compile(&p.target, &known)?,
                    bounds: p.over,
                });
            }
            unreachable!("producer count checked")
        })()
        .map_err(|e| anyhow::anyhow!("derived channel '{}': {e:#}", d.name))?;
        known.push(d.name.clone());
        out.push(CompiledDerived {
            name: d.name.clone(),
            producer,
        });
    }
    Ok(out)
}

fn compile_float_derived(
    name: &str,
    src: &str,
    when: Option<&str>,
    known: &[String],
) -> Result<CompiledExpr> {
    let expr = parse(src).map_err(|e| anyhow::anyhow!("{e} (in `{src}`)"))?;
    let guard = when.map(parse_pred).transpose()?;
    let mut read = Vec::new();
    names(&expr, &mut read);
    if let Some(g) = &guard {
        names(&g.lhs, &mut read);
        names(&g.rhs, &mut read);
    }
    for n in read {
        if !known.contains(&n) {
            bail!("derived channel '{name}' reads '{n}', which is neither a declared input channel nor a derived channel declared above it; names must exist in declaration order, which makes a cycle impossible to write");
        }
    }
    Ok(CompiledExpr { expr, guard })
}

/// Compile an ordinary float formula (the legacy analog grammar).
pub(crate) fn compile_formula(what: &str, src: &str, known: &[String]) -> Result<CompiledExpr> {
    let expr = parse(src).map_err(|e| anyhow::anyhow!("{what}: {e} (in `{src}`)"))?;
    let mut read = Vec::new();
    names(&expr, &mut read);
    for n in &read {
        if !known.iter().any(|k| k == n) {
            bail!(
                "{what} reads '{n}', which is neither a declared input channel nor a derived \
                 channel. Known names: {known:?}"
            );
        }
    }
    Ok(CompiledExpr { expr, guard: None })
}

/// Evaluate every derived channel into `slots`, in declaration order, so a
/// later expression sees the values the earlier ones just produced.
///
/// Called on the observed (noise-applied) slot view, which is what makes a
/// derived channel a function of what the part MEASURED rather than of the
/// noiseless stimulus behind it.
pub(crate) fn eval_derived(
    compiled: &[CompiledDerived],
    slots: &mut HashMap<String, f64>,
) -> Result<()> {
    for c in compiled {
        let v = c
            .eval(slots)
            .map_err(|e| anyhow::anyhow!("derived channel '{}': {e:#}", c.name))?;
        slots.insert(c.name.clone(), v);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(name: &str, expr: &str) -> DerivedChannel {
        DerivedChannel {
            name: name.into(),
            expr: Some(expr.into()),
            integer: None,
            quantize: None,
            invert: None,
            when: None,
        }
    }

    fn d_when(name: &str, when: &str, expr: &str) -> DerivedChannel {
        DerivedChannel {
            name: name.into(),
            expr: Some(expr.into()),
            integer: None,
            quantize: None,
            invert: None,
            when: Some(when.into()),
        }
    }

    fn eval(expr: &str, slots: &[(&str, f64)]) -> f64 {
        let keys: Vec<String> = slots.iter().map(|(k, _)| (*k).to_string()).collect();
        let compiled = compile_derived(&[d("out", expr)], &keys).expect("compiles");
        let mut map: HashMap<String, f64> =
            slots.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
        eval_derived(&compiled, &mut map).unwrap();
        map["out"]
    }

    #[test]
    fn precedence_and_parentheses_follow_arithmetic() {
        assert_eq!(eval("2 + 3 * 4", &[]), 14.0);
        assert_eq!(eval("(2 + 3) * 4", &[]), 20.0);
        assert_eq!(eval("10 / 4", &[]), 2.5);
        assert_eq!(
            eval("1 - 2 - 3", &[]),
            -4.0,
            "subtraction is left-associative"
        );
        assert_eq!(eval("-2 * -3", &[]), 6.0);
        assert_eq!(eval("2 * 1e3", &[]), 2000.0);
    }

    #[test]
    fn the_three_functions_are_the_only_ones() {
        assert_eq!(eval("abs(0 - 4.5)", &[]), 4.5);
        assert_eq!(eval("min(3, 7)", &[]), 3.0);
        assert_eq!(eval("max(3, 7)", &[]), 7.0);
        let err = compile_derived(&[d("out", "round(1.5)")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown function 'round('"), "{err}");
    }

    #[test]
    fn the_ina219_power_expression_is_the_product_of_two_channels() {
        // bus_mV × |I_mA| in watts: 12 V × 1.5 A = 18 W, and the sign of the
        // current does not change the power.
        assert_eq!(
            eval(
                "bus_voltage * abs(current)",
                &[("bus_voltage", 12.0), ("current", 1.5)]
            ),
            18.0
        );
        assert_eq!(
            eval(
                "bus_voltage * abs(current)",
                &[("bus_voltage", 12.0), ("current", -1.5)]
            ),
            18.0
        );
    }

    #[test]
    fn a_later_channel_sees_an_earlier_one() {
        let compiled = compile_derived(
            &[d("half", "x / 2"), d("quarter", "half / 2")],
            &["x".to_string()],
        )
        .expect("compiles");
        let mut slots: HashMap<String, f64> = [("x".to_string(), 8.0)].into_iter().collect();
        eval_derived(&compiled, &mut slots).unwrap();
        assert_eq!(slots["half"], 4.0);
        assert_eq!(slots["quarter"], 2.0);
    }

    #[test]
    fn a_forward_reference_is_a_load_error_which_is_why_a_cycle_cannot_be_written() {
        // `a` reads `b`, which is declared BELOW it. Refusing this is what makes
        // `a → b → a` unwritable: the second half could never compile either.
        let err = compile_derived(&[d("a", "b + 1"), d("b", "a + 1")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("derived channel 'a' reads 'b'"), "{err}");
        assert!(err.contains("makes a cycle impossible to write"), "{err}");
        // …and the self-reference, the shortest cycle of all.
        let err = compile_derived(&[d("a", "a * 2")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("reads 'a'"), "{err}");
    }

    #[test]
    fn a_derived_name_cannot_shadow_a_stimulus_channel() {
        let err = compile_derived(&[d("temp", "temp * 2")], &["temp".to_string()])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("same name as a stimulus input channel"),
            "{err}"
        );
    }

    #[test]
    fn pow_and_exp_are_the_two_functions_the_analog_plants_needed() {
        // The CdS power law: R(lux) = 10k * (lux/10)^-0.7, and at 10 lx the
        // exponent's base is 1, so R is R0 whatever gamma is.
        assert_eq!(eval("pow(10 / 10, -0.7)", &[]), 1.0);
        assert!((eval("10000 * pow(100 / 10, -0.7)", &[]) - 1_995.262_31).abs() < 1e-4);
        // The NTC beta equation collapses to R0 at T0.
        assert_eq!(eval("exp(3950 * (1 / 298.15 - 1 / 298.15))", &[]), 1.0);
        // lux = 0: the base is 0 and the exponent negative, so R is +inf and
        // the divider that reads it lands on the ground rail rather than NaN.
        let r = eval("10000 * pow(0 / 10, -0.7)", &[]);
        assert!(r.is_infinite(), "R at zero lux must be infinite, got {r}");
        assert_eq!(
            eval("3300 * 10000 / (10000 * pow(0 / 10, -0.7) + 10000)", &[]),
            0.0
        );
    }

    #[test]
    fn a_when_guard_gates_a_channel_to_zero() {
        let compiled = compile_derived(
            &[d_when("bump", "usb_present >= 0.5", "150")],
            &["usb_present".to_string()],
        )
        .expect("compiles");
        for (usb, want) in [(0.0, 0.0), (0.49, 0.0), (0.5, 150.0), (1.0, 150.0)] {
            let mut slots: HashMap<String, f64> =
                [("usb_present".to_string(), usb)].into_iter().collect();
            eval_derived(&compiled, &mut slots).unwrap();
            assert_eq!(slots["bump"], want, "usb_present = {usb}");
        }
    }

    #[test]
    fn a_when_guard_reads_only_names_already_in_scope() {
        let err = compile_derived(&[d_when("bump", "charging > 0", "150")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("reads 'charging'"), "{err}");
    }

    #[test]
    fn a_comparison_is_a_guard_and_never_arithmetic() {
        // `>` inside `expr:` is refused rather than silently evaluated as a
        // number, so a descriptor cannot grow a conditional by accident.
        let err = compile_derived(&[d("out", "1 > 0")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("trailing input"), "{err}");
        // …and a `when:` that is not a comparison is refused too.
        let err = compile_derived(
            &[d_when("out", "usb_present", "1")],
            &["usb_present".into()],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("a `when:` is a COMPARISON"), "{err}");
        let err = compile_derived(&[d_when("out", "1 > 0 > 0", "1")], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("compound condition"), "{err}");
    }

    #[test]
    fn every_comparison_operator_means_what_it_says() {
        for (cond, want) in [
            ("x > 1", 0.0),
            ("x >= 1", 150.0),
            ("x < 1", 0.0),
            ("x <= 1", 150.0),
            ("x == 1", 150.0),
            ("x != 1", 0.0),
        ] {
            let compiled =
                compile_derived(&[d_when("g", cond, "150")], &["x".to_string()]).expect("compiles");
            let mut slots: HashMap<String, f64> = [("x".to_string(), 1.0)].into_iter().collect();
            eval_derived(&compiled, &mut slots).unwrap();
            assert_eq!(slots["g"], want, "`{cond}` with x = 1");
        }
    }

    #[test]
    fn a_formula_is_checked_against_the_names_in_scope() {
        let f = compile_formula("analog.formula", "a * 2 + b", &["a".into(), "b".into()])
            .expect("compiles");
        let slots: HashMap<String, f64> = [("a".to_string(), 3.0), ("b".to_string(), 1.0)]
            .into_iter()
            .collect();
        assert_eq!(f.eval_with(&slots), 7.0);
        let err = compile_formula("analog.formula", "a * nope", &["a".into()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("reads 'nope'"), "{err}");
    }

    #[test]
    fn a_malformed_expression_names_the_channel_and_the_text() {
        for bad in ["", "1 +", "2 3", "(1 + 2", "1 $ 2"] {
            let err = compile_derived(&[d("out", bad)], &[])
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("derived channel 'out'") && err.contains(bad.trim()),
                "`{bad}` produced an unhelpful error: {err}"
            );
        }
    }
}

#[cfg(test)]
mod exact_tests {
    use super::*;
    fn run(yaml: &str, values: &[(&str, f64)]) -> Result<f64> {
        let ds: Vec<DerivedChannel> = serde_yaml::from_str(yaml)?;
        let keys = values
            .iter()
            .map(|(k, _)| k.to_string())
            .collect::<Vec<_>>();
        let compiled = compile_derived(&ds, &keys)?;
        let mut slots = values.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        eval_derived(&compiled, &mut slots)?;
        Ok(slots["out"])
    }
    fn integer(expr: &str) -> Result<f64> {
        run(&format!("- name: out\n  integer: {{of: '{expr}'}}"), &[])
    }
    #[test]
    fn checked_integer_arithmetic_and_lazy_guards() {
        for (expr, expected) in [
            ("-7 / 2", -3.0),
            ("shr(-7, 1)", -4.0),
            ("shl(-3, 2)", -12.0),
            ("abs(-7) + min(4, 2) + max(1, 3)", 12.0),
            ("shr(-9223372036854775808, 63)", -1.0),
        ] {
            assert_eq!(integer(expr).unwrap(), expected, "{expr}");
        }
        for expr in [
            "1 / 0",
            "9223372036854775807 + 1",
            "-9223372036854775808 / -1",
            "abs(-9223372036854775808)",
            "-(-9223372036854775808)",
            "shl(1, 63)",
            "shl(1, -1)",
            "shr(1, 64)",
            "9007199254740993",
            "1.2",
            "1e3",
        ] {
            assert!(integer(expr).is_err(), "must reject {expr}");
        }
        assert_eq!(run("- name: out\n  integer:\n    bindings:\n      - {name: x, expr: '1 / 0', when: '9007199254740993 == 9007199254740992'}\n    of: '1 / 0'\n    when: 'x != 0'", &[]).unwrap(), 0.0);
    }
    #[test]
    fn integer_imports_are_exact_and_quantization_is_explicit() {
        let src = "- name: out\n  integer: {of: 'x'}";
        for x in [0.5, f64::NAN, f64::INFINITY, 9007199254740994.0] {
            assert!(run(src, &[("x", x)]).is_err(), "{x}");
        }
        for x in [-9007199254740992.0, 9007199254740992.0] {
            assert_eq!(run(src, &[("x", x)]).unwrap(), x);
        }
        let src = "- name: out\n  quantize: {expr: 'x', rounding: nearest}";
        for (x, y) in [(-1.5, -2.0), (-0.5, -1.0), (0.5, 1.0), (1.5, 2.0)] {
            assert_eq!(run(src, &[("x", x)]).unwrap(), y);
        }
        for x in [f64::NAN, f64::INFINITY, 9007199254740994.0] {
            assert!(run(src, &[("x", x)]).is_err());
        }
    }
    #[test]
    fn inverse_uses_lower_bound_and_predecessor_with_inclusive_nonzero_bounds() {
        let src =
            "- name: out\n  invert: {variable: x, over: [5, 10], target: target, of: 'x * 2'}";
        for (target, expected) in [
            (0.0, 5.0),
            (11.0, 5.0),
            (12.0, 6.0),
            (13.0, 6.0),
            (20.0, 10.0),
            (99.0, 10.0),
        ] {
            assert_eq!(run(src, &[("target", target)]).unwrap(), expected);
        }
        let src =
            "- name: out\n  invert: {variable: x, over: [5, 10], target: '6', of: '(x / 3) * 3'}";
        assert_eq!(run(src, &[]).unwrap(), 6.0, "first point in exact plateau");
        let src = "- name: out\n  invert: {variable: x, over: [5, 10], target: '7', of: '4'}";
        assert_eq!(
            run(src, &[]).unwrap(),
            9.0,
            "upper endpoint's predecessor ties"
        );
    }
    #[test]
    fn singleton_inverse_evaluates_its_forward_program() {
        let src = "- name: out\n  invert: {variable: x, over: [0, 0], target: '0', of: '1 / 0'}";
        let error = run(src, &[]).expect_err("a singleton forward fault must propagate");
        assert!(error.to_string().contains("division by zero"), "{error:#}");
        let src = "- name: out\n  invert: {variable: x, over: [7, 7], target: '0', of: 'x * 2'}";
        assert_eq!(run(src, &[]).unwrap(), 7.0);
    }
    #[test]
    fn inverse_distances_use_i128_and_single_point_domains_are_valid() {
        let src="- name: out\n  invert: {variable: x, over: [0, 1], target: '9223372036854775807', of: '-9223372036854775808 + x'}";
        assert_eq!(run(src, &[]).unwrap(), 1.0);
        let src="- name: out\n  invert: {variable: x, over: [-7, -7], target: '-9223372036854775808', of: 'x'}";
        assert_eq!(run(src, &[]).unwrap(), -7.0);
    }
    #[test]
    fn exact_outer_guards_are_lazy_and_errors_name_binding_and_channel() {
        let src = "- name: out\n  when: 'flag == 0'\n  integer: {of: x}";
        assert_eq!(run(src, &[("flag", 1.0), ("x", f64::NAN)]).unwrap(), 0.0);
        let src =
            "- name: out\n  integer: {bindings: [{name: quotient, expr: '1 / 0'}], of: quotient}";
        let error = run(src, &[]).unwrap_err().to_string();
        assert!(error.contains("derived channel 'out'"), "{error}");
        assert!(error.contains("binding 'quotient'"), "{error}");
    }
    #[test]
    fn producers_and_local_scopes_are_validated() {
        for src in ["- {name: out}", "- {name: out, expr: '1', integer: {of: '1'}}",
            "- {name: out, integer: {bindings: [{name: x, expr: y}, {name: y, expr: '1'}], of: x}}",
            "- {name: out, integer: {bindings: [{name: x, expr: '1'}, {name: x, expr: '2'}], of: x}}",
            "- {name: out, invert: {variable: x, over: [10, 5], target: '1', of: x}}",
            "- {name: out, invert: {variable: x, over: [0, 1048576], target: '1', of: x}}"] {
            assert!(run(src,&[]).is_err(), "{src}");
        }
        assert!(run(
            "- name: out\n  integer: {bindings: [{name: x, expr: '1'}], of: x}",
            &[("x", 2.0)]
        )
        .is_err());
        assert!(integer(&format!("{}1{}", "(".repeat(100), ")".repeat(100))).is_err());
        assert!(integer(&vec!["1"; 1000].join("+")).is_err());
    }
    #[test]
    fn quantize_and_program_resource_limits_are_checked_at_load() {
        let nested = format!("{}1{}", "(".repeat(100), ")".repeat(100));
        assert!(run(
            &format!("- name: out\n  quantize: {{expr: '{nested}', rounding: nearest}}"),
            &[]
        )
        .is_err());
        let bindings = (0..129)
            .map(|i| format!("      - {{name: v{i}, expr: '1'}}\n"))
            .collect::<String>();
        assert!(run(
            &format!("- name: out\n  integer:\n    bindings:\n{bindings}    of: '1'"),
            &[]
        )
        .is_err());
    }
    #[test]
    fn integer_locals_retain_bits_above_float_precision() {
        let result = run("- name: out\n  integer:\n    bindings:\n      - {name: wide, expr: '9007199254740993'}\n    of: 'wide - 9007199254740992'", &[]);
        assert_eq!(result.unwrap(), 1.0);
    }
}

#[cfg(test)]
mod bme_forward_tests {
    use super::*;
    use crate::peripherals::components::bme280::Bme280Calib;
    fn programs() -> (exact::Program, exact::Program, exact::Program) {
        let descriptor: labwired_config::DeviceDescriptor =
            serde_yaml::from_str(include_str!("../../../../../configs/devices/bme280.yaml"))
                .unwrap();
        let program = |name: &str| {
            let p = descriptor
                .behavior
                .derived
                .iter()
                .find(|d| d.name == name)
                .unwrap()
                .invert
                .as_ref()
                .unwrap();
            exact::Program::compile(
                &p.bindings,
                &p.of,
                p.when.as_deref(),
                Some(&p.variable),
                &["t_fine".into()],
            )
            .unwrap()
        };
        (program("adc_t"), program("adc_p"), program("adc_h"))
    }
    #[test]
    fn shipped_temperature_forward_matches_rust_for_every_candidate() {
        let (t, _, _) = programs();
        let c = Bme280Calib::default();
        let slots = HashMap::new();
        for x in 0..=1_048_575 {
            assert_eq!(
                t.eval(&slots, Some(x)).unwrap(),
                c.compensate_t(x as i32) as i64,
                "adc_t={x}"
            );
        }
    }
    #[test]
    fn shipped_humidity_sweeps_and_pressure_samples_match_rust_at_reachable_temperatures() {
        let (_, p, h) = programs();
        let c = Bme280Calib::default();
        for temp in [-40.0, -31.27, -12.345, 0.0, 12.34, 25.0, 43.21, 67.89, 85.0] {
            let fine = c.t_fine(c.invert_t(temp));
            let slots = HashMap::from([("t_fine".to_string(), fine as f64)]);
            let mut last_h = i64::MIN;
            for x in 0..=65535 {
                let value = h.eval(&slots, Some(x)).unwrap();
                assert_eq!(
                    value,
                    c.compensate_h(x as i32, fine) as i64,
                    "H temp={temp} adc={x}"
                );
                assert!(value >= last_h);
                last_h = value;
            }
            let mut last_p = i64::MIN;
            for x in (0..=1_048_575).step_by(127).chain([1_048_574, 1_048_575]) {
                let value = p.eval(&slots, Some(x)).unwrap();
                assert_eq!(
                    value,
                    -(c.compensate_p(x as i32, fine) as i64),
                    "P temp={temp} adc={x}"
                );
                assert!(value >= last_p);
                last_p = value;
            }
        }
    }
}
