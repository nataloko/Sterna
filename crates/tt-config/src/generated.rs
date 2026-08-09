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

/// `ttset.c:1568`. How the title the host set and `terminal.title` combine, and
/// **`off` means the host's title is not even stored** (`vtterm.c:5112`), which
/// also switches off the title stack at `CSI 22 t` / `CSI 23 t`.
///
/// This is the first row to use `*`, and it needs it: the key is read with a
/// default of `overwrite` and then compared down a chain whose `else` is **off**,
/// so `AcceptTitleChangeRequest=ovewrite` is a terminal that ignores every OSC
/// title while an absent key is one that accepts them. Absent and misspelt are
/// two different settings, and only the second is the `else`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowTitleChange {
    /// `overwrite`, `on` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Overwrite,
    /// `ahead`
    Ahead,
    /// `last`
    Last,
    /// `off`
    Off,
}

impl WindowTitleChange {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Overwrite => "overwrite",
            Self::Ahead => "ahead",
            Self::Last => "last",
            Self::Off => "off",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Off`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("overwrite") || s.eq_ignore_ascii_case("on") {
            return Self::Overwrite;
        }
        if s.eq_ignore_ascii_case("ahead") {
            return Self::Ahead;
        }
        if s.eq_ignore_ascii_case("last") {
            return Self::Last;
        }
        if s.eq_ignore_ascii_case("off") {
            return Self::Off;
        }
        Self::Off
    }
}

impl Default for WindowTitleChange {
    fn default() -> Self {
        Self::Overwrite
    }
}

/// `ttset.c:1664`, and it is `WindowFlag` again — with the extra turn that
/// `IdTitleReportEmpty` is **24**, which is `WF_TITLEREPORT` entire. So the
/// shipped `empty` sets both bits, lands on the `default:` arm, and answers
/// `CSI 20 t` and `CSI 21 t` with an empty OSC string. That is deliberate: a
/// terminal that echoes its own title into the input stream lets anything which
/// can write to the screen put text in front of the shell. `accept` reports the
/// real title, combined with `window.title_change`'s four spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowTitleReport {
    /// `empty`
    Empty,
    /// `accept`
    Accept,
    /// `ignore`, `off` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Ignore,
}

impl WindowTitleReport {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Accept => "accept",
            Self::Ignore => "ignore",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("empty") {
            return Self::Empty;
        }
        if s.eq_ignore_ascii_case("accept") {
            return Self::Accept;
        }
        if s.eq_ignore_ascii_case("ignore") || s.eq_ignore_ascii_case("off") {
            return Self::Ignore;
        }
        Self::default()
    }
}

impl Default for WindowTitleReport {
    fn default() -> Self {
        Self::Empty
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

/// `ttset.c:1000`, **and the empty spelling is load-bearing.** Four `_stricmp`s
/// with local time as the `else` — except that an *absent or empty* value
/// consults `LogTimestampUTC` instead (`:1007`), the Tera Term 4 key this one
/// replaced. So `LogTimestampType=Local` alongside `LogTimestampUTC=on` is local
/// time, while no `LogTimestampType=` at all alongside the same key is UTC — and
/// the first of those is exactly the file a Tera Term 5 leaves behind when it
/// saves a Tera Term 4 one, since it writes the new key and does not remove the
/// old. An enum that collapsed absent into `Local` would read that file as UTC.
/// The cost is the one divergence: a *misspelt* value falls to local time
/// upstream and to the empty spelling here, because the schema has one fallback
/// and it is the default. It differs only in a file that misspells this key and
/// carries `LogTimestampUTC=on` as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogTimestampType {
    /// ``
    Unset,
    /// `Local`
    Local,
    /// `UTC`
    Utc,
    /// `LoggingElapsed`
    LoggingElapsed,
    /// `ConnectionElapsed`
    ConnectionElapsed,
}

impl LogTimestampType {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Unset => "",
            Self::Local => "Local",
            Self::Utc => "UTC",
            Self::LoggingElapsed => "LoggingElapsed",
            Self::ConnectionElapsed => "ConnectionElapsed",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("") {
            return Self::Unset;
        }
        if s.eq_ignore_ascii_case("Local") {
            return Self::Local;
        }
        if s.eq_ignore_ascii_case("UTC") {
            return Self::Utc;
        }
        if s.eq_ignore_ascii_case("LoggingElapsed") {
            return Self::LoggingElapsed;
        }
        if s.eq_ignore_ascii_case("ConnectionElapsed") {
            return Self::ConnectionElapsed;
        }
        Self::default()
    }
}

impl Default for LogTimestampType {
    fn default() -> Self {
        Self::Unset
    }
}

/// **`ttset.c:1039`, and the default is the `else` branch again** — plain
/// checksum, not CRC, which is the older and slower of the two and the one a
/// modern peer is least likely to want. The reader has arms for `crc`, `1k` and
/// `1ksum` only; the writer emits `checksum` (`ttset.c:2594`), a spelling the
/// reader has no arm for and which round-trips solely because anything
/// unmatched takes the default. Kept here as the default spelling for that
/// reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferXmodemOpt {
    /// `checksum`
    Checksum,
    /// `crc`
    Crc,
    /// `1k`
    Crc1K,
    /// `1ksum`
    Checksum1K,
}

impl TransferXmodemOpt {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Checksum => "checksum",
            Self::Crc => "crc",
            Self::Crc1K => "1k",
            Self::Checksum1K => "1ksum",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("checksum") {
            return Self::Checksum;
        }
        if s.eq_ignore_ascii_case("crc") {
            return Self::Crc;
        }
        if s.eq_ignore_ascii_case("1k") {
            return Self::Crc1K;
        }
        if s.eq_ignore_ascii_case("1ksum") {
            return Self::Checksum1K;
        }
        Self::default()
    }
}

impl Default for TransferXmodemOpt {
    fn default() -> Self {
        Self::Checksum
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
    /// `ttset.c:1212`, and upstream's comment on it is "special option" — there is
    /// no dialog. It is the ceiling `ScrollBuffSize` is held under, not a second
    /// depth: `buffer.c:511` caps the buffer's line count with it and `:4977` caps
    /// the *terminal's row count* with it too, so `MaxBuffSize=10` is a ten-row
    /// terminal however big the window is. Below 24 takes the default rather than
    /// the floor, which is the `TerminalSize` bound with no ceiling on it.
    pub terminal_buffer_max_lines: i32,
    /// `ttset.c:1875`. Which ISO-2022 shifts the terminal honours, as a
    /// comma-separated list — `SI`, `SO` (`LS0` and `LS1` are read-only aliases for
    /// those two), `LS2`, `LS3`, `LS1R`, `LS2R`, `LS3R`, `SS2`, `SS3` — each
    /// optionally led by `+` or `-`, plus `on`/`all` and `off`/`none`, which assign
    /// the whole word rather than one bit.
    ///
    /// A `string` rather than a type of its own: it is the only key in `ttset.c`
    /// shaped this way, and `ShiftFlags::parse_ini` already lives beside the bits it
    /// names. **The list starts from nothing whatever this default says** — the
    /// `"on"` is what upstream uses when the key is *absent*, and a key that is
    /// present starts at `ISO2022_SHIFT_NONE`, so `ISO2022ShiftFunction=-SS2` is a
    /// terminal with every shift disabled rather than all but one.
    pub terminal_iso2022_shifts: String,
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
    /// `ttset.c:903`, and a **veto rather than a mode**: DECNKM still sets, and
    /// DECRQM still reports it set, but the key encoding ignores it. So a host that
    /// switches the keypad to application mode gets the numeric one anyway and is
    /// not told. Named the way upstream names it, negation included, because a row
    /// called `app_keypad` would mean the opposite of the key it is written from.
    pub keyboard_disable_app_keypad: bool,
    /// `ttset.c:907`. The same veto for DECCKM and the cursor keys.
    pub keyboard_disable_app_cursor: bool,
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
    /// `ttset.c:784`. Whether `SGR 4` gets its own colour pair at all, and **on** —
    /// unlike its two neighbours above, whose keys carry the `Enable` prefix this one
    /// does not. `vtdisp.c:2412` is the only reader.
    pub color_underline_enabled: bool,
    /// `ttset.c:741`. **On**, and this is one of the four flag words `AGENTS.md`
    /// warns about: `ColorFlag` is zeroed at the top of `ttset.c` and built up from
    /// per-key calls a thousand lines later, so reading the zero as the default
    /// turns 256-colour off and looks like a parser bug.
    pub color_xterm_256: bool,
    /// `ttset.c:738`. **Off**, so `SGR 90-97` and `100-107` are ignored and the
    /// previous pen stands — which looks exactly like a painter bug.
    pub color_aixterm_16: bool,
    /// `ttset.c:735`. PC-style bold colour mapping.
    pub color_pc_bold_16: bool,
    /// `ttset.c:856`, and the last of the four `ColorFlag` bits. **On**, and it is
    /// not a parse gate like the three above it: `SGR 30-37` still stores its colour
    /// in the cell, and `vtdisp.c:2417` then declines to draw with it, so the screen
    /// is `color.normal` while the buffer says otherwise. The two reports that name
    /// a colour — DECRQSS' SGR (`vtterm.c:4332`) and `Co` in the termcap query
    /// (`:4451`) — go quiet with it, which is how a host is told.
    pub color_ansi_enabled: bool,
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
    /// `ttset.c:1568`. How the title the host set and `terminal.title` combine, and
    /// **`off` means the host's title is not even stored** (`vtterm.c:5112`), which
    /// also switches off the title stack at `CSI 22 t` / `CSI 23 t`.
    ///
    /// This is the first row to use `*`, and it needs it: the key is read with a
    /// default of `overwrite` and then compared down a chain whose `else` is **off**,
    /// so `AcceptTitleChangeRequest=ovewrite` is a terminal that ignores every OSC
    /// title while an absent key is one that accepts them. Absent and misspelt are
    /// two different settings, and only the second is the `else`.
    pub window_title_change: WindowTitleChange,
    /// `ttset.c:1664`, and it is `WindowFlag` again — with the extra turn that
    /// `IdTitleReportEmpty` is **24**, which is `WF_TITLEREPORT` entire. So the
    /// shipped `empty` sets both bits, lands on the `default:` arm, and answers
    /// `CSI 20 t` and `CSI 21 t` with an empty OSC string. That is deliberate: a
    /// terminal that echoes its own title into the input stream lets anything which
    /// can write to the screen put text in front of the shell. `accept` reports the
    /// real title, combined with `window.title_change`'s four spellings.
    pub window_title_report: WindowTitleReport,
    /// `ttset.c:1523`. **On**, and it was left zeroed here for a while, which
    /// silently disabled every mouse mode and made DECRQM answer "permanently
    /// reset" for all of them.
    pub mouse_tracking: bool,
    /// `ttset.c:1591`. Holding Ctrl suppresses the report so text can still be
    /// selected in a full-screen application.
    pub mouse_ctrl_disables_tracking: bool,
    /// `ttset.c:1515`. Gates `DECSET 7786`, and is what a reset restores it to.
    pub mouse_wheel_to_cursor: bool,
    /// `ttset.c:1419`. Two things at once, and the second is not about copying.
    /// With it on a wrapped line is marked continued, so selecting from column 0
    /// takes the whole logical line and copying it joins the rows — **and the `CR`
    /// and `LF` that the wrap feeds to the log and the macro tap are suppressed**.
    /// That is the `logFlag` argument threaded through `CarriageReturn` and
    /// `LineFeed` (`vtterm.c:677`, `:695`): it is TRUE for a CR or LF that came off
    /// the wire and FALSE for the pair the terminal generated itself, and only the
    /// generated pair is dropped. So a macro's `wait` matches a wrapped line as one
    /// line, which is the whole point of the setting.
    pub clipboard_continued_line_copy: bool,
    /// `ttset.c:1105`. Copy the selection the moment the button comes up, with no
    /// Ctrl-Insert — which is what this shell has always done to the X11 primary
    /// selection, and now does from a key rather than from an opinion.
    pub clipboard_auto_copy: bool,
    /// `ttset.c:1280`, `ts.SelOnActive`. Off **eats** the click that activates the
    /// window (`vtwin.cpp:2387` returns `MA_ACTIVATEANDEAT`), so bringing the
    /// terminal forward cannot start a selection by accident.
    pub clipboard_select_on_activate: bool,
    /// `ttset.c:1449`. On, only the left button starts a selection — and a middle
    /// or right button coming up over a standing selection does **not** copy it
    /// (`vtwin.cpp:819`), which is the half of the setting its name does not say
    /// and the bug it was added to fix.
    pub clipboard_select_only_by_lbutton: bool,
    /// `ttset.c:1954`, `ts.SelectStartDelay`, in milliseconds. How long the button
    /// is held before a drag counts as a selection rather than as a click.
    pub clipboard_select_start_delay: i32,
    /// `ttset.c:1422`, `CPF_DISABLE_RBUTTON`. **Upstream pastes on the right button
    /// by default** — the arm is the `else` of this test (`vtwin.cpp:2645`), so a
    /// right-click over the terminal puts the clipboard on the wire.
    pub clipboard_paste_rbutton_disabled: bool,
    /// `ttset.c:1425`, `CPF_DISABLE_MBUTTON`, and the **on** is upstream's: Tera
    /// Term does not paste on the middle button, because on a wheel mouse that is
    /// the wheel. This shell did, on the X11 convention, and the divergence ends
    /// here the way `keyboard.meta`'s did — faithful by default and one line in the
    /// file away from the other behaviour. Note the two buttons ship opposite ways
    /// round from what a Linux user expects of either.
    pub clipboard_paste_mbutton_disabled: bool,
    /// `ttset.c:1428`, `CPF_CONFIRM_RBUTTON`. The right button raises a menu with
    /// Paste on it instead of pasting, and the button-up paste is then suppressed
    /// as well (`vtwin.cpp:2645` tests both bits).
    pub clipboard_confirm_paste_rbutton: bool,
    /// `ttset.c:1431`, `CPF_CONFIRM_CHANGEPASTE`. **On.** A paste holding a line
    /// break is shown in a dialog first and can be edited there
    /// (`clipboar.c:126`), because a newline pasted into a shell runs whatever
    /// came before it.
    pub clipboard_confirm_paste: bool,
    /// `ttset.c:1434`, `CPF_CONFIRM_CHANGEPASTE_CR`. The same confirmation for
    /// "paste and send a CR", where the newline is the one being *added* rather
    /// than one already in the text. Only consulted on that path, so a plain paste
    /// of text with no break is never confirmed by it.
    pub clipboard_confirm_paste_cr: bool,
    /// `ttset.c:1437`. A file of strings, one per line: a paste containing any of
    /// them is confirmed even with no line break in it. Resolved against the home
    /// directory rather than the working one (`GetFullPathW(ts.HomeDirW, …)`), and
    /// consulted only when `clipboard.confirm_paste` is on.
    pub clipboard_confirm_paste_dictionary: String,
    /// `ttset.c:1871`, `CPF_TRIM_TRAILING_NL`. Off, so the newline on the end of a
    /// copied line is pasted and the shell runs the line.
    pub clipboard_trim_trailing_newline: bool,
    /// `ttset.c:1633`, milliseconds between the lines of a paste — for a host with
    /// no flow control that drops what arrives while it is still echoing. The only
    /// setting in the file clamped at **both** ends; see `int_clamp` above for why
    /// that is a third bound rather than one of the other two.
    pub clipboard_paste_delay_per_line: i32,
    /// `ttset.c:1580`. Upstream writes the size back when the confirmation dialog
    /// is resized, which is the whole reason it is a setting. Below zero takes the
    /// default and there is no ceiling, so it is the `TerminalSize` bound rather
    /// than a clamp.
    pub clipboard_paste_dialog_width: i32,
    /// The second half of the same key, with a default of its own.
    pub clipboard_paste_dialog_height: i32,
    /// `ttset.c:2002`. **A second gate on `DECSET 2004`**, and the one to know
    /// about: `clipboar.c:265` tests the setting *and* the mode, so a host that has
    /// asked for bracketed paste gets an unbracketed one when this is off. It ships
    /// on, so the mode alone is usually the answer — which is exactly why a port
    /// that omits the key looks right until somebody turns it off.
    pub clipboard_bracketed: bool,
    /// `ttset.c:2003`. Brackets only a paste that **contains a control character**
    /// (`iswcntrl`, `clipboar.c:270`) — so a pasted word goes bare and a pasted
    /// block is bracketed. The test runs while the line breaks are still CR LF and
    /// again gives the same answer once they are CR, so any multi-line paste
    /// qualifies either way.
    pub clipboard_bracketed_control_only: bool,
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
    /// `ttset.c:961`. What `TERMINAL-TYPE` answers with, and what an SSH session
    /// sends as `TERM`. Upstream ships plain **`xterm`**, which this port had been
    /// diverging from with a hardcoded `xterm-256color` — a defensible choice and
    /// not one a hardcoded string should be making, since the answer decides what
    /// every curses program on the far end believes about the terminal.
    pub connection_term_type: String,
    /// `ttset.c:1936`. One number or two, `input,output`, for `TERMINAL-SPEED`.
    ///
    /// A string rather than an `int` pair, because **the second field's default is
    /// the first field's value** and the schema has no way to say that: `GetNthNum`
    /// gives 0 for a field that is not there (`ttlib_static_cpp.cpp:1182`) and
    /// `ttset.c:1946` then assigns the input speed. Two `int` rows would have to
    /// default the second to something, and any constant makes `TerminalSpeed=57600`
    /// a terminal claiming two different speeds. Zero or less takes 38400 for the
    /// first field and the first field for the second, so `TerminalSpeed=0,0` is
    /// 38400 both ways.
    pub connection_terminal_speed: String,
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
    /// `ttset.c:1026`. Start logging as soon as the session opens, under
    /// `LogDefaultName` in `LogDefaultPath`. `/NOLOG` turns it off; `/L=` names the
    /// file, which is **not** a setting — `ts.LogFN` has no key of its own.
    pub log_auto_start: bool,
    /// `ttset.c:978`. A byte-for-byte capture of what arrived, rather than the text
    /// the terminal decided to display — the only mode that can be replayed back
    /// through a terminal, and the only one that keeps what a corrupt line really
    /// sent. `filesys_log.cpp:243` overrules `LogTypePlainText` and `LogTimestamp`
    /// when this is on.
    pub log_binary: bool,
    /// `ttset.c:981`, and the key is not the field: it is `ts.Append`, which is why
    /// grepping the struct for `LogAppend` finds nothing. Add to an existing file
    /// rather than truncating it.
    pub log_append: bool,
    /// `ttset.c:984`, and it is **not** "strip the escape sequences" — a text log
    /// never had any, because the tap is downstream of the parser. It is one byte:
    /// `vtterm.c:666` and `:671` put a BS in the stream when a backspace moved the
    /// cursor, and this suppresses it, so a line the host corrected reads as the
    /// correction rather than as the keystrokes. The tap is shared with the macro
    /// language's received-line buffer, so it changes what `wait` sees too.
    pub log_plain_text: bool,
    /// `ttset.c:988`. A `[time] ` at the head of each line. Silently dropped for a
    /// binary log (`filesys_log.cpp:243`), which is the right way round: a timestamp
    /// in the middle of a byte capture makes it no longer a capture.
    pub log_timestamp: bool,
    /// `ttset.c:1000`, **and the empty spelling is load-bearing.** Four `_stricmp`s
    /// with local time as the `else` — except that an *absent or empty* value
    /// consults `LogTimestampUTC` instead (`:1007`), the Tera Term 4 key this one
    /// replaced. So `LogTimestampType=Local` alongside `LogTimestampUTC=on` is local
    /// time, while no `LogTimestampType=` at all alongside the same key is UTC — and
    /// the first of those is exactly the file a Tera Term 5 leaves behind when it
    /// saves a Tera Term 4 one, since it writes the new key and does not remove the
    /// old. An enum that collapsed absent into `Local` would read that file as UTC.
    /// The cost is the one divergence: a *misspelt* value falls to local time
    /// upstream and to the empty spelling here, because the schema has one fallback
    /// and it is the default. It differs only in a file that misspells this key and
    /// carries `LogTimestampUTC=on` as well.
    pub log_timestamp_type: LogTimestampType,
    /// `ttset.c:1007`. Tera Term 4's key, read only when `LogTimestampType` is absent
    /// or empty — see that setting. Upstream reads it and never writes it back, so a
    /// file keeps it until somebody deletes it by hand.
    pub log_timestamp_utc: bool,
    /// `ttset.c:996`, handed to `wcsftime` — plus `%N`, which is upstream's own
    /// addition for fractional seconds and not a strftime conversion. Not applied to
    /// an elapsed-time stamp, which has no date in it to format.
    pub log_timestamp_format: String,
    /// `ttset.c:1018`, the name `LogAutoStart` and the log dialog both start from.
    /// It is a **template**: `strftime` conversions, then `&h` for the host (`COMn`
    /// on a serial line), `&p` for the TCP port and `&u` for the user name.
    pub log_default_name: String,
    /// `ttset.c:1023`. Where a relative log name lands. Empty falls back to
    /// `FileDir` if that exists and to the per-user log directory otherwise —
    /// `GetTermLogDir` (`ttlib_types.cpp:63`), which is why the transfer directory
    /// can decide where a log goes.
    pub log_default_path: String,
    /// `ttset.c:1029`. `rotate_mode`: 0 none, 1 by size (`tttypes.h:106`). Read with
    /// a plain `GetPrivateProfileInt` and **not bounded**, and `filesys_log.cpp:513`
    /// takes anything that is neither of the two as "do not rotate" — so this is an
    /// int here rather than a range, which would turn a 2 into a 1 and switch
    /// rotation on for a file that had it off.
    pub log_rotate: i32,
    /// `ttset.c:1030`, **in bytes whatever `LogRotateSizeType` says.** The dialog
    /// multiplies by 1024 per unit before storing (`log_pp.cpp:471`), so a value
    /// read back and scaled again turns the 1 MB the user asked for into a
    /// terabyte. Zero disables rotation, as does a `LogRotate` of 0.
    pub log_rotate_size: i32,
    /// `ttset.c:1031`. 0 bytes, 1 KB, 2 MB (`log_pp.cpp:72`) — the unit the *dialog*
    /// shows `LogRotateSize` in, and nothing else. It is stored so that reopening
    /// the dialog offers the number the user typed rather than its expansion.
    pub log_rotate_size_type: i32,
    /// `ttset.c:1032`. How many generations to keep: `file.1` is the newest.
    /// **Zero is not "none"** — `filesys_log.cpp:507` leaves `loopmax` at its
    /// hardcoded 10000, so an unset step with rotation on keeps ten thousand files.
    pub log_rotate_step: i32,
    /// `ttset.c:991`. Upstream puts a file-transfer-shaped progress window up for
    /// the duration of a log; this port has a status-bar indicator instead, so the
    /// setting is read and written and acts on nothing.
    pub log_hide_dialog: bool,
    /// `ttset.c:993`, and the field is `LogAllBuffIncludedInFirst`. Write the
    /// scrollback into the log before starting on live output. Read and written and
    /// **not acted on**: the function upstream does it with,
    /// `BuffGetAnyLineDataW`, truncates any line at its first wide character and at
    /// about half the width when a line holds combining marks — two of the five
    /// upstream bugs on file. It waits on those reports being answered.
    pub log_include_screen_buffer: bool,
    /// `ttset.c:1766`, and a `GetOnOff` whose default is **on** — so `=1` reads as
    /// on here where the same value reads as off for every setting above that ships
    /// off. Win32 share modes; nothing on this side opens the file exclusively, so
    /// it is read and written and acts on nothing.
    pub log_lock_exclusive: bool,
    /// `ttset.c:1035`, default **on**, the same asymmetry as `LogLockExclusive`.
    /// Upstream hands the write to a logging thread instead of blocking the one
    /// parsing the stream; here the write is buffered and the terminal's own read
    /// loop is not on the UI thread, so there is nothing to defer.
    pub log_deferred_write: bool,
    /// `ttset.c:1060`. Where a file transfer starts looking, and where a protocol
    /// that names its own file puts it — `GetRecievePath`. `/FD=` sets it, but only
    /// if the directory exists.
    pub transfer_dir: String,
    /// `ttset.c:975`. The Binary checkbox both file dialogs carry, remembered
    /// between transfers — `filesys.cpp:231` copies it into `fv->BinaryMode` and
    /// `:454` uses it to decide whether a send translates line endings. Off, so a
    /// send is text by default and CR becomes CRLF.
    pub transfer_binary: bool,
    /// `ttset.c:1097`, part of `FTFlag`. On receive, never overwrite: add `.1`,
    /// `.2`. Off, so upstream ships willing to replace a file it already has.
    pub transfer_auto_rename: bool,
    /// `ttset.c:1014`. Upstream's transfer progress window; this port has a status
    /// bar for it, so the setting is read and written and acts on nothing — the same
    /// arrangement as `log.hide_dialog`.
    pub transfer_hide_dialog: bool,
    /// **`ttset.c:1039`, and the default is the `else` branch again** — plain
    /// checksum, not CRC, which is the older and slower of the two and the one a
    /// modern peer is least likely to want. The reader has arms for `crc`, `1k` and
    /// `1ksum` only; the writer emits `checksum` (`ttset.c:2594`), a spelling the
    /// reader has no arm for and which round-trips solely because anything
    /// unmatched takes the default. Kept here as the default spelling for that
    /// reason.
    pub transfer_xmodem_opt: TransferXmodemOpt,
    /// `ttset.c:1051`. **On** — so XMODEM's own binary flag defaults the opposite
    /// way to `TransBin` above, which is the flag every other protocol uses.
    pub transfer_xmodem_binary: bool,
    /// `ttset.c:1054`. What upstream sends to the host to start a receive. Read and
    /// written; this port has no "send a command, then receive" path yet, and the
    /// empty default is upstream's, so nothing is lost by an empty one here.
    pub transfer_xmodem_rcv_command: String,
    /// `ttset.c:1384`, part of `LogFlag`. A per-protocol transfer log, which is
    /// `ttpfile`'s own diagnostic and not the session log.
    pub transfer_xmodem_log: bool,
    /// `ttset.c:1820`, and **these five floor at 1 rather than taking the default**
    /// — `int_min`, not `int`. `XmodemTimeouts=0,0,0,0,0` is five one-second
    /// timeouts. Field 1: how long to wait for the first block.
    pub transfer_xmodem_timeout_init: i32,
    /// `ttset.c:1824`. Field 2: the same, while still asking for CRC mode.
    pub transfer_xmodem_timeout_init_crc: i32,
    /// `ttset.c:1827`. Field 3.
    pub transfer_xmodem_timeout_short: i32,
    /// `ttset.c:1830`. Field 4.
    pub transfer_xmodem_timeout_long: i32,
    /// `ttset.c:1833`. Field 5.
    pub transfer_xmodem_timeout_vlong: i32,
    /// `ttset.c:1392`, and unlike XMODEM's this one ships with a value: `rb`.
    pub transfer_ymodem_rcv_command: String,
    /// `ttset.c:1388`.
    pub transfer_ymodem_log: bool,
    /// `ttset.c:1838`, the same five fields and the same floor as XMODEM's.
    pub transfer_ymodem_timeout_init: i32,
    /// `ttset.c:1842`.
    pub transfer_ymodem_timeout_init_crc: i32,
    /// `ttset.c:1845`.
    pub transfer_ymodem_timeout_short: i32,
    /// `ttset.c:1848`.
    pub transfer_ymodem_timeout_long: i32,
    /// `ttset.c:1851`.
    pub transfer_ymodem_timeout_vlong: i32,
    /// `ttset.c:1396`, part of `FTFlag`. Whether the terminal watches the stream for
    /// a peer's `ZRQINIT` and starts a receive by itself.
    pub transfer_zmodem_auto: bool,
    /// `ttset.c:1400`. The subpacket size when sending; `zmodem.c:780` floors it at
    /// 64 and caps it against the block-size ladder, so this is an upper bound
    /// rather than the value used.
    pub transfer_zmodem_data_len: i32,
    /// `ttset.c:1403`. How far ahead the sender may run before an ACK.
    pub transfer_zmodem_win_size: i32,
    /// `ttset.c:1407`, part of `FTFlag`. Escape control characters, for a link that
    /// eats them — a telnet server that has not been told `binary`, or a modem with
    /// software flow control in the path.
    pub transfer_zmodem_escape_ctl: bool,
    /// `ttset.c:1411`.
    pub transfer_zmodem_log: bool,
    /// `ttset.c:1415`.
    pub transfer_zmodem_rcv_command: String,
    /// `ttset.c:1857`. Four fields rather than five, and **the second floors at 0
    /// rather than 1** because 0 is meaningful there: it is how "never time out" is
    /// spelt. Field 1, the normal timeout on a serial link.
    pub transfer_zmodem_timeout_normal: i32,
    /// `ttset.c:1861`. Field 2, and **0 by default**: on a network link a stalled
    /// ZMODEM waits for the socket to notice rather than timing out itself.
    pub transfer_zmodem_timeout_tcpip: i32,
    /// `ttset.c:1865`. Field 3.
    pub transfer_zmodem_timeout_init: i32,
    /// `ttset.c:1868`. Field 4.
    pub transfer_zmodem_timeout_fin: i32,
    /// `ttset.c:1206`, part of `KermitOpt`. Long packets, which every Kermit written
    /// this century supports and which upstream still ships off.
    pub transfer_kermit_long_packet: bool,
    /// `ttset.c:1208`. Send the file's attributes in an `A` packet.
    pub transfer_kermit_file_attr: bool,
    /// `ttset.c:1204`.
    pub transfer_kermit_log: bool,
    /// **`ttset.c:1130`, and turning this on rewrites `Answerback`** — the arm below
    /// it sets the terminal's answerback to `DLE + + DLE 0`, which is B-Plus's own
    /// trigger, so a setting on the transfer page silently changes what the terminal
    /// replies to ENQ. Not reproduced: this port's answerback is not wired to it,
    /// and doing so from a settings load would be a surprise a user cannot see.
    pub transfer_bplus_auto: bool,
    /// `ttset.c:1139`, part of `FTFlag`.
    pub transfer_bplus_escape_ctl: bool,
    /// `ttset.c:1143`.
    pub transfer_bplus_log: bool,
    /// `ttset.c:1270`.
    pub transfer_quickvan_win_size: i32,
    /// `ttset.c:1266`.
    pub transfer_quickvan_log: bool,
    /// `ttset.c:2031`, in seconds. How long a `recvfile` capture waits for the line
    /// to go quiet before stopping — and **the clock starts at the first byte**
    /// (`raw.c:168`), so a capture the host never answers waits for ever whatever
    /// this says.
    pub transfer_raw_autostop: i32,
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
            terminal_buffer_max_lines: 10000,
            terminal_iso2022_shifts: String::from("on"),
            terminal_title: String::from("Tera Term"),
            keyboard_backspace: KeyboardBackspace::default(),
            keyboard_meta: KeyboardMeta::default(),
            keyboard_delete_sends_del: false,
            keyboard_disable_app_keypad: false,
            keyboard_disable_app_cursor: false,
            keyboard_word_delimiters: String::from("$20!\"#$24%&'()*+,-./:;<=>?@[\\]^`{|}~"),
            color_normal: [0, 0, 0, 255, 255, 255],
            color_bold: [0, 0, 255, 255, 255, 255],
            color_blink: [255, 0, 0, 255, 255, 255],
            color_underline: [255, 0, 255, 255, 255, 255],
            color_reverse: [255, 255, 255, 0, 0, 0],
            color_bold_enabled: true,
            color_blink_enabled: true,
            color_reverse_enabled: false,
            color_underline_enabled: true,
            color_xterm_256: true,
            color_aixterm_16: false,
            color_pc_bold_16: false,
            color_ansi_enabled: true,
            cursor_shape: CursorShape::default(),
            cursor_nonblinking: false,
            window_change_allowed: true,
            window_report_allowed: true,
            window_cursor_ctrl_allowed: false,
            window_accept_8bit_ctrl: true,
            window_send_8bit_ctrl: false,
            window_alt_screen: true,
            window_remote_clears_buffer: true,
            window_title_change: WindowTitleChange::default(),
            window_title_report: WindowTitleReport::default(),
            mouse_tracking: true,
            mouse_ctrl_disables_tracking: true,
            mouse_wheel_to_cursor: true,
            clipboard_continued_line_copy: false,
            clipboard_auto_copy: true,
            clipboard_select_on_activate: true,
            clipboard_select_only_by_lbutton: true,
            clipboard_select_start_delay: 0,
            clipboard_paste_rbutton_disabled: false,
            clipboard_paste_mbutton_disabled: true,
            clipboard_confirm_paste_rbutton: false,
            clipboard_confirm_paste: true,
            clipboard_confirm_paste_cr: true,
            clipboard_confirm_paste_dictionary: String::from(""),
            clipboard_trim_trailing_newline: false,
            clipboard_paste_delay_per_line: 10,
            clipboard_paste_dialog_width: 330,
            clipboard_paste_dialog_height: 220,
            clipboard_bracketed: true,
            clipboard_bracketed_control_only: false,
            connection_port_type: ConnectionPortType::default(),
            connection_tcp_port: 23,
            connection_telnet: true,
            connection_telnet_port: 23,
            connection_telnet_binary: false,
            connection_term_type: String::from("xterm"),
            connection_terminal_speed: String::from("38400"),
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
            log_binary: false,
            log_append: false,
            log_plain_text: false,
            log_timestamp: false,
            log_timestamp_type: LogTimestampType::default(),
            log_timestamp_utc: false,
            log_timestamp_format: String::from("%Y-%m-%d %H:%M:%S.%N"),
            log_default_name: String::from("teraterm.log"),
            log_default_path: String::from(""),
            log_rotate: 0,
            log_rotate_size: 0,
            log_rotate_size_type: 0,
            log_rotate_step: 0,
            log_hide_dialog: false,
            log_include_screen_buffer: false,
            log_lock_exclusive: true,
            log_deferred_write: true,
            transfer_dir: String::from(""),
            transfer_binary: false,
            transfer_auto_rename: false,
            transfer_hide_dialog: false,
            transfer_xmodem_opt: TransferXmodemOpt::default(),
            transfer_xmodem_binary: true,
            transfer_xmodem_rcv_command: String::from(""),
            transfer_xmodem_log: false,
            transfer_xmodem_timeout_init: 10,
            transfer_xmodem_timeout_init_crc: 3,
            transfer_xmodem_timeout_short: 10,
            transfer_xmodem_timeout_long: 20,
            transfer_xmodem_timeout_vlong: 60,
            transfer_ymodem_rcv_command: String::from("rb"),
            transfer_ymodem_log: false,
            transfer_ymodem_timeout_init: 10,
            transfer_ymodem_timeout_init_crc: 3,
            transfer_ymodem_timeout_short: 10,
            transfer_ymodem_timeout_long: 20,
            transfer_ymodem_timeout_vlong: 60,
            transfer_zmodem_auto: false,
            transfer_zmodem_data_len: 1024,
            transfer_zmodem_win_size: 32767,
            transfer_zmodem_escape_ctl: false,
            transfer_zmodem_log: false,
            transfer_zmodem_rcv_command: String::from("rz"),
            transfer_zmodem_timeout_normal: 10,
            transfer_zmodem_timeout_tcpip: 0,
            transfer_zmodem_timeout_init: 10,
            transfer_zmodem_timeout_fin: 3,
            transfer_kermit_long_packet: false,
            transfer_kermit_file_attr: false,
            transfer_kermit_log: false,
            transfer_bplus_auto: false,
            transfer_bplus_escape_ctl: false,
            transfer_bplus_log: false,
            transfer_quickvan_win_size: 8,
            transfer_quickvan_log: false,
            transfer_raw_autostop: 5,
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
            terminal_buffer_max_lines: crate::schema::ranged(
                ini.get_int("Tera Term", "MaxBuffSize", d.terminal_buffer_max_lines) as i32,
                d.terminal_buffer_max_lines,
                24,
                2147483647,
            ),
            terminal_iso2022_shifts: ini
                .get_or(
                    "Tera Term",
                    "ISO2022ShiftFunction",
                    &d.terminal_iso2022_shifts,
                )
                .to_string(),
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
            keyboard_disable_app_keypad: crate::schema::on_off(
                ini.get("Tera Term", "DisableAppKeypad"),
                false,
            ),
            keyboard_disable_app_cursor: crate::schema::on_off(
                ini.get("Tera Term", "DisableAppCursor"),
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
                ini.get("Tera Term", "UnderlineAttrColor"),
                true,
            ),
            color_xterm_256: crate::schema::on_off(ini.get("Tera Term", "Xterm256Color"), true),
            color_aixterm_16: crate::schema::on_off(ini.get("Tera Term", "Aixterm16Color"), false),
            color_pc_bold_16: crate::schema::on_off(ini.get("Tera Term", "PcBoldColor"), false),
            color_ansi_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableANSIColor"),
                true,
            ),
            cursor_shape: match ini.get("Tera Term", "CursorShape") {
                Some(v) => CursorShape::from_ini(v),
                None => d.cursor_shape,
            },
            cursor_nonblinking: crate::schema::on_off(
                ini.get("Tera Term", "NonblinkingCursor"),
                false,
            ),
            window_change_allowed: crate::schema::on_off(
                ini.get("Tera Term", "WindowCtrlSequence"),
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
            window_alt_screen: crate::schema::on_off(
                ini.get("Tera Term", "AlternateScreenBuffer"),
                true,
            ),
            window_remote_clears_buffer: crate::schema::on_off(
                ini.get("Tera Term", "ClearScrollBufferFromRemote"),
                true,
            ),
            window_title_change: match ini.get("Tera Term", "AcceptTitleChangeRequest") {
                Some(v) => WindowTitleChange::from_ini(v),
                None => d.window_title_change,
            },
            window_title_report: match ini.get("Tera Term", "TitleReportSequence") {
                Some(v) => WindowTitleReport::from_ini(v),
                None => d.window_title_report,
            },
            mouse_tracking: crate::schema::on_off(ini.get("Tera Term", "MouseEventTracking"), true),
            mouse_ctrl_disables_tracking: crate::schema::on_off(
                ini.get("Tera Term", "DisableMouseTrackingByCtrl"),
                true,
            ),
            mouse_wheel_to_cursor: crate::schema::on_off(
                ini.get("Tera Term", "TranslateWheelToCursor"),
                true,
            ),
            clipboard_continued_line_copy: crate::schema::on_off(
                ini.get("Tera Term", "EnableContinuedLineCopy"),
                false,
            ),
            clipboard_auto_copy: crate::schema::on_off(ini.get("Tera Term", "AutoTextCopy"), true),
            clipboard_select_on_activate: crate::schema::on_off(
                ini.get("Tera Term", "SelectOnActivate"),
                true,
            ),
            clipboard_select_only_by_lbutton: crate::schema::on_off(
                ini.get("Tera Term", "SelectOnlyByLButton"),
                true,
            ),
            clipboard_select_start_delay: ini.get_int(
                "Tera Term",
                "MouseSelectStartDelay",
                d.clipboard_select_start_delay,
            ) as i32,
            clipboard_paste_rbutton_disabled: crate::schema::on_off(
                ini.get("Tera Term", "DisablePasteMouseRButton"),
                false,
            ),
            clipboard_paste_mbutton_disabled: crate::schema::on_off(
                ini.get("Tera Term", "DisablePasteMouseMButton"),
                true,
            ),
            clipboard_confirm_paste_rbutton: crate::schema::on_off(
                ini.get("Tera Term", "ConfirmPasteMouseRButton"),
                false,
            ),
            clipboard_confirm_paste: crate::schema::on_off(
                ini.get("Tera Term", "ConfirmChangePaste"),
                true,
            ),
            clipboard_confirm_paste_cr: crate::schema::on_off(
                ini.get("Tera Term", "ConfirmChangePasteCR"),
                true,
            ),
            clipboard_confirm_paste_dictionary: ini
                .get_or(
                    "Tera Term",
                    "ConfirmChangePasteStringFile",
                    &d.clipboard_confirm_paste_dictionary,
                )
                .to_string(),
            clipboard_trim_trailing_newline: crate::schema::on_off(
                ini.get("Tera Term", "TrimTrailingNLonPaste"),
                false,
            ),
            clipboard_paste_delay_per_line: crate::schema::clamped(
                ini.get_int(
                    "Tera Term",
                    "PasteDelayPerLine",
                    d.clipboard_paste_delay_per_line,
                ) as i32,
                0,
                5000,
            ),
            clipboard_paste_dialog_width: crate::schema::ranged(
                crate::schema::nth_int(
                    ini.get("Tera Term", "PasteDialogSize"),
                    0,
                    d.clipboard_paste_dialog_width,
                ),
                d.clipboard_paste_dialog_width,
                0,
                2147483647,
            ),
            clipboard_paste_dialog_height: crate::schema::ranged(
                crate::schema::nth_int(
                    ini.get("Tera Term", "PasteDialogSize"),
                    1,
                    d.clipboard_paste_dialog_height,
                ),
                d.clipboard_paste_dialog_height,
                0,
                2147483647,
            ),
            clipboard_bracketed: crate::schema::on_off(
                ini.get("Tera Term", "BracketedSupport"),
                true,
            ),
            clipboard_bracketed_control_only: crate::schema::on_off(
                ini.get("Tera Term", "BracketedControlOnly"),
                false,
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
            connection_term_type: ini
                .get_or("Tera Term", "TermType", &d.connection_term_type)
                .to_string(),
            connection_terminal_speed: ini
                .get_or("Tera Term", "TerminalSpeed", &d.connection_terminal_speed)
                .to_string(),
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
            log_binary: crate::schema::on_off(ini.get("Tera Term", "LogBinary"), false),
            log_append: crate::schema::on_off(ini.get("Tera Term", "LogAppend"), false),
            log_plain_text: crate::schema::on_off(ini.get("Tera Term", "LogTypePlainText"), false),
            log_timestamp: crate::schema::on_off(ini.get("Tera Term", "LogTimestamp"), false),
            log_timestamp_type: match ini.get("Tera Term", "LogTimestampType") {
                Some(v) => LogTimestampType::from_ini(v),
                None => d.log_timestamp_type,
            },
            log_timestamp_utc: crate::schema::on_off(
                ini.get("Tera Term", "LogTimestampUTC"),
                false,
            ),
            log_timestamp_format: ini
                .get_or("Tera Term", "LogTimestampFormat", &d.log_timestamp_format)
                .to_string(),
            log_default_name: ini
                .get_or("Tera Term", "LogDefaultName", &d.log_default_name)
                .to_string(),
            log_default_path: ini
                .get_or("Tera Term", "LogDefaultPath", &d.log_default_path)
                .to_string(),
            log_rotate: ini.get_int("Tera Term", "LogRotate", d.log_rotate) as i32,
            log_rotate_size: ini.get_int("Tera Term", "LogRotateSize", d.log_rotate_size) as i32,
            log_rotate_size_type: ini.get_int(
                "Tera Term",
                "LogRotateSizeType",
                d.log_rotate_size_type,
            ) as i32,
            log_rotate_step: ini.get_int("Tera Term", "LogRotateStep", d.log_rotate_step) as i32,
            log_hide_dialog: crate::schema::on_off(ini.get("Tera Term", "LogHideDialog"), false),
            log_include_screen_buffer: crate::schema::on_off(
                ini.get("Tera Term", "LogIncludeScreenBuffer"),
                false,
            ),
            log_lock_exclusive: crate::schema::on_off(
                ini.get("Tera Term", "LogLockExclusive"),
                true,
            ),
            log_deferred_write: crate::schema::on_off(
                ini.get("Tera Term", "DeferredLogWriteMode"),
                true,
            ),
            transfer_dir: ini
                .get_or("Tera Term", "FileDir", &d.transfer_dir)
                .to_string(),
            transfer_binary: crate::schema::on_off(ini.get("Tera Term", "TransBin"), false),
            transfer_auto_rename: crate::schema::on_off(
                ini.get("Tera Term", "AutoFileRename"),
                false,
            ),
            transfer_hide_dialog: crate::schema::on_off(
                ini.get("Tera Term", "FTHideDialog"),
                false,
            ),
            transfer_xmodem_opt: match ini.get("Tera Term", "XmodemOpt") {
                Some(v) => TransferXmodemOpt::from_ini(v),
                None => d.transfer_xmodem_opt,
            },
            transfer_xmodem_binary: crate::schema::on_off(ini.get("Tera Term", "XmodemBin"), true),
            transfer_xmodem_rcv_command: ini
                .get_or(
                    "Tera Term",
                    "XModemRcvCommand",
                    &d.transfer_xmodem_rcv_command,
                )
                .to_string(),
            transfer_xmodem_log: crate::schema::on_off(ini.get("Tera Term", "XmodemLog"), false),
            transfer_xmodem_timeout_init: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "XmodemTimeouts"),
                    0,
                    d.transfer_xmodem_timeout_init,
                ),
                1,
            ),
            transfer_xmodem_timeout_init_crc: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "XmodemTimeouts"),
                    1,
                    d.transfer_xmodem_timeout_init_crc,
                ),
                1,
            ),
            transfer_xmodem_timeout_short: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "XmodemTimeouts"),
                    2,
                    d.transfer_xmodem_timeout_short,
                ),
                1,
            ),
            transfer_xmodem_timeout_long: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "XmodemTimeouts"),
                    3,
                    d.transfer_xmodem_timeout_long,
                ),
                1,
            ),
            transfer_xmodem_timeout_vlong: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "XmodemTimeouts"),
                    4,
                    d.transfer_xmodem_timeout_vlong,
                ),
                1,
            ),
            transfer_ymodem_rcv_command: ini
                .get_or(
                    "Tera Term",
                    "YModemRcvCommand",
                    &d.transfer_ymodem_rcv_command,
                )
                .to_string(),
            transfer_ymodem_log: crate::schema::on_off(ini.get("Tera Term", "YmodemLog"), false),
            transfer_ymodem_timeout_init: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "YmodemTimeouts"),
                    0,
                    d.transfer_ymodem_timeout_init,
                ),
                1,
            ),
            transfer_ymodem_timeout_init_crc: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "YmodemTimeouts"),
                    1,
                    d.transfer_ymodem_timeout_init_crc,
                ),
                1,
            ),
            transfer_ymodem_timeout_short: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "YmodemTimeouts"),
                    2,
                    d.transfer_ymodem_timeout_short,
                ),
                1,
            ),
            transfer_ymodem_timeout_long: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "YmodemTimeouts"),
                    3,
                    d.transfer_ymodem_timeout_long,
                ),
                1,
            ),
            transfer_ymodem_timeout_vlong: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "YmodemTimeouts"),
                    4,
                    d.transfer_ymodem_timeout_vlong,
                ),
                1,
            ),
            transfer_zmodem_auto: crate::schema::on_off(ini.get("Tera Term", "ZmodemAuto"), false),
            transfer_zmodem_data_len: ini.get_int(
                "Tera Term",
                "ZmodemDataLen",
                d.transfer_zmodem_data_len,
            ) as i32,
            transfer_zmodem_win_size: ini.get_int(
                "Tera Term",
                "ZmodemWinSize",
                d.transfer_zmodem_win_size,
            ) as i32,
            transfer_zmodem_escape_ctl: crate::schema::on_off(
                ini.get("Tera Term", "ZmodemEscCtl"),
                false,
            ),
            transfer_zmodem_log: crate::schema::on_off(ini.get("Tera Term", "ZmodemLog"), false),
            transfer_zmodem_rcv_command: ini
                .get_or(
                    "Tera Term",
                    "ZModemRcvCommand",
                    &d.transfer_zmodem_rcv_command,
                )
                .to_string(),
            transfer_zmodem_timeout_normal: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "ZmodemTimeouts"),
                    0,
                    d.transfer_zmodem_timeout_normal,
                ),
                1,
            ),
            transfer_zmodem_timeout_tcpip: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "ZmodemTimeouts"),
                    1,
                    d.transfer_zmodem_timeout_tcpip,
                ),
                0,
            ),
            transfer_zmodem_timeout_init: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "ZmodemTimeouts"),
                    2,
                    d.transfer_zmodem_timeout_init,
                ),
                1,
            ),
            transfer_zmodem_timeout_fin: crate::schema::floored(
                crate::schema::nth_int(
                    ini.get("Tera Term", "ZmodemTimeouts"),
                    3,
                    d.transfer_zmodem_timeout_fin,
                ),
                1,
            ),
            transfer_kermit_long_packet: crate::schema::on_off(
                ini.get("Tera Term", "KmtLongPacket"),
                false,
            ),
            transfer_kermit_file_attr: crate::schema::on_off(
                ini.get("Tera Term", "KmtFileAttr"),
                false,
            ),
            transfer_kermit_log: crate::schema::on_off(ini.get("Tera Term", "KmtLog"), false),
            transfer_bplus_auto: crate::schema::on_off(ini.get("Tera Term", "BPAuto"), false),
            transfer_bplus_escape_ctl: crate::schema::on_off(
                ini.get("Tera Term", "BPEscCtl"),
                false,
            ),
            transfer_bplus_log: crate::schema::on_off(ini.get("Tera Term", "BPLog"), false),
            transfer_quickvan_win_size: ini.get_int(
                "Tera Term",
                "QVWinSize",
                d.transfer_quickvan_win_size,
            ) as i32,
            transfer_quickvan_log: crate::schema::on_off(ini.get("Tera Term", "QVLog"), false),
            transfer_raw_autostop: ini.get_int(
                "Tera Term",
                "ReceivefileAutoStopWaitTime",
                d.transfer_raw_autostop,
            ) as i32,
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
        ini.set(
            "Tera Term",
            "MaxBuffSize",
            &self.terminal_buffer_max_lines.to_string(),
        );
        ini.set(
            "Tera Term",
            "ISO2022ShiftFunction",
            &self.terminal_iso2022_shifts.clone(),
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
            "DisableAppKeypad",
            &if self.keyboard_disable_app_keypad {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableAppCursor",
            &if self.keyboard_disable_app_cursor {
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
            "UnderlineAttrColor",
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
            "EnableANSIColor",
            &if self.color_ansi_enabled { "on" } else { "off" }.to_string(),
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
            "WindowCtrlSequence",
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
            "AlternateScreenBuffer",
            &if self.window_alt_screen { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "ClearScrollBufferFromRemote",
            &if self.window_remote_clears_buffer {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AcceptTitleChangeRequest",
            &self.window_title_change.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "TitleReportSequence",
            &self.window_title_report.as_ini().to_string(),
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
            "EnableContinuedLineCopy",
            &if self.clipboard_continued_line_copy {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoTextCopy",
            &if self.clipboard_auto_copy {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "SelectOnActivate",
            &if self.clipboard_select_on_activate {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "SelectOnlyByLButton",
            &if self.clipboard_select_only_by_lbutton {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "MouseSelectStartDelay",
            &self.clipboard_select_start_delay.to_string(),
        );
        ini.set(
            "Tera Term",
            "DisablePasteMouseRButton",
            &if self.clipboard_paste_rbutton_disabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisablePasteMouseMButton",
            &if self.clipboard_paste_mbutton_disabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmPasteMouseRButton",
            &if self.clipboard_confirm_paste_rbutton {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmChangePaste",
            &if self.clipboard_confirm_paste {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmChangePasteCR",
            &if self.clipboard_confirm_paste_cr {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmChangePasteStringFile",
            &self.clipboard_confirm_paste_dictionary.clone(),
        );
        ini.set(
            "Tera Term",
            "TrimTrailingNLonPaste",
            &if self.clipboard_trim_trailing_newline {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "PasteDelayPerLine",
            &self.clipboard_paste_delay_per_line.to_string(),
        );
        ini.set(
            "Tera Term",
            "PasteDialogSize",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PasteDialogSize"),
                0,
                self.clipboard_paste_dialog_width,
            ),
        );
        ini.set(
            "Tera Term",
            "PasteDialogSize",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PasteDialogSize"),
                1,
                self.clipboard_paste_dialog_height,
            ),
        );
        ini.set(
            "Tera Term",
            "BracketedSupport",
            &if self.clipboard_bracketed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "BracketedControlOnly",
            &if self.clipboard_bracketed_control_only {
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
        ini.set("Tera Term", "TermType", &self.connection_term_type.clone());
        ini.set(
            "Tera Term",
            "TerminalSpeed",
            &self.connection_terminal_speed.clone(),
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
        ini.set(
            "Tera Term",
            "LogBinary",
            &if self.log_binary { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogAppend",
            &if self.log_append { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogTypePlainText",
            &if self.log_plain_text { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogTimestamp",
            &if self.log_timestamp { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogTimestampType",
            &self.log_timestamp_type.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "LogTimestampUTC",
            &if self.log_timestamp_utc { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogTimestampFormat",
            &self.log_timestamp_format.clone(),
        );
        ini.set(
            "Tera Term",
            "LogDefaultName",
            &self.log_default_name.clone(),
        );
        ini.set(
            "Tera Term",
            "LogDefaultPath",
            &self.log_default_path.clone(),
        );
        ini.set("Tera Term", "LogRotate", &self.log_rotate.to_string());
        ini.set(
            "Tera Term",
            "LogRotateSize",
            &self.log_rotate_size.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogRotateSizeType",
            &self.log_rotate_size_type.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogRotateStep",
            &self.log_rotate_step.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogHideDialog",
            &if self.log_hide_dialog { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "LogIncludeScreenBuffer",
            &if self.log_include_screen_buffer {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "LogLockExclusive",
            &if self.log_lock_exclusive { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "DeferredLogWriteMode",
            &if self.log_deferred_write { "on" } else { "off" }.to_string(),
        );
        ini.set("Tera Term", "FileDir", &self.transfer_dir.clone());
        ini.set(
            "Tera Term",
            "TransBin",
            &if self.transfer_binary { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoFileRename",
            &if self.transfer_auto_rename {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "FTHideDialog",
            &if self.transfer_hide_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "XmodemOpt",
            &self.transfer_xmodem_opt.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "XmodemBin",
            &if self.transfer_xmodem_binary {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "XModemRcvCommand",
            &self.transfer_xmodem_rcv_command.clone(),
        );
        ini.set(
            "Tera Term",
            "XmodemLog",
            &if self.transfer_xmodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "XmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "XmodemTimeouts"),
                0,
                self.transfer_xmodem_timeout_init,
            ),
        );
        ini.set(
            "Tera Term",
            "XmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "XmodemTimeouts"),
                1,
                self.transfer_xmodem_timeout_init_crc,
            ),
        );
        ini.set(
            "Tera Term",
            "XmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "XmodemTimeouts"),
                2,
                self.transfer_xmodem_timeout_short,
            ),
        );
        ini.set(
            "Tera Term",
            "XmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "XmodemTimeouts"),
                3,
                self.transfer_xmodem_timeout_long,
            ),
        );
        ini.set(
            "Tera Term",
            "XmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "XmodemTimeouts"),
                4,
                self.transfer_xmodem_timeout_vlong,
            ),
        );
        ini.set(
            "Tera Term",
            "YModemRcvCommand",
            &self.transfer_ymodem_rcv_command.clone(),
        );
        ini.set(
            "Tera Term",
            "YmodemLog",
            &if self.transfer_ymodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "YmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "YmodemTimeouts"),
                0,
                self.transfer_ymodem_timeout_init,
            ),
        );
        ini.set(
            "Tera Term",
            "YmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "YmodemTimeouts"),
                1,
                self.transfer_ymodem_timeout_init_crc,
            ),
        );
        ini.set(
            "Tera Term",
            "YmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "YmodemTimeouts"),
                2,
                self.transfer_ymodem_timeout_short,
            ),
        );
        ini.set(
            "Tera Term",
            "YmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "YmodemTimeouts"),
                3,
                self.transfer_ymodem_timeout_long,
            ),
        );
        ini.set(
            "Tera Term",
            "YmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "YmodemTimeouts"),
                4,
                self.transfer_ymodem_timeout_vlong,
            ),
        );
        ini.set(
            "Tera Term",
            "ZmodemAuto",
            &if self.transfer_zmodem_auto {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ZmodemDataLen",
            &self.transfer_zmodem_data_len.to_string(),
        );
        ini.set(
            "Tera Term",
            "ZmodemWinSize",
            &self.transfer_zmodem_win_size.to_string(),
        );
        ini.set(
            "Tera Term",
            "ZmodemEscCtl",
            &if self.transfer_zmodem_escape_ctl {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ZmodemLog",
            &if self.transfer_zmodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ZModemRcvCommand",
            &self.transfer_zmodem_rcv_command.clone(),
        );
        ini.set(
            "Tera Term",
            "ZmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "ZmodemTimeouts"),
                0,
                self.transfer_zmodem_timeout_normal,
            ),
        );
        ini.set(
            "Tera Term",
            "ZmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "ZmodemTimeouts"),
                1,
                self.transfer_zmodem_timeout_tcpip,
            ),
        );
        ini.set(
            "Tera Term",
            "ZmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "ZmodemTimeouts"),
                2,
                self.transfer_zmodem_timeout_init,
            ),
        );
        ini.set(
            "Tera Term",
            "ZmodemTimeouts",
            &crate::schema::with_nth(
                ini.get("Tera Term", "ZmodemTimeouts"),
                3,
                self.transfer_zmodem_timeout_fin,
            ),
        );
        ini.set(
            "Tera Term",
            "KmtLongPacket",
            &if self.transfer_kermit_long_packet {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "KmtFileAttr",
            &if self.transfer_kermit_file_attr {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "KmtLog",
            &if self.transfer_kermit_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "BPAuto",
            &if self.transfer_bplus_auto {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "BPEscCtl",
            &if self.transfer_bplus_escape_ctl {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "BPLog",
            &if self.transfer_bplus_log { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "QVWinSize",
            &self.transfer_quickvan_win_size.to_string(),
        );
        ini.set(
            "Tera Term",
            "QVLog",
            &if self.transfer_quickvan_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ReceivefileAutoStopWaitTime",
            &self.transfer_raw_autostop.to_string(),
        );
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
            "terminal.buffer_max_lines" => self.terminal_buffer_max_lines.to_string(),
            "terminal.iso2022_shifts" => self.terminal_iso2022_shifts.clone(),
            "terminal.title" => self.terminal_title.clone(),
            "keyboard.backspace" => self.keyboard_backspace.as_ini().to_string(),
            "keyboard.meta" => self.keyboard_meta.as_ini().to_string(),
            "keyboard.delete_sends_del" => if self.keyboard_delete_sends_del {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "keyboard.disable_app_keypad" => if self.keyboard_disable_app_keypad {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "keyboard.disable_app_cursor" => if self.keyboard_disable_app_cursor {
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
            "color.ansi_enabled" => if self.color_ansi_enabled { "on" } else { "off" }.to_string(),
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
            "window.title_change" => self.window_title_change.as_ini().to_string(),
            "window.title_report" => self.window_title_report.as_ini().to_string(),
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
            "clipboard.continued_line_copy" => if self.clipboard_continued_line_copy {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.auto_copy" => if self.clipboard_auto_copy {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.select_on_activate" => if self.clipboard_select_on_activate {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.select_only_by_lbutton" => if self.clipboard_select_only_by_lbutton {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.select_start_delay" => self.clipboard_select_start_delay.to_string(),
            "clipboard.paste_rbutton_disabled" => if self.clipboard_paste_rbutton_disabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.paste_mbutton_disabled" => if self.clipboard_paste_mbutton_disabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.confirm_paste_rbutton" => if self.clipboard_confirm_paste_rbutton {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.confirm_paste" => if self.clipboard_confirm_paste {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.confirm_paste_cr" => if self.clipboard_confirm_paste_cr {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.confirm_paste_dictionary" => self.clipboard_confirm_paste_dictionary.clone(),
            "clipboard.trim_trailing_newline" => if self.clipboard_trim_trailing_newline {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.paste_delay_per_line" => self.clipboard_paste_delay_per_line.to_string(),
            "clipboard.paste_dialog_width" => self.clipboard_paste_dialog_width.to_string(),
            "clipboard.paste_dialog_height" => self.clipboard_paste_dialog_height.to_string(),
            "clipboard.bracketed" => if self.clipboard_bracketed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "clipboard.bracketed_control_only" => if self.clipboard_bracketed_control_only {
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
            "connection.term_type" => self.connection_term_type.clone(),
            "connection.terminal_speed" => self.connection_terminal_speed.clone(),
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
            "log.binary" => if self.log_binary { "on" } else { "off" }.to_string(),
            "log.append" => if self.log_append { "on" } else { "off" }.to_string(),
            "log.plain_text" => if self.log_plain_text { "on" } else { "off" }.to_string(),
            "log.timestamp" => if self.log_timestamp { "on" } else { "off" }.to_string(),
            "log.timestamp_type" => self.log_timestamp_type.as_ini().to_string(),
            "log.timestamp_utc" => if self.log_timestamp_utc { "on" } else { "off" }.to_string(),
            "log.timestamp_format" => self.log_timestamp_format.clone(),
            "log.default_name" => self.log_default_name.clone(),
            "log.default_path" => self.log_default_path.clone(),
            "log.rotate" => self.log_rotate.to_string(),
            "log.rotate_size" => self.log_rotate_size.to_string(),
            "log.rotate_size_type" => self.log_rotate_size_type.to_string(),
            "log.rotate_step" => self.log_rotate_step.to_string(),
            "log.hide_dialog" => if self.log_hide_dialog { "on" } else { "off" }.to_string(),
            "log.include_screen_buffer" => if self.log_include_screen_buffer {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "log.lock_exclusive" => if self.log_lock_exclusive { "on" } else { "off" }.to_string(),
            "log.deferred_write" => if self.log_deferred_write { "on" } else { "off" }.to_string(),
            "transfer.dir" => self.transfer_dir.clone(),
            "transfer.binary" => if self.transfer_binary { "on" } else { "off" }.to_string(),
            "transfer.auto_rename" => if self.transfer_auto_rename {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.hide_dialog" => if self.transfer_hide_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.xmodem_opt" => self.transfer_xmodem_opt.as_ini().to_string(),
            "transfer.xmodem_binary" => if self.transfer_xmodem_binary {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.xmodem_rcv_command" => self.transfer_xmodem_rcv_command.clone(),
            "transfer.xmodem_log" => if self.transfer_xmodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.xmodem_timeout_init" => self.transfer_xmodem_timeout_init.to_string(),
            "transfer.xmodem_timeout_init_crc" => self.transfer_xmodem_timeout_init_crc.to_string(),
            "transfer.xmodem_timeout_short" => self.transfer_xmodem_timeout_short.to_string(),
            "transfer.xmodem_timeout_long" => self.transfer_xmodem_timeout_long.to_string(),
            "transfer.xmodem_timeout_vlong" => self.transfer_xmodem_timeout_vlong.to_string(),
            "transfer.ymodem_rcv_command" => self.transfer_ymodem_rcv_command.clone(),
            "transfer.ymodem_log" => if self.transfer_ymodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.ymodem_timeout_init" => self.transfer_ymodem_timeout_init.to_string(),
            "transfer.ymodem_timeout_init_crc" => self.transfer_ymodem_timeout_init_crc.to_string(),
            "transfer.ymodem_timeout_short" => self.transfer_ymodem_timeout_short.to_string(),
            "transfer.ymodem_timeout_long" => self.transfer_ymodem_timeout_long.to_string(),
            "transfer.ymodem_timeout_vlong" => self.transfer_ymodem_timeout_vlong.to_string(),
            "transfer.zmodem_auto" => if self.transfer_zmodem_auto {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.zmodem_data_len" => self.transfer_zmodem_data_len.to_string(),
            "transfer.zmodem_win_size" => self.transfer_zmodem_win_size.to_string(),
            "transfer.zmodem_escape_ctl" => if self.transfer_zmodem_escape_ctl {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.zmodem_log" => if self.transfer_zmodem_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.zmodem_rcv_command" => self.transfer_zmodem_rcv_command.clone(),
            "transfer.zmodem_timeout_normal" => self.transfer_zmodem_timeout_normal.to_string(),
            "transfer.zmodem_timeout_tcpip" => self.transfer_zmodem_timeout_tcpip.to_string(),
            "transfer.zmodem_timeout_init" => self.transfer_zmodem_timeout_init.to_string(),
            "transfer.zmodem_timeout_fin" => self.transfer_zmodem_timeout_fin.to_string(),
            "transfer.kermit_long_packet" => if self.transfer_kermit_long_packet {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.kermit_file_attr" => if self.transfer_kermit_file_attr {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.kermit_log" => if self.transfer_kermit_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.bplus_auto" => if self.transfer_bplus_auto {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.bplus_escape_ctl" => if self.transfer_bplus_escape_ctl {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.bplus_log" => if self.transfer_bplus_log { "on" } else { "off" }.to_string(),
            "transfer.quickvan_win_size" => self.transfer_quickvan_win_size.to_string(),
            "transfer.quickvan_log" => if self.transfer_quickvan_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.raw_autostop" => self.transfer_raw_autostop.to_string(),
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
            "terminal.buffer_max_lines" => {
                self.terminal_buffer_max_lines = crate::schema::ranged(
                    crate::schema::int(value, self.terminal_buffer_max_lines),
                    10000,
                    24,
                    2147483647,
                )
            }
            "terminal.iso2022_shifts" => self.terminal_iso2022_shifts = value.to_string(),
            "terminal.title" => self.terminal_title = value.to_string(),
            "keyboard.backspace" => self.keyboard_backspace = KeyboardBackspace::from_ini(value),
            "keyboard.meta" => self.keyboard_meta = KeyboardMeta::from_ini(value),
            "keyboard.delete_sends_del" => {
                self.keyboard_delete_sends_del = crate::schema::on_off(Some(value), false)
            }
            "keyboard.disable_app_keypad" => {
                self.keyboard_disable_app_keypad = crate::schema::on_off(Some(value), false)
            }
            "keyboard.disable_app_cursor" => {
                self.keyboard_disable_app_cursor = crate::schema::on_off(Some(value), false)
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
                self.color_underline_enabled = crate::schema::on_off(Some(value), true)
            }
            "color.xterm_256" => self.color_xterm_256 = crate::schema::on_off(Some(value), true),
            "color.aixterm_16" => self.color_aixterm_16 = crate::schema::on_off(Some(value), false),
            "color.pc_bold_16" => self.color_pc_bold_16 = crate::schema::on_off(Some(value), false),
            "color.ansi_enabled" => {
                self.color_ansi_enabled = crate::schema::on_off(Some(value), true)
            }
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
            "window.title_change" => self.window_title_change = WindowTitleChange::from_ini(value),
            "window.title_report" => self.window_title_report = WindowTitleReport::from_ini(value),
            "mouse.tracking" => self.mouse_tracking = crate::schema::on_off(Some(value), true),
            "mouse.ctrl_disables_tracking" => {
                self.mouse_ctrl_disables_tracking = crate::schema::on_off(Some(value), true)
            }
            "mouse.wheel_to_cursor" => {
                self.mouse_wheel_to_cursor = crate::schema::on_off(Some(value), true)
            }
            "clipboard.continued_line_copy" => {
                self.clipboard_continued_line_copy = crate::schema::on_off(Some(value), false)
            }
            "clipboard.auto_copy" => {
                self.clipboard_auto_copy = crate::schema::on_off(Some(value), true)
            }
            "clipboard.select_on_activate" => {
                self.clipboard_select_on_activate = crate::schema::on_off(Some(value), true)
            }
            "clipboard.select_only_by_lbutton" => {
                self.clipboard_select_only_by_lbutton = crate::schema::on_off(Some(value), true)
            }
            "clipboard.select_start_delay" => {
                self.clipboard_select_start_delay =
                    crate::schema::int(value, self.clipboard_select_start_delay)
            }
            "clipboard.paste_rbutton_disabled" => {
                self.clipboard_paste_rbutton_disabled = crate::schema::on_off(Some(value), false)
            }
            "clipboard.paste_mbutton_disabled" => {
                self.clipboard_paste_mbutton_disabled = crate::schema::on_off(Some(value), true)
            }
            "clipboard.confirm_paste_rbutton" => {
                self.clipboard_confirm_paste_rbutton = crate::schema::on_off(Some(value), false)
            }
            "clipboard.confirm_paste" => {
                self.clipboard_confirm_paste = crate::schema::on_off(Some(value), true)
            }
            "clipboard.confirm_paste_cr" => {
                self.clipboard_confirm_paste_cr = crate::schema::on_off(Some(value), true)
            }
            "clipboard.confirm_paste_dictionary" => {
                self.clipboard_confirm_paste_dictionary = value.to_string()
            }
            "clipboard.trim_trailing_newline" => {
                self.clipboard_trim_trailing_newline = crate::schema::on_off(Some(value), false)
            }
            "clipboard.paste_delay_per_line" => {
                self.clipboard_paste_delay_per_line = crate::schema::clamped(
                    crate::schema::int(value, self.clipboard_paste_delay_per_line),
                    0,
                    5000,
                )
            }
            "clipboard.paste_dialog_width" => {
                self.clipboard_paste_dialog_width = crate::schema::ranged(
                    crate::schema::int(value, self.clipboard_paste_dialog_width),
                    330,
                    0,
                    2147483647,
                )
            }
            "clipboard.paste_dialog_height" => {
                self.clipboard_paste_dialog_height = crate::schema::ranged(
                    crate::schema::int(value, self.clipboard_paste_dialog_height),
                    220,
                    0,
                    2147483647,
                )
            }
            "clipboard.bracketed" => {
                self.clipboard_bracketed = crate::schema::on_off(Some(value), true)
            }
            "clipboard.bracketed_control_only" => {
                self.clipboard_bracketed_control_only = crate::schema::on_off(Some(value), false)
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
            "connection.term_type" => self.connection_term_type = value.to_string(),
            "connection.terminal_speed" => self.connection_terminal_speed = value.to_string(),
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
            "log.binary" => self.log_binary = crate::schema::on_off(Some(value), false),
            "log.append" => self.log_append = crate::schema::on_off(Some(value), false),
            "log.plain_text" => self.log_plain_text = crate::schema::on_off(Some(value), false),
            "log.timestamp" => self.log_timestamp = crate::schema::on_off(Some(value), false),
            "log.timestamp_type" => self.log_timestamp_type = LogTimestampType::from_ini(value),
            "log.timestamp_utc" => {
                self.log_timestamp_utc = crate::schema::on_off(Some(value), false)
            }
            "log.timestamp_format" => self.log_timestamp_format = value.to_string(),
            "log.default_name" => self.log_default_name = value.to_string(),
            "log.default_path" => self.log_default_path = value.to_string(),
            "log.rotate" => self.log_rotate = crate::schema::int(value, self.log_rotate),
            "log.rotate_size" => {
                self.log_rotate_size = crate::schema::int(value, self.log_rotate_size)
            }
            "log.rotate_size_type" => {
                self.log_rotate_size_type = crate::schema::int(value, self.log_rotate_size_type)
            }
            "log.rotate_step" => {
                self.log_rotate_step = crate::schema::int(value, self.log_rotate_step)
            }
            "log.hide_dialog" => self.log_hide_dialog = crate::schema::on_off(Some(value), false),
            "log.include_screen_buffer" => {
                self.log_include_screen_buffer = crate::schema::on_off(Some(value), false)
            }
            "log.lock_exclusive" => {
                self.log_lock_exclusive = crate::schema::on_off(Some(value), true)
            }
            "log.deferred_write" => {
                self.log_deferred_write = crate::schema::on_off(Some(value), true)
            }
            "transfer.dir" => self.transfer_dir = value.to_string(),
            "transfer.binary" => self.transfer_binary = crate::schema::on_off(Some(value), false),
            "transfer.auto_rename" => {
                self.transfer_auto_rename = crate::schema::on_off(Some(value), false)
            }
            "transfer.hide_dialog" => {
                self.transfer_hide_dialog = crate::schema::on_off(Some(value), false)
            }
            "transfer.xmodem_opt" => self.transfer_xmodem_opt = TransferXmodemOpt::from_ini(value),
            "transfer.xmodem_binary" => {
                self.transfer_xmodem_binary = crate::schema::on_off(Some(value), true)
            }
            "transfer.xmodem_rcv_command" => self.transfer_xmodem_rcv_command = value.to_string(),
            "transfer.xmodem_log" => {
                self.transfer_xmodem_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.xmodem_timeout_init" => {
                self.transfer_xmodem_timeout_init = crate::schema::floored(
                    crate::schema::int(value, self.transfer_xmodem_timeout_init),
                    1,
                )
            }
            "transfer.xmodem_timeout_init_crc" => {
                self.transfer_xmodem_timeout_init_crc = crate::schema::floored(
                    crate::schema::int(value, self.transfer_xmodem_timeout_init_crc),
                    1,
                )
            }
            "transfer.xmodem_timeout_short" => {
                self.transfer_xmodem_timeout_short = crate::schema::floored(
                    crate::schema::int(value, self.transfer_xmodem_timeout_short),
                    1,
                )
            }
            "transfer.xmodem_timeout_long" => {
                self.transfer_xmodem_timeout_long = crate::schema::floored(
                    crate::schema::int(value, self.transfer_xmodem_timeout_long),
                    1,
                )
            }
            "transfer.xmodem_timeout_vlong" => {
                self.transfer_xmodem_timeout_vlong = crate::schema::floored(
                    crate::schema::int(value, self.transfer_xmodem_timeout_vlong),
                    1,
                )
            }
            "transfer.ymodem_rcv_command" => self.transfer_ymodem_rcv_command = value.to_string(),
            "transfer.ymodem_log" => {
                self.transfer_ymodem_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.ymodem_timeout_init" => {
                self.transfer_ymodem_timeout_init = crate::schema::floored(
                    crate::schema::int(value, self.transfer_ymodem_timeout_init),
                    1,
                )
            }
            "transfer.ymodem_timeout_init_crc" => {
                self.transfer_ymodem_timeout_init_crc = crate::schema::floored(
                    crate::schema::int(value, self.transfer_ymodem_timeout_init_crc),
                    1,
                )
            }
            "transfer.ymodem_timeout_short" => {
                self.transfer_ymodem_timeout_short = crate::schema::floored(
                    crate::schema::int(value, self.transfer_ymodem_timeout_short),
                    1,
                )
            }
            "transfer.ymodem_timeout_long" => {
                self.transfer_ymodem_timeout_long = crate::schema::floored(
                    crate::schema::int(value, self.transfer_ymodem_timeout_long),
                    1,
                )
            }
            "transfer.ymodem_timeout_vlong" => {
                self.transfer_ymodem_timeout_vlong = crate::schema::floored(
                    crate::schema::int(value, self.transfer_ymodem_timeout_vlong),
                    1,
                )
            }
            "transfer.zmodem_auto" => {
                self.transfer_zmodem_auto = crate::schema::on_off(Some(value), false)
            }
            "transfer.zmodem_data_len" => {
                self.transfer_zmodem_data_len =
                    crate::schema::int(value, self.transfer_zmodem_data_len)
            }
            "transfer.zmodem_win_size" => {
                self.transfer_zmodem_win_size =
                    crate::schema::int(value, self.transfer_zmodem_win_size)
            }
            "transfer.zmodem_escape_ctl" => {
                self.transfer_zmodem_escape_ctl = crate::schema::on_off(Some(value), false)
            }
            "transfer.zmodem_log" => {
                self.transfer_zmodem_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.zmodem_rcv_command" => self.transfer_zmodem_rcv_command = value.to_string(),
            "transfer.zmodem_timeout_normal" => {
                self.transfer_zmodem_timeout_normal = crate::schema::floored(
                    crate::schema::int(value, self.transfer_zmodem_timeout_normal),
                    1,
                )
            }
            "transfer.zmodem_timeout_tcpip" => {
                self.transfer_zmodem_timeout_tcpip = crate::schema::floored(
                    crate::schema::int(value, self.transfer_zmodem_timeout_tcpip),
                    0,
                )
            }
            "transfer.zmodem_timeout_init" => {
                self.transfer_zmodem_timeout_init = crate::schema::floored(
                    crate::schema::int(value, self.transfer_zmodem_timeout_init),
                    1,
                )
            }
            "transfer.zmodem_timeout_fin" => {
                self.transfer_zmodem_timeout_fin = crate::schema::floored(
                    crate::schema::int(value, self.transfer_zmodem_timeout_fin),
                    1,
                )
            }
            "transfer.kermit_long_packet" => {
                self.transfer_kermit_long_packet = crate::schema::on_off(Some(value), false)
            }
            "transfer.kermit_file_attr" => {
                self.transfer_kermit_file_attr = crate::schema::on_off(Some(value), false)
            }
            "transfer.kermit_log" => {
                self.transfer_kermit_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.bplus_auto" => {
                self.transfer_bplus_auto = crate::schema::on_off(Some(value), false)
            }
            "transfer.bplus_escape_ctl" => {
                self.transfer_bplus_escape_ctl = crate::schema::on_off(Some(value), false)
            }
            "transfer.bplus_log" => {
                self.transfer_bplus_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.quickvan_win_size" => {
                self.transfer_quickvan_win_size =
                    crate::schema::int(value, self.transfer_quickvan_win_size)
            }
            "transfer.quickvan_log" => {
                self.transfer_quickvan_log = crate::schema::on_off(Some(value), false)
            }
            "transfer.raw_autostop" => {
                self.transfer_raw_autostop = crate::schema::int(value, self.transfer_raw_autostop)
            }
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
        name: "terminal.buffer_max_lines",
        page: "terminal",
        section: "Tera Term",
        key: "MaxBuffSize",
        kind: Kind::IntRange(24, 2147483647),
        default: "10000",
        label: None,
        doc: "`ttset.c:1212`, and upstream's comment on it is \"special option\" — there is no dialog. It is the ceiling `ScrollBuffSize` is held under, not a second depth: `buffer.c:511` caps the buffer's line count with it and `:4977` caps the *terminal's row count* with it too, so `MaxBuffSize=10` is a ten-row terminal however big the window is. Below 24 takes the default rather than the floor, which is the `TerminalSize` bound with no ceiling on it.",
    },
    Field {
        name: "terminal.iso2022_shifts",
        page: "terminal",
        section: "Tera Term",
        key: "ISO2022ShiftFunction",
        kind: Kind::Str,
        default: "on",
        label: None,
        doc: "`ttset.c:1875`. Which ISO-2022 shifts the terminal honours, as a comma-separated list — `SI`, `SO` (`LS0` and `LS1` are read-only aliases for those two), `LS2`, `LS3`, `LS1R`, `LS2R`, `LS3R`, `SS2`, `SS3` — each optionally led by `+` or `-`, plus `on`/`all` and `off`/`none`, which assign the whole word rather than one bit.  A `string` rather than a type of its own: it is the only key in `ttset.c` shaped this way, and `ShiftFlags::parse_ini` already lives beside the bits it names. **The list starts from nothing whatever this default says** — the `\"on\"` is what upstream uses when the key is *absent*, and a key that is present starts at `ISO2022_SHIFT_NONE`, so `ISO2022ShiftFunction=-SS2` is a terminal with every shift disabled rather than all but one.",
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
        name: "keyboard.disable_app_keypad",
        page: "keyboard",
        section: "Tera Term",
        key: "DisableAppKeypad",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:903`, and a **veto rather than a mode**: DECNKM still sets, and DECRQM still reports it set, but the key encoding ignores it. So a host that switches the keypad to application mode gets the numeric one anyway and is not told. Named the way upstream names it, negation included, because a row called `app_keypad` would mean the opposite of the key it is written from.",
    },
    Field {
        name: "keyboard.disable_app_cursor",
        page: "keyboard",
        section: "Tera Term",
        key: "DisableAppCursor",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:907`. The same veto for DECCKM and the cursor keys.",
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
        key: "UnderlineAttrColor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:784`. Whether `SGR 4` gets its own colour pair at all, and **on** — unlike its two neighbours above, whose keys carry the `Enable` prefix this one does not. `vtdisp.c:2412` is the only reader.",
    },
    Field {
        name: "color.xterm_256",
        page: "color",
        section: "Tera Term",
        key: "Xterm256Color",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:741`. **On**, and this is one of the four flag words `AGENTS.md` warns about: `ColorFlag` is zeroed at the top of `ttset.c` and built up from per-key calls a thousand lines later, so reading the zero as the default turns 256-colour off and looks like a parser bug.",
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
        name: "color.ansi_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableANSIColor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:856`, and the last of the four `ColorFlag` bits. **On**, and it is not a parse gate like the three above it: `SGR 30-37` still stores its colour in the cell, and `vtdisp.c:2417` then declines to draw with it, so the screen is `color.normal` while the buffer says otherwise. The two reports that name a colour — DECRQSS' SGR (`vtterm.c:4332`) and `Co` in the termcap query (`:4451`) — go quiet with it, which is how a host is told.",
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
        key: "WindowCtrlSequence",
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
        key: "AlternateScreenBuffer",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1681`, part of `TermFlag`. The alternate screen, which `vim` and `less` need.",
    },
    Field {
        name: "window.remote_clears_buffer",
        page: "window",
        section: "Tera Term",
        key: "ClearScrollBufferFromRemote",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1950`. Whether `ED 3` may discard the scrollback.",
    },
    Field {
        name: "window.title_change",
        page: "window",
        section: "Tera Term",
        key: "AcceptTitleChangeRequest",
        kind: Kind::Enum(&["overwrite", "ahead", "last", "off"]),
        default: "overwrite",
        label: None,
        doc: "`ttset.c:1568`. How the title the host set and `terminal.title` combine, and **`off` means the host's title is not even stored** (`vtterm.c:5112`), which also switches off the title stack at `CSI 22 t` / `CSI 23 t`.  This is the first row to use `*`, and it needs it: the key is read with a default of `overwrite` and then compared down a chain whose `else` is **off**, so `AcceptTitleChangeRequest=ovewrite` is a terminal that ignores every OSC title while an absent key is one that accepts them. Absent and misspelt are two different settings, and only the second is the `else`.",
    },
    Field {
        name: "window.title_report",
        page: "window",
        section: "Tera Term",
        key: "TitleReportSequence",
        kind: Kind::Enum(&["empty", "accept", "ignore"]),
        default: "empty",
        label: None,
        doc: "`ttset.c:1664`, and it is `WindowFlag` again — with the extra turn that `IdTitleReportEmpty` is **24**, which is `WF_TITLEREPORT` entire. So the shipped `empty` sets both bits, lands on the `default:` arm, and answers `CSI 20 t` and `CSI 21 t` with an empty OSC string. That is deliberate: a terminal that echoes its own title into the input stream lets anything which can write to the screen put text in front of the shell. `accept` reports the real title, combined with `window.title_change`'s four spellings.",
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
        name: "clipboard.continued_line_copy",
        page: "clipboard",
        section: "Tera Term",
        key: "EnableContinuedLineCopy",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1419`. Two things at once, and the second is not about copying. With it on a wrapped line is marked continued, so selecting from column 0 takes the whole logical line and copying it joins the rows — **and the `CR` and `LF` that the wrap feeds to the log and the macro tap are suppressed**. That is the `logFlag` argument threaded through `CarriageReturn` and `LineFeed` (`vtterm.c:677`, `:695`): it is TRUE for a CR or LF that came off the wire and FALSE for the pair the terminal generated itself, and only the generated pair is dropped. So a macro's `wait` matches a wrapped line as one line, which is the whole point of the setting.",
    },
    Field {
        name: "clipboard.auto_copy",
        page: "clipboard",
        section: "Tera Term",
        key: "AutoTextCopy",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1105`. Copy the selection the moment the button comes up, with no Ctrl-Insert — which is what this shell has always done to the X11 primary selection, and now does from a key rather than from an opinion.",
    },
    Field {
        name: "clipboard.select_on_activate",
        page: "clipboard",
        section: "Tera Term",
        key: "SelectOnActivate",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1280`, `ts.SelOnActive`. Off **eats** the click that activates the window (`vtwin.cpp:2387` returns `MA_ACTIVATEANDEAT`), so bringing the terminal forward cannot start a selection by accident.",
    },
    Field {
        name: "clipboard.select_only_by_lbutton",
        page: "clipboard",
        section: "Tera Term",
        key: "SelectOnlyByLButton",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1449`. On, only the left button starts a selection — and a middle or right button coming up over a standing selection does **not** copy it (`vtwin.cpp:819`), which is the half of the setting its name does not say and the bug it was added to fix.",
    },
    Field {
        name: "clipboard.select_start_delay",
        page: "clipboard",
        section: "Tera Term",
        key: "MouseSelectStartDelay",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1954`, `ts.SelectStartDelay`, in milliseconds. How long the button is held before a drag counts as a selection rather than as a click.",
    },
    Field {
        name: "clipboard.paste_rbutton_disabled",
        page: "clipboard",
        section: "Tera Term",
        key: "DisablePasteMouseRButton",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1422`, `CPF_DISABLE_RBUTTON`. **Upstream pastes on the right button by default** — the arm is the `else` of this test (`vtwin.cpp:2645`), so a right-click over the terminal puts the clipboard on the wire.",
    },
    Field {
        name: "clipboard.paste_mbutton_disabled",
        page: "clipboard",
        section: "Tera Term",
        key: "DisablePasteMouseMButton",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1425`, `CPF_DISABLE_MBUTTON`, and the **on** is upstream's: Tera Term does not paste on the middle button, because on a wheel mouse that is the wheel. This shell did, on the X11 convention, and the divergence ends here the way `keyboard.meta`'s did — faithful by default and one line in the file away from the other behaviour. Note the two buttons ship opposite ways round from what a Linux user expects of either.",
    },
    Field {
        name: "clipboard.confirm_paste_rbutton",
        page: "clipboard",
        section: "Tera Term",
        key: "ConfirmPasteMouseRButton",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1428`, `CPF_CONFIRM_RBUTTON`. The right button raises a menu with Paste on it instead of pasting, and the button-up paste is then suppressed as well (`vtwin.cpp:2645` tests both bits).",
    },
    Field {
        name: "clipboard.confirm_paste",
        page: "clipboard",
        section: "Tera Term",
        key: "ConfirmChangePaste",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1431`, `CPF_CONFIRM_CHANGEPASTE`. **On.** A paste holding a line break is shown in a dialog first and can be edited there (`clipboar.c:126`), because a newline pasted into a shell runs whatever came before it.",
    },
    Field {
        name: "clipboard.confirm_paste_cr",
        page: "clipboard",
        section: "Tera Term",
        key: "ConfirmChangePasteCR",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1434`, `CPF_CONFIRM_CHANGEPASTE_CR`. The same confirmation for \"paste and send a CR\", where the newline is the one being *added* rather than one already in the text. Only consulted on that path, so a plain paste of text with no break is never confirmed by it.",
    },
    Field {
        name: "clipboard.confirm_paste_dictionary",
        page: "clipboard",
        section: "Tera Term",
        key: "ConfirmChangePasteStringFile",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1437`. A file of strings, one per line: a paste containing any of them is confirmed even with no line break in it. Resolved against the home directory rather than the working one (`GetFullPathW(ts.HomeDirW, …)`), and consulted only when `clipboard.confirm_paste` is on.",
    },
    Field {
        name: "clipboard.trim_trailing_newline",
        page: "clipboard",
        section: "Tera Term",
        key: "TrimTrailingNLonPaste",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1871`, `CPF_TRIM_TRAILING_NL`. Off, so the newline on the end of a copied line is pasted and the shell runs the line.",
    },
    Field {
        name: "clipboard.paste_delay_per_line",
        page: "clipboard",
        section: "Tera Term",
        key: "PasteDelayPerLine",
        kind: Kind::IntClamp(0, 5000),
        default: "10",
        label: None,
        doc: "`ttset.c:1633`, milliseconds between the lines of a paste — for a host with no flow control that drops what arrives while it is still echoing. The only setting in the file clamped at **both** ends; see `int_clamp` above for why that is a third bound rather than one of the other two.",
    },
    Field {
        name: "clipboard.paste_dialog_width",
        page: "clipboard",
        section: "Tera Term",
        key: "PasteDialogSize",
        kind: Kind::IntRange(0, 2147483647),
        default: "330",
        label: None,
        doc: "`ttset.c:1580`. Upstream writes the size back when the confirmation dialog is resized, which is the whole reason it is a setting. Below zero takes the default and there is no ceiling, so it is the `TerminalSize` bound rather than a clamp.",
    },
    Field {
        name: "clipboard.paste_dialog_height",
        page: "clipboard",
        section: "Tera Term",
        key: "PasteDialogSize",
        kind: Kind::IntRange(0, 2147483647),
        default: "220",
        label: None,
        doc: "The second half of the same key, with a default of its own.",
    },
    Field {
        name: "clipboard.bracketed",
        page: "clipboard",
        section: "Tera Term",
        key: "BracketedSupport",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:2002`. **A second gate on `DECSET 2004`**, and the one to know about: `clipboar.c:265` tests the setting *and* the mode, so a host that has asked for bracketed paste gets an unbracketed one when this is off. It ships on, so the mode alone is usually the answer — which is exactly why a port that omits the key looks right until somebody turns it off.",
    },
    Field {
        name: "clipboard.bracketed_control_only",
        page: "clipboard",
        section: "Tera Term",
        key: "BracketedControlOnly",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:2003`. Brackets only a paste that **contains a control character** (`iswcntrl`, `clipboar.c:270`) — so a pasted word goes bare and a pasted block is bracketed. The test runs while the line breaks are still CR LF and again gives the same answer once they are CR, so any multi-line paste qualifies either way.",
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
        name: "connection.term_type",
        page: "connection",
        section: "Tera Term",
        key: "TermType",
        kind: Kind::Str,
        default: "xterm",
        label: None,
        doc: "`ttset.c:961`. What `TERMINAL-TYPE` answers with, and what an SSH session sends as `TERM`. Upstream ships plain **`xterm`**, which this port had been diverging from with a hardcoded `xterm-256color` — a defensible choice and not one a hardcoded string should be making, since the answer decides what every curses program on the far end believes about the terminal.",
    },
    Field {
        name: "connection.terminal_speed",
        page: "connection",
        section: "Tera Term",
        key: "TerminalSpeed",
        kind: Kind::Str,
        default: "38400",
        label: None,
        doc: "`ttset.c:1936`. One number or two, `input,output`, for `TERMINAL-SPEED`.  A string rather than an `int` pair, because **the second field's default is the first field's value** and the schema has no way to say that: `GetNthNum` gives 0 for a field that is not there (`ttlib_static_cpp.cpp:1182`) and `ttset.c:1946` then assigns the input speed. Two `int` rows would have to default the second to something, and any constant makes `TerminalSpeed=57600` a terminal claiming two different speeds. Zero or less takes 38400 for the first field and the first field for the second, so `TerminalSpeed=0,0` is 38400 both ways.",
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
        doc: "`ttset.c:1026`. Start logging as soon as the session opens, under `LogDefaultName` in `LogDefaultPath`. `/NOLOG` turns it off; `/L=` names the file, which is **not** a setting — `ts.LogFN` has no key of its own.",
    },
    Field {
        name: "log.binary",
        page: "log",
        section: "Tera Term",
        key: "LogBinary",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:978`. A byte-for-byte capture of what arrived, rather than the text the terminal decided to display — the only mode that can be replayed back through a terminal, and the only one that keeps what a corrupt line really sent. `filesys_log.cpp:243` overrules `LogTypePlainText` and `LogTimestamp` when this is on.",
    },
    Field {
        name: "log.append",
        page: "log",
        section: "Tera Term",
        key: "LogAppend",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:981`, and the key is not the field: it is `ts.Append`, which is why grepping the struct for `LogAppend` finds nothing. Add to an existing file rather than truncating it.",
    },
    Field {
        name: "log.plain_text",
        page: "log",
        section: "Tera Term",
        key: "LogTypePlainText",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:984`, and it is **not** \"strip the escape sequences\" — a text log never had any, because the tap is downstream of the parser. It is one byte: `vtterm.c:666` and `:671` put a BS in the stream when a backspace moved the cursor, and this suppresses it, so a line the host corrected reads as the correction rather than as the keystrokes. The tap is shared with the macro language's received-line buffer, so it changes what `wait` sees too.",
    },
    Field {
        name: "log.timestamp",
        page: "log",
        section: "Tera Term",
        key: "LogTimestamp",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:988`. A `[time] ` at the head of each line. Silently dropped for a binary log (`filesys_log.cpp:243`), which is the right way round: a timestamp in the middle of a byte capture makes it no longer a capture.",
    },
    Field {
        name: "log.timestamp_type",
        page: "log",
        section: "Tera Term",
        key: "LogTimestampType",
        kind: Kind::Enum(&["", "Local", "UTC", "LoggingElapsed", "ConnectionElapsed"]),
        default: "",
        label: None,
        doc: "`ttset.c:1000`, **and the empty spelling is load-bearing.** Four `_stricmp`s with local time as the `else` — except that an *absent or empty* value consults `LogTimestampUTC` instead (`:1007`), the Tera Term 4 key this one replaced. So `LogTimestampType=Local` alongside `LogTimestampUTC=on` is local time, while no `LogTimestampType=` at all alongside the same key is UTC — and the first of those is exactly the file a Tera Term 5 leaves behind when it saves a Tera Term 4 one, since it writes the new key and does not remove the old. An enum that collapsed absent into `Local` would read that file as UTC. The cost is the one divergence: a *misspelt* value falls to local time upstream and to the empty spelling here, because the schema has one fallback and it is the default. It differs only in a file that misspells this key and carries `LogTimestampUTC=on` as well.",
    },
    Field {
        name: "log.timestamp_utc",
        page: "log",
        section: "Tera Term",
        key: "LogTimestampUTC",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1007`. Tera Term 4's key, read only when `LogTimestampType` is absent or empty — see that setting. Upstream reads it and never writes it back, so a file keeps it until somebody deletes it by hand.",
    },
    Field {
        name: "log.timestamp_format",
        page: "log",
        section: "Tera Term",
        key: "LogTimestampFormat",
        kind: Kind::Str,
        default: "%Y-%m-%d %H:%M:%S.%N",
        label: None,
        doc: "`ttset.c:996`, handed to `wcsftime` — plus `%N`, which is upstream's own addition for fractional seconds and not a strftime conversion. Not applied to an elapsed-time stamp, which has no date in it to format.",
    },
    Field {
        name: "log.default_name",
        page: "log",
        section: "Tera Term",
        key: "LogDefaultName",
        kind: Kind::Str,
        default: "teraterm.log",
        label: None,
        doc: "`ttset.c:1018`, the name `LogAutoStart` and the log dialog both start from. It is a **template**: `strftime` conversions, then `&h` for the host (`COMn` on a serial line), `&p` for the TCP port and `&u` for the user name.",
    },
    Field {
        name: "log.default_path",
        page: "log",
        section: "Tera Term",
        key: "LogDefaultPath",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1023`. Where a relative log name lands. Empty falls back to `FileDir` if that exists and to the per-user log directory otherwise — `GetTermLogDir` (`ttlib_types.cpp:63`), which is why the transfer directory can decide where a log goes.",
    },
    Field {
        name: "log.rotate",
        page: "log",
        section: "Tera Term",
        key: "LogRotate",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1029`. `rotate_mode`: 0 none, 1 by size (`tttypes.h:106`). Read with a plain `GetPrivateProfileInt` and **not bounded**, and `filesys_log.cpp:513` takes anything that is neither of the two as \"do not rotate\" — so this is an int here rather than a range, which would turn a 2 into a 1 and switch rotation on for a file that had it off.",
    },
    Field {
        name: "log.rotate_size",
        page: "log",
        section: "Tera Term",
        key: "LogRotateSize",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1030`, **in bytes whatever `LogRotateSizeType` says.** The dialog multiplies by 1024 per unit before storing (`log_pp.cpp:471`), so a value read back and scaled again turns the 1 MB the user asked for into a terabyte. Zero disables rotation, as does a `LogRotate` of 0.",
    },
    Field {
        name: "log.rotate_size_type",
        page: "log",
        section: "Tera Term",
        key: "LogRotateSizeType",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1031`. 0 bytes, 1 KB, 2 MB (`log_pp.cpp:72`) — the unit the *dialog* shows `LogRotateSize` in, and nothing else. It is stored so that reopening the dialog offers the number the user typed rather than its expansion.",
    },
    Field {
        name: "log.rotate_step",
        page: "log",
        section: "Tera Term",
        key: "LogRotateStep",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1032`. How many generations to keep: `file.1` is the newest. **Zero is not \"none\"** — `filesys_log.cpp:507` leaves `loopmax` at its hardcoded 10000, so an unset step with rotation on keeps ten thousand files.",
    },
    Field {
        name: "log.hide_dialog",
        page: "log",
        section: "Tera Term",
        key: "LogHideDialog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:991`. Upstream puts a file-transfer-shaped progress window up for the duration of a log; this port has a status-bar indicator instead, so the setting is read and written and acts on nothing.",
    },
    Field {
        name: "log.include_screen_buffer",
        page: "log",
        section: "Tera Term",
        key: "LogIncludeScreenBuffer",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:993`, and the field is `LogAllBuffIncludedInFirst`. Write the scrollback into the log before starting on live output. Read and written and **not acted on**: the function upstream does it with, `BuffGetAnyLineDataW`, truncates any line at its first wide character and at about half the width when a line holds combining marks — two of the five upstream bugs on file. It waits on those reports being answered.",
    },
    Field {
        name: "log.lock_exclusive",
        page: "log",
        section: "Tera Term",
        key: "LogLockExclusive",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1766`, and a `GetOnOff` whose default is **on** — so `=1` reads as on here where the same value reads as off for every setting above that ships off. Win32 share modes; nothing on this side opens the file exclusively, so it is read and written and acts on nothing.",
    },
    Field {
        name: "log.deferred_write",
        page: "log",
        section: "Tera Term",
        key: "DeferredLogWriteMode",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1035`, default **on**, the same asymmetry as `LogLockExclusive`. Upstream hands the write to a logging thread instead of blocking the one parsing the stream; here the write is buffered and the terminal's own read loop is not on the UI thread, so there is nothing to defer.",
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
        name: "transfer.binary",
        page: "transfer",
        section: "Tera Term",
        key: "TransBin",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:975`. The Binary checkbox both file dialogs carry, remembered between transfers — `filesys.cpp:231` copies it into `fv->BinaryMode` and `:454` uses it to decide whether a send translates line endings. Off, so a send is text by default and CR becomes CRLF.",
    },
    Field {
        name: "transfer.auto_rename",
        page: "transfer",
        section: "Tera Term",
        key: "AutoFileRename",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1097`, part of `FTFlag`. On receive, never overwrite: add `.1`, `.2`. Off, so upstream ships willing to replace a file it already has.",
    },
    Field {
        name: "transfer.hide_dialog",
        page: "transfer",
        section: "Tera Term",
        key: "FTHideDialog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1014`. Upstream's transfer progress window; this port has a status bar for it, so the setting is read and written and acts on nothing — the same arrangement as `log.hide_dialog`.",
    },
    Field {
        name: "transfer.xmodem_opt",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemOpt",
        kind: Kind::Enum(&["checksum", "crc", "1k", "1ksum"]),
        default: "checksum",
        label: None,
        doc: "**`ttset.c:1039`, and the default is the `else` branch again** — plain checksum, not CRC, which is the older and slower of the two and the one a modern peer is least likely to want. The reader has arms for `crc`, `1k` and `1ksum` only; the writer emits `checksum` (`ttset.c:2594`), a spelling the reader has no arm for and which round-trips solely because anything unmatched takes the default. Kept here as the default spelling for that reason.",
    },
    Field {
        name: "transfer.xmodem_binary",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemBin",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1051`. **On** — so XMODEM's own binary flag defaults the opposite way to `TransBin` above, which is the flag every other protocol uses.",
    },
    Field {
        name: "transfer.xmodem_rcv_command",
        page: "transfer",
        section: "Tera Term",
        key: "XModemRcvCommand",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1054`. What upstream sends to the host to start a receive. Read and written; this port has no \"send a command, then receive\" path yet, and the empty default is upstream's, so nothing is lost by an empty one here.",
    },
    Field {
        name: "transfer.xmodem_log",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1384`, part of `LogFlag`. A per-protocol transfer log, which is `ttpfile`'s own diagnostic and not the session log.",
    },
    Field {
        name: "transfer.xmodem_timeout_init",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1820`, and **these five floor at 1 rather than taking the default** — `int_min`, not `int`. `XmodemTimeouts=0,0,0,0,0` is five one-second timeouts. Field 1: how long to wait for the first block.",
    },
    Field {
        name: "transfer.xmodem_timeout_init_crc",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "3",
        label: None,
        doc: "`ttset.c:1824`. Field 2: the same, while still asking for CRC mode.",
    },
    Field {
        name: "transfer.xmodem_timeout_short",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1827`. Field 3.",
    },
    Field {
        name: "transfer.xmodem_timeout_long",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "20",
        label: None,
        doc: "`ttset.c:1830`. Field 4.",
    },
    Field {
        name: "transfer.xmodem_timeout_vlong",
        page: "transfer",
        section: "Tera Term",
        key: "XmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "60",
        label: None,
        doc: "`ttset.c:1833`. Field 5.",
    },
    Field {
        name: "transfer.ymodem_rcv_command",
        page: "transfer",
        section: "Tera Term",
        key: "YModemRcvCommand",
        kind: Kind::Str,
        default: "rb",
        label: None,
        doc: "`ttset.c:1392`, and unlike XMODEM's this one ships with a value: `rb`.",
    },
    Field {
        name: "transfer.ymodem_log",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1388`.",
    },
    Field {
        name: "transfer.ymodem_timeout_init",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1838`, the same five fields and the same floor as XMODEM's.",
    },
    Field {
        name: "transfer.ymodem_timeout_init_crc",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "3",
        label: None,
        doc: "`ttset.c:1842`.",
    },
    Field {
        name: "transfer.ymodem_timeout_short",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1845`.",
    },
    Field {
        name: "transfer.ymodem_timeout_long",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "20",
        label: None,
        doc: "`ttset.c:1848`.",
    },
    Field {
        name: "transfer.ymodem_timeout_vlong",
        page: "transfer",
        section: "Tera Term",
        key: "YmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "60",
        label: None,
        doc: "`ttset.c:1851`.",
    },
    Field {
        name: "transfer.zmodem_auto",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemAuto",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1396`, part of `FTFlag`. Whether the terminal watches the stream for a peer's `ZRQINIT` and starts a receive by itself.",
    },
    Field {
        name: "transfer.zmodem_data_len",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemDataLen",
        kind: Kind::Int,
        default: "1024",
        label: None,
        doc: "`ttset.c:1400`. The subpacket size when sending; `zmodem.c:780` floors it at 64 and caps it against the block-size ladder, so this is an upper bound rather than the value used.",
    },
    Field {
        name: "transfer.zmodem_win_size",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemWinSize",
        kind: Kind::Int,
        default: "32767",
        label: None,
        doc: "`ttset.c:1403`. How far ahead the sender may run before an ACK.",
    },
    Field {
        name: "transfer.zmodem_escape_ctl",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemEscCtl",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1407`, part of `FTFlag`. Escape control characters, for a link that eats them — a telnet server that has not been told `binary`, or a modem with software flow control in the path.",
    },
    Field {
        name: "transfer.zmodem_log",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1411`.",
    },
    Field {
        name: "transfer.zmodem_rcv_command",
        page: "transfer",
        section: "Tera Term",
        key: "ZModemRcvCommand",
        kind: Kind::Str,
        default: "rz",
        label: None,
        doc: "`ttset.c:1415`.",
    },
    Field {
        name: "transfer.zmodem_timeout_normal",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1857`. Four fields rather than five, and **the second floors at 0 rather than 1** because 0 is meaningful there: it is how \"never time out\" is spelt. Field 1, the normal timeout on a serial link.",
    },
    Field {
        name: "transfer.zmodem_timeout_tcpip",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemTimeouts",
        kind: Kind::IntMin(0),
        default: "0",
        label: None,
        doc: "`ttset.c:1861`. Field 2, and **0 by default**: on a network link a stalled ZMODEM waits for the socket to notice rather than timing out itself.",
    },
    Field {
        name: "transfer.zmodem_timeout_init",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1865`. Field 3.",
    },
    Field {
        name: "transfer.zmodem_timeout_fin",
        page: "transfer",
        section: "Tera Term",
        key: "ZmodemTimeouts",
        kind: Kind::IntMin(1),
        default: "3",
        label: None,
        doc: "`ttset.c:1868`. Field 4.",
    },
    Field {
        name: "transfer.kermit_long_packet",
        page: "transfer",
        section: "Tera Term",
        key: "KmtLongPacket",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1206`, part of `KermitOpt`. Long packets, which every Kermit written this century supports and which upstream still ships off.",
    },
    Field {
        name: "transfer.kermit_file_attr",
        page: "transfer",
        section: "Tera Term",
        key: "KmtFileAttr",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1208`. Send the file's attributes in an `A` packet.",
    },
    Field {
        name: "transfer.kermit_log",
        page: "transfer",
        section: "Tera Term",
        key: "KmtLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1204`.",
    },
    Field {
        name: "transfer.bplus_auto",
        page: "transfer",
        section: "Tera Term",
        key: "BPAuto",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "**`ttset.c:1130`, and turning this on rewrites `Answerback`** — the arm below it sets the terminal's answerback to `DLE + + DLE 0`, which is B-Plus's own trigger, so a setting on the transfer page silently changes what the terminal replies to ENQ. Not reproduced: this port's answerback is not wired to it, and doing so from a settings load would be a surprise a user cannot see.",
    },
    Field {
        name: "transfer.bplus_escape_ctl",
        page: "transfer",
        section: "Tera Term",
        key: "BPEscCtl",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1139`, part of `FTFlag`.",
    },
    Field {
        name: "transfer.bplus_log",
        page: "transfer",
        section: "Tera Term",
        key: "BPLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1143`.",
    },
    Field {
        name: "transfer.quickvan_win_size",
        page: "transfer",
        section: "Tera Term",
        key: "QVWinSize",
        kind: Kind::Int,
        default: "8",
        label: None,
        doc: "`ttset.c:1270`.",
    },
    Field {
        name: "transfer.quickvan_log",
        page: "transfer",
        section: "Tera Term",
        key: "QVLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1266`.",
    },
    Field {
        name: "transfer.raw_autostop",
        page: "transfer",
        section: "Tera Term",
        key: "ReceivefileAutoStopWaitTime",
        kind: Kind::Int,
        default: "5",
        label: None,
        doc: "`ttset.c:2031`, in seconds. How long a `recvfile` capture waits for the line to go quiet before stopping — and **the clock starts at the first byte** (`raw.c:168`), so a capture the host never answers waits for ever whatever this says.",
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
