//! The schema, and the claim that a round trip changes nothing.

use tt_config::gen;
use tt_config::{
    ConnectionPortType, EncodingDecSpecialDirection, EncodingReceive, EncodingSend, FontDrawApi,
    FontQuality, Ini, KeyboardBackspace, KeyboardMeta8bit, Kind, LogTimestampType,
    ProxySocksResolve, ProxyType, SerialDataBits, SerialFlow, SerialParity, SerialStopBits,
    Settings, TerminalCrReceive, TerminalId, WindowIcon, WindowPanelLayout, FIELDS, SETTING_HELP,
};

#[test]
fn the_defaults_are_upstreams() {
    let d = Settings::default();
    // The four `AGENTS.md` calls out by name, because each one is a default
    // that is not where it looks like it is — an `else` branch or a key read
    // a thousand lines after the initialiser that zeroes it.
    assert_eq!(d.keyboard_backspace, KeyboardBackspace::Bs, "ttset.c:877");
    assert!(d.color_xterm_256, "ttset.c:741 — not the zeroed ColorFlag");
    assert!(
        !d.color_aixterm_16,
        "ttset.c:738 — and this one really is off"
    );
    assert_eq!(d.terminal_id, TerminalId::Vt100);
    assert_eq!(d.terminal_cols, 80);
    assert_eq!(d.terminal_rows, 24);
    assert_eq!(d.color_normal, [0, 0, 0, 255, 255, 255], "black on white");
}

/// The one default that is deliberately not upstream's — asserted so that
/// "9600 is what `ttset.c:919` says" cannot quietly restore it. See
/// `docs/deviations.md`; the key and its parse are unchanged, which the second
/// half of this checks.
#[test]
fn the_shipped_baud_rate_is_not_upstreams() {
    assert_eq!(
        Settings::default().serial_baud,
        115200,
        "ttset.c:919 is 9600"
    );

    let ini = Ini::parse(b"[Tera Term]\r\nBaudRate=9600\r\n");
    assert_eq!(Settings::load(&ini).serial_baud, 9600);
}

/// DETECT accepts the line ending a device actually uses, and works it out
/// from the first one rather than reading a bare CR as a line ending for ever
/// the way upstream's own AUTO does. Every explicit upstream value remains
/// compatible for an INI shared between the two programs.
#[test]
fn the_shipped_receive_cr_is_detect() {
    assert_eq!(
        Settings::default().terminal_cr_receive,
        TerminalCrReceive::Detect,
        "Tera Term's ttset.c:643 default is CR"
    );

    let ini = Ini::parse(b"[Tera Term]\r\nCRReceive=AUTO\r\n");
    assert_eq!(
        Settings::load(&ini).terminal_cr_receive,
        TerminalCrReceive::Auto,
        "upstream's own fourth value keeps its own meaning"
    );

    let ini = Ini::parse(b"[Tera Term]\r\nCRReceive=CR\r\n");
    assert_eq!(
        Settings::load(&ini).terminal_cr_receive,
        TerminalCrReceive::Cr
    );
}

/// A log names itself by the clock so a second one cannot land on the first.
/// Upstream's `teraterm.log` (`ttset.c:1018`) is one fixed name for every
/// session, and whether the second log overwrites the first or appends to it is
/// decided by `LogAppend`, which the person starting it is not looking at. The
/// key, the template language and a file that spells the old name are all
/// unchanged, which the second half checks.
#[test]
fn the_shipped_log_name_carries_the_clock() {
    assert_eq!(
        Settings::default().log_default_name,
        "sterna-%Y%m%d_%H%M%S.log",
        "ttset.c:1018 is teraterm.log"
    );

    let ini = Ini::parse(b"[Tera Term]\r\nLogDefaultName=teraterm.log\r\n");
    assert_eq!(Settings::load(&ini).log_default_name, "teraterm.log");
}

/// Where the last log went, in this program's own section — upstream asks
/// `GetTermLogDir` the same three-way question every time and remembers
/// nothing. Empty until a log has been written, so a fresh install falls
/// through to `LogDefaultPath` and the chain behind it.
#[test]
fn the_log_directory_is_remembered_in_sternas_own_section() {
    assert_eq!(Settings::default().recent_log_dir, "");

    let ini = Ini::parse(b"[Sterna]\r\nLogDir=/var/log/consoles\r\n");
    let settings = Settings::load(&ini);
    assert_eq!(settings.recent_log_dir, "/var/log/consoles");

    let mut out = Ini::parse(b"");
    settings.store(&mut out);
    assert_eq!(out.get("Sterna", "LogDir"), Some("/var/log/consoles"));
}

#[test]
fn terminal_dark_mode_is_a_sterna_setting_that_ships_off() {
    assert!(!Settings::default().terminal_dark_mode);
    let ini = Ini::parse(b"[Sterna]\r\nDarkMode=on\r\n");
    assert!(Settings::load(&ini).terminal_dark_mode);
}

#[test]
fn the_panel_layout_is_a_sterna_setting_with_a_safe_fallback() {
    assert_eq!(
        Settings::default().window_panel_layout,
        WindowPanelLayout::Single
    );

    let load = |value: &str| {
        Settings::load(&Ini::parse(
            format!("[Sterna]\r\nPanelLayout={value}\r\n").as_bytes(),
        ))
        .window_panel_layout
    };
    assert_eq!(load("tiled"), WindowPanelLayout::Tiled);
    assert_eq!(load("broken"), WindowPanelLayout::Single);
    // The 0.2.x spellings, from when this was a panel *count* shown alongside
    // the tab bar. A file written by that version still opens tiled.
    assert_eq!(load("two"), WindowPanelLayout::Tiled);
    assert_eq!(load("FOUR"), WindowPanelLayout::Tiled);

    // ...and it is rewritten in the spelling this version uses, so the aliases
    // shrink out of the file rather than being carried forward.
    let mut ini = Ini::parse(b"; formatting stays\n[Sterna]\nUnrelated = yes\nPanelLayout = two\n");
    let settings = Settings::load(&ini);
    assert!(settings.store_one(&mut ini, "window.panel_layout"));
    assert_eq!(
        ini.to_bytes(),
        b"; formatting stays\n[Sterna]\nUnrelated = yes\nPanelLayout=tiled\n"
    );
}

/// The bar's two settings. The buttons themselves are a list and are not in
/// here at all — see `tt-config/src/buttons.rs`, which owns `[Sterna Buttons]`.
#[test]
fn the_quick_button_bar_ships_on_and_down_the_right() {
    let d = Settings::default();
    assert!(d.window_quick_buttons);
    // Down the right is no longer a setting — it is where the panel is. What
    // the file carries instead is how wide it was left, and zero means nobody
    // has said: measure the buttons.
    assert_eq!(d.window_quick_buttons_width, 0);

    let load = |value: &str| {
        Settings::load(&Ini::parse(
            format!("[Sterna]\r\nQuickButtonsWidth={value}\r\n").as_bytes(),
        ))
        .window_quick_buttons_width
    };
    assert_eq!(load("240"), 240);
    // **Clamped, not defaulted.** `int_clamp` is the third of the three
    // bounds: `int(lo..hi)` would answer 0 here, which throws the width away
    // and quietly goes back to measuring instead of honouring what was meant.
    assert_eq!(load("99999"), 2000);
    // A word is 0 and so is a negative, which both land on the sentinel — the
    // one place this key cannot tell "measure it" from "that made no sense".
    // Harmless because the answer is the same and it is the shipped one.
    assert_eq!(load("wide"), 0);
    assert_eq!(load("-40"), 0);

    // Default on, so `GetOnOff` reads anything but a literal `off` as on —
    // the asymmetry `AGENTS.md` warns about, asserted rather than assumed.
    let on = |value: &str| {
        Settings::load(&Ini::parse(
            format!("[Sterna]\r\nQuickButtons={value}\r\n").as_bytes(),
        ))
        .window_quick_buttons
    };
    assert!(!on("off"));
    assert!(on("0"));
}

#[test]
fn the_highlighting_switch_is_a_sterna_setting_that_ships_on() {
    assert!(Settings::default().color_highlighting);

    let load = |value: &str| {
        Settings::load(&Ini::parse(
            format!("[Sterna]\r\nHighlighting={value}\r\n").as_bytes(),
        ))
        .color_highlighting
    };
    // `GetOnOff` with a default of on: only a literal `off` turns it off, so a
    // hand-edited `0` leaves every rule working.
    assert!(!load("off"));
    assert!(load("on"));
    assert!(load("0"));

    // The rules themselves are not here. `[Sterna Highlights]` is a list, which
    // the schema cannot describe, and `tt-config/src/highlight.rs` owns it.
    let ini = Ini::parse(b"[Sterna Highlights]\nHighlight1Pattern=x\n");
    assert!(Settings::load(&ini).color_highlighting);
}

#[test]
fn line_edit_is_an_off_by_default_sterna_setting() {
    let mut settings = Settings::default();
    assert!(!settings.terminal_line_edit);

    let ini = Ini::parse(b"[Sterna]\r\nLineEdit=on\r\n");
    settings = Settings::load(&ini);
    assert!(settings.terminal_line_edit);
    assert_eq!(settings.get_str("terminal.line_edit"), Some("on".into()));

    assert!(settings.set_str("terminal.line_edit", "off"));
    assert!(!settings.terminal_line_edit);
    assert!(!settings.set_str("terminal.line_mode", "on"));

    let mut stored = ini;
    assert!(settings.store_one(&mut stored, "terminal.line_edit"));
    assert_eq!(stored.to_bytes(), b"[Sterna]\r\nLineEdit=off\r\n");
}

#[test]
fn the_deferred_encoding_settings_keep_upstreams_parsers() {
    let d = Settings::default();
    assert_eq!(d.encoding_receive, EncodingReceive::Utf8);
    assert_eq!(d.encoding_send, EncodingSend::Utf8);
    assert!(d.encoding_ctrl_in_kanji);
    assert!(!d.encoding_fixed_jis && !d.encoding_fallback_cp932);
    assert!(d.ime_enabled && d.ime_inline && !d.ime_cursor_related);
    assert_eq!(d.encoding_ambiguous_width, 1);
    assert_eq!(d.encoding_emoji_width, 1);

    let s = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nKanjiReceive=SJIS\r\nKanjiSend=sjis\r\n\
          DecSpMappingDir=9\r\nUnicodeAmbiguousWidth=2\r\nUnicodeEmojiWidth=9\r\n\
          UnicodeToDecSpMapping=-1\r\n",
    ));
    assert_eq!(s.encoding_receive, EncodingReceive::Sjis);
    assert_eq!(
        s.encoding_send,
        EncodingSend::Utf8,
        "the charset table uses strcmp"
    );
    assert_eq!(
        s.encoding_dec_special_direction,
        EncodingDecSpecialDirection::UnicodeToDec,
        "an invalid number takes the switch's else arm, not its default"
    );
    assert_eq!(s.encoding_ambiguous_width, 2);
    assert_eq!(
        s.encoding_emoji_width, 1,
        "this range defaults above its ceiling rather than capping"
    );
    assert_eq!(s.encoding_unicode_to_dec_special, 65_535);
}

#[test]
fn the_deferred_printer_and_tek_settings_keep_their_shapes() {
    let d = Settings::default();
    assert_eq!(d.printer_passthrough_delay, 3);
    assert_eq!(
        (
            d.printer_margin_left,
            d.printer_margin_right,
            d.printer_margin_top,
            d.printer_margin_bottom,
        ),
        (50, 50, 50, 50)
    );
    assert_eq!((d.tek_x, d.tek_y), (i32::MIN, i32::MIN));
    assert_eq!(d.tek_color, [0, 0, 0, 255, 255, 255]);

    let s = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nPassThruDelay=-1\r\nPrnMargin=10,20\r\n\
          TEKPos=30\r\nTEKIcon=unknown\r\n",
    ));
    assert_eq!(s.printer_passthrough_delay, 65_535);
    assert_eq!(
        (
            s.printer_margin_left,
            s.printer_margin_right,
            s.printer_margin_top,
            s.printer_margin_bottom,
        ),
        (10, 20, 0, 0),
        "GetNthNum supplies zero for missing fields"
    );
    assert_eq!((s.tek_x, s.tek_y), (30, 0));
    assert_eq!(s.tek_icon.as_ini(), "Default");
}

#[test]
fn the_final_compatibility_settings_keep_upstreams_parsers() {
    let d = Settings::default();
    assert!(!d.debug_enabled && d.debug_modes == "all");
    assert_eq!(d.settings_source_version, "5.7.0");
    assert_eq!(d.font_draw_api, FontDrawApi::Auto);
    assert_eq!(
        (
            d.font_space_left,
            d.font_space_right,
            d.font_space_top,
            d.font_space_bottom,
        ),
        (0, 0, 0, 0)
    );
    assert_eq!((d.printer_vt_ppi_x, d.printer_vt_ppi_y), (0, 0));
    assert_eq!(d.window_maximized_bug_tweak, 2);
    assert_eq!(d.window_icon, WindowIcon::Default);

    let s = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nVTDrawAPI=ansi\r\nVTFontSpace=-1,2,3\r\nVTPPI=96\r\n\
          MaximizedBugTweak=-1\r\nVTIcon=VT_FLAT\r\n",
    ));
    assert_eq!(s.font_draw_api, FontDrawApi::Ansi);
    assert_eq!(
        (
            s.font_space_left,
            s.font_space_right,
            s.font_space_top,
            s.font_space_bottom,
        ),
        (-1, 2, 3, 0),
        "GetNthNum supplies zero for a missing field"
    );
    assert_eq!((s.printer_vt_ppi_x, s.printer_vt_ppi_y), (96, 0));
    assert_eq!(s.window_maximized_bug_tweak, 65_535);
    assert_eq!(s.window_icon, WindowIcon::VtFlat);

    let on = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nMaximizedBugTweak=ON\r\nVTDrawAPI=other\r\nVTIcon=other\r\n",
    ));
    assert_eq!(on.window_maximized_bug_tweak, 2);
    assert_eq!(on.font_draw_api, FontDrawApi::Auto);
    assert_eq!(on.window_icon, WindowIcon::Default);

    let debug = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nDebug=on\r\nDebugModes=none\r\n",
    ));
    assert!(
        !debug.debug_enabled,
        "an empty debug mask turns Debug back off"
    );
    let mut debug = Settings::default();
    assert!(debug.set_str("debug.enabled", "on"));
    assert!(debug.set_str("debug.modes", "unknown"));
    assert!(
        !debug.debug_enabled,
        "name-addressed writes use the same rule"
    );
}

#[test]
fn an_empty_file_gives_the_defaults() {
    let ini = Ini::parse(b"");
    assert_eq!(Settings::load(&ini), Settings::default());
}

#[test]
fn on_off_is_read_the_way_its_default_biases_it() {
    // `GetOnOff` is not a symmetric parse (`ttset.c:344`): with a default of
    // on, anything but `off` is on; with a default of off, only `on` is on.
    // So the same value means opposite things for two settings.
    let ini = Ini::parse(b"[Tera Term]\r\nXterm256Color=1\r\nAixterm16Color=1\r\n");
    let s = Settings::load(&ini);
    assert!(s.color_xterm_256, "1 is not `off`, and the default is on");
    assert!(!s.color_aixterm_16, "1 is not `on`, and the default is off");

    // ...and only the first three characters ever reach the comparison,
    // because upstream reads into a four-byte buffer.
    let ini = Ini::parse(b"[Tera Term]\r\nXterm256Color=offline\r\n");
    assert!(!Settings::load(&ini).color_xterm_256);
}

#[test]
fn one_key_can_hold_two_settings() {
    let ini = Ini::parse(b"[Tera Term]\r\nTerminalSize=132,50\r\n");
    let mut s = Settings::load(&ini);
    assert_eq!((s.terminal_cols, s.terminal_rows), (132, 50));

    // Writing one half must keep the other, or the setting the caller did not
    // touch silently goes back to its default.
    let mut out = Ini::parse(b"[Tera Term]\r\nTerminalSize=132,50\r\n");
    s.terminal_cols = 100;
    s.store(&mut out);
    assert_eq!(out.get("Tera Term", "TerminalSize"), Some("100,50"));
}

#[test]
fn the_window_position_is_written_only_when_its_switch_is_on() {
    let mut ini = Ini::parse(b"[Tera Term]\r\nVTPos='12,34'\r\n");
    let mut s = Settings::load(&ini);
    assert_eq!((s.window_x, s.window_y), (12, 34));
    assert!(!s.window_save_position);

    // `_WriteIniFile` skips this key with SaveVTWinPos off. Skipping means
    // preserving the exact old line, quotes included — deleting it or writing
    // the sentinel would both change a shared file the user did not ask us to
    // move.
    s.window_x = 56;
    s.window_y = 78;
    s.store(&mut ini);
    assert!(String::from_utf8(ini.to_bytes())
        .expect("utf8")
        .contains("VTPos='12,34'"));

    s.window_save_position = true;
    s.store(&mut ini);
    assert_eq!(ini.get("Tera Term", "VTPos"), Some("56,78"));

    let mut empty = Ini::new();
    let defaults = Settings::default();
    assert_eq!((defaults.window_x, defaults.window_y), (i32::MIN, i32::MIN));
    defaults.store(&mut empty);
    assert_eq!(empty.get("Tera Term", "VTPos"), None);

    // `GetNthNum`, not `GetNthNum2`: once the key exists, a field it omitted
    // is zero rather than that field's default. The absent key above remains
    // the two sentinels because GetPrivateProfileString supplied both first.
    let partial = Settings::load(&Ini::parse(b"[Tera Term]\r\nVTPos=12\r\n"));
    assert_eq!((partial.window_x, partial.window_y), (12, 0));
}

/// `store_one` is `store` for one named setting, which is how a change nobody
/// asked to save is persisted — what the last connection was opened with, and
/// nothing else in the same file. See `docs/deviations.md`.
#[test]
fn one_setting_can_be_written_without_writing_the_rest() {
    let mut ini = Ini::parse(b"; a comment\r\n[Tera Term]\r\nSomethingElse=kept\r\n");
    let mut s = Settings::load(&ini);
    s.serial_baud = 57600;
    s.recent_serial_port = String::from("/dev/ttyS3");
    s.terminal_title = String::from("not written");

    assert!(s.store_one(&mut ini, "serial.baud"));
    assert!(s.store_one(&mut ini, "recent.serial_port"));
    assert!(
        !s.store_one(&mut ini, "no.such.setting"),
        "an unnamed setting is the one refusal this has"
    );

    assert_eq!(ini.get("Tera Term", "BaudRate"), Some("57600"));
    assert_eq!(ini.get("Sterna", "SerialPort"), Some("/dev/ttyS3"));
    assert_eq!(ini.get("Tera Term", "SomethingElse"), Some("kept"));
    assert_eq!(
        ini.get("Tera Term", "Title"),
        None,
        "a full store would have pinned this one and every other default"
    );
    assert!(String::from_utf8(ini.to_bytes())
        .expect("utf8")
        .contains("; a comment"));

    // The same `write-if` rule as `store`, because both come out of one
    // emitter: with `SaveVTWinPos` off the old line is left exactly as it is.
    let mut position = Ini::parse(b"[Tera Term]\r\nVTPos='12,34'\r\n");
    let mut moved = Settings::load(&position);
    moved.window_x = 56;
    moved.window_y = 78;
    assert!(moved.store_one(&mut position, "window.x"));
    assert!(String::from_utf8(position.to_bytes())
        .expect("utf8")
        .contains("VTPos='12,34'"));
    moved.window_save_position = true;
    assert!(moved.store_one(&mut position, "window.x"));
    assert_eq!(position.get("Tera Term", "VTPos"), Some("56,34"));
}

#[test]
fn cygwin_directory_keeps_upstreams_reader_writer_mismatch() {
    // `ttset.c:1476` really has a trailing space in the reader literal, while
    // `:2250` writes the ordinary spelling. Keep both facts in the generated
    // code: hiding the typo in the schema makes the upstream-key diff lie.
    let field = FIELDS
        .iter()
        .find(|f| f.name == "connection.cygwin_directory")
        .expect("the Cygwin setting");
    assert_eq!(field.key, "CygwinDirectory ");

    let input = Ini::parse(b"[Tera Term]\r\nCygwinDirectory=D:\\cygwin64\r\n");
    let settings = Settings::load(&input);
    assert_eq!(settings.connection_cygwin_directory, "D:\\cygwin64");

    let mut output = Ini::new();
    settings.store(&mut output);
    let text = String::from_utf8(output.to_bytes()).expect("utf8");
    assert!(text.contains("CygwinDirectory=D:\\cygwin64\r\n"));
    assert!(!text.contains("CygwinDirectory ="));
}

#[test]
fn the_two_comma_field_helpers_disagree_about_an_omitted_field() {
    // Sizes use `GetNthNum`, where a present value's missing field is zero.
    let s = Settings::load(&Ini::parse(b"[Tera Term]\r\nPasteDialogSize=400\r\n"));
    assert_eq!(s.clipboard_paste_dialog_width, 400);
    assert_eq!(s.clipboard_paste_dialog_height, 0);

    // Transfer timeouts use `GetNthNum2`, where every missing field takes the
    // fallback passed for that field, and only a supplied zero is floored.
    let s = Settings::load(&Ini::parse(b"[Tera Term]\r\nXmodemTimeouts=5\r\n"));
    assert_eq!(s.transfer_xmodem_timeout_init, 5);
    assert_eq!(s.transfer_xmodem_timeout_init_crc, 3);
    assert_eq!(s.transfer_xmodem_timeout_short, 10);
}

#[test]
fn an_unrecognised_value_takes_the_default_rather_than_failing() {
    // Upstream spells nearly every default as the `else` branch of a chain of
    // comparisons, so a typo is not an error — it is the default, silently.
    let ini = Ini::parse(b"[Tera Term]\r\nCRReceive=nonsense\r\nTerminalID=vt220\r\n");
    let s = Settings::load(&ini);
    assert_eq!(s.terminal_cr_receive, TerminalCrReceive::Detect);
    assert_eq!(
        s.terminal_id,
        TerminalId::Vt100,
        "the table is case-sensitive"
    );
}

#[test]
fn the_mouse_cursor_keeps_the_files_own_spelling() {
    // Unlike an enumerated setting, `MouseCursor` is copied into
    // `MouseCursorName` as text and interpreted only when the pointer is set.
    // `_stricmp` accepts this lower-case spelling, and an unknown name is a
    // live no-op rather than the default — both therefore have to survive a
    // round trip unchanged.
    for value in ["cross", "MY-CURSOR"] {
        let ini = Ini::parse(format!("[Tera Term]\r\nMouseCursor={value}\r\n").as_bytes());
        let s = Settings::load(&ini);
        assert_eq!(s.mouse_cursor, value);

        let mut out = Ini::new();
        s.store(&mut out);
        assert_eq!(out.get("Tera Term", "MouseCursor"), Some(value));
    }
    assert_eq!(Settings::default().mouse_cursor, "IBEAM");
}

#[test]
fn character_width_delimits_words_by_default() {
    assert!(Settings::default().keyboard_width_delimits_word);

    let off = Ini::parse(b"[Tera Term]\r\nDelimDBCS=off\r\n");
    assert!(!Settings::load(&off).keyboard_width_delimits_word);

    // `GetOnOff` is biased by the on default, so anything other than the
    // literal `off` remains on. This includes the numeric form people often
    // use for INI booleans.
    let numeric = Ini::parse(b"[Tera Term]\r\nDelimDBCS=1\r\n");
    assert!(Settings::load(&numeric).keyboard_width_delimits_word);
}

#[test]
fn meta_8bit_keeps_raw_and_text_distinct() {
    let load = |value: &str| {
        Settings::load(&Ini::parse(
            format!("[Tera Term]\r\nMeta8Bit={value}\r\n").as_bytes(),
        ))
        .keyboard_meta_8bit
    };
    assert_eq!(
        Settings::default().keyboard_meta_8bit,
        KeyboardMeta8bit::Off
    );
    assert_eq!(load("raw"), KeyboardMeta8bit::Raw);
    assert_eq!(load("on"), KeyboardMeta8bit::Raw, "the read-only alias");
    assert_eq!(load("text"), KeyboardMeta8bit::Text);
    assert_eq!(load("nonsense"), KeyboardMeta8bit::Off, "the else arm");

    let font = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nFontQuality=NONANTIALIASED\r\n",
    ));
    assert_eq!(font.font_quality, FontQuality::NonAntialiased);
}

#[test]
fn setup_backups_are_on_by_default() {
    assert!(Settings::default().settings_auto_backup);

    let off = Ini::parse(b"[Tera Term]\r\nIniAutoBackup=off\r\n");
    assert!(!Settings::load(&off).settings_auto_backup);

    // This is another default-on `GetOnOff`: numeric values do not disable it.
    let numeric = Ini::parse(b"[Tera Term]\r\nIniAutoBackup=0\r\n");
    assert!(Settings::load(&numeric).settings_auto_backup);
}

#[test]
fn automatic_settings_saves_are_opt_in() {
    let mut settings = Settings::default();
    assert!(!settings.settings_auto_save_changes);

    let ini = Ini::parse(b"[Sterna]\r\nAutoSaveSettings=on\r\n");
    settings = Settings::load(&ini);
    assert!(settings.settings_auto_save_changes);
    assert_eq!(
        settings.get_str("settings.auto_save_changes"),
        Some("on".into())
    );

    assert!(settings.set_str("settings.auto_save_changes", "off"));
    let mut stored = ini;
    assert!(settings.store_one(&mut stored, "settings.auto_save_changes"));
    assert_eq!(stored.to_bytes(), b"[Sterna]\r\nAutoSaveSettings=off\r\n");
}

#[test]
fn active_opacity_inherits_the_loaded_inactive_value() {
    let defaults = Settings::default();
    assert_eq!(defaults.window_opacity_inactive, 255);
    assert_eq!(defaults.window_opacity_active, 255);

    // `AlphaBlendActive` passes the inactive value as
    // GetPrivateProfileInt's fallback, after that value has itself been
    // narrowed. Missing and empty values therefore inherit.
    for line in ["", "AlphaBlendActive="] {
        let ini = Ini::parse(format!("[Tera Term]\r\nAlphaBlend=120\r\n{line}\r\n").as_bytes());
        let settings = Settings::load(&ini);
        assert_eq!(settings.window_opacity_inactive, 120);
        assert_eq!(settings.window_opacity_active, 120);
    }

    // A present value that cannot be parsed is Win32's separate integer trap:
    // it becomes zero rather than the fallback.
    let invalid = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nAlphaBlend=120\r\nAlphaBlendActive=not-a-number\r\n",
    ));
    assert_eq!(invalid.window_opacity_active, 0);
}

/// Both opacity keys land in a `BYTE`, so the `max(0, …)`/`min(255, …)` pair
/// upstream applies next (`ttset.c:1467`) is **dead code** — the assignment has
/// already brought the value inside the range.
///
/// This row used to say `int_clamp(0..255)`, which is what that pair looks
/// like, and it inverted the answer for the one value somebody writes:
/// `AlphaBlend=-1` is an *opaque* window upstream and was a fully transparent
/// one here.
#[test]
fn opacity_narrows_into_its_byte_rather_than_clamping() {
    let load = |file: &str| {
        let s = Settings::load(&Ini::parse(format!("[Tera Term]\r\n{file}").as_bytes()));
        (s.window_opacity_inactive, s.window_opacity_active)
    };
    assert_eq!(load("AlphaBlend=-1\r\nAlphaBlendActive=300\r\n"), (255, 44));
    assert_eq!(load("AlphaBlend=256\r\n"), (0, 0), "and not 255 either");
    assert_eq!(load("AlphaBlend=120\r\n"), (120, 120));

    // A script takes the same rule; the dialog is what keeps a person inside
    // 0..255, and it is the only thing that ever did.
    let mut s = Settings::default();
    assert!(s.set_str("window.opacity_inactive", "-1"));
    assert_eq!(s.window_opacity_inactive, 255);
}

/// The keys upstream reads straight into a `WORD` with no bounds test at all,
/// where the narrowing is therefore the *whole* rule.
///
/// A port number is the honest case for it — 16 bits is what a port is — and
/// the delays are where a plain `int` row would otherwise hand a negative
/// number to something that has to wait for it.
#[test]
fn a_word_key_wraps_where_a_plain_int_row_would_not() {
    let load = |key: &str, v: &str| {
        Settings::load(&Ini::parse(
            format!("[Tera Term]\r\n{key}={v}\r\n").as_bytes(),
        ))
    };
    assert_eq!(load("TCPPort", "8080").connection_tcp_port, 8080);
    assert_eq!(load("TCPPort", "65536").connection_tcp_port, 0);
    assert_eq!(load("TCPPort", "-1").connection_tcp_port, 65535);
    assert_eq!(load("TelPort", "-1").connection_telnet_port, 65535);
    assert_eq!(load("DelayPerChar", "-1").serial_delay_per_char, 65535);
    assert_eq!(load("DelayPerLine", "70000").serial_delay_per_line, 4464);
    assert_eq!(load("LogRotateStep", "-1").log_rotate_step, 65535);
    assert_eq!(
        load("AutoComPortReconnectRetryCount", "-1").serial_auto_reconnect_retries,
        65535
    );
}

#[test]
fn title_format_is_the_shipped_bit_word() {
    let defaults = Settings::default();
    assert_eq!(defaults.window_title_format, 13, "endpoint - title VT");

    // It is a WORD, not an enum restricted to the six bits the dialog knows.
    // Unknown bits within the word survive, while values outside it narrow on
    // assignment exactly as they do in C.
    for (value, narrowed) in [("77", 77), ("-1", 65535), ("65537", 1)] {
        let ini = Ini::parse(format!("[Tera Term]\r\nTitleFormat={value}\r\n").as_bytes());
        let settings = Settings::load(&ini);
        assert_eq!(settings.window_title_format, narrowed);

        let mut out = Ini::new();
        settings.store(&mut out);
        let expected = narrowed.to_string();
        assert_eq!(out.get("Tera Term", "TitleFormat"), Some(expected.as_str()));
    }
}

#[test]
fn writing_settings_leaves_the_rest_of_the_file_alone() {
    let original = b"; my notes\r\n[Tera Term]\r\nCRReceive=LF\r\n[Extra]\r\nMine=1\r\n";
    let mut ini = Ini::parse(original);
    let s = Settings::load(&ini);
    assert_eq!(s.terminal_cr_receive, TerminalCrReceive::Lf);

    s.store(&mut ini);
    let text = String::from_utf8(ini.to_bytes()).expect("utf8");
    assert!(text.starts_with("; my notes\r\n"), "the comment went");
    assert!(
        text.contains("[Extra]\r\nMine=1\r\n"),
        "someone else's section went"
    );
    assert!(
        text.contains("CRReceive=LF"),
        "the setting changed spelling"
    );
}

#[test]
fn every_setting_round_trips_through_a_file() {
    // Write the defaults out, read them back, and expect the same thing —
    // which catches a writer and a reader that disagree about a spelling, the
    // failure that turns "save settings" into "reset settings".
    let mut ini = Ini::new();
    let before = Settings::default();
    before.store(&mut ini);
    let after = Settings::load(&Ini::parse(&ini.to_bytes()));
    assert_eq!(before, after);
}

#[test]
fn every_setting_is_reachable_by_name() {
    // The dialog, `setsetting` and the docs all walk `FIELDS`, so a field
    // missing from the name-addressed accessors is a setting that exists in
    // the file and cannot be changed from anywhere.
    let mut s = Settings::default();
    for field in FIELDS {
        let value = s
            .get_str(field.name)
            .unwrap_or_else(|| panic!("{} has no getter", field.name));
        assert!(
            s.set_str(field.name, &value),
            "{} has no setter",
            field.name
        );
        assert_eq!(s.get_str(field.name).as_deref(), Some(value.as_str()));
    }
    assert!(s.get_str("no.such.setting").is_none());
    assert!(!s.set_str("no.such.setting", "1"));
}

#[test]
fn the_metadata_describes_what_is_really_there() {
    assert!(FIELDS.len() >= 39);
    assert_eq!(FIELDS.len(), SETTING_HELP.len());
    for (field, help) in FIELDS.iter().zip(SETTING_HELP) {
        assert!(field.name.contains('.'), "{} is not dotted", field.name);
        assert!(field.name.starts_with(field.page), "{}", field.name);
        assert!(!field.doc.is_empty(), "{} has no documentation", field.name);
        assert!(!help.is_empty(), "{} has no user help", field.name);
        assert!(
            help.starts_with("This setting "),
            "{} user help has no explicit subject: {}",
            field.name,
            help
        );
        assert!(
            !help.contains('`') && !help.contains("ttset.c") && !help.contains("->"),
            "{} exposes implementation details in user help: {}",
            field.name,
            help
        );
        assert!(
            !help.contains(';'),
            "{} uses a semicolon in user help: {}",
            field.name,
            help
        );
        let sentences: Vec<_> = help
            .split(['.', '!', '?'])
            .filter(|s| !s.trim().is_empty())
            .collect();
        assert!(
            sentences.len() <= 6,
            "{} has more than six user-help sentences: {}",
            field.name,
            help
        );
        for sentence in sentences {
            assert!(
                sentence.split_whitespace().count() <= 25,
                "{} has user help longer than 25 words: {}",
                field.name,
                sentence.trim()
            );
        }
        let prose = format!(" {} ", help.to_ascii_lowercase());
        for phrase in [
            " currently ",
            " whether ",
            " instead ",
            " turn on ",
            " turns on ",
            " turning on ",
            " turn off ",
            " turns off ",
            " turning off ",
            " fall back ",
            " falls back ",
            " back up ",
            " backs up ",
            " colour ",
            " colours ",
            " acknowledgement ",
        ] {
            assert!(
                !prose.contains(phrase),
                "{} uses non-STE wording {:?}: {}",
                field.name,
                phrase.trim(),
                help
            );
        }
        assert!(!field.section.is_empty());
        assert!(!field.key.is_empty());
        // A default that the setting's own parser does not accept is a schema
        // bug that would otherwise surface as a silently different value.
        if let Kind::Enum(spellings) = field.kind {
            assert!(
                spellings
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(field.default)),
                "{}: default {} is not one of its spellings",
                field.name,
                field.default
            );
        }
    }
}

#[test]
fn the_generated_file_is_current() {
    // Same arrangement as `tt-ffi`'s committed header: the generator is not
    // wired into either build system, so this is what stops the schema and the
    // code drifting apart. In-process rather than a nested `cargo run` —
    // that is the package-cache-lock trap `tt-ffi/build.rs` documents.
    let schema = std::fs::read_to_string(gen::schema_path()).expect("the schema");
    let committed = std::fs::read_to_string(gen::generated_path()).expect("the generated file");
    assert_eq!(
        gen::generate(&schema),
        committed,
        "src/generated.rs is stale — run `cargo run -p tt-config --bin gen-settings`"
    );
}

/// `TerminalID` is the one enumerated setting upstream compares with `strcmp`
/// rather than `_stricmp`, and it never fails — so the wrong case is not an
/// error, it is a VT100.
#[test]
fn the_terminal_id_is_case_sensitive_and_never_fails() {
    let load = |v: &str| {
        Settings::load(&Ini::parse(
            format!("[Tera Term]\r\nTerminalID={v}\r\n").as_bytes(),
        ))
        .terminal_id
    };
    assert_eq!(load("VT320"), TerminalId::Vt320);
    assert_eq!(load("vt320"), TerminalId::Vt100, "strcmp, not _stricmp");
    assert_eq!(load("VT220"), TerminalId::Vt220, "and it is in the table");
    // `dumb` is the one lower-case spelling upstream ships, so `DUMB` is not it.
    assert_eq!(load("dumb"), TerminalId::Dumb);
    assert_eq!(load("DUMB"), TerminalId::Vt100);
    // ...while a setting read with `_stricmp` does not care.
    let ini = Ini::parse(b"[Tera Term]\r\nCRReceive=crlf\r\n");
    assert_eq!(
        Settings::load(&ini).terminal_cr_receive,
        TerminalCrReceive::CrLf
    );
}

/// `ttset.c:615` is not a clamp in both directions: too small takes the
/// default, too large takes the ceiling.
#[test]
fn a_size_outside_the_range_takes_the_default_below_and_the_ceiling_above() {
    let load = |v: &str| {
        let s = Settings::load(&Ini::parse(
            format!("[Tera Term]\r\nTerminalSize={v}\r\n").as_bytes(),
        ));
        (s.terminal_cols, s.terminal_rows)
    };
    assert_eq!(load("132,50"), (132, 50));
    assert_eq!(
        load("0,0"),
        (80, 24),
        "a window nobody could use, otherwise"
    );
    assert_eq!(load("-5,-5"), (80, 24));
    assert_eq!(
        load("9999,9999"),
        (1000, 500),
        "TermWidthMax, TermHeightMax"
    );

    // A script goes through the same rule, because a value that a file and a
    // `setsetting` disagree about is a setting nobody can reason about.
    let mut s = Settings::default();
    assert!(s.set_str("terminal.cols", "0"));
    assert_eq!(s.terminal_cols, 80);
    assert!(s.set_str("terminal.cols", "4000"));
    assert_eq!(s.terminal_cols, 1000);
}

/// The connection settings, and the one whose default is another setting's
/// *initialiser*.
#[test]
fn tcp_ports_default_is_the_initialiser_and_not_the_file() {
    let d = Settings::default();
    assert_eq!(d.connection_tcp_port, 23);
    assert_eq!(d.connection_telnet_port, 23);

    // **`ttset.c:966` reads `TCPPort` with `ts->TelPort` as its default, and
    // `TelPort=` is not read until `:1311`** — so the value in hand is the
    // hardcoded 23 from `:566`, not the file's. A file that moves telnet to
    // 2323 and says nothing about `TCPPort` still opens 23.
    let ini = Ini::parse(b"[Tera Term]\r\nTelPort=2323\r\n");
    let s = Settings::load(&ini);
    assert_eq!(s.connection_telnet_port, 2323);
    assert_eq!(s.connection_tcp_port, 23, "the initialiser, not TelPort=");
}

/// `Telnet=` and `TelBin=` differ only in their default, which is exactly the
/// pair that makes `GetOnOff` dangerous.
#[test]
fn two_telnet_flags_read_the_same_value_two_ways() {
    let ini = Ini::parse(b"[Tera Term]\r\nTelnet=1\r\nTelBin=1\r\n");
    let s = Settings::load(&ini);
    assert!(s.connection_telnet, "default on, so 1 is not `off`");
    assert!(!s.connection_telnet_binary, "default off, so 1 is not `on`");
}

/// The port type holds two values in the file and four in memory, and upstream
/// writes the other two down as `tcpip`.
#[test]
fn the_port_type_is_serial_or_everything_else() {
    let ini = Ini::parse(b"[Tera Term]\r\nPort=serial\r\n");
    assert_eq!(
        Settings::load(&ini).connection_port_type,
        ConnectionPortType::Serial
    );
    // `ttset.c:591` is an `_stricmp` against `serial` with an `else`, so
    // anything else at all is TCP/IP.
    for value in [&b"tcpip"[..], b"TCPIP", b"file", b"nonsense", b""] {
        let mut ini = Ini::parse(b"[Tera Term]\r\n");
        ini.set("Tera Term", "Port", &String::from_utf8_lossy(value));
        assert_eq!(
            Settings::load(&ini).connection_port_type,
            ConnectionPortType::TcpIp,
            "{}",
            String::from_utf8_lossy(value)
        );
    }
}

/// The four serial tables are the command line's and the file's at once, and
/// one value has two spellings.
#[test]
fn the_serial_tables_are_shared_with_the_command_line() {
    let ini =
        Ini::parse(b"[Tera Term]\r\nDataBit=7\r\nParity=EVEN\r\nStopBit=2\r\nFlowCtrl=rtscts\r\n");
    let s = Settings::load(&ini);
    assert_eq!(s.serial_data_bits, SerialDataBits::Seven);
    assert_eq!(s.serial_parity, SerialParity::Even);
    assert_eq!(s.serial_stop_bits, SerialStopBits::Two);
    // `rtscts` and `hard` are one value (`ttset.c:111`); the canonical spelling
    // is what gets written back.
    assert_eq!(s.serial_flow, SerialFlow::Hardware);
    let mut out = Ini::parse(b"");
    s.store(&mut out);
    assert_eq!(out.get("Tera Term", "FlowCtrl"), Some("hard"));

    // A value none of the tables has takes the default, because only the file's
    // reader has an `else` — `SerialPortConfconvertStr2Id` returns FALSE
    // without storing.
    let ini = Ini::parse(b"[Tera Term]\r\nParity=maybe\r\nDataBit=9\r\n");
    let s = Settings::load(&ini);
    assert_eq!(s.serial_parity, SerialParity::None);
    assert_eq!(s.serial_data_bits, SerialDataBits::Eight);
}

/// `MaxComPort` is floored as well as capped, and the narrowing in front of
/// both is what decides the one value somebody would actually write.
///
/// `ttset.c:1218`: the `GetPrivateProfileInt` result lands in a `WORD` before
/// either test runs, so this is `uint16_clamp` rather than any of the three
/// ordinary bounds. It used to be `int(4..4096)`, which was wrong at both ends
/// — the default instead of the floor below 4, and 4 instead of 4096 for the
/// negative value that wraps.
#[test]
fn max_com_port_is_narrowed_and_then_clamped() {
    let load = |v: &str| {
        Settings::load(&Ini::parse(
            format!("[Tera Term]\r\nMaxComPort={v}\r\n").as_bytes(),
        ))
        .serial_max_com_port
    };
    assert_eq!(load("64"), 64);
    assert_eq!(load("1"), 4, "the floor, not the default");
    assert_eq!(load("99999"), 4096, "65535 short of itself, then the cap");
    // The one a person writes: `-1` for "no limit" is `(UINT)-1`, narrowed to
    // 65535 and capped — the *top* of the range. A plain clamp would read the
    // -1 as below the floor and give 4, which is four COM ports.
    assert_eq!(load("-1"), 4096, "not the floor");
    // ...and 65540 wraps to 4, which is genuinely below the floor and stays
    // there rather than becoming the ceiling.
    assert_eq!(load("65540"), 4);

    // A script takes the same rule, as it does for every other bound.
    let mut s = Settings::default();
    assert!(s.set_str("serial.max_com_port", "-1"));
    assert_eq!(s.serial_max_com_port, 4096);
}

/// `ComPort`'s own bound is `MaxComPort`, and the two are read a thousand lines
/// apart with the test after both (`ttset.c:1223`).
///
/// The rule is a **reset to 1**, not a clamp — the port that was out of range
/// becomes the first one rather than the nearest legal one — and no schema row
/// can carry a bound that is another setting's loaded value, which is why this
/// lives in `Settings::normalize`.
#[test]
fn an_out_of_range_com_port_resets_to_the_first_one() {
    let load = |file: &str| {
        Settings::load(&Ini::parse(format!("[Tera Term]\r\n{file}").as_bytes())).serial_com_port
    };
    assert_eq!(load("ComPort=3\r\n"), 3);
    // The default ceiling is 256, so this is the reset rather than a clamp to
    // 256 — which is the whole difference, and it is what a user's own Tera
    // Term does with the file.
    assert_eq!(load("ComPort=300\r\n"), 1);
    assert_eq!(load("ComPort=0\r\n"), 1);

    // ...and the ceiling really is the file's own `MaxComPort`, whichever
    // order the two keys appear in. Upstream's read order is fixed in the
    // source; a file's is not.
    assert_eq!(load("ComPort=8\r\nMaxComPort=4\r\n"), 1);
    assert_eq!(load("MaxComPort=4\r\nComPort=8\r\n"), 1);
    assert_eq!(load("ComPort=300\r\nMaxComPort=1024\r\n"), 300);

    // The narrowing happens first, so a value that wraps back into range is a
    // real port number rather than something above every ceiling.
    assert_eq!(load("ComPort=65538\r\n"), 2);
    assert_eq!(load("ComPort=-1\r\n"), 1, "65535, and then out of range");

    // Changing either key by name re-runs the test, so a script and a file
    // cannot disagree about which port is open.
    let mut s = Settings::default();
    assert!(s.set_str("serial.com_port", "9"));
    assert_eq!(s.serial_com_port, 9);
    assert!(s.set_str("serial.max_com_port", "4"));
    assert_eq!(s.serial_com_port, 1, "the ceiling moved under it");
}

/// The third bound, and the reason it is a third rather than one of the other
/// two: `min(max(0, v), 5000)` (`ttset.c:1633`) clamps at both ends, so it
/// disagrees with `int(lo..hi)` below the floor and with `int_min(lo)` above
/// the ceiling. `PasteDelayPerLine` is the only setting in the file read this
/// way.
#[test]
fn the_paste_delay_is_clamped_at_both_ends() {
    let load = |v: &str| {
        Settings::load(&Ini::parse(
            format!("[Tera Term]\r\nPasteDelayPerLine={v}\r\n").as_bytes(),
        ))
        .clipboard_paste_delay_per_line
    };
    assert_eq!(load("250"), 250);
    // `ranged` would give the default of 10 here. `GetPrivateProfileInt`
    // answers `-5` with `(UINT)-5`, which lands in upstream's `int` field as
    // -5 and is then floored.
    assert_eq!(load("-5"), 0, "the floor, not the default");
    // ...and `floored` would leave this alone, which is a minute a line.
    assert_eq!(load("60000"), 5000, "the ceiling");

    // A script takes the same rule, as it does for the other two bounds.
    let mut s = Settings::default();
    assert!(s.set_str("clipboard.paste_delay_per_line", "-1"));
    assert_eq!(s.clipboard_paste_delay_per_line, 0);
    assert!(s.set_str("clipboard.paste_delay_per_line", "9999"));
    assert_eq!(s.clipboard_paste_delay_per_line, 5000);
}

/// The clipboard family's defaults, which are the interesting half of it: two
/// of them decide what a mouse button does and ship opposite ways round, and
/// one is a second gate on a mode the host asked for.
#[test]
fn the_clipboard_defaults_are_upstreams() {
    let d = Settings::default();
    assert!(
        !d.clipboard_paste_rbutton_disabled,
        "ttset.c:1422 — the right button pastes"
    );
    assert!(
        d.clipboard_paste_mbutton_disabled,
        "ttset.c:1425 — and the middle button does not"
    );
    assert!(d.clipboard_confirm_paste, "ttset.c:1431");
    assert!(!d.clipboard_trim_trailing_newline, "ttset.c:1871");
    assert!(!d.clipboard_continued_line_copy, "ttset.c:1419");
    assert!(d.clipboard_auto_copy, "ttset.c:1105");
    assert!(d.clipboard_select_only_by_lbutton, "ttset.c:1449");
    assert!(d.clipboard_bracketed, "ttset.c:2002");
    assert!(!d.clipboard_bracketed_control_only, "ttset.c:2003");
    assert_eq!(d.clipboard_paste_dialog_width, 330);
    assert_eq!(d.clipboard_paste_dialog_height, 220);
}

/// `LogTimestampType` is the one setting whose *absence* is a value, because a
/// second key answers for it — so the schema gives the empty spelling a variant
/// of its own rather than folding it into the default.
#[test]
fn the_timestamp_type_keeps_absent_apart_from_local() {
    let of = |bytes: &[u8]| Settings::load(&Ini::parse(bytes)).log_timestamp_type;

    // Nothing said, which is what a Tera Term 4 file looks like.
    assert_eq!(of(b"[Tera Term]\r\n"), LogTimestampType::Unset);
    // `Key=` is an empty string and not the default, and here the two agree.
    assert_eq!(
        of(b"[Tera Term]\r\nLogTimestampType=\r\n"),
        LogTimestampType::Unset
    );
    assert_eq!(
        of(b"[Tera Term]\r\nLogTimestampType=local\r\n"),
        LogTimestampType::Local
    );
    assert_eq!(
        of(b"[Tera Term]\r\nLogTimestampType=utc\r\n"),
        LogTimestampType::Utc
    );
    assert_eq!(
        of(b"[Tera Term]\r\nLogTimestampType=ConnectionElapsed\r\n"),
        LogTimestampType::ConnectionElapsed
    );
    // And the one place this diverges, asserted so that it cannot change by
    // accident: upstream's chain falls to local time for a *typo* and to the
    // compatibility key only for an empty value, while the schema has one
    // fallback and it is the default. So `Locale` reads as absent here. It
    // shows only in a file that misspells this key *and* still carries
    // `LogTimestampUTC=on`; folding absent into `Local` instead would get the
    // ordinary Tera Term 4 file wrong, which is the trade.
    assert_eq!(
        of(b"[Tera Term]\r\nLogTimestampType=Locale\r\n"),
        LogTimestampType::Unset
    );
}

/// The two control lines are read with a default of `-1`, which is a sentinel
/// meaning "derive from the flow control" and not a value the `DCB` has. What
/// the schema has to get right is that the sentinel *survives* the read —
/// resolving it is `tt-session`'s, because the answer depends on another
/// setting.
#[test]
fn the_control_lines_keep_the_sentinel_their_default_is() {
    let s = Settings::load(&Ini::parse(b"[Tera Term]\r\n"));
    assert_eq!(s.serial_rts, -1);
    assert_eq!(s.serial_dtr, -1);

    // A negative number in the file is not an error — `GetPrivateProfileInt`
    // wraps it, which is what `ini-audit` recorded as 4294967291 for `-5`, and
    // the field is an `int`. So a written-out `-1` reads back as the sentinel
    // and this port's own file round-trips.
    let mut out = Ini::parse(b"");
    s.store(&mut out);
    assert_eq!(out.get("Tera Term", "FlowCtrlRTS"), Some("-1"));
    assert_eq!(Settings::load(&out).serial_rts, -1);

    // And a value that is one takes it literally, including the one Win32
    // would refuse. Nothing is clamped here; `serial_params` is where an
    // unknown number becomes Enable.
    let s = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nFlowCtrlRTS=2\r\nFlowCtrlDTR=9\r\n",
    ));
    assert_eq!(s.serial_rts, 2);
    assert_eq!(s.serial_dtr, 9);
}

/// The rest of the serial family, whose defaults are all somewhere other than
/// where the surrounding code suggests.
#[test]
fn the_serial_defaults_are_upstreams() {
    let s = Settings::load(&Ini::parse(b"[Tera Term]\r\n"));
    // `GetOnOff(…, TRUE)` at `ttset.c:1147`, so `ClearComBuffOnOpen=1` is on
    // and only a literal `off` turns it off.
    assert!(s.serial_clear_buffer_on_open);
    assert_eq!(s.serial_break_time, 1000);
    assert!(s.serial_auto_reconnect);
    assert_eq!(s.serial_auto_reconnect_delay, 500);
    assert_eq!(s.serial_auto_reconnect_delay_unknown_port, 2000);
    assert_eq!(s.serial_auto_reconnect_retry_interval, 1000);
    assert_eq!(s.serial_auto_reconnect_retries, 3);

    // The `GetOnOff` asymmetry, on the two bools this family added: both
    // default on, so `1` is on for the same reason `Telnet=1` is.
    let s = Settings::load(&Ini::parse(
        b"[Tera Term]\r\nClearComBuffOnOpen=1\r\nAutoComPortReconnect=0\r\n",
    ));
    assert!(s.serial_clear_buffer_on_open);
    assert!(s.serial_auto_reconnect);
}

#[test]
fn the_proxy_defaults_are_the_plugins() {
    let s = Settings::load(&Ini::parse(b"[Tera Term]\r\n"));
    assert_eq!(s.proxy_type, ProxyType::None);
    assert_eq!(s.proxy_port, 0);
    assert_eq!(s.proxy_timeout, 10);
    assert_eq!(s.proxy_socks_resolve, ProxySocksResolve::Auto);
    // The trailing spaces are part of the strings, and losing one turns a
    // prompt into a prefix of one.
    assert_eq!(s.proxy_telnet_hostname_prompt, ">> Host name: ");
    assert_eq!(s.proxy_telnet_connected_message, "-- Connected to ");
    assert_eq!(s.proxy_telnet_error_message, "!!!!!!!!");
    // `getStringW` with a NULL default, which `Logger::open` returns on.
    assert_eq!(s.proxy_debug_log, "");
}

#[test]
fn the_proxy_type_takes_both_socks_spellings_and_writes_one() {
    // `ProxyInfo::parseType` lower-cases and compares against a table with
    // `socks` and `socks5` both mapping to SOCKS5; `getTypeName` returns the
    // first row for the type, which is `socks`.
    for spelling in ["socks", "socks5", "SOCKS5", "Socks"] {
        let ini = Ini::parse(format!("[TTProxy]\r\nProxyType={spelling}\r\n").as_bytes());
        assert_eq!(
            Settings::load(&ini).proxy_type,
            ProxyType::Socks5,
            "{spelling}"
        );
    }
    let mut out = Ini::new();
    let s = Settings {
        proxy_type: ProxyType::Socks5,
        ..Default::default()
    };
    s.store(&mut out);
    let text = String::from_utf8(out.to_bytes()).unwrap();
    assert!(text.contains("ProxyType=socks\r\n"), "{text}");

    // The five `+ssl` spellings are in upstream's table and do nothing there,
    // so they are not in the schema and take the unrecognised arm.
    for spelling in ["socks5+ssl", "http+ssl", "ssl", "none+ssl", "wat"] {
        let ini = Ini::parse(format!("[TTProxy]\r\nProxyType={spelling}\r\n").as_bytes());
        assert_eq!(
            Settings::load(&ini).proxy_type,
            ProxyType::None,
            "{spelling}"
        );
    }
}

#[test]
fn the_proxy_section_is_quoted_the_way_its_own_plugin_quotes_it() {
    // `YCL`'s `IniFile::setString` escapes and wraps every string, which is
    // the only reason a trailing space survives `GetPrivateProfileString`.
    let mut out = Ini::new();
    let s = Settings {
        proxy_host: "proxy.example".into(),
        proxy_pass: "a\"b\\c".into(),
        ..Default::default()
    };
    s.store(&mut out);
    let text = String::from_utf8(out.to_bytes()).unwrap();
    assert!(text.contains("ProxyHost=\"proxy.example\"\r\n"), "{text}");
    assert!(
        text.contains("TelnetHostnamePrompt=\">> Host name: \"\r\n"),
        "{text}"
    );
    assert!(text.contains("ProxyPass=\"a\\\"b\\\\c\"\r\n"), "{text}");

    let back = Settings::load(&Ini::parse(&out.to_bytes()));
    assert_eq!(back.proxy_pass, "a\"b\\c");
    assert_eq!(back.proxy_telnet_hostname_prompt, ">> Host name: ");

    // And a file upstream wrote reads the same way here.
    let ini = Ini::parse(
        b"[TTProxy]\r\nTelnetErrorMessage=\"\\033[31mrefused\"\r\nProxyUser=\"a\\\\b\"\r\n",
    );
    let s = Settings::load(&ini);
    assert_eq!(s.proxy_telnet_error_message, "\x1b[31mrefused");
    assert_eq!(s.proxy_user, "a\\b");
}

#[test]
fn a_negative_proxy_timeout_takes_the_default() {
    // Upstream would hand `select` an invalid `timeval` and break every proxy
    // connection; below the floor takes the default here.
    let ini = Ini::parse(b"[TTProxy]\r\nConnectionTimeout=-1\r\n");
    assert_eq!(Settings::load(&ini).proxy_timeout, 10);
    let ini = Ini::parse(b"[TTProxy]\r\nConnectionTimeout=0\r\n");
    assert_eq!(Settings::load(&ini).proxy_timeout, 0);
}

#[test]
fn the_proxy_port_is_a_word_like_every_other_narrow_field() {
    // `getInteger` returns a `long` and the field is an `unsigned short`.
    let ini = Ini::parse(b"[TTProxy]\r\nProxyPort=65537\r\n");
    assert_eq!(Settings::load(&ini).proxy_port, 1);
}

/// The counter field ships **on**, which makes it the one `[Sterna]` chrome
/// switch whose default is not the shipped-off shape — and that changes what a
/// value in the file means. `GetOnOff` is default-biased: with a default of on,
/// only the literal `off` turns it off, where `LineEdit` above needs the
/// literal `on` to turn it on.
#[test]
fn counters_are_an_on_by_default_sterna_setting() {
    let mut settings = Settings::default();
    assert!(settings.window_counters);

    let ini = Ini::parse(b"[Sterna]\r\nCounters=off\r\n");
    settings = Settings::load(&ini);
    assert!(!settings.window_counters);
    assert_eq!(settings.get_str("window.counters"), Some("off".into()));

    // Default-biased, and this is the half that surprises: anything which is
    // not literally `off` reads as on.
    assert!(Settings::load(&Ini::parse(b"[Sterna]\r\nCounters=1\r\n")).window_counters);
    assert!(Settings::load(&Ini::parse(b"[Sterna]\r\nCounters=\r\n")).window_counters);

    assert!(settings.set_str("window.counters", "on"));
    assert!(settings.window_counters);
    assert!(!settings.set_str("window.meters", "on"));

    let mut stored = ini;
    assert!(settings.store_one(&mut stored, "window.counters"));
    assert_eq!(stored.to_bytes(), b"[Sterna]\r\nCounters=on\r\n");
}
