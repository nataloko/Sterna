//! `KEYBOARD.CNF`, Tera Term's physical-key map.
//!
//! The file is an INI, but the values are not settings: they map a PC/AT
//! set-1 scan code, with modifier bits in the upper byte, to a terminal key,
//! a local command or one of 99 user-defined actions.  The parser lives beside
//! [`crate::Ini`] so it gets the same first-section, first-key, quote and
//! encoding behaviour as `GetPrivateProfileStringW`.
//!
//! The right-hand number is deliberately called a *scan code*, not a Qt or
//! Windows key.  A frontend normalises its native key event to this legacy
//! number before looking it up; keeping that platform step outside makes a
//! `KEYBOARD.CNF` copied from Windows mean the same thing on Linux.

use std::collections::BTreeMap;
use std::path::Path;

use tt_vt::Key;

use crate::Ini;

/// What a physical key is bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyboardAction {
    /// A terminal key whose bytes depend on the live VT modes.
    Terminal(Key),
    /// A DEC user-defined key (UDK6 through UDK20).
    Udk(u8),
    /// An action owned by the window rather than by the terminal.
    Shortcut(Shortcut),
    /// A free-form `[User keys]` entry.
    User(UserKey),
}

/// The `[Shortcut keys]` names understood by upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    EditCopy,
    EditPaste,
    EditPasteCr,
    EditClearScreen,
    EditClearBuffer,
    ControlOpenTek,
    ControlCloseTek,
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    BufferTop,
    BufferBottom,
    NextWindow,
    PreviousWindow,
    NextShownWindow,
    PreviousShownWindow,
    LocalEcho,
    ScrollLock,
}

/// A `[User keys]` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserKey {
    pub kind: UserKeyType,
    /// The value after the second comma, before `$HH` decoding.
    pub value: String,
}

/// How a user key's value is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserKeyType {
    /// Send bytes, with `$HH` escapes and no text conversion.
    Binary,
    /// Send text, with `$HH` escapes and newline conversion.
    Text,
    /// Start the named macro.
    Macro,
    /// Invoke the decimal menu command id in the value.
    Command,
    /// Upstream stores an unknown integer too; pressing it then does nothing.
    Unknown(i32),
}

/// A parsed keyboard setup file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardMap {
    bindings: BTreeMap<u16, KeyboardAction>,
    duplicates: Vec<u16>,
}

impl KeyboardMap {
    /// Parse a keyboard map through the same INI representation as settings.
    pub fn from_ini(ini: &Ini) -> KeyboardMap {
        let mut entries = Vec::new();

        for (id, section, name, action) in FIXED {
            if let Some(scan) = fixed_scan(ini.get(section, name)) {
                entries.push((*id, scan, action.clone()));
            }
        }

        for i in 1..=99 {
            let name = format!("User{i}");
            let Some((scan, user)) = user_key(ini.get("User keys", &name)) else {
                continue;
            };
            // IdUser1 is 90. Higher internal ids win duplicate scan codes,
            // whatever order the entries appeared in the file.
            entries.push((89 + i, scan, KeyboardAction::User(user)));
        }

        entries.sort_by_key(|(id, _, _)| *id);
        let mut out = KeyboardMap::default();
        for (_, scan, action) in entries {
            if scan == u16::MAX {
                continue;
            }
            if out.bindings.insert(scan, action).is_some() {
                // `_ReadKeyboardCnf` warns once for each older assignment it
                // disables. Keeping the codes lets a UI make the same warning
                // without putting dialogs in the parser.
                out.duplicates.push(scan);
            }
        }
        out
    }

    /// Read a map. A missing file is an empty map, as it is for upstream's
    /// `GetPrivateProfileStringW` calls.
    pub fn load(path: &Path) -> std::io::Result<KeyboardMap> {
        Ok(KeyboardMap::from_ini(&Ini::load(path)?))
    }

    pub fn get(&self, scan: u16) -> Option<&KeyboardAction> {
        self.bindings.get(&scan)
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Scan codes assigned more than once, in the order the older bindings
    /// were displaced.
    pub fn duplicates(&self) -> &[u16] {
        &self.duplicates
    }
}

/// One fixed entry, paired with Tera Term's internal key id. The id is needed
/// only for duplicate resolution: `_ReadKeyboardCnf` keeps the assignment with
/// the highest id, not the one whose section happened to be read last.
type Fixed = (u16, &'static str, &'static str, KeyboardAction);

const FIXED: &[Fixed] = &[
    (
        1,
        "VT editor keypad",
        "Up",
        KeyboardAction::Terminal(Key::Up),
    ),
    (
        2,
        "VT editor keypad",
        "Down",
        KeyboardAction::Terminal(Key::Down),
    ),
    (
        3,
        "VT editor keypad",
        "Right",
        KeyboardAction::Terminal(Key::Right),
    ),
    (
        4,
        "VT editor keypad",
        "Left",
        KeyboardAction::Terminal(Key::Left),
    ),
    (
        5,
        "VT numeric keypad",
        "Num0",
        KeyboardAction::Terminal(Key::Kp0),
    ),
    (
        6,
        "VT numeric keypad",
        "Num1",
        KeyboardAction::Terminal(Key::Kp1),
    ),
    (
        7,
        "VT numeric keypad",
        "Num2",
        KeyboardAction::Terminal(Key::Kp2),
    ),
    (
        8,
        "VT numeric keypad",
        "Num3",
        KeyboardAction::Terminal(Key::Kp3),
    ),
    (
        9,
        "VT numeric keypad",
        "Num4",
        KeyboardAction::Terminal(Key::Kp4),
    ),
    (
        10,
        "VT numeric keypad",
        "Num5",
        KeyboardAction::Terminal(Key::Kp5),
    ),
    (
        11,
        "VT numeric keypad",
        "Num6",
        KeyboardAction::Terminal(Key::Kp6),
    ),
    (
        12,
        "VT numeric keypad",
        "Num7",
        KeyboardAction::Terminal(Key::Kp7),
    ),
    (
        13,
        "VT numeric keypad",
        "Num8",
        KeyboardAction::Terminal(Key::Kp8),
    ),
    (
        14,
        "VT numeric keypad",
        "Num9",
        KeyboardAction::Terminal(Key::Kp9),
    ),
    (
        15,
        "VT numeric keypad",
        "NumMinus",
        KeyboardAction::Terminal(Key::KpMinus),
    ),
    (
        16,
        "VT numeric keypad",
        "NumComma",
        KeyboardAction::Terminal(Key::KpComma),
    ),
    (
        17,
        "VT numeric keypad",
        "NumPeriod",
        KeyboardAction::Terminal(Key::KpPeriod),
    ),
    (
        18,
        "VT numeric keypad",
        "NumSlash",
        KeyboardAction::Terminal(Key::KpSlash),
    ),
    (
        19,
        "VT numeric keypad",
        "NumAsterisk",
        KeyboardAction::Terminal(Key::KpAsterisk),
    ),
    (
        20,
        "VT numeric keypad",
        "NumPlus",
        KeyboardAction::Terminal(Key::KpPlus),
    ),
    (
        21,
        "VT numeric keypad",
        "NumEnter",
        KeyboardAction::Terminal(Key::KpEnter),
    ),
    (
        22,
        "VT numeric keypad",
        "PF1",
        KeyboardAction::Terminal(Key::Pf1),
    ),
    (
        23,
        "VT numeric keypad",
        "PF2",
        KeyboardAction::Terminal(Key::Pf2),
    ),
    (
        24,
        "VT numeric keypad",
        "PF3",
        KeyboardAction::Terminal(Key::Pf3),
    ),
    (
        25,
        "VT numeric keypad",
        "PF4",
        KeyboardAction::Terminal(Key::Pf4),
    ),
    (
        26,
        "VT editor keypad",
        "Find",
        KeyboardAction::Terminal(Key::Find),
    ),
    (
        27,
        "VT editor keypad",
        "Insert",
        KeyboardAction::Terminal(Key::Insert),
    ),
    (
        28,
        "VT editor keypad",
        "Remove",
        KeyboardAction::Terminal(Key::Remove),
    ),
    (
        29,
        "VT editor keypad",
        "Select",
        KeyboardAction::Terminal(Key::Select),
    ),
    (
        30,
        "VT editor keypad",
        "Prev",
        KeyboardAction::Terminal(Key::Prev),
    ),
    (
        31,
        "VT editor keypad",
        "Next",
        KeyboardAction::Terminal(Key::Next),
    ),
    (
        32,
        "VT function keys",
        "F6",
        KeyboardAction::Terminal(Key::F6),
    ),
    (
        33,
        "VT function keys",
        "F7",
        KeyboardAction::Terminal(Key::F7),
    ),
    (
        34,
        "VT function keys",
        "F8",
        KeyboardAction::Terminal(Key::F8),
    ),
    (
        35,
        "VT function keys",
        "F9",
        KeyboardAction::Terminal(Key::F9),
    ),
    (
        36,
        "VT function keys",
        "F10",
        KeyboardAction::Terminal(Key::F10),
    ),
    (
        37,
        "VT function keys",
        "F11",
        KeyboardAction::Terminal(Key::F11),
    ),
    (
        38,
        "VT function keys",
        "F12",
        KeyboardAction::Terminal(Key::F12),
    ),
    (
        39,
        "VT function keys",
        "F13",
        KeyboardAction::Terminal(Key::F13),
    ),
    (
        40,
        "VT function keys",
        "F14",
        KeyboardAction::Terminal(Key::F14),
    ),
    (
        41,
        "VT function keys",
        "Help",
        KeyboardAction::Terminal(Key::Help),
    ),
    (
        42,
        "VT function keys",
        "Do",
        KeyboardAction::Terminal(Key::Do),
    ),
    (
        43,
        "VT function keys",
        "F17",
        KeyboardAction::Terminal(Key::F17),
    ),
    (
        44,
        "VT function keys",
        "F18",
        KeyboardAction::Terminal(Key::F18),
    ),
    (
        45,
        "VT function keys",
        "F19",
        KeyboardAction::Terminal(Key::F19),
    ),
    (
        46,
        "VT function keys",
        "F20",
        KeyboardAction::Terminal(Key::F20),
    ),
    (
        47,
        "X function keys",
        "XF1",
        KeyboardAction::Terminal(Key::XF1),
    ),
    (
        48,
        "X function keys",
        "XF2",
        KeyboardAction::Terminal(Key::XF2),
    ),
    (
        49,
        "X function keys",
        "XF3",
        KeyboardAction::Terminal(Key::XF3),
    ),
    (
        50,
        "X function keys",
        "XF4",
        KeyboardAction::Terminal(Key::XF4),
    ),
    (
        51,
        "X function keys",
        "XF5",
        KeyboardAction::Terminal(Key::XF5),
    ),
    (52, "VT function keys", "UDK6", KeyboardAction::Udk(6)),
    (53, "VT function keys", "UDK7", KeyboardAction::Udk(7)),
    (54, "VT function keys", "UDK8", KeyboardAction::Udk(8)),
    (55, "VT function keys", "UDK9", KeyboardAction::Udk(9)),
    (56, "VT function keys", "UDK10", KeyboardAction::Udk(10)),
    (57, "VT function keys", "UDK11", KeyboardAction::Udk(11)),
    (58, "VT function keys", "UDK12", KeyboardAction::Udk(12)),
    (59, "VT function keys", "UDK13", KeyboardAction::Udk(13)),
    (60, "VT function keys", "UDK14", KeyboardAction::Udk(14)),
    (61, "VT function keys", "UDK15", KeyboardAction::Udk(15)),
    (62, "VT function keys", "UDK16", KeyboardAction::Udk(16)),
    (63, "VT function keys", "UDK17", KeyboardAction::Udk(17)),
    (64, "VT function keys", "UDK18", KeyboardAction::Udk(18)),
    (65, "VT function keys", "UDK19", KeyboardAction::Udk(19)),
    (66, "VT function keys", "UDK20", KeyboardAction::Udk(20)),
    (
        67,
        "VT function keys",
        "Hold",
        KeyboardAction::Terminal(Key::Hold),
    ),
    (
        68,
        "VT function keys",
        "Print",
        KeyboardAction::Terminal(Key::Print),
    ),
    (
        69,
        "VT function keys",
        "Break",
        KeyboardAction::Terminal(Key::Break),
    ),
    (
        70,
        "X function keys",
        "XBackTab",
        KeyboardAction::Terminal(Key::BackTab),
    ),
    (
        71,
        "Shortcut keys",
        "EditCopy",
        KeyboardAction::Shortcut(Shortcut::EditCopy),
    ),
    (
        72,
        "Shortcut keys",
        "EditPaste",
        KeyboardAction::Shortcut(Shortcut::EditPaste),
    ),
    (
        73,
        "Shortcut keys",
        "EditPasteCR",
        KeyboardAction::Shortcut(Shortcut::EditPasteCr),
    ),
    (
        74,
        "Shortcut keys",
        "EditCLS",
        KeyboardAction::Shortcut(Shortcut::EditClearScreen),
    ),
    (
        75,
        "Shortcut keys",
        "EditCLB",
        KeyboardAction::Shortcut(Shortcut::EditClearBuffer),
    ),
    (
        76,
        "Shortcut keys",
        "ControlOpenTEK",
        KeyboardAction::Shortcut(Shortcut::ControlOpenTek),
    ),
    (
        77,
        "Shortcut keys",
        "ControlCloseTEK",
        KeyboardAction::Shortcut(Shortcut::ControlCloseTek),
    ),
    (
        78,
        "Shortcut keys",
        "LineUp",
        KeyboardAction::Shortcut(Shortcut::LineUp),
    ),
    (
        79,
        "Shortcut keys",
        "LineDown",
        KeyboardAction::Shortcut(Shortcut::LineDown),
    ),
    (
        80,
        "Shortcut keys",
        "PageUp",
        KeyboardAction::Shortcut(Shortcut::PageUp),
    ),
    (
        81,
        "Shortcut keys",
        "PageDown",
        KeyboardAction::Shortcut(Shortcut::PageDown),
    ),
    (
        82,
        "Shortcut keys",
        "BuffTop",
        KeyboardAction::Shortcut(Shortcut::BufferTop),
    ),
    (
        83,
        "Shortcut keys",
        "BuffBottom",
        KeyboardAction::Shortcut(Shortcut::BufferBottom),
    ),
    (
        84,
        "Shortcut keys",
        "NextWin",
        KeyboardAction::Shortcut(Shortcut::NextWindow),
    ),
    (
        85,
        "Shortcut keys",
        "PrevWin",
        KeyboardAction::Shortcut(Shortcut::PreviousWindow),
    ),
    (
        86,
        "Shortcut keys",
        "NextShownWin",
        KeyboardAction::Shortcut(Shortcut::NextShownWindow),
    ),
    (
        87,
        "Shortcut keys",
        "PrevShownWin",
        KeyboardAction::Shortcut(Shortcut::PreviousShownWindow),
    ),
    (
        88,
        "Shortcut keys",
        "LocalEcho",
        KeyboardAction::Shortcut(Shortcut::LocalEcho),
    ),
    (
        89,
        "Shortcut keys",
        "ScrollLock",
        KeyboardAction::Shortcut(Shortcut::ScrollLock),
    ),
];

/// `ReadList`'s eleven-wide-character buffer and `%hd` conversion.
fn fixed_scan(value: Option<&str>) -> Option<u16> {
    let value: String = value?.chars().take(10).collect();
    if value.eq_ignore_ascii_case("off") {
        return None;
    }
    decimal_prefix(&value)
        .map(|n| n as u16)
        .filter(|&n| n != u16::MAX)
}

/// `ReadUserkeysSection`'s 256-wide-character buffer and `swscanf_s` format.
fn user_key(value: Option<&str>) -> Option<(u16, UserKey)> {
    let value: String = value?.chars().take(255).collect();
    if value.get(..value.len().min(3))?.eq_ignore_ascii_case("off") {
        return None;
    }

    let (scan, rest) = decimal_field(&value)?;
    let rest = rest.strip_prefix(',')?;
    let (kind, rest) = decimal_field(rest)?;
    let text = rest.strip_prefix(',')?;
    if text.is_empty() {
        return None;
    }
    let kind = match kind {
        0 => UserKeyType::Binary,
        1 => UserKeyType::Text,
        2 => UserKeyType::Macro,
        3 => UserKeyType::Command,
        n => UserKeyType::Unknown(n),
    };
    Some((
        scan as u16,
        UserKey {
            kind,
            value: text.to_string(),
        },
    ))
}

fn decimal_prefix(s: &str) -> Option<i32> {
    decimal_field(s).map(|(n, _)| n)
}

/// The prefix `%d` accepts: optional leading whitespace, one sign and at least
/// one ASCII digit. It leaves trailing whitespace in the input, so the literal
/// comma in `%d,%d` then fails — an easy detail to erase with `split(',')`.
fn decimal_field(s: &str) -> Option<(i32, &str)> {
    let trimmed = s.trim_start_matches(char::is_whitespace);
    let bytes = trimmed.as_bytes();
    let sign = matches!(bytes.first(), Some(b'+') | Some(b'-')) as usize;
    let digits = bytes[sign..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let end = sign + digits;
    let n = trimmed[..end].parse::<i64>().ok()? as i32;
    Some((n, &trimmed[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> KeyboardMap {
        KeyboardMap::from_ini(&Ini::parse(s.as_bytes()))
    }

    #[test]
    fn fixed_sections_map_scan_codes_to_terminal_and_local_actions() {
        let map = parse(
            "[VT editor keypad]\nUp=328\n[VT numeric keypad]\nPF1=59\n\
             [Shortcut keys]\nEditPaste=850\n",
        );
        assert_eq!(map.get(328), Some(&KeyboardAction::Terminal(Key::Up)));
        assert_eq!(map.get(59), Some(&KeyboardAction::Terminal(Key::Pf1)));
        assert_eq!(
            map.get(850),
            Some(&KeyboardAction::Shortcut(Shortcut::EditPaste))
        );
    }

    #[test]
    fn absent_empty_off_and_bad_fixed_values_are_unbound() {
        let map = parse("[VT editor keypad]\nUp=\nDown=off\nRight=offline\nLeft=nope\n");
        assert!(map.is_empty());
    }

    #[test]
    fn fixed_numbers_follow_percent_hd_prefix_and_word_narrowing() {
        let map = parse("[VT editor keypad]\nUp=328junk\nDown=-2\nRight=65537\nLeft=-1\n");
        assert_eq!(map.get(328), Some(&KeyboardAction::Terminal(Key::Up)));
        assert_eq!(map.get(65534), Some(&KeyboardAction::Terminal(Key::Down)));
        assert_eq!(map.get(1), Some(&KeyboardAction::Terminal(Key::Right)));
        assert_eq!(map.len(), 3, "-1 is the unbound sentinel");
    }

    #[test]
    fn later_internal_key_id_wins_a_duplicate() {
        let map = parse(
            "[VT editor keypad]\nUp=59\n[VT numeric keypad]\nPF1=59\n\
             [User keys]\nUser1=59,0,override\n",
        );
        assert_eq!(
            map.get(59),
            Some(&KeyboardAction::User(UserKey {
                kind: UserKeyType::Binary,
                value: "override".into(),
            }))
        );
        assert_eq!(map.duplicates(), &[59, 59]);
    }

    #[test]
    fn user_keys_keep_the_payload_and_all_four_types() {
        let map = parse(
            "[User keys]\n\
             User1=1083,0,$0D$0A\n\
             User2=1084,1,$0D\n\
             User3=1085,2,test.ttl\n\
             User4=1086,3,50110\n\
             User5=1087,9,nothing\n",
        );
        let kinds = [
            UserKeyType::Binary,
            UserKeyType::Text,
            UserKeyType::Macro,
            UserKeyType::Command,
            UserKeyType::Unknown(9),
        ];
        for (scan, kind) in (1083..=1087).zip(kinds) {
            let Some(KeyboardAction::User(user)) = map.get(scan) else {
                panic!("missing user key {scan}");
            };
            assert_eq!(user.kind, kind);
        }
        let Some(KeyboardAction::User(user)) = map.get(1083) else {
            unreachable!()
        };
        assert_eq!(user.value, "$0D$0A");
    }

    #[test]
    fn user_off_is_a_prefix_and_the_scan_fields_require_adjacent_commas() {
        let map =
            parse("[User keys]\nUser1=offline\nUser2=100 ,0,no\nUser3=101, 1,yes\nUser4=102,0,\n");
        assert_eq!(
            map.get(101),
            Some(&KeyboardAction::User(UserKey {
                kind: UserKeyType::Text,
                value: "yes".into(),
            }))
        );
        assert_eq!(map.len(), 1);
    }
}
