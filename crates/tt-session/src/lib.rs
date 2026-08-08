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

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod log;
pub mod settings;
pub mod xfer;

pub use log::{LogMode, LogOptions, SessionLog, Timestamp};
pub use settings::vt_config;
pub use xfer::{xfer_options, TransferError, TransferOutcome, TransferStatus};
// Re-exported rather than reached for directly, so that a frontend — the C ABI
// above all — takes the settings and the metadata that describes them from the
// same place it takes the session they belong to.
pub use tt_config::{Field, Ini, Kind, Settings, FIELDS};

use tt_conn::{Error, Result, Transport, TransportEvent};
use tt_grid::{Cell, Grid};
use tt_vt::{Config, Key, Modifiers, MouseEvent, Tracking, Vt};

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
    /// OSC 0 / OSC 2.
    Title(String),
    /// A line break arrived from the far end. On a serial console this is how
    /// a host asks for attention, and dropping it is a real loss of function.
    Break,
    /// A byte arrived with a parity or framing error.
    BadByte(u8),
    /// The transport went away — unplugged, hung up, or the child exited.
    Disconnected,
    /// The **far end** says the terminal should be this size.
    ///
    /// Backwards from the usual direction and real: telnet's NAWS is defined
    /// client-to-server, and a console server sends it the other way to say
    /// what the equipment behind it actually is. The session does **not**
    /// resize itself on it — the window owns its own size, and a core that
    /// silently changed the grid would leave the frontend painting the wrong
    /// number of cells. Honouring it is the frontend's decision, which is what
    /// upstream does too (`buffer.c:5106` goes through the window).
    Resize { cols: u16, rows: u16 },
    /// The session log could not be written and has been closed. Reported
    /// once: a disk that filled up will not un-fill, and retrying on every
    /// pump turns one problem into a stall.
    LogFailed(String),
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
    /// The file transfer that owns the byte stream, if one is running.
    xfer: Option<xfer::Running>,
}

impl Session {
    pub fn new(config: Config) -> Session {
        let vt = Vt::new(config);
        Session {
            last_title: vt.title().to_string(),
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
            close_note: None,
            settings: Settings::default(),
            xfer: None,
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
    pub fn set_settings(&mut self, settings: Settings) -> Result<()> {
        let config = vt_config(&settings, self.vt.config());
        self.settings = settings;
        self.vt.set_config(config);
        // `set_config` may have resized the grid, and `follow_scroll` is where
        // that is noticed — it re-anchors the viewport on a size change rather
        // than leaving the offset pointing at a line that has moved.
        self.follow_scroll();
        self.events.push(Event::Damage);
        let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
        if let Some(c) = self.conn.as_mut() {
            c.resize(cols as u16, rows as u16)?;
        }
        Ok(())
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
    /// `TERATERM.INI` cannot disagree about what a value means.
    pub fn set_setting(&mut self, name: &str, value: &str) -> Result<bool> {
        let mut settings = self.settings.clone();
        if !settings.set_str(name, value) {
            return Ok(false);
        }
        self.set_settings(settings)?;
        Ok(true)
    }

    /// Attach a connection. The terminal is not reset: reconnecting a serial
    /// console to the same session is how you keep the scrollback that
    /// explains why it dropped.
    pub fn connect(&mut self, conn: Box<dyn Transport>) {
        self.pending.clear();
        self.close_note = None;
        self.conn = Some(conn);
        let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
        if let Some(c) = self.conn.as_mut() {
            let _ = c.resize(cols as u16, rows as u16);
        }
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.pending.clear();
        // A transfer has to be told here rather than on the next pump: with no
        // connection left, `pump` returns before it reaches anything, so the
        // transfer would sit "running" for ever with a progress dialog on top
        // of it and no way to reach the end.
        self.transfer_disconnected();
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

    pub fn grid(&self) -> &Grid {
        self.vt.grid()
    }

    /// The terminal itself, for the settings and mode accessors the frontend
    /// reads — cursor visibility, bracketed paste, reverse video.
    pub fn vt(&self) -> &Vt {
        &self.vt
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
        let grid = self.vt.grid();
        let back = grid.scrollback_len();
        // The oldest line still held, one page-worth of scrollback below the
        // top of the page.
        let first = self.top_line() - back as u64;
        let i = usize::try_from(line.checked_sub(first)?).ok()?;
        if i < back {
            grid.scrollback_line(i)
        } else {
            let y = i - back;
            (y < grid.rows()).then(|| grid.line(y))
        }
    }

    // --- session logging ----------------------------------------------------

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
                    self.close_note = conn.closing_note();
                    self.conn = None;
                    self.transfer_disconnected();
                    self.events.push(Event::Disconnected);
                    return Ok(total);
                }
                Err(e) => return Err(e),
            };

            for ev in self.rx_events.drain(..) {
                self.events.push(match ev {
                    TransportEvent::Break => Event::Break,
                    TransportEvent::BadByte(b) => Event::BadByte(b),
                    TransportEvent::Resize { cols, rows } => Event::Resize { cols, rows },
                });
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
                self.vt.feed(&bytes);
                self.log_bytes_in(&bytes);
                self.rx = bytes;
                self.follow_scroll();
                self.events.push(Event::Damage);
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
                self.queue(&bytes);
                self.flush_pending()?;
                Ok(true)
            }
            None => Ok(false),
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
        self.queue(&bytes);
        self.flush_pending()
    }

    /// Paste, bracketed when the host asked for it (`DECSET 2004`).
    ///
    /// The brackets are the point: without them a shell runs every newline in
    /// the pasted text as a command, which is a well-known way to lose data
    /// to a copied trailing newline.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        // Everything the user could type is refused while a transfer is up —
        // a stray byte in the middle of a packet is a corrupted file, and a
        // paste is the largest stray byte there is.
        if self.xfer.is_some() {
            return Ok(());
        }
        if self.vt.bracketed_paste() {
            self.queue(b"\x1b[200~");
            self.queue(text.as_bytes());
            self.queue(b"\x1b[201~");
        } else {
            self.queue(text.as_bytes());
        }
        self.flush_pending()
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
        // A resize moves lines between the page and the scrollback in both
        // directions, so whatever the view was anchored to has moved and the
        // honest answer is to stop guessing and go live.
        self.view_offset = 0;
        self.seen_scrolled_off = self.vt.grid().scrolled_off();
        self.seen_size = (cols, rows);
        self.events.push(Event::Damage);
        if let Some(c) = self.conn.as_mut() {
            c.resize(cols as u16, rows as u16)?;
        }
        Ok(())
    }

    /// Send a line break — `CommSendBreak`. On a serial console this is how
    /// you reach a `getty` or drop a Sun box to its PROM.
    pub fn send_break(&mut self, dur: Duration) -> Result<()> {
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

    /// Feed bytes as though they had arrived from the far end. For local echo
    /// and for tests; it is also how a replayed session log would work.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.vt.feed(bytes);
        self.log_bytes_in(bytes);
        self.follow_scroll();
        self.events.push(Event::Damage);
        self.collect_title();
    }

    /// Keep a scrolled-back view on the same lines as new output arrives.
    ///
    /// Without this the view is anchored to the bottom, so every line the host
    /// prints slides what the user is reading up by one — which is worst
    /// exactly when it matters, scrolling back through a boot log while the
    /// device is still talking.
    ///
    /// The offset moves by however many lines left the page, and that is the
    /// right number whether the scrollback grew or was already full and
    /// evicted: both shift the content by the same amount. Clamping is left to
    /// the accessors, so a view pushed off the top lands on the oldest line
    /// rather than being reset.
    /// A resize that arrives *in the byte stream* re-anchors the view too.
    ///
    /// [`Session::resize`] is only the frontend's path. DECCOLM and the
    /// XTWINOPS resize both reach `Grid::resize` from inside the parser, and
    /// that moves lines between the page and the scrollback without
    /// `scrolled_off` recording it — so the difference this function reads
    /// stops meaning what it means everywhere else, and a scrolled-back view
    /// jumps by however many lines the resize shuffled. Same answer as the
    /// frontend path: stop guessing, go live.
    fn follow_scroll(&mut self) {
        let size = (self.vt.grid().cols(), self.vt.grid().rows());
        if size != self.seen_size {
            self.seen_size = size;
            self.view_offset = 0;
            self.seen_scrolled_off = self.vt.grid().scrolled_off();
            return;
        }
        let now = self.vt.grid().scrolled_off();
        if self.view_offset > 0 {
            self.view_offset += (now - self.seen_scrolled_off) as usize;
        }
        self.seen_scrolled_off = now;
    }

    fn queue(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
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
                self.close_note = conn.closing_note();
                self.conn = None;
                self.pending.clear();
                self.events.push(Event::Disconnected);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The title is state on `Vt`, not an event, so an edge has to be found.
    fn collect_title(&mut self) {
        if self.vt.title() != self.last_title {
            self.last_title = self.vt.title().to_string();
            self.events.push(Event::Title(self.last_title.clone()));
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

pub struct MemoryTransport(MemoryHandle);

impl MemoryTransport {
    /// The transport and a handle onto its state. The session eats the first.
    pub fn new() -> (MemoryTransport, MemoryHandle) {
        let handle = MemoryHandle::default();
        (MemoryTransport(handle.clone()), handle)
    }
}

impl Transport for MemoryTransport {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        self.0.with(|s| {
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
        self.0.with(|s| {
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

    fn send_break(&mut self, _dur: Duration) -> Result<()> {
        self.0.with(|s| s.breaks += 1);
        Ok(())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.0.with(|s| s.last_resize = Some((cols, rows)));
        Ok(())
    }

    fn describe(&self) -> String {
        "memory".to_string()
    }
}
