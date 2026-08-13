//! Quick buttons — a list of commands the window keeps one click away.
//!
//! Upstream has nothing like this, so `[Sterna Buttons]` is one of this
//! program's own sections and no real Tera Term reads it (`docs/deviations.md`).
//! What a button *does*, though, is entirely upstream's: the four kinds are
//! [`UserKeyType`], the `KEYBOARD.CNF` `[User keys]` types, and pressing one
//! goes through the same [`crate::keyboard::UserKey`] dispatch a physical key
//! does. A quick button is a user key with a label and a face.
//!
//! ```ini
//! [Sterna Buttons]
//! Button1Label=Show version
//! Button1Kind=text
//! Button1Value=show version$0D
//! Button1Shortcut=Ctrl+Alt+1
//! Button1Confirm=off
//! ```
//!
//! Five keys per button rather than one comma-separated line, because a label
//! and a command both contain commas and the alternative is an escape scheme
//! nobody can hand-edit. These are meant to be edited by hand: the dialog is
//! the convenience, not the format.
//!
//! `Button1..Button99` are scanned the way `[User keys]` scans `User1..User99`
//! — a gap is skipped, and the order buttons appear in is index order.

use std::path::Path;

use crate::hex::{hex_decode_str, hex_escape_str};
use crate::keyboard::{UserKey, UserKeyType};
use crate::schema::on_off;
use crate::Ini;

/// The INI section. Not `[Sterna]`: that one is scalar settings the schema
/// generates code for, and this is a list the schema cannot describe.
pub const SECTION: &str = "Sterna Buttons";

/// How many are looked for, matching `[User keys]`' own ceiling.
pub const MAX: usize = 99;

/// One button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Button {
    /// What is written on it. Plain text: no `$HH` decoding, because a label
    /// needs no control characters and a `$` in one should be a `$`.
    pub label: String,
    pub kind: UserKeyType,
    /// The value **as stored**, which for [`UserKeyType::Text`] and
    /// [`UserKeyType::Binary`] is still `$HH`-escaped.
    ///
    /// Kept escaped rather than decoded because this is what
    /// [`UserKey::value`] is, and the send path decodes it there. A macro's
    /// path and a command's number are literal in both.
    pub value: String,
    /// A Qt key sequence in portable spelling — `Ctrl+Alt+1` — or empty, which
    /// is the shipping state. A shortcut is a key taken away from the host, so
    /// nothing assigns one on a user's behalf.
    pub shortcut: String,
    /// Whether to ask before running it. For the `reload` button sitting next
    /// to the `show version` one.
    pub confirm: bool,
}

impl Button {
    /// The action to hand [`crate::keyboard::UserKey`]'s dispatch.
    pub fn action(&self) -> UserKey {
        UserKey {
            kind: self.kind,
            value: self.value.clone(),
        }
    }

    /// The value with the escape undone, for showing and editing.
    ///
    /// Only the two sending kinds are escaped. A macro's value is a path and a
    /// command's is a decimal id, and upstream uses both raw — `RunMacro` hands
    /// the string straight to the launcher and `command_id` parses it — so
    /// decoding them here would change what a `$` in a path means.
    pub fn text(&self) -> String {
        match self.kind {
            UserKeyType::Text | UserKeyType::Binary => hex_decode_str(&self.value),
            _ => self.value.clone(),
        }
    }

    /// Build one from unescaped text, the inverse of [`Button::text`].
    pub fn with_text(kind: UserKeyType, text: &str) -> Button {
        Button {
            kind,
            value: encode(kind, text),
            ..Button::default()
        }
    }

    /// Whether pressing it would put a `CR` on the wire last — what the
    /// dialog's "Send Enter after" box is a view of.
    pub fn sends_enter(&self) -> bool {
        matches!(self.kind, UserKeyType::Text | UserKeyType::Binary) && self.text().ends_with('\r')
    }
}

/// Escape `text` for storage under `kind`.
pub fn encode(kind: UserKeyType, text: &str) -> String {
    match kind {
        UserKeyType::Text | UserKeyType::Binary => hex_escape_str(text),
        // A path and a number, and `Ini::set` refuses a line ending in either.
        _ => text.replace(['\r', '\n'], ""),
    }
}

/// The file's spelling of a kind.
///
/// Words rather than upstream's integers: `[User keys]` writes `0`, `1`, `2`,
/// `3` because a dialog wrote them, and this section is meant to be read.
pub fn kind_name(kind: UserKeyType) -> &'static str {
    match kind {
        UserKeyType::Text => "text",
        UserKeyType::Binary => "bytes",
        UserKeyType::Macro => "macro",
        UserKeyType::Command => "command",
        UserKeyType::Unknown(_) => "",
    }
}

/// Parse a `Kind` value. `binary` is accepted for the same thing as `bytes`,
/// since that is what the rest of this codebase calls it.
///
/// An unrecognised spelling is `None` and the button is skipped. Deliberately
/// unlike the settings schema, whose enums take a default for anything they do
/// not know: a button whose action could not be read must not become a button
/// that does *something else* when it is clicked.
pub fn parse_kind(value: &str) -> Option<UserKeyType> {
    let value = value.trim();
    for kind in [
        UserKeyType::Text,
        UserKeyType::Binary,
        UserKeyType::Macro,
        UserKeyType::Command,
    ] {
        if value.eq_ignore_ascii_case(kind_name(kind)) {
            return Some(kind);
        }
    }
    if value.eq_ignore_ascii_case("binary") {
        return Some(UserKeyType::Binary);
    }
    None
}

fn key(index: usize, field: &str) -> String {
    format!("Button{index}{field}")
}

/// Read the section. Order is index order, and a gap is skipped.
pub fn from_ini(ini: &Ini) -> Vec<Button> {
    let mut out = Vec::new();
    for i in 1..=MAX {
        let label = ini.get(SECTION, &key(i, "Label"));
        let value = ini.get(SECTION, &key(i, "Value"));
        // A button needs *something*; a stray `Button7Confirm=on` left behind
        // by hand is not one.
        if label.is_none() && value.is_none() {
            continue;
        }
        let Some(kind) = parse_kind(ini.get(SECTION, &key(i, "Kind")).unwrap_or("text")) else {
            continue;
        };
        out.push(Button {
            label: label.unwrap_or_default().to_string(),
            kind,
            value: value.unwrap_or_default().to_string(),
            shortcut: ini
                .get(SECTION, &key(i, "Shortcut"))
                .unwrap_or_default()
                .trim()
                .to_string(),
            // Default off, so `GetOnOff`'s asymmetric parse means only a
            // literal `on` arms the confirmation. The safe direction: a
            // typo leaves a button that runs, not one that cannot.
            confirm: on_off(ini.get(SECTION, &key(i, "Confirm")), false),
        });
    }
    out
}

/// Replace the section with `buttons`, leaving the rest of the file alone.
///
/// Every index up to [`MAX`] is cleared first, so removing a button removes
/// its keys rather than leaving a shorter list in front of an orphan.
pub fn write_into(ini: &mut Ini, buttons: &[Button]) {
    for i in 1..=MAX {
        for field in ["Label", "Kind", "Value", "Shortcut", "Confirm"] {
            ini.remove(SECTION, &key(i, field));
        }
    }
    for (n, button) in buttons.iter().take(MAX).enumerate() {
        let i = n + 1;
        ini.set(SECTION, &key(i, "Label"), &button.label);
        ini.set(SECTION, &key(i, "Kind"), kind_name(button.kind));
        ini.set(SECTION, &key(i, "Value"), &button.value);
        if !button.shortcut.is_empty() {
            ini.set(SECTION, &key(i, "Shortcut"), &button.shortcut);
        }
        if button.confirm {
            ini.set(SECTION, &key(i, "Confirm"), "on");
        }
    }
}

/// Read the buttons out of a settings file. A file that is not there is a
/// first run and has no buttons, not an error.
pub fn load(path: &Path) -> Vec<Button> {
    match Ini::load(path) {
        Ok(ini) => from_ini(&ini),
        Err(_) => Vec::new(),
    }
}

/// Write them back into `path`, preserving everything else in it — comments,
/// ordering, and every setting this program does not know about.
pub fn save(path: &Path, buttons: &[Button]) -> std::io::Result<()> {
    let mut ini = Ini::load(path).unwrap_or_else(|_| Ini::new());
    write_into(&mut ini, buttons);
    ini.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<Button> {
        from_ini(&Ini::parse(s.as_bytes()))
    }

    #[test]
    fn a_button_is_five_keys() {
        let b = parse(
            "[Sterna Buttons]\nButton1Label=Show version\nButton1Kind=text\n\
             Button1Value=show version$0D\nButton1Shortcut=Ctrl+Alt+1\n\
             Button1Confirm=on\n",
        );
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].label, "Show version");
        assert_eq!(b[0].kind, UserKeyType::Text);
        assert_eq!(b[0].value, "show version$0D");
        assert_eq!(b[0].text(), "show version\r");
        assert_eq!(b[0].shortcut, "Ctrl+Alt+1");
        assert!(b[0].confirm);
        assert!(b[0].sends_enter());
    }

    #[test]
    fn the_defaults_are_text_no_shortcut_and_no_question() {
        let b = parse("[Sterna Buttons]\nButton1Value=uptime$0D\n");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, UserKeyType::Text);
        assert_eq!(b[0].label, "");
        assert!(b[0].shortcut.is_empty());
        assert!(!b[0].confirm);
    }

    #[test]
    fn confirm_is_get_on_off_with_a_default_of_off() {
        // The asymmetric parse: default off means only a literal `on` is on,
        // so `1` and `yes` are both off. Same rule as the schema's booleans.
        for (value, armed) in [("on", true), ("ON", true), ("1", false), ("yes", false)] {
            let b = parse(&format!(
                "[Sterna Buttons]\nButton1Value=x\nButton1Confirm={value}\n"
            ));
            assert_eq!(b[0].confirm, armed, "Confirm={value}");
        }
    }

    #[test]
    fn a_gap_is_skipped_and_the_order_is_the_index() {
        let b = parse(
            "[Sterna Buttons]\nButton3Label=third\nButton3Value=c\n\
             Button1Label=first\nButton1Value=a\n",
        );
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].label, "first");
        assert_eq!(b[1].label, "third");
    }

    #[test]
    fn a_key_with_no_button_behind_it_is_not_one() {
        // Only `Confirm` — nothing to run, so nothing to show.
        assert!(parse("[Sterna Buttons]\nButton1Confirm=on\n").is_empty());
        assert!(parse("[Sterna Buttons]\n").is_empty());
        assert!(parse("").is_empty());
    }

    #[test]
    fn an_unreadable_kind_drops_the_button_rather_than_guessing() {
        let b = parse(
            "[Sterna Buttons]\nButton1Value=a\nButton1Kind=sausage\n\
             Button2Value=b\nButton2Kind=command\n",
        );
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, UserKeyType::Command);
    }

    #[test]
    fn the_kind_spellings_are_words_and_binary_is_an_alias() {
        assert_eq!(parse_kind("text"), Some(UserKeyType::Text));
        assert_eq!(parse_kind("BYTES"), Some(UserKeyType::Binary));
        assert_eq!(parse_kind(" binary "), Some(UserKeyType::Binary));
        assert_eq!(parse_kind("macro"), Some(UserKeyType::Macro));
        assert_eq!(parse_kind("command"), Some(UserKeyType::Command));
        assert_eq!(parse_kind("1"), None);
    }

    #[test]
    fn only_the_sending_kinds_are_escaped() {
        // A macro's value is a path and upstream uses it raw, so a `$` in one
        // has to survive as a `$`.
        let b = Button {
            kind: UserKeyType::Macro,
            value: "/home/me/$scripts/login.ttl".into(),
            ..Button::default()
        };
        assert_eq!(b.text(), "/home/me/$scripts/login.ttl");
        assert_eq!(
            encode(UserKeyType::Macro, "/home/me/$scripts/login.ttl"),
            "/home/me/$scripts/login.ttl"
        );
        assert_eq!(encode(UserKeyType::Text, "$"), "$24");
    }

    #[test]
    fn a_written_section_reads_back_the_same() {
        let buttons = vec![
            Button {
                label: "Show version".into(),
                kind: UserKeyType::Text,
                value: encode(UserKeyType::Text, "show version\r"),
                shortcut: "Ctrl+Alt+1".into(),
                confirm: false,
            },
            Button {
                label: "Reload".into(),
                kind: UserKeyType::Text,
                value: encode(UserKeyType::Text, "reload\r"),
                shortcut: String::new(),
                confirm: true,
            },
            Button {
                label: "Break".into(),
                kind: UserKeyType::Command,
                value: "50430".into(),
                shortcut: String::new(),
                confirm: false,
            },
        ];
        let mut ini = Ini::new();
        write_into(&mut ini, &buttons);
        assert_eq!(from_ini(&ini), buttons);
        // ...and the file says what a person would write by hand.
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(text.contains("Button1Value=show version$0D"), "{text}");
        assert!(text.contains("Button3Kind=command"), "{text}");
    }

    #[test]
    fn removing_a_button_removes_its_keys() {
        let mut ini = Ini::parse(
            b"[Tera Term]\nBaudRate=115200\n[Sterna Buttons]\n\
              Button1Label=one\nButton1Value=a\nButton1Confirm=on\n\
              Button2Label=two\nButton2Value=b\nButton2Shortcut=Ctrl+Alt+2\n",
        );
        write_into(
            &mut ini,
            &[Button {
                label: "two".into(),
                kind: UserKeyType::Text,
                value: "b".into(),
                ..Button::default()
            }],
        );
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(!text.contains("Button2"), "{text}");
        assert!(!text.contains("Confirm"), "{text}");
        assert!(text.contains("Button1Label=two"), "{text}");
        // Everything that is not ours is left exactly where it was.
        assert!(text.contains("BaudRate=115200"), "{text}");
    }

    #[test]
    fn a_multi_line_command_survives_the_file() {
        // `Ini::set` refuses a value with a line ending in it, so the escape
        // is what makes this storable at all.
        let b = Button::with_text(UserKeyType::Text, "conf t\rinterface eth0\r");
        let mut ini = Ini::new();
        write_into(&mut ini, std::slice::from_ref(&b));
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("Button1")).count(), 3);
        assert_eq!(from_ini(&ini)[0].text(), "conf t\rinterface eth0\r");
    }
}
