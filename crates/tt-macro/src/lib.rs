//! tt-macro — a TTL macro, running against a terminal.
//!
//! `tt-ttl` is the language with nothing attached and `tt-session` is a
//! terminal with nothing scripting it. This is the join, and upstream draws the
//! same line: `ttpmacro.exe` and `ttermpro.exe` are two programs, and
//! everything between them is DDE.
//!
//! The whole of the design is which thread things happen on.
//!
//! ```text
//!   frontend thread                        macro thread
//!   ───────────────                        ────────────
//!   Session  ── pump ──► Vt ── tap ──►  MacroLink ──► read_byte
//!      ▲                                                  │
//!      │                                                  ▼
//!   MacroReceiver::service ◄──── job ────────────── SessionHost
//!      │                                                  ▲
//!      └────────────── answer ────────────────────────────┘
//! ```
//!
//! A macro blocks; the window does not. Both of those matter and neither is
//! free: upstream *cannot* block, because its macro shares a thread with the
//! window, so `wait` parks itself in a state machine and the message loop
//! drives it back to life. Here `wait` is a function that returns when it is
//! done — which is only possible because the terminal is somewhere else, and
//! only safe because nothing is borrowed across the boundary.
//!
//! ```no_run
//! use std::time::Duration;
//! use tt_macro::{channel, NullUi, SessionHost};
//! use tt_session::Session;
//! use tt_ttl::Interp;
//!
//! let mut session = Session::new(Default::default());
//! let link = session.link_macro();
//! let (tx, rx) = channel()?;
//!
//! let script = std::thread::spawn(move || {
//!     let mut host = SessionHost::new(tx, link);
//!     let mut it = Interp::new("login.ttl", b"sendln 'who'".to_vec(), &mut host);
//!     it.run(&mut host);
//! });
//!
//! // The frontend's loop, which is a Qt event loop in the shell: wait on
//! // `session.poll_fd()` and `rx.poll_fd()`, then pump and service.
//! while !script.is_finished() {
//!     session.pump(Duration::from_millis(10))?;
//!     rx.service(&mut session, &mut NullUi);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod channel;
pub mod host;
pub mod ui;

pub use channel::{channel, Job, MacroReceiver, MacroSender};
pub use host::SessionHost;
pub use ui::{MacroError, MacroUi, NullUi};
