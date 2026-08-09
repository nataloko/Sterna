//! The `send` variants, the broadcasts, `scp`, and the two waits that are not
//! about bytes arriving.
//!
//! `send` itself is in [`conncmds`](crate::conncmds), with the argument list
//! all of these share; what is here is everything built on top of it.
//!
//! Three groups, and each is a different kind of "not this terminal":
//!
//! - **`sendtext` and `sendbinary` are `send` with the sniffing turned off.**
//!   All three build the identical buffer and hand it to a different DDE
//!   command; the *terminal* decides what to do with the bytes
//!   (`ttdde.c:1215`). `send` guesses — text if there is no control character
//!   below 0x20 other than CR/LF and the bytes are valid UTF-8, binary
//!   otherwise — and the two newer commands exist because guessing is
//!   sometimes wrong. See [`SendMode`] and [`looks_like_text`].
//! - **`sendbroadcast`, `sendmulticast` and `setmulticastname` address other
//!   windows.** Upstream that is a window message between `ttermpro.exe`
//!   instances; here it is a frontend that owns several sessions, so all three
//!   are host methods and the interpreter only parses.
//! - **`wait4all` reaches into other *macro* processes.** `ttmdde.c:856` walks
//!   a shared-memory table of every running `ttpmacro.exe` and snoops their
//!   receive buffers. There is no in-process equivalent short of the frontend
//!   knowing about all its sessions, which is where it goes.
//!
//! One thing is deliberately **not** reproduced. `GetBroadcastString`
//! (`ttl.cpp:4031`) escapes `0x00` as `0x01 0x01` and `0x01` as `0x01 0x02`
//! before handing the string to DDE, because a DDE string ends at its first
//! NUL. That is transport, not language: there is no DDE here, the bytes go
//! across as themselves, and reproducing the escape would put literal `0x01`
//! bytes into everybody's broadcast.

use std::time::Duration;

use crate::error::{TtlError, TtlResult};
use crate::expr::{self, Eval};
use crate::host::{ScriptHost, SendMode};
use crate::interp::Interp;
use crate::rsv::Rsv;

/// `MaxWait` — ten patterns, the same ceiling `wait` has.
const MAX_WAIT: usize = 10;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn send_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::SendText => self.cmd_send_mode(host, SendMode::Text),
            Rsv::SendBinary => self.cmd_send_mode(host, SendMode::Binary),
            Rsv::SendKCode => self.cmd_send_kcode(host),
            Rsv::SendBroadcast => self.cmd_send_broadcast(host, false),
            Rsv::SendlnBroadcast => self.cmd_send_broadcast(host, true),
            Rsv::SendMulticast => self.cmd_send_multicast(host, false),
            Rsv::SendlnMulticast => self.cmd_send_multicast(host, true),
            Rsv::SetMulticastName => self.cmd_set_multicast_name(host),
            Rsv::ScpSend => self.cmd_scp(host, true),
            Rsv::ScpRecv => self.cmd_scp(host, false),
            Rsv::Wait4all => self.cmd_wait4all(host),
            Rsv::WaitEvent => self.cmd_wait_event(host),
            _ => return None,
        })
    }

    /// `sendtext <data>...` / `sendbinary <data>...` — `send`'s arguments,
    /// `send`'s link check, and a mode the host is told about.
    fn cmd_send_mode(&mut self, host: &mut dyn ScriptHost, mode: SendMode) -> TtlResult<()> {
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        let bytes = self.param_strings()?;
        host.send(&bytes, mode)
    }

    /// `sendkcode <key code> <repeat count>`.
    ///
    /// Both arguments go to the terminal as four hex digits (`Word2HexStr`,
    /// `ttl.cpp:4163`) and come back through `HexStr2Word`, so each is
    /// **sixteen bits** and anything larger wraps silently: `sendkcode 65536 1`
    /// is `sendkcode 0 1`. Reproduced with the cast rather than with a range
    /// check, because a range check would turn a quiet script into a failing
    /// one.
    fn cmd_send_kcode(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let code = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let repeat = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;
        host.send_key_code(code as u16, repeat as u16)
    }

    /// `sendbroadcast <data>...` / `sendlnbroadcast <data>...`.
    ///
    /// The line ending is **CRLF**, not the bare CR `sendln` uses
    /// (`ttl.cpp:4074`). That is not a typo on either side: `sendln` goes
    /// through the terminal's own newline setting and a broadcast does not.
    fn cmd_send_broadcast(&mut self, host: &mut dyn ScriptHost, ln: bool) -> TtlResult<()> {
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        let text = self.broadcast_string(ln)?;
        host.send_broadcast(&text)
    }

    /// `sendmulticast <name> <data>...` / `sendlnmulticast <name> <data>...`.
    fn cmd_send_multicast(&mut self, host: &mut dyn ScriptHost, ln: bool) -> TtlResult<()> {
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let text = self.broadcast_string(ln)?;
        host.send_multicast(&name, &text)
    }

    /// `setmulticastname <multicastname>`.
    ///
    /// **No end-of-line check** — `ttl.cpp:4095` reads its one string and goes
    /// straight to `SendCmnd`, so `setmulticastname 'a' junk` is accepted. The
    /// link check is `SendCmnd`'s, which is why a command whose body never
    /// mentions the connection still needs one.
    fn cmd_set_multicast_name(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        host.set_multicast_name(&name)
    }

    /// `GetBroadcastString` — `send`'s argument list, joined into one string.
    ///
    /// Same shapes as [`param_strings`](Interp::param_strings): a string is
    /// its bytes, an integer is its low byte. The DDE escaping upstream adds
    /// here is transport and is left out; see the module note.
    fn broadcast_string(&mut self, ln: bool) -> TtlResult<Vec<u8>> {
        let mut out = self.param_strings()?;
        if ln {
            out.extend_from_slice(b"\r\n");
        }
        Ok(out)
    }

    /// `scpsend <filename> [<destination>]` and `scprecv`, which parse
    /// identically.
    ///
    /// The optional second argument is optional by **swallowing its error**
    /// (`ttl.cpp:5831`): whatever `GetStrVal` reports is discarded and the
    /// destination becomes empty. That is also how a *wrong* second argument
    /// gets through — `scpsend 'f' 3` reports no type mismatch, because the
    /// expression is consumed on the way to the error that is thrown away, so
    /// the end-of-line check finds nothing left and the file goes to the
    /// default destination.
    fn cmd_scp(&mut self, host: &mut dyn ScriptHost, send: bool) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        let dest = expr::get_str_val(&mut self.lx, &mut self.vars).unwrap_or_default();
        self.end_of_line()?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        host.scp(send, &path, &dest)
    }

    /// `wait4all <string1> [<string2> ...]` — up to ten, as `wait` takes.
    ///
    /// `result` is the index **this** terminal matched, counting from 1, or 0
    /// for a timeout — but the command does not return until every terminal
    /// that was running a macro when it started has matched something. The
    /// timeout is `timeout` seconds plus `mtimeout` milliseconds, as
    /// everywhere else.
    fn cmd_wait4all(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let mut patterns = Vec::new();
        for _ in 0..MAX_WAIT {
            match self.lx.string() {
                Ok(Some(s)) => patterns.push(s),
                Ok(None) => match expr::get_expression(&mut self.lx, &mut self.vars)? {
                    Some(Eval::Str(r)) => patterns.push(self.vars.str_at(r).to_vec()),
                    Some(_) => return Err(TtlError::TypeMismatch),
                    None => break,
                },
                Err(e) => return Err(e),
            }
        }
        self.end_of_line()?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        // As with `wait`, no strings at all is not an error and not a wait.
        if patterns.is_empty() {
            return Ok(());
        }
        let timeout = self.timeout();
        let found = host.wait_for_all(&patterns, timeout)?;
        self.set_result(found as i32);
        Ok(())
    }

    /// `waitevent <events>` — a bit set of 1 timeout, 2 unlink,
    /// 4 disconnection, 8 connection. `result` is the one that happened.
    ///
    /// **The tests are on the current state, not on a transition**
    /// (`ttmmain.cpp:609`), so `waitevent 4` on a connection that is already
    /// closed returns at once rather than waiting for it to close again. That
    /// reads as a bug and is what a script means by it.
    ///
    /// `WakeupCondition &= 15` — bits above the four are dropped, so
    /// `waitevent 16` waits for nothing and, with no timeout set, for ever.
    /// Upstream would sit in its message loop; here the poll is what
    /// [`cancelled`](ScriptHost::cancelled) exists for.
    fn cmd_wait_event(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let events = expr::get_int_val(&mut self.lx, &mut self.vars)? & 15;
        self.end_of_line()?;

        let deadline = self.timeout().map(|d| std::time::Instant::now() + d);
        loop {
            // The order is upstream's: `OnDdeEnd` is a separate handler from
            // `OnCommReady`, and inside the latter connect is tested first.
            if events & 2 != 0 && !host.linked() {
                self.set_result(2);
                return Ok(());
            }
            let ready = host.com_ready();
            if events & 8 != 0 && ready {
                self.set_result(8);
                return Ok(());
            }
            if events & 4 != 0 && !ready {
                self.set_result(4);
                return Ok(());
            }
            if let Some(d) = deadline {
                if std::time::Instant::now() >= d {
                    if events & 1 != 0 {
                        self.set_result(1);
                    }
                    return Ok(());
                }
            }
            if host.cancelled() {
                return Ok(());
            }
            host.sleep(Duration::from_millis(POLL_MS));
        }
    }

    /// `timeout` seconds plus `mtimeout` milliseconds, as a duration.
    fn timeout(&self) -> Option<Duration> {
        use crate::vars::{VarRef, VarType};
        let read = |name: &[u8]| match self.vars.find(name) {
            Some((id, VarType::Integer)) => self.vars.int_at(VarRef::Scalar(id)) as i64,
            _ => 0,
        };
        let total = read(b"timeout")
            .saturating_mul(1000)
            .saturating_add(read(b"mtimeout"));
        (total > 0).then(|| Duration::from_millis(total as u64))
    }
}

/// How often `waitevent` looks. Upstream's timer is `TIMEOUT_TIMER_MS`, which
/// is 100; this is finer because the connection state is polled here rather
/// than pushed, and a `waitevent` is not on any hot path.
const POLL_MS: u64 = 20;

#[cfg(test)]
mod tests {
    use crate::host::{looks_like_text, RecordingHost};
    use crate::interp::Interp;
    use crate::TtlError;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.com_ready = true;
        run_with(&mut host, src);
        host
    }

    fn did(src: &str) -> Vec<String> {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        h.sends.clone()
    }

    fn err_of(src: &str) -> TtlError {
        let h = run(src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn the_three_send_commands_build_the_same_bytes() {
        // `send`, `sendtext` and `sendbinary` differ only in the mode; the
        // argument list is `GetParamStrings` for all three.
        let h = run("send 'ab' 13\nsendtext 'ab' 13\nsendbinary 'ab' 13");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.sent, b"ab\rab\rab\r");
        assert_eq!(h.sends, vec!["Text \"ab\\r\"", "Binary \"ab\\r\""]);
    }

    #[test]
    fn sends_sniff_is_the_languages_and_not_the_hosts() {
        // No control byte below 0x20 but CR and LF, and valid UTF-8.
        assert!(looks_like_text(b"hello\r\nworld"));
        assert!(looks_like_text("caf\u{e9}".as_bytes()));
        assert!(looks_like_text(b""));
        // An escape sequence is binary, which is the point of the test —
        // `send 27 '[2J'` must reach the far end unrewritten.
        assert!(!looks_like_text(b"\x1b[2J"));
        assert!(!looks_like_text(b"\0"));
        assert!(!looks_like_text(b"\x07"));
        // Invalid UTF-8 does not survive the round trip upstream makes.
        assert!(!looks_like_text(b"\xff\xfe"));
        assert!(!looks_like_text(b"caf\xe9"));
    }

    #[test]
    fn every_send_variant_wants_a_terminal() {
        for src in [
            "sendtext 'x'",
            "sendbinary 'x'",
            "sendkcode 336 3",
            "sendbroadcast 'x'",
            "sendlnbroadcast 'x'",
            "sendmulticast 'n' 'x'",
            "sendlnmulticast 'n' 'x'",
            "setmulticastname 'n'",
            "scpsend 'f'",
            "scprecv 'f'",
            "wait4all 'x'",
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
    fn sendkcode_is_sixteen_bits_each_way() {
        assert_eq!(did("sendkcode 336 3"), vec!["kcode 336 x3"]);
        // Four hex digits in and four out, so this is `sendkcode 0 1`.
        assert_eq!(did("sendkcode 65536 1"), vec!["kcode 0 x1"]);
        assert_eq!(err_of("sendkcode 336"), TtlError::Syntax);
    }

    #[test]
    fn the_broadcasts_join_their_arguments_and_ln_adds_crlf() {
        assert_eq!(
            did("msg = 'cal'\nsendbroadcast 'hoge' 13 10 msg"),
            vec!["broadcast \"hoge\\r\\ncal\""]
        );
        // CRLF, where `sendln` appends a bare CR.
        assert_eq!(did("sendlnbroadcast 'x'"), vec!["broadcast \"x\\r\\n\""]);
        assert_eq!(
            did("sendmulticast 'group' 'x' 33"),
            vec!["multicast \"group\" \"x!\""]
        );
        assert_eq!(
            did("sendlnmulticast 'group' 'x'"),
            vec!["multicast \"group\" \"x\\r\\n\""]
        );
    }

    #[test]
    fn setmulticastname_never_checks_for_end_of_line() {
        assert_eq!(did("setmulticastname 'me'"), vec!["multicastname \"me\""]);
        // `ttl.cpp:4095` goes straight to `SendCmnd` after one string.
        assert_eq!(
            did("setmulticastname 'me' and more"),
            vec!["multicastname \"me\""]
        );
    }

    #[test]
    fn scp_takes_an_optional_destination_by_swallowing_its_error() {
        assert_eq!(did("scpsend 'a.txt'"), vec!["scpsend \"a.txt\" \"\""]);
        assert_eq!(
            did("scprecv 'src/f' '/tmp/f'"),
            vec!["scprecv \"src/f\" \"/tmp/f\""]
        );
        // An empty source is a syntax error. A second argument of the wrong
        // type is **not**: the expression is consumed, the type mismatch it
        // reported is discarded, the destination stays empty and the
        // end-of-line check then finds nothing left to object to. A reader of
        // the first half of the function would predict `ErrTypeMismatch`.
        assert_eq!(err_of("scpsend ''"), TtlError::Syntax);
        assert_eq!(did("scpsend 'a' 3"), vec!["scpsend \"a\" \"\""]);
        // Trailing junk after a good destination is still a syntax error.
        assert_eq!(err_of("scpsend 'a' 'b' 'c'"), TtlError::Syntax);
    }

    #[test]
    fn wait4all_hands_its_patterns_over_and_reports_the_index() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.wait4all_match = 2;
        run_with(&mut host, "wait4all 'a' 'b'\ndispstr result");
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(host.sends, vec!["wait4all \"a\" \"b\""]);
        assert_eq!(host.output, b"2");

        // No patterns at all is not an error and not a wait, as with `wait`.
        assert_eq!(did("wait4all"), Vec::<String>::new());
        assert_eq!(err_of("wait4all 3"), TtlError::TypeMismatch);
    }

    #[test]
    fn waitevent_tests_the_state_rather_than_waiting_for_a_change() {
        // Already connected, asked for the connection event: returns at once.
        assert_eq!(run("waitevent 8\ndispstr result").output, b"8");

        // Already disconnected, asked for the disconnection event: likewise.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.com_ready = false;
        run_with(&mut host, "waitevent 4\ndispstr result");
        assert_eq!(host.output, b"4");

        // Unlinked, asked for the unlink event.
        let mut host = RecordingHost::new();
        host.linked = false;
        run_with(&mut host, "waitevent 2\ndispstr result");
        assert_eq!(host.output, b"2");
    }

    #[test]
    fn waitevent_times_out_and_drops_the_bits_above_the_four() {
        // Bit 16 is masked off, so only the timeout can end this one.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.com_ready = true;
        run_with(&mut host, "mtimeout = 30\nwaitevent 17\ndispstr result");
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(host.output, b"1");

        // ...and with the timeout bit clear, the deadline ends the wait
        // without reporting anything, which is upstream's `TestWakeup`.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.com_ready = true;
        run_with(
            &mut host,
            "result = 9\nmtimeout = 30\nwaitevent 16\ndispstr result",
        );
        assert_eq!(host.output, b"9");
    }
}
