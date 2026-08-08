//! tt-ffi — the flat C ABI over [`tt_session`].
//!
//! This is the whole of the core/frontend seam. `PLAN.md` puts it there on
//! purpose: the frontend is replaceable because it only ever sees POD types
//! and functions, never a Rust type, a trait object or an allocator. Nothing
//! Win32- or Qt-shaped crosses in the other direction either — no `HWND`, no
//! `QWidget`, no fonts, no glyphs, and no pixels beyond `cell_w`/`cell_h`.
//!
//! ```c
//! TtConfig cfg;
//! tt_config_default(&cfg);
//! TtSession *s = tt_session_new(&cfg);
//!
//! TtSerialParams p;
//! tt_serial_params_default(&p);
//! p.baud = 115200;
//! if (tt_session_connect_serial(s, "/dev/ttyUSB0", &p) != TT_OK)
//!     fprintf(stderr, "%s\n", tt_last_error());
//!
//! for (;;) {
//!     size_t got;
//!     tt_session_pump(s, 20, &got);
//!     const TtEvent *ev;
//!     for (size_t i = 0, n = tt_session_drain_events(s, &ev); i < n; i++)
//!         handle(&ev[i]);
//! }
//! ```
//!
//! ## Three rules the whole header obeys
//!
//! 1. **Every fallible call returns a `TtStatus`**, and only a negative one
//!    means failure. [`tt_last_error`] then describes it, in words meant for a
//!    status bar rather than a log — `tt_conn`'s error type exists precisely
//!    because "busy" and "unplugged" are the same errno to a naive layer, and
//!    throwing that distinction away at the ABI would undo it.
//! 2. **A `const char *` or `const TtCell *` handed back is borrowed**, owned
//!    by whatever produced it, and valid until the next call that could change
//!    it. Each one says which call that is. Nothing here needs a matching
//!    `free` except the two `_free` functions, which say so in their names.
//! 3. **Nothing spawns a thread.** [`tt_session_pump`] blocks for as long as
//!    the caller allows and no longer, so where the loop runs stays the
//!    frontend's decision — see `tt-session`'s README for why.
//!
//! ## Threading
//!
//! A `TtSession *` is not internally locked, and it is a plain `&mut` in
//! disguise: one thread may touch a given session at a time. Moving it between
//! threads is fine — the transports are `Send` — and driving the pump from a
//! worker while the UI thread reads rows is **not**. The error slot is
//! thread-local, so a failure is reported to the thread that caused it.
//!
//! ## Pointers
//!
//! **Null is handled at every entry point**, and the C test proves it: a null
//! session yields an error status or a zero value, a null out-parameter is
//! simply not written, and nothing here can be made to dereference one. What
//! is left is the contract no ABI can escape — a *non-null* pointer must
//! actually point at what it claims to, and a `TtSession *` must not have been
//! freed. That is stated once here rather than as a `# Safety` paragraph on
//! each of forty functions, which is also why the lint below is allowed:
//! marking every `extern "C"` function `unsafe` would change nothing for a C
//! caller and would put a "Safety" heading in every comment in the generated
//! header.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::slice;
use std::time::Duration;

use tt_conn::serial::{
    DataBits, FlowControl, Parity, PinControl, SerialConn, SerialParams, StopBits,
};
use tt_conn::Error;
use tt_grid::Cell;
use tt_session::{Event, Session};
use tt_vt::{Config, Key, Modifiers, MouseEvent, TermId, Tracking};

// --- status ---------------------------------------------------------------

/// The return of every fallible call. Zero is success and negative is
/// failure; the codes exist so a frontend can branch without parsing English,
/// and [`tt_last_error`] carries the sentence for the user.
pub type TtStatus = i32;

pub const TT_OK: TtStatus = 0;
/// A null pointer, a length that overflows, text that is not UTF-8, or a
/// parameter outside its range. Always a bug on the calling side.
pub const TT_ERR_INVALID: TtStatus = -1;
/// The far end is gone. When a pump or a write says this, the session has
/// already dropped the connection and queued a
/// [`TtEventKind::Disconnected`].
///
/// **An open of a path that does not exist reports this too**, rather than
/// some "no such port". That is `tt_conn`'s call and it is the right one: the
/// case is a saved profile naming an adapter that has since been unplugged,
/// and "not found" sends the user hunting for a typo instead of for the
/// cable.
pub const TT_ERR_DISCONNECTED: TtStatus = -2;
/// The port exists but something else holds it — `minicom` in another window,
/// a ModemManager probe. By far the most common serial failure, and the one
/// where the wrong message wastes the most of the user's time.
pub const TT_ERR_BUSY: TtStatus = -3;
/// The port exists and we are not allowed to open it. On Linux, usually not
/// being in `dialout`.
pub const TT_ERR_PERMISSION: TtStatus = -4;
/// A setting this platform cannot express, or that the driver accepted and
/// then ignored — five data bits on an FTDI, for one.
pub const TT_ERR_UNSUPPORTED: TtStatus = -5;
/// Anything else the operating system said.
pub const TT_ERR_IO: TtStatus = -6;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_error(msg: impl Into<Vec<u8>>) {
    // A NUL inside the message would truncate it rather than fail; strip.
    let mut bytes: Vec<u8> = msg.into();
    bytes.retain(|&b| b != 0);
    let c = CString::new(bytes).expect("NULs removed above");
    LAST_ERROR.with(|slot| *slot.borrow_mut() = c);
}

fn fail(status: TtStatus, msg: impl Into<Vec<u8>>) -> TtStatus {
    set_error(msg);
    status
}

fn report(e: Error) -> TtStatus {
    let status = match &e {
        Error::Disconnected => TT_ERR_DISCONNECTED,
        Error::Busy { .. } => TT_ERR_BUSY,
        Error::PermissionDenied { .. } => TT_ERR_PERMISSION,
        Error::Open { .. } => TT_ERR_IO,
        Error::Unsupported(_) => TT_ERR_UNSUPPORTED,
        Error::Io(_) => TT_ERR_IO,
    };
    fail(status, e.to_string())
}

/// Why the last call on **this thread** failed.
///
/// Never null and never stale-dangling: it points at a thread-local buffer
/// that lives as long as the thread, and is overwritten by the next failing
/// call on it. Copy it if you intend to keep it. The contents are unspecified
/// when nothing has failed yet — check the status, not the string.
#[no_mangle]
pub extern "C" fn tt_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// The core's version, for an about box and for a shell checking it did not
/// pick up a stale library.
#[no_mangle]
pub extern "C" fn tt_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

// --- helpers --------------------------------------------------------------

/// Borrow a session, or return `err` with the error slot already set.
macro_rules! session {
    // The void arm: `return ()` is the same thing but reads as a mistake.
    ($ptr:expr) => {
        match unsafe { $ptr.as_mut() } {
            Some(s) => s,
            None => {
                set_error("null TtSession");
                return;
            }
        }
    };
    ($ptr:expr, $err:expr) => {
        match unsafe { $ptr.as_mut() } {
            Some(s) => s,
            None => {
                set_error("null TtSession");
                return $err;
            }
        }
    };
}

macro_rules! session_ref {
    // The void arm: `return ()` is the same thing but reads as a mistake.
    ($ptr:expr) => {
        match unsafe { $ptr.as_ref() } {
            Some(s) => s,
            None => {
                set_error("null TtSession");
                return;
            }
        }
    };
    ($ptr:expr, $err:expr) => {
        match unsafe { $ptr.as_ref() } {
            Some(s) => s,
            None => {
                set_error("null TtSession");
                return $err;
            }
        }
    };
}

/// A `const char *` plus a length as a `&str`, tolerating a length of
/// `SIZE_MAX` to mean "NUL-terminated" so C callers can skip `strlen`.
///
/// # Safety
/// `ptr` must be valid for `len` bytes, or NUL-terminated when `len` is
/// `usize::MAX`.
unsafe fn str_arg<'a>(ptr: *const c_char, len: usize) -> Result<&'a str, TtStatus> {
    if ptr.is_null() {
        return Err(fail(TT_ERR_INVALID, "null string"));
    }
    let bytes = if len == usize::MAX {
        CStr::from_ptr(ptr).to_bytes()
    } else {
        slice::from_raw_parts(ptr.cast::<u8>(), len)
    };
    std::str::from_utf8(bytes).map_err(|e| fail(TT_ERR_INVALID, format!("not UTF-8: {e}")))
}

fn cstring(s: &str) -> CString {
    let mut bytes = s.as_bytes().to_vec();
    bytes.retain(|&b| b != 0);
    CString::new(bytes).expect("NULs removed above")
}

// --- configuration --------------------------------------------------------

/// What a session is created with.
///
/// Deliberately small. `tt_vt::Config` has thirty-odd fields and every one of
/// them is a `TERATERM.INI` key, which makes it the settings schema's job
/// (Stage 2) rather than something to hand-transcribe into a C struct now and
/// throw away later. What is here is what a window has to decide before it can
/// draw: how big the terminal is, how much history to keep, and which DEC
/// terminal to answer as.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtConfig {
    pub cols: usize,
    pub rows: usize,
    /// Lines of scrollback to retain. Tera Term's `ts.ScrollBuffMax`.
    pub scrollback: usize,
    pub term_id: TermId,
    /// The character cell in pixels. The core wants it for exactly two things:
    /// turning a mouse position into a cell, and SGR-pixel mouse reports,
    /// which do not convert at all. Zero means "not known yet" and mouse
    /// reporting will be wrong until [`tt_session_set_cell_pixels`] says
    /// otherwise.
    pub cell_w: i32,
    pub cell_h: i32,
}

/// Fill `out` with the defaults — 80x24, 10000 lines of scrollback, VT100.
#[no_mangle]
pub extern "C" fn tt_config_default(out: *mut TtConfig) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    let d = Config::default();
    *out = TtConfig {
        cols: d.cols,
        rows: d.rows,
        scrollback: d.scrollback_max,
        term_id: d.term_id,
        cell_w: d.cell_w,
        cell_h: d.cell_h,
    };
}

// --- the session ----------------------------------------------------------

/// A terminal, and optionally something for it to talk to. Opaque.
pub struct TtSession {
    session: Session,
    /// Drained events, kept alive until the next drain because each `Title`
    /// hands out a borrowed pointer.
    events: Vec<TtEvent>,
    event_texts: Vec<CString>,
    describe: CString,
    title: CString,
}

/// Create a session. Returns null only if `config` is null.
///
/// The terminal exists immediately and can be fed and drawn with nothing
/// connected, which is what makes a local echo test and a replayed session log
/// work.
#[no_mangle]
pub extern "C" fn tt_session_new(config: *const TtConfig) -> *mut TtSession {
    let Some(c) = (unsafe { config.as_ref() }) else {
        set_error("null TtConfig");
        return ptr::null_mut();
    };
    let mut cfg = Config {
        cols: c.cols.max(1),
        rows: c.rows.max(1),
        scrollback_max: c.scrollback,
        term_id: c.term_id,
        ..Config::default()
    };
    if c.cell_w > 0 && c.cell_h > 0 {
        cfg.cell_w = c.cell_w;
        cfg.cell_h = c.cell_h;
    }
    let session = Session::new(cfg);
    Box::into_raw(Box::new(TtSession {
        title: cstring(session.vt().title()),
        session,
        events: Vec::new(),
        event_texts: Vec::new(),
        describe: CString::default(),
    }))
}

/// Destroy a session, closing its connection. Null is a no-op.
#[no_mangle]
pub extern "C" fn tt_session_free(session: *mut TtSession) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

/// How long a write may block before it returns short. 200 ms by default; a
/// frontend pumping from its UI thread wants it well under a frame.
#[no_mangle]
pub extern "C" fn tt_session_set_write_timeout(session: *mut TtSession, ms: u32) {
    let s = session!(session);
    s.session
        .set_write_timeout(Duration::from_millis(ms.into()));
}

// --- reading the screen ---------------------------------------------------

#[no_mangle]
pub extern "C" fn tt_session_cols(session: *const TtSession) -> usize {
    session_ref!(session, 0).session.grid().cols()
}

#[no_mangle]
pub extern "C" fn tt_session_rows(session: *const TtSession) -> usize {
    session_ref!(session, 0).session.grid().rows()
}

/// One row of cells, borrowed straight out of the grid — no copy and no
/// allocation, which is the point of a POD [`Cell`].
///
/// `out_len` receives the row's length, which is always
/// [`tt_session_cols`]. Null on an out-of-range `y`.
///
/// **Valid until the next call that can change the grid** — a pump, a feed, a
/// resize, or anything that sends. In practice: read every row you are about
/// to paint, paint, then pump.
#[no_mangle]
pub extern "C" fn tt_session_row(
    session: *const TtSession,
    y: usize,
    out_len: *mut usize,
) -> *const Cell {
    let s = session_ref!(session, ptr::null());
    if y >= s.session.grid().rows() {
        set_error("row out of range");
        return ptr::null();
    }
    let row = s.session.row(y);
    if let Some(len) = unsafe { out_len.as_mut() } {
        *len = row.len();
    }
    row.as_ptr()
}

/// Where the cursor is and whether to draw it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtCursor {
    pub x: usize,
    pub y: usize,
    /// DECTCEM. A hidden cursor is still *somewhere*, so the position stays
    /// meaningful — `x`/`y` are not zeroed when this is false.
    pub visible: bool,
    /// Tera Term's `Wrap`: the last glyph landed on the right margin and the
    /// *next* one wraps. Worth having because it explains a cursor that looks
    /// stuck on the last column.
    pub pending_wrap: bool,
}

#[no_mangle]
pub extern "C" fn tt_session_cursor(session: *const TtSession, out: *mut TtCursor) {
    let s = session_ref!(session);
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    let c = s.session.grid().cursor;
    *out = TtCursor {
        x: c.x,
        y: c.y,
        visible: s.session.vt().cursor_visible(),
        pending_wrap: c.pending_wrap,
    };
}

/// The window title, from OSC 0 / OSC 2. Never null; empty before the host
/// sets one.
///
/// Borrowed, and **valid until the next call to this function** on this
/// session. The [`TtEventKind::Title`] event carries the same string and is
/// the cheaper way to notice a change.
#[no_mangle]
pub extern "C" fn tt_session_title(session: *mut TtSession) -> *const c_char {
    let s = session!(session, c"".as_ptr());
    s.title = cstring(s.session.vt().title());
    s.title.as_ptr()
}

/// DECSCNM. The grid does not apply it — swapping every cell's colours is the
/// renderer's job, and doing it in the buffer would corrupt what a copy or a
/// session log sees.
#[no_mangle]
pub extern "C" fn tt_session_reverse_video(session: *const TtSession) -> bool {
    session_ref!(session, false).session.vt().reverse_video()
}

/// Whether the frontend should be tracking the mouse at all, and therefore
/// whether a drag belongs to the host or to text selection.
#[no_mangle]
pub extern "C" fn tt_session_mouse_tracking(session: *const TtSession) -> Tracking {
    session_ref!(session, Tracking::None)
        .session
        .mouse_tracking()
}

/// A palette entry, for the painter. False for `index > 255`.
///
/// Tera Term stores one byte of colour per cell, so this is the *whole* colour
/// story — `SGR 38;2;r;g;b` has already resolved to the nearest index by the
/// time a cell holds it. Entries 0-15 are the VGA values, not xterm's.
///
/// Note what the cell says: `fg`/`bg` mean a palette index only when
/// `TT_ATTR2_FORE` / `TT_ATTR2_BACK` is set in `attrs`. Without the bit the
/// cell is asking for the terminal's *configured* default text colour, which
/// is the frontend's to choose — painting index 0 there gives a black-on-black
/// screen.
#[no_mangle]
pub extern "C" fn tt_palette_rgb(index: u32, r: *mut u8, g: *mut u8, b: *mut u8) -> bool {
    let Ok(i) = usize::try_from(index) else {
        return false;
    };
    let Some(&(pr, pg, pb)) = tt_vt::palette::default_palette().get(i) else {
        return false;
    };
    unsafe {
        if let Some(r) = r.as_mut() {
            *r = pr;
        }
        if let Some(g) = g.as_mut() {
            *g = pg;
        }
        if let Some(b) = b.as_mut() {
            *b = pb;
        }
    }
    true
}

// --- events ---------------------------------------------------------------

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtEventKind {
    /// The screen changed. Coarse on purpose: a full 80x24 repaint measured
    /// 3.9 ms on the target Qt, about 40x what a 115200 baud link can dirty,
    /// so per-row damage is an optimisation to add when something asks for it.
    Damage = 0,
    /// OSC 0 / OSC 2. `text` is the new title.
    Title = 1,
    /// A line break arrived. On a serial console this is how a host asks for
    /// attention, and dropping it is a real loss of function.
    Break = 2,
    /// A byte arrived with a parity or framing error. `byte` is what came
    /// through, kept rather than dropped: it is usually still readable, and
    /// silently losing it makes a bad cable look like a bad program.
    BadByte = 3,
    /// The transport went away. Reported once; the screen is left alone,
    /// because the text explaining why it dropped is the reason anyone looks.
    Disconnected = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtEvent {
    pub kind: TtEventKind,
    /// Meaningful for [`TtEventKind::BadByte`] only.
    pub byte: u8,
    /// Meaningful for [`TtEventKind::Title`] only; null otherwise.
    pub text: *const c_char,
}

/// Take everything that has happened since the last drain.
///
/// `*out` receives the array and the return is its length; `*out` is null when
/// the length is zero. **Both the array and every `text` in it are borrowed
/// and valid until the next drain on this session.**
///
/// Drained rather than delivered by callback: a callback would have to be
/// `Send` and would fire on whichever thread the pump happens to be on, which
/// is exactly what a UI toolkit cannot take.
#[no_mangle]
pub extern "C" fn tt_session_drain_events(
    session: *mut TtSession,
    out: *mut *const TtEvent,
) -> usize {
    let s = session!(session, 0);
    s.events.clear();
    s.event_texts.clear();
    for ev in s.session.drain_events() {
        let (kind, byte, text) = match ev {
            Event::Damage => (TtEventKind::Damage, 0, ptr::null()),
            Event::Title(t) => {
                s.event_texts.push(cstring(&t));
                let p = s.event_texts.last().expect("just pushed").as_ptr();
                (TtEventKind::Title, 0, p)
            }
            Event::Break => (TtEventKind::Break, 0, ptr::null()),
            Event::BadByte(b) => (TtEventKind::BadByte, b, ptr::null()),
            Event::Disconnected => (TtEventKind::Disconnected, 0, ptr::null()),
        };
        s.events.push(TtEvent { kind, byte, text });
    }
    if let Some(out) = unsafe { out.as_mut() } {
        *out = if s.events.is_empty() {
            ptr::null()
        } else {
            s.events.as_ptr()
        };
    }
    s.events.len()
}

// --- the loop -------------------------------------------------------------

/// Move bytes in both directions for at most `budget_ms`, and write how many
/// arrived to `out_bytes` (which may be null).
///
/// Reading is bounded by the transport's own timeout, so the budget is a
/// ceiling on the call rather than a sleep: a quiet line returns promptly with
/// zero bytes and no events, which is the normal state of a serial console and
/// has to stay cheap.
#[no_mangle]
pub extern "C" fn tt_session_pump(
    session: *mut TtSession,
    budget_ms: u32,
    out_bytes: *mut usize,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.pump(Duration::from_millis(budget_ms.into())) {
        Ok(n) => {
            if let Some(out) = unsafe { out_bytes.as_mut() } {
                *out = n;
            }
            TT_OK
        }
        Err(e) => report(e),
    }
}

// --- input ----------------------------------------------------------------

/// Send a key, encoded by the core because which form it takes is terminal
/// state the frontend never sees — and because `KEYBOARD.CNF` compatibility
/// means the table has to be upstream's, not one a frontend invented.
///
/// `out_sent` (may be null) receives whether anything reached the wire: Hold,
/// Print and Break have key ids so a config can bind them and put nothing on
/// the wire.
///
/// Ordinary printable characters do **not** come through here — use
/// [`tt_session_send_text`].
#[no_mangle]
pub extern "C" fn tt_session_send_key(
    session: *mut TtSession,
    key: Key,
    out_sent: *mut bool,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.send_key(key) {
        Ok(sent) => {
            if let Some(out) = unsafe { out_sent.as_mut() } {
                *out = sent;
            }
            TT_OK
        }
        Err(e) => report(e),
    }
}

/// Send typed text. UTF-8; `len` may be `SIZE_MAX` for NUL-terminated.
#[no_mangle]
pub extern "C" fn tt_session_send_text(
    session: *mut TtSession,
    text: *const c_char,
    len: usize,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let text = match unsafe { str_arg(text, len) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    match s.session.send_text(text) {
        Ok(()) => TT_OK,
        Err(e) => report(e),
    }
}

/// Paste, bracketed when the host asked for it (`DECSET 2004`).
///
/// Separate from [`tt_session_send_text`] because the brackets are the whole
/// point: without them a shell runs every newline in the pasted text as a
/// command, which is a well-known way to lose data to a copied trailing
/// newline.
#[no_mangle]
pub extern "C" fn tt_session_paste(
    session: *mut TtSession,
    text: *const c_char,
    len: usize,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let text = match unsafe { str_arg(text, len) } {
        Ok(t) => t,
        Err(e) => return e,
    };
    match s.session.paste(text) {
        Ok(()) => TT_OK,
        Err(e) => report(e),
    }
}

/// Report a mouse event, in **window pixels** rather than cells.
///
/// Pixels because that is what upstream's `MouseReport` takes and because
/// `DECSET 1016` reports them back unconverted, so a cell-only API could not
/// express it. The core converts using the cell size it was told.
///
/// `out_consumed` (may be null) receives whether the terminal used the event.
/// If it did not, the click is the frontend's, for selection.
#[no_mangle]
pub extern "C" fn tt_session_mouse(
    session: *mut TtSession,
    event: MouseEvent,
    button: u8,
    px: i32,
    py: i32,
    mods: Modifiers,
    out_consumed: *mut bool,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.mouse(event, button, px, py, mods) {
        Ok(consumed) => {
            if let Some(out) = unsafe { out_consumed.as_mut() } {
                *out = consumed;
            }
            TT_OK
        }
        Err(e) => report(e),
    }
}

/// Focus gained or lost. Silent unless the host asked for it
/// (`DECSET 1004`), so it is safe to call on every focus change.
#[no_mangle]
pub extern "C" fn tt_session_focus(session: *mut TtSession, focused: bool) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.focus(focused) {
        Ok(()) => TT_OK,
        Err(e) => report(e),
    }
}

/// Resize the terminal **and** tell the far end.
///
/// Both halves, in one call, because they are easy to separate by accident: a
/// grid that resized without a `TIOCSWINSZ` leaves `vi` drawing to the old
/// size, and the symptom looks like a redraw bug rather than a missing ioctl.
#[no_mangle]
pub extern "C" fn tt_session_resize(session: *mut TtSession, cols: usize, rows: usize) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    if cols == 0 || rows == 0 {
        return fail(TT_ERR_INVALID, "resize to zero");
    }
    match s.session.resize(cols, rows) {
        Ok(()) => TT_OK,
        Err(e) => report(e),
    }
}

/// Tell the core how big a character cell is, in pixels. Needed only for
/// mouse reporting; the core learns nothing else about pixels.
#[no_mangle]
pub extern "C" fn tt_session_set_cell_pixels(session: *mut TtSession, w: i32, h: i32) {
    let s = session!(session);
    s.session.set_cell_pixels(w, h);
}

/// Send a line break — `CommSendBreak`. On a serial console this is how you
/// reach a `getty` or drop a Sun box to its PROM. A no-op with nothing
/// connected.
#[no_mangle]
pub extern "C" fn tt_session_send_break(session: *mut TtSession, ms: u32) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.send_break(Duration::from_millis(ms.into())) {
        Ok(()) => TT_OK,
        Err(e) => report(e),
    }
}

/// Feed bytes as though they had arrived from the far end.
///
/// For local echo and for tests, and it is how a replayed session log will
/// work. Queues a [`TtEventKind::Damage`].
#[no_mangle]
pub extern "C" fn tt_session_feed(session: *mut TtSession, bytes: *const u8, len: usize) {
    let s = session!(session);
    if bytes.is_null() || len == 0 {
        return;
    }
    s.session.feed(unsafe { slice::from_raw_parts(bytes, len) });
}

// --- connections ----------------------------------------------------------

/// The serial line settings, one field per `commlib.c` DCB field that Tera
/// Term actually sets.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSerialParams {
    pub baud: u32,
    /// 5 to 8. **Five and six are usually a lie**: an FTDI refuses `CS6` and
    /// *accepts* `CS5` while still putting eight bits on the wire, so
    /// [`tt_session_connect_serial`] reads the setting back and fails with
    /// [`TT_ERR_UNSUPPORTED`] rather than let a dialog claim five.
    pub data_bits: u8,
    pub parity: Parity,
    /// 1 or 2.
    pub stop_bits: u8,
    pub flow: FlowControl,
    /// The XON/XOFF pair. Tera Term hardcodes the standard characters in
    /// `CommResetSerial` but `TTTSet` carries them, so they are settable.
    pub xon: u8,
    pub xoff: u8,
    pub dtr: PinControl,
    pub rts: PinControl,
    /// Escape the input so a line break arrives as a
    /// [`TtEventKind::Break`] instead of a `0x00`. Without it the two are the
    /// same byte and there is no way to tell them apart.
    pub detect_break: bool,
    /// How long a read waits before returning empty — short enough that a
    /// disconnect is noticed promptly, long enough not to spin.
    pub read_timeout_ms: u32,
}

/// Fill `out` with Tera Term's own defaults: 9600 8N1, no flow control, DTR
/// and RTS asserted, break detection on.
#[no_mangle]
pub extern "C" fn tt_serial_params_default(out: *mut TtSerialParams) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    let d = SerialParams::default();
    *out = TtSerialParams {
        baud: d.baud,
        data_bits: 8,
        parity: d.parity,
        stop_bits: 1,
        flow: d.flow,
        xon: d.xon,
        xoff: d.xoff,
        dtr: d.dtr,
        rts: d.rts,
        detect_break: d.detect_break,
        read_timeout_ms: d.read_timeout.as_millis() as u32,
    };
}

impl TtSerialParams {
    fn to_rust(self) -> Result<SerialParams, TtStatus> {
        let data_bits = match self.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,
            n => return Err(fail(TT_ERR_INVALID, format!("data_bits {n}, want 5 to 8"))),
        };
        let stop_bits = match self.stop_bits {
            1 => StopBits::One,
            2 => StopBits::Two,
            n => return Err(fail(TT_ERR_INVALID, format!("stop_bits {n}, want 1 or 2"))),
        };
        Ok(SerialParams {
            baud: self.baud,
            data_bits,
            parity: self.parity,
            stop_bits,
            flow: self.flow,
            xon: self.xon,
            xoff: self.xoff,
            dtr: self.dtr,
            rts: self.rts,
            detect_break: self.detect_break,
            read_timeout: Duration::from_millis(self.read_timeout_ms.into()),
        })
    }
}

/// Open a serial port and attach it. Replaces any current connection.
///
/// Pass the port's `open_path`, not its `device` — see [`TtPortInfo`].
///
/// The terminal is **not** reset: reconnecting a serial console to the same
/// session is how you keep the scrollback that explains why it dropped.
#[no_mangle]
pub extern "C" fn tt_session_connect_serial(
    session: *mut TtSession,
    path: *const c_char,
    params: *const TtSerialParams,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let Some(params) = (unsafe { params.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtSerialParams");
    };
    let params = match params.to_rust() {
        Ok(p) => p,
        Err(e) => return e,
    };
    match SerialConn::open(path, &params) {
        Ok(conn) => {
            s.session.connect(Box::new(conn));
            TT_OK
        }
        Err(e) => report(e),
    }
}

/// Drop the connection. The screen is left alone. A no-op when there is none.
#[no_mangle]
pub extern "C" fn tt_session_disconnect(session: *mut TtSession) {
    let s = session!(session);
    s.session.disconnect();
}

#[no_mangle]
pub extern "C" fn tt_session_is_connected(session: *const TtSession) -> bool {
    session_ref!(session, false).session.is_connected()
}

/// A short name for the status line — `/dev/ttyUSB0`, `user@host`. Null when
/// nothing is connected.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_describe(session: *mut TtSession) -> *const c_char {
    let s = session!(session, ptr::null());
    match s.session.describe() {
        Some(d) => {
            s.describe = cstring(&d);
            s.describe.as_ptr()
        }
        None => ptr::null(),
    }
}

// --- port enumeration -----------------------------------------------------

/// One serial port, as a picker wants to show it. Every string is borrowed
/// from the owning [`TtPortList`] and dies with it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtPortInfo {
    /// The kernel device node — `/dev/ttyUSB0`. Fine to show; wrong to store.
    pub device: *const c_char,
    /// **What to pass to [`tt_session_connect_serial`], and what to save in a
    /// session profile.** `/dev/ttyUSB<n>` is assigned in attach order, so
    /// unplugging two adapters and replugging them the other way round swaps
    /// their names; this is the `by-path` name where there is one, which
    /// encodes the USB topology and holds still.
    pub open_path: *const c_char,
    /// The `by-path` name on its own, or null when the bus has none.
    pub stable_id: *const c_char,
    /// One line for a picker — device plus product name.
    pub label: *const c_char,
    /// Whether the USB fields below mean anything.
    pub has_usb: bool,
    pub vid: u16,
    pub pid: u16,
    /// Any of these may be null. `serial` in particular is often absent on
    /// multi-port adapters, which is why it is not the identity.
    pub manufacturer: *const c_char,
    pub product: *const c_char,
    pub serial: *const c_char,
}

/// An owned list of ports. Free it with [`tt_port_list_free`].
pub struct TtPortList {
    infos: Vec<TtPortInfo>,
    /// The strings the infos point into. Order does not matter; keeping them
    /// alive does.
    _strings: Vec<CString>,
}

/// Every serial port the system can see, sorted by device node so a picker
/// does not reshuffle between refreshes. Null on failure.
#[no_mangle]
pub extern "C" fn tt_serial_enumerate() -> *mut TtPortList {
    let ports = match tt_conn::serial::enumerate() {
        Ok(p) => p,
        Err(e) => {
            report(e);
            return ptr::null_mut();
        }
    };
    let mut strings: Vec<CString> = Vec::new();
    let mut infos = Vec::with_capacity(ports.len());
    // Two passes: `strings` must stop reallocating before any pointer into it
    // is taken, or every earlier pointer dangles.
    for p in &ports {
        strings.push(cstring(&p.device));
        strings.push(cstring(p.open_path()));
        strings.push(cstring(&p.label()));
        strings.push(cstring(p.stable_id.as_deref().unwrap_or("")));
        let usb = p.usb.as_ref();
        strings.push(cstring(
            usb.and_then(|u| u.manufacturer.as_deref()).unwrap_or(""),
        ));
        strings.push(cstring(
            usb.and_then(|u| u.product.as_deref()).unwrap_or(""),
        ));
        strings.push(cstring(usb.and_then(|u| u.serial.as_deref()).unwrap_or("")));
    }
    for (i, p) in ports.iter().enumerate() {
        let base = i * 7;
        let at = |n: usize| strings[base + n].as_ptr();
        // An absent optional is null, not an empty string: "" and "no serial
        // number" are different answers and a picker shows them differently.
        let or_null = |n: usize, present: bool| if present { at(n) } else { ptr::null() };
        let usb = p.usb.as_ref();
        infos.push(TtPortInfo {
            device: at(0),
            open_path: at(1),
            label: at(2),
            stable_id: or_null(3, p.stable_id.is_some()),
            has_usb: usb.is_some(),
            vid: usb.map_or(0, |u| u.vid),
            pid: usb.map_or(0, |u| u.pid),
            manufacturer: or_null(4, usb.is_some_and(|u| u.manufacturer.is_some())),
            product: or_null(5, usb.is_some_and(|u| u.product.is_some())),
            serial: or_null(6, usb.is_some_and(|u| u.serial.is_some())),
        });
    }
    Box::into_raw(Box::new(TtPortList {
        infos,
        _strings: strings,
    }))
}

#[no_mangle]
pub extern "C" fn tt_port_list_len(list: *const TtPortList) -> usize {
    match unsafe { list.as_ref() } {
        Some(l) => l.infos.len(),
        None => 0,
    }
}

/// Borrow one entry. Null when `index` is out of range. Valid until the list
/// is freed.
#[no_mangle]
pub extern "C" fn tt_port_list_at(list: *const TtPortList, index: usize) -> *const TtPortInfo {
    match unsafe { list.as_ref() } {
        Some(l) => l.infos.get(index).map_or(ptr::null(), |p| p as *const _),
        None => ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn tt_port_list_free(list: *mut TtPortList) {
    if !list.is_null() {
        drop(unsafe { Box::from_raw(list) });
    }
}
