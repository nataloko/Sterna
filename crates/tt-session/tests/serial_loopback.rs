//! The same composition over a real wire.
//!
//! The memory-transport tests prove the session's logic; this proves the
//! `SerialConn`-as-`Transport` impl actually carries it, which is the join
//! nothing else touches. Needs two ports wired back-to-back and skips loudly
//! without them:
//!
//! ```sh
//! TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
//!   cargo test -p tt-session -- --test-threads=1
//! ```

use std::time::{Duration, Instant};

use tt_conn::serial::{SerialConn, SerialParams};
use tt_session::{Event, Session};
use tt_vt::{Config, Key};

fn rig() -> Option<(String, String)> {
    match (std::env::var("TT_SERIAL_A"), std::env::var("TT_SERIAL_B")) {
        (Ok(a), Ok(b)) => Some((a, b)),
        _ => {
            eprintln!("SKIP: set TT_SERIAL_A and TT_SERIAL_B to a back-to-back pair");
            None
        }
    }
}

fn params() -> SerialParams {
    SerialParams {
        baud: 115200,
        ..SerialParams::default()
    }
}

/// Pump until `f` is satisfied or the deadline passes, accumulating events.
fn pump_until(s: &mut Session, dur: Duration, f: impl Fn(&Session) -> bool) -> Vec<Event> {
    let mut events = Vec::new();
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        s.pump(Duration::from_millis(20)).expect("pump");
        events.extend(s.drain_events());
        if f(s) {
            break;
        }
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
fn a_session_over_a_real_serial_line_renders_and_answers() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");

    let mut s = Session::new(Config {
        cols: 40,
        rows: 6,
        ..Config::default()
    });
    s.connect(Box::new(near));
    far.clear(true, true).ok();

    // The far end writes a prompt with a cursor move in it, so this exercises
    // the parser and not just a memcpy.
    far.write(b"\x1b[2J\x1b[1;1Hlogin: ", Duration::from_secs(1))
        .unwrap();
    far.flush(Duration::from_millis(500)).ok();
    pump_until(&mut s, Duration::from_secs(2), |s| row(s, 0) == "login:");
    assert_eq!(row(&s, 0), "login:");

    // And a key goes back out over the same wire, encoded by the core.
    s.send_key(Key::Up).unwrap();
    let mut got = Vec::new();
    let mut evs = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && got.len() < 3 {
        far.read(&mut got, &mut evs).unwrap();
    }
    assert_eq!(got, b"\x1b[A");
}

#[test]
fn a_break_on_the_wire_surfaces_as_a_session_event() {
    // The end-to-end version of what `tt-conn`'s PARMRK decoding buys: a
    // break must reach the frontend as an event, not as a NUL in the grid.
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");

    let mut s = Session::new(Config {
        cols: 40,
        rows: 6,
        ..Config::default()
    });
    s.connect(Box::new(near));
    std::thread::sleep(Duration::from_millis(150));
    s.pump(Duration::from_millis(20)).ok();
    s.drain_events();

    far.write(b"X", Duration::from_secs(1)).unwrap();
    far.flush(Duration::from_millis(300)).ok();
    std::thread::sleep(Duration::from_millis(120));
    far.send_break(Duration::from_millis(250)).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    far.write(&[0x00, b'Y'], Duration::from_secs(1)).unwrap();
    far.flush(Duration::from_millis(300)).ok();

    let events = pump_until(&mut s, Duration::from_secs(2), |s| row(s, 0).contains('Y'));
    assert!(
        events.contains(&Event::Break),
        "expected a Break event, got {events:?}"
    );
    // The NUL between them is a control character the parser ignores, so the
    // two printable bytes end up adjacent.
    assert_eq!(row(&s, 0), "XY");
}

/// Wait on the session's own descriptor, the way the Qt shell does.
fn readable(s: &Session, timeout: Duration) -> bool {
    let Some(fd) = s.poll_fd() else { return false };
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let n = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as libc::c_int) };
    n > 0 && pfd.revents & libc::POLLIN != 0
}

#[test]
fn the_poll_descriptor_sleeps_on_a_quiet_line_and_wakes_on_bytes() {
    // The contract the shell's event loop is built on, and the reason there is
    // no timer in it. Returning a descriptor is not the claim — a descriptor
    // that is *always* readable would pass that and spin the UI at 100% for
    // the life of the session. The claim is that it stays quiet and then
    // wakes, and that a zero-budget pump is enough to collect what woke it.
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");

    let mut s = Session::new(Config {
        cols: 40,
        rows: 6,
        ..Config::default()
    });
    s.connect(Box::new(near));
    far.clear(true, true).ok();
    // Settle whatever the open left in flight before measuring silence.
    pump_until(&mut s, Duration::from_millis(200), |_| false);
    s.drain_events();

    assert!(
        !readable(&s, Duration::from_millis(250)),
        "a quiet line reported readable — the shell would spin"
    );

    far.write(b"hello", Duration::from_secs(1)).unwrap();
    far.flush(Duration::from_millis(500)).ok();
    assert!(
        readable(&s, Duration::from_secs(2)),
        "bytes on the wire did not wake the descriptor"
    );

    // A budget of zero is what the shell passes: one read, no blocking.
    let mut got = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && got < 5 {
        got += s.pump(Duration::ZERO).expect("pump");
        if got < 5 && !readable(&s, Duration::from_millis(200)) {
            break;
        }
    }
    assert_eq!(row(&s, 0), "hello");

    // And it goes quiet again once drained, rather than staying hot.
    assert!(
        !readable(&s, Duration::from_millis(250)),
        "still readable after the bytes were consumed"
    );
}

#[test]
fn the_poll_descriptor_is_none_with_nothing_connected() {
    let s = Session::new(Config::default());
    assert!(s.poll_fd().is_none());
}

#[test]
fn unplugging_the_far_end_ends_the_session() {
    // Closing the other handle is not an unplug, so this only checks the
    // shape a real disconnect takes: `tt-conn` maps it, and the session must
    // turn it into one event and detach. Hotplug proper still needs a human
    // and lives in serial-audit.
    let Some((a, _)) = rig() else { return };
    let near = SerialConn::open(&a, &params()).expect("open");
    let mut s = Session::new(Config::default());
    s.connect(Box::new(near));

    assert!(s.is_connected());
    s.disconnect();
    assert!(!s.is_connected());
    // Pumping a detached session is a no-op, not a panic.
    assert_eq!(s.pump(Duration::from_millis(10)).unwrap(), 0);
    assert!(s.drain_events().is_empty());
}
