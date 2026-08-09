//! The same terminal, driven by the other language.
//!
//! `driving.rs` proves that `tt-ttl` and `tt-session` fit; this proves that
//! [`SessionHost`] is what both of them fit *through*. Nothing in `tt-macro`
//! is Lua-aware — the host was written for the macro language and is reused
//! unchanged — so what is being tested here is the seam rather than the glue,
//! and a divergence would mean the trait had grown a TTL assumption.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use tt_lua::Script;
use tt_macro::{channel, MacroError, MacroUi, SessionHost};
use tt_session::{MemoryTransport, Session};
use tt_vt::Config;

const LIMIT: Duration = Duration::from_secs(10);

struct Run {
    sent: Vec<u8>,
    screen: Vec<String>,
    error: Option<MacroError>,
}

/// A frontend that remembers the one dialog these tests raise.
#[derive(Default)]
struct Recorder {
    error: Option<MacroError>,
}

impl MacroUi for Recorder {
    fn error(&mut self, err: &MacroError) -> bool {
        self.error = Some(err.clone());
        true
    }
}

/// Run a Lua script against a session with a far end, in the frontend's own
/// loop: service the script's jobs, pump the line.
fn drive(script: &str, mut respond: impl FnMut(&[u8]) -> Vec<u8>) -> Run {
    let mut session = Session::new(Config {
        cols: 40,
        rows: 8,
        ..Config::default()
    });
    let (transport, far) = MemoryTransport::new();
    session.connect(Box::new(transport));
    let link = session.link_macro();
    let (tx, rx) = channel().unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    let body = script.as_bytes().to_vec();
    let thread = std::thread::spawn(move || {
        let mut host = SessionHost::new(tx, link);
        let r = Script::new("t.lua", body).run(&mut host);
        if let Err(e) = r {
            // What `tt_macro_start` does with the same error, minus the
            // cancellation test — nothing here cancels.
            host.report(&MacroError::elsewhere(e.to_string(), "t.lua".into()));
        }
        let _ = done_tx.send(());
    });

    let mut ui = Recorder::default();
    let mut sent = Vec::new();
    let start = Instant::now();
    let mut finished = false;
    loop {
        rx.service(&mut session, &mut ui);
        session.pump(Duration::from_millis(1)).unwrap();

        let out = far.with(|s| std::mem::take(&mut s.outbound));
        if !out.is_empty() {
            sent.extend_from_slice(&out);
            let reply = respond(&out);
            if !reply.is_empty() {
                far.feed(&reply);
            }
        }

        if finished {
            break;
        }
        if done_rx.try_recv().is_ok() {
            finished = true;
        }
        if start.elapsed() > LIMIT {
            rx.cancel();
            let _ = thread.join();
            panic!("script did not finish within {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    thread.join().unwrap();

    Run {
        screen: (0..session.grid().rows())
            .map(|y| row(&session, y))
            .collect(),
        sent,
        error: ui.error,
    }
}

fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        if cell.width_class == tt_grid::WIDTH_PAD {
            continue;
        }
        let mut any = false;
        for cp in cell.codepoints() {
            out.push(char::from_u32(cp).unwrap_or('?'));
            any = true;
        }
        if !any {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// The smallest whole thing, and the same conversation `driving.rs` opens
/// with: send, wait for the answer, send again. That is a login.
#[test]
fn a_script_holds_a_conversation() {
    let r = drive(
        "tt.timeout = 2
         tt.sendln('hello')
         tt.wait('world')
         tt.sendln('goodbye')",
        |out| match out {
            b"hello\r" => b"world\r\n".to_vec(),
            _ => Vec::new(),
        },
    );
    // A bare CR on the wire, for the same reason TTL's `sendln` puts one
    // there: `ts.CRSend` defaults to CR and the text path expands by it.
    assert_eq!(r.sent, b"hello\rgoodbye\r");
    assert_eq!(r.screen[0], "world");
}

/// The macro tap gives a script what the terminal **printed**, so a prompt
/// dressed in colour matches anyway — and one that waited for the escape
/// sequence would wait for ever.
#[test]
fn a_wait_matches_through_the_escape_sequences() {
    let r = drive(
        "tt.sendln('go'); tt.timeout = 2; tt.wait('login:'); tt.sendln('root')",
        |out| match out {
            // A prompt in bold green, positioned, with the cursor saved and
            // restored around it — none of which a script can see.
            b"go\r" => b"\x1b[2J\x1b[H\x1b7\x1b[1;32mlogin:\x1b[0m \x1b8".to_vec(),
            _ => Vec::new(),
        },
    );
    assert_eq!(r.sent, b"go\rroot\r");
    assert_eq!(r.screen[0], "login:");
}

/// `tt.timeout` is seconds, and a wait that does not match answers `nil` —
/// which is how a Lua script tells "the host answered" from "it did not", and
/// it is an `if` rather than TTL's `if result = 0`.
#[test]
fn a_wait_that_matches_nothing_answers_nil() {
    let r = drive(
        "tt.timeout = 1
         if not tt.wait('never') then tt.sendln('gave up') end",
        |_| Vec::new(),
    );
    assert_eq!(r.sent, b"gave up\r");
}

/// The half of the language TTL cannot express: the answer is a value, so it
/// composes with the rest of Lua rather than with `if result then`.
#[test]
fn a_line_read_back_is_an_ordinary_lua_string() {
    let r = drive(
        "tt.timeout = 2
         tt.sendln('who')
         tt.sendln(tt.recvln():upper())",
        |out| match out {
            b"who\r" => b"nata\r\n".to_vec(),
            _ => Vec::new(),
        },
    );
    assert_eq!(r.sent, b"who\rNATA\r");
}

/// `dispstr` paints the local screen without touching the connection, and
/// `print` is the same call with a line ending a terminal understands.
#[test]
fn print_reaches_the_screen_and_not_the_wire() {
    let r = drive("print('done')", |_| Vec::new());
    assert_eq!(r.screen[0], "done");
    assert!(r.sent.is_empty());
}

/// An uncaught error reaches the frontend's dialog by the same route TTL's
/// does, carrying its own position because Lua puts it in the message.
#[test]
fn an_error_reaches_the_frontends_dialog() {
    let r = drive("tt.sendln('x'); error('nope')", |_| Vec::new());
    let e = r.error.expect("no error reported");
    assert_eq!(e.code, 0, "a Lua error is not one of ttmparse.h's");
    assert!(e.message.contains("nope"), "{}", e.message);
    assert!(e.message.contains("t.lua:1"), "{}", e.message);
    // Everything before the error still happened.
    assert_eq!(r.sent, b"x\r");
}

/// And a refusal from the host is an ordinary Lua error, so a script can
/// decide for itself — which is the whole difference from `result`.
#[test]
fn a_host_refusal_is_catchable() {
    let r = drive(
        "local ok = pcall(tt.getmodemstatus); tt.sendln(tostring(ok))",
        |_| Vec::new(),
    );
    assert!(r.error.is_none());
    assert_eq!(r.sent, b"true\r");
}
