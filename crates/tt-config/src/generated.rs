// Generated from `schema/settings.txt` by `src/bin/gen-settings.rs`.
// Do not edit: change the schema and re-run the generator.
//
// Committed rather than built, so that a schema change is a reviewable
// diff and neither build system has to run a generator. `tests/generated.rs`
// fails when this file is stale.

#![allow(clippy::all)]

use crate::ini::Ini;
use crate::schema::{Field, Kind};

/// `ts.TerminalID`. `ttset.c:709` reads the key with an empty default and hands
/// it to `TermIDGetID`, which is a case-sensitive `strcmp`
/// (`tttypes_termid.cpp:60`) against the table above it, returning `IdVT100` for
/// anything it does not recognise — so it never fails, a typo silently runs as a
/// VT100, and so does `TerminalID=vt320` in the wrong case. The one enumerated
/// setting here that is not `_stricmp`, hence `enum_exact`. Note `dumb` is
/// lower-case in upstream's own table while every other spelling is upper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalId {
    /// `VT100`
    Vt100,
    /// `VT100J`
    Vt100J,
    /// `VT101`
    Vt101,
    /// `VT102`
    Vt102,
    /// `VT102J`
    Vt102J,
    /// `VT220`
    Vt220,
    /// `VT220J`
    Vt220J,
    /// `VT282`
    Vt282,
    /// `VT320`
    Vt320,
    /// `VT382`
    Vt382,
    /// `VT420`
    Vt420,
    /// `VT520`
    Vt520,
    /// `VT525`
    Vt525,
    /// `dumb`
    Dumb,
}

impl TerminalId {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Vt100 => "VT100",
            Self::Vt100J => "VT100J",
            Self::Vt101 => "VT101",
            Self::Vt102 => "VT102",
            Self::Vt102J => "VT102J",
            Self::Vt220 => "VT220",
            Self::Vt220J => "VT220J",
            Self::Vt282 => "VT282",
            Self::Vt320 => "VT320",
            Self::Vt382 => "VT382",
            Self::Vt420 => "VT420",
            Self::Vt520 => "VT520",
            Self::Vt525 => "VT525",
            Self::Dumb => "dumb",
        }
    }

    /// Case-**sensitive**, because upstream compares this one with
    /// `strcmp` rather than `_stricmp` — and **anything unrecognised
    /// takes the default** rather than failing, so a lower-case
    /// spelling silently reads as that default.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s == "VT100" {
            return Self::Vt100;
        }
        if s == "VT100J" {
            return Self::Vt100J;
        }
        if s == "VT101" {
            return Self::Vt101;
        }
        if s == "VT102" {
            return Self::Vt102;
        }
        if s == "VT102J" {
            return Self::Vt102J;
        }
        if s == "VT220" {
            return Self::Vt220;
        }
        if s == "VT220J" {
            return Self::Vt220J;
        }
        if s == "VT282" {
            return Self::Vt282;
        }
        if s == "VT320" {
            return Self::Vt320;
        }
        if s == "VT382" {
            return Self::Vt382;
        }
        if s == "VT420" {
            return Self::Vt420;
        }
        if s == "VT520" {
            return Self::Vt520;
        }
        if s == "VT525" {
            return Self::Vt525;
        }
        if s == "dumb" {
            return Self::Dumb;
        }
        Self::default()
    }
}

impl Default for TerminalId {
    fn default() -> Self {
        Self::Vt100
    }
}

/// **`ttset.c:631`, and the default is the `else` branch.** A bare CR is a
/// carriage *return*, not a newline, so `"Hello\rWorld"` overwrites the line.
/// Reading this as CRLF shifts every row of every dump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCrReceive {
    /// `CR`
    Cr,
    /// `CRLF`
    CrLf,
    /// `LF`
    Lf,
    /// `AUTO`
    Auto,
}

impl TerminalCrReceive {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Cr => "CR",
            Self::CrLf => "CRLF",
            Self::Lf => "LF",
            Self::Auto => "AUTO",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("CR") {
            return Self::Cr;
        }
        if s.eq_ignore_ascii_case("CRLF") {
            return Self::CrLf;
        }
        if s.eq_ignore_ascii_case("LF") {
            return Self::Lf;
        }
        if s.eq_ignore_ascii_case("AUTO") {
            return Self::Auto;
        }
        Self::default()
    }
}

impl Default for TerminalCrReceive {
    fn default() -> Self {
        Self::Cr
    }
}

/// `ttset.c:646`, the same shape, one variant short — there is no AUTO on send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCrSend {
    /// `CR`
    Cr,
    /// `CRLF`
    CrLf,
    /// `LF`
    Lf,
}

impl TerminalCrSend {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Cr => "CR",
            Self::CrLf => "CRLF",
            Self::Lf => "LF",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("CR") {
            return Self::Cr;
        }
        if s.eq_ignore_ascii_case("CRLF") {
            return Self::CrLf;
        }
        if s.eq_ignore_ascii_case("LF") {
            return Self::Lf;
        }
        Self::default()
    }
}

impl Default for TerminalCrSend {
    fn default() -> Self {
        Self::Cr
    }
}

/// **`ttset.c:877` reads this with an empty fallback and only the literal `DEL`
/// takes the other arm**, so an absent key means BS. That is Tera Term's default
/// and it is probably not what a Linux user wants: a `getty` usually has
/// `stty erase` set to `^?`, so backspace echoes instead of erasing until the
/// host sets DECBKM. Faithful by default, and changeable — which is the whole
/// point of this file existing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardBackspace {
    /// `BS`
    Bs,
    /// `DEL`
    Del,
}

impl KeyboardBackspace {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Bs => "BS",
            Self::Del => "DEL",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("BS") {
            return Self::Bs;
        }
        if s.eq_ignore_ascii_case("DEL") {
            return Self::Del;
        }
        Self::default()
    }
}

impl Default for KeyboardBackspace {
    fn default() -> Self {
        Self::Bs
    }
}

/// `ttset.c:887`. Upstream ships this **off**, and every Linux line editor and
/// Emacs expects Alt to prefix an ESC. The shell has been diverging from
/// upstream here since it was written; this is where that divergence becomes a
/// setting instead of a hard-coded opinion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMeta {
    /// `off`
    Off,
    /// `on`
    On,
    /// `left`
    Left,
    /// `right`
    Right,
}

impl KeyboardMeta {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("off") {
            return Self::Off;
        }
        if s.eq_ignore_ascii_case("on") {
            return Self::On;
        }
        if s.eq_ignore_ascii_case("left") {
            return Self::Left;
        }
        if s.eq_ignore_ascii_case("right") {
            return Self::Right;
        }
        Self::default()
    }
}

impl Default for KeyboardMeta {
    fn default() -> Self {
        Self::Off
    }
}

/// `ttset.c:718`, and the default is again the `else` branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    /// `block`
    Block,
    /// `vertical`
    Vertical,
    /// `horizontal`
    Horizontal,
}

impl CursorShape {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("block") {
            return Self::Block;
        }
        if s.eq_ignore_ascii_case("vertical") {
            return Self::Vertical;
        }
        if s.eq_ignore_ascii_case("horizontal") {
            return Self::Horizontal;
        }
        Self::default()
    }
}

impl Default for CursorShape {
    fn default() -> Self {
        Self::Block
    }
}

/// `ttset.c:589`, and the default is the `else` branch: a `Port=` that says
/// anything but `serial` is TCP/IP, including `Port=tcp` and `Port=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPortType {
    /// `serial`
    Serial,
    /// `tcpip`
    TcpIp,
}

impl ConnectionPortType {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::TcpIp => "tcpip",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("serial") {
            return Self::Serial;
        }
        if s.eq_ignore_ascii_case("tcpip") {
            return Self::TcpIp;
        }
        Self::default()
    }
}

impl Default for ConnectionPortType {
    fn default() -> Self {
        Self::TcpIp
    }
}

/// `ttset.c:929`, default `IdDataBit8` from the `if (!…)` arm. Upstream's dialog
/// offers only these two; `tt-conn` can do 5 and 6 as well, which is a widening
/// and not a setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialDataBits {
    /// `8`
    Eight,
    /// `7`
    Seven,
}

impl SerialDataBits {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Eight => "8",
            Self::Seven => "7",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("8") {
            return Self::Eight;
        }
        if s.eq_ignore_ascii_case("7") {
            return Self::Seven;
        }
        Self::default()
    }
}

impl Default for SerialDataBits {
    fn default() -> Self {
        Self::Eight
    }
}

/// `ttset.c:922`, default `IdParityNone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialParity {
    /// `none`
    None,
    /// `odd`
    Odd,
    /// `even`
    Even,
    /// `mark`
    Mark,
    /// `space`
    Space,
}

impl SerialParity {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Odd => "odd",
            Self::Even => "even",
            Self::Mark => "mark",
            Self::Space => "space",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") {
            return Self::None;
        }
        if s.eq_ignore_ascii_case("odd") {
            return Self::Odd;
        }
        if s.eq_ignore_ascii_case("even") {
            return Self::Even;
        }
        if s.eq_ignore_ascii_case("mark") {
            return Self::Mark;
        }
        if s.eq_ignore_ascii_case("space") {
            return Self::Space;
        }
        Self::default()
    }
}

impl Default for SerialParity {
    fn default() -> Self {
        Self::None
    }
}

/// `ttset.c:936`, default `IdStopBit1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialStopBits {
    /// `1`
    One,
    /// `2`
    Two,
}

impl SerialStopBits {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("1") {
            return Self::One;
        }
        if s.eq_ignore_ascii_case("2") {
            return Self::Two;
        }
        Self::default()
    }
}

impl Default for SerialStopBits {
    fn default() -> Self {
        Self::One
    }
}

/// `ttset.c:943`, default `IdFlowNone`. `rtscts` is a second spelling of `hard`
/// and the only alias in any of the four tables (`ttset.c:111`); the schema lists
/// the canonical one first, which is what gets written back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialFlow {
    /// `none`
    None,
    /// `x`
    XonXoff,
    /// `hard`, `rtscts` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Hardware,
    /// `dsrdtr`
    DsrDtr,
}

impl SerialFlow {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::XonXoff => "x",
            Self::Hardware => "hard",
            Self::DsrDtr => "dsrdtr",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") {
            return Self::None;
        }
        if s.eq_ignore_ascii_case("x") {
            return Self::XonXoff;
        }
        if s.eq_ignore_ascii_case("hard") || s.eq_ignore_ascii_case("rtscts") {
            return Self::Hardware;
        }
        if s.eq_ignore_ascii_case("dsrdtr") {
            return Self::DsrDtr;
        }
        Self::default()
    }
}

impl Default for SerialFlow {
    fn default() -> Self {
        Self::None
    }
}

/// Every setting this project reads out of `TERATERM.INI`.
///
/// Generated from the schema, so the field, its default, its INI key and
/// the citation for that default are one thing rather than four that can
/// disagree.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `ttset.c:615`, bounded by `TermWidthMax` — which is **1000**, not the 500
    /// this used to say; 500 is `TermHeightMax`, the next line of `tttypes.h:633`.
    /// Zero or less takes the default rather than the floor, which is what the range
    /// means here.
    pub terminal_cols: i32,
    /// `ttset.c:619`, the second half of the same key, bounded by `TermHeightMax`.
    pub terminal_rows: i32,
    /// `ts.TerminalID`. `ttset.c:709` reads the key with an empty default and hands
    /// it to `TermIDGetID`, which is a case-sensitive `strcmp`
    /// (`tttypes_termid.cpp:60`) against the table above it, returning `IdVT100` for
    /// anything it does not recognise — so it never fails, a typo silently runs as a
    /// VT100, and so does `TerminalID=vt320` in the wrong case. The one enumerated
    /// setting here that is not `_stricmp`, hence `enum_exact`. Note `dumb` is
    /// lower-case in upstream's own table while every other spelling is upper.
    pub terminal_id: TerminalId,
    /// **`ttset.c:631`, and the default is the `else` branch.** A bare CR is a
    /// carriage *return*, not a newline, so `"Hello\rWorld"` overwrites the line.
    /// Reading this as CRLF shifts every row of every dump.
    pub terminal_cr_receive: TerminalCrReceive,
    /// `ttset.c:646`, the same shape, one variant short — there is no AUTO on send.
    pub terminal_cr_send: TerminalCrSend,
    /// `ttset.c:660`.
    pub terminal_local_echo: bool,
    /// `ttset.c:625`. With it on, resizing the window resizes the terminal.
    pub terminal_size_follows_window: bool,
    /// `ttset.c:628`. With it on, a remote resize resizes the window.
    pub terminal_auto_win_resize: bool,
    /// `ttset.c:743`. Off keeps the terminal to one screen and no history.
    pub terminal_scrollback_enabled: bool,
    /// `ttset.c:751`. Upstream ships **100** lines, which is small for a console
    /// log; it is a setting rather than a constant precisely so it can be raised.
    pub terminal_scrollback_lines: i32,
    /// `ts.Title`, `ttset.c:713`. The window title before the host sends one.
    pub terminal_title: String,
    /// **`ttset.c:877` reads this with an empty fallback and only the literal `DEL`
    /// takes the other arm**, so an absent key means BS. That is Tera Term's default
    /// and it is probably not what a Linux user wants: a `getty` usually has
    /// `stty erase` set to `^?`, so backspace echoes instead of erasing until the
    /// host sets DECBKM. Faithful by default, and changeable — which is the whole
    /// point of this file existing.
    pub keyboard_backspace: KeyboardBackspace,
    /// `ttset.c:887`. Upstream ships this **off**, and every Linux line editor and
    /// Emacs expects Alt to prefix an ESC. The shell has been diverging from
    /// upstream here since it was written; this is where that divergence becomes a
    /// setting instead of a hard-coded opinion.
    pub keyboard_meta: KeyboardMeta,
    /// `ttset.c:882`. Whether the Delete key sends DEL rather than the VT220
    /// Remove sequence.
    pub keyboard_delete_sends_del: bool,
    /// `ttset.c:1167`, and the value is **hex-escaped** (`Hex2StrW`): `$20` is a
    /// space. The default is a space plus every ASCII punctuation mark except
    /// underscore, which is what makes `some_name` one word and `some-name` three
    /// when you double-click it.
    pub keyboard_word_delimiters: String,
    /// `ttset.c:754`. Black on white, which is what Tera Term looks like out of the
    /// box and surprises people who expect a terminal to be dark.
    pub color_normal: [u8; 6],
    /// `ttset.c:757`. Blue.
    pub color_bold: [u8; 6],
    /// `ttset.c:762`. Red.
    pub color_blink: [u8; 6],
    /// `ttset.c:786`. Magenta.
    pub color_underline: [u8; 6],
    /// `ttset.c:767`. White on black — and unlike the other three this one ships
    /// **off**, so reverse video uses the normal pair swapped instead.
    pub color_reverse: [u8; 6],
    /// `ttset.c:758`.
    pub color_bold_enabled: bool,
    /// `ttset.c:763`.
    pub color_blink_enabled: bool,
    /// `ttset.c:768`. **Off**, which is why `SGR 7` swaps the normal pair rather
    /// than using the colours above.
    pub color_reverse_enabled: bool,
    /// `ttset.c:790`. Whether `SGR 4` gets its own colour pair at all.
    pub color_underline_enabled: bool,
    /// `ttset.c:741`. **On**, and this is one of the four flag words `CLAUDE.md`
    /// warns about: `ColorFlag` is zeroed at the top of `ttset.c` and built up from
    /// per-key calls a thousand lines later, so reading the zero as the default
    /// turns 256-colour off and looks like a parser bug.
    pub color_xterm_256: bool,
    /// `ttset.c:738`. **Off**, so `SGR 90-97` and `100-107` are ignored and the
    /// previous pen stands — which looks exactly like a painter bug.
    pub color_aixterm_16: bool,
    /// `ttset.c:735`. PC-style bold colour mapping.
    pub color_pc_bold_16: bool,
    /// `ttset.c:718`, and the default is again the `else` branch.
    pub cursor_shape: CursorShape,
    /// `ttset.c:1227`.
    pub cursor_nonblinking: bool,
    /// `ttset.c:1653`, part of `WindowFlag` — the same trap as `ColorFlag`. Gates
    /// every XTWINOPS operation that *changes* something, the resize included.
    pub window_change_allowed: bool,
    /// `ttset.c:1661`. Gates the ones that answer back.
    pub window_report_allowed: bool,
    /// `ttset.c:1656`. **Off**, so DECSCUSR and `DECSET 12` do nothing until it is
    /// turned on, and DECRQM reports them reset.
    pub window_cursor_ctrl_allowed: bool,
    /// `ttset.c:1075`, part of `TermFlag`. Whether 8-bit C1 bytes are executed as
    /// controls.
    pub window_accept_8bit_ctrl: bool,
    /// `ttset.c:1283`. Whether *replies* use 8-bit C1 introducers.
    pub window_send_8bit_ctrl: bool,
    /// `ttset.c:1681`, part of `TermFlag`. The alternate screen, which `vim` and
    /// `less` need.
    pub window_alt_screen: bool,
    /// `ttset.c:1950`. Whether `ED 3` may discard the scrollback.
    pub window_remote_clears_buffer: bool,
    /// `ttset.c:1523`. **On**, and it was left zeroed here for a while, which
    /// silently disabled every mouse mode and made DECRQM answer "permanently
    /// reset" for all of them.
    pub mouse_tracking: bool,
    /// `ttset.c:1591`. Holding Ctrl suppresses the report so text can still be
    /// selected in a full-screen application.
    pub mouse_ctrl_disables_tracking: bool,
    /// `ttset.c:1515`. Gates `DECSET 7786`, and is what a reset restores it to.
    pub mouse_wheel_to_cursor: bool,
    /// `ttset.c:589`, and the default is the `else` branch: a `Port=` that says
    /// anything but `serial` is TCP/IP, including `Port=tcp` and `Port=`.
    pub connection_port_type: ConnectionPortType,
    /// **`ttset.c:966`, and its default is an initialiser rather than the setting it
    /// looks like.** The call is `GetPrivateProfileInt(…, "TCPPort", ts->TelPort, …)`
    /// — but `TelPort=` is not read until `:1311`, four hundred lines later, so the
    /// value in hand is the hardcoded `ts->TelPort = 23` from `:566`. A file with
    /// `TelPort=2323` and no `TCPPort=` therefore opens port **23**, not 2323.
    /// Reading the file's `TelPort` as the default here is the obvious thing and it
    /// is wrong.
    pub connection_tcp_port: i32,
    /// `ttset.c:958`, `GetOnOff(…, TRUE)` — so `Telnet=1` is **on** and `Telnet=off`
    /// is the only way to turn it off. See `schema.rs`'s `on_off` for why that is
    /// not the same as `Telnet=0`.
    pub connection_telnet: bool,
    /// `ttset.c:1311`. What the New Connection dialog fills the port box with when
    /// Telnet is chosen, and — as above — *not* what `TCPPort` defaults to.
    pub connection_telnet_port: i32,
    /// `ttset.c:1301`, `GetOnOff(…, FALSE)` — so here `TelBin=1` reads as **off**,
    /// which is the opposite of what the same value means for `Telnet=` above.
    pub connection_telnet_binary: bool,
    /// `ttset.c:969`, on by default. Whether the window closes when the connection
    /// does. `/AUTOWINCLOSE=` on a command line is **not** `GetOnOff`: it tests for
    /// `on` and everything else is off, so the two readers disagree about `1`.
    pub connection_auto_win_close: bool,
    /// `ttset.c:1457`, in seconds, zero meaning the stack's own timeout. `/TIMEOUT=`
    /// refuses a negative value rather than clamping it.
    pub connection_timeout: i32,
    /// `ttset.c:1520`, on by default — the New Connection dialog at startup, which
    /// `/DS` suppresses and `/ES` asks for.
    pub connection_host_dialog_on_startup: bool,
    /// `ttset.c:916`. **The bound is a different setting and is not a clamp**:
    /// `:1223` resets the port to 1 when it is below 1 or above `MaxComPort`, which
    /// is read at `:1218` — after this key but before the check. Left as a plain int
    /// here because the schema has no way to say "bounded by that other setting",
    /// and `PLAN.md` carries it as an open item.
    ///
    /// What a number means on Linux is also open: this port opens a device path, and
    /// `/C=1` has to be resolved against enumeration rather than against `COM1`.
    pub serial_com_port: i32,
    /// `ttset.c:919`. Unbounded, and the hardware decides what it can do with it —
    /// `tt-conn` reads the setting back after `tcsetattr` for exactly that reason.
    pub serial_baud: i32,
    /// `ttset.c:929`, default `IdDataBit8` from the `if (!…)` arm. Upstream's dialog
    /// offers only these two; `tt-conn` can do 5 and 6 as well, which is a widening
    /// and not a setting.
    pub serial_data_bits: SerialDataBits,
    /// `ttset.c:922`, default `IdParityNone`.
    pub serial_parity: SerialParity,
    /// `ttset.c:936`, default `IdStopBit1`.
    pub serial_stop_bits: SerialStopBits,
    /// `ttset.c:943`, default `IdFlowNone`. `rtscts` is a second spelling of `hard`
    /// and the only alias in any of the four tables (`ttset.c:111`); the schema lists
    /// the canonical one first, which is what gets written back.
    pub serial_flow: SerialFlow,
    /// `ttset.c:951`, milliseconds between characters — for a device that cannot
    /// keep up with a paste.
    pub serial_delay_per_char: i32,
    /// `ttset.c:955`, milliseconds between lines.
    pub serial_delay_per_line: i32,
    /// `ttset.c:1151`. Wait for the port to appear instead of failing — a USB
    /// adapter that has not been plugged in yet.
    pub serial_wait_com: bool,
    /// `ttset.c:1218`, floored at 4 and capped at `MAXCOMPORT` (4096, `tttypes.h:908`)
    /// — so the range here is the *file's* and the floor is upstream's own. This is
    /// the setting `/C=` is bounded against, which is why the parser takes it as an
    /// argument.
    pub serial_max_com_port: i32,
    /// `ttset.c:1026`. Start logging as soon as the session opens. `/NOLOG` turns it
    /// off; `/L=` names the file, which is **not** a setting — `ts.LogFN` has no key
    /// of its own, only `LogDefaultName` for the dialog's suggestion.
    pub log_auto_start: bool,
    /// `ttset.c:1060`. Where a file transfer starts looking, and where a protocol
    /// that names its own file puts it — `GetRecievePath`. `/FD=` sets it, but only
    /// if the directory exists.
    pub transfer_dir: String,
    /// `ttset.c:728`. No title bar, which `/H` also asks for. `/I` and `/V` —
    /// minimised and invisible — have no keys at all: `_ReadIniFile` zeroes both at
    /// `:554` and never reads one, so they are command-line-only.
    pub window_hide_title: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            terminal_cols: 80,
            terminal_rows: 24,
            terminal_id: TerminalId::default(),
            terminal_cr_receive: TerminalCrReceive::default(),
            terminal_cr_send: TerminalCrSend::default(),
            terminal_local_echo: false,
            terminal_size_follows_window: false,
            terminal_auto_win_resize: false,
            terminal_scrollback_enabled: true,
            terminal_scrollback_lines: 100,
            terminal_title: String::from("Tera Term"),
            keyboard_backspace: KeyboardBackspace::default(),
            keyboard_meta: KeyboardMeta::default(),
            keyboard_delete_sends_del: false,
            keyboard_word_delimiters: String::from("$20!\"#$24%&'()*+,-./:;<=>?@[\\]^`{|}~"),
            color_normal: [0, 0, 0, 255, 255, 255],
            color_bold: [0, 0, 255, 255, 255, 255],
            color_blink: [255, 0, 0, 255, 255, 255],
            color_underline: [255, 0, 255, 255, 255, 255],
            color_reverse: [255, 255, 255, 0, 0, 0],
            color_bold_enabled: true,
            color_blink_enabled: true,
            color_reverse_enabled: false,
            color_underline_enabled: false,
            color_xterm_256: true,
            color_aixterm_16: false,
            color_pc_bold_16: false,
            cursor_shape: CursorShape::default(),
            cursor_nonblinking: false,
            window_change_allowed: true,
            window_report_allowed: true,
            window_cursor_ctrl_allowed: false,
            window_accept_8bit_ctrl: true,
            window_send_8bit_ctrl: false,
            window_alt_screen: true,
            window_remote_clears_buffer: true,
            mouse_tracking: true,
            mouse_ctrl_disables_tracking: true,
            mouse_wheel_to_cursor: true,
            connection_port_type: ConnectionPortType::default(),
            connection_tcp_port: 23,
            connection_telnet: true,
            connection_telnet_port: 23,
            connection_telnet_binary: false,
            connection_auto_win_close: true,
            connection_timeout: 0,
            connection_host_dialog_on_startup: true,
            serial_com_port: 1,
            serial_baud: 9600,
            serial_data_bits: SerialDataBits::default(),
            serial_parity: SerialParity::default(),
            serial_stop_bits: SerialStopBits::default(),
            serial_flow: SerialFlow::default(),
            serial_delay_per_char: 0,
            serial_delay_per_line: 0,
            serial_wait_com: false,
            serial_max_com_port: 256,
            log_auto_start: false,
            transfer_dir: String::from(""),
            window_hide_title: false,
        }
    }
}

impl Settings {
    /// Read every setting, taking the default for anything absent.
    pub fn load(ini: &Ini) -> Settings {
        let d = Settings::default();
        Settings {
            terminal_cols: crate::schema::ranged(
                crate::schema::nth_int(ini.get("Tera Term", "TerminalSize"), 0, d.terminal_cols),
                d.terminal_cols,
                1,
                1000,
            ),
            terminal_rows: crate::schema::ranged(
                crate::schema::nth_int(ini.get("Tera Term", "TerminalSize"), 1, d.terminal_rows),
                d.terminal_rows,
                1,
                500,
            ),
            terminal_id: match ini.get("Tera Term", "TerminalID") {
                Some(v) => TerminalId::from_ini(v),
                None => d.terminal_id,
            },
            terminal_cr_receive: match ini.get("Tera Term", "CRReceive") {
                Some(v) => TerminalCrReceive::from_ini(v),
                None => d.terminal_cr_receive,
            },
            terminal_cr_send: match ini.get("Tera Term", "CRSend") {
                Some(v) => TerminalCrSend::from_ini(v),
                None => d.terminal_cr_send,
            },
            terminal_local_echo: crate::schema::on_off(ini.get("Tera Term", "LocalEcho"), false),
            terminal_size_follows_window: crate::schema::on_off(
                ini.get("Tera Term", "TermIsWin"),
                false,
            ),
            terminal_auto_win_resize: crate::schema::on_off(
                ini.get("Tera Term", "AutoWinResize"),
                false,
            ),
            terminal_scrollback_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableScrollBuff"),
                true,
            ),
            terminal_scrollback_lines: ini.get_int(
                "Tera Term",
                "ScrollBuffSize",
                d.terminal_scrollback_lines,
            ) as i32,
            terminal_title: ini
                .get_or("Tera Term", "Title", &d.terminal_title)
                .to_string(),
            keyboard_backspace: match ini.get("Tera Term", "BSKey") {
                Some(v) => KeyboardBackspace::from_ini(v),
                None => d.keyboard_backspace,
            },
            keyboard_meta: match ini.get("Tera Term", "MetaKey") {
                Some(v) => KeyboardMeta::from_ini(v),
                None => d.keyboard_meta,
            },
            keyboard_delete_sends_del: crate::schema::on_off(
                ini.get("Tera Term", "DeleteKey"),
                false,
            ),
            keyboard_word_delimiters: ini
                .get_or("Tera Term", "DelimList", &d.keyboard_word_delimiters)
                .to_string(),
            color_normal: crate::schema::color2(ini.get("Tera Term", "VTColor"), d.color_normal),
            color_bold: crate::schema::color2(ini.get("Tera Term", "VTBoldColor"), d.color_bold),
            color_blink: crate::schema::color2(ini.get("Tera Term", "VTBlinkColor"), d.color_blink),
            color_underline: crate::schema::color2(
                ini.get("Tera Term", "VTUnderlineColor"),
                d.color_underline,
            ),
            color_reverse: crate::schema::color2(
                ini.get("Tera Term", "VTReverseColor"),
                d.color_reverse,
            ),
            color_bold_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableBoldAttrColor"),
                true,
            ),
            color_blink_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableBlinkAttrColor"),
                true,
            ),
            color_reverse_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableReverseAttrColor"),
                false,
            ),
            color_underline_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableUnderlineAttrColor"),
                false,
            ),
            color_xterm_256: crate::schema::on_off(ini.get("Tera Term", "Xterm256Color"), true),
            color_aixterm_16: crate::schema::on_off(ini.get("Tera Term", "Aixterm16Color"), false),
            color_pc_bold_16: crate::schema::on_off(ini.get("Tera Term", "PcBoldColor"), false),
            cursor_shape: match ini.get("Tera Term", "CursorShape") {
                Some(v) => CursorShape::from_ini(v),
                None => d.cursor_shape,
            },
            cursor_nonblinking: crate::schema::on_off(
                ini.get("Tera Term", "NonblinkingCursor"),
                false,
            ),
            window_change_allowed: crate::schema::on_off(
                ini.get("Tera Term", "WindowChangeSequence"),
                true,
            ),
            window_report_allowed: crate::schema::on_off(
                ini.get("Tera Term", "WindowReportSequence"),
                true,
            ),
            window_cursor_ctrl_allowed: crate::schema::on_off(
                ini.get("Tera Term", "CursorCtrlSequence"),
                false,
            ),
            window_accept_8bit_ctrl: crate::schema::on_off(
                ini.get("Tera Term", "Accept8BitCtrl"),
                true,
            ),
            window_send_8bit_ctrl: crate::schema::on_off(
                ini.get("Tera Term", "Send8BitCtrl"),
                false,
            ),
            window_alt_screen: crate::schema::on_off(ini.get("Tera Term", "AltScreenBuffer"), true),
            window_remote_clears_buffer: crate::schema::on_off(
                ini.get("Tera Term", "RemoteClearsBuffer"),
                true,
            ),
            mouse_tracking: crate::schema::on_off(ini.get("Tera Term", "MouseEventTracking"), true),
            mouse_ctrl_disables_tracking: crate::schema::on_off(
                ini.get("Tera Term", "DisableMouseTrackingByCtrl"),
                true,
            ),
            mouse_wheel_to_cursor: crate::schema::on_off(
                ini.get("Tera Term", "TranslateWheelToCursor"),
                true,
            ),
            connection_port_type: match ini.get("Tera Term", "Port") {
                Some(v) => ConnectionPortType::from_ini(v),
                None => d.connection_port_type,
            },
            connection_tcp_port: ini.get_int("Tera Term", "TCPPort", d.connection_tcp_port) as i32,
            connection_telnet: crate::schema::on_off(ini.get("Tera Term", "Telnet"), true),
            connection_telnet_port: ini.get_int("Tera Term", "TelPort", d.connection_telnet_port)
                as i32,
            connection_telnet_binary: crate::schema::on_off(ini.get("Tera Term", "TelBin"), false),
            connection_auto_win_close: crate::schema::on_off(
                ini.get("Tera Term", "AutoWinClose"),
                true,
            ),
            connection_timeout: ini.get_int("Tera Term", "ConnectingTimeout", d.connection_timeout)
                as i32,
            connection_host_dialog_on_startup: crate::schema::on_off(
                ini.get("Tera Term", "HostDialogOnStartup"),
                true,
            ),
            serial_com_port: ini.get_int("Tera Term", "ComPort", d.serial_com_port) as i32,
            serial_baud: ini.get_int("Tera Term", "BaudRate", d.serial_baud) as i32,
            serial_data_bits: match ini.get("Tera Term", "DataBit") {
                Some(v) => SerialDataBits::from_ini(v),
                None => d.serial_data_bits,
            },
            serial_parity: match ini.get("Tera Term", "Parity") {
                Some(v) => SerialParity::from_ini(v),
                None => d.serial_parity,
            },
            serial_stop_bits: match ini.get("Tera Term", "StopBit") {
                Some(v) => SerialStopBits::from_ini(v),
                None => d.serial_stop_bits,
            },
            serial_flow: match ini.get("Tera Term", "FlowCtrl") {
                Some(v) => SerialFlow::from_ini(v),
                None => d.serial_flow,
            },
            serial_delay_per_char: ini.get_int("Tera Term", "DelayPerChar", d.serial_delay_per_char)
                as i32,
            serial_delay_per_line: ini.get_int("Tera Term", "DelayPerLine", d.serial_delay_per_line)
                as i32,
            serial_wait_com: crate::schema::on_off(ini.get("Tera Term", "WaitCom"), false),
            serial_max_com_port: crate::schema::ranged(
                ini.get_int("Tera Term", "MaxComPort", d.serial_max_com_port) as i32,
                d.serial_max_com_port,
                4,
                4096,
            ),
            log_auto_start: crate::schema::on_off(ini.get("Tera Term", "LogAutoStart"), false),
            transfer_dir: ini
                .get_or("Tera Term", "FileDir", &d.transfer_dir)
                .to_string(),
            window_hide_title: crate::schema::on_off(ini.get("Tera Term", "HideTitle"), false),
        }
    }

    /// Write every setting back, leaving the rest of the file alone.
    pub fn store(&self, ini: &mut Ini) {
        ini.set(
            "Tera Term",
            "TerminalSize",
            &crate::schema::with_nth(ini.get("Tera Term", "TerminalSize"), 0, self.terminal_cols),
        );
        ini.set(
            "Tera Term",
            "TerminalSize",
            &crate::schema::with_nth(ini.get("Tera Term", "TerminalSize"), 1, self.terminal_rows),
        );
        ini.set(
            "Tera Term",
            "TerminalID",
            &self.terminal_id.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "CRReceive",
            &self.terminal_cr_receive.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "CRSend",
            &self.terminal_cr_send.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "LocalEcho",
            &if self.terminal_local_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TermIsWin",
            &if self.terminal_size_follows_window {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoWinResize",
            &if self.terminal_auto_win_resize {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableScrollBuff",
            &if self.terminal_scrollback_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ScrollBuffSize",
            &self.terminal_scrollback_lines.to_string(),
        );
        ini.set("Tera Term", "Title", &self.terminal_title.clone());
        ini.set(
            "Tera Term",
            "BSKey",
            &self.keyboard_backspace.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "MetaKey",
            &self.keyboard_meta.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "DeleteKey",
            &if self.keyboard_delete_sends_del {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DelimList",
            &self.keyboard_word_delimiters.clone(),
        );
        ini.set(
            "Tera Term",
            "VTColor",
            &crate::schema::color2_str(&self.color_normal),
        );
        ini.set(
            "Tera Term",
            "VTBoldColor",
            &crate::schema::color2_str(&self.color_bold),
        );
        ini.set(
            "Tera Term",
            "VTBlinkColor",
            &crate::schema::color2_str(&self.color_blink),
        );
        ini.set(
            "Tera Term",
            "VTUnderlineColor",
            &crate::schema::color2_str(&self.color_underline),
        );
        ini.set(
            "Tera Term",
            "VTReverseColor",
            &crate::schema::color2_str(&self.color_reverse),
        );
        ini.set(
            "Tera Term",
            "EnableBoldAttrColor",
            &if self.color_bold_enabled { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableBlinkAttrColor",
            &if self.color_blink_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableReverseAttrColor",
            &if self.color_reverse_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableUnderlineAttrColor",
            &if self.color_underline_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Xterm256Color",
            &if self.color_xterm_256 { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "Aixterm16Color",
            &if self.color_aixterm_16 { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "PcBoldColor",
            &if self.color_pc_bold_16 { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "CursorShape",
            &self.cursor_shape.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "NonblinkingCursor",
            &if self.cursor_nonblinking { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "WindowChangeSequence",
            &if self.window_change_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "WindowReportSequence",
            &if self.window_report_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "CursorCtrlSequence",
            &if self.window_cursor_ctrl_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Accept8BitCtrl",
            &if self.window_accept_8bit_ctrl {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Send8BitCtrl",
            &if self.window_send_8bit_ctrl {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AltScreenBuffer",
            &if self.window_alt_screen { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "RemoteClearsBuffer",
            &if self.window_remote_clears_buffer {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "MouseEventTracking",
            &if self.mouse_tracking { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableMouseTrackingByCtrl",
            &if self.mouse_ctrl_disables_tracking {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TranslateWheelToCursor",
            &if self.mouse_wheel_to_cursor {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Port",
            &self.connection_port_type.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "TCPPort",
            &self.connection_tcp_port.to_string(),
        );
        ini.set(
            "Tera Term",
            "Telnet",
            &if self.connection_telnet { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "TelPort",
            &self.connection_telnet_port.to_string(),
        );
        ini.set(
            "Tera Term",
            "TelBin",
            &if self.connection_telnet_binary {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoWinClose",
            &if self.connection_auto_win_close {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConnectingTimeout",
            &self.connection_timeout.to_string(),
        );
        ini.set(
            "Tera Term",
            "HostDialogOnStartup",
            &if self.connection_host_dialog_on_startup {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set("Tera Term", "ComPort", &self.serial_com_port.to_string());
        ini.set("Tera Term", "BaudRate", &self.serial_baud.to_string());
        ini.set(
            "Tera Term",
            "DataBit",
            &self.serial_data_bits.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "Parity",
            &self.serial_parity.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "StopBit",
            &self.serial_stop_bits.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "FlowCtrl",
            &self.serial_flow.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "DelayPerChar",
            &self.serial_delay_per_char.to_string(),
        );
        ini.set(
            "Tera Term",
            "DelayPerLine",
            &self.serial_delay_per_line.to_string(),
        );
        ini.set(
            "Tera Term",
            "WaitCom",
            &if self.serial_wait_com { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "MaxComPort",
            &self.serial_max_com_port.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogAutoStart",
            &if self.log_auto_start { "on" } else { "off" }.to_string(),
        );
        ini.set("Tera Term", "FileDir", &self.transfer_dir.clone());
        ini.set(
            "Tera Term",
            "HideTitle",
            &if self.window_hide_title { "on" } else { "off" }.to_string(),
        );
    }

    /// One setting by its dotted name, in the INI's own spelling.
    pub fn get_str(&self, name: &str) -> Option<String> {
        Some(match name {
            "terminal.cols" => self.terminal_cols.to_string(),
            "terminal.rows" => self.terminal_rows.to_string(),
            "terminal.id" => self.terminal_id.as_ini().to_string(),
            "terminal.cr_receive" => self.terminal_cr_receive.as_ini().to_string(),
            "terminal.cr_send" => self.terminal_cr_send.as_ini().to_string(),
            "terminal.local_echo" => if self.terminal_local_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.size_follows_window" => if self.terminal_size_follows_window {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.auto_win_resize" => if self.terminal_auto_win_resize {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.scrollback_enabled" => if self.terminal_scrollback_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.scrollback_lines" => self.terminal_scrollback_lines.to_string(),
            "terminal.title" => self.terminal_title.clone(),
            "keyboard.backspace" => self.keyboard_backspace.as_ini().to_string(),
            "keyboard.meta" => self.keyboard_meta.as_ini().to_string(),
            "keyboard.delete_sends_del" => if self.keyboard_delete_sends_del {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "keyboard.word_delimiters" => self.keyboard_word_delimiters.clone(),
            "color.normal" => crate::schema::color2_str(&self.color_normal),
            "color.bold" => crate::schema::color2_str(&self.color_bold),
            "color.blink" => crate::schema::color2_str(&self.color_blink),
            "color.underline" => crate::schema::color2_str(&self.color_underline),
            "color.reverse" => crate::schema::color2_str(&self.color_reverse),
            "color.bold_enabled" => if self.color_bold_enabled { "on" } else { "off" }.to_string(),
            "color.blink_enabled" => if self.color_blink_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "color.reverse_enabled" => if self.color_reverse_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "color.underline_enabled" => if self.color_underline_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "color.xterm_256" => if self.color_xterm_256 { "on" } else { "off" }.to_string(),
            "color.aixterm_16" => if self.color_aixterm_16 { "on" } else { "off" }.to_string(),
            "color.pc_bold_16" => if self.color_pc_bold_16 { "on" } else { "off" }.to_string(),
            "cursor.shape" => self.cursor_shape.as_ini().to_string(),
            "cursor.nonblinking" => if self.cursor_nonblinking { "on" } else { "off" }.to_string(),
            "window.change_allowed" => if self.window_change_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.report_allowed" => if self.window_report_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.cursor_ctrl_allowed" => if self.window_cursor_ctrl_allowed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.accept_8bit_ctrl" => if self.window_accept_8bit_ctrl {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.send_8bit_ctrl" => if self.window_send_8bit_ctrl {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.alt_screen" => if self.window_alt_screen { "on" } else { "off" }.to_string(),
            "window.remote_clears_buffer" => if self.window_remote_clears_buffer {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "mouse.tracking" => if self.mouse_tracking { "on" } else { "off" }.to_string(),
            "mouse.ctrl_disables_tracking" => if self.mouse_ctrl_disables_tracking {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "mouse.wheel_to_cursor" => if self.mouse_wheel_to_cursor {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.port_type" => self.connection_port_type.as_ini().to_string(),
            "connection.tcp_port" => self.connection_tcp_port.to_string(),
            "connection.telnet" => if self.connection_telnet { "on" } else { "off" }.to_string(),
            "connection.telnet_port" => self.connection_telnet_port.to_string(),
            "connection.telnet_binary" => if self.connection_telnet_binary {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.auto_win_close" => if self.connection_auto_win_close {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.timeout" => self.connection_timeout.to_string(),
            "connection.host_dialog_on_startup" => if self.connection_host_dialog_on_startup {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "serial.com_port" => self.serial_com_port.to_string(),
            "serial.baud" => self.serial_baud.to_string(),
            "serial.data_bits" => self.serial_data_bits.as_ini().to_string(),
            "serial.parity" => self.serial_parity.as_ini().to_string(),
            "serial.stop_bits" => self.serial_stop_bits.as_ini().to_string(),
            "serial.flow" => self.serial_flow.as_ini().to_string(),
            "serial.delay_per_char" => self.serial_delay_per_char.to_string(),
            "serial.delay_per_line" => self.serial_delay_per_line.to_string(),
            "serial.wait_com" => if self.serial_wait_com { "on" } else { "off" }.to_string(),
            "serial.max_com_port" => self.serial_max_com_port.to_string(),
            "log.auto_start" => if self.log_auto_start { "on" } else { "off" }.to_string(),
            "transfer.dir" => self.transfer_dir.clone(),
            "window.hide_title" => if self.window_hide_title { "on" } else { "off" }.to_string(),
            _ => return None,
        })
    }

    /// Set one setting by name, parsed the way the file would be.
    /// False when the name is not one of ours.
    pub fn set_str(&mut self, name: &str, value: &str) -> bool {
        match name {
            "terminal.cols" => {
                self.terminal_cols = crate::schema::ranged(
                    crate::schema::int(value, self.terminal_cols),
                    80,
                    1,
                    1000,
                )
            }
            "terminal.rows" => {
                self.terminal_rows =
                    crate::schema::ranged(crate::schema::int(value, self.terminal_rows), 24, 1, 500)
            }
            "terminal.id" => self.terminal_id = TerminalId::from_ini(value),
            "terminal.cr_receive" => self.terminal_cr_receive = TerminalCrReceive::from_ini(value),
            "terminal.cr_send" => self.terminal_cr_send = TerminalCrSend::from_ini(value),
            "terminal.local_echo" => {
                self.terminal_local_echo = crate::schema::on_off(Some(value), false)
            }
            "terminal.size_follows_window" => {
                self.terminal_size_follows_window = crate::schema::on_off(Some(value), false)
            }
            "terminal.auto_win_resize" => {
                self.terminal_auto_win_resize = crate::schema::on_off(Some(value), false)
            }
            "terminal.scrollback_enabled" => {
                self.terminal_scrollback_enabled = crate::schema::on_off(Some(value), true)
            }
            "terminal.scrollback_lines" => {
                self.terminal_scrollback_lines =
                    crate::schema::int(value, self.terminal_scrollback_lines)
            }
            "terminal.title" => self.terminal_title = value.to_string(),
            "keyboard.backspace" => self.keyboard_backspace = KeyboardBackspace::from_ini(value),
            "keyboard.meta" => self.keyboard_meta = KeyboardMeta::from_ini(value),
            "keyboard.delete_sends_del" => {
                self.keyboard_delete_sends_del = crate::schema::on_off(Some(value), false)
            }
            "keyboard.word_delimiters" => self.keyboard_word_delimiters = value.to_string(),
            "color.normal" => {
                self.color_normal = crate::schema::color2(Some(value), self.color_normal)
            }
            "color.bold" => self.color_bold = crate::schema::color2(Some(value), self.color_bold),
            "color.blink" => {
                self.color_blink = crate::schema::color2(Some(value), self.color_blink)
            }
            "color.underline" => {
                self.color_underline = crate::schema::color2(Some(value), self.color_underline)
            }
            "color.reverse" => {
                self.color_reverse = crate::schema::color2(Some(value), self.color_reverse)
            }
            "color.bold_enabled" => {
                self.color_bold_enabled = crate::schema::on_off(Some(value), true)
            }
            "color.blink_enabled" => {
                self.color_blink_enabled = crate::schema::on_off(Some(value), true)
            }
            "color.reverse_enabled" => {
                self.color_reverse_enabled = crate::schema::on_off(Some(value), false)
            }
            "color.underline_enabled" => {
                self.color_underline_enabled = crate::schema::on_off(Some(value), false)
            }
            "color.xterm_256" => self.color_xterm_256 = crate::schema::on_off(Some(value), true),
            "color.aixterm_16" => self.color_aixterm_16 = crate::schema::on_off(Some(value), false),
            "color.pc_bold_16" => self.color_pc_bold_16 = crate::schema::on_off(Some(value), false),
            "cursor.shape" => self.cursor_shape = CursorShape::from_ini(value),
            "cursor.nonblinking" => {
                self.cursor_nonblinking = crate::schema::on_off(Some(value), false)
            }
            "window.change_allowed" => {
                self.window_change_allowed = crate::schema::on_off(Some(value), true)
            }
            "window.report_allowed" => {
                self.window_report_allowed = crate::schema::on_off(Some(value), true)
            }
            "window.cursor_ctrl_allowed" => {
                self.window_cursor_ctrl_allowed = crate::schema::on_off(Some(value), false)
            }
            "window.accept_8bit_ctrl" => {
                self.window_accept_8bit_ctrl = crate::schema::on_off(Some(value), true)
            }
            "window.send_8bit_ctrl" => {
                self.window_send_8bit_ctrl = crate::schema::on_off(Some(value), false)
            }
            "window.alt_screen" => {
                self.window_alt_screen = crate::schema::on_off(Some(value), true)
            }
            "window.remote_clears_buffer" => {
                self.window_remote_clears_buffer = crate::schema::on_off(Some(value), true)
            }
            "mouse.tracking" => self.mouse_tracking = crate::schema::on_off(Some(value), true),
            "mouse.ctrl_disables_tracking" => {
                self.mouse_ctrl_disables_tracking = crate::schema::on_off(Some(value), true)
            }
            "mouse.wheel_to_cursor" => {
                self.mouse_wheel_to_cursor = crate::schema::on_off(Some(value), true)
            }
            "connection.port_type" => {
                self.connection_port_type = ConnectionPortType::from_ini(value)
            }
            "connection.tcp_port" => {
                self.connection_tcp_port = crate::schema::int(value, self.connection_tcp_port)
            }
            "connection.telnet" => {
                self.connection_telnet = crate::schema::on_off(Some(value), true)
            }
            "connection.telnet_port" => {
                self.connection_telnet_port = crate::schema::int(value, self.connection_telnet_port)
            }
            "connection.telnet_binary" => {
                self.connection_telnet_binary = crate::schema::on_off(Some(value), false)
            }
            "connection.auto_win_close" => {
                self.connection_auto_win_close = crate::schema::on_off(Some(value), true)
            }
            "connection.timeout" => {
                self.connection_timeout = crate::schema::int(value, self.connection_timeout)
            }
            "connection.host_dialog_on_startup" => {
                self.connection_host_dialog_on_startup = crate::schema::on_off(Some(value), true)
            }
            "serial.com_port" => {
                self.serial_com_port = crate::schema::int(value, self.serial_com_port)
            }
            "serial.baud" => self.serial_baud = crate::schema::int(value, self.serial_baud),
            "serial.data_bits" => self.serial_data_bits = SerialDataBits::from_ini(value),
            "serial.parity" => self.serial_parity = SerialParity::from_ini(value),
            "serial.stop_bits" => self.serial_stop_bits = SerialStopBits::from_ini(value),
            "serial.flow" => self.serial_flow = SerialFlow::from_ini(value),
            "serial.delay_per_char" => {
                self.serial_delay_per_char = crate::schema::int(value, self.serial_delay_per_char)
            }
            "serial.delay_per_line" => {
                self.serial_delay_per_line = crate::schema::int(value, self.serial_delay_per_line)
            }
            "serial.wait_com" => self.serial_wait_com = crate::schema::on_off(Some(value), false),
            "serial.max_com_port" => {
                self.serial_max_com_port = crate::schema::ranged(
                    crate::schema::int(value, self.serial_max_com_port),
                    256,
                    4,
                    4096,
                )
            }
            "log.auto_start" => self.log_auto_start = crate::schema::on_off(Some(value), false),
            "transfer.dir" => self.transfer_dir = value.to_string(),
            "window.hide_title" => {
                self.window_hide_title = crate::schema::on_off(Some(value), false)
            }
            _ => return false,
        }
        true
    }
}

/// Every setting, as data — for the dialog that builds itself from it,
/// for `setsetting`/`getsetting`, and for the documentation table.
///
/// This is the point of the schema: the list exists once.
pub const FIELDS: &[Field] = &[
    Field {
        name: "terminal.cols",
        page: "terminal",
        section: "Tera Term",
        key: "TerminalSize",
        kind: Kind::IntRange(1, 1000),
        default: "80",
        label: Some("DLG_TABSHEET_TITLE_TERM"),
        doc: "`ttset.c:615`, bounded by `TermWidthMax` — which is **1000**, not the 500 this used to say; 500 is `TermHeightMax`, the next line of `tttypes.h:633`. Zero or less takes the default rather than the floor, which is what the range means here.",
    },
    Field {
        name: "terminal.rows",
        page: "terminal",
        section: "Tera Term",
        key: "TerminalSize",
        kind: Kind::IntRange(1, 500),
        default: "24",
        label: Some("DLG_TABSHEET_TITLE_TERM"),
        doc: "`ttset.c:619`, the second half of the same key, bounded by `TermHeightMax`.",
    },
    Field {
        name: "terminal.id",
        page: "terminal",
        section: "Tera Term",
        key: "TerminalID",
        kind: Kind::Enum(&["VT100", "VT100J", "VT101", "VT102", "VT102J", "VT220", "VT220J", "VT282", "VT320", "VT382", "VT420", "VT520", "VT525", "dumb"]),
        default: "VT100",
        label: Some("DLG_TERM_TERMID"),
        doc: "`ts.TerminalID`. `ttset.c:709` reads the key with an empty default and hands it to `TermIDGetID`, which is a case-sensitive `strcmp` (`tttypes_termid.cpp:60`) against the table above it, returning `IdVT100` for anything it does not recognise — so it never fails, a typo silently runs as a VT100, and so does `TerminalID=vt320` in the wrong case. The one enumerated setting here that is not `_stricmp`, hence `enum_exact`. Note `dumb` is lower-case in upstream's own table while every other spelling is upper.",
    },
    Field {
        name: "terminal.cr_receive",
        page: "terminal",
        section: "Tera Term",
        key: "CRReceive",
        kind: Kind::Enum(&["CR", "CRLF", "LF", "AUTO"]),
        default: "CR",
        label: Some("DLG_TERM_CRRECEIVE"),
        doc: "**`ttset.c:631`, and the default is the `else` branch.** A bare CR is a carriage *return*, not a newline, so `\"Hello\\rWorld\"` overwrites the line. Reading this as CRLF shifts every row of every dump.",
    },
    Field {
        name: "terminal.cr_send",
        page: "terminal",
        section: "Tera Term",
        key: "CRSend",
        kind: Kind::Enum(&["CR", "CRLF", "LF"]),
        default: "CR",
        label: Some("DLG_TERM_CRSEND"),
        doc: "`ttset.c:646`, the same shape, one variant short — there is no AUTO on send.",
    },
    Field {
        name: "terminal.local_echo",
        page: "terminal",
        section: "Tera Term",
        key: "LocalEcho",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TERM_LOCALECHO"),
        doc: "`ttset.c:660`.",
    },
    Field {
        name: "terminal.size_follows_window",
        page: "terminal",
        section: "Tera Term",
        key: "TermIsWin",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TERM_TERMISWIN"),
        doc: "`ttset.c:625`. With it on, resizing the window resizes the terminal.",
    },
    Field {
        name: "terminal.auto_win_resize",
        page: "terminal",
        section: "Tera Term",
        key: "AutoWinResize",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:628`. With it on, a remote resize resizes the window.",
    },
    Field {
        name: "terminal.scrollback_enabled",
        page: "terminal",
        section: "Tera Term",
        key: "EnableScrollBuff",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TERM_SCROLLBUFF"),
        doc: "`ttset.c:743`. Off keeps the terminal to one screen and no history.",
    },
    Field {
        name: "terminal.scrollback_lines",
        page: "terminal",
        section: "Tera Term",
        key: "ScrollBuffSize",
        kind: Kind::Int,
        default: "100",
        label: Some("DLG_TERM_SCROLLBUFF"),
        doc: "`ttset.c:751`. Upstream ships **100** lines, which is small for a console log; it is a setting rather than a constant precisely so it can be raised.",
    },
    Field {
        name: "terminal.title",
        page: "terminal",
        section: "Tera Term",
        key: "Title",
        kind: Kind::Str,
        default: "Tera Term",
        label: Some("DLG_TERM_TITLE"),
        doc: "`ts.Title`, `ttset.c:713`. The window title before the host sends one.",
    },
    Field {
        name: "keyboard.backspace",
        page: "keyboard",
        section: "Tera Term",
        key: "BSKey",
        kind: Kind::Enum(&["BS", "DEL"]),
        default: "BS",
        label: Some("DLG_KEYB_BSKEY"),
        doc: "**`ttset.c:877` reads this with an empty fallback and only the literal `DEL` takes the other arm**, so an absent key means BS. That is Tera Term's default and it is probably not what a Linux user wants: a `getty` usually has `stty erase` set to `^?`, so backspace echoes instead of erasing until the host sets DECBKM. Faithful by default, and changeable — which is the whole point of this file existing.",
    },
    Field {
        name: "keyboard.meta",
        page: "keyboard",
        section: "Tera Term",
        key: "MetaKey",
        kind: Kind::Enum(&["off", "on", "left", "right"]),
        default: "off",
        label: Some("DLG_KEYB_METAKEY"),
        doc: "`ttset.c:887`. Upstream ships this **off**, and every Linux line editor and Emacs expects Alt to prefix an ESC. The shell has been diverging from upstream here since it was written; this is where that divergence becomes a setting instead of a hard-coded opinion.",
    },
    Field {
        name: "keyboard.delete_sends_del",
        page: "keyboard",
        section: "Tera Term",
        key: "DeleteKey",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_KEYB_DELKEY"),
        doc: "`ttset.c:882`. Whether the Delete key sends DEL rather than the VT220 Remove sequence.",
    },
    Field {
        name: "keyboard.word_delimiters",
        page: "keyboard",
        section: "Tera Term",
        key: "DelimList",
        kind: Kind::Str,
        default: "$20!\"#$24%&'()*+,-./:;<=>?@[\\]^`{|}~",
        label: None,
        doc: "`ttset.c:1167`, and the value is **hex-escaped** (`Hex2StrW`): `$20` is a space. The default is a space plus every ASCII punctuation mark except underscore, which is what makes `some_name` one word and `some-name` three when you double-click it.",
    },
    Field {
        name: "color.normal",
        page: "color",
        section: "Tera Term",
        key: "VTColor",
        kind: Kind::Color2,
        default: "0,0,0,255,255,255",
        label: Some("DLG_TAB_VISUAL_FGCOLOR"),
        doc: "`ttset.c:754`. Black on white, which is what Tera Term looks like out of the box and surprises people who expect a terminal to be dark.",
    },
    Field {
        name: "color.bold",
        page: "color",
        section: "Tera Term",
        key: "VTBoldColor",
        kind: Kind::Color2,
        default: "0,0,255,255,255,255",
        label: Some("DLG_TAB_VISUAL_FGCOLOR"),
        doc: "`ttset.c:757`. Blue.",
    },
    Field {
        name: "color.blink",
        page: "color",
        section: "Tera Term",
        key: "VTBlinkColor",
        kind: Kind::Color2,
        default: "255,0,0,255,255,255",
        label: Some("DLG_TAB_VISUAL_FGCOLOR"),
        doc: "`ttset.c:762`. Red.",
    },
    Field {
        name: "color.underline",
        page: "color",
        section: "Tera Term",
        key: "VTUnderlineColor",
        kind: Kind::Color2,
        default: "255,0,255,255,255,255",
        label: Some("DLG_TAB_VISUAL_FGCOLOR"),
        doc: "`ttset.c:786`. Magenta.",
    },
    Field {
        name: "color.reverse",
        page: "color",
        section: "Tera Term",
        key: "VTReverseColor",
        kind: Kind::Color2,
        default: "255,255,255,0,0,0",
        label: Some("DLG_TAB_VISUAL_FGCOLOR"),
        doc: "`ttset.c:767`. White on black — and unlike the other three this one ships **off**, so reverse video uses the normal pair swapped instead.",
    },
    Field {
        name: "color.bold_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableBoldAttrColor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:758`.",
    },
    Field {
        name: "color.blink_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableBlinkAttrColor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:763`.",
    },
    Field {
        name: "color.reverse_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableReverseAttrColor",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:768`. **Off**, which is why `SGR 7` swaps the normal pair rather than using the colours above.",
    },
    Field {
        name: "color.underline_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableUnderlineAttrColor",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:790`. Whether `SGR 4` gets its own colour pair at all.",
    },
    Field {
        name: "color.xterm_256",
        page: "color",
        section: "Tera Term",
        key: "Xterm256Color",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:741`. **On**, and this is one of the four flag words `CLAUDE.md` warns about: `ColorFlag` is zeroed at the top of `ttset.c` and built up from per-key calls a thousand lines later, so reading the zero as the default turns 256-colour off and looks like a parser bug.",
    },
    Field {
        name: "color.aixterm_16",
        page: "color",
        section: "Tera Term",
        key: "Aixterm16Color",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:738`. **Off**, so `SGR 90-97` and `100-107` are ignored and the previous pen stands — which looks exactly like a painter bug.",
    },
    Field {
        name: "color.pc_bold_16",
        page: "color",
        section: "Tera Term",
        key: "PcBoldColor",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:735`. PC-style bold colour mapping.",
    },
    Field {
        name: "cursor.shape",
        page: "cursor",
        section: "Tera Term",
        key: "CursorShape",
        kind: Kind::Enum(&["block", "vertical", "horizontal"]),
        default: "block",
        label: Some("DLG_TAB_VISUAL_CURSOR"),
        doc: "`ttset.c:718`, and the default is again the `else` branch.",
    },
    Field {
        name: "cursor.nonblinking",
        page: "cursor",
        section: "Tera Term",
        key: "NonblinkingCursor",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TAB_VISUAL_CURSOR"),
        doc: "`ttset.c:1227`.",
    },
    Field {
        name: "window.change_allowed",
        page: "window",
        section: "Tera Term",
        key: "WindowChangeSequence",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1653`, part of `WindowFlag` — the same trap as `ColorFlag`. Gates every XTWINOPS operation that *changes* something, the resize included.",
    },
    Field {
        name: "window.report_allowed",
        page: "window",
        section: "Tera Term",
        key: "WindowReportSequence",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1661`. Gates the ones that answer back.",
    },
    Field {
        name: "window.cursor_ctrl_allowed",
        page: "window",
        section: "Tera Term",
        key: "CursorCtrlSequence",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1656`. **Off**, so DECSCUSR and `DECSET 12` do nothing until it is turned on, and DECRQM reports them reset.",
    },
    Field {
        name: "window.accept_8bit_ctrl",
        page: "window",
        section: "Tera Term",
        key: "Accept8BitCtrl",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1075`, part of `TermFlag`. Whether 8-bit C1 bytes are executed as controls.",
    },
    Field {
        name: "window.send_8bit_ctrl",
        page: "window",
        section: "Tera Term",
        key: "Send8BitCtrl",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1283`. Whether *replies* use 8-bit C1 introducers.",
    },
    Field {
        name: "window.alt_screen",
        page: "window",
        section: "Tera Term",
        key: "AltScreenBuffer",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1681`, part of `TermFlag`. The alternate screen, which `vim` and `less` need.",
    },
    Field {
        name: "window.remote_clears_buffer",
        page: "window",
        section: "Tera Term",
        key: "RemoteClearsBuffer",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1950`. Whether `ED 3` may discard the scrollback.",
    },
    Field {
        name: "mouse.tracking",
        page: "mouse",
        section: "Tera Term",
        key: "MouseEventTracking",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1523`. **On**, and it was left zeroed here for a while, which silently disabled every mouse mode and made DECRQM answer \"permanently reset\" for all of them.",
    },
    Field {
        name: "mouse.ctrl_disables_tracking",
        page: "mouse",
        section: "Tera Term",
        key: "DisableMouseTrackingByCtrl",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1591`. Holding Ctrl suppresses the report so text can still be selected in a full-screen application.",
    },
    Field {
        name: "mouse.wheel_to_cursor",
        page: "mouse",
        section: "Tera Term",
        key: "TranslateWheelToCursor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1515`. Gates `DECSET 7786`, and is what a reset restores it to.",
    },
    Field {
        name: "connection.port_type",
        page: "connection",
        section: "Tera Term",
        key: "Port",
        kind: Kind::Enum(&["serial", "tcpip"]),
        default: "tcpip",
        label: Some("DLG_HOST_TITLE"),
        doc: "`ttset.c:589`, and the default is the `else` branch: a `Port=` that says anything but `serial` is TCP/IP, including `Port=tcp` and `Port=`.",
    },
    Field {
        name: "connection.tcp_port",
        page: "connection",
        section: "Tera Term",
        key: "TCPPort",
        kind: Kind::Int,
        default: "23",
        label: Some("DLG_HOST_TCPIPPORT"),
        doc: "**`ttset.c:966`, and its default is an initialiser rather than the setting it looks like.** The call is `GetPrivateProfileInt(…, \"TCPPort\", ts->TelPort, …)` — but `TelPort=` is not read until `:1311`, four hundred lines later, so the value in hand is the hardcoded `ts->TelPort = 23` from `:566`. A file with `TelPort=2323` and no `TCPPort=` therefore opens port **23**, not 2323. Reading the file's `TelPort` as the default here is the obvious thing and it is wrong.",
    },
    Field {
        name: "connection.telnet",
        page: "connection",
        section: "Tera Term",
        key: "Telnet",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TCPIP_TELNET"),
        doc: "`ttset.c:958`, `GetOnOff(…, TRUE)` — so `Telnet=1` is **on** and `Telnet=off` is the only way to turn it off. See `schema.rs`'s `on_off` for why that is not the same as `Telnet=0`.",
    },
    Field {
        name: "connection.telnet_port",
        page: "connection",
        section: "Tera Term",
        key: "TelPort",
        kind: Kind::Int,
        default: "23",
        label: Some("DLG_TCPIP_PORT"),
        doc: "`ttset.c:1311`. What the New Connection dialog fills the port box with when Telnet is chosen, and — as above — *not* what `TCPPort` defaults to.",
    },
    Field {
        name: "connection.telnet_binary",
        page: "connection",
        section: "Tera Term",
        key: "TelBin",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1301`, `GetOnOff(…, FALSE)` — so here `TelBin=1` reads as **off**, which is the opposite of what the same value means for `Telnet=` above.",
    },
    Field {
        name: "connection.auto_win_close",
        page: "connection",
        section: "Tera Term",
        key: "AutoWinClose",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TCPIP_AUTOCLOSE"),
        doc: "`ttset.c:969`, on by default. Whether the window closes when the connection does. `/AUTOWINCLOSE=` on a command line is **not** `GetOnOff`: it tests for `on` and everything else is off, so the two readers disagree about `1`.",
    },
    Field {
        name: "connection.timeout",
        page: "connection",
        section: "Tera Term",
        key: "ConnectingTimeout",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1457`, in seconds, zero meaning the stack's own timeout. `/TIMEOUT=` refuses a negative value rather than clamping it.",
    },
    Field {
        name: "connection.host_dialog_on_startup",
        page: "connection",
        section: "Tera Term",
        key: "HostDialogOnStartup",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1520`, on by default — the New Connection dialog at startup, which `/DS` suppresses and `/ES` asks for.",
    },
    Field {
        name: "serial.com_port",
        page: "serial",
        section: "Tera Term",
        key: "ComPort",
        kind: Kind::Int,
        default: "1",
        label: Some("DLG_SERIAL_PORT"),
        doc: "`ttset.c:916`. **The bound is a different setting and is not a clamp**: `:1223` resets the port to 1 when it is below 1 or above `MaxComPort`, which is read at `:1218` — after this key but before the check. Left as a plain int here because the schema has no way to say \"bounded by that other setting\", and `PLAN.md` carries it as an open item.  What a number means on Linux is also open: this port opens a device path, and `/C=1` has to be resolved against enumeration rather than against `COM1`.",
    },
    Field {
        name: "serial.baud",
        page: "serial",
        section: "Tera Term",
        key: "BaudRate",
        kind: Kind::Int,
        default: "9600",
        label: Some("DLG_SERIAL_BAUD"),
        doc: "`ttset.c:919`. Unbounded, and the hardware decides what it can do with it — `tt-conn` reads the setting back after `tcsetattr` for exactly that reason.",
    },
    Field {
        name: "serial.data_bits",
        page: "serial",
        section: "Tera Term",
        key: "DataBit",
        kind: Kind::Enum(&["8", "7"]),
        default: "8",
        label: Some("DLG_SERIAL_DATA"),
        doc: "`ttset.c:929`, default `IdDataBit8` from the `if (!…)` arm. Upstream's dialog offers only these two; `tt-conn` can do 5 and 6 as well, which is a widening and not a setting.",
    },
    Field {
        name: "serial.parity",
        page: "serial",
        section: "Tera Term",
        key: "Parity",
        kind: Kind::Enum(&["none", "odd", "even", "mark", "space"]),
        default: "none",
        label: Some("DLG_SERIAL_PARITY"),
        doc: "`ttset.c:922`, default `IdParityNone`.",
    },
    Field {
        name: "serial.stop_bits",
        page: "serial",
        section: "Tera Term",
        key: "StopBit",
        kind: Kind::Enum(&["1", "2"]),
        default: "1",
        label: Some("DLG_SERIAL_STOP"),
        doc: "`ttset.c:936`, default `IdStopBit1`.",
    },
    Field {
        name: "serial.flow",
        page: "serial",
        section: "Tera Term",
        key: "FlowCtrl",
        kind: Kind::Enum(&["none", "x", "hard", "dsrdtr"]),
        default: "none",
        label: Some("DLG_SERIAL_FLOW"),
        doc: "`ttset.c:943`, default `IdFlowNone`. `rtscts` is a second spelling of `hard` and the only alias in any of the four tables (`ttset.c:111`); the schema lists the canonical one first, which is what gets written back.",
    },
    Field {
        name: "serial.delay_per_char",
        page: "serial",
        section: "Tera Term",
        key: "DelayPerChar",
        kind: Kind::Int,
        default: "0",
        label: Some("DLG_SERIAL_DELAYCHAR"),
        doc: "`ttset.c:951`, milliseconds between characters — for a device that cannot keep up with a paste.",
    },
    Field {
        name: "serial.delay_per_line",
        page: "serial",
        section: "Tera Term",
        key: "DelayPerLine",
        kind: Kind::Int,
        default: "0",
        label: Some("DLG_SERIAL_DELAYLINE"),
        doc: "`ttset.c:955`, milliseconds between lines.",
    },
    Field {
        name: "serial.wait_com",
        page: "serial",
        section: "Tera Term",
        key: "WaitCom",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1151`. Wait for the port to appear instead of failing — a USB adapter that has not been plugged in yet.",
    },
    Field {
        name: "serial.max_com_port",
        page: "serial",
        section: "Tera Term",
        key: "MaxComPort",
        kind: Kind::IntRange(4, 4096),
        default: "256",
        label: None,
        doc: "`ttset.c:1218`, floored at 4 and capped at `MAXCOMPORT` (4096, `tttypes.h:908`) — so the range here is the *file's* and the floor is upstream's own. This is the setting `/C=` is bounded against, which is why the parser takes it as an argument.",
    },
    Field {
        name: "log.auto_start",
        page: "log",
        section: "Tera Term",
        key: "LogAutoStart",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1026`. Start logging as soon as the session opens. `/NOLOG` turns it off; `/L=` names the file, which is **not** a setting — `ts.LogFN` has no key of its own, only `LogDefaultName` for the dialog's suggestion.",
    },
    Field {
        name: "transfer.dir",
        page: "transfer",
        section: "Tera Term",
        key: "FileDir",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1060`. Where a file transfer starts looking, and where a protocol that names its own file puts it — `GetRecievePath`. `/FD=` sets it, but only if the directory exists.",
    },
    Field {
        name: "window.hide_title",
        page: "window",
        section: "Tera Term",
        key: "HideTitle",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:728`. No title bar, which `/H` also asks for. `/I` and `/V` — minimised and invisible — have no keys at all: `_ReadIniFile` zeroes both at `:554` and never reads one, so they are command-line-only.",
    },
];
