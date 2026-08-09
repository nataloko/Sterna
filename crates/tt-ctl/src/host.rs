//! What the socket needs from the frontend, and nothing else.
//!
//! Most of what a request asks for is the [`Session`](tt_session::Session)'s
//! own — `send`, `connect`, the grid — and the job carries one, so it is not
//! in this trait. What is left is the two things the *window* owns rather than
//! the terminal: the macro, whose handle lives in the frontend because that is
//! where `tt_macro_start` put it, and the window itself.
//!
//! Every method has a default that refuses. That is [`tt_ttl::ScriptHost`]'s
//! rule and it is here for the same reason: a frontend that has not
//! implemented something should say so, and a client should be able to tell
//! "this build cannot" ([`RpcError::REFUSED`](crate::RpcError::REFUSED)) from
//! "no such method" ([`RpcError::NO_METHOD`](crate::RpcError::NO_METHOD)). The
//! alternative — a default that silently succeeds — is a `macro.run` that
//! reports success and runs nothing.

use std::path::Path;

/// Where a macro has got to.
///
/// One struct rather than a `Result`, because the two fields are read at
/// different times: `running` is polled by a `macro.run` that was asked to
/// wait, and `exit` is `setexitcode`'s value, which is only meaningful once
/// `running` has gone false.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacroStatus {
    pub running: bool,
    /// The last `setexitcode`. Zero when a macro has never set one, which is
    /// also what a macro that ran cleanly leaves behind.
    pub exit: i32,
}

/// Why a macro did not start.
///
/// Two arms rather than one string, because a client tells them apart for a
/// living: "busy" is worth retrying in a second and "failed" never is. The
/// message is carried in both, since the frontend is what knows *which* file
/// would not open.
#[derive(Debug, Clone)]
pub enum RunError {
    /// One is already running. Upstream brings that macro's window to the
    /// front instead (`ttdde.c:1488`), which is not a thing a socket can do,
    /// so this is the honest answer.
    Busy(String),
    /// Anything else: no such file, a frontend that cannot run macros at all.
    Failed(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Busy(m) | RunError::Failed(m) => f.write_str(m),
        }
    }
}

/// The window, as a request sees it.
pub trait CtlHost {
    /// Start a macro.
    fn run_macro(&mut self, _path: &Path, _params: &[String]) -> Result<(), RunError> {
        Err(RunError::Failed("this frontend cannot run macros".into()))
    }

    fn macro_status(&mut self) -> MacroStatus {
        MacroStatus::default()
    }

    /// Open what a Tera Term command line describes — the same string the
    /// macro language's `connect` takes.
    ///
    /// It is the frontend's rather than the session's for one reason: an SSH
    /// target needs the host-key and password prompts, and those are dialogs.
    /// The shell answers this with the same `openTarget` its own command line
    /// and its own SSH dialog go through, so there is one path to a connection
    /// and not three.
    ///
    /// Answering means the attempt has *started*. Upstream's `connect` says no
    /// more than that either — a macro reads its result back out of `linked`
    /// and `com_ready` afterwards.
    fn connect(&mut self, _line: &[u8]) -> Result<(), String> {
        Err("this frontend cannot open a connection from a command line".into())
    }

    /// The End button, and what closing the terminal does. Not an error when
    /// nothing is running.
    fn stop_macro(&mut self) {}

    /// Close the window — `CmdCloseWin`, which the macro language spells
    /// `closett`. `false` is a frontend that will not.
    fn close_window(&mut self) -> bool {
        false
    }

    /// The window's title, when the frontend composes one that is not just the
    /// terminal's OSC title. `None` falls back to the session's.
    fn title(&mut self) -> Option<String> {
        None
    }
}

/// A host that refuses everything — the trait's own defaults, named.
///
/// What the C ABI falls through to when a frontend passes no callbacks, and
/// what the tests that are about the wire rather than about the window use.
pub struct NullHost;

impl CtlHost for NullHost {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_refuse_rather_than_lie() {
        let mut h = NullHost;
        assert!(h.run_macro(Path::new("x.ttl"), &[]).is_err());
        assert!(h.connect(b"myhost").is_err());
        assert!(!h.close_window());
        assert_eq!(h.macro_status(), MacroStatus::default());
    }
}
