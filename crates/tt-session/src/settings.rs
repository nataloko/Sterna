//! `TERATERM.INI` on one side, a running terminal on the other.
//!
//! `tt-config` deliberately does not depend on `tt-vt`: the schema is a
//! description of a *file*, and one that knew about the parser would be two
//! things at once. So the map between them lives here, in the one crate that
//! already holds both — and it is a map, not a copy. Three groups come out of
//! [`Settings`] and only the first reaches the core:
//!
//! 1. **The terminal's**, which is everything below: sizes, the terminal ID,
//!    the CR modes, the colour flags, the window and mouse gates.
//! 2. **The window's** — the five colour pairs, the cursor shape as something
//!    to *draw*, the word delimiters a double-click uses, the title. The
//!    frontend reads those by name through the metadata table rather than
//!    through a second struct that would have to be kept in step.
//! 3. **Nobody's yet** — settings that are read and written faithfully and act
//!    on nothing, because the subsystem they belong to is a later stage.
//!
//! The third group is the reason this file names every field it uses rather
//! than deriving the mapping: a setting that silently does nothing should be
//! visible as an absence here.

use tt_config::{CursorShape, KeyboardBackspace, Settings, TerminalCrReceive, TerminalCrSend};
use tt_vt::{ColorFlags, Config, CrReceive, CrSend, TermId};

/// Build the terminal's configuration from the settings.
///
/// `base` supplies everything the schema has no key for — the cell size the
/// frontend measured, `decrqcra` for the conformance harness, the ISO-2022
/// shift flags — so that applying settings to a running terminal does not
/// quietly reset what nobody asked about. Pass `&Config::default()` at
/// startup and `vt.config()` afterwards.
pub fn vt_config(s: &Settings, base: &Config) -> Config {
    Config {
        cols: s.terminal_cols.max(1) as usize,
        rows: s.terminal_rows.max(1) as usize,
        scrollback_max: scrollback_max(s),
        term_id: term_id(s),
        cr_receive: match s.terminal_cr_receive {
            TerminalCrReceive::Cr => CrReceive::Cr,
            TerminalCrReceive::CrLf => CrReceive::CrLf,
            TerminalCrReceive::Lf => CrReceive::Lf,
            TerminalCrReceive::Auto => CrReceive::Auto,
        },
        cr_send: match s.terminal_cr_send {
            TerminalCrSend::Cr => CrSend::Cr,
            TerminalCrSend::CrLf => CrSend::CrLf,
            TerminalCrSend::Lf => CrSend::Lf,
        },
        local_echo: s.terminal_local_echo,
        bs_key_is_bs: s.keyboard_backspace == KeyboardBackspace::Bs,
        color_flags: ColorFlags {
            xterm256: s.color_xterm_256,
            aixterm16: s.color_aixterm_16,
            pc_bold16: s.color_pc_bold_16,
            // `EnableANSIColor` has no schema row yet, so it keeps whatever it
            // had rather than being invented here.
            ansi_color: base.color_flags.ansi_color,
        },
        // DECSCUSR's numbering, which is what `Config::cursor_shape` holds and
        // what DECRQSS answers with: `vtterm.c:4270` maps IdBlkCur to 1,
        // IdHCur to 3 and IdVCur to **5**, so the enum's own order is not it.
        cursor_shape: match s.cursor_shape {
            CursorShape::Block => 1,
            CursorShape::Horizontal => 3,
            CursorShape::Vertical => 5,
        },
        nonblinking_cursor: s.cursor_nonblinking,
        window_change: s.window_change_allowed,
        window_report: s.window_report_allowed,
        cursor_ctrl_sequence: s.window_cursor_ctrl_allowed,
        accept_8bit_ctrl: s.window_accept_8bit_ctrl,
        send_8bit_ctrl: s.window_send_8bit_ctrl,
        alt_screen_enabled: s.window_alt_screen,
        remote_clears_buffer: s.window_remote_clears_buffer,
        mouse_tracking_enabled: s.mouse_tracking,
        disable_mouse_tracking_by_ctrl: s.mouse_ctrl_disables_tracking,
        translate_wheel_to_cursor: s.mouse_wheel_to_cursor,
        ..*base
    }
}

/// `TerminalID`, which the schema spells and `tt-vt` also spells.
///
/// Two lists of the same fourteen names is one more than there should be, and
/// they cannot be shared: the schema's exists so the *file* can be parsed
/// without depending on the terminal. So they are joined by the INI's own
/// spelling — the string upstream itself compares — and
/// `every_terminal_id_is_a_name_both_crates_know` fails if either list grows a
/// name the other does not have.
fn term_id(s: &Settings) -> TermId {
    TermId::parse(s.terminal_id.as_ini()).unwrap_or_default()
}

/// How deep the history goes, which is **not** what `ScrollBuffSize` says.
///
/// Upstream's is the whole buffer, page included, and it is clamped up to the
/// page rather than the page being clamped down to it (`buffer.c:641`, and
/// again at `:4983`). With `EnableScrollBuff` off the buffer *is* the page.
/// `Grid` counts the lines beyond the page, so the page comes back off.
fn scrollback_max(s: &Settings) -> usize {
    if !s.terminal_scrollback_enabled {
        return 0;
    }
    let rows = s.terminal_rows.max(1) as usize;
    let lines = s.terminal_scrollback_lines.max(0) as usize;
    lines.saturating_sub(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_config::{Ini, TerminalId};

    #[test]
    fn the_defaults_map_onto_the_terminals_defaults() {
        let mapped = vt_config(&Settings::default(), &Config::default());
        let d = Config::default();
        // Everything except the history, which upstream ships at 100 lines
        // *including* the page while `Config::default` carries `MaxBuffSize`.
        assert_eq!(mapped.scrollback_max, 100 - 24);
        assert_eq!(
            Config {
                scrollback_max: d.scrollback_max,
                ..mapped
            },
            d,
            "a setting whose default disagrees with the terminal's is one of \
             them being wrong about upstream"
        );
    }

    #[test]
    fn every_terminal_id_is_a_name_both_crates_know() {
        for name in ["VT100", "VT102J", "VT220", "VT320", "VT525", "dumb"] {
            let id = TerminalId::from_ini(name);
            assert_eq!(id.as_ini(), name, "the schema lost {name}");
            assert!(TermId::parse(name).is_some(), "tt-vt lost {name}");
        }
        let s = Settings {
            terminal_id: TerminalId::Vt320,
            ..Settings::default()
        };
        assert_eq!(term_id(&s), TermId::Vt320);
    }

    #[test]
    fn the_history_is_the_buffer_minus_the_page() {
        let mut s = Settings {
            terminal_rows: 24,
            terminal_scrollback_lines: 1000,
            ..Settings::default()
        };
        assert_eq!(scrollback_max(&s), 976);

        // Upstream grows the buffer to hold the page rather than shrinking the
        // page to the buffer, so this is a terminal with no history, not one
        // with negative history or 10 rows.
        s.terminal_scrollback_lines = 10;
        assert_eq!(scrollback_max(&s), 0);

        s.terminal_scrollback_lines = 1000;
        s.terminal_scrollback_enabled = false;
        assert_eq!(scrollback_max(&s), 0);
    }

    #[test]
    fn a_file_reaches_the_terminal() {
        let ini = Ini::parse(
            b"[Tera Term]\r\n\
              TerminalSize=132,50\r\n\
              TerminalID=VT320\r\n\
              CRReceive=CRLF\r\n\
              BSKey=DEL\r\n\
              Xterm256Color=off\r\n\
              CursorShape=vertical\r\n",
        );
        let c = vt_config(&Settings::load(&ini), &Config::default());
        assert_eq!((c.cols, c.rows), (132, 50));
        assert_eq!(c.term_id, TermId::Vt320);
        assert_eq!(c.cr_receive, CrReceive::CrLf);
        assert!(!c.bs_key_is_bs);
        assert!(!c.color_flags.xterm256);
        assert_eq!(c.cursor_shape, 5, "DECSCUSR's number, not IdVCur's 2");
    }
}
