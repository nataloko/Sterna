//! What a linked macro reads out of a running session.
//!
//! The unit tests in `tt-vt` fix what the tap emits and the ones in
//! `tt_session::macros` fix what the ring does with it. This is the join: a
//! real transport, a real pump, and the bytes a `wait` would be matching
//! against at the other end.

use std::time::Duration;

use tt_session::{MemoryTransport, Session, MACRO_BUF_SIZE};
use tt_vt::Config;

fn session() -> (Session, tt_session::MemoryHandle) {
    let mut s = Session::new(Config {
        cols: 20,
        rows: 5,
        ..Config::default()
    });
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    (s, handle)
}

fn drain(link: &tt_session::MacroLink) -> String {
    let mut out = Vec::new();
    while let Some(b) = link.pop() {
        out.push(b);
    }
    String::from_utf8(out).unwrap()
}

/// The whole path, and the claim the macro language rests on: a script sees
/// the text, never the escape sequences that produced it.
#[test]
fn a_macro_reads_the_session_as_text() {
    let (mut s, far) = session();
    let link = s.link_macro();
    far.feed(b"\x1b[2J\x1b[HUsername: ");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(drain(&link), "Username: ");

    far.feed(b"root\r\nPassword: ");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(drain(&link), "root\r\nPassword: ");
}

/// Nothing is collected before a macro links, and unlinking stops it — so a
/// session with no script running pays for none of this.
#[test]
fn the_ring_fills_only_while_a_macro_is_linked() {
    let (mut s, far) = session();
    assert!(!s.macro_linked());

    far.feed(b"before ");
    s.pump(Duration::from_millis(50)).unwrap();

    let link = s.link_macro();
    assert!(s.macro_linked());
    far.feed(b"during ");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(drain(&link), "during ");

    s.unlink_macro();
    assert!(!s.macro_linked());
    far.feed(b"after");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(drain(&link), "");
}

/// Linking twice is what `connect` from a second macro does, and it must not
/// hand the new one the old one's backlog.
#[test]
fn a_second_link_starts_empty() {
    let (mut s, far) = session();
    let first = s.link_macro();
    far.feed(b"old");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(first.len(), 3);

    let second = s.link_macro();
    assert!(second.is_empty());
    far.feed(b"new");
    s.pump(Duration::from_millis(50)).unwrap();
    assert_eq!(drain(&second), "new");
}

/// A script that stops reading must not stop the terminal. Sixty-five
/// kilobytes in and the oldest go, which is what keeps the parser running at
/// the speed of the line rather than at the speed of the macro.
#[test]
fn a_macro_that_stops_reading_loses_the_oldest_bytes() {
    let (mut s, far) = session();
    let link = s.link_macro();
    // Eighteen bytes a line — sixteen digits and a CRLF, which fits the
    // twenty-column screen, because a line that wrapped would put a second
    // CRLF in the stream and this test is about the ring rather than the tap.
    for i in 0..5000u32 {
        far.feed(format!("{i:0>16}\r\n").as_bytes());
    }
    s.pump(Duration::from_millis(500)).unwrap();
    assert_eq!(link.len(), MACRO_BUF_SIZE);
    assert!(link.dropped() > 0, "nothing was dropped");

    // What survives is the *end* of the stream: the last line is intact.
    let text = drain(&link);
    assert!(
        text.ends_with("0000000000004999\r\n"),
        "{:?}",
        &text[text.len() - 40..]
    );
}

/// Local echo goes through `feed` rather than through the transport, and a
/// macro has to see it — upstream taps the same function for both.
#[test]
fn bytes_fed_locally_reach_the_macro_too() {
    let (mut s, _far) = session();
    let link = s.link_macro();
    s.feed(b"echoed");
    assert_eq!(drain(&link), "echoed");
}

/// The two taps run at once without robbing each other.
#[test]
fn a_session_log_and_a_macro_can_both_be_open() {
    let dir = std::env::temp_dir().join(format!("tt-macro-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.log");

    let (mut s, far) = session();
    let link = s.link_macro();
    s.start_log(&path, tt_session::LogOptions::default())
        .unwrap();
    far.feed(b"line one\r\n");
    s.pump(Duration::from_millis(50)).unwrap();
    s.stop_log();

    assert_eq!(drain(&link), "line one\r\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "line one\n");
    let _ = std::fs::remove_dir_all(&dir);
}
