// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! **Binary frames for the `uart_device` primitive** — byte patterns with named
//! captures, byte-oriented response templates, and the frame checksums a
//! binary protocol carries.
//!
//! The text side of `uart_device` (`match: "AT"`, `respond: "OK\r\n"`) cannot
//! say "a read-registers request from any address", because a request is not a
//! string: it is `[addr, func, reg_hi, reg_lo, count_hi, count_lo, crc_lo,
//! crc_hi]`. This module is the binary half of the same table.
//!
//! Patterns
//! ========
//! ```text
//! pattern := token ( WS token )*
//! token   := "0x" HEX HEX          one literal byte
//!          | "??"                  any one byte, not captured
//!          | NAME ":" TYPE         capture, bound to `var(NAME)`
//!          | "*"                   the rest of the frame, not captured (last)
//!          | NAME ":rest"          the rest of the frame; `var(NAME)` = its length (last)
//! TYPE    := u8 | u16be | u16le | u32be | u32le
//! ```
//! Without a trailing rest token a pattern matches a frame of exactly its own
//! length. A capture is just a rule-machine variable that the engine sets
//! before the frame's rules run, so `var(reg)` works in `respond_bytes:`, in a
//! response's `when:` and in `rules:` with no new expression syntax.
//!
//! Response bytes
//! ==============
//! `respond_bytes:` is a list of items, each one string:
//! ```text
//! item := EXPR [ ":" TYPE ]                  an integer expression, default u8
//!       | "regs(" FIRST "," COUNT ")" [ ":" ("u16be"|"u16le") ]
//!                                            COUNT register words from the table
//!       | "crc16_modbus" | "crc16_ccitt" | "crc8" | "lrc" | "sum8"
//!                                            the checksum of every byte so far
//! ```
//!
//! Checksums
//! =========
//! Each is checked against the published check value of the ASCII string
//! `"123456789"` (Mathematics of CRC catalogue, <https://reveng.sourceforge.io/crc-catalogue/>):
//!
//! | name           | catalogue entry            | check  |
//! |----------------|----------------------------|--------|
//! | `crc16_modbus` | CRC-16/MODBUS              | 0x4B37 |
//! | `crc16_ccitt`  | CRC-16/IBM-3740 (CCITT-FALSE) | 0x29B1 |
//! | `crc8`         | CRC-8/SMBUS (poly 0x07)    | 0xF4   |
//! | `lrc`          | Modbus ASCII LRC (two's complement of the byte sum) | 0x23 |
//! | `sum8`         | modulo-256 byte sum        | 0xDD   |

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::expr::Expr;

// ─── checksums ─────────────────────────────────────────────────────────────

/// CRC-16/MODBUS: poly 0x8005 reflected (0xA001), init 0xFFFF, no final xor.
/// Check value of `"123456789"` is 0x4B37.
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// CRC-16/CCITT-FALSE (catalogue name CRC-16/IBM-3740): poly 0x1021, init
/// 0xFFFF, not reflected, no final xor. Check value 0x29B1.
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-8/SMBUS: poly 0x07, init 0, not reflected, no final xor. Check 0xF4.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Modulo-256 sum of the bytes. Check value of `"123456789"` is 0xDD.
pub fn sum8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, &b| a.wrapping_add(b))
}

/// Longitudinal redundancy check as Modbus ASCII defines it: the two's
/// complement of the modulo-256 byte sum. Check value 0x23.
pub fn lrc(data: &[u8]) -> u8 {
    sum8(data).wrapping_neg()
}

/// Which frame checksum a part uses.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Checksum {
    /// [`crc16_modbus`], sent low byte first.
    Crc16Modbus,
    /// [`crc16_ccitt`], sent high byte first.
    Crc16Ccitt,
    /// [`crc8`].
    Crc8,
    /// [`lrc`].
    Lrc,
    /// [`sum8`].
    Sum8,
}

impl Checksum {
    /// Number of bytes the checksum occupies on the wire.
    pub fn width(self) -> usize {
        match self {
            Checksum::Crc16Modbus | Checksum::Crc16Ccitt => 2,
            _ => 1,
        }
    }

    /// The checksum of `data`, in wire byte order.
    pub fn wire_bytes(self, data: &[u8]) -> Vec<u8> {
        match self {
            Checksum::Crc16Modbus => crc16_modbus(data).to_le_bytes().to_vec(),
            Checksum::Crc16Ccitt => crc16_ccitt(data).to_be_bytes().to_vec(),
            Checksum::Crc8 => vec![crc8(data)],
            Checksum::Lrc => vec![lrc(data)],
            Checksum::Sum8 => vec![sum8(data)],
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "crc16_modbus" => Checksum::Crc16Modbus,
            "crc16_ccitt" => Checksum::Crc16Ccitt,
            "crc8" => Checksum::Crc8,
            "lrc" => Checksum::Lrc,
            "sum8" => Checksum::Sum8,
            _ => return None,
        })
    }
}

// ─── field types ───────────────────────────────────────────────────────────

/// Width and byte order of one multi-byte field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    U8,
    U16Be,
    U16Le,
    U32Be,
    U32Le,
}

impl FieldType {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "u8" => FieldType::U8,
            "u16be" => FieldType::U16Be,
            "u16le" => FieldType::U16Le,
            "u32be" => FieldType::U32Be,
            "u32le" => FieldType::U32Le,
            _ => return None,
        })
    }

    /// Width in bytes.
    pub fn width(self) -> usize {
        match self {
            FieldType::U8 => 1,
            FieldType::U16Be | FieldType::U16Le => 2,
            FieldType::U32Be | FieldType::U32Le => 4,
        }
    }

    fn read(self, b: &[u8]) -> i64 {
        match self {
            FieldType::U8 => i64::from(b[0]),
            FieldType::U16Be => i64::from(u16::from_be_bytes([b[0], b[1]])),
            FieldType::U16Le => i64::from(u16::from_le_bytes([b[0], b[1]])),
            FieldType::U32Be => i64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])),
            FieldType::U32Le => i64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        }
    }

    fn write(self, v: i64, out: &mut Vec<u8>) {
        match self {
            FieldType::U8 => out.push(v as u8),
            FieldType::U16Be => out.extend_from_slice(&(v as u16).to_be_bytes()),
            FieldType::U16Le => out.extend_from_slice(&(v as u16).to_le_bytes()),
            FieldType::U32Be => out.extend_from_slice(&(v as u32).to_be_bytes()),
            FieldType::U32Le => out.extend_from_slice(&(v as u32).to_le_bytes()),
        }
    }
}

// ─── byte patterns ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatTok {
    Lit(u8),
    Any,
    Capture {
        name: String,
        ty: FieldType,
    },
    /// Rest of the frame; the optional name receives its length.
    Rest(Option<String>),
}

/// A byte pattern: literals, wildcards and named captures. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytePattern {
    source: String,
    toks: Vec<PatTok>,
}

fn valid_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic() || f == '_')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl BytePattern {
    /// Compile a pattern. Called once, at load.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut toks = Vec::new();
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return Err("an empty byte pattern matches nothing useful".into());
        }
        for (i, w) in words.iter().enumerate() {
            let last = i + 1 == words.len();
            let tok = if let Some(hex) = w.strip_prefix("0x") {
                if hex.len() != 2 {
                    return Err(format!(
                        "`{w}` in byte pattern `{text}` is not a single byte; write two hex \
                         digits, like 0x03"
                    ));
                }
                PatTok::Lit(
                    u8::from_str_radix(hex, 16)
                        .map_err(|_| format!("`{w}` in byte pattern `{text}` is not hex"))?,
                )
            } else if *w == "??" {
                PatTok::Any
            } else if *w == "*" {
                PatTok::Rest(None)
            } else if let Some((name, ty)) = w.split_once(':') {
                if !valid_ident(name) {
                    return Err(format!("`{name}` in byte pattern `{text}` is not a name"));
                }
                if ty == "rest" {
                    PatTok::Rest(Some(name.to_string()))
                } else {
                    PatTok::Capture {
                        name: name.to_string(),
                        ty: FieldType::parse(ty).ok_or_else(|| {
                            format!(
                                "unknown capture type `{ty}` in byte pattern `{text}`; expected \
                                 u8, u16be, u16le, u32be, u32le or rest"
                            )
                        })?,
                    }
                }
            } else {
                return Err(format!(
                    "`{w}` in byte pattern `{text}` is not a byte (0x..), `??`, `*` or \
                     `name:type`"
                ));
            };
            if matches!(tok, PatTok::Rest(_)) && !last {
                return Err(format!(
                    "the rest-of-frame token in `{text}` must be the last one"
                ));
            }
            toks.push(tok);
        }
        let mut names = self_names(&toks);
        names.sort();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!(
                "capture `{}` appears twice in byte pattern `{text}`",
                w[0]
            ));
        }
        Ok(Self {
            source: text.to_string(),
            toks,
        })
    }

    /// The text the pattern was written with.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Every capture name, in pattern order.
    pub fn capture_names(&self) -> Vec<String> {
        self_names(&self.toks)
    }

    /// Match `frame`; on success return each capture's value in pattern order.
    pub fn matches(&self, frame: &[u8]) -> Option<Vec<(&str, i64)>> {
        let mut at = 0usize;
        let mut caps = Vec::new();
        for tok in &self.toks {
            match tok {
                PatTok::Lit(b) => {
                    if frame.get(at) != Some(b) {
                        return None;
                    }
                    at += 1;
                }
                PatTok::Any => {
                    frame.get(at)?;
                    at += 1;
                }
                PatTok::Capture { name, ty } => {
                    let w = ty.width();
                    let chunk = frame.get(at..at + w)?;
                    caps.push((name.as_str(), ty.read(chunk)));
                    at += w;
                }
                PatTok::Rest(name) => {
                    if let Some(n) = name {
                        caps.push((n.as_str(), (frame.len() - at) as i64));
                    }
                    return Some(caps);
                }
            }
        }
        (at == frame.len()).then_some(caps)
    }
}

fn self_names(toks: &[PatTok]) -> Vec<String> {
    toks.iter()
        .filter_map(|t| match t {
            PatTok::Capture { name, .. } | PatTok::Rest(Some(name)) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

// ─── byte templates ────────────────────────────────────────────────────────

/// One element of a `respond_bytes:` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteItem {
    /// An integer expression written as `ty`.
    Value { expr: Expr, ty: FieldType },
    /// `count` register words starting at `first`.
    Regs {
        first: Expr,
        count: Expr,
        ty: FieldType,
    },
    /// Checksum of every byte emitted so far in this response.
    Check(Checksum),
}

/// A compiled `respond_bytes:` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteTemplate {
    source: Vec<String>,
    items: Vec<ByteItem>,
}

/// Most register words one `regs()` item will emit. The Modbus limit is 125;
/// the cap keeps a bad `count` from allocating without bound.
pub const MAX_REG_WORDS: i64 = 250;

impl Serialize for ByteTemplate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.source.serialize(s)
    }
}

impl<'de> Deserialize<'de> for ByteTemplate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Vec::<String>::deserialize(d)?;
        ByteTemplate::parse(&raw).map_err(D::Error::custom)
    }
}

/// Split at the LAST top-level `:` and return the suffix when it is a type.
fn split_type(item: &str) -> (&str, Option<FieldType>) {
    let mut depth = 0i32;
    let mut at = None;
    for (i, c) in item.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ':' if depth == 0 => at = Some(i),
            _ => {}
        }
    }
    match at.and_then(|i| FieldType::parse(item[i + 1..].trim()).map(|t| (i, t))) {
        Some((i, t)) => (item[..i].trim(), Some(t)),
        None => (item.trim(), None),
    }
}

fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

impl ByteTemplate {
    /// Compile the item list. Called once, at load.
    pub fn parse(sources: &[String]) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("`respond_bytes:` is empty".into());
        }
        let mut items = Vec::new();
        for src in sources {
            let s = src.trim();
            if let Some(ck) = Checksum::parse(s) {
                items.push(ByteItem::Check(ck));
                continue;
            }
            let (body, ty) = split_type(s);
            if let Some(args) = body.strip_prefix("regs(").and_then(|r| r.strip_suffix(')')) {
                let parts = split_args(args);
                if parts.len() != 2 {
                    return Err(format!(
                        "`{s}` in respond_bytes takes two arguments: regs(FIRST, COUNT)"
                    ));
                }
                let ty = ty.unwrap_or(FieldType::U16Be);
                if !matches!(ty, FieldType::U16Be | FieldType::U16Le) {
                    return Err(format!("`{s}`: register words are u16be or u16le"));
                }
                let parse = |p: &str| {
                    Expr::parse(p.trim()).map_err(|e| format!("`{s}` in respond_bytes: {e}"))
                };
                items.push(ByteItem::Regs {
                    first: parse(parts[0])?,
                    count: parse(parts[1])?,
                    ty,
                });
                continue;
            }
            let expr = Expr::parse(body).map_err(|e| format!("`{s}` in respond_bytes: {e}"))?;
            items.push(ByteItem::Value {
                expr,
                ty: ty.unwrap_or(FieldType::U8),
            });
        }
        Ok(Self {
            source: sources.to_vec(),
            items,
        })
    }

    /// The compiled items.
    pub fn items(&self) -> &[ByteItem] {
        &self.items
    }

    /// True when any item reads the register table.
    pub fn uses_regs(&self) -> bool {
        self.items
            .iter()
            .any(|i| matches!(i, ByteItem::Regs { .. }))
    }

    /// Every expression in the template, for name checking.
    pub fn exprs(&self) -> Vec<&Expr> {
        let mut v = Vec::new();
        for i in &self.items {
            match i {
                ByteItem::Value { expr, .. } => v.push(expr),
                ByteItem::Regs { first, count, .. } => {
                    v.push(first);
                    v.push(count);
                }
                ByteItem::Check(_) => {}
            }
        }
        v
    }

    /// Render to bytes. `eval` evaluates an expression; `reg` reads one word of
    /// the register table (absent registers read 0).
    pub fn render(
        &self,
        eval: &mut dyn FnMut(&Expr) -> i64,
        reg: &mut dyn FnMut(i64) -> i64,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        for item in &self.items {
            match item {
                ByteItem::Value { expr, ty } => {
                    let v = eval(expr);
                    ty.write(v, &mut out);
                }
                ByteItem::Regs { first, count, ty } => {
                    let first = eval(first);
                    let count = eval(count).clamp(0, MAX_REG_WORDS);
                    for k in 0..count {
                        ty.write(reg(first.wrapping_add(k)), &mut out);
                    }
                }
                ByteItem::Check(ck) => {
                    let sum = ck.wire_bytes(&out);
                    out.extend_from_slice(&sum);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: &[u8] = b"123456789";

    #[test]
    fn crc16_modbus_matches_the_catalogue_check_value() {
        // CRC-16/MODBUS, reveng catalogue: check = 0x4b37.
        assert_eq!(crc16_modbus(CHECK), 0x4B37);
    }

    #[test]
    fn crc16_ccitt_is_the_ibm_3740_false_variant() {
        // CRC-16/IBM-3740 (CCITT-FALSE), reveng catalogue: check = 0x29b1.
        assert_eq!(crc16_ccitt(CHECK), 0x29B1);
    }

    #[test]
    fn crc8_is_smbus() {
        // CRC-8/SMBUS, reveng catalogue: check = 0xf4.
        assert_eq!(crc8(CHECK), 0xF4);
    }

    #[test]
    fn lrc_and_sum8_check_values() {
        // sum of ASCII 0x31..=0x39 is 477 = 0x1DD.
        assert_eq!(sum8(CHECK), 0xDD);
        assert_eq!(lrc(CHECK), 0x23);
    }

    #[test]
    fn a_known_modbus_request_has_its_published_crc() {
        // Modbus Application Protocol example: slave 1, read 1 holding
        // register at 0 ⇒ 01 03 00 00 00 01, CRC bytes 84 0A (low byte first).
        let req = [0x01u8, 0x03, 0x00, 0x00, 0x00, 0x01];
        assert_eq!(Checksum::Crc16Modbus.wire_bytes(&req), vec![0x84, 0x0A]);
    }

    #[test]
    fn a_pattern_captures_named_fields() {
        let p = BytePattern::parse("addr:u8 0x03 reg:u16be count:u16be").unwrap();
        let caps = p.matches(&[1, 3, 0x12, 0x34, 0, 2]).unwrap();
        assert_eq!(caps, vec![("addr", 1), ("reg", 0x1234), ("count", 2)]);
        assert!(p.matches(&[1, 4, 0, 0, 0, 1]).is_none(), "literal differs");
        assert!(p.matches(&[1, 3, 0, 0, 0]).is_none(), "frame too short");
        assert!(
            p.matches(&[1, 3, 0, 0, 0, 1, 9]).is_none(),
            "frame too long"
        );
    }

    #[test]
    fn wildcards_and_rest_of_frame() {
        let p = BytePattern::parse("0xAA ?? tail:rest").unwrap();
        assert_eq!(p.matches(&[0xAA, 7, 1, 2, 3]).unwrap(), vec![("tail", 3)]);
        assert_eq!(p.matches(&[0xAA, 7]).unwrap(), vec![("tail", 0)]);
        let q = BytePattern::parse("0x10 *").unwrap();
        assert_eq!(q.matches(&[0x10, 5, 5]).unwrap(), vec![]);
    }

    #[test]
    fn little_endian_captures() {
        let p = BytePattern::parse("v:u16le w:u32le").unwrap();
        let caps = p.matches(&[0x34, 0x12, 1, 0, 0, 0]).unwrap();
        assert_eq!(caps, vec![("v", 0x1234), ("w", 1)]);
    }

    #[test]
    fn bad_patterns_are_load_errors() {
        for bad in [
            "",
            "0x3",
            "zz",
            "a:u7",
            "* 0x01",
            "a:u8 a:u8",
            "0xGG",
            "1a:u8",
        ] {
            assert!(BytePattern::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn a_response_template_renders_values_regs_and_a_checksum() {
        let t = ByteTemplate::parse(&[
            "var(addr)".to_string(),
            "0x03".to_string(),
            "var(count) * 2".to_string(),
            "regs(var(reg), var(count))".to_string(),
            "crc16_modbus".to_string(),
        ])
        .unwrap();
        let mut eval = |e: &Expr| match e {
            Expr::Int(n) => *n,
            _ => {
                // The template's expressions are var(addr)=1, var(reg)=0,
                // var(count)=2 and var(count)*2: evaluate through the real
                // parser against a tiny environment.
                struct E;
                impl crate::expr::EvalCtx for E {
                    fn reg(&self, _: &str) -> i64 {
                        0
                    }
                    fn reported(&self, _: &str) -> i64 {
                        0
                    }
                    fn field(&self, _: &str, _: &str) -> i64 {
                        0
                    }
                    fn var(&self, n: &str) -> i64 {
                        match n {
                            "addr" => 1,
                            "reg" => 0,
                            "count" => 2,
                            _ => 0,
                        }
                    }
                    fn input(&self, _: &str) -> i64 {
                        0
                    }
                    fn pin(&self, _: &str) -> i64 {
                        0
                    }
                    fn fifo_len(&self, _: &str) -> i64 {
                        0
                    }
                    fn frame_byte(&self, _: usize) -> i64 {
                        0
                    }
                    fn written(&self) -> i64 {
                        0
                    }
                    fn state(&self) -> &str {
                        ""
                    }
                    fn note_divide_by_zero(&self) {}
                }
                e.eval(&E)
            }
        };
        let mut reg = |r: i64| 0x0100 * (r + 1);
        let out = t.render(&mut eval, &mut reg);
        let body = [0x01, 0x03, 0x04, 0x01, 0x00, 0x02, 0x00];
        assert_eq!(&out[..7], &body);
        assert_eq!(&out[7..], &crc16_modbus(&body).to_le_bytes());
    }

    #[test]
    fn response_template_errors_name_the_item() {
        let e = ByteTemplate::parse(&["regs(1)".to_string()]).unwrap_err();
        assert!(e.contains("regs(1)"), "{e}");
        assert!(ByteTemplate::parse(&[]).is_err());
        assert!(ByteTemplate::parse(&["regs(1,2):u32be".to_string()]).is_err());
    }
}
