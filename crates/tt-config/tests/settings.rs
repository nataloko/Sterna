//! The schema, and the claim that a round trip changes nothing.

use tt_config::gen;
use tt_config::{
    ConnectionPortType, Ini, KeyboardBackspace, Kind, LogTimestampType, SerialDataBits, SerialFlow,
    SerialParity, SerialStopBits, Settings, TerminalCrReceive, TerminalId, FIELDS,
};

#[test]
fn the_defaults_are_upstreams() {
    let d = Settings::default();
    // The four `AGENTS.md` calls out by name, because each one is a default
    // that is not where it looks like it is — an `else` branch or a key read
    // a thousand lines after the initialiser that zeroes it.
    assert_eq!(d.terminal_cr_receive, TerminalCrReceive::Cr, "ttset.c:643");
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
    assert_eq!(s.terminal_cr_receive, TerminalCrReceive::Cr);
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
    for field in FIELDS {
        assert!(field.name.contains('.'), "{} is not dotted", field.name);
        assert!(field.name.starts_with(field.page), "{}", field.name);
        assert!(!field.doc.is_empty(), "{} has no documentation", field.name);
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

/// `MaxComPort` is floored as well as capped, which is the range the schema
/// carries.
#[test]
fn max_com_port_has_a_floor_of_its_own() {
    let ini = Ini::parse(b"[Tera Term]\r\nMaxComPort=1\r\n");
    // At or below the floor takes the *default*, which is what `ranged` means
    // here and is not the same as clamping to 4.
    assert_eq!(Settings::load(&ini).serial_max_com_port, 256);
    let ini = Ini::parse(b"[Tera Term]\r\nMaxComPort=99999\r\n");
    assert_eq!(Settings::load(&ini).serial_max_com_port, 4096);
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
