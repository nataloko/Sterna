//! Hardware tests, against two ports wired back-to-back on data *and* control
//! lines (TX↔RX, DTR↔DSR, RTS↔CTS).
//!
//! ```sh
//! TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 cargo test -p tt-conn
//! ```
//!
//! Without those variables every test here **skips**, loudly, so a machine
//! with no rig still gets a green `cargo test` without quietly pretending the
//! serial layer was exercised. The dev container has the rig; CI does not.
//!
//! Without the control-line wiring the modem-line and flow-control tests are
//! meaningless rather than failing, so check the loom before believing a red
//! result.

use std::time::Duration;

use tt_conn::serial::{
    enumerate, DataBits, FlowControl, Parity, PinControl, SerialConn, SerialEvent, SerialParams,
    StopBits,
};

/// The pair, or `None` with a printed reason.
fn rig() -> Option<(String, String)> {
    match (std::env::var("TT_SERIAL_A"), std::env::var("TT_SERIAL_B")) {
        (Ok(a), Ok(b)) => Some((a, b)),
        _ => {
            eprintln!("SKIP: set TT_SERIAL_A and TT_SERIAL_B to a back-to-back pair");
            None
        }
    }
}

fn open_pair(params: &SerialParams) -> Option<(SerialConn, SerialConn)> {
    let (a, b) = rig()?;
    let mut a = SerialConn::open(&a, params).expect("open A");
    let mut b = SerialConn::open(&b, params).expect("open B");
    settle(&mut a, &mut b);
    Some((a, b))
}

/// Let anything a previous test left in flight arrive, then throw it away.
///
/// Closing a port does not stop bytes already handed to the adapter, and they
/// turn up in the *next* test's first read. Clearing without the wait clears
/// an empty buffer and the stale bytes land immediately afterwards, which
/// looks exactly like a bug in whatever is being measured.
fn settle(a: &mut SerialConn, b: &mut SerialConn) {
    std::thread::sleep(Duration::from_millis(150));
    a.clear(true, true).ok();
    b.clear(true, true).ok();
}

/// Read until `want` bytes have arrived or the deadline passes.
fn read_for(
    port: &mut SerialConn,
    want: usize,
    dur: Duration,
) -> (Vec<u8>, Vec<SerialEvent>) {
    let (mut data, mut events) = (Vec::new(), Vec::new());
    let deadline = std::time::Instant::now() + dur;
    while std::time::Instant::now() < deadline && data.len() < want {
        port.read(&mut data, &mut events).expect("read");
    }
    (data, events)
}

#[test]
fn bytes_cross_the_wire_in_both_directions() {
    let Some((mut a, mut b)) = open_pair(&SerialParams::default()) else {
        return;
    };
    a.write(b"from A", Duration::from_secs(1)).expect("write A");
    a.flush(Duration::from_millis(500)).ok();
    let (data, events) = read_for(&mut b, 6, Duration::from_secs(2));
    assert_eq!(data, b"from A");
    assert!(events.is_empty(), "unexpected events: {events:?}");

    b.write(b"from B", Duration::from_secs(1)).expect("write B");
    b.flush(Duration::from_millis(500)).ok();
    let (data, _) = read_for(&mut a, 6, Duration::from_secs(2));
    assert_eq!(data, b"from B");
}

#[test]
fn a_break_arrives_as_an_event_and_a_nul_as_data() {
    // The reason `detect_break` exists. Without PARMRK both of these are the
    // single byte 0x00 and nothing downstream can tell them apart.
    let Some((mut a, mut b)) = open_pair(&SerialParams::default()) else {
        return;
    };
    b.clear(true, false).ok();

    a.write(b"X", Duration::from_secs(1)).unwrap();
    a.flush(Duration::from_millis(500)).ok();
    std::thread::sleep(Duration::from_millis(120));
    a.send_break(Duration::from_millis(250)).expect("break");
    std::thread::sleep(Duration::from_millis(120));
    a.write(&[0x00, b'Y'], Duration::from_secs(1)).unwrap();
    a.flush(Duration::from_millis(500)).ok();

    let (data, events) = read_for(&mut b, 3, Duration::from_secs(2));
    assert_eq!(data, b"X\x00Y", "the NUL must survive as data");
    assert!(
        events.contains(&SerialEvent::Break),
        "expected a Break event, got {events:?}"
    );
}

#[test]
fn a_raw_ff_byte_is_not_doubled() {
    // PARMRK escapes 0xFF as `FF FF` on the wire; if the decoder did not undo
    // that, every file transfer would corrupt.
    let Some((mut a, mut b)) = open_pair(&SerialParams::default()) else {
        return;
    };
    b.clear(true, false).ok();
    let payload: Vec<u8> = (0u16..=255).map(|v| v as u8).collect();
    a.write(&payload, Duration::from_secs(2)).unwrap();
    a.flush(Duration::from_millis(500)).ok();
    let (data, events) = read_for(&mut b, payload.len(), Duration::from_secs(3));
    assert_eq!(data, payload, "every byte value must round-trip");
    assert!(events.is_empty(), "unexpected events: {events:?}");
}

#[test]
fn modem_lines_cross_over_the_loom() {
    let Some((mut a, mut b)) = open_pair(&SerialParams::default()) else {
        return;
    };
    // DTR on A is wired to DSR on B, RTS to CTS.
    for state in [false, true, false] {
        a.set_dtr(state).unwrap();
        a.set_rts(state).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let lines = b.modem_lines().unwrap();
        assert_eq!(lines.dsr, state, "DTR->DSR at {state}");
        assert_eq!(lines.cts, state, "RTS->CTS at {state}");
    }
}

#[test]
fn every_baud_rate_reads_back_exactly() {
    // Including 250000, which is not in the standard table — the FTDI driver
    // takes it, and a good deal of embedded kit uses it.
    let Some((a, _)) = rig() else { return };
    for baud in [300u32, 9600, 115200, 250000, 921600, 3000000] {
        let params = SerialParams {
            baud,
            ..SerialParams::default()
        };
        let conn = SerialConn::open(&a, &params).expect("open");
        assert_eq!(conn.params().baud, baud);
        drop(conn);
    }
}

#[test]
fn mark_and_space_parity_apply() {
    // The gap that needs CMSPAR. `serialport-rs` has no enum for these, and
    // nothing else on Linux offers them through a GUI.
    let Some((a, _)) = rig() else { return };
    for parity in [Parity::Mark, Parity::Space, Parity::Even, Parity::None] {
        let params = SerialParams {
            parity,
            ..SerialParams::default()
        };
        let mut conn = SerialConn::open(&a, &params).expect("open");
        // Re-applying must not fail, and must leave the setting where it was:
        // this is the path spike 4 found could be clobbered.
        conn.apply(&params).expect("re-apply");
        assert_eq!(conn.params().parity, parity);
    }
}

#[test]
fn the_line_settings_commlib_sets_all_apply() {
    // 7 and 8 bits with 1 and 2 stop bits is exactly what commlib.c's DCB
    // path offers, so these must work on any adapter worth supporting.
    let Some((a, _)) = rig() else { return };
    for data_bits in [DataBits::Seven, DataBits::Eight] {
        for stop_bits in [StopBits::One, StopBits::Two] {
            let params = SerialParams {
                data_bits,
                stop_bits,
                ..SerialParams::default()
            };
            SerialConn::open(&a, &params).expect("open with line settings");
        }
    }
}

#[test]
fn an_unsupported_character_size_fails_rather_than_lying() {
    // The FTDI refuses CS6 with EINVAL and *accepts CS5 then ignores it*. The
    // second is the dangerous one: without the read-back in
    // linux::set_data_bits the dialog would say 5 bits while the wire carried
    // 8. Either outcome is acceptable here; silently succeeding is not.
    let Some((a, _)) = rig() else { return };
    for data_bits in [DataBits::Five, DataBits::Six] {
        let params = SerialParams {
            data_bits,
            ..SerialParams::default()
        };
        if let Ok(conn) = SerialConn::open(&a, &params) {
            // If it opened, the driver must really have applied it. Proving
            // that needs the wire, so the check that matters lives in
            // set_data_bits; here we only assert the settings round-trip.
            assert_eq!(conn.params().data_bits, data_bits);
        }
    }
}

#[test]
fn seven_data_bits_reach_the_wire() {
    // Read-back says the driver applied it; only the wire says it did. Send
    // 0x25 at seven bits and read at eight: the stop bit lands in bit 7, so a
    // port that really is at seven bits reports 0xA5.
    //
    // Do NOT pick a probe byte with bit 7 set. 0xA5 sent at seven bits also
    // reads back as 0xA5, and the test passes whatever the port is doing.
    let Some((a, b)) = rig() else { return };
    let mut tx = SerialConn::open(
        &a,
        &SerialParams {
            data_bits: DataBits::Seven,
            ..SerialParams::default()
        },
    )
    .unwrap();
    let mut rx = SerialConn::open(
        &b,
        &SerialParams {
            detect_break: false,
            ..SerialParams::default()
        },
    )
    .unwrap();
    rx.clear(true, false).ok();
    tx.write(&[0x25], Duration::from_secs(1)).unwrap();
    tx.flush(Duration::from_millis(300)).ok();
    let (data, _) = read_for(&mut rx, 1, Duration::from_secs(1));
    assert_eq!(data, vec![0xA5], "seven data bits did not reach the wire");
}

#[test]
fn rts_cts_flow_control_actually_gates() {
    let Some((mut a, mut b)) = open_pair(&SerialParams {
        baud: 9600,
        flow: FlowControl::RtsCts,
        ..SerialParams::default()
    }) else {
        return;
    };
    b.clear(true, false).ok();

    // B drops RTS, so A's CTS goes low and the driver stops sending.
    b.set_rts(false).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(!a.modem_lines().unwrap().cts, "CTS should have gone low");

    let payload = vec![b'z'; 512];
    a.write(&payload, Duration::from_millis(300)).ok();
    a.flush(Duration::from_millis(500)).ok();
    std::thread::sleep(Duration::from_millis(200));
    let (held, _) = read_for(&mut b, payload.len(), Duration::from_millis(300));
    assert!(
        held.len() < payload.len(),
        "flow control did not hold anything back: got {} of {}",
        held.len(),
        payload.len()
    );

    b.set_rts(true).unwrap();
    let (rest, _) = read_for(&mut b, payload.len() - held.len(), Duration::from_secs(3));
    assert_eq!(
        held.len() + rest.len(),
        payload.len(),
        "everything held back should arrive once CTS returns"
    );
}

#[test]
fn dsr_flow_control_gates_in_userspace() {
    // Linux has no DSR flow-control bit, so this is the userspace shim: the
    // write must return short rather than block, and must resume once DSR
    // comes back.
    let Some((mut a, mut b)) = open_pair(&SerialParams {
        flow: FlowControl::DsrDtr,
        ..SerialParams::default()
    }) else {
        return;
    };
    b.set_dtr(false).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(!a.modem_lines().unwrap().dsr);

    let started = std::time::Instant::now();
    let sent = a
        .write(b"blocked", Duration::from_millis(200))
        .expect("write should time out, not error");
    assert_eq!(sent, 0, "nothing may go out while DSR is low");
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "the write must give up, not block a UI thread"
    );

    b.set_dtr(true).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    let sent = a.write(b"open", Duration::from_millis(500)).unwrap();
    assert_eq!(sent, 4, "and resume once DSR returns");
}

#[test]
fn lock_uses_whatever_the_flow_control_implies() {
    // CommLock: XOFF byte with no hardware flow control, RTS with RTS/CTS.
    let Some((mut a, mut b)) = open_pair(&SerialParams::default()) else {
        return;
    };
    b.clear(true, false).ok();
    a.lock(true).unwrap();
    let (data, _) = read_for(&mut b, 1, Duration::from_millis(500));
    assert_eq!(data, vec![0x13], "no flow control means send the XOFF byte");

    // Reopening while the first handle is alive would fail: TTYPort takes
    // TIOCEXCL, so the port is exclusive to one process *and* one open.
    let (a_path, _) = rig().unwrap();
    drop(a);
    let mut a = SerialConn::open(
        &a_path,
        &SerialParams {
            flow: FlowControl::RtsCts,
            ..SerialParams::default()
        },
    )
    .unwrap();
    a.lock(true).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(!b.modem_lines().unwrap().cts, "RTS/CTS means drop RTS");
    a.lock(false).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(b.modem_lines().unwrap().cts);
}

#[test]
fn the_pin_control_settings_are_honoured_on_open() {
    let Some((a_path, b_path)) = rig() else { return };
    let mut b = SerialConn::open(&b_path, &SerialParams::default()).unwrap();

    for (dtr, rts, want) in [
        (PinControl::Disable, PinControl::Disable, false),
        (PinControl::Enable, PinControl::Enable, true),
    ] {
        let _a = SerialConn::open(
            &a_path,
            &SerialParams {
                dtr,
                rts,
                ..SerialParams::default()
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let lines = b.modem_lines().unwrap();
        assert_eq!(lines.dsr, want, "DTR {dtr:?}");
        assert_eq!(lines.cts, want, "RTS {rts:?}");
    }
}

#[test]
fn a_port_can_be_opened_by_its_stable_path() {
    // The identity that survives a replug. If by-path resolution were broken
    // this would open nothing rather than the wrong thing, which is the
    // failure mode worth having.
    let Some((a, _)) = rig() else { return };
    let real = std::fs::canonicalize(&a).unwrap();
    let ports = enumerate().unwrap();
    let Some(info) = ports
        .iter()
        .find(|p| std::fs::canonicalize(&p.device).ok() == Some(real.clone()))
    else {
        panic!("enumerate() did not report {a}");
    };
    assert!(
        info.stable_id.is_some(),
        "a USB port should have a by-path identity: {info:?}"
    );
    SerialConn::open(info.open_path(), &SerialParams::default())
        .expect("opening by the stable path must work");
}

#[test]
fn a_port_held_by_something_else_reads_as_busy_not_gone() {
    // serialport-rs reports a busy port as ErrorKind::NoDevice with no errno,
    // so the naive mapping tells the user their adapter was unplugged and
    // sends them to check the cable. This is the regression test for the
    // discriminator in Error::from_open.
    let Some((a, _)) = rig() else { return };
    let _held = SerialConn::open(&a, &SerialParams::default()).expect("first open");
    match SerialConn::open(&a, &SerialParams::default()) {
        Ok(_) => panic!("expected the second open to be refused"),
        Err(e) => {
            assert!(
                matches!(e, tt_conn::Error::Busy { .. }),
                "expected Busy, got {e:?} ({e})"
            );
            assert!(!e.is_disconnected(), "a busy port has not disconnected");
        }
    }
}
