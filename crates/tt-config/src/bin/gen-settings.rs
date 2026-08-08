//! Turn `schema/settings.txt` into `src/generated.rs`.
//!
//! ```sh
//! cargo run -p tt-config --bin gen-settings         # rewrite it
//! cargo run -p tt-config --bin gen-settings -- --check   # ...or just check
//! ```
//!
//! The output is **committed**, and `tests/generated.rs` fails when it is stale
//! — the same arrangement as `tt-ffi`'s header, and for the same reason. Two
//! build systems have to see this file (Cargo, and eventually CMake for the
//! dialog), and wiring a generator into both is how `PLAN.md`'s risk 5 starts.
//! Generating on demand and reviewing the diff keeps the build boring and puts
//! a schema change in front of a human once.
//!
//! Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One row of the schema, with the comment lines above it as its docs.
struct Setting {
    name: String,
    kind: Kind,
    section: String,
    key: String,
    /// The INI's own spelling of the default, kept verbatim so the writer and
    /// the reader agree about it without a second conversion.
    default: String,
    label: String,
    doc: Vec<String>,
}

enum Kind {
    Bool,
    /// `Key` or `Key.N`, the latter being the Nth comma-separated field of a
    /// value that holds several — `TerminalSize` is `80,24`.
    Int { field: Option<usize> },
    Str,
    /// `spelling => Variant`, in the order they were written.
    Enum(Vec<(String, String)>),
    /// Two RGB triples in one value.
    Color2,
}

impl Kind {
    fn rust_type(&self, setting: &Setting) -> String {
        match self {
            Kind::Bool => "bool".into(),
            Kind::Int { .. } => "i32".into(),
            Kind::Str => "String".into(),
            Kind::Enum(_) => type_name(&setting.name),
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
        let (default, label) = rest
            .rsplit_once('|')
            .unwrap_or_else(|| panic!("want 6 fields: {trimmed}"));
        assert!(f.len() == 4, "want 6 fields: {trimmed}");

        let (key, kind) = parse_kind(f[1], f[3]);
        out.push(Setting {
            name: f[0].to_string(),
            kind,
            section: f[2].to_string(),
            key,
            default: default.trim().to_string(),
            label: label.trim().to_string(),
            doc: std::mem::take(&mut doc),
        });
    }
    out
}

fn parse_kind(spec: &str, key: &str) -> (String, Kind) {
    if let Some(body) = spec.strip_prefix("enum(").and_then(|s| s.strip_suffix(')')) {
        let variants = body
            .split(',')
            .map(|pair| {
                let (spelling, variant) = pair.split_once('=').expect("spelling=Variant");
                (spelling.trim().to_string(), variant.trim().to_string())
            })
            .collect();
        return (key.to_string(), Kind::Enum(variants));
    }
    match spec {
        "bool" => (key.to_string(), Kind::Bool),
        "string" => (key.to_string(), Kind::Str),
        "color2" => (key.to_string(), Kind::Color2),
        "int" => match key.rsplit_once('.') {
            Some((base, n)) if n.chars().all(|c| c.is_ascii_digit()) => (
                base.to_string(),
                Kind::Int {
                    field: Some(n.parse::<usize>().expect("a digit") - 1),
                },
            ),
            _ => (key.to_string(), Kind::Int { field: None }),
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
        let Kind::Enum(variants) = &s.kind else {
            continue;
        };
        for line in &s.doc {
            writeln!(out, "/// {line}").expect("string");
        }
        writeln!(out, "#[derive(Clone, Copy, Debug, PartialEq, Eq)]").expect("string");
        writeln!(out, "pub enum {} {{", type_name(&s.name)).expect("string");
        for (spelling, variant) in variants {
            writeln!(out, "    /// `{spelling}`").expect("string");
            writeln!(out, "    {variant},").expect("string");
        }
        out.push_str("}\n\n");

        writeln!(out, "impl {} {{", type_name(&s.name)).expect("string");
        out.push_str("    /// The INI's own spelling, which is what gets written back.\n");
        out.push_str("    pub fn as_ini(&self) -> &'static str {\n        match self {\n");
        for (spelling, variant) in variants {
            writeln!(
                out,
                "            Self::{variant} => \"{}\",",
                escape(spelling)
            )
            .expect("string");
        }
        out.push_str("        }\n    }\n\n");
        out.push_str(
            "    /// Case-insensitive, and **anything unrecognised takes the default**\n\
             \x20   /// rather than failing — which is how upstream spells most of its\n\
             \x20   /// defaults, as the `else` branch of a chain of comparisons.\n",
        );
        writeln!(out, "    pub fn from_ini(s: &str) -> Self {{").expect("string");
        out.push_str("        let s = s.trim();\n");
        for (spelling, variant) in variants {
            writeln!(
                out,
                "        if s.eq_ignore_ascii_case(\"{}\") {{ return Self::{variant}; }}",
                escape(spelling)
            )
            .expect("string");
        }
        writeln!(out, "        Self::default()").expect("string");
        out.push_str("    }\n}\n\n");

        let default_variant = variants
            .iter()
            .find(|(spelling, _)| spelling.eq_ignore_ascii_case(&s.default))
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
            Kind::Enum(_) => format!("{}::default()", type_name(&s.name)),
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
         \x20       Settings {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let (section, key) = (escape(&s.section), escape(&s.key));
        let expr = match &s.kind {
            Kind::Bool => format!(
                "crate::schema::on_off(ini.get(\"{section}\", \"{key}\"), {})",
                on_off_literal(&s.default)
            ),
            Kind::Int { field: None } => {
                format!("ini.get_int(\"{section}\", \"{key}\", d.{field}) as i32")
            }
            Kind::Int { field: Some(n) } => format!(
                "crate::schema::nth_int(ini.get(\"{section}\", \"{key}\"), {n}, d.{field})"
            ),
            Kind::Str => format!("ini.get_or(\"{section}\", \"{key}\", &d.{field}).to_string()"),
            Kind::Enum(_) => format!(
                "match ini.get(\"{section}\", \"{key}\") {{ \
                 Some(v) => {}::from_ini(v), None => d.{field} }}",
                type_name(&s.name)
            ),
            Kind::Color2 => format!(
                "crate::schema::color2(ini.get(\"{section}\", \"{key}\"), d.{field})"
            ),
        };
        writeln!(out, "            {field}: {expr},").expect("string");
    }
    out.push_str("        }\n    }\n\n");

    out.push_str(
        "    /// Write every setting back, leaving the rest of the file alone.\n\
         \x20   pub fn store(&self, ini: &mut Ini) {\n",
    );
    for s in settings {
        let field = field_name(&s.name);
        let (section, key) = (escape(&s.section), escape(&s.key));
        let expr = match &s.kind {
            Kind::Bool => format!("if self.{field} {{ \"on\" }} else {{ \"off\" }}.to_string()"),
            Kind::Int { field: None } => format!("self.{field}.to_string()"),
            Kind::Int { field: Some(n) } => format!(
                "crate::schema::with_nth(ini.get(\"{section}\", \"{key}\"), {n}, self.{field})"
            ),
            Kind::Str => format!("self.{field}.clone()"),
            Kind::Enum(_) => format!("self.{field}.as_ini().to_string()"),
            Kind::Color2 => format!("crate::schema::color2_str(&self.{field})"),
        };
        writeln!(out, "        ini.set(\"{section}\", \"{key}\", &{expr});").expect("string");
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
            Kind::Enum(_) => format!("self.{field}.as_ini().to_string()"),
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
            Kind::Int { .. } => format!(
                "self.{field} = crate::schema::int(value, self.{field})"
            ),
            Kind::Str => format!("self.{field} = value.to_string()"),
            Kind::Enum(_) => format!("self.{field} = {}::from_ini(value)", type_name(&s.name)),
            Kind::Color2 => {
                format!("self.{field} = crate::schema::color2(Some(value), self.{field})")
            }
        };
        writeln!(out, "            \"{}\" => {expr},", escape(&s.name)).expect("string");
    }
    out.push_str("            _ => return false,\n        }\n        true\n    }\n}\n\n");

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
            Kind::Int { .. } => "Kind::Int".to_string(),
            Kind::Str => "Kind::Str".to_string(),
            Kind::Color2 => "Kind::Color2".to_string(),
            Kind::Enum(variants) => {
                let list: Vec<String> = variants
                    .iter()
                    .map(|(spelling, _)| format!("\"{}\"", escape(spelling)))
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

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema = crate_dir.join("schema/settings.txt");
    let target = crate_dir.join("src/generated.rs");

    let text = std::fs::read_to_string(&schema).expect("schema/settings.txt is readable");
    let settings = parse(&text);
    assert!(!settings.is_empty(), "the schema is empty");
    let generated = emit(&settings);

    if std::env::args().any(|a| a == "--check") {
        let existing = std::fs::read_to_string(&target).unwrap_or_default();
        if existing != generated {
            eprintln!(
                "src/generated.rs is stale — run `cargo run -p tt-config --bin gen-settings`"
            );
            std::process::exit(1);
        }
        println!("{} settings, generated file is current", settings.len());
        return;
    }

    write_if_changed(&target, &generated);
    println!("{} settings -> {}", settings.len(), target.display());
}

/// Leave the mtime alone when nothing changed, so re-running does not force a
/// rebuild of everything downstream.
fn write_if_changed(path: &Path, contents: &str) {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return;
    }
    std::fs::write(path, contents).expect("cannot write the generated file");
}
