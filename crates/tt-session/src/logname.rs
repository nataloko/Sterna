//! The log's file name, which is a template rather than a name.
//!
//! `LogDefaultName` ships as `teraterm.log` and looks like a plain file name,
//! which is why the machinery behind it is easy to miss —
//! `FLogGetLogFilename` (`filesys_log.cpp:964`) puts it through four passes
//! before anything is opened: a `strftime` expansion, then `&h`/`&p`/`&u` for
//! the connection, then a sweep for characters a file name cannot hold, and
//! finally a join against the log directory. Anybody logging more than one
//! console ends up with a `&h-%Y%m%d.log`, and a port that took the name
//! literally would write every session into one file.
//!
//! **There are two strftime expanders upstream and they are not the same
//! one**, which is the finding that decided the shape of this module. A log
//! *file name* goes through `deleteInvalidStrftimeCharW`
//! (`ttlib_static_cpp.cpp:1925`) and then the C runtime's own `wcsftime`; a
//! log *timestamp* goes through `ttstrftime` (`ttlib_static.c:380`), which is
//! Tera Term's own implementation of a dozen conversions. The two accept
//! different sets and disagree in **both** directions:
//!
//! - `%N` — milliseconds, upstream's own invention — works in a timestamp and
//!   is *deleted* from a file name, because `N` is not in the table the
//!   validator checks. The shipped `LogTimestampFormat` ends in `%N`, so
//!   pasting that format into `LogDefaultName` silently loses it.
//! - `%j`, `%p`, `%U`, `%W`, `%x`, `%X`, `%z`, `%Z`, `%A`, `%c` and `%I` all
//!   work in a file name and come out as literal text in a timestamp, because
//!   `ttstrftime`'s `default` arm emits the `%` and moves on.
//! - `%e` is the other way round again: `ttstrftime` implements it and the
//!   validator rejects it, so it survives a timestamp and vanishes from a name.
//!
//! Both are reproduced. Neither is documented upstream.

use std::path::{Path, PathBuf};
use tt_config::Settings;

/// What the three `&`-escapes expand to — `ConvertLognameW`
/// (`filesys_log.cpp:160`), which reads the *live* connection rather than the
/// settings.
///
/// Every field is optional because upstream's are conditional on `cv.Open`:
/// an escape with nothing to say expands to nothing at all rather than to a
/// placeholder, so a name built before anything connected is short rather than
/// wrong.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogContext {
    /// `&h`. The host name for a TCP session and `COM<n>` for a serial one —
    /// upstream reads `ts.HostName` or formats `ts.ComPort`, and this port
    /// hands over whatever it opened, which on Linux is a device name rather
    /// than a `COM` number.
    pub host: Option<String>,
    /// `&p`, and TCP only: upstream's arm tests `PortType == IdTCPIP` before
    /// it looks, so a serial session drops the escape.
    pub tcp_port: Option<u16>,
    /// `&u`, which is the *logged-in user* and not the account being connected
    /// to — `GetUserNameW`. See [`current_user`].
    pub user: Option<String>,
}

/// `GetUserNameW`'s counterpart: who is running this, for `&u`.
#[cfg(unix)]
pub fn current_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|u| !u.is_empty())
}

#[cfg(windows)]
pub fn current_user() -> Option<String> {
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

    // UNLEN is 256 and the count includes the terminator.
    let mut name = vec![0u16; 257];
    let mut len = name.len() as u32;
    if unsafe { GetUserNameW(name.as_mut_ptr(), &mut len) } == 0 || len <= 1 {
        return None;
    }
    name.truncate(len as usize - 1);
    Some(String::from_utf16_lossy(&name))
}

/// A civil date and time, which is what both expanders read.
///
/// Hand-rolled rather than taken from a date crate for the reason `log.rs`
/// already gives: the requirement is a fixed set of conversions over a wall
/// clock, and a dependency bought for that is a dependency to carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
    /// 0 is Sunday, which is `tm_wday`'s numbering and `%w`'s.
    pub weekday: u32,
}

impl Civil {
    /// Now, shifted by `offset` — pass a zone offset for local time and
    /// [`Duration::ZERO`](std::time::Duration::ZERO) for UTC, which is how
    /// `log.rs` already spells the same choice.
    pub fn now(offset: std::time::Duration) -> Civil {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            + offset;
        Civil::from_unix(now.as_secs(), now.subsec_millis())
    }

    pub fn from_unix(secs: u64, millis: u32) -> Civil {
        let (days, rem) = (secs / 86_400, secs % 86_400);
        let (year, month, day) = civil_from_days(days as i64);
        Civil {
            year,
            month,
            day,
            hour: (rem / 3600) as u32,
            minute: ((rem / 60) % 60) as u32,
            second: (rem % 60) as u32,
            millis,
            // 1970-01-01 was a Thursday, which is `tm_wday` 4.
            weekday: ((days + 4) % 7) as u32,
        }
    }
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

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `ttstrftime` (`ttlib_static.c:380`) — the *timestamp's* expander, which is
/// upstream's own and implements twelve conversions.
///
/// **An unrecognised conversion is not an error and is not dropped**: the
/// `default` arm writes the `%` and does not consume the letter, so `%A`
/// comes out as `%A` and a user who copied a format from `strftime(3)`'s
/// manual gets it back verbatim in their log. Ten conversions the *file name*
/// path accepts land here — see this module's header for the list.
pub fn timestamp_format(format: &str, t: Civil) -> String {
    let src: Vec<char> = format.chars().collect();
    let mut out = String::with_capacity(format.len() * 2);
    let mut i = 0;
    while i < src.len() {
        if src[i] != '%' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        let code = src.get(i + 1).copied().unwrap_or('\0');
        match code {
            'a' => out.push_str(WEEKDAYS[(t.weekday % 7) as usize]),
            'b' => out.push_str(MONTHS[((t.month.max(1) - 1) % 12) as usize]),
            'd' => out.push_str(&format!("{:02}", t.day)),
            // Not in the manual, and not in the file-name path's table either.
            'e' => out.push_str(&format!("{:2}", t.day)),
            'H' => out.push_str(&format!("{:02}", t.hour)),
            // Tera Term's own: milliseconds, three digits.
            'N' => out.push_str(&format!("{:03}", t.millis)),
            'm' => out.push_str(&format!("{:02}", t.month)),
            'M' => out.push_str(&format!("{:02}", t.minute)),
            'S' => out.push_str(&format!("{:02}", t.second)),
            'w' => out.push_str(&format!("{}", t.weekday)),
            'y' => out.push_str(&format!("{:02}", t.year.rem_euclid(100))),
            'Y' => out.push_str(&format!("{:04}", t.year)),
            '%' => out.push('%'),
            _ => {
                // The `%` is kept and the letter is *not* consumed, so it is
                // copied as a literal on the next turn. `%A` is `%A`.
                out.push('%');
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    out
}

/// `IsValidStrftimeCode` (`ttlib_static_cpp.cpp:1881`) — the conversions
/// upstream will let through to the C runtime, which is Visual Studio 2005's
/// set. Note what is *not* here: `%e` and `%N`, both of which the timestamp
/// expander implements.
const FILENAME_CODES: &str = "aAbBcdHIjmMpSUwWxXyYzZ%";

/// `deleteInvalidStrftimeCharW` (`ttlib_static_cpp.cpp:1925`): drop every
/// conversion the runtime might not understand, so that a hand-edited name
/// cannot crash `wcsftime`.
///
/// Two quirks come out of the way it walks the string, and both are
/// reproduced because a file name depends on them:
///
/// - A rejected `%x` loses **both** characters, while a rejected `%#x` loses
///   the `%` and the `#` and leaves the letter behind as literal text. The
///   `if (p-i == 2)` guards that would have made those consistent can never
///   be true — `i` has already been advanced past the `%` when they are
///   tested — so the letter is picked up by the loop's own copy on the next
///   turn.
/// - A trailing `%` is dropped, which is the only case the comment mentions.
fn delete_invalid_codes(format: &str) -> String {
    let src: Vec<char> = format.chars().collect();
    let mut out = String::with_capacity(format.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] != '%' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        let Some(&next) = src.get(i + 1) else {
            // "% で終わっている場合はコピーしない"
            break;
        };
        // `%#d` is Visual Studio's "no leading zero". The modifier is only
        // recognised when something follows it.
        let hashed = next == '#' && src.get(i + 2).is_some();
        let code = if hashed { src[i + 2] } else { next };
        if FILENAME_CODES.contains(code) {
            out.push('%');
            if hashed {
                out.push('#');
            }
            out.push(code);
            i += if hashed { 3 } else { 2 };
        } else {
            // The `%` and one character after it go; anything past that is
            // reconsidered as ordinary text.
            i += 2;
        }
    }
    out
}

/// The file name's expander: upstream's validator, then the C runtime's
/// `strftime` — the same call `wcsftime` is, and deliberately not a
/// reimplementation, because `%c`, `%x`, `%X` and `%Z` are the locale's
/// business and this is the one place a port can have them for free.
///
/// The `#` modifier is the platform exception: MSVC reads `%#d` as "no leading
/// zero" and glibc has never heard of it, so a name using one expands
/// differently on the two platforms. That is upstream's behaviour too; the
/// format goes to each platform's own C runtime unchanged.
fn strftime(format: &str, t: Civil) -> String {
    let cleaned = delete_invalid_codes(format);
    if cleaned.is_empty() {
        return String::new();
    }
    let Ok(c_format) = std::ffi::CString::new(cleaned) else {
        // An interior NUL cannot reach `strftime` at all. Upstream's wide
        // string cannot hold one either; a name that has one is not a name.
        return String::new();
    };
    #[cfg(unix)]
    let tm = libc::tm {
        tm_sec: t.second as i32,
        tm_min: t.minute as i32,
        tm_hour: t.hour as i32,
        tm_mday: t.day as i32,
        tm_mon: t.month as i32 - 1,
        tm_year: (t.year - 1900) as i32,
        tm_wday: t.weekday as i32,
        tm_yday: day_of_year(t) as i32,
        tm_isdst: -1,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    #[cfg(windows)]
    let tm = WindowsTm {
        tm_sec: t.second as i32,
        tm_min: t.minute as i32,
        tm_hour: t.hour as i32,
        tm_mday: t.day as i32,
        tm_mon: t.month as i32 - 1,
        tm_year: (t.year - 1900) as i32,
        tm_wday: t.weekday as i32,
        tm_yday: day_of_year(t) as i32,
        tm_isdst: -1,
    };
    let mut len = 64usize;
    loop {
        let mut buf = vec![0u8; len];
        #[cfg(unix)]
        let n = unsafe {
            libc::strftime(
                buf.as_mut_ptr() as *mut libc::c_char,
                len,
                c_format.as_ptr(),
                &tm,
            )
        };
        #[cfg(windows)]
        let n = unsafe { c_strftime(buf.as_mut_ptr().cast(), len, c_format.as_ptr(), &tm) };
        if n > 0 {
            buf.truncate(n);
            return String::from_utf8_lossy(&buf).into_owned();
        }
        // Upstream doubles until it fits and has no ceiling, so a format whose
        // expansion is legitimately empty — `%Z` where the zone has no name —
        // spins forever. Bounded here instead; the answer is the same empty
        // string it was always going to be.
        if len >= 8192 {
            return String::new();
        }
        len *= 2;
    }
}

/// MSVC and MinGW expose the ISO C `tm` fields in this order. Unlike glibc's
/// `tm`, the Windows structure has no zone-name or UTC-offset extension.
#[cfg(windows)]
#[repr(C)]
struct WindowsTm {
    tm_sec: std::ffi::c_int,
    tm_min: std::ffi::c_int,
    tm_hour: std::ffi::c_int,
    tm_mday: std::ffi::c_int,
    tm_mon: std::ffi::c_int,
    tm_year: std::ffi::c_int,
    tm_wday: std::ffi::c_int,
    tm_yday: std::ffi::c_int,
    tm_isdst: std::ffi::c_int,
}

#[cfg(windows)]
unsafe extern "C" {
    #[link_name = "strftime"]
    fn c_strftime(
        buffer: *mut std::ffi::c_char,
        size: usize,
        format: *const std::ffi::c_char,
        time: *const WindowsTm,
    ) -> usize;
}

fn day_of_year(t: Civil) -> u32 {
    const CUMULATIVE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (t.year % 4 == 0 && t.year % 100 != 0) || t.year % 400 == 0;
    let past_feb = t.month > 2;
    CUMULATIVE[((t.month.max(1) - 1) % 12) as usize] + t.day - 1 + u32::from(leap && past_feb)
}

/// `ConvertLognameW` (`filesys_log.cpp:160`) — `&h`, `&p`, `&u`.
///
/// **There is no escape for a literal `&`.** The `default` arm drops the `&`
/// and leaves the following character to be read again, so `&&h` is the host
/// name rather than a literal `&h`, and `&x` is `x`. A `&` at the very end of
/// the string is the one that survives, because the arm that eats it is
/// guarded on there being a next character at all.
fn convert_logname(src: &str, ctx: &LogContext) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let Some(&code) = chars.get(i + 1) else {
            out.push('&');
            break;
        };
        match code {
            'h' => {
                if let Some(host) = &ctx.host {
                    out.push_str(host);
                }
                i += 2;
            }
            'p' => {
                if let Some(port) = ctx.tcp_port {
                    out.push_str(&port.to_string());
                }
                i += 2;
            }
            'u' => {
                if let Some(user) = &ctx.user {
                    out.push_str(user);
                }
                i += 2;
            }
            // The `&` alone is consumed.
            _ => i += 1,
        }
    }
    out
}

/// `invalidFileNameCharsW` (`ttlib_static_cpp.cpp:57`) plus every control
/// character, replaced with `_`.
///
/// Windows' set, kept on Linux where only `/` and NUL are actually forbidden:
/// a log name is the one artefact that gets carried between the two, and a
/// file called `10:32.log` that cannot be copied to a Windows machine is a
/// worse answer than one that never had the colon.
fn replace_invalid_filename_chars(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c < ' ' || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `FLogGetLogFilenameBase` (`filesys_log.cpp:893`): one component of a path,
/// expanded and made safe. Exposed for the tests and for a frontend that wants
/// to show what a template will produce.
pub fn expand_name(template: &str, ctx: &LogContext, t: Civil) -> String {
    // The path is stripped first, so a `LogDefaultName` of `logs/&h.log`
    // loses its directory rather than creating one.
    let leaf = template.rsplit(['/', '\\']).next().unwrap_or(template);
    replace_invalid_filename_chars(&convert_logname(&strftime(leaf, t), ctx))
}

/// `GetTermLogDir` (`ttlib_types.cpp:63`): where a relative log name lands.
///
/// Three answers in order, and the middle one is the surprise —
/// **`FileDir`, the file-*transfer* directory, decides where a log goes** when
/// `LogDefaultPath` is empty and it names somewhere that exists. Upstream
/// expands environment variables in it first; nothing does that here, because
/// `%VAR%` is not what a Linux path holds and `$VAR` is not what upstream
/// expands.
///
/// The third answer is [`program_log_dir`], which is a **different directory
/// with a similar name** — see there.
pub fn term_log_dir(settings: &Settings) -> PathBuf {
    if !settings.log_default_path.is_empty() {
        return PathBuf::from(&settings.log_default_path);
    }
    if !settings.transfer_dir.is_empty() {
        let dir = expanded_transfer_dir(&settings.transfer_dir);
        if dir.is_dir() {
            return dir;
        }
    }
    program_log_dir()
}

/// `GetLogDirW` (`ttlib_static_dir.cpp:229`), which is `ts.LogDirW`: where the
/// *program's* own logs and dumps go.
///
/// **Not the terminal's log directory**, whatever the two names suggest, and
/// `tttypes.h:579` says so in as many words. It takes no settings at all —
/// `%LOCALAPPDATA%\teraterm5`, or `<exe>\log` in portable mode — where
/// [`term_log_dir`] consults two keys before falling back to it. So the two
/// coincide exactly when neither key is set, which is every default install
/// and no configured one.
///
/// Three things land here rather than beside the session log: `TELNET.LOG`
/// (`telnet.c:129`), the file-transfer protocols' own logs
/// (`ttpfile/zmodem.c:815` and its five siblings), and the proxy's `DebugLog`
/// (`TTProxy.h:198`, which is the `Logger`'s folder).
pub fn program_log_dir() -> PathBuf {
    #[cfg(windows)]
    match std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v).join("sterna"),
        None => std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".")),
    }
    // Logs are state rather than configuration or data on Unix, so the same
    // answer is `XDG_STATE_HOME` there.
    #[cfg(unix)]
    match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v).join("sterna"),
        None => match std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            Some(home) => PathBuf::from(home).join(".local/state/sterna"),
            // Upstream falls back to the executable's directory; the working
            // directory is the same idea for a program launched from a shell.
            None => PathBuf::from("."),
        },
    }
}

#[cfg(unix)]
fn expanded_transfer_dir(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[cfg(windows)]
fn expanded_transfer_dir(path: &str) -> PathBuf {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

    let source: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let needed = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return PathBuf::from(path);
    }
    let mut expanded = vec![0u16; needed as usize];
    let written = unsafe {
        ExpandEnvironmentStringsW(
            source.as_ptr(),
            expanded.as_mut_ptr(),
            expanded.len() as u32,
        )
    };
    if written == 0 || written as usize > expanded.len() {
        return PathBuf::from(path);
    }
    // The count includes the terminator.
    expanded.truncate(written.saturating_sub(1) as usize);
    PathBuf::from(OsString::from_wide(&expanded))
}

/// `FLogGetLogFilename` (`filesys_log.cpp:964`), whole: the name a log is
/// actually opened under.
///
/// `requested` is `/L=`'s argument, or the name a dialog was given, or `None`
/// for the automatic name — and `None` is not the same as passing
/// `LogDefaultName`, because only an *absolute* request escapes the log
/// directory. A relative `/L=out.log` lands in `LogDefaultPath` exactly as the
/// default name does, which is the part that surprises people: the file is
/// not next to the shortcut that asked for it.
pub fn log_file_name(requested: Option<&str>, settings: &Settings, ctx: &LogContext) -> PathBuf {
    let now = Civil::now(crate::log::local_offset());
    let (dir, leaf) = match requested {
        Some(name) if Path::new(name).is_absolute() => {
            let path = Path::new(name);
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let leaf = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            (dir, leaf)
        }
        Some(name) => (term_log_dir(settings), name.to_string()),
        None => (term_log_dir(settings), settings.log_default_name.clone()),
    };
    // An empty expansion joins nothing, so the "name" is the directory — which
    // is upstream's answer too, and fails at the open with a message about a
    // directory rather than about a template.
    dir.join(expand_name(&leaf, ctx, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08-09 12:34:56.789 UTC, a Sunday.
    fn when() -> Civil {
        Civil::from_unix(1_786_278_896, 789)
    }

    #[test]
    fn the_clock_agrees_with_the_calendar() {
        let t = when();
        assert_eq!((t.year, t.month, t.day), (2026, 8, 9));
        assert_eq!((t.hour, t.minute, t.second), (12, 34, 56));
        assert_eq!(t.weekday, 0, "a Sunday");
        assert_eq!(
            day_of_year(t),
            220,
            "`tm_yday` counts from zero; `%j` prints one more"
        );
    }

    /// The timestamp expander is upstream's own, and its `default` arm is the
    /// part worth pinning: a conversion it does not implement comes back as
    /// text rather than being dropped or erroring.
    #[test]
    fn an_unknown_conversion_survives_a_timestamp_as_text() {
        let t = when();
        assert_eq!(
            timestamp_format("%Y-%m-%d %H:%M:%S.%N", t),
            "2026-08-09 12:34:56.789"
        );
        assert_eq!(
            timestamp_format("%a %b %e %w %y %%", t),
            "Sun Aug  9 0 26 %"
        );
        // Ten conversions the *file name* path accepts and this one does not.
        assert_eq!(timestamp_format("%A/%j/%p/%Z", t), "%A/%j/%p/%Z");
        // A trailing `%` is kept, because the arm that would eat it does not
        // consume anything.
        assert_eq!(timestamp_format("end%", t), "end%");
    }

    /// ...and the file name's validator, whose two asymmetries are what the
    /// `p - i == 2` dead branches leave behind.
    #[test]
    fn the_file_name_validator_drops_a_code_and_sometimes_keeps_its_letter() {
        assert_eq!(delete_invalid_codes("%Y-%m-%d"), "%Y-%m-%d");
        // `N` is not in Visual Studio 2005's table, so the milliseconds in the
        // shipped `LogTimestampFormat` vanish from a file name.
        assert_eq!(delete_invalid_codes("%Y%N"), "%Y");
        assert_eq!(delete_invalid_codes("a%qb"), "ab", "both characters go");
        assert_eq!(
            delete_invalid_codes("a%#qb"),
            "aqb",
            "the modifier goes and the letter stays"
        );
        assert_eq!(delete_invalid_codes("%#d"), "%#d");
        assert_eq!(delete_invalid_codes("x%"), "x", "a trailing % is dropped");
        assert_eq!(delete_invalid_codes("%e"), "", "and %e is not in the table");
    }

    #[test]
    fn a_name_expands_its_date_through_the_c_library() {
        let ctx = LogContext::default();
        assert_eq!(expand_name("%Y%m%d.log", &ctx, when()), "20260809.log");
        // `%j` is the pair to the test above: a file name has it, a timestamp
        // does not.
        assert_eq!(expand_name("%j.log", &ctx, when()), "221.log");
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_msvcs_no_padding_modifier() {
        assert_eq!(
            expand_name("%#d.log", &LogContext::default(), when()),
            "9.log"
        );
    }

    #[test]
    fn the_connection_escapes_expand_or_vanish() {
        let ctx = LogContext {
            host: Some("router1".into()),
            tcp_port: Some(2222),
            user: Some("nata".into()),
        };
        assert_eq!(
            expand_name("&h-&p-&u.log", &ctx, when()),
            "router1-2222-nata.log"
        );

        // Nothing open: each escape expands to nothing rather than to a
        // placeholder, so the name is short rather than wrong.
        let empty = LogContext::default();
        assert_eq!(expand_name("&h-&p-&u.log", &empty, when()), "--.log");

        // There is no escape for a literal `&`: the first is dropped and the
        // second is read again.
        assert_eq!(expand_name("&&h.log", &ctx, when()), "router1.log");
        assert_eq!(expand_name("&x.log", &ctx, when()), "x.log");
        assert_eq!(expand_name("a&", &ctx, when()), "a&", "the last one stays");
    }

    /// An IPv6 host is the case upstream's comment names, and it is why the
    /// sweep exists at all.
    #[test]
    fn characters_a_file_name_cannot_hold_become_underscores() {
        let ctx = LogContext {
            host: Some("fe80::1".into()),
            ..LogContext::default()
        };
        assert_eq!(expand_name("&h.log", &ctx, when()), "fe80__1.log");
        assert_eq!(expand_name("a/b*c?.log", &ctx, when()), "b_c_.log");
    }

    #[test]
    fn a_directory_in_the_template_is_stripped_and_not_created() {
        let ctx = LogContext::default();
        // `FLogGetLogFilenameBase` takes the component after the last
        // separator, so a template cannot put itself in a subdirectory.
        assert_eq!(expand_name("logs/x.log", &ctx, when()), "x.log");
    }

    #[test]
    fn only_an_absolute_request_escapes_the_log_directory() {
        let log_dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let settings = Settings {
            log_default_path: log_dir.path().to_string_lossy().into_owned(),
            log_default_name: String::from("&h.log"),
            ..Settings::default()
        };
        let ctx = LogContext {
            host: Some("box".into()),
            ..LogContext::default()
        };

        assert_eq!(
            log_file_name(None, &settings, &ctx),
            log_dir.path().join("box.log")
        );
        // A relative `/L=` is *not* relative to the working directory.
        assert_eq!(
            log_file_name(Some("out.log"), &settings, &ctx),
            log_dir.path().join("out.log")
        );
        let requested = elsewhere.path().join("&h.log");
        assert_eq!(
            log_file_name(Some(&requested.to_string_lossy()), &settings, &ctx),
            elsewhere.path().join("box.log")
        );
    }

    /// The transfer directory decides where a log goes, which is not a
    /// relationship anybody would guess at.
    #[test]
    fn the_transfer_directory_is_the_second_answer_and_only_if_it_exists() {
        let scratch = tempfile::tempdir().unwrap();
        let transfer = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            transfer_dir: scratch
                .path()
                .join("not-here")
                .to_string_lossy()
                .into_owned(),
            ..Settings::default()
        };
        let fallback = term_log_dir(&settings);
        assert!(
            fallback.ends_with("sterna"),
            "a directory that does not exist is skipped: {fallback:?}"
        );

        settings.transfer_dir = transfer.path().to_string_lossy().into_owned();
        assert_eq!(term_log_dir(&settings), transfer.path());

        settings.log_default_path = logs.path().to_string_lossy().into_owned();
        assert_eq!(
            term_log_dir(&settings),
            logs.path(),
            "and it is the third answer once the log path is set"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_transfer_directory_expands_windows_environment_variables() {
        let temp = std::env::var_os("TEMP").expect("Windows has TEMP");
        let settings = Settings {
            transfer_dir: String::from("%TEMP%"),
            ..Settings::default()
        };
        assert_eq!(term_log_dir(&settings), PathBuf::from(temp));
    }
}
