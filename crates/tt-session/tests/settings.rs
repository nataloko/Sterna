//! Settings applied to a terminal that is already running.
//!
//! The unit tests beside `settings.rs` check the map from a file to a
//! `Config`. These check what happens when that `Config` is handed to a
//! session with output on the screen, a viewport scrolled back and a transport
//! attached — which is the state a settings dialog is always opened from.

use tt_config::{Ini, KeyboardBackspace, Settings};
use tt_session::{MemoryHandle, MemoryTransport, Session};
use tt_vt::TermId;

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
fn applying_a_size_resizes_the_grid_and_tells_the_far_end() {
    let (mut s, handle) = session();
    let mut settings = s.settings().clone();
    settings.terminal_cols = 100;
    settings.terminal_rows = 40;
    s.set_settings(settings).expect("apply");
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

    assert!(s.set_setting("keyboard.backspace", "DEL").expect("apply"));
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
    s.set_settings(settings).expect("apply");
    assert!(!s.vt().backspace_sends_bs());
}

#[test]
fn a_name_that_is_not_ours_is_refused_rather_than_ignored() {
    let (mut s, _) = session();
    assert!(!s.set_setting("terminal.nonesuch", "1").expect("apply"));
    assert_eq!(s.setting("terminal.nonesuch"), None);
    // A value out of range is not a refusal: the file's own rule applies, so
    // zero columns is 80 rather than an error nobody could have got from
    // editing the INI by hand.
    assert!(s.set_setting("terminal.cols", "0").expect("apply"));
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
    assert!(s
        .set_setting("keyboard.word_delimiters", "$20,;")
        .expect("apply"));
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
    s.set_settings(settings).expect("apply");
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
    s.set_settings(settings).expect("apply");
    assert_eq!(s.view_offset(), 0);
}
