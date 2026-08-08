//! Session logging — the eight commands that drive the terminal's log file.
//!
//! The log belongs to the *terminal*, not to the macro: upstream sends each of
//! these over DDE to `filesys_log.cpp`, which is the same log `File > Log`
//! opens. Six of the eight are therefore the thin `TTLCommCmd*` shapes, with
//! all their behaviour on the far side of the conversation, and they carry the
//! link check that comes with `SendCmnd` — a `logclose` with no terminal is
//! `ErrLinkFirst` even though its own body says nothing about a link.
//!
//! Two are not thin, and both are worth knowing:
//!
//! - **`logopen` never tests the error from its three mandatory arguments.**
//!   It is reproduced; see the command.
//! - **`logrotate` has no end-of-line check at all**, so anything written
//!   after its arguments is ignored rather than reported.
//!
//! And one answer is inverted: `logopen` writes **0 for success**, where every
//! other command here that reports at all writes 1.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::{LogClock, LogOpen, LogRotate, ScriptHost};
use crate::interp::Interp;
use crate::rsv::Rsv;
use crate::strcmds::scan_int;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn log_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::LogOpen => self.cmd_log_open(host),
            Rsv::LogClose => self.comm_cmd(host).and_then(|()| host.log_close()),
            Rsv::LogPause => self.comm_cmd(host).and_then(|()| host.log_pause(true)),
            Rsv::LogStart => self.comm_cmd(host).and_then(|()| host.log_pause(false)),
            Rsv::LogWrite => self.cmd_log_write(host),
            Rsv::LogInfo => self.cmd_log_info(host),
            Rsv::LogRotate => self.cmd_log_rotate(host),
            Rsv::LogAutoClose => self.cmd_log_auto_close(host),
            _ => return None,
        })
    }

    /// `logopen <filename> <binary> <append> [<plain text> [<timestamp>
    /// [<hide dialog> [<include screen buffer> [<timestamp type>]]]]]`
    /// → 0 on success, 1 on failure.
    ///
    /// **The three mandatory arguments' error is discarded** (`ttl.cpp:3243`).
    /// `TTLLogOpen` accumulates into a sticky `Err` the way every command here
    /// does, but its first test of that variable is *after* the fourth
    /// argument, and the label the optional arguments jump to checks only that
    /// the filename is non-empty and that nothing is left on the line — then
    /// `Err = GetTTParam(...)` overwrites whatever was in it. So `logopen 'f'
    /// 1`, one argument short of the documented three, opens a log with
    /// `append` reading as the 0 its array was initialised with.
    ///
    /// Reproduced rather than corrected, and it is the one place in this file
    /// where that was a close call. A macro that has been opening a log with
    /// two arguments has been working, and the flags it silently gets are the
    /// documented defaults; making it a syntax error would break that script
    /// to no one's benefit.
    fn cmd_log_open(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let mut err = None;
        let path = self.sticky_str(&mut err);
        let binary = self.sticky_int(&mut err);
        let append = self.sticky_int(&mut err);

        // plain text, timestamp, hide dialog, include screen buffer — each
        // optional, each ending the list if it is absent.
        let mut flags = [0i32; 4];
        let mut clock = LogClock::Local;
        'options: {
            for slot in flags.iter_mut() {
                if !self.lx.parameter_given() {
                    break 'options;
                }
                *slot = self.sticky_int(&mut err);
                if let Some(e) = err {
                    return Err(e);
                }
            }
            if !self.lx.parameter_given() {
                break 'options;
            }
            let code = self.sticky_int(&mut err);
            if let Some(e) = err {
                return Err(e);
            }
            // The one argument with a range, and it is checked before the
            // end-of-line test rather than after.
            clock = LogClock::from_code(code).ok_or(TtlError::Syntax)?;
        }

        if path.is_empty() || self.lx.first_char() != 0 {
            return Err(TtlError::Syntax);
        }
        // The link check comes from `GetTTParam` (`ttmdde.c:1062`) rather than
        // from the command, so it is last here rather than before the
        // arguments as `SendCmnd`'s is.
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }

        let opened = host.log_open(&LogOpen {
            path: &path,
            binary: binary != 0,
            append: append != 0,
            plain_text: flags[0] != 0,
            timestamp: flags[1] != 0,
            hide_dialog: flags[2] != 0,
            include_screen: flags[3] != 0,
            timestamp_type: clock,
        })?;
        // Inverted, and upstream's: the terminal answers the character `1` for
        // success and `TTLLogOpen` maps that to `result` 0.
        self.set_result(i32::from(!opened));
        Ok(())
    }

    /// `logwrite <string>` — `TTLCommCmdFile`, so an empty string is a syntax
    /// error and the link check runs after the argument is parsed.
    fn cmd_log_write(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let s = self.comm_cmd_file(host)?;
        host.log_write(&s)
    }

    /// `logautoclosemode <flag>` — `TTLCommCmdBin`, which is `TTLCommCmdInt`
    /// under another name: the terminal reads the number as a boolean.
    fn cmd_log_auto_close(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let v = self.comm_cmd_int(host)?;
        host.log_auto_close(v != 0)
    }

    /// `loginfo <strvar>` → the flags the log was opened with, or **-1** when
    /// the terminal is not logging.
    ///
    /// The flag word is the five booleans `logopen` took, in the order it took
    /// them; the timestamp *type* is not in it and neither is whether the log
    /// is paused. Upstream builds it in `FLogInfo` and ships it as one
    /// character, `'0' + flags`, with the filename in the bytes after — which
    /// is also why it can carry -1 without a sign.
    fn cmd_log_info(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;

        match host.log_info()? {
            Some(info) => {
                let flags = i32::from(info.binary)
                    | i32::from(info.append) << 1
                    | i32::from(info.plain_text) << 2
                    | i32::from(info.timestamp) << 3
                    | i32::from(info.hide_dialog) << 4;
                self.set_result(flags);
                self.vars.set_str(target, &info.path);
            }
            None => {
                self.set_result(-1);
                // Upstream writes the empty string after the flag byte, so the
                // variable is cleared rather than left holding an old name.
                self.vars.set_str(target, b"");
            }
        }
        Ok(())
    }

    /// `logrotate 'size' '<size>'` / `logrotate 'rotate' <count>` /
    /// `logrotate 'halt'`.
    ///
    /// Three quirks, all upstream's. The keyword is matched with `strcmp`, so
    /// it is the one enumerated argument in the family that is **case
    /// sensitive**. The size suffix is an uppercase `K` or `M` and nothing
    /// else — a lowercase `k` is a syntax error. And there is **no
    /// end-of-line check**, so `logrotate 'halt' whatever` runs and ignores
    /// the tail where every neighbouring command would have refused it.
    fn cmd_log_rotate(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let what = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        // The link check is before the keyword rather than after, which is the
        // other way round from `TTLCommCmd*` and visible: `logrotate 'nonsense'`
        // with no terminal is `ErrLinkFirst`, not `ErrSyntax`.
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }

        let how = match what.as_slice() {
            b"size" if self.lx.parameter_given() => {
                let arg = expr::get_str_val(&mut self.lx, &mut self.vars)?;
                LogRotate::Size(parse_size(&arg).ok_or(TtlError::Syntax)?)
            }
            b"rotate" if self.lx.parameter_given() => {
                let n = expr::get_int_val(&mut self.lx, &mut self.vars)?;
                if n <= 0 {
                    return Err(TtlError::Syntax);
                }
                LogRotate::Keep(n)
            }
            b"halt" => LogRotate::Halt,
            // Including `size` and `rotate` with nothing after them: upstream
            // seeds `Err` with `ErrSyntax` before the chain and only the arms
            // that got their argument clear it.
            _ => return Err(TtlError::Syntax),
        };
        host.log_rotate(how)
    }

    /// `GetStrVal` with upstream's sticky error: once one read has failed the
    /// rest are skipped, and the caller decides whether to look.
    fn sticky_str(&mut self, err: &mut Option<TtlError>) -> Vec<u8> {
        if err.is_some() {
            return Vec::new();
        }
        expr::get_str_val(&mut self.lx, &mut self.vars).unwrap_or_else(|e| {
            *err = Some(e);
            Vec::new()
        })
    }

    /// `GetIntVal`, likewise. Zero is what the failed read leaves behind,
    /// because upstream's destination array was `{ 0 }`.
    fn sticky_int(&mut self, err: &mut Option<TtlError>) -> i32 {
        if err.is_some() {
            return 0;
        }
        expr::get_int_val(&mut self.lx, &mut self.vars).unwrap_or_else(|e| {
            *err = Some(e);
            0
        })
    }
}

/// `logrotate 'size'`'s argument: a decimal count with an optional `K` or `M`.
///
/// **An empty string reads one byte before the buffer upstream**
/// (`ttl.cpp:3179`): `Str2[len-1]` with `len` zero, handed to `isdigit`, which
/// is also passed a signed `char` and so has the same trouble with a byte
/// above 0x7F that `strtrim` does. Neither is reproducible in safe Rust and
/// neither has an answer worth being faithful to, so an empty argument is a
/// syntax error here — which is what the non-digit arm would have said anyway.
///
/// The multiplication is upstream's `atoi(...) * num` in an `int` and wraps
/// with it, so a size that overflows lands wherever upstream's lands and is
/// then usually caught by the 128-byte floor.
fn parse_size(arg: &[u8]) -> Option<i32> {
    let (digits, unit) = match arg.last()? {
        b'K' => (&arg[..arg.len() - 1], 1024),
        b'M' => (&arg[..arg.len() - 1], 1024 * 1024),
        b if b.is_ascii_digit() => (arg, 1),
        _ => return None,
    };
    // `atoi` of a string with no digits in it is 0, which the floor rejects.
    let size = scan_int(digits, 10).unwrap_or(0).wrapping_mul(unit);
    (size >= 128).then_some(size)
}

#[cfg(test)]
mod tests {
    use crate::host::{LogClock, LogInfo, RecordingHost};
    use crate::interp::Interp;
    use crate::TtlError;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        run_with(&mut host, src);
        host
    }

    fn asked(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        assert_eq!(h.logs.len(), 1, "{:?}", h.logs);
        h.logs[0].clone()
    }

    fn err_of(src: &str) -> TtlError {
        let h = run(src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn every_logging_command_wants_a_terminal() {
        for src in [
            "logopen 'f' 0 0",
            "logclose",
            "logpause",
            "logstart",
            "logwrite 'x'",
            "loginfo s",
            "logrotate 'halt'",
            "logautoclosemode 1",
        ] {
            let mut host = RecordingHost::new();
            run_with(&mut host, src);
            assert_eq!(
                host.errors.first().map(|e| e.0),
                Some(TtlError::LinkFirst),
                "{src}"
            );
        }
    }

    #[test]
    fn the_five_optional_flags_default_off_and_arrive_in_order() {
        assert_eq!(
            asked("logopen 'f' 0 0"),
            "logopen \"f\" binary=0 append=0 plain=0 ts=0 hide=0 screen=0 clock=Local"
        );
        assert_eq!(
            asked("logopen 'f' 1 1 1 1 1 1 2"),
            "logopen \"f\" binary=1 append=1 plain=1 ts=1 hide=1 screen=1 clock=ElapsedLog"
        );
    }

    #[test]
    fn logopen_reports_success_as_zero() {
        let h = run("logopen 'f' 0 0\ndispstr result");
        assert_eq!(h.output, b"0");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.log_open_fails = true;
        run_with(&mut host, "logopen 'f' 0 0\ndispstr result");
        assert_eq!(host.output, b"1");
    }

    #[test]
    fn a_timestamp_type_outside_zero_to_three_is_a_syntax_error() {
        assert_eq!(err_of("logopen 'f' 0 0 0 1 0 0 4"), TtlError::Syntax);
        assert_eq!(err_of("logopen 'f' 0 0 0 1 0 0 (-1)"), TtlError::Syntax);
        assert_eq!(
            asked("logopen 'f' 0 0 0 1 0 0 3"),
            "logopen \"f\" binary=0 append=0 plain=0 ts=1 hide=0 screen=0 clock=ElapsedConnection"
        );
    }

    #[test]
    fn logopen_swallows_the_error_from_its_mandatory_arguments() {
        // Upstream's, and the reason is in the comment on `cmd_log_open`: one
        // argument short opens a log rather than reporting anything, with the
        // missing flags reading as the documented defaults.
        assert_eq!(
            asked("logopen 'f' 1"),
            "logopen \"f\" binary=1 append=0 plain=0 ts=0 hide=0 screen=0 clock=Local"
        );
        assert_eq!(
            asked("logopen 'f'"),
            "logopen \"f\" binary=0 append=0 plain=0 ts=0 hide=0 screen=0 clock=Local"
        );
        // The two checks that do survive: a name, and nothing left over.
        assert_eq!(err_of("logopen ''"), TtlError::Syntax);
        assert_eq!(err_of("logopen 'f' 0 0 0 0 0 0 0 9"), TtlError::Syntax);
    }

    #[test]
    fn logwrite_refuses_an_empty_string() {
        assert_eq!(asked("logwrite 'hello'"), r#"logwrite "hello""#);
        assert_eq!(err_of("logwrite ''"), TtlError::Syntax);
    }

    #[test]
    fn the_thin_ones_take_nothing_and_report_nothing() {
        let h = run("logclose\nlogpause\nlogstart\nlogautoclosemode 1\nlogautoclosemode 0");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            h.logs,
            vec![
                "logclose",
                "logpause",
                "logstart",
                "logautoclosemode 1",
                "logautoclosemode 0"
            ]
        );
        assert_eq!(err_of("logclose 1"), TtlError::Syntax);
        assert_eq!(err_of("logautoclosemode"), TtlError::Syntax);
    }

    #[test]
    fn loginfo_is_a_flag_word_and_minus_one_when_nothing_is_open() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.log_info = Some(LogInfo {
            path: b"/tmp/s.log".to_vec(),
            binary: true,
            append: false,
            plain_text: true,
            timestamp: false,
            hide_dialog: true,
        });
        run_with(&mut host, "loginfo name\ndispstr result '|' name");
        assert_eq!(host.output, b"21|/tmp/s.log", "1 | 4 | 16");

        let h = run("name = 'stale'\nloginfo name\ndispstr result '|' name");
        assert_eq!(h.output, b"-1|", "and the name is cleared, not left");
        // A literal where a variable name belongs is a syntax error, not a
        // type mismatch: `GetStrVar` needs an identifier before it can look
        // anything up.
        assert_eq!(err_of("loginfo 'not a variable'"), TtlError::Syntax);
    }

    // ---- logrotate ----

    #[test]
    fn logrotate_takes_a_size_with_an_optional_unit() {
        assert_eq!(asked("logrotate 'size' '128'"), "logrotate Size(128)");
        assert_eq!(asked("logrotate 'size' '32K'"), "logrotate Size(32768)");
        assert_eq!(asked("logrotate 'size' '1M'"), "logrotate Size(1048576)");
        assert_eq!(err_of("logrotate 'size' '127'"), TtlError::Syntax);
        assert_eq!(err_of("logrotate 'size' '32k'"), TtlError::Syntax, "K only");
        assert_eq!(err_of("logrotate 'size' ''"), TtlError::Syntax);
        assert_eq!(err_of("logrotate 'size'"), TtlError::Syntax);
    }

    #[test]
    fn logrotate_counts_generations_and_halts() {
        assert_eq!(asked("logrotate 'rotate' 3"), "logrotate Keep(3)");
        assert_eq!(asked("logrotate 'halt'"), "logrotate Halt");
        assert_eq!(err_of("logrotate 'rotate' 0"), TtlError::Syntax);
        assert_eq!(err_of("logrotate 'rotate'"), TtlError::Syntax);
        assert_eq!(
            err_of("logrotate 'Halt'"),
            TtlError::Syntax,
            "strcmp, not _stricmp"
        );
    }

    #[test]
    fn logrotate_ignores_whatever_follows_its_arguments() {
        // No `GetFirstChar` check, alone in this file.
        assert_eq!(asked("logrotate 'halt' 'and then some'"), "logrotate Halt");
        assert_eq!(asked("logrotate 'rotate' 2 99"), "logrotate Keep(2)");
    }

    #[test]
    fn a_host_with_no_logging_answers_unknown_command() {
        struct NoLog(Vec<TtlError>);
        impl crate::host::ScriptHost for NoLog {
            fn linked(&mut self) -> bool {
                true
            }
            fn error(&mut self, report: &crate::host::ErrorReport<'_>) -> bool {
                self.0.push(report.error);
                true
            }
        }

        for src in [
            "logopen 'f' 0 0",
            "logclose",
            "logpause",
            "logstart",
            "logwrite 'x'",
            "loginfo s",
            "logrotate 'halt'",
            "logautoclosemode 1",
        ] {
            let mut host = NoLog(Vec::new());
            let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
            it.run(&mut host);
            assert_eq!(host.0, vec![TtlError::NotSupported], "{src}");
        }
    }

    #[test]
    fn the_clock_codes_are_the_documented_ones() {
        assert_eq!(LogClock::from_code(0), Some(LogClock::Local));
        assert_eq!(LogClock::from_code(1), Some(LogClock::Utc));
        assert_eq!(LogClock::from_code(2), Some(LogClock::ElapsedLog));
        assert_eq!(LogClock::from_code(3), Some(LogClock::ElapsedConnection));
        assert_eq!(LogClock::from_code(4), None);
    }
}
