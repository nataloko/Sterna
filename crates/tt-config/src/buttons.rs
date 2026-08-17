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
//! Page2Name=BMCs
//!
//! Button1Label=Show version
//! Button1Kind=text
//! Button1Value=show version$0D
//! Button1Shortcut=Ctrl+Alt+1
//! Button1Confirm=off
//! Button1Repeat=1
//! Button1IntervalMs=1000
//!
//! Button2Label=Power status
//! Button2Page=2
//! Button2Value=power status$0D
//! ```
//!
//! A key per field rather than one comma-separated line, because a label and a
//! command both contain commas and the alternative is an escape scheme nobody
//! can hand-edit. These are meant to be edited by hand: the dialog is the
//! convenience, not the format.
//!
//! `Button1..Button99` are scanned the way `[User keys]` scans `User1..User99`
//! — a gap is skipped, and the order buttons appear in is index order.
//!
//! **Pages are a field on a button, not a section of their own.** A flat list
//! is the wrong shape as soon as somebody keeps commands for four different
//! devices, but the answer that keeps everything else working is one more key:
//! `Page` is absent for page 1, so a file with one page is byte-for-byte the
//! file this section held before pages existed, and the *index* stays flat —
//! which matters because a running repeat is an index and nothing else. The
//! writer groups the pages as it renumbers, so the file reads in page order
//! even though the reader does not need it to.

use std::path::Path;

use crate::hex::{hex_decode_str, hex_escape_str};
use crate::keyboard::{UserKey, UserKeyType};
use crate::schema::on_off;
use crate::Ini;

/// The INI section. Not `[Sterna]`: that one is scalar settings the schema
/// generates code for, and this is a list the schema cannot describe.
pub const SECTION: &str = "Sterna Buttons";

/// How many are looked for, matching `[User keys]`' own ceiling.
///
/// The whole section, every page together — so pages divide these ninety-nine
/// rather than multiplying them, and the `Button1..Button99` scan is the same
/// scan it always was.
pub const MAX: usize = 99;

/// How many pages there may be — the same ceiling, for a reason.
///
/// Not a smaller, friendlier number. A page above the ceiling is clamped to
/// it, and a low ceiling therefore *merges* pages on somebody's hand-edited
/// file: `Page=30` and `Page=40` become one page and nothing says so. Tying it
/// to [`MAX`] means the only files that can collide are ones naming pages that
/// could not hold a button anyway. This is a corruption guard, exactly as
/// [`MAX`] is; how many pages are usable is the drop-down's business.
pub const MAX_PAGES: u32 = MAX as u32;

/// [`Button::repeat`] for a run with no end — until it is stopped, or until
/// the link it is sending down goes away.
///
/// A sentinel rather than `0`, because a zeroed C `TtQuickButton` is how a
/// frontend says "none" for every other optional field, and a struct that
/// means *send this forever* when it is left blank is the wrong direction for
/// a mistake to point. The file spells it `Repeat=forever`.
pub const REPEAT_FOREVER: u32 = u32::MAX;

/// The most sends one press may be asked for. Four digits is a long afternoon
/// at any sane interval, and past it [`REPEAT_FOREVER`] is what was meant.
pub const MAX_REPEAT: u32 = 9999;

/// The floor on [`Button::interval_ms`]. Ten a second is already faster than
/// anything on the other end can answer, and the point of the floor is that a
/// mistyped interval cannot turn a button into a flood.
pub const MIN_INTERVAL_MS: u32 = 100;

/// ...and the ceiling: an hour between sends.
pub const MAX_INTERVAL_MS: u32 = 60 * 60 * 1000;

/// What a repeating button waits when the file does not say.
pub const DEFAULT_INTERVAL_MS: u32 = 1000;

/// One button.
///
/// [`Button::default`] is written out rather than derived, because a derived
/// one would have `repeat: 0` — a button that sends nothing, or under a
/// different reading one that never stops. One is the count that means "press
/// it, it happens once", and that is the button everybody has.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// How many times one press sends it: `1` is once, which is every button
    /// that has never heard of this field, and [`REPEAT_FOREVER`] is a run
    /// with no end. Bounded by [`MAX_REPEAT`].
    ///
    /// The *clock* is not here. This crate reads a file; a repeat needs a
    /// timer and a live link, so the frontend owns the run and this is only
    /// what it was asked for — the same split as the bell governor.
    pub repeat: u32,
    /// Milliseconds between the starts of two sends, when `repeat` is not 1.
    /// Bounded by [`MIN_INTERVAL_MS`] and [`MAX_INTERVAL_MS`].
    pub interval_ms: u32,
    /// For [`UserKeyType::SendFile`]: what holds each line until the far end
    /// has answered, or `None` to use whatever the settings say.
    ///
    /// Per button rather than only in the settings, because two pages of
    /// buttons is exactly how somebody keeps a switch's `#` and a boot loader's
    /// silence apart. The *numbers* — the interval, the timeout — stay in the
    /// settings: those are about how patient this machine is, not about which
    /// device is on the other end.
    pub gate: Option<SendGate>,
    /// The pattern for [`SendGate::Prompt`]. Empty falls back to the setting.
    pub prompt: String,
    /// Which page of the panel it is on, counting from 1. Bounded by
    /// [`MAX_PAGES`]; `0` reads as 1, so a zeroed C struct is an ordinary
    /// button on the first page.
    ///
    /// A number on the button rather than a list of lists, because everything
    /// above this — a repeat in progress, the shortcut installed on an action,
    /// the frontend's parallel vectors — is keyed on a button's position in one
    /// flat list. A page filters what is on screen; it never renumbers.
    pub page: u32,
}

impl Default for Button {
    fn default() -> Button {
        Button {
            label: String::new(),
            kind: UserKeyType::default(),
            value: String::new(),
            shortcut: String::new(),
            confirm: false,
            repeat: 1,
            interval_ms: DEFAULT_INTERVAL_MS,
            gate: None,
            prompt: String::new(),
            page: 1,
        }
    }
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
            // A macro's value is a path, a command's a decimal id, and a file
            // send's a path again — all three are used raw upstream and here,
            // so decoding would change what a `$` in a path means.
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

    /// Whether one press sends more than once.
    pub fn repeats(&self) -> bool {
        self.repeat != 1
    }

    /// Whether pressing it starts something only a person can stop.
    pub fn repeats_forever(&self) -> bool {
        self.repeat == REPEAT_FOREVER
    }

    /// Put `repeat`, `interval_ms` and `page` inside their bounds.
    ///
    /// Called wherever a button arrives from outside this crate — the file
    /// reader and the C ABI both — so that one place decides what an out-of-
    /// range number means and nothing downstream has to defend itself against
    /// a zero interval.
    pub fn normalize(&mut self) {
        // A page is clamped rather than defaulted: the number names where
        // somebody put this button, and a typo above the ceiling should land it
        // on the last page rather than move it back to the first, where it
        // would sit among commands for a different device.
        self.page = self.page.clamp(1, MAX_PAGES);
        if self.repeat != REPEAT_FOREVER {
            // Zero is the one value with two readings, and neither is what a
            // button is for: it is either "send nothing" or, to somebody who
            // has met a different program, "send forever". Take it as the
            // file having said nothing.
            self.repeat = self.repeat.clamp(1, MAX_REPEAT);
        }
        self.interval_ms = self.interval_ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
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

/// What a [`Button`] waits for after each line of a file it sends.
///
/// The same four the settings have (`transfer.send_gate`), spelled here as
/// well because `[Sterna Buttons]` is read by this crate and the settings enum
/// is generated beside it — one more `use` across that seam would be a
/// dependency in the wrong direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendGate {
    #[default]
    None,
    Prompt,
    Echo,
    Quiet,
}

/// The word a [`SendGate`] is written as.
pub fn gate_name(gate: SendGate) -> &'static str {
    match gate {
        SendGate::None => "none",
        SendGate::Prompt => "prompt",
        SendGate::Echo => "echo",
        SendGate::Quiet => "quiet",
    }
}

/// ...and back. `None` for anything unrecognised, which reads as "the settings
/// decide" — the same answer an absent key gives, and the safe one: a typo
/// leaves a button that sends, not one that holds every line.
pub fn parse_gate(value: &str) -> Option<SendGate> {
    let value = value.trim();
    [
        SendGate::None,
        SendGate::Prompt,
        SendGate::Echo,
        SendGate::Quiet,
    ]
    .into_iter()
    .find(|g| value.eq_ignore_ascii_case(gate_name(*g)))
}

/// The file's spelling of a kind.
///
/// Words rather than upstream's integers: `[User keys]` writes `0`, `1`, `2`,
/// `3` because a dialog wrote them, and this section is meant to be read. It is
/// also what lets a fifth kind exist at all — `file` has no integer, so a
/// `KEYBOARD.CNF` stays portable between the two programs.
pub fn kind_name(kind: UserKeyType) -> &'static str {
    match kind {
        UserKeyType::Text => "text",
        UserKeyType::Binary => "bytes",
        UserKeyType::Macro => "macro",
        UserKeyType::Command => "command",
        UserKeyType::SendFile => "file",
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
        UserKeyType::SendFile,
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

/// Parse a `Repeat` value.
///
/// `forever` is the spelling, `0` is accepted for it because that is what
/// somebody will try first, and anything unreadable is one send — the safe
/// direction, since the alternative is a typo that starts a run.
pub fn parse_repeat(value: Option<&str>) -> u32 {
    let Some(value) = value.map(str::trim) else {
        return 1;
    };
    if value.eq_ignore_ascii_case("forever") || value == "0" {
        return REPEAT_FOREVER;
    }
    value.parse::<u32>().unwrap_or(1).clamp(1, MAX_REPEAT)
}

/// The file's spelling of a repeat count.
fn repeat_name(repeat: u32) -> String {
    if repeat == REPEAT_FOREVER {
        "forever".to_string()
    } else {
        repeat.to_string()
    }
}

fn key(index: usize, field: &str) -> String {
    format!("Button{index}{field}")
}

fn page_key(page: u32) -> String {
    format!("Page{page}Name")
}

/// The whole section: the buttons, and what the pages are called.
///
/// One type rather than two calls, because a save that took only the buttons
/// would write a file whose page names had quietly gone — and the names are the
/// only thing keeping a page somebody has just made and not yet filled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    pub items: Vec<Button>,
    /// Page names, `names[0]` being page 1's. An empty string is a page with no
    /// name, which a frontend shows as `Page N`; trailing empties are trimmed,
    /// so this is as long as the last *named* page and no longer.
    pub names: Vec<String>,
}

impl Buttons {
    /// How many pages there are: enough to hold every button, and every page
    /// that has been named. **A named page with nothing on it counts** — that
    /// is what makes Add page survive until somebody puts a command on it.
    pub fn page_count(&self) -> u32 {
        let used = self.items.iter().map(|b| b.page).max().unwrap_or(1);
        used.max(self.names.len() as u32).max(1)
    }

    /// What page `page` is called, or an empty string.
    pub fn name(&self, page: u32) -> &str {
        if page == 0 {
            return "";
        }
        self.names
            .get(page as usize - 1)
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Name `page`, growing the list if it has to. An empty name un-names it.
    pub fn set_name(&mut self, page: u32, name: &str) {
        if page == 0 || page > MAX_PAGES {
            return;
        }
        if self.names.len() < page as usize {
            self.names.resize(page as usize, String::new());
        }
        self.names[page as usize - 1] = name.to_string();
        self.trim_names();
    }

    /// Drop `page` and its name, moving its buttons to the page beside it and
    /// pulling every page above it down one.
    ///
    /// **Removing a page never removes a command.** Removing a page is
    /// arranging; removing a command is its own act, and the one place that
    /// asks before it happens. So the buttons land on the page before this one
    /// — or, for the first page, on what was the second — and nothing needs a
    /// confirmation, which is what makes pages safe to try out.
    ///
    /// Here rather than in a dialog because two frontends and a C ABI would
    /// otherwise each have their own idea of where page 3 goes when page 2
    /// does. It renumbers nothing in the flat list, so a caller's indices
    /// survive it.
    pub fn remove_page(&mut self, page: u32) {
        if page == 0 || page > MAX_PAGES {
            return;
        }
        let onto = page.saturating_sub(1).max(1);
        for button in &mut self.items {
            if button.page == page {
                button.page = onto;
            } else if button.page > page {
                button.page -= 1;
            }
        }
        if (page as usize) <= self.names.len() {
            self.names.remove(page as usize - 1);
        }
        self.trim_names();
    }

    /// Move a page and everything on it, the way dragging a tab would.
    pub fn move_page(&mut self, from: u32, to: u32) {
        let count = self.page_count();
        if from == 0 || to == 0 || from > count || to > count || from == to {
            return;
        }
        for button in &mut self.items {
            button.page = shift_page(button.page, from, to);
        }
        self.names.resize(count as usize, String::new());
        let moved = self.names.remove(from as usize - 1);
        self.names.insert(to as usize - 1, moved);
        self.trim_names();
    }

    /// Names are only ever as long as the last named page: a trailing empty
    /// would otherwise be a page that exists because somebody once typed a name
    /// and then removed it.
    fn trim_names(&mut self) {
        while self.names.last().is_some_and(String::is_empty) {
            self.names.pop();
        }
    }
}

/// Where page `page` lands when `from` is moved to `to`.
fn shift_page(page: u32, from: u32, to: u32) -> u32 {
    if page == from {
        to
    } else if from < to && page > from && page <= to {
        page - 1
    } else if to < from && page >= to && page < from {
        page + 1
    } else {
        page
    }
}

/// Read the section. Order is index order, and a gap is skipped.
pub fn from_ini(ini: &Ini) -> Buttons {
    let mut names: Vec<String> = (1..=MAX_PAGES)
        .map(|p| {
            ini.get(SECTION, &page_key(p))
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .collect();
    while names.last().is_some_and(String::is_empty) {
        names.pop();
    }

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
            repeat: parse_repeat(ini.get(SECTION, &key(i, "Repeat"))),
            interval_ms: ini
                .get(SECTION, &key(i, "IntervalMs"))
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(DEFAULT_INTERVAL_MS)
                .clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS),
            gate: ini.get(SECTION, &key(i, "Gate")).and_then(parse_gate),
            prompt: ini
                .get(SECTION, &key(i, "Prompt"))
                .unwrap_or_default()
                .to_string(),
            // Absent is page 1, which is every file written before pages
            // existed and every file belonging to somebody who never wanted
            // them.
            page: ini
                .get(SECTION, &key(i, "Page"))
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_PAGES),
        });
    }
    let mut out = Buttons { items: out, names };
    out.trim_names();
    out
}

/// Replace the section with `buttons`, leaving the rest of the file alone.
///
/// Every index up to [`MAX`] is cleared first, so removing a button removes
/// its keys rather than leaving a shorter list in front of an orphan.
///
/// **The buttons are written a page at a time**, renumbered densely from 1
/// within the whole section. `Ini::set` puts a new key at the end of its
/// section, so the order these calls are made in is the order the file reads
/// in — and a file whose pages are grouped is one somebody can edit by hand,
/// which is what this format is for. Reading does not depend on it.
pub fn write_into(ini: &mut Ini, buttons: &Buttons) {
    for i in 1..=MAX {
        for field in [
            "Label",
            "Kind",
            "Value",
            "Shortcut",
            "Confirm",
            "Repeat",
            "IntervalMs",
            "Gate",
            "Prompt",
            "Page",
        ] {
            ini.remove(SECTION, &key(i, field));
        }
    }
    for p in 1..=MAX_PAGES {
        ini.remove(SECTION, &page_key(p));
    }

    // Names first, so the section opens with what its pages are called rather
    // than burying `Page4Name` between two commands.
    for p in 1..=buttons.page_count().min(MAX_PAGES) {
        let name = buttons.name(p);
        if !name.is_empty() {
            ini.set(SECTION, &page_key(p), name);
        }
    }

    // **A stable sort rather than a filter per page**, so that a page nothing
    // else can produce still writes its buttons somewhere. `items` is public;
    // the reader and the C ABI both clamp, but a `Buttons` built in Rust need
    // not have been through either, and a writer that silently drops a command
    // is the worst way to find that out. The order within a page is the order
    // it had, which is the order the buttons are pressed in.
    let mut ordered: Vec<&Button> = buttons.items.iter().collect();
    ordered.sort_by_key(|b| b.page.clamp(1, MAX_PAGES));
    // ...and `take` after the sort, so the cap is on the whole section the way
    // `MAX` reads, rather than on each page.
    for (n, button) in ordered.into_iter().take(MAX).enumerate() {
        let i = n + 1;
        ini.set(SECTION, &key(i, "Label"), &button.label);
        ini.set(SECTION, &key(i, "Kind"), kind_name(button.kind));
        ini.set(SECTION, &key(i, "Value"), &button.value);
        if !button.shortcut.is_empty() {
            ini.set(SECTION, &key(i, "Shortcut"), &button.shortcut);
        }
        // Written only when they say something, so a file with no file-buttons
        // in it is byte for byte the file it was before this kind existed —
        // which is the same rule `Page` follows.
        if let Some(gate) = button.gate {
            ini.set(SECTION, &key(i, "Gate"), gate_name(gate));
        }
        if !button.prompt.is_empty() {
            ini.set(SECTION, &key(i, "Prompt"), &button.prompt);
        }
        if button.confirm {
            ini.set(SECTION, &key(i, "Confirm"), "on");
        }
        // Omitted for page 1, the way every other optional field is omitted at
        // its default — which is what keeps a file that has never had a second
        // page byte-for-byte the file it was before pages existed. Written as
        // the clamped number, matching where the sort above put it.
        let page = button.page.clamp(1, MAX_PAGES);
        if page > 1 {
            ini.set(SECTION, &key(i, "Page"), &page.to_string());
        }
        // The pair travels together: an interval with no repeat behind it is
        // a line that reads as though the button waits a second before doing
        // anything, and a repeat with no interval hides the cadence.
        if button.repeats() {
            ini.set(SECTION, &key(i, "Repeat"), &repeat_name(button.repeat));
            ini.set(
                SECTION,
                &key(i, "IntervalMs"),
                &button.interval_ms.to_string(),
            );
        }
    }
}

/// Read the buttons out of a settings file. A file that is not there is a
/// first run and has no buttons, not an error.
pub fn load(path: &Path) -> Buttons {
    match Ini::load(path) {
        Ok(ini) => from_ini(&ini),
        Err(_) => Buttons::default(),
    }
}

/// Write them back into `path`, preserving everything else in it — comments,
/// ordering, and every setting this program does not know about.
pub fn save(path: &Path, buttons: &Buttons) -> std::io::Result<()> {
    let mut ini = Ini::load(path).unwrap_or_else(|_| Ini::new());
    write_into(&mut ini, buttons);
    ini.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<Button> {
        parse_all(s).items
    }

    fn parse_all(s: &str) -> Buttons {
        from_ini(&Ini::parse(s.as_bytes()))
    }

    /// A page-less set, for the cases that predate pages.
    fn flat(items: Vec<Button>) -> Buttons {
        Buttons {
            items,
            names: Vec::new(),
        }
    }

    fn labels(buttons: &Buttons, page: u32) -> Vec<&str> {
        buttons
            .items
            .iter()
            .filter(|b| b.page == page)
            .map(|b| b.label.as_str())
            .collect()
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
    fn a_button_sends_once_unless_the_file_says_otherwise() {
        let b = parse("[Sterna Buttons]\nButton1Value=uptime$0D\n");
        assert_eq!(b[0].repeat, 1);
        assert!(!b[0].repeats());
        assert_eq!(b[0].interval_ms, DEFAULT_INTERVAL_MS);
    }

    #[test]
    fn repeat_reads_a_count_a_word_and_nothing_else() {
        let cases = [
            ("10", 10),
            (" 3 ", 3),
            ("forever", REPEAT_FOREVER),
            ("FOREVER", REPEAT_FOREVER),
            // The count somebody tries before finding the word.
            ("0", REPEAT_FOREVER),
            // Past the ceiling is the ceiling, and anything unreadable is one
            // send rather than a run nobody asked for.
            ("100000", MAX_REPEAT),
            ("-1", 1),
            ("lots", 1),
            ("", 1),
        ];
        for (value, want) in cases {
            let b = parse(&format!(
                "[Sterna Buttons]\nButton1Value=x\nButton1Repeat={value}\n"
            ));
            assert_eq!(b[0].repeat, want, "Repeat={value}");
        }
    }

    #[test]
    fn the_interval_has_a_floor_so_a_typo_cannot_flood() {
        for (value, want) in [
            ("2500", 2500),
            ("0", MIN_INTERVAL_MS),
            ("1", MIN_INTERVAL_MS),
            ("999999999", MAX_INTERVAL_MS),
            ("soon", DEFAULT_INTERVAL_MS),
        ] {
            let b = parse(&format!(
                "[Sterna Buttons]\nButton1Value=x\nButton1Repeat=5\nButton1IntervalMs={value}\n"
            ));
            assert_eq!(b[0].interval_ms, want, "IntervalMs={value}");
        }
    }

    #[test]
    fn normalize_agrees_with_the_reader() {
        // The C ABI hands buttons in without going through the file, so the
        // two ways in have to land on the same numbers.
        let mut b = Button {
            repeat: 0,
            interval_ms: 0,
            ..Button::default()
        };
        b.normalize();
        assert_eq!(b.repeat, 1);
        assert_eq!(b.interval_ms, MIN_INTERVAL_MS);

        let mut forever = Button {
            repeat: REPEAT_FOREVER,
            interval_ms: MAX_INTERVAL_MS * 2,
            ..Button::default()
        };
        forever.normalize();
        assert!(forever.repeats_forever());
        assert_eq!(forever.interval_ms, MAX_INTERVAL_MS);
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
                ..Button::default()
            },
            Button {
                label: "Reload".into(),
                kind: UserKeyType::Text,
                value: encode(UserKeyType::Text, "reload\r"),
                confirm: true,
                ..Button::default()
            },
            Button {
                label: "Break".into(),
                kind: UserKeyType::Command,
                value: "50430".into(),
                ..Button::default()
            },
            Button {
                label: "Poll".into(),
                kind: UserKeyType::Text,
                value: encode(UserKeyType::Text, "show clock\r"),
                repeat: REPEAT_FOREVER,
                interval_ms: 5000,
                ..Button::default()
            },
        ];
        let buttons = flat(buttons);
        let mut ini = Ini::new();
        write_into(&mut ini, &buttons);
        assert_eq!(from_ini(&ini), buttons);
        // ...and the file says what a person would write by hand.
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(text.contains("Button1Value=show version$0D"), "{text}");
        assert!(text.contains("Button3Kind=command"), "{text}");
        assert!(text.contains("Button4Repeat=forever"), "{text}");
        assert!(text.contains("Button4IntervalMs=5000"), "{text}");
        // A button that sends once says nothing about repeating at all.
        assert!(!text.contains("Button1Repeat"), "{text}");
        assert!(!text.contains("Button1IntervalMs"), "{text}");
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
            &flat(vec![Button {
                label: "two".into(),
                kind: UserKeyType::Text,
                value: "b".into(),
                ..Button::default()
            }]),
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
        write_into(&mut ini, &flat(vec![b]));
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("Button1")).count(), 3);
        assert_eq!(from_ini(&ini).items[0].text(), "conf t\rinterface eth0\r");
    }

    // --- pages ------------------------------------------------------------

    #[test]
    fn a_button_with_no_page_key_is_on_page_one() {
        let b = parse_all("[Sterna Buttons]\nButton1Value=a\nButton2Value=b\nButton2Page=3\n");
        assert_eq!(b.items[0].page, 1);
        assert_eq!(b.items[1].page, 3);
        assert_eq!(b.page_count(), 3);
        // Unnamed, so a frontend calls them Page 1..3 and the file says
        // nothing about them.
        assert!(b.names.is_empty());
    }

    #[test]
    fn a_page_above_the_ceiling_is_clamped() {
        // Clamped and not defaulted: a typo puts the button on the last page,
        // not back among a different device's commands.
        let b = parse("[Sterna Buttons]\nButton1Value=a\nButton1Page=999\n");
        assert_eq!(b[0].page, MAX_PAGES);
        let b = parse("[Sterna Buttons]\nButton1Value=a\nButton1Page=0\n");
        assert_eq!(b[0].page, 1);
        let b = parse("[Sterna Buttons]\nButton1Value=a\nButton1Page=two\n");
        assert_eq!(b[0].page, 1);
        // ...and the C ABI's clamp point agrees with the reader's.
        let mut button = Button {
            page: 999,
            ..Button::default()
        };
        button.normalize();
        assert_eq!(button.page, MAX_PAGES);
    }

    #[test]
    fn a_page_name_round_trips_and_an_empty_one_is_not_written() {
        let mut b = flat(vec![Button::with_text(UserKeyType::Text, "a")]);
        b.items[0].page = 2;
        b.set_name(2, "BMCs");
        let mut ini = Ini::new();
        write_into(&mut ini, &b);
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(text.contains("Page2Name=BMCs"), "{text}");
        // Page 1 has no name, so it has no key — and the names list is only as
        // long as the last named page.
        assert!(!text.contains("Page1Name"), "{text}");
        let back = from_ini(&ini);
        assert_eq!(back.name(2), "BMCs");
        assert_eq!(back.name(1), "");
        assert_eq!(back, b);
    }

    #[test]
    fn a_named_page_with_nothing_on_it_still_exists() {
        // What makes Add page survive until somebody puts a command on it.
        let mut b = flat(vec![Button::with_text(UserKeyType::Text, "a")]);
        b.set_name(3, "Switches");
        assert_eq!(b.page_count(), 3);
        let mut ini = Ini::new();
        write_into(&mut ini, &b);
        assert_eq!(from_ini(&ini).page_count(), 3);
    }

    #[test]
    fn pages_are_grouped_and_renumbered_on_the_way_out() {
        // Interleaved on the way in, grouped on the way out — and the order
        // within a page is kept, because that is the order they are pressed in.
        let ini = Ini::parse(
            b"[Sterna Buttons]\n\
              Button1Label=one\nButton1Value=a\nButton1Page=2\n\
              Button2Label=two\nButton2Value=b\n\
              Button3Label=three\nButton3Value=c\nButton3Page=2\n",
        );
        let b = from_ini(&ini);
        let mut out = Ini::new();
        write_into(&mut out, &b);
        let text = String::from_utf8(out.to_bytes()).unwrap();
        assert!(text.contains("Button1Label=two"), "{text}");
        assert!(text.contains("Button2Label=one"), "{text}");
        assert!(text.contains("Button3Label=three"), "{text}");
        assert!(!text.contains("Button1Page"), "{text}");
        assert!(text.contains("Button2Page=2"), "{text}");
        assert!(text.contains("Button3Page=2"), "{text}");
        // ...and nothing about which page a button is on has changed.
        let back = from_ini(&out);
        assert_eq!(labels(&back, 1), ["two"]);
        assert_eq!(labels(&back, 2), ["one", "three"]);
    }

    #[test]
    fn a_one_page_section_is_written_exactly_as_before() {
        // The compatibility promise: a settings file belonging to somebody who
        // has never made a second page must not gain a byte.
        let source = "[Tera Term]\r\nBaudRate=115200\r\n[Sterna Buttons]\r\n\
                      Button1Label=Show version\r\nButton1Kind=text\r\n\
                      Button1Value=show version$0D\r\n\
                      Button2Label=Reload\r\nButton2Kind=text\r\n\
                      Button2Value=reload$0D\r\nButton2Confirm=on\r\n";
        let mut ini = Ini::parse(source.as_bytes());
        let buttons = from_ini(&ini);
        write_into(&mut ini, &buttons);
        assert_eq!(String::from_utf8(ini.to_bytes()).unwrap(), source);
    }

    #[test]
    fn removing_a_page_keeps_its_buttons() {
        // Removing a page is arranging, not deleting: the commands land on the
        // page beside it, and only Remove — which asks — takes a command away.
        let mut b = parse_all(
            "[Sterna Buttons]\nPage2Name=BMCs\nPage3Name=Switches\n\
             Button1Label=one\nButton1Value=a\n\
             Button2Label=two\nButton2Value=b\nButton2Page=2\n\
             Button3Label=three\nButton3Value=c\nButton3Page=3\n",
        );
        b.remove_page(2);
        assert_eq!(b.items.len(), 3);
        assert_eq!(labels(&b, 1), ["one", "two"]);
        assert_eq!(labels(&b, 2), ["three"]);
        assert_eq!(b.name(2), "Switches");
        assert_eq!(b.page_count(), 2);

        // The first page has no page before it, so its commands join what was
        // the second — which is now the first.
        let mut b = parse_all(
            "[Sterna Buttons]\nButton1Label=one\nButton1Value=a\n\
             Button2Label=two\nButton2Value=b\nButton2Page=2\n",
        );
        b.remove_page(1);
        assert_eq!(labels(&b, 1), ["one", "two"]);
        assert_eq!(b.page_count(), 1);
    }

    #[test]
    fn the_writer_drops_no_button_whatever_page_it_names() {
        // `items` is public and neither the reader nor the C ABI is on this
        // path, so a page past the ceiling can exist here — and a writer that
        // silently dropped the command would be the worst way to discover it.
        let mut b = flat(vec![
            Button {
                label: "sane".into(),
                ..Button::with_text(UserKeyType::Text, "a")
            },
            Button {
                label: "wild".into(),
                page: 4000,
                ..Button::with_text(UserKeyType::Text, "b")
            },
        ]);
        b.items[0].page = 1;
        let mut ini = Ini::new();
        write_into(&mut ini, &b);
        let text = String::from_utf8(ini.to_bytes()).unwrap();
        assert!(text.contains("Button1Label=sane"), "{text}");
        assert!(text.contains("Button2Label=wild"), "{text}");
        // Clamped to the last page rather than dropped, and written as the
        // number it was clamped to.
        assert!(text.contains(&format!("Button2Page={MAX_PAGES}")), "{text}");
        assert_eq!(from_ini(&ini).items.len(), 2);
    }

    #[test]
    fn moving_a_page_takes_its_buttons_with_it() {
        let mut b = parse_all(
            "[Sterna Buttons]\nPage2Name=BMCs\n\
             Button1Label=one\nButton1Value=a\n\
             Button2Label=two\nButton2Value=b\nButton2Page=2\n\
             Button3Label=three\nButton3Value=c\nButton3Page=3\n",
        );
        b.move_page(1, 3);
        assert_eq!(labels(&b, 1), ["two"]);
        assert_eq!(labels(&b, 2), ["three"]);
        assert_eq!(labels(&b, 3), ["one"]);
        assert_eq!(b.name(1), "BMCs");
        // The flat order is untouched: a page move is not a renumbering, which
        // is the whole reason a page is a field rather than a list of lists.
        assert_eq!(
            b.items.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
    }
}
