//! Talking to the far end — `send`, the `wait` family, `pause` and `flushrecv`.
//!
//! **This is where the thread pays for itself.** Upstream cannot block: the
//! macro and the window share one, so `wait` sets `TTLStatus = IdTTLWait`,
//! returns, and the window's message loop calls `Wait()` again on every timer
//! tick until it matches or the deadline passes — with the answer, the
//! `inputstr` handling and the timeout arm each living in a different place
//! from the command that started it. Here every one of them is a function that
//! reads bytes until it is done, and the four states collapse into four loops.
//!
//! What the deadline means is unchanged: `timeout` in seconds plus `mtimeout`
//! in milliseconds, both ordinary variables the macro can assign to, and zero
//! between them means wait for ever.

use std::time::{Duration, Instant};

use crate::error::{TtlError, TtlResult};
use crate::expr::{self, Eval};
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::rsv::Rsv;
use crate::vars::{VarRef, VarType};
use crate::wait::{WaitRecv, MAX_WAIT};

/// The line terminator `waitln` and `recvln` are looking for.
const NL: &[u8] = b"\n";

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn connection_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::Send => self.cmd_send(host, false),
            Rsv::SendLn => self.cmd_send(host, true),
            Rsv::FlushRecv => self.cmd_flush_recv(host),
            Rsv::Wait => self.cmd_wait(host, false),
            Rsv::WaitLn => self.cmd_wait(host, true),
            Rsv::WaitN => self.cmd_wait_n(host),
            Rsv::WaitRecv => self.cmd_wait_recv(host),
            Rsv::RecvLn => self.cmd_recv_ln(host),
            Rsv::Pause => self.cmd_pause(host, 1000),
            Rsv::MilliPause => self.cmd_pause(host, 1),
            _ => return None,
        })
    }

    /// `SetInputStr`, and the same caveat as `result`: a macro that has made
    /// `inputstr` an integer stops being told anything.
    pub(crate) fn set_input_str(&mut self, val: &[u8]) {
        if let Some((id, VarType::String)) = self.vars.find(b"inputstr") {
            self.vars.set_str(VarRef::Scalar(id), val);
        }
    }

    /// `timeout` seconds plus `mtimeout` milliseconds. `None` is "for ever".
    ///
    /// Both are read at the point of use rather than captured, so assigning to
    /// `timeout` changes the next wait and not the one already running — which
    /// is the only thing it could mean when a wait cannot be interrupted.
    fn deadline(&self) -> Option<Instant> {
        let secs = match self.vars.find(b"timeout") {
            Some((id, VarType::Integer)) => self.vars.int_at(VarRef::Scalar(id)) as i64,
            _ => 0,
        };
        let millis = match self.vars.find(b"mtimeout") {
            Some((id, VarType::Integer)) => self.vars.int_at(VarRef::Scalar(id)) as i64,
            _ => 0,
        };
        let total = secs.saturating_mul(1000).saturating_add(millis);
        if total > 0 {
            Some(Instant::now() + Duration::from_millis(total as u64))
        } else {
            None
        }
    }

    /// One byte, or `None` because the deadline passed or the line went away.
    ///
    /// Upstream tells those two apart — `TimeOut` versus `ComReady == 0` — and
    /// then does the same thing for both in every arm, so they are one here.
    fn read_byte(&mut self, host: &mut dyn ScriptHost, deadline: Option<Instant>) -> Option<u8> {
        let left = match deadline {
            None => None,
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    return None;
                }
                Some(d - now)
            }
        };
        host.read_byte(left)
    }

    /// `GetParamStrings` — the argument list `send` and `sendln` share.
    ///
    /// A string goes out as its bytes and an integer as its **low byte only**,
    /// which is how `send 'x' 13 10` writes a line ending. Anything else is a
    /// type mismatch.
    fn param_strings(&mut self) -> TtlResult<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            if let Some(s) = self.lx.string()? {
                out.extend_from_slice(&s);
                continue;
            }
            match expr::get_expression(&mut self.lx, &mut self.vars)? {
                Some(Eval::Int(v)) => out.push(v as u8),
                Some(Eval::Str(r)) => {
                    let s = self.vars.str_at(r).to_vec();
                    out.extend_from_slice(&s);
                }
                Some(_) => return Err(TtlError::TypeMismatch),
                None => return Ok(out),
            }
        }
    }

    /// `send` / `sendln`. The line ending `sendln` adds is a bare **CR**; what
    /// the far end sees also depends on the terminal's own newline setting, as
    /// it does for anything typed.
    fn cmd_send(&mut self, host: &mut dyn ScriptHost, ln: bool) -> TtlResult<()> {
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        let mut bytes = self.param_strings()?;
        if ln {
            bytes.push(0x0d);
        }
        host.send(&bytes)
    }

    fn cmd_flush_recv(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.end_of_line()?;
        host.flush_recv();
        Ok(())
    }

    /// `pause <seconds>` / `mpause <milliseconds>`.
    ///
    /// Upstream's `pause` is a timer the window can interrupt and its `mpause`
    /// is a hard `Sleep`; both are the host's here, so a frontend that wants
    /// either to be cancellable can make it so.
    fn cmd_pause(&mut self, host: &mut dyn ScriptHost, unit_ms: u64) -> TtlResult<()> {
        let n = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if n > 0 {
            host.sleep(Duration::from_millis(n as u64 * unit_ms));
        }
        Ok(())
    }

    /// `wait` / `waitln` — up to ten strings; `result` is which one matched,
    /// counting from 1, or 0 if none did.
    ///
    /// `waitln` is `wait` plus a second phase: having matched, it goes on
    /// waiting for the newline that ends the line the match was in, and puts
    /// that whole line in `inputstr`. If the newline never comes, `result` is
    /// overwritten with 0 — so a `waitln` can report failure having matched.
    fn cmd_wait(&mut self, host: &mut dyn ScriptHost, ln: bool) -> TtlResult<()> {
        self.waits.clear();
        let mut count = 0;
        for i in 0..MAX_WAIT {
            let pattern = match self.lx.string() {
                Ok(Some(s)) => s,
                Ok(None) => match expr::get_expression(&mut self.lx, &mut self.vars) {
                    Ok(Some(Eval::Str(r))) => self.vars.str_at(r).to_vec(),
                    Ok(Some(_)) => {
                        self.waits.clear();
                        return Err(TtlError::TypeMismatch);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        self.waits.clear();
                        return Err(e);
                    }
                },
                Err(e) => {
                    self.waits.clear();
                    return Err(e);
                }
            };
            self.waits.set(i + 1, &pattern);
            count += 1;
        }

        if let Err(e) = self.end_of_line() {
            self.waits.clear();
            return Err(e);
        }
        if !host.linked() {
            self.waits.clear();
            return Err(TtlError::LinkFirst);
        }
        // No strings at all is not an error and not a wait; upstream's `i > 0`
        // test simply falls through to `ClearWait` and the macro carries on.
        if count == 0 {
            return Ok(());
        }

        let deadline = self.deadline();
        let mut found = 0;
        while let Some(b) = self.read_byte(host, deadline) {
            self.recv_line.put(b);
            if let Some(i) = self.waits.feed(b) {
                found = i;
                break;
            }
        }
        self.set_result(found as i32);

        if !ln || found == 0 {
            self.waits.clear();
            return Ok(());
        }

        // The newline may already have been what matched.
        if self.waits.is(found, NL) {
            self.waits.clear();
            let line = self.recv_line.take();
            self.set_input_str(&line);
            return Ok(());
        }

        self.waits.clear();
        self.waits.set(1, NL);
        let got = self.wait_for_newline(host, deadline);
        if !got {
            self.set_result(0);
        }
        let line = self.recv_line.take();
        self.set_input_str(&line);
        self.waits.clear();
        Ok(())
    }

    /// The `IdTTLWaitNL` phase, shared by `waitln` and `recvln`.
    fn wait_for_newline(&mut self, host: &mut dyn ScriptHost, deadline: Option<Instant>) -> bool {
        while let Some(b) = self.read_byte(host, deadline) {
            self.recv_line.put(b);
            if self.waits.feed(b).is_some() {
                return true;
            }
        }
        false
    }

    /// `recvln` — read one line into `inputstr`. `result` is 1, or 0 if the
    /// line did not arrive in time; the partial line is in `inputstr` either
    /// way.
    fn cmd_recv_ln(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.end_of_line()?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        self.waits.clear();
        self.waits.set(1, NL);
        self.set_input_str(b"");
        self.set_result(1);

        let deadline = self.deadline();
        if !self.wait_for_newline(host, deadline) {
            self.set_result(0);
        }
        let line = self.recv_line.take();
        self.set_input_str(&line);
        self.waits.clear();
        Ok(())
    }

    /// `waitn <count>` — read that many bytes into `inputstr`. `result` is 1
    /// for all of them, -1 for some, 0 for none.
    ///
    /// Two things it inherits. The line buffer is **not** emptied first, so
    /// bytes left over from an earlier `wait` count towards the total. And the
    /// suppression of the newline-clearing that makes the counting work is only
    /// undone on the success path — upstream's timeout arm forgets to call
    /// `ClearWaitN`, so after a `waitn` that times out, every later `inputstr`
    /// accumulates across lines instead of holding one. Reproduced; see the
    /// crate README, which lists it as wanting a report.
    fn cmd_wait_n(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.recv_line.clear_on_newline = true;
        let want = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        self.recv_line.clear_on_newline = false;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }

        let want = want.max(0) as usize;
        let deadline = self.deadline();
        while self.recv_line.len() < want {
            let Some(b) = self.read_byte(host, deadline) else {
                break;
            };
            self.recv_line.put(b);
        }

        if self.recv_line.len() >= want {
            self.set_result(1);
            let got = self.recv_line.take();
            self.set_input_str(&got);
            self.recv_line.clear_on_newline = true;
        } else {
            let got = self.recv_line.take();
            self.set_result(if got.is_empty() { 0 } else { -1 });
            self.set_input_str(&got);
        }
        Ok(())
    }

    /// `waitrecv <string> <len> <pos>` — wait for exactly `len` bytes with
    /// `string` at 1-based position `pos` among them.
    ///
    /// `result` is 1 when both held, -1 when the string was seen but the count
    /// was not reached, and 0 when neither. It does not use the line buffer, so
    /// what it reads is not visible to a later `waitln`.
    fn cmd_wait_recv(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let sub = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let len = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let pos = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        self.set_input_str(b"");

        let mut w = WaitRecv::set(&sub, len, pos);
        let deadline = self.deadline();
        while !w.done() {
            let Some(b) = self.read_byte(host, deadline) else {
                break;
            };
            w.feed(b);
        }

        if w.done() {
            self.set_result(1);
            let window = w.window().to_vec();
            self.set_input_str(&window);
        } else if w.found() {
            self.set_result(-1);
            let window = w.window().to_vec();
            self.set_input_str(&window);
        } else {
            self.set_result(0);
            self.set_input_str(b"");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;

    fn run(input: &[u8], src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input = input.to_vec().into();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(input: &[u8], src: &str) -> String {
        let h = run(input, src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    #[test]
    fn send_puts_strings_and_low_bytes_on_the_wire() {
        let h = run(b"", "send 'ab' 13 10");
        assert_eq!(h.sent, b"ab\r\n");
        let h = run(b"", "sendln 'hi'");
        assert_eq!(h.sent, b"hi\r");
        // 0x141 keeps only its low byte, which is how upstream writes it out.
        let h = run(b"", "send $141");
        assert_eq!(h.sent, b"\x41");
    }

    #[test]
    fn nothing_reaches_the_wire_without_a_connection() {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", b"send 'x'".to_vec(), &mut host);
        it.run(&mut host);
        assert_eq!(host.errors[0].0, TtlError::LinkFirst);
        assert!(host.sent.is_empty());
    }

    #[test]
    fn wait_reports_which_string_matched() {
        assert_eq!(out(b"hello login: ", "wait 'login:'\ndispstr result"), "1");
        assert_eq!(
            out(
                b"...Password: ",
                "wait 'login:' 'Password:'\ndispstr result"
            ),
            "2"
        );
        // Nothing matches and the input runs out: zero, not a hang.
        assert_eq!(out(b"nothing here", "wait 'login:'\ndispstr result"), "0");
    }

    #[test]
    fn waitln_holds_the_whole_line_it_matched_in() {
        assert_eq!(
            out(
                b"noise\r\nfound the thing\r\nmore",
                "waitln 'found'\ndispstr result'|'inputstr"
            ),
            "1|found the thing"
        );
        // Matching the newline itself needs no second phase.
        assert_eq!(
            out(b"abc\n", "waitln #10\ndispstr result'|'inputstr"),
            "1|abc"
        );
    }

    #[test]
    fn a_waitln_that_matched_but_never_saw_the_newline_reports_zero() {
        let h = run(
            b"found the thing",
            "waitln 'found'\ndispstr result'|'inputstr",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            String::from_utf8_lossy(&h.output),
            "0|found the thing",
            "result is overwritten, and the partial line still arrives"
        );
    }

    #[test]
    fn recvln_takes_one_line_at_a_time() {
        assert_eq!(
            out(
                b"one\r\ntwo\r\n",
                "recvln\ndispstr inputstr'|'\nrecvln\ndispstr inputstr"
            ),
            "one|two"
        );
        assert_eq!(
            out(b"no terminator", "recvln\ndispstr result'|'inputstr"),
            "0|no terminator"
        );
    }

    #[test]
    fn waitn_counts_bytes_and_says_how_it_did() {
        assert_eq!(
            out(b"abcdef", "waitn 3\ndispstr result'|'inputstr"),
            "1|abc"
        );
        assert_eq!(out(b"ab", "waitn 5\ndispstr result'|'inputstr"), "-1|ab");
        assert_eq!(out(b"", "waitn 5\ndispstr result'|'inputstr"), "0|");
        // A newline in the middle does not end it, which is the whole point.
        assert_eq!(out(b"a\r\nbc", "waitn 5\ndispstr result"), "1");
    }

    #[test]
    fn waitrecv_wants_the_string_in_place_among_a_fixed_count() {
        assert_eq!(
            out(b"xokyz", "waitrecv 'ok' 4 2\ndispstr result'|'inputstr"),
            "1|xoky"
        );
        // Seen but the count never reached: -1, and what there was.
        assert_eq!(
            out(b"xok", "waitrecv 'ok' 9 2\ndispstr result'|'inputstr"),
            "-1|xok"
        );
        // Never seen: 0, and nothing.
        assert_eq!(
            out(b"abcd", "waitrecv 'ok' 4 2\ndispstr result'|'inputstr"),
            "0|"
        );
    }

    #[test]
    fn the_timeout_variables_are_read_at_the_point_of_use() {
        // With no bytes coming and a timeout set, the wait ends rather than
        // hanging. The recording host has nothing to give, so this only checks
        // that the deadline is computed and the loop is bounded.
        assert_eq!(out(b"", "timeout = 1\nwait 'x'\ndispstr result"), "0");
        assert_eq!(out(b"", "mtimeout = 50\nwait 'x'\ndispstr result"), "0");
    }

    #[test]
    fn pause_and_flushrecv_reach_the_host() {
        let h = run(b"", "mpause 5\npause 0\nflushrecv");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.slept.as_millis(), 5, "a pause of zero sleeps not at all");
        assert_eq!(h.flushes, 1);
    }
}
