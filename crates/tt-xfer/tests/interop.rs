//! The protocols against the reference Unix implementations, over a real pty.
//!
//! This is `xfer/run_tests.sh` moved inside the crate, and it is the test that
//! matters: everything else here checks that the plumbing is wired the way the
//! headers say, while this checks that a file arrives byte for byte at the
//! other end of `sz`, `rz` and `gkermit`. The spike proved the C could do it
//! driven by a file descriptor; what is proved here is that it still does when
//! the bytes come through a queue instead, which is the arrangement the
//! terminal needs and the one thing the spike could not test.
//!
//! Needs `lrzsz` and `gkermit`. Without them the cases skip loudly rather than
//! passing quietly.

#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_conn::Transport;
use tt_xfer::{Direction, Job, KermitMode, Link, Options, Transfer, XmodemOpt, YmodemOpt};

/// A payload with the bytes that break naive implementations in it: the
/// ZMODEM and Kermit control set, a run of `0xff`, and a NUL.
fn payload(path: &Path, size: usize) {
    let mut f = std::fs::File::create(path).unwrap();
    // Deterministic rather than random: a failure that only reproduces one
    // run in twenty is a failure nobody fixes.
    let body: Vec<u8> = (0..size).map(|i| (i * 31 + i / 251) as u8).collect();
    f.write_all(&body).unwrap();
    f.write_all(b"\x11\x13\x18\x0d\x0a\x1a\xff\xff\xff\x00")
        .unwrap();
}

fn have(tool: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn the peer on a pty.
///
/// Through `sh -c` so its diagnostics can go to `/dev/null`. That redirect is
/// load-bearing rather than tidiness: the peer shares the pty, so a warning it
/// prints lands in the protocol stream — and `ymodem.c` meets an unexpected
/// byte with `assert(0)`.
fn spawn(cmd: &str, cwd: &Path) -> PtyConn {
    PtyConn::open(&PtyParams {
        argv: vec!["sh".into(), "-c".into(), format!("{cmd} 2>/dev/null")],
        cwd: Some(cwd.to_path_buf()),
        login_shell: false,
        ..PtyParams::default()
    })
    .expect("cannot open a pty")
}

/// Wait until the pty has something to say, or the deadline passes.
fn wait_readable(conn: &PtyConn, timeout: Duration) {
    let Some(fd) = conn.poll_fd() else { return };
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = timeout.as_millis().min(50) as libc::c_int;
    // SAFETY: one initialised pollfd, and the fd outlives the call because
    // `conn` is borrowed for it.
    unsafe { libc::poll(&mut pfd, 1, ms.max(1)) };
}

/// Run a transfer to completion against `conn`.
///
/// This is the loop `tt-session` will own: read, feed, poll, write, and treat
/// the protocol's armed timeout as the sleep bound. It is written out here
/// rather than hidden in a helper because it is the thing under test.
fn drive(xfer: &mut Transfer, conn: &mut PtyConn, limit: Duration) -> String {
    let start = Instant::now();
    let mut rx = Vec::new();
    let mut events = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut peer_gone = false;

    loop {
        // Outbound first: a protocol waiting for an ACK it has not sent the
        // question for makes no progress.
        if !pending.is_empty() {
            match conn.write(&pending, Duration::from_millis(50)) {
                Ok(0) => {}
                Ok(n) => {
                    pending.drain(..n);
                }
                Err(e) if e.is_disconnected() => peer_gone = true,
                Err(e) => return format!("write failed: {e}"),
            }
        }

        if !peer_gone {
            rx.clear();
            events.clear();
            match conn.read(&mut rx, &mut events) {
                Ok(0) => {}
                Ok(_) => {
                    let mut at = 0;
                    while at < rx.len() {
                        let took = xfer.feed(&rx[at..]);
                        if took == 0 {
                            // The 64 KB buffer is full and the protocol has
                            // not drained it. Parsing is what empties it.
                            xfer.poll();
                            if xfer.is_done() {
                                break;
                            }
                            continue;
                        }
                        at += took;
                    }
                }
                Err(e) if e.is_disconnected() => peer_gone = true,
                Err(e) => return format!("read failed: {e}"),
            }
        }

        xfer.poll();
        xfer.take_output(&mut pending);

        if xfer.is_done() {
            return String::new();
        }

        if peer_gone {
            // The far end went away. Tell the protocol, give it one more
            // parse to notice, and stop — `rb` exits without acknowledging
            // the closing null block of a YMODEM batch, so this is a normal
            // ending for a transfer that is nonetheless complete.
            xfer.disconnected();
            xfer.poll();
            return String::new();
        }

        match xfer.wait_hint() {
            Some(left) if left.is_zero() => {
                xfer.fire_timeout();
                xfer.take_output(&mut pending);
            }
            Some(left) => wait_readable(conn, left),
            None => wait_readable(conn, Duration::from_millis(50)),
        }

        if start.elapsed() > limit {
            return format!("wall-clock limit hit after {:?}", start.elapsed());
        }
    }
}

struct Case {
    dir: tempfile::TempDir,
    src: PathBuf,
    out: PathBuf,
}

fn case(size: usize) -> Case {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("payload.bin");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    payload(&src, size);
    Case { dir, src, out }
}

/// XMODEM has no length field, so the receiver gets whole blocks and trailing
/// padding is correct rather than a failure. Kermit transmits names in its
/// "common form", which is upper case, so the file may arrive as `PAYLOAD.BIN`
/// — also correct on both sides.
fn compare(c: &Case, padded: bool) {
    let mut got = c.out.join("payload.bin");
    if !got.exists() {
        got = c.out.join("PAYLOAD.BIN");
    }
    assert!(
        got.exists(),
        "nothing received into {}: {:?}",
        c.out.display(),
        std::fs::read_dir(&c.out)
            .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
            .unwrap_or_default()
    );
    let want = std::fs::read(&c.src).unwrap();
    let have = std::fs::read(&got).unwrap();
    if padded {
        assert!(
            have.len() >= want.len(),
            "short: {} of {}",
            have.len(),
            want.len()
        );
        assert_eq!(&have[..want.len()], &want[..], "payload differs");
    } else {
        assert_eq!(have.len(), want.len(), "length differs");
        assert_eq!(have, want, "payload differs");
    }
}

fn receive_from(peer: &str, job: Job, size: usize, padded: bool, name: Option<&str>) {
    let c = case(size);
    let cmd = peer.replace("@FILE@", c.src.to_str().unwrap());
    let mut conn = spawn(&cmd, c.dir.path());
    let mut xfer = Transfer::receive(job, &c.out, name, &opts()).unwrap();
    let err = drive(&mut xfer, &mut conn, Duration::from_secs(90));
    assert!(err.is_empty(), "{err} ({xfer:?})");
    compare(&c, padded);
}

fn send_to(peer: &str, job: Job, size: usize, padded: bool) {
    let c = case(size);
    let mut conn = spawn(peer, &c.out);
    let mut xfer = Transfer::send(job, [&c.src], &opts()).unwrap();
    let err = drive(&mut xfer, &mut conn, Duration::from_secs(90));
    assert!(err.is_empty(), "{err} ({xfer:?})");
    compare(&c, padded);
}

fn opts() -> Options {
    Options {
        link: Link::local_pty(),
        ..Options::default()
    }
}

macro_rules! needs {
    ($tool:literal) => {
        if !have($tool) {
            eprintln!("skipping: {} is not installed", $tool);
            return;
        }
    };
}

const XRECV: Job = Job::XModem {
    dir: Direction::Receive,
    opt: XmodemOpt::Crc,
    text: false,
};
const XSEND: Job = Job::XModem {
    dir: Direction::Send,
    opt: XmodemOpt::Crc,
    text: false,
};
const YRECV: Job = Job::YModem {
    dir: Direction::Receive,
    opt: YmodemOpt::K1,
};
const YSEND: Job = Job::YModem {
    dir: Direction::Send,
    opt: YmodemOpt::K1,
};
const ZRECV: Job = Job::ZModem {
    dir: Direction::Receive,
    binary: true,
    auto: false,
};
const ZSEND: Job = Job::ZModem {
    dir: Direction::Send,
    binary: true,
    auto: false,
};

#[test]
fn xmodem_receive() {
    needs!("sx");
    receive_from("sx @FILE@", XRECV, 4096, true, Some("payload.bin"));
}

#[test]
fn xmodem_send() {
    needs!("rx");
    send_to("rx payload.bin", XSEND, 4096, true);
}

#[test]
fn ymodem_receive() {
    needs!("sb");
    receive_from("sb @FILE@", YRECV, 4096, false, None);
}

#[test]
fn ymodem_send() {
    needs!("rb");
    send_to("rb", YSEND, 4096, false);
}

#[test]
fn zmodem_receive() {
    needs!("sz");
    receive_from("sz -b @FILE@", ZRECV, 4096, false, None);
}

#[test]
fn zmodem_send() {
    needs!("rz");
    send_to("rz -b", ZSEND, 4096, false);
}

/// A megabyte, which is what exercises zmodem's windowing and the multi-packet
/// paths — and, here, the 64 KB receive queue actually filling up.
#[test]
fn zmodem_receive_1m() {
    needs!("sz");
    receive_from("sz -b @FILE@", ZRECV, 1024 * 1024, false, None);
}

#[test]
fn zmodem_send_1m() {
    needs!("rz");
    send_to("rz -b", ZSEND, 1024 * 1024, false);
}

/// G-Kermit rather than C-Kermit: C-Kermit sees a pty as a tty and drops into
/// interactive command mode instead of speaking the protocol.
#[test]
fn kermit_receive() {
    needs!("gkermit");
    receive_from(
        "gkermit -s @FILE@",
        Job::Kermit {
            mode: KermitMode::Receive,
        },
        4096,
        false,
        None,
    );
}

#[test]
fn kermit_send() {
    needs!("gkermit");
    send_to(
        "gkermit -r",
        Job::Kermit {
            mode: KermitMode::Send,
        },
        4096,
        false,
    );
}

/// A cancelled transfer must stop, and stop *soon*.
///
/// ZMODEM's cancel is not a state change: it sends `ZCAN`, arms a 500 ms timer
/// and finishes when that fires (`zmodem.c:1586`). A host that ignores the
/// timer leaves the transfer waiting for a peer it has already told to go
/// away, which looks like a hang in the UI at exactly the moment the user
/// asked for it to stop.
#[test]
fn a_cancelled_transfer_finishes() {
    needs!("rz");
    let c = case(1024 * 1024);
    let mut conn = spawn("rz -b", &c.out);
    let mut xfer = Transfer::send(ZSEND, [&c.src], &opts()).unwrap();

    // Get it moving first, so the cancel interrupts a real transfer.
    let start = Instant::now();
    let mut pending = Vec::new();
    let mut rx = Vec::new();
    let mut events = Vec::new();
    while xfer.progress().bytes == 0 && start.elapsed() < Duration::from_secs(30) {
        rx.clear();
        events.clear();
        let _ = conn.read(&mut rx, &mut events);
        xfer.feed(&rx);
        xfer.poll();
        pending.clear();
        xfer.take_output(&mut pending);
        let _ = conn.write(&pending, Duration::from_millis(50));
        wait_readable(&conn, Duration::from_millis(20));
    }
    assert!(xfer.progress().bytes > 0, "the transfer never started");

    xfer.cancel();
    let at_cancel = Instant::now();
    let err = drive(&mut xfer, &mut conn, Duration::from_secs(10));
    assert!(err.is_empty(), "{err}");
    assert!(xfer.is_done());
    assert!(
        !xfer.succeeded(),
        "a cancelled transfer did not report failure"
    );
    assert!(
        at_cancel.elapsed() < Duration::from_secs(5),
        "cancel took {:?}",
        at_cancel.elapsed()
    );
}

/// The far end going away is not the same as cancelling, and the protocols
/// know the difference: each tests `cv->Ready` and the ones that cannot finish
/// call `ProtoEnd` there and then, because they know Parse will not run again.
#[test]
fn a_dropped_connection_ends_the_transfer() {
    needs!("rz");
    let c = case(4096);
    let conn = spawn("rz -b", &c.out);
    let mut xfer = Transfer::send(ZSEND, [&c.src], &opts()).unwrap();
    xfer.poll();
    if let Some(pid) = conn.pid() {
        // SAFETY: a signal to a child this test spawned.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }

    xfer.disconnected();
    xfer.cancel();
    xfer.poll();
    assert!(xfer.is_done());
    assert!(!xfer.succeeded());
}
