//! The paced send queue against a whole session — upstream's `SendMem`.
//!
//! `send.rs`'s own tests are about the algorithm and hand it a fake clock.
//! These are about the *composition*: that a file reaches the wire through the
//! terminal's own encoding, that a pace really costs wall-clock time, that a
//! transport which will not take a line does not lose it, and that everything
//! which can end a send ends it exactly once.

use std::time::{Duration, Instant};

use tt_session::send::{Body, FileSend, Gate, Job, Pace, SendEnd, SendError};
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

/// `PasteDelayPerLine` — the setting that has been read, written and acted on
/// by nothing since this program had a settings file. It ships at 10 ms, so a
/// paste is paced on a fresh install, which is what `clipboar.c:205` asks for.
#[test]
fn a_paste_is_paced_by_the_setting_it_has_always_carried() {
    let (mut s, h) = connected();
    assert_eq!(s.settings().clipboard_paste_delay_per_line, 10);

    s.paste("one\ntwo\nthree\n", false).expect("paste");
    // The first line and no more: the rest is on the frontend's timer.
    assert_eq!(h.outbound(), b"one\r");
    assert!(s.sending());
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\rthree\r");

    // ...and with the delay switched off it is over before `paste` returns,
    // which is what every caller written before the queue existed expects.
    let (mut s, h) = connected();
    let mut settings = s.settings().clone();
    settings.clipboard_paste_delay_per_line = 0;
    s.set_settings(settings);
    s.paste("one\ntwo\nthree\n", false).expect("paste");
    assert_eq!(h.outbound(), b"one\rtwo\rthree\r");
    assert!(!s.sending());
}

/// A second paste while the first is still going is dropped, not queued —
/// `clipboar.c:231` refuses one whenever `TalkStatus` is not the keyboard.
#[test]
fn a_paste_during_a_paste_is_dropped() {
    let (mut s, h) = connected();
    s.paste("one\ntwo\nthree\n", false).expect("paste");
    assert!(s.sending());
    s.paste("interloper\n", false).expect("paste");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\rthree\r");
}

// --- the gate, which is the half upstream has no equivalent for --------------

/// Drive the queue *without* feeding it anything, for as long as `ms`.
///
/// A gated send that nobody answers should be sitting still, and the only way
/// to say that is to keep offering it the chance to move.
fn idle(s: &mut Session, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        s.service_send().expect("service");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn prompt_gate(pattern: &str, timeout_ms: u64) -> Gate {
    Gate::Prompt {
        re: regex::Regex::new(pattern).expect("pattern"),
        timeout: Duration::from_millis(timeout_ms),
    }
}

/// The whole point of the feature: the next line goes when the device says it
/// is ready, and not before.
#[test]
fn a_prompt_holds_the_next_line_until_it_arrives() {
    let (mut s, h) = connected();
    s.queue_send(Job::new(Body::Text("one\rtwo\rthree\r".into())).gated(prompt_gate(r"# $", 5000)))
        .expect("queue");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\r");

    // Nothing arrives, so nothing moves — for a good deal longer than the pace.
    idle(&mut s, 60);
    assert_eq!(h.outbound(), b"one\r");
    assert!(s.send_progress().unwrap().gated);

    // The prompt has **no line ending**, which is the case a whole-line matcher
    // could never see.
    s.feed(b"Switch# ");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\rtwo\r");

    s.feed(b"Switch# ");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\rtwo\rthree\r");
    run(&mut s);
    assert_eq!(s.send_progress(), None);
}

/// A gate with no pace at all still goes one line at a time. It has to: the
/// gate holds the queue *between* pieces, and a single-piece job would send the
/// whole file in one write with the gate never consulted.
#[test]
fn a_gate_with_no_pace_still_goes_one_line_at_a_time() {
    let (mut s, h) = connected();
    s.queue_send(Job::new(Body::Text("one\rtwo\r".into())).gated(prompt_gate("go", 5000)))
        .expect("queue");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\r");
    assert_eq!(s.send_progress().unwrap().sent, 4);
}

/// The gate the settings describe, including the two ways they describe none.
#[test]
fn the_settings_answer_for_the_gate() {
    let mut settings = tt_session::Settings::default();
    assert!(tt_session::send::gate_from(&settings).is_none());

    settings.transfer_send_gate = tt_config::TransferSendGate::Prompt;
    // A prompt with no pattern is no gate: holding every line for its timeout
    // against an empty expression is worse than not holding it at all.
    assert!(tt_session::send::gate_from(&settings).is_none());

    settings.transfer_send_gate_pattern = "# $".into();
    assert_eq!(
        tt_session::send::gate_from(&settings),
        Gate::Prompt {
            re: regex::Regex::new("# $").unwrap(),
            timeout: Duration::from_millis(500),
        }
    );

    // ...and a pattern the engine will not compile is the same answer, because
    // the alternative is a send that stalls on every line and says nothing.
    settings.transfer_send_gate_pattern = "(unclosed".into();
    assert!(tt_session::send::gate_from(&settings).is_none());

    settings.transfer_send_gate = tt_config::TransferSendGate::Quiet;
    assert_eq!(
        tt_session::send::gate_from(&settings),
        Gate::Quiet {
            idle: Duration::from_millis(300),
            timeout: Duration::from_millis(500),
        }
    );
}

/// A whole line matches too, CR and all — the same rule `waitregex` follows,
/// so one pattern means one thing in both places.
#[test]
fn a_whole_line_can_be_the_prompt() {
    let (mut s, h) = connected();
    s.queue_send(Job::new(Body::Text("one\rtwo\r".into())).gated(prompt_gate("ready", 5000)))
        .expect("queue");
    s.service_send().expect("service");
    s.feed(b"ready\r\n");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\rtwo\r");
}

/// A wrong pattern must not wedge the send. The line goes anyway, and the count
/// of how many went that way is what says the pattern was wrong.
#[test]
fn a_gate_nobody_answers_releases_on_the_timeout_and_counts_it() {
    let (mut s, h) = connected();
    s.queue_send(Job::new(Body::Text("one\rtwo\r".into())).gated(prompt_gate("never-appears", 40)))
        .expect("queue");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\r");
    assert_eq!(s.send_progress().unwrap().timeouts, 0);

    idle(&mut s, 80);
    assert_eq!(h.outbound(), b"one\rtwo\r");
    run(&mut s);
    let done = s
        .drain_events()
        .into_iter()
        .find_map(|e| match e {
            Event::SendDone(o) => Some(*o),
            _ => None,
        })
        .expect("a done event");
    assert_eq!(done.end, SendEnd::Finished);
    assert_eq!(done.timeouts, 1);
}

/// An answer that arrives during the pace's own interval has already opened the
/// gate by the time the interval expires — the watcher is armed when the piece
/// goes out, not when the gate is entered.
#[test]
fn an_answer_during_the_pace_interval_is_not_missed() {
    let (mut s, h) = connected();
    s.queue_send(
        Job::new(Body::Text("one\rtwo\r".into()))
            .paced(Pace::PerLine(Duration::from_millis(40)))
            .gated(prompt_gate("ok", 5000)),
    )
    .expect("queue");
    s.service_send().expect("service");
    s.feed(b"ok");
    std::thread::sleep(Duration::from_millis(45));
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\rtwo\r");
}

/// The echo gate, for a console with no fixed prompt. A substring, because a
/// device that prints its prompt and the echo on one line is the commonest
/// arrangement there is.
#[test]
fn the_echo_gate_waits_for_the_line_to_come_back() {
    let (mut s, h) = connected();
    s.queue_send(
        Job::new(Body::Text("alpha\rbravo\r".into())).gated(Gate::Echo {
            timeout: Duration::from_millis(5000),
        }),
    )
    .expect("queue");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"alpha\r");

    // Something else coming back is not the echo.
    s.feed(b"banner text\r\n");
    idle(&mut s, 30);
    assert_eq!(h.outbound(), b"alpha\r");

    s.feed(b"Switch> alpha");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"alpha\rbravo\r");
}

/// The quiet gate answers to the clock rather than to any text, so it needs no
/// pattern and no echo — and it must not fire while the far end is still
/// talking.
#[test]
fn the_quiet_gate_waits_for_the_talking_to_stop() {
    let (mut s, h) = connected();
    s.queue_send(
        Job::new(Body::Text("one\rtwo\r".into())).gated(Gate::Quiet {
            idle: Duration::from_millis(50),
            timeout: Duration::from_millis(5000),
        }),
    )
    .expect("queue");
    s.service_send().expect("service");
    assert_eq!(h.outbound(), b"one\r");

    // Kept talking: each byte pushes the quiet window out again.
    for _ in 0..5 {
        s.feed(b"still going\r\n");
        idle(&mut s, 20);
    }
    assert_eq!(h.outbound(), b"one\r");

    // ...and then stopped.
    idle(&mut s, 90);
    assert_eq!(h.outbound(), b"one\rtwo\r");
}

/// The tap costs something to keep, so it is on for exactly as long as a gate
/// is watching — and a send that ends under a running macro must not take the
/// macro's tap with it.
#[test]
fn the_parser_tap_belongs_to_whoever_still_wants_it() {
    let (mut s, _h) = connected();
    let link = s.link_macro();
    s.queue_send(Job::new(Body::Text("one\rtwo\r".into())).gated(prompt_gate("ok", 30)))
        .expect("queue");
    run(&mut s);
    assert!(!s.sending());
    // The macro still has it: a byte arriving now still reaches its ring.
    s.feed(b"after the send\r\n");
    assert!(
        !link.is_empty(),
        "the macro's tap was switched off under it"
    );
}

// --- the other pacing layer, which is the serial port's own ------------------

fn serial(per_char: u16, per_line: u16) -> (Session, MemoryHandle) {
    let mut s = Session::new(Config::default());
    let mut settings = s.settings().clone();
    settings.serial_delay_per_char = per_char as i32;
    settings.serial_delay_per_line = per_line as i32;
    s.set_settings(settings);
    let (transport, handle) = MemoryTransport::with_kind(tt_conn::LinkKind::Serial {
        baud: 115200,
        seven_bit: false,
    });
    s.connect(Box::new(transport));
    (s, handle)
}

/// `DelayPerChar` — read, written and acted on by nothing since this program
/// had a settings file. It paces **everything** the port sends, not a queued
/// job: this is a plain keystroke.
#[test]
fn a_serial_port_with_a_character_delay_sends_one_byte_at_a_time() {
    let (mut s, h) = serial(20, 0);
    s.send_text("abc").expect("send");
    assert_eq!(h.outbound(), b"a");
    // ...and the frontend's send timer is what offers the rest, exactly as it
    // does for a queued job. There is one timer and two reasons for it.
    assert!(s.send_deadline().is_some());
    run(&mut s);
    assert_eq!(h.outbound(), b"abc");
}

#[test]
fn a_line_delay_sends_up_to_the_line_end() {
    let (mut s, h) = serial(0, 20);
    s.send_text("one\rtwo\r").expect("send");
    // `CRSend` ships as a bare CR, so that is the line end upstream looks for.
    assert_eq!(h.outbound(), b"one\r");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\r");
}

/// The line end is the **live** newline mode and not the file's — `cv->CRSend`
/// at `commlib.c:1071`, which `SM 20` moves.
#[test]
fn the_line_end_follows_the_terminals_newline_mode() {
    let (mut s, h) = serial(0, 20);
    s.feed(b"\x1b[20h"); // LNM, so CR becomes CRLF and the line end is the LF
    s.send_text("one\rtwo\r").expect("send");
    assert_eq!(h.outbound(), b"one\r\n");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\r\ntwo\r\n");
}

/// Both set is not "a line, then a character": upstream skips the line scan
/// entirely and sends one byte, waiting the per-line interval only when that
/// one byte happens to be the line end (`commlib.c:1077`).
#[test]
fn both_delays_set_sends_one_byte_and_mostly_waits_the_character_one() {
    let (mut s, h) = serial(15, 500);
    s.send_text("ab\r").expect("send");
    assert_eq!(h.outbound(), b"a");
    // Two characters at 15 ms each; if the line interval were being used for
    // them this would take a second.
    let started = Instant::now();
    for _ in 0..2 {
        let d = s.send_deadline().expect("armed");
        std::thread::sleep(d);
        s.service_send().expect("service");
    }
    assert_eq!(h.outbound(), b"ab\r");
    assert!(started.elapsed() < Duration::from_millis(400));
}

/// `cv.DelayFlag` is cleared for the length of a protocol transfer
/// (`filesys_proto.cpp:436`): pacing a ZMODEM packet would time it out.
#[test]
fn a_protocol_transfer_is_not_paced() {
    let dir = tempfile::tempdir().unwrap();
    let (mut s, _h) = serial(50, 0);
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
    // Nothing is holding the line, so nothing is armed for it.
    assert_eq!(s.send_deadline(), None);
    s.cancel_transfer();
}

/// It is the *port's* governor. A telnet or SSH session carrying the same
/// settings is not paced, because `commlib.c:1068` tests `PortType==IdSerial`.
#[test]
fn only_a_serial_port_is_paced() {
    let mut s = Session::new(Config::default());
    let mut settings = s.settings().clone();
    settings.serial_delay_per_char = 50;
    s.set_settings(settings);
    let (transport, h) = MemoryTransport::new();
    s.connect(Box::new(transport));
    s.send_text("abc").expect("send");
    assert_eq!(h.outbound(), b"abc");
    assert_eq!(s.send_deadline(), None);
}

/// `setserialdelaychar` and `setserialdelayline` — the two macro commands that
/// were refused for want of this queue.
///
/// The change is **queued**, which is `SendMemSetDelay`: it paces what comes
/// after the send in front of it and not the tail of that send.
#[test]
fn a_macro_can_move_the_serial_delays_and_the_change_waits_its_turn() {
    let (mut s, h) = serial(0, 0);
    // Nothing paced yet, so a line goes out whole.
    s.send_text("first\r").expect("send");
    assert_eq!(h.outbound(), b"first\r");

    assert!(s.queue_write_delay(Duration::ZERO, Duration::from_millis(20)));
    run(&mut s);
    assert_eq!(s.write_delay().per_line(), Duration::from_millis(20));

    s.send_text("one\rtwo\r").expect("send");
    assert_eq!(h.outbound(), b"first\rone\r");
    run(&mut s);
    assert_eq!(h.outbound(), b"first\rone\rtwo\r");

    // ...and setting them back to nothing lets go of the line at once rather
    // than waiting out an interval that no longer exists.
    assert!(s.queue_write_delay(Duration::ZERO, Duration::ZERO));
    run(&mut s);
    s.send_text("three\rfour\r").expect("send");
    assert_eq!(h.outbound(), b"first\rone\rtwo\rthree\rfour\r");
}

/// It is the port's, so a session that is not a serial port answers no — which
/// is what the command reports, and is not a failure.
#[test]
fn moving_the_serial_delays_over_something_else_answers_no() {
    let (mut s, _h) = connected();
    assert!(!s.queue_write_delay(Duration::ZERO, Duration::from_millis(20)));
    assert!(!s.sending());
}

/// Both layers at once, which is what a paste on a slow serial console is.
#[test]
fn a_paced_paste_on_a_paced_port_is_subject_to_both() {
    let (mut s, h) = serial(0, 20);
    s.paste("one\ntwo\n", false).expect("paste");
    run(&mut s);
    assert_eq!(h.outbound(), b"one\rtwo\r");
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
