//! What arms a serial auto-reopen, and what ends one.
//!
//! The *decisions* — the delays, the retry arithmetic, the back-off — are unit
//! tests inside `src/reopen.rs` against a synthetic clock. What is covered here
//! is the composition: which kinds of disconnect start a wait, which do not,
//! and every way one is called off. All of it runs against `MemoryTransport`,
//! so it needs no adapter and no cable.
//!
//! The one thing it cannot cover is the open itself. There is no headless stand
//! in for a serial port — a pty slave is a tty but has no modem lines, so
//! `SerialConn::open` fails on it — which is why the success path is in
//! `serial_loopback.rs`, behind the rig.

use std::time::Duration;

use tt_conn::serial::SerialParams;
use tt_conn::LinkKind;
use tt_session::{Event, MemoryHandle, MemoryTransport, Session};
use tt_vt::Config;

const PORT: &str = "/dev/serial/by-path/sterna-test-no-such-port";

fn serial_kind() -> LinkKind {
    LinkKind::Serial {
        baud: 115_200,
        seven_bit: false,
    }
}

/// A session holding a serial-shaped memory transport that knows how it would
/// be reopened.
fn connected_serial(path: &str) -> (Session, MemoryHandle) {
    let (transport, handle) = MemoryTransport::with_kind(serial_kind());
    let mut s = Session::new(Config::default());
    s.connect(Box::new(
        transport.reopening_as(path, SerialParams::default()),
    ));
    (s, handle)
}

/// Everything the session has queued, drained.
fn drain(s: &mut Session) -> Vec<Event> {
    s.drain_events()
}

fn pump(s: &mut Session) {
    let _ = s.pump(Duration::from_millis(5));
}

#[test]
fn a_serial_line_that_dropped_by_itself_starts_a_wait() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);

    assert!(!s.is_connected());
    assert!(s.is_reopening());
    assert_eq!(s.reopening_port(), Some(PORT));
    assert!(
        s.reopen_deadline().is_some(),
        "an armed wait has something to wait for"
    );

    // The order matters: a frontend reading the queue sees the line drop and
    // then sees what is being done about it.
    let events = drain(&mut s);
    let disconnected = events
        .iter()
        .position(|e| matches!(e, Event::Disconnected))
        .expect("the drop is reported");
    let reopening = events
        .iter()
        .position(|e| matches!(e, Event::Reopening(p) if p == PORT))
        .expect("and so is the wait");
    assert!(disconnected < reopening);
}

/// The same drop, found by the clock instead of by a read.
///
/// **This is the only way Windows finds one on a serial port.** An unplugged
/// COM adapter makes no descriptor readable and completes no pending wait, so
/// nothing calls `read` and nothing ever asks: the window went on saying
/// "connected" until somebody typed at it, and then reported `os error 22`
/// rather than reconnecting. `Session::tick` therefore has to reach the same
/// `line_went_away` the read and write paths do, arming and all.
#[test]
fn a_line_the_clock_finds_gone_starts_the_same_wait() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    // No pump: the point is that nothing read anything.
    assert!(s.tick().is_ok(), "a dropped line is not a tick failure");

    assert!(!s.is_connected());
    assert!(s.is_reopening());
    assert_eq!(s.reopening_port(), Some(PORT));

    let events = drain(&mut s);
    assert!(events.iter().any(|e| matches!(e, Event::Disconnected)));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Reopening(p) if p == PORT)));

    // ...and it happens once. A second tick has no transport to ask.
    assert!(s.tick().is_ok());
    assert!(!drain(&mut s)
        .iter()
        .any(|e| matches!(e, Event::Disconnected)));
}

/// The setting ships on, so this is the off case rather than the on one.
#[test]
fn the_setting_off_means_no_wait() {
    let (mut s, h) = connected_serial(PORT);
    assert!(s.set_setting("serial.auto_reconnect", "off"));
    h.with(|st| st.disconnected = true);
    pump(&mut s);

    assert!(!s.is_reopening());
    assert_eq!(s.reopen_deadline(), None);
    assert!(!drain(&mut s)
        .iter()
        .any(|e| matches!(e, Event::Reopening(_))));
}

/// Deviation 15's seam: `asked` is the difference, and a disconnect somebody
/// asked for is not something to undo.
#[test]
fn asking_to_disconnect_starts_no_wait() {
    let (mut s, _h) = connected_serial(PORT);
    s.disconnect();
    assert!(!s.is_connected());
    assert!(!s.is_reopening());
}

/// ...and it is also how a wait already running is called off, which is the
/// only thing `disconnect` does with nothing connected.
#[test]
fn disconnecting_calls_off_a_wait() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);
    assert!(s.is_reopening());

    s.disconnect();
    assert!(!s.is_reopening());
    assert_eq!(s.reopening_port(), None);
}

#[test]
fn connecting_somewhere_else_calls_off_a_wait() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);
    assert!(s.is_reopening());

    let (transport, _h2) = MemoryTransport::new();
    s.connect(Box::new(transport));
    assert!(
        !s.is_reopening(),
        "this session is about something else now"
    );
    assert!(s.is_connected());
}

/// Turning the switch off stops a wait already running, rather than only
/// stopping the next one.
#[test]
fn turning_the_setting_off_stops_a_wait_in_progress() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);
    assert!(s.is_reopening());

    assert!(s.set_setting("serial.auto_reconnect", "off"));
    assert!(!s.is_reopening());

    // And turning it back on does not resurrect it: the port that went away is
    // forgotten, which is the honest answer — nothing was watching it.
    assert!(s.set_setting("serial.auto_reconnect", "on"));
    assert!(!s.is_reopening());
}

/// Changing the numbers while a wait runs is not a reason to stop it.
#[test]
fn changing_the_timings_leaves_a_wait_alone() {
    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);
    assert!(s.is_reopening());

    assert!(s.set_setting("serial.auto_reconnect_retry_interval", "50"));
    assert!(s.set_setting("serial.auto_reconnect_retries", "9"));
    assert!(s.is_reopening());
    assert_eq!(s.reopening_port(), Some(PORT));
}

/// A link with no serial port under it has nothing to reopen, whatever the
/// setting says.
#[test]
fn a_network_line_that_dropped_starts_no_wait() {
    let (transport, h) = MemoryTransport::new();
    let mut s = Session::new(Config::default());
    s.connect(Box::new(transport));
    h.with(|st| st.disconnected = true);
    pump(&mut s);

    assert!(!s.is_connected());
    assert!(!s.is_reopening());
}

/// Servicing a machine that is not armed, or one whose deadline has not come,
/// must cost nothing and say nothing — a frontend's timer is allowed to be
/// early.
#[test]
fn servicing_early_or_unarmed_does_nothing() {
    let mut s = Session::new(Config::default());
    s.service_reopen();
    assert!(drain(&mut s).is_empty());

    let (mut s, h) = connected_serial(PORT);
    h.with(|st| st.disconnected = true);
    pump(&mut s);
    let _ = drain(&mut s);

    // The node named above does not exist, so this can only stay in the wait.
    for _ in 0..5 {
        s.service_reopen();
    }
    assert!(
        s.is_reopening(),
        "an absent node is waited for, not given up on"
    );
    assert!(
        drain(&mut s).is_empty(),
        "and it says nothing while it waits"
    );
}

/// The reopen record is the *port's* parameters, so a session whose speed was
/// changed after it opened comes back at the speed it was using.
#[test]
fn the_wait_remembers_the_speed_the_port_was_using() {
    let (transport, h) = MemoryTransport::with_kind(serial_kind());
    let mut s = Session::new(Config::default());
    s.connect(Box::new(transport.reopening_as(
        PORT,
        SerialParams {
            baud: 921_600,
            ..SerialParams::default()
        },
    )));
    h.with(|st| st.disconnected = true);
    pump(&mut s);

    assert!(s.is_reopening());
    // The settings file still says the shipped speed; the wait does not.
    assert_ne!(s.settings().serial_baud, 921_600);
}
