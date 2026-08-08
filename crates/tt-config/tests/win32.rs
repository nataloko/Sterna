//! The same battery a real Win32 answered, put to this implementation.
//!
//! `ini-audit/cases.txt` is the questions and `ini-audit/win32.txt` is what
//! `GetPrivateProfile*` said to them, recorded under Wine. This replays every
//! one and diffs, which is the only way to hold "bug-compatible" to anything.
//!
//! Deliberate differences live in `ini-audit/divergences.txt` with a reason
//! each, and the gate is **drift**: a case that starts agreeing fails just as
//! loudly as one that starts differing, so a stale entry cannot outlive the
//! decision it describes. Same shape as `esctest/expected` and the differential
//! suite's `xfail` files.
//!
//! Nothing here needs Wine, mingw or a network — the record is committed. Wine
//! is only needed to *re-record* it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tt_config::Ini;

fn audit_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ini-audit")
}

/// `\r \n \t \0 \\ \xHH`, the same escaping the battery is written in.
fn unescape(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            b'r' => out.push(b'\r'),
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'0' => out.push(0),
            b'\\' => out.push(b'\\'),
            b'x' => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).expect("ascii");
                out.push(u8::from_str_radix(hex, 16).expect("hex"));
                i += 2;
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

fn escape(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &c in bytes {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            0 => out.push_str("\\0"),
            0x20..=0x7e => out.push(c as char),
            _ => write!(out, "\\x{c:02x}").expect("string"),
        }
    }
    out
}

fn arg(s: &str) -> String {
    if s == "@empty" {
        return String::new();
    }
    String::from_utf8_lossy(&unescape(s)).into_owned()
}

/// Win32 hands a key or section list back as one double-NUL-terminated block,
/// and an empty list as nothing at all rather than as a lone NUL.
fn name_block(names: &[String]) -> String {
    if names.is_empty() {
        return "len=0 str=".to_string();
    }
    let mut block = String::new();
    for name in names {
        block.push_str(name);
        block.push('\0');
    }
    // The block ends with a second NUL, but the returned length stops before
    // it — and a caller only ever reads that many characters, which is what
    // the Win32 exerciser printed.
    let len = block.len();
    format!("len={} str={}", len, escape(block.as_bytes()))
}

fn answer(fields: &[&str]) -> String {
    let bytes = if fields[1] == "@empty" {
        Vec::new()
    } else {
        unescape(fields[1])
    };
    let mut ini = Ini::parse(&bytes);
    let op = fields[2];

    match op {
        "get" => {
            let value = ini
                .get(&arg(fields[3]), &arg(fields[4]))
                .map(str::to_string)
                .unwrap_or_else(|| arg(fields[5]));
            format!("len={} str={}", value.chars().count(), escape(value.as_bytes()))
        }
        "int" => {
            let default: i32 = arg(fields[5]).parse().expect("a number");
            format!("{}", ini.get_int(&arg(fields[3]), &arg(fields[4]), default))
        }
        "keys" => name_block(&ini.keys(&arg(fields[3]))),
        "sections" => name_block(&ini.sections()),
        "truncate" => {
            let value = ini
                .get(&arg(fields[3]), &arg(fields[4]))
                .map(str::to_string)
                .unwrap_or_else(|| arg(fields[5]));
            // A buffer of `n` wide characters holds `n - 1` and a NUL.
            let size: usize = fields[6].parse().expect("a number");
            let cut: String = value.chars().take(size.saturating_sub(1)).collect();
            format!("len={} str={}", cut.chars().count(), escape(cut.as_bytes()))
        }
        "write" => {
            let section = arg(fields[3]);
            let ok = match (fields[4], fields[5]) {
                ("@null", _) => {
                    ini.remove_section(&section);
                    true
                }
                (key, "@null") => {
                    ini.remove(&section, &arg(key));
                    true
                }
                (key, value) => ini.set(&section, &arg(key), &arg(value)),
            };
            format!("ok={} file={}", ok as u8, escape(&ini.to_bytes()))
        }
        other => format!("<unknown op {other}>"),
    }
}

/// `name | reason` — the reason is prose and only has to be readable.
fn divergences() -> HashMap<String, String> {
    let path = audit_dir().join("divergences.txt");
    let text = std::fs::read_to_string(path).expect("divergences.txt is readable");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (name, reason) = l.split_once('|').expect("name | reason");
            (name.trim().to_string(), reason.trim().to_string())
        })
        .collect()
}

#[test]
fn every_answer_matches_what_win32_gave() {
    let cases = std::fs::read_to_string(audit_dir().join("cases.txt")).expect("cases.txt");
    let recorded = std::fs::read_to_string(audit_dir().join("win32.txt")).expect("win32.txt");
    let expected: HashMap<&str, &str> = recorded
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .collect();
    let known = divergences();

    let mut ran = 0;
    let mut diverged = 0;
    let mut failures = Vec::new();

    for line in cases.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        assert!(fields.len() >= 3, "malformed case: {line}");
        let name = fields[0];
        let want = expected
            .get(name)
            .unwrap_or_else(|| panic!("{name} is in cases.txt but not in win32.txt"));

        ran += 1;
        let got = answer(&fields);
        match (got == *want, known.get(name)) {
            (true, None) => {}
            (false, Some(_)) => diverged += 1,
            (false, None) => failures.push(format!("{name}\n  win32: {want}\n  ours:  {got}")),
            (true, Some(reason)) => failures.push(format!(
                "{name} now agrees with win32, so the divergence is stale — remove it\n  reason was: {reason}\n  both:  {got}"
            )),
        }
    }

    assert!(
        ran >= 100,
        "only {ran} cases ran — cases.txt looks truncated"
    );
    assert!(
        failures.is_empty(),
        "{} of {ran} cases disagree with the recorded Win32 answers \
         ({diverged} deliberate):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
