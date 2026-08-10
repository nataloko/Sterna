//! Win32 serial wakeups, on a real loopback pair.
//!
//! Set `TT_SERIAL_A=COM<n>` and `TT_SERIAL_B=COM<n>` to the two ends. The
//! test skips loudly without them because a named pipe is not a COM driver and
//! cannot prove `WaitCommEvent` or `ClearCommError` behavior.

#![cfg(windows)]

use std::time::Duration;

use tt_conn::serial::{SerialConn, SerialParams};
use tt_conn::Transport;
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

fn ports() -> Option<(String, String)> {
    let a = std::env::var("TT_SERIAL_A").ok();
    let b = std::env::var("TT_SERIAL_B").ok();
    match (a, b) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => {
            eprintln!("SKIPPED: set TT_SERIAL_A and TT_SERIAL_B to a Windows loopback pair");
            None
        }
    }
}

#[test]
fn received_bytes_wake_once_and_keep_ff_literal() {
    let Some((a, b)) = ports() else {
        return;
    };
    let params = SerialParams {
        read_timeout: Duration::from_millis(50),
        ..SerialParams::default()
    };
    let mut near = SerialConn::open(&a, &params).expect("open receiving port");
    let mut far = SerialConn::open(&b, &params).expect("open sending port");
    let handle = near.wait_handle().expect("serial wait event");

    // SAFETY: `near` owns the event for every wait in this test.
    assert_eq!(unsafe { WaitForSingleObject(handle, 0) }, WAIT_TIMEOUT);
    assert_eq!(
        far.write(&[0xff, b'X'], Duration::from_secs(1))
            .expect("write"),
        2
    );
    // SAFETY: as above.
    assert_eq!(unsafe { WaitForSingleObject(handle, 5000) }, WAIT_OBJECT_0);

    let (mut data, mut events) = (Vec::new(), Vec::new());
    assert_eq!(near.read(&mut data, &mut events).expect("read"), 2);
    assert_eq!(data, [0xff, b'X']);
    assert!(events.is_empty());

    // The manual event resets as the notice is consumed; an idle line does
    // not turn into a timer after one successful read.
    // SAFETY: as above.
    assert_eq!(unsafe { WaitForSingleObject(handle, 0) }, WAIT_TIMEOUT);
}
