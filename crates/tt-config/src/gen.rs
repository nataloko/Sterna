//! Turn `schema/settings.txt` into `src/generated.rs`.
//!
//! ```sh
//! cargo run -p tt-config --bin gen-settings         # rewrite it
//! cargo run -p tt-config --bin gen-settings -- --check   # ...or just check
//! ```
//!
//! The output is **committed**, and `tests/settings.rs` fails when it is stale
//! — the same arrangement as `tt-ffi`'s header, and for the same reason. Two
//! build systems have to see this file (Cargo, and eventually CMake for the
//! dialog), and wiring a generator into both is how `PLAN.md`'s risk 5 starts.
//! Generating on demand and reviewing the diff keeps the build boring and puts
//! a schema change in front of a human once.
//!
//! This is a module rather than a binary's private code so that the staleness
//! test can call it directly. Shelling out to a nested `cargo run` from inside
//! a test is the package-cache-lock trap `tt-ffi/build.rs` already documents.

use std::fmt::Write as _;

/// One row of the schema, with the comment lines above it as its docs.
struct Setting {
    name: String,
    kind: Kind,
    section: String,
    /// The key upstream reads. Backtick quoting in the schema preserves
    /// leading or trailing whitespace for the one reader whose literal has
    /// it (`CygwinDirectory `).
    key: String,
    /// The key upstream writes when it differs from the reader's spelling.
    /// Almost always identical to `key`.
    write_key: String,
    /// The INI's own spelling of the default, kept verbatim so the writer and
    /// the reader agree about it without a second conversion.
    default: String,
    label: String,
    /// A boolean setting whose true arm permits this key to be written. Most
    /// keys are unconditional; `VTPos` is not written when `SaveVTWinPos` is
    /// off, and preserving an existing line there is part of round-tripping a
    /// real file rather than cosmetic output.
    write_if: Option<String>,
    /// Another integer setting whose loaded value is this setting's fallback.
    /// `AlphaBlendActive` is read after `AlphaBlend` and passes the latter to
    /// `GetPrivateProfileInt`, so an absent or empty active value follows the
    /// inactive one rather than a constant. A non-numeric value remains zero,
    /// which is the Win32 integer parser's separate rule.
    default_from: Option<String>,
    doc: Vec<String>,
}

/// How upstream bounds an int on read. The two shapes are genuinely different
/// and the difference is visible in a file somebody already has.
enum Bound {
    /// `int(lo..hi)`, `ttset.c:615`: below `lo` takes the **default**, above
    /// `hi` takes `hi`. Not a clamp in both directions. `int(lo..)` is the
    /// same test with no ceiling, which is how `MaxBuffSize` is read
    /// (`ttset.c:1214`) — upstream caps that one against a compile-time
    /// constant in `buffer.c` rather than on the way in.
    Ranged(i32, i32),
    /// `int_default(lo..hi)`: outside either end takes the default. The two
    /// Unicode width settings accept only 1 or 2 (`ttset.c:1965`).
    Validated(i32, i32),
    /// `int_min(lo)`, `ttset.c:1822` onward: below `lo` takes `lo`. A real
    /// clamp, and the transfer timeouts are the only settings read this way —
    /// `XmodemTimeouts=0,0,0,0,0` is five one-second timeouts rather than
    /// upstream's `10,3,10,20,60`.
    Floor(i32),
    /// `int_clamp(lo..hi)`, `ttset.c:1633`: `min(max(lo, v), hi)`, which is a
    /// clamp at *both* ends. `PasteDelayPerLine` is the only setting written
    /// this way, and it is neither of the two above — `Ranged` would give the
    /// default for a negative value where upstream gives the floor, and
    /// `Floor` would leave a `PasteDelayPerLine=60000` at a minute a line.
    Clamped(i32, i32),
    /// `uint16`: assignment to a Win32 `WORD`, which wraps modulo 65536.
    Word,
    /// `uint16_clamp(lo..hi)`, `ttset.c:1218`: the same narrowing followed by
    /// [`Bound::Clamped`]'s test. `MaxComPort` is the only setting read this
    /// way and it needs both halves — the narrowing turns `-1` into 65535 and
    /// the clamp then gives 4096, where the clamp alone would give 4.
    WordClamped(i32, i32),
    /// `int_alias(spelling=value)`: recognise one non-numeric spelling before
    /// using the ordinary integer parser. `MaximizedBugTweak=on` means 2.
    Alias(String, i32),
    /// `uint16_alias(spelling=value)`: the same alias rule, followed by the
    /// narrowing assignment to a Win32 `WORD`.
    WordAlias(String, i32),
}

/// What a missing comma-separated field becomes when the key itself exists.
enum FieldFallback {
    /// `GetNthNum2`: the field's own default, used by transfer timeouts.
    Default,
    /// `GetNthNum`: zero, used by geometry and sizes.
    Zero,
}

enum Kind {
    Bool,
    /// `Key` or `Key.N`, the latter being the Nth comma-separated field of a
    /// value that holds several — `TerminalSize` is `80,24`.
    Int {
        field: Option<usize>,
        bound: Option<Bound>,
        fallback: FieldFallback,
    },
    Str,
    /// `spelling => Variant`, in the order they were written, and whether the
    /// comparison is case-sensitive. Almost every enumerated setting upstream
    /// is read with `_stricmp`; `TerminalID` alone uses `strcmp`.
    ///
    /// A variant may have several spellings, written `hard/rtscts=Hardware`.
    /// The first is what gets written back and what a dialog offers; the rest
    /// are read-only aliases, because upstream's tables have them — `rtscts`
    /// and `hard` are one flow-control value under two names (`ttset.c:111`),
    /// and a file that says `rtscts` has to keep meaning what the user's own
    /// Tera Term makes of it.
    ///
    /// One spelling is special. **`*` is the `else` branch**, and it is not the
    /// same thing as the default: upstream reads most of these with
    /// `GetPrivateProfileString(…, "<the default spelling>", …)` and then runs
    /// a chain of `_stricmp`s whose last arm catches everything, so an *absent*
    /// key takes the default and a *misspelt* value takes the `else` — and for
    /// `AcceptTitleChangeRequest` those are two different settings
    /// (`ttset.c:1568`: absent is `overwrite`, misspelt is **off**). Written
    /// `off/*=Off`, alongside the real spelling that variant writes back.
    Enum {
        variants: Vec<(Vec<String>, String)>,
        /// The `*` variant, if the schema named one. `from_ini` returns it
        /// instead of the default for anything it does not recognise.
        fallback: Option<String>,
        exact: bool,
    },
    /// Two RGB triples in one value.
    Color2,
}

impl Kind {
    fn rust_type(&self, setting: &Setting) -> String {
        match self {
            Kind::Bool => "bool".into(),
            Kind::Int { .. } => "i32".into(),
            Kind::Str => "String".into(),
            Kind::Enum { .. } => type_name(&setting.name),
            Kind::Color2 => "[u8; 6]".into(),
        }
    }
}

/// `terminal.cr_receive` → `TerminalCrReceive`, for the generated enum.
fn type_name(name: &str) -> String {
    name.split(['.', '_'])
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// `terminal.cr_receive` → `terminal_cr_receive`, for the struct field.
fn field_name(name: &str) -> String {
    name.replace('.', "_")
}

fn parse(text: &str) -> Vec<Setting> {
    let mut out = Vec::new();
    let mut doc = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            // A run of comments directly above a setting is its documentation.
            // A run followed by a blank line is a section heading for humans.
            doc.push(rest.trim().to_string());
            continue;
        }
        if trimmed.is_empty() {
            doc.clear();
            continue;
        }
        // The first four fields from the left and the label from the right,
        // because the *default* is allowed to contain a `|` and one of them
        // does: `DelimList` holds every ASCII punctuation mark.
        let mut head = trimmed.splitn(5, '|');
        let f: Vec<&str> = head.by_ref().take(4).map(str::trim).collect();
        let rest = head.next().unwrap_or("");
        // Optional fields are recognisable from their prefixes, so a default
        // remains free to contain `|` — `DelimList` uses one. Peel only known
        // suffixes and leave everything else as part of the default.
        let mut rest = rest;
        let mut write_if = None;
        let mut default_from = None;
        let mut write_key = None;
        while let Some((before, tail)) = rest.rsplit_once('|') {
            let tail = tail.trim();
            if let Some(value) = tail.strip_prefix("write-if=") {
                assert!(write_if.is_none(), "two write-if options: {trimmed}");
                write_if = Some(value.trim().to_string());
                rest = before;
            } else if let Some(value) = tail.strip_prefix("default-from=") {
                assert!(
                    default_from.is_none(),
                    "two default-from options: {trimmed}"
                );
                default_from = Some(value.trim().to_string());
                rest = before;
            } else if let Some(value) = tail.strip_prefix("write-key=") {
                assert!(write_key.is_none(), "two write-key options: {trimmed}");
                write_key = Some(value.trim().to_string());
                rest = before;
            } else {
                break;
            }
        }
        let (default, label) = rest
            .rsplit_once('|')
            .unwrap_or_else(|| panic!("want 6 fields: {trimmed}"));
        assert!(f.len() == 4, "want 6 fields: {trimmed}");

        let key = match f[3].strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
            Some(quoted) => quoted.to_string(),
            None => f[3].to_string(),
        };
        let (key, kind) = parse_kind(f[1], &key);
        let write_key = write_key.unwrap_or_else(|| key.clone());
        out.push(Setting {
            name: f[0].to_string(),
            kind,
            section: f[2].to_string(),
            key,
            write_key,
            default: default.trim().to_string(),
            label: label.trim().to_string(),
            write_if,
            default_from,
            doc: std::mem::take(&mut doc),
        });
    }

    for setting in &out {
        let Some(condition) = &setting.write_if else {
            continue;
        };
        let referenced = out
            .iter()
            .find(|candidate| candidate.name == *condition)
            .unwrap_or_else(|| panic!("{}: no write condition named {condition}", setting.name));
        assert!(
            matches!(&referenced.kind, Kind::Bool),
            "{}: write condition {condition} is not a bool",
            setting.name
        );
    }
    for setting in &out {
        let Some(source) = &setting.default_from else {
            continue;
        };
        let referenced = out
            .iter()
            .find(|candidate| candidate.name == *source)
            .unwrap_or_else(|| panic!("{}: no default source named {source}", setting.name));
        assert!(
            matches!(
                (&setting.kind, &referenced.kind),
                (Kind::Int { field: None, .. }, Kind::Int { field: None, .. })
            ),
            "{}: default source {source} is not a scalar int",
            setting.name
        );
    }
    out
}

fn parse_kind(spec: &str, key: &str) -> (String, Kind) {
    for (prefix, exact) in [("enum(", false), ("enum_exact(", true)] {
        let Some(body) = spec.strip_prefix(prefix).and_then(|s| s.strip_suffix(')')) else {
            continue;
        };
        let mut fallback = None;
        let variants = body
            .split(',')
            .map(|pair| {
                let (spelling, variant) = pair.split_once('=').expect("spelling=Variant");
                let variant = variant.trim().to_string();
                let mut spellings: Vec<String> =
                    spelling.split('/').map(|s| s.trim().to_string()).collect();
                // `*` is the `else` arm rather than a string anyone can write,
                // so it comes out of the list the matcher and the writer see.
                if let Some(at) = spellings.iter().position(|s| s == "*") {
                    spellings.remove(at);
                    assert!(
                        !spellings.is_empty(),
                        "{key}: `*` marks the else branch of a variant that also \
                         has a spelling to write back, so it cannot be the only one"
                    );
                    assert!(fallback.is_none(), "{key}: two else branches");
                    fallback = Some(variant.clone());
                }
                (spellings, variant)
            })
            .collect();
        return (
            key.to_string(),
            Kind::Enum {
                variants,
                fallback,
                exact,
            },
        );
    }
    // `int`, `int_zero`, their ranged forms, `int_min(floor)`,
    // `int_clamp(min..max)` and its wrapping `uint16_clamp` counterpart,
    // `int_alias(spelling=value)`, its wrapping `uint16_alias` counterpart or a
    // plain wrapping `uint16`.
    let (spec, bound, fallback) = if let Some((body, word)) = spec
        .strip_prefix("int_clamp(")
        .and_then(|s| s.strip_suffix(')'))
        .map(|body| (body, false))
        .or_else(|| {
            spec.strip_prefix("uint16_clamp(")
                .and_then(|s| s.strip_suffix(')'))
                .map(|body| (body, true))
        }) {
        let (lo, hi) = body.split_once("..").expect("a range is `min..max`");
        let (lo, hi) = (
            lo.trim().parse::<i32>().expect("a number"),
            hi.trim().parse::<i32>().expect("a number"),
        );
        (
            "int",
            Some(if word {
                Bound::WordClamped(lo, hi)
            } else {
                Bound::Clamped(lo, hi)
            }),
            FieldFallback::Default,
        )
    } else if let Some(body) = spec
        .strip_prefix("int_default(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (lo, hi) = body.split_once("..").expect("a range is `min..max`");
        (
            "int",
            Some(Bound::Validated(
                lo.trim().parse::<i32>().expect("a number"),
                hi.trim().parse::<i32>().expect("a number"),
            )),
            FieldFallback::Default,
        )
    } else if let Some(body) = spec
        .strip_prefix("int_zero(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (lo, hi) = body.split_once("..").expect("a range is `min..max`");
        let hi = hi.trim();
        (
            "int",
            Some(Bound::Ranged(
                lo.trim().parse::<i32>().expect("a number"),
                if hi.is_empty() {
                    i32::MAX
                } else {
                    hi.parse::<i32>().expect("a number")
                },
            )),
            FieldFallback::Zero,
        )
    } else if let Some(body) = spec.strip_prefix("int(").and_then(|s| s.strip_suffix(')')) {
        let (lo, hi) = body.split_once("..").expect("a range is `min..max`");
        let hi = hi.trim();
        let bound = Bound::Ranged(
            lo.trim().parse::<i32>().expect("a number"),
            // `int(lo..)` has no ceiling. Reading it as `i32::MAX` rather
            // than as a second shape of bound keeps one code path: the
            // comparison is then never true, which is what "no ceiling"
            // means.
            if hi.is_empty() {
                i32::MAX
            } else {
                hi.parse::<i32>().expect("a number")
            },
        );
        ("int", Some(bound), FieldFallback::Default)
    } else if let Some(body) = spec
        .strip_prefix("int_min(")
        .and_then(|s| s.strip_suffix(')'))
    {
        (
            "int",
            Some(Bound::Floor(body.trim().parse::<i32>().expect("a number"))),
            FieldFallback::Default,
        )
    } else if let Some((body, word)) = spec
        .strip_prefix("int_alias(")
        .and_then(|s| s.strip_suffix(')'))
        .map(|body| (body, false))
        .or_else(|| {
            spec.strip_prefix("uint16_alias(")
                .and_then(|s| s.strip_suffix(')'))
                .map(|body| (body, true))
        })
    {
        let (alias, value) = body
            .split_once('=')
            .expect("an integer alias is `spelling=value`");
        (
            "int",
            Some(if word {
                Bound::WordAlias(
                    alias.trim().to_string(),
                    value.trim().parse::<i32>().expect("a number"),
                )
            } else {
                Bound::Alias(
                    alias.trim().to_string(),
                    value.trim().parse::<i32>().expect("a number"),
                )
            }),
            FieldFallback::Default,
        )
    } else if spec == "uint16" {
        ("int", Some(Bound::Word), FieldFallback::Default)
    } else if spec == "int_zero" {
        ("int", None, FieldFallback::Zero)
    } else {
        (spec, None, FieldFallback::Default)
    };
    match spec {
        "bool" => (key.to_string(), Kind::Bool),
        "string" => (key.to_string(), Kind::Str),
        "color2" => (key.to_string(), Kind::Color2),
        "int" => match key.rsplit_once('.') {
            Some((base, n)) if n.chars().all(|c| c.is_ascii_digit()) => (
                base.to_string(),
                Kind::Int {
                    field: Some(n.parse::<usize>().expect("a digit") - 1),
                    bound,
                    fallback,
                },
            ),
            _ => (
                key.to_string(),
                Kind::Int {
                    field: None,
                    bound,
                    fallback,
                },
            ),
        },
        other => panic!("unknown type {other}"),
    }
}

fn emit(settings: &[Setting]) -> String {
    let mut out = String::new();
    out.push_str(
        "// Generated from `schema/settings.txt` by `src/bin/gen-settings.rs`.\n\
         // Do not edit: change the schema and re-run the generator.\n\
         //\n\
         // Committed rather than built, so that a schema change is a reviewable\n\
         // diff and neither build system has to run a generator. `tests/generated.rs`\n\
         // fails when this file is stale.\n\
         \n\
         #![allow(clippy::all)]\n\
         \n\
         use crate::ini::Ini;\n\
         use crate::schema::{Field, Kind};\n\n",
    );

    // One enum per enumerated setting. Generated rather than shared with
    // `tt-vt`'s: this crate deliberately does not depend on the terminal, so
    // the schema stays a description of a *file* and the wiring that maps it
    // onto a running terminal lives one layer up.
    for s in settings {
        let Kind::Enum {
            variants,
            fallback,
            exact,
        } = &s.kind
        else {
            continue;
        };
        for line in &s.doc {
            writeln!(out, "/// {line}").expect("string");
        }
        writeln!(out, "#[derive(Clone, Copy, Debug, PartialEq, Eq)]").expect("string");
        writeln!(out, "pub enum {} {{", type_name(&s.name)).expect("string");
        for (spellings, variant) in variants {
            let list: Vec<String> = spellings.iter().map(|s| format!("`{s}`")).collect();
            match list.len() {
                1 => writeln!(out, "    /// {}", list[0]).expect("string"),
                _ => writeln!(
                    out,
                    "    /// {} — the first is written back, the rest are aliases the\n\
                     \x20   /// file may hold because upstream's own table has them.",
                    list.join(", ")
                )
                .expect("string"),
            }
            writeln!(out, "    {variant},").expect("string");
        }
        out.push_str("}\n\n");

        writeln!(out, "impl {} {{", type_name(&s.name)).expect("string");
        out.push_str("    /// The INI's own spelling, which is what gets written back.\n");
        out.push_str("    pub fn as_ini(&self) -> &'static str {\n        match self {\n");
        for (spellings, variant) in variants {
            writeln!(
                out,
                "            Self::{variant} => \"{}\",",
                escape(&spellings[0])
            )
            .expect("string");
        }
        out.push_str("        }\n    }\n\n");
        match (fallback, exact) {
            (Some(v), _) => writeln!(
                out,
                "    /// Case-insensitive, and **anything unrecognised is `{v}`** — which\n\
                 \x20   /// is *not* this type's default. Upstream reads the key with a\n\
                 \x20   /// default string and then runs a chain of comparisons whose last\n\
                 \x20   /// arm catches everything, so an absent key and a misspelt value\n\
                 \x20   /// are two different settings."
            )
            .expect("string"),
            (None, true) => out.push_str(
                "    /// Case-**sensitive**, because upstream compares this one with\n\
                 \x20   /// `strcmp` rather than `_stricmp` — and **anything unrecognised\n\
                 \x20   /// takes the default** rather than failing, so a lower-case\n\
                 \x20   /// spelling silently reads as that default.\n",
            ),
            (None, false) => out.push_str(
                "    /// Case-insensitive, and **anything unrecognised takes the default**\n\
                 \x20   /// rather than failing — which is how upstream spells most of its\n\
                 \x20   /// defaults, as the `else` branch of a chain of comparisons.\n",
            ),
        }
        writeln!(out, "    pub fn from_ini(s: &str) -> Self {{").expect("string");
        out.push_str("        let s = s.trim();\n");
        for (spellings, variant) in variants {
            let tests: Vec<String> = spellings
                .iter()
                .map(|spelling| match exact {
                    true => format!("s == \"{}\"", escape(spelling)),
                    false => format!("s.eq_ignore_ascii_case(\"{}\")", escape(spelling)),
                })
                .collect();
            writeln!(
                out,
                "        if {} {{ return Self::{variant}; }}",
                tests.join(" || ")
            )
            .expect("string");
        }
        match fallback {
            Some(v) => writeln!(out, "        Self::{v}").expect("string"),
            None => writeln!(out, "        Self::default()").expect("string"),
        }
        out.push_str("    }\n}\n\n");

        let default_variant = variants
            .iter()
            .find(|(spellings, _)| {
                spellings.iter().any(|spelling| match exact {
                    true => *spelling == s.default,
                    false => spelling.eq_ignore_ascii_case(&s.default),
                })
            })
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{}: default {} is not a spelling", s.name, s.default));
        writeln!(out, "impl Default for {} {{", type_name(&s.name)).expect("string");
        writeln!(
            out,
            "    fn default() -> Self {{ Self::{default_variant} }}\n}}\n"
        )
        .expect("string");
    }

    // The struct.
    out.push_str(
        "/// Every setting this project reads out of `TERATERM.INI`.\n\
         ///\n\
         /// Generated from the schema, so the field, its default, its INI key and\n\
         /// the citation for that default are one thing rather than four that can\n\
         /// disagree.\n\
         #[derive(Clone, Debug, PartialEq)]\n\
         pub struct Settings {\n",
    );
    for s in settings {
        for line in &s.doc {
            writeln!(out, "    /// {line}").expect("string");
        }
        writeln!(
            out,
            "    pub {}: {},",
            field_name(&s.name),
            s.kind.rust_type(s)
        )
        .expect("string");
    }
    out.push_str("}\n\n");

    // Defaults.
    out.push_str("impl Default for Settings {\n    fn default() -> Self {\n        Settings {\n");
    for s in settings {
        let field = field_name(&s.name);
        let value = match &s.kind {
            Kind::Bool => on_off_literal(&s.default).to_string(),
            Kind::Int { .. } => format!("{}", s.default.parse::<i32>().expect("a number")),
            Kind::Str => format!("String::from(\"{}\")", escape(&s.default)),
            Kind::Enum { .. } => format!("{}::default()", type_name(&s.name)),
            Kind::Color2 => color2_literal(&s.default),
        };
        writeln!(out, "            {field}: {value},").expect("string");
    }
    out.push_str("        }\n    }\n}\n\n");

    // Reading and writing.
    out.push_str(
        "impl Settings {\n\
         \x20   /// Read every setting, taking the default for anything absent.\n\
         \x20   pub fn load(ini: &Ini) -> Settings {\n\
         \x20       let d = Settings::default();\n\
         \x20       let mut settings = Settings {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let (section, key) = (escape(&s.section), escape(&s.key));
        let expr = match &s.kind {
            Kind::Bool => format!(
                "crate::schema::on_off(ini.get(\"{section}\", \"{key}\"), {})",
                on_off_literal(&s.default)
            ),
            Kind::Int {
                field: nth,
                bound,
                fallback,
            } => {
                let read = match (nth, bound) {
                    (None, Some(Bound::Alias(alias, value))) => format!(
                        "crate::schema::int_alias(ini.get(\"{section}\", \"{key}\"), d.{field}, \"{}\", {value})",
                        escape(alias)
                    ),
                    (None, Some(Bound::WordAlias(alias, value))) => format!(
                        "crate::schema::word_alias(ini.get(\"{section}\", \"{key}\"), d.{field}, \"{}\", {value})",
                        escape(alias)
                    ),
                    (None, _) => {
                        format!("ini.get_int(\"{section}\", \"{key}\", d.{field}) as i32")
                    }
                    (Some(_), Some(Bound::Alias(_, _) | Bound::WordAlias(_, _))) => {
                        panic!("{field}: an integer alias cannot be one field of a list")
                    }
                    (Some(n), _) => match fallback {
                        FieldFallback::Default => format!(
                            "crate::schema::nth_int(ini.get(\"{section}\", \"{key}\"), {n}, d.{field})"
                        ),
                        FieldFallback::Zero => format!(
                            "crate::schema::nth_int_zero(ini.get(\"{section}\", \"{key}\"), {n}, d.{field})"
                        ),
                    },
                };
                match bound {
                    None => read,
                    Some(Bound::Ranged(lo, hi)) => {
                        format!("crate::schema::ranged({read}, d.{field}, {lo}, {hi})")
                    }
                    Some(Bound::Validated(lo, hi)) => {
                        format!("crate::schema::validated({read}, d.{field}, {lo}, {hi})")
                    }
                    Some(Bound::Floor(lo)) => format!("crate::schema::floored({read}, {lo})"),
                    Some(Bound::Clamped(lo, hi)) => {
                        format!("crate::schema::clamped({read}, {lo}, {hi})")
                    }
                    Some(Bound::Word) => format!("crate::schema::word({read})"),
                    Some(Bound::WordClamped(lo, hi)) => {
                        format!("crate::schema::word_clamped({read}, {lo}, {hi})")
                    }
                    Some(Bound::Alias(_, _)) => read,
                    Some(Bound::WordAlias(_, _)) => read,
                }
            }
            Kind::Str => format!("ini.get_or(\"{section}\", \"{key}\", &d.{field}).to_string()"),
            Kind::Enum { .. } => format!(
                "match ini.get(\"{section}\", \"{key}\") {{ \
                 Some(v) => {}::from_ini(v), None => d.{field} }}",
                type_name(&s.name)
            ),
            Kind::Color2 => {
                format!("crate::schema::color2(ini.get(\"{section}\", \"{key}\"), d.{field})")
            }
        };
        writeln!(out, "            {field}: {expr},").expect("string");
    }
    out.push_str("        };\n");
    for s in settings {
        let Some(source) = &s.default_from else {
            continue;
        };
        let field = field_name(&s.name);
        let source = field_name(source);
        let (section, key) = (escape(&s.section), escape(&s.key));
        let Kind::Int { bound, .. } = &s.kind else {
            unreachable!("default-from was validated as an int")
        };
        let read = format!("ini.get_int(\"{section}\", \"{key}\", settings.{source}) as i32");
        let expr = match bound {
            None => read,
            Some(Bound::Ranged(lo, hi)) => {
                format!("crate::schema::ranged({read}, settings.{source}, {lo}, {hi})")
            }
            Some(Bound::Validated(lo, hi)) => {
                format!("crate::schema::validated({read}, settings.{source}, {lo}, {hi})")
            }
            Some(Bound::Floor(lo)) => format!("crate::schema::floored({read}, {lo})"),
            Some(Bound::Clamped(lo, hi)) => {
                format!("crate::schema::clamped({read}, {lo}, {hi})")
            }
            Some(Bound::Word) => format!("crate::schema::word({read})"),
            Some(Bound::WordClamped(lo, hi)) => {
                format!("crate::schema::word_clamped({read}, {lo}, {hi})")
            }
            Some(Bound::Alias(alias, aliased)) => format!(
                "crate::schema::int_alias(ini.get(\"{section}\", \"{key}\"), settings.{source}, \"{}\", {aliased})",
                escape(alias)
            ),
            Some(Bound::WordAlias(alias, aliased)) => format!(
                "crate::schema::word_alias(ini.get(\"{section}\", \"{key}\"), settings.{source}, \"{}\", {aliased})",
                escape(alias)
            ),
        };
        writeln!(out, "        settings.{field} = {expr};").expect("string");
    }
    out.push_str("        settings.normalize();\n        settings\n    }\n\n");

    out.push_str(
        "    /// Write every setting back, leaving the rest of the file alone.\n\
         \x20   pub fn store(&self, ini: &mut Ini) {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let (section, key) = (escape(&s.section), escape(&s.write_key));
        let expr = match &s.kind {
            Kind::Bool => format!("if self.{field} {{ \"on\" }} else {{ \"off\" }}.to_string()"),
            Kind::Int { field: None, .. } => format!("self.{field}.to_string()"),
            Kind::Int { field: Some(n), .. } => format!(
                "crate::schema::with_nth(ini.get(\"{section}\", \"{key}\"), {n}, self.{field})"
            ),
            Kind::Str => format!("self.{field}.clone()"),
            Kind::Enum { .. } => format!("self.{field}.as_ini().to_string()"),
            Kind::Color2 => format!("crate::schema::color2_str(&self.{field})"),
        };
        let indent = if let Some(condition) = &s.write_if {
            writeln!(out, "        if self.{} {{", field_name(condition)).expect("string");
            "            "
        } else {
            "        "
        };
        writeln!(out, "{indent}ini.set(\"{section}\", \"{key}\", &{expr});").expect("string");
        if s.write_if.is_some() {
            out.push_str("        }\n");
        }
    }
    out.push_str("    }\n\n");

    // Name-addressed access, which is what a generic dialog and the scripting
    // commands walk instead of each holding their own copy of this list.
    out.push_str(
        "    /// One setting by its dotted name, in the INI's own spelling.\n\
         \x20   pub fn get_str(&self, name: &str) -> Option<String> {\n\
         \x20       Some(match name {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let expr = match &s.kind {
            Kind::Bool => format!("if self.{field} {{ \"on\" }} else {{ \"off\" }}.to_string()"),
            Kind::Int { .. } => format!("self.{field}.to_string()"),
            Kind::Str => format!("self.{field}.clone()"),
            Kind::Enum { .. } => format!("self.{field}.as_ini().to_string()"),
            Kind::Color2 => format!("crate::schema::color2_str(&self.{field})"),
        };
        writeln!(out, "            \"{}\" => {expr},", escape(&s.name)).expect("string");
    }
    out.push_str("            _ => return None,\n        })\n    }\n\n");

    out.push_str(
        "    /// Set one setting by name, parsed the way the file would be.\n\
         \x20   /// False when the name is not one of ours.\n\
         \x20   pub fn set_str(&mut self, name: &str, value: &str) -> bool {\n\
         \x20       match name {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let expr = match &s.kind {
            Kind::Bool => format!(
                "self.{field} = crate::schema::on_off(Some(value), {})",
                on_off_literal(&s.default)
            ),
            Kind::Int { bound, .. } => {
                let read = format!("crate::schema::int(value, self.{field})");
                match bound {
                    None => format!("self.{field} = {read}"),
                    // The same rule the file gets: below the range takes the
                    // default, above it takes the ceiling. A script and a
                    // hand-edited INI must not disagree about a value.
                    Some(Bound::Ranged(lo, hi)) => format!(
                        "self.{field} = crate::schema::ranged({read}, {}, {lo}, {hi})",
                        s.default.parse::<i32>().expect("a number")
                    ),
                    Some(Bound::Validated(lo, hi)) => format!(
                        "self.{field} = crate::schema::validated({read}, {}, {lo}, {hi})",
                        s.default.parse::<i32>().expect("a number")
                    ),
                    Some(Bound::Floor(lo)) => {
                        format!("self.{field} = crate::schema::floored({read}, {lo})")
                    }
                    Some(Bound::Clamped(lo, hi)) => {
                        format!("self.{field} = crate::schema::clamped({read}, {lo}, {hi})")
                    }
                    Some(Bound::Word) => {
                        format!("self.{field} = crate::schema::word({read})")
                    }
                    Some(Bound::WordClamped(lo, hi)) => {
                        format!("self.{field} = crate::schema::word_clamped({read}, {lo}, {hi})")
                    }
                    Some(Bound::Alias(alias, aliased)) => format!(
                        "self.{field} = crate::schema::int_alias(Some(value), self.{field}, \"{}\", {aliased})",
                        escape(alias)
                    ),
                    Some(Bound::WordAlias(alias, aliased)) => format!(
                        "self.{field} = crate::schema::word_alias(Some(value), self.{field}, \"{}\", {aliased})",
                        escape(alias)
                    ),
                }
            }
            Kind::Str => format!("self.{field} = value.to_string()"),
            Kind::Enum { .. } => format!("self.{field} = {}::from_ini(value)", type_name(&s.name)),
            Kind::Color2 => {
                format!("self.{field} = crate::schema::color2(Some(value), self.{field})")
            }
        };
        writeln!(out, "            \"{}\" => {expr},", escape(&s.name)).expect("string");
    }
    out.push_str(
        "            _ => return false,\n        }\n        self.normalize();\n        true\n    }\n}\n\n",
    );

    // The metadata table.
    out.push_str(
        "/// Every setting, as data — for the dialog that builds itself from it,\n\
         /// for `setsetting`/`getsetting`, and for the documentation table.\n\
         ///\n\
         /// This is the point of the schema: the list exists once.\n\
         pub const FIELDS: &[Field] = &[\n",
    );
    for s in settings {
        let kind = match &s.kind {
            Kind::Bool => "Kind::Bool".to_string(),
            Kind::Int { bound: None, .. } => "Kind::Int".to_string(),
            // The dialog needs the bounds to build a spin box, and it must not
            // hold its own copy of them.
            Kind::Int {
                bound: Some(Bound::Ranged(lo, hi)),
                ..
            } => format!("Kind::IntRange({lo}, {hi})"),
            Kind::Int {
                bound: Some(Bound::Validated(lo, hi)),
                ..
            } => format!("Kind::IntRange({lo}, {hi})"),
            Kind::Int {
                bound: Some(Bound::Floor(lo)),
                ..
            } => format!("Kind::IntMin({lo})"),
            // The narrowing has nothing to say to a spin box, which cannot
            // produce a value outside the range in the first place — so a
            // wrapping clamp is the same control as a plain one.
            Kind::Int {
                bound: Some(Bound::Clamped(lo, hi) | Bound::WordClamped(lo, hi)),
                ..
            } => format!("Kind::IntClamp({lo}, {hi})"),
            Kind::Int {
                bound: Some(Bound::Word),
                ..
            } => "Kind::IntWord".to_string(),
            Kind::Int {
                bound: Some(Bound::Alias(_, _)),
                ..
            } => "Kind::Int".to_string(),
            Kind::Int {
                bound: Some(Bound::WordAlias(_, _)),
                ..
            } => "Kind::IntWord".to_string(),
            Kind::Str => "Kind::Str".to_string(),
            Kind::Color2 => "Kind::Color2".to_string(),
            Kind::Enum { variants, .. } => {
                // The canonical spelling only: a dialog offers one item per
                // value, and an alias is a second name for a value already
                // there rather than a choice of its own.
                let list: Vec<String> = variants
                    .iter()
                    .map(|(spellings, _)| format!("\"{}\"", escape(&spellings[0])))
                    .collect();
                format!("Kind::Enum(&[{}])", list.join(", "))
            }
        };
        let label = if s.label == "-" {
            "None".to_string()
        } else {
            format!("Some(\"{}\")", escape(&s.label))
        };
        let page = s.name.split('.').next().expect("a dotted name");
        writeln!(out, "    Field {{").expect("string");
        writeln!(out, "        name: \"{}\",", escape(&s.name)).expect("string");
        writeln!(out, "        page: \"{}\",", escape(page)).expect("string");
        writeln!(out, "        section: \"{}\",", escape(&s.section)).expect("string");
        writeln!(out, "        key: \"{}\",", escape(&s.key)).expect("string");
        writeln!(out, "        kind: {kind},").expect("string");
        writeln!(out, "        default: \"{}\",", escape(&s.default)).expect("string");
        writeln!(out, "        label: {label},").expect("string");
        writeln!(out, "        doc: \"{}\",", escape(&s.doc.join(" "))).expect("string");
        writeln!(out, "    }},").expect("string");
    }
    out.push_str("];\n");
    out
}

/// The default's own spelling decides how `GetOnOff` reads the file, so it is
/// carried through to the generated call rather than collapsed to a bool here.
fn on_off_literal(default: &str) -> &'static str {
    match default {
        "on" => "true",
        "off" => "false",
        other => panic!("a bool default is `on` or `off`, not {other}"),
    }
}

fn color2_literal(default: &str) -> String {
    let parts: Vec<&str> = default.split(',').map(str::trim).collect();
    assert_eq!(parts.len(), 6, "a colour pair is six numbers: {default}");
    let numbers: Vec<String> = parts
        .iter()
        .map(|p| p.parse::<u8>().expect("0..255").to_string())
        .collect();
    format!("[{}]", numbers.join(", "))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The whole of `src/generated.rs`, from the whole of `schema/settings.txt`.
pub fn generate(schema: &str) -> String {
    let settings = parse(schema);
    assert!(!settings.is_empty(), "the schema is empty");
    rustfmt(emit(&settings))
}

/// The last step, and it is not cosmetic.
///
/// `cargo fmt --check` covers every file in the workspace, this one included,
/// so a generated file rustfmt disagrees with makes the lint gate permanently
/// red — and the fix is not to reformat it by hand, because then the staleness
/// test fails instead and says "stale", which is the opposite of what happened.
///
/// The alternative is emitting code rustfmt would not touch, and that was
/// tried: the one-line `if` bodies are easy, but where a call or a match arm
/// wraps depends on the *width of a setting's name*, so the emitter would
/// silently start losing to the gate the first time somebody added a long one.
/// Formatting the output removes the whole class instead of chasing it.
///
/// Panics rather than falling back to the unformatted text, because the
/// fallback is worse than the failure: the staleness test would compare
/// unformatted output against the formatted file and report a schema that had
/// not changed as out of date.
fn rustfmt(src: String) -> String {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut child = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("rustfmt on PATH — `rustup component add rustfmt`");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(src.as_bytes())
        .expect("rustfmt takes the source");
    let out = child.wait_with_output().expect("rustfmt finishes");
    assert!(out.status.success(), "rustfmt rejected the generated code");
    String::from_utf8(out.stdout).expect("rustfmt writes UTF-8")
}

/// Where the two files live, so the binary and the test agree about it.
pub fn schema_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/settings.txt")
}

pub fn generated_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generated.rs")
}
