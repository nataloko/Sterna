//! Highlight rules — colouring what is on screen by regular expression.
//!
//! Upstream has nothing like this. Tera Term colours a cell six ways and every
//! one of them is the *host's* decision — SGR, bold, blink, reverse and the URL
//! attribute — while its regex library lives in `ttpmacro`, a separate process
//! that never sees the screen. `[Sterna Highlights]` is one of this program's
//! own sections and no real Tera Term reads it (`docs/deviations.md`).
//!
//! ```ini
//! [Sterna Highlights]
//! Highlight1Label=Errors
//! Highlight1Pattern=\b(ERROR|FATAL|CRITICAL)\b
//! Highlight1Fore=255,80,80
//! Highlight1Style=bold
//! Highlight1Scope=line
//! ```
//!
//! One key per field rather than one comma-separated line, because a pattern
//! contains commas — `\d{1,3}` in the first useful rule anybody writes — and
//! the alternative is an escape scheme nobody can hand-edit. These are meant to
//! be edited by hand: the dialog is the convenience, not the format.
//!
//! `Highlight1..Highlight99` are scanned the way `[User keys]` scans
//! `User1..User99` — a gap is skipped, and the order rules apply in is index
//! order, which is also their priority.
//!
//! This module is the *file* and nothing else. What a pattern means, and the
//! engine it is compiled by, live in `tt-session` beside the cells being
//! coloured — so nobody who only wants to read a settings file has to compile a
//! regex engine to do it.

use std::path::Path;

use crate::schema::on_off;
use crate::Ini;

/// The INI section. Not `[Sterna]`: that one is scalar settings the schema
/// generates code for, and this is a list the schema cannot describe.
pub const SECTION: &str = "Sterna Highlights";

/// How many are looked for, matching `[User keys]`' own ceiling.
pub const MAX: usize = 99;

/// Bold, in [`Rule::style`].
///
/// Deliberately this crate's own numbering rather than `tt_grid`'s attribute
/// bits, which these are eventually mapped onto: `tt-config` describes the file
/// and knows nothing about cells. The mapping is one `match` in `tt-session`,
/// where both halves are in view.
pub const STYLE_BOLD: u32 = 1 << 0;
/// Underline, in [`Rule::style`].
pub const STYLE_UNDERLINE: u32 = 1 << 1;
/// Reverse video, in [`Rule::style`].
pub const STYLE_REVERSE: u32 = 1 << 2;

/// What a rule paints over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// The matched text alone — or the capture group, if [`Rule::group`] names
    /// one.
    #[default]
    Match,
    /// The whole logical line the match sits on, continuation rows included.
    /// For severity lines, where the marker is one word and the line is what
    /// wants seeing.
    Line,
}

/// One rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// What the editor's list calls it. Optional — the pattern is shown when
    /// this is empty, which for a short pattern is the better label anyway.
    pub label: String,
    /// The pattern, as written. Regular expression syntax unless
    /// [`Rule::literal`] is set; **not** compiled here.
    pub pattern: String,
    /// The pattern is plain text, not a pattern. So somebody can highlight
    /// `10.0.0.1` without knowing that `.` means something.
    pub literal: bool,
    /// Match without regard to case.
    pub ignore_case: bool,
    /// The foreground, `r,g,b`. **`None` means leave it alone**, which is how a
    /// rule changes only the background — and the reason this is an `Option`
    /// rather than the schema's `color2`, where absent means a default.
    pub fore: Option<[u8; 3]>,
    /// The background, on the same terms.
    pub back: Option<[u8; 3]>,
    /// [`STYLE_BOLD`] and friends, OR-ed. A rule that only underlines spends no
    /// colour on itself.
    pub style: u32,
    pub scope: Scope,
    /// Which capture group to colour; 0, the whole match, is the default. A
    /// group that did not take part in the match colours nothing.
    pub group: u32,
    /// Off keeps a rule in the file without applying it — for the one somebody
    /// is debugging, which is otherwise deleted and retyped.
    pub enabled: bool,
}

impl Default for Rule {
    fn default() -> Rule {
        Rule {
            label: String::new(),
            pattern: String::new(),
            literal: false,
            ignore_case: false,
            fore: None,
            back: None,
            style: 0,
            scope: Scope::Match,
            group: 0,
            // The one field whose default is not the zero value: a rule written
            // into the file is a rule somebody wants.
            enabled: true,
        }
    }
}

impl Rule {
    /// Whether this rule could change a single pixel.
    ///
    /// A rule with no colours and no style is inert — legal, because it is what
    /// a half-written rule looks like in the editor, and worth being able to
    /// ask about rather than silently dropping somebody's pattern.
    pub fn paints(&self) -> bool {
        self.fore.is_some() || self.back.is_some() || self.style != 0
    }
}

/// The file's spelling of a scope.
pub fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Match => "match",
        Scope::Line => "line",
    }
}

/// Parse a `Scope` value; an unrecognised spelling takes the default.
///
/// Deliberately unlike `buttons.rs`'s `parse_kind`, which drops a record whose
/// kind it cannot read. The rule there is that a record whose *action* could
/// not be read must not do something else instead; a scope decides only how
/// much of a line is coloured, and dropping the rule would throw away a pattern
/// somebody worked on to fix a field the editor can only ever write correctly.
pub fn parse_scope(value: &str) -> Scope {
    if value.trim().eq_ignore_ascii_case("line") {
        Scope::Line
    } else {
        Scope::Match
    }
}

/// The file's spelling of a style word set.
pub fn style_names(style: u32) -> String {
    let mut out = Vec::new();
    for (bit, name) in [
        (STYLE_BOLD, "bold"),
        (STYLE_UNDERLINE, "underline"),
        (STYLE_REVERSE, "reverse"),
    ] {
        if style & bit != 0 {
            out.push(name);
        }
    }
    out.join(",")
}

/// Parse a comma-separated style word list. An unknown word is ignored rather
/// than failing the rule, the way the schema's own list settings read.
pub fn parse_style(value: &str) -> u32 {
    let mut out = 0;
    for word in value.split(',') {
        let word = word.trim();
        for (bit, name) in [
            (STYLE_BOLD, "bold"),
            (STYLE_UNDERLINE, "underline"),
            (STYLE_REVERSE, "reverse"),
        ] {
            if word.eq_ignore_ascii_case(name) {
                out |= bit;
            }
        }
    }
    out
}

/// Parse one `r,g,b` colour.
///
/// All three channels or nothing: unlike the schema's [`crate::schema::color2`]
/// there is no default to fall back to field by field, and a half-read colour
/// would be a colour the user did not choose. Absent, empty and malformed all
/// mean the same thing — leave that channel as the host sent it.
pub fn color3(value: Option<&str>) -> Option<[u8; 3]> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let mut out = [0u8; 3];
    let mut fields = value.split(',');
    for channel in out.iter_mut() {
        *channel = fields.next()?.trim().parse::<u8>().ok()?;
    }
    if fields.next().is_some() {
        return None;
    }
    Some(out)
}

/// Write one `r,g,b` colour.
pub fn color3_str(value: &[u8; 3]) -> String {
    format!("{},{},{}", value[0], value[1], value[2])
}

fn key(index: usize, field: &str) -> String {
    format!("Highlight{index}{field}")
}

const FIELDS: [&str; 10] = [
    "Label",
    "Pattern",
    "Literal",
    "IgnoreCase",
    "Fore",
    "Back",
    "Style",
    "Scope",
    "Group",
    "Enabled",
];

/// Read the section. Order is index order, and a gap is skipped.
pub fn from_ini(ini: &Ini) -> Vec<Rule> {
    let mut out = Vec::new();
    for i in 1..=MAX {
        let label = ini.get(SECTION, &key(i, "Label"));
        let pattern = ini.get(SECTION, &key(i, "Pattern"));
        // A rule needs *something*; a stray `Highlight7Style=bold` left behind
        // by hand is not one.
        if label.is_none() && pattern.is_none() {
            continue;
        }
        out.push(Rule {
            label: label.unwrap_or_default().to_string(),
            pattern: pattern.unwrap_or_default().to_string(),
            literal: on_off(ini.get(SECTION, &key(i, "Literal")), false),
            ignore_case: on_off(ini.get(SECTION, &key(i, "IgnoreCase")), false),
            fore: color3(ini.get(SECTION, &key(i, "Fore"))),
            back: color3(ini.get(SECTION, &key(i, "Back"))),
            style: parse_style(ini.get(SECTION, &key(i, "Style")).unwrap_or_default()),
            scope: parse_scope(ini.get(SECTION, &key(i, "Scope")).unwrap_or_default()),
            group: ini
                .get(SECTION, &key(i, "Group"))
                .unwrap_or_default()
                .trim()
                .parse::<u32>()
                .unwrap_or(0),
            // Default on, so `GetOnOff`'s asymmetric parse means only a literal
            // `off` disables one. The safe direction for a switch whose whole
            // purpose is to be flipped back: a typo leaves the rule working.
            enabled: on_off(ini.get(SECTION, &key(i, "Enabled")), true),
        });
    }
    out
}

/// Replace the section with `rules`, leaving the rest of the file alone.
///
/// Every index up to [`MAX`] is cleared first, so removing a rule removes its
/// keys rather than leaving a shorter list in front of an orphan.
pub fn write_into(ini: &mut Ini, rules: &[Rule]) {
    for i in 1..=MAX {
        for field in FIELDS {
            ini.remove(SECTION, &key(i, field));
        }
    }
    for (n, rule) in rules.iter().take(MAX).enumerate() {
        let i = n + 1;
        // Everything but the pattern is written only when it is not the
        // default, so a file stays as short as what it actually says.
        if !rule.label.is_empty() {
            ini.set(SECTION, &key(i, "Label"), &rule.label);
        }
        ini.set(SECTION, &key(i, "Pattern"), &rule.pattern);
        if rule.literal {
            ini.set(SECTION, &key(i, "Literal"), "on");
        }
        if rule.ignore_case {
            ini.set(SECTION, &key(i, "IgnoreCase"), "on");
        }
        if let Some(fore) = &rule.fore {
            ini.set(SECTION, &key(i, "Fore"), &color3_str(fore));
        }
        if let Some(back) = &rule.back {
            ini.set(SECTION, &key(i, "Back"), &color3_str(back));
        }
        if rule.style != 0 {
            ini.set(SECTION, &key(i, "Style"), &style_names(rule.style));
        }
        if rule.scope != Scope::Match {
            ini.set(SECTION, &key(i, "Scope"), scope_name(rule.scope));
        }
        if rule.group != 0 {
            ini.set(SECTION, &key(i, "Group"), &rule.group.to_string());
        }
        if !rule.enabled {
            ini.set(SECTION, &key(i, "Enabled"), "off");
        }
    }
}

/// Read the rules out of a settings file. A file that is not there is a first
/// run and has no rules, not an error.
pub fn load(path: &Path) -> Vec<Rule> {
    match Ini::load(path) {
        Ok(ini) => from_ini(&ini),
        Err(_) => Vec::new(),
    }
}

/// Write them back into `path`, preserving everything else in it — comments,
/// ordering, and every setting this program does not know about.
pub fn save(path: &Path, rules: &[Rule]) -> std::io::Result<()> {
    let mut ini = Ini::load(path).unwrap_or_else(|_| Ini::new());
    write_into(&mut ini, rules);
    ini.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<Rule> {
        from_ini(&Ini::parse(s.as_bytes()))
    }

    #[test]
    fn a_rule_is_a_pattern_and_what_to_do_with_it() {
        let r = parse(
            "[Sterna Highlights]\nHighlight1Label=Errors\n\
             Highlight1Pattern=\\b(ERROR|FATAL)\\b\nHighlight1Fore=255,80,80\n\
             Highlight1Back=32,0,0\nHighlight1Style=bold,underline\n\
             Highlight1Scope=line\nHighlight1Group=1\nHighlight1IgnoreCase=on\n",
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].label, "Errors");
        assert_eq!(r[0].pattern, "\\b(ERROR|FATAL)\\b");
        assert_eq!(r[0].fore, Some([255, 80, 80]));
        assert_eq!(r[0].back, Some([32, 0, 0]));
        assert_eq!(r[0].style, STYLE_BOLD | STYLE_UNDERLINE);
        assert_eq!(r[0].scope, Scope::Line);
        assert_eq!(r[0].group, 1);
        assert!(r[0].ignore_case);
        assert!(r[0].enabled);
        assert!(r[0].paints());
    }

    #[test]
    fn the_defaults_are_a_case_sensitive_pattern_over_its_own_match() {
        let r = parse("[Sterna Highlights]\nHighlight1Pattern=x\n");
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0],
            Rule {
                pattern: "x".into(),
                ..Rule::default()
            }
        );
        assert_eq!(r[0].scope, Scope::Match);
        assert_eq!(r[0].group, 0);
        assert!(!r[0].literal);
        assert!(!r[0].ignore_case);
        assert!(r[0].enabled);
        // ...and it cannot colour anything yet, which the editor is allowed to
        // show and this module is not allowed to discard.
        assert!(!r[0].paints());
    }

    #[test]
    fn an_absent_colour_is_not_black() {
        // The whole reason these are `Option`: a rule with only a background
        // has to leave the foreground as the host sent it.
        let r = parse("[Sterna Highlights]\nHighlight1Pattern=x\nHighlight1Back=0,80,0\n");
        assert_eq!(r[0].fore, None);
        assert_eq!(r[0].back, Some([0, 80, 0]));
        // Empty and malformed mean the same as absent — all three channels or
        // none, because there is no per-field default to fall back to.
        assert_eq!(color3(None), None);
        assert_eq!(color3(Some("")), None);
        assert_eq!(color3(Some("255,0")), None);
        assert_eq!(color3(Some("255,0,0,0")), None);
        assert_eq!(color3(Some("255,green,0")), None);
        assert_eq!(color3(Some("300,0,0")), None);
        assert_eq!(color3(Some(" 255 , 80 , 80 ")), Some([255, 80, 80]));
    }

    #[test]
    fn the_style_is_a_word_list_and_an_unknown_word_is_ignored() {
        assert_eq!(parse_style("bold"), STYLE_BOLD);
        assert_eq!(parse_style("BOLD, Reverse"), STYLE_BOLD | STYLE_REVERSE);
        assert_eq!(parse_style("bold,italic"), STYLE_BOLD);
        assert_eq!(parse_style("italic"), 0);
        assert_eq!(parse_style(""), 0);
        assert_eq!(style_names(STYLE_BOLD | STYLE_UNDERLINE), "bold,underline");
        assert_eq!(style_names(0), "");
    }

    #[test]
    fn an_unreadable_scope_keeps_the_rule_and_takes_the_default() {
        // Unlike a quick button's kind, which drops the record. A scope cannot
        // make a rule do something dangerous, and dropping it would throw away
        // the pattern to fix a word the editor always writes correctly.
        let r = parse("[Sterna Highlights]\nHighlight1Pattern=x\nHighlight1Scope=sausage\n");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].scope, Scope::Match);
        assert_eq!(parse_scope("line"), Scope::Line);
        assert_eq!(parse_scope(" LINE "), Scope::Line);
    }

    #[test]
    fn enabled_is_get_on_off_with_a_default_of_on() {
        // The asymmetric parse, the other way up from a quick button's Confirm:
        // default on means anything but a literal `off` is on, so `0` and `no`
        // both leave the rule working.
        for (value, live) in [("off", false), ("OFF", false), ("0", true), ("no", true)] {
            let r = parse(&format!(
                "[Sterna Highlights]\nHighlight1Pattern=x\nHighlight1Enabled={value}\n"
            ));
            assert_eq!(r[0].enabled, live, "Enabled={value}");
        }
    }

    #[test]
    fn a_gap_is_skipped_and_the_order_is_the_index() {
        // Which is also the priority: rule 1 claims a cell before rule 3 sees
        // it.
        let r = parse(
            "[Sterna Highlights]\nHighlight3Label=third\nHighlight3Pattern=c\n\
             Highlight1Label=first\nHighlight1Pattern=a\n",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].label, "first");
        assert_eq!(r[1].label, "third");
    }

    #[test]
    fn a_key_with_no_rule_behind_it_is_not_one() {
        assert!(parse("[Sterna Highlights]\nHighlight1Style=bold\n").is_empty());
        assert!(parse("[Sterna Highlights]\n").is_empty());
        assert!(parse("").is_empty());
    }

    #[test]
    fn a_written_section_reads_back_the_same() {
        let rules = vec![
            Rule {
                label: "Errors".into(),
                pattern: "\\b(ERROR|FATAL)\\b".into(),
                fore: Some([255, 80, 80]),
                style: STYLE_BOLD,
                scope: Scope::Line,
                ..Rule::default()
            },
            Rule {
                label: "IPv4".into(),
                pattern: "\\d{1,3}(\\.\\d{1,3}){3}".into(),
                fore: Some([120, 200, 255]),
                ..Rule::default()
            },
            Rule {
                pattern: "10.0.0.1".into(),
                literal: true,
                ignore_case: true,
                back: Some([0, 80, 0]),
                group: 2,
                enabled: false,
                ..Rule::default()
            },
        ];
        let mut ini = Ini::new();
        write_into(&mut ini, &rules);
        assert_eq!(from_ini(&ini), rules);
        // ...and the file says what a person would write by hand. The comma in
        // the second pattern is why one key per field is not a luxury.
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(
            text.contains("Highlight2Pattern=\\d{1,3}(\\.\\d{1,3}){3}"),
            "{text}"
        );
        assert!(text.contains("Highlight1Scope=line"), "{text}");
        assert!(text.contains("Highlight3Enabled=off"), "{text}");
    }

    #[test]
    fn a_default_field_is_not_written_at_all() {
        let mut ini = Ini::new();
        write_into(
            &mut ini,
            &[Rule {
                pattern: "x".into(),
                ..Rule::default()
            }],
        );
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert_eq!(
            text.lines().filter(|l| l.starts_with("Highlight1")).count(),
            1,
            "{text}"
        );
        assert!(text.contains("Highlight1Pattern=x"), "{text}");
    }

    #[test]
    fn removing_a_rule_removes_its_keys() {
        let mut ini = Ini::parse(
            b"[Tera Term]\nBaudRate=115200\n[Sterna Highlights]\n\
              Highlight1Label=one\nHighlight1Pattern=a\nHighlight1Style=bold\n\
              Highlight2Label=two\nHighlight2Pattern=b\nHighlight2Scope=line\n",
        );
        write_into(
            &mut ini,
            &[Rule {
                label: "two".into(),
                pattern: "b".into(),
                ..Rule::default()
            }],
        );
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(!text.contains("Highlight2"), "{text}");
        assert!(!text.contains("Style"), "{text}");
        assert!(!text.contains("Scope"), "{text}");
        assert!(text.contains("Highlight1Label=two"), "{text}");
        // Everything that is not ours is left exactly where it was.
        assert!(text.contains("BaudRate=115200"), "{text}");
    }
}
