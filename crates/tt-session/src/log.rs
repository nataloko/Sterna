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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    /// `%Y-%m-%d %H:%M:%S.%N` in local time — upstream's default format.
    Local,
    /// The same, in UTC.
    Utc,
    /// Seconds since the log was opened, as `H:MM:SS.mmm`. The one that is
    /// actually useful on a console: "how long after reset did it hang".
    Elapsed,
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
        }
    }
}

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

    /// Raw bytes off the wire. Ignored unless the log is in [`LogMode::Raw`].
    pub fn write_raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.opts.mode != LogMode::Raw || bytes.is_empty() {
            return Ok(());
        }
        self.put(bytes)?;
        self.maybe_rotate()
    }

    /// Displayed text, from the parser's tap. Ignored unless the log is in
    /// [`LogMode::Text`].
    pub fn write_text(&mut self, text: &str) -> std::io::Result<()> {
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
            Timestamp::Elapsed => {
                let d = self.started.elapsed();
                let secs = d.as_secs();
                format!(
                    "[{}:{:02}:{:02}.{:03}] ",
                    secs / 3600,
                    (secs / 60) % 60,
                    secs % 60,
                    d.subsec_millis()
                )
            }
            Timestamp::Local => format!("[{}] ", civil_now(local_offset())),
            Timestamp::Utc => format!("[{}] ", civil_now(Duration::ZERO)),
        }
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
fn local_offset() -> Duration {
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

/// Windows wants `GetTimeZoneInformation`, which is Stage 3's to add along
/// with the rest of the platform. Until then a local timestamp there reads as
/// UTC, which is wrong but not *silently* wrong — it is written down here and
/// the UTC option says the same thing honestly.
#[cfg(not(unix))]
fn local_offset() -> Duration {
    Duration::ZERO
}

/// `%Y-%m-%d %H:%M:%S.%N` — upstream's default `LogTimestampFormat`.
///
/// Hand-rolled rather than pulling in a date crate, because the whole
/// requirement is one fixed format: an arbitrary strftime string is a
/// `TERATERM.INI` key and belongs to the settings schema, and a dependency
/// bought for a format nobody has asked for yet is a dependency to carry.
fn civil_now(offset: Duration) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        + offset;
    let millis = now.subsec_millis();
    let secs = now.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{millis:03}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days`, which is the standard way to do this
/// without a table and is exact for every date this will ever see.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
