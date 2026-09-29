// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! The **`led_matrix` primitive**: a row/column-multiplexed LED matrix that the
//! MCU drives directly from GPIO row lines and column lines — the BBC micro:bit
//! display (5×5 on V2, wired as 3×9 on V1), a bare 8×8 matrix without a driver
//! chip, a scanned LED bar.
//!
//! # The part
//!
//! Every LED sits at the crossing of one row line and one column line. Firmware
//! lights ONE row at a time: it puts that row's pattern on the column lines,
//! turns the row on, waits, and moves to the next. The eye averages the light
//! over time (persistence of vision), so a human sees the whole picture. Within
//! its row slot a firmware may dim a pixel by turning its column off early
//! (the micro:bit's greyscale does exactly this), so brightness is a duty, not
//! a bit.
//!
//! # The model
//!
//! * An LED emits light only while its row line AND its column line are BOTH
//!   active. Active levels are configuration (`row_active_high`,
//!   `col_active_high`).
//! * The model INTEGRATES that light over time, exactly, in CPU cycles: each
//!   GPIO store moves the pads, and the time since the previous store is added
//!   to every LED that was lit during it. This is the integration of the
//!   `segment_display` primitive, with rows as the select lines and columns as
//!   the segment lines.
//! * Every `persistence_us` window the model turns each LED's on-time into a
//!   brightness 0..=255. Full brightness is being lit for a whole ROW SLOT,
//!   `window / rows`: a row-scanned matrix can light each LED for at most that
//!   long, so a picture scanned at full duty reads 255 however many rows the
//!   matrix has. A pixel lit for more (two rows selected at once) saturates.
//! * `layout` maps the electrical matrix onto the picture a human sees. The
//!   micro:bit V1 display is 25 LEDs on 3 row lines × 9 column lines, arranged
//!   on the board as 5×5; the layout says, per (row line, column line), which
//!   (x, y) pixel that LED is, or that there is no LED there. Without a layout
//!   the picture is the electrical matrix (width = columns, height = rows).
//!
//! # Readback
//!
//! * [`BusResidentDevice::logs`]: `frames` has one line per change of the
//!   visible picture on a 0..=9 scale (the micro:bit's own brightness scale),
//!   one group of digits per picture row, top first:
//!   `09090 99999 99999 09990 00900 at cycle 3200000`. `labwired test`
//!   asserts on it with `peripheral_log`.
//! * [`BusResidentDevice::evidence`]: a `framebuffer` artifact, format
//!   [`crate::inspect::artifact_format::GRAY8`], `w`×`h`, bytes = brightness row-major.
//!   `meta.levels` carries the same picture on the 0..=9 scale so a reader that
//!   does not ask for bytes still sees it.
//!
//! # Where it sits
//!
//! Exactly where `segment_display` sits: one [`BusResidentDevice`] on
//! `SystemBus::gpio_devices`, serviced from the MMIO write hook of every GPIO
//! port that hosts one of its pads, with the end of each window as a resident
//! deadline so a window closes also when firmware stops writing.
//!
//! # Not modelled
//!
//! * Electrical drive strength: the model reads the pad LEVEL (the port's pad
//!   view, which honours a peripheral that owns the pin, such as an nRF
//!   GPIOTE channel), not a current.
//! * Current sharing, LED colour and forward voltage: a lit LED is a duty.
//! * Ambient-light sensing through the matrix (the micro:bit reads light by
//!   reverse-biasing the LEDs): the column lines are never read back.

use std::collections::VecDeque;

use anyhow::{anyhow, bail, Result};

use crate::bus::{BusResidentDevice, DevicePins};
use crate::inspect::{
    artifact_bytes, artifact_format, artifact_generation, Artifact, DeviceEvidence, InspectOpts,
};
use crate::peripheral_log::PeripheralLog;
use crate::sim_input::{InputChannel, SimInput, SimInputError};

/// Most log lines kept per log. The oldest lines go first.
pub const LOG_CAPACITY: usize = 4096;
/// Most row lines, and most column lines, one matrix can have.
pub const MAX_LINES: usize = 32;

/// Everything a placement says about the matrix, before pad resolution.
#[derive(Debug, Clone)]
pub struct LedMatrixSpec {
    /// Pad labels of the row lines, in electrical order.
    pub row_pins: Vec<String>,
    /// Pad labels of the column lines, in electrical order.
    pub col_pins: Vec<String>,
    pub row_active_high: bool,
    pub col_active_high: bool,
    /// Picture size.
    pub width: usize,
    pub height: usize,
    /// `[row][col]` → picture pixel index `y * width + x`, or `None` where
    /// there is no LED.
    pub pixel_of: Vec<Vec<Option<usize>>>,
    pub persistence_us: u64,
}

/// Read a YAML value as a list of strings: a sequence, or one string split at
/// spaces and commas.
fn string_list(value: &serde_yaml::Value, what: &str) -> Result<Vec<String>> {
    match value {
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .map(|v| match v {
                serde_yaml::Value::String(s) => Ok(s.clone()),
                serde_yaml::Value::Number(n) => Ok(n.to_string()),
                other => Err(anyhow!("{what}: expected a string, got {other:?}")),
            })
            .collect(),
        serde_yaml::Value::String(s) => Ok(s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()),
        other => Err(anyhow!("{what}: expected a list, got {other:?}")),
    }
}

fn get_bool(
    config: &std::collections::HashMap<String, serde_yaml::Value>,
    key: &str,
    default: bool,
) -> Result<bool> {
    match config.get(key) {
        None => Ok(default),
        Some(serde_yaml::Value::Bool(b)) => Ok(*b),
        Some(other) => bail!("{key}: expected true or false, got {other:?}"),
    }
}

/// One `layout` cell: `"x,y"` (a pixel) or `"-"` (no LED at this crossing).
fn layout_cell(value: &serde_yaml::Value, row: usize, col: usize) -> Result<Option<(usize, usize)>> {
    let text = match value {
        serde_yaml::Value::String(s) => s.trim().to_string(),
        serde_yaml::Value::Null => return Ok(None),
        other => bail!("layout[{row}][{col}]: expected \"x,y\" or \"-\", got {other:?}"),
    };
    if text == "-" {
        return Ok(None);
    }
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| anyhow!("layout[{row}][{col}]: expected \"x,y\" or \"-\", got '{text}'"))?;
    let parse = |s: &str| -> Result<usize> {
        s.trim()
            .parse::<usize>()
            .map_err(|_| anyhow!("layout[{row}][{col}]: '{text}' is not two whole numbers"))
    };
    Ok(Some((parse(x)?, parse(y)?)))
}

impl LedMatrixSpec {
    /// Parse the placement `config:`. `persistence_us` is already resolved
    /// from the placement or the descriptor default.
    pub fn from_config(
        config: &std::collections::HashMap<String, serde_yaml::Value>,
        persistence_us: u64,
    ) -> Result<Self> {
        let row_pins = string_list(
            config
                .get("row_pins")
                .ok_or_else(|| anyhow!("config is missing the 'row_pins' list"))?,
            "row_pins",
        )?;
        let col_pins = string_list(
            config
                .get("col_pins")
                .ok_or_else(|| anyhow!("config is missing the 'col_pins' list"))?,
            "col_pins",
        )?;
        for (what, pins) in [("row_pins", &row_pins), ("col_pins", &col_pins)] {
            if pins.is_empty() || pins.len() > MAX_LINES {
                bail!("{what} must list 1..={MAX_LINES} pads, got {}", pins.len());
            }
        }
        if persistence_us == 0 {
            bail!("persistence_us must be more than 0");
        }
        let (rows, cols) = (row_pins.len(), col_pins.len());

        let (width, height, pixel_of) = match config.get("layout") {
            None => {
                let pixel_of = (0..rows)
                    .map(|r| (0..cols).map(|c| Some(r * cols + c)).collect())
                    .collect();
                (cols, rows, pixel_of)
            }
            Some(serde_yaml::Value::Sequence(layout_rows)) => {
                if layout_rows.len() != rows {
                    bail!(
                        "layout has {} rows but row_pins has {rows} pads",
                        layout_rows.len()
                    );
                }
                let mut cells = Vec::with_capacity(rows);
                for (r, line) in layout_rows.iter().enumerate() {
                    let items = line
                        .as_sequence()
                        .ok_or_else(|| anyhow!("layout[{r}]: expected a list of cells"))?;
                    if items.len() != cols {
                        bail!(
                            "layout[{r}] has {} cells but col_pins has {cols} pads",
                            items.len()
                        );
                    }
                    cells.push(
                        items
                            .iter()
                            .enumerate()
                            .map(|(c, v)| layout_cell(v, r, c))
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                let used: Vec<(usize, usize)> = cells.iter().flatten().flatten().copied().collect();
                if used.is_empty() {
                    bail!("layout places no LED");
                }
                let width = used.iter().map(|p| p.0).max().unwrap_or(0) + 1;
                let height = used.iter().map(|p| p.1).max().unwrap_or(0) + 1;
                for (i, p) in used.iter().enumerate() {
                    if used[..i].contains(p) {
                        bail!("layout places two LEDs at pixel {},{}", p.0, p.1);
                    }
                }
                let pixel_of = cells
                    .iter()
                    .map(|row| row.iter().map(|p| p.map(|(x, y)| y * width + x)).collect())
                    .collect();
                (width, height, pixel_of)
            }
            Some(other) => bail!("layout: expected a list of rows, got {other:?}"),
        };

        Ok(Self {
            row_pins,
            col_pins,
            row_active_high: get_bool(config, "row_active_high", true)?,
            col_active_high: get_bool(config, "col_active_high", true)?,
            width,
            height,
            pixel_of,
            persistence_us,
        })
    }
}

/// One resolved pad: output register address and bit.
#[derive(Debug, Clone, Copy)]
pub struct Pad {
    pub addr: u64,
    pub bit: u8,
}

/// Brightness 0..=255 → the 0..=9 scale of the `frames` log (rounded).
fn level9(brightness: u8) -> u8 {
    ((brightness as u16 * 9 + 127) / 255) as u8
}

/// The multiplexed LED matrix.
#[derive(Debug)]
pub struct DeclarativeLedMatrix {
    id: String,
    spec: LedMatrixSpec,
    rows: Vec<Pad>,
    cols: Vec<Pad>,
    /// Output registers that host a pad, for the write hook.
    edge_addrs: Vec<u64>,
    window_cycles: u64,
    /// Active row lines and active column lines, as last sampled.
    row_active: u32,
    col_active: u32,
    last_cycle: Option<u64>,
    window_start: u64,
    /// Lit time in cycles, `[row][col]`, in the open window.
    on_time: Vec<Vec<u64>>,
    /// Picture brightness 0..=255, row-major, from the last closed window.
    shown: Vec<u8>,
    /// `shown` on the 0..=9 scale; a change of it is a `frames` line.
    shown9: Vec<u8>,
    frame_log: VecDeque<String>,
}

impl DeclarativeLedMatrix {
    /// Build from a parsed spec whose pads are already resolved.
    pub fn new(
        id: String,
        spec: LedMatrixSpec,
        rows: Vec<Pad>,
        cols: Vec<Pad>,
        cpu_hz: u64,
    ) -> Result<Self> {
        if rows.len() != spec.row_pins.len() || cols.len() != spec.col_pins.len() {
            bail!("led_matrix '{id}': resolved pads do not match the spec");
        }
        let window_cycles =
            ((spec.persistence_us as u128 * cpu_hz.max(1) as u128) / 1_000_000).max(1) as u64;
        let mut edge_addrs: Vec<u64> = rows.iter().chain(&cols).map(|p| p.addr).collect();
        edge_addrs.sort_unstable();
        edge_addrs.dedup();
        let pixels = spec.width * spec.height;
        Ok(Self {
            id,
            on_time: vec![vec![0; cols.len()]; rows.len()],
            shown: vec![0; pixels],
            shown9: vec![0; pixels],
            spec,
            rows,
            cols,
            edge_addrs,
            window_cycles,
            row_active: 0,
            col_active: 0,
            last_cycle: None,
            window_start: 0,
            frame_log: VecDeque::new(),
        })
    }

    /// Picture brightness 0..=255, row-major (`width` × `height`), from the
    /// last closed window.
    pub fn brightness(&self) -> &[u8] {
        &self.shown
    }

    /// Picture size.
    pub fn size(&self) -> (usize, usize) {
        (self.spec.width, self.spec.height)
    }

    /// Length of one persistence window in cycles.
    pub fn window_cycles(&self) -> u64 {
        self.window_cycles
    }

    fn sample(&mut self, pins: &dyn DevicePins) {
        let active = |pad: &Pad, high: bool| {
            // The PAD, not the output register: on the micro:bit V2 the
            // columns are driven by GPIOTE, which owns the pin while the
            // port's OUT holds something else. An address that does not read
            // back drives nothing: LED off.
            pins.pad_bit(pad.addr, pad.bit)
                .is_some_and(|level| level == high)
        };
        let mut row = 0u32;
        for (i, pad) in self.rows.iter().enumerate() {
            if active(pad, self.spec.row_active_high) {
                row |= 1 << i;
            }
        }
        let mut col = 0u32;
        for (i, pad) in self.cols.iter().enumerate() {
            if active(pad, self.spec.col_active_high) {
                col |= 1 << i;
            }
        }
        self.row_active = row;
        self.col_active = col;
    }

    /// Add `dt` cycles to every LED that is lit in the current pad state.
    fn integrate(&mut self, dt: u64) {
        if dt == 0 || self.row_active == 0 || self.col_active == 0 {
            return;
        }
        for (r, line) in self.on_time.iter_mut().enumerate() {
            if self.row_active & (1 << r) == 0 {
                continue;
            }
            for (c, t) in line.iter_mut().enumerate() {
                if self.col_active & (1 << c) != 0 {
                    *t = t.saturating_add(dt);
                }
            }
        }
    }

    fn close_window(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.window_start).max(1) as u128;
        // Full brightness: lit for a whole row slot of this window.
        let slot = (elapsed / self.rows.len() as u128).max(1);
        let mut picture = vec![0u8; self.spec.width * self.spec.height];
        for (r, line) in self.on_time.iter().enumerate() {
            for (c, &t) in line.iter().enumerate() {
                if let Some(px) = self.spec.pixel_of[r][c] {
                    picture[px] = ((t as u128 * 255) / slot).min(255) as u8;
                }
            }
        }
        for line in &mut self.on_time {
            line.fill(0);
        }
        self.window_start = now;
        let picture9: Vec<u8> = picture.iter().map(|&b| level9(b)).collect();
        self.shown = picture;
        if picture9 != self.shown9 {
            self.shown9 = picture9;
            let line = self.levels_text();
            push_capped(&mut self.frame_log, format!("{line} at cycle {now}"));
        }
    }

    /// The picture on the 0..=9 scale, one group per row, top first.
    fn levels_text(&self) -> String {
        self.shown9
            .chunks(self.spec.width)
            .map(|row| row.iter().map(|d| char::from(b'0' + d)).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Integrate the pad state held since the last service up to `now`,
    /// closing every window whose end falls inside that span at its exact
    /// end cycle.
    fn advance(&mut self, now: u64) {
        let Some(mut last) = self.last_cycle else {
            self.last_cycle = Some(now);
            self.window_start = now;
            return;
        };
        if now <= last {
            return;
        }
        loop {
            let end = self.window_start.saturating_add(self.window_cycles);
            if now < end {
                self.integrate(now - last);
                break;
            }
            self.integrate(end - last);
            self.close_window(end);
            last = end;
            // The pad state is constant up to `now`, so every further whole
            // window looks the same: close one, then skip the rest.
            let whole = (now - end) / self.window_cycles;
            if whole >= 2 {
                let next = end + self.window_cycles;
                self.integrate(self.window_cycles);
                self.close_window(next);
                self.window_start = end + whole * self.window_cycles;
                last = self.window_start;
            }
        }
        self.last_cycle = Some(now);
    }
}

fn push_capped(log: &mut VecDeque<String>, line: String) {
    if log.len() == LOG_CAPACITY {
        log.pop_front();
    }
    log.push_back(line);
}

const NO_CHANNELS: &[InputChannel] = &[];

impl BusResidentDevice for DeclarativeLedMatrix {
    /// The tick pass: the pads cannot have moved since the last store (the
    /// write hook reads them), so only time advances.
    fn service(&mut self, _pins: &mut dyn DevicePins, now: u64) {
        self.advance(now);
    }

    fn service_edge(&mut self, pins: &mut dyn DevicePins, now: u64) {
        // Time up to `now` belongs to the levels that were on the pads before
        // this store; only then read the new levels.
        self.advance(now);
        self.sample(pins);
    }

    /// The pads move only on a GPIO store, which the write hook services. A
    /// window end with no store is a deadline (below), not a tick.
    fn needs_per_cycle_service(&self) -> bool {
        false
    }

    fn edge_service_addrs(&self) -> &[u64] {
        &self.edge_addrs
    }

    /// The end of the open persistence window.
    fn next_edge_deadline_cycle(&self, _now: u64, _interval: u64) -> Option<u64> {
        self.last_cycle
            .map(|_| self.window_start.saturating_add(self.window_cycles))
    }

    /// Close a window that ended with no GPIO store. The pads did not move,
    /// so they are not read.
    fn service_scheduled_edges(&mut self, _pins: &mut dyn DevicePins, now: u64, _interval: u64) {
        if self.last_cycle.is_some() && now.saturating_sub(self.window_start) >= self.window_cycles
        {
            self.advance(now);
        }
    }

    fn evidence(&self) -> Option<&dyn DeviceEvidence> {
        Some(self)
    }

    fn logs(&self) -> Vec<PeripheralLog> {
        vec![PeripheralLog::new(
            "frames",
            self.frame_log.iter().cloned().collect(),
        )]
    }

    fn as_sim_input(&mut self) -> &mut dyn SimInput {
        self
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl DeviceEvidence for DeclarativeLedMatrix {
    fn artifacts(&self, id: &str, opts: &InspectOpts) -> Vec<Artifact> {
        let meta = serde_json::json!({
            "format": artifact_format::GRAY8,
            "generation": artifact_generation(&self.shown),
            "w": self.spec.width,
            "h": self.spec.height,
            "rows": self.rows.len(),
            "cols": self.cols.len(),
            "levels": self.levels_text(),
        });
        vec![Artifact {
            kind: "framebuffer".to_string(),
            id: id.to_string(),
            meta,
            bytes: artifact_bytes(&self.shown, opts),
        }]
    }
}

impl SimInput for DeclarativeLedMatrix {
    /// None: the matrix is driven by its pads.
    fn input_channels(&self) -> &[InputChannel] {
        NO_CHANNELS
    }

    fn set_input(&mut self, key: &str, value: f64) -> Result<(), SimInputError> {
        self.require_channel(key, value)?;
        unreachable!("a led_matrix declares no channels")
    }

    fn component_id(&self) -> Option<&str> {
        Some(&self.id)
    }
}

/// Validate the static descriptor contract of the `led_matrix` primitive.
pub(crate) fn validate_descriptor(desc: &labwired_config::DeviceDescriptor) -> Result<()> {
    if desc.behavior.primitive != "led_matrix" {
        bail!(
            "'{}' is not a led_matrix descriptor (primitive '{}')",
            desc.r#type,
            desc.behavior.primitive
        );
    }
    if !desc.behavior.pins.is_empty() || !desc.behavior.rules.is_empty() {
        bail!(
            "led_matrix '{}': pins come from the placement lists \
             (row_pins, col_pins); the descriptor declares no pins or rules",
            desc.r#type
        );
    }
    let default = desc
        .behavior
        .params
        .get("persistence_us")
        .and_then(|v| v.get("default"))
        .and_then(|v| v.as_u64());
    if default.is_none() {
        bail!(
            "led_matrix '{}': params.persistence_us needs an integer default",
            desc.r#type
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    /// Two GPIO output registers: rows on 0x100, columns on 0x200.
    #[derive(Default)]
    struct Pads {
        regs: BTreeMap<u64, u32>,
    }

    impl DevicePins for Pads {
        fn output_bit(&self, addr: u64, bit: u8) -> Option<bool> {
            self.regs.get(&addr).map(|v| (v >> bit) & 1 != 0)
        }
        fn drive_idr_bit(&mut self, _: u64, _: u8, _: bool) {}
        fn drive_input_bit(&mut self, _: u64, _: u8, _: bool) -> bool {
            false
        }
    }

    const ROW: u64 = 0x100;
    const COL: u64 = 0x200;
    /// 1 MHz: one cycle is one microsecond.
    const HZ: u64 = 1_000_000;

    fn config(yaml: &str) -> HashMap<String, serde_yaml::Value> {
        serde_yaml::from_str(yaml).expect("config parses")
    }

    /// A micro:bit V2-shaped 5×5 matrix: rows active high on ROW bits 0..4,
    /// columns active LOW on COL bits 0..4.
    fn matrix(extra: &str) -> DeclarativeLedMatrix {
        let yaml = format!(
            "row_pins: [r0, r1, r2, r3, r4]\ncol_pins: [c0, c1, c2, c3, c4]\n\
             col_active_high: false\n{extra}"
        );
        let spec = LedMatrixSpec::from_config(&config(&yaml), 20_000).expect("spec");
        let rows = (0..5).map(|b| Pad { addr: ROW, bit: b }).collect();
        let cols = (0..5).map(|b| Pad { addr: COL, bit: b }).collect();
        DeclarativeLedMatrix::new("m".into(), spec, rows, cols, HZ).expect("new")
    }

    /// Store row lines `rows` and lit columns `lit` (active low, so the
    /// register holds the complement) at cycle `now` and service.
    fn store(m: &mut DeclarativeLedMatrix, pads: &mut Pads, now: u64, rows: u32, lit: u32) {
        pads.regs.insert(ROW, rows);
        pads.regs.insert(COL, !lit & 0x1F);
        m.service_edge(pads, now);
    }

    /// Firmware that scans `picture` (lit-column mask per row, top first) one
    /// row per 1 ms slot until `end`. `on_us` of each slot the columns are
    /// lit; the rest of the slot they are off (dimming by duty).
    fn scan(m: &mut DeclarativeLedMatrix, pads: &mut Pads, picture: &[u32; 5], on_us: u64, end: u64) {
        let mut now = 0;
        let mut row = 0usize;
        while now < end {
            store(m, pads, now, 1 << row, picture[row]);
            if on_us < 1000 {
                store(m, pads, now + on_us, 1 << row, 0);
            }
            now += 1000;
            row = (row + 1) % 5;
        }
        m.service(pads, now);
    }

    fn frames(m: &DeclarativeLedMatrix) -> Vec<String> {
        m.logs()
            .into_iter()
            .find(|l| l.name == "frames")
            .expect("frames log")
            .lines()
    }

    /// micro:bit's heart: `.#.#.` `#####` `#####` `.###.` `..#..`, bit =
    /// column (bit 0 = left).
    const HEART: [u32; 5] = [0b01010, 0b11111, 0b11111, 0b01110, 0b00100];

    fn levels(m: &DeclarativeLedMatrix) -> String {
        m.levels_text()
    }

    #[test]
    fn a_scanned_picture_is_seen_whole_at_full_brightness() {
        let mut m = matrix("");
        let mut pads = Pads::default();
        scan(&mut m, &mut pads, &HEART, 1000, 100_000);
        assert_eq!(levels(&m), "09090 99999 99999 09990 00900");
        let b = m.brightness();
        assert_eq!(b[1], 255, "a pixel lit for its whole row slot is full brightness");
        assert_eq!(b[0], 0);
        assert!(frames(&m)
            .iter()
            .any(|l| l.starts_with("09090 99999 99999 09990 00900 at cycle")));
    }

    #[test]
    fn a_column_off_early_in_its_slot_dims_the_pixel() {
        let mut m = matrix("");
        let mut pads = Pads::default();
        // Lit for 300 us of every 1000 us slot: about a third of full.
        scan(&mut m, &mut pads, &HEART, 300, 100_000);
        let lit = m.brightness()[1];
        assert!((70..=90).contains(&lit), "30 % duty reads about 77, got {lit}");
        assert_eq!(levels(&m), "03030 33333 33333 03330 00300");
    }

    #[test]
    fn a_row_line_alone_or_a_column_alone_lights_nothing() {
        let mut m = matrix("");
        let mut pads = Pads::default();
        store(&mut m, &mut pads, 0, 0b11111, 0);
        m.service(&mut pads, 50_000);
        assert_eq!(levels(&m), "00000 00000 00000 00000 00000");
        let mut m = matrix("");
        store(&mut m, &mut pads, 0, 0, 0b11111);
        m.service(&mut pads, 50_000);
        assert_eq!(levels(&m), "00000 00000 00000 00000 00000");
        assert!(frames(&m).is_empty(), "a picture that never lit logs no frame");
    }

    #[test]
    fn a_matrix_that_stops_scanning_goes_dark() {
        let mut m = matrix("");
        let mut pads = Pads::default();
        scan(&mut m, &mut pads, &HEART, 1000, 60_000);
        assert_eq!(levels(&m), "09090 99999 99999 09990 00900");
        store(&mut m, &mut pads, 60_010, 0, 0);
        m.service(&mut pads, 200_000);
        assert_eq!(levels(&m), "00000 00000 00000 00000 00000");
        assert_eq!(
            frames(&m).last().map(|l| l.starts_with("00000 00000 00000 00000 00000")),
            Some(true)
        );
    }

    #[test]
    fn active_levels_are_configuration() {
        // The same scan read with active-HIGH columns sees the complement.
        let mut m = matrix("col_active_high: true\n");
        let mut pads = Pads::default();
        scan(&mut m, &mut pads, &HEART, 1000, 100_000);
        assert_eq!(levels(&m), "90909 00000 00000 90009 99099");
    }

    #[test]
    fn a_layout_maps_the_electrical_matrix_onto_the_picture() {
        // 2 row lines × 3 column lines arranged as a 3×2 picture, with the
        // second line's order reversed and one crossing without an LED.
        let yaml = "row_pins: [a, b]\ncol_pins: [x, y, z]\n\
                    layout: [[\"0,0\", \"1,0\", \"2,0\"], [\"-\", \"1,1\", \"0,1\"]]\n";
        let spec = LedMatrixSpec::from_config(&config(yaml), 20_000).expect("spec");
        assert_eq!((spec.width, spec.height), (3, 2));
        let rows = (0..2).map(|b| Pad { addr: ROW, bit: b }).collect();
        let cols = (0..3).map(|b| Pad { addr: COL, bit: b }).collect();
        let mut m = DeclarativeLedMatrix::new("l".into(), spec, rows, cols, HZ).expect("new");
        let mut pads = Pads::default();
        // Row line b, column line z (active high here): pixel (0,1).
        pads.regs.insert(ROW, 0b10);
        pads.regs.insert(COL, 0b100);
        m.service_edge(&mut pads, 0);
        m.service(&mut pads, 50_000);
        assert_eq!(m.levels_text(), "000 900");
        // Row line b, column line x has no LED: nothing.
        let spec = LedMatrixSpec::from_config(&config(yaml), 20_000).expect("spec");
        let rows = (0..2).map(|b| Pad { addr: ROW, bit: b }).collect();
        let cols = (0..3).map(|b| Pad { addr: COL, bit: b }).collect();
        let mut m = DeclarativeLedMatrix::new("l".into(), spec, rows, cols, HZ).expect("new");
        pads.regs.insert(COL, 0b001);
        m.service_edge(&mut pads, 0);
        m.service(&mut pads, 50_000);
        assert_eq!(m.levels_text(), "000 000");
    }

    #[test]
    fn the_artifact_is_a_gray8_framebuffer() {
        let mut m = matrix("");
        let mut pads = Pads::default();
        scan(&mut m, &mut pads, &HEART, 1000, 100_000);
        let with = m.artifacts("m", &InspectOpts { include_bytes: true, peripheral: None });
        assert_eq!(with.len(), 1);
        assert!(crate::inspect::is_display_artifact(&with[0]));
        assert_eq!(with[0].meta["format"], artifact_format::GRAY8);
        assert_eq!((with[0].meta["w"].as_u64(), with[0].meta["h"].as_u64()), (Some(5), Some(5)));
        assert_eq!(with[0].bytes.as_deref(), Some(m.brightness()));
        assert_eq!(with[0].meta["levels"], "09090 99999 99999 09990 00900");
        let without = m.artifacts("m", &InspectOpts { include_bytes: false, peripheral: None });
        assert!(without[0].bytes.is_none());
    }

    #[test]
    fn bad_config_is_refused() {
        let err = LedMatrixSpec::from_config(&config("col_pins: [a]\n"), 1).unwrap_err();
        assert!(err.to_string().contains("row_pins"), "{err}");
        let two_by_one = "row_pins: [a, b]\ncol_pins: [x]\n";
        let err = LedMatrixSpec::from_config(
            &config(&format!("{two_by_one}layout: [[\"0,0\"]]\n")),
            1,
        )
        .unwrap_err();
        assert!(err.to_string().contains("layout has 1 rows"), "{err}");
        let err = LedMatrixSpec::from_config(
            &config(&format!("{two_by_one}layout: [[\"0,0\"], [\"0,0\"]]\n")),
            1,
        )
        .unwrap_err();
        assert!(err.to_string().contains("two LEDs"), "{err}");
        let err = LedMatrixSpec::from_config(&config(two_by_one), 0).unwrap_err();
        assert!(err.to_string().contains("persistence_us"), "{err}");
    }
}
