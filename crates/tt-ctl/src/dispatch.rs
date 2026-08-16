//! The method table.
//!
//! Nine methods. `ttddecmnd.h` has ninety-odd, and the difference is the whole
//! design: DDE's command set *was* the macro language, because a macro was a
//! second process and had no other way to reach the terminal. Here the macro
//! is inside the window, the language is [`tt_ttl`], and this socket only has
//! to be able to start one. So the surface is the four things a script wants
//! that are not a macro — say hello, look at the terminal, type at it, open
//! and close the line — plus the two that are.
//!
//! **Every method runs on the frontend's thread**, posted through
//! [`CtlSender::call`]. The connection's own thread does the JSON and nothing
//! else. That is not an optimisation: a [`Session`] belongs to the thread that
//! pumps it, and `connect` may raise an SSH host-key dialog, which belongs to
//! the thread that has an event loop.
//!
//! **A method that changes the terminal answers when the change is made, not
//! when it has taken effect.** `connect` returns once the attempt has started,
//! for the same reason the macro language's own `connect` does: upstream reads
//! its result back afterwards out of `linked` and `com_ready` rather than from
//! the command (`ttdde.c`), because an SSH negotiation involves the user. A
//! client that needs to know polls `status`.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tt_grid::WIDTH_PAD;

use crate::channel::CtlSender;
use crate::host::RunError;
use crate::proto::{params, RpcError};

/// How often a `macro.run` that was asked to wait looks again.
///
/// The frontend owns the macro handle, so "has it finished" is a question that
/// has to be asked over the channel; there is no condvar to wait on without
/// inventing a second wakeup path through the C ABI for a client that is
/// already prepared to block. Fifty milliseconds is invisible next to a macro
/// that logs into something, and a hundredth of the cost of the process
/// upstream would have started to do the same job.
const POLL: Duration = Duration::from_millis(50);

/// Run one method. `Err` is the error object the client is sent.
///
/// The `Option<T>` from every [`CtlSender::call`] is the window having gone,
/// and it is turned into [`RpcError::GONE`] here rather than at each call site
/// — `gone()` is that conversion, and the `?` after each call is where a
/// closed window stops being this connection's problem.
pub fn call(tx: &CtlSender, method: &str, p: Option<Value>) -> Result<Value, RpcError> {
    match method {
        "ping" => ping(tx),
        "status" => status(tx),
        "send" => send(tx, p),
        "sendln" => sendln(tx, p),
        "connect" => connect(tx, p),
        "disconnect" => disconnect(tx),
        "screen" => screen(tx, p),
        "macro.run" => macro_run(tx, p),
        "macro.stop" => macro_stop(tx),
        "close" => close(tx),
        _ => Err(RpcError::new(
            RpcError::NO_METHOD,
            format!("no method {method:?}"),
        )),
    }
}

/// The names, for `ttctl --help` and for a client that wants to check one
/// before sending it. Kept next to the table above so the two cannot drift.
pub const METHODS: &[&str] = &[
    "ping",
    "status",
    "send",
    "sendln",
    "connect",
    "disconnect",
    "screen",
    "macro.run",
    "macro.stop",
    "close",
];

fn gone<T>(v: Option<T>) -> Result<T, RpcError> {
    v.ok_or_else(|| RpcError::new(RpcError::GONE, "the window has closed"))
}

/// Liveness, and enough to tell one window from another.
///
/// A client that has just found a socket in the directory calls this to learn
/// whether it is the window it wanted — which is what a DDE wildcard connect
/// could not answer either, and why upstream's `ttpmacro` had to be *told* its
/// topic.
fn ping(tx: &CtlSender) -> Result<Value, RpcError> {
    // Answered from the frontend's thread rather than from here, so that a
    // window whose event loop has stopped fails the ping instead of passing
    // it from the listener behind its back.
    let title = gone(tx.call(|s, h| h.title().unwrap_or_else(|| s.vt().window_title())))?;
    Ok(json!({
        "pid": std::process::id(),
        "version": env!("CARGO_PKG_VERSION"),
        "title": title,
    }))
}

fn status(tx: &CtlSender) -> Result<Value, RpcError> {
    gone(tx.call(|s, h| {
        let m = h.macro_status();
        let c = s.counters();
        // Asked here rather than left to the caller, because this is the one
        // place a script can reach them at all — and it costs an ioctl only on
        // a serial link, where `modem_lines` answers `None` for everything
        // else without touching the port.
        let modem = s.modem_lines();
        json!({
            "connected": s.is_connected(),
            "transport": s.describe(),
            "title": h.title().unwrap_or_else(|| s.vt().window_title()),
            "cols": s.grid().cols(),
            "rows": s.grid().rows(),
            "scrollback": s.scrollback_len(),
            "log": s.log_path().map(|p| p.display().to_string()),
            "macro": { "running": m.running, "exit": m.exit },
            // Deviation 24. Every number belongs to the current connection, or
            // to the last one when `live` is false.
            "counters": {
                "bytes_in": c.bytes_in,
                "bytes_out": c.bytes_out,
                "lines_in": c.lines_in,
                "breaks": c.breaks,
                "rate_in": c.rate_in,
                "rate_out": c.rate_out,
                // `null`, where the C ABI spells this -1: a sentinel is right
                // in a struct with no room for absence and wrong in JSON,
                // which has a spelling of its own.
                "connected_ms": c.connected_for.map(|d| d.as_millis() as u64),
                "live": c.live,
            },
            // Null on every link that is not serial, and on a serial port whose
            // read failed — one answer for both, as `getmodemstatus` gives a
            // macro one.
            "modem": modem.map(|l| json!({
                "cts": l.cts, "dsr": l.dsr, "ri": l.ri, "cd": l.cd,
            })),
        })
    }))
}

/// `send` and `sendln`'s argument.
///
/// Exactly one of the three, and the choice is not a convenience: `text` goes
/// through [`Session::send_text`], where a newline is expanded by `ts.CRSend`
/// and the string is encoded, and the other two through
/// [`Session::send_bytes`], which puts what it is given on the wire. The macro
/// language draws the same line between `sendtext` and `sendbinary` and
/// `AGENTS.md` records what picking one for both costs.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SendParams {
    #[serde(default)]
    text: Option<String>,
    /// Raw bytes, as numbers. Legible in a shell and adequate for the control
    /// characters this is actually used for.
    #[serde(default)]
    bytes: Option<Vec<u8>>,
    /// The same, for anything long enough that an array would be silly.
    #[serde(default)]
    base64: Option<String>,
}

impl SendParams {
    /// What to put on the wire, and whether it is text.
    fn resolve(self) -> Result<(Vec<u8>, bool), RpcError> {
        match (self.text, self.bytes, self.base64) {
            (Some(t), None, None) => Ok((t.into_bytes(), true)),
            (None, Some(b), None) => Ok((b, false)),
            (None, None, Some(b)) => Ok((decode_base64(&b)?, false)),
            (None, None, None) => Err(RpcError::new(
                RpcError::BAD_PARAMS,
                "one of text, bytes or base64",
            )),
            _ => Err(RpcError::new(
                RpcError::BAD_PARAMS,
                "only one of text, bytes or base64",
            )),
        }
    }
}

fn send(tx: &CtlSender, p: Option<Value>) -> Result<Value, RpcError> {
    let (data, text) = params::<SendParams>(p)?.resolve()?;
    put(tx, data, text)
}

/// `sendln`, which is `send` with a line ending — and which ending is the
/// point.
///
/// A **CR**, both ways, exactly as the macro language's own `sendln` appends
/// one (`ttl.cpp`). It is the *text* path that then expands it by `ts.CRSend`
/// — CR, CRLF or LF (`ttcmn.c:814`) — so the same request puts `go\r` on the
/// wire for a default host and `go\r\n` for one configured for CRLF, while
/// the binary path always puts `go\r`. Appending an LF here instead would
/// send a bare LF under every setting, because
/// [`Vt::encode_text`](tt_vt::Vt::encode_text) translates the CR and nothing
/// else.
fn sendln(tx: &CtlSender, p: Option<Value>) -> Result<Value, RpcError> {
    let (mut data, text) = params::<SendParams>(p)?.resolve()?;
    data.push(b'\r');
    put(tx, data, text)
}

fn put(tx: &CtlSender, data: Vec<u8>, text: bool) -> Result<Value, RpcError> {
    let n = data.len();
    let connected = gone(tx.call(move |s, _| {
        if !s.is_connected() {
            return false;
        }
        if text {
            // Lossy rather than an error: the bytes came out of a JSON string,
            // so they are UTF-8 by construction and the branch cannot be
            // taken. It is written this way so that a future caller handing in
            // arbitrary bytes gets the terminal's own substitution rather than
            // a panic.
            let _ = s.send_text(&String::from_utf8_lossy(&data));
        } else {
            let _ = s.send_bytes(&data);
        }
        true
    }))?;
    if !connected {
        return Err(RpcError::new(
            RpcError::NOT_CONNECTED,
            "nothing is connected",
        ));
    }
    Ok(json!({ "sent": n }))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ConnectParams {
    /// A Tera Term command line — the same string the macro language's
    /// `connect` takes, through the same two parsers, so
    /// `"myhost /ssh /auth=publickey"` means here what it means there.
    line: String,
}

fn connect(tx: &CtlSender, p: Option<Value>) -> Result<Value, RpcError> {
    let line = params::<ConnectParams>(p)?.line;
    let r = gone(tx.call(move |_, h| h.connect(line.as_bytes())))?;
    match r {
        Ok(()) => Ok(json!({ "started": true })),
        Err(e) => Err(RpcError::new(RpcError::REFUSED, e)),
    }
}

fn disconnect(tx: &CtlSender) -> Result<Value, RpcError> {
    let was = gone(tx.call(|s, _| {
        let was = s.is_connected();
        s.disconnect();
        was
    }))?;
    Ok(json!({ "disconnected": was }))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ScreenParams {
    /// How many lines of history to put above the screen. Clamped to what
    /// there is.
    #[serde(default)]
    scrollback: usize,
    /// Drop trailing blanks from each line. On by default, because a screen
    /// read for a prompt is read with `grep`.
    #[serde(default = "yes")]
    trim: bool,
}

fn yes() -> bool {
    true
}

/// The terminal as text.
///
/// This is the half of the socket that *reads*, and it reads the same place a
/// macro's `wait` does not: `wait` matches the log tap's stream — the
/// characters as they were printed, in order — and this is the grid, which is
/// what the screen looks like now. A prompt that was redrawn appears once
/// here and twice there. Both are right for their question.
///
/// A wide character's padding cell contributes nothing, so a row of CJK comes
/// back at half the column count and the string is the text rather than the
/// geometry.
fn screen(tx: &CtlSender, p: Option<Value>) -> Result<Value, RpcError> {
    let p = params::<ScreenParams>(p)?;
    gone(tx.call(move |s, _| {
        let rows = s.grid().rows();
        let history = p.scrollback.min(s.scrollback_len());
        let mut lines = Vec::with_capacity(history + rows);
        // `line_at(0)` is the first line on the screen in the buffer's own
        // numbering, so the history is what lies before it.
        let first = s.line_at(0).saturating_sub(history as u64);
        for n in first..s.line_at(0) {
            lines.push(text_of(s.line(n).unwrap_or(&[]), p.trim));
        }
        for y in 0..rows {
            lines.push(text_of(s.row(y), p.trim));
        }
        json!({
            "cols": s.grid().cols(),
            "rows": rows,
            "history": history,
            "cursor": { "col": s.grid().cursor.x, "row": s.grid().cursor.y },
            "lines": lines,
        })
    }))
}

fn text_of(cells: &[tt_grid::Cell], trim: bool) -> String {
    let mut out = String::new();
    for cell in cells {
        // The right half of a wide character holds no text of its own; taking
        // its blank would put a space inside every CJK glyph. A control mark is
        // the terminal annotating its own screen, and a client asking what the
        // host said must get what the host said.
        if cell.width_class == WIDTH_PAD || cell.attrs & tt_grid::ATTR_CONTROL != 0 {
            continue;
        }
        for cp in cell.codepoints() {
            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
        }
    }
    if trim {
        // Only the end: leading spaces are indentation and a client reading a
        // prompt wants them.
        while out.ends_with(' ') {
            out.pop();
        }
    }
    out
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct MacroParams {
    path: String,
    /// The macro's own arguments — `param1`..`param9` and `params[]`, which
    /// `ttpmacro`'s command line would have supplied.
    #[serde(default)]
    params: Vec<String>,
    /// Answer when the macro *ends* rather than when it starts. What a `.bat`
    /// wrapper needs, since `ttpmacro.exe` is a process it waits on.
    #[serde(default)]
    wait: bool,
}

fn macro_run(tx: &CtlSender, p: Option<Value>) -> Result<Value, RpcError> {
    let p = params::<MacroParams>(p)?;
    // Relative to the *client's* directory, not the window's: a path typed in
    // a shell means what it says there. `ttctl` resolves it before sending,
    // and this is the second half of the same promise for anything that does
    // not.
    let path = PathBuf::from(&p.path);
    let args = p.params.clone();
    match gone(tx.call(move |_, h| h.run_macro(&path, &args)))? {
        Ok(()) => {}
        // The two are separate codes because a client tells them apart for a
        // living: `MACRO_RUNNING` is worth trying again in a second and
        // `FAILED` never is.
        Err(RunError::Busy(m)) => return Err(RpcError::new(RpcError::MACRO_RUNNING, m)),
        Err(RunError::Failed(m)) => return Err(RpcError::new(RpcError::FAILED, m)),
    }
    if !p.wait {
        return Ok(json!({ "started": true }));
    }
    loop {
        let m = gone(tx.call(|_, h| h.macro_status()))?;
        if !m.running {
            return Ok(json!({ "started": true, "ended": true, "exit": m.exit }));
        }
        std::thread::sleep(POLL);
    }
}

fn macro_stop(tx: &CtlSender) -> Result<Value, RpcError> {
    let was = gone(tx.call(|_, h| {
        let was = h.macro_status().running;
        h.stop_macro();
        was
    }))?;
    // The stop is a request, not an event: a macro parked in a `wait` notices
    // at its next poll. `was` is whether there was one to ask, which is all
    // that can honestly be said from here.
    Ok(json!({ "stopped": was }))
}

fn close(tx: &CtlSender) -> Result<Value, RpcError> {
    let ok = gone(tx.call(|_, h| h.close_window()))?;
    if !ok {
        return Err(RpcError::new(
            RpcError::REFUSED,
            "this frontend will not close its window",
        ));
    }
    Ok(json!({ "closed": true }))
}

/// Base64, without a dependency for eleven lines of table lookup.
///
/// RFC 4648's alphabet, padding required. Whitespace is skipped, because a
/// shell pipeline puts newlines in the middle of base64 and refusing that
/// would be refusing the way it is produced.
fn decode_base64(s: &str) -> Result<Vec<u8>, RpcError> {
    fn value(b: u8) -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a') as u32 + 26,
            b'0'..=b'9' => (b - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !bytes.len().is_multiple_of(4) {
        return Err(RpcError::new(RpcError::BAD_PARAMS, "base64: bad length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        if pad > 2 {
            return Err(RpcError::new(RpcError::BAD_PARAMS, "base64: bad padding"));
        }
        let mut acc = 0u32;
        for &b in &chunk[..4 - pad] {
            acc = (acc << 6)
                | value(b)
                    .ok_or_else(|| RpcError::new(RpcError::BAD_PARAMS, "base64: bad character"))?;
        }
        acc <<= 6 * pad;
        for i in 0..3 - pad {
            out.push((acc >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{CtlHost, NullHost};
    use tt_session::Session;
    use tt_vt::Config;

    /// Run a method against a session on this thread, the way the frontend
    /// would — a thread for the request, and a loop here servicing it.
    fn run(session: &mut Session, method: &'static str, p: Value) -> Result<Value, RpcError> {
        run_with(session, &mut NullHost, method, p)
    }

    fn run_with(
        session: &mut Session,
        host: &mut dyn CtlHost,
        method: &'static str,
        p: Value,
    ) -> Result<Value, RpcError> {
        let (tx, rx) = crate::channel::channel().unwrap();
        let thread = std::thread::spawn(move || call(&tx, method, Some(p)));
        loop {
            rx.service(session, host);
            if thread.is_finished() {
                // Anything posted between the last service and the join.
                rx.service(session, host);
                return thread.join().unwrap();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn session() -> Session {
        Session::new(Config::default())
    }

    #[test]
    fn an_unknown_method_says_which() {
        let mut s = session();
        let e = run(&mut s, "no.such.thing", json!({})).unwrap_err();
        assert_eq!(e.code, RpcError::NO_METHOD);
        assert!(e.message.contains("no.such.thing"));
    }

    #[test]
    fn status_reports_the_terminal() {
        let mut s = session();
        s.feed(b"\x1b]0;a title\x07");
        let v = run(&mut s, "status", json!({})).unwrap();
        assert_eq!(v["connected"], json!(false));
        assert_eq!(v["title"], json!("a title"));
        assert_eq!(v["cols"], json!(80));
        assert_eq!(v["macro"]["running"], json!(false));
    }

    /// Nothing has connected, so the counters are the shape a script has to
    /// read before it asserts anything: present, zero, and honest about the
    /// two things that have no value yet.
    #[test]
    fn status_reports_the_counters() {
        let mut s = session();
        let v = run(&mut s, "status", json!({})).unwrap();
        assert_eq!(v["counters"]["bytes_in"], json!(0));
        assert_eq!(v["counters"]["lines_in"], json!(0));
        assert_eq!(v["counters"]["live"], json!(false));
        // Absent rather than -1 or 0: neither of those is "never connected".
        assert_eq!(v["counters"]["connected_ms"], json!(null));
        // And a session with no serial port has no control lines to report.
        assert_eq!(v["modem"], json!(null));
    }

    #[test]
    fn ping_answers_from_the_window() {
        let mut s = session();
        let v = run(&mut s, "ping", json!({})).unwrap();
        assert_eq!(v["pid"], json!(std::process::id()));
    }

    /// The screen is the grid, and a redrawn prompt appears once.
    #[test]
    fn screen_reads_the_grid() {
        let mut s = session();
        s.feed(b"hello\r\nworld");
        let v = run(&mut s, "screen", json!({})).unwrap();
        let lines = v["lines"].as_array().unwrap();
        assert_eq!(lines[0], json!("hello"));
        assert_eq!(lines[1], json!("world"));
        assert_eq!(v["cursor"], json!({"col": 5, "row": 1}));
        // Trimmed, so a blank row is empty rather than eighty spaces.
        assert_eq!(lines[2], json!(""));
    }

    #[test]
    fn screen_can_be_asked_not_to_trim() {
        let mut s = session();
        s.feed(b"hi");
        let v = run(&mut s, "screen", json!({ "trim": false })).unwrap();
        assert_eq!(v["lines"][0].as_str().unwrap().len(), 80);
    }

    /// A wide character occupies two columns and contributes one character,
    /// so a row of CJK is not padded with the spaces its padding cells hold.
    #[test]
    fn a_wide_character_is_one_character_in_the_text() {
        let mut s = session();
        s.feed("日本".as_bytes());
        let v = run(&mut s, "screen", json!({})).unwrap();
        assert_eq!(v["lines"][0], json!("日本"));
    }

    #[test]
    fn scrollback_comes_back_above_the_screen() {
        let mut s = session();
        for i in 0..30 {
            s.feed(format!("line {i}\r\n").as_bytes());
        }
        let v = run(&mut s, "screen", json!({ "scrollback": 3 })).unwrap();
        assert_eq!(v["history"], json!(3));
        let lines = v["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 3 + 24);
        // Thirty lines and a trailing newline is thirty-one, of which the last
        // twenty-four are the screen — so it starts at `line 7` and the three
        // above it are 4, 5 and 6.
        assert_eq!(lines[0], json!("line 4"));
        assert_eq!(lines[3], json!("line 7"));
    }

    /// Nothing is connected, so a send is refused rather than silently
    /// dropped — the one thing a script cannot recover from is being told a
    /// password went somewhere it did not.
    #[test]
    fn sending_with_no_connection_is_an_error() {
        let mut s = session();
        let e = run(&mut s, "send", json!({ "text": "x" })).unwrap_err();
        assert_eq!(e.code, RpcError::NOT_CONNECTED);
    }

    #[test]
    fn send_wants_exactly_one_of_its_three_spellings() {
        let mut s = session();
        assert_eq!(
            run(&mut s, "send", json!({})).unwrap_err().code,
            RpcError::BAD_PARAMS
        );
        assert_eq!(
            run(&mut s, "send", json!({ "text": "a", "bytes": [1] }))
                .unwrap_err()
                .code,
            RpcError::BAD_PARAMS
        );
    }

    #[test]
    fn an_unknown_parameter_is_reported_rather_than_ignored() {
        let mut s = session();
        let e = run(&mut s, "send", json!({ "txet": "typo" })).unwrap_err();
        assert_eq!(e.code, RpcError::BAD_PARAMS);
    }

    /// The two send paths differ in what they do to a line ending, which is
    /// the whole reason `send` has three spellings rather than one. With the
    /// default `CRSend` they agree, so the test that matters is the other
    /// setting.
    #[test]
    fn text_expands_its_line_ending_and_bytes_do_not() {
        let (mut s, h) = connected();
        assert!(run(&mut s, "sendln", json!({ "text": "go" })).is_ok());
        assert_eq!(h.outbound(), b"go\r");
        let (mut s, h) = connected();
        assert!(run(&mut s, "sendln", json!({ "bytes": [b'g', b'o'] })).is_ok());
        assert_eq!(h.outbound(), b"go\r");
        // ...and a bare `send` of bytes puts exactly them on the wire.
        let (mut s, h) = connected();
        assert!(run(&mut s, "send", json!({ "bytes": [0x00, 0xff] })).is_ok());
        assert_eq!(h.outbound(), &[0x00, 0xff]);
    }

    #[test]
    fn a_host_configured_for_crlf_gets_one_from_the_text_path_only() {
        let config = Config {
            cr_send: tt_vt::CrSend::CrLf,
            ..Config::default()
        };
        let (t, h) = tt_session::MemoryTransport::new();
        let mut s = Session::new(config.clone());
        s.connect(Box::new(t));
        assert!(run(&mut s, "sendln", json!({ "text": "go" })).is_ok());
        assert_eq!(h.outbound(), b"go\r\n");

        let (t, h) = tt_session::MemoryTransport::new();
        let mut s = Session::new(config);
        s.connect(Box::new(t));
        assert!(run(&mut s, "sendln", json!({ "bytes": [b'g', b'o'] })).is_ok());
        assert_eq!(h.outbound(), b"go\r", "the binary path is untouched");
    }

    #[test]
    fn base64_is_the_same_wire_as_bytes() {
        let (mut s, h) = connected();
        // "hi\n" — with the whitespace a shell pipeline would put in it.
        assert!(run(&mut s, "send", json!({ "base64": "aGkK\n" })).is_ok());
        assert_eq!(h.outbound(), b"hi\n");
    }

    #[test]
    fn bad_base64_is_a_parameter_error() {
        let (mut s, _h) = connected();
        assert_eq!(
            run(&mut s, "send", json!({ "base64": "aGk" }))
                .unwrap_err()
                .code,
            RpcError::BAD_PARAMS
        );
        assert_eq!(
            run(&mut s, "send", json!({ "base64": "a!k=" }))
                .unwrap_err()
                .code,
            RpcError::BAD_PARAMS
        );
    }

    #[test]
    fn disconnect_reports_whether_there_was_one() {
        let (mut s, _h) = connected();
        assert_eq!(
            run(&mut s, "disconnect", json!({})).unwrap()["disconnected"],
            json!(true)
        );
        assert_eq!(
            run(&mut s, "disconnect", json!({})).unwrap()["disconnected"],
            json!(false)
        );
    }

    /// A frontend with no macro support refuses rather than reporting a macro
    /// that is not running.
    #[test]
    fn a_frontend_that_cannot_run_macros_says_so() {
        let mut s = session();
        let e = run(&mut s, "macro.run", json!({ "path": "x.ttl" })).unwrap_err();
        assert_eq!(e.code, RpcError::FAILED);
        let e = run(&mut s, "close", json!({})).unwrap_err();
        assert_eq!(e.code, RpcError::REFUSED);
    }

    /// A host that runs a macro for two poll intervals and then stops, which
    /// is what `wait` has to sit through.
    struct Fake {
        left: u32,
        exit: i32,
        started: Option<PathBuf>,
        args: Vec<String>,
    }

    impl CtlHost for Fake {
        fn run_macro(&mut self, path: &std::path::Path, params: &[String]) -> Result<(), RunError> {
            self.started = Some(path.to_path_buf());
            self.args = params.to_vec();
            self.left = 2;
            Ok(())
        }
        fn macro_status(&mut self) -> crate::MacroStatus {
            let running = self.left > 0;
            self.left = self.left.saturating_sub(1);
            crate::MacroStatus {
                running,
                exit: self.exit,
            }
        }
        fn stop_macro(&mut self) {
            self.left = 0;
        }
    }

    #[test]
    fn a_macro_can_be_started_and_waited_for() {
        let mut s = session();
        let mut h = Fake {
            left: 0,
            exit: 7,
            started: None,
            args: vec![],
        };
        let v = run_with(
            &mut s,
            &mut h,
            "macro.run",
            json!({ "path": "login.ttl", "params": ["a", "b"], "wait": true }),
        )
        .unwrap();
        assert_eq!(v["ended"], json!(true));
        assert_eq!(v["exit"], json!(7));
        assert_eq!(h.started.unwrap(), PathBuf::from("login.ttl"));
        assert_eq!(h.args, vec!["a".to_string(), "b".to_string()]);
    }

    /// Without `wait` it answers immediately and the macro is still running.
    /// Busy is its own code, because a client retries one and gives up on the
    /// other.
    #[test]
    fn a_second_macro_is_refused_as_busy_rather_than_as_a_failure() {
        struct Busy;
        impl CtlHost for Busy {
            fn run_macro(&mut self, _: &std::path::Path, _: &[String]) -> Result<(), RunError> {
                Err(RunError::Busy("a macro is already running".into()))
            }
        }
        let mut s = session();
        let e = run_with(&mut s, &mut Busy, "macro.run", json!({ "path": "x.ttl" })).unwrap_err();
        assert_eq!(e.code, RpcError::MACRO_RUNNING);
    }

    #[test]
    fn a_macro_started_without_wait_answers_at_once() {
        let mut s = session();
        let mut h = Fake {
            left: 0,
            exit: 0,
            started: None,
            args: vec![],
        };
        let v = run_with(&mut s, &mut h, "macro.run", json!({ "path": "x.ttl" })).unwrap();
        assert_eq!(v, json!({ "started": true }));
        assert!(h.left > 0, "it is still going");
    }

    fn connected() -> (Session, tt_session::MemoryHandle) {
        let (t, h) = tt_session::MemoryTransport::new();
        let mut s = session();
        s.connect(Box::new(t));
        (s, h)
    }
}
