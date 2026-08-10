//! The whole stack, end to end: bytes arrive, the grid changes, keys go out.
//!
//! These run against `MemoryTransport`, so they are fast and need no hardware.
//! `tt-conn`'s loopback tests still cover the wire; what is covered here is
//! the *composition*, which nothing else exercised.

use std::time::Duration;

use tt_conn::LinkKind;
use tt_session::{Event, MemoryHandle, MemoryTransport, Session};
use tt_vt::{Config, Key, Modifiers, MouseEvent};

const TICK: Duration = Duration::from_millis(20);

/// A session with a memory transport already attached, and the handle onto
/// it. The session owns the transport, so this is how a test — or a frontend
/// wanting a serial port's settings — keeps a way back to it.
fn connected(cols: usize, rows: usize) -> (Session, MemoryHandle) {
    let mut s = Session::new(Config {
        cols,
        rows,
        ..Config::default()
    });
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    (s, handle)
}

/// The grid row as text, trailing blanks trimmed.
fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        if cell.width_class == tt_grid::WIDTH_PAD {
            continue;
        }
        match cell.codepoints().next() {
            Some(0) | None => out.push(' '),
            Some(cp) => out.push(char::from_u32(cp).unwrap_or(' ')),
        }
    }
    out.trim_end().to_string()
}

fn pump(s: &mut Session) -> Vec<Event> {
    s.pump(TICK).expect("pump");
    s.drain_events()
}

#[test]
fn bytes_from_the_transport_reach_the_grid() {
    let (mut s, h) = connected(20, 4);
    h.feed(b"hello\r\n\x1b[3;3Hworld");

    let events = pump(&mut s);
    assert!(events.contains(&Event::Damage));
    assert_eq!(row(&s, 0), "hello");
    assert_eq!(row(&s, 2), "  world");
}

#[test]
fn a_quiet_line_costs_nothing_and_says_nothing() {
    // The normal state of a serial console. It must not spin to the deadline
    // and must not manufacture damage, or the frontend repaints forever.
    let (mut s, _h) = connected(20, 4);

    let started = std::time::Instant::now();
    let events = pump(&mut s);
    assert!(events.is_empty(), "quiet line produced {events:?}");
    assert!(
        started.elapsed() < TICK,
        "pump burned its whole budget on an idle line"
    );
}

#[test]
fn the_live_serial_speed_comes_from_the_transport() {
    let mut s = Session::new(Config::default());
    assert_eq!(s.serial_baud(), None);

    let (transport, _handle) = MemoryTransport::with_kind(LinkKind::Serial {
        baud: 115_200,
        seven_bit: false,
    });
    s.connect(Box::new(transport));
    assert_eq!(s.serial_baud(), Some(115_200));
}

#[test]
fn keys_are_encoded_by_the_core_and_follow_the_modes() {
    let (mut s, h) = connected(20, 4);

    assert!(s.send_key(Key::Up).unwrap());
    // DECCKM arrives from the host, and the very next key changes shape.
    s.feed(b"\x1b[?1h");
    assert!(s.send_key(Key::Up).unwrap());
    // A local command puts nothing on the wire, and says so.
    assert!(!s.send_key(Key::Break).unwrap());

    assert_eq!(h.outbound(), b"\x1b[A\x1bOA");
}

#[test]
fn a_reply_the_parser_owes_goes_out_on_the_same_pump() {
    // A host that sent DSR is usually blocked waiting for the answer, so
    // holding it until the next pump stalls the session by however long the
    // frontend's timer is.
    let (mut s, h) = connected(20, 4);
    h.feed(b"\x1b[6n");

    pump(&mut s);
    assert_eq!(h.outbound(), b"\x1b[1;1R");
}

#[test]
fn paste_is_bracketed_only_when_the_host_asked() {
    let (mut s, h) = connected(20, 4);

    // ...and the LF has become a CR on the way, which is `NormalizeLineBreakCR`
    // and is what the Return key would have sent.
    s.paste("plain\n").unwrap();
    assert_eq!(h.outbound(), b"plain\r");

    s.feed(b"\x1b[?2004h");
    s.paste("x").unwrap();
    assert_eq!(h.outbound(), b"plain\r\x1b[200~x\x1b[201~");
}

/// `NormalizeLineBreakCR` (`ttlib_static_cpp.cpp:535`): a terminal sends what
/// the keyboard sends, and the Return key is a CR whatever the clipboard holds.
#[test]
fn a_paste_puts_one_cr_on_the_wire_for_every_line_break() {
    let (mut s, h) = connected(20, 4);
    s.paste("a\r\nb\nc\rd").unwrap();
    assert_eq!(h.outbound(), b"a\rb\rc\rd");
}

/// The two settings that gate the brackets on top of `DECSET 2004`, and the
/// one that decides what a trailing newline is worth.
#[test]
fn the_clipboard_settings_decide_what_a_paste_looks_like() {
    let with = |edit: fn(&mut tt_session::Settings), text: &str| {
        let (mut s, h) = connected(20, 4);
        let mut settings = s.settings().clone();
        edit(&mut settings);
        s.set_settings(settings).unwrap();
        s.feed(b"\x1b[?2004h");
        s.paste(text).unwrap();
        h.outbound()
    };

    // `BracketedSupport=off` refuses the mode the host asked for.
    assert_eq!(
        with(|s| s.clipboard_bracketed = false, "x\ny"),
        b"x\ry".to_vec()
    );
    // `BracketedControlOnly` brackets a block and not a word.
    assert_eq!(
        with(|s| s.clipboard_bracketed_control_only = true, "word"),
        b"word".to_vec()
    );
    assert_eq!(
        with(|s| s.clipboard_bracketed_control_only = true, "two\nlines"),
        b"\x1b[200~two\rlines\x1b[201~".to_vec()
    );
    // `TrimTrailingNLonPaste` cuts every trailing break, not one.
    assert_eq!(
        with(|s| s.clipboard_trim_trailing_newline = true, "cmd\r\n\r\n"),
        b"\x1b[200~cmd\x1b[201~".to_vec()
    );
    // ...and off, which is how it ships, the newline goes and the shell runs
    // the line.
    assert_eq!(with(|_| {}, "cmd\r\n"), b"\x1b[200~cmd\r\x1b[201~".to_vec());
}

#[test]
fn a_title_change_is_reported_once() {
    let (mut s, h) = connected(20, 4);
    h.feed(b"\x1b]0;first\x07");

    let events = pump(&mut s);
    assert!(events.contains(&Event::Title("first".into())), "{events:?}");

    // The same title again must not produce a second event, or a status bar
    // redraws on every shell prompt.
    s.feed(b"\x1b]0;first\x07");
    let events = s.drain_events();
    assert!(
        !events.iter().any(|e| matches!(e, Event::Title(_))),
        "{events:?}"
    );
}

#[test]
fn a_break_from_the_far_end_becomes_an_event_not_a_nul() {
    let (mut s, h) = connected(20, 4);
    h.feed(b"ab");
    h.with(|st| st.events.push(tt_conn::TransportEvent::Break));

    let events = pump(&mut s);
    assert!(events.contains(&Event::Break), "{events:?}");
    assert_eq!(row(&s, 0), "ab");
}

#[test]
fn send_break_reaches_the_transport_and_holds_for_what_the_file_says() {
    let (mut s, h) = connected(20, 4);
    s.send_break().unwrap();
    assert_eq!(h.with(|st| st.breaks), 1);
    // `SendBreakTime`'s default, and the point of the setting being the only
    // thing allowed to say: this used to be 300 ms in the window and 250 in
    // the macro host, neither of them upstream's.
    assert_eq!(
        h.with(|st| st.last_break),
        Some(Duration::from_millis(1000))
    );

    let mut settings = s.settings().clone();
    settings.serial_break_time = 5;
    s.set_settings(settings).unwrap();
    s.send_break().unwrap();
    assert_eq!(h.with(|st| st.last_break), Some(Duration::from_millis(5)));

    // And with nothing attached it is a no-op rather than an error: the user
    // pressing the break key at a dead session should not raise a dialog.
    s.disconnect();
    s.send_break().unwrap();
}

#[test]
fn a_network_disconnect_requests_auto_close_and_leaves_the_screen_alone() {
    let (mut s, h) = connected(20, 4);
    h.feed(b"keep me");
    pump(&mut s);
    assert_eq!(row(&s, 0), "keep me");

    // Now the adapter is unplugged.
    h.with(|st| st.disconnected = true);
    let events = pump(&mut s);
    assert!(events.contains(&Event::Disconnected), "{events:?}");
    assert!(events.contains(&Event::CloseRequested), "{events:?}");
    assert!(!s.is_connected());
    // ClearScreenOnCloseConnection ships off. The close request belongs to a
    // window and a headless caller may decline it, so the terminal state still
    // has to be the exact state it would see if the window stayed open.
    assert_eq!(row(&s, 0), "keep me");

    // And a second pump does not report it again.
    let events = pump(&mut s);
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn a_window_that_stays_open_can_clear_on_disconnect() {
    let (mut s, h) = connected(20, 4);
    let mut settings = s.settings().clone();
    settings.terminal_cols = 20;
    settings.terminal_rows = 4;
    settings.connection_auto_win_close = false;
    settings.connection_clear_screen_on_close = true;
    s.set_settings(settings).unwrap();
    s.drain_events();

    h.feed(b"keep me");
    pump(&mut s);
    assert_eq!(row(&s, 0), "keep me");

    h.with(|st| st.disconnected = true);
    let events = pump(&mut s);
    assert!(events.contains(&Event::Disconnected), "{events:?}");
    assert!(events.contains(&Event::Damage), "{events:?}");
    assert!(!events.contains(&Event::CloseRequested), "{events:?}");
    assert_eq!(row(&s, 0), "");
    assert_eq!((s.grid().cursor.x, s.grid().cursor.y), (0, 0));

    // Clear screen is BuffClearScreen, not an erase. The old page moved into
    // history and its first character remains available there.
    assert_eq!(s.scrollback_len(), 4);
    let old = s.line(0).expect("old page is in scrollback");
    assert_eq!(old[0].codepoints().next(), Some(u32::from(b'k')));
}

#[test]
fn auto_close_is_network_only() {
    let mut s = Session::new(Config {
        cols: 20,
        rows: 4,
        ..Config::default()
    });
    let mut settings = s.settings().clone();
    settings.terminal_cols = 20;
    settings.terminal_rows = 4;
    settings.connection_clear_screen_on_close = true;
    s.set_settings(settings).unwrap();
    s.drain_events();

    let (transport, h) = MemoryTransport::with_kind(LinkKind::LocalPty);
    s.connect(Box::new(transport));
    h.feed(b"local");
    pump(&mut s);
    h.with(|st| st.disconnected = true);

    let events = pump(&mut s);
    assert!(events.contains(&Event::Disconnected), "{events:?}");
    assert!(events.contains(&Event::Damage), "{events:?}");
    assert!(!events.contains(&Event::CloseRequested), "{events:?}");
    assert_eq!(row(&s, 0), "");
}

#[test]
fn choosing_disconnect_also_applies_the_close_outcome() {
    let (mut s, _h) = connected(20, 4);
    s.drain_events();

    s.disconnect();
    let events = s.drain_events();
    assert!(events.contains(&Event::CloseRequested), "{events:?}");
    // The caller initiated this one and already knows the connection changed;
    // only a transport disappearing reports the generic disconnect notice.
    assert!(!events.contains(&Event::Disconnected), "{events:?}");
}

#[test]
fn a_disconnect_while_writing_takes_the_same_outcome_branch() {
    let (mut s, h) = connected(20, 4);
    s.drain_events();
    h.with(|st| st.disconnected = true);

    s.send_text("x").unwrap();
    let events = s.drain_events();
    assert!(events.contains(&Event::Disconnected), "{events:?}");
    assert!(events.contains(&Event::CloseRequested), "{events:?}");
    assert!(!s.is_connected());
}

#[test]
fn typing_into_a_dead_session_does_not_grow_without_bound() {
    let mut s = Session::new(Config::default());
    for _ in 0..1000 {
        s.send_text("some text that goes nowhere").unwrap();
    }
    // No transport, nothing queued, no panic. Buffering it forever is a leak
    // that only shows up after a cable is pulled.
    assert!(!s.is_connected());
}

#[test]
fn resize_moves_the_grid_and_tells_the_far_end() {
    // Two halves that are easy to separate by accident. A grid that resized
    // without the ioctl leaves `vi` drawing to the old size, and the symptom
    // looks like a redraw bug.
    let (mut s, h) = connected(20, 4);
    s.resize(40, 10).unwrap();

    assert_eq!(s.grid().cols(), 40);
    assert_eq!(s.grid().rows(), 10);
    assert_eq!(h.with(|st| st.last_resize), Some((40, 10)));
}

#[test]
fn connecting_announces_the_current_size() {
    // A pty started at the default 80x24 while the window is 120x30 emits one
    // screenful of wrongly-wrapped output before anything resizes it.
    let (_s, h) = connected(120, 30);
    assert_eq!(h.with(|st| st.last_resize), Some((120, 30)));
}

#[test]
fn mouse_reports_reach_the_wire_and_report_consumption() {
    let (mut s, h) = connected(20, 6);

    // Nothing is tracking yet, so the click belongs to the frontend.
    assert!(!s
        .mouse(MouseEvent::Press, 0, 24, 80, Modifiers::default())
        .unwrap());
    assert!(h.outbound().is_empty());

    s.feed(b"\x1b[?1000h\x1b[?1006h");
    assert!(s
        .mouse(MouseEvent::Press, 0, 24, 80, Modifiers::default())
        .unwrap());
    assert_eq!(h.outbound(), b"\x1b[<0;4;6M");
}

#[test]
fn focus_reports_are_silent_until_asked_for() {
    let (mut s, h) = connected(20, 4);
    s.focus(true).unwrap();
    assert!(h.outbound().is_empty());

    s.feed(b"\x1b[?1004h");
    s.focus(true).unwrap();
    s.focus(false).unwrap();
    assert_eq!(h.outbound(), b"\x1b[I\x1b[O");
}

#[test]
fn a_short_write_keeps_the_rest_for_the_next_pump() {
    // Flow control is entitled to hold the line, and the bytes it held back
    // must not be lost — a dropped keystroke on a console session is the kind
    // of bug people abandon a tool over.
    let (mut s, h) = connected(20, 4);
    h.with(|st| st.write_chunk = 1);

    s.send_text("abcd").unwrap();
    assert_eq!(h.outbound(), b"a", "one byte per write, as set");
    // And the frontend can see that something is stuck. A window waiting only
    // on `poll_fd` gets no wakeup here — the far end is holding the line, not
    // talking — so this is what tells it to keep pumping.
    assert_eq!(s.pending_out(), 3);
    for _ in 0..8 {
        s.pump(TICK).unwrap();
    }
    assert_eq!(h.outbound(), b"abcd", "the rest must arrive, in order");
    assert_eq!(s.pending_out(), 0, "nothing left, so the retry timer stops");
}

#[test]
fn a_typed_cr_is_expanded_by_newline_mode() {
    // The main Return key is not a `Key` — upstream handles VK_RETURN outside
    // the key table, marking it `IdText` so `OutControl` converts it. So the
    // frontend sends "\r" and this is where LNM has to be applied; a shell
    // that had to check the mode itself would be holding a piece of the keymap
    // the core is supposed to own.
    let (mut s, h) = connected(20, 4);

    s.send_text("a\r").unwrap();
    assert_eq!(h.outbound(), b"a\r", "LNM off: a CR stays a CR");

    h.with(|st| st.outbound.clear());
    s.feed(b"\x1b[20h"); // SM 20 — LNM
    s.send_text("b\r").unwrap();
    assert_eq!(h.outbound(), b"b\r\n");

    // And a paste is left alone, because bracketed paste means verbatim.
    h.with(|st| st.outbound.clear());
    s.paste("x\ry").unwrap();
    assert_eq!(h.outbound(), b"x\ry");
}

#[test]
fn a_write_that_fails_hard_is_an_error_not_a_disconnect() {
    let (mut s, h) = connected(20, 4);
    h.with(|st| st.write_error = Some("write"));

    let err = s.send_text("x").unwrap_err();
    assert!(!err.is_disconnected(), "{err}");
    // Still attached: a failed write is not proof the far end left.
    assert!(s.is_connected());
}

#[test]
fn output_and_input_interleave_over_repeated_pumps() {
    // The shape of a real session: the host writes, we answer, it writes
    // again. Anything that resets state per pump breaks here and nowhere
    // else.
    let (mut s, h) = connected(20, 6);

    h.feed(b"login: ");
    pump(&mut s);
    s.send_text("root\r").unwrap();

    h.feed(b"root\r\npassword: ");
    pump(&mut s);
    s.send_text("hunter2\r").unwrap();

    h.feed(b"\r\n$ ");
    pump(&mut s);

    assert_eq!(h.outbound(), b"root\rhunter2\r");
    assert_eq!(row(&s, 0), "login: root");
    assert_eq!(row(&s, 1), "password:");
    assert_eq!(row(&s, 2), "$");
}

#[test]
fn the_control_lines_are_declined_by_everything_that_is_not_a_serial_port() {
    // Upstream's guard is `!cv.Open || cv.PortType != IdSerial`, and both
    // halves land here as `Transport::as_serial` answering `None`. What the
    // guard rejects is not an error — the terminal answers
    // `DDE_FNOTPROCESSED` and a macro reads that as success — so the only
    // thing to assert is that nothing happened, including to the settings.
    let (mut s, _h) = connected(20, 4);
    let before = s.settings().clone();

    assert!(!s.set_dtr(false));
    assert!(!s.set_rts(false));
    assert!(!s.set_baud(19200));
    assert!(!s.set_flow_control(tt_conn::serial::FlowControl::RtsCts));
    assert!(s.modem_lines().is_none());

    // The guard comes *before* the assignment upstream, so a `setbaud` over
    // SSH must not leave the serial settings changed for the next connection.
    assert_eq!(s.settings().serial_baud, before.serial_baud);
    assert_eq!(s.settings().serial_flow, before.serial_flow);

    // ...and with nothing connected at all, which is the other half.
    let mut s = Session::new(Config::default());
    assert!(!s.set_dtr(true));
    assert!(s.modem_lines().is_none());
}
