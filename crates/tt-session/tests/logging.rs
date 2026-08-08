//! Session logging: what lands in the file.
//!
//! The interesting claims are all about what is *not* in it — escape sequences
//! in a text log, timestamps in a raw one — and about rotation shifting
//! generations in the right direction, which is the kind of loop that quietly
//! collapses a history to two files when written backwards.

use std::fs;
use std::path::PathBuf;

use tt_session::{Event, LogMode, LogOptions, MemoryTransport, Session, Timestamp};
use tt_vt::Config;

/// A scratch directory that cleans up after itself, keyed by test name so
/// concurrent tests do not share one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("tt-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn session() -> Session {
    let mut s = Session::new(Config {
        cols: 40,
        rows: 6,
        ..Config::default()
    });
    let (transport, _handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    s
}

#[test]
fn a_text_log_holds_the_text_and_not_the_escape_sequences() {
    let dir = Scratch::new("text");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(&path, LogOptions::default()).unwrap();

    // Colour, a cursor move and an erase all around the text. None of it is
    // text, and none of it should reach the file — which is the whole reason
    // the tap is inside the parser rather than beside the log.
    s.feed(b"\x1b[31mhello\x1b[0m\r\n\x1b[2Kworld\r\n");
    s.stop_log();

    assert_eq!(fs::read_to_string(&path).unwrap(), "hello\nworld\n");
}

#[test]
fn a_raw_log_holds_every_byte_including_the_escape_sequences() {
    let dir = Scratch::new("raw");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(
        &path,
        LogOptions {
            mode: LogMode::Raw,
            ..LogOptions::default()
        },
    )
    .unwrap();

    // The point of a raw log is that it can be replayed, so it has to be
    // byte-identical to what arrived.
    let stream: &[u8] = b"\x1b[31mhello\x1b[0m\r\n";
    s.feed(stream);
    s.stop_log();

    assert_eq!(fs::read(&path).unwrap(), stream);
}

#[test]
fn timestamps_go_at_the_head_of_each_line_and_only_there() {
    let dir = Scratch::new("stamp");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(
        &path,
        LogOptions {
            timestamp: Timestamp::Elapsed,
            ..LogOptions::default()
        },
    )
    .unwrap();

    // Fed in two goes on purpose: the second chunk continues a line, and a
    // stamp there would prove the flag is per-write rather than per-line.
    s.feed(b"one");
    s.feed(b" more\r\ntwo\r\n");
    s.stop_log();

    let text = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text:?}");
    assert!(lines[0].starts_with('['), "{:?}", lines[0]);
    assert!(lines[0].ends_with("] one more"), "{:?}", lines[0]);
    assert!(lines[1].ends_with("] two"), "{:?}", lines[1]);
    assert_eq!(lines[0].matches('[').count(), 1, "one stamp per line");
}

#[test]
fn a_raw_log_is_never_timestamped() {
    // `filesys_log.cpp:243` turns the timestamp off with the mode, because a
    // `[time] ` in the middle of a byte capture makes it no longer a capture.
    let dir = Scratch::new("rawstamp");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(
        &path,
        LogOptions {
            mode: LogMode::Raw,
            timestamp: Timestamp::Elapsed,
            ..LogOptions::default()
        },
    )
    .unwrap();
    s.feed(b"abc\r\ndef\r\n");
    s.stop_log();

    assert_eq!(fs::read(&path).unwrap(), b"abc\r\ndef\r\n");
}

#[test]
fn rotation_shifts_the_generations_oldest_first() {
    // Renaming forwards overwrites `.2` with `.1` before `.2` has moved to
    // `.3`, and the history silently collapses to two files. This walks
    // backwards; the test is that all three generations survive and hold
    // different content.
    let dir = Scratch::new("rotate");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(
        &path,
        LogOptions {
            rotate_size: 16,
            rotate_keep: 3,
            ..LogOptions::default()
        },
    )
    .unwrap();

    for i in 0..8 {
        s.feed(format!("line{i} padded out\r\n").as_bytes());
    }
    s.stop_log();

    assert!(dir.path("session.log.3").exists());
    assert!(
        !dir.path("session.log.4").exists(),
        "kept more generations than asked for"
    );

    // The claim is the *order*: a lower generation number is newer. Rotation
    // happens after the write that crossed the threshold — upstream checks
    // `ByteCount > RotateSize` on the way out — so the current file can be
    // empty and the newest content sits in `.1`.
    let first_line_number = |p: PathBuf| -> Option<u32> {
        let text = fs::read_to_string(p).unwrap();
        let line = text.lines().next()?.to_string();
        line.strip_prefix("line")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    let mut seen: Vec<u32> = Vec::new();
    for n in [None, Some(1), Some(2), Some(3)] {
        let p = match n {
            None => path.clone(),
            Some(n) => dir.path(&format!("session.log.{n}")),
        };
        if let Some(first) = first_line_number(p) {
            seen.push(first);
        }
    }
    assert!(
        seen.len() >= 3,
        "expected several generations, got {seen:?}"
    );
    assert!(
        seen.windows(2).all(|w| w[0] > w[1]),
        "generations are not newest-first: {seen:?}"
    );
}

#[test]
fn nothing_is_logged_before_the_log_was_opened() {
    let dir = Scratch::new("late");
    let path = dir.path("session.log");
    let mut s = session();

    s.feed(b"before\r\n");
    s.start_log(&path, LogOptions::default()).unwrap();
    s.feed(b"after\r\n");
    s.stop_log();

    // The tap collects into a buffer the parser owns, so opening a log has to
    // discard whatever was already in it — otherwise the first write carries
    // however much of the session preceded it.
    assert_eq!(fs::read_to_string(&path).unwrap(), "after\n");
}

#[test]
fn appending_keeps_what_was_there_and_truncating_does_not() {
    let dir = Scratch::new("append");
    let path = dir.path("session.log");

    let mut s = session();
    s.start_log(&path, LogOptions::default()).unwrap();
    s.feed(b"first\r\n");
    s.stop_log();

    s.start_log(
        &path,
        LogOptions {
            append: true,
            ..LogOptions::default()
        },
    )
    .unwrap();
    s.feed(b"second\r\n");
    s.stop_log();
    assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");

    s.start_log(&path, LogOptions::default()).unwrap();
    s.feed(b"third\r\n");
    s.stop_log();
    assert_eq!(fs::read_to_string(&path).unwrap(), "third\n");
}

#[test]
fn a_failed_write_closes_the_log_and_says_so_once() {
    // A directory cannot be opened for writing, so this fails at `start_log`
    // rather than mid-stream — which is the case worth checking, since a
    // frontend must not end up believing it is logging when it is not.
    let dir = Scratch::new("fail");
    let mut s = session();
    let err = s.start_log(&dir.0.clone(), LogOptions::default());
    assert!(err.is_err());
    assert!(s.log_path().is_none());

    // And no event was queued for a log that never opened: the error came back
    // from the call.
    s.feed(b"x");
    assert!(!s
        .drain_events()
        .iter()
        .any(|e| matches!(e, Event::LogFailed(_))));
}

#[test]
fn the_log_reports_where_it_is_and_how_much_it_has_written() {
    let dir = Scratch::new("status");
    let path = dir.path("session.log");
    let mut s = session();
    assert!(s.log_path().is_none());
    assert_eq!(s.log_bytes(), 0);

    s.start_log(&path, LogOptions::default()).unwrap();
    assert_eq!(s.log_path(), Some(path.as_path()));
    s.feed(b"hello\r\n");
    assert_eq!(s.log_bytes(), 6, "five characters and a newline");

    s.stop_log();
    assert!(s.log_path().is_none());
}

#[test]
fn crlf_is_available_for_a_byte_identical_tera_term_log() {
    let dir = Scratch::new("crlf");
    let path = dir.path("session.log");
    let mut s = session();
    s.start_log(
        &path,
        LogOptions {
            crlf: true,
            ..LogOptions::default()
        },
    )
    .unwrap();
    s.feed(b"one\r\ntwo\r\n");
    s.stop_log();

    assert_eq!(fs::read(&path).unwrap(), b"one\r\ntwo\r\n");
}
