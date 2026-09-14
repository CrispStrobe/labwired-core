// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
//
// This software is released under the MIT License.
// See the LICENSE file in the project root for full license information.

//! Errors a [`crate::session::Session`] reports.

/// Why a session operation did not complete.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The engine cannot do this here. The one error for an honest gap: never
    /// a silent no-op. `tracker` names where the gap is tracked.
    #[error("not supported: {what} (tracked in {tracker})")]
    NotSupported {
        what: &'static str,
        tracker: &'static str,
    },
    /// `expect` spent its whole virtual-time budget without a match, or the
    /// machine stopped (`halted`) before one could appear.
    #[error(
        "expect timed out after {virtual_seconds}s waiting for /{pattern}/{}; last output: {tail:?}",
        if *halted { " (machine halted)" } else { "" }
    )]
    ExpectTimeout {
        pattern: String,
        virtual_seconds: f64,
        /// The last 200 characters of the UART transcript.
        tail: String,
        /// The machine stopped executing before the budget ran out.
        halted: bool,
    },
    #[error("unknown uart {0:?}")]
    UnknownUart(String),
    #[error(transparent)]
    Sim(#[from] crate::SimulationError),
    #[error(transparent)]
    Input(#[from] crate::sim_input::SimInputError),
    #[error("{0}")]
    Other(String),
}

pub type SessionResult<T> = Result<T, SessionError>;
