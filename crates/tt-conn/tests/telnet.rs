//! The telnet transport, against a real `telnetd`.
//!
//! ```sh
//! cd telnet-audit && ./servers.sh start     # :2323 telnetd, :2324 raw echo
//! TT_TELNET_HOST=127.0.0.1 TT_TELNET_PORT=2323 TT_TELNET_RAW_PORT=2324 \
//!   cargo test -p tt-conn --test telnet
//! cd telnet-audit && ./servers.sh stop
//! ```
//!
//! Without those the tests **skip loudly**, the same rule the serial rig and
//! the SSH suite follow.
//!
//! **The counterparty is GNU inetutils' `telnetd`, not something written
//! here.** That distinction is the whole value: `protocol.rs`'s unit tests are
//! byte strings derived from Tera Term's C, so they prove the port matches
//! upstream and nothing about whether upstream matches the world. A real
//! server is what closes that — and it turns out to open with
//! `WILL AUTHENTICATION`, `WILL ENCRYPT`, `DO XDISPLOC` and `DO NEW-ENVIRON`,
//! every one of them above upstream's `MaxTelOpt`, so the first thing a
//! session does is exercise the refusal path.

use std::time::{Duration, Instant};

use tt_conn::telnet::{TelnetConn, TelnetMode, TelnetParams};
use tt_conn::{Transport, TransportEvent};

fn host() -> Option<String> {
    std::env::var("TT_TELNET_HOST").ok()
}

fn port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(default)
}

macro_rules! host_or_skip {
    () => {
        match host() {
            Some(h) => h,
            None => {
                eprintln!("SKIPPED: set TT_TELNET_HOST to run this (see the module docs)");
                return;
            }
        }
    };
}

fn connect(host: &str, port: u16, mode: TelnetMode) -> TelnetConn {
    TelnetConn::connect(
        host,
        port,
        &TelnetParams {
            mode,
            term_type: "vt100".into(),
            cols: 80,
            rows: 24,
            ..TelnetParams::default()
        },
        Duration::from_secs(5),
    )
    .expect("connect")
}

/// Read until the far end has been quiet for a moment, returning everything.
///
/// Every test needs this before it types: `telnetd` runs a login program on a
/// pty and the negotiation, the pty's own setup and the first output all
/// arrive first.
fn settle(conn: &mut TelnetConn) -> (Vec<u8>, Vec<TransportEvent>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut quiet = Instant::now();
    let (mut data, mut events) = (Vec::new(), Vec::new());
    while Instant::now() < deadline && quiet.elapsed() < Duration::from_millis(400) {
        let before = data.len();
        match conn.read(&mut data, &mut events) {
            Ok(_) if data.len() > before => quiet = Instant::now(),
            Ok(_) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("read while settling: {e}"),
        }
    }
    (data, events)
}

fn read_until(conn: &mut TelnetConn, needle: &str, how_long: Duration) -> String {
    let deadline = Instant::now() + how_long;
    let (mut data, mut events) = (Vec::new(), Vec::new());
    while Instant::now() < deadline {
        match conn.read(&mut data, &mut events) {
            Ok(_) => {
                if String::from_utf8_lossy(&data).contains(needle) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) if e.is_disconnected() => break,
            Err(e) => panic!("read: {e}"),
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

#[test]
fn a_real_telnetd_negotiates_and_then_carries_bytes() {
    let host = host_or_skip!();
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);

    let (data, _) = settle(&mut conn);
    // The framing worked if no IAC reached the terminal. One that did would be
    // painted as U+00FF and look like a font problem.
    assert!(
        !data.contains(&0xFF),
        "an IAC reached the data stream: {data:?}"
    );

    // `-E /bin/cat` puts a cat on the pty, so a line comes back.
    conn.write(b"telnet-marker\r\n", Duration::from_secs(1))
        .expect("write");
    let out = read_until(&mut conn, "telnet-marker", Duration::from_secs(5));
    assert!(out.contains("telnet-marker"), "got {out:?}");
}

#[test]
fn options_above_upstreams_table_are_refused_without_stalling() {
    let host = host_or_skip!();
    // telnetd opens with WILL AUTHENTICATION (37), WILL ENCRYPT (38),
    // DO XDISPLOC (35) and DO NEW-ENVIRON (39/36) — all above MaxTelOpt.
    // Upstream declines every one, and the risk is not the decline but a
    // decline that loops: a server re-offering what it was refused, or a
    // refusal that gets answered again.
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);
    settle(&mut conn);
    // Still usable after all that is the assertion.
    conn.write(b"after-refusals\r\n", Duration::from_secs(1))
        .expect("write");
    let out = read_until(&mut conn, "after-refusals", Duration::from_secs(5));
    assert!(out.contains("after-refusals"), "got {out:?}");
}

#[test]
fn a_resize_reaches_the_far_end_without_breaking_the_stream() {
    let host = host_or_skip!();
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);
    settle(&mut conn);

    // telnetd asks for NAWS, so this goes out as a subnegotiation. A wrong
    // one desynchronises the far end and the next line never comes back —
    // which is exactly what the assertion after it detects.
    conn.resize(132, 43).expect("resize");
    conn.write(b"after-resize\r\n", Duration::from_secs(1))
        .expect("write");
    let out = read_until(&mut conn, "after-resize", Duration::from_secs(5));
    assert!(out.contains("after-resize"), "got {out:?}");
}

#[test]
fn a_break_is_sent_and_the_session_survives_it() {
    let host = host_or_skip!();
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);
    settle(&mut conn);
    // The one thing telnet does that SSH cannot: a console server turns this
    // into a real line break on the serial port behind it.
    assert!(conn.supports_break());
    conn.send_break(Duration::from_millis(250)).expect("break");
    conn.write(b"after-break\r\n", Duration::from_secs(1))
        .expect("write");
    let out = read_until(&mut conn, "after-break", Duration::from_secs(5));
    assert!(out.contains("after-break"), "got {out:?}");
}

#[test]
fn raw_mode_delivers_the_negotiation_as_data() {
    let host = host_or_skip!();
    // The mode a console server's per-line port needs. Pointed at a *telnet*
    // server on purpose: if Raw quietly processed IAC, this is where it would
    // show, because the bytes are unmistakable.
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Raw);
    let (data, _) = settle(&mut conn);
    assert!(
        data.contains(&0xFF),
        "Raw ate the negotiation instead of delivering it: {data:?}"
    );
}

#[test]
fn a_raw_port_round_trips_every_byte() {
    let host = host_or_skip!();
    let raw_port = port("TT_TELNET_RAW_PORT", 2324);
    // A plain echo on a socket — no telnet, no pty, no line discipline. The
    // closest thing here to a console server streaming binary, and the case
    // where eating an 0xFF corrupts a firmware upload.
    let mut conn = connect(&host, raw_port, TelnetMode::Raw);
    let payload: Vec<u8> = (0u8..=255).collect();
    conn.write(&payload, Duration::from_secs(1)).expect("write");

    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut data, mut events) = (Vec::new(), Vec::new());
    while Instant::now() < deadline && data.len() < payload.len() {
        conn.read(&mut data, &mut events).expect("read");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(data, payload, "raw mode changed a byte");
}

#[test]
fn auto_mode_stays_raw_on_a_port_that_never_speaks_telnet() {
    let host = host_or_skip!();
    // Upstream's TelAutoDetect. The failure it prevents is subtle: a port that
    // happens to carry an 0xFF *in data* would flip a naive detector into
    // telnet for the rest of the session and eat every subsequent one.
    // Nothing here can prevent that — upstream cannot either — but a port that
    // never sends 0xFF must stay raw.
    let mut conn = connect(&host, port("TT_TELNET_RAW_PORT", 2324), TelnetMode::Auto);
    conn.write(b"plain bytes\r\n", Duration::from_secs(1))
        .expect("write");
    let out = read_until(&mut conn, "plain bytes", Duration::from_secs(5));
    // The CR is not followed by a NUL, because telnet never turned on: an
    // escape on a raw port is two bytes of corruption per line.
    assert!(out.contains("plain bytes\r\n"), "got {out:?}");
    assert!(!out.contains('\0'), "escaped a CR on a raw port: {out:?}");
}

#[test]
fn the_far_end_hanging_up_reads_as_a_disconnect() {
    let host = host_or_skip!();
    let mut conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);
    settle(&mut conn);
    // `cat` exits on EOF, which ends the login program and closes the session.
    conn.write(&[0x04], Duration::from_secs(1)).expect("write");

    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut data, mut events) = (Vec::new(), Vec::new());
    loop {
        assert!(Instant::now() < deadline, "never reported a disconnect");
        match conn.read(&mut data, &mut events) {
            Ok(_) => {
                data.clear();
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                assert!(e.is_disconnected(), "wrong error: {e}");
                break;
            }
        }
    }
}

#[test]
fn a_refused_port_is_an_error_rather_than_a_hang() {
    let host = host_or_skip!();
    // Port 1 is tcpmux and nothing listens on it.
    match TelnetConn::connect(&host, 1, &TelnetParams::default(), Duration::from_secs(2)) {
        Ok(_) => panic!("connected to a closed port"),
        Err(e) => assert!(!e.to_string().is_empty()),
    }
}

#[test]
fn describe_leaves_out_the_default_port() {
    let host = host_or_skip!();
    let conn = connect(&host, port("TT_TELNET_PORT", 2323), TelnetMode::Negotiate);
    // Not 23, so the port is shown.
    assert!(conn.describe().contains(':'), "{}", conn.describe());
}
