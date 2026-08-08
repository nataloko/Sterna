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

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        }
    }

    /// Attach a connection. The terminal is not reset: reconnecting a serial
    /// console to the same session is how you keep the scrollback that
    /// explains why it dropped.
    pub fn connect(&mut self, conn: Box<dyn Transport>) {
        self.pending.clear();
        self.conn = Some(conn);
        let (cols, rows) = (self.vt.grid().cols(), self.vt.grid().rows());
        if let Some(c) = self.conn.as_mut() {
            let _ = c.resize(cols as u16, rows as u16);
        }
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.pending.clear();
    }

    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    pub fn describe(&self) -> Option<String> {
        self.conn.as_ref().map(|c| c.describe())
    }

    pub fn grid(&self) -> &Grid {
        self.vt.grid()
    }

    /// The terminal itself, for the settings and mode accessors the frontend
    /// reads — cursor visibility, bracketed paste, reverse video.
    pub fn vt(&self) -> &Vt {
        &self.vt
    }

    /// One row of cells, for a renderer. Borrowed, not copied.
    pub fn row(&self, y: usize) -> &[Cell] {
        self.vt.grid().line(y)
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
                    self.conn = None;
                    self.events.push(Event::Disconnected);
                    return Ok(total);
                }
                Err(e) => return Err(e),
            };

            for ev in self.rx_events.drain(..) {
                self.events.push(match ev {
                    TransportEvent::Break => Event::Break,
                    TransportEvent::BadByte(b) => Event::BadByte(b),
                });
            }

            if n > 0 {
                total += n;
                let bytes = std::mem::take(&mut self.rx);
                self.vt.feed(&bytes);
                self.rx = bytes;
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
    pub fn send_text(&mut self, text: &str) -> Result<()> {
        let bytes = text.as_bytes().to_vec();
        self.queue(&bytes);
        self.flush_pending()
    }

    /// Paste, bracketed when the host asked for it (`DECSET 2004`).
    ///
    /// The brackets are the point: without them a shell runs every newline in
    /// the pasted text as a command, which is a well-known way to lose data
    /// to a copied trailing newline.
    pub fn paste(&mut self, text: &str) -> Result<()> {
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

    /// Resize the terminal and tell the far end.
    ///
    /// Both halves matter and they are easy to separate by accident: a grid
    /// that resized without a `TIOCSWINSZ` leaves `vi` drawing to the old
    /// size, and the symptom looks like a redraw bug rather than a missing
    /// ioctl.
    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        self.vt.grid_mut().resize(cols, rows);
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

    /// Feed bytes as though they had arrived from the far end. For local echo
    /// and for tests; it is also how a replayed session log would work.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.vt.feed(bytes);
        self.events.push(Event::Damage);
        self.collect_title();
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
