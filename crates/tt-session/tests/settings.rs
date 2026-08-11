//! Settings applied to a terminal that is already running.
//!
//! The unit tests beside `settings.rs` check the map from a file to a
//! `Config`. These check what happens when that `Config` is handed to a
//! session with output on the screen, a viewport scrolled back and a transport
//! attached — which is the state a settings dialog is always opened from.

use std::time::Duration;

use tt_config::{Ini, KeyboardBackspace, Settings};
use tt_conn::{Result, Transport, TransportEvent};
use tt_session::{MemoryHandle, MemoryTransport, Session};
use tt_vt::{CrSend, DebugMode, DebugModes, TermId};

fn session() -> (Session, MemoryHandle) {
    let mut s = Session::from_settings(Settings::default());
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    (s, handle)
}

fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        match cell.codepoints().next() {
            Some(0) | None => out.push(' '),
            Some(cp) => out.push(char::from_u32(cp).unwrap_or(' ')),
        }
    }
    out.trim_end().to_string()
}

#[test]
fn a_session_starts_from_the_file() {
    let ini = Ini::parse(b"[Tera Term]\r\nTerminalSize=100,40\r\nTerminalID=VT320\r\n");
    let s = Session::from_settings(Settings::load(&ini));
    assert_eq!((s.grid().cols(), s.grid().rows()), (100, 40));
    assert_eq!(s.vt().config().term_id, TermId::Vt320);
    // ...and the settings the core does not read came along anyway, because
    // the frontend and the file both need them back.
    assert_eq!(
        s.setting("color.normal").as_deref(),
        Some("0,0,0,255,255,255")
    );
}

#[test]
fn debug_modes_are_applied_and_can_be_selected_directly() {
    let ini = Ini::parse(b"[Tera Term]\r\nDebug=on\r\nDebugModes=hex,noout\r\n");
    let mut s = Session::from_settings(Settings::load(&ini));
    assert!(s.vt().config().debug_enabled);
    assert_eq!(
        s.vt().config().debug_modes.bits(),
        DebugModes::HEX | DebugModes::NO_OUTPUT
    );
    assert!(s.cycle_debug_mode());
    assert_eq!(s.vt().debug_mode(), DebugMode::Hex);

    // TTL does not consult `Debug=` or the cycle mask.
    s.set_debug_mode(DebugMode::Normal);
    s.feed(&[0x01]);
    assert_eq!(row(&s, 0), "^A");
}

#[test]
fn applying_a_size_resizes_the_grid_and_tells_the_far_end() {
    let (mut s, handle) = session();
    let mut settings = s.settings().clone();
    settings.terminal_cols = 100;
    settings.terminal_rows = 40;
    s.set_settings(settings);
    assert_eq!((s.grid().cols(), s.grid().rows()), (100, 40));
    assert_eq!(handle.with(|m| m.last_resize), Some((100, 40)));
}

/// The one a user notices first: `keyboard.backspace` is BS out of the box
/// because that is what Tera Term sends, and changing it has to reach the
/// window without a reconnect. Backspace is not a `Key` — upstream handles it
/// in `KeyDown` rather than in the table — so what the frontend reads is
/// DECBKM's state, and that is what has to move.
#[test]
fn applying_the_backspace_key_changes_what_the_key_sends() {
    let (mut s, _) = session();
    assert!(s.vt().backspace_sends_bs());

    assert!(s.set_setting("keyboard.backspace", "DEL"));
    assert!(!s.vt().backspace_sends_bs());
    assert_eq!(
        s.settings().keyboard_backspace,
        KeyboardBackspace::Del,
        "and the stored value moved with it"
    );

    // The host can still change its mind with DECBKM, and applying the
    // settings again takes it back — upstream's `ts.BSKey` is the one variable
    // both of them write.
    s.feed(b"\x1b[?67h");
    assert!(s.vt().backspace_sends_bs());
    let settings = s.settings().clone();
    s.set_settings(settings);
    assert!(!s.vt().backspace_sends_bs());
}

#[test]
fn a_name_that_is_not_ours_is_refused_rather_than_ignored() {
    let (mut s, _) = session();
    assert!(!s.set_setting("terminal.nonesuch", "1"));
    assert_eq!(s.setting("terminal.nonesuch"), None);
    // A value out of range is not a refusal: the file's own rule applies, so
    // zero columns is 80 rather than an error nobody could have got from
    // editing the INI by hand.
    assert!(s.set_setting("terminal.cols", "0"));
    assert_eq!(s.grid().cols(), 80);
}

/// Settings that belong to a stage nobody has written yet still have to
/// survive a round trip, or the first save silently rewrites the user's file.
#[test]
fn a_setting_the_core_ignores_is_still_kept() {
    let (mut s, _) = session();
    // `$20`, not a space: `GetPrivateProfileString` strips the whitespace
    // around a value, which is exactly why upstream's own default spells the
    // space that way (`ttset.c`'s `DelimList`).
    assert!(s.set_setting("keyboard.word_delimiters", "$20,;"));
    assert_eq!(
        s.setting("keyboard.word_delimiters").as_deref(),
        Some("$20,;")
    );

    let mut ini = Ini::parse(b"[Tera Term]\r\n");
    s.settings().store(&mut ini);
    assert_eq!(ini.get("Tera Term", "DelimList"), Some("$20,;"));
}

/// Turning the history off is a terminal with no scrollback, not a terminal
/// one row tall — the depth and the height are different settings and the
/// grid used to conflate them.
#[test]
fn turning_the_history_off_leaves_the_page_alone() {
    let (mut s, _) = session();
    s.feed(b"hello\r\n");
    let mut settings = s.settings().clone();
    settings.terminal_scrollback_enabled = false;
    s.set_settings(settings);
    assert_eq!(s.grid().rows(), 24);
    assert_eq!(s.scrollback_len(), 0);
    assert_eq!(row(&s, 0), "hello");

    // And the line numbering keeps counting, so a frontend holding a line
    // number still gets an honest answer about it.
    let before = s.top_line();
    s.feed(&b"x\r\n".repeat(30));
    assert_eq!(s.top_line(), before + 8);
    assert_eq!(s.line(before), None, "evicted, not renumbered");
}

/// A resize moves lines between the page and the history in both directions,
/// so whatever a scrolled-back view was anchored to has moved.
#[test]
fn applying_a_size_takes_a_scrolled_back_view_live() {
    let (mut s, _) = session();
    s.feed(&b"line\r\n".repeat(60));
    s.set_view_offset(10);
    assert_eq!(s.view_offset(), 10);

    let mut settings = s.settings().clone();
    settings.terminal_rows = 30;
    s.set_settings(settings);
    assert_eq!(s.view_offset(), 0);
}

/// A TCP session that is not telnet, which is the only kind `TCPLocalEcho` and
/// `TCPCRSend` apply to.
struct RawTcp(MemoryTransport);

impl Transport for RawTcp {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        self.0.read(data, events)
    }

    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        self.0.write(data, timeout)
    }

    fn tcp_without_telnet(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        "raw".into()
    }
}

/// `vtwin.cpp:3690` — the two settings a raw TCP connection spends on the
/// terminal, and gives back.
///
/// The giving back is the half worth testing: upstream keeps `ts.LocalEcho_ini`
/// and `ts.CRSend_ini` for it, and a port that assigned the live value without
/// keeping the file's would leave a terminal echoing after the socket that
/// asked for it had gone.
#[test]
fn a_raw_tcp_connection_borrows_local_echo_and_gives_it_back() {
    let ini = Ini::parse(b"[Tera Term]\r\nTCPLocalEcho=on\r\nTCPCRSend=CRLF\r\n");
    let mut s = Session::from_settings(Settings::load(&ini));
    assert!(!s.vt().local_echo(), "the file's own value, to start");
    assert_eq!(s.vt().cr_send(), CrSend::Cr);

    let (transport, _handle) = MemoryTransport::new();
    s.connect(Box::new(RawTcp(transport)));
    assert!(s.vt().local_echo());
    assert_eq!(s.vt().cr_send(), CrSend::CrLf);
    // ...and LNM did not come with it: upstream's `LFMode` is seeded at reset
    // and `TCPCRSend` is not a reset. A received LF still does not carry a CR.
    assert!(!s.vt().newline_mode());

    s.disconnect();
    assert!(!s.vt().local_echo(), "the file's value is back");
    assert_eq!(s.vt().cr_send(), CrSend::Cr);
}

/// The same connection with neither key set changes nothing — and, because
/// nothing was borrowed, gives nothing back either. A host's own `SM 12`
/// therefore survives the disconnect, which is what the two `*Used` flags
/// upstream keeps are for.
#[test]
fn without_the_keys_a_hosts_own_local_echo_survives_the_disconnect() {
    let mut s = Session::from_settings(Settings::default());
    let (transport, _handle) = MemoryTransport::new();
    s.connect(Box::new(RawTcp(transport)));
    assert!(!s.vt().local_echo());

    s.feed(b"\x1b[12h"); // SRM: the host asks the terminal to stop echoing...
    assert!(!s.vt().local_echo());
    s.feed(b"\x1b[12l"); // ...and to start.
    assert!(s.vt().local_echo());

    s.disconnect();
    assert!(
        s.vt().local_echo(),
        "nothing was borrowed, so nothing is put back"
    );
}

/// A telnet session at the telnet port is excluded — it is upstream's `else`,
/// not a separate test — and so is every non-TCP transport.
#[test]
fn the_override_does_not_reach_a_transport_that_declines_it() {
    let ini = Ini::parse(b"[Tera Term]\r\nTCPLocalEcho=on\r\n");
    let mut s = Session::from_settings(Settings::load(&ini));
    let (transport, _handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    assert!(!s.vt().local_echo());
}
