//! Every key in the schema, held against `ttset.c` itself.
//!
//! The schema is a transcription, and the way to check a transcription is to
//! extract both lists and diff them rather than to read them — the same rule
//! `CLAUDE.md` states for `CheckReservedWord`. It is worth having as a test
//! because a wrong key **cannot fail loudly**: reading one that upstream never
//! writes gives the default from a file that sets the setting, and writing it
//! puts a line in the user's `TERATERM.INI` that their own Tera Term ignores.
//! Four of the first 77 were invented this way — `AltScreenBuffer`,
//! `EnableUnderlineAttrColor`, `RemoteClearsBuffer` and `WindowChangeSequence`,
//! none of which appears anywhere in 157k lines of upstream — and one of the
//! four had the wrong default as well, which is the same accident twice.
//!
//! This needs the read-only reference checkout and skips without it, the way
//! `tt-ttl`'s script suite does.

use std::path::{Path, PathBuf};
use tt_config::{Kind, FIELDS};

/// `../teraterm` is the sibling read-only checkout; `TERATERM_SRC` overrides
/// the root of it.
fn ttset_c() -> Option<PathBuf> {
    let root = match std::env::var_os("TERATERM_SRC") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../teraterm"),
    };
    let f = root.join("teraterm/ttpset/ttset.c");
    f.is_file().then_some(f)
}

/// Is `key` read or written as a quoted INI key anywhere in `src`?
///
/// Deliberately a substring test for `"<key>"` rather than a parse of the call:
/// upstream reads some keys through `GetOnOff`, some through
/// `GetPrivateProfileInt`, some through `hGetPrivateProfileStringW` with a wide
/// literal, and some only in the *writer* — and the question here is whether
/// the string exists at all, not which arm it is in.
fn mentions(src: &str, key: &str) -> bool {
    src.contains(&format!("\"{key}\""))
}

#[test]
fn every_key_is_one_upstream_reads() {
    let Some(path) = ttset_c() else {
        eprintln!(
            "skipped: no ../teraterm — this needs the read-only reference \
             checkout, or TERATERM_SRC pointing at one"
        );
        return;
    };
    let src = std::fs::read_to_string(&path).unwrap();

    let mut invented = Vec::new();
    for f in FIELDS {
        // `TerminalSize.1` and `.2` are the schema's two halves of one key,
        // which is how a `cols,rows` pair becomes two settings a dialog can
        // show. Upstream spells it once.
        let key = f.key.split('.').next().unwrap();
        if !mentions(&src, key) {
            invented.push(format!("{} -> {}", f.name, f.key));
        }
    }
    assert!(
        invented.is_empty(),
        "these keys appear nowhere in ttset.c, so they read as the default \
         from a file that sets them:\n  {}",
        invented.join("\n  ")
    );

    // How far the transcription has got, printed rather than asserted: this
    // number only ever goes up, and a test that pinned it would fail on every
    // setting added. `PLAN.md`'s "the rest of the settings" is the difference.
    let ours: std::collections::BTreeSet<&str> = FIELDS
        .iter()
        .map(|f| f.key.split('.').next().unwrap())
        .collect();
    let theirs = upstream_keys(&src);
    eprintln!(
        "schema: {} settings over {} keys; ttset.c reads {}, so {} to go",
        FIELDS.len(),
        ours.len(),
        theirs.len(),
        theirs.difference(&ours).count()
    );
}

/// Every INI key `ttset.c` reads, however it reads it.
///
/// Four call shapes and two string widths, which is why this matches on the
/// *second* quoted argument of any of them rather than trying to parse the
/// call. Over-matching would only inflate the "to go" count it feeds.
fn upstream_keys(src: &str) -> std::collections::BTreeSet<&str> {
    let mut out = std::collections::BTreeSet::new();
    for call in [
        "GetPrivateProfileInt(Section, \"",
        "GetPrivateProfileString(Section, \"",
        "GetOnOff(Section, \"",
        "GetPrivateProfileColor2(Section, \"",
    ] {
        let mut rest = src;
        while let Some(at) = rest.find(call) {
            rest = &rest[at + call.len()..];
            if let Some(end) = rest.find('"') {
                out.insert(&rest[..end]);
            }
        }
    }
    out
}

#[test]
fn every_bool_default_is_the_one_get_on_off_was_given() {
    let Some(path) = ttset_c() else {
        eprintln!("skipped: no ../teraterm");
        return;
    };
    let src = std::fs::read_to_string(&path).unwrap();

    // `GetOnOff(Section, "Key", FName, TRUE)`. The default is the last
    // argument, and it is the whole of what the value in the file means:
    // with a default of on anything but `off` is on, with a default of off
    // only `on` is on (`ttset.c:344`). Getting it wrong inverts the setting
    // for every file that mentions it.
    let mut wrong = Vec::new();
    for f in FIELDS {
        if f.kind != Kind::Bool {
            continue;
        }
        let Some(upstream) = get_on_off_default(&src, f.key) else {
            // Not read through `GetOnOff` at all — either upstream builds it
            // some other way, or it is one of the settings this port added.
            // The key itself is covered by the test above.
            continue;
        };
        let ours = f.default == "on";
        if ours != upstream {
            wrong.push(format!(
                "{} ({}): schema says {}, ttset.c says {}",
                f.name,
                f.key,
                f.default,
                if upstream { "on" } else { "off" }
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "a `GetOnOff` default decides what a value in the file means:\n  {}",
        wrong.join("\n  ")
    );
}

/// The last argument of the `GetOnOff` call that reads `key`, if there is one.
fn get_on_off_default(src: &str, key: &str) -> Option<bool> {
    let needle = format!("GetOnOff(Section, \"{key}\", FName, ");
    let rest = &src[src.find(&needle)? + needle.len()..];
    let arg = rest.split(')').next()?.trim();
    match arg {
        "TRUE" => Some(true),
        "FALSE" => Some(false),
        _ => None,
    }
}
