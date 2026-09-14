// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Scriptable single-machine session over the simulator core.
//!
//! A [`Session`] owns one machine built by
//! [`crate::system::builder::build_machine`] and advances it only when asked.
//! Nothing runs between calls, and every duration is **virtual**: an `expect`
//! timeout is simulated seconds at the board's clock, never host wall time, so
//! a timing claim is a claim about the firmware and not about the machine the
//! test happens to run on.

pub mod error;
pub mod machine;
pub mod uart;

pub use error::{SessionError, SessionResult};

use crate::machine::{AdvanceRequest, AdvanceStop};
use crate::system::builder::{build_machine, BuildRequest};
use std::time::Duration;

/// How a session is opened.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// Clock used to convert cycles to and from seconds. `None` (the default)
    /// takes the board's clock: `SystemManifest::cpu_hz`, else
    /// `ChipDescriptor::cpu_hz` — the same resolution the bus uses, so a
    /// session never disagrees with the peripherals about how long a second is.
    pub cpu_hz: Option<u64>,
    /// Fuel per advance batch; bounds how far past a UART match `expect` may
    /// overshoot.
    pub batch_fuel: u64,
    /// Echo the console to the host's stdout.
    pub echo_uart_stdout: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            cpu_hz: None,
            batch_fuel: 20_000,
            echo_uart_stdout: false,
        }
    }
}

/// Why a run returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The requested time elapsed.
    Reached,
    /// The machine stopped making progress (halt, no forward progress, or the
    /// firmware ended its own run).
    Halted,
    /// Execution reached a breakpoint at this PC.
    Breakpoint(u32),
    Error,
}

/// A successful `expect`.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// The matched text (lossy UTF-8).
    pub text: String,
    /// Capture groups 1.., `None` for a group that did not participate.
    pub captures: Vec<Option<String>>,
    /// Virtual time at which the match was observed.
    pub at: Duration,
}

/// One machine, driven step by step from a script.
pub struct Session {
    machine: Box<dyn machine::SessionMachine>,
    uart: uart::UartStream,
    cpu_hz: u64,
    opts: OpenOptions,
}

impl Session {
    /// Build the machine through [`build_machine`] and wrap it.
    pub fn open(mut req: BuildRequest<'_>, opts: OpenOptions) -> anyhow::Result<Session> {
        let cpu_hz = opts
            .cpu_hz
            .unwrap_or_else(|| req.system.cpu_hz.unwrap_or(req.chip.cpu_hz));
        if cpu_hz == 0 {
            anyhow::bail!(
                "chip '{}' declares no cpu_hz and none was passed in OpenOptions; a session \
                 cannot convert cycles to time without a clock",
                req.chip.name
            );
        }
        if opts.batch_fuel == 0 {
            anyhow::bail!("OpenOptions::batch_fuel must be greater than zero");
        }
        req.options.echo_uart_stdout |= opts.echo_uart_stdout;
        let built = build_machine(req)?;
        Ok(Session {
            machine: built.machine,
            uart: uart::UartStream::new(built.uart),
            cpu_hz,
            opts,
        })
    }

    /// Simulated machine cycles since the machine was built.
    pub fn cycles(&self) -> u64 {
        self.machine.cycles()
    }

    /// The clock this session converts cycles with, in Hz.
    pub fn cpu_hz(&self) -> u64 {
        self.cpu_hz
    }

    /// Virtual time: cycles / cpu_hz.
    pub fn time(&self) -> Duration {
        let nanos = u128::from(self.cycles()) * 1_000_000_000 / u128::from(self.cpu_hz);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    /// Cycles covering `d` at this session's clock, rounded up, at least one.
    fn cycles_for(&self, d: Duration) -> u64 {
        let c = (d.as_nanos() * u128::from(self.cpu_hz)).div_ceil(1_000_000_000);
        u64::try_from(c).unwrap_or(u64::MAX).max(1)
    }

    /// Advance by `virtual_time` of simulated time.
    pub fn run_for(&mut self, virtual_time: Duration) -> SessionResult<StopReason> {
        let cycles = self.cycles_for(virtual_time);
        self.run_cycles(cycles)
    }

    /// Advance by exactly `cycles` machine cycles, unless the machine stops
    /// first.
    pub fn run_cycles(&mut self, cycles: u64) -> SessionResult<StopReason> {
        let target = self.machine.cycles().saturating_add(cycles);
        while self.machine.cycles() < target {
            let remaining = target - self.machine.cycles();
            let req = AdvanceRequest::run(Some(self.opts.batch_fuel)).with_cycle_limit(remaining);
            match self.machine.advance(req) {
                Ok(report) => match report.stop {
                    AdvanceStop::Breakpoint(pc) => return Ok(StopReason::Breakpoint(pc)),
                    AdvanceStop::NoProgress | AdvanceStop::FirmwareExit { .. } => {
                        return Ok(StopReason::Halted)
                    }
                    _ if report.primary_steps == 0 && report.idle_cycles == 0 => {
                        return Ok(StopReason::Halted)
                    }
                    _ => {}
                },
                Err(crate::SimulationError::Halt) => return Ok(StopReason::Halted),
                Err(crate::SimulationError::BreakpointHit(pc)) => {
                    return Ok(StopReason::Breakpoint(pc))
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(StopReason::Reached)
    }

    /// Wait, in virtual time, for `pattern` (a regex) on the console.
    ///
    /// Unread bytes are matched first, so output a previous call ran past is
    /// never lost. On a match the cursor moves to the end of it. The budget is
    /// `timeout` of simulated time; if it runs out, or the machine stops before
    /// a match, the result is [`SessionError::ExpectTimeout`].
    pub fn expect(&mut self, pattern: &str, timeout: Duration) -> SessionResult<Match> {
        let re = regex::bytes::Regex::new(pattern)
            .map_err(|e| SessionError::Other(format!("invalid expect pattern /{pattern}/: {e}")))?;
        let deadline = self
            .machine
            .cycles()
            .saturating_add(self.cycles_for(timeout));
        loop {
            if let Some(m) = self.take_match(&re) {
                return Ok(m);
            }
            let now = self.machine.cycles();
            if now >= deadline {
                return Err(self.expect_timeout(pattern, timeout, false));
            }
            let step = (deadline - now).min(self.opts.batch_fuel);
            match self.run_cycles(step)? {
                StopReason::Reached => {}
                StopReason::Breakpoint(pc) => {
                    if let Some(m) = self.take_match(&re) {
                        return Ok(m);
                    }
                    return Err(crate::SimulationError::BreakpointHit(pc).into());
                }
                StopReason::Halted | StopReason::Error => {
                    if let Some(m) = self.take_match(&re) {
                        return Ok(m);
                    }
                    return Err(self.expect_timeout(pattern, timeout, true));
                }
            }
        }
    }

    fn take_match(&mut self, re: &regex::bytes::Regex) -> Option<Match> {
        let unread = self.uart.unread();
        let caps = re.captures(&unread)?;
        let whole = caps.get(0)?;
        let m = Match {
            text: String::from_utf8_lossy(whole.as_bytes()).into_owned(),
            captures: caps
                .iter()
                .skip(1)
                .map(|g| g.map(|g| String::from_utf8_lossy(g.as_bytes()).into_owned()))
                .collect(),
            at: self.time(),
        };
        self.uart.consume(whole.end());
        Some(m)
    }

    fn expect_timeout(&self, pattern: &str, timeout: Duration, halted: bool) -> SessionError {
        let transcript = self.uart_transcript();
        let skip = transcript.chars().count().saturating_sub(200);
        SessionError::ExpectTimeout {
            pattern: pattern.to_string(),
            virtual_seconds: timeout.as_secs_f64(),
            tail: transcript.chars().skip(skip).collect(),
            halted,
        }
    }

    /// Queue bytes on every UART RX feeder; firmware sees them as time
    /// advances.
    pub fn send(&mut self, bytes: &[u8]) {
        self.uart.send(bytes);
    }

    /// Drain console bytes not yet read by `read_uart` or matched by `expect`.
    pub fn read_uart(&mut self) -> Vec<u8> {
        let unread = self.uart.unread();
        self.uart.consume(unread.len());
        unread
    }

    /// Everything the console has printed since the session opened.
    pub fn uart_transcript(&self) -> String {
        String::from_utf8_lossy(&self.uart.all()).into_owned()
    }
}
