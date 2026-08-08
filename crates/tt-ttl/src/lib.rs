//! TTL — Tera Term's macro language, as a library.
//!
//! Upstream is `ttpmacro.exe`, a second process that talks to the terminal over
//! DDE. Here the interpreter is a crate: the language is ported, and everything
//! it wants from the outside world goes through a trait the caller implements.
//! That deletes the DDE glue on both sides — `ttpmacro/ttmdde.c` and
//! `teraterm/ttdde.c`, about 2,600 lines — and with it the races that come from
//! two processes agreeing about a terminal by message passing.
//!
//! The port is faithful rather than tidy. TTL re-parses each line every time it
//! runs it, keeps no syntax tree, decides types at first assignment and has a
//! handful of behaviours that read as bugs until you meet the script that
//! depends on them. Those are reproduced and commented, not fixed; where a
//! choice was open, `PLAN.md` and the comments here say which way it went.

pub mod buffer;
pub mod error;
pub mod expr;
pub mod host;
pub mod interp;
pub mod lexer;
pub mod rsv;
pub mod strcmds;
pub mod vars;

pub use error::{TtlError, TtlResult};
pub use host::{ErrorReport, RecordingHost, ScriptHost};
pub use interp::Interp;
pub use lexer::Lexer;
pub use rsv::Rsv;
pub use vars::{Value, VarRef, VarType, Vars};
