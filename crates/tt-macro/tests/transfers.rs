//! A macro running a file transfer, which is the one command that takes over
//! the connection and the one that blocks for minutes rather than for
//! milliseconds.
//!
//! `tt-xfer`'s suite proves the protocols interoperate and `tt-session`'s
//! proves the session hands the byte stream over. What is proved here is the
//! third join: that a script blocks until the transfer is over, reads the
//! right `result` afterwards, and can still be stopped while it waits.
//!
//! The protocol under test is the raw receive, because it is the only one of
//! the sixteen that needs no peer — `recvfile` is a line poured into a file
//! until it goes quiet, so a `MemoryTransport` is a complete far end for it.

use std::time::{Duration, Instant};

use tt_macro::{channel, NullUi, SessionHost};
use tt_session::{MemoryHandle, MemoryTransport, Session};
use tt_ttl::Interp;
use tt_vt::Config;

/// How long a test will wait for a macro before calling it hung.
const LIMIT: Duration = Duration::from_secs(20);

/// The frontend, for a script that runs a transfer.
///
/// Two things separate it from `driving.rs`'s loop. The far end speaks on a
/// signal from the *session* rather than in answer to the macro, because a
/// transfer's data does not arrive as a reply to anything; and it is fed only
/// after a pump, because a raw receive throws away whatever was already
/// buffered on its first parse (`raw.c:184`) — upstream making sure the prompt
/// that triggered the transfer does not land in the file.
struct Rig {
    session: Session,
    far: MemoryHandle,
    rx: tt_macro::MacroReceiver,
    thread: Option<std::thread::JoinHandle<()>>,
    sent: Vec<u8>,
}

impl Rig {
    fn start(script: &str) -> Rig {
        let mut session = Session::new(Config::default());
        let (transport, far) = MemoryTransport::new();
        session.connect(Box::new(transport));
        let link = session.link_macro();
        let (tx, rx) = channel().unwrap();

        let body = script.as_bytes().to_vec();
        let thread = std::thread::spawn(move || {
            let mut host = SessionHost::new(tx, link);
            let mut it = Interp::new("t.ttl", body, &mut host);
            it.run(&mut host);
        });
        Rig {
            session,
            far,
            rx,
            thread: Some(thread),
            sent: Vec::new(),
        }
    }

    /// One turn of the frontend's loop: run the macro's jobs, then the line.
    fn turn(&mut self) {
        self.rx.service(&mut self.session, &mut NullUi);
        self.session.pump(Duration::from_millis(1)).unwrap();
        let out = self.far.with(|s| std::mem::take(&mut s.outbound));
        self.sent.extend_from_slice(&out);
        std::thread::sleep(Duration::from_millis(1));
    }

    /// Turn until `f` is true of the rig, or panic saying what was being
    /// waited for.
    fn until(&mut self, what: &str, mut f: impl FnMut(&mut Rig) -> bool) {
        let start = Instant::now();
        while !f(self) {
            assert!(start.elapsed() < LIMIT, "gave up waiting for {what}");
            self.turn();
        }
    }

    fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }

    /// Run to the end of the script and hand back everything it sent.
    fn finish(mut self) -> Vec<u8> {
        self.until("the macro to end", |r| r.finished());
        // One more turn, so a last `sendln` is not left in the queue.
        self.turn();
        self.thread.take().unwrap().join().unwrap();
        self.sent
    }
}

/// The whole thing end to end: the command blocks, the bytes land in the file,
/// and `result` says it worked.
///
/// The 1-second auto-stop is the transfer's own clock, so this test really
/// does take about a second — and that is the point of it. A `recvfile` that
/// returned as soon as it had started would pass every other assertion here.
#[test]
fn recvfile_blocks_until_the_line_goes_quiet_and_then_reports_success() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("captured.bin");
    let script = format!(
        "recvfile '{}' 0 1\nint2str s result\nsendln s",
        out.display()
    );

    let mut rig = Rig::start(&script);
    rig.until("the transfer to start", |r| r.session.transfer().is_some());
    rig.far.feed(b"one\r\ntwo\r\n");

    let started = Instant::now();
    let sent = rig.finish();

    assert_eq!(std::fs::read(&out).unwrap(), b"one\r\ntwo\r\n");
    // `result` is 1, and it was sent *after* the transfer — which is the
    // blocking, since the script has no other way to reach this line.
    assert_eq!(sent, b"1\r");
    // Two clocks, and the slack is what stands between them. The transfer's
    // deadline is `GetTickCount64` on Windows — faithfully, since upstream's
    // `FTSetTimeOut` is `SetTimer` — whose resolution is one system tick, about
    // 15.6 ms, on a counter `Instant`'s QPC knows nothing about. So a nominal
    // second can measure 993 ms with nothing wrong, which is exactly what CI
    // reported. What is being asserted is that it *waited*: a `recvfile` that
    // returned as soon as it started comes back in milliseconds and still
    // fails this by three orders of magnitude.
    const TICK: Duration = Duration::from_millis(50);
    assert!(
        started.elapsed() + TICK >= Duration::from_secs(1),
        "it cannot have waited out a one-second auto-stop in {:?}",
        started.elapsed()
    );
}

/// Nothing arrived, so nothing is written and the file is still created — and
/// `recvfile` waits for ever, because `raw.c:168` arms the auto-stop timer in
/// the packet reader and a transfer with no packets never starts the clock.
/// The auto-stop argument is "quiet for this long *after* something", not "give
/// up after this long".
#[test]
fn a_recvfile_that_receives_nothing_never_stops_by_itself() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("nothing.bin");
    let script = format!("recvfile '{}' 0 1\nsendln 'ended'", out.display());

    let mut rig = Rig::start(&script);
    rig.until("the transfer to start", |r| r.session.transfer().is_some());

    // Three times the auto-stop, and it is still waiting.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        rig.turn();
    }
    assert!(
        !rig.finished(),
        "it stopped without ever receiving anything"
    );
    assert!(rig.session.transfer().is_some());

    // Which leaves the End button, and it works from inside a transfer.
    rig.rx.cancel();
    rig.until("End to release it", |r| r.finished());
    assert!(rig.sent.is_empty(), "a cancelled macro ran the next line");
    assert_eq!(std::fs::read(&out).unwrap(), b"");
}

/// End stops a transfer that is running, and the script reads the failure.
///
/// It does not stop *here*: `cancel_transfer` asks, the protocol ends on its
/// own terms, and this is the wait that has to survive the gap.
#[test]
fn cancelling_ends_a_running_transfer() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("partial.bin");
    // Auto-stop 0 — upstream's "wait for ever" — so nothing but the cancel can
    // end it.
    let script = format!("recvfile '{}' 0 0", out.display());

    let mut rig = Rig::start(&script);
    rig.until("the transfer to start", |r| r.session.transfer().is_some());
    rig.far.feed(b"half a file");
    rig.until("the bytes to be written", |r| {
        r.session.transfer().is_some_and(|t| t.progress.bytes >= 11)
    });

    rig.rx.cancel();
    rig.until("the cancel to take", |r| r.finished());
    assert!(rig.session.transfer().is_none());
    // What had arrived is on disk: `RawCancel` closes the file rather than
    // discarding it, which is the behaviour a half-captured log wants.
    assert_eq!(std::fs::read(&out).unwrap(), b"half a file");
}

/// A transfer that cannot start is not an error a script sees — it is
/// `result` 0, which is `filesys_proto.cpp:442` and what `ttdde.c` turns
/// `DDE_FNOTPROCESSED` into.
#[test]
fn a_transfer_that_cannot_start_reports_zero_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("not-here.bin");
    // `result` is seeded, so that the 0 below is the command's answer rather
    // than a variable nobody wrote to.
    let script = format!(
        "result = 9\nxmodemsend '{}' 2\nint2str s result\nsendln s",
        missing.display()
    );

    let start = Instant::now();
    let sent = Rig::start(&script).finish();
    assert_eq!(sent, b"0\r");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "it waited for a transfer that never began"
    );
}
