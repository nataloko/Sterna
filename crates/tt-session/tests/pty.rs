//! A session over a local shell.
//!
//! The same job `serial_loopback.rs` does for the wire — prove the composition
//! and not just the pieces — except this one needs no rig, so it runs
//! everywhere and is the only end-to-end session test that always executes.
//!
//! It is also the only place `close_note` is exercised end to end: the pty is
//! the transport that has something to say on its way out, and the whole
//! mechanism exists for it. This harness drives `/bin/sh` and `poll(2)`, so it
//! is the POSIX end-to-end test; ConPTY needs its own native Windows exercise.

#![cfg(unix)]

use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_session::{Event, Session};
use tt_vt::Config;

fn sh(script: &str) -> PtyParams {
    PtyParams {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        cols: 40,
        rows: 6,
        ..PtyParams::default()
    }
}

fn session() -> Session {
    Session::new(Config {
        cols: 40,
        rows: 6,
        ..Config::default()
    })
}

fn pump_until(s: &mut Session, dur: Duration, f: impl Fn(&Session) -> bool) -> Vec<Event> {
    let mut events = Vec::new();
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        s.pump(Duration::from_millis(20)).expect("pump");
        events.extend(s.drain_events());
        if f(s) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    events
}

fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        if cell.width_class == tt_grid::WIDTH_PAD {
            continue;
        }
        match cell.codepoints().next() {
            Some(0) | None => out.push(' '),
            Some(cp) => out.push(char::from_u32(cp).unwrap_or(' ')),
        }
    }
    out.trim_end().to_string()
}

#[test]
fn a_session_over_a_local_shell_renders_what_the_child_prints() {
    let mut s = session();
    // Two lines with a cursor move between them, so the parser is doing work
    // rather than the grid taking a memcpy.
    s.connect(Box::new(
        PtyConn::open(&sh("printf 'one\\r\\n'; printf '\\033[3;5Hfive'")).expect("open"),
    ));
    pump_until(&mut s, Duration::from_secs(5), |s| row(s, 2) == "    five");
    assert_eq!(row(&s, 0), "one");
    assert_eq!(row(&s, 2), "    five");
}

/// The child exits, the session says so, and the note says what a bare
/// "disconnected" cannot.
#[test]
fn the_shell_exiting_reaches_the_frontend_with_its_status() {
    let mut s = session();
    s.connect(Box::new(PtyConn::open(&sh("exit 7")).expect("open")));
    let events = pump_until(&mut s, Duration::from_secs(5), |s| !s.is_connected());

    assert!(
        events.contains(&Event::Disconnected),
        "no Disconnected in {events:?}"
    );
    assert!(!s.is_connected());
    assert_eq!(
        s.close_note(),
        Some("sh -c exit 7 exited with status 7"),
        "close_note was {:?}",
        s.close_note()
    );
    // And the descriptor goes with the transport, so a frontend that kept
    // watching the old one would be watching a closed fd.
    #[cfg(unix)]
    assert_eq!(s.poll_fd(), None);
}

#[test]
fn connecting_again_clears_the_previous_note() {
    let mut s = session();
    s.connect(Box::new(PtyConn::open(&sh("exit 1")).expect("open")));
    pump_until(&mut s, Duration::from_secs(5), |s| !s.is_connected());
    assert!(s.close_note().is_some());

    s.connect(Box::new(PtyConn::open(&sh("sleep 5")).expect("open")));
    assert_eq!(s.close_note(), None);
}

/// Typed input reaches the child and its answer comes back through the
/// parser — the round trip the window actually makes.
#[test]
fn what_is_typed_reaches_the_shell_and_the_answer_comes_back() {
    let mut s = session();
    s.connect(Box::new(
        PtyConn::open(&sh(
            "read line; printf '\\033[2J\\033[1;1Hgot %s' \"$line\"",
        ))
        .expect("open"),
    ));
    s.send_text("hello\r").expect("send");
    pump_until(&mut s, Duration::from_secs(5), |s| row(s, 0) == "got hello");
    assert_eq!(row(&s, 0), "got hello");
}

/// Resizing the terminal has to reach the child's `winsize`, or every
/// full-screen program in the shell draws at the wrong width.
#[test]
fn resizing_the_session_resizes_the_pty() {
    let mut s = session();
    s.connect(Box::new(
        PtyConn::open(&sh("sleep 0.3; stty size")).expect("open"),
    ));
    s.resize(90, 30).expect("resize");
    pump_until(&mut s, Duration::from_secs(5), |s| {
        row(s, 0).starts_with("30 90")
    });
    assert_eq!(row(&s, 0), "30 90");
}

/// The pty is the transport with no break, and the session has to report that
/// rather than the frontend guessing from the connection type.
#[test]
fn a_local_shell_offers_no_break() {
    let mut s = session();
    assert!(!s.supports_break());
    s.connect(Box::new(PtyConn::open(&sh("sleep 5")).expect("open")));
    assert!(!s.supports_break());
    assert_eq!(s.describe().as_deref(), Some("sh -c sleep 5"));
}

/// The descriptor sleeps while the child is quiet and wakes when it speaks.
/// This is the contract the shell's event loop is built on, and asserting the
/// fd is merely *returned* would prove none of it.
#[cfg(unix)]
#[test]
fn the_descriptor_sleeps_and_then_wakes() {
    let mut s = session();
    s.connect(Box::new(
        PtyConn::open(&sh("sleep 0.4; printf ready")).expect("open"),
    ));
    let fd = s.poll_fd().expect("a pty has a descriptor");

    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // Quiet: nothing to wake on.
    assert_eq!(unsafe { libc::poll(&mut pfd, 1, 100) }, 0);
    // And then the child speaks.
    assert_eq!(unsafe { libc::poll(&mut pfd, 1, 5000) }, 1);

    s.pump(Duration::ZERO).expect("pump");
    assert_eq!(row(&s, 0), "ready");
}
