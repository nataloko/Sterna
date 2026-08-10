//! Session logging — a tap on the byte stream, written to a file.
//!
//! Ported from `filesys_log.cpp`, which is worth saying because the shape
//! looks over-specified until you know what it is compatible with: `[time] `
//! prefixes at the head of each line, and generation rotation that shifts
//! `file.1` to `file.2` rather than stamping names with a date.
//!
//! The settings are `TERATERM.INI` keys and become Stage 2's generated schema.
//! They are a struct here so that the schema has one place to write into.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What goes in the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogMode {
    /// Every byte that arrived, verbatim — `ts.LogBinary`.
    ///
    /// The only mode that can be replayed back through a terminal, and the
    /// only one that keeps what a corrupt line actually sent.
    ///
    /// **A timestamp asked for alongside this is silently dropped**, which is
    /// upstream's behaviour — `filesys_log.cpp:243` clears `LogTimestamp` with
    /// the mode — and the right one: a `[time] ` in the middle of a byte
    /// capture makes it no longer a capture.
    Raw,
    /// The text the terminal decided to display, escape sequences already
    /// consumed — `ts.LogTypePlainText`.
    #[default]
    Text,
}

/// Which clock the `[time] ` prefix reads.
///
/// `repr(u32)` because the C ABI names these variants directly rather than
/// keeping a second copy of the list that can drift — the same trade as
/// `tt_vt::Key`, and with the same consequence: reordering them is an ABI
/// break whose only symptom is the committed header's diff.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Timestamp {
    /// No prefix.
    #[default]
    None,
    /// A wall clock in local time, formatted by [`LogOptions::format`] —
    /// `%Y-%m-%d %H:%M:%S.%N` unless the settings say otherwise.
    Local,
    /// The same, in UTC.
    Utc,
    /// Time since the log was opened, as `D HH:MM:SS.mmm` — `strelapsedW`
    /// (`ttlib_static.c:554`), whose leading field is **days** and is printed
    /// whether or not there have been any. The one that is actually useful on
    /// a console: "how long after reset did it hang".
    Elapsed,
    /// The same clock, started at the *connection* rather than at the log —
    /// upstream's `TIMESTAMP_ELAPSED_CONNECTED`, which reads `cv.ConnectedTime`
    /// (`commlib.c:787`) where the one above reads `fv->StartTime`. The two
    /// agree until a log is opened by hand part-way through a session, which
    /// is exactly when the difference is the thing being asked about.
    ///
    /// With no connection open it falls back to the log's own start: upstream
    /// subtracts a `ConnectedTime` of zero from `GetTickCount()` and prints
    /// how long the machine has been up, which is not worth reproducing.
    ElapsedConnection,
}

/// `ts.LogRotate*` plus the mode and timestamp keys.
#[derive(Clone, Debug)]
pub struct LogOptions {
    pub mode: LogMode,
    pub timestamp: Timestamp,
    /// Add to an existing file rather than truncating it — `ts.Append`.
    pub append: bool,
    /// Rotate once the file passes this many bytes. Zero disables it.
    pub rotate_size: u64,
    /// How many generations to keep. Upstream's `LogRotateStep`, whose zero
    /// means its internal cap of 10000; that is a strange default to inherit,
    /// so zero here means "do not rotate" and the count is explicit.
    pub rotate_keep: u32,
    /// What a line feed writes. **Upstream writes CR LF**
    /// (`vtterm.c:361` sets `log_cr_type = 0`); this defaults to LF instead,
    /// deliberately, because the artefact is a text file a Linux user opens in
    /// a pager and `^M` at every line end has no upside off Windows. Set it to
    /// CR LF for a byte-identical Tera Term log.
    pub crlf: bool,
    /// `ts.LogTimestampFormat`, for the two timestamps that are a wall clock.
    /// The elapsed pair ignore it — they have no date in them to format.
    ///
    /// Expanded by [`logname::timestamp_format`](crate::logname::timestamp_format),
    /// which is upstream's own `ttstrftime` and **not** the C library's: it
    /// knows twelve conversions, `%N` for milliseconds among them, and hands
    /// back anything else as literal text. This is the one field of the
    /// options that does not cross the C ABI — see `tt_session_log_start`.
    pub format: String,
}

impl Default for LogOptions {
    fn default() -> Self {
        LogOptions {
            mode: LogMode::Text,
            timestamp: Timestamp::None,
            append: false,
            rotate_size: 0,
            rotate_keep: 0,
            crlf: false,
            format: String::from(DEFAULT_TIMESTAMP_FORMAT),
        }
    }
}

/// `ttset.c:996`, and the `%N` on the end is upstream's own conversion rather
/// than a strftime one.
pub const DEFAULT_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S.%N";

/// An open session log.
pub struct SessionLog {
    path: PathBuf,
    file: BufWriter<File>,
    opts: LogOptions,
    /// Bytes in the *current* generation, which is what rotation measures.
    written: u64,
    /// Total bytes over the life of the log, for a status line.
    total: u64,
    started: Instant,
    /// Whether the next character begins a line and so wants a timestamp.
    /// True at the start so the first line gets one.
    at_line_start: bool,
    /// `logpause` — see [`set_paused`](SessionLog::set_paused).
    paused: bool,
}

impl SessionLog {
    pub fn open(path: &Path, opts: LogOptions) -> std::io::Result<SessionLog> {
        // Normalised here rather than left to the write path, so that anything
        // reporting the log's settings back to a user cannot claim a timestamp
        // that will never be written.
        let mut opts = opts;
        if opts.mode == LogMode::Raw {
            opts.timestamp = Timestamp::None;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(opts.append)
            .truncate(!opts.append)
            .open(path)?;
        let written = if opts.append {
            file.metadata()?.len()
        } else {
            0
        };
        Ok(SessionLog {
            path: path.to_path_buf(),
            file: BufWriter::new(file),
            opts,
            written,
            total: 0,
            started: Instant::now(),
            at_line_start: true,
            paused: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mode(&self) -> LogMode {
        self.opts.mode
    }

    /// The options as they are actually in force, which is not always what was
    /// passed in — see [`LogMode::Raw`].
    pub fn options(&self) -> &LogOptions {
        &self.opts
    }

    /// Bytes written since the log was opened, across all generations.
    pub fn bytes(&self) -> u64 {
        self.total
    }

    /// Stop or resume writing — `logpause` and `logstart`.
    ///
    /// **What arrives while paused is discarded, not buffered**
    /// (`logpause.html`), so this is a tap that closes rather than a valve on a
    /// queue. Upstream does the discarding in two different places — at the
    /// input for a binary log (`filesys_log.cpp:1038`) and while draining the
    /// ring for a text one (`:647`) — and both amount to the same thing here,
    /// where there is no ring between the tap and the file.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Count [`Timestamp::ElapsedConnection`] from `at` rather than from the
    /// moment this log opened.
    ///
    /// Separate from [`SessionLog::open`] because the log does not know what a
    /// connection is and should not learn: the caller that has both is
    /// [`crate::Session::start_log`], and it is the only one that calls this.
    pub fn set_elapsed_origin(&mut self, at: Instant) {
        self.started = at;
    }

    /// Raw bytes off the wire. Ignored unless the log is in [`LogMode::Raw`].
    pub fn write_raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.paused || self.opts.mode != LogMode::Raw || bytes.is_empty() {
            return Ok(());
        }
        self.put(bytes)?;
        self.maybe_rotate()
    }

    /// Displayed text, from the parser's tap. Ignored unless the log is in
    /// [`LogMode::Text`].
    pub fn write_text(&mut self, text: &str) -> std::io::Result<()> {
        if self.paused {
            return Ok(());
        }
        self.write_text_now(text)
    }

    /// `logwrite` — a string put into the log by something other than the far
    /// end, **including while the log is paused**, and flushed at once.
    ///
    /// The pause is where this diverges, deliberately, and it is documentation
    /// against code: `logwrite.html` says in as many words that the string "can
    /// be written even while logging is paused", and upstream's cannot.
    /// `FLogWriteStr` (`filesys_log.cpp:833`) puts the characters in the same
    /// ring the tap fills and then drains it, and the drain loop discards
    /// everything it pulls while paused (`:647`) — so the note a script writes
    /// to explain the gap falls into the gap. Reproducing that would mean
    /// implementing the sentence the manual does not say.
    ///
    /// In [`LogMode::Raw`] the string's own bytes go down verbatim, which is
    /// upstream converting it to `ts.KanjiCode` first; here that is UTF-8.
    pub fn write_str(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        match self.opts.mode {
            LogMode::Text => self.write_text_now(text)?,
            LogMode::Raw => {
                self.put(text.as_bytes())?;
                self.maybe_rotate()?;
            }
        }
        // On the spot rather than at the next drain — `FLogWriteStr` calls
        // `LogToFile` itself, so the note is in the file before the script's
        // next line runs, which is what a script writing one wants.
        self.flush()
    }

    /// `logrotate size` — rotate once the file passes this many bytes, or stop
    /// measuring it at zero (`FLogRotateSize`).
    ///
    /// Reconfiguration only: none of the three rotates anything now, which the
    /// documentation says twice.
    pub fn set_rotate_size(&mut self, size: u64) {
        self.opts.rotate_size = size;
    }

    /// `logrotate rotate` — how many generations to keep (`FLogRotateRotate`).
    pub fn set_rotate_keep(&mut self, keep: u32) {
        self.opts.rotate_keep = keep;
    }

    /// `logrotate halt` — stop rotating, and forget both numbers
    /// (`FLogRotateHalt`, which clears the size and the step as well as the
    /// mode).
    pub fn halt_rotate(&mut self) {
        self.opts.rotate_size = 0;
        self.opts.rotate_keep = 0;
    }

    fn write_text_now(&mut self, text: &str) -> std::io::Result<()> {
        if self.opts.mode != LogMode::Text || text.is_empty() {
            return Ok(());
        }
        // Built once and written once rather than per character: a busy line
        // is thousands of characters a second, and `at_line_start` is the only
        // state that has to be threaded through.
        let mut out: Vec<u8> = Vec::with_capacity(text.len() + 16);
        for ch in text.chars() {
            if self.at_line_start && self.opts.timestamp != Timestamp::None {
                out.extend_from_slice(self.stamp().as_bytes());
                self.at_line_start = false;
            }
            if ch == '\n' {
                // Upstream normalises the line ending rather than echoing what
                // arrived, which is why a CR LF from the host does not become
                // a blank line here.
                out.extend_from_slice(if self.opts.crlf { b"\r\n" } else { b"\n" });
                self.at_line_start = true;
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                self.at_line_start = false;
            }
        }
        self.put(&out)?;
        self.maybe_rotate()
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }

    fn put(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.file.write_all(bytes)?;
        self.written += bytes.len() as u64;
        self.total += bytes.len() as u64;
        Ok(())
    }

    /// `[<time>] `, exactly upstream's bracket-and-space.
    fn stamp(&self) -> String {
        match self.opts.timestamp {
            Timestamp::None => String::new(),
            Timestamp::Elapsed | Timestamp::ElapsedConnection => {
                // `strelapsedW`'s own fields, days included: a console log
                // left open over a weekend is why upstream prints one.
                let d = self.started.elapsed();
                let secs = d.as_secs();
                format!(
                    "[{} {:02}:{:02}:{:02}.{:03}] ",
                    secs / 86_400,
                    (secs / 3600) % 24,
                    (secs / 60) % 60,
                    secs % 60,
                    d.subsec_millis()
                )
            }
            Timestamp::Local => self.civil_stamp(local_offset()),
            Timestamp::Utc => self.civil_stamp(Duration::ZERO),
        }
    }

    fn civil_stamp(&self, offset: Duration) -> String {
        let now = crate::logname::Civil::now(offset);
        format!(
            "[{}] ",
            crate::logname::timestamp_format(&self.opts.format, now)
        )
    }

    /// Shift the generations along and start a new file — upstream's
    /// `LogRotate`, which renames rather than dating the name.
    ///
    /// Renaming from the oldest backwards matters: going the other way
    /// overwrites `file.2` with `file.1` before `file.2` has moved to
    /// `file.3`, and the history quietly collapses to two entries.
    fn maybe_rotate(&mut self) -> std::io::Result<()> {
        if self.opts.rotate_size == 0
            || self.opts.rotate_keep == 0
            || self.written <= self.opts.rotate_size
        {
            return Ok(());
        }
        self.file.flush()?;

        let keep = self.opts.rotate_keep;
        let gen = |n: u32| -> PathBuf {
            let mut p = self.path.clone().into_os_string();
            p.push(format!(".{n}"));
            PathBuf::from(p)
        };
        // The oldest generation falls off the end.
        let _ = std::fs::remove_file(gen(keep));
        for n in (1..keep).rev() {
            if gen(n).exists() {
                std::fs::rename(gen(n), gen(n + 1))?;
            }
        }
        std::fs::rename(&self.path, gen(1))?;

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        self.file = BufWriter::new(file);
        self.written = 0;
        self.at_line_start = true;
        Ok(())
    }
}

impl Drop for SessionLog {
    fn drop(&mut self) {
        // A log that lost its tail because nothing flushed is worse than no
        // log: the interesting part of a console capture is always the end.
        let _ = self.file.flush();
    }
}

/// The system's UTC offset, from `localtime_r` — the one thing here that has
/// to ask libc, because the offset depends on the date *and* on the zone
/// database, and nothing in std reads either.
#[cfg(unix)]
pub(crate) fn local_offset() -> Duration {
    // `Duration` is unsigned, so a west-of-Greenwich offset is expressed by
    // wrapping: the caller adds it to a UTC timestamp modulo a day, which is
    // what a fixed-offset clock display needs and all it needs.
    let secs = unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return Duration::ZERO;
        }
        tm.tm_gmtoff
    };
    Duration::from_secs(secs.rem_euclid(86_400) as u64)
}

/// The current Windows UTC offset. `Bias` and its seasonal adjustment are
/// minutes to add to local time to obtain UTC, so the sign is reversed here.
#[cfg(windows)]
pub(crate) fn local_offset() -> Duration {
    use windows_sys::Win32::System::SystemServices::{
        TIME_ZONE_ID_DAYLIGHT, TIME_ZONE_ID_STANDARD, TIME_ZONE_ID_UNKNOWN,
    };
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let state = unsafe { GetTimeZoneInformation(&mut zone) };
    let seasonal = match state {
        TIME_ZONE_ID_STANDARD => zone.StandardBias,
        TIME_ZONE_ID_DAYLIGHT => zone.DaylightBias,
        TIME_ZONE_ID_UNKNOWN => 0,
        _ => return Duration::ZERO,
    };
    let east = -i64::from(zone.Bias + seasonal) * 60;
    Duration::from_secs(east.rem_euclid(86_400) as u64)
}
