//! A file transfer over the session's own connection.
//!
//! `tt-xfer`'s own suite proves the protocols interoperate. What is proved
//! here is the composition: that the session hands the byte stream over and
//! takes it back, that the terminal goes deaf for the duration, and that the
//! frontend is told enough to draw a dialog and to know when to stop.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_session::{Event, Session, TransferError, TransferOutcome, TransferReply};
use tt_vt::Config;
use tt_xfer::{Direction, Job, Link, Options};

fn have(tool: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn payload(path: &Path, size: usize) {
    let mut f = std::fs::File::create(path).unwrap();
    let body: Vec<u8> = (0..size).map(|i| (i * 31 + i / 251) as u8).collect();
    f.write_all(&body).unwrap();
}

fn opts() -> Options {
    Options {
        link: Link::local_pty(),
        ..Options::default()
    }
}

const ZSEND: Job = Job::ZModem {
    dir: Direction::Send,
    binary: true,
    auto: false,
};

/// Pump until the transfer reports done, and return how it went.
///
/// The loop a frontend runs: pump, drain events, and sleep no longer than
/// [`Session::transfer_deadline`] says. That last part is the one a frontend
/// can get wrong — a quiet line produces no descriptor wakeup at all, and the
/// protocols retry by timeout.
fn run(session: &mut Session, limit: Duration) -> TransferOutcome {
    let start = Instant::now();
    loop {
        session.pump(Duration::from_millis(20)).unwrap();
        for ev in session.drain_events() {
            if let Event::TransferDone(outcome) = ev {
                return *outcome;
            }
        }
        assert!(
            start.elapsed() < limit,
            "the transfer never finished: {:?}",
            session.transfer()
        );
        if let Some(d) = session.transfer_deadline() {
            std::thread::sleep(d.min(Duration::from_millis(20)));
        }
    }
}

fn session_on(cmd: &str, cwd: &Path) -> Session {
    let conn = PtyConn::open(&PtyParams {
        argv: vec!["sh".into(), "-c".into(), format!("{cmd} 2>/dev/null")],
        cwd: Some(cwd.to_path_buf()),
        login_shell: false,
        ..PtyParams::default()
    })
    .expect("cannot open a pty");
    let mut session = Session::new(Config::default());
    session.connect(Box::new(conn));
    session
}

#[test]
fn a_file_goes_out_over_the_sessions_own_connection() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 64 * 1024);

    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();
    assert!(session.transfer().is_some());

    let outcome = run(&mut session, Duration::from_secs(60));
    assert!(outcome.success, "{outcome:?}");
    assert!(!outcome.cancelled);
    assert_eq!(
        std::fs::read(out.join("payload.bin")).unwrap(),
        std::fs::read(&src).unwrap()
    );
    // And the session is a terminal again.
    assert!(session.transfer().is_none());
}

/// The outcome also reaches somebody *waiting* for it on another thread, which
/// is what a macro's transfer command blocks on: the event queue is the
/// frontend's way of hearing and no use to a script that is not draining it.
#[test]
fn a_finished_transfer_answers_a_waiting_caller() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 4096);

    let mut session = session_on("rz -b", &out);
    let reply = TransferReply::new();
    session.send_files(ZSEND, &[&src], &opts()).unwrap();
    session.notify_transfer(reply.clone());

    // The waiter, which is the macro thread's shape: block in short turns so
    // that it would still notice its own End button, and give up long after
    // `run` below would have panicked.
    let waiter = std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(90) {
            if let Some(o) = reply.wait(Duration::from_millis(50)) {
                return Some(o);
            }
        }
        None
    });

    let outcome = run(&mut session, Duration::from_secs(60));
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(waiter.join().unwrap(), Some(outcome));
}

/// The protocol's traffic must not reach the parser.
///
/// A ZMODEM header is `**\x18B00…` and a screenful of that is what a terminal
/// that kept feeding the VT engine would paint. The grid staying empty is the
/// assertion.
#[test]
fn the_terminal_sees_none_of_the_protocol() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 4096);

    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();
    let outcome = run(&mut session, Duration::from_secs(60));
    assert!(outcome.success, "{outcome:?}");

    let painted: String = (0..session.grid().rows())
        .flat_map(|y| {
            session
                .row(y)
                .iter()
                .filter_map(|c| char::from_u32(c.text[0]))
        })
        .filter(|c| !c.is_whitespace() && *c != '\0')
        .collect();
    assert!(
        painted.is_empty(),
        "the parser was fed the protocol: {painted:?}"
    );
}

/// And nothing the user could type reaches the peer.
///
/// One stray byte in the middle of a packet is a corrupted file, so the
/// terminal is mute as well as deaf — which is what upstream's modal transfer
/// dialog achieves by other means.
#[test]
fn typing_during_a_transfer_goes_nowhere() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 64 * 1024);

    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();

    let start = Instant::now();
    loop {
        // Type into it on every turn, which is what an impatient user does.
        // Before the pump, so the last round still happens while the transfer
        // is up rather than after it has ended.
        session.send_text("rm -rf /\r").unwrap();
        assert!(!session.send_key(tt_vt::Key::KpEnter).unwrap());
        session.paste("and a paste as well").unwrap();

        session.pump(Duration::from_millis(20)).unwrap();
        let mut done = None;
        for ev in session.drain_events() {
            if let Event::TransferDone(o) = ev {
                done = Some(*o);
            }
        }
        if let Some(o) = done {
            assert!(o.success, "{o:?}");
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "never finished");
    }

    // If any of that had reached `rz` the file would not match.
    assert_eq!(
        std::fs::read(out.join("payload.bin")).unwrap(),
        std::fs::read(&src).unwrap()
    );
}

#[test]
fn a_transfer_needs_a_connection_and_only_one_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    payload(&src, 16);

    let mut session = Session::new(Config::default());
    assert_eq!(
        session.send_files(ZSEND, &[&src], &opts()).unwrap_err(),
        TransferError::NotConnected
    );

    if !have("rz") {
        return;
    }
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();
    assert_eq!(
        session.send_files(ZSEND, &[&src], &opts()).unwrap_err(),
        TransferError::AlreadyRunning
    );
}

/// Cancelling a transfer whose peer never answers must still end it.
///
/// The hard case, and the reason it is run over a transport with nothing on
/// the other end: ZMODEM's cancel is not a state change but a state change
/// plus a timer. It sends `ZCAN`, arms 500 ms through `SetTimer`
/// (`zmodem.c:1586`) and finishes on that — so with a silent line there is no
/// descriptor wakeup to carry it, and only the session honouring
/// [`Session::transfer_deadline`] gets the user out of the dialog they just
/// asked to close.
#[test]
fn cancelling_a_transfer_nobody_answered_still_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    payload(&src, 4096);

    let mut session = Session::new(Config::default());
    let (transport, peer) = tt_session::MemoryTransport::new();
    session.connect(Box::new(transport));
    session.send_files(ZSEND, &[&src], &opts()).unwrap();

    // Let it say hello to nobody.
    session.pump(Duration::from_millis(20)).unwrap();
    session.drain_events();
    assert!(
        !peer.outbound().is_empty(),
        "the protocol sent nothing at all"
    );
    assert!(session.transfer().is_some());

    session.cancel_transfer();
    let start = Instant::now();
    let outcome = run(&mut session, Duration::from_secs(10));
    assert!(outcome.cancelled);
    assert!(!outcome.success);
    assert!(session.transfer().is_none());
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "cancelling took {:?}; the 500 ms timer is not being honoured",
        start.elapsed()
    );
}

/// A cancelled transfer over a real peer reports itself as one, and never as
/// a success — ZMODEM's own verdict on a cancel is `Success`, because
/// `zmodem.c:1047` sets it on the `ZFIN` the cancel provokes.
#[test]
fn cancelling_a_running_transfer_is_never_a_success() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 4 * 1024 * 1024);

    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();

    // One pump is enough to have the conversation under way. Waiting for
    // `progress.bytes` to move would not be: the protocols throttle their
    // progress reporting to ten updates a second (`zmodem.c:197`), and four
    // megabytes over a pty is gone in less than that.
    session.pump(Duration::from_millis(20)).unwrap();
    session.drain_events();

    session.cancel_transfer();
    let outcome = run(&mut session, Duration::from_secs(15));
    assert!(outcome.cancelled);
    assert!(!outcome.success);
    assert!(session.transfer().is_none());
}

/// The connection going away under a transfer ends it, and the frontend hears
/// about both — the transfer first, because "the transfer failed" is the more
/// specific of the two and a dialog is waiting on it.
#[test]
fn a_dropped_connection_ends_the_transfer_too() {
    if !have("rz") {
        eprintln!("skipping: lrzsz is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, 4 * 1024 * 1024);

    let mut session = session_on("rz -b", &out);
    session.send_files(ZSEND, &[&src], &opts()).unwrap();
    session.pump(Duration::from_millis(50)).unwrap();
    session.drain_events();

    session.disconnect();
    // Told at once, not on the next pump: with the connection gone `pump`
    // returns before it reaches the transfer, so a dialog waiting for
    // `TransferDone` would wait for ever.
    let saw_done = session
        .drain_events()
        .iter()
        .any(|ev| matches!(ev, Event::TransferDone(_)));
    assert!(saw_done, "the transfer was never told the line had gone");
    assert!(session.transfer().is_none());
}
