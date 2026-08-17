//! The paced send queue against a whole session — upstream's `SendMem`.
//!
//! `send.rs`'s own tests are about the algorithm and hand it a fake clock.
//! These are about the *composition*: that a file reaches the wire through the
//! terminal's own encoding, that a pace really costs wall-clock time, that a
//! transport which will not take a line does not lose it, and that everything
//! which can end a send ends it exactly once.

use std::time::{Duration, Instant};

use tt_session::send::{Body, FileSend, Job, Pace, SendEnd, SendError};
use tt_session::{Event, MemoryHandle, MemoryTransport, Session};
use tt_vt::{Config, CrSend};

fn connected() -> (Session, MemoryHandle) {
    session_with(Config::default())
}

fn session_with(config: Config) -> (Session, MemoryHandle) {
    let mut s = Session::new(config);
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    (s, handle)
}

/// Drive the queue the way the frontend's single-shot timer does: read the
/// deadline, sleep exactly that long, service, read it again.
///
/// Bounded rather than `while`, so a sender that stops making progress fails
/// an assertion below instead of hanging the suite.
fn run(s: &mut Session) {
    for _ in 0..4000 {
        let Some(d) = s.send_deadline() else { return };
        if !d.is_zero() {
            std::thread::sleep(d);
        }
        s.service_send().expect("service");
    }
    panic!("the send never finished");
}

fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write");
    path
}

fn text_file(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    write(dir, "config.txt", body.as_bytes())
}

#[test]
fn a_file_reaches_the_wire_through_the_terminals_own_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "show version\nshow clock\n");
    let (mut s, h) = connected();
    s.send_file(&path, &FileSend::default()).expect("send");
    run(&mut s);
    // `CRSend` ships as a bare CR, so the file's LFs are normalised to CR and
    // then encoded to CR. Nothing invents a line feed.
    assert_eq!(h.outbound(), b"show version\rshow clock\r");
}

/// The same file, on a terminal whose `CRSend` is CRLF. The normalisation and
/// the encoding are two separate steps and only the second reads the setting.
#[test]
fn the_line_ending_on_the_wire_is_the_terminals_and_not_the_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "one\r\ntwo\r\n");
    let (mut s, h) = session_with(Config {
        cr_send: CrSend::CrLf,
        ..Config::default()
    });
    s.send_file(&path, &FileSend::default()).expect("send");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\r\ntwo\r\n");
}

#[test]
fn a_byte_order_mark_is_not_sent() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "bom.txt", "\u{feff}hello".as_bytes());
    let (mut s, h) = connected();
    s.send_file(&path, &FileSend::default()).expect("send");
    run(&mut s);
    assert_eq!(h.outbound(), b"hello");
}

/// A binary send is the other path entirely: no normalisation, no encoding, no
/// BOM removal. What is on disk is what goes out.
#[test]
fn a_binary_file_goes_out_exactly_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let raw = [0xffu8, 0x00, b'\r', b'\n', 0xfe];
    let path = write(dir.path(), "blob.bin", &raw);
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            binary: true,
            ..FileSend::default()
        },
    )
    .expect("send");
    run(&mut s);
    assert_eq!(h.outbound(), raw);
}

#[test]
fn a_paced_send_really_waits() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "a\nb\nc\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(30)),
            ..FileSend::default()
        },
    )
    .expect("send");
    let started = Instant::now();
    run(&mut s);
    // Three lines, and the wait comes *after* a line and only when something
    // is left — so two intervals, not three.
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(60), "took {took:?}");
    assert_eq!(h.outbound(), b"a\rb\rc\r");
}

/// Nothing is lost and nothing is reordered when the far end takes the bytes a
/// few at a time — which is what a line held by flow control looks like.
#[test]
fn a_line_the_transport_will_not_take_at_once_still_arrives_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "the first line\nthe second line\n");
    let (mut s, h) = connected();
    h.with(|st| st.write_chunk = 3);
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(5)),
            ..FileSend::default()
        },
    )
    .expect("send");
    run(&mut s);
    assert_eq!(h.outbound(), b"the first line\rthe second line\r");
}

/// Upstream sets `TalkStatus = IdTalkSendMem` for the duration and
/// `keyboard.c:1480` tests it before every key: a line typed into the middle of
/// a configuration is a line the far end runs in the wrong place.
#[test]
fn typing_is_dropped_while_a_send_owns_the_wire() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "one\ntwo\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(20)),
            ..FileSend::default()
        },
    )
    .expect("send");
    assert!(s.sending());
    s.service_send().expect("service");
    s.send_text("typed").expect("typed");
    s.paste("pasted", true).expect("pasted");
    s.send_bytes(b"raw").expect("raw");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\r");
    // ...and the keyboard comes back the moment it is over.
    assert!(!s.sending());
    s.send_text("typed").expect("typed");
    assert_eq!(h.outbound(), b"one\rtwo\rtyped");
}

/// A second job queues behind the first rather than replacing it, which is what
/// upstream's FIFO of them is for (`smptrPush`, `sendmem.cpp:107`).
#[test]
fn a_second_job_follows_the_first_rather_than_interleaving_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "first\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(5)),
            ..FileSend::default()
        },
    )
    .expect("send");
    s.queue_send(Job::new(Body::Text("second\r".into())))
        .expect("queue");
    assert_eq!(s.send_progress().unwrap().queued, 1);
    run(&mut s);
    assert_eq!(h.outbound(), b"first\rsecond\r");
}

#[test]
fn a_finished_send_says_so_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "hello\n");
    let (mut s, _h) = connected();
    s.send_file(&path, &FileSend::default()).expect("send");
    run(&mut s);
    let done: Vec<_> = s
        .drain_events()
        .into_iter()
        .filter_map(|e| match e {
            Event::SendDone(o) => Some(*o),
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].end, SendEnd::Finished);
    assert_eq!(done[0].sent, done[0].total);
    assert_eq!(done[0].name.as_deref(), Some(path.to_str().unwrap()));
    assert_eq!(s.send_progress(), None);
}

#[test]
fn cancelling_stops_it_where_it_stands() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "one\ntwo\nthree\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(20)),
            ..FileSend::default()
        },
    )
    .expect("send");
    s.service_send().expect("service");
    s.cancel_send();
    assert!(!s.sending());
    assert_eq!(s.send_deadline(), None);
    let done = s
        .drain_events()
        .into_iter()
        .find_map(|e| match e {
            Event::SendDone(o) => Some(*o),
            _ => None,
        })
        .expect("a done event");
    assert_eq!(done.end, SendEnd::Cancelled);
    assert_eq!(done.sent, 4);
    assert_eq!(h.outbound(), b"one\r");
    // Cancelling twice is not two events.
    s.cancel_send();
    assert!(!s
        .drain_events()
        .iter()
        .any(|e| matches!(e, Event::SendDone(_))));
}

#[test]
fn a_pause_stops_the_clock_and_arms_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "one\ntwo\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(5)),
            ..FileSend::default()
        },
    )
    .expect("send");
    s.service_send().expect("service");
    s.pause_send(true);
    assert_eq!(s.send_deadline(), None);
    assert!(s.send_progress().unwrap().paused);
    // Servicing a paused send does nothing at all, however often it is asked.
    for _ in 0..5 {
        s.service_send().expect("service");
    }
    assert_eq!(h.outbound(), b"one\r");
    s.pause_send(false);
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\r");
}

#[test]
fn the_link_going_away_ends_the_send() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "one\ntwo\nthree\n");
    let (mut s, h) = connected();
    s.send_file(
        &path,
        &FileSend {
            pace: Pace::PerLine(Duration::from_millis(20)),
            ..FileSend::default()
        },
    )
    .expect("send");
    s.service_send().expect("service");
    h.with(|st| st.disconnected = true);
    s.pump(Duration::from_millis(20)).expect("pump");
    assert!(!s.sending());
    assert_eq!(s.send_deadline(), None);
    let events = s.drain_events();
    let done = events
        .iter()
        .find_map(|e| match e {
            Event::SendDone(o) => Some(o.as_ref()),
            _ => None,
        })
        .expect("a done event");
    assert_eq!(done.end, SendEnd::LinkLost);
    // ...and it is reported before the disconnection that caused it, so a
    // progress panel closes before the window says the line dropped.
    let done_at = events
        .iter()
        .position(|e| matches!(e, Event::SendDone(_)))
        .unwrap();
    let gone_at = events
        .iter()
        .position(|e| matches!(e, Event::Disconnected))
        .unwrap();
    assert!(done_at < gone_at);
}

#[test]
fn a_session_with_nothing_connected_refuses_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "hello\n");
    let mut s = Session::new(Config::default());
    assert_eq!(
        s.send_file(&path, &FileSend::default()),
        Err(SendError::NotConnected)
    );
    assert!(!s.sending());
}

#[test]
fn a_file_that_cannot_be_read_says_what_the_system_said() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _h) = connected();
    let err = s
        .send_file(&dir.path().join("nothing-here"), &FileSend::default())
        .expect_err("no file");
    assert!(matches!(err, SendError::Unreadable(_)));
    assert!(!s.sending());
}

/// The other half of `wire_is_busy`: a transfer owns the stream, so nothing
/// queues behind it — a stray byte in the middle of a packet is a corrupt file.
#[test]
fn a_send_is_refused_while_a_transfer_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "hello\n");
    let (mut s, _h) = connected();
    let opts = s.transfer_options();
    s.receive_files(
        tt_xfer::Job::Raw {
            autostop: Duration::from_secs(0),
        },
        dir.path(),
        Some("received.bin"),
        &opts,
    )
    .expect("start a receive");
    assert_eq!(
        s.send_file(&path, &FileSend::default()),
        Err(SendError::TransferRunning)
    );
    s.cancel_transfer();
}

/// Local echo is captured when the job is queued, so it reaches the screen the
/// same way a typed line would — `SendMemInitEcho`.
#[test]
fn an_echoed_send_reaches_the_screen() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "visible\n");
    let (mut s, _h) = connected();
    s.send_file(
        &path,
        &FileSend {
            echo: true,
            ..FileSend::default()
        },
    )
    .expect("send");
    run(&mut s);
    let row: String = s
        .row(0)
        .iter()
        .filter_map(|c| c.codepoints().next())
        .filter(|&cp| cp != 0)
        .filter_map(char::from_u32)
        .collect();
    assert_eq!(row.trim_end(), "visible");
}

#[test]
fn an_unechoed_send_leaves_the_screen_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = text_file(dir.path(), "invisible\n");
    let (mut s, _h) = connected();
    s.send_file(&path, &FileSend::default()).expect("send");
    run(&mut s);
    let row: String = s
        .row(0)
        .iter()
        .filter_map(|c| c.codepoints().next())
        .filter(|&cp| cp != 0)
        .filter_map(char::from_u32)
        .collect();
    assert_eq!(row.trim_end(), "");
}
