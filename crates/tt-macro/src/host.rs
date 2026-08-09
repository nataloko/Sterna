//! [`tt_ttl::ScriptHost`], answered by a real terminal.
//!
//! Every method here is one of three shapes and the shape is the whole design:
//!
//! - **The ring.** `read_byte` and `flush_recv` go to [`MacroLink`] and take no
//!   lock the frontend cares about, because a `wait` asks thousands of times a
//!   second.
//! - **A job.** Everything the session or the window can answer is a closure
//!   posted to the frontend's thread — see [`crate::channel`] for why it is a
//!   closure rather than a request enum, and why it is a thread rather than a
//!   mutex.
//! - **Refused.** What no part of this port can do yet, left as the trait's own
//!   default so a macro is told "Unknown command" instead of being lied to.
//!
//! The third list is short and it is written down at the bottom of this file
//! rather than left to be discovered by a script.

use std::time::{Duration, Instant};

use tt_session::{LogMode, LogOptions, MacroLink, Session, Timestamp};
use tt_ttl::host::{
    BeepSound, ClearScreen, DialogEnd, DialogPos, ErrorReport, ListBoxOpts, LogClock, LogInfo,
    LogOpen, MacroWindow, ScriptHost, SendMode, ShowWindow, WindowGeometry,
};
use tt_ttl::TtlError;

use crate::channel::MacroSender;
use crate::ui::MacroError;

/// How often a blocked `read_byte` looks again.
///
/// Upstream has no equivalent: its macro is driven by a timer on the window's
/// message loop, so the granularity is the timer's. This is a poll because the
/// ring is filled by a different thread and a condvar would have to be taken by
/// the parser on the hot path — one lock per byte of output, to save a wait
/// nobody is watching. Two milliseconds is under a character time at 4800 baud
/// and invisible against any dialog or prompt.
const POLL: Duration = Duration::from_millis(2);

/// A macro's view of a running session.
pub struct SessionHost {
    tx: MacroSender,
    link: MacroLink,
    /// Whether this macro still has the terminal — upstream's `Linked`, which
    /// `unlink` and `closett` clear and which every communication command
    /// tests before it does anything.
    ///
    /// Kept here rather than asked for: it is a property of the *macro*, not of
    /// the session, and a round trip per `send` to learn something this side
    /// already knows would be a job per line.
    linked: bool,
    /// `ts.FileDir` — where a relative filename in a transfer or a log lands.
    /// Distinct from the macro's own directory, which the interpreter owns.
    transfer_dir: Option<Vec<u8>>,
}

impl SessionHost {
    /// `link` must be the one [`Session::link_macro`] returned, and `tx` the
    /// sender whose receiver that same frontend is servicing.
    pub fn new(tx: MacroSender, link: MacroLink) -> SessionHost {
        SessionHost {
            tx,
            link,
            linked: true,
            transfer_dir: None,
        }
    }

    /// Run something against the session, or fail the command if the frontend
    /// has gone.
    fn ask<T, F>(&self, f: F) -> Result<T, TtlError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        self.tx.call(move |s, _| f(s)).ok_or(TtlError::CantCall)
    }

    /// The same, for the half of the world that is a window.
    fn ask_ui<T, F>(&self, f: F) -> Result<T, TtlError>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn crate::ui::MacroUi) -> T + Send + 'static,
    {
        self.tx.call(move |_, ui| f(ui)).ok_or(TtlError::CantCall)
    }
}

impl ScriptHost for SessionHost {
    // ---- the macro itself ----

    fn error(&mut self, report: &ErrorReport<'_>) -> bool {
        let owned = MacroError::from_report(report);
        // A frontend that has gone cannot be told, and a macro with nobody
        // watching should stop rather than run on past a syntax error.
        self.ask_ui(move |ui| ui.error(&owned)).unwrap_or(true)
    }

    fn read_macro(&mut self, path: &[u8]) -> Result<Vec<u8>, TtlError> {
        // `include` is the macro's own filesystem, not the terminal's — the
        // interpreter has already resolved the path against the running
        // macro's directory, and reading it here costs no round trip.
        let name = String::from_utf8(path.to_vec()).map_err(|_| TtlError::CantOpen)?;
        std::fs::read(name).map_err(|_| TtlError::CantOpen)
    }

    fn set_exit_code(&mut self, code: i32) {
        let _ = self.ask_ui(move |ui| ui.set_exit_code(code));
    }

    fn cancelled(&mut self) -> bool {
        self.tx.cancelled()
    }

    fn sleep(&mut self, d: Duration) {
        // Broken into polls so that End stops a `pause 3600` in two
        // milliseconds rather than in an hour. Upstream's `pause` is
        // interruptible for the same reason and its `mpause` is not; both are
        // here, and making both interruptible is the kinder of the two
        // behaviours rather than the more faithful one.
        let deadline = Instant::now() + d;
        while Instant::now() < deadline {
            if self.tx.cancelled() {
                return;
            }
            std::thread::sleep(POLL.min(deadline - Instant::now()));
        }
    }

    // ---- the line ----

    fn linked(&mut self) -> bool {
        self.linked
    }

    fn com_ready(&mut self) -> bool {
        self.linked && self.ask(|s| s.is_connected()).unwrap_or(false)
    }

    fn unlink(&mut self) {
        self.linked = false;
        let _ = self.tx.call(|s, _| s.unlink_macro());
    }

    fn send(&mut self, bytes: &[u8], mode: SendMode) -> Result<(), TtlError> {
        // The mode is not decoration: the two paths differ in whether a CR is
        // **expanded**. Upstream's text path runs the bytes through
        // `OutControl` (`ttcmn.c:800`), where a CR becomes CR, CRLF, LF or
        // CR NUL depending on `cv->CRSend` and on whether telnet is in binary
        // mode; the binary path writes what it was given. So `sendln 'go'` puts
        // `go\r` on the wire with the default setting and `go\r\n` with
        // `CRSend=CRLF`, and a `sendbinary` of the same string always puts
        // `go\r`. Choosing one path for both would break whichever half of the
        // world the choice went against.
        let text = match mode {
            SendMode::Binary => false,
            // `SendStringU8` converts first and **returns without sending** if
            // the conversion fails, so a `sendtext` of bytes that are not
            // UTF-8 puts nothing on the wire at all. That is not an error a
            // macro can see.
            SendMode::Text => match std::str::from_utf8(bytes) {
                Ok(_) => true,
                Err(_) => return Ok(()),
            },
            SendMode::Compat => tt_ttl::host::looks_like_text(bytes),
        };
        if text {
            let s = String::from_utf8_lossy(bytes).into_owned();
            self.ask(move |sess| sess.send_text(&s))?
        } else {
            let bytes = bytes.to_vec();
            self.ask(move |sess| sess.send_bytes(&bytes))?
        }
        .map_err(|_| TtlError::CantCall)
    }

    fn read_byte(&mut self, timeout: Option<Duration>) -> Option<u8> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            if let Some(b) = self.link.pop() {
                return Some(b);
            }
            if self.tx.cancelled() {
                return None;
            }
            if let Some(d) = deadline {
                if Instant::now() >= d {
                    return None;
                }
            }
            // A macro with no timeout waiting on a connection that has gone
            // would otherwise wait for ever. Upstream would too — its `wait`
            // with `timeout = 0` never gives up — but upstream's user can
            // press End, and so can this one; the connection check is here so
            // that they do not have to.
            if !self.linked {
                return None;
            }
            std::thread::sleep(POLL);
        }
    }

    fn flush_recv(&mut self) {
        self.link.clear();
    }

    fn disp_str(&mut self, s: &[u8]) -> Result<(), TtlError> {
        let bytes = s.to_vec();
        // Into the terminal, not out of the connection — `Session::feed` is
        // exactly that, and it goes through the macro tap on the way, so a
        // script can `wait` for what it printed itself. Upstream's does too.
        self.ask(move |s| s.feed(&bytes))
    }

    fn disconnect(&mut self, _confirm: bool) -> Result<(), TtlError> {
        // The confirmation is upstream's dialog and is deliberately not asked
        // for: it exists so a person does not close a session by accident, and
        // there is no person in a `disconnect` a script ran. A frontend that
        // wants to ask has `MacroUi::yes_no_box` and the argument.
        self.ask(|s| s.disconnect())
    }

    fn close_terminal(&mut self) -> Result<(), TtlError> {
        self.ask(|s| s.disconnect())?;
        self.unlink();
        Ok(())
    }

    fn send_break(&mut self) -> Result<(), TtlError> {
        // 250 ms, which is what upstream's serial break holds for
        // (`commlib.c`'s `SetCommBreak`/`Sleep`/`ClearCommBreak`).
        self.ask(|s| s.send_break(Duration::from_millis(250)))?
            .map_err(|_| TtlError::CantCall)
    }

    fn hostname(&mut self) -> Result<Vec<u8>, TtlError> {
        // `ttdde.c:1125` writes nothing for a terminal with nothing open, so a
        // linked terminal with no connection answers the empty string rather
        // than failing.
        Ok(self
            .ask(|s| s.describe())?
            .map(String::into_bytes)
            .unwrap_or_default())
    }

    fn set_local_echo(&mut self, on: bool) -> Result<(), TtlError> {
        self.ask(move |s| {
            let _ = s.set_setting("LocalEcho", if on { "on" } else { "off" });
        })
    }

    fn clear_screen(&mut self, what: ClearScreen) -> Result<(), TtlError> {
        // The TEK arm is not a refusal: upstream has a second window and this
        // port has not, so clearing it is a no-op rather than an error, which
        // is what a Tera Term with the TEK window closed does as well.
        let seq: &[u8] = match what {
            ClearScreen::Screen => b"\x1b[2J\x1b[H",
            ClearScreen::ScreenAndBuffer => b"\x1b[3J\x1b[2J\x1b[H",
            ClearScreen::TekScreen => return Ok(()),
        };
        self.ask(move |s| s.feed(seq))
    }

    fn set_title(&mut self, title: &[u8]) -> Result<(), TtlError> {
        let text = String::from_utf8_lossy(title).into_owned();
        // Through the parser, because that is where a title lives and where
        // the frontend already listens for one. Upstream sets `ts.Title` and
        // then repaints the caption, which is the same two steps in the other
        // order.
        let seq = format!("\x1b]2;{text}\x07").into_bytes();
        self.ask(move |s| s.feed(&seq))
    }

    fn title(&mut self) -> Result<Vec<u8>, TtlError> {
        Ok(self.ask(|s| s.vt().title().to_string())?.into_bytes())
    }

    // ---- the session log ----

    fn log_open(&mut self, req: &LogOpen<'_>) -> Result<bool, TtlError> {
        let path = resolve(&self.transfer_dir, req.path);
        let opts = LogOptions {
            mode: if req.binary {
                LogMode::Raw
            } else {
                LogMode::Text
            },
            timestamp: if req.timestamp {
                match req.timestamp_type {
                    LogClock::Local => Timestamp::Local,
                    LogClock::Utc => Timestamp::Utc,
                    // Upstream measures its two elapsed clocks from different
                    // events; this log has one, from the file being opened.
                    LogClock::ElapsedLog | LogClock::ElapsedConnection => Timestamp::Elapsed,
                }
            } else {
                Timestamp::None
            },
            append: req.append,
            ..LogOptions::default()
        };
        // `hide_dialog` is the log's progress window, which this port does not
        // have; `include_screen` is `LogAllBuffIncludedInFirst`, whose upstream
        // implementation is one of the reported bugs — it truncates every line
        // at its first wide character — so it waits for that report to be
        // answered rather than being reproduced. `plain_text` is the mode.
        let _ = (req.hide_dialog, req.include_screen, req.plain_text);
        self.ask(move |s| s.start_log(&path, opts).is_ok())
    }

    fn log_close(&mut self) -> Result<(), TtlError> {
        self.ask(|s| s.stop_log())
    }

    fn log_info(&mut self) -> Result<Option<LogInfo>, TtlError> {
        self.ask(|s| {
            s.log_path().map(|p| LogInfo {
                path: p.to_string_lossy().into_owned().into_bytes(),
                binary: false,
                append: false,
                plain_text: false,
                timestamp: false,
                hide_dialog: false,
            })
        })
    }

    // ---- the window and the user ----

    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let (t, h) = (text.to_vec(), title.to_vec());
        self.ask_ui(move |ui| ui.message_box(&t, &h))?
    }

    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let (t, h) = (text.to_vec(), title.to_vec());
        self.ask_ui(move |ui| ui.yes_no_box(&t, &h))?
    }

    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        let (t, h) = (text.to_vec(), title.to_vec());
        self.ask_ui(move |ui| ui.status_box(&t, &h))?
    }

    fn close_status_box(&mut self) -> Result<(), TtlError> {
        self.ask_ui(|ui| ui.close_status_box())?
    }

    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        self.ask_ui(|ui| ui.bringup_status_box())?
    }

    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[&[u8]],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let (t, h) = (text.to_vec(), title.to_vec());
        let items: Vec<Vec<u8>> = items.iter().map(|i| i.to_vec()).collect();
        let opts = *opts;
        self.ask_ui(move |ui| ui.list_box(&t, &h, &items, selected, &opts))?
    }

    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let (t, h, d) = (text.to_vec(), title.to_vec(), default.to_vec());
        self.ask_ui(move |ui| ui.input_box(&t, &h, &d, password))?
    }

    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        let (h, d) = (title.to_vec(), init_dir.to_vec());
        self.ask_ui(move |ui| ui.filename_box(&h, save, &d))?
    }

    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        let (h, d) = (title.to_vec(), init_dir.to_vec());
        self.ask_ui(move |ui| ui.dirname_box(&h, &d))?
    }

    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        let _ = self.ask_ui(move |ui| ui.set_dialog_pos(pos));
    }

    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        self.ask_ui(move |ui| ui.beep(sound))?
    }

    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        self.ask_ui(move |ui| ui.call_menu(id))?
    }

    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        self.ask_ui(move |ui| ui.show_window(which))?
    }

    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        self.ask_ui(move |ui| ui.show_macro_window(how))?
    }

    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        self.ask_ui(|ui| ui.terminal_geometry())?
    }

    fn enable_keyboard(&mut self, on: bool) -> Result<(), TtlError> {
        self.ask_ui(move |ui| ui.enable_keyboard(on))?
    }

    fn clipboard_text(&mut self) -> Option<Vec<u8>> {
        self.ask_ui(|ui| ui.clipboard_text()).ok().flatten()
    }

    fn set_clipboard_text(&mut self, text: &[u8]) -> bool {
        let t = text.to_vec();
        self.ask_ui(move |ui| ui.set_clipboard_text(&t))
            .unwrap_or(false)
    }

    // ---- the transfer directory ----

    fn set_transfer_dir(&mut self, path: &[u8]) -> Result<(), TtlError> {
        let dir = resolve(&self.transfer_dir, path);
        if !dir.is_dir() {
            return Err(TtlError::CantCall);
        }
        self.transfer_dir = Some(dir.into_os_string().into_encoded_bytes());
        Ok(())
    }

    fn random_u32(&mut self) -> u32 {
        // The interpreter's rejection loop makes this uniform; all it needs is
        // entropy, and the same crate `setpassword2` already uses supplies it.
        let mut b = [0u8; 4];
        match getrandom(&mut b) {
            true => u32::from_ne_bytes(b),
            false => 0,
        }
    }

    // Everything not written above keeps the trait's own refusing default, and
    // each is refused for a reason rather than for want of typing:
    //
    // `connect`/`cygconnect` — the argument is a Tera Term command line, and
    //   parsing it is `ttermpro`'s own front door rather than the macro
    //   language's. It arrives with the CLI entry point, which needs the same
    //   parser.
    // `setdtr`, `setrts`, `setbaud`, `setflowctrl`, `getmodemstatus`,
    //   `setserialdelay*` — the control lines are on `SerialConn` and not on
    //   `Transport`, so a `Session` cannot reach them through the box it holds.
    //   Refusing is *not* what a macro sees: the interpreter turns these into
    //   silent successes because upstream's terminal answers
    //   `DDE_FNOTPROCESSED` for a connection that is not serial, which is the
    //   right answer for every transport this port has but one.
    // `transfer` — sixteen commands over `tt-xfer`, which the session can
    //   already run; what is missing is that `Session::send_files` returns
    //   immediately and this has to block until the protocol reports, so it
    //   needs a completion the channel can wait on rather than a job.
    // `scp` — an SSH channel `tt-conn` does not open yet.
    // `send_broadcast`, `send_multicast`, `set_multicast_name`, `wait_for_all`
    //   — all four are about the *other* sessions, and this crate is handed
    //   one. They belong to whatever owns the tab bar.
    // `send_key_code` — needs `KEYBOARD.CNF`, which is Stage 2's other half.
    // `load_key_map`, `restore_setup` — the same file, and `TERATERM.INI`.
    // `set_debug_mode` — `ts.DebugMode` has no equivalent in `tt-vt` yet.
    // `log_pause`, `log_write`, `log_rotate` — `SessionLog` has no pause and no
    //   out-of-band write; small, and they want a test each.
    // `local_ip_addresses` — enumerating interfaces needs more than `std`.
}

/// Resolve a filename the way a transfer or a log does: against `changedir`'s
/// directory when one has been set, and against the process's otherwise.
fn resolve(dir: &Option<Vec<u8>>, path: &[u8]) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(String::from_utf8_lossy(path).into_owned());
    match dir {
        Some(d) if p.is_relative() => {
            std::path::PathBuf::from(String::from_utf8_lossy(d).into_owned()).join(p)
        }
        _ => p,
    }
}

/// Four bytes of entropy, or nothing.
///
/// `/dev/urandom` rather than a dependency: this crate has three and all three
/// are the project's own. `random` is not a security primitive — `setpassword2`
/// is, and it uses `getrandom` in the crate that needs it.
fn getrandom(buf: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .is_ok()
}
