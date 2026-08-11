//! `ttctl` — one window, reached from outside the process.
//!
//! This is what DDE was for and the last thing in `PLAN.md`'s Stage 2 that had
//! not been built. Upstream's macro is a second process and every command it
//! runs is a DDE transaction (`teraterm/ttdde.c` and `ttpmacro/ttmdde.c`,
//! 2,600 lines between them); this port runs the macro on a thread inside the
//! window, which deleted all of it — and with it, the only way anything
//! outside the process could ask the terminal for anything. A macro could be
//! started by a person clicking Control > Run macro, or by `/M=` on the
//! command line at startup, and that was the whole list.
//!
//! So the socket is not a replacement for DDE's *command set* — that set is
//! `ttddecmnd.h`, it is the macro language, and it is already implemented once
//! in [`tt_ttl`] and answered by `tt-macro`. It is a replacement for DDE's
//! *reachability*: a running window has a name, and something else on the
//! machine can find it and ask.
//!
//! ```text
//! $ printf '{"jsonrpc":"2.0","id":1,"method":"status"}\n' | nc -U ~/.../4321.sock
//! {"jsonrpc":"2.0","id":1,"result":{"connected":true,"cols":80,...}}
//! ```
//!
//! Four modules, and the split is the same one the crate's dependencies make:
//!
//! - [`proto`] is the wire — JSON-RPC 2.0, one object per line — and knows
//!   nothing about terminals.
//! - [`addr`] is where a socket lives and how a client finds one, which is the
//!   job DDE's topic names did.
//! - [`channel`] carries a request to the thread that owns the terminal, since
//!   the listener is not that thread and must never touch a [`Session`]
//!   directly.
//! - [`server`] is the accept loop, and [`dispatch`] is the method table.
//!
//! **What it can ask for is deliberately small.** Nine methods, not the
//! hundred-odd of `ttddecmnd.h`: start a macro, stop one, send text, connect,
//! disconnect, read the screen, report status, close the window, and say
//! hello. Everything else is a macro, because the macro language is the thing
//! that has been ported and tested against upstream's own scripts, and a
//! second command surface would be a second set of answers about what `send`
//! means. A client with something complicated to do writes a `.ttl` file and
//! calls `macro.run`, which is exactly what `ttpmacro.exe` did.
//!
//! [`Session`]: tt_session::Session

pub mod addr;
pub mod channel;
pub mod client;
pub mod dispatch;
pub mod host;
mod ipc;
pub mod proto;
pub mod server;

pub use channel::{channel, CtlReceiver, CtlSender};
pub use client::Client;
pub use host::{CtlHost, MacroStatus, NullHost, RunError};
pub use proto::{Request, Response, RpcError};
pub use server::{Listener, Server};

/// A path resolved where it was typed, in the spelling a person recognises.
///
/// Both command-line clients resolve a macro's path before sending it: a
/// relative name means what it says in the shell it was typed in, and the
/// window's working directory is wherever the window was started. The
/// resolution is `canonicalize`, and on Windows that answers with the `\\?\`
/// verbatim form — a spelling `ttpmacro.exe` never produced, which would then
/// be what the macro sees as its own name in `params[1]`, what a failure
/// message quotes back, and what the window's title bar shows. The prefix
/// exists to escape `MAX_PATH`; a path that needs it is a path that will not
/// survive being shown to anyone anyway.
///
/// Only the drive spelling is shortened. `\\?\UNC\server\share` loses its host
/// if the prefix goes, which is a working path turned into a broken one.
pub fn full_path(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let full = std::fs::canonicalize(path)?;
    let text = full.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
            return Ok(std::path::PathBuf::from(rest));
        }
    }
    Ok(full)
}
