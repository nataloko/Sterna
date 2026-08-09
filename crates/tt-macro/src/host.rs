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

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tt_conn::serial::FlowControl as ConnFlow;
use tt_session::open::{Startup, Target};
use tt_session::{LogMode, LogOptions, MacroLink, Session, Timestamp, TransferReply};
use tt_ttl::host::{
    BeepSound, ClearScreen, DialogEnd, DialogPos, ErrorReport, FlowControl, ListBoxOpts, LogClock,
    LogInfo, LogOpen, LogRotate, MacroWindow, ModemLines, ScriptHost, SendMode, ShowWindow,
    WindowGeometry, Xfer, XmodemOpt,
};
use tt_ttl::TtlError;
// The sixteen transfer commands are the one place a macro reaches past the
// session into the protocols themselves, because the command *is* the protocol
// and its options.
use tt_xfer::{Direction, Job as XferJob, KermitMode, XmodemOpt as XferXmodemOpt, YmodemOpt};

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

/// How often something blocked on the *frontend* checks that it is still
/// there.
///
/// A `wait` needs none of this: it polls a ring the frontend fills, so a
/// frontend that has gone shows up as a line that has gone quiet. A transfer
/// is the other shape — the answer is posted from over there and nothing here
/// would ever notice its absence — so it asks, rarely enough to be free
/// against a transfer's own traffic and often enough that a closed window does
/// not leave a thread behind.
const PROBE: Duration = Duration::from_millis(250);

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

    /// And for the one command that needs both at once.
    fn ask_both<T, F>(&self, f: F) -> Result<T, TtlError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session, &mut dyn crate::ui::MacroUi) -> T + Send + 'static,
    {
        self.tx.call(f).ok_or(TtlError::CantCall)
    }

    /// `DispErr`'s dialog, for an error from any language.
    ///
    /// Public because TTL is not the only caller: `tt-lua` cannot report
    /// through [`ScriptHost::error`], whose report borrows a source line out
    /// of the interpreter's buffer and carries one of `ttmparse.h`'s numbered
    /// codes. **`true` stops the run.**
    ///
    /// A frontend that has gone cannot be told, and a script with nobody
    /// watching should stop rather than run on past an error.
    pub fn report(&mut self, err: &MacroError) -> bool {
        let owned = err.clone();
        self.ask_ui(move |ui| ui.error(&owned)).unwrap_or(true)
    }
}

impl ScriptHost for SessionHost {
    // ---- the macro itself ----

    fn error(&mut self, report: &ErrorReport<'_>) -> bool {
        self.report(&MacroError::from_report(report))
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

    fn connect(&mut self, cmdline: &[u8], cygwin: bool) -> Result<(), TtlError> {
        // "link to Tera Term", which upstream reaches by launching a second
        // `ttermpro.exe` and opening a DDE conversation with it. In-process
        // there is one terminal and the macro is already inside it, so linking
        // is taking the ring back — and the only way to be here without a link
        // is to have given it up with `unlink` or `closett`.
        if !self.linked {
            self.link = self.ask(|s| s.link_macro())?;
            self.linked = true;
        }

        let arg = cmdline.to_vec();
        // One job for the whole thing: the parse needs the session's settings
        // and its size, and the open needs the session. Splitting it would let
        // a resize land between the two.
        self.ask_both(move |s, ui| open(s, ui, &arg, cygwin))?;
        // Nothing is reported from here. `connect`'s `result` is read back off
        // `linked` and `com_ready` afterwards — upstream's own three-value
        // answer — so a connection that did not come up is a 1 rather than an
        // error, and this returns `Err` only when the frontend has gone.
        Ok(())
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

    // ---- the control lines ----
    //
    // Five jobs and no reports. Each is guarded on the session's side by
    // there being a serial port to guard — see `tt_session`'s `serial` module
    // — and what the guard rejects is not an error here for the reason the
    // trait's own defaults give: upstream answers `DDE_FNOTPROCESSED`, which
    // a macro reads as success. So a script that says `setdtr 0` over SSH
    // carries on, exactly as it does upstream.

    fn set_dtr(&mut self, on: bool) {
        let _ = self.ask(move |s| s.set_dtr(on));
    }

    fn set_rts(&mut self, on: bool) {
        let _ = self.ask(move |s| s.set_rts(on));
    }

    fn set_baud(&mut self, baud: u32) {
        let _ = self.ask(move |s| s.set_baud(baud));
    }

    fn set_flow_control(&mut self, flow: FlowControl) {
        // Two enumerations of the same four things, one in the language and
        // one in the port. Neither is the other's to import.
        let flow = match flow {
            FlowControl::None => ConnFlow::None,
            FlowControl::XonXoff => ConnFlow::XonXoff,
            FlowControl::RtsCts => ConnFlow::RtsCts,
            FlowControl::DsrDtr => ConnFlow::DsrDtr,
        };
        let _ = self.ask(move |s| s.set_flow_control(flow));
    }

    fn modem_lines(&mut self) -> Option<ModemLines> {
        // `None` twice over: the frontend is gone, or the connection has no
        // control lines to read. The macro cannot tell either from all four
        // being low, which is upstream's answer too — see
        // `Interp::cmd_get_modem_status` for why `result` stays 0 regardless.
        self.ask(|s| s.modem_lines()).ok().flatten().map(|m| {
            ModemLines {
                cts: m.cts,
                dsr: m.dsr,
                // `ri` and `cd` in the port, where they are pin names; `ring`
                // and `carrier` in the language, where the documentation
                // spells them out.
                ring: m.ri,
                carrier: m.cd,
            }
        })
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
        // The **dotted** name, which is what `Settings::set_str` matches on.
        // The INI key is `LocalEcho` and naming it that here is what this did
        // first: `set_str` has no arm for it, so it answered `false` and
        // `setecho` was a command that parsed, reported nothing and changed
        // nothing. Every write through this seam wants the schema's name.
        self.ask(move |s| {
            // The error is the transport's — a resize on a line that has gone
            // — and is not this command's to report. `Ok(false)` is the typo.
            let r = s.set_setting("terminal.local_echo", if on { "on" } else { "off" });
            debug_assert!(!matches!(r, Ok(false)), "no setting by that name");
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

    fn log_pause(&mut self, paused: bool) -> Result<(), TtlError> {
        self.ask(move |s| s.pause_log(paused))
    }

    fn log_write(&mut self, s: &[u8]) -> Result<(), TtlError> {
        // Lossy, like every other string that crosses into a terminal here:
        // the macro language's strings are bytes and the log's are characters.
        let text = String::from_utf8_lossy(s).into_owned();
        self.ask(move |sess| sess.write_log(&text))
    }

    fn log_rotate(&mut self, how: LogRotate) -> Result<(), TtlError> {
        // The interpreter has already enforced the floors — 128 bytes and one
        // generation — and multiplied out a `K` or `M` suffix.
        self.ask(move |s| match how {
            LogRotate::Size(n) => s.set_log_rotate_size(n.max(0) as u64),
            LogRotate::Keep(n) => s.set_log_rotate_keep(n.max(0) as u32),
            LogRotate::Halt => s.halt_log_rotate(),
        })
    }

    fn log_info(&mut self) -> Result<Option<LogInfo>, TtlError> {
        self.ask(|s| {
            let opts = s.log_options()?.clone();
            Some(LogInfo {
                path: s.log_path()?.to_string_lossy().into_owned().into_bytes(),
                binary: opts.mode == LogMode::Raw,
                append: opts.append,
                // One flag upstream, two here — `LogTypePlainText` is what a
                // text log *is* in this port, because there is no mode that
                // writes decoded text with the escape sequences left in. So it
                // answers what the log does rather than what `logopen` was
                // handed, which for `logopen` is the same thing.
                plain_text: opts.mode == LogMode::Text,
                timestamp: opts.timestamp != Timestamp::None,
                // The log's progress window, which this port does not have.
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

    // ---- the transfers ----

    /// All sixteen of them, of which fifteen are `tt-xfer`'s.
    ///
    /// **The command blocks until the transfer ends**, which is upstream's
    /// shape as well and for a reason worth keeping: a script's next line
    /// nearly always depends on the file having arrived. Upstream gets it by
    /// parking `ttpmacro` in `IdTTLWaitCmndResult` until `ProtoEnd` sends a
    /// DDE reply; here the transfer runs on the frontend's thread, the outcome
    /// is posted to a [`TransferReply`], and this waits on that.
    ///
    /// The answer is `filesys_proto.cpp:442`'s: `result` is 1 when the
    /// protocol succeeded and 0 for everything else, including a transfer that
    /// never started because nothing is connected.
    fn transfer(&mut self, req: &Xfer<'_>) -> Result<bool, TtlError> {
        let plan = Plan::of(req, &self.transfer_dir)?;
        let reply = TransferReply::new();
        let armed = reply.clone();
        if !self.ask(move |s| plan.start(s, armed))? {
            // Upstream's `*Start*` returns FALSE, `ttdde.c` answers
            // `DDE_FNOTPROCESSED`, and the macro reads a 0.
            return Ok(false);
        }

        let mut cancelling = false;
        let mut probed = Instant::now();
        loop {
            if let Some(outcome) = reply.wait(POLL) {
                return Ok(outcome.success);
            }
            if !cancelling && self.tx.cancelled() {
                cancelling = true;
                // Asking is all it does. The protocol sends its cancel
                // sequence and ends on its own terms — ZMODEM arms a 500 ms
                // timer for it — so the wait goes on until the outcome
                // arrives, which is also how upstream's End button behaves.
                let _ = self.tx.call(|s, _| s.cancel_transfer());
            }
            if probed.elapsed() >= PROBE {
                // The outcome is posted by the frontend's thread, so a
                // frontend that has gone would leave this waiting for ever —
                // and unlike a `wait`, nothing about a transfer polls the
                // session. Every other method here notices by a call coming
                // back empty; this one has to ask.
                self.ask(|_| ())?;
                probed = Instant::now();
            }
        }
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

    fn local_ip_addresses(&mut self, v6: bool) -> Option<Vec<Vec<u8>>> {
        // Answered here rather than posted as a job: it is a property of the
        // machine, not of the session, so making the frontend's thread look it
        // up would only be a way to fail when the window has gone.
        tt_conn::local_ip_addresses(v6).map(|v| v.into_iter().map(String::into_bytes).collect())
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
    // `setserialdelaychar`, `setserialdelayline` — the two of the serial group
    //   that are not a control line. They pace what is *sent*, and upstream
    //   paces it in `SendMem`, a queue between the macro and the wire that
    //   this port does not have: `send` writes straight through. One feature
    //   with three other callers waiting on it — a paste, a `sendfile` and
    //   the File menu's own send all go through the same queue — so it wants
    //   building once, for all of them, rather than behind these two.
    // `sendfile` — the one of the sixteen transfer commands that is not a
    //   protocol; the reason is in `Plan::of`, where the other fifteen are.
    // `scp` — an SSH channel `tt-conn` does not open yet.
    // `send_broadcast`, `send_multicast`, `set_multicast_name`, `wait_for_all`
    //   — all four are about the *other* sessions, and this crate is handed
    //   one. They belong to whatever owns the tab bar.
    // `send_key_code` — needs `KEYBOARD.CNF`, which is Stage 2's other half.
    // `load_key_map`, `restore_setup` — the same file, and `TERATERM.INI`.
    //   `restore_setup` is also the half of `connect` that is missing: a `/F=`
    //   naming a *different* settings file makes upstream re-read it and
    //   re-apply it (`ttdde.c:622`), and nothing here knows where the settings
    //   came from. The option is parsed and applied; the file is not re-read.
    // `set_debug_mode` — `ts.DebugMode` has no equivalent in `tt-vt` yet.
    //
    // Everything left on that list now wants a subsystem rather than a
    // method, which is the point of keeping it here: it is a list of what the
    // port has not built, not of what has not been typed.
    //
    // And one thing `connect` does *not* do that upstream's does: a `/L=` on
    // the line starts a log when the connection comes up (`vtwin.cpp:3631`,
    // inside `OnCommOpen`). It is the frontend's there and the frontend's here
    // — `shell/src/MainWindow.cpp` does it for the startup line — so putting a
    // second implementation behind `connect` would be two of them.
}

/// `connect` and `cygconnect`, on the frontend's thread — which is where
/// upstream's is too: `ttdde.c:608` parses the macro's string into `ts` and
/// posts `WM_USER_COMMSTART` to the *window*, and the macro sleeps until
/// `SetDdeComReady` answers.
///
/// Nothing is reported. Every arm that opens nothing leaves the session
/// disconnected, and `connect` reads that back as its `result` — which is the
/// documented three-value answer and covers a refused port, a name that does
/// not resolve, a frontend with no SSH dialogs and a line that named nothing at
/// all, without the macro having to tell them apart.
fn open(s: &mut Session, ui: &mut dyn crate::ui::MacroUi, arg: &[u8], cygwin: bool) {
    let (cols, rows) = (s.grid().cols() as u16, s.grid().rows() as u16);
    let startup = if cygwin {
        // `cyglaunch.exe`'s job, which here is a shell on a pty. The argument
        // is CygTerm's command line and not Tera Term's, and none of it is a
        // setting: CygTerm is a separate program reading `cygterm.cfg`.
        Startup::Open(Target::cygterm(arg, cols, rows))
    } else {
        let mut settings = s.settings().clone();
        // The parsed line is dropped: what this needs from it is already in
        // `startup` and in `settings`, and the two things it still holds —
        // `/F=` and `/L=` — are the frontend's, for the reasons at the bottom
        // of the list above.
        let (startup, _) = Startup::of_connect(arg, &mut settings, cols, rows);
        // Only when the line actually said something. Upstream writes `ts` and
        // does **not** re-apply it to the running terminal — only a `/F=` that
        // changed the settings file does that, through `IdCmdRestoreSetup` —
        // so a `connect 'myhost'` must not quietly reset the modes the last
        // host set, which is what `set_settings` would do.
        if settings != *s.settings() {
            let _ = s.set_settings(settings);
        }
        startup
    };
    let conn = match startup {
        // The prompt case, and the only one that leaves this crate: see
        // `MacroUi::connect_ssh`.
        Startup::Open(t @ Target::Ssh { .. }) => ui.connect_ssh(&t).ok().flatten(),
        Startup::Open(t) => t.open().ok(),
        // `OnFileNewConnection` — upstream puts the New Connection dialog up
        // and the macro waits for whoever is sitting there. There is no such
        // dialog to reach from here: the shell's own is serial-only and the
        // one this wants is the whole four-transport thing. So a `connect`
        // that named nothing reports "not connected" rather than asking, which
        // is `SetDdeComReady(0)` — the arm upstream takes when the dialog is
        // switched off.
        Startup::Dialog | Startup::Idle => None,
        // A transport the line named and this port does not have. Silent for
        // the same reason as the rest: `result` is the channel a macro has.
        Startup::Unsupported(_) => None,
    };
    if let Some(conn) = conn {
        s.connect(conn);
    }
}

/// A transfer command, resolved into what the session needs to start it.
///
/// Built on the macro's thread out of the borrowed request and then moved
/// across, because nothing may be borrowed over the boundary.
enum Plan {
    Send {
        job: XferJob,
        files: Vec<PathBuf>,
    },
    Recv {
        job: XferJob,
        dir: PathBuf,
        name: Option<PathBuf>,
    },
}

impl Plan {
    /// The mapping, which is `filesys_proto.cpp`'s `*Start*` functions read
    /// for what they do to `ts` before calling `OpenProtoDlg`.
    ///
    /// Two things are the same in every arm and both are upstream's. A
    /// relative filename is resolved against `ts.FileDir` — every one of those
    /// functions does `IsRelativePathW` then `GetFileDir(&ts)` — and that same
    /// directory is where a protocol that names its own file puts it, because
    /// it is what `GetRecievePath` answers.
    fn of(req: &Xfer<'_>, dir: &Option<Vec<u8>>) -> Result<Plan, TtlError> {
        let send = |job, path: &[u8]| Plan::Send {
            job,
            files: vec![resolve(dir, path)],
        };
        let recv = |job| Plan::Recv {
            job,
            dir: recv_dir(dir),
            name: None,
        };
        // For the three that are told a name: the same resolution, handed on
        // as the name rather than as the directory. `Transfer::receive` joins
        // it against the directory, and an absolute path wins that join.
        let recv_named = |job, path: &[u8]| Plan::Recv {
            job,
            dir: recv_dir(dir),
            name: Some(resolve(dir, path)),
        };

        Ok(match *req {
            // `xmodemsend` has no binary argument, so the mode is whatever
            // `ts.XmodemBin` says — and `GetOnOff(..., TRUE)` at
            // `ttset.c:1051` says binary. It is not in the schema yet; when it
            // is, this is one of the values `Session::transfer_options` should
            // be answering.
            Xfer::XmodemSend { path, opt } => send(
                XferJob::XModem {
                    dir: Direction::Send,
                    opt: xmodem_opt(opt),
                    text: false,
                },
                path,
            ),
            Xfer::XmodemRecv { path, binary, opt } => recv_named(
                XferJob::XModem {
                    dir: Direction::Receive,
                    opt: xmodem_opt(opt),
                    text: !binary,
                },
                path,
            ),
            // `Yopt1K` in both directions, hardcoded at
            // `filesys_proto.cpp:1409` and `:1447` with the comment saying so.
            Xfer::YmodemSend { path } => send(
                XferJob::YModem {
                    dir: Direction::Send,
                    opt: YmodemOpt::K1,
                },
                path,
            ),
            Xfer::YmodemRecv => recv(XferJob::YModem {
                dir: Direction::Receive,
                opt: YmodemOpt::K1,
            }),
            Xfer::ZmodemSend { path, binary } => send(
                XferJob::ZModem {
                    dir: Direction::Send,
                    binary,
                    auto: false,
                },
                path,
            ),
            // `ZMODEMStartReceive` passes 0 and it does not matter:
            // `zmodem.c:1008` overwrites `BinFlag` from the sender's own
            // ZFILE header, so the peer decides.
            Xfer::ZmodemRecv => recv(XferJob::ZModem {
                dir: Direction::Receive,
                binary: false,
                auto: false,
            }),
            Xfer::KmtSend { path } => send(
                XferJob::Kermit {
                    mode: KermitMode::Send,
                },
                path,
            ),
            Xfer::KmtRecv => recv(XferJob::Kermit {
                mode: KermitMode::Receive,
            }),
            // The one place the resolved path is not a local file: it is the
            // name asked of the peer, and `kermit.c:1160` takes its basename
            // before it goes in the `R` packet — so `kmtget` cannot name a
            // remote directory, here or upstream.
            Xfer::KmtGet { path } => recv_named(
                XferJob::Kermit {
                    mode: KermitMode::Get,
                },
                path,
            ),
            Xfer::KmtFinish => recv(XferJob::Kermit {
                mode: KermitMode::Finish,
            }),
            Xfer::BPlusSend { path } => send(
                XferJob::BPlus {
                    dir: Direction::Send,
                    auto: false,
                },
                path,
            ),
            Xfer::BPlusRecv => recv(XferJob::BPlus {
                dir: Direction::Receive,
                auto: false,
            }),
            Xfer::QuickVanSend { path } => send(
                XferJob::QuickVan {
                    dir: Direction::Send,
                },
                path,
            ),
            Xfer::QuickVanRecv => recv(XferJob::QuickVan {
                dir: Direction::Receive,
            }),
            // Not a protocol: the line into a file until it has been quiet for
            // `autostop`. **A `recvfile` that receives nothing never ends** —
            // `raw.c:168` arms the stop timer in the packet reader, so it is
            // the first byte that starts the clock and a zero-byte transfer
            // waits as long as one with `autostop 0`.
            Xfer::RecvFile { path, autostop } => recv_named(XferJob::Raw { autostop }, path),
            // `sendfile` is the File menu's, not `ttpfile`'s: upstream runs it
            // from `filesys.cpp:359` a byte at a time through the terminal's
            // own write path, with bracketed paste, local echo and the DBCS
            // decoding that goes with them. `raw.h` says outright that there
            // is no raw *send* protocol. It wants `Session`'s own file send,
            // which nothing has needed yet — the shell has no File menu.
            Xfer::SendFile { .. } => return Err(TtlError::NotSupported),
        })
    }

    fn start(self, s: &mut Session, reply: TransferReply) -> bool {
        let opts = s.transfer_options();
        let started = match self {
            Plan::Send { job, files } => s.send_files(job, &files, &opts).is_ok(),
            Plan::Recv { job, dir, name } => {
                let name = name.map(|p| p.to_string_lossy().into_owned());
                s.receive_files(job, &dir, name.as_deref(), &opts).is_ok()
            }
        };
        // Only once it is actually running, so that a refused start cannot
        // leave a reply armed for somebody else's transfer.
        if started {
            s.notify_transfer(reply);
        }
        started
    }
}

/// The macro language's XMODEM option, in `tt-xfer`'s terms.
///
/// The two enumerations are the same three values under different names; the
/// fourth, `Xopt1kCksum`, is unreachable from a macro because the interpreter
/// has already folded the argument against what the *sender* gets to choose.
fn xmodem_opt(opt: XmodemOpt) -> XferXmodemOpt {
    match opt {
        XmodemOpt::Checksum => XferXmodemOpt::Checksum,
        XmodemOpt::Crc => XferXmodemOpt::Crc,
        XmodemOpt::Crc1K => XferXmodemOpt::Crc1K,
    }
}

/// Where a protocol that names its own file puts it — `GetRecievePath`, which
/// upstream answers with `ts.FileDir`.
fn recv_dir(dir: &Option<Vec<u8>>) -> PathBuf {
    match dir {
        Some(d) => PathBuf::from(String::from_utf8_lossy(d).into_owned()),
        None => PathBuf::from("."),
    }
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
