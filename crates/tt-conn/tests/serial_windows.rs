//! Win32 serial wakeups, on a real loopback pair.
//!
//! Set `TT_SERIAL_A=COM<n>` and `TT_SERIAL_B=COM<n>` to the two ends. The
//! test skips loudly without them because a named pipe is not a COM driver and
//! cannot prove `WaitCommEvent` or `ClearCommError` behavior. The cases
//! serialize themselves because both own the same physical pair.

#![cfg(windows)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tt_conn::serial::{FlowControl, SerialConn, SerialParams};
use tt_conn::Transport;
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

static RIG: Mutex<()> = Mutex::new(());

#[test]
fn a_missing_com_port_is_a_disconnect() {
    assert!(matches!(
        SerialConn::open("COM65535", &SerialParams::default()),
        Err(tt_conn::Error::Disconnected)
    ));
}

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

#[test]
fn an_exclusively_held_com_port_is_busy() {
    let Some((a, _)) = ports() else {
        return;
    };
    let _rig = RIG.lock().unwrap();
    let _held = SerialConn::open(&a, &SerialParams::default()).expect("first open");
    assert!(matches!(
        SerialConn::open(&a, &SerialParams::default()),
        Err(tt_conn::Error::Busy { .. })
    ));
}

#[test]
fn writes_and_flushes_stop_at_their_own_deadlines() {
    let Some((a, b)) = ports() else {
        return;
    };
    let _rig = RIG.lock().unwrap();
    let params = SerialParams {
        baud: 9600,
        flow: FlowControl::RtsCts,
        // Deliberately much longer than the write below: this catches a write
        // which accidentally inherits the port's read timeout.
        read_timeout: Duration::from_secs(5),
        ..SerialParams::default()
    };
    let mut near = SerialConn::open(&a, &params).expect("open transmitting port");
    let mut far = SerialConn::open(&b, &params).expect("open backpressure port");
    std::thread::sleep(Duration::from_millis(150));
    near.clear(true, true).ok();
    far.clear(true, true).ok();

    // RTS on the far port is wired to CTS on the transmitter. With it low,
    // the driver may accept some bytes into its queue but cannot drain them.
    far.set_rts(false).expect("lower far RTS");
    std::thread::sleep(Duration::from_millis(60));
    assert!(!near.modem_lines().unwrap().cts, "CTS should be low");

    let payload = vec![b'z'; 1024 * 1024];
    let write_started = Instant::now();
    let written = near
        .write(&payload, Duration::from_millis(40))
        .expect("bounded write");
    let write_elapsed = write_started.elapsed();

    let flush_started = Instant::now();
    let drained = near
        .flush(Duration::from_millis(40))
        .expect("bounded flush");
    let flush_elapsed = flush_started.elapsed();

    // Restore the rig before asserting, so even a timing failure cannot leave
    // the following hardware test behind a lowered control line or old data.
    near.clear(false, true).ok();
    far.set_rts(true).ok();
    std::thread::sleep(Duration::from_millis(60));

    assert!(
        write_elapsed < Duration::from_secs(2),
        "40 ms write waited {write_elapsed:?} (read timeout is five seconds)"
    );
    assert!(
        flush_elapsed < Duration::from_secs(2),
        "40 ms flush waited {flush_elapsed:?}"
    );
    assert!(
        written == 0 || !drained,
        "driver accepted {written} bytes and reported an empty queue while CTS was low"
    );
}
