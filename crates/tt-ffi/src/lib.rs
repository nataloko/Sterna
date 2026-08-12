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
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::sync::OnceLock;
use std::time::Duration;

use tt_config::cmdline::proxy::ProxyOptions;
use tt_config::cmdline::ssh::SshOptions;
use tt_config::cmdline::{self, CommandLine, MacroArg};
use tt_conn::pty::{PtyConn, PtyParams};
use tt_conn::serial::{
    DataBits, FlowControl, Parity, PinControl, SerialConn, SerialParams, StopBits,
};
use tt_conn::ssh::{
    AuthPromptKind, HostKeyDecision, HostKeyPolicy, KnownHosts, SshConfig, SshConnect, SshParams,
    Step, Verdict,
};
use tt_conn::telnet::{TelnetConn, TelnetMode, TelnetParams};
use tt_conn::Error;
use tt_ctl::{CtlHost, MacroStatus, NullHost, RunError, Server as CtlServer};
use tt_grid::Cell;
use tt_i18n::Catalog;
use tt_macro::{MacroError, MacroReceiver, MacroUi, NullUi, SessionHost};
use tt_session::open::{proxy_params, Startup, Target};
use tt_session::{
    Event, Ini, KeyCodeResult, LogMode, LogOptions, Session, Settings, Shortcut, Timestamp,
};
use tt_ttl::host::{
    BeepSound, DialogAnchor, DialogEnd, DialogOrigin, DialogPos, ListBoxOpts, MacroWindow,
    ShowWindow, WindowGeometry, WindowState,
};
use tt_ttl::{CmdLine, Interp, TtlError};
use tt_vt::{ClipboardRequest, Config, Key, Modifiers, MouseEvent, TermId, Tracking};

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
/// The proxy in front of the host refused, failed, or is not a proxy.
/// **Separate from [`TT_ERR_SSH`] and [`TT_ERR_IO`] because it sends the user
/// to a different page of the settings**: nothing about the host or its
/// credentials is wrong when a SOCKS server answers `REP 2`.
pub const TT_ERR_PROXY: TtStatus = -10;

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
        Error::Proxy(_) => TT_ERR_PROXY,
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

// --- language catalogs ---------------------------------------------------

/// One loaded Tera Term `.lng` file. Opaque.
pub struct TtI18n {
    catalog: Catalog,
    /// The last lookup, kept alive until the next one. This is bytes rather
    /// than a `CString`: upstream's file-dialog filters contain embedded NULs.
    text: Vec<u8>,
}

/// Load a Tera Term `.lng` catalog. Null on an unreadable path, with the
/// reason in [`tt_last_error`].
#[no_mangle]
pub extern "C" fn tt_i18n_load(path: *const c_char) -> *mut TtI18n {
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(path) => path,
        Err(_) => return ptr::null_mut(),
    };
    match Catalog::load(Path::new(path)) {
        Ok(catalog) => Box::into_raw(Box::new(TtI18n {
            catalog,
            text: Vec::new(),
        })),
        Err(e) => {
            set_error(e.to_string());
            ptr::null_mut()
        }
    }
}

/// Free a language catalog. Does nothing for null.
#[no_mangle]
pub extern "C" fn tt_i18n_free(catalog: *mut TtI18n) {
    if !catalog.is_null() {
        drop(unsafe { Box::from_raw(catalog) });
    }
}

/// Look up one translated UTF-8 string.
///
/// `fallback` is returned when the key is absent; it may be null to ask for a
/// null result instead. `out_len` receives the byte length. The result is
/// **not NUL-terminated and may contain embedded NULs**, so use the length. It
/// is borrowed from `catalog` and valid until the next lookup or free.
#[no_mangle]
pub extern "C" fn tt_i18n_text(
    catalog: *mut TtI18n,
    section: *const c_char,
    key: *const c_char,
    fallback: *const c_char,
    out_len: *mut usize,
) -> *const u8 {
    if let Some(len) = unsafe { out_len.as_mut() } {
        *len = 0;
    }
    let Some(catalog) = (unsafe { catalog.as_mut() }) else {
        set_error("null TtI18n");
        return ptr::null();
    };
    let (section, key) = match (unsafe { str_arg(section, usize::MAX) }, unsafe {
        str_arg(key, usize::MAX)
    }) {
        (Ok(section), Ok(key)) => (section, key),
        _ => return ptr::null(),
    };
    let value = match catalog.catalog.get(section, key) {
        Some(value) => value,
        None if fallback.is_null() => return ptr::null(),
        None => match unsafe { str_arg(fallback, usize::MAX) } {
            Ok(value) => value.to_owned(),
            Err(_) => return ptr::null(),
        },
    };
    catalog.text = value.into_bytes();
    if let Some(len) = unsafe { out_len.as_mut() } {
        *len = catalog.text.len();
    }
    catalog.text.as_ptr()
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
    /// The last answer from `tt_session_log_name`, which is a *different*
    /// string from the one above: one is where a log is going and the other is
    /// where one would go.
    log_name: CString,
    close_note: CString,
    setting: CString,
    /// The string returned by the last physical-key dispatch, when it asked
    /// the frontend to start a macro.
    key_action: CString,
    /// Duplicate scan codes found by the last successful keyboard-map load.
    key_duplicates: Vec<u16>,
    /// The decoded `DelimList`, which is not the file's own spelling and so
    /// cannot share the buffer above.
    delimiters: CString,
    /// The last AttrURL run returned to the frontend.
    url: CString,
    /// The strings the last `TtTransferStatus` handed out, and the outcome of
    /// the last transfer to finish. Kept on the session because the C caller
    /// has nowhere to put them.
    xfer_protocol: CString,
    xfer_file: CString,
    xfer_message: CString,
    transfer_result: Option<tt_session::TransferOutcome>,
    /// The window operations the last event drain turned up. Kept here for the
    /// same reason the strings above are: a C caller has nowhere to put them,
    /// and `TtEvent` has no room to carry two `int`s.
    window_requests: Vec<TtWindowRequest>,
    /// And the printer's, for the same reason again — with the strings the
    /// `Write` events point at kept alongside them, because `event_texts` is
    /// cleared on the same schedule but is indexed by nothing.
    printer_events: Vec<TtPrinterEvent>,
    printer_texts: Vec<CString>,
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
        title: cstring(&session.vt().window_title()),
        session,
        events: Vec::new(),
        event_texts: Vec::new(),
        describe: CString::default(),
        log_path: CString::default(),
        log_name: CString::default(),
        close_note: CString::default(),
        setting: CString::default(),
        key_action: CString::default(),
        key_duplicates: Vec::new(),
        delimiters: CString::default(),
        url: CString::default(),
        xfer_protocol: CString::default(),
        xfer_file: CString::default(),
        xfer_message: CString::default(),
        transfer_result: None,
        window_requests: Vec::new(),
        printer_events: Vec::new(),
        printer_texts: Vec::new(),
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

/// DECSTBM's rows, zero-based and inclusive — the whole screen unless a host
/// has narrowed it.
///
/// One caller: `CSI 0 i` with DECPEX reset prints the scroll region rather than
/// the screen (`vtterm.c:2085`), and the frontend that owns the printer has to
/// know which rows those are. Nothing else above the core needs the margins,
/// which is why this is the only half of them exposed.
#[no_mangle]
pub extern "C" fn tt_session_scroll_region(
    session: *const TtSession,
    top: *mut usize,
    bottom: *mut usize,
) {
    let s = session_ref!(session);
    let (t, b) = s.session.grid().scroll_region();
    unsafe {
        if let Some(top) = top.as_mut() {
            *top = t;
        }
        if let Some(bottom) = bottom.as_mut() {
            *bottom = b;
        }
    }
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
/// **`options` may be null, and that is the ordinary call**: it means "however
/// the settings say", which is what a menu item and `LogAutoStart` both want
/// and what keeps a frontend from having to know that `LogBinary` and
/// `LogTimestampType` exist. Pass a struct only to override one.
///
/// `LogTimestampFormat` is a string and does not fit in a `#[repr(C)]` struct
/// that a caller allocates, so it always comes from the settings — an override
/// changes which clock is printed, never how.
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
    let from_settings = tt_session::log_options(s.session.settings());
    let opts = match unsafe { options.as_ref() } {
        None => from_settings,
        Some(o) => LogOptions {
            mode: if o.raw { LogMode::Raw } else { LogMode::Text },
            timestamp: o.timestamp,
            append: o.append,
            rotate_size: o.rotate_size,
            rotate_keep: o.rotate_keep,
            crlf: o.crlf,
            format: from_settings.format,
        },
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

/// Say what this session is connected to, for the `&h` and `&p` a log name may
/// hold. `host` may be null and a `tcp_port` of 0 means "none".
///
/// A serial line passes its port's name as `host` and no port number, which is
/// upstream putting `COM<n>` through the same escape.
#[no_mangle]
pub extern "C" fn tt_session_set_connection_name(
    session: *mut TtSession,
    host: *const c_char,
    tcp_port: u16,
) {
    let s = session!(session);
    let host = match unsafe { str_arg(host, usize::MAX) } {
        Ok(h) if !h.is_empty() => Some(h.to_string()),
        _ => None,
    };
    s.session
        .set_connection_name(host, (tcp_port != 0).then_some(tcp_port));
}

/// The file a log would be opened under, expanded and absolute.
///
/// `requested` is `/L=`'s argument or a name a user typed, and **null asks for
/// `LogDefaultName`** — which is not the same as passing it, because only an
/// absolute request escapes the log directory.
///
/// The answer is a template's expansion, so it is not stable: a name holding
/// `%H` changes on the hour. Ask for it at the moment the log is opened, or at
/// the moment a dialog is filled in, and not before.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_log_name(
    session: *mut TtSession,
    requested: *const c_char,
) -> *const c_char {
    let s = session!(session, ptr::null());
    let requested = match unsafe { str_arg(requested, usize::MAX) } {
        Ok(r) if !r.is_empty() => Some(r.to_string()),
        _ => None,
    };
    let name = s.session.log_file_name(requested.as_deref());
    s.log_name = CString::new(name.to_string_lossy().as_bytes()).unwrap_or_default();
    s.log_name.as_ptr()
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

/// The absolute number of the line shown at viewport row `y`.
///
/// A viewport row says *where* a line is, and that changes every time the host
/// prints. This says *which* line it is, and that never changes — which is what
/// a frontend needs to hold on to one across output. A selection is the thing
/// that needs it: highlighting rows 3 to 5 means a highlight that walks up the
/// screen as the device talks, and a copy that takes whatever slid underneath.
///
/// The numbering starts at zero on the first line the terminal ever showed and
/// counts every line that has scrolled off since, so the top of the live page
/// is always [`tt_session_top_line`]. `y` is not range-checked: a row past the
/// bottom of the page names a line that has not been printed yet, which
/// [`tt_session_line`] then reports as absent.
#[no_mangle]
pub extern "C" fn tt_session_line_at(session: *const TtSession, y: usize) -> u64 {
    session_ref!(session, 0).session.line_at(y)
}

/// The absolute number of the line at the top of the **live** page.
///
/// Equivalently: how many lines have ever scrolled off it. The difference
/// between two readings is how far the content moved.
#[no_mangle]
pub extern "C" fn tt_session_top_line(session: *const TtSession) -> u64 {
    session_ref!(session, 0).session.top_line()
}

/// One line by absolute number — the scrollback and the page alike, and
/// **without regard to what is currently in view**.
///
/// [`tt_session_row`] is the painter's call and this is the one for anything
/// that outlived a scroll. Null when the line has been evicted from the
/// scrollback or has not been printed yet, which a caller holding an old number
/// has to be able to ask about rather than range-check first. Sets no error:
/// asking about a line that has aged out is ordinary.
///
/// Borrowed on the same terms as [`tt_session_row`] — valid until the next call
/// that can change the grid.
#[no_mangle]
pub extern "C" fn tt_session_line(
    session: *const TtSession,
    line: u64,
    out_len: *mut usize,
) -> *const Cell {
    let s = session_ref!(session, ptr::null());
    let Some(cells) = s.session.line(line) else {
        return ptr::null();
    };
    if let Some(len) = unsafe { out_len.as_mut() } {
        *len = cells.len();
    }
    cells.as_ptr()
}

/// The URL-marked run containing cell `(line, x)`.
///
/// `line` is an absolute number on the same scale as [`tt_session_line`], so a
/// click still names the same text if output arrives between hit-testing and
/// invocation. Null when the cell is absent or not marked as a URL; that is an
/// ordinary answer and sets no error.
///
/// The returned UTF-8 string is owned by `session` and remains valid until the
/// next call to this function on the same session, or until the session is
/// freed.
#[no_mangle]
pub extern "C" fn tt_session_url_at(session: *mut TtSession, line: u64, x: usize) -> *const c_char {
    let s = session!(session, ptr::null());
    let Some(url) = s.session.url_at(line, x) else {
        return ptr::null();
    };
    s.url = cstring(&url);
    s.url.as_ptr()
}

/// The live text-cursor shape, in DECSCUSR's numbering.
///
/// It is live terminal state rather than only the value loaded from the file:
/// a host may change it with DECSCUSR when `CursorCtrlSequence` permits that.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtCursorShape {
    Block = 1,
    Horizontal = 3,
    Vertical = 5,
}

/// Where the cursor is and how to draw it.
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
    /// The shape as it stands now, after both the setting and any accepted
    /// DECSCUSR sequence.
    pub shape: TtCursorShape,
    /// `NonblinkingCursor`, likewise live after DECSET 12 or DECSCUSR.
    pub nonblinking: bool,
}

#[no_mangle]
pub extern "C" fn tt_session_cursor(session: *const TtSession, out: *mut TtCursor) {
    let s = session_ref!(session);
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    let c = s.session.grid().cursor;
    let config = s.session.vt().config();
    *out = TtCursor {
        x: c.x,
        y: c.y,
        visible: s.session.vt().cursor_visible(),
        pending_wrap: c.pending_wrap,
        shape: match config.cursor_shape {
            3 => TtCursorShape::Horizontal,
            5 => TtCursorShape::Vertical,
            _ => TtCursorShape::Block,
        },
        nonblinking: config.nonblinking_cursor,
    };
}

/// The window title — `terminal.title` and whatever the host set with OSC 0,
/// 1 or 2, combined the way `window.title_change` says. Never null; empty only
/// when neither has been set.
///
/// Borrowed, and **valid until the next call to this function** on this
/// session. The [`TtEventKind::Title`] event carries the same string and is
/// the cheaper way to notice a change.
#[no_mangle]
pub extern "C" fn tt_session_title(session: *mut TtSession) -> *const c_char {
    let s = session!(session, c"".as_ptr());
    s.title = cstring(&s.session.vt().window_title());
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

/// Whether a wheel notch should go out as a cursor key rather than scroll the
/// window's own view — `vtterm.c:WheelToCursorMode`.
///
/// Four terms, and a frontend that assembles them itself will get one wrong:
/// `DECSET 7786`, the application cursor mode, `DisableAppCursor` — which
/// vetoes that mode without unsetting it, so DECRQM still reports it set — and
/// Ctrl under `DisableWheelToCursorByCtrl`, which is the escape hatch that
/// reaches the terminal's history while a full-screen program is up. Hence the
/// modifiers, the same way [`tt_session_mouse`] takes them.
///
/// Ask *after* [`tt_session_mouse`] has declined the wheel: mouse tracking
/// comes first (`vtwin.cpp:2543`), and `less` scrolling its own buffer is not
/// the same thing as the window scrolling ours.
#[no_mangle]
pub extern "C" fn tt_session_wheel_to_cursor(session: *const TtSession, mods: Modifiers) -> bool {
    session_ref!(session, false)
        .session
        .vt()
        .wheel_to_cursor_now(mods)
}

/// Whether the frontend should be tracking the mouse at all, and therefore
/// whether a drag belongs to the host or to text selection.
#[no_mangle]
pub extern "C" fn tt_session_mouse_tracking(session: *const TtSession) -> Tracking {
    session_ref!(session, Tracking::None)
        .session
        .mouse_tracking()
}

fn palette_rgb(
    palette: &[tt_vt::palette::Rgb; 256],
    index: u32,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
) -> bool {
    let Ok(i) = usize::try_from(index) else {
        return false;
    };
    let Some(&color) = palette.get(i) else {
        return false;
    };
    palette_rgb_one(color, r, g, b);
    true
}

/// A live session's palette entry, for the painter. False for a null session
/// or `index > 255`.
///
/// Tera Term stores one byte of colour per cell, so this is the *whole* colour
/// story — `SGR 38;2;r;g;b` has already resolved to the nearest index by the
/// time a cell holds it. Entries 0-15 reflect the session's `ANSIColor`
/// setting; entries 16-255 are the fixed xterm cube and greyscale ramp.
///
/// **This is the live table, which a host can repaint with `OSC 4`.** Read it
/// again after each pump rather than caching it at startup; the same sequence
/// also moves which index a truecolor SGR resolves to, so the two would
/// otherwise disagree about the same cell.
///
/// Note what the cell says: `fg`/`bg` mean a palette index only when
/// `TT_ATTR2_FORE` / `TT_ATTR2_BACK` is set in `attrs`. Without the bit the
/// cell is asking for the terminal's *configured* default text colour, which
/// is the frontend's to choose — painting index 0 there gives a black-on-black
/// screen.
#[no_mangle]
pub extern "C" fn tt_session_palette_rgb(
    session: *const TtSession,
    index: u32,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
) -> bool {
    let session = session_ref!(session, false);
    palette_rgb(&session.session.vt().colors().ansi, index, r, g, b)
}

/// Which of the terminal's six attribute colour pairs
/// [`tt_session_color_rgb`] is being asked about.
///
/// These are the pairs `vtdisp.c:GetDrawAttr` chooses between, in upstream's
/// own priority order; the palette is a separate question and belongs to
/// [`tt_session_palette_rgb`]. Tek's pair is not here because no window in this
/// shell paints with it.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtColorPair {
    /// `VTColor` — ordinary text.
    Normal,
    /// `VTBoldColor`, which is also what a selection is drawn with.
    Bold,
    Blink,
    Reverse,
    Url,
    Underline,
}

/// A live session's attribute colour, for the painter. False for a null
/// session.
///
/// The counterpart of [`tt_session_palette_rgb`] for the colours that are not
/// palette entries, and live for the same reason: `OSC 10`-`19` move them while
/// the session runs, so reading the settings once at startup paints a window
/// that ignores what the host asked for.
///
/// **The settings and these are not interchangeable even when nothing has
/// changed them.** `tt_session_setting` gives what the file says, which is what
/// a reset returns to; this gives what the terminal is painting with now.
#[no_mangle]
pub extern "C" fn tt_session_color_rgb(
    session: *const TtSession,
    pair: TtColorPair,
    background: bool,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
) -> bool {
    let session = session_ref!(session, false);
    let colors = session.session.vt().colors();
    let pair = match pair {
        TtColorPair::Normal => &colors.normal,
        TtColorPair::Bold => &colors.bold,
        TtColorPair::Blink => &colors.blink,
        TtColorPair::Reverse => &colors.reverse,
        TtColorPair::Url => &colors.url,
        TtColorPair::Underline => &colors.underline,
    };
    palette_rgb_one(pair[usize::from(background)], r, g, b);
    true
}

/// What a window looks like, for the XTWINOPS reports that describe one —
/// `CSI 11`/`13`/`14`/`15`/`16`/`19 t`.
///
/// Push this on every move, resize and window-state change. The terminal has
/// to answer those reports while it is parsing, so it holds the last snapshot
/// rather than asking; a frontend that never pushes leaves a notional 8x16-cell
/// window at the origin on a 1920x1080 work area, which is what a headless
/// build reports.
///
/// Pixel sizes of zero mean "no frontend has said", and the terminal derives
/// them from the grid and the cell. That is one value rather than a flag
/// because a window of zero pixels is not a state a frontend can be in.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtWindowMetrics {
    /// The outer frame's origin in screen pixels.
    pub x: i32,
    pub y: i32,
    /// The text area's origin in screen pixels — the frame's, plus the border
    /// and the caption.
    pub client_x: i32,
    pub client_y: i32,
    /// The outer frame in pixels.
    pub width: i32,
    pub height: i32,
    /// The text area in pixels.
    pub client_width: i32,
    pub client_height: i32,
    /// One cell in pixels, which is the font advance plus `VTFontSpace`.
    pub cell_width: i32,
    pub cell_height: i32,
    /// The **work area** of the screen the window is on — what is left after
    /// the panels and docks, not the whole monitor. Qt spells it
    /// `QScreen::availableGeometry()`.
    pub screen_width: i32,
    pub screen_height: i32,
    pub iconified: bool,
}

/// Tell the terminal what its window is. See [`TtWindowMetrics`].
#[no_mangle]
pub extern "C" fn tt_session_set_window_metrics(
    session: *mut TtSession,
    metrics: *const TtWindowMetrics,
) {
    let s = session!(session);
    let Some(m) = (unsafe { metrics.as_ref() }) else {
        return;
    };
    let some = |w: i32, h: i32| (w > 0 && h > 0).then_some((w, h));
    s.session.set_window_metrics(tt_vt::WindowMetrics {
        pos: (m.x, m.y),
        client_pos: (m.client_x, m.client_y),
        size: some(m.width, m.height),
        client_size: some(m.client_width, m.client_height),
        cell: (m.cell_width, m.cell_height),
        screen: (m.screen_width, m.screen_height),
        iconified: m.iconified,
    });
}

/// Which XTWINOPS operation a [`TtWindowRequest`] is.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtWindowOp {
    /// `CSI 1 t`.
    Deiconify = 0,
    /// `CSI 2 t`.
    Iconify = 1,
    /// `CSI 3 ; x ; y t`. `x` and `y` are screen pixels for the outer frame.
    Move = 2,
    /// `CSI 4 ; height ; width t`. `x` is the width and `y` the height, in
    /// pixels — **and a zero means "leave that axis where it is"**, which is
    /// what upstream's `DispResizeWin` does with an omitted one.
    ResizePixels = 3,
    /// `CSI 5 t`. Raise **without taking focus**, which is upstream's choice:
    /// `BringWindowToTop` and a taskbar flash, not `SetForegroundWindow`.
    Raise = 4,
    /// `CSI 6 t`.
    Lower = 5,
    /// `CSI 7 t` — repaint the whole window.
    Refresh = 6,
    /// `CSI 9 ; 0 t` and `CSI 10 ; 0 t`.
    Unmaximize = 7,
    /// `CSI 9 ; 1 t` and `CSI 10 ; 1 t`. **`CSI 10 t` is maximise here, not
    /// full screen** — upstream's comment says so.
    Maximize = 8,
    /// `CSI 10 ; 2 t`.
    ToggleMaximize = 9,
}

/// One thing `CSI Ps t` asked the window to do.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtWindowRequest {
    pub op: TtWindowOp,
    /// Meaningful for [`TtWindowOp::Move`] and [`TtWindowOp::ResizePixels`].
    pub x: i32,
    pub y: i32,
}

/// The window operations the last [`tt_session_drain_events`] turned up, in
/// the order the host asked for them.
///
/// Announced by [`TtEventKind::WindowRequest`] and read separately for the
/// reason a transfer's progress is: [`TtEvent`] is a fixed struct, and giving
/// it fields for one event would be an ABI break for every other.
///
/// **The array is borrowed and valid until the next event drain on this
/// session**, and reading it does not consume it — one call answers however
/// many of those events came out of that drain.
///
/// A frontend that cannot honour an operation should drop it. Wayland has no
/// request to place a window, so [`TtWindowOp::Move`] cannot be carried out
/// there — and must not be pretended, because `CSI 13 t` answers from
/// [`tt_session_set_window_metrics`] and a pretence becomes a lie on the wire.
#[no_mangle]
pub extern "C" fn tt_session_window_requests(
    session: *mut TtSession,
    out: *mut *const TtWindowRequest,
) -> usize {
    let s = session!(session, 0);
    if let Some(out) = unsafe { out.as_mut() } {
        *out = if s.window_requests.is_empty() {
            ptr::null()
        } else {
            s.window_requests.as_ptr()
        };
    }
    s.window_requests.len()
}

/// Which media-copy operation a [`TtPrinterEvent`] is.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtPrinterOp {
    /// A job begins. Nothing prints until [`TtPrinterOp::Close`].
    Open = 0,
    /// Code points for the open job, in `text`.
    Write = 1,
    /// The job is complete and can be sent to the printer. Upstream waits
    /// `PassThruDelay` seconds first, and that timer is the frontend's.
    Close = 2,
    /// `CSI 0 i` — print the screen. Not a byte stream: upstream renders the
    /// grid graphically through the print dialog, so this is a request.
    Screen = 3,
}

/// One thing `CSI Ps i` or `CSI ? Ps i` asked the printer for.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtPrinterEvent {
    pub op: TtPrinterOp,
    /// UTF-8 for [`TtPrinterOp::Write`], null otherwise.
    ///
    /// **Code points, not the printer's bytes.** Upstream spools UTF-32 and
    /// converts with `UTF32ToMBCP(u32, CP_ACP)` on the way out, so a control
    /// byte the host sent arrives here as the character of that value and the
    /// encoding on the way to the device is the frontend's decision.
    pub text: *const c_char,
    /// For [`TtPrinterOp::Screen`]: nonzero when DECPEX asked for the scroll
    /// region rather than the whole screen.
    pub scroll_region: u8,
}

/// The printer operations the last [`tt_session_drain_events`] turned up, in
/// the order the host asked for them.
///
/// Announced by [`TtEventKind::Printer`] and read separately for the reason
/// [`tt_session_window_requests`] is. **The array and every `text` in it are
/// borrowed and valid until the next event drain on this session**, and reading
/// does not consume.
///
/// A frontend with no printer can drop the lot, and dropping is complete: the
/// terminal never waits on a job and the host is told nothing either way. What
/// it must not do is act on a `Write` it has not seen an `Open` for.
#[no_mangle]
pub extern "C" fn tt_session_printer_events(
    session: *mut TtSession,
    out: *mut *const TtPrinterEvent,
) -> usize {
    let s = session!(session, 0);
    if let Some(out) = unsafe { out.as_mut() } {
        *out = if s.printer_events.is_empty() {
            ptr::null()
        } else {
            s.printer_events.as_ptr()
        };
    }
    s.printer_events.len()
}

fn palette_rgb_one(color: tt_vt::palette::Rgb, r: *mut u8, g: *mut u8, b: *mut u8) {
    let (pr, pg, pb) = color;
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
}

/// An entry from the compiled-in default palette. False for `index > 255`.
///
/// Kept for callers that need colours before they own a session. A painter of
/// a live terminal should use [`tt_session_palette_rgb`], because `ANSIColor`
/// can replace its first sixteen entries.
#[no_mangle]
pub extern "C" fn tt_palette_rgb(index: u32, r: *mut u8, g: *mut u8, b: *mut u8) -> bool {
    palette_rgb(tt_vt::palette::default_palette(), index, r, g, b)
}

// --- settings -------------------------------------------------------------

/// What a setting holds — enough for a dialog to pick a widget and no more.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtSettingKind {
    /// On or off. Note the value is spelled `on`/`off`, not `1`/`0`, and that
    /// **the file's parse of it is not symmetric** — see [`TtSettingField`].
    Bool,
    Int,
    /// An int with bounds: `min` and `max` are meaningful.
    IntRange,
    Str,
    /// One of `choices` spellings, fetched with [`tt_settings_choice`].
    Enum,
    /// Six numbers, `fg_r,fg_g,fg_b,bg_r,bg_g,bg_b`. Upstream's attributes
    /// each carry their own foreground *and* background, which is why a colour
    /// setting is a pair rather than one value.
    Color2,
}

/// One setting, as data — the row a generated dialog builds a widget from.
///
/// **This is the point of having a schema at all.** `PLAN.md` puts ~13.8k
/// lines of dialog code over Tera Term's 909-line settings struct, across 76
/// dialog templates; a dialog that reads this table has nothing to keep in
/// step with it, while one generated as C++ would be a second copy of the list
/// living in the other build system.
///
/// Every string is `NUL`-terminated, never null except `label`, and **lives
/// for the life of the process**: they describe the schema rather than any
/// session's values, so unlike everything else in this header they need no
/// "valid until" rule.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtSettingField {
    /// The dotted name a script and [`tt_session_setting`] use.
    pub name: *const c_char,
    /// Everything before the first dot: which page of the dialog it belongs on.
    pub page: *const c_char,
    /// Its `TERATERM.INI` section and key. Two settings may share a key —
    /// `TerminalSize` holds both `terminal.cols` and `terminal.rows`.
    pub section: *const c_char,
    pub key: *const c_char,
    /// The default, in the INI's own spelling. For a `Bool` this is
    /// load-bearing rather than cosmetic: `GetOnOff` is default-biased
    /// (`ttset.c:344`), so with a default of `on` anything but `off` reads as
    /// on, and with a default of `off` only `on` does. `Key=1` therefore means
    /// opposite things for two settings out of the same file.
    pub default_value: *const c_char,
    /// The `.lng` key for the label, or **null** where upstream has no dialog
    /// for this setting. Those are real settings people set by hand; they have
    /// no widget yet, not no meaning.
    pub label: *const c_char,
    /// The schema's own comment, which is where the citation for the default
    /// lives. Meant for a tooltip and for the generated documentation.
    pub doc: *const c_char,
    pub kind: TtSettingKind,
    /// Bounds, for `TT_SETTING_KIND_INT_RANGE`. Note what the file does
    /// outside them, because a spin box cannot express it: at or below `min`
    /// takes the *default*, above `max` takes `max` (`ttset.c:615`).
    pub min: i32,
    pub max: i32,
    /// How many spellings an `Enum` accepts; zero for every other kind.
    pub choices: usize,
}

/// The schema as C sees it. Built once and never freed — it is 39 rows of
/// static description, and handing out pointers that outlive every call is
/// what lets a dialog keep them.
struct Schema {
    fields: Vec<TtSettingField>,
    choices: Vec<Vec<*const c_char>>,
}

// Raw pointers into leaked, immutable, `NUL`-terminated strings. Nothing here
// is ever written after `OnceLock` publishes it.
unsafe impl Send for Schema {}
unsafe impl Sync for Schema {}

static SCHEMA: OnceLock<Schema> = OnceLock::new();

/// A string that lives as long as the process. Leaked deliberately: the
/// alternative is a `Vec<CString>` whose pointers a caller has to stop using
/// at some moment nobody can name.
fn leak(s: &str) -> *const c_char {
    cstring(s).into_raw().cast_const()
}

fn schema() -> &'static Schema {
    SCHEMA.get_or_init(|| {
        let mut fields = Vec::with_capacity(tt_session::FIELDS.len());
        let mut choices = Vec::with_capacity(tt_session::FIELDS.len());
        for f in tt_session::FIELDS {
            let (kind, min, max) = match f.kind {
                tt_session::Kind::Bool => (TtSettingKind::Bool, 0, 0),
                tt_session::Kind::Int => (TtSettingKind::Int, i32::MIN, i32::MAX),
                tt_session::Kind::IntRange(lo, hi) => (TtSettingKind::IntRange, lo, hi),
                // A floor with no ceiling is still the pair a spin box needs,
                // so it does not earn a `TtSettingKind` of its own. What the
                // two kinds disagree about is what happens to a value *below*
                // the bound — the default for one, the bound itself for the
                // other — and that is settled in the core before a dialog ever
                // sees the number.
                tt_session::Kind::IntMin(lo) => (TtSettingKind::IntRange, lo, i32::MAX),
                // Same argument for the third bound: the spin box wants the
                // pair and nothing else.
                tt_session::Kind::IntClamp(lo, hi) => (TtSettingKind::IntRange, lo, hi),
                tt_session::Kind::IntWord => (TtSettingKind::IntRange, 0, u16::MAX as i32),
                tt_session::Kind::IntByte => (TtSettingKind::IntRange, 0, u8::MAX as i32),
                tt_session::Kind::Str => (TtSettingKind::Str, 0, 0),
                tt_session::Kind::Enum(_) => (TtSettingKind::Enum, 0, 0),
                tt_session::Kind::Color2 => (TtSettingKind::Color2, 0, 0),
            };
            let spellings: Vec<*const c_char> = match f.kind {
                tt_session::Kind::Enum(list) => list.iter().map(|s| leak(s)).collect(),
                _ => Vec::new(),
            };
            fields.push(TtSettingField {
                name: leak(f.name),
                page: leak(f.page),
                section: leak(f.section),
                key: leak(f.key),
                default_value: leak(f.default),
                label: f.label.map_or(ptr::null(), leak),
                doc: leak(f.doc),
                kind,
                min,
                max,
                choices: spellings.len(),
            });
            choices.push(spellings);
        }
        Schema { fields, choices }
    })
}

/// How many settings there are. The index runs `0..count` and is stable for
/// the life of the process, but **not across versions** — the schema grows.
#[no_mangle]
pub extern "C" fn tt_settings_field_count() -> usize {
    schema().fields.len()
}

/// Describe setting `index`. False, and `out` untouched, past the end.
#[no_mangle]
pub extern "C" fn tt_settings_field(index: usize, out: *mut TtSettingField) -> bool {
    let Some(out) = (unsafe { out.as_mut() }) else {
        set_error("null TtSettingField");
        return false;
    };
    match schema().fields.get(index) {
        Some(f) => {
            *out = *f;
            true
        }
        None => false,
    }
}

/// The `n`th spelling an `Enum` setting accepts, or null.
///
/// These are the INI's own spellings, which is what
/// [`tt_session_set_setting`] takes and [`tt_session_setting`] returns — so a
/// combo box can be built out of them and read back without a table of its
/// own. They are not translated; the `.lng` label belongs to the setting, not
/// to its values.
#[no_mangle]
pub extern "C" fn tt_settings_choice(index: usize, n: usize) -> *const c_char {
    schema()
        .choices
        .get(index)
        .and_then(|c| c.get(n))
        .copied()
        .unwrap_or(ptr::null())
}

/// One setting's current value, in the INI's own spelling. Null for a name
/// that is not in the schema.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_setting(
    session: *mut TtSession,
    name: *const c_char,
) -> *const c_char {
    let s = session!(session, ptr::null());
    let Ok(name) = (unsafe { str_arg(name, usize::MAX) }) else {
        return ptr::null();
    };
    match s.session.setting(name) {
        Some(v) => {
            s.setting = cstring(&v);
            s.setting.as_ptr()
        }
        None => {
            set_error(format!("no setting named {name}"));
            ptr::null()
        }
    }
}

/// What ends a word, for a double-click — `DelimList`, decoded.
///
/// Not reachable through [`tt_session_setting`], deliberately: that returns the
/// file's own spelling, and this setting's spelling is `Hex2StrW`'s `$xx`
/// escape. Its default *opens* with one — `$20!"#$24%…` — so a frontend that
/// read the raw string would have a list with no space in it and every word
/// running into the next. UTF-8, and characters rather than bytes, because it
/// is compared against what is on the screen.
///
/// Borrowed, and valid until the next call to this function on this session.
#[no_mangle]
pub extern "C" fn tt_session_word_delimiters(session: *mut TtSession) -> *const c_char {
    let s = session!(session, ptr::null());
    s.delimiters = cstring(&s.session.word_delimiters());
    s.delimiters.as_ptr()
}

/// Set one setting by name and apply it to the running terminal.
///
/// The value is parsed exactly as the file would parse it — bounds, quote
/// stripping, and `GetOnOff`'s default-biased booleans included — so a dialog,
/// a script and a hand-edited `TERATERM.INI` cannot disagree about what a
/// value means. An out-of-range number is therefore **not** an error; it lands
/// where the file would put it.
///
/// `TT_ERR_INVALID` for a name that is not in the schema, which is the only
/// thing this refuses: applying is local and cannot fail, so a failure here
/// always means the *name* was wrong. See [`tt_session::Session::set_settings`]
/// for why telling the far end about a resize is not part of that answer.
///
/// **It overwrites modes the host set**, and that is upstream's behaviour
/// rather than an oversight: in Tera Term the setting and the mode are the
/// same variable, so `DECSET 67` writes `ts.BSKey` and the settings dialog
/// writes it back.
#[no_mangle]
pub extern "C" fn tt_session_set_setting(
    session: *mut TtSession,
    name: *const c_char,
    value: *const c_char,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let (name, value) = match unsafe { (str_arg(name, usize::MAX), str_arg(value, usize::MAX)) } {
        (Ok(n), Ok(v)) => (n, v),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    if s.session.set_setting(name, value) {
        TT_OK
    } else {
        fail(TT_ERR_INVALID, format!("no setting named {name}"))
    }
}

/// Read `TERATERM.INI` and apply all of it.
///
/// A file that does not exist reads as an empty one — every setting takes its
/// default — because that is a first run, not a failure.
#[no_mangle]
pub extern "C" fn tt_session_settings_load(
    session: *mut TtSession,
    path: *const c_char,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let ini = match Ini::load(std::path::Path::new(path)) {
        Ok(ini) => ini,
        Err(e) => return fail(TT_ERR_IO, format!("{path}: {e}")),
    };
    s.session.set_settings(Settings::load(&ini));
    TT_OK
}

/// Copy the live settings from one session to another.
///
/// Session duplication needs the source window's current values, not a second
/// read of its INI file. The live grid size is folded in for the same reason
/// [`settings_for_save`] does it: a user resize updates the terminal, while
/// the schema remains the snapshot last loaded from disk.
#[no_mangle]
pub extern "C" fn tt_session_copy_settings(
    destination: *mut TtSession,
    source: *const TtSession,
) -> TtStatus {
    if destination.is_null() || source.is_null() {
        return fail(TT_ERR_INVALID, "null session");
    }
    if std::ptr::eq(destination.cast_const(), source) {
        return TT_OK;
    }
    let source = unsafe { &*source };
    let mut settings = source.session.settings().clone();
    settings.terminal_cols = source.session.grid().cols() as i32;
    settings.terminal_rows = source.session.grid().rows() as i32;
    let destination = unsafe { &mut *destination };
    destination.session.set_settings(settings);
    TT_OK
}

/// Where a full settings save gets the window position it may write.
enum SavedWindowPosition {
    /// The values already in `Settings`, for a caller with no window.
    Settings,
    /// A frontend's live answer. `None` means the window system cannot report
    /// a useful position — Wayland deliberately cannot — so the old line must
    /// survive byte-for-byte even when `SaveVTWinPos` is on.
    Live(Option<(i32, i32)>),
}

fn settings_for_save(s: &TtSession) -> Settings {
    let mut settings = s.session.settings().clone();
    // `ts.TerminalWidth/Height` are live variables upstream, updated with the
    // window. The schema is a snapshot from the last load, so take the grid at
    // the moment of saving or dragging an 80x24 window to 132x50 would write
    // the old size back with complete confidence.
    settings.terminal_cols = s.session.grid().cols() as i32;
    settings.terminal_rows = s.session.grid().rows() as i32;
    settings
}

fn save_all_settings(s: &TtSession, path: &Path, position: SavedWindowPosition) -> TtStatus {
    let mut ini = match Ini::load(path) {
        Ok(ini) => ini,
        Err(e) => return fail(TT_ERR_IO, format!("{}: {e}", path.display())),
    };
    let mut settings = settings_for_save(s);

    match position {
        SavedWindowPosition::Settings => {}
        SavedWindowPosition::Live(Some((x, y))) => {
            settings.window_x = x;
            settings.window_y = y;
        }
        SavedWindowPosition::Live(None) if settings.window_save_position => {
            // Make `Settings::store` skip VTPos while retaining the real value
            // of SaveVTWinPos below. Restoring the parsed value afterwards is
            // not equivalent: it would strip a matched pair of quotes from a
            // line this save was supposed to leave alone.
            settings.window_save_position = false;
            settings.store(&mut ini);
            ini.set("Tera Term", "SaveVTWinPos", "on");
            return match ini.save(path) {
                Ok(()) => TT_OK,
                Err(e) => fail(TT_ERR_IO, format!("{}: {e}", path.display())),
            };
        }
        SavedWindowPosition::Live(None) => {}
    }

    settings.store(&mut ini);
    match ini.save(path) {
        Ok(()) => TT_OK,
        Err(e) => fail(TT_ERR_IO, format!("{}: {e}", path.display())),
    }
}

/// Write every setting back, leaving the rest of the file alone.
///
/// The file is re-read first and only the keys the schema owns are touched, so
/// comments, ordering, spelling and every setting this project does not know
/// about survive — which is what makes a `TERATERM.INI` shared with a real
/// Tera Term keep working. The terminal size is read from the live grid rather
/// than from the last settings load. A caller with a window should use
/// [`tt_session_settings_save_for_window`] so `VTPos` is live too.
#[no_mangle]
pub extern "C" fn tt_session_settings_save(
    session: *const TtSession,
    path: *const c_char,
) -> TtStatus {
    let s = session_ref!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    save_all_settings(s, Path::new(path), SavedWindowPosition::Settings)
}

/// Save every setting, taking the live position from the frontend's window.
///
/// `position_valid` is false when the window system does not have a client-
/// controlled position. Wayland is that case: `QWidget::pos()` commonly says
/// `(0,0)` and `move()` is deliberately ignored. With it false an existing
/// `VTPos` line is preserved byte-for-byte, while the live terminal size and
/// every other setting are still saved.
#[no_mangle]
pub extern "C" fn tt_session_settings_save_for_window(
    session: *const TtSession,
    path: *const c_char,
    x: i32,
    y: i32,
    position_valid: bool,
) -> TtStatus {
    let s = session_ref!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    save_all_settings(
        s,
        Path::new(path),
        SavedWindowPosition::Live(position_valid.then_some((x, y))),
    )
}

/// Persist only the window geometry on close.
///
/// This is upstream's `SaveVTPos`, not a shortened Save setup: when
/// `SaveVTWinPos` is off it does nothing, and when it is on it writes only
/// `VTPos` (if the window system has one) and the live `TerminalSize`. A close
/// must not pin every schema default into a file merely because the user
/// enabled position memory.
#[no_mangle]
pub extern "C" fn tt_session_window_geometry_save(
    session: *const TtSession,
    path: *const c_char,
    x: i32,
    y: i32,
    position_valid: bool,
) -> TtStatus {
    let s = session_ref!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let settings = settings_for_save(s);
    if !settings.window_save_position {
        return TT_OK;
    }

    let path = Path::new(path);
    let mut ini = match Ini::load(path) {
        Ok(ini) => ini,
        Err(e) => return fail(TT_ERR_IO, format!("{}: {e}", path.display())),
    };
    if position_valid {
        ini.set("Tera Term", "VTPos", &format!("{x},{y}"));
    }
    ini.set(
        "Tera Term",
        "TerminalSize",
        &format!("{},{}", settings.terminal_cols, settings.terminal_rows),
    );
    match ini.save(path) {
        Ok(()) => TT_OK,
        Err(e) => fail(TT_ERR_IO, format!("{}: {e}", path.display())),
    }
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
    /// The transport went away. Reported once. The screen is left alone by
    /// default because the text explaining why it dropped is the reason
    /// anyone looks; `ClearScreenOnCloseConnection` can change that.
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
    /// A file transfer moved. Read [`tt_session_transfer_status`] for where
    /// it has got to.
    TransferProgress = 7,
    /// A file transfer ended, for any reason. Read
    /// [`tt_session_transfer_result`] for how it went — and read it *on this
    /// event*, because the next transfer replaces it.
    TransferDone = 8,
    /// Make a noise. Already governed — the core has thinned a runaway host
    /// down to the bells Tera Term would have sounded — so a frontend should
    /// beep on every one of these and needs no rate limit of its own.
    Bell = 9,
    /// Flash the screen instead, which is what `Beep=visual` asks for. Invert
    /// it for `bell.visual_wait_ms` milliseconds; the setting is read by name
    /// through [`tt_settings_field`] like every other one the window draws
    /// with.
    ///
    /// A second kind rather than a flag on [`TtEventKind::Bell`] because the
    /// two are different actions, and because a frontend with no way to flash
    /// can ignore this one and still be honest about it.
    VisualBell = 10,
    /// An authorised OSC 52 read. `text` is the ASCII selection name; read
    /// the system clipboard and call [`tt_session_clipboard_reply`]. `byte`
    /// is nonzero when the user also asked to be notified.
    ClipboardRead = 11,
    /// An authorised OSC 52 write. `text` is decoded UTF-8 to put on the
    /// system clipboard; `byte` is the notification flag.
    ClipboardWrite = 12,
    /// An OSC 52 read rejected by the setting. It is emitted only when remote
    /// clipboard notifications are enabled.
    ClipboardReadRejected = 13,
    /// The corresponding rejected write.
    ClipboardWriteRejected = 14,
    /// `AutoWinClose` after a network connection ended. Close the window if
    /// it can close now; serial ports and local ptys never emit this.
    CloseRequested = 15,
    /// A colour OSC moved something the painter caches. Re-read
    /// [`tt_session_palette_rgb`] and [`tt_session_color_rgb`], then repaint.
    ///
    /// Separate from [`TtEventKind::Damage`], which says the cells changed:
    /// re-reading 262 colours on every pump would pay for a change that
    /// happens once a session, if at all.
    ColorsChanged = 16,
    /// `CSI 1`-`10 t` asked the window to move, resize, iconify, raise, lower,
    /// repaint or maximise. Read [`tt_session_window_requests`] for what — one
    /// call answers however many of these came out of this drain.
    WindowRequest = 17,
    /// `CSI Ps i` or `CSI ? Ps i` asked the printer for something. Read
    /// [`tt_session_printer_events`] for what — one call answers however many
    /// of these came out of this drain.
    Printer = 18,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtEvent {
    pub kind: TtEventKind,
    /// The byte for [`TtEventKind::BadByte`], or the notification flag for an
    /// authorised clipboard event.
    pub byte: u8,
    /// Meaningful for [`TtEventKind::Resize`] only.
    pub cols: u16,
    pub rows: u16,
    /// Meaningful for [`TtEventKind::Title`], [`TtEventKind::LogFailed`], and
    /// authorised clipboard events; null otherwise.
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
    s.window_requests.clear();
    s.printer_events.clear();
    s.printer_texts.clear();
    for ev in s.session.drain_events() {
        let mut size = (0u16, 0u16);
        let (kind, byte, text) = match ev {
            Event::Damage => (TtEventKind::Damage, 0, ptr::null()),
            Event::Title(t) => {
                s.event_texts.push(cstring(&t));
                let p = s.event_texts.last().expect("just pushed").as_ptr();
                (TtEventKind::Title, 0, p)
            }
            Event::ColorsChanged => (TtEventKind::ColorsChanged, 0, ptr::null()),
            Event::WindowRequest(req) => {
                use tt_session::WindowRequest as W;
                let (op, x, y) = match req {
                    W::Deiconify => (TtWindowOp::Deiconify, 0, 0),
                    W::Iconify => (TtWindowOp::Iconify, 0, 0),
                    W::Move(x, y) => (TtWindowOp::Move, x, y),
                    W::ResizePixels { width, height } => (TtWindowOp::ResizePixels, width, height),
                    W::Raise => (TtWindowOp::Raise, 0, 0),
                    W::Lower => (TtWindowOp::Lower, 0, 0),
                    W::Refresh => (TtWindowOp::Refresh, 0, 0),
                    W::Unmaximize => (TtWindowOp::Unmaximize, 0, 0),
                    W::Maximize => (TtWindowOp::Maximize, 0, 0),
                    W::ToggleMaximize => (TtWindowOp::ToggleMaximize, 0, 0),
                };
                s.window_requests.push(TtWindowRequest { op, x, y });
                (TtEventKind::WindowRequest, 0, ptr::null())
            }
            Event::Printer(p) => {
                use tt_session::PrinterEvent as P;
                let (op, text, scroll_region) = match p {
                    P::Open => (TtPrinterOp::Open, ptr::null(), 0),
                    P::Write(w) => {
                        s.printer_texts.push(cstring(&w));
                        let ptr = s.printer_texts.last().expect("just pushed").as_ptr();
                        (TtPrinterOp::Write, ptr, 0)
                    }
                    P::Close => (TtPrinterOp::Close, ptr::null(), 0),
                    P::Screen { scroll_region } => {
                        (TtPrinterOp::Screen, ptr::null(), u8::from(scroll_region))
                    }
                };
                s.printer_events.push(TtPrinterEvent {
                    op,
                    text,
                    scroll_region,
                });
                (TtEventKind::Printer, 0, ptr::null())
            }
            Event::Break => (TtEventKind::Break, 0, ptr::null()),
            Event::BadByte(b) => (TtEventKind::BadByte, b, ptr::null()),
            Event::Disconnected => (TtEventKind::Disconnected, 0, ptr::null()),
            Event::CloseRequested => (TtEventKind::CloseRequested, 0, ptr::null()),
            Event::Resize { cols, rows } => {
                size = (cols, rows);
                (TtEventKind::Resize, 0, ptr::null())
            }
            Event::LogFailed(msg) => {
                s.event_texts.push(CString::new(msg).unwrap_or_default());
                let p = s.event_texts.last().expect("just pushed").as_ptr();
                (TtEventKind::LogFailed, 0, p)
            }
            // The payload does not travel in the event: `TtEvent` is a fixed
            // struct and a transfer's is two strings and six numbers. It is
            // read through its own accessor instead, which is also what lets
            // a frontend poll a progress bar without draining.
            Event::TransferProgress(_) => (TtEventKind::TransferProgress, 0, ptr::null()),
            Event::TransferDone(outcome) => {
                s.transfer_result = Some(*outcome);
                (TtEventKind::TransferDone, 0, ptr::null())
            }
            Event::Bell { visual: false } => (TtEventKind::Bell, 0, ptr::null()),
            Event::Bell { visual: true } => (TtEventKind::VisualBell, 0, ptr::null()),
            Event::Clipboard(request) => match request {
                ClipboardRequest::Read { selection, notify } => {
                    s.event_texts.push(cstring(&selection));
                    let p = s.event_texts.last().expect("just pushed").as_ptr();
                    (TtEventKind::ClipboardRead, u8::from(notify), p)
                }
                ClipboardRequest::Write { text, notify } => {
                    s.event_texts.push(cstring(&text));
                    let p = s.event_texts.last().expect("just pushed").as_ptr();
                    (TtEventKind::ClipboardWrite, u8::from(notify), p)
                }
                ClipboardRequest::ReadRejected => {
                    (TtEventKind::ClipboardReadRejected, 0, ptr::null())
                }
                ClipboardRequest::WriteRejected => {
                    (TtEventKind::ClipboardWriteRejected, 0, ptr::null())
                }
            },
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

// --- file transfer --------------------------------------------------------

/// Which protocol. Mirrors `tt_xfer::Job`'s discriminants.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtXferProtocol {
    XModem = 0,
    YModem = 1,
    ZModem = 2,
    Kermit = 3,
    BPlus = 4,
    QuickVan = 5,
    /// Not a protocol: read the connection into a file until it goes quiet.
    /// Receive only.
    Raw = 6,
}

/// How to run a transfer.
///
/// One struct rather than a function per protocol, because a frontend builds
/// it from a dialog: the user picks a protocol and the options that belong to
/// it, and the fields that do not apply are ignored. Which those are is in
/// `tt-xfer`'s `Job` — XMODEM has a text flag and no filename on the wire,
/// Kermit has four modes and no direction, Raw has neither.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtXferJob {
    pub protocol: TtXferProtocol,
    /// Ignored by Kermit, which takes `kermit_mode` instead, and by Raw.
    pub sending: bool,
    /// XMODEM: 1 = checksum, 2 = CRC, 3 = 1K CRC, 4 = 1K checksum.
    /// YMODEM: 1 = 1K, 2 = G, 3 = single. Zero takes the setting's default,
    /// which for XMODEM is **checksum** (`ttset.c:1039`'s `else` branch, not
    /// the CRC a reader would expect) and for YMODEM is the *only* value its
    /// packet builder handles. Call [`tt_session_xfer_defaults`] rather than
    /// leaving this zero: it answers from the user's own file.
    pub option: i32,
    /// XMODEM only: CRLF translation and `^Z` padding. The inverse of
    /// `XmodemBin`, which is its own setting and not `binary` below.
    pub text: bool,
    /// ZMODEM only. Clearing it asks for end-of-line translation, which is
    /// almost never wanted — but upstream's `TransBin` ships **off**, so a
    /// struct seeded by [`tt_session_xfer_defaults`] starts with it clear.
    pub binary: bool,
    /// ZMODEM and B-Plus: the peer's trigger has already gone past in the
    /// terminal stream, so the protocol pushes it back and reads it itself.
    pub auto_start: bool,
    /// Kermit only: 1 = receive, 2 = get, 3 = send, 4 = finish.
    pub kermit_mode: i32,
    /// Raw only: stop after this many seconds of silence.
    pub autostop_sec: i32,
}

/// Where a running transfer has got to.
///
/// **Every number here is something the protocol reported, and the protocols
/// throttle themselves to ten updates a second** (`zmodem.c:197`). A transfer
/// that finishes in under a tenth of a second finishes having reported
/// nothing, so a frontend must not read `bytes == 0` as "not started".
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtTransferStatus {
    /// "ZMODEM", "Kermit" — the protocol's own word for itself. Borrowed and
    /// valid until the next call to this function on this session.
    pub protocol: *const c_char,
    /// The file in flight, as the protocol named it. Empty before the first
    /// one is opened.
    pub file: *const c_char,
    pub sending: bool,
    pub bytes: i64,
    pub packets: i64,
    /// Position within the current file, and its size. `total` is 0 when the
    /// size is not known — XMODEM never learns it.
    pub done: i64,
    pub total: i64,
    /// The whole-percent high-water mark, or -1 when there is no meaningful
    /// bar to draw.
    pub percent: i32,
    pub elapsed_ms: u32,
}

/// How the last transfer ended.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtTransferResult {
    pub success: bool,
    pub cancelled: bool,
    /// What the protocol said when it failed — "Cannot create file" — or null.
    /// Often the only account of the failure there is. Borrowed, and valid
    /// until the next call to this function on this session.
    pub message: *const c_char,
    pub bytes: i64,
    pub elapsed_ms: u32,
}

fn job_from(j: &TtXferJob) -> tt_xfer::Job {
    use tt_xfer::{Direction, Job, KermitMode, XmodemOpt, YmodemOpt};
    let dir = if j.sending {
        Direction::Send
    } else {
        Direction::Receive
    };
    match j.protocol {
        TtXferProtocol::XModem => Job::XModem {
            dir,
            opt: match j.option {
                1 => XmodemOpt::Checksum,
                2 => XmodemOpt::Crc,
                3 => XmodemOpt::Crc1K,
                4 => XmodemOpt::Checksum1K,
                // Deliberately `Default` rather than a fourth spelling of
                // upstream's default: one answer to "what is XMODEM's block
                // format when nobody said", in one place.
                _ => XmodemOpt::default(),
            },
            text: j.text,
        },
        TtXferProtocol::YModem => Job::YModem {
            dir,
            opt: match j.option {
                2 => YmodemOpt::G,
                3 => YmodemOpt::Single,
                _ => YmodemOpt::K1,
            },
        },
        TtXferProtocol::ZModem => Job::ZModem {
            dir,
            binary: j.binary,
            auto: j.auto_start,
        },
        TtXferProtocol::Kermit => Job::Kermit {
            mode: match j.kermit_mode {
                2 => KermitMode::Get,
                3 => KermitMode::Send,
                4 => KermitMode::Finish,
                _ => KermitMode::Receive,
            },
        },
        TtXferProtocol::BPlus => Job::BPlus {
            dir,
            auto: j.auto_start,
        },
        TtXferProtocol::QuickVan => Job::QuickVan { dir },
        TtXferProtocol::Raw => Job::Raw {
            autostop: Duration::from_secs(j.autostop_sec.max(0) as u64),
        },
    }
}

/// Fill `job` with what this session's settings say a transfer should start
/// as, leaving `protocol`, `sending` and `kermit_mode` alone — those are the
/// user's choice and nothing in the file describes them.
///
/// A frontend calls this to seed its dialog. Without it the dialog invents the
/// values, which is how three of them ended up hardcoded here: `binary` was
/// always on, the XMODEM block format was always CRC — where upstream's own
/// default is plain **checksum**, `ttset.c:1039`'s `else` branch — and a raw
/// capture never stopped, because its wait was zero.
///
/// Does nothing on a null pointer.
#[no_mangle]
pub extern "C" fn tt_session_xfer_defaults(session: *mut TtSession, job: *mut TtXferJob) {
    let (Some(s), Some(job)) = (unsafe { session.as_ref() }, unsafe { job.as_mut() }) else {
        set_error("null session or TtXferJob");
        return;
    };
    let d = tt_session::job_defaults(s.session.settings());
    // `option` is per-protocol and the caller has already chosen one, so only
    // XMODEM's is answerable from the file. YMODEM's is the one value its
    // packet builder handles, which `job_from` supplies for a zero.
    if job.protocol == TtXferProtocol::XModem {
        job.option = match d.xmodem_opt {
            tt_xfer::XmodemOpt::Checksum => 1,
            tt_xfer::XmodemOpt::Crc => 2,
            tt_xfer::XmodemOpt::Crc1K => 3,
            tt_xfer::XmodemOpt::Checksum1K => 4,
        };
    }
    // XMODEM keeps its own binary flag and upstream ships the two disagreeing
    // — `XmodemBin` on, `TransBin` off — so the text flag is not `!binary`.
    job.text = !d.xmodem_binary;
    job.binary = d.binary;
    job.auto_start = match job.protocol {
        TtXferProtocol::ZModem => d.zmodem_auto,
        TtXferProtocol::BPlus => d.bplus_auto,
        _ => false,
    };
    job.autostop_sec = d.raw_autostop.as_secs() as i32;
}

/// Start sending files. `paths` is `count` UTF-8 paths.
///
/// **The terminal goes deaf and mute for the duration**: keystrokes are
/// dropped and the protocol's traffic never reaches the parser, which is what
/// upstream's modal transfer dialog achieves by other means. One transfer at a
/// time per session.
#[no_mangle]
pub extern "C" fn tt_session_send_files(
    session: *mut TtSession,
    job: *const TtXferJob,
    paths: *const *const c_char,
    count: usize,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let Some(job) = (unsafe { job.as_ref() }) else {
        set_error("null TtXferJob");
        return TT_ERR_INVALID;
    };
    if paths.is_null() || count == 0 {
        set_error("no files to send");
        return TT_ERR_INVALID;
    }
    let raw = unsafe { slice::from_raw_parts(paths, count) };
    let mut files = Vec::with_capacity(count);
    for p in raw {
        match unsafe { str_arg(*p, usize::MAX) } {
            Ok(p) => files.push(std::path::PathBuf::from(p)),
            Err(e) => return e,
        }
    }
    let opts = s.session.transfer_options();
    match s.session.send_files(job_from(job), &files, &opts) {
        Ok(()) => TT_OK,
        Err(e) => xfer_error(e),
    }
}

/// Start receiving into `dir`.
///
/// `name` is XMODEM's alone and may be null for everything else: XMODEM's wire
/// format carries no filename, so there is nothing to derive a destination
/// from. Supplying one to a protocol that *does* carry a name would override
/// the peer's, so it is ignored there — which is what upstream's receive
/// dialog does with the same field.
#[no_mangle]
pub extern "C" fn tt_session_receive_files(
    session: *mut TtSession,
    job: *const TtXferJob,
    dir: *const c_char,
    name: *const c_char,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let Some(job) = (unsafe { job.as_ref() }) else {
        set_error("null TtXferJob");
        return TT_ERR_INVALID;
    };
    let dir = match unsafe { str_arg(dir, usize::MAX) } {
        Ok(d) => d,
        Err(e) => return e,
    };
    let name = if name.is_null() {
        None
    } else {
        match unsafe { str_arg(name, usize::MAX) } {
            Ok(n) => Some(n),
            Err(e) => return e,
        }
    };
    let opts = s.session.transfer_options();
    match s
        .session
        .receive_files(job_from(job), std::path::Path::new(dir), name, &opts)
    {
        Ok(()) => TT_OK,
        Err(e) => xfer_error(e),
    }
}

fn xfer_error(e: tt_session::TransferError) -> TtStatus {
    use tt_session::TransferError;
    let code = match e {
        TransferError::NotConnected => TT_ERR_DISCONNECTED,
        TransferError::AlreadyRunning => TT_ERR_BUSY,
        TransferError::Protocol(_) => TT_ERR_INVALID,
    };
    set_error(e.to_string());
    code
}

/// Whether a transfer is running, and where it has got to. False when none is.
#[no_mangle]
pub extern "C" fn tt_session_transfer_status(
    session: *mut TtSession,
    out: *mut TtTransferStatus,
) -> bool {
    let s = session!(session, false);
    let Some(st) = s.session.transfer() else {
        return false;
    };
    s.xfer_protocol = cstring(&st.protocol);
    s.xfer_file = cstring(&st.file);
    if let Some(out) = unsafe { out.as_mut() } {
        *out = TtTransferStatus {
            protocol: s.xfer_protocol.as_ptr(),
            file: s.xfer_file.as_ptr(),
            sending: st.sending,
            bytes: st.progress.bytes,
            packets: st.progress.packets,
            done: st.progress.done,
            total: st.progress.total,
            percent: st.progress.percent,
            elapsed_ms: st.progress.elapsed.as_millis() as u32,
        };
    }
    true
}

/// How the last transfer ended. False when none has.
///
/// Read it on [`TtEventKind::TransferDone`]: it is kept until the next
/// transfer finishes and then replaced.
#[no_mangle]
pub extern "C" fn tt_session_transfer_result(
    session: *mut TtSession,
    out: *mut TtTransferResult,
) -> bool {
    let s = session!(session, false);
    let Some(r) = s.transfer_result.clone() else {
        return false;
    };
    s.xfer_message = match &r.message {
        Some(m) => cstring(m),
        None => CString::default(),
    };
    if let Some(out) = unsafe { out.as_mut() } {
        *out = TtTransferResult {
            success: r.success,
            cancelled: r.cancelled,
            message: if r.message.is_some() {
                s.xfer_message.as_ptr()
            } else {
                ptr::null()
            },
            bytes: r.bytes,
            elapsed_ms: r.elapsed.as_millis() as u32,
        };
    }
    true
}

/// Ask the running transfer to stop. A no-op when none is.
///
/// It does not stop here. The protocol sends its cancel sequence and finishes
/// on its own terms — ZMODEM arms a 500 ms timer and ends on that — so keep
/// pumping and wait for [`TtEventKind::TransferDone`].
#[no_mangle]
pub extern "C" fn tt_session_cancel_transfer(session: *mut TtSession) {
    let s = session!(session);
    s.session.cancel_transfer();
}

/// Milliseconds until the running transfer needs attention, or -1 when
/// nothing is armed and -2 when no transfer is running.
///
/// **A frontend that only wakes on [`tt_session_poll_fd`] will stall a
/// transfer.** The protocols retry by timeout — an XMODEM receiver that hears
/// nothing re-sends its `NAK` after ten seconds — and a quiet line produces no
/// descriptor wakeup at all, so nothing would ever fire it. Arm a timer for
/// this value, and re-read it after every pump.
#[no_mangle]
pub extern "C" fn tt_session_transfer_deadline_ms(session: *const TtSession) -> i64 {
    let s = session_ref!(session, -2);
    if s.session.transfer().is_none() {
        return -2;
    }
    match s.session.transfer_deadline() {
        Some(d) => d.as_millis() as i64,
        None => -1,
    }
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

/// Give the transport the wakeup a quiet line cannot give it.
///
/// Telnet's keepalive is the only caller so far, and it is exactly the case
/// [`tt_session_pump`] cannot serve: `IAC NOP` goes out when the link has been
/// *silent* for `TelKeepAliveInterval`, and silence produces no descriptor
/// wakeup and no pump. A frontend that only ever pumps has a keepalive setting
/// that does nothing.
///
/// Cheap on every transport and a no-op with nothing connected, so a
/// once-a-second timer is the whole of what this needs. It writes no bytes to
/// the terminal and raises no events.
#[no_mangle]
pub extern "C" fn tt_session_tick(session: *mut TtSession) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.tick() {
        Ok(()) => TT_OK,
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
/// Returns `-1` on Windows, whose native spelling is
/// [`tt_session_wait_handle`].
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

/// A waitable Windows event that becomes signalled when
/// [`tt_session_pump`] has something to do; null when there is none.
///
/// This is the native Windows spelling of [`tt_session_poll_fd`]. Pass it to
/// `WaitForSingleObject`, `QWinEventNotifier`, or an equivalent event-loop
/// primitive. When it fires, call `tt_session_pump` with a budget of **0**.
/// The handle is borrowed: do not close it, and re-read it after every connect
/// or disconnect. Returns null on Unix, whose native spelling is the fd.
#[no_mangle]
pub extern "C" fn tt_session_wait_handle(session: *const TtSession) -> *mut std::ffi::c_void {
    let s = session_ref!(session, ptr::null_mut());
    #[cfg(windows)]
    {
        s.session.wait_handle().unwrap_or(ptr::null_mut())
    }
    #[cfg(not(windows))]
    {
        let _ = s;
        ptr::null_mut()
    }
}

// --- input ----------------------------------------------------------------

/// What [`tt_session_send_key_code`] did.
pub type TtKeyCodeKind = u32;
pub const TT_KEY_CODE_UNMAPPED: TtKeyCodeKind = 0;
pub const TT_KEY_CODE_SENT: TtKeyCodeKind = 1;
pub const TT_KEY_CODE_LOCAL_KEY: TtKeyCodeKind = 2;
pub const TT_KEY_CODE_UDK: TtKeyCodeKind = 3;
pub const TT_KEY_CODE_SHORTCUT: TtKeyCodeKind = 4;
pub const TT_KEY_CODE_MACRO: TtKeyCodeKind = 5;
pub const TT_KEY_CODE_COMMAND: TtKeyCodeKind = 6;
pub const TT_KEY_CODE_IGNORED: TtKeyCodeKind = 7;

/// A `[Shortcut keys]` action. Values follow Tera Term's internal ids 71–89,
/// so a command dispatcher can keep one table for these and type-3 user keys.
pub type TtShortcut = u32;
pub const TT_SHORTCUT_EDIT_COPY: TtShortcut = 71;
pub const TT_SHORTCUT_EDIT_PASTE: TtShortcut = 72;
pub const TT_SHORTCUT_EDIT_PASTE_CR: TtShortcut = 73;
pub const TT_SHORTCUT_EDIT_CLEAR_SCREEN: TtShortcut = 74;
pub const TT_SHORTCUT_EDIT_CLEAR_BUFFER: TtShortcut = 75;
pub const TT_SHORTCUT_CONTROL_OPEN_TEK: TtShortcut = 76;
pub const TT_SHORTCUT_CONTROL_CLOSE_TEK: TtShortcut = 77;
pub const TT_SHORTCUT_LINE_UP: TtShortcut = 78;
pub const TT_SHORTCUT_LINE_DOWN: TtShortcut = 79;
pub const TT_SHORTCUT_PAGE_UP: TtShortcut = 80;
pub const TT_SHORTCUT_PAGE_DOWN: TtShortcut = 81;
pub const TT_SHORTCUT_BUFFER_TOP: TtShortcut = 82;
pub const TT_SHORTCUT_BUFFER_BOTTOM: TtShortcut = 83;
pub const TT_SHORTCUT_NEXT_WINDOW: TtShortcut = 84;
pub const TT_SHORTCUT_PREVIOUS_WINDOW: TtShortcut = 85;
pub const TT_SHORTCUT_NEXT_SHOWN_WINDOW: TtShortcut = 86;
pub const TT_SHORTCUT_PREVIOUS_SHOWN_WINDOW: TtShortcut = 87;
pub const TT_SHORTCUT_LOCAL_ECHO: TtShortcut = 88;
pub const TT_SHORTCUT_SCROLL_LOCK: TtShortcut = 89;

/// Result of a physical-key lookup.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtKeyCodeResult {
    pub kind: TtKeyCodeKind,
    /// A [`Key`] for `LOCAL_KEY`, 6–20 for `UDK`, one of the
    /// `TT_SHORTCUT_*` constants for `SHORTCUT`, or the menu id for `COMMAND`;
    /// zero otherwise.
    pub value: u32,
    /// The macro path for `MACRO`, null otherwise. Borrowed until the next
    /// call to [`tt_session_send_key_code`] on this session.
    pub text: *const c_char,
}

fn shortcut_id(s: Shortcut) -> TtShortcut {
    match s {
        Shortcut::EditCopy => TT_SHORTCUT_EDIT_COPY,
        Shortcut::EditPaste => TT_SHORTCUT_EDIT_PASTE,
        Shortcut::EditPasteCr => TT_SHORTCUT_EDIT_PASTE_CR,
        Shortcut::EditClearScreen => TT_SHORTCUT_EDIT_CLEAR_SCREEN,
        Shortcut::EditClearBuffer => TT_SHORTCUT_EDIT_CLEAR_BUFFER,
        Shortcut::ControlOpenTek => TT_SHORTCUT_CONTROL_OPEN_TEK,
        Shortcut::ControlCloseTek => TT_SHORTCUT_CONTROL_CLOSE_TEK,
        Shortcut::LineUp => TT_SHORTCUT_LINE_UP,
        Shortcut::LineDown => TT_SHORTCUT_LINE_DOWN,
        Shortcut::PageUp => TT_SHORTCUT_PAGE_UP,
        Shortcut::PageDown => TT_SHORTCUT_PAGE_DOWN,
        Shortcut::BufferTop => TT_SHORTCUT_BUFFER_TOP,
        Shortcut::BufferBottom => TT_SHORTCUT_BUFFER_BOTTOM,
        Shortcut::NextWindow => TT_SHORTCUT_NEXT_WINDOW,
        Shortcut::PreviousWindow => TT_SHORTCUT_PREVIOUS_WINDOW,
        Shortcut::NextShownWindow => TT_SHORTCUT_NEXT_SHOWN_WINDOW,
        Shortcut::PreviousShownWindow => TT_SHORTCUT_PREVIOUS_SHOWN_WINDOW,
        Shortcut::LocalEcho => TT_SHORTCUT_LOCAL_ECHO,
        Shortcut::ScrollLock => TT_SHORTCUT_SCROLL_LOCK,
    }
}

/// Load a `KEYBOARD.CNF`. A missing file installs an empty map; another I/O
/// error leaves the current one intact.
#[no_mangle]
pub extern "C" fn tt_session_key_map_load(
    session: *mut TtSession,
    path: *const c_char,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let path = match unsafe { str_arg(path, usize::MAX) } {
        Ok(path) => path,
        Err(e) => return e,
    };
    match s.session.load_key_map(Path::new(path)) {
        Ok(duplicates) => {
            s.key_duplicates = duplicates;
            TT_OK
        }
        Err(e) => fail(TT_ERR_IO, e.to_string()),
    }
}

/// Number of duplicate scan-code assignments in the last loaded map.
#[no_mangle]
pub extern "C" fn tt_session_key_map_duplicate_count(session: *const TtSession) -> usize {
    session_ref!(session, 0).key_duplicates.len()
}

/// One duplicate scan code, or zero when `index` is out of range.
#[no_mangle]
pub extern "C" fn tt_session_key_map_duplicate(session: *const TtSession, index: usize) -> u16 {
    session_ref!(session, 0)
        .key_duplicates
        .get(index)
        .copied()
        .unwrap_or(0)
}

/// Dispatch a PC/AT set-1 scan code through the active `KEYBOARD.CNF`.
///
/// Shift, Ctrl and Alt are bits `0x200`, `0x400` and `0x800` in `scan`. `out`
/// may be null when only the wire side matters.
#[no_mangle]
pub extern "C" fn tt_session_send_key_code(
    session: *mut TtSession,
    scan: u16,
    out: *mut TtKeyCodeResult,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    s.key_action = CString::default();
    let result = match s.session.send_key_code(scan) {
        Ok(result) => result,
        Err(e) => return report(e),
    };
    let (kind, value) = match result {
        KeyCodeResult::Unmapped => (TT_KEY_CODE_UNMAPPED, 0),
        KeyCodeResult::Sent => (TT_KEY_CODE_SENT, 0),
        KeyCodeResult::LocalKey(key) => (TT_KEY_CODE_LOCAL_KEY, key as u32),
        KeyCodeResult::Udk(n) => (TT_KEY_CODE_UDK, n.into()),
        KeyCodeResult::Shortcut(action) => (TT_KEY_CODE_SHORTCUT, shortcut_id(action)),
        KeyCodeResult::RunMacro(path) => {
            s.key_action = cstring(&path);
            (TT_KEY_CODE_MACRO, 0)
        }
        KeyCodeResult::Command(command) => (TT_KEY_CODE_COMMAND, command.into()),
        KeyCodeResult::Ignored => (TT_KEY_CODE_IGNORED, 0),
    };
    if let Some(out) = unsafe { out.as_mut() } {
        *out = TtKeyCodeResult {
            kind,
            value,
            text: if kind == TT_KEY_CODE_MACRO {
                s.key_action.as_ptr()
            } else {
                ptr::null()
            },
        };
    }
    TT_OK
}

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

/// Put bytes on the wire unchanged: no UTF-8 validation, key table, LNM or
/// other text processing. Used by a macro's binary `send` and by
/// `Meta8Bit=raw` in a frontend. An empty slice succeeds.
#[no_mangle]
pub extern "C" fn tt_session_send_bytes(
    session: *mut TtSession,
    bytes: *const u8,
    len: usize,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    if len == 0 {
        return TT_OK;
    }
    if bytes.is_null() {
        return fail(TT_ERR_INVALID, "null bytes");
    }
    match s
        .session
        .send_bytes(unsafe { slice::from_raw_parts(bytes, len) })
    {
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

/// Answer an authorised [`TtEventKind::ClipboardRead`] with UTF-8 clipboard
/// text. `selection` is the event's `text`; `len` may be `SIZE_MAX` for a
/// NUL-terminated clipboard string.
///
/// `out_sent` (may be null) is false when upstream would intentionally send
/// nothing — its fixed response header cannot hold the selector, or the
/// clipboard contains a control character which makes it binary rather than
/// text. An empty string is text and sends an empty base64 payload.
#[no_mangle]
pub extern "C" fn tt_session_clipboard_reply(
    session: *mut TtSession,
    selection: *const c_char,
    text: *const c_char,
    len: usize,
    out_sent: *mut bool,
) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let selection = match unsafe { str_arg(selection, usize::MAX) } {
        Ok(selection) => selection,
        Err(e) => return e,
    };
    let text = match unsafe { str_arg(text, len) } {
        Ok(text) => text,
        Err(e) => return e,
    };
    match s.session.clipboard_reply(selection, text) {
        Ok(sent) => {
            if let Some(out) = unsafe { out_sent.as_mut() } {
                *out = sent;
            }
            TT_OK
        }
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
///
/// How long it holds is `SendBreakTime`, and there is deliberately no
/// parameter for it: upstream has one break length and every caller reaches
/// it, so an argument here is one every frontend has to make up.
#[no_mangle]
pub extern "C" fn tt_session_send_break(session: *mut TtSession) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    match s.session.send_break() {
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

/// Handle Tera Term's Shift+Escape debug-mode shortcut.
///
/// False means debug cycling is disabled and the frontend should continue
/// treating Escape as an ordinary key.
#[no_mangle]
pub extern "C" fn tt_session_cycle_debug_mode(session: *mut TtSession) -> bool {
    let s = session!(session, false);
    s.session.cycle_debug_mode()
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
/// with. Upstream does this only when the port is the telnet port.
pub const TT_TELNET_NEGOTIATE: TtTelnetMode = 2;
/// Telnet from the first byte and nothing offered — `Telnet=on` at a port that
/// is not the telnet port, which is the ordinary state of a console server.
pub const TT_TELNET_FRAMED: TtTelnetMode = 3;

/// What to tell the far end about this terminal.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtTelnetParams {
    /// [`TT_TELNET_RAW`], [`TT_TELNET_AUTO`], [`TT_TELNET_NEGOTIATE`] or
    /// [`TT_TELNET_FRAMED`] — four, because the framing and the opening burst
    /// are two questions rather than one.
    ///
    /// [`tt_telnet_params_default`] sets it from the port, which is upstream's
    /// rule: negotiate on 23, **frame** elsewhere. Not auto-detect: the
    /// framing is on from the first byte at any port, and all that the port
    /// decides is whether the burst goes out. **Do not "improve" this to
    /// always negotiating** — a terminal server is not a telnet server, and
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
    /// `ts.TelEcho`, off by default. Whether the `ECHO` option decides local
    /// echo — in both directions: it changes what the burst asks for, and it
    /// makes the answer change the terminal.
    pub echo_negotiates: bool,
    /// Local echo as the terminal has it, read only when `echo_negotiates` is
    /// set. Ignored otherwise.
    pub local_echo: bool,
    /// `ts.TelKeepAliveInterval` in **seconds**, zero meaning none. An
    /// `IAC NOP` after this much quiet — and it is measured from the last thing
    /// sent, so a session being typed at sends none.
    pub keepalive_secs: u32,
    /// Where `TelLog` writes, or null for no log. Upstream's is `TELNET.LOG` in
    /// the log directory, and it holds only what this end sent.
    pub log_path: *const c_char,
}

/// Fill `out` with the defaults for `port`: upstream's mode rule, 38400 both
/// ways, no `BINARY`, a ten-second connect timeout.
#[no_mangle]
pub extern "C" fn tt_telnet_params_default(out: *mut TtTelnetParams, port: u16) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    *out = TtTelnetParams {
        mode: telnet_mode_c(TelnetMode::for_port(port)),
        term_type: ptr::null(),
        input_speed: 38400,
        output_speed: 38400,
        binary: false,
        connect_timeout_ms: 10_000,
        echo_negotiates: false,
        local_echo: false,
        keepalive_secs: 300,
        log_path: ptr::null(),
    };
}

fn telnet_mode_c(mode: TelnetMode) -> TtTelnetMode {
    match mode {
        TelnetMode::Raw => TT_TELNET_RAW,
        TelnetMode::Auto => TT_TELNET_AUTO,
        TelnetMode::Framed => TT_TELNET_FRAMED,
        TelnetMode::Negotiate => TT_TELNET_NEGOTIATE,
    }
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
            TT_TELNET_FRAMED => TelnetMode::Framed,
            _ => TelnetMode::Auto,
        },
        speed: (p.input_speed, p.output_speed),
        cols: s.session.grid().cols() as u16,
        rows: s.session.grid().rows() as u16,
        binary: p.binary,
        echo_negotiates: p.echo_negotiates,
        local_echo: p.local_echo,
        keepalive: match p.keepalive_secs {
            0 => None,
            n => Some(Duration::from_secs(n.into())),
        },
        ..TelnetParams::default()
    };
    if !p.term_type.is_null() {
        match unsafe { str_arg(p.term_type, usize::MAX) } {
            Ok(t) => rust.term_type = t.to_string(),
            Err(e) => return e,
        }
    }
    if !p.log_path.is_null() {
        match unsafe { str_arg(p.log_path, usize::MAX) } {
            Ok(l) => rust.log = Some(PathBuf::from(l)),
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

/// Drop the connection. Applies `AutoWinClose` and
/// `ClearScreenOnCloseConnection`, whose results are drained as ordinary
/// events. A no-op when there is no connection.
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

/// What kind of link is attached — upstream's `cv.PortType`, as far as a
/// frontend needs it.
pub type TtLinkKind = u32;

/// Nothing is connected.
pub const TT_LINK_NONE: TtLinkKind = 0;
/// A real serial port.
pub const TT_LINK_SERIAL: TtLinkKind = 1;
/// Telnet or SSH — `IdTCPIP`, which is what the settings that say "TCP" mean.
pub const TT_LINK_NETWORK: TtLinkKind = 2;
/// A local shell on a pty. Upstream reaches this through CygTerm, which is a
/// *telnet* session to a local process, so it has no equivalent.
pub const TT_LINK_LOCAL_PTY: TtLinkKind = 3;

/// Which of the four the current connection is.
///
/// Exists because several of upstream's settings are conditioned on
/// `cv.PortType` rather than on anything about the transport itself —
/// `ConfirmDisconnect` asks only for a TCP session (`vtwin.cpp:1668`),
/// `BeepOnConnect` sounds only for one (`:3018`) — and a frontend acting on
/// those has to be able to ask.
#[no_mangle]
pub extern "C" fn tt_session_link_kind(session: *const TtSession) -> TtLinkKind {
    let s = session_ref!(session, TT_LINK_NONE);
    match s.session.link_kind() {
        None => TT_LINK_NONE,
        Some(tt_conn::LinkKind::Serial { .. }) => TT_LINK_SERIAL,
        Some(tt_conn::LinkKind::Network) => TT_LINK_NETWORK,
        Some(tt_conn::LinkKind::LocalPty) => TT_LINK_LOCAL_PTY,
    }
}

/// The current serial speed, or zero when the attached link is not serial.
///
/// This is live transport state rather than the `serial.baud` setting: a
/// command-line open can override the file, and a macro's `setbaud` changes it
/// while the window is running.
#[no_mangle]
pub extern "C" fn tt_session_serial_baud(session: *const TtSession) -> u32 {
    session_ref!(session, 0).session.serial_baud().unwrap_or(0)
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

/// Nothing yet. Wait for the platform's descriptor or event to become ready.
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
    ssh_connect(params, None)
}

/// Start connecting with the proxy configured on `session`, if any.
///
/// TTSSH has no proxy settings of its own upstream: TTProxy hooks the socket
/// beneath it. The flat ABI keeps that relationship explicit, so a frontend
/// passes the destination session whose `[TTProxy]` settings are live.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_for_session(
    params: *const TtSshParams,
    session: *const TtSession,
) -> *mut TtSshConnect {
    let Some(session) = (unsafe { session.as_ref() }) else {
        fail(TT_ERR_INVALID, "null session");
        return ptr::null_mut();
    };
    ssh_connect(params, Some(session))
}

fn ssh_connect(params: *const TtSshParams, session: Option<&TtSession>) -> *mut TtSshConnect {
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
    if let Some(session) = session {
        rust.proxy = proxy_params(session.session.settings()).map(Box::new);
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
///
/// Returns `-1` on Windows, whose native spelling is
/// [`tt_ssh_connect_wait_handle`].
#[no_mangle]
pub extern "C" fn tt_ssh_connect_poll_fd(c: *const TtSshConnect) -> c_int {
    match unsafe { c.as_ref() } {
        #[cfg(unix)]
        Some(c) => c.inner.poll_fd(),
        #[cfg(not(unix))]
        Some(_) => -1,
        None => -1,
    }
}

/// The Windows event to wait on while an SSH connection is being set up.
///
/// **The same event [`tt_session_wait_handle`] returns once connected**, so a
/// frontend can keep its notifier across the handover. Borrowed until the
/// connection handle is freed or handed to the session. Returns null on Unix.
#[no_mangle]
pub extern "C" fn tt_ssh_connect_wait_handle(c: *const TtSshConnect) -> *mut std::ffi::c_void {
    match unsafe { c.as_ref() } {
        #[cfg(windows)]
        Some(c) => c.inner.wait_handle(),
        #[cfg(not(windows))]
        Some(_) => ptr::null_mut(),
        None => ptr::null_mut(),
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

// --- the command line -----------------------------------------------------

/// `ts.MacroFNW` after a command line has had its say — three states, not two.
pub type TtMacroArg = u32;

/// No `/M`, so whatever the settings file said stands.
pub const TT_MACRO_UNSET: TtMacroArg = 0;
/// A `/D=` topic was given, which **frees the name unconditionally**
/// (`ttset.c:3963`) — so a terminal launched by a macro does not also run the
/// startup macro from the settings file.
pub const TT_MACRO_CLEARED: TtMacroArg = 1;
/// `/M`, or `/M=` with nothing or a `*` after it: ask which macro to run.
pub const TT_MACRO_PROMPT: TtMacroArg = 2;
/// `/M=<name>`, which is in `TtCmdLineInfo::macro_file`.
pub const TT_MACRO_FILE: TtMacroArg = 3;

/// What [`tt_cmdline_startup`] decided — `OnCommStart` (`vtwin.cpp:3708`),
/// whose single `if` has two arms that open nothing.
pub type TtStartupKind = u32;

/// `CommOpen` — connect to the target in the same [`TtStartup`].
pub const TT_STARTUP_OPEN: TtStartupKind = 0;
/// `OnFileNewConnection` — nothing was named and `HostDialogOnStartup` is on,
/// so put the New Connection dialog up.
pub const TT_STARTUP_DIALOG: TtStartupKind = 1;
/// `SetDdeComReady(0)` — nothing was named and the dialog is suppressed, so
/// open a terminal with no connection. `/DS` is how a session that will
/// `connect` for itself starts up.
pub const TT_STARTUP_IDLE: TtStartupKind = 2;
/// The line named a transport this port does not have.
/// `TtStartup::reason` says which; upstream would have opened it, and saying
/// so beats opening something else.
pub const TT_STARTUP_UNSUPPORTED: TtStartupKind = 3;
/// The arguments could not be resolved into anything — a null handle, or a
/// `/C=` naming a serial port that is not there. [`tt_last_error`] says why.
pub const TT_STARTUP_ERROR: TtStartupKind = 4;

/// Which transport a [`TT_STARTUP_OPEN`] means.
pub type TtTargetKind = u32;

pub const TT_TARGET_SERIAL: TtTargetKind = 0;
pub const TT_TARGET_TELNET: TtTargetKind = 1;
pub const TT_TARGET_SSH: TtTargetKind = 2;
/// A local shell. **No Tera Term command line produces one** — upstream
/// launches `cyglaunch.exe` for that — and it is here so the conversion is
/// total rather than so it can be reached.
pub const TT_TARGET_SHELL: TtTargetKind = 3;

/// A parsed command line, both halves of it: TTSSH's hook and `_ParseParam`.
///
/// Opaque because it holds the parse rather than a copy of the arguments, and
/// because the strings it lends out have to outlive the call that produced
/// them. Free it with [`tt_cmdline_free`].
pub struct TtCmdLine {
    cmd: CommandLine,
    ssh: SshOptions,
    /// TTProxy's half, which has no accessor of its own: everything it holds
    /// is a setting, so [`tt_cmdline_apply`] is where it comes out and
    /// `tt_session_setting("proxy.type")` is where it is read back.
    proxy: ProxyOptions,
    /// Backing store for what [`tt_cmdline_info`] hands out. Built once at
    /// parse time, so those pointers are good for the handle's whole life.
    setup_file: Option<CString>,
    key_cnf_file: Option<CString>,
    log_file: Option<CString>,
    macro_file: Option<CString>,
    dde_topic: Option<CString>,
    unknown: Vec<CString>,
    /// Backing store for what the **last** [`tt_cmdline_startup`] handed out,
    /// and only that one: resolving again replaces these.
    resolved: Vec<CString>,
    /// The null-terminated array `TtSshParams::identities` points at, pointing
    /// in turn into `resolved`.
    identities: Vec<*const c_char>,
    argv: Vec<*const c_char>,
}

/// The options a window has to act on itself, none of which is a setting.
///
/// Everything that *is* a setting — the title (`/W=`), the hidden title bar
/// (`/H`), the port, the speed, the timeout — is applied by
/// [`tt_cmdline_apply`] and read back with [`tt_session_setting`], because
/// that is upstream's order too: `_ParseParam` writes into `ts` and everything
/// downstream reads `ts`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtCmdLineInfo {
    /// `/F=`, **as given**. Null when there was none.
    ///
    /// Resolving it is the frontend's, which knows where its own settings
    /// live. Upstream would prepend `ts.HomeDirW` to a relative name and
    /// append `.INI` to one with no dot in it; the extension half is dropped
    /// here on purpose, because on a case-sensitive filesystem `work.INI` is
    /// not the `work.ini` the user has.
    pub setup_file: *const c_char,
    /// `/K=`, as given. Null when there was none. A frontend resolves a
    /// relative name beside the active setup file and supplies `.CNF` when
    /// the file part has no extension, which is `GetFilePath`'s rule.
    pub key_cnf_file: *const c_char,
    /// `/L=`, as given, and **null when `/NOLOG` was there too**.
    ///
    /// That is the port's second documented divergence from upstream's code
    /// and its second agreement with upstream's manual: `ttset.c:3850` clears
    /// only the ANSI copy of the name while `vtwin.cpp:3631` starts logging
    /// from the wide one, so `ttermpro /L=out.log /NOLOG` writes `out.log` —
    /// the one thing the option exists to prevent.
    pub log_file: *const c_char,
    /// Which of [`TT_MACRO_UNSET`], [`TT_MACRO_CLEARED`], [`TT_MACRO_PROMPT`]
    /// and [`TT_MACRO_FILE`].
    pub macro_kind: TtMacroArg,
    /// The `/M=<name>`, for [`TT_MACRO_FILE`]. Null otherwise.
    pub macro_file: *const c_char,
    /// `/D=`'s topic, or null.
    ///
    /// Upstream this is the DDE conversation a window registers so that the
    /// `ttpmacro.exe` it launched can find it (`ttdde.c:208`, `:1497`). Here
    /// it names the window's **control socket**, which is the same job through
    /// the mechanism that replaced DDE — so a shortcut that said
    /// `ttermpro /D=A1B2C3D4` still ends up with a window a
    /// `ttpmacro /D=A1B2C3D4` can find. Twenty characters, which is
    /// `TopicName[21]` at both ends.
    pub dde_topic: *const c_char,
    /// `/I` — start minimised.
    pub minimize: bool,
    /// `/V` — no window at all, for a session driven entirely by a macro.
    pub hide_window: bool,
    /// `/X=` and `/Y=`, each given or not.
    ///
    /// Upstream pairs them: setting one puts the other at 0 **if it is still
    /// `CW_USEDEFAULT`** (`ttset.c:3917`), because a real coordinate in one
    /// axis and "wherever you like" in the other is not a position Windows
    /// will take. With no saved window position — which is this port, since
    /// the schema has no `VTPos` — that reduces to "the axis that was not
    /// given is 0".
    pub has_x: bool,
    pub has_y: bool,
    pub x: i32,
    pub y: i32,
    /// How many options beginning with `/ssh` matched nothing, for
    /// [`tt_cmdline_unknown`]. Upstream puts these in a message box, and it is
    /// the only diagnostic in either parser.
    pub unknown_count: usize,
}

/// Where to connect, in the terms the `tt_session_connect_*` family takes.
///
/// Every pointer is borrowed from the [`TtCmdLine`] and is valid until the
/// next [`tt_cmdline_startup`] on it, or until it is freed — the same contract
/// [`TtSshHostKeyPrompt`] has. Only the fields belonging to `target` are set;
/// the rest are null or zero.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtStartup {
    pub kind: TtStartupKind,
    /// Meaningful only when `kind` is [`TT_STARTUP_OPEN`].
    pub target: TtTargetKind,
    /// Why there is nothing to open, for [`TT_STARTUP_UNSUPPORTED`].
    pub reason: *const c_char,

    /// [`TT_TARGET_SERIAL`]: the device, resolved from `/C=<n>` through
    /// enumeration — the *n*th port, which is what a `COM<n>` in a shortcut
    /// converted from Windows means, rather than `/dev/ttyS<n-1>`.
    pub path: *const c_char,
    pub serial: TtSerialParams,

    /// [`TT_TARGET_TELNET`]: where to connect. For [`TT_TARGET_SSH`] the host
    /// and port are in `ssh` instead, where the connect call wants them.
    pub host: *const c_char,
    pub port: u16,
    pub telnet: TtTelnetParams,

    /// [`TT_TARGET_SSH`]: for [`tt_ssh_connect`], because an SSH connection
    /// has prompts and therefore belongs to whoever owns a window.
    ///
    /// `port` is 0 when the line asked for none, which means `~/.ssh/config`'s
    /// `Port` and then 22 — **not** the `TCPPort=` in the settings file, which
    /// on a fresh install is 23. That divergence is upstream's own bug: TTSSH
    /// never assigns `ts.TCPPort` (only its half of the New Connection dialog
    /// does, `ttxssh.c:1347`), so `ttermpro /ssh myhost` is an SSH client
    /// pointed at the telnet port.
    pub ssh: TtSshParams,
    /// `/passwd=`, or a URL's. Handed over rather than used: whether an
    /// automatic login may skip a prompt is the frontend's policy.
    pub password: *const c_char,
    /// `/ask4passwd` — ask even though a password was given.
    pub ask_password: bool,
    /// `/nosecuritywarning`, which is **already folded into
    /// `ssh.host_key_policy`** as [`TT_HOST_KEY_POLICY_ACCEPT_ANY`]. It is
    /// here as well so a frontend can say out loud that the `known_hosts`
    /// check was skipped, which upstream calls a hidden option for a reason.
    pub no_known_hosts_check: bool,

    /// [`TT_TARGET_SHELL`]: see [`TT_TARGET_SHELL`] for why nothing produces
    /// this yet.
    pub pty: TtPtyParams,
}

/// Parse a Tera Term command line — TTSSH's hook first, then `_ParseParam`
/// over what is left, which is how the two compose on Windows too.
///
/// `argv` is the arguments **without the program name**, as the platform split
/// them: `argv + 1` from `main`, after Qt has taken its own out. There is no
/// tokenising and no dequoting here, because the shell did both — running
/// upstream's tokeniser over a rejoined line would quote-process everything
/// twice and turn `/W="My Session"`, which arrives as one argument, into a
/// `/W=My` and a stray `Session`.
///
/// `max_com_port` bounds `/C=`, and out of range is **dropped rather than
/// clamped** — `/C=300` on a default setup selects the serial transport with
/// no port and puts the New Connection dialog up. Pass 0 for upstream's
/// default of 256. It is a setting (`MaxComPort=`), and a settings file cannot
/// be read until `/F=` has been, which is why upstream parses twice: parse
/// with 0, read `TtCmdLineInfo::setup_file`, load it, and parse again with
/// `serial.max_com_port` if it is not 256.
///
/// Null only if `argv` holds a null entry. Free with [`tt_cmdline_free`].
#[no_mangle]
pub extern "C" fn tt_cmdline_parse(
    argv: *const *const c_char,
    argc: usize,
    max_com_port: u16,
) -> *mut TtCmdLine {
    let mut args: Vec<Vec<u8>> = Vec::with_capacity(argc);
    if !argv.is_null() {
        for i in 0..argc {
            let entry = unsafe { *argv.add(i) };
            if entry.is_null() {
                fail(TT_ERR_INVALID, format!("argv[{i}] is null"));
                return ptr::null_mut();
            }
            // Bytes, not `str`: a command line carries paths, and a path is
            // not required to be UTF-8 on this platform.
            args.push(unsafe { CStr::from_ptr(entry) }.to_bytes().to_vec());
        }
    }

    let cmdline::Parsed { cmd, ssh, proxy } = cmdline::parse_all_args_with(
        &args,
        match max_com_port {
            0 => cmdline::DEFAULT_MAX_COM_PORT,
            n => n,
        },
    );

    Box::into_raw(Box::new(TtCmdLine {
        setup_file: cmd.setup_file.as_deref().map(cbytes),
        key_cnf_file: cmd.key_cnf_file.as_deref().map(cbytes),
        // `/NOLOG` wins over `/L=`, which is the manual's answer and not the
        // code's — see `TtCmdLineInfo::log_file`.
        log_file: cmd.log_file.as_deref().filter(|_| !cmd.no_log).map(cbytes),
        macro_file: match &cmd.macro_file {
            MacroArg::File(f) => Some(cbytes(f)),
            _ => None,
        },
        dde_topic: cmd.dde_topic.as_deref().map(cbytes),
        unknown: ssh.unknown.iter().map(|u| cbytes(u)).collect(),
        cmd,
        ssh,
        proxy,
        resolved: Vec::new(),
        identities: Vec::new(),
        argv: Vec::new(),
    }))
}

/// The same, over a whole command line rather than over arguments.
///
/// This is a macro's `connect` — an argument that *is* a command line, with no
/// program name in front of it. `ttdde.c:617` prepends a literal `"a "` for
/// that reason, since `_ParseParam` throws its first token away, and passes
/// **NULL** for the DDE topic: so a `/D=` inside one of these neither sets a
/// topic nor cancels the startup macro, which [`tt_cmdline_parse`] over the
/// process's own `argv` does do.
///
/// Both parsers again, TTSSH's first — `ssh://user@host/` is rewritten *into*
/// a bare `host:22` token before Tera Term's own parser sees it, which is the
/// only reason it can find a host in an SSH URL.
///
/// Free with [`tt_cmdline_free`]. Null only if `line` is null.
#[no_mangle]
pub extern "C" fn tt_cmdline_parse_line(line: *const c_char, max_com_port: u16) -> *mut TtCmdLine {
    if line.is_null() {
        fail(TT_ERR_INVALID, "null command line");
        return ptr::null_mut();
    }
    let arg = unsafe { CStr::from_ptr(line) }.to_bytes().to_vec();
    let cmdline::Parsed { cmd, ssh, proxy } = cmdline::parse_all_argument(
        &arg,
        match max_com_port {
            0 => cmdline::DEFAULT_MAX_COM_PORT,
            n => n,
        },
    );
    Box::into_raw(Box::new(TtCmdLine {
        setup_file: cmd.setup_file.as_deref().map(cbytes),
        key_cnf_file: cmd.key_cnf_file.as_deref().map(cbytes),
        log_file: cmd.log_file.as_deref().filter(|_| !cmd.no_log).map(cbytes),
        macro_file: match &cmd.macro_file {
            MacroArg::File(f) => Some(cbytes(f)),
            _ => None,
        },
        dde_topic: cmd.dde_topic.as_deref().map(cbytes),
        unknown: ssh.unknown.iter().map(|u| cbytes(u)).collect(),
        cmd,
        ssh,
        proxy,
        resolved: Vec::new(),
        identities: Vec::new(),
        argv: Vec::new(),
    }))
}

#[no_mangle]
pub extern "C" fn tt_cmdline_free(cmd: *mut TtCmdLine) {
    if !cmd.is_null() {
        drop(unsafe { Box::from_raw(cmd) });
    }
}

/// The options a window acts on itself. False on a null argument.
#[no_mangle]
pub extern "C" fn tt_cmdline_info(cmd: *const TtCmdLine, out: *mut TtCmdLineInfo) -> bool {
    let (Some(c), Some(out)) = (unsafe { cmd.as_ref() }, unsafe { out.as_mut() }) else {
        return false;
    };
    *out = TtCmdLineInfo {
        setup_file: cptr(&c.setup_file),
        key_cnf_file: cptr(&c.key_cnf_file),
        log_file: cptr(&c.log_file),
        macro_kind: match c.cmd.macro_file {
            MacroArg::Unset => TT_MACRO_UNSET,
            MacroArg::Cleared => TT_MACRO_CLEARED,
            MacroArg::Prompt => TT_MACRO_PROMPT,
            MacroArg::File(_) => TT_MACRO_FILE,
        },
        macro_file: cptr(&c.macro_file),
        dde_topic: cptr(&c.dde_topic),
        minimize: c.cmd.minimize,
        hide_window: c.cmd.hide_window,
        has_x: c.cmd.window_x.is_some(),
        has_y: c.cmd.window_y.is_some(),
        x: c.cmd.window_x.unwrap_or(0),
        y: c.cmd.window_y.unwrap_or(0),
        unknown_count: c.unknown.len(),
    };
    true
}

/// One unrecognised `/ssh…` option. Null when `index` is out of range; valid
/// until the command line is freed.
#[no_mangle]
pub extern "C" fn tt_cmdline_unknown(cmd: *const TtCmdLine, index: usize) -> *const c_char {
    match unsafe { cmd.as_ref() } {
        Some(c) => c.unknown.get(index).map_or(ptr::null(), |s| s.as_ptr()),
        None => ptr::null(),
    }
}

/// Write the command line into the session's settings — `_ParseParam`'s effect
/// on `ts`.
///
/// **Call this before [`tt_cmdline_startup`] and after loading the settings
/// file**, which is upstream's order: the line is applied over the file, and
/// everything downstream then reads one place. A setting the line did not
/// mention is left alone, so this is safe to call over settings a user has
/// already changed.
///
/// It applies to the running terminal too, exactly as
/// [`tt_session_settings_load`] does — so a `/W=` is in `terminal.title` and a
/// `/H` in `window.hide_title` immediately afterwards.
#[no_mangle]
pub extern "C" fn tt_cmdline_apply(cmd: *const TtCmdLine, session: *mut TtSession) -> TtStatus {
    let s = session!(session, TT_ERR_INVALID);
    let Some(c) = (unsafe { cmd.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtCmdLine");
    };
    let mut settings = s.session.settings().clone();
    c.cmd.apply(&mut settings);
    // The proxy plugin's half, which is a settings record of its own upstream
    // and is replaced entire — so a `/proxy=` naming no credentials clears the
    // file's, and `/noproxy` is a proxy of type `none`.
    c.proxy.apply(&mut settings);
    s.session.set_settings(settings);
    TT_OK
}

/// What to open — `OnCommStart`, over a command line that
/// [`tt_cmdline_apply`] has already been called with.
///
/// The answer is one of five, and the two that open nothing are the ones a
/// reimplementation drops: a TCP session is decided by whether there is a
/// **host name** rather than by the port type, and a serial one by
/// `ComAutoConnect`, which `/M` turns off and an in-range `/C=` turns back on
/// in either order. So `myhost /M=x` connects and `/C=1 /M=x` connects, while
/// `/M=x` alone opens the dialog — or nothing at all under `/DS`.
///
/// The terminal's size goes into the target, so call it on a session whose
/// window has settled: a serial console that opens at 80x24 and resizes is one
/// wrong prompt for the user to read.
///
/// The return is also written to `out->kind`, so a caller may ignore either.
#[no_mangle]
pub extern "C" fn tt_cmdline_startup(
    cmd: *mut TtCmdLine,
    session: *const TtSession,
    out: *mut TtStartup,
) -> TtStartupKind {
    // Zeroed and then filled by the same `_default` writers a C caller would
    // use. Sound rather than merely conventional: every field of the four is a
    // pointer, an integer, a `bool` or one of `tt-conn`'s three `repr(u8)`
    // serial enums, and each of those has a variant at discriminant zero.
    let mut answer: TtStartup = unsafe { std::mem::zeroed() };
    answer.kind = TT_STARTUP_ERROR;
    tt_serial_params_default(&mut answer.serial);
    tt_telnet_params_default(&mut answer.telnet, 0);
    tt_ssh_params_default(&mut answer.ssh);
    tt_pty_params_default(&mut answer.pty);

    let write = |answer: &TtStartup| {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = *answer;
        }
        answer.kind
    };

    let Some(c) = (unsafe { cmd.as_mut() }) else {
        fail(TT_ERR_INVALID, "null TtCmdLine");
        return write(&answer);
    };
    let Some(s) = (unsafe { session.as_ref() }) else {
        fail(TT_ERR_INVALID, "null TtSession");
        return write(&answer);
    };
    // The previous resolution's strings die here, which is the contract on
    // `TtStartup`: one live answer per command line.
    c.resolved.clear();
    c.identities.clear();
    c.argv.clear();

    let startup = Startup::of(
        &c.cmd,
        &c.ssh,
        s.session.settings(),
        s.session.grid().cols() as u16,
        s.session.grid().rows() as u16,
    );
    let target = match startup {
        Startup::Dialog => {
            answer.kind = TT_STARTUP_DIALOG;
            return write(&answer);
        }
        Startup::Idle => {
            answer.kind = TT_STARTUP_IDLE;
            return write(&answer);
        }
        Startup::Unsupported(why) => {
            answer.kind = TT_STARTUP_UNSUPPORTED;
            c.resolved.push(cstring(why));
            answer.reason = c.resolved[0].as_ptr();
            // Also in the error slot, so a caller that only checks
            // `tt_last_error` is not left guessing.
            fail(TT_ERR_UNSUPPORTED, why);
            return write(&answer);
        }
        Startup::Open(t) => t,
    };

    answer.kind = TT_STARTUP_OPEN;
    match target {
        Target::Serial { path, params } => {
            answer.target = TT_TARGET_SERIAL;
            c.resolved.push(cstring(&path));
            answer.path = c.resolved[0].as_ptr();
            answer.serial = serial_params_c(&params);
        }
        Target::Telnet {
            host,
            port,
            params,
            timeout,
        } => {
            answer.target = TT_TARGET_TELNET;
            c.resolved.push(cstring(&host));
            c.resolved.push(cstring(&params.term_type));
            answer.host = c.resolved[0].as_ptr();
            answer.port = port;
            let log = params
                .log
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            answer.telnet = TtTelnetParams {
                mode: telnet_mode_c(params.mode),
                term_type: c.resolved[1].as_ptr(),
                input_speed: params.speed.0,
                output_speed: params.speed.1,
                binary: params.binary,
                connect_timeout_ms: ms(timeout),
                echo_negotiates: params.echo_negotiates,
                local_echo: params.local_echo,
                keepalive_secs: params.keepalive.map_or(0, |d| d.as_secs() as u32),
                log_path: match log {
                    Some(l) => {
                        c.resolved.push(cstring(&l));
                        c.resolved[2].as_ptr()
                    }
                    None => ptr::null(),
                },
            };
        }
        Target::Ssh {
            params,
            port_chosen,
            password,
            method: _,
            ask_password,
            no_known_hosts_check,
        } => {
            answer.target = TT_TARGET_SSH;
            c.resolved.push(cstring(&params.host));
            answer.host = c.resolved[0].as_ptr();
            answer.ssh.host = c.resolved[0].as_ptr();
            // Zero means `~/.ssh/config`'s `Port`, then 22 — a better answer
            // than this crate's own fallback, and the reason `port_chosen`
            // exists rather than a bare port number.
            answer.ssh.port = match port_chosen {
                true => params.port,
                false => 0,
            };
            answer.port = params.port;
            // Null is not the empty string here: it means "whatever the
            // config says", which is what a line with no `/user=` wants.
            if !params.user.is_empty() {
                c.resolved.push(cstring(&params.user));
                answer.ssh.user = c.resolved.last().expect("just pushed").as_ptr();
            }
            for key in &params.identities {
                c.resolved.push(cbytes(key.to_string_lossy().as_bytes()));
                let p = c.resolved.last().expect("just pushed").as_ptr();
                c.identities.push(p);
            }
            if !c.identities.is_empty() {
                c.identities.push(ptr::null());
                answer.ssh.identities = c.identities.as_ptr();
            }
            answer.ssh.use_agent = params.use_agent;
            answer.ssh.legacy = params.legacy;
            answer.ssh.connect_timeout_ms = ms(params.connect_timeout);
            answer.ssh.keepalive_ms = params.keepalive.map_or(0, ms);
            if no_known_hosts_check {
                answer.ssh.host_key_policy = TT_HOST_KEY_POLICY_ACCEPT_ANY;
            }
            if let Some(pw) = password {
                c.resolved.push(cstring(&pw));
                answer.password = c.resolved.last().expect("just pushed").as_ptr();
            }
            answer.ask_password = ask_password;
            answer.no_known_hosts_check = no_known_hosts_check;
        }
        Target::Shell(params) => {
            answer.target = TT_TARGET_SHELL;
            for arg in &params.argv {
                c.resolved.push(cstring(arg));
                let p = c.resolved.last().expect("just pushed").as_ptr();
                c.argv.push(p);
            }
            answer.pty.argv = c.argv.as_ptr();
            answer.pty.argc = c.argv.len();
            answer.pty.login_shell = params.login_shell;
        }
    }
    write(&answer)
}

// --- macros ---------------------------------------------------------------

/// How a dialog ended.
///
/// Three answers rather than a bool because upstream distinguishes the close
/// box from Cancel, and a macro can test for it.
pub type TtDialogEnd = i32;
/// OK, or Yes.
pub const TT_DIALOG_OK: TtDialogEnd = 0;
/// Cancel, or No.
pub const TT_DIALOG_CANCEL: TtDialogEnd = 1;
/// The window's close box.
pub const TT_DIALOG_CLOSED: TtDialogEnd = 2;

/// `beep`'s argument.
pub type TtBeepSound = u32;
pub const TT_BEEP_SIMPLE: TtBeepSound = 0;
pub const TT_BEEP_ASTERISK: TtBeepSound = 1;
pub const TT_BEEP_EXCLAMATION: TtBeepSound = 2;
pub const TT_BEEP_CRITICAL_STOP: TtBeepSound = 3;
pub const TT_BEEP_QUESTION: TtBeepSound = 4;
/// What a bare `beep` plays.
pub const TT_BEEP_DEFAULT: TtBeepSound = 5;

/// `showtt`'s ten arms. The four TEK ones exist because the command has them;
/// a port with no TEK window refuses them, which is honest.
pub type TtShowWindow = u32;
pub const TT_SHOW_VT_HIDE: TtShowWindow = 0;
pub const TT_SHOW_VT_MINIMIZE: TtShowWindow = 1;
pub const TT_SHOW_VT_RESTORE: TtShowWindow = 2;
pub const TT_SHOW_TEK_HIDE: TtShowWindow = 3;
pub const TT_SHOW_TEK_MINIMIZE: TtShowWindow = 4;
pub const TT_SHOW_TEK_OPEN: TtShowWindow = 5;
pub const TT_SHOW_TEK_CLOSE: TtShowWindow = 6;
pub const TT_SHOW_LOG_HIDE: TtShowWindow = 7;
pub const TT_SHOW_LOG_MINIMIZE: TtShowWindow = 8;
pub const TT_SHOW_LOG_RESTORE: TtShowWindow = 9;

/// `show` — the macro's own control window, which this port does not draw.
pub type TtMacroWindow = u32;
pub const TT_MACRO_WINDOW_HIDE: TtMacroWindow = 0;
pub const TT_MACRO_WINDOW_MINIMIZE: TtMacroWindow = 1;
/// Anything positive, which also raises it.
pub const TT_MACRO_WINDOW_RESTORE: TtMacroWindow = 2;

/// `getttpos`'s first output. Upstream tests iconic, then zoomed, then
/// visible, so a minimised window reports [`TT_WINDOW_MINIMIZED`] whether or
/// not it is also maximised.
pub type TtWindowState = u32;
pub const TT_WINDOW_NORMAL: TtWindowState = 0;
pub const TT_WINDOW_MINIMIZED: TtWindowState = 1;
pub const TT_WINDOW_MAXIMIZED: TtWindowState = 2;
pub const TT_WINDOW_HIDDEN: TtWindowState = 3;

/// What a macro's `DispErr` dialog says.
///
/// Every string is borrowed and dies when the callback returns.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtMacroError {
    /// `ttmparse.h`'s number for it — 11 is a syntax error — or **0** for an
    /// error from a language upstream never numbered, which is every Lua one.
    pub code: u32,
    /// The sentence to show. For TTL that is upstream's, verbatim and spelling
    /// included; for Lua it is the traceback, which names its own position.
    pub message: *const c_char,
    /// The script the error is in. Not always the one that was launched:
    /// `include` opens another.
    pub file: *const c_char,
    /// The source line, whole. Empty when `code` is 0 — the message has it.
    pub line: *const c_char,
    /// Counting from 1, and 0 when there is no line to point at.
    pub line_no: usize,
    /// Byte offsets into `line` bounding what the interpreter was reading when
    /// it gave up. `start == end` is possible and means "at that point".
    pub start: usize,
    pub end: usize,
}

/// `listbox`'s keyword parameters (`ttl_gui.cpp:476`).
///
/// All of them are hints about the window rather than about the choice, so a
/// frontend that ignores the lot still implements `listbox` correctly.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtListBoxOpts {
    /// `dblclick=on` — a double click chooses the item under it.
    pub double_click: bool,
    /// `minmaxbutton=on`.
    pub min_max_button: bool,
    /// `minimize=on`. Exclusive with `maximized`: each keyword clears the
    /// other, so the last one written wins.
    pub minimized: bool,
    /// `maximize=on`.
    pub maximized: bool,
    /// `listboxsize=WxH`, in characters. **Both zero when the macro did not
    /// ask**, which is the only way this struct spells "absent".
    pub width: u32,
    pub height: u32,
}

/// Where `setdlgpos` wants the dialogs, in pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtDialogPos {
    /// The top-left corner, from the primary display's origin.
    pub x: i32,
    pub y: i32,
    /// The `<position>` argument as the macro wrote it: 1-5 anchors against
    /// the display and 6-10 against the terminal window, each in the order
    /// top-left, top-right, bottom-left, bottom-right, centre. **Zero is the
    /// two-argument form**, where the coordinates alone decide.
    pub position: i32,
    /// Added to the anchored position, and zero unless a `position` was given.
    pub offset_x: i32,
    pub offset_y: i32,
}

/// What `getttpos` reports (`ttdde.c:1136`) — the frame, then the text area
/// inside it, both in screen pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtWindowGeometry {
    pub state: TtWindowState,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub client_x: i32,
    pub client_y: i32,
    pub client_width: i32,
    pub client_height: i32,
}

/// The window a running macro asks things of.
///
/// **This is the one place the ABI calls back into C**, and the reason is the
/// mirror image of why SSH does not: `tt_ssh_connect_poll` refuses a callback
/// because it would fire on a worker thread, which is exactly where a Qt
/// frontend cannot raise a dialog. These fire from inside
/// [`tt_macro_service`], on the thread that called it, which is the frontend's
/// own — so a modal dialog spinning a nested event loop is an ordinary modal
/// dialog. The macro is blocked on another thread while it is up, which is the
/// whole point of putting it there.
///
/// **Zero-initialise it and fill in what you have.** A null function pointer
/// is not a crash and not a silent success: the command reports "Unknown
/// command" to the macro, exactly as though this port had never implemented
/// it. A frontend with three dialogs is useful.
///
/// Every `const char *` handed to a callback is borrowed and dies when it
/// returns; every one handed *back* is copied before the callback returns, so
/// a `static` buffer or a `QByteArray` that lives to the end of the function
/// is enough. All of them are UTF-8.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TtMacroUi {
    /// Passed back to every callback below and never touched here.
    pub user: *mut std::ffi::c_void,

    /// `DispErr` — the error dialog. **Returning true stops the macro**;
    /// upstream's two buttons are Stop and Continue, and Continue is the one
    /// that is not the default. A null pointer stops it, because a script that
    /// has hit a syntax error and cannot say so is better stopped than left
    /// running.
    pub error: Option<extern "C" fn(user: *mut std::ffi::c_void, err: *const TtMacroError) -> bool>,

    /// `messagebox` — one OK button.
    pub message_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            text: *const c_char,
            title: *const c_char,
        ) -> TtDialogEnd,
    >,
    /// `yesnobox`.
    pub yes_no_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            text: *const c_char,
            title: *const c_char,
        ) -> TtDialogEnd,
    >,
    /// `statusbox` — a *modeless* box the macro updates as it goes. Called
    /// again with one already up, it replaces the text rather than opening a
    /// second. Return [`TT_OK`] or a negative status.
    pub status_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            text: *const c_char,
            title: *const c_char,
        ) -> TtStatus,
    >,
    /// `closesbox`. Closing one that is not open is not an error.
    pub close_status_box: Option<extern "C" fn(user: *mut std::ffi::c_void) -> TtStatus>,
    /// `bringupbox` — raise it.
    pub bringup_status_box: Option<extern "C" fn(user: *mut std::ffi::c_void) -> TtStatus>,

    /// `listbox`. `items` is `count` strings; `selected` is the one to start
    /// on. Write the chosen index through `out_index` when returning
    /// [`TT_DIALOG_OK`].
    pub list_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            text: *const c_char,
            title: *const c_char,
            items: *const *const c_char,
            count: usize,
            selected: usize,
            opts: *const TtListBoxOpts,
            out_index: *mut usize,
        ) -> TtDialogEnd,
    >,
    /// `inputbox`, and `passwordbox` when `password` is set — which is the
    /// only difference between them and means "do not echo what is typed".
    /// Write the answer through `out_text` when returning [`TT_DIALOG_OK`]; a
    /// null there is an empty string.
    pub input_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            text: *const c_char,
            title: *const c_char,
            initial: *const c_char,
            password: bool,
            out_text: *mut *const c_char,
        ) -> TtDialogEnd,
    >,
    /// `filenamebox` — **null is cancelled**, which is why this returns a
    /// string rather than a [`TtDialogEnd`]. A frontend that cannot show one
    /// leaves the pointer null instead, and the macro is told so.
    pub filename_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            title: *const c_char,
            save: bool,
            init_dir: *const c_char,
        ) -> *const c_char,
    >,
    /// `dirnamebox`. Null is cancelled, as above.
    pub dirname_box: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            title: *const c_char,
            init_dir: *const c_char,
        ) -> *const c_char,
    >,
    /// `setdlgpos`. A null `pos` clears it back to wherever the frontend puts
    /// dialogs by default. A preference with no user in it, so it cannot fail.
    pub set_dialog_pos: Option<extern "C" fn(user: *mut std::ffi::c_void, pos: *const TtDialogPos)>,

    /// `beep`.
    pub beep: Option<extern "C" fn(user: *mut std::ffi::c_void, sound: TtBeepSound) -> TtStatus>,
    /// `callmenu` — invoke a menu item by `teraterm.rc`'s command id. There
    /// are about ninety; answer the ones this build has a menu item for and
    /// refuse the rest.
    pub call_menu: Option<extern "C" fn(user: *mut std::ffi::c_void, id: i32) -> TtStatus>,
    /// `showtt`.
    pub show_window:
        Option<extern "C" fn(user: *mut std::ffi::c_void, which: TtShowWindow) -> TtStatus>,
    /// `show` — the macro's own control window.
    pub show_macro_window:
        Option<extern "C" fn(user: *mut std::ffi::c_void, how: TtMacroWindow) -> TtStatus>,
    /// `getttpos`. Return false for "there is no window to measure", which the
    /// macro reads as a failure rather than as an unknown command.
    pub terminal_geometry:
        Option<extern "C" fn(user: *mut std::ffi::c_void, out: *mut TtWindowGeometry) -> bool>,
    /// `enablekeyb` — lock the keyboard so a script's prompts are not typed
    /// over.
    pub enable_keyboard: Option<extern "C" fn(user: *mut std::ffi::c_void, on: bool) -> TtStatus>,

    /// `clipb2var` — null when the clipboard holds no text.
    pub clipboard_text: Option<extern "C" fn(user: *mut std::ffi::c_void) -> *const c_char>,
    /// `var2clipb`. False is a failure the command reports.
    pub set_clipboard_text:
        Option<extern "C" fn(user: *mut std::ffi::c_void, text: *const c_char) -> bool>,
    /// `setexitcode` — what the process should exit with once the macro ends.
    /// [`tt_macro_exit_code`] reports the last one set, so a frontend that
    /// only wants it at the end can leave this null.
    pub set_exit_code: Option<extern "C" fn(user: *mut std::ffi::c_void, code: i32)>,
}

/// [`TtMacroUi`] on the Rust side of the seam.
///
/// Where a callback is null this falls back to [`NullUi`] rather than to a
/// hand-written refusal, so the "not implemented" behaviour is the trait's own
/// documented default and cannot drift from it.
struct CUi {
    vt: TtMacroUi,
    /// The last `setexitcode`, for [`tt_macro_exit_code`].
    exit_code: i32,
}

/// Borrow a `const char *` a callback handed back, as owned bytes. Null is
/// `None`; anything else is copied before the callback's storage can die.
unsafe fn taken(p: *const c_char) -> Option<Vec<u8>> {
    (!p.is_null()).then(|| CStr::from_ptr(p).to_bytes().to_vec())
}

impl MacroUi for CUi {
    fn error(&mut self, err: &MacroError) -> bool {
        let Some(f) = self.vt.error else {
            return MacroUi::error(&mut NullUi, err);
        };
        let message = cstring(&err.message);
        let file = cstring(&err.file);
        let line = cbytes(&err.line);
        let c = TtMacroError {
            code: err.code,
            message: message.as_ptr(),
            file: file.as_ptr(),
            line: line.as_ptr(),
            line_no: err.line_no,
            start: err.start,
            end: err.end,
        };
        f(self.vt.user, &c)
    }

    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        match self.vt.message_box {
            Some(f) => Ok(dialog_end(f(
                self.vt.user,
                cbytes(text).as_ptr(),
                cbytes(title).as_ptr(),
            ))),
            None => MacroUi::message_box(&mut NullUi, text, title),
        }
    }

    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        match self.vt.yes_no_box {
            Some(f) => Ok(dialog_end(f(
                self.vt.user,
                cbytes(text).as_ptr(),
                cbytes(title).as_ptr(),
            ))),
            None => MacroUi::yes_no_box(&mut NullUi, text, title),
        }
    }

    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        match self.vt.status_box {
            Some(f) => status(f(
                self.vt.user,
                cbytes(text).as_ptr(),
                cbytes(title).as_ptr(),
            )),
            None => MacroUi::status_box(&mut NullUi, text, title),
        }
    }

    fn close_status_box(&mut self) -> Result<(), TtlError> {
        match self.vt.close_status_box {
            Some(f) => status(f(self.vt.user)),
            None => MacroUi::close_status_box(&mut NullUi),
        }
    }

    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        match self.vt.bringup_status_box {
            Some(f) => status(f(self.vt.user)),
            None => MacroUi::bringup_status_box(&mut NullUi),
        }
    }

    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[Vec<u8>],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let Some(f) = self.vt.list_box else {
            return MacroUi::list_box(&mut NullUi, text, title, items, selected, opts);
        };
        let owned: Vec<CString> = items.iter().map(|i| cbytes(i)).collect();
        let ptrs: Vec<*const c_char> = owned.iter().map(|s| s.as_ptr()).collect();
        let (width, height) = opts.size.unwrap_or((0, 0));
        let c = TtListBoxOpts {
            double_click: opts.double_click,
            min_max_button: opts.min_max_button,
            minimized: opts.minimized,
            maximized: opts.maximized,
            width,
            height,
        };
        let mut index = selected;
        let end = f(
            self.vt.user,
            cbytes(text).as_ptr(),
            cbytes(title).as_ptr(),
            ptrs.as_ptr(),
            ptrs.len(),
            selected,
            &c,
            &mut index,
        );
        Ok(match dialog_end(end) {
            // A frontend that answers with an index it was never offered would
            // otherwise index somebody's array with it.
            DialogEnd::Ok(()) => DialogEnd::Ok(index.min(items.len().saturating_sub(1))),
            DialogEnd::Cancel => DialogEnd::Cancel,
            DialogEnd::Closed => DialogEnd::Closed,
        })
    }

    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let Some(f) = self.vt.input_box else {
            return MacroUi::input_box(&mut NullUi, text, title, default, password);
        };
        let mut answer: *const c_char = ptr::null();
        let end = f(
            self.vt.user,
            cbytes(text).as_ptr(),
            cbytes(title).as_ptr(),
            cbytes(default).as_ptr(),
            password,
            &mut answer,
        );
        Ok(match dialog_end(end) {
            DialogEnd::Ok(()) => DialogEnd::Ok(unsafe { taken(answer) }.unwrap_or_default()),
            DialogEnd::Cancel => DialogEnd::Cancel,
            DialogEnd::Closed => DialogEnd::Closed,
        })
    }

    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        match self.vt.filename_box {
            Some(f) => Ok(unsafe {
                taken(f(
                    self.vt.user,
                    cbytes(title).as_ptr(),
                    save,
                    cbytes(init_dir).as_ptr(),
                ))
            }),
            None => MacroUi::filename_box(&mut NullUi, title, save, init_dir),
        }
    }

    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        match self.vt.dirname_box {
            Some(f) => Ok(unsafe {
                taken(f(
                    self.vt.user,
                    cbytes(title).as_ptr(),
                    cbytes(init_dir).as_ptr(),
                ))
            }),
            None => MacroUi::dirname_box(&mut NullUi, title, init_dir),
        }
    }

    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        let Some(f) = self.vt.set_dialog_pos else {
            return MacroUi::set_dialog_pos(&mut NullUi, pos);
        };
        match pos {
            Some(p) => {
                let c = TtDialogPos {
                    x: p.x,
                    y: p.y,
                    position: p.anchor.map_or(0, |(a, o)| anchor_code(a, o)),
                    offset_x: p.offset_x,
                    offset_y: p.offset_y,
                };
                f(self.vt.user, &c)
            }
            None => f(self.vt.user, ptr::null()),
        }
    }

    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        match self.vt.beep {
            Some(f) => status(f(
                self.vt.user,
                match sound {
                    BeepSound::Simple => TT_BEEP_SIMPLE,
                    BeepSound::Asterisk => TT_BEEP_ASTERISK,
                    BeepSound::Exclamation => TT_BEEP_EXCLAMATION,
                    BeepSound::CriticalStop => TT_BEEP_CRITICAL_STOP,
                    BeepSound::Question => TT_BEEP_QUESTION,
                    BeepSound::Default => TT_BEEP_DEFAULT,
                },
            )),
            None => MacroUi::beep(&mut NullUi, sound),
        }
    }

    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        match self.vt.call_menu {
            Some(f) => status(f(self.vt.user, id)),
            None => MacroUi::call_menu(&mut NullUi, id),
        }
    }

    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        match self.vt.show_window {
            Some(f) => status(f(
                self.vt.user,
                match which {
                    ShowWindow::VtHide => TT_SHOW_VT_HIDE,
                    ShowWindow::VtMinimize => TT_SHOW_VT_MINIMIZE,
                    ShowWindow::VtRestore => TT_SHOW_VT_RESTORE,
                    ShowWindow::TekHide => TT_SHOW_TEK_HIDE,
                    ShowWindow::TekMinimize => TT_SHOW_TEK_MINIMIZE,
                    ShowWindow::TekOpen => TT_SHOW_TEK_OPEN,
                    ShowWindow::TekClose => TT_SHOW_TEK_CLOSE,
                    ShowWindow::LogHide => TT_SHOW_LOG_HIDE,
                    ShowWindow::LogMinimize => TT_SHOW_LOG_MINIMIZE,
                    ShowWindow::LogRestore => TT_SHOW_LOG_RESTORE,
                },
            )),
            None => MacroUi::show_window(&mut NullUi, which),
        }
    }

    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        match self.vt.show_macro_window {
            Some(f) => status(f(
                self.vt.user,
                match how {
                    MacroWindow::Hide => TT_MACRO_WINDOW_HIDE,
                    MacroWindow::Minimize => TT_MACRO_WINDOW_MINIMIZE,
                    MacroWindow::Restore => TT_MACRO_WINDOW_RESTORE,
                },
            )),
            None => MacroUi::show_macro_window(&mut NullUi, how),
        }
    }

    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        let Some(f) = self.vt.terminal_geometry else {
            return MacroUi::terminal_geometry(&mut NullUi);
        };
        let mut g: TtWindowGeometry = unsafe { std::mem::zeroed() };
        if !f(self.vt.user, &mut g) {
            return Ok(None);
        }
        Ok(Some(WindowGeometry {
            state: match g.state {
                TT_WINDOW_MINIMIZED => WindowState::Minimized,
                TT_WINDOW_MAXIMIZED => WindowState::Maximized,
                TT_WINDOW_HIDDEN => WindowState::Hidden,
                _ => WindowState::Normal,
            },
            window: (g.x, g.y, g.width, g.height),
            client: (g.client_x, g.client_y, g.client_width, g.client_height),
        }))
    }

    fn enable_keyboard(&mut self, on: bool) -> Result<(), TtlError> {
        match self.vt.enable_keyboard {
            Some(f) => status(f(self.vt.user, on)),
            None => MacroUi::enable_keyboard(&mut NullUi, on),
        }
    }

    fn clipboard_text(&mut self) -> Option<Vec<u8>> {
        match self.vt.clipboard_text {
            Some(f) => unsafe { taken(f(self.vt.user)) },
            None => MacroUi::clipboard_text(&mut NullUi),
        }
    }

    fn set_clipboard_text(&mut self, text: &[u8]) -> bool {
        match self.vt.set_clipboard_text {
            Some(f) => f(self.vt.user, cbytes(text).as_ptr()),
            None => MacroUi::set_clipboard_text(&mut NullUi, text),
        }
    }

    fn set_exit_code(&mut self, code: i32) {
        self.exit_code = code;
        if let Some(f) = self.vt.set_exit_code {
            f(self.vt.user, code);
        }
    }
}

/// `setdlgpos`'s `<position>`, rebuilt from what the interpreter parsed out of
/// it — 1-5 against the display and 6-10 against the terminal window.
fn anchor_code(anchor: DialogAnchor, origin: DialogOrigin) -> i32 {
    let base = match anchor {
        DialogAnchor::TopLeft => 1,
        DialogAnchor::TopRight => 2,
        DialogAnchor::BottomLeft => 3,
        DialogAnchor::BottomRight => 4,
        DialogAnchor::Center => 5,
    };
    match origin {
        DialogOrigin::Display => base,
        DialogOrigin::VtWindow => base + 5,
    }
}

fn dialog_end(v: TtDialogEnd) -> DialogEnd {
    match v {
        TT_DIALOG_CANCEL => DialogEnd::Cancel,
        TT_DIALOG_CLOSED => DialogEnd::Closed,
        _ => DialogEnd::Ok(()),
    }
}

/// A callback's [`TtStatus`] as the answer a command wants. Anything negative
/// is the command refusing; the macro is told "Unknown command", which is the
/// only refusal the language has.
fn status(v: TtStatus) -> Result<(), TtlError> {
    match v < 0 {
        true => Err(TtlError::NotSupported),
        false => Ok(()),
    }
}

/// A macro running against a session, on a thread of its own.
///
/// Free it with [`tt_macro_free`] whether or not it has finished.
pub struct TtMacro {
    rx: MacroReceiver,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Set by the macro's thread as its last act but one, before the wakeup
    /// below. See [`tt_macro_start`] for why this is not `JoinHandle::is_finished`.
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ui: CUi,
}

/// One Lua script, run to the end, with its error reported the way TTL's is.
///
/// Two things differ from the interpreter's loop and both come from the
/// language rather than from here. A Lua error cannot be continued past, so
/// the dialog's Stop/Continue answer is read and discarded — the script has
/// already stopped either way. And a script the user ended is **not** an
/// error: it arrives as one because raising is the only way out of a Lua
/// chunk, and putting a dialog up because somebody pressed End would be the
/// wrong end of that.
fn run_lua(path: &std::path::Path, body: Vec<u8>, cmd: &CmdLine, host: &mut SessionHost) {
    let script = tt_lua::Script::new(path.display().to_string(), body).with_args(cmd.args.clone());
    let Err(e) = script.run(host) else { return };
    if tt_lua::is_cancelled(&e) {
        return;
    }
    let err = MacroError::elsewhere(e.to_string(), path.display().to_string());
    let _ = host.report(&err);
}

/// Start `args`' macro against `session`, on a new thread.
///
/// `args` is `ttpmacro`'s command line **already split** — a null-terminated
/// array, without the program name. The first word that is not a switch names
/// the file, `.TTL` is fitted onto it if it has no extension, and everything
/// after it reaches the macro as `param2`..`param9` and `params[]`. So the
/// simplest call is a one-element array holding a path.
///
/// **The extension picks the language.** A name ending in `.lua` is run by
/// `tt-lua` and anything else by `tt-ttl`, which includes every extensionless
/// name — `FitTTLFileName` has already made those `.TTL`. One entry point
/// rather than two because the caller is answering "run this script", and a
/// frontend that had to know which language a file was in would be asking the
/// user a question the file already answers.
///
/// Returns null if there is no macro named, if the file cannot be read, or if
/// the thread or its pipe could not be created; [`tt_last_error`] says which.
/// **A `/M` that named nothing is not this function's to report** — the
/// command line already said so as `TT_MACRO_ASK`, and the answer to it is a
/// file dialog.
///
/// `ui` may be null, which is every dialog refused. It is **copied**, so the
/// struct itself need not outlive this call; the `user` pointer inside it must
/// outlive the macro.
///
/// The session is linked to the macro here — upstream's `DDELog = TRUE` — and
/// anything a previous macro had collected is thrown away. Starting a second
/// macro takes the terminal from the first, which is upstream's rule as well.
#[no_mangle]
pub extern "C" fn tt_macro_start(
    session: *mut TtSession,
    args: *const *const c_char,
    ui: *const TtMacroUi,
) -> *mut TtMacro {
    let s = session!(session, ptr::null_mut());
    let argv = match unsafe { byte_array(args) } {
        Some(a) => a,
        None => {
            fail(TT_ERR_INVALID, "null argument array");
            return ptr::null_mut();
        }
    };
    let cmd = CmdLine::from_args(argv);
    if cmd.needs_prompt() {
        fail(TT_ERR_INVALID, "no macro file named");
        return ptr::null_mut();
    }
    let path = PathBuf::from(String::from_utf8_lossy(&cmd.fitted_file_name()).into_owned());
    let body = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            fail(TT_ERR_IO, format!("{}: {e}", path.display()));
            return ptr::null_mut();
        }
    };

    let (tx, rx) = match tt_macro::channel() {
        Ok(pair) => pair,
        Err(e) => {
            fail(TT_ERR_IO, format!("cannot start a macro: {e}"));
            return ptr::null_mut();
        }
    };
    let lua = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lua"));
    let link = s.session.link_macro();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let thread = std::thread::Builder::new()
        .name(if lua { "lua" } else { "ttl" }.into())
        .spawn(move || {
            let mut host = SessionHost::new(tx.clone(), link);
            if lua {
                run_lua(&path, body, &cmd, &mut host);
            } else {
                let mut it = Interp::with_cmdline(&cmd, body, &mut host);
                it.run(&mut host);
            }
            // A macro that ends without asking for anything would otherwise
            // never be noticed: the frontend only looks when its descriptor
            // wakes it, and the last thing a script does is usually a `sendln`
            // whose job it has already serviced. So the thread knocks once on
            // its way out.
            //
            // The flag is set *first*, and it is what [`tt_macro_running`]
            // reads. `JoinHandle::is_finished` is false until the closure has
            // actually returned — which is after this line — so a frontend
            // servicing the wakeup would find the macro still running and go
            // back to waiting for a knock that has already happened.
            flag.store(true, std::sync::atomic::Ordering::Release);
            let _ = tx.post(Box::new(|_, _| {}));
        });
    let thread = match thread {
        Ok(t) => t,
        Err(e) => {
            // The link is already on; take it back off rather than leave the
            // terminal collecting for a macro that never started.
            s.session.unlink_macro();
            fail(TT_ERR_IO, format!("cannot start a macro: {e}"));
            return ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(TtMacro {
        rx,
        thread: Some(thread),
        done,
        ui: CUi {
            vt: match unsafe { ui.as_ref() } {
                Some(vt) => *vt,
                None => unsafe { std::mem::zeroed() },
            },
            exit_code: 0,
        },
    }))
}

/// A null-terminated array of `const char *` as owned bytes.
///
/// Null-terminated rather than pointer-plus-count for the same reason
/// [`path_array`] is, and bytes rather than `&str` because a macro's arguments
/// are paths: a filename that is not UTF-8 is still a filename.
///
/// # Safety
/// `array`, if non-null, must be a null-terminated array of NUL-terminated
/// strings.
unsafe fn byte_array(array: *const *const c_char) -> Option<Vec<Vec<u8>>> {
    if array.is_null() {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 0isize;
    loop {
        let entry = *array.offset(i);
        if entry.is_null() {
            return Some(out);
        }
        out.push(CStr::from_ptr(entry).to_bytes().to_vec());
        i += 1;
    }
}

/// A descriptor that becomes readable when the macro wants something.
///
/// The same bargain as [`tt_session_poll_fd`]: wait on it, and call
/// [`tt_macro_service`] when it fires. A quiet macro — one in a `wait`, which
/// polls a ring this side fills — costs nothing, so there is no timer to run.
/// **Both descriptors want watching**; they are not the same one. Returns
/// `-1` on Windows, whose native spelling is [`tt_macro_wait_handle`].
#[no_mangle]
pub extern "C" fn tt_macro_poll_fd(m: *const TtMacro) -> c_int {
    match unsafe { m.as_ref() } {
        #[cfg(unix)]
        Some(m) => m.rx.poll_fd(),
        #[cfg(not(unix))]
        Some(_) => -1,
        None => -1,
    }
}

/// The Windows event that becomes signalled when the macro wants something.
///
/// The native spelling of [`tt_macro_poll_fd`]. It is borrowed until
/// [`tt_macro_free`], must not be closed by the frontend, and is null on Unix.
#[no_mangle]
pub extern "C" fn tt_macro_wait_handle(m: *const TtMacro) -> *mut std::ffi::c_void {
    match unsafe { m.as_ref() } {
        #[cfg(windows)]
        Some(m) => m.rx.wait_handle(),
        #[cfg(not(windows))]
        Some(_) => ptr::null_mut(),
        None => ptr::null_mut(),
    }
}

/// Run whatever the macro is waiting on, against `session`. Returns how many
/// jobs ran; never blocks on the macro.
///
/// **This is where the [`TtMacroUi`] callbacks fire**, so it can take as long
/// as the user does — a `messagebox` is a job like any other, and the event
/// loop it spins is the frontend's own.
///
/// `session` must be the one [`tt_macro_start`] was given. It is not stored
/// between calls, which is why it is passed again rather than remembered.
#[no_mangle]
pub extern "C" fn tt_macro_service(m: *mut TtMacro, session: *mut TtSession) -> usize {
    let Some(m) = (unsafe { m.as_mut() }) else {
        set_error("null TtMacro");
        return 0;
    };
    let s = session!(session, 0);
    m.rx.service(&mut s.session, &mut m.ui)
}

/// Whether the macro is still running.
///
/// **Service it before believing a false**: a macro that has just ended may
/// have left a last job — the error dialog it stopped at, for one — which is
/// only run by [`tt_macro_service`]. And the end of a macro always produces one
/// wakeup of its own, so asking this after every service is enough; there is
/// nothing to poll for.
#[no_mangle]
pub extern "C" fn tt_macro_running(m: *const TtMacro) -> bool {
    match unsafe { m.as_ref() } {
        Some(m) => !m.done.load(std::sync::atomic::Ordering::Acquire),
        None => false,
    }
}

/// Ask it to stop — the End button on upstream's macro control window, and
/// what closing the window should do.
///
/// It stops at the next line rather than immediately, and a `pause 3600` ends
/// in milliseconds rather than in an hour. It does not by itself end a dialog
/// that is already up; that is the frontend's own.
#[no_mangle]
pub extern "C" fn tt_macro_cancel(m: *mut TtMacro) {
    if let Some(m) = unsafe { m.as_mut() } {
        m.rx.cancel();
    }
}

/// The last `setexitcode`, and zero if the macro never set one.
#[no_mangle]
pub extern "C" fn tt_macro_exit_code(m: *const TtMacro) -> i32 {
    match unsafe { m.as_ref() } {
        Some(m) => m.ui.exit_code,
        None => 0,
    }
}

/// Detach whatever macro is linked — upstream's `DDELog = FALSE`, and what
/// closing a script's window should do.
///
/// [`tt_macro_free`] cannot do this on its own: it is not given a session,
/// deliberately, so that nothing here holds a `TtSession *` it does not own.
/// Without it the terminal goes on collecting every character it prints into a
/// ring nobody is reading — bounded, since the ring drops its oldest bytes, but
/// paid for on every line of output for the rest of the session.
#[no_mangle]
pub extern "C" fn tt_session_unlink_macro(session: *mut TtSession) {
    let s = session!(session);
    s.session.unlink_macro();
}

/// Stop it and wait for the thread, then free the handle.
///
/// Safe to call on a macro that is still running: it is cancelled first, and
/// the channel is dropped before the join so that anything blocked waiting for
/// this frontend is released rather than deadlocked against a join that would
/// never return.
#[no_mangle]
pub extern "C" fn tt_macro_free(m: *mut TtMacro) {
    if m.is_null() {
        return;
    }
    let mut m = unsafe { Box::from_raw(m) };
    m.rx.cancel();
    let thread = m.thread.take();
    // The order matters: dropping the receiver turns a macro blocked on an
    // answer from this side into a macro whose frontend has gone, which is the
    // one thing it treats as the end of the run.
    drop(m);
    if let Some(t) = thread {
        let _ = t.join();
    }
}

// --- Lua plugins ----------------------------------------------------------

/// What kind of window action a Lua plugin declared.
pub type TtPluginActionKind = u32;
/// A visible item in a slash-separated menu path.
pub const TT_PLUGIN_ACTION_MENU: TtPluginActionKind = 0;
/// A global shortcut with no menu item of its own.
pub const TT_PLUGIN_ACTION_KEY: TtPluginActionKind = 1;

/// A session lifecycle edge delivered to Lua plugins.
pub type TtPluginHook = u32;
pub const TT_PLUGIN_HOOK_CONNECT: TtPluginHook = 0;
pub const TT_PLUGIN_HOOK_DISCONNECT: TtPluginHook = 1;

/// One action declared while loading a Lua plugin.
///
/// All strings are UTF-8, borrowed from the [`TtPlugins`] handle and valid
/// until [`tt_plugins_free`]. `menu` and `label` are null for a key binding;
/// `shortcut` is null only when a menu item did not declare one. `id` is the
/// value to pass to [`tt_plugins_invoke`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TtPluginAction {
    pub id: usize,
    pub kind: TtPluginActionKind,
    pub plugin: *const c_char,
    pub menu: *const c_char,
    pub label: *const c_char,
    pub shortcut: *const c_char,
}

struct PluginAction {
    plugin_index: usize,
    callback: tt_lua::CallbackId,
    kind: TtPluginActionKind,
    plugin: CString,
    menu: Option<CString>,
    label: Option<CString>,
    shortcut: Option<CString>,
}

enum PluginCommand {
    Invoke {
        plugin: usize,
        callback: tt_lua::CallbackId,
    },
    Hook(tt_lua::Hook),
    Stop,
}

/// The loaded plugin directory and its one worker.
///
/// The worker owns every Lua VM so callbacks can block without blocking the
/// frontend. Host calls come back through `rx`, on the same event-loop seam as
/// a macro; actions are copied out because their labels are needed before any
/// callback runs.
pub struct TtPlugins {
    commands: Option<std::sync::mpsc::Sender<PluginCommand>>,
    rx: Option<MacroReceiver>,
    thread: Option<std::thread::JoinHandle<()>>,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ui: CUi,
    actions: Vec<PluginAction>,
    hooks: [bool; 2],
}

fn run_plugins(
    plugins: Vec<tt_lua::Plugin>,
    commands: std::sync::mpsc::Receiver<PluginCommand>,
    notify: tt_macro::MacroSender,
    mut host: SessionHost,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    while let Ok(command) = commands.recv() {
        match command {
            PluginCommand::Invoke { plugin, callback } => {
                if let Some(plugin) = plugins.get(plugin) {
                    if let Err(error) = plugin.invoke(callback, &mut host) {
                        if !tt_lua::is_cancelled(&error) {
                            let report =
                                MacroError::elsewhere(error.to_string(), plugin.name().to_string());
                            let _ = host.report(&report);
                        }
                    }
                }
            }
            PluginCommand::Hook(hook) => {
                for plugin in &plugins {
                    if let Err(error) = plugin.emit(hook, &mut host) {
                        if !tt_lua::is_cancelled(&error) {
                            let report =
                                MacroError::elsewhere(error.to_string(), plugin.name().to_string());
                            let _ = host.report(&report);
                        }
                    }
                }
            }
            PluginCommand::Stop => break,
        }
        busy.store(false, std::sync::atomic::Ordering::Release);
        // A callback which touched no host still needs to wake the frontend so
        // it can observe that the action finished and re-enable its UI.
        let _ = notify.post(Box::new(|_, _| {}));
    }
}

fn plugin_paths(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Load every `.lua` file directly inside `dir`, in filename order.
///
/// A missing directory is a successful empty plugin set. A bad file rejects
/// the whole set and [`tt_last_error`] names it, so the window never presents a
/// half-loaded menu. Top-level chunks run during this call to declare actions
/// and hooks; callbacks run later on one worker and reach the frontend only
/// through [`tt_plugins_service`].
///
/// `ui` has the same lifetime and callback rules as [`tt_macro_start`] and is
/// copied. The plugin receive tap is independent of a running macro's.
#[no_mangle]
pub extern "C" fn tt_plugins_load(
    session: *mut TtSession,
    dir: *const c_char,
    ui: *const TtMacroUi,
) -> *mut TtPlugins {
    let s = session!(session, ptr::null_mut());
    if dir.is_null() {
        fail(TT_ERR_INVALID, "null plugin directory");
        return ptr::null_mut();
    }
    let dir = PathBuf::from(
        String::from_utf8_lossy(unsafe { CStr::from_ptr(dir) }.to_bytes()).into_owned(),
    );
    let paths = match plugin_paths(&dir) {
        Ok(paths) => paths,
        Err(error) => {
            fail(TT_ERR_IO, format!("{}: {error}", dir.display()));
            return ptr::null_mut();
        }
    };

    let mut plugins = Vec::with_capacity(paths.len());
    for path in paths {
        let body = match std::fs::read(&path) {
            Ok(body) => body,
            Err(error) => {
                fail(TT_ERR_IO, format!("{}: {error}", path.display()));
                return ptr::null_mut();
            }
        };
        match tt_lua::Plugin::load(path.display().to_string(), body) {
            Ok(plugin) => plugins.push(plugin),
            Err(error) => {
                fail(TT_ERR_INVALID, format!("{}: {error}", path.display()));
                return ptr::null_mut();
            }
        }
    }

    let mut actions = Vec::new();
    let mut hooks = [false; 2];
    for (plugin_index, plugin) in plugins.iter().enumerate() {
        let name = Path::new(plugin.name())
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        for menu in plugin.menus() {
            actions.push(PluginAction {
                plugin_index,
                callback: menu.callback,
                kind: TT_PLUGIN_ACTION_MENU,
                plugin: cstring(&name),
                menu: Some(cstring(&menu.menu)),
                label: Some(cstring(&menu.label)),
                shortcut: menu.shortcut.as_deref().map(cstring),
            });
        }
        for key in plugin.keys() {
            actions.push(PluginAction {
                plugin_index,
                callback: key.callback,
                kind: TT_PLUGIN_ACTION_KEY,
                plugin: cstring(&name),
                menu: None,
                label: None,
                shortcut: Some(cstring(&key.sequence)),
            });
        }
        hooks[0] |= plugin.has_hook(tt_lua::Hook::Connect);
        hooks[1] |= plugin.has_hook(tt_lua::Hook::Disconnect);
    }

    let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handle = TtPlugins {
        commands: None,
        rx: None,
        thread: None,
        busy: busy.clone(),
        ui: CUi {
            vt: match unsafe { ui.as_ref() } {
                Some(vt) => *vt,
                None => unsafe { std::mem::zeroed() },
            },
            exit_code: 0,
        },
        actions,
        hooks,
    };

    if !plugins.is_empty() {
        let (notify, rx) = match tt_macro::channel() {
            Ok(pair) => pair,
            Err(error) => {
                fail(TT_ERR_IO, format!("cannot start Lua plugins: {error}"));
                return ptr::null_mut();
            }
        };
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let link = s.session.link_plugin();
        let host = SessionHost::new_plugin(notify.clone(), link);
        let thread = std::thread::Builder::new()
            .name("lua-plugins".into())
            .spawn(move || run_plugins(plugins, command_rx, notify, host, busy));
        match thread {
            Ok(thread) => {
                handle.commands = Some(command_tx);
                handle.rx = Some(rx);
                handle.thread = Some(thread);
            }
            Err(error) => {
                s.session.unlink_plugin();
                fail(TT_ERR_IO, format!("cannot start Lua plugins: {error}"));
                return ptr::null_mut();
            }
        }
    }

    Box::into_raw(Box::new(handle))
}

/// How many menu and key actions the loaded plugins declared.
#[no_mangle]
pub extern "C" fn tt_plugins_action_count(plugins: *const TtPlugins) -> usize {
    unsafe { plugins.as_ref() }.map_or(0, |plugins| plugins.actions.len())
}

/// Read one action by index. False for nulls or an out-of-range index.
#[no_mangle]
pub extern "C" fn tt_plugins_action(
    plugins: *const TtPlugins,
    index: usize,
    out: *mut TtPluginAction,
) -> bool {
    let Some(plugins) = (unsafe { plugins.as_ref() }) else {
        set_error("null TtPlugins");
        return false;
    };
    let Some(action) = plugins.actions.get(index) else {
        set_error("plugin action index out of range");
        return false;
    };
    let Some(out) = (unsafe { out.as_mut() }) else {
        set_error("null TtPluginAction");
        return false;
    };
    *out = TtPluginAction {
        id: index,
        kind: action.kind,
        plugin: action.plugin.as_ptr(),
        menu: action
            .menu
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr()),
        label: action
            .label
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr()),
        shortcut: action
            .shortcut
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr()),
    };
    true
}

fn start_plugin_command(plugins: &TtPlugins, command: PluginCommand) -> TtStatus {
    let Some(commands) = &plugins.commands else {
        return TT_OK;
    };
    if plugins
        .busy
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return fail(TT_ERR_BUSY, "a Lua plugin callback is already running");
    }
    if commands.send(command).is_err() {
        plugins
            .busy
            .store(false, std::sync::atomic::Ordering::Release);
        return fail(TT_ERR_IO, "the Lua plugin worker has stopped");
    }
    TT_OK
}

/// Start one declared action. The callback runs asynchronously; wait on the
/// plugin descriptor and call [`tt_plugins_service`] just as for a macro.
#[no_mangle]
pub extern "C" fn tt_plugins_invoke(plugins: *mut TtPlugins, id: usize) -> TtStatus {
    let Some(plugins) = (unsafe { plugins.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtPlugins");
    };
    let Some(action) = plugins.actions.get(id) else {
        return fail(TT_ERR_INVALID, "plugin action index out of range");
    };
    start_plugin_command(
        plugins,
        PluginCommand::Invoke {
            plugin: action.plugin_index,
            callback: action.callback,
        },
    )
}

/// Start all callbacks registered for one lifecycle edge, in plugin filename
/// and declaration order. An edge with no listeners is an immediate success.
#[no_mangle]
pub extern "C" fn tt_plugins_emit(plugins: *mut TtPlugins, hook: TtPluginHook) -> TtStatus {
    let Some(plugins) = (unsafe { plugins.as_ref() }) else {
        return fail(TT_ERR_INVALID, "null TtPlugins");
    };
    let (index, hook) = match hook {
        TT_PLUGIN_HOOK_CONNECT => (0, tt_lua::Hook::Connect),
        TT_PLUGIN_HOOK_DISCONNECT => (1, tt_lua::Hook::Disconnect),
        _ => return fail(TT_ERR_INVALID, "unknown Lua plugin hook"),
    };
    if !plugins.hooks[index] {
        return TT_OK;
    }
    start_plugin_command(plugins, PluginCommand::Hook(hook))
}

/// Whether a callback is still running. One plugin set serialises callbacks,
/// so a second action is refused rather than re-entering a Lua VM.
#[no_mangle]
pub extern "C" fn tt_plugins_busy(plugins: *const TtPlugins) -> bool {
    unsafe { plugins.as_ref() }
        .is_some_and(|plugins| plugins.busy.load(std::sync::atomic::Ordering::Acquire))
}

/// The Unix descriptor which wakes for host calls and callback completion.
/// Returns `-1` on Windows and for an empty plugin set.
#[no_mangle]
pub extern "C" fn tt_plugins_poll_fd(plugins: *const TtPlugins) -> c_int {
    match unsafe { plugins.as_ref() }.and_then(|plugins| plugins.rx.as_ref()) {
        #[cfg(unix)]
        Some(rx) => rx.poll_fd(),
        #[cfg(not(unix))]
        Some(_) => -1,
        None => -1,
    }
}

/// The Windows waitable event corresponding to [`tt_plugins_poll_fd`].
#[no_mangle]
pub extern "C" fn tt_plugins_wait_handle(plugins: *const TtPlugins) -> *mut std::ffi::c_void {
    match unsafe { plugins.as_ref() }.and_then(|plugins| plugins.rx.as_ref()) {
        #[cfg(windows)]
        Some(rx) => rx.wait_handle(),
        #[cfg(not(windows))]
        Some(_) => ptr::null_mut(),
        None => ptr::null_mut(),
    }
}

/// Service pending plugin host calls on the frontend's thread.
#[no_mangle]
pub extern "C" fn tt_plugins_service(plugins: *mut TtPlugins, session: *mut TtSession) -> usize {
    let Some(plugins) = (unsafe { plugins.as_mut() }) else {
        set_error("null TtPlugins");
        return 0;
    };
    let s = session!(session, 0);
    plugins
        .rx
        .as_ref()
        .map_or(0, |rx| rx.service(&mut s.session, &mut plugins.ui))
}

/// Detach the persistent plugin receive tap from a session.
#[no_mangle]
pub extern "C" fn tt_session_unlink_plugins(session: *mut TtSession) {
    let s = session!(session);
    s.session.unlink_plugin();
}

/// Cancel any active callback, stop the worker and free the plugin set.
/// Call [`tt_session_unlink_plugins`] as well so the terminal stops collecting
/// bytes for it; the two handles deliberately do not retain pointers to one
/// another.
#[no_mangle]
pub extern "C" fn tt_plugins_free(plugins: *mut TtPlugins) {
    if plugins.is_null() {
        return;
    }
    let mut plugins = unsafe { Box::from_raw(plugins) };
    if let Some(rx) = plugins.rx.take() {
        rx.cancel();
        drop(rx);
    }
    if let Some(commands) = plugins.commands.take() {
        let _ = commands.send(PluginCommand::Stop);
    }
    if let Some(thread) = plugins.thread.take() {
        let _ = thread.join();
    }
}

/// [`SerialParams`] the other way round — [`TtSerialParams::to_rust`]'s
/// inverse, for handing a resolved target back to C.
fn serial_params_c(p: &SerialParams) -> TtSerialParams {
    TtSerialParams {
        baud: p.baud,
        data_bits: match p.data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        },
        parity: p.parity,
        stop_bits: match p.stop_bits {
            StopBits::One => 1,
            StopBits::Two => 2,
        },
        flow: p.flow,
        xon: p.xon,
        xoff: p.xoff,
        dtr: p.dtr,
        rts: p.rts,
        detect_break: p.detect_break,
        read_timeout_ms: ms(p.read_timeout),
    }
}

/// A duration in milliseconds, saturating — the ABI counts them in a `u32`,
/// and 49 days is not a timeout anybody meant.
fn ms(d: Duration) -> u32 {
    d.as_millis().min(u128::from(u32::MAX)) as u32
}

/// [`cstring`] for bytes that are not required to be UTF-8 — a path off a
/// command line.
fn cbytes(v: &[u8]) -> CString {
    let mut bytes = v.to_vec();
    bytes.retain(|&b| b != 0);
    CString::new(bytes).expect("NULs removed above")
}

fn cptr(s: &Option<CString>) -> *const c_char {
    s.as_ref().map_or(ptr::null(), |s| s.as_ptr())
}

// --- the control socket ---------------------------------------------------

/// The window, as the control socket asks about it.
///
/// The **second** place the ABI calls back into C, and the reasoning is the
/// same as [`TtMacroUi`]'s: these fire from inside [`tt_ctl_service`], on the
/// thread that called it, which is the frontend's own. So `connect` can raise
/// an SSH host-key dialog and `run_macro` can put an error box up, and the
/// client on the other end of the socket is parked on its own thread until
/// they close — which is what its own request already promised it.
///
/// **Zero-initialise it and fill in what you have.** A null pointer is not a
/// crash and not a silent success: the client is told
/// `-32003` — "this build cannot" — which it can tell apart from "no such
/// method". A window that answers `status` and nothing else is useful.
///
/// Everything a callback is handed is borrowed and dies when it returns.
/// Everything it hands *back* is copied before it returns, so a member
/// `QByteArray` is enough.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TtCtlHost {
    /// Passed back to every callback below and never touched here.
    pub user: *mut std::ffi::c_void,

    /// Start a macro. `argv` is `[path, param...]`, null-terminated — the same
    /// array [`tt_macro_start`] takes, because it is the same call.
    ///
    /// Return [`TT_OK`], or [`TT_ERR_BUSY`] when one is already running — the
    /// client tells those apart, since one is worth retrying. `error` may be
    /// filled in with a message; it is copied before this returns.
    pub run_macro: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            argv: *const *const c_char,
            error: *mut *const c_char,
        ) -> TtStatus,
    >,
    /// Whether a macro is running. See [`tt_macro_running`] for why a frontend
    /// must service before believing a false.
    pub macro_running: Option<extern "C" fn(user: *mut std::ffi::c_void) -> bool>,
    /// The last `setexitcode` — [`tt_macro_exit_code`].
    pub macro_exit_code: Option<extern "C" fn(user: *mut std::ffi::c_void) -> i32>,
    /// The End button — [`tt_macro_cancel`].
    pub stop_macro: Option<extern "C" fn(user: *mut std::ffi::c_void)>,

    /// Open what a Tera Term command line describes, through whatever path the
    /// frontend's own command line and its SSH dialog already use. Answering
    /// means the attempt has *started*.
    pub connect: Option<
        extern "C" fn(
            user: *mut std::ffi::c_void,
            line: *const c_char,
            error: *mut *const c_char,
        ) -> TtStatus,
    >,
    /// Close the window — `CmdCloseWin`, the macro language's `closett`.
    /// `false` is a frontend that will not.
    pub close_window: Option<extern "C" fn(user: *mut std::ffi::c_void) -> bool>,
    /// The window's own title, when it is not simply the terminal's OSC one.
    /// Null falls back to the session's.
    pub title: Option<extern "C" fn(user: *mut std::ffi::c_void) -> *const c_char>,
}

/// [`TtCtlHost`] on the Rust side of the seam.
///
/// Where a callback is null this falls through to [`NullHost`] rather than to
/// a hand-written refusal, so "not implemented" stays the trait's own
/// documented default and cannot drift from it.
struct CCtlHost {
    vt: TtCtlHost,
}

impl CtlHost for CCtlHost {
    fn run_macro(&mut self, path: &Path, params: &[String]) -> Result<(), RunError> {
        let Some(f) = self.vt.run_macro else {
            return CtlHost::run_macro(&mut NullHost, path, params);
        };
        // `[path, param...]`, which is the array `tt_macro_start` parses back
        // into a `CmdLine` — so a parameter that looks like a switch is a
        // parameter, exactly as it would be on `ttpmacro`'s own line.
        let mut owned = vec![cbytes(path.as_os_str().as_encoded_bytes())];
        owned.extend(params.iter().map(|p| cbytes(p.as_bytes())));
        let mut argv: Vec<*const c_char> = owned.iter().map(|s| s.as_ptr()).collect();
        argv.push(ptr::null());

        let mut err: *const c_char = ptr::null();
        let status = f(self.vt.user, argv.as_ptr(), &mut err);
        if status == TT_OK {
            return Ok(());
        }
        let message = unsafe { taken(err) }
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_else(|| "the macro would not start".into());
        match status {
            TT_ERR_BUSY => Err(RunError::Busy(message)),
            _ => Err(RunError::Failed(message)),
        }
    }

    fn macro_status(&mut self) -> MacroStatus {
        MacroStatus {
            running: self.vt.macro_running.is_some_and(|f| f(self.vt.user)),
            exit: self.vt.macro_exit_code.map_or(0, |f| f(self.vt.user)),
        }
    }

    fn stop_macro(&mut self) {
        if let Some(f) = self.vt.stop_macro {
            f(self.vt.user);
        }
    }

    fn connect(&mut self, line: &[u8]) -> Result<(), String> {
        let Some(f) = self.vt.connect else {
            return CtlHost::connect(&mut NullHost, line);
        };
        let mut err: *const c_char = ptr::null();
        if f(self.vt.user, cbytes(line).as_ptr(), &mut err) == TT_OK {
            return Ok(());
        }
        Err(unsafe { taken(err) }
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_else(|| "the connection would not open".into()))
    }

    fn close_window(&mut self) -> bool {
        self.vt.close_window.is_some_and(|f| f(self.vt.user))
    }

    fn title(&mut self) -> Option<String> {
        let f = self.vt.title?;
        let p = f(self.vt.user);
        unsafe { taken(p) }.map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

/// A bound control socket, and the threads behind it.
///
/// Free it with [`tt_ctl_free`], which is also what unlinks the socket file.
pub struct TtCtl {
    server: CtlServer,
    host: CCtlHost,
    path: CString,
}

/// Bind this window's control socket and start listening.
///
/// `name` is the socket's name in the runtime directory — a `/D=` topic, or
/// null for this process's pid, which is what a window with no `/D=` uses.
/// Letters, digits, `-` and `_`, at most twenty; anything else is refused
/// rather than turned into a path.
///
/// Null on failure, with [`tt_last_error`] set. The failure worth handling
/// separately is a name another window already has: that is the user's `/D=`
/// rather than the machine's, and it is reported as such.
///
/// `host` may be null, which is every callback refused. It is **copied**, so
/// the struct need not outlive this call; the `user` pointer inside it must
/// outlive the socket.
#[no_mangle]
pub extern "C" fn tt_ctl_start(name: *const c_char, host: *const TtCtlHost) -> *mut TtCtl {
    let name = match unsafe { name.as_ref() } {
        Some(_) => match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                fail(TT_ERR_INVALID, "socket name is not UTF-8");
                return ptr::null_mut();
            }
        },
        None => tt_ctl::addr::default_name(),
    };
    let server = match CtlServer::start(&name) {
        Ok(s) => s,
        Err(e) => {
            fail(TT_ERR_IO, format!("control socket: {e}"));
            return ptr::null_mut();
        }
    };
    let path = cbytes(server.path().as_os_str().as_encoded_bytes());
    Box::into_raw(Box::new(TtCtl {
        server,
        host: CCtlHost {
            vt: match unsafe { host.as_ref() } {
                Some(vt) => *vt,
                None => unsafe { std::mem::zeroed() },
            },
        },
        path,
    }))
}

/// Where it is listening.
///
/// Borrowed and valid until [`tt_ctl_free`]. Worth putting in `$STERNA_CTL`
/// for anything the window starts — the local shell above all, so that a
/// script running *inside* the terminal can drive the window it is running in.
#[no_mangle]
pub extern "C" fn tt_ctl_path(ctl: *const TtCtl) -> *const c_char {
    match unsafe { ctl.as_ref() } {
        Some(c) => c.path.as_ptr(),
        None => ptr::null(),
    }
}

/// A descriptor that becomes readable when a client wants something.
///
/// The same bargain as [`tt_session_poll_fd`] and [`tt_macro_poll_fd`]: wait
/// on it and call [`tt_ctl_service`] when it fires. A window nobody is talking
/// to costs nothing, so there is no timer. Returns `-1` on Windows, whose
/// native spelling is [`tt_ctl_wait_handle`].
#[no_mangle]
pub extern "C" fn tt_ctl_poll_fd(ctl: *const TtCtl) -> c_int {
    match unsafe { ctl.as_ref() } {
        #[cfg(unix)]
        Some(c) => c.server.poll_fd(),
        #[cfg(not(unix))]
        Some(_) => -1,
        None => -1,
    }
}

/// The Windows event that becomes signalled when a control client wants
/// something.
///
/// The native spelling of [`tt_ctl_poll_fd`]. It is borrowed until
/// [`tt_ctl_free`], must not be closed by the frontend, and is null on Unix.
#[no_mangle]
pub extern "C" fn tt_ctl_wait_handle(ctl: *const TtCtl) -> *mut std::ffi::c_void {
    match unsafe { ctl.as_ref() } {
        #[cfg(windows)]
        Some(c) => c.server.wait_handle(),
        #[cfg(not(windows))]
        Some(_) => ptr::null_mut(),
        None => ptr::null_mut(),
    }
}

/// Run whatever the clients have asked for, against `session`. Returns how
/// many ran; never blocks on a client.
///
/// **This is where the [`TtCtlHost`] callbacks fire**, so it can take as long
/// as the user does — and it can close the window, which means the caller must
/// not touch anything it owns afterwards without checking.
///
/// The same re-entrancy rule as [`tt_macro_service`]: the fd or manual-reset
/// event remains ready inside a dialog's nested event loop, so disable the
/// notifier across this call.
#[no_mangle]
pub extern "C" fn tt_ctl_service(ctl: *mut TtCtl, session: *mut TtSession) -> usize {
    let Some(c) = (unsafe { ctl.as_mut() }) else {
        set_error("null TtCtl");
        return 0;
    };
    let s = session!(session, 0);
    c.server.service(&mut s.session, &mut c.host)
}

/// Stop listening, hang up on every client, and unlink the socket.
///
/// A client blocked on an answer is released rather than left waiting: it is
/// told the window has gone, which is the one error every request can return.
#[no_mangle]
pub extern "C" fn tt_ctl_free(ctl: *mut TtCtl) {
    if ctl.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(ctl) });
}
