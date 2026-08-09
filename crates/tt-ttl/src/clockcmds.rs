//! The clock: `getdate`, `gettime`, `setdate`, `settime` and `uptime`.
//!
//! `getdate` and `gettime` are **one function upstream** (`TTLGetTime`,
//! `ttl.cpp:2746`) differing only in the format they fall back to, which the
//! documentation says outright: "the behavior of the getdate command specified
//! with the format equals the gettime command specified with the same format".
//!
//! Three things about the family are not obvious from reading it:
//!
//! - **Only the two-argument form touches `result`.** With no `<format>`,
//!   `set_result` is `FALSE` and `result` keeps whatever it held. A script
//!   that tests `result` after a bare `getdate` is reading the previous
//!   command's answer.
//! - **`strftime` returning 0 is reported as "too long"**, and it also returns
//!   0 for a format that produced nothing at all. So `gettime t ''` is
//!   `result` 1 and leaves the variable alone, which looks like a length error
//!   and is an empty one.
//! - **`setdate` and `settime` cannot fail and cannot report.** A string that
//!   does not parse returns success having done nothing, and `SetLocalTime`'s
//!   `BOOL` is discarded — so on an unelevated Windows, which is the ordinary
//!   case, the command silently does nothing there too. See
//!   [`ScriptHost::set_system_date`].
//!
//! And one that is a defect rather than a quirk, written up in `PLAN.md`:
//! **the `<timezone>` argument leaks.** Upstream applies it by putting it in
//! the process environment and puts the old value back on the way out — but
//! the `GetFirstChar()` check sits *between* the two, so `gettime t '%H' 'UTC'
//! junk` returns `ErrSyntax` with `TZ` still overwritten, and every later
//! `localtime` in the run is in the wrong zone. The zone is passed as an
//! argument here instead of going through the environment at all, so there is
//! nothing to leak.

use crate::error::TtlResult;
use crate::expr;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::rsv::Rsv;
use crate::strftime;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn clock_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::GetDate => self.cmd_get_time(host, b"%Y-%m-%d"),
            Rsv::GetTime => self.cmd_get_time(host, b"%H:%M:%S"),
            Rsv::SetDate => self.cmd_set_date(host),
            Rsv::SetTime => self.cmd_set_time(host),
            Rsv::Uptime => self.cmd_uptime(host),
            _ => return None,
        })
    }

    /// `getdate <strvar> [<format> [<timezone>]]` and its twin `gettime`,
    /// which differ only in `default_format`.
    ///
    /// `result` is 0 for a string that was stored, 1 for one that was not and
    /// 2 for a format holding a conversion `strftime` would not take — and it
    /// is left alone entirely when no format was given.
    ///
    /// The order is upstream's and shows: the format is validated **before**
    /// the timezone argument is even read, so `gettime t '%Q' junk` is
    /// `result` 2 rather than a syntax error.
    pub(crate) fn cmd_get_time(
        &mut self,
        host: &mut dyn ScriptHost,
        default_format: &[u8],
    ) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let mut tz = None;
        let (format, report) = if self.lx.parameter_given() {
            let f = expr::get_str_val(&mut self.lx, &mut self.vars)?;
            if !strftime::format_is_valid(&f) {
                self.set_result(2);
                return Ok(());
            }
            if self.lx.parameter_given() {
                tz = Some(expr::get_str_val(&mut self.lx, &mut self.vars)?);
            }
            (f, true)
        } else {
            (default_format.to_vec(), false)
        };
        self.end_of_line()?;

        let now = host.now_unix();
        match host.strftime(now, &format, tz.as_deref()) {
            Some(s) => {
                self.vars.set_str(target, &s);
                if report {
                    self.set_result(0);
                }
            }
            None => {
                if report {
                    self.set_result(1);
                }
            }
        }
        Ok(())
    }

    /// `setdate <date>` — `YYYY-MM-DD`, by fixed column and not by parsing.
    ///
    /// Upstream cuts the string at offsets 4, 7 and 10 and `sscanf`s `%u` out
    /// of each piece, so the separators are never checked: `1997/08/01` and
    /// `1997x08y01` are both accepted, and only the *positions* matter. A
    /// piece that does not start with a digit ends the command — silently,
    /// with no `result` and no error, having changed nothing.
    fn cmd_set_date(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let s = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let (Some(y), Some(m), Some(d)) = (field(&s, 0, 4), field(&s, 5, 7), field(&s, 8, 10))
        else {
            return Ok(());
        };
        host.set_system_date(y, m, d);
        Ok(())
    }

    /// `settime <time>` — `HH:MM:SS`, cut at 2, 5 and 8. Same shape as
    /// [`cmd_set_date`](Interp::cmd_set_date), same silence.
    fn cmd_set_time(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let s = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let (Some(h), Some(m), Some(sec)) = (field(&s, 0, 2), field(&s, 3, 5), field(&s, 6, 8))
        else {
            return Ok(());
        };
        host.set_system_time(h, m, sec);
        Ok(())
    }

    /// `uptime <intvar>` — milliseconds since boot.
    ///
    /// `GetTickCount`'s `DWORD` is assigned to an `int`, so this goes negative
    /// after 24.9 days and back through zero after 49.7. The documentation
    /// mentions the second and not the first; both are what the cast does.
    fn cmd_uptime(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let ms = host.uptime_ms().unwrap_or(0);
        self.vars.set_int(target, ms as u32 as i32);
        Ok(())
    }
}

/// `sscanf(&s[from], "%u")` after `s[to] = 0` — upstream's fixed-column read.
///
/// `%u` skips leading whitespace and takes an optional sign, so ` 8` and `+8`
/// are both eight; a piece with no digit at all is `None`, which ends the
/// command. A string shorter than the column is `None` for the same reason:
/// there is nothing there to read.
fn field(s: &[u8], from: usize, to: usize) -> Option<i32> {
    let end = to.min(s.len());
    let piece = s.get(from..end)?;
    let mut i = 0;
    while i < piece.len() && piece[i].is_ascii_whitespace() {
        i += 1;
    }
    let negative = match piece.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut val: i64 = 0;
    while i < piece.len() && piece[i].is_ascii_digit() {
        val = (val * 10 + i64::from(piece[i] - b'0')).min(i64::from(u32::MAX));
        i += 1;
    }
    if i == start {
        return None;
    }
    // `%u` into an `int`: the value wraps through unsigned on its way in.
    let val = if negative { val.wrapping_neg() } else { val };
    Some(val as u32 as i32)
}

#[cfg(test)]
mod tests {
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;

    /// 2026-08-09 12:34:56 UTC, a Sunday.
    const WHEN: i64 = 1_786_278_896;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn at(src: &str) -> String {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.now = Some(WHEN);
        run_with(&mut host, src);
        assert!(
            host.errors.is_empty(),
            "unexpected errors: {:?}",
            host.errors
        );
        String::from_utf8_lossy(&host.output).into_owned()
    }

    fn err_of(src: &str) -> TtlError {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.now = Some(WHEN);
        run_with(&mut host, src);
        assert_eq!(
            host.errors.len(),
            1,
            "expected one error: {:?}",
            host.errors
        );
        host.errors[0].0
    }

    #[test]
    fn getdate_and_gettime_differ_only_in_their_default_format() {
        assert_eq!(at("getdate d\ndispstr d"), "2026-08-09");
        assert_eq!(at("gettime t\ndispstr t"), "12:34:56");
        // With a format given they are the same command.
        assert_eq!(
            at("getdate a '%Y%m%d-%H%M%S'\ngettime b '%Y%m%d-%H%M%S'\ndispstr a '|' b"),
            "20260809-123456|20260809-123456"
        );
    }

    #[test]
    fn only_the_form_with_a_format_touches_result() {
        assert_eq!(at("result = 42\ngetdate d\ndispstr result"), "42");
        assert_eq!(at("result = 42\ngetdate d '%Y'\ndispstr result"), "0");
    }

    #[test]
    fn a_format_upstream_would_not_pass_on_is_result_two() {
        assert_eq!(at("getdate d '%F'\ndispstr result"), "2");
        assert_eq!(at("getdate d 'ends in %'\ndispstr result"), "2");
        // ...and the variable is not written.
        assert_eq!(at("d = 'kept'\ngetdate d '%F'\ndispstr d"), "kept");
        // The format is checked before the rest of the line is even looked at.
        assert_eq!(at("getdate d '%F' junk\ndispstr result"), "2");
    }

    #[test]
    fn an_empty_result_is_reported_as_a_length_error() {
        // `strftime` answers 0 for "did not fit" and for "produced nothing",
        // and the caller cannot tell them apart.
        assert_eq!(
            at("d = 'kept'\ngetdate d ''\ndispstr result '|' d"),
            "1|kept"
        );
    }

    #[test]
    fn a_timezone_shifts_the_clock_without_leaking_into_the_run() {
        assert_eq!(at("gettime t '%H:%M %Z' 'JST-9'\ndispstr t"), "21:34 JST");
        assert_eq!(at("gettime t '%H:%M' 'GMT'\ndispstr t"), "12:34");
        // Upstream would leave `TZ` set here; there is no `TZ` to leave.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.now = Some(WHEN);
        host.stop_on_error = false;
        run_with(
            &mut host,
            "gettime t '%H' 'JST-9' junk\ngettime u '%H'\ndispstr u",
        );
        assert_eq!(host.errors.first().map(|e| e.0), Some(TtlError::Syntax));
        assert_eq!(host.output, b"12", "and the next command is still UTC");
    }

    #[test]
    fn getdate_wants_a_variable_to_write_to() {
        assert_eq!(err_of("getdate 'literal'"), TtlError::Syntax);
        assert_eq!(err_of("getdate d '%Y' 'UTC' extra"), TtlError::Syntax);
    }

    #[test]
    fn setdate_and_settime_read_fixed_columns_and_ignore_the_separators() {
        let did = |src: &str| {
            let mut host = RecordingHost::new();
            host.linked = true;
            run_with(&mut host, src);
            assert!(host.errors.is_empty(), "{:?}", host.errors);
            host.clock_sets.clone()
        };
        assert_eq!(did("setdate '1997-08-01'"), vec!["setdate 1997-8-1"]);
        assert_eq!(did("settime '01:05:00'"), vec!["settime 1:5:0"]);
        // The separators are never checked — only the columns.
        assert_eq!(did("setdate '1997/08/01'"), vec!["setdate 1997-8-1"]);
        assert_eq!(did("settime '01x05y00'"), vec!["settime 1:5:0"]);
        // A piece with no digits ends it, silently and with nothing changed.
        assert_eq!(did("setdate 'not a date'"), Vec::<String>::new());
        assert_eq!(did("setdate ''"), Vec::<String>::new());
        assert_eq!(did("settime '12'"), Vec::<String>::new());
    }

    #[test]
    fn uptime_is_milliseconds_and_wraps_the_way_a_dword_in_an_int_does() {
        let ms = |v: u64| {
            let mut host = RecordingHost::new();
            host.linked = true;
            host.uptime_ms = Some(v);
            run_with(&mut host, "uptime u\ndispstr u");
            String::from_utf8_lossy(&host.output).into_owned()
        };
        assert_eq!(ms(1_000), "1000");
        // 24.9 days: the top bit is set and the `int` is negative.
        assert_eq!(ms(0x8000_0000), "-2147483648");
        // 49.7 days: back through zero, which the documentation warns about.
        assert_eq!(ms(0x1_0000_0000), "0");

        // A machine that cannot be asked stores 0 rather than failing.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.uptime_ms = None;
        run_with(&mut host, "uptime u\ndispstr u");
        assert_eq!(host.output, b"0");
        assert_eq!(err_of("uptime 'literal'"), TtlError::Syntax);
    }
}
