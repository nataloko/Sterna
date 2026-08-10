#![cfg(windows)]

//! ConPTY's real pipes and event, against real Windows child processes.
//!
//! These are separate from `pty.rs` because `/bin/sh`, signals and termios are
//! the POSIX half's specification. Here `cmd.exe` is the counterparty and the
//! contract is the one the Windows frontend needs: output wakes an event,
//! queued input reaches the child, final output precedes disconnect, and the
//! child's status survives long enough to explain the close.
//!
//! These require native Windows. Wine 9's console host rejects the internal
//! `--inheritcursor` option selected by `portable-pty` and closes the output
//! pipe empty before `cmd.exe` runs; the worker/event unit test remains
//! runnable there without pretending that Wine result says anything about
//! ConPTY.

use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_conn::{Error, Transport};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

fn command(args: &[&str]) -> PtyParams {
    PtyParams {
        argv: std::iter::once("cmd.exe".to_string())
            .chain(args.iter().map(|s| (*s).to_string()))
            .collect(),
        login_shell: false,
        ..PtyParams::default()
    }
}

fn prime_cursor(conn: &mut PtyConn) -> Vec<u8> {
    let handle = conn.wait_handle().expect("ConPTY has a waitable event");
    // SAFETY: `conn` owns the borrowed event throughout this wait.
    assert_eq!(unsafe { WaitForSingleObject(handle, 5000) }, WAIT_OBJECT_0);
    let mut data = Vec::new();
    conn.read(&mut data, &mut Vec::new())
        .expect("read ConPTY cursor request");
    if data.windows(4).any(|w| w == b"\x1b[6n") {
        let reply = b"\x1b[1;1R";
        assert_eq!(
            conn.write(reply, Duration::from_millis(10))
                .expect("answer cursor request"),
            reply.len()
        );
    }
    data
}

fn drain(conn: &mut PtyConn, mut data: Vec<u8>) -> (Vec<u8>, Error) {
    let handle = conn.wait_handle().expect("ConPTY has a waitable event");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    loop {
        // SAFETY: `conn` owns the borrowed event throughout this wait.
        let ready = unsafe { WaitForSingleObject(handle, 1000) };
        assert!(
            ready == WAIT_OBJECT_0 || ready == WAIT_TIMEOUT,
            "waiting for ConPTY failed with {ready}"
        );
        match conn.read(&mut data, &mut events) {
            Ok(_) => {}
            Err(e) => return (data, e),
        }
        assert!(Instant::now() < deadline, "ConPTY never hung up");
    }
}

#[test]
fn output_wakes_and_precedes_the_exit_status() {
    let mut conn = PtyConn::open(&command(&["/d", "/c", "echo conpty-read-ok & exit /b 7"]))
        .expect("open ConPTY");

    let initial = prime_cursor(&mut conn);
    let (data, end) = drain(&mut conn, initial);
    assert!(matches!(end, Error::Disconnected), "ended with {end:?}");
    assert!(
        String::from_utf8_lossy(&data).contains("conpty-read-ok"),
        "got {:?}",
        String::from_utf8_lossy(&data)
    );
    assert_eq!(conn.exit_status().map(|s| s.code), Some(7));
}

#[test]
fn queued_input_reaches_an_interactive_child() {
    let mut conn = PtyConn::open(&command(&["/d", "/q"])).expect("open ConPTY");
    let initial = prime_cursor(&mut conn);
    let input = b"echo conpty-write-ok\r\nexit /b 0\r\n";
    assert_eq!(
        conn.write(input, Duration::from_millis(10))
            .expect("queue input"),
        input.len()
    );

    let (data, end) = drain(&mut conn, initial);
    assert!(matches!(end, Error::Disconnected), "ended with {end:?}");
    assert!(
        String::from_utf8_lossy(&data).contains("conpty-write-ok"),
        "got {:?}",
        String::from_utf8_lossy(&data)
    );
    assert_eq!(conn.exit_status().map(|s| s.code), Some(0));
}
