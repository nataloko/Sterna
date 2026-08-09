//! What the Win32 battery cannot ask: files, and round-tripping.
//!
//! `tests/win32.rs` covers behaviour, case by case, against a recorded real
//! implementation. This covers the things that only exist on our side of the
//! seam — reading and writing an actual file, and the promise that a file
//! nobody edited comes back byte for byte.

use tt_config::{Encoding, Ini};

#[test]
fn a_file_nobody_edited_comes_back_unchanged() {
    // Every encoding, every line ending, and the awkward lines: the promise
    // is that parsing and re-emitting is the identity, or a save after a
    // no-op rewrites a file the user will diff.
    let files: [&[u8]; 6] = [
        b"[Tera Term]\r\nVTColor=0,0,0,255,255,255\r\n",
        b"[Tera Term]\nVTColor=0,0,0\n",
        b"; a note\r\n[s]\r\n\r\nA=1\r\n; another\r\nB=\r\n",
        b"\xef\xbb\xbf[s]\r\nA=1\r\n",
        b"\xff\xfe[\x00s\x00]\x00\r\x00\n\x00A\x00=\x001\x00\r\x00\n\x00",
        b"[s]\r\nPath=C:\\caf\xe9\\log.txt\r\n",
    ];
    for original in files {
        let ini = Ini::parse(original);
        assert_eq!(
            ini.to_bytes(),
            original,
            "round trip changed {:?}",
            String::from_utf8_lossy(original)
        );
    }
}

#[test]
fn a_file_that_is_not_utf8_survives_being_written_back() {
    // Latin-1 is not a claim about what the bytes mean — a Japanese Tera Term
    // 4 wrote Shift-JIS — it is the decoding under which they all survive. A
    // lossy decode would turn each one into U+FFFD and destroy it on save.
    let original = b"[s]\r\nA=1\r\nPath=\x93\xfa\x96{\x8c\xea\r\n";
    let mut ini = Ini::parse(original);
    assert_eq!(ini.encoding(), Encoding::Latin1);
    assert!(ini.set("s", "A", "2"));
    assert_eq!(
        ini.to_bytes(),
        b"[s]\r\nA=2\r\nPath=\x93\xfa\x96{\x8c\xea\r\n",
        "the bytes nobody asked about moved"
    );
}

#[test]
fn writing_something_latin1_cannot_hold_upgrades_the_file() {
    // The one case where changing the encoding beats losing the value.
    let mut ini = Ini::parse(b"[s]\r\nA=1\r\n");
    assert_eq!(ini.encoding(), Encoding::Utf8);
    assert!(ini.set("s", "Font", "\u{6f22}\u{5b57}"));
    assert_eq!(ini.encoding(), Encoding::Utf8Bom);
    assert_eq!(ini.get("s", "Font"), Some("\u{6f22}\u{5b57}"));
    assert!(ini.to_bytes().starts_with(&[0xEF, 0xBB, 0xBF]));
}

#[test]
fn an_ascii_write_never_changes_the_encoding() {
    for original in [&b"[s]\r\nA=1\r\n"[..], &b"[s]\r\nA=\xe9\r\n"[..]] {
        let mut ini = Ini::parse(original);
        let before = ini.encoding();
        assert!(ini.set("s", "B", "2"));
        assert_eq!(ini.encoding(), before);
    }
}

#[test]
fn a_missing_file_reads_as_an_empty_one() {
    let dir = std::env::temp_dir().join("tt-config-missing");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("TERATERM.INI");

    let mut ini = Ini::load(&path).expect("a missing file is not an error");
    assert_eq!(ini.sections(), Vec::<String>::new());

    // ...and a new one is written with a BOM, which is what makes a Tera Term
    // on Windows read it as UTF-8 rather than in its ANSI codepage.
    assert!(ini.set("Tera Term", "VTFlag", "1"));
    ini.save(&path).expect("save");
    let written = std::fs::read(&path).expect("read back");
    assert!(written.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(
        Ini::load(&path).expect("reload").get("Tera Term", "VTFlag"),
        Some("1")
    );

    // Nothing left behind from the temporary the save wrote through.
    let strays: Vec<_> = std::fs::read_dir(&dir)
        .expect("list")
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .filter(|n| n != "TERATERM.INI")
        .collect();
    assert!(strays.is_empty(), "left behind {strays:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_value_with_a_line_ending_is_refused_rather_than_written() {
    // Win32 writes it raw and splits the file in two, leaving a fragment
    // behind as a bogus key. There is no way to spell this that survives.
    let mut ini = Ini::parse(b"[s]\r\nA=1\r\n");
    assert!(!ini.set("s", "B", "x\r\ny"));
    assert!(!ini.set("s", "B", "x\ny"));
    assert_eq!(ini.to_bytes(), b"[s]\r\nA=1\r\n");
}
