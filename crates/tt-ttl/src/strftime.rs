//! `strftime`, because there isn't one — and neither a calendar nor a time
//! zone — in the standard library, and this crate has no dependencies.
//!
//! It is the default behind [`ScriptHost::strftime`], which `getdate`,
//! `gettime` and `filestat` all go through. A frontend with a date library is
//! expected to override that method; what is here has to be right anyway,
//! because it is what a caller with no such library gets.
//!
//! **The conversions are MSVC's, not glibc's**, and the two differ where it is
//! most visible: `%c` in the C locale is `08/09/26 14:30:00` on MSVC and
//! `Sun Aug  9 14:30:00 2026` on glibc. Upstream is MSVC, a macro that prints
//! `%c` was written against MSVC, so MSVC is what this produces. The `#` flag
//! is MSVC's too — `%#d` drops the leading zero, `%#c` is the long form — and
//! it has no equivalent anywhere else.
//!
//! Only the twenty-three conversions `isInvalidStrftimeCharW`
//! (`ttlib_static_cpp.cpp:1894`) lets through are implemented, because a
//! format holding any other one never reaches `strftime`: `getdate` rejects it
//! with `result` 2 first. So `%e`, `%F`, `%T` and `%s` are *not* omissions —
//! reproducing them would be this port accepting a format Tera Term refuses.
//!
//! [`ScriptHost::strftime`]: crate::ScriptHost::strftime

/// A broken-down time, `struct tm`'s useful half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tm {
    pub year: i64,
    /// 1-12.
    pub month: i64,
    /// 1-31.
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    /// 0-6, Sunday first.
    pub wday: i64,
    /// 0-365.
    pub yday: i64,
}

/// Split a Unix timestamp into a civil date and a clock, in UTC.
///
/// Hinnant's `civil_from_days`: shift the epoch to March 1st of year 0 so the
/// leap day lands at the end of the era and the month arithmetic becomes exact
/// integer division. Correct for any date the proleptic Gregorian calendar
/// covers, which is more than a filesystem will produce.
pub fn civil_from_unix(unix_secs: i64) -> Tm {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    // 1970-01-01 was a Thursday, so shifting by 4 puts Sunday at 0.
    let wday = (days + 4).rem_euclid(7);

    Tm {
        year,
        month,
        day,
        hour: secs / 3600,
        minute: (secs / 60) % 60,
        second: secs % 60,
        wday,
        yday: day_of_year(year, month, day),
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn day_of_year(year: i64, month: i64, day: i64) -> i64 {
    const CUMULATIVE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut n = CUMULATIVE[(month - 1).clamp(0, 11) as usize] + day - 1;
    if month > 2 && is_leap(year) {
        n += 1;
    }
    n
}

const DAY_ABBREV: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTH_ABBREV: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A fixed-offset zone: seconds east of UTC, and what `%Z` should print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub offset_secs: i64,
    pub name: String,
}

impl Zone {
    pub fn utc() -> Zone {
        Zone {
            offset_secs: 0,
            name: "UTC".into(),
        }
    }
}

/// A `TZ` value, as far as a fixed-offset reading of POSIX gets.
///
/// `std[±]hh[:mm[:ss]]` — a name of at least three characters and an offset
/// **west** of UTC, so `JST-9` is UTC+9 and `EST5` is UTC-5. That is the whole
/// of what can be honoured here: the `dst[offset],start,end` tail needs a
/// transition rule this crate has no way to apply, and an Olson name like
/// `Asia/Tokyo` needs the database.
///
/// Anything not understood is UTC, which is also what POSIX says an
/// unparseable `TZ` means — so a macro naming a zone this cannot do gets a
/// wrong time rather than an error, exactly as it would from a system with no
/// tz database installed. A host that can do better should override
/// [`ScriptHost::strftime`](crate::ScriptHost::strftime).
pub fn parse_tz(tz: &[u8]) -> Zone {
    let s = String::from_utf8_lossy(tz);
    let s = s.trim();
    if s.is_empty() {
        return Zone::utc();
    }
    // A leading ':' is the implementation-defined form; strip it and try.
    let s = s.strip_prefix(':').unwrap_or(s);

    let name_len = s
        .find(|c: char| c == '+' || c == '-' || c.is_ascii_digit())
        .unwrap_or(s.len());
    let name = &s[..name_len];
    if name.len() < 3 {
        return Zone::utc();
    }
    let rest = &s[name_len..];
    if rest.is_empty() {
        // `UTC`, `GMT` — a name with no offset is UTC by convention, and any
        // other bare name is a zone this cannot resolve, which is also UTC.
        return Zone {
            offset_secs: 0,
            name: name.to_string(),
        };
    }

    let (sign, digits) = match rest.as_bytes()[0] {
        b'-' => (-1, &rest[1..]),
        b'+' => (1, &rest[1..]),
        _ => (1, rest),
    };
    let mut parts = digits.split(':');
    let Some(h) = parts.next().and_then(|p| p.parse::<i64>().ok()) else {
        return Zone::utc();
    };
    let m = parts
        .next()
        .and_then(|p| p.parse::<i64>().ok())
        .unwrap_or(0);
    let sec = parts
        .next()
        .and_then(|p| p.parse::<i64>().ok())
        .unwrap_or(0);
    // POSIX counts the offset *west* of UTC, so the sign flips going in.
    Zone {
        offset_secs: -sign * (h * 3600 + m * 60 + sec),
        name: name.to_string(),
    }
}

/// `isInvalidStrftimeCharW` (`ttlib_static_cpp.cpp:1894`), inverted — `true`
/// when every conversion in the format is one Tera Term will pass on.
///
/// The set is `aAbBcdHIjmMpSUwWxXyYzZ%` with an optional MSVC `#` in front,
/// and a format ending in a bare `%` is invalid. Note what it does *not*
/// check: a `#` at the very end takes the `format[i+2] != 0` branch and then
/// fails on `#` itself, which is the same answer by a different route.
pub fn format_is_valid(format: &[u8]) -> bool {
    const VALID: &[u8] = b"aAbBcdHIjmMpSUwWxXyYzZ%";
    let mut i = 0;
    while i < format.len() {
        if format[i] != b'%' {
            i += 1;
            continue;
        }
        let Some(&next) = format.get(i + 1) else {
            // A format ending in `%` is rejected outright.
            return false;
        };
        let p = if next == b'#' && i + 2 < format.len() {
            i + 2
        } else {
            i + 1
        };
        if !VALID.contains(&format[p]) {
            return false;
        }
        i = p + 1;
    }
    true
}

/// Format a timestamp. `zone` shifts the clock and names itself to `%Z`.
///
/// The format is bytes and so is the answer: a TTL string is not required to
/// be UTF-8 and neither is a filename built out of one.
pub fn format(unix_secs: i64, format: &[u8], zone: &Zone) -> Vec<u8> {
    let tm = civil_from_unix(unix_secs + zone.offset_secs);
    let mut out = Vec::with_capacity(format.len() + 16);
    let mut i = 0;
    while i < format.len() {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        let Some(&next) = format.get(i + 1) else {
            out.push(b'%');
            break;
        };
        let (bare, conv, width) = if next == b'#' && i + 2 < format.len() {
            (true, format[i + 2], i + 3)
        } else {
            (false, next, i + 2)
        };
        i = width;
        conversion(&mut out, &tm, zone, conv, bare);
    }
    out
}

/// One conversion. `bare` is MSVC's `#` flag, which drops leading zeros from
/// the numeric ones, asks for the long form of `%c` and `%x`, and is ignored
/// by the rest.
fn conversion(out: &mut Vec<u8>, tm: &Tm, zone: &Zone, conv: u8, bare: bool) {
    let wday = tm.wday.clamp(0, 6) as usize;
    let month = (tm.month - 1).clamp(0, 11) as usize;
    let mut num = |v: i64, w: usize| {
        let s = if bare {
            v.to_string()
        } else {
            format!("{v:0w$}")
        };
        out.extend_from_slice(s.as_bytes());
    };
    match conv {
        b'a' => out.extend_from_slice(DAY_ABBREV[wday].as_bytes()),
        b'A' => out.extend_from_slice(DAY_FULL[wday].as_bytes()),
        b'b' => out.extend_from_slice(MONTH_ABBREV[month].as_bytes()),
        b'B' => out.extend_from_slice(MONTH_FULL[month].as_bytes()),
        b'c' => {
            if bare {
                // MSVC's "long date and time": `Tuesday, March 14, 1995, 12:41:29`.
                let s = format!(
                    "{}, {} {}, {}, {:02}:{:02}:{:02}",
                    DAY_FULL[wday],
                    MONTH_FULL[month],
                    tm.day,
                    tm.year,
                    tm.hour,
                    tm.minute,
                    tm.second
                );
                out.extend_from_slice(s.as_bytes());
            } else {
                let s = format!(
                    "{:02}/{:02}/{:02} {:02}:{:02}:{:02}",
                    tm.month,
                    tm.day,
                    tm.year.rem_euclid(100),
                    tm.hour,
                    tm.minute,
                    tm.second
                );
                out.extend_from_slice(s.as_bytes());
            }
        }
        b'd' => num(tm.day, 2),
        b'H' => num(tm.hour, 2),
        b'I' => {
            let h = tm.hour % 12;
            num(if h == 0 { 12 } else { h }, 2);
        }
        b'j' => num(tm.yday + 1, 3),
        b'm' => num(tm.month, 2),
        b'M' => num(tm.minute, 2),
        b'p' => out.extend_from_slice(if tm.hour < 12 { b"AM" } else { b"PM" }),
        b'S' => num(tm.second, 2),
        // Sunday starts the week; the days before the first Sunday are week 0.
        b'U' => num((tm.yday + 7 - tm.wday) / 7, 2),
        b'w' => num(tm.wday, 1),
        b'W' => num((tm.yday + 7 - (tm.wday + 6) % 7) / 7, 2),
        b'x' => {
            if bare {
                let s = format!(
                    "{}, {} {}, {}",
                    DAY_FULL[wday], MONTH_FULL[month], tm.day, tm.year
                );
                out.extend_from_slice(s.as_bytes());
            } else {
                let s = format!(
                    "{:02}/{:02}/{:02}",
                    tm.month,
                    tm.day,
                    tm.year.rem_euclid(100)
                );
                out.extend_from_slice(s.as_bytes());
            }
        }
        // `#` is ignored on `%X`, which is why this one does not go through
        // `num` — MSVC lists it with `%a` and friends, not with `%H`.
        b'X' => {
            let s = format!("{:02}:{:02}:{:02}", tm.hour, tm.minute, tm.second);
            out.extend_from_slice(s.as_bytes());
        }
        b'y' => num(tm.year.rem_euclid(100), 2),
        b'Y' => num(tm.year, 4),
        b'z' => {
            let total = zone.offset_secs / 60;
            let s = format!(
                "{}{:02}{:02}",
                if total < 0 { '-' } else { '+' },
                total.abs() / 60,
                total.abs() % 60
            );
            out.extend_from_slice(s.as_bytes());
        }
        b'Z' => out.extend_from_slice(zone.name.as_bytes()),
        b'%' => out.push(b'%'),
        // Unreachable from `getdate`, which validates first. A caller that
        // formats without validating gets the conversion back untouched,
        // which is neither MSVC's nor glibc's — both are undefined here.
        other => {
            out.push(b'%');
            if bare {
                out.push(b'#');
            }
            out.push(other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(secs: i64, f: &str) -> String {
        String::from_utf8(format(secs, f.as_bytes(), &Zone::utc())).unwrap()
    }

    #[test]
    fn the_calendar_agrees_with_known_dates() {
        // 2026-08-09 is a Sunday; the epoch itself was a Thursday.
        assert_eq!(
            fmt(0, "%Y-%m-%d %H:%M:%S %A"),
            "1970-01-01 00:00:00 Thursday"
        );
        assert_eq!(fmt(1_786_233_600, "%Y-%m-%d %A"), "2026-08-09 Sunday");
        // A leap day, and the day-of-year either side of it.
        assert_eq!(fmt(1_709_164_800, "%Y-%m-%d %j"), "2024-02-29 060");
        assert_eq!(fmt(1_709_251_200, "%Y-%m-%d %j"), "2024-03-01 061");
        assert_eq!(fmt(1_677_628_800, "%Y-%m-%d %j"), "2023-03-01 060");
        // 2000 is a leap year and 1900 is not.
        assert_eq!(fmt(951_782_400, "%Y-%m-%d"), "2000-02-29");
        // Before the epoch, which `div_euclid` has to get right.
        assert_eq!(fmt(-1, "%Y-%m-%d %H:%M:%S"), "1969-12-31 23:59:59");
    }

    #[test]
    fn the_twelve_hour_clock_has_no_zero() {
        assert_eq!(fmt(0, "%I %p"), "12 AM");
        assert_eq!(fmt(11 * 3600, "%I %p"), "11 AM");
        assert_eq!(fmt(12 * 3600, "%I %p"), "12 PM");
        assert_eq!(fmt(13 * 3600, "%I %p"), "01 PM");
    }

    #[test]
    fn the_hash_flag_is_msvcs() {
        // The documentation's own example: `|%d|%#d|` on the 7th is `|07|7|`.
        let seventh = 1_786_060_800; // 2026-08-07
        assert_eq!(fmt(seventh, "|%d|%#d|"), "|07|7|");
        assert_eq!(fmt(seventh, "%#Y-%#m-%#d"), "2026-8-7");
        // ...and it is ignored by the ones MSVC says ignore it.
        assert_eq!(fmt(seventh, "%#a %#A %#p"), fmt(seventh, "%a %A %p"));
        assert_eq!(fmt(seventh, "%#X"), fmt(seventh, "%X"));
        // `%c` and `%x` take a long form instead.
        assert_eq!(fmt(seventh, "%c"), "08/07/26 00:00:00");
        assert_eq!(fmt(seventh, "%x"), "08/07/26");
        assert_eq!(fmt(seventh, "%#x"), "Friday, August 7, 2026");
        assert_eq!(fmt(seventh, "%#c"), "Friday, August 7, 2026, 00:00:00");
    }

    #[test]
    fn the_week_numbers_count_from_the_first_sunday_and_the_first_monday() {
        // 2023-01-01 was a Sunday, so `%U` reaches 1 on day 0 and `%W` does not.
        assert_eq!(fmt(1_672_531_200, "%U %W %w"), "01 00 0");
        // 2024-01-01 was a Monday: the other way round.
        assert_eq!(fmt(1_704_067_200, "%U %W %w"), "00 01 1");
    }

    #[test]
    fn a_zone_shifts_the_clock_and_names_itself() {
        let jst = parse_tz(b"JST-9");
        assert_eq!(jst.offset_secs, 9 * 3600, "POSIX counts west, so -9 is +9");
        assert_eq!(
            String::from_utf8(format(0, b"%Y-%m-%d %H:%M %z %Z", &jst)).unwrap(),
            "1970-01-01 09:00 +0900 JST"
        );
        let est = parse_tz(b"EST5");
        assert_eq!(est.offset_secs, -5 * 3600);
        assert_eq!(
            String::from_utf8(format(0, b"%H:%M %z", &est)).unwrap(),
            "19:00 -0500"
        );
        // Half-hour and named-but-offsetless zones.
        assert_eq!(parse_tz(b"IST-5:30").offset_secs, 5 * 3600 + 1800);
        assert_eq!(
            parse_tz(b"GMT"),
            Zone {
                offset_secs: 0,
                name: "GMT".into()
            }
        );
        // A zone this cannot resolve is UTC, which is what an unparseable
        // POSIX `TZ` means as well.
        assert_eq!(parse_tz(b"Asia/Tokyo").offset_secs, 0);
        assert_eq!(parse_tz(b""), Zone::utc());
        assert_eq!(parse_tz(b"XX-9"), Zone::utc(), "a name under three letters");
    }

    #[test]
    fn the_validator_is_upstreams_list_and_nothing_else() {
        assert!(format_is_valid(b"%Y-%m-%d %H:%M:%S"));
        assert!(format_is_valid(b"plain text"));
        assert!(format_is_valid(b"100%% done"));
        assert!(format_is_valid(b"%#d %#c"));
        // Not on the list, however ordinary they look elsewhere.
        assert!(!format_is_valid(b"%F"));
        assert!(!format_is_valid(b"%T"));
        assert!(!format_is_valid(b"%e"));
        assert!(!format_is_valid(b"%s"));
        assert!(!format_is_valid(b"%n"));
        // A trailing `%`, and a trailing `%#`.
        assert!(!format_is_valid(b"ends in %"));
        assert!(!format_is_valid(b"%#"));
        // `%%d` is an escaped percent followed by a literal `d`.
        assert!(format_is_valid(b"%%d"));
        assert_eq!(fmt(0, "%%d"), "%d");
    }
}
