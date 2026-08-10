//! Win32 serial wakeups, on a real loopback pair.
//!
//! Set `TT_SERIAL_A=COM<n>` and `TT_SERIAL_B=COM<n>` to the two ends. The
//! test skips loudly without them because a named pipe is not a COM driver and
//! cannot prove `WaitCommEvent` or `ClearCommError` behavior. The cases
//! serialize themselves because both own the same physical pair.

#![cfg(windows)]

use std::sync::Mutex;
use std::time::Duration;

use tt_conn::serial::{SerialConn, SerialParams};
use tt_conn::Transport;
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

static RIG: Mutex<()> = Mutex::new(());

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
    let _rig = RIG.lock().unwrap();
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

#[test]
fn the_driver_reads_back_the_extended_dcb() {
    let Some((a, _)) = ports() else {
        return;
    };
    let _rig = RIG.lock().unwrap();
    let params = SerialParams {
        baud: 115_200,
        data_bits: tt_conn::serial::DataBits::Seven,
        parity: tt_conn::serial::Parity::Mark,
        stop_bits: tt_conn::serial::StopBits::Two,
        flow: tt_conn::serial::FlowControl::DsrDtr,
        xon: 0x12,
        xoff: 0x14,
        dtr: tt_conn::serial::PinControl::Handshake,
        rts: tt_conn::serial::PinControl::Enable,
        ..SerialParams::default()
    };

    // `open` reads the DCB back and fails if the driver silently kept any old
    // value. Reaching this assertion proves the settings, not merely the
    // success return from SetCommState.
    let mut conn = SerialConn::open(&a, &params).expect("open with extended DCB");
    assert_eq!(conn.params(), &params);

    let software = SerialParams {
        parity: tt_conn::serial::Parity::Space,
        flow: tt_conn::serial::FlowControl::XonXoff,
        xon: 0x12,
        xoff: 0x14,
        dtr: tt_conn::serial::PinControl::Enable,
        rts: tt_conn::serial::PinControl::Handshake,
        ..params
    };
    conn.apply(&software)
        .expect("reapply with software flow DCB");
    assert_eq!(conn.params(), &software);
}
