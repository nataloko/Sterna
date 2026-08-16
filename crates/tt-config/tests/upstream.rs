//! Every key in the schema, held against `ttset.c` itself.
//!
//! The schema is a transcription, and the way to check a transcription is to
//! extract both lists and diff them rather than to read them — the same rule
//! `AGENTS.md` states for `CheckReservedWord`. It is worth having as a test
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
fn upstream_root() -> PathBuf {
    match std::env::var_os("TERATERM_SRC") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../teraterm"),
    }
}

fn ttset_c() -> Option<PathBuf> {
    let f = upstream_root().join("teraterm/ttpset/ttset.c");
    f.is_file().then_some(f)
}

/// Which upstream file reads a section's keys, or `None` for a section that is
/// this project's own.
///
/// `[Tera Term]` is `ttset.c` and the plugins read their own: `TTProxy` hooks
/// `ReadIniFile` (`TTProxy/TTProxy.h:63`), so its keys appear nowhere in
/// `ttset.c` and checking them against it would report every one of them as
/// invented. `[Sterna]` is the other direction — a section nothing upstream
/// reads at all, checked by [`every_sterna_key_is_this_projects_own`] instead.
/// A section that is in neither list is a section nobody has said where to
/// check, which is a schema mistake rather than a pass.
fn source_for(section: &str) -> Option<&'static str> {
    match section {
        "Tera Term" => Some("teraterm/ttpset/ttset.c"),
        "TTProxy" => Some("TTProxy/ProxyWSockHook.h"),
        "Sterna" => None,
        other => panic!("no upstream source is recorded for section [{other}]"),
    }
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

    let root = upstream_root();
    let mut sources: std::collections::BTreeMap<&str, String> = Default::default();
    let mut invented = Vec::new();
    for f in FIELDS {
        let Some(relative) = source_for(f.section) else {
            continue;
        };
        let source = sources.entry(relative).or_insert_with(|| {
            std::fs::read_to_string(root.join(relative))
                .unwrap_or_else(|e| panic!("cannot read {relative}: {e}"))
        });
        // `TerminalSize.1` and `.2` are the schema's two halves of one key,
        // which is how a `cols,rows` pair becomes two settings a dialog can
        // show. Upstream spells it once.
        let key = f.key.split('.').next().unwrap();
        if !mentions(source, key) {
            invented.push(format!("{} -> {} (in {relative})", f.name, f.key));
        }
    }
    assert!(
        invented.is_empty(),
        "these keys appear nowhere in the file that reads their section, so \
         they read as the default from a file that sets them:\n  {}",
        invented.join("\n  ")
    );

    // How far the transcription has got, printed rather than asserted: this
    // number only ever goes up, and a test that pinned it would fail on every
    // setting added. `PLAN.md`'s "the rest of the settings" is the difference.
    let ours: std::collections::BTreeSet<&str> = FIELDS
        .iter()
        .filter(|f| source_for(f.section).is_some())
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

/// `[Sterna]` holds what upstream has no key for, and nothing else.
///
/// The test above asks whether an upstream section's key really exists
/// upstream; this asks the same question backwards, because the two mistakes
/// are different. A setting put in `[Sterna]` that `ttset.c` also reads is one
/// this project owns a *second* copy of: the file then has two answers to the
/// same question, only one of which a real Tera Term can see. That is what
/// `docs/deviations.md` says the section is not for — the remembered serial
/// speed is `[Tera Term] BaudRate` for exactly this reason.
///
/// A name that merely reads like an upstream one is fine and expected
/// (`SshHost` is not `HostName`); it is the literal key spelling that must not
/// collide.
#[test]
fn every_sterna_key_is_this_projects_own() {
    let Some(path) = ttset_c() else {
        eprintln!("skipped: no ../teraterm");
        return;
    };
    let src = std::fs::read_to_string(&path).unwrap();

    let borrowed: Vec<String> = FIELDS
        .iter()
        .filter(|f| f.section == "Sterna")
        .filter(|f| mentions(&src, f.key.split('.').next().unwrap()))
        .map(|f| format!("{} -> {}", f.name, f.key))
        .collect();
    assert!(
        borrowed.is_empty(),
        "ttset.c reads these keys itself, so a copy of them in [Sterna] is a \
         second answer their own Tera Term cannot see:\n  {}",
        borrowed.join("\n  ")
    );
}

/// Every INI key `ttset.c` reads, however it reads it.
///
/// Seven call shapes and two string widths, which is why this matches on the
/// *second* quoted argument of any of them rather than trying to parse the
/// call. Over-matching would only inflate the "to go" count it feeds.
fn upstream_keys(src: &str) -> std::collections::BTreeSet<&str> {
    let mut out = std::collections::BTreeSet::new();
    for call in [
        "GetPrivateProfileInt(Section, \"",
        "GetPrivateProfileIntW(SectionW, L\"",
        "GetPrivateProfileString(Section, \"",
        "GetPrivateProfileStringW(SectionW, L\"",
        "hGetPrivateProfileStringW(SectionW, L\"",
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
fn upstream_key_extractor_includes_wide_reads() {
    let src = r#"
        GetPrivateProfileIntW(SectionW, L"WideInt", 0, FName);
        GetPrivateProfileStringW(SectionW, L"WideFixed", L"", out, 1, FName);
        hGetPrivateProfileStringW(SectionW, L"WideAllocated", L"", FName, &out);
    "#;
    assert_eq!(
        upstream_keys(src),
        ["WideAllocated", "WideFixed", "WideInt"]
            .into_iter()
            .collect()
    );
}

/// INI keys whose shipped default this port moves on purpose. Every entry
/// costs a `docs/deviations.md` row saying what moved and why, and leaves the
/// key itself working both ways — a deviation here changes what a *fresh*
/// install does, never what a file that names the key means.
const DEFAULTS_MOVED_ON_PURPOSE: &[&str] = &[
    // The right button raises upstream's own two-item paste menu instead of
    // putting the clipboard straight on the wire. Upstream ships the menu
    // behind this key and ships the key off; this port ships it on.
    "ConfirmPasteMouseRButton",
    // The terminal is the window on a fresh install, which is what this port
    // did unconditionally before the key was honoured at all. Upstream ships
    // off, and a window narrower than its terminal is then the ordinary state
    // there — it has had a horizontal scrollbar since before this port existed
    // (`vtwin.cpp:650`). Deviation 21.
    "TermIsWin",
];

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
        // Deliberate, listed in `docs/deviations.md`, and named here one key
        // at a time — a deviation that is not worth spelling out in this
        // array is not worth making, and a blanket escape would hide the
        // seventeen accidents this test was written to find.
        if DEFAULTS_MOVED_ON_PURPOSE.contains(&f.key) {
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
