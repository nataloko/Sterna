//! The connection — what a script says to the far end, and what it waits for.
//!
//! The matching is `tt-ttl`'s, deliberately. [`WaitSet`] is upstream's
//! incremental matcher with upstream's back-off, and two languages disagreeing
//! about when `wait 'ogin:'` fires would be a bug nobody could find. What is
//! *not* reused is the line buffer: `RecvLnBuff` stops growing at 511 bytes
//! and says nothing about it, which is `ttmdde.c`'s array size rather than
//! anything about terminals, so [`Recv`] below is the same rules without the
//! ceiling.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use mlua::{BString, Scope, Table, Value, Variadic};
use tt_ttl::wait::{WaitSet, MAX_WAIT};
use tt_ttl::{SendMode, TtlError};

use crate::{deadline, lua_err, Host};

/// The bytes seen since the last newline, which is what `tt.waitln` returns.
///
/// The clear happens on the byte *after* a newline rather than on the newline
/// itself, so the terminator stays in the line it ended — upstream's
/// `PutRecvLnBuff`, and the reason a `waitln` can report the line its match
/// was in rather than the one after it.
#[derive(Debug, Clone, Default)]
pub struct Recv {
    buf: Vec<u8>,
    last: u8,
    /// Off while `tt.waitn` runs, which counts bytes and must not lose them at
    /// a line break.
    keep_lines: bool,
}

impl Recv {
    fn put(&mut self, b: u8) {
        if self.last == 0x0a && !self.keep_lines {
            self.buf.clear();
        }
        self.buf.push(b);
        self.last = b;
    }

    /// The line without its terminator. A CR is dropped only when it came
    /// immediately before the LF, so `\r` alone in the middle of a line stays.
    fn take(&mut self) -> Vec<u8> {
        let mut out = std::mem::take(&mut self.buf);
        if out.last() == Some(&0x0a) {
            out.pop();
            if out.last() == Some(&0x0d) {
                out.pop();
            }
        }
        out
    }

    fn len(&self) -> usize {
        self.buf.len()
    }
}

/// The check every command that touches the far end makes first.
///
/// It belongs to the *language* rather than to the host — `TTLCommCmd`
/// (`ttl.cpp`) makes it before it dispatches anything, which is why a macro
/// run against no window fails loudly at its first `send` instead of quietly
/// writing into nothing. A host is entitled to assume it has already happened.
pub(crate) fn link(host: &Host<'_>) -> mlua::Result<()> {
    if host.borrow_mut().linked() {
        Ok(())
    } else {
        Err(lua_err(TtlError::LinkFirst))
    }
}

/// Success is one value, so `tt.send(tt.recvln())` means what it looks like.
///
/// Lua expands a call's *last* argument to all of its results, so a function
/// that answers `line, nil` on success puts a `nil` into the argument list of
/// whatever it is nested in. The convention here is `io.open`'s: one value when
/// it worked, and `nil` plus the detail when it did not.
fn ok(v: Value) -> Variadic<Value> {
    Variadic::from_iter([v])
}

/// It did not happen, and here is what arrived anyway.
fn no(detail: Value) -> Variadic<Value> {
    Variadic::from_iter([Value::Nil, detail])
}

/// One byte in, or `None` because the deadline passed or the line went away.
fn read_byte(host: &Host<'_>, until: Option<Instant>) -> Option<u8> {
    let left = match until {
        None => None,
        Some(d) => {
            let now = Instant::now();
            if now >= d {
                return None;
            }
            Some(d - now)
        }
    };
    host.borrow_mut().read_byte(left)
}

/// The bytes a variadic call means.
///
/// A string is its bytes and an integer is its **low byte**, which is TTL's
/// rule for `send` and is worth keeping: `tt.send(27, '[2J')` is how a script
/// writes an escape sequence, and coercing the 27 to `"27"` would silently
/// send two digits. Anything else is an error rather than a `tostring`.
pub(crate) fn bytes_of(args: &Variadic<Value>) -> mlua::Result<Vec<u8>> {
    let mut out = Vec::new();
    for (i, v) in args.iter().enumerate() {
        match v {
            Value::String(s) => out.extend_from_slice(&s.as_bytes()),
            Value::Integer(n) => out.push(*n as u8),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "argument {} is a {}; expected a string or an integer",
                    i + 1,
                    other.type_name()
                )))
            }
        }
    }
    Ok(out)
}

/// Install the patterns, lowest argument first. Ten is [`WaitSet`]'s ceiling
/// and upstream's; above it the script is told rather than quietly matching on
/// the first ten.
fn patterns(pats: &Variadic<BString>) -> mlua::Result<WaitSet> {
    if pats.len() > MAX_WAIT {
        return Err(mlua::Error::runtime(format!(
            "at most {MAX_WAIT} patterns, not {}",
            pats.len()
        )));
    }
    let mut set = WaitSet::new();
    for (i, p) in pats.iter().enumerate() {
        set.set(i + 1, p);
    }
    Ok(set)
}

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
    recv: &'e RefCell<Recv>,
) -> mlua::Result<()> {
    // `tt.timeout` is seconds, and a float — TTL needs `timeout` and
    // `mtimeout` because its variables are integers. Zero is for ever, which
    // is the same rule.
    tt.set("timeout", 0.0)?;

    tt.set(
        "linked",
        scope.create_function(move |_, ()| Ok(host.borrow_mut().linked()))?,
    )?;
    tt.set(
        "connected",
        scope.create_function(move |_, ()| Ok(host.borrow_mut().com_ready()))?,
    )?;

    // ---- out ----

    tt.set(
        "send",
        scope.create_function(move |_, args: Variadic<Value>| {
            let bytes = bytes_of(&args)?;
            link(host)?;
            host.borrow_mut()
                .send(&bytes, SendMode::Compat)
                .map_err(lua_err)
        })?,
    )?;
    // The line ending is a bare CR, as TTL's `sendln` is: what the far end
    // sees depends on the terminal's own newline setting, exactly as it does
    // for anything typed. Appending LF here would put a bare LF on the wire
    // under every setting including the default.
    tt.set(
        "sendln",
        scope.create_function(move |_, args: Variadic<Value>| {
            let mut bytes = bytes_of(&args)?;
            bytes.push(0x0d);
            link(host)?;
            host.borrow_mut()
                .send(&bytes, SendMode::Compat)
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendtext",
        scope.create_function(move |_, s: BString| {
            link(host)?;
            host.borrow_mut().send(&s, SendMode::Text).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendbinary",
        scope.create_function(move |_, s: BString| {
            link(host)?;
            host.borrow_mut()
                .send(&s, SendMode::Binary)
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendbreak",
        scope.create_function(move |_, ()| {
            link(host)?;
            host.borrow_mut().send_break().map_err(lua_err)
        })?,
    )?;
    tt.set(
        "dispstr",
        scope.create_function(move |_, args: Variadic<Value>| {
            // No link check, which is upstream's: `dispstr` paints the local
            // screen and does not touch the connection.
            let bytes = bytes_of(&args)?;
            host.borrow_mut().disp_str(&bytes).map_err(lua_err)
        })?,
    )?;

    // ---- in ----

    let t = tt.clone();
    tt.set(
        "wait",
        scope.create_function(move |_, pats: Variadic<BString>| {
            let mut set = patterns(&pats)?;
            link(host)?;
            if pats.is_empty() {
                return Ok(None);
            }
            let until = deadline(&t)?;
            let mut rc = recv.borrow_mut();
            while let Some(b) = read_byte(host, until) {
                rc.put(b);
                if let Some(i) = set.feed(b) {
                    return Ok(Some(i));
                }
            }
            Ok(None)
        })?,
    )?;

    let t = tt.clone();
    tt.set(
        "waitln",
        scope.create_function(move |lua, pats: Variadic<BString>| {
            let mut set = patterns(&pats)?;
            link(host)?;
            let until = deadline(&t)?;
            let mut rc = recv.borrow_mut();

            // With no pattern at all this is `recvln`: wait for the newline
            // and hand back the line. With patterns it is `wait` plus a second
            // phase that runs on to the end of the line the match was in.
            let mut found = if pats.is_empty() { Some(0) } else { None };
            if found.is_none() {
                while let Some(b) = read_byte(host, until) {
                    rc.put(b);
                    if let Some(i) = set.feed(b) {
                        found = Some(i);
                        break;
                    }
                }
            }
            let Some(index) = found else {
                // Timed out before anything matched. The partial line is the
                // second value, which is what a script wants for a diagnostic
                // and what TTL leaves in `inputstr`.
                return Ok(no(Value::String(lua.create_string(rc.take())?)));
            };

            // The newline may already have been what matched.
            let complete = set.is(index, b"\n") || {
                let mut nl = WaitSet::new();
                nl.set(1, b"\n");
                let mut got = false;
                while let Some(b) = read_byte(host, until) {
                    rc.put(b);
                    if nl.feed(b).is_some() {
                        got = true;
                        break;
                    }
                }
                got
            };
            let line = Value::String(lua.create_string(rc.take())?);
            if complete {
                // Two values on the success path, which is the exception to
                // the one-value rule and is why the *line* comes first: it is
                // the payload, and a single-pattern wait already knows which
                // pattern it was.
                Ok(Variadic::from_iter([line, Value::Integer(index as i64)]))
            } else {
                Ok(no(line))
            }
        })?,
    )?;

    let t = tt.clone();
    tt.set(
        "recvln",
        scope.create_function(move |lua, ()| {
            link(host)?;
            let until = deadline(&t)?;
            let mut nl = WaitSet::new();
            nl.set(1, b"\n");
            let mut rc = recv.borrow_mut();
            let mut got = false;
            while let Some(b) = read_byte(host, until) {
                rc.put(b);
                if nl.feed(b).is_some() {
                    got = true;
                    break;
                }
            }
            let line = Value::String(lua.create_string(rc.take())?);
            Ok(if got { ok(line) } else { no(line) })
        })?,
    )?;

    let t = tt.clone();
    tt.set(
        "waitn",
        scope.create_function(move |lua, want: i64| {
            link(host)?;
            let want = want.max(0) as usize;
            let until = deadline(&t)?;
            let mut rc = recv.borrow_mut();
            // Bytes left over from an earlier wait count towards the total,
            // which is TTL's `waitn` and the reason the line buffer is shared
            // between all of these rather than one per command.
            rc.keep_lines = true;
            while rc.len() < want {
                let Some(b) = read_byte(host, until) else {
                    break;
                };
                rc.put(b);
            }
            let enough = rc.len() >= want;
            let got = Value::String(lua.create_string(rc.take())?);
            rc.keep_lines = false;
            Ok(if enough { ok(got) } else { no(got) })
        })?,
    )?;

    tt.set(
        "flush",
        scope.create_function(move |_, ()| {
            host.borrow_mut().flush_recv();
            recv.borrow_mut().take();
            Ok(())
        })?,
    )?;

    // ---- the session ----

    tt.set(
        "sleep",
        scope.create_function(move |_, secs: f64| {
            if secs > 0.0 {
                host.borrow_mut().sleep(Duration::from_secs_f64(secs));
            }
            Ok(())
        })?,
    )?;

    // TTL reports 0/1/2 out of `connect` and `testlink` because it has one
    // number to say two things in. Here `tt.linked()` and `tt.connected()`
    // answer each half, so this returns only whether there is a connection
    // now — and raises when the host refused, which TTL cannot express.
    tt.set(
        "connect",
        scope.create_function(move |_, cmdline: Option<BString>| {
            let line = cmdline.unwrap_or_default();
            let mut h = host.borrow_mut();
            if h.linked() && h.com_ready() {
                return Ok(true);
            }
            h.connect(&line, false).map_err(lua_err)?;
            Ok(h.com_ready())
        })?,
    )?;
    tt.set(
        "cygconnect",
        scope.create_function(move |_, cmdline: Option<BString>| {
            let line = cmdline.unwrap_or_default();
            let mut h = host.borrow_mut();
            if h.linked() && h.com_ready() {
                return Ok(true);
            }
            h.connect(&line, true).map_err(lua_err)?;
            Ok(h.com_ready())
        })?,
    )?;
    // TTL's bare `disconnect` puts the confirmation dialog up and only
    // `disconnect 0` skips it. A script that says nothing here means "just do
    // it": an omitted argument in Lua is an omitted argument, not a request
    // for a dialog, and a script asking for one can say so.
    tt.set(
        "disconnect",
        scope.create_function(move |_, confirm: Option<bool>| {
            link(host)?;
            host.borrow_mut()
                .disconnect(confirm.unwrap_or(false))
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "closett",
        scope.create_function(move |_, ()| host.borrow_mut().close_terminal().map_err(lua_err))?,
    )?;
    tt.set(
        "unlink",
        scope.create_function(move |_, ()| {
            host.borrow_mut().unlink();
            Ok(())
        })?,
    )?;
    tt.set(
        "setsync",
        scope.create_function(move |_, on: bool| {
            link(host)?;
            host.borrow_mut().set_sync(on);
            Ok(())
        })?,
    )?;
    tt.set(
        "setexitcode",
        scope.create_function(move |_, code: i32| {
            host.borrow_mut().set_exit_code(code);
            Ok(())
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use crate::Script;
    use tt_ttl::RecordingHost;

    /// A host with bytes already waiting, so the wait family can be tested
    /// with no terminal and no clock.
    fn with_input(input: &[u8], src: &str) -> (RecordingHost, mlua::Result<()>) {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input = input.iter().copied().collect();
        let r = Script::new("t.lua", src.as_bytes().to_vec()).run(&mut host);
        (host, r)
    }

    #[test]
    fn send_takes_bytes_and_integers_the_way_ttl_does() {
        let (host, r) = run("tt.send(27, '[2J')");
        r.unwrap();
        assert_eq!(host.sent, b"\x1b[2J");
    }

    #[test]
    fn a_float_is_refused_rather_than_stringified() {
        let (_, r) = run("tt.send(1.5)");
        assert!(r.unwrap_err().to_string().contains("expected a string"));
    }

    #[test]
    fn wait_answers_with_which_pattern_matched() {
        let (host, r) = with_input(
            b"Password: ",
            "local i = tt.wait('ogin:', 'assword:'); tt.send(tostring(i))",
        );
        r.unwrap();
        assert_eq!(host.sent, b"2");
    }

    #[test]
    fn wait_that_runs_out_of_input_answers_nil() {
        let (host, r) = with_input(b"nothing here", "tt.send(tostring(tt.wait('$ ')))");
        r.unwrap();
        assert_eq!(host.sent, b"nil");
    }

    #[test]
    fn waitln_hands_back_the_whole_line_the_match_was_in() {
        let (host, r) = with_input(
            b"noise\r\nuser@host:~$ ready\r\n",
            "local line, i = tt.waitln('nothing', 'ready'); tt.send(line, i)",
        );
        r.unwrap();
        assert_eq!(host.sent, b"user@host:~$ ready\x02");
    }

    /// The convention that makes nesting work: one value when it worked.
    #[test]
    fn a_success_is_a_single_value_so_it_can_be_passed_straight_on() {
        let (host, r) = with_input(b"one\r\n", "tt.send(tt.recvln())");
        r.unwrap();
        assert_eq!(host.sent, b"one");
    }

    #[test]
    fn recvln_reads_one_line_and_strips_only_a_paired_cr() {
        let (host, r) = with_input(
            b"one\r\ntwo\n",
            "tt.send(tt.recvln()); tt.send(tt.recvln())",
        );
        r.unwrap();
        assert_eq!(host.sent, b"onetwo");
    }

    #[test]
    fn recvln_that_never_sees_a_newline_returns_the_partial_second() {
        let (host, r) = with_input(b"half", "local l, p = tt.recvln(); tt.send(tostring(l), p)");
        r.unwrap();
        assert_eq!(host.sent, b"nilhalf");
    }

    #[test]
    fn waitn_counts_bytes_across_line_endings() {
        let (host, r) = with_input(b"ab\r\ncd", "tt.send(tt.waitn(6))");
        r.unwrap();
        assert_eq!(host.sent, b"ab\r\ncd");
    }

    #[test]
    fn waitn_short_of_its_count_hands_back_what_arrived() {
        let (host, r) = with_input(b"abc", "local g, p = tt.waitn(10); tt.send(tostring(g), p)");
        r.unwrap();
        assert_eq!(host.sent, b"nilabc");
    }

    #[test]
    fn more_than_ten_patterns_is_an_error_rather_than_a_silent_ten() {
        let (_, r) = run("tt.wait('1','2','3','4','5','6','7','8','9','a','b')");
        assert!(r.unwrap_err().to_string().contains("at most 10"));
    }

    #[test]
    fn a_command_that_needs_a_link_raises_upstreams_sentence() {
        let mut host = RecordingHost::new();
        host.linked = false;
        let r = Script::new("t.lua", b"tt.send('x')".to_vec()).run(&mut host);
        assert!(r.unwrap_err().to_string().contains("Link macro first"));
        assert!(host.sent.is_empty());
    }

    #[test]
    fn a_refusal_is_catchable() {
        let mut host = RecordingHost::new();
        host.linked = false;
        let src = "local ok, err = pcall(tt.send, 'x'); tt.dispstr(tostring(ok))";
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"false");
    }
}
