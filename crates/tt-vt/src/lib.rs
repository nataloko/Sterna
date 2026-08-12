//! tt-vt — the escape-sequence state machine.
//!
//! Byte-level parsing is delegated to the `vte` crate (the "adopt, don't build"
//! call in PLAN.md); everything here is *semantics*, and the semantics are the
//! part being ported from Tera Term rather than from the DEC manuals. Where the
//! two disagree, Tera Term wins — `run_diff.sh` diffs this engine against
//! `oracle/`, which is Tera Term's real `vtterm.c` running headless.
//!
//! Comments citing `vtterm.c` line numbers refer to the pinned upstream SHA in
//! `.github/workflows/ci.yml`.

use tt_charset::{gset_from_intermediate, sbcs_final, Iso2022, Iso2022State, Shift};
use tt_grid::{
    Grid, Pen, Rect, ATTR2_BACK, ATTR2_COLOR_MASK, ATTR2_FORE, ATTR2_PROTECT, ATTR_BLINK,
    ATTR_BOLD, ATTR_MASK, ATTR_REVERSE, ATTR_SGR_MASK, ATTR_SPECIAL, ATTR_UNDER, DEFAULT_BG,
    DEFAULT_FG,
};
use vte::{Params, Perform};

pub mod color;
pub mod keys;
pub mod mouse;
pub mod palette;
pub mod printer;
mod sixel;
pub mod term_id;
pub mod window;
pub use color::Colors;
pub use keys::{CrSend, Key, KeyModes};
pub use mouse::{Encoding, Modifiers, MouseEvent, Tracking};
/// Re-exported because [`Config`] has a field of this type, so a caller that
/// builds one needs to be able to name it without depending on `tt-charset`.
pub use printer::PrinterEvent;
pub use sixel::SixelImage;
pub use term_id::TermId;
pub use tt_charset::ShiftFlags;
pub use window::{WindowMetrics, WindowRequest};

/// What an incoming CR and LF mean. Tera Term's `ts.CRReceive`.
///
/// The default is [`CrReceive::Cr`] — the `else` branch at `ttset.c:643`, not
/// the CRLF the surrounding code suggests. It shifts every row of output, so it
/// is the first thing to suspect when a dump looks uniformly wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrReceive {
    #[default]
    Cr,
    Lf,
    CrLf,
    Auto,
}

/// What a BEL does. Tera Term's `ts.Beep`.
///
/// The default is [`Beep::On`], and it is another `else` branch: `ttset.c:1112`
/// reads the key with an empty default and tests only `off` and `visual`, so
/// both an absent key and a misspelt value ring the bell.
///
/// The terminal decides *whether* there is a bell and never makes a sound
/// itself. [`Vt::take_bells`] is the whole of the engine's part; the governor
/// that thins a runaway out needs a clock, and the noise and the flash both
/// need a frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Beep {
    Off,
    #[default]
    On,
    /// The screen inverts for `BeepVBellWait` milliseconds. Upstream does it by
    /// toggling the DECSCNM flag either side of a `Sleep` on the parser's own
    /// thread (`vtterm.c:5784`), which is why a visual bell there stops the
    /// terminal for as long as it lasts.
    Visual,
}

/// What the host asked of the bell in one chunk — [`Vt::take_bells`].
///
/// Two facts and not an ordered list, which is a deliberate simplification and
/// the one place the bell diverges. Upstream's governor is stepped and reset in
/// stream order; here a chunk holding a RIS *and* bells collapses to "reset,
/// then this many bells", so bells that arrived **before** a RIS are counted
/// against the state it cleared. The cost is which of the next few bells inside
/// the same over-used window are heard, after a host reset the terminal in the
/// middle of a burst it was itself producing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BellRequests {
    /// A RIS went past, so the governor's clocks want putting back
    /// (`vtterm.c:348`, inside `ResetTerminal`).
    pub reset: bool,
    /// How many bells were asked for.
    pub count: u32,
}

/// What OSC 52 may do to the frontend's clipboard —
/// `ts.CtrlFlag & CSF_CBMASK` (`tttypes.h:223`).
///
/// Off is deliberately the default. A program on the far end otherwise gains
/// access to text which may never have passed through this terminal at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardAccess {
    #[default]
    Off,
    Read,
    Write,
    ReadWrite,
}

impl ClipboardAccess {
    fn can_read(self) -> bool {
        matches!(self, ClipboardAccess::Read | ClipboardAccess::ReadWrite)
    }

    fn can_write(self) -> bool {
        matches!(self, ClipboardAccess::Write | ClipboardAccess::ReadWrite)
    }
}

/// An OSC 52 action which needs the frontend. The terminal parses and
/// authorises it; only the frontend can touch the operating system clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardRequest {
    /// Read the named selection and return it with [`Vt::clipboard_reply`].
    Read { selection: String, notify: bool },
    /// Replace the clipboard with decoded UTF-8 text.
    Write { text: String, notify: bool },
    /// A read was refused. Produced only when notification is enabled.
    ReadRejected,
    /// A write was refused. Produced only when notification is enabled.
    WriteRejected,
}

/// Tera Term's `ts.ColorFlag`, or the two bits of it that change how SGR parses.
///
/// `Xterm256Color` defaults to **on** (`ttset.c:743`) and `Aixterm16Color` to
/// off (`:739`). The asymmetry is load-bearing in an unobvious way: when a bit
/// is clear, the corresponding SGR parameter is ignored *without consuming its
/// arguments*, so with 256-colour disabled `ESC [ 38;5;196 m` would be read as
/// "38 ignored, 5 = blink on, 196 ignored". `vtterm.c:2239`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorFlags {
    pub xterm256: bool,
    pub aixterm16: bool,
    /// `CF_PCBOLD16` (`ttset.c` key `PcBoldColor`, default off). Bold plus a
    /// colour below 8 means the bright version of it.
    pub pc_bold16: bool,
    /// `CF_ANSICOLOR` (key `EnableANSIColor`, default on). Without it DECRQSS
    /// reports no colour at all, whatever the pen holds.
    pub ansi_color: bool,
}

impl ColorFlags {
    /// `CF_FULLCOLOR` — any of PC-bold-16, aixterm-16 or xterm-256. It gates
    /// the bright/dim flip in the nearest-colour search, so 256-colour being on
    /// by default means the flip is on by default too.
    pub fn full_color(self) -> bool {
        self.xterm256 || self.aixterm16 || self.pc_bold16
    }
}

impl Default for ColorFlags {
    fn default() -> Self {
        ColorFlags {
            xterm256: true,
            aixterm16: false,
            pc_bold16: false,
            ansi_color: true,
        }
    }
}

/// What `CSI 20 t` and `CSI 21 t` answer — `ts.WindowFlag & WF_TITLEREPORT`,
/// `vtterm.c:2668`.
///
/// The shipped value is [`Empty`](Self::Empty), and that is a deliberate
/// mitigation rather than an oversight: a terminal that echoes its own title
/// back into the input stream lets anything which can write to the screen put
/// text in front of the shell, and the title is the one thing the host chose
/// itself. Upstream spells it in a way that hides the choice — `IdTitleReportEmpty`
/// is **24**, which is `WF_TITLEREPORT` entire, so the name reads like "no
/// bits" and sets both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleReport {
    /// `ignore`. Nothing is sent at all.
    Ignore,
    /// `accept`. The real title, interleaved with [`Config::title`] the way
    /// [`TitleChange`] says.
    Accept,
    /// `empty`, and the default. The reply is sent, with nothing in it.
    #[default]
    Empty,
}

/// How the title the host set and the one in the file combine —
/// `ts.AcceptTitleChangeRequest`, `ttwinman.c:109` for the window and
/// `vtterm.c:2677` for the report.
///
/// It is also a switch: [`Off`](Self::Off) means the host's title is never
/// stored at all (`vtterm.c:5112`), which takes the title stack down with it
/// since `CSI 22 t` and `CSI 23 t` are gated on the same field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleChange {
    /// `off`. The host's title is discarded on arrival.
    Off,
    /// `overwrite`, and the default. The host's title replaces the file's,
    /// falling back to the file's while the host has set none.
    #[default]
    Overwrite,
    /// `ahead`. The host's title, a space, then the file's.
    Ahead,
    /// `last`. The file's title, a space, then the host's.
    Last,
}

impl TitleChange {
    /// The window title, out of the file's and whatever the host has set.
    ///
    /// `ttwinman.c:101`: an empty remote title is no remote title, and takes
    /// the `Off` arm's answer whatever the mode is. That is not the same test
    /// the report path makes, which is why this is not shared with it.
    pub fn combine(self, file: &str, remote: &str) -> String {
        if remote.is_empty() {
            return file.to_string();
        }
        match self {
            TitleChange::Off => file.to_string(),
            TitleChange::Overwrite => remote.to_string(),
            TitleChange::Ahead => format!("{remote} {file}"),
            TitleChange::Last => format!("{file} {remote}"),
        }
    }
}

/// `ts.TabStopFlag` — which sequences a *host* may move the tab stops with.
///
/// `tttypes.h:196`. Four bits and two pairs: `HTS7` is `ESC H`, `HTS8` is the
/// 8-bit C1 at `0x88`, `TBC0` is `CSI 0 g` and `TBC3` is `CSI 3 g`. Upstream
/// gates each at its own site (`vtterm.c:1512`, `:1160`, `buffer.c:5266`,
/// `:5280`) rather than in one place, so the four are genuinely independent —
/// a terminal can accept `ESC H` and refuse the C1 spelling of the same thing.
///
/// It exists because a host that clears all the stops and sets its own leaves
/// the terminal unusable for whatever runs next, and the user has no way back
/// short of a reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStopFlags(pub u16);

impl TabStopFlags {
    pub const HTS7: u16 = 1;
    pub const HTS8: u16 = 2;
    pub const TBC0: u16 = 4;
    pub const TBC3: u16 = 8;
    pub const HTS: u16 = Self::HTS7 | Self::HTS8;
    pub const TBC: u16 = Self::TBC0 | Self::TBC3;

    pub const NONE: TabStopFlags = TabStopFlags(0);
    pub const ALL: TabStopFlags = TabStopFlags(Self::HTS | Self::TBC);

    /// The names the file spells, longest-matching irrelevant because the
    /// comparison is whole-word — `ttset.c:1724`.
    const NAMES: [(&'static str, u16); 6] = [
        ("HTS", Self::HTS),
        ("HTS7", Self::HTS7),
        ("HTS8", Self::HTS8),
        ("TBC", Self::TBC),
        ("TBC0", Self::TBC0),
        ("TBC3", Self::TBC3),
    ];

    pub fn allows(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    /// `TabStopModifySequence`'s value — `ttset.c:1717`.
    ///
    /// `on`/`all` and `off`/`none` are tested against the **whole** value and
    /// assign the whole word; anything else is a comma list starting from
    /// nothing, so a spelling with no recognised word in it — including an
    /// empty value — is a terminal that refuses every one of the four.
    ///
    /// Unlike [`ShiftFlags::parse_ini`] there is no `-` prefix and no
    /// subtraction: the list only ever adds. The trap that key carries, where
    /// a present value starts from zero rather than from the default, is the
    /// same here and one arm less surprising, because `on` is a value the list
    /// arm never sees.
    pub fn parse_ini(value: &str) -> TabStopFlags {
        let whole = value.trim();
        if whole.eq_ignore_ascii_case("on") || whole.eq_ignore_ascii_case("all") {
            return TabStopFlags::ALL;
        }
        if whole.eq_ignore_ascii_case("off") || whole.eq_ignore_ascii_case("none") {
            return TabStopFlags::NONE;
        }
        let mut out = TabStopFlags::NONE;
        for item in value.split(',') {
            let name = item.trim();
            if let Some(&(_, bit)) = Self::NAMES
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
            {
                out.0 |= bit;
            }
        }
        out
    }

    /// The spelling upstream's writer produces — `ttset.c:3072`. `on` and `off`
    /// for the two whole-word states, and otherwise at most one HTS word and
    /// one TBC word, in that order.
    pub fn to_ini(self) -> String {
        if self == TabStopFlags::ALL {
            return "on".into();
        }
        if self == TabStopFlags::NONE {
            return "off".into();
        }
        let mut parts: Vec<&str> = Vec::new();
        match self.0 & Self::HTS {
            Self::HTS7 => parts.push("HTS7"),
            Self::HTS8 => parts.push("HTS8"),
            Self::HTS => parts.push("HTS"),
            _ => {}
        }
        match self.0 & Self::TBC {
            Self::TBC0 => parts.push("TBC0"),
            Self::TBC3 => parts.push("TBC3"),
            Self::TBC => parts.push("TBC"),
            _ => {}
        }
        // Upstream's own "shouldn't happen but just in case" arm, which is
        // reachable: TabStopFlag has bits for nothing else, so an empty list
        // here means NONE and the arm above already answered.
        if parts.is_empty() {
            return "off".into();
        }
        parts.join(",")
    }
}

/// The eight hex digits the tertiary DA answers with — `ts.TerminalUID`.
///
/// Validated the same way in the two places it is assigned: `ttset.c:1691`
/// reading the file and `vtterm.c:4567` taking DECSTUI off the wire. Both want
/// exactly eight characters, every one a hex digit, upper-cased in place;
/// anything else leaves the field as it was, which for the file means the
/// default and for DECSTUI means whatever the file gave.
pub fn valid_terminal_uid(value: &str) -> Option<String> {
    let uid: String = value.to_ascii_uppercase();
    (uid.len() == 8 && uid.bytes().all(|b| b.is_ascii_hexdigit())).then_some(uid)
}

/// Upstream's fallback for a `TerminalUID` that is not eight hex digits, which
/// is also the key's default (`ttset.c:1702`).
pub const DEFAULT_TERMINAL_UID: &str = "FFFFFFFF";

/// What the receive-debug decoder does with each raw byte.
///
/// These values deliberately match `DEBUG_FLAG_*` (`charset.h:83`) so TTL's
/// `setdebug 0..3` maps without a second interpretation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugMode {
    #[default]
    Off = 0,
    Normal = 1,
    Hex = 2,
    NoOutput = 3,
}

impl DebugMode {
    fn next(self) -> DebugMode {
        match self {
            DebugMode::Off => DebugMode::Normal,
            DebugMode::Normal => DebugMode::Hex,
            DebugMode::Hex => DebugMode::NoOutput,
            DebugMode::NoOutput => DebugMode::Off,
        }
    }
}

/// Which non-off modes Shift+Escape may cycle through (`ts.DebugModes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugModes(u8);

impl DebugModes {
    pub const NORMAL: u8 = 1;
    pub const HEX: u8 = 2;
    pub const NO_OUTPUT: u8 = 4;
    pub const ALL: DebugModes = DebugModes(Self::NORMAL | Self::HEX | Self::NO_OUTPUT);
    pub const NONE: DebugModes = DebugModes(0);

    pub fn from_bits(bits: u8) -> DebugModes {
        DebugModes(bits & Self::ALL.0)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn allows(self, mode: DebugMode) -> bool {
        match mode {
            DebugMode::Off => true,
            DebugMode::Normal => self.0 & Self::NORMAL != 0,
            DebugMode::Hex => self.0 & Self::HEX != 0,
            DebugMode::NoOutput => self.0 & Self::NO_OUTPUT != 0,
        }
    }
}

impl Default for DebugModes {
    fn default() -> Self {
        DebugModes::ALL
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub cols: usize,
    pub rows: usize,
    pub term_id: TermId,
    pub cr_receive: CrReceive,
    /// `Debug=`: the keyboard gate on Shift+Escape. TTL's `setdebug` bypasses
    /// it, as upstream's DDE command does.
    pub debug_enabled: bool,
    /// `DebugModes=`: which three non-off modes that key cycles through.
    pub debug_modes: DebugModes,
    pub color_flags: ColorFlags,
    /// The terminal's 256 drawing colours. Entries 0-15 come from the
    /// `ANSIColor` setting after `vtdisp.c:GetIndex256From16` swaps its legacy
    /// bright/dim order; 16-255 are the fixed xterm cube and greyscale ramp.
    ///
    /// This belongs in the terminal rather than only in the painter because
    /// truecolor SGR is resolved to the nearest *index* as it is parsed. A
    /// custom palette therefore changes what the grid stores as well as what
    /// that index looks like later.
    pub palette: [palette::Rgb; 256],
    /// The six attribute colour pairs and Tek's, foreground then background —
    /// `ts.VTColor`, `ts.VTBoldColor`, `ts.VTBlinkColor`, `ts.VTReverseColor`,
    /// `ts.URLColor`, `ts.VTUnderlineColor` and `ts.TEKColor`.
    ///
    /// These are the *configured* values, which is a distinct thing from what
    /// the terminal is currently painting with: `OSC 10`-`19` change the live
    /// copy in [`color::Colors`] and these are what a reset returns to. The
    /// engine holds them because upstream's OSC handler does, and because
    /// `DispGetColor` answers a query out of exactly these rather than out of
    /// the live pair.
    pub color_normal: [palette::Rgb; 2],
    pub color_bold: [palette::Rgb; 2],
    pub color_blink: [palette::Rgb; 2],
    pub color_reverse: [palette::Rgb; 2],
    pub color_url: [palette::Rgb; 2],
    pub color_underline: [palette::Rgb; 2],
    pub color_tek: [palette::Rgb; 2],
    /// `ts.ISO2022Flag`. Defaults to every shift enabled.
    pub iso2022_flags: ShiftFlags,
    /// `LangIsJapanese(ts.KanjiCode)`. False for a UTF-8 terminal, which is all
    /// we support; it gates the Katakana designations only.
    pub japanese: bool,
    /// `TF_ACCEPT8BITCTRL` (`ttset.c:1075`, key default on).
    pub accept_8bit_ctrl: bool,
    /// `TF_ALTSCR` (`ttset.c:1681`, key default on).
    pub alt_screen_enabled: bool,
    /// `TF_REMOTECLEARSBUFF` (`ttset.c:1950`, key default on). Gates `ED 3`.
    pub remote_clears_buffer: bool,
    /// `TF_CLEARONRESIZE` (`ttset.c:1676`, key default **off**). With it on a
    /// resize scrolls the page away and homes the cursor; with it off the
    /// screen survives and only the page's position over the buffer moves.
    pub clear_on_resize: bool,
    /// `ts.ScrollWindowClearScreen` (`ttset.c:1444`, key default on). Whether
    /// an `ED 0` with the cursor at the home position is treated as `ED 2`.
    /// **Not** a gate on `ED 2`, which clears the screen either way.
    pub home_erase_clears_screen: bool,
    /// `TF_PRINTERCTRL` (`ttset.c:1245` `PrinterCtrlSequence`, key default
    /// **off**). Gates four of the five media-copy sequences — `CSI 0 i`,
    /// `CSI 5 i`, `CSI ? 1 i` and `CSI ? 5 i`. The fifth, `CSI ? 4 i`, is
    /// deliberately ungated so a host can always turn auto print off again.
    pub printer_ctrl_sequence: bool,
    /// `DirectPrn` — `ts.PrnDev[0] != 0` (`vtterm.c:2095`), i.e. whether
    /// `PassThruPort` names a device. It is not a gate on printing: it decides
    /// whether the locking shifts and ISO-2022 designations arriving during
    /// controller mode are the *terminal's* to interpret or bytes the printer
    /// should receive. The device name itself never reaches the engine.
    pub printer_direct: bool,
    /// `WF_WINDOWCHANGE` (`ttset.c:1653`, key default on). Gates the XTWINOPS
    /// operations that *change* something, including the resize.
    pub window_change: bool,
    /// `WF_WINDOWREPORT` (`ttset.c:1661`, key default on). Gates the ones that
    /// answer back.
    pub window_report: bool,
    /// `WF_TITLEREPORT` (`ttset.c:1664`). See [`TitleReport`].
    pub title_report: TitleReport,
    /// `ts.AcceptTitleChangeRequest` (`ttset.c:1568`). See [`TitleChange`].
    pub accept_title_change: TitleChange,
    /// `ts.Title` — the title out of `TERATERM.INI`, before the host has said
    /// anything. The terminal needs it for one thing only: `CSI 20 t` and
    /// `CSI 21 t` under [`TitleReport::Accept`] interleave it with the host's
    /// (`vtterm.c:2677`). Everything else about the title bar belongs to the
    /// frontend, which is where upstream does it too (`ttwinman.c:95`).
    pub title: String,
    /// DECRQCRA, the rectangular-area checksum — **and the one thing here that
    /// is not upstream's**. Tera Term has no `CSI * y` at all; `vtterm.c` never
    /// mentions a checksum, so the faithful answer to the request is silence,
    /// and that is what the default gives.
    ///
    /// It exists because it is the only way to read a cell back over the wire,
    /// and `esctest/` — iTerm2's conformance suite, which runs *inside* the
    /// terminal — asserts on screen contents through nothing else. So the
    /// conformance harness turns it on and everything else leaves it off,
    /// which keeps a real connection byte-for-byte Tera Term while still
    /// letting a thousand assertions look at the grid.
    pub decrqcra: bool,
    /// `ts.Send8BitCtrl` (`ttset.c:1283`, key default off). Above VT level 1 it
    /// decides whether replies use 8-bit C1 introducers; DECSCL can turn it on
    /// as well.
    pub send_8bit_ctrl: bool,
    /// `ts.CursorShape` in DECSCUSR's numbering — 1 block, 3 underline, 5 bar.
    /// `ttset.c:725`'s else branch is `IdBlkCur`. Only DECRQSS reads it; the
    /// shape itself is the frontend's business.
    pub cursor_shape: u16,
    /// `ts.NonblinkingCursor` (`ttset.c:1227`, key default off). Adds one to
    /// the DECSCUSR value.
    pub nonblinking_cursor: bool,
    pub scrollback_max: usize,
    /// `ts.MouseEventTracking` (`ttset.c:1523`, key default **on**). With it
    /// off, every `DECSET 9/1000…1016` is a no-op and DECRQM reports those
    /// modes as permanently reset.
    pub mouse_tracking_enabled: bool,
    /// `ts.DisableMouseTrackingByCtrl` (`ttset.c:1591`, key default on). Holding
    /// Ctrl suppresses the report so the user can still select text.
    pub disable_mouse_tracking_by_ctrl: bool,
    /// The character cell in pixels. The core needs it for exactly two things:
    /// converting a mouse position to a cell, and SGR-pixel reports, which do
    /// not convert at all. It never learns anything else about pixels.
    pub cell_w: i32,
    pub cell_h: i32,
    /// `ts.TranslateWheelToCursor` (`ttset.c:1515`, key default on). Gates
    /// `DECSET 7786`, and is what a reset restores that mode to.
    pub translate_wheel_to_cursor: bool,
    /// `ts.DisableWheelToCursorByCtrl` (`ttset.c:1594`, key default on).
    /// Holding Ctrl cancels the translation, so the wheel reaches the
    /// terminal's own history while a full-screen program is up. Read at the
    /// wheel rather than at DECSET time — see [`Vt::wheel_to_cursor_now`].
    pub disable_wheel_to_cursor_by_ctrl: bool,
    /// `WF_CURSORCHANGE` (`ttset.c:1656` `CursorCtrlSequence`, key default
    /// **off**). Gates DECSCUSR and `DECSET 12`, and shifts what DECRQM
    /// answers for the three cursor modes by two.
    pub cursor_ctrl_sequence: bool,
    /// `ts.LocalEcho` (`ttset.c:660`, key default off). SRM writes it.
    pub local_echo: bool,
    /// `ts.CRSend` (`ttset.c:657`, default `IdCR`). LNM overwrites it at
    /// runtime, so this is only the starting value.
    pub cr_send: CrSend,
    /// `ts.BSKey == IdBS` (`ttset.c:882`, the else branch). DECBKM writes it.
    pub bs_key_is_bs: bool,
    /// `ts.DisableAppKeypad` (`ttset.c:903`, key default off). Vetoes DECNKM
    /// for key encoding while leaving the mode itself set, which is what
    /// DECRQM keeps reporting.
    pub disable_app_keypad: bool,
    /// `ts.DisableAppCursor` (`ttset.c:907`, key default off). Same veto for
    /// DECCKM.
    pub disable_app_cursor: bool,
    /// `ts.LogTypePlainText` (`ttset.c:984`, key default off), and it is one
    /// byte rather than a mode: `vtterm.c:666` and `:671` put a BS into the
    /// tap when a backspace moved the cursor, and this suppresses it — so a
    /// line the host corrected is logged as the correction rather than as the
    /// keystrokes that made it.
    ///
    /// **It is not only the log.** The tap upstream gates here feeds the
    /// macro language's received-line buffer as well (`NeedsOutputBufs`), so
    /// this changes what `wait` matches against, and both taps here are gated
    /// with it for that reason.
    pub log_plain_text: bool,
    /// `ts.EnableContinuedLineCopy` (`ttset.c:1419`, key default off), and the
    /// half of it that lives in the terminal rather than in the frontend.
    ///
    /// It is upstream's `logFlag`, threaded through `CarriageReturn` and
    /// `LineFeed` (`vtterm.c:675`, `:688`): TRUE for a CR or an LF that came
    /// off the wire, FALSE for the pair the terminal generates when a line
    /// wraps. With this on, only the generated pair is kept out of the log and
    /// the macro tap — so `wait` matches a wrapped line as the one line the
    /// host meant to send, instead of two.
    ///
    /// The other half is [`ATTR_LINE_CONTINUED`](tt_grid::ATTR_LINE_CONTINUED),
    /// which the grid maintains whatever this says because nothing reads it
    /// when the setting is off.
    pub continued_line_copy: bool,
    /// `ts.Beep` (`ttset.c:1112`). The terminal needs it for one thing: BEL
    /// does not even ask for a bell when this is [`Beep::Off`]
    /// (`vtterm.c:1077`), so the governor above it never advances either.
    pub beep: Beep,
    /// `ts.Answerback` (`ttset.c:663`), already decoded out of its `$xx` form.
    ///
    /// ENQ sends these bytes verbatim — `vtterm.c:1075` uses `CommBinaryOut`,
    /// so no CR translation, no local echo, and no length limit beyond the 32
    /// bytes upstream's buffer holds. Empty by default, which is a terminal
    /// that answers ENQ with nothing at all.
    pub answerback: Vec<u8>,
    /// `TF_BACKWRAP` (`ttset.c:1108`, key default off). Whether a BS on the
    /// left margin steps back to the previous line. Held here as well as in the
    /// grid because the grid is where it acts and this is where it is read.
    pub back_wrap: bool,
    /// `ts.VTCompatTab` (`ttset.c:1343`, key default off). Off — as shipped —
    /// a tab is like a printed character at the end of a line: `Tab`
    /// (`vtterm.c:713`) breaks the line before tabbing and `CursorForwardTab`
    /// arms the pending wrap. On, a tab is only ever a cursor move.
    pub vt_compat_tab: bool,
    /// `ts.TabStopFlag` (`ttset.c:1717`, key default `on`). See
    /// [`TabStopFlags`].
    pub tab_stop_modify: TabStopFlags,
    /// `TF_INVALIDDECRPSS` (`ttset.c:1756`, key default off), and upstream's
    /// comment on it is "(for testing)". Flips the leading digit of every
    /// DECRQSS reply, so a request the terminal understood is answered as one
    /// it did not and the other way round. It exists to exercise a *host's*
    /// error handling.
    pub invalid_decrqss: bool,
    /// `ts.TerminalUID` (`ttset.c:1688`), the eight hex digits the tertiary DA
    /// answers with. Held validated — see [`valid_terminal_uid`].
    pub terminal_uid: String,
    /// `TF_LOCKTUID` (`ttset.c:1711`, key default **on**). With it on DECSTUI
    /// is read and dropped, which is how Tera Term ships: a host cannot change
    /// the identity the terminal reports.
    pub lock_uid: bool,
    /// `TF_AUTOINVOKE` (`ttset.c:1101`, key default off). Whether designating
    /// into G0 also invokes G0 into GL — see [`Vt::esc_dispatch`]'s designation
    /// arm for the two things about it that the name does not say.
    pub auto_invoke: bool,
    /// `ts.MaxOSCBufferSize` (`ttset.c:1789`, default 4096). The ceiling on a
    /// string collected out of an OSC; upstream drops every byte past it and
    /// lets the sequence terminate normally, so a long title arrives cut.
    pub max_osc_buffer: usize,
    /// `ts.CtrlFlag & CSF_CBMASK` (`ttset.c:1742`). The two OSC 52
    /// permissions are independent; see [`ClipboardAccess`].
    pub clipboard_access: ClipboardAccess,
    /// `ts.NotifyClipboardAccess` (`ttset.c:1753`, key default on). Accepted
    /// actions carry this bit to the frontend; rejected ones become events
    /// only when it is set.
    pub notify_clipboard_access: bool,
}

/// The private and ANSI modes that are one flag each. Grouped so a reset can
/// name them together, and so `DECRQM` has one place to read.
#[derive(Clone, Debug)]
struct Modes {
    /// DECCKM. The frontend needs it to pick `ESC O A` over `ESC [ A`.
    appli_cursor: bool,
    /// DECNKM.
    appli_key: bool,
    /// `AppliEscapeMode` — 0, or mintty's 1 from `DECSET 7727`, or 2/3/4 from
    /// the three upstream test modes at 14002-14004.
    appli_escape: u16,
    /// DECARM. On out of reset.
    auto_repeat: bool,
    /// DECTCEM, `IsCaretEnabled()`. On out of reset.
    caret: bool,
    /// DECPEX. Starts **set** — `vtterm.c:176` initialises it TRUE and
    /// nothing resets it, so DECRQM reports 1 on a fresh terminal.
    print_ex: bool,
    /// Sixel scrolling, in xterm's modern sense. It starts on: a graphic is
    /// anchored at the text cursor, follows scrollback, and leaves the cursor
    /// below it. DECSDM (`?80`) has the opposite sense, so setting that mode
    /// clears this flag and resetting it sets the flag.
    sixel_scrolling: bool,
    /// `DECSET 2004`.
    bracketed_paste: bool,
    /// `DECSET 7786`, `AcceptWheelToCursor`.
    wheel_to_cursor: bool,
    /// `DECSET 8200`. Makes `ED 2` home the cursor afterwards.
    clear_then_home: bool,
    /// DECSCNM. Upstream keeps it in `ts.ColorFlag`; nothing in the grid reads
    /// it, since reversing the whole screen is the renderer's job.
    reverse_video: bool,
    /// KAM. On out of reset — the keyboard is not locked.
    keyb_enabled: bool,
    /// SRM, mirroring `ts.LocalEcho`. Note the sense: `SM 12` turns local echo
    /// *off*.
    local_echo: bool,
    /// LNM. It also rewrites `cv.CRSend`, which is why the pair moves
    /// together — `SM 20` means CR LF, `RM 20` means bare CR, and neither
    /// leaves an `IdLF` from the config in place.
    lf_mode: bool,
    cr_send: CrSend,
    /// DECBKM, mirroring `ts.BSKey`.
    bs_key_is_bs: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cols: 80,
            rows: 24,
            term_id: TermId::Vt100,
            cr_receive: CrReceive::Cr,
            debug_enabled: false,
            debug_modes: DebugModes::ALL,
            color_flags: ColorFlags::default(),
            palette: *palette::default_palette(),
            // `ttset.c:754` and the keys around it: black on white, blue,
            // red, white on black, green and magenta, then Tek's black on
            // white. The backgrounds are all white except reverse's.
            color_normal: [(0, 0, 0), (255, 255, 255)],
            color_bold: [(0, 0, 255), (255, 255, 255)],
            color_blink: [(255, 0, 0), (255, 255, 255)],
            color_reverse: [(255, 255, 255), (0, 0, 0)],
            color_url: [(0, 255, 0), (255, 255, 255)],
            color_underline: [(255, 0, 255), (255, 255, 255)],
            color_tek: [(0, 0, 0), (255, 255, 255)],
            iso2022_flags: ShiftFlags::ALL,
            japanese: false,
            accept_8bit_ctrl: true,
            alt_screen_enabled: true,
            remote_clears_buffer: true,
            clear_on_resize: false,
            home_erase_clears_screen: true,
            printer_ctrl_sequence: false,
            printer_direct: false,
            window_change: true,
            window_report: true,
            title_report: TitleReport::default(),
            accept_title_change: TitleChange::default(),
            // Upstream's `Title=` default is its own product name. Empty here:
            // the frontend owns what this program is called, and a core that
            // shipped "Tera Term" would put it in the title bar of anything
            // that forgot to set it.
            title: String::new(),
            decrqcra: false,
            send_8bit_ctrl: false,
            cursor_shape: 1,
            nonblinking_cursor: false,
            // ttset.c:1213 MaxBuffSize. Not ttset.c:750's ScrollBuffSize (100),
            // which is the *initial* depth the user can grow up to this.
            scrollback_max: 10_000,
            mouse_tracking_enabled: true,
            disable_mouse_tracking_by_ctrl: true,
            // Matches the oracle's nominal cell (`oracle.h:ORACLE_CELL_W`), so
            // an injected event lands on the same character in both engines.
            cell_w: 8,
            cell_h: 16,
            translate_wheel_to_cursor: true,
            disable_wheel_to_cursor_by_ctrl: true,
            cursor_ctrl_sequence: false,
            local_echo: false,
            cr_send: CrSend::Cr,
            bs_key_is_bs: true,
            disable_app_keypad: false,
            disable_app_cursor: false,
            log_plain_text: false,
            continued_line_copy: false,
            beep: Beep::On,
            answerback: Vec::new(),
            back_wrap: false,
            vt_compat_tab: false,
            tab_stop_modify: TabStopFlags::ALL,
            invalid_decrqss: false,
            terminal_uid: String::from(DEFAULT_TERMINAL_UID),
            lock_uid: true,
            auto_invoke: false,
            max_osc_buffer: 4096,
            clipboard_access: ClipboardAccess::Off,
            notify_clipboard_access: true,
        }
    }
}

impl Modes {
    /// `vtterm.c:ResetTerminal`. Note what is *not* here: DECPEX, the keyboard
    /// lock, local echo and DECBKM all survive a RIS, because upstream never
    /// clears them. Bracketed paste does not — it is cleared much further
    /// down the same function (`vtterm.c:336`), a hundred lines after the
    /// block that clears the other modes.
    fn reset(&mut self, config: &Config) -> Modes {
        Modes {
            appli_cursor: false,
            appli_key: false,
            appli_escape: 0,
            auto_repeat: true,
            caret: true,
            print_ex: self.print_ex,
            sixel_scrolling: true,
            bracketed_paste: false,
            wheel_to_cursor: config.translate_wheel_to_cursor,
            clear_then_home: false,
            reverse_video: false,
            keyb_enabled: self.keyb_enabled,
            local_echo: self.local_echo,
            lf_mode: self.lf_mode,
            cr_send: self.cr_send,
            bs_key_is_bs: self.bs_key_is_bs,
        }
    }

    /// `vtterm.c:SoftReset`, which is a much shorter list.
    fn soft_reset(&mut self, config: &Config) {
        self.auto_repeat = true;
        self.caret = true;
        self.appli_cursor = false;
        self.appli_key = false;
        self.appli_escape = 0;
        self.wheel_to_cursor = config.translate_wheel_to_cursor;
        self.sixel_scrolling = true;
    }

    fn from_config(config: &Config) -> Modes {
        Modes {
            appli_cursor: false,
            appli_key: false,
            appli_escape: 0,
            auto_repeat: true,
            caret: true,
            print_ex: true,
            sixel_scrolling: true,
            bracketed_paste: false,
            wheel_to_cursor: config.translate_wheel_to_cursor,
            clear_then_home: false,
            reverse_video: false,
            keyb_enabled: true,
            local_echo: config.local_echo,
            lf_mode: config.cr_send == CrSend::CrLf,
            cr_send: config.cr_send,
            bs_key_is_bs: config.bs_key_is_bs,
        }
    }
}

/// The terminal. Owns the parser and the grid.
pub struct Vt {
    parser: vte::Parser,
    state: State,
    debug_mode: DebugMode,
    /// A `0xC2` seen at the end of the previous chunk. Without this, feeding
    /// `[0xC2]` then `[0x8D]` would print a replacement character where a
    /// single call would have produced a carriage return.
    pending_c2: bool,
    /// Continuation bytes still owed by a UTF-8 sequence in progress, across
    /// chunk boundaries for the same reason. A byte in `80..=9F` is a C1
    /// control only when nothing is expecting it.
    utf8_left: u8,
    /// The start of that sequence, held back rather than handed to `vte`.
    ///
    /// **`vte` 0.15.0 loses bytes when it resumes one.**
    /// `advance_partial_utf8` (`lib.rs:687`) fills a 4-byte buffer, decodes it,
    /// prints only the *first* character — "we just ignore the rest", says the
    /// comment — and then returns `valid_up_to()` as the number of bytes it
    /// consumed. Any complete character between that first one and the
    /// incomplete tail is dropped without a trace. Reachable whenever a 2-byte
    /// sequence is cut by a read boundary and the next read holds exactly one
    /// ASCII byte before another multi-byte lead: `[C3 A9]` split as
    /// `[.. C3] [A9 'a' E4 B8 80]` prints `é一` and eats the `a`.
    ///
    /// So the partial sequence never reaches `vte` at all — it waits here and
    /// goes out with the next chunk. That is the right place for it regardless:
    /// this walk already has to know where sequences begin and end, because
    /// telling a continuation byte from a bare C1 control is the whole reason
    /// [`Vt::rewrite_c1`] exists.
    held: Vec<u8>,
}

impl Vt {
    pub fn new(config: Config) -> Self {
        let mut grid = Grid::new(config.cols, config.rows, config.scrollback_max);
        grid.set_clear_on_resize(config.clear_on_resize);
        grid.set_back_wrap(config.back_wrap);
        grid.set_vt_compat_tab(config.vt_compat_tab);
        // `vtterm.c:ChangeTerminalID` — level 1 never sends 8-bit controls,
        // whatever the setting says.
        let vt_level = config.term_id.vt_level();
        let send_8bit = vt_level >= 2 && config.send_8bit_ctrl;
        let colors = color::Colors::new(&config);
        Vt {
            parser: vte::Parser::new(),
            state: State {
                grid,
                modes: Modes::from_config(&config),
                colors,
                config,
                vt_level,
                send_8bit,
                ..State::empty()
            },
            debug_mode: DebugMode::Off,
            pending_c2: false,
            utf8_left: 0,
            held: Vec::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if self.debug_mode != DebugMode::Off {
            for &byte in bytes {
                self.debug_byte(byte);
            }
            self.state.reconcile_sixels();
            return;
        }
        // Pure ASCII needs no rewriting at all, and almost every chunk is. The
        // test cannot be narrower than this: the walk has to see whole UTF-8
        // sequences to know a continuation byte from a bare one, so any byte
        // over 0x7F puts it back in play. The two printer conditions are
        // [`Vt::advance_split`]'s — with `PrinterCtrlSequence` off, which is how
        // Tera Term ships, nothing can take the stream away from `vte` and this
        // stays the one-call path it has always been.
        if !self.pending_c2
            && self.utf8_left == 0
            && !self.state.printer.is_on()
            && !self.state.config.printer_ctrl_sequence
            && !bytes.iter().any(|&b| b >= 0x80)
        {
            self.parser.advance(&mut self.state, bytes);
            self.state.reconcile_sixels();
            return;
        }
        let rewritten = self.rewrite_c1(bytes);
        self.advance_split(&rewritten);
        self.state.reconcile_sixels();
    }

    /// Hand a rewritten chunk to `vte`, letting printer controller mode take the
    /// stream away in the middle of it.
    ///
    /// `CSI 5 i` turns the controller on from inside `vte`'s own dispatch and
    /// `advance` cannot be asked to stop there, so the chunk is cut after every
    /// `i` — the only final byte that can change the answer, and cheap to find.
    /// Nothing is needed in the other direction: while the controller is on the
    /// only bytes `vte` is given are printable ones, so it cannot dispatch
    /// anything at all, and `CSI 4 i` is [`printer::Controller`]'s to notice.
    fn advance_split(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.state.printer.is_on() {
                let used = self.printer_consume(rest);
                rest = &rest[used..];
                continue;
            }
            if !self.state.config.printer_ctrl_sequence {
                self.parser.advance(&mut self.state, rest);
                return;
            }
            match rest.iter().position(|&b| b == b'i') {
                Some(n) => {
                    self.parser.advance(&mut self.state, &rest[..=n]);
                    rest = &rest[n + 1..];
                }
                None => {
                    self.parser.advance(&mut self.state, rest);
                    return;
                }
            }
        }
    }

    /// Run [`printer::Controller`] over as much of the chunk as it wants, and
    /// answer with how much it took.
    ///
    /// The interleaving is the point. Text still reaches the screen and is
    /// copied to the printer from there, so a run of characters has to go
    /// through `vte` *before* the next control byte is written — otherwise
    /// `A LF B` prints as `LF A B`, which is the sort of thing that only shows
    /// up on paper.
    fn printer_consume(&mut self, bytes: &[u8]) -> usize {
        let iso = self.state.config.iso2022_flags;
        let mut for_vte: Vec<u8> = Vec::new();
        let mut used = 0;
        let mut exited = false;
        for (i, &b) in bytes.iter().enumerate() {
            used = i + 1;
            let mut out = String::new();
            match self.state.printer.step(b, iso, &mut out) {
                printer::Step::Terminal => for_vte.push(b),
                printer::Step::Replay(seq) => for_vte.extend_from_slice(&seq),
                printer::Step::Drop => {}
                printer::Step::Printer(cp) => printer::push_cp(&mut out, cp),
                printer::Step::Exit => exited = true,
            }
            if !out.is_empty() {
                if !for_vte.is_empty() {
                    self.parser.advance(&mut self.state, &for_vte);
                    for_vte.clear();
                }
                self.state.printer_write(&out);
            }
            if exited {
                break;
            }
        }
        if !for_vte.is_empty() {
            self.parser.advance(&mut self.state, &for_vte);
        }
        // `PrnParseCS` closes the job on the way out unless auto print still
        // owns it (`vtterm.c:4034`).
        if exited && !self.state.auto_print {
            self.state.printer_close();
        }
        used
    }

    /// `charset.cpp:PutDebugChar`: display one raw byte without letting the
    /// escape parser consume it.
    fn debug_byte(&mut self, mut byte: u8) {
        let insert = self.state.grid.insert_mode;
        let autowrap = self.state.grid.autowrap;
        self.state.grid.insert_mode = false;
        self.state.grid.autowrap = true;

        // `char_attr.Attr = AttrDefault` clears only the low byte; explicit
        // colours and DECSCA protection live in Attr2 and survive. Upstream's
        // apparent save is not restored at the end — it writes `char_attr`
        // rather than `svCharAttr` back — so the last debug byte also leaves
        // this primary attribute behind after debug mode is switched off.
        self.state.grid.pen.attrs &= !ATTR_MASK;

        match self.debug_mode {
            DebugMode::Off => unreachable!("the ordinary parser handles off"),
            DebugMode::Hex => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                self.state.print(char::from(HEX[usize::from(byte >> 4)]));
                self.state.print(char::from(HEX[usize::from(byte & 0x0f)]));
                self.state.print(' ');
            }
            DebugMode::Normal => {
                if byte & 0x80 != 0 {
                    self.state.grid.pen.attrs |= ATTR_REVERSE;
                    byte &= 0x7f;
                }
                if byte <= 0x1f {
                    self.state.print('^');
                    self.state.print(char::from(byte + 0x40));
                } else if byte == 0x7f {
                    for c in "<DEL>".chars() {
                        self.state.print(c);
                    }
                } else {
                    self.state.print(char::from(byte));
                }
            }
            DebugMode::NoOutput => {}
        }

        self.state.grid.insert_mode = insert;
        self.state.grid.autowrap = autowrap;
    }

    /// TTL's `setdebug`: select a mode directly, without consulting the file's
    /// keyboard gate or cycle mask.
    pub fn set_debug_mode(&mut self, mode: DebugMode) {
        self.debug_mode = mode;
    }

    pub fn debug_mode(&self) -> DebugMode {
        self.debug_mode
    }

    /// Shift+Escape. False means the key was not a debug key and should carry
    /// on through ordinary keyboard encoding.
    pub fn cycle_debug_mode(&mut self) -> bool {
        if !self.state.config.debug_enabled || self.state.config.debug_modes.is_empty() {
            return false;
        }
        loop {
            self.debug_mode = self.debug_mode.next();
            if self.state.config.debug_modes.allows(self.debug_mode) {
                return true;
            }
        }
    }

    /// Fold 8-bit C1 controls into something `vte` can act on.
    ///
    /// `vtterm.c:1053`: on a non-English terminal — which UTF-8 is, for that
    /// predicate — a C1 byte is dropped outright when `TF_ACCEPT8BITCTRL` is
    /// clear, and masked to `b & 0x7F` when the terminal's VT level is below 2.
    /// So on the default VT100, `U+008D` is a **carriage return**, not RI, and
    /// `U+009B` is an ESC rather than a CSI introducer. Verified against the
    /// oracle across all 32 C1 codes rather than assumed.
    ///
    /// At level 2 and up the mask does not apply and the control keeps its C1
    /// meaning; we hand those to `vte` in the equivalent `ESC Fe` form, since
    /// its parser reaches the same states either way.
    ///
    /// This runs over the whole stream, including OSC and DCS payloads. Tera
    /// Term decodes UTF-8 before its escape parser too, so it has the same
    /// property; a C1 inside a string is mangled by both.
    ///
    /// The other half of the job is a byte in `80..=9F` that is **not** part of
    /// any sequence. `vte` executes it as a C1 control; Tera Term's decoder
    /// never sees a control at all, because a lone continuation byte is invalid
    /// UTF-8 and comes out as U+FFFD. Telling the two apart is the whole reason
    /// this walk has to track sequence lengths rather than look at bytes one at
    /// a time — the `80` in an em dash's `E2 80 94` is a continuation byte, and
    /// replacing it would eat the character.
    fn rewrite_c1(&mut self, bytes: &[u8]) -> Vec<u8> {
        let accept = self.state.config.accept_8bit_ctrl;
        let level = self.state.vt_level;
        // **HTS is the one C1 that must not be folded**, because
        // `ts.TabStopFlag` has a separate bit for each of its two spellings
        // (`vtterm.c:1160` for `0x88`, `:1512` for `ESC H`) and the fold is
        // what makes them the same sequence. So `0x88` goes through raw and
        // `Perform::execute` answers for it — the one channel `vte` has that
        // an `ESC H` cannot arrive on. Everything else is folded, because
        // `vte` does *not* route a raw C1 into its sequence: `0x9B` arrives at
        // `execute` too, rather than opening a CSI.
        let hts8 = self.state.config.tab_stop_modify.allows(TabStopFlags::HTS8);
        // Whatever was held back last time leads off, already vetted. If it is
        // there at all then `utf8_left` is non-zero, so the loop below picks up
        // mid-sequence exactly where it left off.
        let mut out = std::mem::take(&mut self.held);
        out.reserve(bytes.len());
        // Where the sequence in progress starts in `out`. Only meaningful while
        // `utf8_left > 0`, and 0 on entry because that is where `held` put it.
        let mut seq_start = 0;

        for &b in bytes {
            if self.pending_c2 {
                self.pending_c2 = false;
                if (0x80..=0x9f).contains(&b) {
                    if !accept {
                        continue; // dropped, as upstream drops it
                    } else if level < 2 {
                        // C1 folds to C0 below VT level 2, so `0x88` is a BS
                        // here and never reaches HTS at all — which is why the
                        // gate above it is not consulted on this arm.
                        out.push(b & 0x7f);
                    } else if b == 0x88 {
                        // Refused, exactly as upstream refuses it: the control
                        // is consumed and no stop is set.
                        if hts8 {
                            out.push(0x88);
                        }
                    } else {
                        out.push(0x1b);
                        out.push(b - 0x40);
                    }
                    continue;
                }
                // Not a C1 after all — put the lead byte back and fall through
                // so this byte is handled normally.
                out.push(0xc2);
            } else if self.utf8_left > 0 {
                if (0x80..=0xbf).contains(&b) {
                    self.utf8_left -= 1;
                    out.push(b);
                    continue;
                }
                // The sequence was cut short. Whatever this byte is, it starts
                // something new rather than continuing what came before.
                //
                // **The two decoders disagree about how loud that is.** Tera
                // Term's `ParseFirst` emits one U+FFFD for *every byte* it had
                // taken (`charset.c`'s fallback path), and `vte` emits one for
                // the whole maximal subpart — so `E2 82 'b'` is two
                // replacement characters upstream and one here, and every
                // wider sequence widens the gap. Nothing caught it, because
                // the only broken sequence in `cases/` was a bare C1 byte,
                // which is one byte either way.
                let taken = out.len() - seq_start;
                out.truncate(seq_start);
                for _ in 0..taken {
                    out.extend_from_slice("\u{fffd}".as_bytes());
                }
                self.utf8_left = 0;
            }

            match b {
                // The only lead byte that can produce a C1 codepoint, and so
                // the only one worth holding back.
                0xc2 => self.pending_c2 = true,
                0xc0..=0xdf => {
                    self.utf8_left = 1;
                    seq_start = out.len();
                    out.push(b);
                }
                0xe0..=0xef => {
                    self.utf8_left = 2;
                    seq_start = out.len();
                    out.push(b);
                }
                0xf0..=0xf7 => {
                    self.utf8_left = 3;
                    seq_start = out.len();
                    out.push(b);
                }
                // A bare C1 byte. Invisible until a line is not 8-bit clean or
                // the baud rate is wrong, and then it is the whole difference
                // between a screen full of replacement characters — which says
                // *something is arriving and it is wrong* — and a screen that
                // stays blank, which says the far end is dead.
                0x80..=0x9f => out.extend_from_slice("\u{fffd}".as_bytes()),
                _ => out.push(b),
            }
        }

        // A sequence still in progress goes no further. See [`Vt::held`] — the
        // point is that `vte` never sees a partial one, because its resumption
        // path drops bytes.
        if self.utf8_left > 0 {
            self.held = out.split_off(seq_start);
        }
        out
    }

    pub fn grid(&self) -> &Grid {
        &self.state.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.state.grid
    }

    /// Images belonging to the active screen, in painting order.
    pub fn sixel_images(&self) -> impl Iterator<Item = &SixelImage> {
        let alternate = self.state.alt_screen;
        self.state
            .sixel_images
            .iter()
            .filter(move |image| image.alternate() == alternate)
    }

    /// Reconcile image tiles after a caller changed the grid directly through
    /// [`Vt::grid_mut`]. Incoming bytes do this automatically; resize and the
    /// few user-side screen operations use this seam after their edit.
    pub fn reconcile_sixels(&mut self) {
        self.state.reconcile_sixels();
    }

    /// The settings the terminal is running under.
    ///
    /// A settings dialog starts from this rather than from the defaults, so
    /// that a page nobody opened does not quietly reset what it holds — and so
    /// that the four fields an escape sequence can write are read back as they
    /// now are, not as the file left them.
    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Apply settings to a running terminal — the dialog's OK button, which
    /// upstream spells `CVTWindow::SetupTerm` (`vtwin.cpp:1383`).
    ///
    /// **Upstream keeps no separate copy of most of this.** `vtterm.c` reads
    /// `ts` at the point of use, so writing the setting *is* applying it, and
    /// the escape sequences write the very same variables back: DECBKM assigns
    /// `ts.BSKey` (`vtterm.c:2992`), SRM assigns `ts.LocalEcho` (`:2053`), LNM
    /// assigns `ts.CRSend` (`:2059`). Our `Modes` holds copies of those
    /// three, so they are refreshed here — which means applying settings
    /// overwrites what the host asked for, exactly as it does upstream. That is
    /// surprising the first time it happens and is not a bug.
    ///
    /// The two upstream *does* keep separately are deliberately left alone:
    /// `LFMode` and `AcceptWheelToCursor` are seeded from `ts` at reset
    /// (`vtterm.c:285`, `:290`) and `SetupTerm` never touches them.
    ///
    /// The grid is resized to match, so a caller with a connection has to tell
    /// the far end afterwards — `tt_session::Session::apply_settings` is the
    /// thing that does both.
    pub fn set_config(&mut self, config: Config) {
        let s = &mut self.state;
        s.config = config;

        // `ChangeTerminalID` (`vtterm.c:5850`): a terminal ID changed in the
        // dialog re-derives the VT level, and level 1 never sends 8-bit
        // controls whatever the setting says.
        s.vt_level = s.config.term_id.vt_level();
        s.send_8bit = s.vt_level >= 2 && s.config.send_8bit_ctrl;

        s.modes.local_echo = s.config.local_echo;
        s.modes.bs_key_is_bs = s.config.bs_key_is_bs;
        s.modes.cr_send = s.config.cr_send;

        s.grid.set_scrollback_max(s.config.scrollback_max);
        s.grid.set_clear_on_resize(s.config.clear_on_resize);
        s.grid.set_back_wrap(s.config.back_wrap);
        s.grid.set_vt_compat_tab(s.config.vt_compat_tab);
        s.grid.resize(s.config.cols, s.config.rows);

        // `SetupTerm` opens with `ResetCharSet()`, so a G1 designation made by
        // the host does not survive the dialog. Reproduced rather than
        // improved on: it is the same rule as the three above.
        s.charset.reset();

        // **Diverges, and the divergence is one line of upstream that has been
        // commented out.** `ResetSetup` has a `BGInitialize(FALSE)` at
        // `vtwin.cpp:1348` inside an `#if 0`, whose comment says it was removed
        // because it would disable a theme that is only read at startup — so
        // applying settings in Tera Term leaves every live colour alone, and a
        // colour changed in the dialog does not appear until Setup > Restore
        // setup or Control > Reset terminal. The reason for that is a theme
        // file this port does not have, and the cost of copying it is a
        // settings dialog whose colour tab silently does nothing. So the live
        // colours are refreshed here, which is what upstream's `SetColor` does
        // on the two paths where it still runs.
        s.colors = color::Colors::new(&s.config);
        s.colors_dirty = true;
        s.reconcile_sixels();
    }

    /// The colours the frontend should paint with — the live ones, which a host
    /// can move with `OSC 4`/`5`/`10`-`19`. [`Config`] holds what the settings
    /// asked for, which is a different question and is what a reset returns to.
    pub fn colors(&self) -> &color::Colors {
        &self.state.colors
    }

    /// Whether [`Vt::colors`] has moved since this was last asked, and clear.
    ///
    /// The frontend caches the colours — a painter that asked the core for one
    /// per cell would cross the ABI a few thousand times a frame — so it needs
    /// to be told when the cache is stale. Upstream's equivalent is the
    /// `InvalidateRect` at the end of `DispSetColor` and `DispResetColor`.
    pub fn take_colors_changed(&mut self) -> bool {
        std::mem::take(&mut self.state.colors_dirty)
    }

    /// Tell the engine what its window is, so that `CSI 11`/`13`/`14`/`15`/`16`
    /// `/19 t` can answer.
    ///
    /// A snapshot rather than a callback because the answer has to be composed
    /// *while the sequence is parsed* — there is nowhere in `advance` to go and
    /// ask a toolkit — so the frontend pushes on every move, resize and window
    /// state change and the engine reads what it was last told. Stale between
    /// the change and the push, which is one turn of an event loop and no
    /// worse than what a host asking two questions in a row would see anyway.
    pub fn set_window_metrics(&mut self, metrics: window::WindowMetrics) {
        self.state.window = metrics;
    }

    pub fn window_metrics(&self) -> window::WindowMetrics {
        self.state.window
    }

    /// Whether `CSI 8 t` has resized the terminal since this was last asked,
    /// and clear.
    ///
    /// Only that sequence sets it — not [`Vt::resize`] — so a frontend can
    /// resize its window in response without the answer coming back round as
    /// another request.
    pub fn take_terminal_resized(&mut self) -> bool {
        std::mem::take(&mut self.state.terminal_resized)
    }

    /// What `CSI 1`-`10 t` asked the window to do, and clear.
    ///
    /// A queue rather than an immediate action for the reason
    /// [`Vt::take_bells`] is a count: the engine has no window. A frontend that
    /// has none either can drop these, which is what `tt-host` does.
    pub fn take_window_requests(&mut self) -> Vec<window::WindowRequest> {
        std::mem::take(&mut self.state.window_requests)
    }

    /// What the media-copy sequences asked the printer for, in order, and
    /// clear. See [`printer::PrinterEvent`].
    ///
    /// A queue for the same reason [`Vt::take_window_requests`] is one, plus
    /// one of its own: a job is `Open`, some `Write`s and a `Close`, and a
    /// frontend that saw only the last of those would print nothing. A
    /// frontend with no printer can drop them, which is what `tt-host` does.
    pub fn take_printer_events(&mut self) -> Vec<printer::PrinterEvent> {
        std::mem::take(&mut self.state.printer_events)
    }

    /// Whether printer controller mode has the stream — `PrinterMode`. The
    /// frontend needs it for nothing; it is here because a test that cannot see
    /// this mode cannot tell it from a terminal that has stopped responding.
    pub fn printer_controller(&self) -> bool {
        self.state.printer.is_on()
    }

    /// Whether every completed line is being printed — `AutoPrintMode`.
    pub fn auto_print(&self) -> bool {
        self.state.auto_print
    }

    /// Bytes the terminal wants to send back to the host: DA, DSR, and friends.
    pub fn reply(&self) -> &[u8] {
        &self.state.reply
    }

    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.state.reply)
    }

    /// Append bytes to the host-bound stream, for input the terminal did not
    /// generate itself — a key, or a paste. Upstream's keyboard writes
    /// through the same `CommBinaryOut` the escape replies use, so keeping
    /// them in one stream is what makes the two engines comparable.
    pub fn push_reply(&mut self, bytes: &[u8]) {
        self.state.send(bytes);
    }

    /// What the host has asked of the bell since this was last called.
    ///
    /// A count rather than an event, and not one bell rather than a count,
    /// because upstream's governor (`vtterm.c:5791`) is a state machine that
    /// every request steps whether or not it makes a sound. Collapsing a burst
    /// here would leave the terminal audible through the next one.
    ///
    /// The governor itself is deliberately **not** in the engine: it is four
    /// settings and a clock, and `Vt` has no clock — which is what lets the
    /// differential suite and the fuzzers treat it as a function of its bytes.
    /// `tt_session` runs it, one step per count, against a single `Instant`.
    pub fn take_bells(&mut self) -> BellRequests {
        BellRequests {
            reset: std::mem::take(&mut self.state.bell_reset),
            count: std::mem::take(&mut self.state.bells),
        }
    }

    /// OSC 52 work waiting for the frontend. Drained for the same reason as a
    /// session event: a GUI clipboard may only be used on the GUI thread.
    pub fn take_clipboard_requests(&mut self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.state.clipboard_requests)
    }

    /// Answer one accepted OSC 52 read with UTF-8 text.
    ///
    /// The selector is copied back verbatim, and ST is used even when the
    /// request ended in BEL — `CBStartPasteB64` is always handed `"\e\\"`
    /// (`vtterm.c:5006`). False means upstream would send nothing: the
    /// selector does not fit its 20-byte header, or the clipboard is not text.
    pub fn clipboard_reply(&mut self, selection: &str, text: &str) -> bool {
        // `hdr[20]` starts with five bytes of `ESC ] 5 2 ;`, then receives the
        // selector and its semicolon through `strncat_s`.
        if selection.len() > 13 || !selection.bytes().all(is_clipboard_selector) {
            return false;
        }
        // `GetClipboardTextW` is passed through `IsTextW`. Its zero length is
        // valid; a NUL ends the Win32 string before this test.
        let text = text.split('\0').next().unwrap_or("");
        if !text.chars().all(is_clipboard_text) {
            return false;
        }

        self.state.send(b"\x1b]52;");
        self.state.send(selection.as_bytes());
        self.state.send(b";");
        self.state.send(&base64_encode(text.as_bytes()));
        self.state.send(b"\x1b\\");
        true
    }

    /// The last title the *host* set, with OSC 0, 1 or 2 — `cv.TitleRemoteW`.
    /// Empty if it has never set one, or if [`TitleChange::Off`] is discarding
    /// them.
    ///
    /// This is what the host asked for and not what the window shows; see
    /// [`Vt::window_title`] for that.
    pub fn remote_title(&self) -> &str {
        &self.state.title
    }

    /// Set [`Config::title`] — `ts.Title`, which is what `settitle` writes and
    /// `gettitle` reads (`ttdde.c:636`, `:646`).
    ///
    /// Under [`TitleChange::Overwrite`] it also **discards the host's title**
    /// (`ttdde.c:838`), because that mode would otherwise keep hiding the one
    /// just set. The other three leave it: `ahead` and `last` show both, and
    /// `off` never had one.
    pub fn set_title(&mut self, title: String) {
        self.state.config.title = title;
        if self.state.config.accept_title_change == TitleChange::Overwrite {
            self.state.title.clear();
        }
    }

    /// What the title bar should say — `ChangeTitle`'s half of `ttwinman.c:95`,
    /// which is [`Config::title`] and [`Vt::remote_title`] combined the way
    /// [`Config::accept_title_change`] says.
    ///
    /// Upstream does this in the frontend and so could this, but the same two
    /// strings are combined again inside `CSI 20 t`, which is the terminal's —
    /// so one of the two callers is here regardless and the rule lives with it.
    pub fn window_title(&self) -> String {
        self.state
            .config
            .accept_title_change
            .combine(&self.state.config.title, &self.state.title)
    }

    /// Report a mouse event, in **window pixels**. Returns true if the
    /// terminal consumed it — the frontend should then not also treat the
    /// click as the start of a text selection.
    ///
    /// `button` is upstream's numbering ([`mouse::BUTTON_LEFT`] and friends);
    /// for [`MouseEvent::Wheel`] it is 0 for up and 1 for down.
    pub fn mouse_event(
        &mut self,
        event: MouseEvent,
        button: u8,
        px: i32,
        py: i32,
        mods: Modifiers,
    ) -> bool {
        self.state.mouse_report(event, button, px, py, mods)
    }

    /// Focus gained or lost. Emits nothing unless `DECSET 1004` is on.
    pub fn focus_event(&mut self, focused: bool) {
        self.state.focus_report(focused);
    }

    /// Which mouse events the host has asked for. The frontend needs this to
    /// decide whether to track motion at all.
    pub fn mouse_tracking(&self) -> Tracking {
        self.state.mouse.tracking
    }

    pub fn mouse_encoding(&self) -> Encoding {
        self.state.mouse.encoding
    }

    pub fn focus_reporting(&self) -> bool {
        self.state.mouse.focus_report
    }

    /// Tell the core how big a character cell is, after a font change. It is
    /// used only to convert mouse positions and to answer pixel-mode reports.
    pub fn set_cell_pixels(&mut self, w: i32, h: i32) {
        let cell = (w.max(1), h.max(1));
        self.state.config.cell_w = cell.0;
        self.state.config.cell_h = cell.1;
        // A lightweight frontend may only have a terminal view rather than a
        // top-level window whose full metrics it can report. Sixel placement
        // and `CSI 16 t` still need the cell it did provide.
        self.state.window.cell = cell;
    }

    /// DECCKM. The frontend sends `ESC O A` rather than `ESC [ A` while it is
    /// on — but the keymap lives in the core, so this exists for the shell's
    /// own decisions (scrollbar behaviour, wheel translation) rather than for
    /// building key sequences.
    pub fn application_cursor_keys(&self) -> bool {
        self.state.modes.appli_cursor
    }

    /// DECNKM.
    pub fn application_keypad(&self) -> bool {
        self.state.modes.appli_key
    }

    /// DECTCEM. False means the cursor should not be drawn.
    pub fn cursor_visible(&self) -> bool {
        self.state.modes.caret
    }

    /// `DECSET 2004`. A paste must be wrapped in `ESC [ 200 ~` … `ESC [ 201 ~`
    /// while it is on.
    pub fn bracketed_paste(&self) -> bool {
        self.state.modes.bracketed_paste
    }

    /// DECSCNM. The whole screen is drawn with foreground and background
    /// swapped; nothing in the grid changes, which is why it lives here.
    pub fn reverse_video(&self) -> bool {
        self.state.modes.reverse_video
    }

    /// KAM. The host has asked that typing be ignored.
    pub fn keyboard_enabled(&self) -> bool {
        self.state.modes.keyb_enabled
    }

    /// SRM — the host wants the terminal to echo locally.
    pub fn local_echo(&self) -> bool {
        self.state.modes.local_echo
    }

    /// Set local echo from outside the byte stream.
    ///
    /// The same variable SRM writes, deliberately: upstream has one
    /// `ts.LocalEcho` and three things assign it — the file, `ESC [ 12 h`
    /// (`vtterm.c:2053`) and the telnet `ECHO` negotiation when `TelEcho` is on
    /// (`telnet.c:411`). Giving the transport a second flag to be ANDed in
    /// would make `DECRQM`'s answer for SRM stop describing what the terminal
    /// does, which is the one thing that mode is for.
    pub fn set_local_echo(&mut self, on: bool) {
        self.state.modes.local_echo = on;
    }

    /// What a CR from the keyboard sends. LNM writes it and so does the file.
    pub fn cr_send(&self) -> CrSend {
        self.state.modes.cr_send
    }

    /// Set it from outside the byte stream, the way `TCPCRSend` does.
    ///
    /// `vtwin.cpp:3691` assigns `ts.CRSend` **and** `cv.CRSend` when a
    /// non-telnet TCP connection opens; one variable here covers both, since
    /// [`Vt::encode_text`] and [`Key::encode`] read the same field.
    ///
    /// **`lf_mode` is deliberately not touched**, unlike in `SM 20`, which
    /// moves the two together (`vtterm.c:2058`). Upstream's `LFMode` is a
    /// separate variable seeded from `ts.CRSend` at reset and nowhere else
    /// (`:285`), so a `TCPCRSend=CRLF` connection sends CR LF from the keyboard
    /// while a received LF still does not carry a CR with it — and DECRQM goes
    /// on reporting mode 20 reset. The pair is only one fact when the host says
    /// it is.
    pub fn set_cr_send(&mut self, cr_send: CrSend) {
        self.state.modes.cr_send = cr_send;
    }

    /// LNM. A CR from the keyboard sends CR LF while it is on.
    pub fn newline_mode(&self) -> bool {
        self.state.modes.lf_mode
    }

    /// DECBKM. False means Backspace sends DEL rather than BS.
    pub fn backspace_sends_bs(&self) -> bool {
        self.state.modes.bs_key_is_bs
    }

    /// `DECSET 7786` — translate the wheel into cursor keys when the
    /// application cursor mode is on.
    pub fn wheel_to_cursor(&self) -> bool {
        self.state.modes.wheel_to_cursor
    }

    /// Whether a wheel notch should go out as a cursor key rather than scroll
    /// the frontend's own view — `vtterm.c:WheelToCursorMode`, whole.
    ///
    /// Four terms, and the frontend has no business assembling them: the mode,
    /// the application cursor mode, the setting that vetoes that mode without
    /// unsetting it, and Ctrl under `DisableWheelToCursorByCtrl`. The last is
    /// why this takes the modifiers, the same way [`Vt::mouse`] does — Ctrl is
    /// the escape hatch that gets the history back while a full-screen program
    /// is up.
    pub fn wheel_to_cursor_now(&self, mods: Modifiers) -> bool {
        let m = &self.state.modes;
        m.wheel_to_cursor
            && m.appli_cursor
            && !self.state.config.disable_app_cursor
            && !(mods.ctrl && self.state.config.disable_wheel_to_cursor_by_ctrl)
    }

    /// The modes [`Key::encode`] reads, as the terminal currently stands.
    ///
    /// The two `Disable*` settings veto their mode *here* rather than at
    /// DECSET time, matching `keyboard.c:KeyCodeSend` — so a host can set
    /// DECCKM, have DECRQM confirm it, and still get the normal cursor keys.
    pub fn key_modes(&self) -> KeyModes {
        let m = &self.state.modes;
        KeyModes {
            application_cursor: m.appli_cursor && !self.state.config.disable_app_cursor,
            application_keypad: m.appli_key && !self.state.config.disable_app_keypad,
            eight_bit: self.state.send_8bit,
            cr_send: m.cr_send,
        }
    }

    /// What `key` puts on the wire right now, or `None` for a key that is a
    /// local command. The core owns this because the encoding depends on
    /// terminal state the frontend never sees.
    pub fn key(&self, key: Key) -> Option<Vec<u8>> {
        key.encode(self.key_modes())
    }

    /// Turn the plain-text log tap on or off.
    ///
    /// While it is on, every character the parser decides to *print* is
    /// recorded, plus a newline for each line feed. Escape sequences never
    /// appear because the parser has already consumed them — which is the
    /// point, and the reason this lives here rather than in a stripper beside
    /// the log. A second escape-sequence scanner would be one more place to
    /// disagree with the one that is verified against Tera Term.
    ///
    /// This is upstream's `FLogPutUTF32` seam, reached from `vtterm.c:468`
    /// with the same characters at the same moments.
    pub fn set_log_text_enabled(&mut self, on: bool) {
        if on {
            self.state.log_text.get_or_insert_with(String::new);
        } else {
            self.state.log_text = None;
        }
    }

    /// Take what the tap has collected since the last call, leaving the buffer
    /// allocated for reuse. Empty when the tap is off.
    pub fn take_log_text(&mut self) -> String {
        match &mut self.state.log_text {
            Some(buf) => std::mem::take(buf),
            None => String::new(),
        }
    }

    /// Turn the macro tap on or off — upstream's `DDELog`, which is set when a
    /// macro links to the terminal and cleared when it unlinks
    /// (`ttdde.c:1382`, `:1432`).
    ///
    /// **What a macro reads is not what the far end sent.** It is the text the
    /// parser printed, plus `CR`, `LF`, `BS` and `HT` where those controls
    /// executed and a `CR LF` where a line wrapped — so `wait 'ESC'` can never
    /// match, an erased character is in the stream anyway, and a line arrives
    /// with the CR still on it. The `MacroTap` type this crate keeps privately
    /// documents why each of those is the way it is; between them they decide
    /// what every `wait` in every script matches against.
    ///
    /// Turning it off discards what it had collected, which is upstream's
    /// `DDEFreeBuf`.
    pub fn set_macro_tap_enabled(&mut self, on: bool) {
        if on {
            self.state.macro_tap.get_or_insert_with(MacroTap::default);
        } else {
            self.state.macro_tap = None;
        }
    }

    /// Whether a macro is listening.
    pub fn macro_tap_enabled(&self) -> bool {
        self.state.macro_tap.is_some()
    }

    /// Take what the macro tap has collected, leaving its buffer allocated.
    ///
    /// Unbounded between calls, deliberately: the caller drains it every pump
    /// into the ring that *is* bounded, and putting the 64 KiB limit here as
    /// well would drop bytes twice with two different policies.
    pub fn take_macro_bytes(&mut self) -> Vec<u8> {
        match &mut self.state.macro_tap {
            Some(t) => std::mem::take(&mut t.buf),
            None => Vec::new(),
        }
    }

    /// Encode typed text — `ttcmn.c:OutControl`, which every `IdText` byte
    /// goes through on its way out.
    ///
    /// The only thing it does today is expand CR by [`CrSend`], and that one
    /// thing is why the function exists: the main Return key is **not** in
    /// [`Key`], because upstream handles `VK_RETURN` in `KeyDown` rather than
    /// in the key table, marking it `IdText` precisely so this conversion
    /// applies (`keyboard.c:908`). A frontend sending a bare `\r` would
    /// otherwise have to know about LNM, and the keymap is the core's.
    ///
    /// Not applied to a paste: bracketed paste means "send this verbatim",
    /// and rewriting newlines inside pasted text is how a heredoc or a YAML
    /// block arrives corrupted.
    ///
    /// Telnet adds an arm here — `IdCR` with the connection not in binary
    /// mode appends a NUL — which is `cv->TelFlag` and belongs with the
    /// transport that has it.
    pub fn encode_text(&self, text: &str) -> Vec<u8> {
        let cr_send = self.state.modes.cr_send;
        if cr_send == CrSend::Cr || !text.contains('\r') {
            return text.as_bytes().to_vec();
        }
        let mut out = Vec::with_capacity(text.len() + 8);
        for b in text.bytes() {
            match (b, cr_send) {
                (0x0d, CrSend::CrLf) => out.extend_from_slice(&[0x0d, 0x0a]),
                (0x0d, CrSend::Lf) => out.push(0x0a),
                _ => out.push(b),
            }
        }
        out
    }
}

enum Dcs {
    /// Tera Term's 255-byte request strings: DECRQSS, XTGETTCAP and DECSTUI.
    Short {
        intermediate: Option<u8>,
        action: char,
    },
    /// A sixel DCS is decoded as it arrives rather than collected. `line` and
    /// `column` name the text position where its first pixel belongs even if
    /// completing the image later scrolls that line into history.
    Sixel {
        decoder: Box<sixel::Decoder>,
        line: u64,
        column: usize,
        scrolling: bool,
        alternate: bool,
    },
}

struct State {
    grid: Grid,
    config: Config,
    charset: Iso2022,
    /// DECSC saves the G-sets alongside the cursor — `vtterm.c:228` — and so
    /// there is one slot per screen, the same two `Grid` keeps the position in.
    saved_charset: [Option<Iso2022State>; 2],
    alt_screen: bool,
    reply: Vec<u8>,
    title: String,
    /// `TitleStack` (`vtterm.c:2757`) — what `CSI 22 t` puts away and
    /// `CSI 23 t` brings back.
    title_stack: Vec<String>,
    /// `ts.CRReceive == Auto` keeps one byte of history to collapse CR+LF.
    prev_was_cr: bool,
    prev_was_lf: bool,
    auto_generated_crlf: bool,
    /// The last printable codepoint, for REP.
    last_printed: Option<u32>,
    /// The plain-text log tap — `Some` only while a text log is open, so the
    /// cost when it is not is one branch per printed character. See
    /// [`Vt::take_log_text`].
    log_text: Option<String>,
    /// The macro tap — `Some` only while a macro is linked, which is upstream's
    /// `DDELog` (`ttdde.c:69`). See [`Vt::set_macro_tap_enabled`].
    macro_tap: Option<MacroTap>,
    /// DECSACE's `RectangleMode` (`vtterm.c:113`). False — stream — out of
    /// reset, and it decides how DECCARA and DECRARA read their rectangle.
    rect_mode: bool,
    /// DECLRMM's `LRMarginMode` (`vtterm.c:112`). While it is off, `CSI Ps ; Ps s`
    /// is SCP (save cursor) rather than DECSLRM, and there are no margins to
    /// set — which is why the mode exists at all.
    lr_margin_mode: bool,
    /// `VTlevel`. Starts at the terminal id's level and can only be *lowered*
    /// by DECSCL, never raised past what the id claims.
    vt_level: u8,
    /// `Send8BitMode`. Only ever true above level 1, and only when the setting
    /// or DECSCL says so.
    send_8bit: bool,
    /// The DCS in progress. Short terminal queries retain their small payload;
    /// sixel graphics decode into a bounded raster as the bytes arrive.
    dcs: Option<Dcs>,
    dcs_buf: Vec<u8>,
    /// Persistent sixel rasters, oldest first. Each carries an absolute line
    /// anchor, so ordinary terminal scrolling moves it without rewriting it.
    sixel_images: Vec<sixel::SixelImage>,
    mouse: mouse::MouseState,
    modes: Modes,
    /// Bells asked for and not yet collected. See [`Vt::take_bells`].
    bells: u32,
    /// A RIS went past, which is where `ResetTerminal` puts the governor's
    /// clocks back (`vtterm.c:348`).
    bell_reset: bool,
    /// OSC 52 actions waiting for the toolkit which owns the clipboard.
    clipboard_requests: Vec<ClipboardRequest>,
    /// The live colours — `vtdraw_t`'s, not `ts`'s. See [`color::Colors`].
    colors: color::Colors,
    /// A colour OSC moved something and the painter has not been told.
    ///
    /// A flag rather than a repaint, for the reason `take_bells` is a count
    /// rather than a noise: `InvalidateRect` is the one thing `DispSetColor`
    /// does that this engine has no window to do. See
    /// [`Vt::take_colors_changed`].
    colors_dirty: bool,
    /// What the frontend last said its window was. See [`WindowMetrics`].
    window: window::WindowMetrics,
    /// XTWINOPS operations waiting for the frontend which owns a window.
    window_requests: Vec<window::WindowRequest>,
    /// `CSI 8 t` resized the terminal and the window has not been told.
    ///
    /// The engine resizes here rather than asking, because upstream does and
    /// because the differential dump is taken at `NumOfColumns`/`NumOfLines` —
    /// but a window that does not follow leaves the painter drawing more cells
    /// than it has room for until the next resize event puts it back. See
    /// [`Vt::take_terminal_resized`].
    terminal_resized: bool,
    /// Printer controller mode — `CSI 5 i` until `CSI 4 i`.
    printer: printer::Controller,
    /// `AutoPrintMode` (`vtterm.c:178`): every completed line goes to the
    /// printer as well as to the screen.
    auto_print: bool,
    /// Whether a job is open — upstream's `PrintFile_ != NULL`. The two modes
    /// share one, which is why `CSI ? 1 i` closes the job it opened only when
    /// auto print is off.
    printer_open: bool,
    /// What the printer has been asked for and not yet collected. See
    /// [`Vt::take_printer_events`].
    printer_events: Vec<printer::PrinterEvent>,
    /// `CheckEOLData_st::cr_hold` for the printer's copy of the text. Upstream
    /// runs *one* `CheckEOLCheck` per character and fans its answer out to the
    /// log, the macro and the printer (`vtterm.c:453`); here each tap keeps its
    /// own, for the reason [`MacroTap`] gives.
    printer_cr_hold: bool,
    /// The line auto print dumped just before a wrap, reused rather than
    /// allocated per character. See [`Perform::print`].
    printer_line: String,
}

/// What a linked macro sees of the session — `ttdde.c`'s `DDEPut1` sink, and
/// the `CheckEOLCheckLog` state in front of it (`checkeol.cpp:105`).
///
/// **A macro does not see the wire.** This is the surprise at the bottom of the
/// whole macro language: `wait`, `waitln`, `waitregex` and `recvln` match
/// against the characters the parser decided to *display*, re-encoded as UTF-8,
/// not against the bytes the far end sent. An escape sequence never reaches a
/// macro at all, because the parser consumed it; a character the host sent and
/// then erased is in the stream anyway, because it was printed once.
///
/// Upstream reaches this sink from one function, `OutputLogUTF32`
/// (`vtterm.c:448`), which also feeds the text session log — so the two taps
/// see the same moments. Here they are two fields because they do not want the
/// same bytes: the log's newline is a policy the user sets
/// (`LogOptions::crlf`) and the macro's is not, and getting the macro's wrong
/// breaks every script that ever matched a `$`.
#[derive(Debug, Default, Clone)]
struct MacroTap {
    buf: Vec<u8>,
    /// `CheckEOLData_st::cr_hold`. A lone CR is **dropped**, and a CR followed
    /// by an LF becomes one `CR LF`. So `abc\rdef` reaches a macro as `abcdef`
    /// — the text as it was printed, with the overwrite invisible — and
    /// `abc\r\n` reaches it with its CR intact, which is why a `waitregex`
    /// pattern ending in `$` never matches a line from a normal host.
    cr_hold: bool,
}

impl MacroTap {
    /// `OutputLogUTF32`, macro branch. Everything the tap sees comes through
    /// here, characters and control bytes alike, because upstream's control
    /// bytes go through `OutputLogByte` which is a one-line call to it.
    fn put(&mut self, u32: u32) {
        // `CheckEOLCheckLog`.
        let (eol, chr) = match u32 {
            0x0d => {
                self.cr_hold = true;
                (false, false)
            }
            0x0a if self.cr_hold => {
                self.cr_hold = false;
                (true, false)
            }
            0x0a => (false, true),
            _ => {
                self.cr_hold = false;
                (false, true)
            }
        };
        if eol {
            self.buf.extend_from_slice(b"\r\n");
        }
        // `UTF32ToUTF8` writes nothing for a value it cannot encode, and a
        // surrogate is the only way to reach that from a parser that has
        // already decoded its input.
        if chr {
            if let Some(c) = char::from_u32(u32) {
                let mut b = [0u8; 4];
                self.buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
        }
    }
}

impl State {
    fn empty() -> Self {
        State {
            grid: Grid::new(1, 1, 0),
            config: Config::default(),
            charset: Iso2022::new(),
            saved_charset: [None, None],
            alt_screen: false,
            reply: Vec::new(),
            title: String::new(),
            title_stack: Vec::new(),
            prev_was_cr: false,
            prev_was_lf: false,
            auto_generated_crlf: false,
            last_printed: None,
            log_text: None,
            macro_tap: None,
            rect_mode: false,
            lr_margin_mode: false,
            vt_level: 1,
            send_8bit: false,
            dcs: None,
            dcs_buf: Vec::new(),
            sixel_images: Vec::new(),
            mouse: mouse::MouseState::default(),
            modes: Modes::from_config(&Config::default()),
            bells: 0,
            bell_reset: false,
            clipboard_requests: Vec::new(),
            colors: color::Colors::new(&Config::default()),
            colors_dirty: false,
            window: window::WindowMetrics::default(),
            window_requests: Vec::new(),
            terminal_resized: false,
            printer: printer::Controller::default(),
            auto_print: false,
            printer_open: false,
            printer_events: Vec::new(),
            printer_cr_hold: false,
            printer_line: String::new(),
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.reply.extend_from_slice(bytes);
    }

    fn install_sixel(
        &mut self,
        raster: sixel::Raster,
        line: u64,
        column: usize,
        scrolling: bool,
        alternate: bool,
    ) {
        let cell_width = usize::try_from(self.window.cell.0.max(1)).unwrap_or(1);
        let cell_height = usize::try_from(self.window.cell.1.max(1)).unwrap_or(1);
        let max_width = if scrolling {
            self.grid
                .cols()
                .saturating_sub(column)
                .saturating_mul(cell_width)
        } else {
            self.grid.cols().saturating_mul(cell_width)
        };
        let max_height = if scrolling {
            sixel::MAX_HEIGHT
        } else {
            self.grid.rows().saturating_mul(cell_height)
        };
        let Some(raster) = raster.crop(max_width, max_height) else {
            return;
        };

        if scrolling {
            // xterm's default leaves the text cursor in the same column on the
            // first complete row below the image. Advancing through the grid
            // is what gives an image ordinary scrollback semantics when it
            // reaches the bottom of the page.
            let rows = raster.height.div_ceil(cell_height);
            for _ in 0..rows {
                self.grid.line_feed();
            }
            self.grid
                .move_cursor(column.min(self.grid.cols() - 1), self.grid.cursor.y);
        }

        let image = sixel::SixelImage::new(
            raster,
            line,
            column,
            alternate,
            cell_width,
            cell_height,
            &self.grid,
        );
        if image.is_empty() {
            return;
        }

        // A large inline-image workload must not become an unbounded terminal
        // history of its own. The text scrollback has its own cap; graphics
        // get 128 MiB and evict oldest-first, as a screen cache should.
        const MAX_STORAGE: usize = 128 * 1024 * 1024;
        let wanted = image.pixels().len();
        let mut used: usize = self
            .sixel_images
            .iter()
            .map(|stored| stored.pixels().len())
            .sum();
        while !self.sixel_images.is_empty() && used.saturating_add(wanted) > MAX_STORAGE {
            used = used.saturating_sub(self.sixel_images.remove(0).pixels().len());
        }
        if wanted <= MAX_STORAGE {
            self.sixel_images.push(image);
        }
    }

    fn reconcile_sixels(&mut self) {
        let alternate = self.alt_screen;
        for image in &mut self.sixel_images {
            if image.alternate() == alternate {
                image.reconcile(&self.grid);
            }
        }
        self.sixel_images
            .retain(|image| image.alternate() != alternate || !image.is_empty());
    }

    /// `vtterm.c:SendCSIstr` — the introducer is the 8-bit CSI once the host
    /// has asked for one, via S8C1T or DECSCL. Every CSI reply goes through
    /// here **except** the Primary DA, which upstream builds by hand with a
    /// stricter rule of its own; see `'c'` in [`State::csi_dispatch`].
    fn send_csi(&mut self, body: &str) {
        self.send_csi_bytes(body.as_bytes());
    }

    /// The mouse formats are not all UTF-8, so they arrive as raw bytes.
    fn send_csi_bytes(&mut self, body: &[u8]) {
        if self.send_8bit {
            self.send(&[0x9b]);
        } else {
            self.send(b"\x1b[");
        }
        self.send(body);
    }

    /// `vtterm.c:MouseReport`. Returns whether the event was consumed by
    /// reporting — the frontend uses that to decide whether the click is also
    /// a text selection.
    ///
    /// `px`/`py` are window pixels. Every branch below is upstream's, including
    /// the ones that return `true` after sending nothing.
    fn mouse_report(
        &mut self,
        event: MouseEvent,
        button: u8,
        px: i32,
        py: i32,
        m: Modifiers,
    ) -> bool {
        let button = button as i32;

        // The button mask and the last position are updated before any of the
        // early exits, so they stay live while reporting is off. DECRQLP can
        // read them back afterwards.
        match event {
            MouseEvent::Press => self.mouse.button_stat |= 8 >> (button + 1),
            MouseEvent::Release => self.mouse.button_stat &= !(8 >> (button + 1)),
            _ => {}
        }
        self.mouse.last = (px, py);

        if self.mouse.tracking == Tracking::None {
            return false;
        }
        if self.config.disable_mouse_tracking_by_ctrl && m.ctrl {
            return false;
        }
        if self.mouse.tracking == Tracking::DecLocator {
            return self.locator_report(event, button);
        }

        let (x, y) = if self.mouse.encoding == Encoding::SgrPixels {
            // Pixels go out unconverted; only the floor at 1 applies.
            (px.max(1), py.max(1))
        } else {
            let (cx, cy) = self.win_to_screen(px, py);
            (
                (cx + 1).clamp(1, self.grid.cols() as i32),
                (cy + 1).clamp(1, self.grid.rows() as i32),
            )
        };
        let modifier = m.bits();

        let body = match event {
            MouseEvent::CurStat => return false,
            MouseEvent::Press => match self.mouse.tracking {
                Tracking::X10 => mouse::encode(self.mouse.encoding, button, x, y),
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    self.mouse.last_send = (x, y);
                    self.mouse.last_button = button;
                    mouse::encode(self.mouse.encoding, button | modifier, x, y)
                }
                Tracking::NetTerm => {
                    // Not a CSI, and not routed through SendCSIstr — upstream
                    // writes it to the wire directly (`vtterm.c:5687`).
                    let raw = format!("\x1b}}{y},{x}\r");
                    self.send(raw.as_bytes());
                    return true;
                }
                _ => return false,
            },
            MouseEvent::Release => match self.mouse.tracking {
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    // The SGR forms can say *which* button was released; the
                    // others cannot, and report the anonymous button 3.
                    let mb = if matches!(self.mouse.encoding, Encoding::Sgr | Encoding::SgrPixels) {
                        button | modifier | 128
                    } else {
                        mouse::BUTTON_RELEASE as i32 | modifier
                    };
                    self.mouse.last_send = (x, y);
                    self.mouse.last_button = mouse::BUTTON_RELEASE as i32;
                    mouse::encode(self.mouse.encoding, mb, x, y)
                }
                // Nothing to send, but the event is still consumed.
                Tracking::X10 | Tracking::NetTerm => return true,
                _ => return false,
            },
            MouseEvent::Move => match self.mouse.tracking {
                // 1002 reports motion only while a button is held, and the
                // test is against the last button *sent*, not the mask.
                Tracking::BtnEvent | Tracking::AllEvent => {
                    if self.mouse.tracking == Tracking::BtnEvent
                        && self.mouse.last_button == mouse::BUTTON_RELEASE as i32
                    {
                        return false;
                    }
                    if (x, y) == self.mouse.last_send {
                        return false;
                    }
                    self.mouse.last_send = (x, y);
                    let mb = self.mouse.last_button | modifier | 32;
                    mouse::encode(self.mouse.encoding, mb, x, y)
                }
                _ => return false,
            },
            MouseEvent::Wheel => match self.mouse.tracking {
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    mouse::encode(self.mouse.encoding, button | modifier | 64, x, y)
                }
                _ => return false,
            },
        };

        if body.is_empty() {
            return false;
        }
        self.send_csi_bytes(&body);
        true
    }

    /// `vtterm.c:DecLocatorReport`.
    fn locator_report(&mut self, event: MouseEvent, button: i32) -> bool {
        let (last_x, last_y) = self.mouse.last;
        let pixel = self.mouse.locator_flags & mouse::PIXEL != 0;

        // Out of range is signalled by a negative x alone; y keeps its value
        // and is simply not printed.
        let (mut x, y) = if pixel {
            let (max_x, max_y) =
                self.screen_to_win(self.grid.cols() as i32 + 1, self.grid.rows() as i32 + 1);
            let (x, y) = (last_x + 1, last_y + 1);
            (
                if x < 1 || x > max_x || y < 1 || y > max_y {
                    -1
                } else {
                    x
                },
                y,
            )
        } else {
            let (cx, cy) = self.win_to_screen(last_x, last_y);
            let (x, y) = (cx + 1, cy + 1);
            let ok =
                x >= 1 && x <= self.grid.cols() as i32 && y >= 1 && y <= self.grid.rows() as i32;
            (if ok { x } else { -1 }, y)
        };

        let stat = self.mouse.button_stat;
        let body = match event {
            MouseEvent::CurStat => {
                if self.mouse.tracking == Tracking::DecLocator {
                    mouse::encode_locator(1, stat, x, y)
                } else {
                    // DECRQLP with no locator enabled still answers, to say so.
                    b"0&w".to_vec()
                }
            }
            MouseEvent::Press if self.mouse.locator_flags & mouse::BUTTON_DOWN != 0 => {
                mouse::encode_locator(button * 2 + 2, stat, x, y)
            }
            MouseEvent::Release if self.mouse.locator_flags & mouse::BUTTON_UP != 0 => {
                mouse::encode_locator(button * 2 + 3, stat, x, y)
            }
            MouseEvent::Move if self.mouse.locator_flags & mouse::FILTERED != 0 => {
                let (top, left, bottom, right) = self.mouse.filter;
                if y < top || y > bottom || x < left || x > right {
                    self.mouse.locator_flags &= !mouse::FILTERED;
                    // The filter compares against the *unclamped* x, so an
                    // out-of-page locator escapes the rectangle by definition.
                    x = if x < 1 { -1 } else { x };
                    mouse::encode_locator(10, stat, x, y)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };

        if body.is_empty() {
            return false;
        }
        self.send_csi_bytes(&body);
        if self.mouse.locator_flags & mouse::ONE_SHOT != 0 {
            self.mouse.tracking = Tracking::None;
        }
        true
    }

    /// `vtdisp.c:DispConvWinToScreen`, with `WinOrgX`/`WinOrgY` fixed at zero —
    /// the core has no scrolled-back viewport of its own.
    fn win_to_screen(&self, px: i32, py: i32) -> (i32, i32) {
        (
            px / self.config.cell_w.max(1),
            py / self.config.cell_h.max(1),
        )
    }

    fn screen_to_win(&self, cx: i32, cy: i32) -> (i32, i32) {
        (cx * self.config.cell_w, cy * self.config.cell_h)
    }

    /// `vtterm.c:FocusReport`.
    fn focus_report(&mut self, focused: bool) {
        if self.mouse.focus_report {
            self.send_csi(if focused { "I" } else { "O" });
        }
    }

    /// DECBI/DECFI act only inside the scroll region *and* between the
    /// margins — `vtterm.c:1484`.
    fn cursor_in_region(&self) -> bool {
        let (top, bottom) = self.grid.scroll_region();
        let (left, right) = self.grid.margins();
        (top..=bottom).contains(&self.grid.cursor.y) && (left..=right).contains(&self.grid.cursor.x)
    }

    /// Every locking and single shift is gated on `ts.ISO2022Flag` upstream, at
    /// the call site rather than inside the charset code. Same split here.
    fn shift(&mut self, shift: Shift) {
        if self.config.iso2022_flags.allows(shift) {
            self.charset.invoke(shift);
        }
    }

    // --- C0 --------------------------------------------------------------

    /// One character or control byte into the macro tap, if one is listening.
    ///
    /// Upstream's `OutputLogUTF32` also feeds the text log and the printer from
    /// here; the text log has its own field because it wants different bytes,
    /// and there is no printer.
    #[inline]
    fn tap(&mut self, u32: u32) {
        if let Some(t) = &mut self.macro_tap {
            t.put(u32);
        }
    }

    // --- the printer ------------------------------------------------------

    /// MC and DECMC — `CSI Ps i` (`vtterm.c:2079`) and `CSI ? Ps i`
    /// (`CSQ_i_Mode`, `:3082`).
    ///
    /// Four of the five arms are behind `TF_PRINTERCTRL` and the fifth is not:
    /// `CSI ? 4 i` turns auto print off whatever the setting says, so a host
    /// cannot leave a terminal printing every line because the user turned the
    /// gate off half way through. Everything not listed falls through in
    /// silence, `CSI 4 i` included — with the controller already off there is
    /// nothing for it to do, and while it is on this sequence never reaches
    /// here at all, because [`printer::Controller`] takes it out of the stream.
    fn media_copy(&mut self, params: &Params, private: bool) {
        let gate = self.config.printer_ctrl_sequence;
        match (private, arg0(params, 0)) {
            // Print screen. Not a byte stream: upstream renders the grid
            // through the print dialog, so the frontend is asked rather than
            // fed. DECPEX picks the rectangle.
            (false, 0) if gate => {
                let scroll_region = !self.modes.print_ex;
                self.printer_events
                    .push(printer::PrinterEvent::Screen { scroll_region });
            }
            // Printer controller on. It shares auto print's job when there is
            // one, which is why closing is conditional in three places.
            (false, 5) if gate => {
                if !self.auto_print {
                    self.printer_open();
                }
                self.printer.start(self.config.printer_direct);
            }
            // Print the cursor's line, and print it *now* — this is the one
            // arm that opens and closes a job by itself.
            (true, 1) if gate => {
                if !self.auto_print {
                    self.printer_open();
                }
                self.dump_current_line(0x0a);
                if !self.auto_print {
                    self.printer_close();
                }
            }
            // Auto print off — ungated, deliberately.
            (true, 4) => {
                if self.auto_print {
                    self.printer_close();
                    self.auto_print = false;
                }
            }
            (true, 5) if gate && !self.auto_print => {
                self.printer_open();
                self.auto_print = true;
            }
            _ => {}
        }
    }

    /// `OpenPrnFile`. Upstream can fail here and returns NULL, which every
    /// caller then hands straight to `WriteToPrnFile` without a test; there is
    /// nothing to fail at on this side of the seam.
    fn printer_open(&mut self) {
        self.printer_events.push(printer::PrinterEvent::Open);
        self.printer_open = true;
        self.printer_cr_hold = false;
    }

    /// `ClosePrnFile`, minus the `PassThruDelay` timer that starts the printing
    /// — the engine has no clock, so waiting is the frontend's.
    fn printer_close(&mut self) {
        if self.printer_open {
            self.printer_events.push(printer::PrinterEvent::Close);
            self.printer_open = false;
        }
    }

    /// `WriteToPrnFile`, coalesced. Consecutive writes join rather than
    /// producing an event each, since the order that matters is the one against
    /// [`printer::PrinterEvent::Open`] and `Close`.
    fn printer_write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(printer::PrinterEvent::Write(w)) = self.printer_events.last_mut() {
            w.push_str(text);
        } else {
            self.printer_events
                .push(printer::PrinterEvent::Write(text.to_string()));
        }
    }

    /// The printer's branch of `OutputLogUTF32` (`vtterm.c:487`) — the copy of
    /// the *displayed* text that controller mode makes, behind the same EOL
    /// check the macro tap runs.
    fn printer_tap(&mut self, u32: u32) {
        if !self.printer.is_on() {
            return;
        }
        let (eol, chr) = match u32 {
            0x0d => {
                self.printer_cr_hold = true;
                (false, false)
            }
            0x0a if self.printer_cr_hold => {
                self.printer_cr_hold = false;
                (true, false)
            }
            0x0a => (false, true),
            _ => {
                self.printer_cr_hold = false;
                (false, true)
            }
        };
        let mut out = String::new();
        if eol {
            out.push_str("\r\n");
        }
        if chr {
            if let Some(c) = char::from_u32(u32) {
                out.push(c);
            }
        }
        self.printer_write(&out);
    }

    /// `NeedsOutputBufs` (`vtterm.c:512`), which is the log **or** the macro and
    /// pointedly not the printer — so a line break the wrap generated reaches
    /// the printer's copy only when one of the other two happens to be running.
    /// Reproduced rather than tidied: it is the only thing that decides whether
    /// a printed wrapped line is one line or two.
    fn needs_output_bufs(&self) -> bool {
        self.macro_tap.is_some() || self.log_text.is_some()
    }

    /// `BuffDumpCurrentLine` (`buffer.c:2400`) — the cursor's line, trailing
    /// spaces trimmed, followed by `CR` and the terminator when that terminator
    /// is LF, VT or FF.
    ///
    /// **This is where the port stops transcribing, and it is deliberate.**
    /// Upstream dumps `ansi_char`, the cell's code-page form, through four
    /// faults in twenty-eight lines: it writes the *low* byte of a double-byte
    /// character twice where `buffer.c:3597` a hundred lines away writes the
    /// high byte and then the low one; it bounds the write loop by the column
    /// count rather than by the bytes it produced, so those extra bytes are
    /// dropped; a padding cell's zero byte reaches `WriteToPrnFile(0, FALSE)`,
    /// which is the *clear the buffer* form, discarding everything accumulated
    /// for the line so far; and `char bufA[TermWidthMax+1]` is a thousand and
    /// one bytes holding up to two per column, so a wide line of full-width
    /// characters runs about five hundred bytes off the end of a stack buffer
    /// with content the host chose. Reproducing that means reproducing a remote
    /// stack overflow, so this prints what upstream meant to print. For a line
    /// of single-byte characters the two agree exactly, which is every line
    /// that does not contain a full-width glyph.
    fn dump_current_line(&mut self, term: u8) {
        let text = self.line_dump_text(term);
        self.printer_write(&text);
    }

    /// The same dump, composed but not yet written. The wrap needs it a moment
    /// before it happens: upstream's `LineFeed` dumps at its top, while the
    /// cursor is still on the line that filled up, and here the wrap is inside
    /// [`tt_grid::Grid::put`] and reports itself afterwards.
    fn line_dump_text(&self, term: u8) -> String {
        let y = self.grid.cursor.y;
        let line = self.grid.line(y);
        let mut end = line.len();
        while end > 0 && line[end - 1].text[0] == u32::from(b' ') {
            end -= 1;
        }
        let mut out = String::new();
        for cell in &line[..end] {
            if cell.width_class == tt_grid::WIDTH_PAD {
                continue;
            }
            for &cp in cell.text.iter().take_while(|&&cp| cp != 0) {
                if let Some(c) = char::from_u32(cp) {
                    out.push(c);
                }
            }
        }
        if (0x0a..=0x0c).contains(&term) {
            out.push('\r');
            out.push(char::from(term));
        }
        out
    }

    /// `vtterm.c:CarriageReturn` — the move, and the tap upstream does inside
    /// it. Used at the sites that call that function and not at every cursor
    /// motion to column zero: `ESC E` moves with `MoveCursor` and so is silent.
    ///
    /// `log_flag` is upstream's argument of the same name, and the only thing
    /// it decides is whether `ts.EnableContinuedLineCopy` may suppress the
    /// tap: TRUE for a CR that arrived on the wire, FALSE for one the wrap
    /// generated. See [`Config::continued_line_copy`].
    fn carriage_return(&mut self, log_flag: bool) {
        if log_flag || !self.config.continued_line_copy {
            self.tap(0x0d);
        }
        self.grid.carriage_return();
    }

    /// `vtterm.c:725`.
    fn process_cr(&mut self) {
        match self.config.cr_receive {
            CrReceive::Auto => {
                if !self.prev_was_lf || !self.auto_generated_crlf {
                    self.carriage_return(true);
                    // Upstream's `LineFeed(CR, TRUE)`, minus the LNM tail —
                    // see the note on `line_feed`, which this deliberately does
                    // not call.
                    self.tap(0x0a);
                    self.grid.line_feed();
                    self.auto_generated_crlf = true;
                } else {
                    self.auto_generated_crlf = false;
                }
            }
            CrReceive::CrLf => {
                self.carriage_return(true);
                // Upstream returns here and pushes an LF back into the input
                // stream instead (`CommInsert1Byte`), which arrives as an
                // ordinary line feed a moment later. The grid ends up the same
                // and so does the tap.
                self.tap(0x0a);
                self.grid.line_feed();
            }
            _ => self.carriage_return(true),
        }
    }

    /// `vtterm.c:AnswerTerminalType` — Primary DA, and DECID (`ESC Z`), which
    /// upstream answers with the same function.
    ///
    /// It does *not* use `SendCSIstr`. It writes its own introducer, and gates
    /// the 8-bit form on `ts.TerminalID >= IdVT320` as well as on
    /// `Send8BitMode` — so a VT220 told S8C1T answers DSR with `9B` and this
    /// with `ESC [`. Note the test is the terminal *id*, not the VT level, so
    /// DECSCL cannot change it.
    fn primary_da(&mut self) {
        let da = self.config.term_id.primary_da();
        if self.send_8bit && self.config.term_id.ordinal() >= TermId::Vt320.ordinal() {
            self.send(&[0x9b, b'?']);
        } else {
            self.send(b"\x1b[?");
        }
        self.send(da.as_bytes());
        self.send(b"c");
    }

    /// `vtterm.c:LineFeed` — the whole of it, which is what IND and NEL call
    /// as well as the LF byte.
    ///
    /// The tail is the one that surprises: with LNM set, a line feed **returns
    /// the carriage too**, and that is the *receive* side of a mode everything
    /// else treats as being about what the keyboard sends. Upstream does it
    /// after the vertical move, not before (`vtterm.c:706`).
    ///
    /// `byte` is upstream's first argument and its only other use is auto
    /// print: the line is dumped when the feed came from an LF, VT or FF byte
    /// and not when it came from IND or NEL, which pass a zero (`vtterm.c:1153`,
    /// `:1505`). So `ESC D` scrolls a line the printer never sees.
    fn line_feed(&mut self, byte: u8) {
        if self.auto_print && (0x0a..=0x0c).contains(&byte) {
            self.dump_current_line(byte);
        }
        self.tap(0x0a);
        self.grid.line_feed();
        if self.modes.lf_mode {
            // Upstream passes its own `logFlag` down here (`vtterm.c:706`).
            // Every caller of this function is one of the wire's, so it is
            // always TRUE — the wrap does its break inline rather than through
            // `LineFeed`, which is where the FALSE arm lives.
            self.carriage_return(true);
        }
    }

    /// `vtterm.c:747`.
    fn process_lf(&mut self, byte: u8) {
        match self.config.cr_receive {
            CrReceive::Lf => {
                // "the server sends LF alone" — so LF means CR+LF.
                self.carriage_return(true);
                self.line_feed(byte);
            }
            CrReceive::Auto => {
                if !self.prev_was_cr || !self.auto_generated_crlf {
                    self.carriage_return(true);
                    self.line_feed(byte);
                    self.auto_generated_crlf = true;
                } else {
                    self.auto_generated_crlf = false;
                }
            }
            _ => self.line_feed(byte),
        }
    }

    // --- SGR -------------------------------------------------------------

    /// SGR — `ParseSGRParams` applied to the pen, with no mask.
    fn sgr(&mut self, params: &Params) {
        let groups = sgr_groups(params);
        let mut pen = self.grid.pen;
        self.parse_sgr_params(&groups, 0, &mut pen, &mut 0);
        self.grid.pen = pen;
    }

    /// `vtterm.c:ParseSGRParams`, including the parameter-consumption quirk
    /// described on [`ColorFlags`].
    ///
    /// `attr` accumulates onto whatever it starts as — the pen for SGR, a
    /// cleared attribute for DECCARA — and `mask` gathers the bits the
    /// parameters actually *named*, which is what tells DECCARA which of them
    /// to write over the cells it covers and which to leave.
    fn parse_sgr_params(&self, groups: &[Vec<u16>], start: usize, attr: &mut Pen, mask: &mut u32) {
        let mut i = start;
        while i < groups.len() {
            let p = groups[i].first().copied().unwrap_or(0);
            match p {
                0 => {
                    // The protect bit survives SGR 0 — `vtterm.c:2178` ORs it
                    // back in explicitly, so DECSCA outlives an attribute
                    // reset and only another DECSCA clears it.
                    let protect = attr.attrs & ATTR2_PROTECT;
                    *attr = Pen::default();
                    attr.attrs |= protect;
                    // Assignment, not an OR: SGR 0 resets the mask too.
                    *mask = ATTR_SGR_MASK | ATTR2_COLOR_MASK;
                }
                1 => set(attr, mask, ATTR_BOLD, true),
                4 => set(attr, mask, ATTR_UNDER, true),
                5 => set(attr, mask, ATTR_BLINK, true),
                7 => set(attr, mask, ATTR_REVERSE, true),
                22 => set(attr, mask, ATTR_BOLD, false),
                24 => set(attr, mask, ATTR_UNDER, false),
                25 => set(attr, mask, ATTR_BLINK, false),
                27 => set(attr, mask, ATTR_REVERSE, false),
                30..=37 => {
                    set(attr, mask, ATTR2_FORE, true);
                    attr.fg = (p - 30) as u32;
                }
                38 | 48 => {
                    if self.config.color_flags.xterm256 {
                        let full = self.config.color_flags.full_color();
                        if let Some((color, consumed)) =
                            // The **live** table, not the configured one: a host
                            // that repainted the palette with `OSC 4` moves
                            // which index this resolves to, exactly as
                            // `DispFindClosestColor` searching `vt->ANSIColor`
                            // does upstream.
                            extended_color(groups, i, full, &self.colors.ansi)
                        {
                            if p == 38 {
                                set(attr, mask, ATTR2_FORE, true);
                                attr.fg = color;
                            } else {
                                set(attr, mask, ATTR2_BACK, true);
                                attr.bg = color;
                            }
                            i += consumed;
                        }
                    }
                    // With CF_XTERM256 clear, upstream falls straight out of the
                    // switch without touching `i` — so the arguments are parsed
                    // as further SGR parameters. Reproduced deliberately.
                }
                39 => {
                    set(attr, mask, ATTR2_FORE, false);
                    attr.fg = DEFAULT_FG;
                }
                40..=47 => {
                    set(attr, mask, ATTR2_BACK, true);
                    attr.bg = (p - 40) as u32;
                }
                49 => {
                    set(attr, mask, ATTR2_BACK, false);
                    attr.bg = DEFAULT_BG;
                }
                90..=97 if self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_FORE, true);
                    attr.fg = (p - 90 + 8) as u32;
                }
                // Order matters: with aixterm16 off, 100 resets both colours;
                // with it on, 100 is bright-black background and falls to the
                // arm below. That fall-through is upstream's, comment and all.
                100 if !self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_COLOR_MASK, false);
                    attr.fg = DEFAULT_FG;
                    attr.bg = DEFAULT_BG;
                }
                100..=107 if self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_BACK, true);
                    attr.bg = (p - 100 + 8) as u32;
                }
                _ => {}
            }
            i += 1;
        }
    }

    // --- modes -----------------------------------------------------------

    fn set_mode(&mut self, private: bool, params: &Params, on: bool) {
        for group in params.iter() {
            let p = group.first().copied().unwrap_or(0);
            if private {
                match p {
                    6 => {
                        // DECOM. Setting it homes the cursor to the region origin.
                        self.grid.origin_mode = on;
                        let (top, _) = self.grid.scroll_region();
                        self.grid.move_cursor(0, if on { top } else { 0 });
                    }
                    7 => self.grid.autowrap = on,
                    // DECLRMM. Turning it off also throws the margins away
                    // (`vtterm.c:3168`); turning it on does not set them.
                    69 => {
                        self.lr_margin_mode = on;
                        if !on {
                            self.grid.reset_lr_margins();
                        }
                    }
                    // xterm's current DECSDM spelling is the inverse of the
                    // useful state: DECSET 80 fixes graphics to the page;
                    // DECRST 80 restores cursor-relative scrolling.
                    80 => self.modes.sixel_scrolling = !on,
                    47 | 1047 | 1048 | 1049 => self.alt_screen(p, on),

                    // Mouse tracking. Every *set* is gated on the setting;
                    // every *reset* is unconditional, and resets the whole
                    // mode rather than only the one named — so `DECRESET 9`
                    // turns off any-event tracking too (`vtterm.c:3135`).
                    9 | 1000 | 1001 | 1002 | 1003 | 14001 => {
                        if !on {
                            self.mouse.tracking = Tracking::None;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.tracking = match p {
                                9 => Tracking::X10,
                                1000 => Tracking::Vt200,
                                1001 => Tracking::Vt200Hl,
                                1002 => Tracking::BtnEvent,
                                1003 => Tracking::AllEvent,
                                _ => Tracking::NetTerm,
                            };
                        }
                    }
                    1004 => {
                        if !on {
                            self.mouse.focus_report = false;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.focus_report = true;
                        }
                    }
                    1005 | 1006 | 1015 | 1016 => {
                        if !on {
                            self.mouse.encoding = Encoding::Normal;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.encoding = match p {
                                1005 => Encoding::Utf8,
                                1006 => Encoding::Sgr,
                                1015 => Encoding::Urxvt,
                                _ => Encoding::SgrPixels,
                            };
                        }
                    }
                    // The rest are one flag each, and are here rather than
                    // dropped because the frontend reads several of them and
                    // DECRQM reports all of them.
                    1 => self.modes.appli_cursor = on,
                    3 => self.dec_colm(on),
                    5 => self.modes.reverse_video = on,
                    8 => self.modes.auto_repeat = on,
                    // `DECSET 12` and DECSCUSR are both gated on a setting
                    // that ships *off*, so by default this does nothing.
                    12 => {
                        if self.config.cursor_ctrl_sequence {
                            self.config.nonblinking_cursor = !on;
                        }
                    }
                    19 => self.modes.print_ex = on,
                    25 => self.modes.caret = on,
                    66 => self.modes.appli_key = on,
                    67 => self.modes.bs_key_is_bs = on,
                    2004 => self.modes.bracketed_paste = on,
                    7727 => self.modes.appli_escape = u16::from(on),
                    7786 => {
                        if !on {
                            self.modes.wheel_to_cursor = false;
                        } else if self.config.translate_wheel_to_cursor {
                            self.modes.wheel_to_cursor = true;
                        }
                    }
                    8200 => self.modes.clear_then_home = on,
                    14002..=14004 => {
                        self.modes.appli_escape = if on { p - 14000 } else { 0 };
                    }
                    _ => {}
                }
            } else {
                match p {
                    // KAM. `SM 2` *locks* the keyboard, so the sense inverts.
                    2 => self.modes.keyb_enabled = !on,
                    4 => self.grid.insert_mode = on,
                    // SRM inverts too: `SM 12` means "send/receive", which is
                    // local echo off.
                    12 => self.modes.local_echo = !on,
                    20 => {
                        self.modes.lf_mode = on;
                        self.modes.cr_send = if on { CrSend::CrLf } else { CrSend::Cr };
                    }
                    _ => {}
                }
            }
        }
    }

    /// DECCOLM — `vtterm.c:CSQChangeColumnMode`. It is a resize, it throws the
    /// left/right margins away, and because `TF_CLEARONRESIZE` ships off it
    /// also clears the screen and homes the cursor by hand.
    ///
    /// The clear is skipped when the flag is *on*, and upstream says why in a
    /// comment: `ChangeTerminalSize` has already scrolled the page out, and
    /// doing it twice would put a second blank page in the history.
    fn dec_colm(&mut self, wide: bool) {
        let rows = self.grid.rows();
        self.grid.resize(if wide { 132 } else { 80 }, rows);
        self.lr_margin_mode = false;
        self.grid.reset_lr_margins();
        if !self.config.clear_on_resize {
            self.grid.move_cursor(0, 0);
            self.grid.clear_screen();
        }
    }

    /// `vtterm.c:2970` / `:3030` / `:3144` / `:3194`.
    ///
    /// The `!alt`/`alt` guards are upstream's and they matter: a second
    /// `ESC [ ? 1049 h` while already on the alternate screen must not stash
    /// the alternate screen over the saved main one. Programs that re-arm the
    /// mode on redraw do exactly that.
    fn alt_screen(&mut self, mode: u16, on: bool) {
        if !self.config.alt_screen_enabled {
            return;
        }
        match (mode, on) {
            // 1048 is the cursor half alone, and shares DECSC's slot.
            (1048, true) => self.save_cursor(),
            (1048, false) => self.restore_cursor(),

            (47 | 1047, true) if !self.alt_screen => {
                self.sixel_images.retain(|image| !image.alternate());
                self.grid.save_screen();
                self.alt_screen = true;
            }
            (47 | 1047, false) if self.alt_screen => {
                self.sixel_images.retain(|image| !image.alternate());
                self.grid.restore_screen();
                self.alt_screen = false;
            }
            (1049, true) if !self.alt_screen => {
                self.sixel_images.retain(|image| !image.alternate());
                self.save_cursor();
                self.grid.save_screen();
                self.grid.clear_screen();
                self.alt_screen = true;
            }
            (1049, false) if self.alt_screen => {
                self.sixel_images.retain(|image| !image.alternate());
                self.grid.clear_screen();
                self.grid.restore_screen();
                self.alt_screen = false;
                self.restore_cursor();
            }
            _ => {}
        }
    }

    /// The `top, left, bottom, right` quadruple every rectangular operation
    /// opens with: 1-based and inclusive on the wire, 0-based here. `None`
    /// means the rectangle is inside out, which upstream treats as "do
    /// nothing" rather than as an empty region.
    ///
    /// Note the two clamps differ. The top-left corner uses `CheckParamVal`,
    /// where an omitted parameter means 1; the bottom-right uses
    /// `CheckParamValMax`, where it means the far edge.
    fn area_rect(&self, params: &Params, first: usize) -> Option<Rect> {
        let rows = self.grid.rows() as u16;
        let cols = self.grid.cols() as u16;
        let mut top = check_param_val(arg0(params, first), rows);
        let left = check_param_val(arg0(params, first + 1), cols);
        let mut bottom = check_param_val_max(arg0(params, first + 2), rows);
        let right = check_param_val_max(arg0(params, first + 3), cols);
        if top > bottom || left > right {
            return None;
        }
        if self.grid.origin_mode {
            let (region_top, region_bottom) = self.grid.scroll_region();
            top = origin_shift(top, region_top, region_bottom);
            bottom = origin_shift(bottom, region_top, region_bottom);
        }
        Some(Rect {
            x0: left as usize - 1,
            y0: top as usize - 1,
            x1: right as usize - 1,
            y1: bottom as usize - 1,
        })
    }

    /// DECRQCRA — `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`, answered
    /// `DCS Pid ! ~ HHHH ST`.
    ///
    /// **Nothing upstream corresponds to this**; see [`Config::decrqcra`] for
    /// why it exists and why it is off by default. Three decisions it makes on
    /// its own, none of which upstream can arbitrate:
    ///
    /// - **The sum is over characters only, not attributes.** DEC STD 070 folds
    ///   the attribute bits in and xterm has resources to choose; `esctest`
    ///   asserts that a single cell's checksum *equals its character code*, so
    ///   anything added here would make every screen assertion in the suite
    ///   fail. A combining mark adds its own codepoint, and the padding half of
    ///   a wide character holds no codepoint and so adds nothing.
    /// - **It is the plain sum, not the two's complement.** That is xterm from
    ///   patch #279 onward; the harness passes `--xterm-checksum` high enough
    ///   to say so.
    /// - **An erased cell counts as a space**, because that is what an erase
    ///   leaves in the grid — Tera Term has no "empty versus blank"
    ///   distinction to preserve, which is xterm's behaviour from patch #334.
    ///
    /// An inverted rectangle answers zero rather than staying silent. The
    /// rectangular *operations* return without acting on one, which is
    /// upstream's behaviour and fine for something with no reply; doing the
    /// same to a request would leave the far end waiting on an answer that
    /// never comes.
    fn decrqcra(&mut self, params: &Params) {
        if !self.config.decrqcra {
            return;
        }
        let id = arg0(params, 0);
        let sum = match self.area_rect(params, 2) {
            Some(area) => {
                let mut sum: u16 = 0;
                for y in area.y0..=area.y1 {
                    for cell in &self.grid.line(y)[area.x0..=area.x1] {
                        for cp in cell.codepoints() {
                            sum = sum.wrapping_add(cp as u16);
                        }
                    }
                }
                sum
            }
            None => 0,
        };
        self.send_dcs(&format!("{id}!~{sum:04X}"));
    }

    /// `vtterm.c:SoftReset` — DECSTR (`CSI ! p`), and the first half of
    /// DECSCL.
    fn soft_reset(&mut self) {
        self.grid.soft_reset();
        self.charset.reset();
        self.saved_charset[usize::from(self.alt_screen)] = Some(self.charset.save());
        self.modes.soft_reset(&self.config);
        self.sixel_images.clear();
    }

    /// `vtterm.c:SendDCSstr` — `ESC P … ESC \`, or the 8-bit `DCS … ST` when
    /// the terminal has been told it may.
    /// `vtterm.c:SendOSCstr` — `ESC ] … ST`, terminated with ST rather than
    /// BEL because that is what the title reports pass.
    fn send_osc(&mut self, body: &str) {
        self.send_osc_terminated(body, false);
    }

    /// The same, for the one reply that mirrors the request's terminator rather
    /// than always answering with ST.
    ///
    /// `SendOSCstr` takes a `TermChar`, and only the colour replies pass the
    /// byte the request arrived with (`vtterm.c:4912`); the title reports at
    /// `:2699` and `:2740` pass a literal `ST`. So a `OSC 4;1;? BEL` is
    /// answered with BEL and the same request ended with ST is answered with
    /// ST — and the BEL form uses the 7-bit `ESC ]` introducer even on a
    /// terminal sending 8-bit controls, because that arm of `SendOSCstrW` does
    /// not consult `Send8BitMode` at all.
    fn send_osc_terminated(&mut self, body: &str, bell: bool) {
        if bell {
            self.send(b"\x1b]");
            self.send(body.as_bytes());
            self.send(b"\x07");
        } else if self.send_8bit {
            self.send(&[0x9d]);
            self.send(body.as_bytes());
            self.send(&[0x9c]);
        } else {
            self.send(b"\x1b]");
            self.send(body.as_bytes());
            self.send(b"\x1b\\");
        }
    }

    fn send_dcs(&mut self, body: &str) {
        if self.send_8bit {
            self.send(&[0x90]);
            self.send(body.as_bytes());
            self.send(&[0x9c]);
        } else {
            self.send(b"\x1bP");
            self.send(body.as_bytes());
            self.send(b"\x1b\\");
        }
    }

    /// DECRQSS — `vtterm.c:RequestStatusString`. The request names a setting
    /// by its own final bytes; the reply is `1$r<value><those same bytes>`,
    /// or a bare `0$r` for anything not recognised.
    fn decrqss(&mut self, req: &[u8]) {
        let body = match req {
            [b' ', b'q', ..] => {
                // DECSCUSR. Non-blinking is the odd numbering's +1.
                let n = self.config.cursor_shape + u16::from(self.config.nonblinking_cursor);
                Some(format!("1$r{n} q"))
            }
            [b'"', b'p', ..] => {
                let eight = if self.vt_level > 1 && self.send_8bit {
                    0
                } else {
                    1
                };
                Some(format!("1$r6{};{}\"p", self.vt_level, eight))
            }
            [b'"', b'q', ..] => {
                let on = u8::from(self.grid.pen.attrs & ATTR2_PROTECT != 0);
                Some(format!("1$r{on}\"q"))
            }
            [b'*', b'x', ..] => Some(format!("1$r{}*x", if self.rect_mode { 2 } else { 0 })),
            // These three insist on being the whole request, unlike the four
            // above, which only look at their first two bytes.
            [b'm'] => Some(self.sgr_report()),
            [b'r'] => {
                let (top, bottom) = self.grid.scroll_region();
                Some(format!("1$r{};{}r", top + 1, bottom + 1))
            }
            [b's'] => {
                let (left, right) = self.grid.margins();
                Some(format!("1$r{};{}s", left + 1, right + 1))
            }
            _ => None,
        };
        let mut body = body.unwrap_or_else(|| "0$r".to_string());
        // `TF_INVALIDDECRPSS` (`vtterm.c:4400`) — flip the leading digit, so a
        // request the terminal understood is answered as one it did not and
        // the other way round. It flips the *character*, not the meaning: an
        // "invalid" reply still carries the value it was about to send, which
        // is what makes it a test of the host's parser rather than of its
        // arithmetic.
        if self.config.invalid_decrqss {
            let flipped = if body.starts_with('0') { "1" } else { "0" };
            body = flipped.to_string() + body.get(1..).unwrap_or("");
        }
        self.send_dcs(&body);
    }

    /// The SGR half of DECRQSS, which rebuilds a parameter string from the
    /// pen. Every colour branch is gated on a different `ColorFlag`, so the
    /// same pen reports differently depending on which of them are on.
    fn sgr_report(&self) -> String {
        let pen = self.grid.pen;
        let cf = self.config.color_flags;
        let mut out = String::from("1$r0");
        for (bit, code) in [
            (ATTR_BOLD, "1"),
            (ATTR_UNDER, "4"),
            (ATTR_BLINK, "5"),
            (ATTR_REVERSE, "7"),
        ] {
            if pen.attrs & bit != 0 {
                out.push(';');
                out.push_str(code);
            }
        }

        let colour = |set: bool, mut c: u32, brighten: bool, low: u32, high: u32, ext: u32| {
            if !set || !cf.ansi_color {
                return String::new();
            }
            if c <= 7 && brighten && cf.pc_bold16 {
                c += 8;
            }
            if c <= 7 {
                format!(";{}", low + c)
            } else if c <= 15 {
                if cf.aixterm16 {
                    format!(";{}", high + c - 8)
                } else if cf.xterm256 {
                    format!(";{ext};5;{c}")
                } else if cf.pc_bold16 {
                    format!(";{}", low + c - 8)
                } else {
                    String::new()
                }
            } else if cf.xterm256 {
                format!(";{ext};5;{c}")
            } else {
                String::new()
            }
        };

        // Bold brightens the foreground, blink the background — upstream's
        // pairing, and it is not a typo.
        out.push_str(&colour(
            pen.attrs & ATTR2_FORE != 0,
            pen.fg,
            pen.attrs & ATTR_BOLD != 0,
            30,
            90,
            38,
        ));
        out.push_str(&colour(
            pen.attrs & ATTR2_BACK != 0,
            pen.bg,
            pen.attrs & ATTR_BLINK != 0,
            40,
            100,
            48,
        ));
        out.push('m');
        out
    }

    /// `vtterm.c:CSSunSequence` — XTWINOPS, `CSI Ps ; ... t`.
    ///
    /// The operations that move a window go on [`State::window_requests`] for
    /// the frontend; the ones that describe it are answered from the snapshot
    /// the frontend last pushed. See [`window::WindowMetrics`] for why it is a
    /// snapshot.
    ///
    /// There is no `case 12`, no `case 17` and nothing above 23 — so `CSI 24 t`,
    /// which is DECSLPP everywhere else, does nothing at all here.
    fn window_op(&mut self, params: &Params) {
        let (cols, rows) = (self.grid.cols(), self.grid.rows());
        let px = |v: u16| i32::from(v);
        match arg0(params, 0) {
            1 if self.config.window_change => self.window_request(WindowRequest::Deiconify),
            2 if self.config.window_change => self.window_request(WindowRequest::Iconify),
            3 if self.config.window_change => {
                let (x, y) = (px(arg0(params, 1)), px(arg0(params, 2)));
                self.window_request(WindowRequest::Move(x, y));
            }
            // `CSI 4 ; height ; width t`. Height first, and a zero or absent
            // axis keeps what the window already has (`vtdisp.c:3652`).
            4 if self.config.window_change => {
                let (height, width) = (px(arg0(params, 1)), px(arg0(params, 2)));
                self.window_request(WindowRequest::ResizePixels { width, height });
            }
            5 if self.config.window_change => self.window_request(WindowRequest::Raise),
            6 if self.config.window_change => self.window_request(WindowRequest::Lower),
            7 if self.config.window_change => self.window_request(WindowRequest::Refresh),
            // Set terminal size. A height or width of 0 or 1 is refused and
            // replaced by the 24x80 default rather than honoured.
            8 if self.config.window_change => {
                let mut want_rows = arg0(params, 1);
                let mut want_cols = arg0(params, 2);
                if want_rows <= 1 {
                    want_rows = 24;
                }
                if want_cols <= 1 {
                    want_cols = 80;
                }
                self.grid.resize(want_cols as usize, want_rows as usize);
                self.terminal_resized = true;
            }
            // Maximise and restore. Case 9 has arms for 0 and 1 and no toggle;
            // case 10 has all three, and its 1 is *maximise* rather than full
            // screen — see [`WindowRequest::Maximize`].
            9 | 10 if self.config.window_change => {
                let req = match (arg0(params, 0), arg0(params, 1)) {
                    (_, 0) => WindowRequest::Unmaximize,
                    (_, 1) => WindowRequest::Maximize,
                    (10, 2) => WindowRequest::ToggleMaximize,
                    _ => return,
                };
                self.window_request(req);
            }
            // Report whether the window is iconified. No sub-parameter, and no
            // `RequiredParams` — the only arm of the switch with neither.
            11 if self.config.window_report => {
                let body = format!("{}t", if self.window.iconified { 2 } else { 1 });
                self.send_csi(&body);
            }
            // Window position, `x` then `y` — the one report in the family
            // that is not height-first. Sub-parameter 2 is the text area's
            // origin, 0 and 1 the frame's, and anything else answers nothing.
            13 if self.config.window_report => {
                let (x, y) = match arg0(params, 1) {
                    0 | 1 => self.window.pos,
                    2 => self.window.client_pos,
                    _ => return,
                };
                let body = format!("3;{x};{y}t");
                self.send_csi(&body);
            }
            // Window size in pixels, height then width. The sub-parameters are
            // the other way round from `CSI 13 t`: 0 and 1 are the *text area*
            // and 2 is the frame, which is xterm's meaning and upstream's.
            14 if self.config.window_report => {
                let (w, h) = match arg0(params, 1) {
                    0 | 1 => self.window.text_area(cols, rows),
                    2 => self.window.frame(cols, rows),
                    _ => return,
                };
                let body = format!("4;{h};{w}t");
                self.send_csi(&body);
            }
            15 if self.config.window_report => {
                let (w, h) = self.window.screen;
                let body = format!("5;{h};{w}t");
                self.send_csi(&body);
            }
            16 if self.config.window_report => {
                let (w, h) = self.window.cell;
                let body = format!("6;{h};{w}t");
                self.send_csi(&body);
            }
            // Report terminal size, in the same spelling that sets it.
            18 if self.config.window_report => {
                let body = format!("8;{};{}t", self.grid.rows(), self.grid.cols());
                self.send_csi(&body);
            }
            19 if self.config.window_report => {
                let (w, h) = self.window.screen_cells(cols, rows);
                let body = format!("9;{h};{w}t");
                self.send_csi(&body);
            }
            // Report icon label and window title. Gated on the *title* setting,
            // not on `WF_WINDOWREPORT` — see `Config::title_report`.
            20 => self.report_title('L'),
            21 => self.report_title('l'),
            // Push and pop the title — `vtterm.c:2751`. The parameter names
            // which of icon and window title to stack, and all three values
            // do the same thing upstream because there is only one title.
            22 if self.config.accept_title_change != TitleChange::Off => {
                if matches!(arg0(params, 1), 0..=2) {
                    let title = self.title.clone();
                    self.title_stack.push(title);
                }
            }
            23 if self.config.accept_title_change != TitleChange::Off => {
                if matches!(arg0(params, 1), 0..=2) {
                    if let Some(title) = self.title_stack.pop() {
                        self.title = title;
                    }
                }
            }
            _ => {}
        }
    }

    /// Queue an XTWINOPS action for whoever owns a window.
    ///
    /// Bounded, because a host is allowed to send `CSI 5 t` forever and a
    /// frontend is not obliged to drain: an unbounded queue would grow without
    /// limit on a session nobody is watching. Dropping the *newest* is right
    /// here where the macro ring drops the oldest — the ring holds a
    /// conversation and this holds instructions, and the hundredth "raise the
    /// window" is worth less than the first.
    fn window_request(&mut self, req: window::WindowRequest) {
        const MAX: usize = 64;
        if self.window_requests.len() < MAX {
            self.window_requests.push(req);
        }
    }

    /// `CSI 20 t` and `CSI 21 t`, whose answer is an OSC string led by `lead` —
    /// `L` for the icon label and `l` for the window title (`vtterm.c:2668`).
    ///
    /// The `Accept` arm is not [`TitleChange::combine`]. Upstream writes the
    /// two chains separately and they disagree in one place: the window falls
    /// back to the file's title whenever the host's is empty, while the report
    /// only does that under `Overwrite`, so `ahead` with no host title answers
    /// with a **leading space** (`vtterm.c:2683`). Reproduced rather than
    /// tidied — the reply goes on somebody's wire.
    fn report_title(&mut self, lead: char) {
        let body = match self.config.title_report {
            TitleReport::Ignore => return,
            TitleReport::Empty => String::new(),
            TitleReport::Accept => {
                let file = &self.config.title;
                let remote = &self.title;
                match self.config.accept_title_change {
                    TitleChange::Off => file.clone(),
                    TitleChange::Ahead => format!("{remote} {file}"),
                    TitleChange::Last => format!("{file} {remote}"),
                    TitleChange::Overwrite if remote.is_empty() => file.clone(),
                    TitleChange::Overwrite => remote.clone(),
                }
            }
        };
        self.send_osc(&format!("{lead}{body}"));
    }

    /// XTSMGRAPHICS — xterm's capability query for palette and sixel raster
    /// limits. Sterna's limits are fixed, so reset is a successful no-op and
    /// set fails. The current geometry follows xterm in being the smaller of
    /// the drawable text area and the decoder's allocation ceiling.
    fn xtsmgraphics(&mut self, params: &Params) {
        let item = arg0(params, 0);
        let action = arg0(params, 1);
        let response = match (item, action) {
            // Number of sixel colour registers.
            (1, 1 | 2 | 4) => "?1;0;256S".to_owned(),
            // Current sixel geometry in pixels.
            (2, 1 | 2) => {
                let cell_width = usize::try_from(self.window.cell.0.max(1)).unwrap_or(1);
                let cell_height = usize::try_from(self.window.cell.1.max(1)).unwrap_or(1);
                let width = self
                    .grid
                    .cols()
                    .saturating_mul(cell_width)
                    .min(sixel::MAX_WIDTH);
                let height = self
                    .grid
                    .rows()
                    .saturating_mul(cell_height)
                    .min(sixel::MAX_HEIGHT);
                format!("?2;0;{width};{height}S")
            }
            // Maximum raster the bounded decoder will accept.
            (2, 4) => format!("?2;0;{};{}S", sixel::MAX_WIDTH, sixel::MAX_HEIGHT),
            // ReGIS is a known item which this terminal does not implement.
            (3, 1..=4) => "?3;3;0S".to_owned(),
            // The two supported attributes are read-only.
            (1 | 2, 3) => format!("?{item};3;0S"),
            (1..=3, _) => format!("?{item};2;0S"),
            _ => format!("?{item};1;0S"),
        };
        self.send_csi(&response);
    }

    /// DECRQM — `vtterm.c:CSDolRequestMode`. Answers `CSI [?]Ps;Ps $ y`, where
    /// the second parameter is 1 set, 2 reset, 3 permanently set, 4 permanently
    /// reset, and 0 "not recognised".
    ///
    /// The two halves differ in their fallback and that is upstream's, not a
    /// slip: an unknown **ANSI** mode answers 4, an unknown **DEC private**
    /// mode answers 0.
    fn decrqm(&mut self, mode: u16, private: bool) {
        let onoff = |b: bool| if b { 1 } else { 2 };
        let m = &self.modes;

        let resp: u16 = if private {
            // `!ts.MouseEventTracking` makes every mouse mode permanently
            // reset rather than merely off.
            let mouse = |live: bool| {
                if !self.config.mouse_tracking_enabled {
                    4
                } else {
                    onoff(live)
                }
            };
            // Without TF_ALTSCR the alternate screen is permanently reset —
            // except 1048, the cursor half, which is permanently *set*.
            let alt = |live: bool| {
                if !self.config.alt_screen_enabled {
                    4
                } else {
                    onoff(live)
                }
            };
            // The three cursor modes add two to their answer when the setting
            // that gates them is off, turning "set/reset" into "permanently
            // set/permanently reset".
            let cursor = |v: u16| {
                v + if self.config.cursor_ctrl_sequence {
                    0
                } else {
                    2
                }
            };

            match mode {
                1 => onoff(m.appli_cursor),
                3 => onoff(self.grid.cols() == 132),
                5 => onoff(m.reverse_video),
                6 => onoff(self.grid.origin_mode),
                7 => onoff(self.grid.autowrap),
                8 => onoff(m.auto_repeat),
                9 => mouse(self.mouse.tracking == Tracking::X10),
                12 => cursor(onoff(!self.config.nonblinking_cursor)),
                19 => onoff(m.print_ex),
                25 => onoff(m.caret),
                // DECTEK is permanently reset: the oracle has no Tek window
                // and neither will we.
                38 => 4,
                47 => alt(self.alt_screen),
                // DECKKDM answers 0 on a terminal that is not Japanese, which
                // this one never is.
                59 => 0,
                66 => onoff(m.appli_key),
                67 => onoff(m.bs_key_is_bs),
                69 => onoff(self.lr_margin_mode),
                80 => onoff(!m.sixel_scrolling),
                1000 => mouse(self.mouse.tracking == Tracking::Vt200),
                // Highlight tracking is `#if 0`'d out upstream and always
                // answers permanently reset, even though setting it works.
                1001 => 4,
                1002 => mouse(self.mouse.tracking == Tracking::BtnEvent),
                1003 => mouse(self.mouse.tracking == Tracking::AllEvent),
                1004 => mouse(self.mouse.focus_report),
                1005 => mouse(self.mouse.encoding == Encoding::Utf8),
                1006 => mouse(self.mouse.encoding == Encoding::Sgr),
                1015 => mouse(self.mouse.encoding == Encoding::Urxvt),
                1016 => mouse(self.mouse.encoding == Encoding::SgrPixels),
                1047 => alt(self.alt_screen),
                1048 => {
                    if self.config.alt_screen_enabled {
                        1
                    } else {
                        4
                    }
                }
                1049 => alt(self.alt_screen),
                2004 => onoff(m.bracketed_paste),
                7727 => onoff(m.appli_escape == 1),
                7786 => {
                    if !self.config.translate_wheel_to_cursor {
                        4
                    } else {
                        onoff(m.wheel_to_cursor)
                    }
                }
                8200 => onoff(m.clear_then_home),
                14001 => mouse(self.mouse.tracking == Tracking::NetTerm),
                14002..=14004 => onoff(m.appli_escape == mode - 14000),
                _ => 0,
            }
        } else {
            let cursor = |v: u16| {
                v + if self.config.cursor_ctrl_sequence {
                    0
                } else {
                    2
                }
            };
            match mode {
                2 => onoff(!m.keyb_enabled),
                4 => onoff(self.grid.insert_mode),
                12 => onoff(!m.local_echo),
                20 => onoff(m.lf_mode),
                33 => cursor(onoff(self.config.nonblinking_cursor)),
                34 => cursor(onoff(self.config.cursor_shape == 3)),
                _ => 4,
            }
        };

        let body = format!("{}{};{}$y", if private { "?" } else { "" }, mode, resp);
        self.send_csi(&body);
    }

    /// `vtterm.c:CSQuote` — DEC's locator, which is a second mouse protocol
    /// sharing one state variable with the xterm family.
    fn csi_quote(&mut self, params: &Params, action: char) {
        match action {
            // DECEFR — enable filter rectangle. A zero edge means "the
            // locator's current position", so `CSI ' w` alone arms a
            // one-cell rectangle around wherever it is.
            'w' => {
                if self.mouse.tracking != Tracking::DecLocator {
                    return;
                }
                let (last_x, last_y) = self.mouse.last;
                let (x, y) = if self.mouse.locator_flags & mouse::PIXEL != 0 {
                    (last_x + 1, last_y + 1)
                } else {
                    let (cx, cy) = self.win_to_screen(last_x, last_y);
                    (cx + 1, cy + 1)
                };
                let edge = |n, dflt| match arg0(params, n) {
                    0 => dflt,
                    v => v as i32,
                };
                let (mut top, mut left) = (edge(0, y), edge(1, x));
                let (mut bottom, mut right) = (edge(2, y), edge(3, x));
                if top > bottom {
                    std::mem::swap(&mut top, &mut bottom);
                }
                if left > right {
                    std::mem::swap(&mut left, &mut right);
                }
                self.mouse.filter = (top, left, bottom, right);
                self.mouse.locator_flags |= mouse::FILTERED;
                self.locator_report(MouseEvent::Move, 0);
            }
            // DECELR — enable locator reporting. The second parameter picks
            // cell or pixel coordinates and is re-read on every call, so a
            // bare `CSI 0'z` also drops back to cells.
            'z' => {
                match arg0(params, 0) {
                    0 if self.mouse.tracking == Tracking::DecLocator => {
                        self.mouse.tracking = Tracking::None;
                    }
                    v @ (1 | 2) if self.config.mouse_tracking_enabled => {
                        self.mouse.tracking = Tracking::DecLocator;
                        if v == 1 {
                            self.mouse.locator_flags &= !mouse::ONE_SHOT;
                        } else {
                            self.mouse.locator_flags |= mouse::ONE_SHOT;
                        }
                    }
                    _ => {}
                }
                if params.len() > 1 && arg0(params, 1) == 1 {
                    self.mouse.locator_flags |= mouse::PIXEL;
                } else {
                    self.mouse.locator_flags &= !mouse::PIXEL;
                }
            }
            // DECSLE — which locator events to report.
            '{' => {
                for group in params.iter() {
                    match group.first().copied().unwrap_or(0) {
                        0 => {
                            self.mouse.locator_flags &=
                                !(mouse::BUTTON_UP | mouse::BUTTON_DOWN | mouse::FILTERED)
                        }
                        1 => self.mouse.locator_flags |= mouse::BUTTON_DOWN,
                        2 => self.mouse.locator_flags &= !mouse::BUTTON_DOWN,
                        3 => self.mouse.locator_flags |= mouse::BUTTON_UP,
                        4 => self.mouse.locator_flags &= !mouse::BUTTON_UP,
                        _ => {}
                    }
                }
            }
            // DECRQLP — where is the locator? Answers even when no locator is
            // enabled, with the `0&w` that says so.
            '|' => {
                self.locator_report(MouseEvent::CurStat, 0);
            }
            _ => {}
        }
    }

    /// `vtterm.c:CSDol` — the `$`-intermediate family, which is every
    /// rectangular area operation.
    fn csi_dollar(&mut self, params: &Params, action: char) {
        match action {
            // DECCARA (change) and DECRARA (toggle). Both take the SGR
            // parameters that follow the rectangle, from the fifth onward.
            'r' | 't' => {
                let Some(area) = self.area_rect(params, 0) else {
                    return;
                };
                let groups = sgr_groups(params);
                let mut attr = Pen {
                    fg: DEFAULT_FG,
                    bg: DEFAULT_BG,
                    attrs: 0,
                };
                let mut mask = 0u32;
                self.parse_sgr_params(&groups, 4, &mut attr, &mut mask);
                let keep = ATTR_SGR_MASK | ATTR2_COLOR_MASK;
                attr.attrs &= keep;
                let rect = self.rect_mode;
                if action == 'r' {
                    self.grid
                        .change_attr_area(rect, area, attr, Some(mask & keep));
                } else {
                    self.grid.change_attr_area(rect, area, attr, None);
                }
            }
            // DECCRA. Eight parameters, of which the two page numbers are
            // parsed and ignored — there is only ever one page.
            'v' => {
                let rows = self.grid.rows() as u16;
                let cols = self.grid.cols() as u16;
                let mut sy0 = check_param_val(arg0(params, 0), rows);
                let sx0 = check_param_val(arg0(params, 1), cols);
                let mut sy1 = check_param_val_max(arg0(params, 2), rows);
                let sx1 = check_param_val_max(arg0(params, 3), cols);
                let mut dy = check_param_val(arg0(params, 5), rows);
                let dx = check_param_val(arg0(params, 6), cols);
                if sy0 > sy1 || sx0 > sx1 {
                    return;
                }
                if self.grid.origin_mode {
                    let (top, bottom) = self.grid.scroll_region();
                    sy0 = origin_shift(sy0, top, bottom);
                    sy1 = origin_shift(sy1, top, bottom);
                    dy = origin_shift(dy, top, bottom);
                    // Trim the source rather than the destination, so the copy
                    // stops at the bottom margin instead of crossing it.
                    if (dy + sy1 - sy0) as usize > bottom {
                        sy1 = sy0 + bottom as u16 - dy + 1;
                    }
                }
                let src = Rect {
                    x0: sx0 as usize - 1,
                    y0: sy0 as usize - 1,
                    x1: sx1 as usize - 1,
                    y1: sy1 as usize - 1,
                };
                self.grid.copy_box(src, dx as usize - 1, dy as usize - 1);
            }
            // DECFRA. The fill character comes first and is rejected outright
            // if it is a control code — including the C1 range, which is why
            // the test has a hole in the middle.
            'x' => {
                let ch = arg0(params, 0);
                if !((32..=127).contains(&ch) || (160..=255).contains(&ch)) {
                    return;
                }
                let Some(area) = self.area_rect(params, 1) else {
                    return;
                };
                self.grid.fill_box(ch as u32, area);
            }
            // DECERA and DECSERA.
            'z' | '{' => {
                let Some(area) = self.area_rect(params, 0) else {
                    return;
                };
                if action == 'z' {
                    self.grid.erase_box(area);
                } else {
                    self.grid.selective_erase_box(area);
                }
            }
            _ => {}
        }
    }

    /// Every final byte that carries no intermediate, or ignores the one it
    /// has because upstream does.
    fn csi_plain(
        &mut self,
        params: &Params,
        private: bool,
        gt: bool,
        inter: Option<u8>,
        action: char,
    ) {
        // DECSTBM and friends take no intermediate; anything that arrives with
        // one is a sequence we have not ported, and running it as its
        // no-intermediate namesake would be worse than dropping it.
        //
        // The three exceptions are not intermediates at all — `?`, `>` and `=`
        // are *private markers*, in the parameter byte range (0x30..=0x3F)
        // rather than the intermediate one, and `vte` reports them here
        // because it has nowhere else to put them. Leaving `=` out of this
        // list is what kept the tertiary DA unreachable while its arm was
        // written and looked right.
        if matches!(inter, Some(b) if b != b'?' && b != b'>' && b != b'=') {
            return;
        }
        match action {
            '@' => self.grid.insert_chars(arg(params, 0, 1) as usize),
            'A' => self.grid.move_up(arg(params, 0, 1) as usize, true),
            'B' => self.grid.move_down(arg(params, 0, 1) as usize, true),
            'C' => self.grid.move_right(arg(params, 0, 1) as usize, true),
            'D' => self.grid.move_left(arg(params, 0, 1) as usize, true),
            // CNL and CPL move to the *left margin*, and do it before the
            // vertical move rather than after — `vtterm.c:1691`.
            'E' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_down(arg(params, 0, 1) as usize, true);
            }
            'F' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_up(arg(params, 0, 1) as usize, true);
            }
            'G' | '`' => {
                let x = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_to_column(x);
            }
            'H' | 'f' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                let x = arg(params, 1, 1).saturating_sub(1) as usize;
                self.grid.move_cursor_abs(x, y);
            }
            'I' => self.grid.forward_tab(arg(params, 0, 1) as usize),
            // ED, and DECSED under `?`. Mode 3 is not an erase at all: it is
            // `ClearBuffer`, gated on TF_REMOTECLEARSBUFF, and it homes the
            // cursor and resets the scroll region on the way out.
            'J' => match (private, arg0(params, 0)) {
                (_, 3) => {
                    if self.config.remote_clears_buffer {
                        self.grid.clear_buffer();
                    }
                }
                (true, 0) => self.grid.selective_erase_to_end(),
                (true, 1) => self.grid.selective_erase_to_cursor(),
                (true, 2) => {
                    self.grid.selective_erase_to_cursor();
                    self.grid.selective_erase_to_end();
                }
                (true, _) => {}
                // `ESC [ H ESC [ J` is what a good many programs send in
                // place of `ESC [ 2 J`, and upstream promotes it to one so the
                // screen goes into the history rather than being erased out of
                // it (`vtterm.c:1728`). Gated on the setting, and on the
                // cursor being at the *screen's* origin rather than the
                // region's — `CursorX == 0 && CursorY == 0`, tested without
                // reference to origin mode.
                (false, 0)
                    if self.config.home_erase_clears_screen
                        && self.grid.cursor.x == 0
                        && self.grid.cursor.y == 0 =>
                {
                    self.grid.clear_screen();
                }
                (false, mode) => {
                    self.grid.erase_display(mode);
                    // `DECSET 8200` homes the cursor after `ED 2` — to the
                    // region origin, or the screen origin under origin mode
                    // (`vtterm.c:1749`).
                    if mode == 2 && self.modes.clear_then_home {
                        if self.grid.origin_mode {
                            self.grid.move_cursor(0, 0);
                        } else {
                            let (top, _) = self.grid.scroll_region();
                            let (left, _) = self.grid.margins();
                            self.grid.move_cursor(left, top);
                        }
                    }
                }
            },
            // EL, and DECSEL under `?`.
            'K' => {
                let mode = arg0(params, 0);
                if private {
                    self.grid.selective_erase_line(mode);
                } else {
                    self.grid.erase_line(mode);
                }
            }
            'L' => self.grid.insert_lines(arg(params, 0, 1) as usize),
            'M' => self.grid.delete_lines(arg(params, 0, 1) as usize),
            'P' => self.grid.delete_chars(arg(params, 0, 1) as usize),
            // XTSMGRAPHICS shares SU's final byte and is selected by the DEC
            // private marker.
            'S' if private => self.xtsmgraphics(params),
            'S' => self.grid.scroll_up(arg(params, 0, 1) as usize),
            'T' => self.grid.scroll_down(arg(params, 0, 1) as usize),
            'X' => self.grid.erase_chars(arg(params, 0, 1) as usize),
            'Z' => self.grid.backward_tab(arg(params, 0, 1) as usize),
            'b' => {
                if let Some(cp) = self.last_printed {
                    for _ in 0..arg(params, 0, 1) {
                        self.grid.put(cp);
                    }
                }
            }
            'c' => {
                // The primary form answers whatever the parameter is
                // (`vtterm.c:4098`); the other two insist on `Param[1] == 0`,
                // which upstream's reset seeds to 0 (`:218`) so a bare request
                // qualifies and `CSI > 1 c` is silence.
                if gt {
                    // Secondary DA: VT382(>32) + xterm rev 331 (vtterm.c:2841).
                    if arg0(params, 0) == 0 {
                        self.send_csi(">32;331;0c");
                    }
                } else if inter == Some(b'=') {
                    // Tertiary DA — `vtterm.c:CSEQ`, which answers with the
                    // terminal's unit ID in a DCS. `ts.TerminalUID` is
                    // validated at eight hex digits wherever it is assigned, so
                    // upstream's `%8s` never pads and this is a plain join.
                    if arg0(params, 0) == 0 {
                        let uid = self.config.terminal_uid.clone();
                        self.send_dcs(&format!("!|{uid}"));
                    }
                } else if !private {
                    self.primary_da();
                }
            }
            // HPR and VPR — `vtterm.c:4096` and `:4100`. The same motion as CUF
            // and CUD against the page rather than the margins.
            'a' => self.grid.move_right(arg(params, 0, 1) as usize, false),
            'e' => self.grid.move_down(arg(params, 0, 1) as usize, false),
            // HPB and VPB — `vtterm.c:4105` and `:4106`, the backwards pair.
            'j' => self.grid.move_left(arg(params, 0, 1) as usize, false),
            'k' => self.grid.move_up(arg(params, 0, 1) as usize, false),
            // VPA. Origin mode applies, which is what separates it from a bare
            // cursor move — `vtterm.c:CSMoveToLineN`.
            'd' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_to_row(y);
            }
            // TBC, and each of the two forms has its own bit
            // (`buffer.c:5266`, `:5280`). Note where upstream puts the gate:
            // inside `ClearTabStop`, under an `if (NTabStops>0)` — so with no
            // stops at all neither arm runs, which is the same outcome by a
            // different route and the reason this reads as one test here.
            'g' => match arg0(params, 0) {
                0 if self.config.tab_stop_modify.allows(TabStopFlags::TBC0) => {
                    self.grid.clear_tab()
                }
                3 if self.config.tab_stop_modify.allows(TabStopFlags::TBC3) => {
                    self.grid.clear_all_tabs()
                }
                _ => {}
            },
            'h' => self.set_mode(private, params, true),
            'l' => self.set_mode(private, params, false),
            'm' => {
                if !private && !gt {
                    self.sgr(params)
                }
            }
            // DECDSR, the private form — `vtterm.c:CSQ_n_Mode`. Two requests
            // and no more: everything else under `?` is left alone, *including*
            // 5 and 6. Answering those here would be answering DSR to a
            // question that was not DSR, and a host that asked `CSI ? 6 n`
            // (DECXCPR) wants a page number in the reply.
            'n' if private => match arg0(params, 0) {
                53 | 55 => self.send_csi("?50n"),
                _ => {}
            },
            'n' => match arg0(params, 0) {
                5 => self.send_csi("0n"),
                6 => {
                    let (x, y) = (self.grid.cursor.x, self.grid.cursor.y);
                    let (top, _) = self.grid.scroll_region();
                    let (left, _) = self.grid.margins();
                    let (col, row) = if self.grid.origin_mode {
                        (x.saturating_sub(left), y.saturating_sub(top))
                    } else {
                        (x, y)
                    };
                    let body = format!("{};{}R", row + 1, col + 1);
                    self.send_csi(&body);
                }
                _ => {}
            },
            'r' => {
                let rows = self.grid.rows() as u16;
                let top = arg(params, 0, 1).saturating_sub(1) as usize;
                let bottom = arg(params, 1, rows).saturating_sub(1) as usize;
                self.grid.set_scroll_region(top, bottom);
            }
            'i' => self.media_copy(params, private),
            't' => self.window_op(params),
            // DECSLRM, but only while DECLRMM is on: otherwise the same final
            // byte is SCP, the ANSI.SYS save-cursor. `vtterm.c:4115`.
            's' => {
                if self.lr_margin_mode {
                    let cols = self.grid.cols() as u16;
                    let left = check_param_val(arg0(params, 0), cols);
                    let right = check_param_val_max(arg0(params, 1), cols);
                    self.grid
                        .set_lr_margins(left as usize - 1, right as usize - 1);
                } else {
                    self.save_cursor();
                }
            }
            'u' => self.restore_cursor(),
            _ => {}
        }
    }

    /// DECSC, and the save half of `ESC [ ? 1048 h` — upstream shares the slot,
    /// charset state included.
    fn save_cursor(&mut self) {
        self.grid.save_cursor();
        self.saved_charset[usize::from(self.alt_screen)] = Some(self.charset.save());
    }

    fn restore_cursor(&mut self) {
        self.grid.restore_cursor();
        if let Some(s) = self.saved_charset[usize::from(self.alt_screen)] {
            self.charset.restore(s);
        }
    }
}

/// Decode `38;2;r;g;b` / `38;5;idx` in all the colon and semicolon spellings
/// upstream accepts. Returns the colour and how many extra parameter groups it
/// swallowed.
fn extended_color(
    groups: &[Vec<u16>],
    i: usize,
    full_color: bool,
    palette: &[palette::Rgb; 256],
) -> Option<(u32, usize)> {
    let rgb = |r: u16, g: u16, b: u16| {
        palette::find_closest(palette, r as i32, g as i32, b as i32, full_color)
    };

    // Colon form: 38:5:idx arrives as a single group.
    let g = &groups[i];
    if g.len() > 1 {
        return match g[1] {
            2 if g.len() >= 5 => Some((rgb(g[2], g[3], g[4])?, 0)),
            5 if g.len() >= 3 => Some(((g[2] as u32).min(255), 0)),
            _ => None,
        };
    }
    // Semicolon form: 38;5;idx arrives as separate groups.
    let kind = groups.get(i + 1)?.first().copied()?;
    match kind {
        2 => {
            let r = groups.get(i + 2)?.first().copied()?;
            let g_ = groups.get(i + 3)?.first().copied()?;
            let b = groups.get(i + 4)?.first().copied()?;
            Some((rgb(r, g_, b)?, 4))
        }
        5 => {
            let idx = groups.get(i + 2)?.first().copied()?;
            Some(((idx as u32).min(255), 2))
        }
        _ => None,
    }
}

/// Set or clear `bits` on the attribute and record them in the mask either way
/// — upstream's `attr->Attr |= X; mask->Attr |= X` pair, which is what makes
/// "bold off" and "bold on" both *mention* bold as far as DECCARA cares.
fn set(attr: &mut Pen, mask: &mut u32, bits: u32, on: bool) {
    if on {
        attr.attrs |= bits;
    } else {
        attr.attrs &= !bits;
    }
    *mask |= bits;
}

/// The parameter list as groups, with an absent list standing in for a bare
/// `0` the way `CSI m` does.
fn sgr_groups(params: &Params) -> Vec<Vec<u16>> {
    let groups: Vec<Vec<u16>> = params.iter().map(|g| g.to_vec()).collect();
    if groups.is_empty() {
        vec![vec![0u16]]
    } else {
        groups
    }
}

/// Origin mode moves a rectangle's row down to the scroll region, and parks it
/// one past the bottom margin rather than on it when it would overshoot —
/// which is upstream's arithmetic, 1-based row against 0-based margin and all.
fn origin_shift(row: u16, region_top: usize, region_bottom: usize) -> u16 {
    let shifted = row + region_top as u16;
    if shifted as usize > region_bottom {
        region_bottom as u16 + 1
    } else {
        shifted
    }
}

/// `vtterm.c:CheckParamVal` — zero means one, out of range means the maximum.
fn check_param_val(p: u16, max: u16) -> u16 {
    if p == 0 {
        1
    } else if p > max {
        max
    } else {
        p
    }
}

/// `vtterm.c:CheckParamValMax` — zero means the *maximum*, not one. The
/// difference is what makes an omitted bottom-right corner mean "the far
/// corner" while an omitted top-left means "the origin".
fn check_param_val_max(p: u16, max: u16) -> u16 {
    if p == 0 || p > max {
        max
    } else {
        p
    }
}

fn arg(params: &Params, n: usize, default: u16) -> u16 {
    match params.iter().nth(n).and_then(|g| g.first().copied()) {
        Some(0) | None => default,
        Some(v) => v,
    }
}

/// Like [`arg`] but keeps an explicit zero, which ED/EL/TBC need.
fn arg0(params: &Params, n: usize) -> u16 {
    params
        .iter()
        .nth(n)
        .and_then(|g| g.first().copied())
        .unwrap_or(0)
}

fn is_clipboard_selector(byte: u8) -> bool {
    matches!(byte, b'c' | b'p' | b's' | b'0'..=b'7')
}

fn is_clipboard_text(c: char) -> bool {
    c >= ' ' || matches!(c, '\u{7}'..='\r' | '\u{1b}')
}

/// `ttlib.c:b64decode`, including its permissive stop rule. Whitespace is
/// skipped, an invalid byte (padding included) ends the input, and a final
/// group of two or three digits is still decoded.
fn base64_decode(input: &[u8]) -> Vec<u8> {
    let value = |byte| match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 1);
    let mut bits = 0u32;
    let mut state = 0u8;
    for &byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let Some(digit) = value(byte) else { break };
        bits = (bits << 6) | digit;
        state += 1;
        if state == 4 {
            out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            bits = 0;
            state = 0;
        }
    }
    match state {
        2 => {
            bits <<= 4;
            out.push((bits >> 8) as u8);
        }
        3 => {
            bits <<= 6;
            out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8]);
        }
        _ => {}
    }
    out
}

fn base64_encode(input: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(DIGITS[((bits >> 18) & 0x3f) as usize]);
        out.push(DIGITS[((bits >> 12) & 0x3f) as usize]);
        out.push(if chunk.len() > 1 {
            DIGITS[((bits >> 6) & 0x3f) as usize]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            DIGITS[(bits & 0x3f) as usize]
        } else {
            b'='
        });
    }
    out
}

/// `ConvertUTF16`'s UTF-8 failure rule: one U+FFFD for each byte in a broken
/// sequence, not one for the whole maximal invalid subpart as
/// `String::from_utf8_lossy` gives.
fn clipboard_utf8(input: &[u8]) -> String {
    let input = &input[..input.iter().position(|&b| b == 0).unwrap_or(input.len())];
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // SAFETY: `valid_up_to` is the UTF-8 validator's own boundary.
                out.push_str(unsafe { std::str::from_utf8_unchecked(&rest[..valid]) });
                rest = &rest[valid..];
                let invalid = error.error_len().unwrap_or(rest.len()).min(rest.len());
                for _ in 0..invalid {
                    out.push('\u{fffd}');
                }
                rest = &rest[invalid..];
            }
        }
    }
    out
}

impl State {
    /// Everything after the OSC's first `;`, bounded by `ts.MaxOSCBufferSize`.
    ///
    /// **`vte` splits on every semicolon and upstream splits on the first**,
    /// so the parameters after the leading number have to be joined back up:
    /// `ParseString` (`vtterm.c:5297`) reads digits into `Param[1]` until a
    /// `;` sets `HasParamStr`, and from then on *every* byte including the
    /// next semicolon goes into the string. Taking `params[1]` alone turns a
    /// window title of `a;b` into `a`, which is what this used to do.
    ///
    /// The bound is upstream's `StrLen + 1 < StrBuffSize` against a buffer that
    /// doubles up to `ts.MaxOSCBufferSize` (`:5265`) — so the ceiling is one
    /// byte short of the setting, further bytes are dropped, and the sequence
    /// still terminates normally. It counts **bytes**, not characters, and
    /// cutting a UTF-8 sequence in half is reachable; `from_utf8_lossy` is
    /// what the caller does with the result and gives U+FFFD, which is what
    /// Tera Term's own decoder gives for the same tail.
    ///
    /// What this cannot do is bound the *allocation*: `vte` collects the OSC
    /// into a `Vec` of its own with no ceiling under the `std` feature, so an
    /// OSC that never terminates still grows without limit. That half of the
    /// setting is `vte`'s to enforce and is recorded in `PLAN.md`.
    fn osc_string(&self, params: &[&[u8]]) -> Vec<u8> {
        let limit = self.config.max_osc_buffer.saturating_sub(1);
        let mut out: Vec<u8> = Vec::new();
        for (i, part) in params.iter().skip(1).enumerate() {
            if i > 0 {
                out.push(b';');
            }
            out.extend_from_slice(part);
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        out
    }

    /// OSC 52 — `XsProcClipboard` (`vtterm.c:4981`). The parser owns syntax,
    /// permissions and base64; the queued action owns no clipboard itself.
    fn osc_clipboard(&mut self, params: &[&[u8]]) {
        let body = self.osc_string(params);
        let selection_len = body
            .iter()
            .position(|&byte| !is_clipboard_selector(byte))
            .unwrap_or(body.len());
        if body.get(selection_len) != Some(&b';') {
            return;
        }
        let selection =
            String::from_utf8(body[..selection_len].to_vec()).expect("OSC 52 selectors are ASCII");
        let payload = &body[selection_len + 1..];

        if payload == b"?" {
            if self.config.clipboard_access.can_read() {
                self.clipboard_requests.push(ClipboardRequest::Read {
                    selection,
                    notify: self.config.notify_clipboard_access,
                });
            } else if self.config.notify_clipboard_access {
                self.clipboard_requests.push(ClipboardRequest::ReadRejected);
            }
        } else if self.config.clipboard_access.can_write() {
            self.clipboard_requests.push(ClipboardRequest::Write {
                text: clipboard_utf8(&base64_decode(payload)),
                notify: self.config.notify_clipboard_access,
            });
        } else if self.config.notify_clipboard_access {
            self.clipboard_requests
                .push(ClipboardRequest::WriteRejected);
        }
    }

    /// `vtterm.c:XsProcColor` — one colour, named by an OSC number and a colour
    /// number, either set from a spec or read back by the literal `?`.
    ///
    /// A spec that is neither `?` nor a form [`color::parse_spec`] knows is
    /// silently dropped: no colour changes and, unlike a query, nothing is sent
    /// back. So a host that asks in `rgbi:` or one of the CIE spellings cannot
    /// tell that from a terminal which took the colour and painted it.
    /// `vtterm.c:TermcapString` — the value of one capability, or `None` for a
    /// name upstream has nothing to say about.
    ///
    /// **One capability exists**, under two spellings, and its answer is the
    /// colour flags rather than the palette: 256 when `Xterm256Color` is on,
    /// 16 when one of the other two full-colour bits is, and 8 otherwise.
    /// `EnableANSIColor` off answers *nothing at all* — which is the one place
    /// that setting is visible on the wire, since it gates painting rather
    /// than parsing and the grid looks identical either way.
    fn termcap_value(&self, name: &[u8]) -> Option<&'static str> {
        if name != b"Co" && name != b"colors" {
            return None;
        }
        let cf = self.config.color_flags;
        if !cf.ansi_color {
            return None;
        }
        Some(if cf.xterm256 {
            "256"
        } else if cf.full_color() {
            "16"
        } else {
            "8"
        })
    }

    /// `vtterm.c:RequestTermcapString` — `DCS + q <hex name> ; … ST`.
    ///
    /// Names arrive hex-encoded and the reply repeats each name, hex-encoded
    /// again, with `=<hex value>` after it. The lead digit is the whole
    /// verdict: `1` when anything was answered and `0` when nothing was.
    ///
    /// Two shapes of malformed request are upstream's rather than tidy. A byte
    /// that is not a hex pair, and a `;` with no name in front of it, both
    /// abandon the rest of the list; and a name that *is* well formed but
    /// unknown ends the list too — after its separator has already been
    /// written, so the reply can end in a bare `;`.
    fn termcap_query(&mut self, req: &[u8]) {
        let mut reply = String::from("1+r");
        let mut answered = false;
        let mut name: Vec<u8> = Vec::new();
        let mut i = 0;
        let mut stop = false;
        while i < req.len() && !stop {
            if req[i] == b';' {
                if name.is_empty() || name.len() >= 16 {
                    break;
                }
                match self.termcap_append(&mut reply, &name, answered) {
                    Some(()) => answered = true,
                    None => stop = true,
                }
                name.clear();
                i += 1;
                continue;
            }
            let pair = req.get(i..i + 2).and_then(|p| {
                let hex = |b: u8| (b as char).to_digit(16);
                Some((hex(p[0])? * 16 + hex(p[1])?) as u8)
            });
            let Some(byte) = pair else { break };
            if name.len() >= 15 {
                break;
            }
            name.push(byte);
            i += 2;
        }
        if !stop
            && !name.is_empty()
            && name.len() < 16
            && self.termcap_append(&mut reply, &name, answered).is_some()
        {
            answered = true;
        }
        if !answered {
            reply = String::from("0+r");
        }
        self.send_dcs(&reply);
    }

    /// One capability's `name=value`, hex for hex, with the separator upstream
    /// writes *before* it knows whether there is anything to write.
    fn termcap_append(&self, reply: &mut String, name: &[u8], any: bool) -> Option<()> {
        if any {
            reply.push(';');
        }
        let value = self.termcap_value(name)?;
        for byte in name {
            reply.push_str(&format!("{byte:02x}"));
        }
        reply.push('=');
        for byte in value.as_bytes() {
            reply.push_str(&format!("{byte:02x}"));
        }
        Some(())
    }

    fn osc_color(&mut self, mode: u32, number: u32, spec: &[u8], bell: bool) {
        let Some(slot) = color::slot_of(mode, number) else {
            return;
        };
        let full = self.config.color_flags.full_color();
        if spec == b"?" {
            let (r, g, b) = self.colors.get(slot, &self.config, full);
            // `GetRValue(color)*257` — an eight-bit channel widened to
            // sixteen by repeating it, which is the scaling `#RGB`'s `<< 4`
            // notably does *not* use on the way in.
            let (r, g, b) = (u32::from(r) * 257, u32::from(g) * 257, u32::from(b) * 257);
            // Only `4` and `5` echo the colour number, because only they have
            // one; `10`-`19` name the colour with the OSC number itself.
            let body = if mode == 4 || mode == 5 {
                format!("{mode};{number};rgb:{r:04x}/{g:04x}/{b:04x}")
            } else {
                format!("{mode};rgb:{r:04x}/{g:04x}/{b:04x}")
            };
            self.send_osc_terminated(&body, bell);
        } else if let Some(color) = color::parse_spec(spec) {
            self.colors.set(slot, color, full);
            self.colors_dirty = true;
        }
    }

    fn osc_reset(&mut self, mode: u32, number: u32) {
        if let Some(slot) = color::slot_of(mode, number) {
            let full = self.config.color_flags.full_color();
            self.colors.reset(slot, &self.config, full);
            self.colors_dirty = true;
        }
    }

    /// `OSC 4` and `OSC 5`'s payload: `<number>;<spec>` repeated.
    ///
    /// The loop is upstream's, including where it gives up. Before the first
    /// `;` only digits are allowed, and **any other byte abandons the whole
    /// sequence** rather than skipping one pair — so `OSC 4;x;red;1;blue`
    /// changes nothing at all. After it, everything up to the next `;` is the
    /// spec, which is why a spec cannot contain one.
    fn osc_palette(&mut self, mode: u32, s: &[u8], bell: bool) {
        let mut number: u32 = 0;
        let mut spec: Option<usize> = None;
        for i in 0..s.len() {
            match spec {
                None => {
                    if s[i].is_ascii_digit() {
                        number = number.wrapping_mul(10).wrapping_add(u32::from(s[i] - b'0'));
                    } else if s[i] == b';' {
                        spec = Some(i + 1);
                    } else {
                        return;
                    }
                }
                Some(start) => {
                    if s[i] == b';' {
                        self.osc_color(mode, number, &s[start..i], bell);
                        number = 0;
                        spec = None;
                    }
                }
            }
        }
        if let Some(start) = spec {
            self.osc_color(mode, number, &s[start..], bell);
        }
    }

    /// `OSC 10` through `OSC 19`'s payload: one spec per colour, and **the OSC
    /// number walks forward with each `;`**.
    ///
    /// So `OSC 10;#000;#fff` is a foreground *and* a background, and
    /// `OSC 10;?;?;?` asks three questions of which the third — `OSC 12`, the
    /// cursor — has no arm and goes unanswered. Reading the number as fixed
    /// gives a terminal that sets its foreground three times.
    fn osc_dynamic(&mut self, mode: u32, s: &[u8], bell: bool) {
        let mut number = mode;
        let mut start = 0;
        for i in 0..s.len() {
            if s[i] == b';' {
                self.osc_color(number, 0, &s[start..i], bell);
                number += 1;
                start = i + 1;
            }
        }
        self.osc_color(number, 0, &s[start..], bell);
    }

    /// `OSC 104` and `OSC 105`.
    ///
    /// With no parameter string at all this is the whole palette or the three
    /// special colours; with one it is a `;`-separated list of numbers. The
    /// two edges are upstream's and neither is what it looks like: an **empty**
    /// list is not "no list", so `OSC 104;` resets palette entry 0 alone, and
    /// a non-digit does not end the list — it poisons the number in hand, so
    /// `OSC 104;1;x;2` resets 1, then *everything*, then 2.
    fn osc_reset_palette(&mut self, mode: u32, s: Option<&[u8]>) {
        let Some(s) = s else {
            self.osc_reset(mode, color::UNSPEC);
            return;
        };
        let mut number: u32 = 0;
        for &b in s {
            if b.is_ascii_digit() {
                number = number.wrapping_mul(10).wrapping_add(u32::from(b - b'0'));
            } else if b == b';' {
                self.osc_reset(mode, number);
                number = 0;
            } else {
                number = color::UNSPEC;
            }
        }
        if number != color::UNSPEC {
            self.osc_reset(mode, number);
        }
    }

    /// `OSC 110` through `OSC 119`.
    ///
    /// Its own colour goes back first, unconditionally. Then — and this is not
    /// xterm's reading of the sequence — any parameter string is a list of
    /// further **OSC numbers** to reset, so `OSC 110;11` puts back the
    /// foreground and the background. Here a non-digit does end the list,
    /// which is the opposite of the arm above it.
    fn osc_reset_dynamic(&mut self, mode: u32, s: Option<&[u8]>) {
        self.osc_reset(mode, color::UNSPEC);
        let Some(s) = s else { return };
        let mut number: u32 = 0;
        for &b in s {
            if b.is_ascii_digit() {
                number = number.wrapping_mul(10).wrapping_add(u32::from(b - b'0'));
            } else if b == b';' {
                self.osc_reset(number, color::UNSPEC);
                number = 0;
            } else {
                number = color::UNSPEC;
                break;
            }
        }
        if number != color::UNSPEC {
            self.osc_reset(number, color::UNSPEC);
        }
    }
}

impl Perform for State {
    fn print(&mut self, c: char) {
        let cp = c as u32;
        // `vtterm.c:788` only consults the charset for codepoints that could
        // have come from a single byte; anything above U+00FF is text by
        // definition and never DEC special graphics.
        let special = cp <= 0xff && self.charset.is_special(cp);
        // Auto print dumps the line the wrap is about to leave, so the text
        // has to be taken before the character lands. Upstream gets this for
        // free: its wrap calls `LineFeed(LF, FALSE)` explicitly and the dump
        // is at the top of that function.
        //
        // Into a field rather than a local, and the whole thing behind the
        // flag: a `String` held across the write below is a destructor in
        // every character's frame whether or not auto print is on, and that
        // alone measured 4% of this parser's throughput.
        if self.auto_print {
            self.printer_line = self.line_dump_text(0x0a);
        }
        let wrapped = if special {
            // Upstream builds a throwaway attribute for the one character
            // (`CharAttrTmp`), leaving the pen alone. Same here.
            let pen = self.grid.pen.attrs;
            self.grid.pen.attrs |= ATTR_SPECIAL;
            let w = self.grid.put(cp);
            self.grid.pen.attrs = pen;
            w
        } else {
            self.grid.put(cp)
        };
        // An automatic line break is a `CarriageReturn` and a `LineFeed`
        // upstream (`vtterm.c:870`, `:900`), so a macro sees a wrapped line as
        // two lines — which is the whole reason `Grid::put` reports it. The
        // taps go in *before* the character, which is the order upstream
        // reaches them in: the break happens, then the character lands.
        //
        // Both calls pass `logFlag` FALSE, so this is one of the two places
        // `ts.EnableContinuedLineCopy` suppresses the pair and a macro sees
        // the host's line whole.
        if wrapped {
            // `CarriageReturn(FALSE)`, then `LineFeed(LF, FALSE)` — and the
            // dump is at the *top* of the second, so it lands between the two
            // taps rather than before them. Both taps reach the printer's copy
            // through `NeedsOutputBufs`, which does not count the printer, so
            // whether a wrapped line arrives at the printer as one line or two
            // depends on whether a log or a macro is also running.
            let bufs = self.needs_output_bufs();
            if !self.config.continued_line_copy {
                self.tap(0x0d);
                if bufs {
                    self.printer_tap(0x0d);
                }
            }
            if self.auto_print {
                let text = std::mem::take(&mut self.printer_line);
                self.printer_write(&text);
                self.printer_line = text;
            }
            if !self.config.continued_line_copy {
                self.tap(0x0a);
                if bufs {
                    self.printer_tap(0x0a);
                }
            }
        }
        self.tap(cp);
        // `PutU32` reaches `OutputLogUTF32` directly rather than through
        // `NeedsOutputBufs`, so the character itself is always copied. The
        // test is at the call site rather than inside: this is the hottest
        // line in the engine and a call per character is not free.
        if self.printer.is_on() {
            self.printer_tap(cp);
        }
        if let Some(log) = &mut self.log_text {
            log.push(c);
        }
        self.last_printed = Some(cp);
        self.prev_was_cr = false;
        self.prev_was_lf = false;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // ENQ. `vtterm.c:1075` writes `ts.Answerback` with `CommBinaryOut`
            // — the binary path, so the bytes go out exactly as the file spelt
            // them, with no CR translation and no echo. The default is empty,
            // which answers nothing.
            0x05 => {
                if !self.config.answerback.is_empty() {
                    let answer = self.config.answerback.clone();
                    self.send(&answer);
                }
            }
            // BEL. `vtterm.c:1077` tests the setting *before* calling
            // `RingBell`, so with the bell off nothing is asked for and the
            // governor's clock never moves — which is not true of `ESC g`, the
            // other way in.
            0x07 => {
                if self.config.beep != Beep::Off {
                    self.bells = self.bells.saturating_add(1);
                }
            }
            0x08 => {
                // `BackSpace()` taps a BS in each of its two moving arms and
                // not in the arm that does nothing, so the test is whether the
                // cursor moved rather than whether a BS arrived — and both
                // arms carry `!ts.LogTypePlainText`, which is the whole of
                // what that setting does.
                let before = (self.grid.cursor.x, self.grid.cursor.y);
                self.grid.backspace();
                if (self.grid.cursor.x, self.grid.cursor.y) != before && !self.config.log_plain_text
                {
                    self.tap(0x08);
                }
            }
            0x09 => {
                // `vtterm.c:Tab()` — a plain HT takes the *pending wrap first*
                // and only then tabs, so a tab arriving on a full line starts
                // the next one. CHT (`CSI Ps I`) does not do this; it calls
                // `CursorForwardTab` directly. `ts.VTCompatTab` suppresses it,
                // and that is one of the setting's two halves: the other is in
                // `Grid::forward_tab`, which stops arming the wrap on the way
                // out.
                if self.grid.cursor.pending_wrap && !self.config.vt_compat_tab {
                    // `CarriageReturn(FALSE); LineFeed(LF,FALSE);` — the second
                    // of the two places the wrap generates a line break, and
                    // so the second `ts.EnableContinuedLineCopy` suppresses.
                    if self.auto_print {
                        self.dump_current_line(0x0a);
                    }
                    self.carriage_return(false);
                    if !self.config.continued_line_copy {
                        self.tap(0x0a);
                        // As in the character path: the printer's copy of a
                        // generated line break rides `NeedsOutputBufs`, which
                        // does not count the printer.
                        if self.needs_output_bufs() {
                            self.printer_tap(0x0d);
                            self.printer_tap(0x0a);
                        }
                    }
                    self.grid.line_feed();
                    // `SetLineContinued()` (`vtterm.c:717`). Unlike the
                    // character path, a tab writes nothing on the new row to
                    // carry the bit, so upstream sets it on the row itself —
                    // and only here does it gate that on the setting, which
                    // makes no difference to anything that reads it.
                    self.grid.set_line_continued(true);
                    self.grid.cursor.pending_wrap = false;
                }
                self.grid.forward_tab(1);
                self.tap(0x09);
            }
            0x0e => self.shift(Shift::Ls1), // SO
            0x0f => self.shift(Shift::Ls0), // SI
            // HTS, the 8-bit spelling — `vtterm.c:1160`. The only C1 that
            // reaches here rather than being folded into its `ESC` form by
            // `Vt::rewrite_c1`, and the reason is `TABF_HTS8`: it is a
            // different bit from `TABF_HTS7`, so the two spellings have to
            // stay apart. The gate was applied on the way in, where the byte
            // was still eight-bit; there is nothing left to test here.
            0x88 => self.grid.set_tab(),
            // LF, VT and FF all line-feed (vtterm.c treats them alike).
            //
            // Not quite: upstream sends VT and FF straight to `LineFeed` and
            // only LF through `ProcessLF`, so `ts.CRReceive` does not apply to
            // them. The grid cannot tell the difference; the macro tap can, and
            // this is where it would show up if it is ever worth fixing.
            0x0a..=0x0c => {
                if let Some(log) = &mut self.log_text {
                    log.push('\n');
                }
                self.process_lf(byte);
                self.prev_was_lf = true;
                self.prev_was_cr = false;
                return;
            }
            0x0d => {
                self.process_cr();
                self.prev_was_cr = true;
                self.prev_was_lf = false;
                return;
            }
            _ => {}
        }
        self.prev_was_cr = false;
        self.prev_was_lf = false;
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let inter = intermediates.first().copied();
        let private = inter == Some(b'?');
        let gt = inter == Some(b'>');

        // Intermediates change what a final byte means, and silently ignoring
        // them is how `CSI ... $ r` (DECCARA) ends up reprogramming the scroll
        // region. Everything below assumes no intermediate unless it says so.
        match (inter, action) {
            // DECSCA — `vtterm.c:3335`. 0 and 2 both clear.
            (Some(b'"'), 'q') => match arg0(params, 0) {
                0 | 2 => self.grid.pen.attrs &= !ATTR2_PROTECT,
                1 => self.grid.pen.attrs |= ATTR2_PROTECT,
                _ => {}
            },
            // DECSACE — `vtterm.c:CSAster`. Anything but 0, 1 and 2 leaves the
            // mode alone rather than resetting it.
            (Some(b'*'), 'x') => match arg0(params, 0) {
                0 | 1 => self.rect_mode = false,
                2 => self.rect_mode = true,
                _ => {}
            },
            // DECRQCRA. Not upstream's — see `Config::decrqcra`.
            (Some(b'*'), 'y') => self.decrqcra(params),
            // DECSTR, the soft reset.
            (Some(b'!'), 'p') => self.soft_reset(),
            // DECSCL. It soft-resets, re-reads the terminal id's level, and
            // can then only *lower* it — `CSI 61"p` forces level 1.
            (Some(b'"'), 'p') => {
                self.soft_reset();
                self.vt_level = self.config.term_id.vt_level();
                self.send_8bit = self.vt_level >= 2 && self.config.send_8bit_ctrl;
                match arg0(params, 0) {
                    p @ 61..=65 => {
                        let want = (p - 60) as u8;
                        if self.vt_level > want {
                            self.vt_level = want;
                        }
                    }
                    _ => self.vt_level = 1,
                }
                self.send_8bit = !(self.vt_level < 2 || arg0(params, 1) == 1);
            }
            // DECSCUSR. The even values are the non-blinking forms, and zero
            // is the same blinking block as one (`vtterm.c:3966`). Like
            // DECSET 12, the entire sequence is ignored unless the file has
            // enabled host cursor control.
            (Some(b' '), 'q') if self.config.cursor_ctrl_sequence => {
                let (shape, nonblinking) = match arg0(params, 0) {
                    0 | 1 => (1, false),
                    2 => (1, true),
                    3 => (3, false),
                    4 => (3, true),
                    5 => (5, false),
                    6 => (5, true),
                    _ => return,
                };
                self.config.cursor_shape = shape;
                self.config.nonblinking_cursor = nonblinking;
            }
            (Some(b'\''), _) => self.csi_quote(params, action),
            // DECRQM. `vte` puts the private marker in the intermediates, so
            // `CSI ? Ps $ p` arrives as `?$` and would otherwise fall through
            // to the plain path as a bare `p`.
            (Some(b'?'), 'p') if intermediates.get(1) == Some(&b'$') => {
                self.decrqm(arg0(params, 0), true)
            }
            (Some(b'$'), 'p') => self.decrqm(arg0(params, 0), false),
            (Some(b'$'), _) => self.csi_dollar(params, action),
            _ => self.csi_plain(params, private, gt, inter, action),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }

        // DECALN — `ESC # 8`. It fills the screen with `E` and resets both
        // margin pairs, which is why it is here and not with the charsets.
        if intermediates.first() == Some(&b'#') {
            if byte == b'8' {
                self.grid.fill_with_e();
                self.grid.reset_scroll_region();
                self.grid.reset_lr_margins();
                self.grid.move_cursor(0, 0);
            }
            return;
        }

        // S7C1T / S8C1T — `ESC SP F` and `ESC SP G` (`vtterm.c:ESCSpace`).
        // The 8-bit form is refused below VT level 2, and DECSCL having
        // lowered the level is enough to refuse it.
        if intermediates.first() == Some(&b' ') {
            match byte {
                b'F' => self.send_8bit = false,
                b'G' if self.vt_level >= 2 => self.send_8bit = true,
                _ => {}
            }
            return;
        }

        // Single-byte character-set designation: ESC ( ) * + <final>.
        if let Some(&i) = intermediates.first() {
            if matches!(i, b'(' | b')' | b'*' | b'+') && intermediates.len() == 1 {
                if let Some(cs) = sbcs_final(byte, self.config.japanese) {
                    self.charset.designate(gset_from_intermediate(i), cs);
                }
                // `TF_AUTOINVOKE` (`ttset.c:1101`, key default off) — a
                // designation into G0 invokes G0 into GL, so `ESC ( B` puts
                // ASCII back without an SI.
                //
                // Two things `ESCSBCSSelect` (`vtterm.c:1409`) does that the
                // name would not give. The invoke sits **outside** the switch
                // that handled the final byte, so `ESC ( Z` — a designation of
                // nothing — still performs it; and it is not gated on
                // `ts.ISO2022Flag`, unlike every other locking shift in the
                // parser, so `ISO2022ShiftFunction=off` does not stop it.
                // Hence `self.charset.invoke` here rather than `self.shift`,
                // which is the gated path.
                if self.config.auto_invoke && gset_from_intermediate(i) == 0 {
                    self.charset.invoke(Shift::Ls0);
                }
            }
            // Multi-byte designations (ESC $ ...) are Kanji, deferred with CJK.
            return;
        }

        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            // IND. The full `LineFeed`, LNM tail included — and its byte is a
            // zero, so auto print does not dump the line it just left.
            b'D' => self.line_feed(0),
            // NEL. `MoveCursor(0, CursorY)` and then a line feed
            // (`vtterm.c:1508`) — **column zero**, not the left margin and not
            // `CarriageReturn`, which is the one place the two differ.
            b'E' => {
                self.grid.move_cursor(0, self.grid.cursor.y);
                self.line_feed(0);
            }
            // DECID, the obsolete spelling of Primary DA — `vtterm.c:1539`
            // hands it to the same `AnswerTerminalType`.
            b'Z' => self.primary_da(),
            // HTS, the 7-bit spelling — `vtterm.c:1512`, gated on `TABF_HTS7`.
            // The 8-bit `0x88` is a separate bit, so a file can accept one and
            // refuse the other.
            b'H' => {
                if self.config.tab_stop_modify.allows(TabStopFlags::HTS7) {
                    self.grid.set_tab();
                }
            }
            b'M' => self.grid.reverse_index(),
            // DECBI / DECFI (vtterm.c:1482, :1493). Both are no-ops when the
            // cursor is outside the scroll region, and scroll the region
            // sideways rather than moving when it is already on the margin.
            b'6' => {
                if self.cursor_in_region() {
                    if self.grid.cursor.x == self.grid.margins().0 {
                        self.grid.scroll_right(1);
                    } else {
                        self.grid.move_left(1, true);
                    }
                }
            }
            b'9' => {
                if self.cursor_in_region() {
                    if self.grid.cursor.x == self.grid.margins().1 {
                        self.grid.scroll_left(1);
                    } else {
                        self.grid.move_right(1, true);
                    }
                }
            }
            // `ESC g` — GNU screen's visual bell, and the one place upstream
            // asks for a bell without consulting the setting first
            // (`vtterm.c:1561`). It passes `IdBeepVisual` to say which kind it
            // wants and `RingBell` never reads its argument, so what actually
            // happens is whatever `ts.Beep` says: an audible beep by default,
            // and nothing at all when the bell is off. Reproduced, because it
            // is what a user of Tera Term sees; written up as a defect.
            b'g' => self.bells = self.bells.saturating_add(1),
            b'N' => self.shift(Shift::Ss2),
            b'O' => self.shift(Shift::Ss3),
            b'n' => self.shift(Shift::Ls2),
            b'o' => self.shift(Shift::Ls3),
            b'|' => self.shift(Shift::Ls3r),
            b'}' => self.shift(Shift::Ls2r),
            b'~' => self.shift(Shift::Ls1r),
            b'c' => {
                self.grid.reset();
                self.sixel_images.clear();
                self.charset.reset();
                self.saved_charset = [None, None];
                self.title.clear();
                self.title_stack.clear();
                self.rect_mode = false;
                self.lr_margin_mode = false;
                // `ResetTerminal` ends with `ChangeTerminalID()`
                // (`vtterm.c:5850`), which recomputes both from the terminal
                // *id* — so RIS undoes a DECSCL that lowered the level, and
                // undoes an S8C1T only as far as the configured default.
                self.vt_level = self.config.term_id.vt_level();
                self.send_8bit = self.vt_level >= 2 && self.config.send_8bit_ctrl;
                // `ResetTerminal` clears the mouse state outright, position
                // and button mask included. `SoftReset` clears none of it.
                self.mouse.reset();
                self.modes = self.modes.reset(&self.config);
                // ...and puts the bell governor's clocks back (`:348`), which
                // is the one part of `ResetTerminal` that lives outside the
                // engine. `SoftReset` does not.
                self.bell_reset = true;
                // `ResetTerminal` clears `PrinterMode` (`vtterm.c:327`) and
                // stops there: it does not close the job and does not clear
                // `AutoPrintMode`. So a RIS in the middle of controller-mode
                // printing leaves an open job that nothing will print until
                // some later sequence closes it. Reproduced — diverging means
                // printing a page the user's own Tera Term does not.
                self.printer.stop();
            }
            _ => {}
        }
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.dcs_buf.clear();
        if ignore {
            self.dcs = None;
            return;
        }
        let intermediate = intermediates.first().copied();
        if intermediate.is_none() && action == 'q' {
            let scrolling = self.modes.sixel_scrolling;
            let (line, column) = if scrolling {
                (
                    self.grid.scrolled_off() + self.grid.cursor.y as u64,
                    self.grid.cursor.x,
                )
            } else {
                (self.grid.scrolled_off(), 0)
            };
            self.dcs = Some(Dcs::Sixel {
                decoder: Box::new(sixel::Decoder::new(
                    arg0(params, 0),
                    arg0(params, 1),
                    self.colors.normal[1],
                )),
                line,
                column,
                scrolling,
                alternate: self.alt_screen,
            });
        } else {
            self.dcs = Some(Dcs::Short {
                intermediate,
                action,
            });
        }
    }

    fn put(&mut self, byte: u8) {
        // `ts.MaxOSCBufferSize` bounds the *OSC* buffer upstream; the DCS one
        // is a separate `static unsigned char StrBuff[256]` in `DeviceControl`
        // (`vtterm.c:4601`) filled under `StrLen < sizeof(StrBuff)-1`, so 255
        // bytes and no setting reaches it. Either way an unterminated DCS must
        // not be able to grow without limit.
        match &mut self.dcs {
            Some(Dcs::Sixel { decoder, .. }) => decoder.put(byte),
            Some(Dcs::Short { .. }) if self.dcs_buf.len() < 255 => self.dcs_buf.push(byte),
            Some(Dcs::Short { .. }) | None => {}
        }
    }

    fn unhook(&mut self) {
        let Some(dcs) = self.dcs.take() else {
            return;
        };
        match dcs {
            Dcs::Sixel {
                decoder,
                line,
                column,
                scrolling,
                alternate,
            } => {
                if let Some(raster) = (*decoder).finish() {
                    self.install_sixel(raster, line, column, scrolling, alternate);
                }
            }
            Dcs::Short {
                intermediate,
                action,
            } => {
                let req = std::mem::take(&mut self.dcs_buf);
                match (intermediate, action) {
                    (Some(b'$'), 'q') => self.decrqss(&req),
                    // XTGETTCAP — `vtterm.c:RequestTermcapString`, marked
                    // "xterm experimental" there and answering exactly one
                    // capability.
                    (Some(b'+'), 'q') => self.termcap_query(&req),
                    // DECSTUI — normally read and dropped because TF_LOCKTUID
                    // ships on. The same eight-digit validation as the file's
                    // keeps a long value from truncating into a new identity.
                    (Some(b'!'), '{') if !self.config.lock_uid => {
                        if let Some(uid) =
                            std::str::from_utf8(&req).ok().and_then(valid_terminal_uid)
                        {
                            self.config.terminal_uid = uid;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let Some(&kind) = params.first() else { return };
        let Ok(kind) = std::str::from_utf8(kind).unwrap_or("").parse::<u32>() else {
            return;
        };
        // **All three set the window title**, icon name included: `vtterm.c`'s
        // `case 0: case 1: case 2:` fall into one arm (`:5109`), which writes
        // `cv.TitleRemoteW` and calls `ChangeTitle`. There is one title here as
        // there is upstream, so an icon name set with OSC 1 lands in the title
        // bar — which reads like a bug and is what Tera Term does.
        //
        // `off` discards it rather than merely hiding it: the arm is gated on
        // `ts.AcceptTitleChangeRequest` before `cv.TitleRemoteW` is touched, so
        // a title that arrived while the setting was off is not there to be
        // shown if it is turned on afterwards.
        // The `params.len() > 1` is upstream's `if (StrBuff)`: an OSC with no
        // string at all leaves the title alone, where one with an *empty*
        // string clears it.
        if matches!(kind, 0..=2)
            && self.config.accept_title_change != TitleChange::Off
            && params.len() > 1
        {
            self.title = String::from_utf8_lossy(&self.osc_string(params)).into_owned();
        }
        if kind == 52 {
            self.osc_clipboard(params);
        }
        // `HasParamStr`: whether a `;` followed the number at all, which four
        // of the six colour arms below distinguish from an empty string.
        let payload = (params.len() > 1).then(|| self.osc_string(params));
        match kind {
            4 | 5 => {
                if let Some(s) = &payload {
                    self.osc_palette(kind, s, bell_terminated);
                }
            }
            10..=19 => {
                if let Some(s) = &payload {
                    self.osc_dynamic(kind, s, bell_terminated);
                }
            }
            104 | 105 => self.osc_reset_palette(kind, payload.as_deref()),
            110..=119 => self.osc_reset_dynamic(kind, payload.as_deref()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[u8], cols: usize, rows: usize) -> Vt {
        let mut vt = Vt::new(Config {
            cols,
            rows,
            ..Config::default()
        });
        vt.feed(input);
        vt
    }

    fn row(vt: &Vt, y: usize) -> String {
        let mut s = String::new();
        for cell in vt.grid().line(y) {
            if cell.width_class == tt_grid::WIDTH_PAD {
                continue;
            }
            let mut any = false;
            for cp in cell.codepoints() {
                s.push(char::from_u32(cp).unwrap_or('?'));
                any = true;
            }
            if !any {
                s.push(' ');
            }
        }
        s.trim_end().to_string()
    }

    #[test]
    fn bare_cr_is_a_carriage_return_by_default() {
        // The CRReceive=IdCR default: "Hello, world!\rSecond line" overwrites.
        let vt = run(b"Hello, world!\rSecond line", 40, 4);
        assert_eq!(row(&vt, 0), "Second lined!");
        assert_eq!(vt.grid().cursor.x, 11);
    }

    #[test]
    fn normal_debug_spells_raw_controls_and_marks_high_bytes() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"\x1b[1;31m");
        vt.grid_mut().insert_mode = true;
        vt.grid_mut().autowrap = false;
        vt.set_macro_tap_enabled(true);
        vt.set_debug_mode(DebugMode::Normal);
        vt.feed(&[0x01, b'A', 0x80, 0x7f]);

        assert_eq!(row(&vt, 0), "^AA^@<DEL>");
        assert_eq!(vt.take_macro_bytes(), b"^AA^@<DEL>");
        let cells = vt.grid().line(0);
        assert_eq!(cells[0].attrs & ATTR_MASK, 0, "bold was cleared");
        assert_ne!(cells[0].attrs & ATTR2_FORE, 0, "the SGR colour survived");
        assert_ne!(cells[3].attrs & ATTR_REVERSE, 0);
        assert_ne!(cells[4].attrs & ATTR_REVERSE, 0);
        assert_eq!(cells[5].attrs & ATTR_MASK, 0, "the next byte reset reverse");
        assert!(vt.grid().insert_mode && !vt.grid().autowrap);
        assert_eq!(vt.grid().pen.attrs & ATTR_MASK, 0);
    }

    #[test]
    fn hex_and_no_output_debug_bypass_the_escape_parser() {
        let mut vt = Vt::new(Config::default());
        vt.set_debug_mode(DebugMode::Hex);
        vt.feed(b"\x1b[A");
        assert_eq!(row(&vt, 0), "1B 5B 41");

        vt.set_debug_mode(DebugMode::NoOutput);
        vt.feed(b"nothing\x1b[2J");
        assert_eq!(row(&vt, 0), "1B 5B 41");
    }

    #[test]
    fn the_debug_key_uses_its_gate_and_mode_mask() {
        let mut vt = Vt::new(Config {
            debug_enabled: true,
            debug_modes: DebugModes::from_bits(DebugModes::HEX),
            ..Config::default()
        });
        assert!(vt.cycle_debug_mode());
        assert_eq!(vt.debug_mode(), DebugMode::Hex);
        assert!(vt.cycle_debug_mode());
        assert_eq!(vt.debug_mode(), DebugMode::Off);

        vt.set_config(Config {
            debug_enabled: false,
            ..vt.config().clone()
        });
        assert!(!vt.cycle_debug_mode());
        assert_eq!(vt.debug_mode(), DebugMode::Off);
    }

    #[test]
    fn primary_da_identifies_as_vt100() {
        let vt = run(b"\x1b[c", 20, 2);
        assert_eq!(vt.reply(), b"\x1b[?1;2c");
    }

    #[test]
    fn xtsmgraphics_reports_sixel_limits_without_changing_them() {
        let mut vt = Vt::new(Config {
            cols: 4,
            rows: 3,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            cell: (9, 17),
            ..WindowMetrics::default()
        });

        vt.feed(b"\x1b[?1;1;0S\x1b[?1;4;0S\x1b[?2;1;0S\x1b[?2;4;0S\x1b[?1;3;512S");
        assert_eq!(
            vt.take_reply(),
            concat!(
                "\x1b[?1;0;256S",
                "\x1b[?1;0;256S",
                "\x1b[?2;0;36;51S",
                "\x1b[?2;0;4096;4096S",
                "\x1b[?1;3;0S",
            )
            .as_bytes()
        );

        vt.feed(b"\x1b[?3;1;0S\x1b[?9;1;0S\x1b[?2;9;0S");
        assert_eq!(vt.take_reply(), b"\x1b[?3;3;0S\x1b[?9;1;0S\x1b[?2;2;0S");
    }

    #[test]
    fn sixel_is_decoded_at_the_cursor_and_moves_below_it() {
        let mut vt = Vt::new(Config {
            cols: 4,
            rows: 3,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            cell: (1, 6),
            ..WindowMetrics::default()
        });
        vt.feed(b"\x1b[2;2H\x1bP7;1q\"1;1;2;6#2;2;100;0;0~~\x1b\\");

        let image = vt.sixel_images().next().expect("sixel image");
        assert_eq!((image.line(), image.column()), (1, 1));
        assert_eq!((image.width(), image.height()), (2, 6));
        assert_eq!(&image.pixels()[..4], &[255, 0, 0, 255]);
        assert_eq!((vt.grid().cursor.x, vt.grid().cursor.y), (1, 2));
    }

    #[test]
    fn sixel_scrolls_with_the_lines_and_survives_in_history() {
        let mut vt = Vt::new(Config {
            cols: 4,
            rows: 2,
            scrollback_max: 10,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            cell: (1, 6),
            ..WindowMetrics::default()
        });
        vt.feed(b"\x1b[2;2H\x1bP7;1q\"1;1;1;12@-@\x1b\\");

        assert_eq!(vt.grid().scrolled_off(), 2);
        assert_eq!((vt.grid().cursor.x, vt.grid().cursor.y), (1, 1));
        assert_eq!(vt.sixel_images().next().unwrap().line(), 1);
        assert!(vt.grid().absolute_line(1).is_some());
    }

    #[test]
    fn decsdm_fixes_sixel_to_the_page_and_leaves_the_cursor() {
        let mut vt = Vt::new(Config {
            cols: 4,
            rows: 3,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            cell: (1, 6),
            ..WindowMetrics::default()
        });
        vt.feed(b"\x1b[3;3H\x1b[?80h\x1bP7;1q@\x1b\\\x1b[?80$p");

        let image = vt.sixel_images().next().unwrap();
        assert_eq!((image.line(), image.column()), (0, 0));
        assert_eq!((vt.grid().cursor.x, vt.grid().cursor.y), (2, 2));
        assert_eq!(vt.reply(), b"\x1b[?80;1$y");
    }

    #[test]
    fn later_text_erases_the_sixel_tile_it_overwrites() {
        let mut vt = Vt::new(Config {
            cols: 4,
            rows: 3,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            cell: (1, 6),
            ..WindowMetrics::default()
        });
        vt.feed(b"\x1bP7;1q@\x1b\\");
        assert_eq!(vt.sixel_images().count(), 1);
        vt.feed(b"\x1b[1;1HX");
        assert_eq!(vt.sixel_images().count(), 0);
    }

    #[test]
    fn cursor_position_report_is_one_based() {
        let vt = run(b"\x1b[10;20H\x1b[6n", 40, 24);
        assert_eq!(vt.reply(), b"\x1b[10;20R");
    }

    #[test]
    fn osc_zero_sets_the_title() {
        let vt = run(b"\x1b]0;My Session\x07text", 20, 2);
        assert_eq!(vt.remote_title(), "My Session");
        assert_eq!(row(&vt, 0), "text");
    }

    /// The four spellings, and the one place the window and the report
    /// disagree about them.
    #[test]
    fn the_file_and_the_host_combine_four_ways() {
        let of = |mode, input: &[u8]| {
            let mut vt = Vt::new(Config {
                cols: 20,
                rows: 2,
                title: "file".into(),
                accept_title_change: mode,
                title_report: TitleReport::Accept,
                ..Config::default()
            });
            vt.feed(input);
            (
                vt.window_title(),
                String::from_utf8_lossy(vt.reply()).into_owned(),
            )
        };

        let set = b"\x1b]2;host\x1b\\\x1b[21t";
        assert_eq!(
            of(TitleChange::Overwrite, set),
            ("host".into(), "\x1b]lhost\x1b\\".into())
        );
        assert_eq!(
            of(TitleChange::Ahead, set),
            ("host file".into(), "\x1b]lhost file\x1b\\".into())
        );
        assert_eq!(
            of(TitleChange::Last, set),
            ("file host".into(), "\x1b]lfile host\x1b\\".into())
        );
        // `off` never stored it, so there is nothing to combine.
        assert_eq!(
            of(TitleChange::Off, set),
            ("file".into(), "\x1b]lfile\x1b\\".into())
        );

        // With no host title the window falls back to the file's under every
        // mode, and the *report* does not — `ahead` answers with a leading
        // space, which is upstream's `vtterm.c:2683` and not a slip here.
        let ask = b"\x1b[21t";
        assert_eq!(
            of(TitleChange::Overwrite, ask),
            ("file".into(), "\x1b]lfile\x1b\\".into())
        );
        assert_eq!(
            of(TitleChange::Ahead, ask),
            ("file".into(), "\x1b]l file\x1b\\".into())
        );
        assert_eq!(
            of(TitleChange::Last, ask),
            ("file".into(), "\x1b]lfile \x1b\\".into())
        );
    }

    /// `empty` and `ignore` are two different silences: one sends a reply with
    /// nothing in it, the other sends nothing.
    #[test]
    fn the_title_report_has_three_answers_and_off_discards_the_title() {
        let of = |report, input: &[u8]| {
            let mut vt = Vt::new(Config {
                cols: 20,
                rows: 2,
                title: "file".into(),
                title_report: report,
                ..Config::default()
            });
            vt.feed(input);
            String::from_utf8_lossy(vt.reply()).into_owned()
        };
        assert_eq!(
            of(TitleReport::Empty, b"\x1b[20t\x1b[21t"),
            "\x1b]L\x1b\\\x1b]l\x1b\\"
        );
        assert_eq!(of(TitleReport::Ignore, b"\x1b[20t\x1b[21t"), "");
        assert_eq!(of(TitleReport::Accept, b"\x1b[20t"), "\x1b]Lfile\x1b\\");

        // `off` is a discard, not a mask: the title stack goes with it, so the
        // pop at the end restores nothing.
        let mut vt = Vt::new(Config {
            accept_title_change: TitleChange::Off,
            ..Config::default()
        });
        vt.feed(b"\x1b]2;host\x1b\\\x1b[22;0t\x1b]2;second\x1b\\\x1b[23;0t");
        assert_eq!(vt.remote_title(), "");
    }

    /// `settitle` writes the *file's* half — the one `gettitle` reads.
    #[test]
    fn set_title_is_the_file_half_and_overwrite_clears_the_host() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"\x1b]2;host\x1b\\");
        vt.set_title("mine".into());
        assert_eq!(vt.config().title, "mine");
        assert_eq!(vt.remote_title(), "", "overwrite would keep hiding it");
        assert_eq!(vt.window_title(), "mine");

        // `last` shows both, so it has nothing to clear.
        let mut vt = Vt::new(Config {
            accept_title_change: TitleChange::Last,
            ..Config::default()
        });
        vt.feed(b"\x1b]2;host\x1b\\");
        vt.set_title("mine".into());
        assert_eq!(vt.window_title(), "mine host");
    }

    #[test]
    fn sgr_38_sets_a_256_colour_foreground_by_default() {
        let vt = run(b"\x1b[38;5;196mR", 20, 2);
        let cell = vt.grid().line(0)[0];
        assert_eq!(cell.attrs & ATTR2_FORE, ATTR2_FORE);
        assert_eq!(cell.fg, 196);
        assert_eq!(cell.attrs & ATTR_BLINK, 0);
    }

    #[test]
    fn sgr_38_leaks_its_arguments_when_xterm256_is_off() {
        // Not a bug, and the reason ColorFlags is modelled at all: with the bit
        // clear, `38` is ignored *without consuming* `5`, which then turns
        // blink on. vtterm.c:2239.
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 2,
            color_flags: ColorFlags {
                xterm256: false,
                ..ColorFlags::default()
            },
            ..Config::default()
        });
        vt.feed(b"\x1b[38;5;196mR");
        let cell = vt.grid().line(0)[0];
        assert_eq!(cell.attrs & ATTR_BLINK, ATTR_BLINK);
        assert_eq!(cell.attrs & ATTR2_COLOR_MASK, 0);
    }

    #[test]
    fn so_switches_to_line_drawing_and_si_switches_back() {
        let vt = run(b"\x0eqq\x0fqq", 12, 1);
        let attrs: Vec<u32> = (0..4)
            .map(|x| vt.grid().line(0)[x].attrs & ATTR_SPECIAL)
            .collect();
        assert_eq!(attrs, vec![ATTR_SPECIAL, ATTR_SPECIAL, 0, 0]);
    }

    #[test]
    fn esc_open_paren_zero_designates_dec_special_graphics() {
        let vt = run(b"\x1b(0qq\x1b(Bqq", 12, 1);
        let attrs: Vec<u32> = (0..4)
            .map(|x| vt.grid().line(0)[x].attrs & ATTR_SPECIAL)
            .collect();
        assert_eq!(attrs, vec![ATTR_SPECIAL, ATTR_SPECIAL, 0, 0]);
        // The byte is stored as-is; mapping it to U+2500 is the renderer's job,
        // because DecSpMappingDir defaults to "do not map".
        assert_eq!(vt.grid().line(0)[0].text[0], b'q' as u32);
    }

    #[test]
    fn c1_controls_fold_to_c0_on_a_vt100() {
        // U+008D is 0x0D once masked, so it is a carriage return and C lands on
        // top of A. On a VT220 the mask does not apply and it would be RI.
        let vt = run("A\u{84}B\u{8d}C".as_bytes(), 16, 3);
        assert_eq!(row(&vt, 0), "CB");
        assert_eq!(vt.grid().cursor.x, 1);
    }

    #[test]
    fn c1_controls_keep_their_meaning_above_vt100() {
        let mut vt = Vt::new(Config {
            cols: 16,
            rows: 3,
            term_id: TermId::Vt220,
            ..Config::default()
        });
        vt.feed("A\u{8d}B".as_bytes()); // U+008D = RI at level 2
        assert_eq!(row(&vt, 0), " B");
        assert_eq!(row(&vt, 1), "A");
    }

    #[test]
    fn a_split_c1_survives_the_chunk_boundary() {
        let mut vt = Vt::new(Config {
            cols: 16,
            rows: 3,
            ..Config::default()
        });
        vt.feed(b"A\xc2");
        vt.feed(b"\x8dC");
        assert_eq!(row(&vt, 0), "C");
    }

    #[test]
    fn alt_screen_hides_its_contents_and_restores_the_cursor() {
        let vt = run(b"main\x1b[?1049hALT\x1b[?1049l", 12, 3);
        assert_eq!(row(&vt, 0), "main");
        assert_eq!(vt.grid().cursor.x, 4);
    }

    #[test]
    fn re_entering_the_alt_screen_does_not_clobber_the_saved_main() {
        // A program that re-arms 1049 on every redraw would otherwise stash the
        // alternate screen over the main one and lose it.
        let vt = run(b"main\x1b[?1049hALT\x1b[?1049hMORE\x1b[?1049l", 12, 3);
        assert_eq!(row(&vt, 0), "main");
    }

    #[test]
    fn truecolor_resolves_through_the_palette() {
        let vt = run(b"\x1b[38;2;255;0;0mR", 16, 2);
        assert_eq!(vt.grid().line(0)[0].fg, 1);

        // The palette is terminal state, not a painter lookup: changing it
        // changes which index truecolor stores in the grid.
        let mut palette = *palette::default_palette();
        palette[42] = (1, 2, 3);
        let mut vt = Vt::new(Config {
            cols: 16,
            rows: 2,
            palette,
            ..Config::default()
        });
        vt.feed(b"\x1b[38;2;1;2;3mR");
        assert_eq!(vt.grid().line(0)[0].fg, 42);
    }

    #[test]
    fn decsc_restores_the_g_sets() {
        let vt = run(b"\x1b(0\x1b7\x1b(Bq\x1b8q", 12, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_SPECIAL, ATTR_SPECIAL);
    }

    #[test]
    fn crreceive_lf_treats_lf_as_crlf() {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 4,
            cr_receive: CrReceive::Lf,
            ..Config::default()
        });
        vt.feed(b"one\r\ntwo\r\nthree");
        assert_eq!(row(&vt, 0), "one");
        assert_eq!(row(&vt, 1), "two");
        assert_eq!(row(&vt, 2), "three");
    }

    #[test]
    fn decsca_survives_sgr_zero_and_only_decsca_clears_it() {
        // vtterm.c:2178 ORs the protect bit back in after SGR 0, so a program
        // that resets attributes between fields keeps its protected regions.
        let vt = run(b"\x1b[1\"q\x1b[0ma\x1b[2\"qb", 4, 1);
        assert_eq!(
            vt.grid().line(0)[0].attrs & tt_grid::ATTR2_PROTECT,
            tt_grid::ATTR2_PROTECT
        );
        assert_eq!(vt.grid().line(0)[1].attrs & tt_grid::ATTR2_PROTECT, 0);
    }

    #[test]
    fn decsel_skips_protected_cells_and_el_does_not() {
        let vt = run(b"\x1b[0\"qAA\x1b[1\"qBB\x1b[0\"qCC\x1b[1;1H\x1b[?2K", 6, 2);
        assert_eq!(row(&vt, 0), "  BB");
        let vt = run(b"\x1b[0\"qAA\x1b[1\"qBB\x1b[0\"qCC\x1b[1;1H\x1b[2K", 6, 2);
        assert_eq!(row(&vt, 0), "");
    }

    #[test]
    fn selective_erase_keeps_the_sgr_bits_where_a_plain_erase_drops_them() {
        // BuffSelectedEraseCharsInLine masks to AttrSgrMask instead of
        // painting the pen, so bold outlives DECSEL.
        let vt = run(b"\x1b[1mA\x1b[1;1H\x1b[?2K", 4, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_BOLD, ATTR_BOLD);
        let vt = run(b"\x1b[1mA\x1b[1;1H\x1b[2K", 4, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_BOLD, 0);
    }

    #[test]
    fn decrqss_answers_for_a_known_setting_and_refuses_an_unknown_one() {
        let vt = run(b"\x1b[2;5r\x1bP$qr\x1b\\", 20, 8);
        assert_eq!(vt.reply(), b"\x1bP1$r2;5r\x1b\\");
        let vt = run(b"\x1bP$qZ\x1b\\", 20, 4);
        assert_eq!(vt.reply(), b"\x1bP0$r\x1b\\");
    }

    #[test]
    fn decscusr_is_gated_and_sets_each_live_cursor_style() {
        // The shipped gate is off, so even a valid sequence leaves both
        // halves where the settings put them.
        let mut vt = Vt::new(Config {
            cursor_shape: 5,
            nonblinking_cursor: true,
            ..Config::default()
        });
        vt.feed(b"\x1b[1 q");
        assert_eq!(vt.config().cursor_shape, 5);
        assert!(vt.config().nonblinking_cursor);

        for (n, shape, nonblinking) in [
            (0, 1, false),
            (1, 1, false),
            (2, 1, true),
            (3, 3, false),
            (4, 3, true),
            (5, 5, false),
            (6, 5, true),
        ] {
            let mut vt = Vt::new(Config {
                cursor_ctrl_sequence: true,
                cursor_shape: 5,
                nonblinking_cursor: true,
                ..Config::default()
            });
            vt.feed(format!("\x1b[{n} q").as_bytes());
            assert_eq!(vt.config().cursor_shape, shape, "DECSCUSR {n}");
            assert_eq!(vt.config().nonblinking_cursor, nonblinking, "DECSCUSR {n}");

            // A value outside the table is ignored rather than reset.
            vt.feed(b"\x1b[7 q");
            assert_eq!(vt.config().cursor_shape, shape);
            assert_eq!(vt.config().nonblinking_cursor, nonblinking);
        }
    }

    /// `TF_INVALIDDECRPSS` flips the leading digit and nothing else, so the
    /// "I did not understand" reply still carries the value it was about to
    /// send. That is the whole of what makes it a test of the host's parser.
    #[test]
    fn the_invalid_decrqss_response_flips_the_digit_and_keeps_the_body() {
        let cfg = Config {
            cols: 20,
            rows: 4,
            invalid_decrqss: true,
            ..Config::default()
        };
        let mut vt = Vt::new(cfg.clone());
        vt.feed(b"\x1b[2;4r\x1bP$qr\x1b\\");
        assert_eq!(vt.reply(), b"\x1bP0$r2;4r\x1b\\");
        let mut vt = Vt::new(cfg);
        vt.feed(b"\x1bP$qZ\x1b\\");
        assert_eq!(vt.reply(), b"\x1bP1$r\x1b\\");
    }

    /// The tertiary DA, and the two things that make it more than a constant:
    /// it insists on a zero parameter, and DECSTUI can move what it answers —
    /// but only with `LockTUID` off, which is not how Tera Term ships.
    #[test]
    fn the_tertiary_da_answers_with_the_unit_id_and_decstui_is_locked() {
        let vt320 = |lock_uid| Config {
            cols: 20,
            rows: 4,
            term_id: TermId::Vt320,
            lock_uid,
            ..Config::default()
        };

        let mut vt = Vt::new(vt320(true));
        vt.feed(b"\x1b[=c");
        assert_eq!(vt.reply(), b"\x1bP!|FFFFFFFF\x1b\\");

        // A parameter that is not zero is not a request (`vtterm.c:CSEQ`).
        let mut vt = Vt::new(vt320(true));
        vt.feed(b"\x1b[=1c");
        assert_eq!(vt.reply(), b"");

        // Locked, which is the default: read and dropped.
        let mut vt = Vt::new(vt320(true));
        vt.feed(b"\x1bP!{deadBEEF\x1b\\\x1b[=c");
        assert_eq!(vt.reply(), b"\x1bP!|FFFFFFFF\x1b\\");

        // Unlocked, and upper-cased on the way in.
        let mut vt = Vt::new(vt320(false));
        vt.feed(b"\x1bP!{deadBEEF\x1b\\\x1b[=c");
        assert_eq!(vt.reply(), b"\x1bP!|DEADBEEF\x1b\\");

        // Nine digits is not eight: the old value stands rather than the
        // string being truncated to fit.
        let mut vt = Vt::new(vt320(false));
        vt.feed(b"\x1bP!{123456789\x1b\\\x1b[=c");
        assert_eq!(vt.reply(), b"\x1bP!|FFFFFFFF\x1b\\");
    }

    /// The four bits are independent, and the 8-bit spelling of HTS has to be
    /// refused where the byte still exists — before `rewrite_c1` folds it into
    /// `ESC H` and the two become indistinguishable.
    #[test]
    fn the_tab_stop_bits_gate_each_spelling_on_its_own() {
        let cfg = |flags| Config {
            cols: 40,
            rows: 2,
            term_id: TermId::Vt320,
            tab_stop_modify: flags,
            ..Config::default()
        };
        // `CSI 3 g` clears every stop, then HTS sets one at column 4 and a
        // tab from home lands on it. With the HTS refused there is no stop at
        // all, so the tab parks on the last column and arms the wrap instead —
        // which is `VTCompatTab` being off, and is what makes the refusal
        // visible on the screen rather than only in the stop table.
        let seven = b"\x1b[3g\x1b[5G\x1bH\x1b[H\tA".to_vec();
        let eight = "\x1b[3g\x1b[5G\u{88}\x1b[H\tA".as_bytes().to_vec();
        let hts7 = TabStopFlags(TabStopFlags::HTS7 | TabStopFlags::TBC3);
        let hts8 = TabStopFlags(TabStopFlags::HTS8 | TabStopFlags::TBC3);

        for (stream, taken, refused) in [(&seven, hts7, hts8), (&eight, hts8, hts7)] {
            let mut vt = Vt::new(cfg(TabStopFlags::ALL));
            vt.feed(stream);
            assert_eq!(row(&vt, 0), "    A");

            let mut vt = Vt::new(cfg(taken));
            vt.feed(stream);
            assert_eq!(row(&vt, 0), "    A");

            let mut vt = Vt::new(cfg(refused));
            vt.feed(stream);
            assert_eq!(row(&vt, 0), "");
            assert_eq!(row(&vt, 1), "A");
        }
    }

    /// `TabStopModifySequence`'s two whole-word spellings and its list, held
    /// against the writer that has to produce them again.
    #[test]
    fn the_tab_stop_list_round_trips_through_the_files_own_spelling() {
        let of = TabStopFlags::parse_ini;
        assert_eq!(of("on"), TabStopFlags::ALL);
        assert_eq!(of("ALL"), TabStopFlags::ALL);
        assert_eq!(of("off"), TabStopFlags::NONE);
        assert_eq!(of("none"), TabStopFlags::NONE);
        // A list starts from nothing, so naming one word disables the rest.
        assert_eq!(of("HTS7"), TabStopFlags(TabStopFlags::HTS7));
        assert_eq!(
            of("hts,tbc3"),
            TabStopFlags(TabStopFlags::HTS | TabStopFlags::TBC3)
        );
        // And a value with no word in it is a terminal that refuses all four,
        // which is the same trap `ISO2022ShiftFunction` carries.
        assert_eq!(of("HTS9"), TabStopFlags::NONE);
        assert_eq!(of(""), TabStopFlags::NONE);

        assert_eq!(TabStopFlags::ALL.to_ini(), "on");
        assert_eq!(TabStopFlags::NONE.to_ini(), "off");
        assert_eq!(TabStopFlags(TabStopFlags::HTS7).to_ini(), "HTS7");
        assert_eq!(
            TabStopFlags(TabStopFlags::HTS | TabStopFlags::TBC3).to_ini(),
            "HTS,TBC3"
        );
        for f in [
            TabStopFlags::HTS7,
            TabStopFlags::HTS8,
            TabStopFlags::TBC0,
            TabStopFlags::TBC3,
            TabStopFlags::HTS | TabStopFlags::TBC0,
        ] {
            let f = TabStopFlags(f);
            assert_eq!(TabStopFlags::parse_ini(&f.to_ini()), f);
        }
    }

    /// `TF_AUTOINVOKE`, and the two things about it the name does not say: an
    /// unrecognised designation still invokes, and the ISO-2022 shift gate
    /// does not apply.
    #[test]
    fn auto_invoke_folds_g0_into_gl_even_for_a_designation_that_did_nothing() {
        let cfg = |auto_invoke, iso2022_flags| Config {
            cols: 20,
            rows: 2,
            auto_invoke,
            iso2022_flags,
            ..Config::default()
        };
        // G1 is DEC special graphics and SO has invoked it, so `q` is a line.
        // `ESC ( B` designates G0 and, with the key on, puts G0 back in GL.
        let stream = b"\x1b)0\x0eq\x1b(Bq";
        let mut vt = Vt::new(cfg(true, ShiftFlags::ALL));
        vt.feed(stream);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_SPECIAL, ATTR_SPECIAL);
        assert_eq!(vt.grid().line(0)[1].attrs & ATTR_SPECIAL, 0);

        let mut vt = Vt::new(cfg(false, ShiftFlags::ALL));
        vt.feed(stream);
        assert_eq!(vt.grid().line(0)[1].attrs & ATTR_SPECIAL, ATTR_SPECIAL);

        // `ESC ( Z` designates nothing — the invoke is outside the switch.
        let mut vt = Vt::new(cfg(true, ShiftFlags::ALL));
        vt.feed(b"\x1b)0\x0eq\x1b(Zq");
        assert_eq!(vt.grid().line(0)[1].attrs & ATTR_SPECIAL, 0);

        // And it is not one of the shifts `ISO2022ShiftFunction` can switch
        // off, unlike the SO that got GL here in the first place.
        let mut vt = Vt::new(cfg(true, ShiftFlags::NONE));
        vt.feed(b"\x1b)0\x0eq\x1b(Bq");
        assert_eq!(vt.grid().line(0)[1].attrs & ATTR_SPECIAL, 0);
    }

    /// `ts.MaxOSCBufferSize`, and the semicolon rule the bound sits on top of.
    #[test]
    fn the_osc_string_keeps_its_semicolons_and_stops_at_the_buffer() {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 2,
            ..Config::default()
        });
        vt.feed(b"\x1b]2;a;b;c\x07");
        assert_eq!(vt.remote_title(), "a;b;c");

        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 2,
            max_osc_buffer: 8,
            ..Config::default()
        });
        vt.feed(b"\x1b]2;abcdefghij\x07");
        // Seven, not eight: upstream's test is `StrLen + 1 < StrBuffSize`.
        assert_eq!(vt.remote_title(), "abcdefg");
    }

    #[test]
    fn osc52_keeps_read_and_write_as_separate_permissions() {
        let requests = |access, notify| {
            let mut vt = Vt::new(Config {
                clipboard_access: access,
                notify_clipboard_access: notify,
                ..Config::default()
            });
            vt.feed(b"\x1b]52;c;?\x07\x1b]52;c;aGk=\x1b\\");
            vt.take_clipboard_requests()
        };

        assert_eq!(
            requests(ClipboardAccess::Off, true),
            vec![
                ClipboardRequest::ReadRejected,
                ClipboardRequest::WriteRejected
            ]
        );
        assert!(requests(ClipboardAccess::Off, false).is_empty());
        assert_eq!(
            requests(ClipboardAccess::Read, false),
            vec![ClipboardRequest::Read {
                selection: "c".into(),
                notify: false
            }]
        );
        assert_eq!(
            requests(ClipboardAccess::Write, true),
            vec![
                ClipboardRequest::ReadRejected,
                ClipboardRequest::Write {
                    text: "hi".into(),
                    notify: true
                }
            ]
        );
    }

    #[test]
    fn osc52_has_upstreams_selector_and_base64_rules() {
        let mut vt = Vt::new(Config {
            clipboard_access: ClipboardAccess::Write,
            ..Config::default()
        });
        // Padding is the invalid byte which ends upstream's permissive
        // decoder; whitespace is skipped, and an incomplete group is kept.
        vt.feed(b"\x1b]52;c;a Gk=ignored\x07");
        // E2 82 followed by `b`: the charset decoder emits one replacement
        // for each bad byte. The NUL in the next request ends Win32 text.
        vt.feed(b"\x1b]52;p;4oJi\x07\x1b]52;s;YQBi\x07");
        // `x` is not one of Pc's accepted selector bytes, so this is not an
        // OSC 52 action at all.
        vt.feed(b"\x1b]52;x;aGk=\x07");
        assert_eq!(
            vt.take_clipboard_requests(),
            vec![
                ClipboardRequest::Write {
                    text: "hi".into(),
                    notify: true
                },
                ClipboardRequest::Write {
                    text: "\u{fffd}\u{fffd}b".into(),
                    notify: true
                },
                ClipboardRequest::Write {
                    text: "a".into(),
                    notify: true
                }
            ]
        );
    }

    #[test]
    fn an_osc52_read_reply_is_utf8_base64_and_always_ends_in_st() {
        let mut vt = Vt::new(Config::default());
        assert!(vt.clipboard_reply("c", "hé"));
        assert_eq!(vt.take_reply(), b"\x1b]52;c;aMOp\x1b\\");

        // An empty clipboard is still text upstream and produces an empty
        // payload. A selector one byte beyond `hdr[20]` produces no reply.
        assert!(vt.clipboard_reply("", ""));
        assert_eq!(vt.take_reply(), b"\x1b]52;;\x1b\\");
        assert!(vt.clipboard_reply("cps0123456701", "ok"));
        vt.take_reply();
        assert!(!vt.clipboard_reply("cps01234567012", "too long"));
        assert!(!vt.clipboard_reply("c", "not\u{1}text"));
        assert!(vt.reply().is_empty());
    }

    #[test]
    fn the_termcap_query_answers_for_colours_and_nothing_else() {
        let mut vt = Vt::new(Config::default());
        // "Co", hex-encoded, and its long spelling.
        vt.feed(b"\x1bP+q436F\x1b\\");
        assert_eq!(vt.take_reply(), b"\x1bP1+r436f=323536\x1b\\".to_vec());
        vt.feed(b"\x1bP+q636f6c6f7273\x1b\\");
        assert_eq!(
            vt.take_reply(),
            b"\x1bP1+r636f6c6f7273=323536\x1b\\".to_vec()
        );

        // Anything else is `0+r` — and a known capability followed by an
        // unknown one keeps the separator upstream wrote before it found out.
        vt.feed(b"\x1bP+q7878\x1b\\");
        assert_eq!(vt.take_reply(), b"\x1bP0+r\x1b\\".to_vec());
        vt.feed(b"\x1bP+q436F;7878\x1b\\");
        assert_eq!(vt.take_reply(), b"\x1bP1+r436f=323536;\x1b\\".to_vec());
    }

    #[test]
    fn the_colour_count_is_the_flags_and_ansi_colour_switches_it_off_entirely() {
        let flags = |xterm256, aixterm16| ColorFlags {
            xterm256,
            aixterm16,
            pc_bold16: false,
            ansi_color: true,
        };
        let ask = |color_flags| {
            let mut vt = Vt::new(Config {
                color_flags,
                ..Config::default()
            });
            vt.feed(b"\x1bP+q436F\x1b\\");
            String::from_utf8(vt.take_reply()).expect("ascii")
        };
        assert!(ask(flags(true, false)).contains("=323536")); // 256
        assert!(ask(flags(false, true)).contains("=3136")); // 16
        assert!(ask(flags(false, false)).contains("=38")); // 8
                                                           // `EnableANSIColor` gates painting rather than parsing, so this reply
                                                           // is the only place in the protocol it shows.
        assert_eq!(
            ask(ColorFlags {
                ansi_color: false,
                ..flags(true, false)
            }),
            "\u{1b}P0+r\u{1b}\\"
        );
    }

    #[test]
    fn osc4_repaints_the_palette_and_moves_what_truecolor_resolves_to() {
        let mut vt = Vt::new(Config::default());
        // Nothing in the shipped table is near-black-but-not-black, so this
        // resolves to 0 until entry 42 is moved onto it exactly.
        vt.feed(b"\x1b[38;2;1;2;3m");
        assert_eq!(vt.grid().pen.fg, 0);
        vt.feed(b"\x1b]4;42;rgb:01/02/03\x1b\\\x1b[38;2;1;2;3m");
        assert_eq!(vt.colors().ansi[42], (1, 2, 3));
        assert_eq!(vt.grid().pen.fg, 42);
    }

    #[test]
    fn an_osc4_query_answers_with_the_requests_own_terminator() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"\x1b]4;1;#f00\x1b\\\x1b]4;1;?\x1b\\");
        // `#f00` is `<< 4`, so this reads back as f0f0 and not as ffff — the
        // one place upstream's short-form scaling is visible on the wire.
        assert_eq!(
            vt.take_reply(),
            b"\x1b]4;1;rgb:f0f0/0000/0000\x1b\\".to_vec()
        );
        vt.feed(b"\x1b]4;1;?\x07");
        assert_eq!(vt.take_reply(), b"\x1b]4;1;rgb:f0f0/0000/0000\x07".to_vec());
    }

    #[test]
    fn a_dynamic_colour_query_answers_from_the_settings_and_not_from_the_set() {
        // `DispSetColor` writes `vt->BGVTColor` and `DispGetColor` reads
        // `ts.VTColor`, so upstream cannot read back what it just set. The
        // paint changes; the answer does not.
        let mut vt = Vt::new(Config::default());
        vt.feed(b"\x1b]10;rgb:12/34/56\x1b\\");
        assert_eq!(vt.colors().normal[0], (0x12, 0x34, 0x56));
        vt.feed(b"\x1b]10;?\x1b\\");
        assert_eq!(
            vt.take_reply(),
            b"\x1b]10;rgb:0000/0000/0000\x1b\\".to_vec()
        );
    }

    #[test]
    fn osc10_walks_its_own_number_along_the_list() {
        let mut vt = Vt::new(Config::default());
        // Foreground, background, then the cursor — which has no arm, so the
        // third spec is parsed and dropped rather than repainting either of
        // the first two.
        vt.feed(b"\x1b]10;#010101;#020202;#030303\x1b\\");
        assert_eq!(vt.colors().normal, [(1, 1, 1), (2, 2, 2)]);
    }

    #[test]
    fn a_bad_number_abandons_the_whole_osc4_list() {
        let mut vt = Vt::new(Config::default());
        let before = vt.colors().ansi;
        vt.feed(b"\x1b]4;x;#ff0000;1;#00ff00\x1b\\");
        assert_eq!(vt.colors().ansi, before);
        // Two good pairs in one sequence, for contrast.
        vt.feed(b"\x1b]4;1;#ff0000;2;#00ff00\x1b\\");
        assert_eq!(vt.colors().ansi[1], (255, 0, 0));
        assert_eq!(vt.colors().ansi[2], (0, 255, 0));
    }

    #[test]
    fn osc104_with_an_empty_list_resets_one_colour_and_without_one_resets_all() {
        let mut vt = Vt::new(Config::default());
        let before = vt.colors().ansi;
        vt.feed(b"\x1b]4;0;#111111;1;#222222\x1b\\");
        // An empty parameter string is still a parameter string, and the
        // number in hand is zero.
        vt.feed(b"\x1b]104;\x1b\\");
        assert_eq!(vt.colors().ansi[0], before[0]);
        assert_eq!(vt.colors().ansi[1], (0x22, 0x22, 0x22));
        vt.feed(b"\x1b]104\x1b\\");
        assert_eq!(vt.colors().ansi, before);
    }

    #[test]
    fn a_non_digit_inside_an_osc104_list_resets_everything_after_it() {
        let mut vt = Vt::new(Config::default());
        let before = vt.colors().ansi;
        vt.feed(b"\x1b]4;1;#111111;2;#222222;3;#333333\x1b\\");
        // 1, then the poisoned number reaching the `;` as CS_UNSPEC — which
        // for 104 is the whole table — then 3, which is already back.
        vt.feed(b"\x1b]104;1;x;3\x1b\\");
        assert_eq!(vt.colors().ansi, before);
    }

    #[test]
    fn osc110_resets_its_own_colour_and_any_it_is_handed() {
        let mut vt = Vt::new(Config::default());
        let normal = vt.colors().normal;
        vt.feed(b"\x1b]10;#111111;#222222\x1b\\\x1b]110;11\x1b\\");
        assert_eq!(vt.colors().normal, normal);

        // Its own colour goes back whatever the list says, and a non-digit
        // ends the list here rather than poisoning it.
        vt.feed(b"\x1b]10;#111111;#222222\x1b\\\x1b]110;x\x1b\\");
        assert_eq!(vt.colors().normal, [normal[0], (0x22, 0x22, 0x22)]);
    }

    #[test]
    fn osc105_puts_back_three_of_the_four_colours_osc5_can_set() {
        let mut vt = Vt::new(Config::default());
        let config = Config::default();
        vt.feed(b"\x1b]5;0;#111111;1;#222222;2;#333333;3;#444444\x1b\\");
        assert_eq!(vt.colors().underline[0], (0x22, 0x22, 0x22));
        vt.feed(b"\x1b]105\x1b\\");
        assert_eq!(vt.colors().bold[0], config.color_bold[0]);
        assert_eq!(vt.colors().blink[0], config.color_blink[0]);
        assert_eq!(vt.colors().reverse[1], config.color_reverse[1]);
        // `CS_SP_ALL` does not name the underline, so `OSC 5;1` is a colour
        // the matching reset cannot undo.
        assert_eq!(vt.colors().underline[0], (0x22, 0x22, 0x22));
    }

    #[test]
    fn a_colour_spec_upstream_cannot_parse_changes_nothing_and_answers_nothing() {
        let mut vt = Vt::new(Config::default());
        let before = vt.colors().ansi;
        for spec in [
            &b"\x1b]4;1;rgbi:1.0/0.0/0.0\x1b\\"[..],
            &b"\x1b]4;1;CIELab:50/0/0\x1b\\"[..],
            &b"\x1b]4;1;RGB:ff/00/00\x1b\\"[..],
            &b"\x1b]4;1;red\x1b\\"[..],
        ] {
            vt.feed(spec);
        }
        assert_eq!(vt.colors().ansi, before);
        assert!(vt.reply().is_empty());
    }

    #[test]
    fn eight_colour_mode_permutes_the_index_an_osc4_names() {
        let mut vt = Vt::new(Config {
            color_flags: ColorFlags {
                xterm256: false,
                aixterm16: false,
                pc_bold16: false,
                ..ColorFlags::default()
            },
            ..Config::default()
        });
        // With every full-colour bit off the wire's index is the legacy one,
        // so 1 — "red" in the old ordering — is drawing index 9.
        vt.feed(b"\x1b]4;1;#010203\x1b\\");
        assert_eq!(vt.colors().ansi[9], (1, 2, 3));
    }

    /// A broken multi-byte sequence is one replacement character **per byte**,
    /// which is Tera Term's decoder and not `vte`'s.
    #[test]
    fn a_broken_utf8_sequence_is_one_replacement_per_byte() {
        let vt = run("a\u{fffd}".as_bytes(), 10, 1); // sanity: a real one is one
        assert_eq!(row(&vt, 0), "a\u{fffd}");
        let vt = run(b"a\xe2\x82b", 10, 1);
        assert_eq!(row(&vt, 0), "a\u{fffd}\u{fffd}b");
        let vt = run(b"a\xf0\x9f\x98b", 10, 1);
        assert_eq!(row(&vt, 0), "a\u{fffd}\u{fffd}\u{fffd}b");
        // One byte in, one out — the case that already agreed.
        let vt = run(b"a\xc3b", 10, 1);
        assert_eq!(row(&vt, 0), "a\u{fffd}b");
    }

    #[test]
    fn decstr_reloads_the_saved_cursor_with_the_origin() {
        // SoftReset saves the cursor at 0,0 rather than where it is, so a
        // DECRC straight afterwards homes it. Surprising, and upstream's.
        let vt = run(b"\x1b[3;3Hx\x1b[!p\x1b8X", 12, 4);
        assert_eq!(row(&vt, 0), "X");
        assert_eq!(vt.grid().margins(), (0, 11));
    }

    #[test]
    fn decslrm_moves_the_wrap_point_and_the_carriage_return() {
        // DECSLRM needs DECLRMM on first; without it `CSI 4;10s` is SCP.
        let vt = run(b"\x1b[?69h\x1b[4;10s\x1b[1;4HABCDEFGHIJ", 16, 3);
        assert_eq!(row(&vt, 0), "   ABCDEFG");
        assert_eq!(row(&vt, 1), "   HIJ");
    }

    #[test]
    fn decslrm_is_scp_while_declrmm_is_off() {
        let vt = run(b"\x1b[2;3Hx\x1b[5;5s\x1b[1;1Hy\x1b[uz", 16, 6);
        assert_eq!(vt.grid().margins(), (0, 15));
        // The cursor came back to where `CSI 5;5s` saved it, after the `y`.
        assert_eq!(row(&vt, 1), "  xz");
    }

    #[test]
    fn a_tab_at_the_end_of_a_line_wraps_before_it_tabs() {
        // `vtterm.c:Tab()` takes the pending wrap first. Getting this wrong
        // leaves the tab on the old row and the next character a row too high.
        let vt = run(b"\x1b[1;1H\t\t\tX", 16, 3);
        assert_eq!(row(&vt, 0), "");
        assert_eq!(row(&vt, 1), "        X");
    }

    #[test]
    fn scroll_region_is_set_and_homes_the_cursor() {
        // DECSTBM homes to the screen origin, not the region top, when origin
        // mode is off — vtterm.c:2473.
        let vt = run(b"\x1b[2;4r", 10, 6);
        assert_eq!(vt.grid().scroll_region(), (1, 3));
        assert_eq!((vt.grid().cursor.x, vt.grid().cursor.y), (0, 0));
    }

    /// Drives a stream, then a sequence of mouse events, and returns the reply.
    fn mouse(stream: &[u8], events: &[(MouseEvent, u8, i32, i32)], mods: Modifiers) -> Vec<u8> {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            ..Config::default()
        });
        vt.feed(stream);
        let _ = vt.take_reply();
        for &(e, b, x, y) in events {
            vt.mouse_event(e, b, x, y, mods);
        }
        vt.take_reply()
    }

    #[test]
    fn mouse_tracking_is_off_until_asked_for() {
        let out = mouse(b"", &[(MouseEvent::Press, 0, 24, 80)], Modifiers::default());
        assert!(out.is_empty());
    }

    #[test]
    fn ctrl_suppresses_the_report_so_the_user_can_still_select() {
        // `ts.DisableMouseTrackingByCtrl` defaults on (ttset.c:1591).
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let out = mouse(b"\x1b[?1000h", &[(MouseEvent::Press, 0, 24, 80)], ctrl);
        assert!(out.is_empty());

        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            disable_mouse_tracking_by_ctrl: false,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        vt.mouse_event(MouseEvent::Press, 0, 24, 80, ctrl);
        assert_eq!(vt.take_reply(), b"\x1b[<16;4;6M");
    }

    #[test]
    fn the_report_position_follows_the_cell_size() {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        vt.set_cell_pixels(10, 20);
        vt.mouse_event(MouseEvent::Press, 0, 25, 41, Modifiers::default());
        // 25/10 = 2, 41/20 = 2, both one-based in the report.
        assert_eq!(vt.take_reply(), b"\x1b[<0;3;3M");
    }

    #[test]
    fn mouse_state_is_visible_to_the_frontend() {
        let mut vt = Vt::new(Config::default());
        assert_eq!(vt.mouse_tracking(), Tracking::None);
        vt.feed(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h");
        assert_eq!(vt.mouse_tracking(), Tracking::AllEvent);
        assert_eq!(vt.mouse_encoding(), Encoding::Sgr);
        assert!(vt.focus_reporting());
    }

    #[test]
    fn the_setting_can_switch_mouse_tracking_off_entirely() {
        let mut vt = Vt::new(Config {
            mouse_tracking_enabled: false,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h\x1b[?1004h");
        assert_eq!(vt.mouse_tracking(), Tracking::None);
        assert!(!vt.focus_reporting());
        vt.mouse_event(MouseEvent::Press, 0, 24, 80, Modifiers::default());
        vt.focus_event(true);
        assert!(vt.reply().is_empty());
    }

    fn checksummer(cols: usize, rows: usize) -> Vt {
        Vt::new(Config {
            cols,
            rows,
            decrqcra: true,
            ..Config::default()
        })
    }

    #[test]
    fn decrqcra_is_silent_unless_it_is_asked_for() {
        // The default, and the faithful one: `vtterm.c` has no `CSI * y`.
        let vt = run(b"ab\x1b[42;0;1;1;1;2*y", 10, 2);
        assert!(vt.reply().is_empty());
    }

    #[test]
    fn decrqcra_sums_the_characters_in_the_rectangle() {
        let mut vt = checksummer(10, 2);
        vt.feed(b"ab\x1b[42;0;1;1;1;2*y");
        // 'a' + 'b' = 0x61 + 0x62.
        assert_eq!(vt.take_reply(), b"\x1bP42!~00C3\x1b\\");
    }

    #[test]
    fn decrqcra_reads_one_cell_as_its_own_character() {
        // The shape the whole conformance suite is built on: a single-cell
        // request must come back as exactly that character's code.
        let mut vt = checksummer(10, 2);
        vt.feed(b"Hi\x1b[7;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP7!~0048\x1b\\");
    }

    #[test]
    fn decrqcra_counts_an_erased_cell_as_a_space() {
        // xterm from patch #334, and the only thing our grid can say: an
        // erase leaves a space behind, not a distinguishable "empty".
        let mut vt = checksummer(10, 2);
        vt.feed(b"X\x1b[2J\x1b[1;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP1!~0020\x1b\\");
    }

    #[test]
    fn decrqcra_answers_an_inverted_rectangle_with_zero() {
        // The rectangular *operations* return silently on one. A request
        // cannot: the far end is waiting.
        let mut vt = checksummer(10, 4);
        vt.feed(b"\x1b[9;0;3;1;2;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP9!~0000\x1b\\");
    }

    #[test]
    fn decrqcra_defaults_the_rectangle_to_the_whole_screen() {
        let mut vt = checksummer(4, 2);
        vt.feed(b"ab\x1b[5*y");
        // Two written cells plus six spaces.
        let want = 0x61 + 0x62 + 6 * 0x20;
        assert_eq!(
            vt.take_reply(),
            format!("\x1bP5!~{want:04X}\x1b\\").as_bytes()
        );
    }

    #[test]
    fn decrqcra_follows_the_reply_encoding_the_terminal_was_told_to_use() {
        let mut vt = Vt::new(Config {
            cols: 10,
            rows: 2,
            decrqcra: true,
            term_id: TermId::Vt320,
            ..Config::default()
        });
        // S8C1T, so the reply is wrapped in 8-bit DCS/ST rather than ESC P … ESC \.
        vt.feed(b"a\x1b G\x1b[3;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x903!~0061\x9c");
    }

    #[test]
    fn the_normal_encoding_saturates_rather_than_wrapping() {
        // MOUSE_POS_LIMIT is 223; a wider terminal simply stops counting.
        let mut vt = Vt::new(Config {
            cols: 300,
            rows: 200,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h");
        let _ = vt.take_reply();
        vt.mouse_event(MouseEvent::Press, 0, 2392, 3184, Modifiers::default());
        // Column 300 saturates at 223 and is reported as 255; row 200 is
        // under the limit and comes through as 232.
        assert_eq!(vt.take_reply(), vec![0x1b, b'[', b'M', 32, 255, 232]);
    }

    /// Applying settings overwrites the modes the host set, because upstream
    /// keeps no second copy of them — the setting and the mode are the same
    /// variable there.
    #[test]
    fn applying_settings_overwrites_what_the_host_set() {
        let mut vt = Vt::new(Config::default());
        // SRM, DECBKM and LNM, each writing a `ts` field upstream.
        vt.feed(b"\x1b[?67l\x1b[12l\x1b[20h");
        assert!(vt.local_echo());
        assert!(!vt.backspace_sends_bs());
        assert_eq!(vt.state.modes.cr_send, CrSend::CrLf);
        // LFMode is upstream's own variable rather than a view of `ts`.
        assert!(vt.newline_mode());

        vt.set_config(Config::default());
        assert!(!vt.local_echo());
        assert!(vt.backspace_sends_bs());
        assert_eq!(vt.state.modes.cr_send, CrSend::Cr);
        // ...so it survives, and that asymmetry is `SetupTerm`'s.
        assert!(vt.newline_mode());
    }

    #[test]
    fn applying_settings_resizes_and_re_derives_the_vt_level() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"hello");
        vt.set_config(Config {
            cols: 100,
            rows: 40,
            term_id: TermId::Vt320,
            scrollback_max: 4,
            ..Config::default()
        });
        assert_eq!((vt.grid().cols(), vt.grid().rows()), (100, 40));
        assert_eq!(vt.grid().scrollback_max(), 4);
        assert_eq!(row(&vt, 0), "hello");
        vt.feed(b"\x1b[c");
        assert_eq!(vt.take_reply(), b"\x1b[?63;1;2;6;7;8;9c");
    }

    // --- the macro tap ---------------------------------------------------

    /// Feed a stream to a terminal with a macro listening and return what the
    /// macro would have read.
    fn tapped(input: &[u8], cols: usize, rows: usize) -> Vec<u8> {
        let mut vt = Vt::new(Config {
            cols,
            rows,
            ..Config::default()
        });
        vt.set_macro_tap_enabled(true);
        vt.feed(input);
        vt.take_macro_bytes()
    }

    fn tapped_str(input: &[u8]) -> String {
        String::from_utf8(tapped(input, 20, 5)).unwrap()
    }

    /// The headline: escape sequences never reach a macro, because the parser
    /// consumed them. A `wait 'ESC['` cannot match, ever.
    #[test]
    fn a_macro_reads_the_text_and_not_the_wire() {
        assert_eq!(tapped_str(b"\x1b[31mred\x1b[m"), "red");
        assert_eq!(tapped_str(b"a\x1b[2Jb"), "ab");
        // A character that was printed and then erased is in the stream
        // anyway, because it was printed once.
        assert_eq!(tapped_str(b"secret\x1b[6D\x1b[K"), "secret");
    }

    /// The trap `AGENTS.md` records from the other end: a line reaches a macro
    /// with its CR still on it, which is why a `waitregex` ending in `$` never
    /// matches one.
    #[test]
    fn a_crlf_line_keeps_its_cr_and_a_lone_cr_is_dropped() {
        assert_eq!(tapped_str(b"abc\r\ndef\r\n"), "abc\r\ndef\r\n");
        // A bare LF is a bare LF — `CheckEOLCheckLog` passes it through as a
        // character rather than turning it into an EOL.
        assert_eq!(tapped_str(b"abc\ndef"), "abc\ndef");
        // And a CR with no LF after it vanishes, so an overwrite reaches the
        // macro as the two texts run together while the screen shows only the
        // second.
        assert_eq!(tapped_str(b"abc\rdef"), "abcdef");
        let mut vt = run(b"abc\rdef", 20, 5);
        vt.feed(b"");
        assert_eq!(row(&vt, 0).trim_end(), "def");
    }

    /// `CarriageReturn` and `LineFeed` are what tap, so `ts.CRReceive` changes
    /// what a macro sees and not only what the screen does.
    #[test]
    fn cr_receive_reaches_the_tap() {
        fn with(cr: CrReceive, input: &[u8]) -> String {
            let mut vt = Vt::new(Config {
                cr_receive: cr,
                ..Config::default()
            });
            vt.set_macro_tap_enabled(true);
            vt.feed(input);
            String::from_utf8(vt.take_macro_bytes()).unwrap()
        }
        // The default: a CR is a carriage return and nothing more, so on its
        // own it is held and dropped.
        assert_eq!(with(CrReceive::Cr, b"a\rb"), "ab");
        // Told the far end sends CR alone, a CR is a CR *and* a line feed —
        // and the pair reaches the tap as one EOL.
        assert_eq!(with(CrReceive::CrLf, b"a\rb"), "a\r\nb");
        // Told it sends LF alone, an LF is a CR and an LF.
        assert_eq!(with(CrReceive::Lf, b"a\nb"), "a\r\nb");
    }

    /// BS, HT and the wrap are the three that a tap written from `DDEPut1`
    /// alone would miss — they are emitted by the functions that execute the
    /// control, not by the sink.
    #[test]
    fn the_controls_that_moved_the_cursor_are_in_the_stream() {
        assert_eq!(tapped_str(b"ab\x08c"), "ab\x08c");
        // A backspace that could not move taps nothing.
        assert_eq!(tapped_str(b"\x08a"), "a");
        assert_eq!(tapped_str(b"a\tb"), "a\tb");
    }

    /// `LogTypePlainText` is one byte, and the byte is that one — so the
    /// setting named after the log also decides what a macro's `wait` sees.
    #[test]
    fn plain_text_drops_the_backspace_and_nothing_else() {
        let mut vt = Vt::new(Config {
            log_plain_text: true,
            ..Config::default()
        });
        vt.set_macro_tap_enabled(true);
        vt.feed(b"ab\x08c\td\r\n");
        assert_eq!(
            String::from_utf8(vt.take_macro_bytes()).unwrap(),
            "abc\td\r\n",
            "the tab and the line ending stay; only the BS goes"
        );
        // The cursor still moved: this is about the tap, not about the grid.
        assert_eq!(row(&vt, 0).trim_end(), "ac      d");
    }

    /// A wrapped line reaches a macro as two lines, which is what makes
    /// `waitln` usable against a host that does not know the terminal's width.
    #[test]
    fn an_automatic_wrap_is_a_line_break_to_a_macro() {
        assert_eq!(tapped(b"abcde", 4, 5), b"abcd\r\ne");
        // The other wrap: a double-width glyph that will not fit in the last
        // column parks a space and breaks the line before it. **The space is
        // not in the stream** — upstream parks it with `BuffPutUnicode`
        // (`vtterm.c:896`), which is the buffer's write path and not `PutU32`,
        // so it never reaches `OutputLogUTF32`. A macro's copy of that line is
        // one column narrower than the screen's.
        assert_eq!(
            String::from_utf8(tapped("abc\u{4f60}".as_bytes(), 4, 5)).unwrap(),
            "abc\r\n\u{4f60}"
        );
        // With autowrap off there is no break, and upstream taps nothing.
        assert_eq!(tapped(b"\x1b[?7labcde", 4, 5), b"abcde");
    }

    /// ...unless `ts.EnableContinuedLineCopy` is on, which is the whole of
    /// what upstream's `logFlag` argument decides. A break the *host* sent
    /// still reaches the tap; only the one the wrap invented is dropped.
    #[test]
    fn continued_line_copy_keeps_the_wrap_out_of_the_tap() {
        fn with(input: &[u8], cols: usize) -> String {
            let mut vt = Vt::new(Config {
                cols,
                rows: 5,
                continued_line_copy: true,
                ..Config::default()
            });
            vt.set_macro_tap_enabled(true);
            vt.feed(input);
            String::from_utf8(vt.take_macro_bytes()).unwrap()
        }
        assert_eq!(with(b"abcde", 4), "abcde", "one line, as the host sent it");
        assert_eq!(with("abc\u{4f60}".as_bytes(), 4), "abc\u{4f60}");
        // The tab's wrap is the second of the two `logFlag`-FALSE sites.
        assert_eq!(with(b"abcd\te", 4), "abcd\te");
        // And a CR LF off the wire is untouched, which is the distinction the
        // argument exists to make.
        assert_eq!(with(b"ab\r\ncd", 20), "ab\r\ncd");
    }

    /// The other half of the same setting, which the frontend reads: a row
    /// that a wrap landed on says so, and one a line feed landed on does not.
    #[test]
    fn a_wrapped_row_is_marked_continued() {
        let vt = run(b"abcde", 4, 5);
        assert!(!vt.grid().line_continued(0));
        assert!(vt.grid().line_continued(1), "the row `e` landed on");
        // The last cell of the row that was left carries it too, which is what
        // a selection walking backwards off column 0 looks for.
        assert_ne!(vt.grid().line(0)[3].attrs & tt_grid::ATTR_LINE_CONTINUED, 0);

        // A host's own line break is not a continuation.
        let vt = run(b"ab\r\ncd", 20, 5);
        assert!(!vt.grid().line_continued(1));

        // ...and it clears the mark left by an earlier wrap, so a row written
        // over does not go on claiming to continue anything.
        let vt = run(b"abcde\x1b[H\r\n\r\nxy", 4, 5);
        assert!(!vt.grid().line_continued(1));
    }

    /// Nothing is collected while no macro is linked, and unlinking throws
    /// away what was collected — `DDEFreeBuf`.
    #[test]
    fn the_tap_costs_nothing_and_keeps_nothing_when_it_is_off() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"before");
        assert!(!vt.macro_tap_enabled());
        assert!(vt.take_macro_bytes().is_empty());
        vt.set_macro_tap_enabled(true);
        vt.feed(b"after");
        assert!(vt.macro_tap_enabled());
        vt.set_macro_tap_enabled(false);
        vt.set_macro_tap_enabled(true);
        assert!(vt.take_macro_bytes().is_empty());
    }

    /// The two taps are independent: draining one leaves the other alone, and
    /// they do not agree about newlines on purpose.
    #[test]
    fn the_log_tap_and_the_macro_tap_do_not_share_a_buffer() {
        let mut vt = Vt::new(Config::default());
        vt.set_log_text_enabled(true);
        vt.set_macro_tap_enabled(true);
        vt.feed(b"a\tb\r\n");
        assert_eq!(vt.take_macro_bytes(), b"a\tb\r\n");
        // The log's is `\n` with no tab, which is this port's own choice — see
        // `LogOptions::crlf` and the note on `MacroTap`.
        assert_eq!(vt.take_log_text(), "ab\n");
    }

    /// Wide characters go out as UTF-8, which is what `DDEPut1U32` does with
    /// `UTF32ToUTF8`.
    #[test]
    fn the_tap_is_utf8_whatever_the_terminal_decoded() {
        assert_eq!(tapped_str("héllo".as_bytes()), "héllo");
        assert_eq!(tapped_str("日本".as_bytes()), "日本");
    }

    #[test]
    fn enq_answers_with_nothing_by_default() {
        // `Answerback=` is empty out of the box, so the host asking who is
        // there gets silence rather than a terminal name.
        let vt = run(b"\x05", 20, 2);
        assert!(vt.reply().is_empty());
    }

    #[test]
    fn enq_sends_the_answerback_verbatim() {
        let mut vt = Vt::new(Config {
            // What `Answerback=sterna$0D` decodes to.
            answerback: b"sterna\r".to_vec(),
            ..Config::default()
        });
        vt.feed(b"\x05");
        assert_eq!(vt.reply(), b"sterna\r");
        // It is not a one-shot: every ENQ answers.
        vt.feed(b"\x05");
        assert_eq!(vt.reply(), b"sterna\rsterna\r");
        // And nothing of it reaches the screen.
        assert_eq!(row(&vt, 0), "");
    }

    /// `CommBinaryOut`, not the text path — so a `CRSend` of CRLF does not turn
    /// the answerback's own CR into two bytes.
    #[test]
    fn the_answerback_is_binary_whatever_cr_send_says() {
        let mut vt = Vt::new(Config {
            answerback: b"\r".to_vec(),
            cr_send: CrSend::CrLf,
            ..Config::default()
        });
        vt.feed(b"\x05");
        assert_eq!(vt.reply(), b"\r");
    }

    #[test]
    fn bel_asks_for_a_bell_and_prints_nothing() {
        let mut vt = run(b"a\x07b", 20, 2);
        assert_eq!(vt.take_bells().count, 1);
        assert_eq!(row(&vt, 0), "ab");
        // Drained, so a second call sees none.
        assert_eq!(vt.take_bells().count, 0);
    }

    /// Every BEL is counted. The engine does not thin a burst out — that is the
    /// governor's job, one layer up, and it needs each request to step it.
    #[test]
    fn a_burst_of_bels_is_a_burst_of_requests() {
        let mut vt = run(b"\x07\x07\x07\x07\x07", 20, 2);
        assert_eq!(vt.take_bells().count, 5);
    }

    #[test]
    fn the_bell_being_off_stops_bel_before_the_governor() {
        // `vtterm.c:1077` tests the setting before calling `RingBell`, so with
        // it off there is not even a request to count.
        let mut vt = Vt::new(Config {
            beep: Beep::Off,
            ..Config::default()
        });
        vt.feed(b"\x07\x07");
        assert_eq!(vt.take_bells().count, 0);
    }

    /// `ESC g` is screen's visual bell, and it does **not** consult the
    /// setting — `vtterm.c:1561` calls `RingBell(IdBeepVisual)` outright, and
    /// `RingBell` then ignores the argument it was given. So with the bell off
    /// it still steps the governor and still makes no sound, and with the bell
    /// on it is an ordinary beep rather than a flash.
    #[test]
    fn esc_g_asks_for_a_bell_whatever_the_setting_says() {
        let mut vt = run(b"\x1bg", 20, 2);
        assert_eq!(vt.take_bells().count, 1);
        assert_eq!(row(&vt, 0), "");

        let mut off = Vt::new(Config {
            beep: Beep::Off,
            ..Config::default()
        });
        off.feed(b"\x1bg");
        assert_eq!(off.take_bells().count, 1);
    }

    /// RIS puts the governor's clocks back (`vtterm.c:348`), which is the one
    /// part of `ResetTerminal` the engine cannot do itself. A soft reset does
    /// not — it is a much shorter list.
    #[test]
    fn ris_asks_for_the_governor_to_be_reset_and_decstr_does_not() {
        let mut vt = run(b"\x1bc", 20, 2);
        assert_eq!(
            vt.take_bells(),
            BellRequests {
                reset: true,
                count: 0
            }
        );
        vt.feed(b"\x1b[!p\x07");
        assert_eq!(
            vt.take_bells(),
            BellRequests {
                reset: false,
                count: 1
            }
        );
    }

    // ---- XTWINOPS -----------------------------------------------------

    /// A frontend that has said nothing leaves the notional window, which is
    /// exactly the text area on a 1920x1080 work area with an 8x16 cell. Every
    /// one of these numbers is what the oracle's stubs answer, so that
    /// `esctest/run_diff.sh` compares the *logic* rather than a desktop.
    #[test]
    fn the_window_reports_answer_from_a_notional_window() {
        let mut vt = run(
            b"\x1b[11t\x1b[13t\x1b[14t\x1b[15t\x1b[16t\x1b[18t\x1b[19t",
            80,
            24,
        );
        assert_eq!(
            String::from_utf8(vt.take_reply()).unwrap(),
            concat!(
                "\x1b[1t",         // not iconified
                "\x1b[3;0;0t",     // position, x then y
                "\x1b[4;384;640t", // text area, height then width
                "\x1b[5;1080;1920t",
                "\x1b[6;16;8t",  // the cell
                "\x1b[8;24;80t", // the grid
                "\x1b[9;67;240t",
            )
        );
    }

    /// `CSI 13 t` reports x then y and `CSI 14 t` reports height then width,
    /// which is upstream's order and xterm's and looks like a typo in both.
    #[test]
    fn the_frontends_metrics_reach_the_reports() {
        let mut vt = Vt::new(Config {
            cols: 80,
            rows: 24,
            ..Config::default()
        });
        vt.set_window_metrics(WindowMetrics {
            pos: (100, 50),
            client_pos: (108, 86),
            size: Some((660, 420)),
            client_size: Some((640, 384)),
            cell: (8, 16),
            screen: (2560, 1440),
            iconified: true,
        });
        vt.feed(b"\x1b[11t\x1b[13t\x1b[13;2t\x1b[14t\x1b[14;2t\x1b[19t");
        assert_eq!(
            String::from_utf8(vt.take_reply()).unwrap(),
            concat!(
                "\x1b[2t",         // iconified
                "\x1b[3;100;50t",  // the frame
                "\x1b[3;108;86t",  // the text area
                "\x1b[4;384;640t", // the text area, height first
                "\x1b[4;420;660t", // the frame
                "\x1b[9;87;317t",  // (2560-20)/8 by (1440-36)/16
            )
        );
    }

    /// An unrecognised sub-parameter answers nothing at all — the `default:
    /// return` in `vtterm.c`'s cases 13 and 14. Silence, not an error and not
    /// a fallback to the plain form.
    #[test]
    fn an_unknown_sub_parameter_is_answered_with_silence() {
        let mut vt = run(b"\x1b[13;3t\x1b[14;9t\x1b[12t\x1b[17t\x1b[24t", 80, 24);
        assert!(vt.take_reply().is_empty());
        assert!(vt.take_window_requests().is_empty());
    }

    #[test]
    fn the_actions_queue_for_the_frontend() {
        let mut vt = run(
            b"\x1b[1t\x1b[2t\x1b[3;120;40t\x1b[4;480;0t\x1b[5t\x1b[6t\x1b[7t\
              \x1b[9;1t\x1b[9;0t\x1b[10;2t\x1b[9;2t",
            80,
            24,
        );
        assert_eq!(
            vt.take_window_requests(),
            vec![
                WindowRequest::Deiconify,
                WindowRequest::Iconify,
                WindowRequest::Move(120, 40),
                // Height first on the wire, and the zero width means "leave
                // that axis where it is" rather than "zero pixels wide".
                WindowRequest::ResizePixels {
                    width: 0,
                    height: 480
                },
                WindowRequest::Raise,
                WindowRequest::Lower,
                WindowRequest::Refresh,
                WindowRequest::Maximize,
                WindowRequest::Unmaximize,
                WindowRequest::ToggleMaximize,
                // `CSI 9;2 t` is not a toggle: case 9 has no arm for it.
            ]
        );
        assert!(vt.take_window_requests().is_empty(), "the queue drains");
    }

    // --- the printer ------------------------------------------------------

    fn printing(cols: usize, rows: usize) -> Vt {
        Vt::new(Config {
            cols,
            rows,
            printer_ctrl_sequence: true,
            ..Config::default()
        })
    }

    /// `PrinterCtrlSequence` ships off, and with it off four of the five arms
    /// do nothing at all. Missing this makes every other printer test pass
    /// against a terminal that would print nothing for a real user.
    #[test]
    fn the_printer_gate_ships_off_and_covers_four_of_the_five() {
        let mut vt = run(b"\x1b[0i\x1b[5i\x1b[?1i\x1b[?5i", 20, 3);
        assert!(vt.take_printer_events().is_empty());
        assert!(!vt.printer_controller());
        assert!(!vt.auto_print());
        // ...and the fifth is reachable whatever the setting says, so a host
        // can always stop a terminal printing every line.
        let mut on = printing(20, 3);
        on.feed(b"\x1b[?5i");
        assert!(on.auto_print());
        on.set_config(Config {
            cols: 20,
            rows: 3,
            printer_ctrl_sequence: false,
            ..Config::default()
        });
        on.take_printer_events();
        on.feed(b"\x1b[?4i");
        assert!(!on.auto_print());
        assert_eq!(on.take_printer_events(), vec![PrinterEvent::Close]);
    }

    /// The whole shape of a controller-mode job: the text still reaches the
    /// screen, the controls reach the printer instead of being obeyed, and the
    /// two arrive in the order they were sent.
    #[test]
    fn controller_mode_prints_the_controls_and_displays_the_text() {
        let mut vt = printing(20, 3);
        vt.feed(b"\x1b[5iA\r\nB\x1b[2J\x1b[4iC");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                PrinterEvent::Open,
                // `A`, then the CR/LF pair the EOL check folds, then `B`, then
                // the erase upstream never performs.
                PrinterEvent::Write("A\r\nB\u{1b}[2J".into()),
                PrinterEvent::Close,
            ]
        );
        // Nothing was cleared and nothing moved: the row is still `ABC`,
        // because neither the CR, the LF nor the `ED 2` was executed.
        assert_eq!(row(&vt, 0).trim_end(), "ABC");
        assert!(!vt.printer_controller());
    }

    /// Auto print is the other mode, and it dumps the *grid* rather than the
    /// stream — so the line that reaches the printer is what was displayed,
    /// overwrites and all, with its trailing spaces trimmed.
    #[test]
    fn auto_print_dumps_each_finished_line_from_the_screen() {
        let mut vt = printing(20, 3);
        vt.feed(b"\x1b[?5ihello\rH\r\nworld\n");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                PrinterEvent::Open,
                // Two lines, one event: consecutive writes coalesce, since the
                // order that matters is the one against `Open` and `Close`.
                PrinterEvent::Write("Hello\r\nworld\r\n".into()),
            ]
        );
        // The job stays open: only `CSI ? 4 i` closes it.
        vt.feed(b"\x1b[?4i");
        assert_eq!(vt.take_printer_events(), vec![PrinterEvent::Close]);
    }

    /// `LineFeed`'s byte argument is the whole of the difference: LF, VT and FF
    /// dump the line and IND and NEL, which pass a zero, do not.
    #[test]
    fn ind_and_nel_scroll_a_line_the_printer_never_sees() {
        let mut vt = printing(20, 4);
        vt.feed(b"\x1b[?5ione\x1bDtwo\x1bEthree\n");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                PrinterEvent::Open,
                // Only the last line, and only because of its LF. `ESC D`
                // kept its column and `ESC E` went to column zero, so the two
                // lines the printer never saw are also the reason this one
                // starts where it does.
                PrinterEvent::Write("three\r\n".into()),
            ]
        );
    }

    /// `CSI ? 1 i` opens and closes a job of its own — unless auto print is
    /// already holding one, which is the same shared-job rule `CSI 5 i` follows.
    #[test]
    fn print_this_line_is_a_job_by_itself() {
        let mut vt = printing(20, 3);
        vt.feed(b"line\x1b[?1i");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                PrinterEvent::Open,
                PrinterEvent::Write("line\r\n".into()),
                PrinterEvent::Close,
            ]
        );
    }

    /// DECPEX picks the rectangle `CSI 0 i` prints, and the request crosses the
    /// seam rather than being answered here: upstream renders it graphically.
    #[test]
    fn print_screen_is_a_request_and_decpex_chooses_the_rectangle() {
        let mut vt = printing(20, 3);
        vt.feed(b"\x1b[0i\x1b[?19l\x1b[0i\x1b[?19h\x1b[0i");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                // DECPEX defaults set, so the whole screen.
                PrinterEvent::Screen {
                    scroll_region: false
                },
                PrinterEvent::Screen {
                    scroll_region: true
                },
                PrinterEvent::Screen {
                    scroll_region: false
                },
            ]
        );
    }

    /// The controller has to be able to start and stop inside one chunk, which
    /// is the ordinary case on a real connection and the reason `feed` cuts the
    /// stream at every `i` rather than handing `vte` the lot.
    #[test]
    fn the_controller_starts_and_stops_inside_one_chunk() {
        let mut vt = printing(20, 3);
        vt.feed(b"a\x1b[5i\x07\x1b[4ib\x1b[5i\x08\x1b[4ic");
        assert_eq!(
            vt.take_printer_events(),
            vec![
                PrinterEvent::Open,
                PrinterEvent::Write("\u{7}".into()),
                PrinterEvent::Close,
                PrinterEvent::Open,
                PrinterEvent::Write("\u{8}".into()),
                PrinterEvent::Close,
            ]
        );
        // Neither the bell nor the backspace was executed, so all three
        // characters are on the row in order.
        assert_eq!(row(&vt, 0).trim_end(), "abc");
        assert_eq!(vt.take_bells().count, 0);
    }

    /// `ResetTerminal` clears `PrinterMode`, and a host cannot reach it: while
    /// the controller has the stream a RIS is four bytes of printer data like
    /// any other sequence. So the reset that ends controller mode is the
    /// *user's* — Reset terminal on the menu — and the flag clearing in
    /// `vtterm.c:327` is unreachable from the wire.
    #[test]
    fn a_reset_off_the_wire_is_printer_data_and_does_not_reset() {
        let mut vt = printing(20, 3);
        vt.feed(b"x\x1b[5i\x1bcy");
        assert!(vt.printer_controller());
        assert_eq!(
            vt.take_printer_events(),
            // `y` is text, so it is displayed *and* copied to the printer
            // through the tap — which is the half of controller mode that is
            // not this machine's.
            vec![PrinterEvent::Open, PrinterEvent::Write("\u{1b}cy".into())]
        );
        // Nothing was reset: `x` is still there and `y` followed it.
        assert_eq!(row(&vt, 0).trim_end(), "xy");
    }

    /// `WF_WINDOWCHANGE` gates everything that moves and `WF_WINDOWREPORT`
    /// everything that answers, and the two are separate keys.
    #[test]
    fn the_two_window_flags_gate_their_own_halves() {
        let mut vt = Vt::new(Config {
            window_change: false,
            window_report: true,
            ..Config::default()
        });
        vt.feed(b"\x1b[2t\x1b[3;1;1t\x1b[8;10;10t\x1b[18t");
        assert!(vt.take_window_requests().is_empty());
        assert_eq!(
            String::from_utf8(vt.take_reply()).unwrap(),
            "\x1b[8;24;80t",
            "the resize was refused, so the report is still the default size"
        );

        let mut vt = Vt::new(Config {
            window_change: true,
            window_report: false,
            ..Config::default()
        });
        vt.feed(b"\x1b[2t\x1b[11t\x1b[13t\x1b[14t\x1b[15t\x1b[16t\x1b[18t\x1b[19t");
        assert_eq!(vt.take_window_requests(), vec![WindowRequest::Iconify]);
        assert!(vt.take_reply().is_empty());
    }

    /// A host may ask forever and a frontend need not drain, so the queue has
    /// a ceiling. Unlike the macro ring, which drops its oldest byte, this
    /// drops the newest instruction.
    #[test]
    fn the_request_queue_has_a_ceiling() {
        let mut vt = Vt::new(Config::default());
        vt.feed(&b"\x1b[5t".repeat(500));
        assert_eq!(vt.take_window_requests().len(), 64);
    }
}
