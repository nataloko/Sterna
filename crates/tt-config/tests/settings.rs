//! The schema, and the claim that a round trip changes nothing.

use tt_config::gen;
use tt_config::{Ini, Kind, KeyboardBackspace, Settings, TerminalCrReceive, TerminalId, FIELDS};

#[test]
fn the_defaults_are_upstreams() {
    let d = Settings::default();
    // The four `CLAUDE.md` calls out by name, because each one is a default
    // that is not where it looks like it is — an `else` branch or a key read
    // a thousand lines after the initialiser that zeroes it.
    assert_eq!(d.terminal_cr_receive, TerminalCrReceive::Cr, "ttset.c:643");
    assert_eq!(d.keyboard_backspace, KeyboardBackspace::Bs, "ttset.c:877");
    assert!(d.color_xterm_256, "ttset.c:741 — not the zeroed ColorFlag");
    assert!(!d.color_aixterm_16, "ttset.c:738 — and this one really is off");
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
    assert_eq!(s.terminal_id, TerminalId::Vt100, "the table is case-sensitive");
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
    assert!(text.contains("[Extra]\r\nMine=1\r\n"), "someone else's section went");
    assert!(text.contains("CRReceive=LF"), "the setting changed spelling");
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
        assert!(s.set_str(field.name, &value), "{} has no setter", field.name);
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
                spellings.iter().any(|s| s.eq_ignore_ascii_case(field.default)),
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
