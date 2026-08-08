//! What the interpreter asks of the world outside it.
//!
//! Upstream's macro engine is a second process, `ttpmacro.exe`, which reaches
//! the terminal over DDE: `ttpmacro/ttmdde.c` on one side, `teraterm/ttdde.c`
//! on the other, about 2,600 lines between them and a conversation to keep in
//! step. Here the engine is a library and the terminal is on the other side of
//! this trait, so the two are one process and there is nothing to keep in step.
//!
//! The trait is deliberately wide and shallow — one method per command that
//! needs the world, rather than a general "do a thing" channel — because that
//! is what makes a host that implements half of it useful, and what makes the
//! interpreter testable with no terminal at all.

use crate::error::TtlError;

/// What `DispErr` needs to draw its dialog.
#[derive(Debug, Clone, Copy)]
pub struct ErrorReport<'a> {
    pub error: TtlError,
    /// The whole source line, so the dialog can show it.
    pub line: &'a [u8],
    pub line_no: usize,
    /// Byte range within `line` to highlight. `DispErr` widens an empty range
    /// to the end of the line.
    pub start: usize,
    pub end: usize,
    pub file: &'a str,
}

/// The macro's view of the terminal, the filesystem and the user.
///
/// Every method has a default that refuses, so a host can implement the part it
/// has and the rest reports "Unknown command" rather than pretending to work.
pub trait ScriptHost {
    /// `DispErr` — show an error. Returning `true` ends the macro, which is
    /// what upstream's dialog does when the user chooses OK.
    fn error(&mut self, report: &ErrorReport<'_>) -> bool {
        let _ = report;
        true
    }

    /// `include` — read a macro file. The path is as written in the source,
    /// which the host resolves against the running macro's directory.
    fn read_macro(&mut self, path: &[u8]) -> Result<Vec<u8>, TtlError> {
        let _ = path;
        Err(TtlError::CantOpen)
    }

    /// `dispstr` — put bytes on the screen as if they had arrived from the far
    /// end. Not `send`: nothing goes out of the connection.
    fn disp_str(&mut self, s: &[u8]) -> Result<(), TtlError> {
        let _ = s;
        Err(TtlError::NotSupported)
    }

    /// `setexitcode` — the value the process exits with.
    fn set_exit_code(&mut self, code: i32) {
        let _ = code;
    }

    /// Whether the run has been cancelled from outside.
    ///
    /// The interpreter runs on its own thread and blocks in `wait` and `pause`,
    /// so this is the only way a stop request reaches a macro that is not
    /// executing lines. Checked once per line.
    fn cancelled(&mut self) -> bool {
        false
    }
}

/// A host that records what it was told and refuses everything else.
///
/// Used by the tests here, and useful to a caller that wants to run the pure
/// part of a macro — the arithmetic, the strings and the control flow — with
/// no terminal attached.
#[derive(Debug, Default)]
pub struct RecordingHost {
    pub output: Vec<u8>,
    pub errors: Vec<(TtlError, usize)>,
    pub exit_code: i32,
    /// Files `include` may find, by the path as written.
    pub files: std::collections::HashMap<Vec<u8>, Vec<u8>>,
    /// Whether an error ends the run. Upstream's dialog decides; here it is a
    /// field so a test can assert on what happens after one.
    pub stop_on_error: bool,
}

impl RecordingHost {
    pub fn new() -> Self {
        Self {
            stop_on_error: true,
            ..Default::default()
        }
    }
}

impl ScriptHost for RecordingHost {
    fn error(&mut self, report: &ErrorReport<'_>) -> bool {
        self.errors.push((report.error, report.line_no));
        self.stop_on_error
    }

    fn read_macro(&mut self, path: &[u8]) -> Result<Vec<u8>, TtlError> {
        self.files.get(path).cloned().ok_or(TtlError::CantOpen)
    }

    fn disp_str(&mut self, s: &[u8]) -> Result<(), TtlError> {
        self.output.extend_from_slice(s);
        Ok(())
    }

    fn set_exit_code(&mut self, code: i32) {
        self.exit_code = code;
    }
}
