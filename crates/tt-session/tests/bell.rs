//! The bell, and the other control that answers back.
//!
//! `bell.rs`'s own tests are about the governor as an algorithm. These are
//! about the two things only a session can show: that a burst arriving in one
//! read is one event and still costs the terminal its allowance, and that ENQ
//! puts the answerback on the wire.

use std::time::Duration;
use tt_config::Settings;
use tt_conn::{LinkKind, Result, Transport, TransportEvent};
use tt_session::{Event, MemoryHandle, MemoryTransport, Session};

fn session_with(keys: &[(&str, &str)]) -> (Session, MemoryHandle) {
    let mut settings = Settings::default();
    for (name, value) in keys {
        assert!(settings.set_str(name, value), "no such setting: {name}");
    }
    let mut s = Session::from_settings(settings);
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    s.drain_events();
    (s, handle)
}

fn bells(s: &mut Session) -> Vec<bool> {
    s.drain_events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Bell { visual } => Some(visual),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bel_rings_the_bell() {
    let (mut s, _h) = session_with(&[]);
    s.feed(b"a\x07b");
    assert_eq!(bells(&mut s), [false]);
}

#[test]
fn the_bell_can_be_switched_off_entirely() {
    let (mut s, _h) = session_with(&[("bell.mode", "off")]);
    s.feed(b"\x07\x07\x07");
    assert_eq!(bells(&mut s), Vec::<bool>::new());
}

#[test]
fn the_visual_bell_says_which_it_is() {
    let (mut s, _h) = session_with(&[("bell.mode", "visual")]);
    s.feed(b"\x07");
    assert_eq!(bells(&mut s), [true]);
}

/// Two beeps in the same read are one beep — but the governor is stepped once
/// per BEL even so, which is the whole reason the count crosses the boundary
/// instead of a flag.
#[test]
fn a_burst_is_one_event_and_still_spends_the_allowance() {
    let (mut s, _h) = session_with(&[]);
    s.feed(&[0x07; 6]);
    assert_eq!(bells(&mut s), [false], "six BELs, one noise");

    // The default allows six inside two seconds and this is the seventh, so
    // the terminal is now quiet. A frontend that had thinned the burst itself
    // would still be beeping here.
    s.feed(b"\x07");
    assert_eq!(bells(&mut s), Vec::<bool>::new());
}

/// The settings are read at the bell rather than held, so raising the limit
/// applies to the next BEL — which is where upstream reads `ts` too.
#[test]
fn a_bigger_allowance_applies_at_once() {
    let (mut s, _h) = session_with(&[]);
    s.feed(&[0x07; 7]);
    assert_eq!(bells(&mut s), [false]);
    s.feed(b"\x07");
    assert_eq!(bells(&mut s), Vec::<bool>::new(), "suppressed");

    s.set_setting("bell.suppress_time", "0");
    s.set_setting("bell.over_used_time", "0");
    s.feed(b"\x07");
    assert_eq!(bells(&mut s), [false], "a governor with no windows");
}

/// `ESC g` is heard as whatever the setting says, because `RingBell` ignores
/// the kind it was asked for — see `Vt::esc_dispatch`.
#[test]
fn esc_g_is_an_ordinary_beep() {
    let (mut s, _h) = session_with(&[]);
    s.feed(b"\x1bg");
    assert_eq!(bells(&mut s), [false]);
}

#[test]
fn enq_answers_with_nothing_until_a_file_says_otherwise() {
    let (mut s, h) = session_with(&[]);
    h.feed(b"\x05");
    s.pump(Duration::from_millis(10)).unwrap();
    assert!(h.outbound().is_empty());
}

/// The value is hex, so the CR at the end of a realistic answerback is `$0D`
/// and the bytes on the wire are exactly what it decodes to.
#[test]
fn enq_puts_the_answerback_on_the_wire() {
    let (mut s, h) = session_with(&[("terminal.answerback", "sterna$0D")]);
    h.feed(b"\x05");
    s.pump(Duration::from_millis(10)).unwrap();
    assert_eq!(h.outbound(), b"sterna\r");
    // And the file keeps the spelling the user wrote, not the bytes.
    assert_eq!(
        s.setting("terminal.answerback").as_deref(),
        Some("sterna$0D")
    );
}

/// A serial port that is otherwise a `MemoryTransport`, for the one setting
/// that asks what kind of link this is.
struct Serialish(MemoryTransport);

impl Transport for Serialish {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        self.0.read(data, events)
    }
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        self.0.write(data, timeout)
    }
    fn describe(&self) -> String {
        "/dev/ttyUSB0".into()
    }
    fn link_kind(&self) -> LinkKind {
        LinkKind::Serial {
            baud: 9600,
            seven_bit: false,
        }
    }
}

/// `BeepOnConnect` is named after connecting and tests the port type first, so
/// the console this project exists for is the one link it never fires on.
#[test]
fn beep_on_connect_skips_a_serial_port() {
    let mut settings = Settings::default();
    assert!(settings.set_str("bell.on_connect", "on"));
    let mut s = Session::from_settings(settings);

    let (transport, _h) = MemoryTransport::new();
    s.connect(Box::new(Serialish(transport)));
    assert_eq!(bells(&mut s), Vec::<bool>::new());
    s.disconnect();
    assert_eq!(bells(&mut s), Vec::<bool>::new());

    let (transport, _h) = MemoryTransport::new();
    s.connect(Box::new(transport));
    assert_eq!(bells(&mut s), [false], "a network link beeps");
    s.disconnect();
    assert_eq!(bells(&mut s), [false], "and beeps on the way out too");
}

/// It bypasses `RingBell`, so it is audible even when the bell is visual and
/// it does not spend the terminal's allowance.
#[test]
fn beep_on_connect_is_not_the_terminals_bell() {
    let (mut s, _h) = session_with(&[("bell.on_connect", "on"), ("bell.mode", "visual")]);
    s.disconnect();
    assert_eq!(bells(&mut s), [false], "audible, not a flash");
}
