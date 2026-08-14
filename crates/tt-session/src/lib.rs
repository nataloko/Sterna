//! tt-session — a terminal attached to a connection.
//!
//! `tt-vt` turns bytes into a grid and keys into bytes. `tt-conn` moves bytes.
//! Neither knows about the other, and something has to own the loop between
//! them; that is this, and it is what the C ABI will export more or less
//! directly. A frontend deals with a [`Session`] and never with a `Vt`.
//!
//! ```no_run
//! use std::time::Duration;
//! use tt_conn::serial::{SerialConn, SerialParams};
//! use tt_session::{Event, Session};
//!
//! let port = SerialConn::open("/dev/ttyUSB0", &SerialParams::default())?;
//! let mut session = Session::new(Default::default());
//! session.connect(Box::new(port));
//!
//! session.pump(Duration::from_millis(50))?;
//! for event in session.drain_events() {
//!     if let Event::Title(t) = event {
//!         println!("title: {t}");
//!     }
//! }
//! # Ok::<(), tt_conn::Error>(())
//! ```
//!
//! **Nothing here spawns a thread.** [`Session::pump`] blocks for as long as
//! the caller allows and no longer, so where the loop runs is the frontend's
//! decision — a Qt worker thread, a tokio task once SSH arrives, or a test's
//! main thread. Baking a runtime in before the second transport exists would
//! be guessing at the shape of a problem we have not met.

use std::borrow::Cow;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod bell;
pub mod highlight;
pub mod log;
pub mod logname;
pub mod macros;
pub mod open;
mod serial;
pub mod settings;
pub mod xfer;

pub use bell::{BellGovernor, BellLimits};
pub use log::{LogMode, LogOptions, SessionLog, Timestamp};
pub use macros::{MacroLink, MACRO_BUF_SIZE};
use settings::cr_send_of;
pub use settings::{log_options, vt_config};
pub use xfer::{
    job_defaults, xfer_options, JobDefaults, TransferError, TransferOutcome, TransferReply,
    TransferStatus,
};
// Re-exported rather than reached for directly, so that a frontend — the C ABI
// above all — takes the settings and the metadata that describes them from the
// same place it takes the session they belong to.
pub use tt_config::buttons;
use tt_config::ConnectionTcpCrSend;
pub use tt_config::{
    Button, Field, Ini, KeyboardMap, Kind, Settings, Shortcut, UserKey, UserKeyType, FIELDS,
};
pub use tt_vt::DebugMode;
pub use tt_vt::{PrinterEvent, WindowMetrics, WindowRequest};

use tt_conn::{Error, Result, Transport, TransportEvent};
use tt_grid::{Cell, Grid, ATTR_LINE_CONTINUED, ATTR_URL, WIDTH_PAD, WIDTH_WIDE};
use tt_vt::{ClipboardRequest, Config, CrSend, Key, Modifiers, MouseEvent, Tracking, Vt};

/// Which half of the terminal byte stream a plugin filter is transforming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamDirection {
    Input,
    Output,
}

/// Bytes and non-fatal failures from one stream-filter pass.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamFilterResult {
    pub bytes: Vec<u8>,
    pub errors: Vec<String>,
}

/// A synchronous, bounded transform installed at the terminal stream seam.
///
/// Implementations own their runtime and must not wait on the terminal or a
/// window: input filtering runs between a transport read and VT parsing, and
/// output filtering runs before bytes enter the pending write queue. A failure
/// belongs in [`StreamFilterResult::errors`]; the implementation should pass
/// the corresponding bytes through so one bad extension cannot cut the line.
pub trait StreamFilter: Send {
    fn filter(&mut self, direction: StreamDirection, bytes: &[u8]) -> StreamFilterResult;
}

/// Something the frontend needs to know about. Drained, not delivered: a
/// callback would have to be `Send` and would run on whichever thread the
/// pump happens to be on, which is exactly what a UI toolkit cannot take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The screen changed. Coarse on purpose — a full 80x24 repaint measured
    /// 3.9 ms on the target Qt (`PLAN.md`), roughly 40x what a 115200 baud
    /// link can dirty, so per-row damage is an optimisation to add when
    /// something says it is needed rather than a thing to design around now.
    Damage,
    /// Something visible in the window caption changed: OSC 0 / OSC 2, or a
    /// serial speed which `TitleFormat` asks the frontend to include.
    Title(String),
    /// A line break arrived from the far end. On a serial console this is how
    /// a host asks for attention, and dropping it is a real loss of function.
    Break,
    /// A byte arrived with a parity or framing error.
    BadByte(u8),
    /// The transport went away — unplugged, hung up, or the child exited.
    Disconnected,
    /// `AutoWinClose`, after a network connection ended. The core cannot
    /// close a window, so the frontend which owns one does it. Serial ports
    /// and local ptys never produce this request.
    CloseRequested,
    /// The **far end** says the terminal should be this size. Resize the
    /// window to suit, which is what upstream does — `buffer.c:5106` goes
    /// through the window rather than round it.
    ///
    /// Two sources, and the difference is which way the grid has already moved:
    ///
    /// - **Telnet's NAWS**, backwards from the usual direction and real: RFC
    ///   1073 defines it client-to-server, and a console server sends it the
    ///   other way to say what the equipment behind it actually is. The grid
    ///   has **not** moved — the window owns its own size, and a core that
    ///   silently changed it would leave the frontend painting the wrong
    ///   number of cells. Honouring it is the frontend's decision.
    /// - **`CSI 8 ; rows ; cols t`**, where the grid **has** already moved,
    ///   because upstream's `ChangeTerminalSize` resizes there too. A window
    ///   that does not follow paints more cells than it has room for until its
    ///   next resize event undoes the change.
    ///
    /// A frontend does the same thing either way: resize the window and let
    /// its own resize handler settle the grid. That is why they are one event
    /// and not two.
    Resize { cols: u16, rows: u16 },
    /// The session log could not be written and has been closed. Reported
    /// once: a disk that filled up will not un-fill, and retrying on every
    /// pump turns one problem into a stall.
    LogFailed(String),
    /// A plugin's byte-stream filter failed and was bypassed. `String` names
    /// the plugin and the error; bytes still crossed the seam unchanged.
    StreamFilterFailed(String),
    /// A file transfer moved. Emitted once per pump while one is running,
    /// which is as often as anything changes.
    ///
    /// Boxed because it is much the largest variant and every other event
    /// would otherwise carry its footprint — and `Damage`, the one that costs
    /// nothing to produce, is the one produced most.
    TransferProgress(Box<TransferStatus>),
    /// A file transfer ended, for any reason: finished, cancelled, refused by
    /// the peer, or cut off by the connection going away.
    TransferDone(Box<TransferOutcome>),
    /// The terminal wants a bell. Already governed, so this is a bell that
    /// upstream would have made a noise for — a burst of BELs in one read
    /// arrives as at most one of these, because two beeps in the same
    /// millisecond are one beep.
    ///
    /// `visual` is `bell.mode` being `visual`: invert the screen for
    /// `bell.visual_wait_ms` rather than making a sound. The frontend owns
    /// both, since the core has neither a speaker nor a window.
    Bell { visual: bool },
    /// OSC 52, parsed and authorised by the terminal but waiting for the
    /// frontend which owns the operating system clipboard.
    Clipboard(ClipboardRequest),
    /// `OSC 4`/`5`/`10`-`19` or one of their resets moved a colour the painter
    /// caches. Re-read `tt_session_palette_rgb` and `tt_session_color_rgb`.
    ///
    /// Separate from [`Event::Damage`], which says the *cells* changed, because
    /// a colour change is rare and re-reading 262 values on every pump would
    /// pay for it constantly. Upstream's equivalent is the `InvalidateRect` at
    /// the end of `DispSetColor`.
    ColorsChanged,
    /// `CSI 1`-`10 t` asked the window to iconify, move, resize in pixels,
    /// raise, lower, repaint or maximise.
    ///
    /// The core has no window, so this is a request and not a report. A
    /// frontend that cannot honour one — a Wayland client asked to move, which
    /// has no protocol request for it — should drop it rather than pretend:
    /// the terminal answers `CSI 13 t` from what the frontend last said its
    /// window was, so a lie here becomes a lie on the wire.
    WindowRequest(WindowRequest),
    /// `CSI Ps i` and `CSI ? Ps i` asked the printer for something. See
    /// [`PrinterEvent`].
    ///
    /// A frontend with no printer drops these, and dropping them is a complete
    /// answer: nothing in the terminal waits on a job, and the host is told
    /// nothing either way. What it must not do is act on a `Write` without
    /// having seen its `Open`.
    Printer(PrinterEvent),
}

/// What pressing a legacy `KEYBOARD.CNF` scan code did.
///
/// Bytes are sent here, where the live terminal modes are. Everything else is
/// handed back to the frontend which owns the window, clipboard, menus and
/// macro runner. A mapped action which this build cannot perform is
/// [`Ignored`](KeyCodeResult::Ignored), distinct from an unassigned physical
/// key so `StrictKeyMapping` can suppress the built-in fallback correctly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyCodeResult {
    Unmapped,
    Sent,
    /// Hold, Print or Break — `Key` variants with no wire sequence.
    LocalKey(Key),
    /// A DEC user-defined key whose live definition belongs to the VT parser.
    Udk(u8),
    Shortcut(Shortcut),
    RunMacro(String),
    Command(u16),
    Ignored,
}

/// A terminal, and optionally something for it to talk to.
pub struct Session {
    vt: Vt,
    conn: Option<Box<dyn Transport>>,
    events: Vec<Event>,
    /// Reused across pumps so a busy line does not allocate per read.
    rx: Vec<u8>,
    rx_events: Vec<TransportEvent>,
    /// Bytes bound for the far end that a short write left behind.
    pending: Vec<u8>,
    last_title: String,
    write_timeout: Duration,
    /// The open session log, if any. A tap on the same byte stream, not a
    /// second one — see [`Session::start_log`].
    log: Option<SessionLog>,
    /// Lines scrolled back from the live screen; 0 is live.
    view_offset: usize,
    /// `Grid::scrolled_off` as of the last time the view was reconciled. The
    /// difference is how far the content moved under a scrolled-back viewer.
    seen_scrolled_off: u64,
    /// The grid's size as of the same moment — see [`Session::follow_scroll`].
    seen_size: (usize, usize),
    /// What the last transport said on its way out, if it said anything.
    close_note: Option<String>,
    /// Every setting, including the ones this layer does not act on — see
    /// [`Session::settings`].
    settings: Settings,
    /// The active `KEYBOARD.CNF`, empty until one is loaded. The frontend
    /// still supplies its built-in semantic key when no physical binding
    /// exists; `StrictKeyMapping` decides whether that fallback is allowed.
    key_map: KeyboardMap,
    /// `TCPLocalEchoUsed` and `TCPCRSendUsed` (`vtwin.cpp:140`): whether this
    /// connection spent the terminal's own value and so owes it back. See
    /// [`Session::tcp_echo_cr_override`].
    tcp_local_echo_used: bool,
    tcp_cr_send_used: bool,
    /// The file transfer that owns the byte stream, if one is running.
    xfer: Option<xfer::Running>,
    /// Where the running transfer's outcome goes besides the event queue, for
    /// a caller on another thread that is blocked on it. See
    /// [`Session::notify_transfer`].
    xfer_reply: Option<TransferReply>,
    /// The linked macro's byte ring — `Some` for exactly as long as one is
    /// linked, which is what turns the tap in `tt-vt` on. See
    /// [`Session::link_macro`].
    macro_link: Option<MacroLink>,
    /// The Lua plugin worker's independent byte ring.
    ///
    /// Plugins live for the session rather than for one macro run, and a macro
    /// started from the menu must not steal their receive stream. Both rings
    /// are fed from the parser's one tap in [`Session::macro_bytes_in`].
    plugin_link: Option<MacroLink>,
    /// The window's fast-path plugin filters. Kept separate from the plugin
    /// receive ring: `tt.unlink()` detaches a callback from the ring but must
    /// not silently remove a window-wide input/output policy.
    stream_filter: Option<Box<dyn StreamFilter>>,
    /// `cv.ConnectedTime` (`commlib.c:787`) — when the current connection
    /// opened, for the one log timestamp that counts from there rather than
    /// from the log. `None` until something connects.
    connected_at: Option<Instant>,
    /// `ts.HostName` and `ts.TCPPort`, as far as a log name is concerned. See
    /// [`Session::set_connection_name`].
    conn_host: Option<String>,
    conn_port: Option<u16>,
    /// `RingBell`'s three statics — see [`bell`].
    bell: BellGovernor,
    /// The compiled highlight rules — see [`highlight`]. Empty until a
    /// frontend hands some over, and asked only while painting.
    highlights: highlight::Matcher,
    /// Moved by [`Session::mark_damage`], which is what makes the memo below
    /// safe to keep between calls: everything that changes the grid also tells
    /// the frontend to repaint.
    damage_epoch: u64,
    /// The last logical line matched, so a wrapped line is scanned once per
    /// frame rather than once per row it occupies.
    highlight_memo: highlight::Memo,
    /// Scratch for [`Session::row_highlights`], kept across calls so painting
    /// a screen does not allocate once per row.
    highlight_flat: highlight::Flattened,
    highlight_styles: Vec<highlight::Style>,
}

impl Session {
    pub fn new(config: Config) -> Session {
        let vt = Vt::new(config);
        Session {
            last_title: vt.window_title(),
            vt,
            conn: None,
            events: Vec::new(),
            rx: Vec::new(),
            rx_events: Vec::new(),
            pending: Vec::new(),
            write_timeout: Duration::from_millis(200),
            log: None,
            view_offset: 0,
            seen_scrolled_off: 0,
            seen_size: (0, 0),
            tcp_local_echo_used: false,
            tcp_cr_send_used: false,
            close_note: None,
            settings: Settings::default(),
            key_map: KeyboardMap::default(),
            xfer: None,
            xfer_reply: None,
            macro_link: None,
            plugin_link: None,
            stream_filter: None,
            connected_at: None,
            conn_host: None,
            conn_port: None,
            bell: BellGovernor::default(),
            highlights: highlight::Matcher::default(),
            damage_epoch: 0,
            highlight_memo: highlight::Memo::default(),
            highlight_flat: highlight::Flattened::default(),
            highlight_styles: Vec::new(),
        }
    }

    /// A session configured from `TERATERM.INI` rather than from the
    /// terminal's own defaults.
    ///
    /// The two agree today — `settings::tests` asserts it, because a
    /// disagreement means one of them is wrong about upstream — so this is
    /// about where the truth *lives*, not about changing any value.
    pub fn from_settings(settings: Settings) -> Session {
        let mut session = Session::new(vt_config(&settings, &Config::default()));
        session.settings = settings;
        session
    }

    /// Every setting, the terminal's and the window's alike.
    ///
    /// The session keeps them because it is the thing both sides of the C ABI
    /// can reach: the frontend asks by name for the ones it draws with — the
    /// colour pairs, the word delimiters — and the terminal takes the ones it
    /// runs on. Two copies would be two things to keep in step.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Replace them and apply what a running terminal can take.
    ///
    /// **This overwrites modes the host set**, because upstream's settings
    /// *are* those modes — see [`tt_vt::Vt::set_config`]. It also resizes: the
    /// grid takes the new size, the far end is told, and a scrolled-back view
    /// goes live, since a resize moves lines between the page and the history
    /// in both directions.
    ///
    /// Everything the core does not read is still stored, which is the point:
    /// a setting with no subsystem yet has to survive being written back to
    /// the file it came from.
    ///
    /// **It cannot fail, and the missing `Result` is the decision rather than
    /// an omission.** Applying settings is local: the only call under here
    /// that can refuse is the one telling the far end its new size, and by the
    /// time it runs everything else has already happened. Returning that
    /// error made the call say "this did not work" about a call that had
    /// worked — which is how a `/W=` setting a *title* came back reporting
    /// `ResizePseudoConsole`, and sent the reader looking at the command line.
    /// The two failures behind it are both answered elsewhere: a link that has
    /// really gone reports [`Event::Disconnected`] at the next pump, and a
    /// platform without the call is not this session's news to break. Upstream
    /// has no error path here at all, and both other callers of the same
    /// `resize` — [`Session::connect`] and the macro host's `connect` — had
    /// already discarded it on their own.
    pub fn set_settings(&mut self, settings: Settings) {
        let config = vt_config(&settings, self.vt.config());
        self.settings = settings;
        self.vt.set_config(config);
        // `set_config` may have resized the grid, which is the one thing here
        // that moves the viewport's anchor: the offset would otherwise point
        // at a line that has moved between the page and the history.
        self.reanchor_after_resize();
        self.mark_damage();
        // Here rather than only in the pump: applying settings rebuilds the
        // live colours, and a session with nothing arriving on it would
        // otherwise not pump again until it did.
        self.collect_colors();
        let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
        if let Some(c) = self.conn.as_mut() {
            let _ = c.resize(cols as u16, rows as u16);
        }
    }

    /// What ends a word, for a double-click — `ts.DelimListW` (`ttset.c:1171`).
    ///
    /// Decoded here rather than read by name, because `DelimList` is stored in
    /// `Hex2StrW`'s `$xx` escape and its own default *opens* with one: the raw
    /// value begins `$20!"#$24%`, so a frontend that took the string as it
    /// stands would have a list with no space in it, a literal `$`, `2` and
    /// `0`, and every word running into the next.
    ///
    /// Characters rather than bytes: this is compared against what is on the
    /// screen. See [`tt_config::hex_decode_str`] for why upstream has two
    /// decoders.
    pub fn word_delimiters(&self) -> String {
        tt_config::hex_decode_str(&self.settings.keyboard_word_delimiters)
    }

    /// One setting by name, in the INI's own spelling. `None` for a name that
    /// is not in the schema.
    pub fn setting(&self, name: &str) -> Option<String> {
        self.settings.get_str(name)
    }

    /// Set one by name and apply it. False for a name that is not ours.
    ///
    /// The value is parsed exactly as the file would parse it, bounds and
    /// default-biased booleans included, so a script and a hand-edited
    /// `TERATERM.INI` cannot disagree about what a value means. An
    /// out-of-range number is therefore not a failure — it lands where the
    /// file would put it — and a name that is not in the schema is the only
    /// thing this can refuse. See [`Session::set_settings`] for why applying
    /// itself has no error to report.
    pub fn set_setting(&mut self, name: &str, value: &str) -> bool {
        let mut settings = self.settings.clone();
        if !settings.set_str(name, value) {
            return false;
        }
        self.set_settings(settings);
        true
    }

    /// Attach a connection. The terminal is not reset: reconnecting a serial
    /// console to the same session is how you keep the scrollback that
    /// explains why it dropped.
    pub fn connect(&mut self, conn: Box<dyn Transport>) {
        self.pending.clear();
        self.close_note = None;
        self.connected_at = Some(Instant::now());
        // Upstream reads `cv.ConnectedTime` at every stamp rather than at the
        // log's open, so a log left running across a reconnect restarts its
        // connection clock. Reproduced here by moving the origin, since the
        // log holds an instant rather than asking for one.
        self.sync_log_epoch();
        let kind = conn.link_kind();
        self.conn = Some(conn);
        let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
        if let Some(c) = self.conn.as_mut() {
            let _ = c.resize(cols as u16, rows as u16);
        }
        self.clear_com_buff_on_open();
        self.tcp_echo_cr_override();
        self.connect_beep(kind);
    }

    /// `TCPLocalEcho` and `TCPCRSend` — the two settings a TCP connection that
    /// is not telnet applies to the *terminal* (`vtwin.cpp:3690`).
    ///
    /// Both are overrides rather than settings beside the terminal's own: they
    /// assign `ts.LocalEcho` and `ts.CRSend`, which the file already had a
    /// value for, and `:3589` puts the file's value back when the connection
    /// closes. Upstream keeps a second copy — `ts.LocalEcho_ini`,
    /// `ts.CRSend_ini` — for exactly that; here [`Session::settings`] *is* that
    /// copy, since it holds the file rather than the live terminal.
    ///
    /// **Off is not a value.** Neither key applies when it is off, so the
    /// terminal keeps whatever it had; that is why the two `_used` flags exist
    /// rather than restoring unconditionally, and why a host's `ESC [ 12 h`
    /// survives a disconnect on a session where `TCPLocalEcho` was never set.
    fn tcp_echo_cr_override(&mut self) {
        if !self.conn.as_ref().is_some_and(|c| c.tcp_without_telnet()) {
            return;
        }
        if self.settings.connection_tcp_local_echo {
            self.tcp_local_echo_used = true;
            self.vt.set_local_echo(true);
        }
        let cr = match self.settings.connection_tcp_cr_send {
            ConnectionTcpCrSend::Disabled => None,
            ConnectionTcpCrSend::Cr => Some(CrSend::Cr),
            ConnectionTcpCrSend::CrLf => Some(CrSend::CrLf),
        };
        if let Some(cr) = cr {
            self.tcp_cr_send_used = true;
            self.vt.set_cr_send(cr);
        }
    }

    /// Put the file's values back, which is `FD_CLOSE`'s half of the pair
    /// above (`vtwin.cpp:3588`).
    fn restore_tcp_echo_cr(&mut self) {
        if std::mem::take(&mut self.tcp_local_echo_used) {
            self.vt.set_local_echo(self.settings.terminal_local_echo);
        }
        if std::mem::take(&mut self.tcp_cr_send_used) {
            self.vt
                .set_cr_send(cr_send_of(self.settings.terminal_cr_send));
        }
    }

    /// Give the transport the wakeup a quiet line cannot give it — telnet's
    /// keepalive, and nothing else so far. Cheap on every other transport.
    ///
    /// Separate from [`Session::pump`] because the whole point is that it runs
    /// when *nothing* arrived; a frontend drives it from a timer.
    pub fn tick(&mut self) -> Result<()> {
        match self.conn.as_mut() {
            Some(c) => c.tick(),
            None => Ok(()),
        }
    }

    /// `ClearComBuffOnOpen` — `CommOpen` purges the driver's queues as the
    /// port opens (`commlib.c:475`), and this is the same moment: nothing has
    /// read from the transport yet.
    ///
    /// The error is dropped because upstream's is not checked either, and
    /// because a purge that failed leaves the session with more data than
    /// asked for rather than less. When the setting is off, upstream marks the
    /// port readable instead (`:477`'s `cv->RRQ`), which here needs nothing —
    /// the frontend's notifier will find the bytes on its own.
    fn clear_com_buff_on_open(&mut self) {
        if !self.settings.serial_clear_buffer_on_open {
            return;
        }
        if let Some(port) = self.conn.as_mut().and_then(|c| c.as_serial()) {
            let _ = port.clear(true, true);
        }
    }

    /// `BeepOnConnect` — `vtwin.cpp:3658` on the way in and `:3018` on the way
    /// out, both of them a bare `MessageBeep(0)`.
    ///
    /// It bypasses `RingBell` entirely, so it is always audible, never the
    /// visual bell, and the governor neither thins it nor is stepped by it.
    ///
    /// Upstream's condition is `PortType == IdTCPIP`, and it is written here as
    /// "not a serial port" because its three port types are serial, TCP and a
    /// file: the case that separates the two readings does not exist. A local
    /// pty is the one link upstream has no word for, and it beeps — CygTerm,
    /// which is the same thing there, reaches Tera Term over a TCP socket and
    /// beeps for that reason.
    fn connect_beep(&mut self, kind: tt_conn::LinkKind) {
        if self.settings.bell_on_connect && !matches!(kind, tt_conn::LinkKind::Serial { .. }) {
            self.events.push(Event::Bell { visual: false });
        }
    }

    /// Point an open [`Timestamp::ElapsedConnection`] log at the current
    /// connection. A no-op for every other timestamp, and when nothing has
    /// connected — see that variant for what upstream prints instead.
    fn sync_log_epoch(&mut self) {
        let (Some(at), Some(log)) = (self.connected_at, self.log.as_mut()) else {
            return;
        };
        if log.options().timestamp == Timestamp::ElapsedConnection {
            log.set_elapsed_origin(at);
        }
    }

    pub fn disconnect(&mut self) {
        let kind = self.conn.as_ref().map(|c| c.link_kind());
        self.conn = None;
        self.pending.clear();
        self.restore_tcp_echo_cr();
        // A transfer has to be told here rather than on the next pump: with
        // no connection left, `pump` returns before it reaches anything, so
        // the transfer would sit "running" for ever with a progress dialog
        // on top of it and no way to reach the end.
        self.transfer_disconnected();
        if let Some(kind) = kind {
            self.connection_closed(kind);
        }
        // `close_note` is deliberately not cleared: the user disconnecting is
        // not a reason to forget why the *last* connection ended, and the note
        // is what the status line is showing.
    }

    /// Why the last connection ended, when the transport had something to add
    /// beyond "disconnected" — "bash exited with status 1" from a local shell.
    ///
    /// Set alongside [`Event::Disconnected`] and cleared by the next
    /// [`connect`](Session::connect). `None` means nothing more is known,
    /// which is the usual case: an unplugged adapter and a closed socket are
    /// what they look like.
    pub fn close_note(&self) -> Option<&str> {
        self.close_note.as_deref()
    }

    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    pub fn describe(&self) -> Option<String> {
        self.conn.as_ref().map(|c| c.describe())
    }

    /// How many bytes are still waiting for the far end.
    ///
    /// Non-zero means flow control held the line — CTS low, an XOFF, a DSR
    /// that dropped — and a short write left the rest here for the next
    /// [`pump`](Session::pump). A frontend has to know, because the pump it is
    /// waiting for may never come: [`poll_fd`](Session::poll_fd) wakes on
    /// *incoming* bytes, and a device asserting backpressure is usually not
    /// sending any. Without this the keystrokes sit in the queue until the
    /// host happens to say something, which reads as dropped input.
    ///
    /// The intended use is a retry timer that exists only while this is
    /// non-zero, so the idle case still costs nothing.
    pub fn pending_out(&self) -> usize {
        self.pending.len()
    }

    /// A descriptor that becomes readable when [`pump`](Session::pump) has
    /// something to do, so a frontend can wait in its own event loop instead
    /// of polling this one.
    ///
    /// This is the whole reason the shell does not have a timer. `pump` blocks
    /// for the transport's read timeout, so calling it on a UI thread freezes
    /// the window for as long as the line is quiet — which is most of the time
    /// on a serial console — and calling it on a timer instead trades that for
    /// a wakeup every frame, forever, to learn that nothing happened. Handing
    /// the descriptor out lets the toolkit wait properly and then call
    /// `pump(Duration::ZERO)`, which reads exactly once and returns.
    ///
    /// **Re-read it after every connect and disconnect.** It belongs to the
    /// transport, not to the session, so it changes when the transport does
    /// and is `None` when there is none.
    #[cfg(unix)]
    pub fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        self.conn.as_ref().and_then(|c| c.poll_fd())
    }

    /// The Windows event that becomes signalled when [`pump`](Session::pump)
    /// has something to do. This is the native spelling of
    /// [`poll_fd`](Session::poll_fd), with the same borrowed lifetime.
    #[cfg(windows)]
    pub fn wait_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        self.conn.as_ref().and_then(|c| c.wait_handle())
    }

    pub fn grid(&self) -> &Grid {
        self.vt.grid()
    }

    /// The terminal itself, for the settings and mode accessors the frontend
    /// reads — cursor visibility, bracketed paste, reverse video.
    pub fn vt(&self) -> &Vt {
        &self.vt
    }

    /// Tell the terminal what its window is, so `CSI 11`/`13`/`14`/`15`/`16`
    /// `/19 t` can answer. See [`WindowMetrics`].
    ///
    /// Push on every move, resize and window-state change. A frontend with no
    /// window never calls this and gets the notional one, which is what
    /// `tt-host` and the oracle both report from.
    pub fn set_window_metrics(&mut self, metrics: WindowMetrics) {
        self.vt.set_window_metrics(metrics);
    }

    /// TTL's `setdebug`: select the receive display directly, independently
    /// of the keyboard shortcut's setting and allowed-mode list.
    pub fn set_debug_mode(&mut self, mode: DebugMode) {
        self.vt.set_debug_mode(mode);
    }

    /// Shift+Escape. True when the settings made it a debug key.
    pub fn cycle_debug_mode(&mut self) -> bool {
        self.vt.cycle_debug_mode()
    }

    /// `settitle` — `ts.Title`, and a title event if the window's changed.
    ///
    /// Narrow rather than a `vt_mut()`, because the title is the one piece of
    /// terminal state the session watches for an edge: anything that could set
    /// it behind [`Session::collect_title`] would leave the title bar showing
    /// the previous one until the host next printed something.
    pub fn set_title(&mut self, title: String) {
        self.vt.set_title(title);
        self.collect_title();
    }

    /// One row of what the window is *showing*, which is the live screen until
    /// something scrolls back. Borrowed, not copied.
    ///
    /// `y` runs 0..rows over the viewport, not over the grid: with an offset of
    /// `n`, row 0 is `n` lines up in the scrollback and the bottom `n` rows of
    /// the live screen are off the bottom. Every line is `cols` wide, including
    /// the retained ones — `Grid::resize` refits the scrollback alongside the
    /// page.
    pub fn row(&self, y: usize) -> &[Cell] {
        self.line(self.line_at(y)).unwrap_or(&[])
    }

    /// The absolute line number of the top of the *live* page.
    ///
    /// Which is exactly [`Grid::scrolled_off`], and true by construction: every
    /// scroll pushes one line off the top and increments it by one. That makes
    /// it the origin the whole numbering below is measured from.
    pub fn top_line(&self) -> u64 {
        self.vt.grid().scrolled_off()
    }

    /// The absolute line number shown at viewport row `y`.
    ///
    /// This is the name a frontend needs for anything it wants to *keep* across
    /// output — a selection, most obviously. Viewport rows and grid rows both
    /// mean "wherever this line has slid to since", so a highlight held in
    /// either walks up the screen as the host prints.
    pub fn line_at(&self, y: usize) -> u64 {
        // `view_offset` is clamped to the scrollback, which the grid asserts is
        // never longer than `scrolled_off`, so this cannot go negative.
        self.top_line() - self.view_offset() as u64 + y as u64
    }

    /// One line of the buffer by absolute number, scrollback and page alike.
    ///
    /// `None` once the line has been evicted from the scrollback, or for one
    /// that has not been printed yet — both of which a frontend holding an old
    /// number has to be able to ask about without guessing at the range first.
    pub fn line(&self, line: u64) -> Option<&[Cell]> {
        self.vt.grid().absolute_line(line)
    }

    /// The AttrURL run containing one cell, by absolute line number.
    ///
    /// `buffer.c:invokeBrowserW` does not parse the text again. It walks the
    /// already-marked cells in both directions, crossing only automatic line
    /// continuations, and hands that exact run to the launcher. Doing the same
    /// here preserves upstream's incremental-marking edges instead of silently
    /// turning them into a different URL detector at click time.
    ///
    /// The surprising `CR CR LF` in a split URL is upstream too:
    /// `BuffGetStringForCB` is shared with copying, so it joins an automatic
    /// wrap only when `EnableContinuedLineCopy` is on. The setting ships off.
    pub fn url_at(&self, line: u64, x: usize) -> Option<String> {
        let cell = self.line(line)?.get(x)?;
        if cell.attrs & ATTR_URL == 0 || cell.width_class == WIDTH_PAD {
            return None;
        }

        let mut first = (line, x);
        while let Some(previous) = self.previous_buffer_cell(first.0, first.1) {
            let cell = &self.line(previous.0)?[previous.1];
            if cell.attrs & ATTR_URL == 0 {
                break;
            }
            first = previous;
        }

        let mut last = (line, x);
        while let Some(next) = self.next_buffer_cell(last.0, last.1) {
            let cell = &self.line(next.0)?[next.1];
            if cell.attrs & ATTR_URL == 0 {
                break;
            }
            last = next;
        }

        let mut out = String::new();
        let mut at = first;
        loop {
            let cell = &self.line(at.0)?[at.1];
            for cp in cell.codepoints() {
                out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
            }
            if at == last {
                break;
            }
            let next = self.next_buffer_cell(at.0, at.1)?;
            if next.0 != at.0 && !self.settings.clipboard_continued_line_copy {
                // Yes, two CRs. `buffer.c:1889` writes precisely that before
                // its LF, and URL launch inherits the clipboard routine.
                out.push_str("\r\r\n");
            }
            at = next;
        }
        Some(out)
    }

    fn previous_buffer_cell(&self, line: u64, x: usize) -> Option<(u64, usize)> {
        let current = self.line(line)?;
        if x >= current.len() {
            return None;
        }
        let (line, mut x) = if x > 0 {
            (line, x - 1)
        } else {
            if current.first()?.attrs & ATTR_LINE_CONTINUED == 0 {
                return None;
            }
            let line = line.checked_sub(1)?;
            (line, self.line(line)?.len().checked_sub(1)?)
        };
        let cells = self.line(line)?;
        while cells[x].width_class == WIDTH_PAD {
            x = x.checked_sub(1)?;
        }
        Some((line, x))
    }

    fn next_buffer_cell(&self, line: u64, x: usize) -> Option<(u64, usize)> {
        let cells = self.line(line)?;
        let cell = cells.get(x)?;
        let step = if cell.width_class == WIDTH_WIDE { 2 } else { 1 };
        if x + step < cells.len() {
            return Some((line, x + step));
        }
        if cells.last()?.attrs & ATTR_LINE_CONTINUED == 0 {
            return None;
        }
        let line = line.checked_add(1)?;
        self.line(line)?.first()?;
        Some((line, 0))
    }

    // --- highlight rules ----------------------------------------------------

    /// Tell the frontend the cells changed, and retire anything derived from
    /// them.
    ///
    /// Every path that edits the grid comes through here, because every one of
    /// them has to ask for a repaint — which is what makes
    /// [`Session::row_highlights`]' memo safe to keep between calls without a
    /// per-line dirty flag the grid does not have.
    fn mark_damage(&mut self) {
        self.damage_epoch = self.damage_epoch.wrapping_add(1);
        self.events.push(Event::Damage);
    }

    /// Compile a rule list and use it from the next repaint.
    ///
    /// Compiling here rather than per frame is the whole cost of the feature:
    /// what a paint does is run an already-built automaton over the rows it is
    /// drawing.
    pub fn set_highlights(&mut self, rules: &[tt_config::highlight::Rule]) {
        self.highlights = highlight::Matcher::new(rules);
        self.mark_damage();
    }

    /// The rules that would not compile, for a frontend to say so once.
    ///
    /// A pattern only reaches this by being hand-edited into the file — the
    /// editor will not save one — and silence would leave somebody looking at
    /// a rule that is in the file and does nothing.
    pub fn highlight_rejected(&self) -> &[highlight::Rejected] {
        self.highlights.rejected()
    }

    /// What to recolour on viewport row `y`, in column order.
    ///
    /// Borrowed, and valid until the next call — the same contract as
    /// [`Session::row`], which a painter is calling beside this one.
    ///
    /// Empty is the answer whenever it can be: the switch is off, no rule
    /// compiled, or nothing on this row matched. A frontend with no rules
    /// configured pays one comparison per row for the whole feature.
    pub fn row_highlights(&mut self, y: usize) -> &[highlight::Span] {
        if !self.settings.color_highlighting || self.highlights.is_empty() {
            return &[];
        }
        let line = self.line_at(y);
        if self.line(line).is_none() {
            return &[];
        }
        if !self.highlight_memo.covers(self.damage_epoch, line) {
            let (first, last) = self.logical_line_bounds(line);
            self.flatten_logical_line(first, last);
            // Taken out and put back so the buffer survives the call without
            // borrowing `self` twice.
            let mut styles = std::mem::take(&mut self.highlight_styles);
            styles.clear();
            styles.resize(self.highlight_flat.cells.len(), highlight::Style::default());
            self.highlights.paint(
                &self.highlight_flat.text,
                &self.highlight_flat.starts,
                &mut styles,
            );
            let mut rows = std::mem::take(&mut self.highlight_memo.rows);
            self.highlight_flat.spans_into(&styles, &mut rows);
            self.highlight_styles = styles;
            self.highlight_memo = highlight::Memo {
                epoch: self.damage_epoch,
                first,
                last,
                rows,
            };
        }
        self.highlight_memo.row(line)
    }

    /// How far a logical line is followed in either direction.
    ///
    /// A cap, because `ATTR_LINE_CONTINUED` can in principle run the length of
    /// the scrollback — `yes | tr -d '\n'` is one logical line — and this is
    /// walked while painting. Past it the line is treated as ending, so `^` and
    /// `$` anchor to the cap rather than to the real ends; a rule that notices
    /// is a rule about a line hundreds of rows long.
    const MAX_LOGICAL_ROWS: u64 = 128;

    /// The first and last absolute line of the logical line containing `line`.
    ///
    /// The wrap marker sits on both ends of the join — the last cell of the
    /// upper row and the first cell of the lower one — which is what lets this
    /// be answered from either side. Same walk as [`Session::url_at`]'s.
    fn logical_line_bounds(&self, line: u64) -> (u64, u64) {
        let mut first = line;
        while first > 0 && line - first < Self::MAX_LOGICAL_ROWS {
            let continued = self
                .line(first)
                .and_then(|cells| cells.first())
                .is_some_and(|cell| cell.attrs & ATTR_LINE_CONTINUED != 0);
            if !continued || self.line(first - 1).is_none() {
                break;
            }
            first -= 1;
        }
        let mut last = line;
        while last - line < Self::MAX_LOGICAL_ROWS {
            let continued = self
                .line(last)
                .and_then(|cells| cells.last())
                .is_some_and(|cell| cell.attrs & ATTR_LINE_CONTINUED != 0);
            if !continued || self.line(last + 1).is_none() {
                break;
            }
            last += 1;
        }
        (first, last)
    }

    /// Flatten rows `first..=last` into text a pattern can be run over.
    ///
    /// Padding cells are not entries: a wide character is one cell that is two
    /// columns wide, so a match on it colours both of them. Trailing blanks go
    /// — every line in the grid is `cols` wide, and a `$` that had to be typed
    /// as ` *$` would be a puzzle nobody should have to solve.
    fn flatten_logical_line(&mut self, first: u64, last: u64) {
        let mut flat = std::mem::take(&mut self.highlight_flat);
        flat.clear();
        for line in first..=last {
            let Some(cells) = self.line(line) else {
                continue;
            };
            let mut x = 0;
            while x < cells.len() {
                let cell = &cells[x];
                if cell.width_class == WIDTH_PAD {
                    x += 1;
                    continue;
                }
                let width = if cell.width_class == WIDTH_WIDE { 2 } else { 1 };
                flat.starts.push(flat.text.len() as u32);
                for cp in cell.codepoints() {
                    flat.text.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                }
                flat.cells.push((line, x as u16, width));
                x += width as usize;
            }
        }
        while flat
            .cells
            .len()
            .checked_sub(1)
            .and_then(|i| flat.starts.get(i).map(|&s| &flat.text[s as usize..] == " "))
            .unwrap_or(false)
        {
            flat.cells.pop();
            let start = flat.starts.pop().unwrap_or(0);
            flat.text.truncate(start as usize);
        }
        flat.starts.push(flat.text.len() as u32);
        self.highlight_flat = flat;
    }

    // --- session logging ----------------------------------------------------

    /// Say what this session is connected *to*, for the `&h` and `&p` in a log
    /// name.
    ///
    /// The session cannot work this out from its own transport: a
    /// `Box<dyn Transport>` is a thing that moves bytes and deliberately knows
    /// nothing about how it was addressed. Upstream has the same split and
    /// reads `ts.HostName` — which the dialog and the command line write —
    /// rather than asking the connection.
    ///
    /// `host` is the host name for anything over TCP and the port's own name
    /// for a serial line, where upstream formats `COM<n>`.
    pub fn set_connection_name(&mut self, host: Option<String>, tcp_port: Option<u16>) {
        self.conn_host = host;
        self.conn_port = tcp_port;
    }

    /// What the `&`-escapes in a log name expand to right now.
    ///
    /// Both connection escapes go empty while nothing is connected, which is
    /// `ConvertLognameW`'s test on `cv.Open` rather than an accident of when
    /// the name happens to be asked for.
    pub fn log_context(&self) -> logname::LogContext {
        let open = self.conn.is_some();
        logname::LogContext {
            host: self.conn_host.clone().filter(|_| open),
            tcp_port: self.conn_port.filter(|_| open),
            user: logname::current_user(),
        }
    }

    /// The file a log would be opened under — `FLogGetLogFilename`.
    ///
    /// `requested` is `/L=`'s argument or a name a user typed, and `None` asks
    /// for `LogDefaultName`. Either way the answer is expanded and absolute,
    /// so a frontend can put it in a save dialog and a caller can open it.
    pub fn log_file_name(&self, requested: Option<&str>) -> std::path::PathBuf {
        logname::log_file_name(requested, &self.settings, &self.log_context())
    }

    /// Start writing a session log, replacing any log already open.
    ///
    /// A tap on the same byte stream rather than a second one: a raw log gets
    /// what the transport handed over, and a text log gets what the *parser*
    /// decided to display. Stripping escape sequences with a scanner beside
    /// the log would be a second parser to keep in agreement with the one
    /// that is verified against Tera Term.
    ///
    /// Nothing is logged retroactively. Upstream can prepend the scrollback
    /// (`LogAllBuffIncludedInFirst`) and the function it uses to do that is
    /// one of the upstream bugs on file — it truncates every line at its first
    /// wide character — so that option waits for the report to be answered.
    pub fn start_log(&mut self, path: &Path, opts: LogOptions) -> std::io::Result<()> {
        let text = opts.mode == LogMode::Text;
        let log = SessionLog::open(path, opts)?;
        self.log = Some(log);
        self.sync_log_epoch();
        self.vt.set_log_text_enabled(text);
        // Whatever the tap collected before this point belongs to no log.
        let _ = self.vt.take_log_text();
        Ok(())
    }

    /// Close the log, flushing it. A no-op when none is open.
    pub fn stop_log(&mut self) {
        self.vt.set_log_text_enabled(false);
        // Dropping flushes, but taking it explicitly means a failed flush is
        // not silently swallowed by a destructor that cannot report.
        if let Some(mut log) = self.log.take() {
            let _ = log.flush();
        }
    }

    pub fn log_path(&self) -> Option<&Path> {
        self.log.as_ref().map(|l| l.path())
    }

    /// Bytes written to the log since it was opened, for a status line.
    pub fn log_bytes(&self) -> u64 {
        self.log.as_ref().map_or(0, |l| l.bytes())
    }

    /// Stop or resume writing the log — `logpause` and `logstart`, and the
    /// button on upstream's logging dialog.
    ///
    /// **Not an error with no log open**, in either direction: `FLogPause`
    /// returns on a NULL `LogVar` and so do the other four of these. A macro
    /// cannot tell the difference and upstream's dialog cannot get there.
    pub fn pause_log(&mut self, paused: bool) {
        if let Some(log) = self.log.as_mut() {
            log.set_paused(paused);
        }
    }

    pub fn log_paused(&self) -> bool {
        self.log.as_ref().is_some_and(|l| l.is_paused())
    }

    /// `logwrite` — put a string in the log that did not come from the far
    /// end. See [`SessionLog::write_str`], which is also where the one
    /// deliberate divergence from upstream lives.
    pub fn write_log(&mut self, text: &str) {
        let Some(log) = self.log.as_mut() else {
            return;
        };
        // The same shape as `log_bytes_in`, and for the same reason: a write
        // that failed closes the log, and reporting it needs `self` back.
        let r = log.write_str(text);
        if let Err(e) = r {
            self.events.push(Event::LogFailed(e.to_string()));
            self.stop_log();
        }
    }

    /// `logrotate size`, `logrotate rotate` and `logrotate halt` — upstream's
    /// three setters, which reconfigure and rotate nothing now.
    pub fn set_log_rotate_size(&mut self, size: u64) {
        if let Some(log) = self.log.as_mut() {
            log.set_rotate_size(size);
        }
    }

    pub fn set_log_rotate_keep(&mut self, keep: u32) {
        if let Some(log) = self.log.as_mut() {
            log.set_rotate_keep(keep);
        }
    }

    pub fn halt_log_rotate(&mut self) {
        if let Some(log) = self.log.as_mut() {
            log.halt_rotate();
        }
    }

    /// The log's settings as they stand, for `loginfo` and a status line.
    pub fn log_options(&self) -> Option<&LogOptions> {
        self.log.as_ref().map(|l| l.options())
    }

    /// Attach a macro, and hand back the ring it will read the session
    /// through.
    ///
    /// This is upstream's `DDELog = TRUE` (`ttdde.c:1382`): until it is called
    /// the terminal collects nothing for a macro, and calling it throws away
    /// anything left over from a previous one. What arrives in the ring is
    /// **not** the byte stream from the far end — see
    /// [`Vt::set_macro_tap_enabled`], which is where that surprise is written
    /// down.
    ///
    /// Calling it twice replaces the link, which is upstream's rule too: one
    /// macro at a time, and `connect`ing a second one takes the terminal from
    /// the first.
    pub fn link_macro(&mut self) -> MacroLink {
        let link = MacroLink::new();
        self.vt.set_macro_tap_enabled(true);
        // Whatever the tap collected before this point belongs to no macro —
        // the same rule `start_log` follows.
        let _ = self.vt.take_macro_bytes();
        self.macro_link = Some(link.clone());
        link
    }

    /// Detach it — `DDELog = FALSE` and `DDEFreeBuf`. A no-op when none is
    /// linked.
    pub fn unlink_macro(&mut self) {
        if let Some(link) = self.macro_link.take() {
            link.clear();
        }
        self.vt.set_macro_tap_enabled(self.plugin_link.is_some());
    }

    /// Whether a macro is driving this session.
    pub fn macro_linked(&self) -> bool {
        self.macro_link.is_some()
    }

    /// Attach the window's long-lived Lua plugin worker.
    ///
    /// This is a second consumer of the same parser tap, not a second tap and
    /// not the macro slot. A plugin callback can therefore wait for terminal
    /// output while an ordinary `.ttl` or `.lua` macro runs, and starting that
    /// macro does not silently strand the plugin on an old ring.
    pub fn link_plugin(&mut self) -> MacroLink {
        let link = MacroLink::new();
        self.vt.set_macro_tap_enabled(true);
        let _ = self.vt.take_macro_bytes();
        self.plugin_link = Some(link.clone());
        link
    }

    /// Detach the Lua plugin worker without disturbing an ordinary macro.
    pub fn unlink_plugin(&mut self) {
        if let Some(link) = self.plugin_link.take() {
            link.clear();
        }
        self.vt.set_macro_tap_enabled(self.macro_link.is_some());
    }

    pub fn plugin_linked(&self) -> bool {
        self.plugin_link.is_some()
    }

    /// Install or replace the window's byte-stream filters.
    pub fn set_stream_filter(&mut self, filter: Box<dyn StreamFilter>) {
        self.stream_filter = Some(filter);
    }

    /// Remove the byte-stream filters without changing either script ring.
    pub fn clear_stream_filter(&mut self) {
        self.stream_filter = None;
    }

    /// Move what the tap collected into the macro's ring.
    ///
    /// Called wherever the log is fed, and for the same reason: the tap fills
    /// as the parser runs and something has to take it away before it grows.
    fn macro_bytes_in(&mut self) {
        if self.macro_link.is_none() && self.plugin_link.is_none() {
            return;
        }
        let bytes = self.vt.take_macro_bytes();
        if let Some(link) = &self.macro_link {
            link.push(&bytes);
        }
        if let Some(link) = &self.plugin_link {
            link.push(&bytes);
        }
    }

    /// Feed the log from whatever just arrived. Errors are reported once and
    /// then the log is closed: a disk that filled up will not un-fill, and
    /// retrying every pump turns one problem into a stall.
    fn log_bytes_in(&mut self, raw: &[u8]) {
        let Some(log) = self.log.as_mut() else {
            return;
        };
        let text = log.mode() == LogMode::Text;
        let result = if text {
            let collected = self.vt.take_log_text();
            log.write_text(&collected)
        } else {
            log.write_raw(raw)
        };
        if let Err(e) = result {
            self.events.push(Event::LogFailed(e.to_string()));
            self.stop_log();
        }
    }

    /// How many lines of history there are to scroll through.
    pub fn scrollback_len(&self) -> usize {
        self.vt.grid().scrollback_len()
    }

    /// How far back the view is, in lines. Zero is the live screen.
    ///
    /// Clamped on read rather than only on write, because the scrollback can
    /// shrink under a view that was legal when it was set — `ED 3` drops the
    /// whole history, and a resize moves lines between the page and it.
    pub fn view_offset(&self) -> usize {
        self.view_offset.min(self.vt.grid().scrollback_len())
    }

    /// Scroll the view. Clamped to the history that exists.
    pub fn set_view_offset(&mut self, offset: usize) {
        self.view_offset = offset.min(self.vt.grid().scrollback_len());
    }

    /// Edit > Clear screen — scroll the visible page into history and home the
    /// cursor. This is deliberately not `ED 2`: the menu command is local and
    /// unconditional, and upstream's `BuffClearScreen` preserves the page in
    /// the scrollback rather than erasing its cells in place.
    pub fn clear_screen(&mut self) {
        self.vt.grid_mut().clear_screen();
        self.vt.grid_mut().move_cursor(0, 0);
        self.vt.reconcile_sixels();
        self.follow_scroll();
        self.mark_damage();
    }

    /// Edit > Clear buffer — discard the history and blank the live page.
    ///
    /// Unlike remote `ED 3`, the local command is not gated by
    /// `ClearScrollBufferFromRemote`. `Grid::clear_buffer` also homes the
    /// cursor and restores the full scrolling margins, matching upstream's
    /// `ClearBuffer`.
    pub fn clear_buffer(&mut self) {
        self.vt.grid_mut().clear_buffer();
        self.vt.reconcile_sixels();
        // The old offset may become legal again as new history accumulates,
        // so clamping it only on read is not enough here. Clearing the buffer
        // is an explicit request to return to the blank live page.
        self.view_offset = 0;
        self.seen_scrolled_off = self.vt.grid().scrolled_off();
        self.mark_damage();
    }

    /// Whether the cursor's row is in view, and where — the frontend needs
    /// both, since a scrolled-back window must not paint a cursor that belongs
    /// to a screen it is not showing.
    ///
    /// Live screen row `y` appears at viewport row `y + offset`.
    pub fn cursor_view_row(&self) -> Option<usize> {
        let y = self.vt.grid().cursor.y + self.view_offset();
        (y < self.vt.grid().rows()).then_some(y)
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Move bytes in both directions for at most `budget`.
    ///
    /// Returns how many bytes arrived. Reading is bounded by the transport's
    /// own read timeout, so `budget` is a ceiling on the whole call rather
    /// than a sleep — a quiet line returns promptly with `Ok(0)` and no
    /// events, which is the normal case and must stay cheap.
    pub fn pump(&mut self, budget: Duration) -> Result<usize> {
        let deadline = Instant::now() + budget;
        let mut total = 0;

        // Outbound first. A key the user pressed should not wait behind a
        // screenful of output, and a short write earlier may have left some
        // of it behind.
        self.flush_pending()?;

        loop {
            let Some(conn) = self.conn.as_mut() else {
                return Ok(total);
            };
            self.rx.clear();
            self.rx_events.clear();
            let n = match conn.read(&mut self.rx, &mut self.rx_events) {
                Ok(n) => n,
                Err(e) if e.is_disconnected() => {
                    // Asked before the transport is dropped, which is the only
                    // moment it still knows: a pty's exit status dies with the
                    // child handle.
                    let kind = conn.link_kind();
                    self.close_note = conn.closing_note();
                    self.conn = None;
                    self.pending.clear();
                    self.restore_tcp_echo_cr();
                    self.transfer_disconnected();
                    self.events.push(Event::Disconnected);
                    self.connection_closed(kind);
                    return Ok(total);
                }
                Err(e) => return Err(e),
            };

            for ev in self.rx_events.drain(..) {
                match ev {
                    TransportEvent::Break => self.events.push(Event::Break),
                    TransportEvent::BadByte(b) => self.events.push(Event::BadByte(b)),
                    TransportEvent::Resize { cols, rows } => {
                        self.events.push(Event::Resize { cols, rows })
                    }
                    // `ts.LocalEcho`, assigned from the `ECHO` negotiation the
                    // way `telnet.c:411` assigns it. Not an [`Event`]: nothing
                    // above needs telling, because the terminal is where local
                    // echo is decided and it has just been told.
                    TransportEvent::LocalEcho(on) => self.vt.set_local_echo(on),
                }
            }

            // A running transfer owns the byte stream: its traffic must not
            // reach the parser, and the parser's replies must not reach the
            // peer. The empty-input call still matters — the protocols make
            // progress on their own, and a sender does nothing else.
            if self.xfer.is_some() {
                total += n;
                let bytes = std::mem::take(&mut self.rx);
                let finished = self.pump_transfer(&bytes)?;
                self.rx = bytes;
                if finished || Instant::now() >= deadline {
                    break;
                }
                if n == 0 {
                    break;
                }
                continue;
            }

            if n > 0 {
                total += n;
                let bytes = std::mem::take(&mut self.rx);
                let filtered = self.filter_stream(StreamDirection::Input, &bytes);
                self.vt.feed(&filtered);
                self.log_bytes_in(&filtered);
                self.macro_bytes_in();
                self.collect_clipboard();
                self.rx = bytes;
                self.follow_scroll();
                self.mark_damage();
                // Anything the parser answered — DA, DSR, DECRQSS — goes back
                // now rather than waiting for the next pump, because a host
                // that asked is usually blocked waiting.
                let reply = self.vt.take_reply();
                if !reply.is_empty() {
                    self.queue(&reply);
                    self.flush_pending()?;
                }
            }

            if Instant::now() >= deadline {
                break;
            }
            // A read that returned nothing means the line is quiet; spinning
            // on it until the deadline would burn a core for no bytes.
            if n == 0 && self.rx_events.is_empty() {
                break;
            }
        }

        self.collect_title();
        self.collect_bells();
        self.collect_colors();
        self.collect_printer_events();
        self.collect_window_requests();
        Ok(total)
    }

    /// Send a key, encoded by the core because the encoding depends on
    /// terminal state the frontend never sees.
    ///
    /// Returns whether anything went out: a key bound to a local command —
    /// Hold, Print, Break — produces no bytes.
    pub fn send_key(&mut self, key: Key) -> Result<bool> {
        if self.xfer.is_some() {
            return Ok(false);
        }
        match self.vt.key(key) {
            Some(bytes) => {
                self.feed_local_echo(&bytes);
                self.queue(&bytes);
                self.flush_pending()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Replace the physical-key map. Kept as a value operation so a frontend
    /// can parse on its own thread and so tests need no filesystem.
    pub fn set_key_map(&mut self, map: KeyboardMap) {
        self.key_map = map;
    }

    /// Read and install a `KEYBOARD.CNF`, returning the duplicate scan codes
    /// the UI may want to warn about.
    ///
    /// The old map survives an I/O error. A missing file is a successful empty
    /// map, matching the `GetPrivateProfileStringW` reads upstream makes.
    pub fn load_key_map(&mut self, path: &Path) -> std::io::Result<Vec<u16>> {
        let map = KeyboardMap::load(path)?;
        let duplicates = map.duplicates().to_vec();
        self.key_map = map;
        Ok(duplicates)
    }

    pub fn key_map(&self) -> &KeyboardMap {
        &self.key_map
    }

    /// Press a PC/AT set-1 scan code from `KEYBOARD.CNF`.
    ///
    /// Modifier bits are already part of `scan`: Shift `0x200`, Ctrl `0x400`
    /// and Alt `0x800`, exactly as `keyboard.c:KeyDown` builds the number.
    /// The binding is cloned before dispatch because sending mutates the
    /// session while the map is borrowed.
    pub fn send_key_code(&mut self, scan: u16) -> Result<KeyCodeResult> {
        use tt_config::KeyboardAction;

        let Some(action) = self.key_map.get(scan).cloned() else {
            return Ok(KeyCodeResult::Unmapped);
        };
        match action {
            KeyboardAction::Terminal(key @ (Key::Hold | Key::Print | Key::Break)) => {
                Ok(KeyCodeResult::LocalKey(key))
            }
            KeyboardAction::Terminal(key) => {
                self.send_key(key)?;
                Ok(KeyCodeResult::Sent)
            }
            KeyboardAction::Udk(n) => Ok(KeyCodeResult::Udk(n)),
            KeyboardAction::Shortcut(action) => Ok(KeyCodeResult::Shortcut(action)),
            KeyboardAction::User(user) => self.run_user_key(&user),
        }
    }

    /// Whether `KEYBOARD.CNF` binds this scan code, without pressing it.
    ///
    /// For the one caller that has to ask rather than dispatch: a dialog
    /// offering to give a quick button a key sequence, which has to say so when
    /// that key already belongs to the host.
    pub fn key_code_bound(&self, scan: u16) -> bool {
        self.key_map.get(scan).is_some()
    }

    /// Do what a `[User keys]` entry says, with no scan code involved.
    ///
    /// Split out of [`send_key_code`](Session::send_key_code) so that a quick
    /// button and a pressed key are the same action and not two
    /// implementations of it — the four kinds and their quirks are enough to
    /// get wrong once.
    pub fn run_user_key(&mut self, user: &UserKey) -> Result<KeyCodeResult> {
        match user.kind {
            UserKeyType::Binary => {
                // `Hex2StrW` first, then `SendBinary`: each UTF-16 code
                // unit below 256 narrows to its byte and every other one
                // becomes FF. A supplementary character is two units and
                // therefore two FF bytes, which a Rust `char` loop would
                // quietly collapse into one.
                let decoded = tt_config::hex_decode_str(&user.value);
                let bytes: Vec<u8> = decoded
                    .encode_utf16()
                    .map(|u| u8::try_from(u).unwrap_or(0xff))
                    .collect();
                self.send_bytes(&bytes)?;
                Ok(KeyCodeResult::Sent)
            }
            UserKeyType::Text => {
                self.send_text(&tt_config::hex_decode_str(&user.value))?;
                Ok(KeyCodeResult::Sent)
            }
            UserKeyType::Macro => Ok(KeyCodeResult::RunMacro(user.value.clone())),
            UserKeyType::Command => Ok(command_id(&user.value)
                .map(KeyCodeResult::Command)
                .unwrap_or(KeyCodeResult::Ignored)),
            UserKeyType::Unknown(_) => Ok(KeyCodeResult::Ignored),
        }
    }

    /// Send typed text. Ordinary characters do not go through the key table.
    ///
    /// A CR in it is expanded by LNM — see [`Vt::encode_text`]. That is how
    /// the main Return key works: it is not a [`Key`], because upstream is not
    /// either, so a frontend sends `"\r"` and the core decides whether a line
    /// feed follows.
    pub fn send_text(&mut self, text: &str) -> Result<()> {
        if self.xfer.is_some() {
            return Ok(());
        }
        let bytes = self.vt.encode_text(text);
        self.feed_local_echo(&bytes);
        self.queue(&bytes);
        self.flush_pending()
    }

    /// Send one locally edited line and its configured Return sequence.
    ///
    /// The line editor is a frontend concern: macros, protocol replies and
    /// mapped control keys must continue to use their ordinary immediate
    /// paths. What the core owns is the terminal-dependent encoding of Return
    /// and local echo. This path echoes exactly once even when SRM or
    /// `LocalEcho` currently says not to, without assigning that live mode or
    /// the saved setting. Turning the editor off therefore restores the
    /// previous echo behaviour instead of silently pinning it on.
    pub fn send_edited_line(&mut self, text: &str) -> Result<()> {
        if self.xfer.is_some() {
            return Ok(());
        }
        let mut bytes = self.vt.encode_text(text);
        bytes.extend_from_slice(&self.vt.encode_text("\r"));
        if !bytes.is_empty() {
            self.feed(&bytes);
            self.queue(&bytes);
        }
        self.flush_pending()
    }

    /// Put bytes on the wire exactly as given — no key table, no LNM, no
    /// encoding.
    ///
    /// For input which is bytes rather than text. A TTL string is the main
    /// caller — `#255` is a legal escape and `send` is documented to put what
    /// it was given on the line unchanged — and `Meta8Bit=raw` is the other:
    /// it sets bit 7 on the keyboard byte before sending. Routing either
    /// through [`send_text`](Session::send_text) would UTF-8-encode the byte
    /// the caller chose. Upstream keeps the same distinction one layer up:
    /// `SendData` sniffs and `SendBinary` does not (`ttdde.c:368`).
    ///
    /// Refused during a transfer, like everything else that could put a stray
    /// byte in the middle of a packet.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.xfer.is_some() {
            return Ok(());
        }
        self.feed_local_echo(bytes);
        self.queue(bytes);
        self.flush_pending()
    }

    /// Paste, the way `CBPreparePaste` prepares one (`clipboar.c:216`).
    ///
    /// Three things happen to the text on the way, and only the third is the
    /// one everybody knows about:
    ///
    /// 1. `TrimTrailingNLonPaste` cuts **every** trailing CR and LF, not one
    ///    (`clipboar.c:55`). Off by default, so a copied line still ends in a
    ///    newline and the shell still runs it.
    /// 2. **Every line break becomes a single CR** — `NormalizeLineBreakCR`
    ///    (`ttlib_static_cpp.cpp:535`) maps `LF` and `CR LF` alike onto `CR`.
    ///    A terminal sends what a keyboard sends, and the Return key is a CR;
    ///    passing the clipboard's own LFs through is the obvious build and
    ///    puts a byte on the wire that no key on the keyboard produces.
    /// 3. The brackets, which need the host's `DECSET 2004` **and** the two
    ///    settings — see [`Settings::clipboard_bracketed`].
    ///
    /// The confirmation dialog between 1 and 2 is the frontend's: it is modal,
    /// it can edit the text, and a core that blocked on it would be a core
    /// that owned a window.
    ///
    /// `add_cr` is upstream's `Paste<CR>` (`ID_EDIT_PASTECR`, the second item
    /// on the right button's menu): one CR appended to the text, which is what
    /// makes a pasted command run. Its position in the sequence is load
    /// bearing and is not the end — `clipboar.c:280` appends it *after* the
    /// bracket decision and *before* the normalisation, so with
    /// `BracketedControlOnly` on a single line pasted this way is sent
    /// unbracketed even though a control character is going out, and a
    /// clipboard already ending in a CR sends two.
    pub fn paste(&mut self, text: &str, add_cr: bool) -> Result<()> {
        // Everything the user could type is refused while a transfer is up —
        // a stray byte in the middle of a packet is a corrupted file, and a
        // paste is the largest stray byte there is.
        if self.xfer.is_some() {
            return Ok(());
        }
        let mut text = paste_text(text, &self.settings);
        // Upstream decides this before normalising to CR and after normalising
        // to CRLF, so `iswcntrl` sees the line breaks either way; doing it on
        // the normalised text is the same answer for one fewer copy. What it
        // must not see is the appended CR, which is added below it.
        let bracket = self.vt.bracketed_paste()
            && self.settings.clipboard_bracketed
            && (!self.settings.clipboard_bracketed_control_only
                || text.chars().any(|c| c.is_control()));
        if add_cr {
            text.push('\r');
        }
        if bracket {
            self.queue(b"\x1b[200~");
        }
        // The brackets describe the paste to the host; they are not keyboard
        // input and upstream does not put them through `CommTextEchoW`.
        self.feed_local_echo(text.as_bytes());
        self.queue(text.as_bytes());
        if bracket {
            self.queue(b"\x1b[201~");
        }
        self.flush_pending()
    }

    /// Answer an accepted OSC 52 clipboard read. `selection` is the string
    /// carried by [`ClipboardRequest::Read`]; `text` is UTF-8 from the
    /// frontend's clipboard.
    ///
    /// False is not an I/O error: it means upstream would send no reply
    /// because the selector does not fit its fixed header or the clipboard is
    /// not text. A true result has already been flushed to the far end.
    pub fn clipboard_reply(&mut self, selection: &str, text: &str) -> Result<bool> {
        if !self.vt.clipboard_reply(selection, text) {
            return Ok(false);
        }
        let reply = self.vt.take_reply();
        self.queue(&reply);
        self.flush_pending()?;
        Ok(true)
    }

    /// Report a mouse event, in window pixels. Returns whether the terminal
    /// consumed it — if not, the click is the frontend's to use for
    /// selection.
    pub fn mouse(
        &mut self,
        event: MouseEvent,
        button: u8,
        px: i32,
        py: i32,
        mods: Modifiers,
    ) -> Result<bool> {
        let consumed = self.vt.mouse_event(event, button, px, py, mods);
        let reply = self.vt.take_reply();
        if !reply.is_empty() {
            self.queue(&reply);
            self.flush_pending()?;
        }
        Ok(consumed)
    }

    /// Focus gained or lost. Silent unless `DECSET 1004` is on.
    pub fn focus(&mut self, focused: bool) -> Result<()> {
        self.vt.focus_event(focused);
        let reply = self.vt.take_reply();
        if !reply.is_empty() {
            self.queue(&reply);
            self.flush_pending()?;
        }
        Ok(())
    }

    /// Whether the frontend should be tracking the mouse at all.
    pub fn mouse_tracking(&self) -> Tracking {
        self.vt.mouse_tracking()
    }

    /// Tell the core how big a character cell is, in pixels. Mouse reporting
    /// is the only thing that reads it, and `DECSET 1016` reports pixels back
    /// unconverted, which is why the number crosses the boundary at all.
    pub fn set_cell_pixels(&mut self, w: i32, h: i32) {
        self.vt.set_cell_pixels(w, h);
    }

    /// Resize the terminal and tell the far end.
    ///
    /// Both halves matter and they are easy to separate by accident: a grid
    /// that resized without a `TIOCSWINSZ` leaves `vi` drawing to the old
    /// size, and the symptom looks like a redraw bug rather than a missing
    /// ioctl.
    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        self.vt.grid_mut().resize(cols, rows);
        self.vt.reconcile_sixels();
        // A resize moves lines between the page and the scrollback in both
        // directions, so whatever the view was anchored to has moved and the
        // honest answer is to stop guessing and go live.
        self.view_offset = 0;
        self.seen_scrolled_off = self.vt.grid().scrolled_off();
        self.seen_size = (cols, rows);
        self.mark_damage();
        if let Some(c) = self.conn.as_mut() {
            c.resize(cols as u16, rows as u16)?;
        }
        Ok(())
    }

    /// Send a line break — `CommSendBreak`. On a serial console this is how
    /// you reach a `getty` or drop a Sun box to its PROM.
    ///
    /// **The duration is the setting's and not the caller's.** Upstream has
    /// exactly one break length, `ts.SendBreakTime` (`vtwin.cpp:4906`), and
    /// every way of asking for one arrives there: the menu, the accelerator,
    /// and a macro's `sendbreak`, which posts the menu command through DDE
    /// (`ttdde.c:801`) rather than carrying a length of its own. So a
    /// parameter here is a parameter every caller has to invent a value for,
    /// which is what happened — 300 ms in the window, 250 in the macro host,
    /// and neither of them the file's 1000.
    pub fn send_break(&mut self) -> Result<()> {
        let dur = Duration::from_millis(self.settings.serial_break_time.max(0) as u64);
        match self.conn.as_mut() {
            Some(c) => c.send_break(dur),
            None => Ok(()),
        }
    }

    /// Whether [`send_break`](Session::send_break) will do anything on the
    /// current connection. False when there is none.
    ///
    /// A frontend needs this to draw its menu rather than to handle a failure:
    /// a break is what someone reaches for when a console has stopped
    /// answering, which is the worst moment to discover the transport cannot
    /// send one.
    pub fn supports_break(&self) -> bool {
        self.conn.as_ref().is_some_and(|c| c.supports_break())
    }

    /// What is attached, or `None` with nothing connected.
    ///
    /// The nearest thing this port has to `cv.PortType`, which several
    /// settings are conditioned on rather than on anything the transport does
    /// — `ConfirmDisconnect` and `BeepOnConnect` both ask only about a TCP
    /// session.
    pub fn link_kind(&self) -> Option<tt_conn::LinkKind> {
        self.conn.as_ref().map(|c| c.link_kind())
    }

    /// The current serial speed, or `None` on every other link.
    ///
    /// Read from the transport rather than the settings: a command-line open
    /// need not have used the file's speed, and `setbaud` changes the live
    /// port before a frontend asks again.
    pub fn serial_baud(&self) -> Option<u32> {
        match self.link_kind()? {
            tt_conn::LinkKind::Serial { baud, .. } => Some(baud),
            _ => None,
        }
    }

    /// Feed bytes as though they had arrived from the far end. For local echo
    /// and for tests; it is also how a replayed session log would work.
    pub fn feed(&mut self, bytes: &[u8]) {
        let bytes = self.filter_stream(StreamDirection::Input, bytes);
        self.vt.feed(&bytes);
        self.log_bytes_in(&bytes);
        self.macro_bytes_in();
        self.collect_clipboard();
        self.follow_scroll();
        self.mark_damage();
        self.collect_title();
        self.collect_bells();
        self.collect_colors();
        self.collect_printer_events();
        self.collect_window_requests();
    }

    /// Run the governor over whatever the parser asked for, and emit at most
    /// one [`Event::Bell`].
    ///
    /// One event for a burst because two beeps in the same millisecond are one
    /// beep — but the governor is stepped once per request even so, since its
    /// whole job is to count them. A burst that is thinned to nothing here
    /// still leaves the terminal quiet for the next one, which is the point.
    fn collect_bells(&mut self) {
        let asked = self.vt.take_bells();
        if asked.reset {
            self.bell.reset();
        }
        if asked.count == 0 {
            return;
        }
        // `ESC g` asks for a bell without consulting the setting, so this has
        // to test it again — `RingBell`'s own switch is where upstream does.
        let visual = match self.settings.bell_mode {
            tt_config::BellMode::Off => return,
            tt_config::BellMode::On => false,
            tt_config::BellMode::Visual => true,
        };
        let limits = settings::bell_limits(&self.settings);
        let now = Instant::now();
        let mut heard = false;
        for _ in 0..asked.count {
            heard |= self.bell.ring(now, &limits);
        }
        if heard {
            self.events.push(Event::Bell { visual });
        }
    }

    /// Keep a scrolled-back view on the same lines as new output arrives —
    /// `AutoScrollOnlyInBottomLine`, and only when it is on.
    ///
    /// Without it the view is anchored to the bottom, so every line the host
    /// prints slides what the user is reading up by one — which is worst
    /// exactly when it matters, scrolling back through a boot log while the
    /// device is still talking. **That is upstream's shipped behaviour**: the
    /// key defaults off, `MoveCursor` and `MoveRight` then call
    /// `DispScrollToCursor` on every step (`buffer.c:3794`, `:3805`), and
    /// `BuffScrollNLines` leaves `NewOrgY` where it was so the content slides
    /// under the view (`:3866`). This port had the `on` behaviour hardcoded
    /// until the key existed to ask.
    ///
    /// `DispScrollToCursor` is the minimum scroll that keeps the cursor on
    /// screen rather than a jump to the bottom (`vtdisp.c:3095`) — which comes
    /// to the same thing while a host is printing, since the cursor is then on
    /// the last row, and does not when it is drawing higher up.
    ///
    /// With the key on, the offset moves by however many lines left the page,
    /// and that is the right number whether the scrollback grew or was already
    /// full and evicted: both shift the content by the same amount. Clamping
    /// is left to the accessors, so a view pushed off the top lands on the
    /// oldest line rather than being reset.
    /// A resize that arrives *in the byte stream* re-anchors the view either
    /// way.
    ///
    /// [`Session::resize`] is only the frontend's path. DECCOLM and the
    /// XTWINOPS resize both reach `Grid::resize` from inside the parser, and
    /// that moves lines between the page and the scrollback without
    /// `scrolled_off` recording it — so the difference this function reads
    /// stops meaning what it means everywhere else, and a scrolled-back view
    /// jumps by however many lines the resize shuffled. Same answer as the
    /// frontend path: stop guessing, go live.
    fn follow_scroll(&mut self) {
        if self.reanchor_after_resize() {
            return;
        }
        let now = self.vt.grid().scrolled_off();
        if self.view_offset > 0 {
            if self.settings.window_auto_scroll_only_at_bottom {
                self.view_offset += (now - self.seen_scrolled_off) as usize;
            } else {
                let grid = self.vt.grid();
                self.view_offset = self.view_offset.min(grid.rows() - 1 - grid.cursor.y);
            }
        }
        self.seen_scrolled_off = now;
    }

    /// The half of [`Session::follow_scroll`] that a settings change needs, and
    /// the only half: a resize moves lines between the page and the history in
    /// both directions, so the difference `follow_scroll` reads stops meaning
    /// what it means and the view goes live.
    ///
    /// Separate because the other half is the *cursor* following, which
    /// upstream does from `MoveCursor` and not from `SetupTerm` — folding the
    /// two together made opening the settings dialog on a terminal whose
    /// cursor is on the last row snap the reader back to live, which is a
    /// thing no dialog should do.
    fn reanchor_after_resize(&mut self) -> bool {
        let size = (self.vt.grid().cols(), self.vt.grid().rows());
        if size == self.seen_size {
            return false;
        }
        self.seen_size = size;
        self.view_offset = 0;
        self.seen_scrolled_off = self.vt.grid().scrolled_off();
        true
    }

    fn queue(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let bytes = self.filter_stream(StreamDirection::Output, bytes);
        self.pending.extend_from_slice(&bytes);
    }

    /// Put keyboard input through the receive parser when SRM says to echo.
    ///
    /// Upstream writes the same text or binary bytes into `cv.InBuff` through
    /// `CommTextEchoW` / `CommBinaryEcho`; [`Session::feed`] is this port's
    /// corresponding path. Keeping it before the output filter also keeps a
    /// plugin's receive and transmit directions independent.
    fn feed_local_echo(&mut self, bytes: &[u8]) {
        if self.vt.local_echo() && !bytes.is_empty() {
            self.feed(bytes);
        }
    }

    /// Apply the optional filter without paying for a copy in the ordinary
    /// case. The filter is taken out for the call so reporting its failures can
    /// append to `events` without overlapping the mutable borrow.
    fn filter_stream<'a>(&mut self, direction: StreamDirection, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        let Some(mut filter) = self.stream_filter.take() else {
            return Cow::Borrowed(bytes);
        };
        let result = filter.filter(direction, bytes);
        self.stream_filter = Some(filter);
        self.events
            .extend(result.errors.into_iter().map(Event::StreamFilterFailed));
        Cow::Owned(result.bytes)
    }

    /// Push what is queued, keeping whatever the transport would not take.
    fn flush_pending(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let Some(conn) = self.conn.as_mut() else {
            // Nowhere to send it. Dropping beats growing without bound while
            // someone types at a disconnected window.
            self.pending.clear();
            return Ok(());
        };
        match conn.write(&self.pending, self.write_timeout) {
            Ok(n) => {
                self.pending.drain(..n);
                Ok(())
            }
            Err(e) if e.is_disconnected() => {
                let kind = conn.link_kind();
                self.close_note = conn.closing_note();
                self.conn = None;
                self.pending.clear();
                self.restore_tcp_echo_cr();
                self.transfer_disconnected();
                self.events.push(Event::Disconnected);
                self.connection_closed(kind);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The title is state on `Vt`, not an event, so an edge has to be found.
    ///
    /// The *window* title — what the frontend puts in the title bar — rather
    /// than the one the host set, so that `AcceptTitleChangeRequest`'s four
    /// spellings are applied once, here, instead of in every frontend.
    fn collect_title(&mut self) {
        let title = self.vt.window_title();
        if title != self.last_title {
            self.last_title = title;
            self.events.push(Event::Title(self.last_title.clone()));
        }
    }

    /// Say that a colour OSC moved something the painter caches.
    ///
    /// Called from all three places bytes reach the parser — the pump, `feed`,
    /// and `set_settings`, which rebuilds the live colours from the file. It
    /// was missing from the pump when the colour OSCs first landed, which is
    /// the one path a *host* reaches: `feed` covered the tests and the local
    /// echo, so an `OSC 4` over a real connection repainted nothing until
    /// something else invalidated the cache.
    fn collect_colors(&mut self) {
        if self.vt.take_colors_changed() {
            self.events.push(Event::ColorsChanged);
        }
    }

    /// Everything the media-copy sequences asked of a printer, in order.
    ///
    /// Beside [`Session::collect_window_requests`] and called from the same
    /// three places, for the same reason: the sequences arrive while `advance`
    /// is parsing and there is no printer down there to hand them to.
    fn collect_printer_events(&mut self) {
        self.events.extend(
            self.vt
                .take_printer_events()
                .into_iter()
                .map(Event::Printer),
        );
    }

    fn collect_window_requests(&mut self) {
        self.events.extend(
            self.vt
                .take_window_requests()
                .into_iter()
                .map(Event::WindowRequest),
        );
        // `CSI 8 t` has already moved the grid — upstream resizes there too,
        // and the differential dump is taken at the new size — so this is the
        // window being told to follow rather than being asked whether to. It
        // does not come back round: `take_terminal_resized` is set by that
        // sequence alone and not by `Session::resize`.
        if self.vt.take_terminal_resized() {
            self.reanchor_after_resize();
            let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
            if let Some(c) = self.conn.as_mut() {
                let _ = c.resize(cols as u16, rows as u16);
            }
            self.events.push(Event::Resize {
                cols: cols as u16,
                rows: rows as u16,
            });
        }
    }

    fn collect_clipboard(&mut self) {
        self.events.extend(
            self.vt
                .take_clipboard_requests()
                .into_iter()
                .map(Event::Clipboard),
        );
    }

    /// The branch after `CommClose` in `vtwin.cpp:3020`.
    ///
    /// `AutoWinClose` is network-only. If the window remains, the independent
    /// clear setting runs the ordinary Clear screen command: scroll the page
    /// into history, home the cursor, and reconcile a scrolled-back view. The
    /// clear is also done before a close request when enabled, because the
    /// core cannot know whether a disabled/modal frontend will accept that
    /// request; it is unobservable when the window does close and is the
    /// upstream fallback when it cannot.
    fn connection_closed(&mut self, kind: tt_conn::LinkKind) {
        self.connect_beep(kind);

        if self.settings.connection_clear_screen_on_close {
            self.clear_screen();
        }

        if matches!(kind, tt_conn::LinkKind::Network) && self.settings.connection_auto_win_close {
            self.events.push(Event::CloseRequested);
        }
    }
}

impl Session {
    /// How long a write may block before returning short. The default is
    /// 200 ms; a frontend driving the pump from its UI thread wants it
    /// smaller.
    pub fn set_write_timeout(&mut self, d: Duration) {
        self.write_timeout = d;
    }
}

/// What a paste puts on the wire, before the brackets: `TrimTrailingNLW` and
/// then `NormalizeLineBreakCR` (`clipboar.c:241`, `:289`).
///
/// Public because the confirmation dialog is the frontend's, and it has to be
/// able to show the text the terminal is really about to send rather than what
/// the clipboard happened to hold.
pub fn paste_text(text: &str, s: &Settings) -> String {
    // "Trim the trailing newline" is every one of them: the loop walks back
    // over CR and LF alike until it finds something else (`clipboar.c:55`).
    let text = if s.clipboard_trim_trailing_newline {
        text.trim_end_matches(['\r', '\n'])
    } else {
        text
    };
    // `CR LF` and a bare `LF` both become one `CR`; a bare `CR` stays one. The
    // pair has to be collapsed rather than mapped byte by byte, or a file
    // copied out of an editor arrives as two line endings for every line.
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('\r');
            }
            '\n' => out.push('\r'),
            _ => out.push(c),
        }
    }
    out
}

/// The `%hd` used when a type-3 user key is pressed. Leading whitespace and
/// a numeric prefix are accepted, and the result narrows to the command word.
fn command_id(s: &str) -> Option<u16> {
    let s = s.trim_start_matches(char::is_whitespace);
    let b = s.as_bytes();
    let sign = matches!(b.first(), Some(b'+') | Some(b'-')) as usize;
    let digits = b[sign..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    s[..sign + digits].parse::<i64>().ok().map(|n| n as u16)
}

/// A [`Transport`] over two byte queues, for tests.
///
/// Not a mock: a real implementation of the trait with no I/O, so the session
/// can be tested at full speed and without hardware. `tt-conn`'s loopback
/// tests still cover the wire; this covers the composition.
///
/// The session takes ownership of a boxed transport, so the state lives
/// behind a [`MemoryHandle`] the caller keeps — which is also the honest
/// shape for a frontend that needs to reach a serial port's settings after
/// connecting.
#[derive(Debug, Default)]
pub struct MemoryState {
    /// Bytes waiting to be read by the terminal.
    pub inbound: Vec<u8>,
    /// Everything the terminal has sent.
    pub outbound: Vec<u8>,
    pub events: Vec<TransportEvent>,
    pub disconnected: bool,
    pub last_resize: Option<(u16, u16)>,
    pub breaks: usize,
    /// How long the last one was asked to hold for — `SendBreakTime`, since
    /// nothing else is allowed to name a duration.
    pub last_break: Option<Duration>,
    /// Most bytes to accept per write, or 0 for all of them. Set it to 1 to
    /// behave like a line held by flow control.
    pub write_chunk: usize,
    /// Fail writes with this instead of accepting them.
    pub write_error: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
pub struct MemoryHandle(Arc<Mutex<MemoryState>>);

impl MemoryHandle {
    pub fn with<T>(&self, f: impl FnOnce(&mut MemoryState) -> T) -> T {
        f(&mut self.0.lock().expect("memory transport poisoned"))
    }

    pub fn outbound(&self) -> Vec<u8> {
        self.with(|s| s.outbound.clone())
    }

    pub fn feed(&self, bytes: &[u8]) {
        self.with(|s| s.inbound.extend_from_slice(bytes));
    }
}

pub struct MemoryTransport {
    handle: MemoryHandle,
    kind: tt_conn::LinkKind,
}

impl MemoryTransport {
    /// The transport and a handle onto its state. The session eats the first.
    pub fn new() -> (MemoryTransport, MemoryHandle) {
        Self::with_kind(tt_conn::LinkKind::Network)
    }

    /// The same in-memory wire carrying a particular kind of link. Most tests
    /// want the default network shape; settings conditioned on `PortType`
    /// need to distinguish it from a serial port or local pty without opening
    /// real hardware or a child process.
    pub fn with_kind(kind: tt_conn::LinkKind) -> (MemoryTransport, MemoryHandle) {
        let handle = MemoryHandle::default();
        (
            MemoryTransport {
                handle: handle.clone(),
                kind,
            },
            handle,
        )
    }
}

impl Transport for MemoryTransport {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        self.handle.with(|s| {
            if s.disconnected {
                return Err(Error::Disconnected);
            }
            events.append(&mut s.events);
            let n = s.inbound.len();
            data.append(&mut s.inbound);
            Ok(n)
        })
    }

    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<usize> {
        self.handle.with(|s| {
            if s.disconnected {
                return Err(Error::Disconnected);
            }
            if let Some(what) = s.write_error {
                return Err(Error::Unsupported(what.to_string()));
            }
            let n = if s.write_chunk == 0 {
                data.len()
            } else {
                data.len().min(s.write_chunk)
            };
            s.outbound.extend_from_slice(&data[..n]);
            Ok(n)
        })
    }

    fn send_break(&mut self, dur: Duration) -> Result<()> {
        self.handle.with(|s| {
            s.breaks += 1;
            s.last_break = Some(dur);
        });
        Ok(())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.handle.with(|s| s.last_resize = Some((cols, rows)));
        Ok(())
    }

    fn link_kind(&self) -> tt_conn::LinkKind {
        self.kind
    }

    fn describe(&self) -> String {
        "memory".to_string()
    }
}
