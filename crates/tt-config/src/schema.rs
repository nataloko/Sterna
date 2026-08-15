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
    /// An int upstream bounds on read — the pair a spin box needs. See
    /// [`ranged`] for what happens to a value outside it, which is not a
    /// clamp in both directions.
    IntRange(i32, i32),
    /// An int with a floor and no ceiling. A different rule from
    /// [`IntRange`](Kind::IntRange) and not a special case of it — see
    /// [`floored`].
    IntMin(i32),
    /// An int clamped at both ends — the third of the three, and see
    /// [`clamped`] for why it is not either of the others.
    IntClamp(i32, i32),
    /// `GetPrivateProfileInt` narrowed into a Win32 `WORD`. The dialog range
    /// is 0..65535, while the reader and name-addressed setter wrap into it.
    IntWord,
    /// The same for a `BYTE`, so 0..255.
    IntByte,
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
    /// The schema's developer documentation, including behavioural citations.
    pub doc: &'static str,
}

/// The receive-debug modes named by `DebugModes` (`ttset.c:1798`).
///
/// This is the parsed meaning beside the schema's raw string. Keeping the raw
/// spelling lets a shared file round-trip without losing order or unknown
/// words; this mask is what the terminal and `Debug=on` validation consume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugModes(u8);

impl DebugModes {
    pub const NORMAL: u8 = 1;
    pub const HEX: u8 = 2;
    pub const NO_OUTPUT: u8 = 4;
    pub const ALL: u8 = Self::NORMAL | Self::HEX | Self::NO_OUTPUT;

    pub fn parse_ini(value: &str) -> DebugModes {
        let whole = value.trim();
        if whole.eq_ignore_ascii_case("on") || whole.eq_ignore_ascii_case("all") {
            return DebugModes(Self::ALL);
        }
        if whole.eq_ignore_ascii_case("off") || whole.eq_ignore_ascii_case("none") {
            return DebugModes(0);
        }
        let mut bits = 0;
        for item in value.split(',').map(str::trim) {
            if item.eq_ignore_ascii_case("normal") {
                bits |= Self::NORMAL;
            } else if item.eq_ignore_ascii_case("hex") {
                bits |= Self::HEX;
            } else if item.eq_ignore_ascii_case("noout") {
                bits |= Self::NO_OUTPUT;
            }
        }
        DebugModes(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
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

/// Upstream's bounds check on an int, which is **not** a clamp in both
/// directions: `ttset.c:615` takes the *default* for anything at or below the
/// floor and the *ceiling* for anything above it.
///
/// ```text
/// if (ts->TerminalWidth <= 0)            ts->TerminalWidth = 80;
/// else if (ts->TerminalWidth > TermWidthMax) ts->TerminalWidth = TermWidthMax;
/// ```
///
/// So `TerminalSize=0,0` is an 80x24 terminal and `TerminalSize=9999,9999` is
/// 1000x500. Clamping the low end to the floor instead would give a
/// one-column terminal, which is a window nobody can use out of a file
/// somebody's Tera Term opens fine.
pub fn ranged(value: i32, default: i32, lo: i32, hi: i32) -> i32 {
    if value < lo {
        default
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// A range where *either* invalid end takes the default.
///
/// The Unicode width settings use this fourth shape (`ttset.c:1965`): only 1
/// and 2 are accepted, and every other value becomes the platform default.
/// That differs from [`ranged`], which caps a value above its ceiling.
pub fn validated(value: i32, default: i32, lo: i32, hi: i32) -> i32 {
    if (lo..=hi).contains(&value) {
        value
    } else {
        default
    }
}

/// An integer with one textual alias before the ordinary Win32 parse.
///
/// `MaximizedBugTweak=on` is the sole upstream instance: `on` means 2 and
/// every other spelling goes through `atoi` (`ttset.c:1527`). The writer emits
/// the resulting number, so this returns an integer rather than preserving the
/// alias as text.
pub fn int_alias(value: Option<&str>, default: i32, alias: &str, aliased: i32) -> i32 {
    match value {
        None => default,
        Some(value) if value.eq_ignore_ascii_case(alias) => aliased,
        // This shape comes from `GetPrivateProfileString` followed by `atoi`,
        // not `GetPrivateProfileInt`: a present empty or non-numeric value is
        // zero rather than the fallback.
        Some(value) => crate::ini::parse_int_public(value.trim()) as i32,
    }
}

/// Upstream's *other* bounds check, which really is a clamp — and the two must
/// not be confused, because they disagree about exactly the values a
/// hand-edited file is likely to hold.
///
/// ```text
/// ts->XmodemTimeOutInit = GetNthNum2(Temp, 1, 10);
/// if (ts->XmodemTimeOutInit < 1) ts->XmodemTimeOutInit = 1;
/// ```
///
/// `ttset.c:1822` onward. So `XmodemTimeouts=0,0,0,0,0` is five **one-second**
/// timeouts, where [`ranged`] would have given upstream's `10,3,10,20,60`. The
/// transfer timeouts are the only settings read this way, and `ZmodemTimeouts`'
/// second field floors at 0 rather than 1 because 0 is meaningful there: it is
/// what "never time out" is spelt as on a network link.
pub fn floored(value: i32, lo: i32) -> i32 {
    value.max(lo)
}

/// And upstream's third, which clamps at both ends.
///
/// ```text
/// int tmp = min(max(0, ts->PasteDelayPerLine), 5000);
/// ```
///
/// `ttset.c:1633`. `PasteDelayPerLine` is the only setting read this way, and
/// it is neither of the two above: [`ranged`] would give the *default* for a
/// negative value where upstream gives the floor — and a negative value is
/// reachable, since `GetPrivateProfileInt` answers `Key=-5` with `(UINT)-5`,
/// which lands in an `int` field as -5 — while [`floored`] would leave
/// `PasteDelayPerLine=60000` at a minute a line on a paste nobody could stop.
pub fn clamped(value: i32, lo: i32, hi: i32) -> i32 {
    value.clamp(lo, hi)
}

/// Narrow a `GetPrivateProfileInt` result the way assignment to a `WORD` does.
///
/// **This runs before any bound, because C does it in the assignment.** The
/// order decides the answer rather than tidying it: `MaxComPort=-1` is
/// `(UINT)-1`, so upstream sees 65535 and its `min(4096, …)` gives **4096**,
/// while a clamp applied to a bare -1 reads it as below the floor and gives 4.
/// Opposite ends of the same range (`ttset.c:1218`). `TitleFormat` is the
/// plainest instance, with no bound at all: upstream reads `-1` as 65535 and
/// `65537` as 1, and writes the narrowed value back.
pub fn word(value: i32) -> i32 {
    i32::from(value as u16)
}

/// The same, for a `BYTE` field.
///
/// `AlphaBlend`'s two keys are where this decides something visible. They are
/// `BYTE`s, so the `max(0, …)`/`min(255, …)` pair upstream applies next
/// (`ttset.c:1467`) can never fire — **the clamp that looks like the rule is
/// dead code**, and the narrowing is the whole of it. `AlphaBlend=-1` is
/// therefore 255, an opaque window; reading the clamp as the rule gives 0, a
/// window nobody can see.
pub fn byte(value: i32) -> i32 {
    i32::from(value as u8)
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

/// The `GetNthNum` variant: an absent **key** takes the field's default, but a
/// missing or empty field in a value that exists is zero.
///
/// `VTPos=12` is therefore `(12, 0)`, not `(12, CW_USEDEFAULT)`, while no
/// `VTPos` key at all is the sentinel in both axes. Transfer timeouts use
/// [`nth_int`] instead, because upstream reads those with `GetNthNum2` and a
/// per-field fallback.
pub fn nth_int_zero(value: Option<&str>, n: usize, default: i32) -> i32 {
    let Some(value) = value else {
        return default;
    };
    value
        .split(',')
        .nth(n)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| int(s, 0))
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::{byte, clamped, int_alias, validated, word, DebugModes};

    #[test]
    fn validated_ranges_default_at_both_ends() {
        assert_eq!(validated(0, 1, 1, 2), 1);
        assert_eq!(validated(1, 1, 1, 2), 1);
        assert_eq!(validated(2, 1, 1, 2), 2);
        assert_eq!(validated(3, 1, 1, 2), 1);
    }

    #[test]
    fn integer_aliases_are_resolved_before_numbers() {
        assert_eq!(int_alias(None, 2, "on", 2), 2);
        assert_eq!(int_alias(Some("ON"), 9, "on", 2), 2);
        assert_eq!(int_alias(Some("7"), 9, "on", 2), 7);
        assert_eq!(int_alias(Some(""), 9, "on", 2), 0);
        assert_eq!(int_alias(Some("other"), 9, "on", 2), 0);
    }

    /// The generator composes the narrowing around whatever produced the
    /// number, which for `MaximizedBugTweak` is the alias parse. Same order as
    /// the C: resolve the value, assign it to the field, and only then run
    /// whatever `if` follows.
    #[test]
    fn a_width_narrows_after_the_alias_or_number_is_resolved() {
        assert_eq!(word(int_alias(None, 2, "on", 2)), 2);
        assert_eq!(word(int_alias(Some("ON"), 9, "on", 2)), 2);
        assert_eq!(word(int_alias(Some("-1"), 9, "on", 2)), 65_535);
        assert_eq!(word(int_alias(Some("65537"), 9, "on", 2)), 1);
    }

    /// And the order against a *bound* is what `MaxComPort` and `AlphaBlend`
    /// turn on: narrowing first puts `-1` at the top of the range, while the
    /// bound alone puts it at the bottom.
    #[test]
    fn a_width_runs_before_the_bound_and_not_after() {
        assert_eq!(clamped(word(-1), 4, 4096), 4096);
        assert_eq!(clamped(-1, 4, 4096), 4, "which is the wrong answer");
        // `AlphaBlend`'s clamp is dead code behind the same narrowing: a BYTE
        // cannot be outside 0..255 for `max`/`min` to correct.
        assert_eq!(byte(-1), 255);
        assert_eq!(byte(256), 0);
        assert_eq!(clamped(byte(-1), 0, 255), byte(-1));
    }

    #[test]
    fn debug_modes_have_whole_values_and_a_list() {
        assert_eq!(DebugModes::parse_ini("ON").bits(), DebugModes::ALL);
        assert_eq!(DebugModes::parse_ini("none").bits(), 0);
        assert_eq!(
            DebugModes::parse_ini("hex, unknown, NORMAL").bits(),
            DebugModes::HEX | DebugModes::NORMAL
        );
        assert!(DebugModes::parse_ini("unknown").is_empty());
    }
}
