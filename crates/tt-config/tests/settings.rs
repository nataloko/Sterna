//! The schema, and the claim that a round trip changes nothing.

use tt_config::gen;
use tt_config::{
    ConnectionPortType, Ini, KeyboardBackspace, Kind, SerialDataBits, SerialFlow, SerialParity,
    SerialStopBits, Settings, TerminalCrReceive, TerminalId, FIELDS,
};

#[test]
fn the_defaults_are_upstreams() {
    let d = Settings::default();
    // The four `CLAUDE.md` calls out by name, because each one is a default
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
