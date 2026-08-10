//! OSC 52 across the terminal/session boundary. The core authorises and
//! decodes; this test plays the frontend which owns the system clipboard.

use std::time::Duration;

use tt_config::ClipboardRemoteAccess;
use tt_session::{Event, MemoryTransport, Session, Settings};
use tt_vt::ClipboardRequest;

fn connected(settings: Settings) -> (Session, tt_session::MemoryHandle) {
    let mut session = Session::from_settings(settings);
    let (transport, peer) = MemoryTransport::new();
    session.connect(Box::new(transport));
    (session, peer)
}

#[test]
fn an_allowed_read_and_write_cross_the_frontend_boundary() {
    let settings = Settings {
        clipboard_remote_access: ClipboardRemoteAccess::ReadWrite,
        ..Settings::default()
    };
    let (mut session, peer) = connected(settings);
    peer.feed(b"\x1b]52;c;?\x07\x1b]52;p;aGk=\x1b\\");
    session.pump(Duration::ZERO).unwrap();

    let events = session.drain_events();
    assert!(events.contains(&Event::Clipboard(ClipboardRequest::Read {
        selection: "c".into(),
        notify: true,
    })));
    assert!(events.contains(&Event::Clipboard(ClipboardRequest::Write {
        text: "hi".into(),
        notify: true,
    })));

    // What the Qt frontend does for the read event: fetch the clipboard and
    // hand the text back. The response is flushed immediately, because the
    // host which asked is normally blocked waiting for it.
    assert!(session.clipboard_reply("c", "hé").unwrap());
    assert_eq!(peer.outbound(), b"\x1b]52;c;aMOp\x1b\\");
}

#[test]
fn denied_access_is_visible_only_when_notification_is_on() {
    let (mut session, peer) = connected(Settings::default());
    peer.feed(b"\x1b]52;c;?\x07\x1b]52;c;eA==\x07");
    session.pump(Duration::ZERO).unwrap();
    let events = session.drain_events();
    assert!(events.contains(&Event::Clipboard(ClipboardRequest::ReadRejected)));
    assert!(events.contains(&Event::Clipboard(ClipboardRequest::WriteRejected)));

    let settings = Settings {
        clipboard_remote_notify: false,
        ..Settings::default()
    };
    let (mut session, peer) = connected(settings);
    peer.feed(b"\x1b]52;c;?\x07\x1b]52;c;eA==\x07");
    session.pump(Duration::ZERO).unwrap();
    assert!(!session
        .drain_events()
        .iter()
        .any(|event| matches!(event, Event::Clipboard(_))));
}
