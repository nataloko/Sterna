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

use std::time::Duration;

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

    /// Whether there is a connection — upstream's `Linked`.
    ///
    /// Every command that touches the far end asks first and answers
    /// `ErrLinkFirst` if not, which is why a macro run against no window fails
    /// loudly at its first `send` rather than quietly doing nothing.
    fn linked(&mut self) -> bool {
        false
    }

    /// `send` and `sendln` — bytes out of the connection, unchanged.
    fn send(&mut self, bytes: &[u8]) -> Result<(), TtlError> {
        let _ = bytes;
        Err(TtlError::NotSupported)
    }

    /// One byte in, blocking up to `timeout` — or for ever when it is `None`.
    ///
    /// `None` back means the wait is over without a byte: the deadline passed,
    /// or the connection went away. Upstream tells those apart and then treats
    /// them identically in every arm, so they are one answer here. A host that
    /// never returns `None` for a dead line will hang a macro that has no
    /// timeout set, which is upstream's behaviour too.
    fn read_byte(&mut self, timeout: Option<Duration>) -> Option<u8> {
        let _ = timeout;
        None
    }

    /// `flushrecv` — throw away whatever has arrived and not been read.
    fn flush_recv(&mut self) {}

    /// `pause` and `mpause`. Upstream's `pause` is a timer the window can
    /// interrupt and its `mpause` is a hard `Sleep`; both are here so that a
    /// frontend can make either cancellable.
    fn sleep(&mut self, d: Duration) {
        let _ = d;
    }

    /// `random`'s entropy. Upstream seeds SFMT from the clock; the rejection
    /// loop that makes the result uniform stays in the interpreter, so a host
    /// that wants a repeatable run only has to make this repeatable.
    fn random_u32(&mut self) -> u32 {
        0
    }

    /// A Unix timestamp as `%Y-%m-%d %H:%M:%S` — `filestat`'s third output,
    /// and what `getdate` and `gettime` will want.
    ///
    /// It is here because a wall clock is not in the standard library: there
    /// are no time zones and no `strftime`, and this crate has no
    /// dependencies. Upstream's `localtime` is the *user's* zone, which the
    /// frontend knows and the interpreter does not, so the default below
    /// answers in **UTC** — right shape, and honest about not knowing where it
    /// is. A host with a date library should override it.
    fn format_time(&mut self, unix_secs: i64) -> String {
        format_utc(unix_secs)
    }

    /// Whether the run has been cancelled from outside.
    ///
    /// The interpreter runs on its own thread and blocks in `wait` and `pause`,
    /// so this is the only way a stop request reaches a macro that is not
    /// executing lines. Checked once per line.
    fn cancelled(&mut self) -> bool {
        false
    }

    // ---- the session ----

    /// Whether the terminal has a connection open — upstream's `ComReady`.
    ///
    /// Distinct from [`linked`](ScriptHost::linked), which is whether there is
    /// a terminal at all. `testlink` reports the pair as one number and
    /// `connect` answers with the same one.
    fn com_ready(&mut self) -> bool {
        false
    }

    /// `connect` / `cygconnect` — open a connection described by a Tera Term
    /// command line, blocking until it is up or has failed.
    ///
    /// The command line is passed through unparsed. `cygwin` selects
    /// `cygconnect`, which upstream implements by launching `cyglaunch.exe`
    /// instead of `ttermpro.exe` and which here means a local shell.
    ///
    /// **Upstream's two branches are one call.** `TTLConnect` either tells an
    /// existing Tera Term to connect or spawns a fresh `ttermpro.exe` and
    /// links to it over DDE; in-process the second has no analogue, because
    /// the host either has a window or it does not and the macro cannot
    /// conjure one. What the macro observes is unchanged: `result` is read
    /// back off `linked` and `com_ready` afterwards, which is exactly the
    /// three-value table the documentation promises.
    fn connect(&mut self, cmdline: &[u8], cygwin: bool) -> Result<(), TtlError> {
        let _ = (cmdline, cygwin);
        Err(TtlError::NotSupported)
    }

    /// `disconnect` — close the connection, keeping the terminal.
    ///
    /// `confirm` is upstream's optional argument and defaults to *true*: a
    /// bare `disconnect` puts the confirmation dialog up, and only
    /// `disconnect 0` skips it.
    fn disconnect(&mut self, confirm: bool) -> Result<(), TtlError> {
        let _ = confirm;
        Err(TtlError::NotSupported)
    }

    /// `closett` — close the terminal, blocking until it has gone.
    fn close_terminal(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `unlink` and the tail of `closett` — give up the terminal.
    ///
    /// After this [`linked`](ScriptHost::linked) must answer `false`, which is
    /// what makes every communication command fail with `ErrLinkFirst` until a
    /// `connect` links again.
    fn unlink(&mut self) {}

    /// `setsync` — whether the terminal throttles itself to what the macro has
    /// read.
    ///
    /// Upstream's asynchronous mode is a 16 KB ring the terminal fills whether
    /// or not anything is reading, and the synchronous mode is that ring with
    /// backpressure. A host whose reads are already backpressured has nothing
    /// to do here, which is why this refuses nothing and returns nothing.
    fn set_sync(&mut self, on: bool) {
        let _ = on;
    }

    // ---- the serial control lines ----
    //
    // Every one of these is a no-op upstream unless the connection is serial,
    // and `setdtr`/`setrts` need the flow control to be "none" as well
    // (`ttdde.c:1013`, `:1032`). None of that is the interpreter's business:
    // the terminal answers `DDE_FNOTPROCESSED` and the macro carries on, so
    // the host declining quietly is the faithful shape.

    /// `setdtr` — raise or lower DTR.
    fn set_dtr(&mut self, on: bool) {
        let _ = on;
    }

    /// `setrts` — raise or lower RTS.
    fn set_rts(&mut self, on: bool) {
        let _ = on;
    }

    /// `setbaud` / `setspeed` — the serial line's speed in bits per second.
    ///
    /// Upstream ignores a value that is not positive (`ttdde.c:983`), so this
    /// is never called with zero.
    fn set_baud(&mut self, baud: u32) {
        let _ = baud;
    }

    /// `setflowctrl`.
    fn set_flow_control(&mut self, flow: FlowControl) {
        let _ = flow;
    }

    /// `getmodemstatus` — the four input control lines.
    ///
    /// `None` is "could not ask", which the macro cannot tell from all four
    /// being low; see [`ModemLines`].
    fn modem_lines(&mut self) -> Option<ModemLines> {
        None
    }

    /// `sendbreak`. Serial holds the line at space; telnet sends `IAC BREAK`
    /// and SSH a `break` channel request, which is why this is not serial-only.
    fn send_break(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    // ---- file transfer ----

    /// One of the transfer commands, run to completion. `true` if the file
    /// arrived.
    ///
    /// The one method here that is not one-per-command, because [`Xfer`] is:
    /// sixteen commands that differ only in what they name go through one
    /// call, and the enum keeps each one's arguments exactly as its own
    /// command supplies them rather than flattening them into a request struct
    /// with fields that half the protocols ignore.
    ///
    /// It blocks, as upstream's does — `IdTTLWaitCmndResult` parks the macro
    /// until the protocol reports — so a frontend must run the interpreter off
    /// the UI thread, which it must anyway for `wait`.
    fn transfer(&mut self, req: &Xfer<'_>) -> Result<bool, TtlError> {
        let _ = req;
        Err(TtlError::NotSupported)
    }

    // ---- the dialogs ----
    //
    // Upstream's are `ttpmacro.exe`'s own windows, put up on the thread that
    // is also running the macro, so each is an ordinary `DoModal` and the
    // interpreter simply waits. That is the shape here too — every one of
    // these blocks — and it is why the interpreter must not be on the UI
    // thread: a frontend answers these by spinning its own event loop.
    //
    // They refuse by default rather than answering for the absent user. A
    // macro that puts up a `messagebox` on a host with no UI would otherwise
    // carry on as though somebody had read it.

    /// `messagebox` — one button, and no answer to report.
    ///
    /// [`Cancel`] cannot happen: `CMsgDlg::OnCancel` swallows Escape when
    /// there is no No button (`msgdlg.cpp:216`). The only way out other than
    /// OK is the window's close button and its confirmation, which upstream
    /// ends this dialog with `IDCANCEL` and the yes/no one with `IDCLOSE` —
    /// so the interpreter ends the macro on either non-`Ok` answer.
    ///
    /// [`Cancel`]: DialogEnd::Cancel
    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `yesnobox` — [`Ok`] is Yes, [`Cancel`] is No, and [`Closed`] ends the
    /// macro with `result` still 0.
    ///
    /// [`Ok`]: DialogEnd::Ok
    /// [`Cancel`]: DialogEnd::Cancel
    /// [`Closed`]: DialogEnd::Closed
    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `statusbox` — put up the modeless status box, or retitle the one that
    /// is already there.
    ///
    /// One dialog, not one per call: `OpenStatDlg` (`ttmdlg.cpp:326`) creates
    /// it the first time and updates it every time after, and the user cannot
    /// dismiss it — only `closesbox` closes it. It does not block.
    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `closesbox`. Nothing to close is not an error upstream, and must not be
    /// one here.
    fn close_status_box(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `bringupbox` — raise the status box. Also a no-op when there is none.
    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `listbox` — choose one of `items`, starting on `selected`.
    ///
    /// `selected` has already been folded to 0 when the macro's index was out
    /// of range, which is upstream's (`ttl_gui.cpp:512`), so it is always a
    /// valid index and `items` is never empty.
    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[&[u8]],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let _ = (text, title, items, selected, opts);
        Err(TtlError::NotSupported)
    }

    /// `inputbox`, and `passwordbox` with `password` set.
    ///
    /// [`Cancel`] is Escape. Upstream cannot tell it from OK — see the note on
    /// the command — so the interpreter answers it with an empty `inputstr`,
    /// which is what the documentation promises and what upstream's own
    /// `getpassword` does with the same dialog.
    ///
    /// [`Cancel`]: DialogEnd::Cancel
    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let _ = (text, title, default, password);
        Err(TtlError::NotSupported)
    }

    /// `filenamebox` — the platform's file chooser. `None` is cancelled.
    ///
    /// `save` is the documented meaning of the `<dialogtype>` argument, not
    /// upstream's flags, which are swapped; see the note on the command.
    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        let _ = (title, save, init_dir);
        Err(TtlError::NotSupported)
    }

    /// `dirnamebox` — the platform's folder chooser. `None` is cancelled.
    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        let _ = (title, init_dir);
        Err(TtlError::NotSupported)
    }

    /// `setdlgpos` — where the next dialog opens, and where the status box
    /// moves to if one is already up (`ttmdlg.cpp:189`).
    ///
    /// `None` is the no-argument form: upstream stores `CW_USEDEFAULT` in both
    /// coordinates, which every dialog reads as "centre me". A preference with
    /// no user in it, so it does not refuse — a host with no dialogs has
    /// nothing to do and upstream's command cannot fail either.
    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        let _ = pos;
    }

    // ---- session logging ----
    //
    // The log is the *terminal's*, not the macro's: upstream sends each of
    // these over DDE to `filesys_log.cpp`, which is also where the file the
    // user opened from the menu lives. So `logopen` from a macro and
    // `File > Log` are one log and the second of them to run displaces the
    // first — which is why `logopen` reports failure when one is already
    // open rather than quietly opening a second.

    /// `logopen` — start logging. `true` if the file was opened.
    ///
    /// Reported to the macro **inverted**: `result` is 0 for success and 1 for
    /// failure, which is the opposite of every other command here and is what
    /// the documentation promises.
    fn log_open(&mut self, req: &LogOpen<'_>) -> Result<bool, TtlError> {
        let _ = req;
        Err(TtlError::NotSupported)
    }

    /// `logclose`. Closing a log that is not open is not an error.
    fn log_close(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `logpause` and `logstart` — stop and resume writing.
    ///
    /// What arrives while paused is **discarded**, not buffered
    /// (`logpause.html`), so this is not a valve on a queue.
    fn log_pause(&mut self, paused: bool) -> Result<(), TtlError> {
        let _ = paused;
        Err(TtlError::NotSupported)
    }

    /// `logwrite` — put a string into the log without it having come from the
    /// far end. Works while paused, which is the point of it.
    fn log_write(&mut self, s: &[u8]) -> Result<(), TtlError> {
        let _ = s;
        Err(TtlError::NotSupported)
    }

    /// `loginfo` — the open log's name and flags, or `None` when not logging.
    fn log_info(&mut self) -> Result<Option<LogInfo>, TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `logrotate` — reconfigure rotation. It does not rotate anything now;
    /// the documentation says so twice.
    fn log_rotate(&mut self, how: LogRotate) -> Result<(), TtlError> {
        let _ = how;
        Err(TtlError::NotSupported)
    }

    // ---- the terminal's odds and ends ----
    //
    // The `TTLCommCmd*` two-liners, whose behaviour is all in `ttdde.c`.
    // Three of them switch on the *first character* of the decimal argument;
    // that is folded into the `from_code` on each enum, so a host sees only
    // the meaning.

    /// `beep` — a sound, not a terminal bell. Upstream's is `MessageBeep` in
    /// the macro process, so it works with no terminal attached and the sounds
    /// are Windows' system events; a host maps them to whatever it has.
    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        let _ = sound;
        Err(TtlError::NotSupported)
    }

    /// `callmenu` — invoke a menu item by its Win32 command id.
    ///
    /// The ids are the ones in upstream's `Keycode` documentation and there is
    /// no way to make them portable: a macro that says `callmenu 50210` means
    /// Edit > Copy, and a host either has that table or it does not.
    /// Upstream routes 51110..51990 to the TEK window and everything else to
    /// the VT one (`ttdde.c:931`).
    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        let _ = id;
        Err(TtlError::NotSupported)
    }

    /// `changedir` — the **file transfer** directory, which is not the macro's
    /// own.
    ///
    /// `setdir` moves the macro; this moves what a relative filename in
    /// `sendfile`, `zmodemrecv` and the rest resolves against, and where a log
    /// lands. Two commands, two directories, and the names are the wrong way
    /// round for guessing.
    fn set_transfer_dir(&mut self, path: &[u8]) -> Result<(), TtlError> {
        let _ = path;
        Err(TtlError::NotSupported)
    }

    /// `clearscreen`.
    fn clear_screen(&mut self, what: ClearScreen) -> Result<(), TtlError> {
        let _ = what;
        Err(TtlError::NotSupported)
    }

    /// `enablekeyb` — stop the user typing into the terminal while a macro
    /// drives it.
    fn enable_keyboard(&mut self, on: bool) -> Result<(), TtlError> {
        let _ = on;
        Err(TtlError::NotSupported)
    }

    /// `loadkeymap` — read a `KEYBOARD.CNF`.
    fn load_key_map(&mut self, path: &[u8]) -> Result<(), TtlError> {
        let _ = path;
        Err(TtlError::NotSupported)
    }

    /// `restoresetup` — read a `TERATERM.INI` and apply it.
    fn restore_setup(&mut self, path: &[u8]) -> Result<(), TtlError> {
        let _ = path;
        Err(TtlError::NotSupported)
    }

    /// `setdebug` — what the terminal does with what arrives.
    fn set_debug_mode(&mut self, mode: DebugMode) -> Result<(), TtlError> {
        let _ = mode;
        Err(TtlError::NotSupported)
    }

    /// `setecho`. Upstream also renegotiates telnet echo when the connection
    /// is a telnet one and `ts.TelEcho` is set (`ttdde.c:830`), which is the
    /// host's business rather than the interpreter's.
    fn set_local_echo(&mut self, on: bool) -> Result<(), TtlError> {
        let _ = on;
        Err(TtlError::NotSupported)
    }

    /// `settitle`.
    fn set_title(&mut self, title: &[u8]) -> Result<(), TtlError> {
        let _ = title;
        Err(TtlError::NotSupported)
    }

    /// `gettitle`.
    fn title(&mut self) -> Result<Vec<u8>, TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `showtt` — the terminal, the TEK window or the log window.
    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        let _ = which;
        Err(TtlError::NotSupported)
    }

    /// `show` — the **macro's** own window, which upstream has and this port
    /// only has if the frontend made one to show a macro running.
    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        let _ = how;
        Err(TtlError::NotSupported)
    }

    /// `getttpos` — where the terminal's window is. `None` is "cannot say",
    /// which the macro reads as -1.
    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `setserialdelaychar` / `setserialdelayline` — pace what `send` writes,
    /// by character or by line. `true` if the connection took it.
    ///
    /// Serial-only upstream, and one of the few commands that waits for a
    /// result, so a host with another kind of connection answers `false`
    /// rather than refusing — the same shape as the control lines.
    fn set_serial_delay(&mut self, per_line: bool, ms: i32) -> Result<bool, TtlError> {
        let _ = (per_line, ms);
        Err(TtlError::NotSupported)
    }

    /// `logautoclosemode` — whether the log closes when the **macro** ends.
    ///
    /// Not the connection. Upstream hangs it off the DDE conversation going
    /// away (`ttdde.c:1340`), and clears the flag at the same time, so it
    /// lasts one run and no longer — which the documentation calls out because
    /// it surprises people.
    fn log_auto_close(&mut self, on: bool) -> Result<(), TtlError> {
        let _ = on;
        Err(TtlError::NotSupported)
    }
}

/// `beep`'s optional argument (`ttl.cpp:TTLBeep`).
///
/// Windows system-event sounds, so a host that has none of them plays whatever
/// it does have. Only [`Simple`](BeepSound::Simple) has a portable meaning:
/// upstream passes `MessageBeep(-1)`, which is the speaker rather than a
/// theme sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BeepSound {
    Simple,
    Asterisk,
    Exclamation,
    CriticalStop,
    Question,
    /// The default beep, and what a bare `beep` plays.
    #[default]
    Default,
}

impl BeepSound {
    /// 0 through 5. Unlike its neighbours in this family, `beep` reports a
    /// syntax error for anything else rather than doing nothing.
    pub fn from_code(v: i32) -> Option<BeepSound> {
        Some(match v {
            0 => BeepSound::Simple,
            1 => BeepSound::Asterisk,
            2 => BeepSound::Exclamation,
            3 => BeepSound::CriticalStop,
            4 => BeepSound::Question,
            5 => BeepSound::Default,
            _ => return None,
        })
    }
}

/// `clearscreen`'s target (`ttdde.c:592`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearScreen {
    /// The visible screen, leaving the scrollback.
    Screen,
    /// The scrollback as well.
    ScreenAndBuffer,
    /// The TEK window's, which this port does not have.
    TekScreen,
}

impl ClearScreen {
    /// **The first character of the decimal argument**, which is what reaches
    /// the terminal. `clearscreen 25` is `clearscreen 2`, and anything with no
    /// arm — including every negative, since `'-'` is the character — does
    /// nothing at all rather than reporting.
    pub fn from_code(v: i32) -> Option<ClearScreen> {
        Some(match first_digit(v) {
            b'0' => ClearScreen::Screen,
            b'1' => ClearScreen::ScreenAndBuffer,
            b'2' => ClearScreen::TekScreen,
            _ => return None,
        })
    }
}

/// `showtt`'s ten states (`ttdde.c:846`), across three windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowWindow {
    VtHide,
    VtMinimize,
    VtRestore,
    /// The four TEK arms, which this port does not have a window for.
    TekHide,
    TekMinimize,
    TekOpen,
    TekClose,
    LogHide,
    LogMinimize,
    LogRestore,
}

impl ShowWindow {
    /// The first character again, so `showtt 100` restores the VT window and
    /// **any** negative hides it — `-1` and `-99` are the same `'-'` arm.
    pub fn from_code(v: i32) -> Option<ShowWindow> {
        Some(match first_digit(v) {
            b'-' => ShowWindow::VtHide,
            b'0' => ShowWindow::VtMinimize,
            b'1' => ShowWindow::VtRestore,
            b'2' => ShowWindow::TekHide,
            b'3' => ShowWindow::TekMinimize,
            b'4' => ShowWindow::TekOpen,
            b'5' => ShowWindow::TekClose,
            b'6' => ShowWindow::LogHide,
            b'7' => ShowWindow::LogMinimize,
            b'8' => ShowWindow::LogRestore,
            _ => return None,
        })
    }
}

/// `setdebug`'s four modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMode {
    /// Display what arrives, as usual.
    Off,
    /// Show control codes as their caret spellings.
    Normal,
    /// Show every byte as two uppercase hex digits, space separated.
    Hex,
    /// Display nothing at all.
    Silent,
}

impl DebugMode {
    /// The first character once more (`ttdde.c:834` sends `Command[1]`), so
    /// `setdebug 10` is `setdebug 1`.
    pub fn from_code(v: i32) -> Option<DebugMode> {
        Some(match first_digit(v) {
            b'0' => DebugMode::Off,
            b'1' => DebugMode::Normal,
            b'2' => DebugMode::Hex,
            b'3' => DebugMode::Silent,
            _ => return None,
        })
    }
}

/// The first character of a number's decimal spelling — `'-'` for a negative.
///
/// Three of `ttdde.c`'s commands read exactly this and nothing else, because
/// the argument crosses the DDE boundary as text and the switch never looks
/// past `[0]`.
fn first_digit(v: i32) -> u8 {
    // The same rendering upstream sends: `"%d"`, then byte zero of it.
    v.to_string().as_bytes()[0]
}

/// `show`'s three states — the macro's own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroWindow {
    /// A negative argument.
    Hide,
    /// Zero.
    Minimize,
    /// Anything positive, which also raises it.
    Restore,
}

/// What `getttpos` reports (`ttdde.c:1136`).
///
/// Both rectangles are `(x, y, width, height)` in screen pixels: the frame
/// first, then the text area inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub state: WindowState,
    pub window: (i32, i32, i32, i32),
    pub client: (i32, i32, i32, i32),
}

/// `getttpos`'s first output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Hidden,
}

impl WindowState {
    /// The number the macro sees. Upstream tests iconic, then zoomed, then
    /// visible, in that order, so a minimised window reports 1 and not 3.
    pub fn code(self) -> i32 {
        match self {
            WindowState::Normal => 0,
            WindowState::Minimized => 1,
            WindowState::Maximized => 2,
            WindowState::Hidden => 3,
        }
    }
}

/// `logopen`'s arguments, in the order the command reads them.
///
/// The four flags after `append` are optional and each defaults to off, so a
/// host sees the same struct however many the macro wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogOpen<'a> {
    /// As written. A bare name is relative to the macro's current directory,
    /// which is `changedir`'s, not the process's.
    pub path: &'a [u8],
    /// Every byte as it arrived, escape sequences included.
    pub binary: bool,
    /// Add to the file rather than truncating it.
    pub append: bool,
    /// Drop non-printable ASCII.
    pub plain_text: bool,
    /// Prefix each line with a time.
    pub timestamp: bool,
    /// Do not show the log's progress window.
    pub hide_dialog: bool,
    /// Write the scrollback into the file before anything new.
    pub include_screen: bool,
    /// Which clock [`timestamp`](LogOpen::timestamp) reads.
    pub timestamp_type: LogClock,
}

/// `logopen`'s `<timestamp type>` (`logopen.html`).
///
/// Not `tt_session::log::Timestamp`, for the reason [`FlowControl`] is not
/// `tt_conn`'s: the numbering is the macro language's and this crate depends
/// on nothing. Note there is no "none" here — that is the `timestamp` flag —
/// and that upstream's two elapsed clocks measure from different events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogClock {
    #[default]
    Local,
    Utc,
    /// Since the log was opened.
    ElapsedLog,
    /// Since the connection came up.
    ElapsedConnection,
}

impl LogClock {
    /// 0 through 3. `logopen` rejects anything else, so `None` is unreachable
    /// from a macro.
    pub fn from_code(v: i32) -> Option<LogClock> {
        match v {
            0 => Some(LogClock::Local),
            1 => Some(LogClock::Utc),
            2 => Some(LogClock::ElapsedLog),
            3 => Some(LogClock::ElapsedConnection),
            _ => None,
        }
    }
}

/// What `loginfo` reports about an open log (`filesys_log.cpp:852`).
///
/// The five flags are the ones `logopen` was given, not what the log is doing
/// now — pausing it does not show up here, and neither does the timestamp
/// *type*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogInfo {
    pub path: Vec<u8>,
    pub binary: bool,
    pub append: bool,
    pub plain_text: bool,
    pub timestamp: bool,
    pub hide_dialog: bool,
}

/// `logrotate`'s three forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRotate {
    /// Rotate once the file passes this many bytes. At least 128, which the
    /// command enforces; a `K` or `M` suffix has already been multiplied out.
    Size(i32),
    /// How many generations to keep, at least 1.
    Keep(i32),
    /// Stop rotating.
    Halt,
}

/// How a modal dialog ended, and what it produced.
///
/// The three ways out are the same for all of them: `Ok` is the affirmative
/// button, `Cancel` is No or Escape, and `Closed` is the window's close button
/// **after** the "halt the script?" confirmation upstream puts in front of it
/// (`msgdlg.cpp:227`). That confirmation is why `Closed` ends the macro and
/// `Cancel` does not — the user has already said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEnd<T = ()> {
    Ok(T),
    Cancel,
    Closed,
}

/// `listbox`'s keyword parameters (`ttl_gui.cpp:476`).
///
/// All of them are hints about the window rather than about the choice, so a
/// host that ignores the lot still implements `listbox` correctly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListBoxOpts {
    /// `dblclick=on` — a double click chooses the item under it.
    pub double_click: bool,
    /// `minmaxbutton=on`.
    pub min_max_button: bool,
    /// `minimize=on`. Exclusive with [`maximized`](ListBoxOpts::maximized):
    /// each keyword clears the other, so the last one written wins.
    pub minimized: bool,
    /// `maximize=on`.
    pub maximized: bool,
    /// `listboxsize=WxH`, in characters — width then height.
    pub size: Option<(u32, u32)>,
}

/// Where `setdlgpos` wants the dialogs, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogPos {
    /// The top-left corner, from the primary display's origin.
    pub x: i32,
    pub y: i32,
    /// The `<position>` argument, when one was given. Absent means the
    /// coordinates alone decide, which is the two-argument form.
    pub anchor: Option<(DialogAnchor, DialogOrigin)>,
    /// Added to the anchored position, and zero unless an anchor was given.
    pub offset_x: i32,
    pub offset_y: i32,
}

/// Which corner of [`DialogOrigin`] a dialog is placed against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

/// What [`DialogAnchor`] is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOrigin {
    /// The display the terminal is on, or the primary one when there is no
    /// terminal to ask.
    Display,
    /// The terminal window itself. Upstream falls back to the stored
    /// coordinates when there is no window, or when it is minimised or hidden
    /// (`ttmdlg.cpp:247`), because a window nobody can see is not a position.
    VtWindow,
}

impl DialogAnchor {
    /// `setdlgpos`'s `<position>`: 1-5 against the display and 6-10 against
    /// the VT window, each in the order top-left, top-right, bottom-left,
    /// bottom-right, centre. The command rejects anything else outright, so
    /// the `None` here is unreachable from a macro.
    pub fn from_code(v: i32) -> Option<(DialogAnchor, DialogOrigin)> {
        let origin = match v {
            1..=5 => DialogOrigin::Display,
            6..=10 => DialogOrigin::VtWindow,
            _ => return None,
        };
        let anchor = match (v - 1) % 5 {
            0 => DialogAnchor::TopLeft,
            1 => DialogAnchor::TopRight,
            2 => DialogAnchor::BottomLeft,
            3 => DialogAnchor::BottomRight,
            _ => DialogAnchor::Center,
        };
        Some((anchor, origin))
    }
}

/// A Unix timestamp as `%Y-%m-%d %H:%M:%S`, in UTC.
///
/// Hinnant's `civil_from_days`: shift the epoch to March 1st of year 0 so that
/// the leap day lands at the end of the era, and the month arithmetic becomes
/// exact integer division. Correct for any date the proleptic Gregorian
/// calendar covers, which is more than a filesystem will produce.
pub fn format_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);

    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

/// `setflowctrl`'s four values (`ttdde.c:1002`).
///
/// Not `tt_conn::serial::FlowControl`, and deliberately: the numbering is the
/// macro language's, this crate depends on nothing, and a host that has a
/// serial port maps one enum to the other in the one place that has both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    XonXoff,
    RtsCts,
    None,
    DsrDtr,
}

impl FlowControl {
    /// The number as `setflowctrl` writes it. Anything else is not an error —
    /// the terminal's `switch` has no arm for it, so the command does nothing
    /// at all.
    pub fn from_code(v: i32) -> Option<FlowControl> {
        match v {
            1 => Some(FlowControl::XonXoff),
            2 => Some(FlowControl::RtsCts),
            3 => Some(FlowControl::None),
            4 => Some(FlowControl::DsrDtr),
            _ => None,
        }
    }
}

/// What `getmodemstatus` reports, before it becomes a bit mask.
///
/// The mask itself — 1, 2, 4, 8 — is built in the interpreter because the
/// numbering is the macro language's; upstream builds it in `ttdde.c:1112`
/// out of `GetCommModemStatus`'s `MS_*` bits, which are Win32's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModemLines {
    pub cts: bool,
    pub dsr: bool,
    pub ring: bool,
    /// RLSD, which everything outside Win32 calls DCD.
    pub carrier: bool,
}

/// XMODEM's block format, numbered as the macro language numbers it
/// (`xmodem.h:43`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmodemOpt {
    Checksum,
    Crc,
    /// 1K blocks. Sender-side only: `xmodemrecv 3` is folded to [`Crc`] before
    /// it ever reaches here, which upstream comments "for compatibility".
    ///
    /// [`Crc`]: XmodemOpt::Crc
    Crc1K,
}

/// A transfer command, with the arguments its own command line supplies.
///
/// One variant per command rather than a protocol plus a direction, because
/// the commands are not symmetrical: `xmodemsend` has no binary flag
/// (`XMODEMStartSend` does not take one), the receive halves of ZMODEM, YMODEM,
/// Kermit, B-Plus and Quick-VAN name no file at all, and `recvfile` is not a
/// protocol. Flattening the sixteen into one struct would mean inventing
/// values for the fields each of them does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Xfer<'a> {
    XmodemSend {
        path: &'a [u8],
        opt: XmodemOpt,
    },
    XmodemRecv {
        path: &'a [u8],
        binary: bool,
        opt: XmodemOpt,
    },
    YmodemSend {
        path: &'a [u8],
    },
    YmodemRecv,
    ZmodemSend {
        path: &'a [u8],
        binary: bool,
    },
    ZmodemRecv,
    KmtSend {
        path: &'a [u8],
    },
    KmtRecv,
    /// Ask a peer in server mode for a file by name.
    KmtGet {
        path: &'a [u8],
    },
    /// Tell a peer in server mode to leave it.
    KmtFinish,
    BPlusSend {
        path: &'a [u8],
    },
    BPlusRecv,
    QuickVanSend {
        path: &'a [u8],
    },
    QuickVanRecv,
    /// `sendfile` — no protocol: the file's bytes down the line, with CR/LF
    /// translation and control-character stripping unless `binary`.
    SendFile {
        path: &'a [u8],
        binary: bool,
    },
    /// `recvfile` — the line into a file until it has been quiet for
    /// `autostop`. Zero means wait for ever; upstream floors a negative
    /// argument at zero rather than treating it as an error.
    RecvFile {
        path: &'a [u8],
        autostop: Duration,
    },
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
    /// A counter rather than entropy, so `random` is repeatable in a test.
    pub random_seq: u32,
    /// Whether the connection commands should believe there is a connection.
    pub linked: bool,
    /// What the far end has to say, consumed a byte at a time.
    pub input: std::collections::VecDeque<u8>,
    /// What the macro sent.
    pub sent: Vec<u8>,
    /// How long it asked to sleep for, in total.
    pub slept: Duration,
    pub flushes: usize,

    // The session, recorded rather than acted on.
    /// Whether the connection commands should believe there is a connection.
    /// `connect` sets it, `disconnect` clears it, so `testlink` moves.
    pub com_ready: bool,
    /// Every `connect` / `cygconnect`: the command line, and whether it was
    /// the cygwin one.
    pub connects: Vec<(Vec<u8>, bool)>,
    /// Whether `connect` should succeed.
    pub connect_fails: bool,
    /// Every `disconnect`, and whether it asked for confirmation.
    pub disconnects: Vec<bool>,
    pub unlinks: usize,
    pub closes: usize,
    pub syncs: Vec<bool>,
    /// Every control-line change, rendered, in order — one list so a test can
    /// assert the order across the whole family rather than a field each.
    pub lines: Vec<String>,
    /// What `getmodemstatus` should find. `None` is a port that cannot answer.
    pub modem: Option<ModemLines>,
    /// Every transfer, rendered — the request borrows, and its `Debug` is
    /// exactly what a test wants to assert on.
    pub transfers: Vec<String>,
    /// Whether a transfer should report success.
    pub transfer_fails: bool,

    // The dialogs. This host implements them all rather than refusing, which
    // is the point of it — a test asserts on what was asked and answers from
    // the queues below. For the refusing default, use a host that does not
    // override them.
    /// Every dialog put up, rendered, in order — including the ones that ask
    /// nothing, so a test can assert `closesbox` ran.
    pub dialogs: Vec<String>,
    /// What `messagebox` and `yesnobox` should answer, in order. An empty
    /// queue is `Ok(())`, which is the button a user presses without thinking.
    pub msg_replies: std::collections::VecDeque<DialogEnd>,
    /// What `inputbox` and `passwordbox` should answer. Empty is `Cancel`.
    pub input_replies: std::collections::VecDeque<DialogEnd<Vec<u8>>>,
    /// What `listbox` should answer. Empty is `Cancel`.
    pub list_replies: std::collections::VecDeque<DialogEnd<usize>>,
    /// What `filenamebox` and `dirnamebox` should answer. Empty is cancelled.
    pub file_replies: std::collections::VecDeque<Option<Vec<u8>>>,
    /// The last `setdlgpos`, or `None` if it asked for the default position.
    pub dialog_pos: Option<DialogPos>,

    /// Every terminal odds-and-ends command, rendered, in order.
    pub terminal: Vec<String>,
    /// What `gettitle` should find.
    pub title: Vec<u8>,
    /// What `getttpos` should find. `None` is a host with no window.
    pub geometry: Option<WindowGeometry>,
    /// Whether the two serial-delay commands should report failure, which is
    /// what a connection that is not serial does.
    pub serial_delay_fails: bool,

    /// Every logging command, rendered, in order.
    pub logs: Vec<String>,
    /// Whether `logopen` should report that it opened the file.
    pub log_open_fails: bool,
    /// What `loginfo` should find. `None` is a terminal that is not logging.
    pub log_info: Option<LogInfo>,
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

    fn random_u32(&mut self) -> u32 {
        self.random_seq = self.random_seq.wrapping_add(1);
        self.random_seq
    }

    fn linked(&mut self) -> bool {
        self.linked
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), TtlError> {
        self.sent.extend_from_slice(bytes);
        Ok(())
    }

    /// The recorded input, then `None` for ever — which stands in for both a
    /// timeout and a closed line, and is what keeps a test from blocking.
    fn read_byte(&mut self, _timeout: Option<Duration>) -> Option<u8> {
        self.input.pop_front()
    }

    fn flush_recv(&mut self) {
        self.flushes += 1;
        self.input.clear();
    }

    fn sleep(&mut self, d: Duration) {
        self.slept += d;
    }

    fn com_ready(&mut self) -> bool {
        self.com_ready
    }

    fn connect(&mut self, cmdline: &[u8], cygwin: bool) -> Result<(), TtlError> {
        self.connects.push((cmdline.to_vec(), cygwin));
        if self.connect_fails {
            return Ok(());
        }
        self.linked = true;
        self.com_ready = true;
        Ok(())
    }

    fn disconnect(&mut self, confirm: bool) -> Result<(), TtlError> {
        self.disconnects.push(confirm);
        self.com_ready = false;
        Ok(())
    }

    fn close_terminal(&mut self) -> Result<(), TtlError> {
        self.closes += 1;
        self.com_ready = false;
        Ok(())
    }

    fn unlink(&mut self) {
        self.unlinks += 1;
        self.linked = false;
    }

    fn set_sync(&mut self, on: bool) {
        self.syncs.push(on);
    }

    fn set_dtr(&mut self, on: bool) {
        self.lines.push(format!("dtr={}", on as u8));
    }

    fn set_rts(&mut self, on: bool) {
        self.lines.push(format!("rts={}", on as u8));
    }

    fn set_baud(&mut self, baud: u32) {
        self.lines.push(format!("baud={baud}"));
    }

    fn set_flow_control(&mut self, flow: FlowControl) {
        self.lines.push(format!("flow={flow:?}"));
    }

    fn modem_lines(&mut self) -> Option<ModemLines> {
        self.modem
    }

    fn send_break(&mut self) -> Result<(), TtlError> {
        self.lines.push("break".into());
        Ok(())
    }

    fn transfer(&mut self, req: &Xfer<'_>) -> Result<bool, TtlError> {
        self.transfers.push(format!("{req:?}"));
        Ok(!self.transfer_fails)
    }

    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        self.dialogs
            .push(format!("messagebox {}", show2(text, title)));
        Ok(self.msg_replies.pop_front().unwrap_or(DialogEnd::Ok(())))
    }

    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        self.dialogs
            .push(format!("yesnobox {}", show2(text, title)));
        Ok(self.msg_replies.pop_front().unwrap_or(DialogEnd::Ok(())))
    }

    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        self.dialogs
            .push(format!("statusbox {}", show2(text, title)));
        Ok(())
    }

    fn close_status_box(&mut self) -> Result<(), TtlError> {
        self.dialogs.push("closesbox".into());
        Ok(())
    }

    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        self.dialogs.push("bringupbox".into());
        Ok(())
    }

    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[&[u8]],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let items: Vec<String> = items.iter().map(|i| show(i)).collect();
        self.dialogs.push(format!(
            "listbox {} [{}] sel={selected} {opts:?}",
            show2(text, title),
            items.join(", ")
        ));
        Ok(self.list_replies.pop_front().unwrap_or(DialogEnd::Cancel))
    }

    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let name = if password { "passwordbox" } else { "inputbox" };
        self.dialogs
            .push(format!("{name} {} {}", show2(text, title), show(default)));
        Ok(self.input_replies.pop_front().unwrap_or(DialogEnd::Cancel))
    }

    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        self.dialogs.push(format!(
            "filenamebox {} save={} {}",
            show(title),
            save as u8,
            show(init_dir)
        ));
        Ok(self.file_replies.pop_front().flatten())
    }

    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        self.dialogs
            .push(format!("dirnamebox {} {}", show(title), show(init_dir)));
        Ok(self.file_replies.pop_front().flatten())
    }

    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        self.dialogs.push(match &pos {
            Some(p) => format!("setdlgpos {p:?}"),
            None => "setdlgpos default".into(),
        });
        self.dialog_pos = pos;
    }

    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        self.terminal.push(format!("beep {sound:?}"));
        Ok(())
    }

    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        self.terminal.push(format!("callmenu {id}"));
        Ok(())
    }

    fn set_transfer_dir(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.terminal.push(format!("changedir {}", show(path)));
        Ok(())
    }

    fn clear_screen(&mut self, what: ClearScreen) -> Result<(), TtlError> {
        self.terminal.push(format!("clearscreen {what:?}"));
        Ok(())
    }

    fn enable_keyboard(&mut self, on: bool) -> Result<(), TtlError> {
        self.terminal.push(format!("enablekeyb {}", on as u8));
        Ok(())
    }

    fn load_key_map(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.terminal.push(format!("loadkeymap {}", show(path)));
        Ok(())
    }

    fn restore_setup(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.terminal.push(format!("restoresetup {}", show(path)));
        Ok(())
    }

    fn set_debug_mode(&mut self, mode: DebugMode) -> Result<(), TtlError> {
        self.terminal.push(format!("setdebug {mode:?}"));
        Ok(())
    }

    fn set_local_echo(&mut self, on: bool) -> Result<(), TtlError> {
        self.terminal.push(format!("setecho {}", on as u8));
        Ok(())
    }

    fn set_title(&mut self, title: &[u8]) -> Result<(), TtlError> {
        self.terminal.push(format!("settitle {}", show(title)));
        Ok(())
    }

    fn title(&mut self) -> Result<Vec<u8>, TtlError> {
        self.terminal.push("gettitle".into());
        Ok(self.title.clone())
    }

    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        self.terminal.push(format!("showtt {which:?}"));
        Ok(())
    }

    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        self.terminal.push(format!("show {how:?}"));
        Ok(())
    }

    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        self.terminal.push("getttpos".into());
        Ok(self.geometry)
    }

    fn set_serial_delay(&mut self, per_line: bool, ms: i32) -> Result<bool, TtlError> {
        let which = if per_line { "line" } else { "char" };
        self.terminal.push(format!("serialdelay {which} {ms}"));
        Ok(!self.serial_delay_fails)
    }

    fn log_open(&mut self, req: &LogOpen<'_>) -> Result<bool, TtlError> {
        self.logs.push(format!(
            "logopen {} binary={} append={} plain={} ts={} hide={} screen={} clock={:?}",
            show(req.path),
            req.binary as u8,
            req.append as u8,
            req.plain_text as u8,
            req.timestamp as u8,
            req.hide_dialog as u8,
            req.include_screen as u8,
            req.timestamp_type,
        ));
        Ok(!self.log_open_fails)
    }

    fn log_close(&mut self) -> Result<(), TtlError> {
        self.logs.push("logclose".into());
        Ok(())
    }

    fn log_pause(&mut self, paused: bool) -> Result<(), TtlError> {
        self.logs
            .push(if paused { "logpause" } else { "logstart" }.into());
        Ok(())
    }

    fn log_write(&mut self, s: &[u8]) -> Result<(), TtlError> {
        self.logs.push(format!("logwrite {}", show(s)));
        Ok(())
    }

    fn log_info(&mut self) -> Result<Option<LogInfo>, TtlError> {
        self.logs.push("loginfo".into());
        Ok(self.log_info.clone())
    }

    fn log_rotate(&mut self, how: LogRotate) -> Result<(), TtlError> {
        self.logs.push(format!("logrotate {how:?}"));
        Ok(())
    }

    fn log_auto_close(&mut self, on: bool) -> Result<(), TtlError> {
        self.logs.push(format!("logautoclosemode {}", on as u8));
        Ok(())
    }
}

/// A dialog string as a test wants to read it. Lossy on purpose — a TTL string
/// is bytes and need not be UTF-8, and a record nobody can print is no use.
fn show(s: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(s))
}

fn show2(text: &[u8], title: &[u8]) -> String {
    format!("{} {}", show(text), show(title))
}
