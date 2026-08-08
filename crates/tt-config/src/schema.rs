//! The hand-written half of the schema: what a setting *is*, and the four
//! conversions the generated code calls.
//!
//! `schema/settings.txt` is the list and `src/generated.rs` is what the
//! generator makes of it. Everything here is the part that would be identical
//! in every generated line, so it is written once and called.

/// What kind of value a setting holds — enough for a dialog to choose a widget
/// and for a script to validate an assignment, and no more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Str,
    /// The spellings the file accepts, in order. Anything else reads as the
    /// default, which is upstream's convention rather than an oversight.
    Enum(&'static [&'static str]),
    /// Two RGB triples: a foreground and a background, because upstream's
    /// attributes each carry their own pair.
    Color2,
}

/// One setting, as data.
///
/// The dialog builds itself from these, `setsetting`/`getsetting` resolve
/// through them, and the documentation table is printed from them — so the
/// list of settings exists exactly once, which is the whole argument for
/// having a schema at all.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The dotted name a script uses.
    pub name: &'static str,
    /// Everything before the first dot: which dialog page it belongs on.
    pub page: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub kind: Kind,
    /// The default, in the INI's own spelling.
    pub default: &'static str,
    /// The `.lng` key for the dialog label, where upstream has a dialog for it.
    pub label: Option<&'static str>,
    /// The schema's own comment, which is where the citation for the default
    /// lives. Shown as a tooltip and printed in the docs.
    pub doc: &'static str,
}

/// `GetOnOff` (`ttset.c:344`), which is **not** a symmetric parse.
///
/// With a default of on, anything that is not literally `off` is on. With a
/// default of off, only literally `on` is on. So `Key=1` means opposite things
/// for two settings that differ only in their default, and `Key=yes` reads as
/// off for half of them.
///
/// It also reads into a four-byte buffer, so only the first three characters
/// ever reach the comparison and `offline` is `off`. Reproduced, because a
/// file that says `offline` is a file somebody's Tera Term is already treating
/// as off.
pub fn on_off(value: Option<&str>, default: bool) -> bool {
    let Some(value) = value else {
        return default;
    };
    let truncated: String = value.chars().take(3).collect();
    if default {
        !truncated.eq_ignore_ascii_case("off")
    } else {
        truncated.eq_ignore_ascii_case("on")
    }
}

/// `GetPrivateProfileInt`'s parse, for a value already in hand.
pub fn int(value: &str, default: i32) -> i32 {
    if value.trim().is_empty() {
        return default;
    }
    crate::ini::parse_int_public(value.trim()) as i32
}

/// The `n`th comma-separated number of a value that holds several —
/// `TerminalSize` is `80,24`, one key for two settings.
pub fn nth_int(value: Option<&str>, n: usize, default: i32) -> i32 {
    value
        .and_then(|v| v.split(',').nth(n))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| int(s, default))
        .unwrap_or(default)
}

/// Put `value` in the `n`th field, keeping whatever the others were.
///
/// Reading before writing matters: the other half of `TerminalSize` belongs to
/// a different setting, and a writer that rebuilt the whole value from one of
/// them would silently reset the other to its default.
pub fn with_nth(existing: Option<&str>, n: usize, value: i32) -> String {
    let mut fields: Vec<String> = existing
        .unwrap_or("")
        .split(',')
        .map(|f| f.trim().to_string())
        .collect();
    while fields.len() <= n {
        fields.push(String::from("0"));
    }
    fields[n] = value.to_string();
    fields.join(",")
}

/// Six numbers: a foreground triple and a background triple.
pub fn color2(value: Option<&str>, default: [u8; 6]) -> [u8; 6] {
    let Some(value) = value else {
        return default;
    };
    let mut out = default;
    for (i, field) in value.split(',').take(6).enumerate() {
        // A short or malformed value keeps the default for the fields it did
        // not supply, rather than throwing the whole colour away — upstream's
        // `GetNthNum` leaves its out-parameter alone the same way.
        if let Ok(n) = field.trim().parse::<u8>() {
            out[i] = n;
        }
    }
    out
}

pub fn color2_str(value: &[u8; 6]) -> String {
    value
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
