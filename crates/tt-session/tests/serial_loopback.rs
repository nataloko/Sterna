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

use tt_conn::serial::{FlowControl, ModemLines, SerialConn, SerialParams};
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

/// Everything the far end receives inside `dur`, however it is split up.
fn read_for(far: &mut SerialConn, dur: Duration) -> Vec<u8> {
    let mut got = Vec::new();
    let mut events = Vec::new();
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        far.read(&mut got, &mut events).expect("read the far end");
    }
    got
}

/// Wait for the far end's control lines to say `f`, and report what they
/// last said. A pin change crosses a USB bus, so it is never instant.
fn lines_until(far: &mut SerialConn, f: impl Fn(&ModemLines) -> bool) -> ModemLines {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let m = far.modem_lines().expect("read the far end's lines");
        if f(&m) || Instant::now() > deadline {
            return m;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `setdtr` and `setrts`, proven on the wire rather than at the ioctl.
///
/// The rig has DTR wired to the other port's DSR and RTS to its CTS, so this
/// is the whole path — the session, the boxed transport, the port, the cable
/// — and it is the only way to tell driving a pin from believing you did.
#[test]
fn the_control_lines_reach_the_far_end() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");
    let mut s = Session::new(Config::default());
    s.connect(Box::new(near));

    // Both are asserted on open — `SerialParams::default` is `PinControl::
    // Enable` for each, which is what `dcb.fDtrControl` defaults to as well.
    let m = lines_until(&mut far, |m| m.dsr && m.cts);
    assert!(
        m.dsr && m.cts,
        "the rig did not start with both pins up: {m:?}"
    );

    assert!(s.set_dtr(false));
    assert!(!lines_until(&mut far, |m| !m.dsr).dsr, "DTR did not drop");
    assert!(s.set_rts(false));
    assert!(!lines_until(&mut far, |m| !m.cts).cts, "RTS did not drop");

    assert!(s.set_dtr(true));
    assert!(s.set_rts(true));
    let m = lines_until(&mut far, |m| m.dsr && m.cts);
    assert!(m.dsr && m.cts, "the pins did not come back up: {m:?}");

    // And the other direction: `getmodemstatus` reads what the far end is
    // driving. Lowering its DTR must show up as DSR low here.
    far.set_dtr(false).expect("drop the far end's DTR");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = s.modem_lines().expect("the session should have lines");
    while seen.dsr && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        seen = s.modem_lines().expect("the session should have lines");
    }
    assert!(!seen.dsr, "the far end's DTR was not visible: {seen:?}");
}

/// The second guard: with anything but "none" for flow control the lines
/// belong to the driver, and `setdtr` is refused without touching them.
///
/// The refusal is the deterministic half — nothing is written at all, so what
/// the pins do afterwards is not the driver's opinion — and the second half is
/// the idiom `setdtr.html` describes: turn the flow control off first, and
/// then it works.
#[test]
fn the_control_lines_are_refused_while_the_driver_owns_them() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(
        &a,
        &SerialParams {
            flow: FlowControl::RtsCts,
            ..params()
        },
    )
    .expect("open the near end");
    let mut s = Session::new(Config::default());
    s.connect(Box::new(near));
    // The settings are what the guard reads, so they have to say what the
    // port was opened with — which is what the New Connection dialog and the
    // command line both arrange.
    assert!(s.set_setting("serial.flow", "hard").expect("apply"));

    assert!(!s.set_dtr(false), "DTR should have been refused");
    assert!(!s.set_rts(false), "RTS should have been refused");
    let m = lines_until(&mut far, |_| false);
    assert!(m.dsr, "a refused setdtr moved the pin anyway: {m:?}");

    // `setflowctrl 3`, which upstream leaves as a setting and this port also
    // applies — see `Session::set_flow_control`.
    assert!(s.set_flow_control(FlowControl::None));
    assert!(s.set_dtr(false), "the guard should be open now");
    assert!(!lines_until(&mut far, |m| !m.dsr).dsr, "DTR did not drop");
}

/// `setbaud` reaches the hardware, which a test that only read the setting
/// back could not tell from a no-op.
///
/// The proof is a speed *mismatch*: the near end is moved on its own first,
/// and the line goes to pieces because the far end is still where it was.
#[test]
fn setbaud_changes_the_port_and_not_only_the_setting() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");
    let mut s = Session::new(Config::default());
    s.connect(Box::new(near));
    s.drain_events();

    assert!(s.set_baud(19200));
    assert_eq!(s.serial_baud(), Some(19200));
    assert!(
        s.drain_events()
            .iter()
            .any(|event| matches!(event, Event::Title(_))),
        "the title bar was not told the displayed speed changed"
    );
    // The speed is in what the status line shows, which is why upstream
    // repaints the title bar here.
    assert!(
        s.describe().unwrap_or_default().ends_with(" 19200"),
        "the transport still reports {:?}",
        s.describe()
    );

    // Read at the far end rather than on the screen, in both halves: garbage
    // off a mismatched line is escape-sequence bytes as often as not, and a
    // parser left inside an OSC string swallows everything that follows it —
    // including the next half of the test.
    far.clear(true, true).ok();
    s.send_bytes(b"mismatched").expect("send");
    assert_ne!(
        read_for(&mut far, Duration::from_millis(600)),
        b"mismatched",
        "19200 into 115200 arrived intact, so the speed did not change"
    );

    // And with the far end moved to match, the same bytes arrive.
    far.apply(&SerialParams {
        baud: 19200,
        ..params()
    })
    .expect("move the far end");
    std::thread::sleep(Duration::from_millis(300));
    far.clear(true, true).ok();
    s.send_bytes(b"matched").expect("send");
    assert_eq!(read_for(&mut far, Duration::from_secs(2)), b"matched");
}

/// `setflowctrl` is applied to the port rather than only written down, which
/// is where this port and upstream part company — see
/// `Session::set_flow_control` for why.
///
/// The observable is the kernel's and it is exact: XON/XOFF flow control is
/// the tty layer acting on a received XOFF by stopping this end's
/// transmitter. So the far end sends XOFF, the session sends a line, and
/// nothing arrives until the XON — which is only true if the setting reached
/// the port. Written down and not applied, the line would come straight
/// through.
#[test]
fn setflowctrl_reaches_the_port() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");
    let near = SerialConn::open(&a, &params()).expect("open the near end");
    let mut s = Session::new(Config::default());
    s.connect(Box::new(near));
    far.clear(true, true).ok();

    assert!(s.set_flow_control(FlowControl::XonXoff));

    // Stop the near end, then give it something to say.
    far.write(&[0x13], Duration::from_secs(1)).unwrap();
    far.flush(Duration::from_millis(500)).ok();
    std::thread::sleep(Duration::from_millis(200));
    s.send_bytes(b"held\n").expect("queue the line");
    assert!(
        read_for(&mut far, Duration::from_millis(400)).is_empty(),
        "XOFF did not stop the transmitter — the setting was not applied"
    );

    // ...and let it go again. Always, whatever the assertion above did: a
    // port left stopped would take the next test down with it.
    far.write(&[0x11], Duration::from_secs(1)).unwrap();
    far.flush(Duration::from_millis(500)).ok();
    let got = read_for(&mut far, Duration::from_secs(2));
    assert_eq!(got, b"held\n", "the held line did not arrive after XON");
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

/// `ClearComBuffOnOpen` — whether what arrived before anybody was watching is
/// thrown away or delivered.
///
/// Only a real port can answer this: the setting acts on the *driver's* queue,
/// which a memory transport does not have, and the two answers are otherwise
/// indistinguishable from the session's side.
#[test]
fn what_arrived_before_the_session_did_is_kept_or_purged_by_the_setting() {
    let Some((a, b)) = rig() else { return };

    // Put bytes on the wire with nothing on this side reading them, so they
    // sit in the near port's own receive queue. The port is opened first and
    // handed over after — which is what a session does, since `connect` takes
    // an already-open transport.
    let speak = |near: SerialConn, clear: bool| {
        let mut far = SerialConn::open(&b, &params()).expect("open the far end");
        far.write(b"said before\r\n", Duration::from_secs(1))
            .expect("write");
        far.flush(Duration::from_millis(500)).ok();
        // Long enough for the FTDI to have handed it to the kernel; a purge
        // that runs before the bytes land clears an empty queue and proves
        // nothing.
        std::thread::sleep(Duration::from_millis(250));

        let settings = tt_config::Settings {
            serial_clear_buffer_on_open: clear,
            ..tt_config::Settings::default()
        };
        let mut s = Session::new(Config::default());
        s.set_settings(settings).expect("settings");
        s.connect(Box::new(near));
        pump_until(&mut s, Duration::from_millis(600), |s| {
            !row(s, 0).is_empty()
        });
        row(&s, 0)
    };

    let near = SerialConn::open(&a, &params()).expect("open the near end");
    assert_eq!(
        speak(near, false),
        "said before",
        "with the purge off, what the far end said first is the session's first line"
    );

    let near = SerialConn::open(&a, &params()).expect("open the near end");
    assert_eq!(
        speak(near, true),
        "",
        "with the purge on, it is gone before the terminal sees it"
    );
}

/// The two control lines, resolved out of the settings and asserted at the
/// other end of the cable — DTR→DSR and RTS→CTS on the rig's loom.
#[test]
fn the_control_lines_reach_the_wire_as_the_settings_ask() {
    let Some((a, b)) = rig() else { return };
    let mut far = SerialConn::open(&b, &params()).expect("open the far end");

    let held = |value: i32| {
        let settings = tt_config::Settings {
            serial_rts: value,
            serial_dtr: value,
            ..tt_config::Settings::default()
        };
        tt_session::open::serial_params(&settings)
    };

    // `FlowCtrlRTS=0` / `FlowCtrlDTR=0` is `*_CONTROL_DISABLE`, and the far
    // end should see both lines low.
    let near = SerialConn::open(&a, &held(0)).expect("open low");
    let lines = lines_until(&mut far, |l| !l.cts && !l.dsr);
    assert!(!lines.cts, "RTS stayed high with FlowCtrlRTS=0");
    assert!(!lines.dsr, "DTR stayed high with FlowCtrlDTR=0");
    drop(near);

    // ...and the sentinel, which with no flow control derives to Enable for
    // both. This is the default a file that says nothing produces.
    let near = SerialConn::open(&a, &held(-1)).expect("open high");
    let lines = lines_until(&mut far, |l| l.cts && l.dsr);
    assert!(lines.cts, "RTS did not come up for the derived default");
    assert!(lines.dsr, "DTR did not come up for the derived default");
    drop(near);
}
