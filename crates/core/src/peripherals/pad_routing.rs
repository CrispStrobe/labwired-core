// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! The ONE way a GPIO port decides that a pad is being driven by a peripheral
//! rather than by its own output latch.
//!
//! # The question every family asks
//!
//! "Firmware is reading pad N — who is actually driving it?" Every chip answers
//! it the same way in principle and differently in registers:
//!
//! | family | selector | where it lives |
//! |---|---|---|
//! | STM32 V2 | AF nibble | `MODER` selects AF, `AFRL`/`AFRH` pick which |
//! | STM32 F1 | fixed map | `CRL`/`CRH` mode+CNF say "AF output" |
//! | ESP32 / -S3 / -C3 | matrix index | `GPIO_FUNCn_OUT_SEL_CFG.out_sel` |
//! | RP2040 | function number | `IO_BANK0.GPIOn_CTRL.FUNCSEL` |
//!
//! The register decode differs; the *shape* does not. A pad currently selects
//! some peripheral signal, and if that signal is one a live peripheral
//! publishes a wire for, the pad reads that wire instead of the GPIO latch.
//!
//! [`PadRoutes`] is that shape, once. A family binds `(pad, selector) → (wire,
//! line)` at config-build time and supplies a closure that decodes its own
//! registers into "the selector pad N currently has". Everything downstream —
//! `read_gpio_pad`, the logic analyzer sampling through it, push-capture
//! registration, and the signal name in `gpio_routing` — is handled here.
//!
//! Before this, three families had grown three parallel copies of it (the STM32
//! AF table, and bespoke `set_i2c_lines` paths on the C3 and S3), each with its
//! own tap-registration bookkeeping. Adding UART or SPI visibility to a family
//! meant a fourth. Now it means one more binding.
//!
//! # What a family owes this type
//!
//! * Call [`PadRoutes::bind`] once per (pad, signal) the datasheet allows.
//! * Pass a selector decode to [`PadRoutes::level`] and friends that reads the
//!   live registers — so re-routing a pad at runtime follows immediately, and a
//!   pad taken back for plain GPIO stops reading the wire.
//! * Call [`PadRoutes::sync_taps`] after any register write, so push capture
//!   follows a pad that changed hands.

use std::sync::Arc;

use crate::logic_capture::LogicTap;
use crate::peripherals::pad_lines::PadLines;

/// One pad that a peripheral signal can drive.
#[derive(Debug, Clone)]
struct Route {
    pin: u8,
    /// The selector value this pad must currently hold for the route to be
    /// live: an AF nibble, a GPIO-matrix signal index, a FUNCSEL number.
    /// `None` means "this family has a fixed mapping" — the decode closure
    /// answers whether the pad is handed to a peripheral at all.
    selector: Option<u32>,
    /// Index into the bound cell's lines.
    line: usize,
    /// Signal name surfaced through `gpio_routing().func`, e.g. `"I2C1_SDA"`.
    func: &'static str,
    /// A SPAN route ([`PadRoutes::bind_span`]): selectors `selector ..
    /// selector + span_funcs.len()` drive lines `line ..` of the same cell, one
    /// name each. Empty for an ordinary one-selector route.
    span_funcs: &'static [&'static str],
    /// Index into [`PadRoutes::cells`].
    cell: usize,
    /// A CONDITIONAL route ([`PadRoutes::bind_when_driving`]): live only
    /// while its line's output stage drives the pad, and then ahead of every
    /// ordinary route on the same pin.
    when_driving: bool,
}

/// The one rule for which route owns a pad, shared by [`PadRoutes`] and
/// [`PinRoute::select`] so a listener and a pad read can never disagree.
/// `view` answers `(on this pad, selector, conditional)`. Read the wire
/// drive state only for a matching conditional route: ordinary routes and
/// routes on other pads do not need that observation.
/// A conditional route wins while its line drives the pad; otherwise the
/// first ordinary route matching the pad's current selector does.
#[inline]
fn select_route<T>(
    routes: &[T],
    current: u32,
    view: impl Fn(&T) -> (bool, Option<u32>, bool),
    drives: impl Fn(&T) -> bool,
) -> Option<&T> {
    let conditional = routes.iter().find(|r| {
        let (on_pin, selector, when_driving) = view(r);
        on_pin && when_driving && selector.is_none_or(|want| want == current) && drives(r)
    });
    if conditional.is_some() {
        return conditional;
    }
    routes.iter().find(|r| {
        let (on_pin, selector, when_driving) = view(r);
        on_pin && !when_driving && selector.is_none_or(|want| want == current)
    })
}

/// An owned copy of one [`PadRoutes`] binding, for a consumer that must decide
/// off the CPU walk (from the wire's own transition) whether the pad currently
/// selects this wire.
#[derive(Debug, Clone)]
pub struct PinRoute {
    pub cell: Arc<PadLines>,
    pub line: usize,
    selector: Option<u32>,
    when_driving: bool,
}

impl PinRoute {
    /// Index of the route that owns the pad when it selects `current`
    /// (`None`: the pad is plain GPIO), by the same rule as a pad read.
    pub fn select(routes: &[PinRoute], current: Option<u32>) -> Option<usize> {
        let current = current?;
        let hit = select_route(
            routes,
            current,
            |r| (true, r.selector, r.when_driving),
            |r| r.cell.drives(r.line),
        )?;
        routes.iter().position(|r| std::ptr::eq(r, hit))
    }
}

/// Every pad-to-peripheral-wire binding on one GPIO port.
///
/// Empty on a port with no peripheral routed to it, where every operation
/// costs one `is_empty` check.
#[derive(Debug, Default)]
pub struct PadRoutes {
    /// Deduplicated wires, so several pads of one controller share an entry.
    cells: Vec<Arc<PadLines>>,
    routes: Vec<Route>,
    /// Append-only route indices for each pad. Reads never scan other pads.
    by_pin: Vec<Vec<usize>>,
    /// Per cell, per line: the watch channels last registered with that wire.
    /// Cached so an unrelated register write costs no mutex traffic.
    registered: Vec<Vec<Vec<u32>>>,
}

impl PadRoutes {
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when no peripheral is routed to this port at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// Every distinct signal name BOUND to this port, regardless of which one a
    /// pad currently selects — `["I2C1_SCL", "I2C1_SDA", …]`, in binding order.
    ///
    /// Deliberately *not* [`Self::func`]: that answers "who drives pin N right
    /// now", which depends on live register state and is therefore zero on a
    /// freshly built bus. This answers the static question "what could ever be
    /// seen here", which is what the bus-visibility scoreboard
    /// (`crates/core/tests/bus_visibility.rs`) measures — a bus with no binding
    /// can never reach a pad, so the logic analyzer can never show it, no matter
    /// what the firmware does.
    ///
    /// Read-only and allocating; it is a reporting seam, not a hot path.
    pub fn bound_functions(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for route in &self.routes {
            let names = if route.span_funcs.is_empty() {
                std::slice::from_ref(&route.func)
            } else {
                route.span_funcs
            };
            for &func in names {
                if !out.contains(&func) {
                    out.push(func);
                }
            }
        }
        out
    }

    /// Bind `pin` to `line` of `cell`, live whenever the pad's selector reads
    /// `selector`. Call once per datasheet-allowed (pad, signal) pair.
    pub fn bind(
        &mut self,
        cell: &Arc<PadLines>,
        pin: u8,
        selector: Option<u32>,
        line: usize,
        func: &'static str,
    ) {
        let cell_idx = self.cell_index(cell);
        self.push_route(Route {
            pin,
            selector,
            line,
            func,
            span_funcs: &[],
            cell: cell_idx,
            when_driving: false,
        });
    }

    /// Bind `pin` to `line` of `cell` CONDITIONALLY: the route is live only
    /// while the line's output stage drives the pad ([`PadLines::drives`]),
    /// and while it is, it wins the pad over every ordinary route bound there.
    ///
    /// This is the STM32F1 shared-pad rule. DS5319 Rev 20 Table 5 puts several
    /// DEFAULT functions on one pad (PA7 = `SPI1_MOSI/TIM3_CH2`, PA9 =
    /// `USART1_TX/TIM1_CH2`, PB6 = `I2C1_SCL/TIM4_CH1`), and note 4 of that
    /// table resolves the clash by "only one peripheral should be enabled at a
    /// time" — the F1 pad has no mux to ask. A timer channel whose output is
    /// not enabled (`CCxE = 0`, or MOE clear on TIM1) does not drive its pad,
    /// so the pad keeps whatever an ordinary route (or nothing) gives it; a
    /// channel firmware has enabled does drive it, and that is the level the
    /// pad carries.
    pub fn bind_when_driving(
        &mut self,
        cell: &Arc<PadLines>,
        pin: u8,
        selector: Option<u32>,
        line: usize,
        func: &'static str,
    ) {
        let cell_idx = self.cell_index(cell);
        self.push_route(Route {
            pin,
            selector,
            line,
            func,
            span_funcs: &[],
            cell: cell_idx,
            when_driving: true,
        });
    }

    /// Bind `pin` to a contiguous BLOCK of signals in one call: selector
    /// `first_selector + k` drives line `first_line + k` of `cell`, named
    /// `funcs[k]`.
    ///
    /// For a controller with many outputs on consecutive matrix indices — the
    /// ESP LEDC's eight (C3: six) channel outputs per group — where one route
    /// per (pad, channel) would multiply the table by the channel count, and a
    /// routed pad read scans the table.
    pub fn bind_span(
        &mut self,
        cell: &Arc<PadLines>,
        pin: u8,
        first_selector: u32,
        first_line: usize,
        funcs: &'static [&'static str],
    ) {
        if funcs.is_empty() {
            return;
        }
        let cell_idx = self.cell_index(cell);
        self.push_route(Route {
            pin,
            selector: Some(first_selector),
            line: first_line,
            func: funcs[0],
            span_funcs: funcs,
            cell: cell_idx,
            when_driving: false,
        });
    }

    fn push_route(&mut self, route: Route) {
        let pin = usize::from(route.pin);
        if self.by_pin.len() <= pin {
            self.by_pin.resize_with(pin + 1, Vec::new);
        }
        self.by_pin[pin].push(self.routes.len());
        self.routes.push(route);
    }

    /// Index of `cell` in [`Self::cells`], registering it on first use.
    fn cell_index(&mut self, cell: &Arc<PadLines>) -> usize {
        match self.cells.iter().position(|c| Arc::ptr_eq(c, cell)) {
            Some(i) => i,
            None => {
                self.cells.push(cell.clone());
                self.registered.push(vec![Vec::new(); cell.names().len()]);
                self.cells.len() - 1
            }
        }
    }

    /// The route currently driving `pin`, if any.
    ///
    /// `selector_of` decodes the family's own registers: `Some(value)` when the
    /// pad is handed to a peripheral (the AF nibble / matrix index / FUNCSEL),
    /// `None` when it is a plain GPIO. A route with `selector: None` matches
    /// any `Some`, which is the fixed-mapping case (STM32 F1).
    ///
    /// Returns the route and the live `(line, name)` it resolves to — which, for
    /// a span route, depends on WHICH selector of the span the pad holds.
    fn active<F>(&self, pin: u8, selector_of: F) -> Option<(&Route, usize, &'static str)>
    where
        F: Fn(u8) -> Option<u32>,
    {
        let candidates = self.by_pin.get(usize::from(pin))?;
        if candidates.is_empty() {
            return None;
        }
        let current = selector_of(pin)?;
        let index = select_route(
            candidates,
            current,
            |&i| {
                let r = &self.routes[i];
                let selector = if r.span_funcs.is_empty() {
                    r.selector
                } else {
                    // The family selector chooses a different line within this span.
                    r.selector
                        .filter(|start| {
                            current
                                .checked_sub(*start)
                                .is_some_and(|k| (k as usize) < r.span_funcs.len())
                        })
                        .map(|_| current)
                };
                let in_span = r.span_funcs.is_empty() || selector.is_some();
                (in_span, selector, r.when_driving)
            },
            |&i| {
                let r = &self.routes[i];
                self.cells[r.cell].drives(r.line)
            },
        )?;
        let route = &self.routes[*index];
        let offset = if route.span_funcs.is_empty() {
            0
        } else {
            current.checked_sub(route.selector?)? as usize
        };
        let func = if route.span_funcs.is_empty() {
            route.func
        } else {
            *route.span_funcs.get(offset)?
        };
        Some((route, route.line + offset, func))
    }

    /// Every route `pin` can ever take, as owned handles a listener can keep
    /// (see [`PinRoute`]). Allocating; call when routing is (re)built, not per
    /// edge.
    pub fn pin_routes(&self, pin: u8) -> Vec<PinRoute> {
        self.routes
            .iter()
            .filter(|r| r.pin == pin)
            .flat_map(|r| {
                (0..r.span_funcs.len().max(1)).filter_map(move |offset| {
                    let selector = match r.selector {
                        Some(start) => Some(start.checked_add(offset as u32)?),
                        None => None,
                    };
                    Some(PinRoute {
                        cell: self.cells[r.cell].clone(),
                        line: r.line + offset,
                        selector,
                        when_driving: r.when_driving,
                    })
                })
            })
            .collect()
    }

    /// How many routes are bound; a change means routing was (re)built.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// The live wire level for `pin`, or `None` when no peripheral drives it
    /// and the caller should fall back to its own register truth.
    pub fn level<F>(&self, pin: u8, selector_of: F) -> Option<bool>
    where
        F: Fn(u8) -> Option<u32>,
    {
        let (route, line, _) = self.active(pin, selector_of)?;
        Some(self.cells[route.cell].level(line))
    }

    /// The wire and line `pin` carries right now, if it selects one.
    pub fn active_line<F>(&self, pin: u8, selector_of: F) -> Option<(&Arc<PadLines>, usize)>
    where
        F: Fn(u8) -> Option<u32>,
    {
        let (route, line, _) = self.active(pin, selector_of)?;
        Some((&self.cells[route.cell], line))
    }

    /// Mark every line `pin` can carry (whatever it selects now) as reachable
    /// from a world `gpio_net`: the pad was put on one.
    pub fn mark_on_net(&self, pin: u8) {
        for route in self.routes.iter().filter(|r| r.pin == pin) {
            // A span route carries one line per selector in its span.
            for k in 0..route.span_funcs.len().max(1) {
                self.cells[route.cell].mark_on_net(route.line + k);
            }
        }
    }

    /// The datasheet name of the signal currently driving `pin`, for
    /// `gpio_routing().func`.
    pub fn func<F>(&self, pin: u8, selector_of: F) -> Option<&'static str>
    where
        F: Fn(u8) -> Option<u32>,
    {
        self.active(pin, selector_of).map(|(_, _, func)| func)
    }

    /// Register `watched` pads' channels with the wires that drive them, so
    /// each wire reports its own transitions at the cycles they occurred
    /// (push capture). Only wires whose channel set actually changed are
    /// touched, so an unrelated register write is nearly free.
    ///
    /// Call after every register write: a pad that changed hands must stop
    /// reporting from its old source and start reporting from its new one.
    pub fn sync_taps<F>(&mut self, tap: &LogicTap, watched: &[(u8, u32)], selector_of: F)
    where
        F: Fn(u8) -> Option<u32>,
    {
        if self.routes.is_empty() {
            return;
        }
        let mut per_cell: Vec<Vec<Vec<u32>>> = self
            .cells
            .iter()
            .map(|cell| vec![Vec::new(); cell.names().len()])
            .collect();
        for &(pin, channel) in watched {
            if let Some((route, line, _)) = self.active(pin, &selector_of) {
                if let Some(slot) = per_cell[route.cell].get_mut(line) {
                    slot.push(channel);
                }
            }
            // A conditional route can take the pad over WITHOUT a write to
            // this port (a timer enabling its channel output), and only a
            // port write re-syncs taps. So its line reports to the watch
            // channel whenever the pad selects it (any AF on an F1 port, whose
            // routes carry no selector; the route's own AF nibble on a V2
            // port); it publishes nothing while it does not drive.
            if let Some(current) = selector_of(pin) {
                for r in self.routes.iter().filter(|r| {
                    r.when_driving && r.pin == pin && r.selector.is_none_or(|want| want == current)
                }) {
                    if let Some(slot) = per_cell[r.cell].get_mut(r.line) {
                        if !slot.contains(&channel) {
                            slot.push(channel);
                        }
                    }
                }
            }
        }
        for (idx, channels) in per_cell.into_iter().enumerate() {
            if self.registered[idx] != channels {
                // MERGE, never replace: this cell's channel table may also hold
                // registrations from a DIFFERENT port's routing table, and
                // `install_tap` would erase them. See `PadLines::merge_tap` —
                // the STM32WBA52 splits SPI1_SCK (PB4) from SPI1_MOSI (PA15)
                // across two ports, so on that part the overwrite lost the data
                // channel every time.
                let stale = std::mem::replace(&mut self.registered[idx], channels.clone());
                // `invalidate_registrations` seeds `u32::MAX` sentinels to force
                // a reinstall; those were never real channels, so removing them
                // is a no-op on the cell.
                self.cells[idx].merge_tap(Some(tap.clone()), &stale, &channels);
            }
        }
    }

    /// Drop the push registrations THIS routing table installed — the disarm
    /// path.
    ///
    /// Only the ones it installed, which is the whole subtlety. A controller's
    /// wire reaches pads on SEVERAL ports — an STM32 USART3 can come out on
    /// PB10, PC10 or PD8 — so one [`PadLines`] cell is held by several
    /// `PadRoutes`, one per port. `Machine::logic_watch` then offers every
    /// peripheral its slice of the watch set, and a port with no watched pins
    /// lands here. Clearing the cell wholesale at that point wipes the
    /// registration a DIFFERENT port installed moments earlier, and since ports
    /// are visited in bus-index order, watching PB10 and then reaching gpioc
    /// silently disarmed the tap: the pad still read correctly, so the levels
    /// looked right and the trace was simply empty.
    ///
    /// A cell this table never registered channels on is therefore left alone.
    pub fn clear_taps(&mut self) {
        for (idx, cell) in self.cells.iter().enumerate() {
            if self.registered[idx]
                .iter()
                .all(|channels| channels.is_empty())
            {
                continue;
            }
            cell.clear_tap();
            self.registered[idx] = vec![Vec::new(); cell.names().len()];
        }
    }

    /// Force the next [`sync_taps`](Self::sync_taps) to install unconditionally,
    /// for use when arming a fresh watch.
    pub fn invalidate_registrations(&mut self) {
        for slot in &mut self.registered {
            for line in slot.iter_mut() {
                *line = vec![u32::MAX];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I2C: &[&str] = &["SCL", "SDA"];
    const SCL: usize = 0;
    const SDA: usize = 1;

    fn wire() -> Arc<PadLines> {
        Arc::new(PadLines::new(I2C, &[true, true]))
    }

    #[test]
    fn a_pad_reads_the_wire_only_while_it_selects_that_signal() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        lines.set(&[false, true]);

        // Pad 6 currently selects AF4 → it reads the wire.
        assert_eq!(routes.level(6, |_| Some(4)), Some(false));
        // Same pad handed back to plain GPIO → the caller falls back.
        assert_eq!(routes.level(6, |_| None), None);
        // Selecting a DIFFERENT function must not read this wire.
        assert_eq!(routes.level(6, |_| Some(7)), None);
        // An unbound pad is never routed.
        assert_eq!(routes.level(9, |_| Some(4)), None);
    }

    #[test]
    fn a_fixed_mapping_route_matches_any_selector() {
        // STM32 F1: the registers say "this pad is an AF output" but not which.
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, None, SDA, "I2C1_SDA");
        lines.set(&[true, false]);
        assert_eq!(routes.level(6, |_| Some(0)), Some(false));
        assert_eq!(routes.level(6, |_| None), None, "still not a plain GPIO");
    }

    #[test]
    fn one_wire_serves_several_pads_without_duplicating_the_cell() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        routes.bind(&lines, 7, Some(4), SDA, "I2C1_SDA");
        routes.bind(&lines, 8, Some(4), SCL, "I2C1_SCL");
        assert_eq!(routes.cells.len(), 1, "deduplicated by identity");
        lines.set(&[false, true]);
        assert_eq!(routes.level(6, |_| Some(4)), Some(false));
        assert_eq!(routes.level(7, |_| Some(4)), Some(true));
    }

    #[test]
    fn the_signal_name_follows_the_live_routing() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 7, Some(4), SDA, "I2C1_SDA");
        assert_eq!(routes.func(7, |_| Some(4)), Some("I2C1_SDA"));
        assert_eq!(routes.func(7, |_| Some(5)), None);
    }

    #[test]
    fn watched_pads_register_with_the_wire_that_drives_them() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        routes.bind(&lines, 7, Some(4), SDA, "I2C1_SDA");

        let tap = LogicTap::new();
        routes.sync_taps(&tap, &[(6, 0), (7, 1)], |_| Some(4));

        lines.set(&[false, false]);
        assert_eq!(
            tap.take_events()
                .iter()
                .map(|e| (e.ch, e.value))
                .collect::<Vec<_>>(),
            vec![(0, false), (1, false)],
            "each watched pad reports from its own line",
        );
    }

    #[test]
    fn a_pad_taken_back_for_gpio_stops_reporting_from_the_wire() {
        // The regression this guards: re-routing must follow immediately, or a
        // plain GPIO pin silently keeps showing bus traffic.
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        let tap = LogicTap::new();
        routes.sync_taps(&tap, &[(6, 0)], |_| Some(4));
        lines.set(&[false, true]);
        assert_eq!(tap.take_events().len(), 1);

        // Firmware hands the pad back to GPIO.
        routes.sync_taps(&tap, &[(6, 0)], |_| None);
        lines.set(&[true, true]);
        assert!(
            tap.take_events().is_empty(),
            "the wire still moves, but this pad is no longer listening",
        );
    }

    #[test]
    fn re_syncing_an_unchanged_routing_does_not_reinstall() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        let tap = LogicTap::new();
        routes.sync_taps(&tap, &[(6, 0)], |_| Some(4));
        let before = routes.registered.clone();
        routes.sync_taps(&tap, &[(6, 0)], |_| Some(4));
        assert_eq!(routes.registered, before, "no churn on a quiet write");
    }

    #[test]
    fn clearing_taps_stops_every_wire() {
        let lines = wire();
        let mut routes = PadRoutes::new();
        routes.bind(&lines, 6, Some(4), SCL, "I2C1_SCL");
        let tap = LogicTap::new();
        routes.sync_taps(&tap, &[(6, 0)], |_| Some(4));
        routes.clear_taps();
        lines.set(&[false, false]);
        assert!(tap.take_events().is_empty());
    }

    #[test]
    fn a_span_route_picks_the_line_its_selector_names() {
        const PWM: &[&str] = &["CH0", "CH1", "CH2"];
        let lines = Arc::new(PadLines::new(PWM, &[false, false, false]));
        let mut routes = PadRoutes::new();
        routes.bind_span(&lines, 5, 71, 0, PWM);
        lines.set(&[false, true, false]);
        assert_eq!(routes.level(5, |_| Some(72)), Some(true), "71 + 1 -> CH1");
        assert_eq!(routes.level(5, |_| Some(71)), Some(false));
        assert_eq!(routes.func(5, |_| Some(73)), Some("CH2"));
        assert_eq!(routes.level(5, |_| Some(74)), None, "past the span");
        assert_eq!(routes.level(5, |_| Some(70)), None, "before the span");
        assert_eq!(routes.bound_functions(), vec!["CH0", "CH1", "CH2"]);
        let listeners = routes.pin_routes(5);
        for offset in 0..3 {
            let index = PinRoute::select(&listeners, Some(71 + offset)).unwrap();
            assert_eq!(listeners[index].line, offset as usize);
        }
        assert_eq!(PinRoute::select(&listeners, Some(70)), None);
        assert_eq!(PinRoute::select(&listeners, Some(74)), None);
        let tap = LogicTap::new();
        routes.sync_taps(&tap, &[(5, 9)], |_| Some(73));
        lines.set(&[true, true, true]);
        assert_eq!(
            tap.take_events()
                .iter()
                .map(|e| (e.ch, e.value))
                .collect::<Vec<_>>(),
            vec![(9, true)],
            "the watched pad reports from CH2 only",
        );
    }

    #[test]
    fn a_span_at_max_selector_keeps_its_valid_prefix_without_wrapping_listeners() {
        const PWM: &[&str] = &["CH0", "CH1"];
        let lines = Arc::new(PadLines::new(PWM, &[true, false]));
        let mut routes = PadRoutes::new();
        routes.bind_span(&lines, 5, u32::MAX, 0, PWM);
        assert_eq!(routes.level(5, |_| Some(u32::MAX)), Some(true));
        assert_eq!(routes.func(5, |_| Some(u32::MAX)), Some("CH0"));
        let listeners = routes.pin_routes(5);
        assert_eq!(listeners.len(), 1);
        let index = PinRoute::select(&listeners, Some(u32::MAX)).unwrap();
        assert_eq!(listeners[index].line, 0);
        for selector in [0, u32::MAX - 1] {
            assert_eq!(routes.level(5, |_| Some(selector)), None);
            assert_eq!(PinRoute::select(&listeners, Some(selector)), None);
        }
    }

    #[test]
    fn an_empty_routing_table_answers_nothing_and_costs_nothing() {
        let routes = PadRoutes::new();
        assert!(routes.is_empty());
        assert_eq!(routes.level(0, |_| Some(1)), None);
        assert_eq!(routes.func(0, |_| Some(1)), None);
    }

    #[test]
    fn route_selection_samples_only_a_matching_conditional_wire() {
        let routes = [
            (false, Some(4), true),
            (true, Some(7), true),
            (true, Some(4), false),
            (true, Some(4), true),
        ];
        let reads = std::cell::Cell::new(0);
        let hit = select_route(
            &routes,
            4,
            |r| *r,
            |_| {
                reads.set(reads.get() + 1);
                true
            },
        );
        assert!(std::ptr::eq(hit.unwrap(), &routes[3]));
        assert_eq!(reads.get(), 1, "irrelevant routes must not sample a wire");
        let hit = select_route(&routes, 4, |r| *r, |_| false);
        assert!(std::ptr::eq(hit.unwrap(), &routes[2]));
    }

    #[test]
    fn indexed_routes_keep_conditional_priority_and_live_wire_changes() {
        use crate::peripherals::pad_lines::LineDrive;
        let ordinary = wire();
        let conditional = wire();
        let mut routes = PadRoutes::new();
        for pin in (0..40).rev() {
            routes.bind(&ordinary, pin, Some(4), SCL, "ordinary");
        }
        routes.bind_when_driving(&conditional, 27, Some(4), SDA, "conditional");
        ordinary.set(&[false, true]);
        conditional.drive_line(SDA, LineDrive::PushPull, true);
        assert_eq!(routes.level(27, |_| Some(4)), Some(true));
        assert_eq!(routes.level(26, |_| Some(4)), Some(false));
        assert_eq!(routes.level(27, |_| Some(7)), None);
        conditional.drive_line(SDA, LineDrive::Input, true);
        assert_eq!(routes.level(27, |_| Some(4)), Some(false));
        let owned = routes.pin_routes(27);
        assert_eq!(PinRoute::select(&owned, Some(4)), Some(0));
        conditional.drive_line(SDA, LineDrive::PushPull, true);
        assert_eq!(PinRoute::select(&owned, Some(4)), Some(1));
        assert_eq!(routes.func(27, |_| Some(4)), Some("conditional"));
        assert_eq!(routes.level(255, |_| Some(4)), None);
    }
}
