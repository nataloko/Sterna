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
use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;
use std::slice;
use std::time::Duration;

use tt_conn::serial::{
    DataBits, FlowControl, Parity, PinControl, SerialConn, SerialParams, StopBits,
};
use tt_conn::ssh::{
    AuthPromptKind, HostKeyDecision, HostKeyPolicy, KnownHosts, SshConfig, SshConnect, SshParams,
    Step, Verdict,
};
use tt_conn::pty::{PtyConn, PtyParams};
use tt_conn::telnet::{TelnetConn, TelnetMode, TelnetParams};
use tt_conn::Error;
use tt_grid::Cell;
use tt_session::{Event, LogMode, LogOptions, Session, Timestamp};
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
/// The SSH protocol failed — the socket, the banner, key exchange, opening
/// the channel.
pub const TT_ERR_SSH: TtStatus = -7;
/// The far end is not who `known_hosts` says it is, or is who it says must be
/// refused. **Separate from [`TT_ERR_SSH`] because a frontend must not offer
/// to retry**: this is the one failure where the right affordance is none.
pub const TT_ERR_HOST_KEY: TtStatus = -8;
/// Every authentication method failed or was not on offer. [`tt_last_error`]
/// names what the server said it would still accept, which is the only thing
/// that makes the message actionable.
pub const TT_ERR_AUTH: TtStatus = -9;

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
        Error::Ssh(_) => TT_ERR_SSH,
        Error::HostKey(_) => TT_ERR_HOST_KEY,
        Error::Auth { .. } => TT_ERR_AUTH,
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
    log_path: CString,
    close_note: CString,
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
        log_path: CString::default(),
        close_note: CString::default(),
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

/// One row of what the window is **showing**, borrowed straight out of the
/// grid — no copy and no allocation, which is the point of a POD [`Cell`].
///
/// `y` runs over the viewport, not over the grid. With
/// [`tt_session_view_offset`] at zero — which it is until something scrolls
/// back — the two are the same thing and this is the live screen. Otherwise
/// row 0 is that many lines up in the scrollback.
///
/// `out_len` receives the row's length, which is always
/// [`tt_session_cols`], history included. Null on an out-of-range `y`.
///
/// **Valid until the next call that can change the grid** — a pump, a feed, a
/// resize, a scroll, or anything that sends. In practice: read every row you
/// are about to paint, paint, then pump.
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

// --- session logging ------------------------------------------------------

/// What a session log records and how it is written.
///
/// Every field is a `TERATERM.INI` key, so this is one of the few places the
/// settings schema will land in Stage 2 rather than a struct to keep growing
/// by hand. It is here now because a console capture is the reason people
/// leave a serial terminal open, not a nicety.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtLogOptions {
    /// True for a byte-for-byte capture — `ts.LogBinary`. False logs the text
    /// the terminal decided to display, with escape sequences already
    /// consumed by the parser.
    ///
    /// **A raw log is silently untimestamped.** That is upstream's behaviour
    /// and the right one: a `[time] ` in the middle of a byte capture makes it
    /// no longer replayable.
    pub raw: bool,
    pub timestamp: Timestamp,
    /// Add to an existing file rather than truncating it.
    pub append: bool,
    /// Rotate past this many bytes. Zero disables rotation, as does
    /// `rotate_keep` of zero.
    pub rotate_size: u64,
    /// Generations to keep: `file.1` is the newest, `file.<n>` the oldest.
    pub rotate_keep: u32,
    /// Write CR LF for each line rather than LF. Upstream always does;
    /// this defaults to off, because the artefact is a text file read on
    /// Linux.
    pub crlf: bool,
}

/// Fill `out` with the defaults: text, no timestamp, truncate, no rotation.
#[no_mangle]
pub extern "C" fn tt_log_options_default(out: *mut TtLogOptions) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        set_error("null TtLogOptions");
        return;
    };
    let d = LogOptions::default();
    *out = TtLogOptions {
        raw: d.mode == LogMode::Raw,
        timestamp: d.timestamp,
        append: d.append,
        rotate_size: d.rotate_size,
        rotate_keep: d.rotate_keep,
        crlf: d.crlf,
    };
}

/// Start writing a session log to `path`, replacing any log already open.
///
/// Nothing is logged retroactively — the capture starts here. (Upstream can
/// prepend the scrollback; the function it uses to do that is one of the
/// upstream bugs on file, since it truncates every line at its first wide
/// character, so that option waits for the report to be answered.)
#[no_mangle]
pub extern "C" fn tt_session_log_start(
    session: *mut TtSession,
    path: *const c_char,
    options: *const TtLogOptions,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let Some(o) = (unsafe { options.as_ref() }) else {
        set_error("null TtLogOptions");
        return TT_ERR_INVALID;
    };
    let opts = LogOptions {
        mode: if o.raw { LogMode::Raw } else { LogMode::Text },
        timestamp: o.timestamp,
        append: o.append,
        rotate_size: o.rotate_size,
        rotate_keep: o.rotate_keep,
        crlf: o.crlf,
    };
    match s.session.start_log(std::path::Path::new(path), opts) {
        Ok(()) => TT_OK,
        Err(e) => {
            set_error(e.to_string());
            TT_ERR_IO
        }
    }
}

/// Close the log, flushing it. A no-op when none is open.
#[no_mangle]
pub extern "C" fn tt_session_log_stop(session: *mut TtSession) {
    let s = session!(session);
    s.session.stop_log();
}

/// The path being logged to, or null when nothing is.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_log_path(session: *mut TtSession) -> *const c_char {
    let s = session!(session, ptr::null());
    match s.session.log_path() {
        Some(p) => {
            s.log_path = CString::new(p.to_string_lossy().as_bytes()).unwrap_or_default();
            s.log_path.as_ptr()
        }
        None => ptr::null(),
    }
}

/// Bytes written to the log since it was opened, across all generations.
#[no_mangle]
pub extern "C" fn tt_session_log_bytes(session: *const TtSession) -> u64 {
    session_ref!(session, 0).session.log_bytes()
}

/// How many lines of history there are to scroll through.
#[no_mangle]
pub extern "C" fn tt_session_scrollback_len(session: *const TtSession) -> usize {
    session_ref!(session, 0).session.scrollback_len()
}

/// How far back the view is, in lines. Zero is the live screen.
///
/// **Read it again after every pump.** It is not a value the frontend owns:
/// the core moves it so that a scrolled-back view stays on the same *lines*
/// while the host keeps printing. A scrollbar that assumed its own last write
/// was still current would fight the terminal for the thumb.
#[no_mangle]
pub extern "C" fn tt_session_view_offset(session: *const TtSession) -> usize {
    session_ref!(session, 0).session.view_offset()
}

/// Scroll the view back by `offset` lines, or to the live screen with zero.
///
/// Clamped to the history that exists, so `SIZE_MAX` means "as far back as it
/// goes" and needs no separate call.
#[no_mangle]
pub extern "C" fn tt_session_set_view_offset(session: *mut TtSession, offset: usize) {
    let s = session!(session);
    s.session.set_view_offset(offset);
}

/// Which viewport row the cursor is on, or false when it is not in view.
///
/// The cursor belongs to the live screen, so scrolling back moves it *down*
/// and eventually off the bottom. A painter that used [`TtCursor::y`] directly
/// would draw a cursor onto a line of history it has nothing to do with.
#[no_mangle]
pub extern "C" fn tt_session_cursor_view_row(session: *const TtSession, out: *mut usize) -> bool {
    let s = session_ref!(session, false);
    match s.session.cursor_view_row() {
        Some(y) => {
            if let Some(out) = unsafe { out.as_mut() } {
                *out = y;
            }
            true
        }
        None => false,
    }
}

/// Where the cursor is and whether to draw it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtCursor {
    pub x: usize,
    /// The row on the **live screen**, which is not where to paint it once the
    /// view has scrolled back — [`tt_session_cursor_view_row`] is.
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

/// Whether the Backspace key should send `BS` (0x08) rather than `DEL`
/// (0x7F) — `ts.BSKey`, which `DECSET 67` (DECBKM) sets and resets.
///
/// One of the two keys a frontend has to encode itself, because upstream
/// handles them in `KeyDown` rather than in the key table and so they are not
/// [`Key`]s. The other is Return, which needs nothing: send `"\r"` through
/// [`tt_session_send_text`] and LNM is applied there.
///
/// Getting this backwards is not cosmetic. A host expecting DEL and receiving
/// BS erases nothing and the line editor beeps, which reads as a broken
/// keyboard rather than as a mode.
#[no_mangle]
pub extern "C" fn tt_session_backspace_sends_bs(session: *const TtSession) -> bool {
    session_ref!(session, false)
        .session
        .vt()
        .backspace_sends_bs()
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
    /// The session log could not be written and has been closed. `text` says
    /// why. Reported once — a disk that filled up will not un-fill, and
    /// retrying on every pump turns one problem into a stall.
    LogFailed = 5,
    /// The **far end** says the terminal should be this size — `cols` and
    /// `rows`. Telnet's NAWS, arriving backwards: RFC 1073 defines it
    /// client-to-server, and a console server sends it the other way to say
    /// what the equipment behind it actually is.
    ///
    /// **The core does not resize itself on this.** The window owns its own
    /// size, and a grid that changed under the frontend would leave it
    /// painting the wrong number of cells. Call [`tt_session_resize`] — and
    /// resize the window with it — or ignore it.
    Resize = 6,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtEvent {
    pub kind: TtEventKind,
    /// Meaningful for [`TtEventKind::BadByte`] only.
    pub byte: u8,
    /// Meaningful for [`TtEventKind::Resize`] only.
    pub cols: u16,
    pub rows: u16,
    /// Meaningful for [`TtEventKind::Title`] and [`TtEventKind::LogFailed`];
    /// null otherwise.
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
        let mut size = (0u16, 0u16);
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
            Event::Resize { cols, rows } => {
                size = (cols, rows);
                (TtEventKind::Resize, 0, ptr::null())
            }
            Event::LogFailed(msg) => {
                s.event_texts.push(CString::new(msg).unwrap_or_default());
                let p = s.event_texts.last().expect("just pushed").as_ptr();
                (TtEventKind::LogFailed, 0, p)
            }
        };
        s.events.push(TtEvent {
            kind,
            byte,
            cols: size.0,
            rows: size.1,
            text,
        });
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

/// How many bytes are still waiting for the far end.
///
/// Non-zero means flow control held the line — CTS low, an XOFF, a DSR that
/// dropped — and a short write left the rest queued for the next
/// [`tt_session_pump`].
///
/// **A frontend that waits on [`tt_session_poll_fd`] has to watch this**, or it
/// will appear to drop keystrokes. That descriptor wakes on bytes *arriving*,
/// and a device asserting backpressure is usually not sending any, so the pump
/// that would flush the queue never happens. Run a short retry timer while
/// this is non-zero and stop it when it reaches zero; the idle case then still
/// costs nothing.
#[no_mangle]
pub extern "C" fn tt_session_pending_out(session: *const TtSession) -> usize {
    let s = session_ref!(session, 0);
    s.session.pending_out()
}

/// A descriptor that becomes readable when [`tt_session_pump`] has something
/// to do — `-1` when there is none.
///
/// Hand it to whatever the frontend already waits on: `QSocketNotifier`,
/// `poll(2)`, `epoll`, a `GSource`. When it fires, call `tt_session_pump` with
/// a budget of **0**, which reads exactly once and returns.
///
/// This exists because the two obvious event loops are both wrong. Pumping
/// from the UI thread blocks for the transport's read timeout every time the
/// line is quiet, which on a serial console is nearly always, and the window
/// stops repainting for that long. Pumping on a timer instead trades the
/// freeze for a wakeup every frame, forever, to discover that nothing arrived
/// — on a terminal whose whole claim is being light.
///
/// Three things to get right:
///
/// - **Re-read it after every connect and disconnect**, including the implicit
///   disconnect a [`TtEventKind::Disconnected`] event reports. It belongs to
///   the transport, not to the session.
/// - **Readable does not promise bytes.** A pump may still report zero — the
///   break decoder can be holding a partial `PARMRK` escape. It is a wakeup,
///   not a guarantee, and a frontend that repaints only on a `Damage` event is
///   already doing the right thing.
/// - **Do not close it, and do not keep it past a disconnect.** It is the
///   transport's own descriptor, not a copy.
///
/// Returns `-1` on Windows, where a serial port is a `HANDLE` and an
/// `OVERLAPPED` event rather than a descriptor. A frontend that gets `-1` has
/// to fall back to a timer, which is what that platform will need until this
/// grows a second spelling.
#[no_mangle]
pub extern "C" fn tt_session_poll_fd(session: *const TtSession) -> c_int {
    let s = session_ref!(session, -1);
    #[cfg(unix)]
    {
        s.session.poll_fd().unwrap_or(-1)
    }
    #[cfg(not(unix))]
    {
        let _ = s;
        -1
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

/// How much of the telnet protocol to speak.
pub type TtTelnetMode = u32;

/// Every byte is data, `0xFF` included. What a console server's per-line port
/// needs, and the only mode that cannot corrupt a binary stream.
pub const TT_TELNET_RAW: TtTelnetMode = 0;
/// Data until the first `IAC` arrives, and telnet from then on. Upstream's
/// `TelAutoDetect`, which defaults on.
pub const TT_TELNET_AUTO: TtTelnetMode = 1;
/// Telnet from the first byte, opening with the negotiation upstream opens
/// with. Upstream does this only when the port is 23.
pub const TT_TELNET_NEGOTIATE: TtTelnetMode = 2;

/// What to tell the far end about this terminal.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtTelnetParams {
    /// [`TT_TELNET_RAW`], [`TT_TELNET_AUTO`] or [`TT_TELNET_NEGOTIATE`].
    ///
    /// [`tt_telnet_params_default`] sets it from the port, which is upstream's
    /// rule: negotiate on 23, auto-detect elsewhere. **Do not "improve" that
    /// to always negotiating** — a terminal server is not a telnet server, and
    /// opening at one with `WILL TERMINAL-TYPE` puts five bytes of protocol
    /// into somebody's serial console.
    pub mode: TtTelnetMode,
    /// `$TERM` for `TERMINAL-TYPE`. Null means `xterm-256color`.
    pub term_type: *const c_char,
    /// `TERMINAL-SPEED`, which is a claim about the line behind the server
    /// rather than about this connection. Upstream defaults both to 38400.
    pub input_speed: u32,
    pub output_speed: u32,
    /// Ask for `BINARY` in the opening burst. Upstream's `ts.TelBin`, which
    /// defaults **off** — it agrees if asked, but does not ask.
    pub binary: bool,
    pub connect_timeout_ms: u32,
}

/// Fill `out` with the defaults for `port`: upstream's mode rule, 38400 both
/// ways, no `BINARY`, a ten-second connect timeout.
#[no_mangle]
pub extern "C" fn tt_telnet_params_default(out: *mut TtTelnetParams, port: u16) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = TtTelnetParams {
        mode: match TelnetMode::for_port(port) {
            TelnetMode::Raw => TT_TELNET_RAW,
            TelnetMode::Auto => TT_TELNET_AUTO,
            TelnetMode::Negotiate => TT_TELNET_NEGOTIATE,
        },
        term_type: ptr::null(),
        input_speed: 38400,
        output_speed: 38400,
        binary: false,
        connect_timeout_ms: 10_000,
    };
}

/// Open a telnet (or raw TCP) connection and attach it. Replaces any current
/// one.
///
/// Synchronous, unlike [`tt_ssh_connect`], and that is not an inconsistency:
/// telnet asks no questions. There is no host key and no password — the login
/// prompt a server sends is *terminal output*, typed into like any other.
///
/// The terminal is **not** reset, so reconnecting keeps the scrollback that
/// explains why it dropped.
#[no_mangle]
pub extern "C" fn tt_session_connect_telnet(
    session: *mut TtSession,
    host: *const c_char,
    port: u16,
    params: *const TtTelnetParams,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let host = match unsafe { str_arg(host, usize::MAX) } {
        Ok(h) => h,
        Err(e) => return e,
    };
    let Some(p) = (unsafe { params.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtTelnetParams");
    };
    let mut rust = TelnetParams {
        mode: match p.mode {
            TT_TELNET_RAW => TelnetMode::Raw,
            TT_TELNET_NEGOTIATE => TelnetMode::Negotiate,
            _ => TelnetMode::Auto,
        },
        speed: (p.input_speed, p.output_speed),
        cols: s.session.grid().cols() as u16,
        rows: s.session.grid().rows() as u16,
        binary: p.binary,
        ..TelnetParams::default()
    };
    if !p.term_type.is_null() {
        match unsafe { str_arg(p.term_type, usize::MAX) } {
            Ok(t) => rust.term_type = t.to_string(),
            Err(e) => return e,
        }
    }
    let timeout = Duration::from_millis(if p.connect_timeout_ms == 0 {
        10_000
    } else {
        p.connect_timeout_ms.into()
    });
    match TelnetConn::connect(host, port, &rust, timeout) {
        Ok(conn) => {
            s.session.connect(Box::new(conn));
            TT_OK
        }
        Err(e) => report(e),
    }
}

/// What to run on a local pty, and what to tell it about itself.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtPtyParams {
    /// The command and its arguments, `argv[0]` first. **Null or empty means
    /// the user's login shell**, which is what a "Local shell" menu item
    /// wants; anything else is run as itself.
    pub argv: *const *const c_char,
    pub argc: usize,
    /// Working directory. Null means the user's home, which is where a window
    /// opened from a desktop menu should start.
    pub cwd: *const c_char,
    /// `$TERM`. Null means `xterm-256color`.
    ///
    /// Set by us and **never inherited**: a window launched from another
    /// terminal would otherwise hand the shell that terminal's name, and one
    /// launched from a desktop menu would hand it nothing at all.
    pub term: *const c_char,
    /// Start the shell as a login shell, so `~/.profile` runs. Upstream's
    /// default too — `cygterm.cfg`'s `LOGIN_SHELL = Yes`.
    ///
    /// **Only meaningful when `argv` is empty**: the trick is a `-bash` in
    /// `argv[0]`, and `argv[0]` is also what gets looked up on `PATH`.
    pub login_shell: bool,
}

/// Fill `out` with the defaults: the user's login shell, their home
/// directory, `xterm-256color`.
#[no_mangle]
pub extern "C" fn tt_pty_params_default(out: *mut TtPtyParams) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = TtPtyParams {
        argv: ptr::null(),
        argc: 0,
        cwd: ptr::null(),
        term: ptr::null(),
        login_shell: true,
    };
}

/// Fork a shell onto a local pty and attach it. Replaces any current
/// connection.
///
/// The size comes from the session rather than from `params`, because the
/// window already knows it and a child that starts at the wrong width draws
/// one wrong prompt before the first resize corrects it.
///
/// When the child exits, the session reports `TT_EVENT_DISCONNECTED` and
/// [`tt_session_close_note`] says what happened to it.
#[no_mangle]
pub extern "C" fn tt_session_connect_pty(
    session: *mut TtSession,
    params: *const TtPtyParams,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let Some(p) = (unsafe { params.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtPtyParams");
    };

    let mut argv = Vec::with_capacity(p.argc);
    if !p.argv.is_null() {
        for i in 0..p.argc {
            let entry = unsafe { *p.argv.add(i) };
            match unsafe { str_arg(entry, usize::MAX) } {
                Ok(a) => argv.push(a.to_string()),
                Err(e) => return e,
            }
        }
    }

    let mut rust = PtyParams {
        argv,
        login_shell: p.login_shell,
        cols: s.session.grid().cols() as u16,
        rows: s.session.grid().rows() as u16,
        ..PtyParams::default()
    };
    if !p.cwd.is_null() {
        match unsafe { str_arg(p.cwd, usize::MAX) } {
            Ok(d) => rust.cwd = Some(d.into()),
            Err(e) => return e,
        }
    }
    if !p.term.is_null() {
        match unsafe { str_arg(p.term, usize::MAX) } {
            Ok(t) => rust.term = t.to_string(),
            Err(e) => return e,
        }
    }

    match PtyConn::open(&rust) {
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

/// Whether [`tt_session_send_break`] will do anything on the current
/// connection. False when there is none.
///
/// **For drawing the menu, not for handling the failure.** SSH has no break —
/// RFC 4335 defines one and `russh` does not implement it — so a window that
/// offers the item on an SSH session is offering an error message, and it
/// offers it at the moment a console has stopped answering, which is the worst
/// time to find out.
#[no_mangle]
pub extern "C" fn tt_session_supports_break(session: *const TtSession) -> bool {
    session_ref!(session, false).session.supports_break()
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

/// Why the last connection ended, when the transport knew something the word
/// "disconnected" does not say — "bash exited with status 1" from a local
/// shell. Null when nothing more is known, which is the usual case.
///
/// Read it after `TT_EVENT_DISCONNECTED`. It survives until the next
/// [`tt_session_connect_serial`] or friend, so a status line can keep showing
/// it while the window sits there disconnected.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_close_note(session: *mut TtSession) -> *const c_char {
    let s = session!(session, ptr::null());
    match s.session.close_note() {
        Some(n) => {
            s.close_note = cstring(n);
            s.close_note.as_ptr()
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

// --- ssh ------------------------------------------------------------------

/// What to do about a host key the `known_hosts` files do not already trust.
///
/// `TT_HOST_KEY_POLICY_ASK` is what a GUI wants; the other three exist because
/// `~/.ssh/config` contains them and a client that ignores `StrictHostKeyChecking`
/// is a client the user has to configure twice.
pub type TtHostKeyPolicy = u32;

pub const TT_HOST_KEY_POLICY_ASK: TtHostKeyPolicy = 0;
/// `accept-new`: record a first-seen host silently, refuse a changed one.
pub const TT_HOST_KEY_POLICY_ACCEPT_NEW: TtHostKeyPolicy = 1;
/// `yes`: refuse anything not recorded, and never prompt.
pub const TT_HOST_KEY_POLICY_STRICT: TtHostKeyPolicy = 2;
/// `no`: connect to anything.
pub const TT_HOST_KEY_POLICY_ACCEPT_ANY: TtHostKeyPolicy = 3;

/// What the `known_hosts` files made of the key the server presented.
pub type TtHostKeyVerdict = u32;

/// Nothing anywhere mentions this host. A first connection.
pub const TT_HOST_KEY_UNKNOWN: TtHostKeyVerdict = 0;
/// Recorded under this algorithm, and **different**. The alarming one — a
/// frontend should not present it with the same words as the others.
pub const TT_HOST_KEY_CHANGED: TtHostKeyVerdict = 1;
/// The host is recorded, but only under other key algorithms. Usually a
/// server that gained an Ed25519 key.
pub const TT_HOST_KEY_NEW_ALGORITHM: TtHostKeyVerdict = 2;

/// What is being asked for.
pub type TtSshAuthKind = u32;

pub const TT_SSH_AUTH_PASSWORD: TtSshAuthKind = 0;
pub const TT_SSH_AUTH_KEYBOARD_INTERACTIVE: TtSshAuthKind = 1;
/// The passphrase for a local private key. `path` says which file, and it is
/// the only prompt that is ours rather than the server's.
pub const TT_SSH_AUTH_PASSPHRASE: TtSshAuthKind = 2;

/// What [`tt_ssh_connect_poll`] found.
pub type TtSshStep = i32;

/// Nothing yet. Wait for the descriptor to become readable.
pub const TT_SSH_WORKING: TtSshStep = 0;
/// [`tt_ssh_connect_host_key`] has a question. Answer it with
/// [`tt_ssh_connect_answer_host_key`].
pub const TT_SSH_HOST_KEY: TtSshStep = 1;
/// [`tt_ssh_connect_auth`] has a question. Answer it with
/// [`tt_ssh_connect_answer_auth`].
pub const TT_SSH_AUTH: TtSshStep = 2;
/// Connected, and attached to the session that was passed in. The handle has
/// nothing more to say and can be freed.
pub const TT_SSH_READY: TtSshStep = 3;
/// Over. [`tt_last_error`] says why.
pub const TT_SSH_FAILED: TtSshStep = 4;

/// Where to connect and what to try.
///
/// Every string may be null, and null does not mean "empty" — it means "take
/// it from `~/.ssh/config`, or from the default". That distinction is the
/// whole value of `use_ssh_config`: a dialog that leaves the user field blank
/// must get the config's `User`, not an empty user name.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSshParams {
    /// The host, or an alias from `~/.ssh/config`. Required.
    pub host: *const c_char,
    /// 0 means the config's `Port`, or 22.
    pub port: u16,
    /// Null means the config's `User`, or `$USER`.
    pub user: *const c_char,
    /// `$TERM` for the far end. Null means `xterm-256color`, which is what
    /// the engine actually implements.
    pub term: *const c_char,
    /// Read `~/.ssh/config` and fill in everything this struct leaves unset —
    /// the user, the port, the identity files, `StrictHostKeyChecking`, and
    /// whether the config already says this is old equipment.
    pub use_ssh_config: bool,
    /// A null-terminated array of key paths, or null for the OpenSSH defaults
    /// (or whatever the config named).
    pub identities: *const *const c_char,
    /// A null-terminated array of `known_hosts` files, or null for the
    /// config's `UserKnownHostsFile` and then `~/.ssh/known_hosts`.
    ///
    /// Present because Tera Term keeps its own `ssh_known_hosts`, and the
    /// migration path in `PLAN.md` is to read *both*. New keys are recorded
    /// in the first.
    pub known_hosts: *const *const c_char,
    pub use_agent: bool,
    /// Offer the pre-2020 algorithms as well. A config naming SHA-1 key
    /// exchange or a CBC cipher turns this on by itself.
    pub legacy: bool,
    pub connect_timeout_ms: u32,
    /// 0 for none, which is OpenSSH's default.
    pub keepalive_ms: u32,
    pub host_key_policy: TtHostKeyPolicy,
}

/// Fill `out` with the sensible defaults: read `~/.ssh/config`, use the agent,
/// ask about unknown host keys, modern algorithms only, 30-second timeout.
#[no_mangle]
pub extern "C" fn tt_ssh_params_default(out: *mut TtSshParams) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = TtSshParams {
        host: ptr::null(),
        port: 0,
        user: ptr::null(),
        term: ptr::null(),
        use_ssh_config: true,
        identities: ptr::null(),
        known_hosts: ptr::null(),
        use_agent: true,
        legacy: false,
        connect_timeout_ms: 30_000,
        keepalive_ms: 0,
        host_key_policy: TT_HOST_KEY_POLICY_ASK,
    };
}

/// The far end's host key, and what the files said about it.
///
/// Every pointer is borrowed from the [`TtSshConnect`] and is valid until the
/// next [`tt_ssh_connect_poll`] on it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSshHostKeyPrompt {
    pub host: *const c_char,
    pub port: u16,
    /// The key's own type name as `known_hosts` records it — `ssh-ed25519`,
    /// `ssh-rsa` — not the negotiated signature algorithm.
    pub algorithm: *const c_char,
    /// `SHA256:…`, the form every other client prints.
    pub fingerprint: *const c_char,
    pub verdict: TtHostKeyVerdict,
    /// For [`TT_HOST_KEY_CHANGED`]: `path:line` of the entry that disagrees,
    /// so the message can tell the user what to delete. Null otherwise.
    pub recorded_at: *const c_char,
    /// For [`TT_HOST_KEY_CHANGED`]: the fingerprint on file, for showing
    /// beside the new one. Null otherwise.
    pub recorded_fingerprint: *const c_char,
    /// For [`TT_HOST_KEY_NEW_ALGORITHM`]: the algorithms already recorded,
    /// comma-separated. Null otherwise.
    pub also_known: *const c_char,
}

/// One line of something to type.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSshPrompt {
    pub text: *const c_char,
    /// Whether to show what is typed. The server chooses.
    pub echo: bool,
}

/// A question that has to reach the user before authentication can go on.
///
/// Borrowed like [`TtSshHostKeyPrompt`], and valid for exactly as long.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSshAuthPrompt {
    pub kind: TtSshAuthKind,
    /// The server's own wording, where it sent any. Never null; often empty.
    pub name: *const c_char,
    pub instruction: *const c_char,
    /// For [`TT_SSH_AUTH_PASSPHRASE`]: the key file. Null otherwise.
    pub path: *const c_char,
    pub prompts: *const TtSshPrompt,
    pub prompt_count: usize,
}

/// A connection being set up. Free it with [`tt_ssh_connect_free`], whether or
/// not it reached [`TT_SSH_READY`].
pub struct TtSshConnect {
    inner: SshConnect,
    /// The prompt the last poll produced, kept alive until the next one.
    host_key: Option<TtSshHostKeyPrompt>,
    auth: Option<TtSshAuthPrompt>,
    prompts: Vec<TtSshPrompt>,
    /// Backing store for every `*const c_char` above.
    _strings: Vec<CString>,
    /// How many answers the outstanding auth prompt wants, so a caller that
    /// sends the wrong number is corrected here rather than desynchronising
    /// the exchange with the server.
    wanted: usize,
}

/// Start connecting. Returns immediately, before anything has happened; null
/// only if the thread or the pipe could not be created.
///
/// The session is **not** passed here. It is passed to
/// [`tt_ssh_connect_poll`], which attaches the transport the moment the shell
/// is running — so nothing stores a `TtSession *` it does not own.
#[no_mangle]
pub extern "C" fn tt_ssh_connect(params: *const TtSshParams) -> *mut TtSshConnect {
    let Some(p) = (unsafe { params.as_ref() }) else {
        fail(TT_ERR_INVALID, "null TtSshParams");
        return ptr::null_mut();
    };
    let host = match unsafe { str_arg(p.host, usize::MAX) } {
        Ok(h) => h,
        Err(_) => {
            fail(TT_ERR_INVALID, "TtSshParams.host is required");
            return ptr::null_mut();
        }
    };
    let opt = |ptr: *const c_char| -> Option<&str> {
        if ptr.is_null() {
            None
        } else {
            unsafe { str_arg(ptr, usize::MAX) }.ok()
        }
    };

    let mut rust = if p.use_ssh_config {
        let config = match SshConfig::user_default() {
            Ok(c) => c,
            Err(e) => {
                // A config that exists and cannot be read would otherwise
                // connect as the wrong user to the wrong port and look like a
                // server problem.
                fail(TT_ERR_IO, format!("~/.ssh/config: {e}"));
                return ptr::null_mut();
            }
        };
        SshParams::from_config(
            &config,
            host,
            opt(p.user),
            if p.port == 0 { None } else { Some(p.port) },
        )
    } else {
        SshParams::new(
            host,
            if p.port == 0 { 22 } else { p.port },
            opt(p.user)
                .map(str::to_string)
                .or_else(|| std::env::var("USER").ok())
                .unwrap_or_default(),
        )
    };

    if let Some(term) = opt(p.term) {
        rust.term = term.to_string();
    }
    match unsafe { path_array(p.identities) } {
        Ok(Some(files)) => rust.identities = files,
        Ok(None) => {}
        Err(_) => {
            fail(TT_ERR_INVALID, "TtSshParams.identities is not UTF-8");
            return ptr::null_mut();
        }
    }
    match unsafe { path_array(p.known_hosts) } {
        Ok(Some(files)) => rust.known_hosts = KnownHosts::with_files(files),
        Ok(None) => {}
        Err(_) => {
            fail(TT_ERR_INVALID, "TtSshParams.known_hosts is not UTF-8");
            return ptr::null_mut();
        }
    }
    // The struct's own switches only ever *narrow* the agent and *widen* the
    // algorithms, so a config that already said "old equipment" is not undone
    // by a dialog whose legacy box is unticked.
    rust.use_agent &= p.use_agent;
    rust.legacy |= p.legacy;
    if p.connect_timeout_ms > 0 {
        rust.connect_timeout = Duration::from_millis(p.connect_timeout_ms.into());
    }
    if p.keepalive_ms > 0 {
        rust.keepalive = Some(Duration::from_millis(p.keepalive_ms.into()));
    }
    if p.host_key_policy != TT_HOST_KEY_POLICY_ASK {
        rust.host_key_policy = match p.host_key_policy {
            TT_HOST_KEY_POLICY_ACCEPT_NEW => HostKeyPolicy::AcceptNew,
            TT_HOST_KEY_POLICY_STRICT => HostKeyPolicy::Strict,
            TT_HOST_KEY_POLICY_ACCEPT_ANY => HostKeyPolicy::AcceptAny,
            _ => HostKeyPolicy::Ask,
        };
    }

    match SshConnect::start(rust) {
        Ok(inner) => Box::into_raw(Box::new(TtSshConnect {
            inner,
            host_key: None,
            auth: None,
            prompts: Vec::new(),
            _strings: Vec::new(),
            wanted: 0,
        })),
        Err(e) => {
            report(e);
            ptr::null_mut()
        }
    }
}

/// A null-terminated array of paths, or `None` when the pointer is null.
///
/// Null-terminated rather than pointer-plus-count, because a count is a second
/// thing a caller can get wrong in a way nothing here can detect.
unsafe fn path_array(
    array: *const *const c_char,
) -> std::result::Result<Option<Vec<std::path::PathBuf>>, ()> {
    if array.is_null() {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut i = 0isize;
    loop {
        let entry = *array.offset(i);
        if entry.is_null() {
            return Ok(Some(out));
        }
        match str_arg(entry, usize::MAX) {
            Ok(s) => out.push(std::path::PathBuf::from(s)),
            Err(_) => return Err(()),
        }
        i += 1;
    }
}

/// The descriptor to wait on.
///
/// **The same one [`tt_session_poll_fd`] returns once the session is
/// connected**, so a frontend registers its notifier once and keeps it across
/// the handover rather than swapping it at the moment output starts.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_poll_fd(c: *const TtSshConnect) -> c_int {
    match unsafe { c.as_ref() } {
        Some(c) => c.inner.poll_fd(),
        None => -1,
    }
}

/// What the connection needs next. Never blocks.
///
/// On [`TT_SSH_READY`] the transport is attached to `session`, replacing
/// whatever was there. `session` may be null only if the caller intends to
/// throw the connection away, in which case `READY` still ends it.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_poll(c: *mut TtSshConnect, session: *mut TtSession) -> TtSshStep {
    let Some(c) = (unsafe { c.as_mut() }) else {
        fail(TT_ERR_INVALID, "null TtSshConnect");
        return TT_SSH_FAILED;
    };
    // Anything the previous poll handed out dies here, which is what the
    // "valid until the next poll" contract in the header means.
    c.host_key = None;
    c.auth = None;
    c.prompts.clear();
    c._strings.clear();
    c.wanted = 0;

    match c.inner.poll() {
        Step::Working => TT_SSH_WORKING,
        Step::HostKey(p) => {
            let mut strings = Vec::new();
            let mut push = |s: &str| {
                strings.push(cstring(s));
                strings.len() - 1
            };
            let host = push(&p.host);
            let algorithm = push(&p.algorithm);
            let fingerprint = push(&p.fingerprint);
            let (verdict, at, recorded_fp, also) = match &p.verdict {
                Verdict::Changed { site, recorded } => (
                    TT_HOST_KEY_CHANGED,
                    Some(push(&site.to_string())),
                    Some(push(&recorded.fingerprint())),
                    None,
                ),
                Verdict::NewAlgorithm { also_known } => (
                    TT_HOST_KEY_NEW_ALGORITHM,
                    None,
                    None,
                    Some(push(&also_known.join(", "))),
                ),
                // `Trusted` and `Revoked` never reach a prompt: one connects
                // silently and the other refuses without asking.
                _ => (TT_HOST_KEY_UNKNOWN, None, None, None),
            };
            c._strings = strings;
            let at_ptr = |i: Option<usize>| i.map_or(ptr::null(), |i| c._strings[i].as_ptr());
            c.host_key = Some(TtSshHostKeyPrompt {
                host: c._strings[host].as_ptr(),
                port: p.port,
                algorithm: c._strings[algorithm].as_ptr(),
                fingerprint: c._strings[fingerprint].as_ptr(),
                verdict,
                recorded_at: at_ptr(at),
                recorded_fingerprint: at_ptr(recorded_fp),
                also_known: at_ptr(also),
            });
            TT_SSH_HOST_KEY
        }
        Step::Auth(p) => {
            let mut strings = Vec::new();
            strings.push(cstring(&p.name));
            strings.push(cstring(&p.instruction));
            let path = match &p.kind {
                AuthPromptKind::Passphrase(path) => {
                    strings.push(cstring(&path.display().to_string()));
                    Some(strings.len() - 1)
                }
                _ => None,
            };
            let first_prompt = strings.len();
            for prompt in &p.prompts {
                strings.push(cstring(&prompt.text));
            }
            c.wanted = p.prompts.len();
            c._strings = strings;
            c.prompts = p
                .prompts
                .iter()
                .enumerate()
                .map(|(i, prompt)| TtSshPrompt {
                    text: c._strings[first_prompt + i].as_ptr(),
                    echo: prompt.echo,
                })
                .collect();
            c.auth = Some(TtSshAuthPrompt {
                kind: match p.kind {
                    AuthPromptKind::Password => TT_SSH_AUTH_PASSWORD,
                    AuthPromptKind::KeyboardInteractive => TT_SSH_AUTH_KEYBOARD_INTERACTIVE,
                    AuthPromptKind::Passphrase(_) => TT_SSH_AUTH_PASSPHRASE,
                },
                name: c._strings[0].as_ptr(),
                instruction: c._strings[1].as_ptr(),
                path: path.map_or(ptr::null(), |i| c._strings[i].as_ptr()),
                prompts: c.prompts.as_ptr(),
                prompt_count: c.prompts.len(),
            });
            TT_SSH_AUTH
        }
        Step::Ready(conn) => {
            if let Some(s) = unsafe { session.as_mut() } {
                s.session.connect(Box::new(conn));
            }
            TT_SSH_READY
        }
        Step::Failed(e) => {
            report(e);
            TT_SSH_FAILED
        }
    }
}

/// The outstanding host-key question, or null when the last poll did not
/// return [`TT_SSH_HOST_KEY`]. Valid until the next poll.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_host_key(c: *const TtSshConnect) -> *const TtSshHostKeyPrompt {
    match unsafe { c.as_ref() } {
        Some(c) => c.host_key.as_ref().map_or(ptr::null(), |p| p as *const _),
        None => ptr::null(),
    }
}

/// The outstanding authentication question, or null when the last poll did
/// not return [`TT_SSH_AUTH`]. Valid until the next poll.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_auth(c: *const TtSshConnect) -> *const TtSshAuthPrompt {
    match unsafe { c.as_ref() } {
        Some(c) => c.auth.as_ref().map_or(ptr::null(), |p| p as *const _),
        None => ptr::null(),
    }
}

/// Answer a [`TT_SSH_HOST_KEY`] with 1 to accept and record, 2 to accept just
/// this once, and anything else to refuse.
///
/// Deliberately three answers rather than a bool. "Yes, but do not write it
/// down" is what a user on a network they do not trust means, and collapsing
/// it into "yes" silently records a key they did not want recorded.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_answer_host_key(c: *mut TtSshConnect, decision: c_int) {
    let Some(c) = (unsafe { c.as_mut() }) else {
        return;
    };
    c.inner.answer_host_key(match decision {
        1 => HostKeyDecision::AcceptAndSave,
        2 => HostKeyDecision::AcceptOnce,
        _ => HostKeyDecision::Refuse,
    });
}

/// Answer a [`TT_SSH_AUTH`], one string per prompt in the order asked.
///
/// A short or long list is padded or truncated to what the server asked for
/// rather than passed on: the protocol requires one response per prompt, and
/// getting it wrong desynchronises the exchange in a way that reads as a
/// server bug.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_answer_auth(
    c: *mut TtSshConnect,
    answers: *const *const c_char,
    count: usize,
) {
    let Some(c) = (unsafe { c.as_mut() }) else {
        return;
    };
    let mut out = Vec::with_capacity(c.wanted);
    if !answers.is_null() {
        for i in 0..count {
            let entry = unsafe { *answers.add(i) };
            let s = if entry.is_null() {
                String::new()
            } else {
                unsafe { str_arg(entry, usize::MAX) }
                    .map(str::to_string)
                    .unwrap_or_default()
            };
            out.push(s);
        }
    }
    out.resize(c.wanted, String::new());
    c.inner.answer_auth(out);
}

/// Stop, and free. Safe on null, and safe after [`TT_SSH_READY`] — the
/// session owns the connection by then and freeing the handle does not touch
/// it.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_free(c: *mut TtSshConnect) {
    if !c.is_null() {
        drop(unsafe { Box::from_raw(c) });
    }
}

/// An owned list of strings. Free it with [`tt_string_list_free`].
pub struct TtStringList {
    items: Vec<CString>,
}

/// Every `Host` alias in `~/.ssh/config` that names one machine — no
/// wildcards, no negations — for filling a picker. Null on failure.
///
/// A `Host *` block is not somewhere to connect, and a `!bastion` names a host
/// its block is *about* rather than one it configures. Offering either would
/// put entries in a dropdown that cannot be connected to.
#[no_mangle]
pub extern "C" fn tt_ssh_config_aliases() -> *mut TtStringList {
    match SshConfig::user_default() {
        Ok(c) => Box::into_raw(Box::new(TtStringList {
            items: c.aliases().iter().map(|a| cstring(a)).collect(),
        })),
        Err(e) => {
            fail(TT_ERR_IO, format!("~/.ssh/config: {e}"));
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn tt_string_list_len(list: *const TtStringList) -> usize {
    match unsafe { list.as_ref() } {
        Some(l) => l.items.len(),
        None => 0,
    }
}

/// Borrow one entry. Null when `index` is out of range. Valid until the list
/// is freed.
#[no_mangle]
pub extern "C" fn tt_string_list_at(list: *const TtStringList, index: usize) -> *const c_char {
    match unsafe { list.as_ref() } {
        Some(l) => l.items.get(index).map_or(ptr::null(), |s| s.as_ptr()),
        None => ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn tt_string_list_free(list: *mut TtStringList) {
    if !list.is_null() {
        drop(unsafe { Box::from_raw(list) });
    }
}
