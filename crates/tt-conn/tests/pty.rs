#![cfg(unix)]

//! The local shell, against real forked processes.
//!
//! Unlike every other suite in this crate these need no hardware, no server
//! and no environment variables — a pty is always available — so they run
//! on every POSIX build, unconditionally. That is worth saying out loud,
//! because the
//! serial, SSH and telnet suites all skip themselves when their rig is absent
//! and it would be easy to assume this one does too.

use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_conn::{Error, Transport};

/// Run a `/bin/sh` script. `/bin/sh` rather than `$SHELL` so the tests assert
/// on POSIX behaviour rather than on whatever the developer happens to run.
fn sh(script: &str) -> PtyParams {
    PtyParams {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        ..PtyParams::default()
    }
}

/// Read until `done` says so, the child goes away, or five seconds pass.
///
/// Returns everything read and the error that ended it, if one did. Five
/// seconds is enormous for a local `printf` and is a deadlock guard, not a
/// timing assumption.
fn drain(conn: &mut PtyConn, done: impl Fn(&[u8]) -> bool) -> (Vec<u8>, Option<Error>) {
    drain_for(conn, Duration::from_secs(5), done)
}

fn drain_for(
    conn: &mut PtyConn,
    limit: Duration,
    done: impl Fn(&[u8]) -> bool,
) -> (Vec<u8>, Option<Error>) {
    let deadline = Instant::now() + limit;
    let (mut data, mut events) = (Vec::new(), Vec::new());
    loop {
        match conn.read(&mut data, &mut events) {
            Ok(0) => {}
            Ok(_) => {
                if done(&data) {
                    return (data, None);
                }
                continue;
            }
            Err(e) => return (data, Some(e)),
        }
        if done(&data) || Instant::now() >= deadline {
            return (data, None);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_child_that_prints_is_read_back() {
    let mut conn = PtyConn::open(&sh("printf hello")).expect("open");
    let (data, _) = drain(&mut conn, |d| d.ends_with(b"hello"));
    assert_eq!(text(&data), "hello");
}

/// The trap this whole transport turns on: the child exits, and the terminal
/// has to *hear about it*. If the slave end were still held open here the read
/// would simply never end — no error, no data — and the window would sit
/// waiting on a shell that left minutes ago.
#[test]
fn the_child_exiting_is_a_disconnect_not_a_quiet_line() {
    let mut conn = PtyConn::open(&sh("exit 3")).expect("open");
    let (_, err) = drain(&mut conn, |_| false);
    assert!(
        matches!(err, Some(Error::Disconnected)),
        "expected Disconnected, got {err:?}"
    );
    let exit = conn.exit_status().expect("the child was reaped");
    assert_eq!(exit.code, 3);
    assert_eq!(exit.signal, None);
    assert_eq!(exit.to_string(), "exited with status 3");
}

/// Output written just before the exit must arrive *before* the disconnect,
/// not be lost to it. The last line a dying shell prints is usually the one
/// that says why.
#[test]
fn output_written_before_the_exit_is_not_lost() {
    let mut conn = PtyConn::open(&sh("printf goodbye; exit 0")).expect("open");
    let (data, err) = drain(&mut conn, |_| false);
    assert_eq!(text(&data), "goodbye");
    assert!(matches!(err, Some(Error::Disconnected)));
    assert_eq!(conn.exit_status().map(|e| e.code), Some(0));
}

/// A shell killed by a signal has no meaningful exit code, so the signal is
/// what gets reported.
#[test]
fn a_signalled_child_reports_the_signal() {
    let mut conn = PtyConn::open(&sh("kill -TERM $$")).expect("open");
    let (_, err) = drain(&mut conn, |_| false);
    assert!(matches!(err, Some(Error::Disconnected)));
    let exit = conn.exit_status().expect("the child was reaped");
    assert!(exit.signal.is_some(), "expected a signal, got {exit:?}");
    assert!(exit.to_string().starts_with("killed by"));
}

/// Silence is `Ok(0)`, the same as on a serial line. A terminal spends nearly
/// all its time here, so it must be cheap and must not look like a failure.
#[test]
fn a_quiet_child_reads_zero_rather_than_erroring() {
    let mut conn = PtyConn::open(&sh("sleep 5")).expect("open");
    let (mut data, mut events) = (Vec::new(), Vec::new());
    for _ in 0..20 {
        assert_eq!(
            conn.read(&mut data, &mut events)
                .expect("quiet is not an error"),
            0
        );
    }
    assert!(data.is_empty());
}

#[test]
fn what_is_written_reaches_the_child() {
    let mut conn = PtyConn::open(&sh("read line; printf '[%s]' \"$line\"")).expect("open");
    let n = conn
        .write(b"ping\n", Duration::from_secs(1))
        .expect("write");
    assert_eq!(n, 5);
    let (data, _) = drain(&mut conn, |d| d.ends_with(b"[ping]"));
    // The pty echoes what was typed, exactly as a real terminal line
    // discipline does, so the reply is preceded by the input.
    assert!(text(&data).contains("[ping]"), "got {:?}", text(&data));
}

#[test]
fn the_child_starts_at_the_size_it_was_given() {
    let params = PtyParams {
        cols: 100,
        rows: 40,
        ..sh("stty size")
    };
    let mut conn = PtyConn::open(&params).expect("open");
    let (data, _) = drain(&mut conn, |d| d.contains(&b'\n'));
    assert_eq!(text(&data).trim(), "40 100");
}

/// A resize has to reach the kernel's `winsize`, not just our own idea of the
/// window — otherwise every full-screen program keeps drawing at the old size.
#[test]
fn a_resize_reaches_the_child() {
    let mut conn = PtyConn::open(&sh("sleep 0.3; stty size")).expect("open");
    conn.resize(120, 50).expect("resize");
    let (data, _) = drain(&mut conn, |d| d.contains(&b'\n'));
    assert_eq!(text(&data).trim(), "50 120");
}

/// `TERM` is set by us and never inherited. Launched from another terminal we
/// would otherwise hand the child *that* terminal's name; launched from a
/// desktop menu we would hand it nothing at all.
#[test]
fn term_is_ours_rather_than_the_parents() {
    let params = PtyParams {
        term: "vt220-sterna-test".into(),
        ..sh("printf '[%s]' \"$TERM\"")
    };
    let mut conn = PtyConn::open(&params).expect("open");
    let (data, _) = drain(&mut conn, |d| d.ends_with(b"]"));
    assert_eq!(text(&data), "[vt220-sterna-test]");
}

#[test]
fn extra_environment_is_passed_through() {
    let params = PtyParams {
        env: vec![("TT_PTY_TEST".into(), "yes".into())],
        ..sh("printf '[%s]' \"$TT_PTY_TEST\"")
    };
    let mut conn = PtyConn::open(&params).expect("open");
    let (data, _) = drain(&mut conn, |d| d.ends_with(b"]"));
    assert_eq!(text(&data), "[yes]");
}

#[test]
fn the_working_directory_is_honoured() {
    let params = PtyParams {
        cwd: Some("/".into()),
        ..sh("pwd")
    };
    let mut conn = PtyConn::open(&params).expect("open");
    let (data, _) = drain(&mut conn, |d| d.contains(&b'\n'));
    assert_eq!(text(&data).trim(), "/");
}

/// A burst bigger than one read, ending in a disconnect. Exercises the loop
/// the frontend actually runs: many reads, then `EIO` once the last byte is
/// out.
#[test]
fn a_burst_arrives_whole_and_then_ends() {
    let mut conn = PtyConn::open(&sh("seq 1 5000")).expect("open");
    let (data, err) = drain(&mut conn, |_| false);
    assert!(matches!(err, Some(Error::Disconnected)));
    let s = text(&data);
    assert!(s.starts_with("1\r\n"), "got {:?}", &s[..s.len().min(20)]);
    assert!(s.trim_end().ends_with("5000"));
    // `\r\n` because the slave's line discipline has ONLCR on, which is how a
    // real terminal gets its carriage returns.
    assert_eq!(s.matches("\r\n").count(), 5000);
}

#[test]
fn a_program_that_does_not_exist_fails_at_open() {
    let params = PtyParams {
        argv: vec!["sterna-no-such-program".into()],
        ..PtyParams::default()
    };
    assert!(PtyConn::open(&params).is_err());
}

/// The descriptor is the whole reason the frontend has no timer in its event
/// loop, so it has to actually become readable.
#[test]
fn the_descriptor_becomes_readable_when_the_child_writes() {
    let mut conn = PtyConn::open(&sh("sleep 0.2; printf x")).expect("open");
    let fd = conn.poll_fd().expect("a pty has a descriptor");

    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut pfd, 1, 5000) };
    assert_eq!(ready, 1, "the descriptor never became readable");

    let (data, _) = drain(&mut conn, |d| !d.is_empty());
    assert_eq!(text(&data), "x");
}

/// A pty has no line to break, and saying so is the point: the frontend draws
/// the menu from `supports_break`, and offering an item that can only produce
/// an error is worse than not offering it.
#[test]
fn there_is_no_break_on_a_pty() {
    let mut conn = PtyConn::open(&sh("sleep 5")).expect("open");
    assert!(!conn.supports_break());
    assert!(matches!(
        conn.send_break(Duration::from_millis(1)),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn the_connection_names_itself_for_the_status_line() {
    let conn = PtyConn::open(&sh("sleep 5")).expect("open");
    assert_eq!(conn.describe(), "sh -c sleep 5");
    assert!(conn.pid().is_some());
    let tty = conn.tty_name().expect("a slave path").to_owned();
    assert!(
        tty.starts_with("/dev/pts") || tty.starts_with("/dev/tty"),
        "unexpected tty name {tty:?}"
    );
}

/// Dropping the connection must not leave the child running. Closing the
/// master hangs the line up, and anything that ignores `SIGHUP` is killed.
#[test]
fn dropping_the_connection_takes_the_child_with_it() {
    let mut conn = PtyConn::open(&sh("trap '' HUP; sleep 30")).expect("open");
    let pid = conn.pid().expect("a pid") as libc::pid_t;
    // Wait until the shell is actually running before pulling the rug out.
    let _ = conn.write(b"\n", Duration::from_millis(100));
    std::thread::sleep(Duration::from_millis(100));
    drop(conn);

    // Reaped by us, so the pid is gone rather than a zombie. `kill(pid, 0)`
    // succeeds for a zombie, which is exactly the leak this asserts against.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_eq!(alive, -1, "the child outlived the connection");
}

/// A home directory with a `.profile` in it, so the login-shell tests turn on
/// the observable *consequence* of a login shell rather than on typing into an
/// interactive prompt and racing its startup.
struct FakeHome(std::path::PathBuf);

impl FakeHome {
    fn new(tag: &str) -> FakeHome {
        let dir = std::env::temp_dir().join(format!("tt-pty-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the fake home");
        std::fs::write(dir.join(".profile"), "printf '[login]'\n").expect("write .profile");
        FakeHome(dir)
    }

    /// `SHELL` is pinned to `/bin/sh` so the test asserts on POSIX startup
    /// behaviour rather than on whatever the developer's login shell reads.
    fn env(&self) -> Vec<(String, String)> {
        vec![
            ("HOME".into(), self.0.display().to_string()),
            ("SHELL".into(), "/bin/sh".into()),
        ]
    }
}

impl Drop for FakeHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The default is a login shell, which is upstream's default too
/// (`cygterm.cfg`'s `LOGIN_SHELL = Yes`) and is what makes `~/.profile` run.
#[test]
fn the_default_is_a_login_shell() {
    let home = FakeHome::new("login");
    let mut conn = PtyConn::open(&PtyParams {
        env: home.env(),
        ..PtyParams::default()
    })
    .expect("open");
    let (data, _) = drain(&mut conn, |d| {
        String::from_utf8_lossy(d).contains("[login]")
    });
    assert!(
        text(&data).contains("[login]"),
        "the profile did not run: {:?}",
        text(&data)
    );
}

/// And the flag turns it off, which is the half that would otherwise go
/// untested — a `login_shell` that silently did nothing would pass the test
/// above.
#[test]
fn login_shell_off_skips_the_profile() {
    let home = FakeHome::new("nologin");
    let mut conn = PtyConn::open(&PtyParams {
        env: home.env(),
        login_shell: false,
        ..PtyParams::default()
    })
    .expect("open");
    let (data, _) = drain_for(&mut conn, Duration::from_secs(1), |d| {
        String::from_utf8_lossy(d).contains("[login]")
    });
    assert!(
        !text(&data).contains("[login]"),
        "the profile ran for a non-login shell: {:?}",
        text(&data)
    );
}
