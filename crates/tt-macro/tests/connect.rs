//! A macro opening its own connection — `connect` and `cygconnect`.
//!
//! The session here starts with **nothing attached**, which is what `/DS` gives
//! a macro-driven terminal upstream, and every test below is the real thing on
//! both sides: a real command line through both parsers, a real transport, and
//! a real `tt_ttl::Interp` on its own thread. Two of them open something that
//! is genuinely there — a local shell and a TCP listener this file binds — so
//! what they prove is the whole path and not a mapping.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tt_conn::Transport;
use tt_macro::{channel, MacroUi, NullUi, SessionHost};
use tt_session::open::Target;
use tt_session::Session;
use tt_ttl::{Interp, TtlError};
use tt_vt::Config;

/// How long a test will wait for a macro before calling it hung.
const LIMIT: Duration = Duration::from_secs(10);

/// Run a script against a session with no connection, and hand the session
/// back so a test can ask what happened to it.
///
/// The loop is the frontend's: service the macro's jobs, pump the line. It is
/// the same pair of calls `tests/driving.rs` makes and the same two the Qt
/// shell makes from its notifiers — the only difference is that here the
/// transport arrives *during* the run, which is the thing being tested.
fn drive(script: &str, ui: &mut dyn MacroUi) -> Session {
    let mut session = Session::new(Config {
        cols: 60,
        rows: 8,
        ..Config::default()
    });
    let link = session.link_macro();
    let (tx, rx) = channel().unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    let body = script.as_bytes().to_vec();
    let thread = std::thread::spawn(move || {
        let mut host = SessionHost::new(tx, link);
        let mut it = Interp::new("t.ttl", body, &mut host);
        it.run(&mut host);
        let _ = done_tx.send(());
    });

    let start = Instant::now();
    let mut finished = false;
    loop {
        rx.service(&mut session, ui);
        let _ = session.pump(Duration::from_millis(1));
        if finished {
            break;
        }
        if done_rx.try_recv().is_ok() {
            finished = true;
        }
        if start.elapsed() > LIMIT {
            rx.cancel();
            let _ = thread.join();
            panic!("macro did not finish within {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    thread.join().unwrap();
    session
}

fn screen(s: &Session) -> Vec<String> {
    (0..s.grid().rows())
        .map(|y| {
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
        })
        .collect()
}

/// `cygconnect` is a local shell here, and this is the whole path: CygTerm's
/// command line, split by two different splitters, into a pty whose output the
/// macro then waits for.
#[test]
fn cygconnect_opens_a_shell_and_the_macro_reads_what_it_printed() {
    #[cfg(unix)]
    let command = "/bin/echo sterna-ok";
    #[cfg(windows)]
    let command = "cmd.exe /d /c echo sterna-ok";
    let script = format!(
        "cygconnect \"-s '{command}' -nols\"\ntimeout = 5\nwait 'sterna-ok'\n\
         if result = 1 then\n  dispstr 'matched'\nendif"
    );
    let s = drive(&script, &mut NullUi);
    assert!(
        screen(&s).iter().any(|l| l == "matched"),
        "{:?}",
        screen(&s)
    );
}

/// A `connect` that names a host opens it, and `testlink` reports the pair as
/// the documented 2 — link **and** connection.
#[test]
fn connect_opens_a_tcp_session_and_testlink_says_so() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let far = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        sock.write_all(b"banner\r\n").unwrap();
        // To the end rather than one read: closing early would hang the
        // session up under the test and make "still connected" impossible to
        // assert. It returns when the session is dropped below.
        let mut buf = Vec::new();
        let _ = sock.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });

    // `/T=0` is telnet **off**, which is raw — no negotiation, so what the
    // listener wrote is what the terminal shows. `/nossh` is TTSSH's, and
    // without it the port type would be whatever the settings file last said.
    let script = format!(
        "connect '127.0.0.1:{port} /nossh /T=0'\ntestlink\nif result = 2 then\n  \
         sendln 'linked'\nendif"
    );
    let s = drive(&script, &mut NullUi);
    assert!(
        s.is_connected(),
        "the transport was attached to the session"
    );
    assert_eq!(screen(&s)[0], "banner");
    drop(s);
    assert!(far.join().unwrap().starts_with("linked"));
}

/// A `connect` that cannot connect is **not** an error: the macro is told by
/// `result`, which is what the documentation promises for every failure.
#[test]
fn a_connection_that_is_refused_reports_one_rather_than_failing() {
    // Port 1 on the loopback, which nothing is listening on.
    let s = drive(
        "connect '127.0.0.1:1 /nossh /T=0'\nif result = 1 then\n  dispstr 'one'\nendif",
        &mut NullUi,
    );
    assert!(!s.is_connected());
    assert_eq!(screen(&s)[0], "one");
}

/// `connect ''` names nothing. Upstream would put the New Connection dialog up;
/// with no dialog to reach, it is the other arm of the same `if` —
/// `SetDdeComReady(0)`, which the macro reads as 1.
#[test]
fn a_connect_that_names_nothing_leaves_the_terminal_alone() {
    let s = drive(
        "connect ''\nif result = 1 then\n  dispstr 'nothing'\nendif",
        &mut NullUi,
    );
    assert!(!s.is_connected());
    assert_eq!(screen(&s)[0], "nothing");
}

/// The line is parsed **into the settings** and what it set stays there, which
/// is `ParseParam(commandline, &ts, NULL)` and is why a shortcut converted into
/// a macro behaves the same way twice.
#[test]
fn what_the_command_line_set_is_still_set_afterwards() {
    let s = drive(
        "connect '127.0.0.1:1 /nossh /T=0 /BAUD=115200 /W=Router'",
        &mut NullUi,
    );
    assert_eq!(s.settings().serial_baud, 115200);
    assert_eq!(s.settings().terminal_title, "Router");
}

/// `unlink` gives the terminal up and `connect` takes it back — upstream
/// launches a second Tera Term at this point, and in-process the terminal is
/// the caller, so there is one to re-acquire.
#[test]
fn connect_after_an_unlink_links_again() {
    let s = drive("unlink\nconnect '127.0.0.1:1 /nossh /T=0'", &mut NullUi);
    assert!(s.macro_linked(), "the ring was taken back");

    let s = drive("unlink", &mut NullUi);
    assert!(!s.macro_linked(), "and without a connect it stays given up");
}

/// SSH is the one target that leaves this crate, because its host key and its
/// password are prompts. What the frontend is handed is the whole
/// [`Target::Ssh`], TTSSH's options included.
#[test]
fn an_ssh_line_is_handed_to_the_frontend_rather_than_opened() {
    #[derive(Default)]
    struct Recorder {
        asked: Vec<String>,
    }
    impl MacroUi for Recorder {
        fn connect_ssh(&mut self, target: &Target) -> Result<Option<Box<dyn Transport>>, TtlError> {
            let Target::Ssh {
                params,
                port_chosen,
                password,
                ask_password,
                ..
            } = target
            else {
                panic!("connect_ssh was handed {target:?}");
            };
            self.asked.push(format!(
                "{}@{}:{} chosen={port_chosen} pw={} ask={ask_password}",
                params.user,
                params.host,
                params.port,
                password.as_deref().unwrap_or("-"),
            ));
            // A frontend that could not connect — which is also the default.
            Ok(None)
        }
    }

    let mut ui = Recorder::default();
    let s = drive(
        "connect 'myhost /ssh /auth=password /user=alice /passwd=hunter2 /ask4passwd'\n\
         if result = 1 then\n  dispstr 'asked'\nendif",
        &mut ui,
    );
    assert_eq!(
        ui.asked,
        ["alice@myhost:22 chosen=false pw=hunter2 ask=true"],
        "the port is SSH's own, since the line asked for none"
    );
    assert!(!s.is_connected());
    assert_eq!(screen(&s)[0], "asked");
}

/// And a frontend that never opened one — `NullUi`, which is `ttpmacro` with
/// no window — reports a connection that did not come up rather than an
/// unknown command.
#[test]
fn ssh_without_a_frontend_is_a_failed_connection_and_not_an_error() {
    let s = drive(
        "connect 'myhost /ssh'\nif result = 1 then\n  dispstr 'one'\nendif",
        &mut NullUi,
    );
    assert!(!s.is_connected());
    assert_eq!(screen(&s)[0], "one");
}

/// The connection a `connect` opened is one a later `disconnect` closes, and
/// the terminal survives it — which is the difference between `disconnect` and
/// `closett`.
#[test]
fn disconnect_closes_what_connect_opened() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let far = std::thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        // Read to the end: the macro's `disconnect` is what closes it.
        let mut buf = Vec::new();
        let mut sock = sock;
        let _ = sock.read_to_end(&mut buf);
    });

    let script = format!(
        "connect '127.0.0.1:{port} /nossh /T=0'\ntestlink\nresult1 = result\n\
         disconnect 0\ntestlink\nif result1 = 2 then\n  if result = 1 then\n    \
         dispstr 'closed'\n  endif\nendif"
    );
    let s = drive(&script, &mut NullUi);
    assert!(!s.is_connected());
    assert_eq!(screen(&s)[0], "closed");
    far.join().unwrap();
}
