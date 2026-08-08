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
}
