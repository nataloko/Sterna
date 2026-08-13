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

/// `ttset.c:670` calls `GetKanjiCodeFromStr`, whose table is compared with
/// case-sensitive `strcmp` (`ttlib_charset.cpp:147`). Empty and unknown values
/// become UTF-8. Aliases are ordered with the spelling `GetKanjiCodeStr` writes
/// first: EUC is written as `EUC-JP`, and CP1251 as `Windows-1251`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingReceive {
    /// `UTF-8`
    Utf8,
    /// `ISO8859-1`
    Iso8859_1,
    /// `ISO8859-2`
    Iso8859_2,
    /// `ISO8859-3`
    Iso8859_3,
    /// `ISO8859-4`
    Iso8859_4,
    /// `ISO8859-5`
    Iso8859_5,
    /// `ISO8859-6`
    Iso8859_6,
    /// `ISO8859-7`
    Iso8859_7,
    /// `ISO8859-8`
    Iso8859_8,
    /// `ISO8859-9`
    Iso8859_9,
    /// `ISO8859-10`
    Iso8859_10,
    /// `ISO8859-11`
    Iso8859_11,
    /// `ISO8859-13`
    Iso8859_13,
    /// `ISO8859-14`
    Iso8859_14,
    /// `ISO8859-15`
    Iso8859_15,
    /// `ISO8859-16`
    Iso8859_16,
    /// `SJIS`
    Sjis,
    /// `EUC-JP`, `EUC` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Euc,
    /// `JIS`
    Jis,
    /// `Windows-1251`, `CP1251` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Windows1251,
    /// `KOI8-R`
    Koi8R,
    /// `CP866`
    Cp866,
    /// `KS5601`
    Ks5601,
    /// `GB2312`
    Gb2312,
    /// `BIG5`
    Big5,
    /// `CP437`
    Cp437,
    /// `CP737`
    Cp737,
    /// `CP775`
    Cp775,
    /// `CP850`
    Cp850,
    /// `CP852`
    Cp852,
    /// `CP855`
    Cp855,
    /// `CP857`
    Cp857,
    /// `CP860`
    Cp860,
    /// `CP861`
    Cp861,
    /// `CP862`
    Cp862,
    /// `CP863`
    Cp863,
    /// `CP864`
    Cp864,
    /// `CP865`
    Cp865,
    /// `CP869`
    Cp869,
    /// `CP874`
    Cp874,
    /// `CP1250`
    Cp1250,
    /// `CP1252`
    Cp1252,
    /// `CP1253`
    Cp1253,
    /// `CP1254`
    Cp1254,
    /// `CP1255`
    Cp1255,
    /// `CP1256`
    Cp1256,
    /// `CP1257`
    Cp1257,
    /// `CP1258`
    Cp1258,
}

impl EncodingReceive {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Iso8859_1 => "ISO8859-1",
            Self::Iso8859_2 => "ISO8859-2",
            Self::Iso8859_3 => "ISO8859-3",
            Self::Iso8859_4 => "ISO8859-4",
            Self::Iso8859_5 => "ISO8859-5",
            Self::Iso8859_6 => "ISO8859-6",
            Self::Iso8859_7 => "ISO8859-7",
            Self::Iso8859_8 => "ISO8859-8",
            Self::Iso8859_9 => "ISO8859-9",
            Self::Iso8859_10 => "ISO8859-10",
            Self::Iso8859_11 => "ISO8859-11",
            Self::Iso8859_13 => "ISO8859-13",
            Self::Iso8859_14 => "ISO8859-14",
            Self::Iso8859_15 => "ISO8859-15",
            Self::Iso8859_16 => "ISO8859-16",
            Self::Sjis => "SJIS",
            Self::Euc => "EUC-JP",
            Self::Jis => "JIS",
            Self::Windows1251 => "Windows-1251",
            Self::Koi8R => "KOI8-R",
            Self::Cp866 => "CP866",
            Self::Ks5601 => "KS5601",
            Self::Gb2312 => "GB2312",
            Self::Big5 => "BIG5",
            Self::Cp437 => "CP437",
            Self::Cp737 => "CP737",
            Self::Cp775 => "CP775",
            Self::Cp850 => "CP850",
            Self::Cp852 => "CP852",
            Self::Cp855 => "CP855",
            Self::Cp857 => "CP857",
            Self::Cp860 => "CP860",
            Self::Cp861 => "CP861",
            Self::Cp862 => "CP862",
            Self::Cp863 => "CP863",
            Self::Cp864 => "CP864",
            Self::Cp865 => "CP865",
            Self::Cp869 => "CP869",
            Self::Cp874 => "CP874",
            Self::Cp1250 => "CP1250",
            Self::Cp1252 => "CP1252",
            Self::Cp1253 => "CP1253",
            Self::Cp1254 => "CP1254",
            Self::Cp1255 => "CP1255",
            Self::Cp1256 => "CP1256",
            Self::Cp1257 => "CP1257",
            Self::Cp1258 => "CP1258",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Utf8`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s == "UTF-8" {
            return Self::Utf8;
        }
        if s == "ISO8859-1" {
            return Self::Iso8859_1;
        }
        if s == "ISO8859-2" {
            return Self::Iso8859_2;
        }
        if s == "ISO8859-3" {
            return Self::Iso8859_3;
        }
        if s == "ISO8859-4" {
            return Self::Iso8859_4;
        }
        if s == "ISO8859-5" {
            return Self::Iso8859_5;
        }
        if s == "ISO8859-6" {
            return Self::Iso8859_6;
        }
        if s == "ISO8859-7" {
            return Self::Iso8859_7;
        }
        if s == "ISO8859-8" {
            return Self::Iso8859_8;
        }
        if s == "ISO8859-9" {
            return Self::Iso8859_9;
        }
        if s == "ISO8859-10" {
            return Self::Iso8859_10;
        }
        if s == "ISO8859-11" {
            return Self::Iso8859_11;
        }
        if s == "ISO8859-13" {
            return Self::Iso8859_13;
        }
        if s == "ISO8859-14" {
            return Self::Iso8859_14;
        }
        if s == "ISO8859-15" {
            return Self::Iso8859_15;
        }
        if s == "ISO8859-16" {
            return Self::Iso8859_16;
        }
        if s == "SJIS" {
            return Self::Sjis;
        }
        if s == "EUC-JP" || s == "EUC" {
            return Self::Euc;
        }
        if s == "JIS" {
            return Self::Jis;
        }
        if s == "Windows-1251" || s == "CP1251" {
            return Self::Windows1251;
        }
        if s == "KOI8-R" {
            return Self::Koi8R;
        }
        if s == "CP866" {
            return Self::Cp866;
        }
        if s == "KS5601" {
            return Self::Ks5601;
        }
        if s == "GB2312" {
            return Self::Gb2312;
        }
        if s == "BIG5" {
            return Self::Big5;
        }
        if s == "CP437" {
            return Self::Cp437;
        }
        if s == "CP737" {
            return Self::Cp737;
        }
        if s == "CP775" {
            return Self::Cp775;
        }
        if s == "CP850" {
            return Self::Cp850;
        }
        if s == "CP852" {
            return Self::Cp852;
        }
        if s == "CP855" {
            return Self::Cp855;
        }
        if s == "CP857" {
            return Self::Cp857;
        }
        if s == "CP860" {
            return Self::Cp860;
        }
        if s == "CP861" {
            return Self::Cp861;
        }
        if s == "CP862" {
            return Self::Cp862;
        }
        if s == "CP863" {
            return Self::Cp863;
        }
        if s == "CP864" {
            return Self::Cp864;
        }
        if s == "CP865" {
            return Self::Cp865;
        }
        if s == "CP869" {
            return Self::Cp869;
        }
        if s == "CP874" {
            return Self::Cp874;
        }
        if s == "CP1250" {
            return Self::Cp1250;
        }
        if s == "CP1252" {
            return Self::Cp1252;
        }
        if s == "CP1253" {
            return Self::Cp1253;
        }
        if s == "CP1254" {
            return Self::Cp1254;
        }
        if s == "CP1255" {
            return Self::Cp1255;
        }
        if s == "CP1256" {
            return Self::Cp1256;
        }
        if s == "CP1257" {
            return Self::Cp1257;
        }
        if s == "CP1258" {
            return Self::Cp1258;
        }
        Self::Utf8
    }
}

impl Default for EncodingReceive {
    fn default() -> Self {
        Self::Utf8
    }
}

/// `ttset.c:683`, the same exact table and UTF-8 fallback for transmitted text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingSend {
    /// `UTF-8`
    Utf8,
    /// `ISO8859-1`
    Iso8859_1,
    /// `ISO8859-2`
    Iso8859_2,
    /// `ISO8859-3`
    Iso8859_3,
    /// `ISO8859-4`
    Iso8859_4,
    /// `ISO8859-5`
    Iso8859_5,
    /// `ISO8859-6`
    Iso8859_6,
    /// `ISO8859-7`
    Iso8859_7,
    /// `ISO8859-8`
    Iso8859_8,
    /// `ISO8859-9`
    Iso8859_9,
    /// `ISO8859-10`
    Iso8859_10,
    /// `ISO8859-11`
    Iso8859_11,
    /// `ISO8859-13`
    Iso8859_13,
    /// `ISO8859-14`
    Iso8859_14,
    /// `ISO8859-15`
    Iso8859_15,
    /// `ISO8859-16`
    Iso8859_16,
    /// `SJIS`
    Sjis,
    /// `EUC-JP`, `EUC` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Euc,
    /// `JIS`
    Jis,
    /// `Windows-1251`, `CP1251` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Windows1251,
    /// `KOI8-R`
    Koi8R,
    /// `CP866`
    Cp866,
    /// `KS5601`
    Ks5601,
    /// `GB2312`
    Gb2312,
    /// `BIG5`
    Big5,
    /// `CP437`
    Cp437,
    /// `CP737`
    Cp737,
    /// `CP775`
    Cp775,
    /// `CP850`
    Cp850,
    /// `CP852`
    Cp852,
    /// `CP855`
    Cp855,
    /// `CP857`
    Cp857,
    /// `CP860`
    Cp860,
    /// `CP861`
    Cp861,
    /// `CP862`
    Cp862,
    /// `CP863`
    Cp863,
    /// `CP864`
    Cp864,
    /// `CP865`
    Cp865,
    /// `CP869`
    Cp869,
    /// `CP874`
    Cp874,
    /// `CP1250`
    Cp1250,
    /// `CP1252`
    Cp1252,
    /// `CP1253`
    Cp1253,
    /// `CP1254`
    Cp1254,
    /// `CP1255`
    Cp1255,
    /// `CP1256`
    Cp1256,
    /// `CP1257`
    Cp1257,
    /// `CP1258`
    Cp1258,
}

impl EncodingSend {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Iso8859_1 => "ISO8859-1",
            Self::Iso8859_2 => "ISO8859-2",
            Self::Iso8859_3 => "ISO8859-3",
            Self::Iso8859_4 => "ISO8859-4",
            Self::Iso8859_5 => "ISO8859-5",
            Self::Iso8859_6 => "ISO8859-6",
            Self::Iso8859_7 => "ISO8859-7",
            Self::Iso8859_8 => "ISO8859-8",
            Self::Iso8859_9 => "ISO8859-9",
            Self::Iso8859_10 => "ISO8859-10",
            Self::Iso8859_11 => "ISO8859-11",
            Self::Iso8859_13 => "ISO8859-13",
            Self::Iso8859_14 => "ISO8859-14",
            Self::Iso8859_15 => "ISO8859-15",
            Self::Iso8859_16 => "ISO8859-16",
            Self::Sjis => "SJIS",
            Self::Euc => "EUC-JP",
            Self::Jis => "JIS",
            Self::Windows1251 => "Windows-1251",
            Self::Koi8R => "KOI8-R",
            Self::Cp866 => "CP866",
            Self::Ks5601 => "KS5601",
            Self::Gb2312 => "GB2312",
            Self::Big5 => "BIG5",
            Self::Cp437 => "CP437",
            Self::Cp737 => "CP737",
            Self::Cp775 => "CP775",
            Self::Cp850 => "CP850",
            Self::Cp852 => "CP852",
            Self::Cp855 => "CP855",
            Self::Cp857 => "CP857",
            Self::Cp860 => "CP860",
            Self::Cp861 => "CP861",
            Self::Cp862 => "CP862",
            Self::Cp863 => "CP863",
            Self::Cp864 => "CP864",
            Self::Cp865 => "CP865",
            Self::Cp869 => "CP869",
            Self::Cp874 => "CP874",
            Self::Cp1250 => "CP1250",
            Self::Cp1252 => "CP1252",
            Self::Cp1253 => "CP1253",
            Self::Cp1254 => "CP1254",
            Self::Cp1255 => "CP1255",
            Self::Cp1256 => "CP1256",
            Self::Cp1257 => "CP1257",
            Self::Cp1258 => "CP1258",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Utf8`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s == "UTF-8" {
            return Self::Utf8;
        }
        if s == "ISO8859-1" {
            return Self::Iso8859_1;
        }
        if s == "ISO8859-2" {
            return Self::Iso8859_2;
        }
        if s == "ISO8859-3" {
            return Self::Iso8859_3;
        }
        if s == "ISO8859-4" {
            return Self::Iso8859_4;
        }
        if s == "ISO8859-5" {
            return Self::Iso8859_5;
        }
        if s == "ISO8859-6" {
            return Self::Iso8859_6;
        }
        if s == "ISO8859-7" {
            return Self::Iso8859_7;
        }
        if s == "ISO8859-8" {
            return Self::Iso8859_8;
        }
        if s == "ISO8859-9" {
            return Self::Iso8859_9;
        }
        if s == "ISO8859-10" {
            return Self::Iso8859_10;
        }
        if s == "ISO8859-11" {
            return Self::Iso8859_11;
        }
        if s == "ISO8859-13" {
            return Self::Iso8859_13;
        }
        if s == "ISO8859-14" {
            return Self::Iso8859_14;
        }
        if s == "ISO8859-15" {
            return Self::Iso8859_15;
        }
        if s == "ISO8859-16" {
            return Self::Iso8859_16;
        }
        if s == "SJIS" {
            return Self::Sjis;
        }
        if s == "EUC-JP" || s == "EUC" {
            return Self::Euc;
        }
        if s == "JIS" {
            return Self::Jis;
        }
        if s == "Windows-1251" || s == "CP1251" {
            return Self::Windows1251;
        }
        if s == "KOI8-R" {
            return Self::Koi8R;
        }
        if s == "CP866" {
            return Self::Cp866;
        }
        if s == "KS5601" {
            return Self::Ks5601;
        }
        if s == "GB2312" {
            return Self::Gb2312;
        }
        if s == "BIG5" {
            return Self::Big5;
        }
        if s == "CP437" {
            return Self::Cp437;
        }
        if s == "CP737" {
            return Self::Cp737;
        }
        if s == "CP775" {
            return Self::Cp775;
        }
        if s == "CP850" {
            return Self::Cp850;
        }
        if s == "CP852" {
            return Self::Cp852;
        }
        if s == "CP855" {
            return Self::Cp855;
        }
        if s == "CP857" {
            return Self::Cp857;
        }
        if s == "CP860" {
            return Self::Cp860;
        }
        if s == "CP861" {
            return Self::Cp861;
        }
        if s == "CP862" {
            return Self::Cp862;
        }
        if s == "CP863" {
            return Self::Cp863;
        }
        if s == "CP864" {
            return Self::Cp864;
        }
        if s == "CP865" {
            return Self::Cp865;
        }
        if s == "CP869" {
            return Self::Cp869;
        }
        if s == "CP874" {
            return Self::Cp874;
        }
        if s == "CP1250" {
            return Self::Cp1250;
        }
        if s == "CP1252" {
            return Self::Cp1252;
        }
        if s == "CP1253" {
            return Self::Cp1253;
        }
        if s == "CP1254" {
            return Self::Cp1254;
        }
        if s == "CP1255" {
            return Self::Cp1255;
        }
        if s == "CP1256" {
            return Self::Cp1256;
        }
        if s == "CP1257" {
            return Self::Cp1257;
        }
        if s == "CP1258" {
            return Self::Cp1258;
        }
        Self::Utf8
    }
}

impl Default for EncodingSend {
    fn default() -> Self {
        Self::Utf8
    }
}

/// `ttset.c:675`; only the literal `7` selects seven-bit JIS Katakana, and the
/// empty default and every other value select eight-bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingKatakanaReceive {
    /// `7`
    Seven,
    /// `8`
    Eight,
}

impl EncodingKatakanaReceive {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Seven => "7",
            Self::Eight => "8",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Eight`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("7") {
            return Self::Seven;
        }
        if s.eq_ignore_ascii_case("8") {
            return Self::Eight;
        }
        Self::Eight
    }
}

impl Default for EncodingKatakanaReceive {
    fn default() -> Self {
        Self::Eight
    }
}

/// `ttset.c:688`, the transmit-side copy of the same switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingKatakanaSend {
    /// `7`
    Seven,
    /// `8`
    Eight,
}

impl EncodingKatakanaSend {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Seven => "7",
            Self::Eight => "8",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Eight`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("7") {
            return Self::Seven;
        }
        if s.eq_ignore_ascii_case("8") {
            return Self::Eight;
        }
        Self::Eight
    }
}

impl Default for EncodingKatakanaSend {
    fn default() -> Self {
        Self::Eight
    }
}

/// `ttset.c:696` and `makeoutputstring.cpp:72`; exact `@` selects the 1978 JIS
/// designation and exact `B` plus everything else selects the 1983 one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingKanjiIn {
    /// `@`
    At,
    /// `B`
    B,
}

impl EncodingKanjiIn {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::At => "@",
            Self::B => "B",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `B`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s == "@" {
            return Self::At;
        }
        if s == "B" {
            return Self::B;
        }
        Self::B
    }
}

impl Default for EncodingKanjiIn {
    fn default() -> Self {
        Self::B
    }
}

/// `ttset.c:701` and `makeoutputstring.cpp:83`; exact `B`, `J` and `H` are the
/// three designations and an absent or unknown value takes `J`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingKanjiOut {
    /// `B`
    B,
    /// `J`
    J,
    /// `H`
    H,
}

impl EncodingKanjiOut {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::B => "B",
            Self::J => "J",
            Self::H => "H",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `J`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s == "B" {
            return Self::B;
        }
        if s == "J" {
            return Self::J;
        }
        if s == "H" {
            return Self::H;
        }
        Self::J
    }
}

impl Default for EncodingKanjiOut {
    fn default() -> Self {
        Self::J
    }
}

/// `ttset.c:911`. Only `KOI8-R`, case-insensitively, selects that keyboard map;
/// the `Windows` spelling and everything else select the Windows map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingRussianKeyboard {
    /// `KOI8-R`
    Koi8R,
    /// `Windows`
    Windows,
}

impl EncodingRussianKeyboard {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Koi8R => "KOI8-R",
            Self::Windows => "Windows",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Windows`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("KOI8-R") {
            return Self::Koi8R;
        }
        if s.eq_ignore_ascii_case("Windows") {
            return Self::Windows;
        }
        Self::Windows
    }
}

impl Default for EncodingRussianKeyboard {
    fn default() -> Self {
        Self::Windows
    }
}

/// `ttset.c:1537`. Values 0, 1 and 2 select Unicode-to-DEC, DEC-to-Unicode and
/// no mapping. An out-of-range integer falls back to the first, not to the file
/// default of 2, so `*` records the separate else arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingDecSpecialDirection {
    /// `0`
    UnicodeToDec,
    /// `1`
    DecToUnicode,
    /// `2`
    Off,
}

impl EncodingDecSpecialDirection {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::UnicodeToDec => "0",
            Self::DecToUnicode => "1",
            Self::Off => "2",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `UnicodeToDec`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("0") {
            return Self::UnicodeToDec;
        }
        if s.eq_ignore_ascii_case("1") {
            return Self::DecToUnicode;
        }
        if s.eq_ignore_ascii_case("2") {
            return Self::Off;
        }
        Self::UnicodeToDec
    }
}

impl Default for EncodingDecSpecialDirection {
    fn default() -> Self {
        Self::Off
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

/// `ttset.c:1643`. What an enabled Meta key does to a one-byte character: `raw`
/// sets bit 7 on the byte, `text` sets U+0080 on the character before encoding,
/// and `off` prefixes ESC. `on` is a read-only alias of `raw`; the writer emits
/// `raw` (`ttset.c:3012`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMeta8bit {
    /// `off`
    Off,
    /// `raw`, `on` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Raw,
    /// `text`
    Text,
}

impl KeyboardMeta8bit {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Raw => "raw",
            Self::Text => "text",
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
        if s.eq_ignore_ascii_case("raw") || s.eq_ignore_ascii_case("on") {
            return Self::Raw;
        }
        if s.eq_ignore_ascii_case("text") {
            return Self::Text;
        }
        Self::default()
    }
}

impl Default for KeyboardMeta8bit {
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

/// `ttset.c:1769`. The Win32 `LOGFONT` rasterisation request. Anything unknown
/// falls to `default`; comparisons are case-insensitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontQuality {
    /// `default`
    Default,
    /// `nonantialiased`
    NonAntialiased,
    /// `antialiased`
    Antialiased,
    /// `cleartype`
    ClearType,
}

impl FontQuality {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::NonAntialiased => "nonantialiased",
            Self::Antialiased => "antialiased",
            Self::ClearType => "cleartype",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("default") {
            return Self::Default;
        }
        if s.eq_ignore_ascii_case("nonantialiased") {
            return Self::NonAntialiased;
        }
        if s.eq_ignore_ascii_case("antialiased") {
            return Self::Antialiased;
        }
        if s.eq_ignore_ascii_case("cleartype") {
            return Self::ClearType;
        }
        Self::default()
    }
}

impl Default for FontQuality {
    fn default() -> Self {
        Self::Default
    }
}

/// `ttset.c:2017`, parsed by `VTDrawFromIni` (`vtdraw.cpp:63`). These are the
/// three Win32 text APIs upstream can select; an unknown spelling returns to
/// `Auto`. Qt always draws Unicode glyphs, so this remains compatibility data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontDrawApi {
    /// `Auto`
    Auto,
    /// `Unicode`
    Unicode,
    /// `ANSI`
    Ansi,
}

impl FontDrawApi {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Unicode => "Unicode",
            Self::Ansi => "ANSI",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("Auto") {
            return Self::Auto;
        }
        if s.eq_ignore_ascii_case("Unicode") {
            return Self::Unicode;
        }
        if s.eq_ignore_ascii_case("ANSI") {
            return Self::Ansi;
        }
        Self::default()
    }
}

impl Default for FontDrawApi {
    fn default() -> Self {
        Self::Auto
    }
}

/// `ttset.c:1112`, read with an **empty** default and compared down an
/// `_stricmp` chain that tests only `off` and `visual` — so the `on` spelling
/// below matches nothing and lands on the same `else` the absent key does, which
/// is why both give the same variant here. Ninth member of the family
/// `AGENTS.md` keeps returning to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BellMode {
    /// `off`
    Off,
    /// `visual`
    Visual,
    /// `on`
    On,
}

impl BellMode {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Visual => "visual",
            Self::On => "on",
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
        if s.eq_ignore_ascii_case("visual") {
            return Self::Visual;
        }
        if s.eq_ignore_ascii_case("on") {
            return Self::On;
        }
        Self::default()
    }
}

impl Default for BellMode {
    fn default() -> Self {
        Self::On
    }
}

/// `ttset.c:1742`, `ts.CtrlFlag & CSF_CBMASK`. The two bits are independent:
/// `read` lets OSC 52 ask for the local clipboard, `write` lets it replace the
/// clipboard, and `on`/`readwrite` sets both. Anything else is off — including
/// an empty value — and the writer canonicalises the two-bit form to `on`.
///
/// Off by default because a remote process reading the clipboard can disclose a
/// password or token which never went near this terminal, while writing it can
/// replace text the user is about to paste somewhere else. `/OSC52=` overrides
/// this setting for one launch through the same four-state value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardRemoteAccess {
    /// `on`, `readwrite` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    ReadWrite,
    /// `read`
    Read,
    /// `write`
    Write,
    /// `off`
    Off,
}

impl ClipboardRemoteAccess {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::ReadWrite => "on",
            Self::Read => "read",
            Self::Write => "write",
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
        if s.eq_ignore_ascii_case("on") || s.eq_ignore_ascii_case("readwrite") {
            return Self::ReadWrite;
        }
        if s.eq_ignore_ascii_case("read") {
            return Self::Read;
        }
        if s.eq_ignore_ascii_case("write") {
            return Self::Write;
        }
        if s.eq_ignore_ascii_case("off") {
            return Self::Off;
        }
        Self::Off
    }
}

impl Default for ClipboardRemoteAccess {
    fn default() -> Self {
        Self::Off
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

/// `ttset.c:1325`, the same override for the line ending, and the empty spelling
/// is the one that means "do not override" — `Temp[0] = 0` is exactly what the
/// writer emits for it (`:2820`), so the disabled state round-trips as an empty
/// value rather than as a missing key.
///
/// The `else` and the default are the same arm here, unlike
/// `window.title_change`: an unrecognised spelling is disabled too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionTcpCrSend {
    /// ``
    Disabled,
    /// `CR`
    Cr,
    /// `CRLF`
    CrLf,
}

impl ConnectionTcpCrSend {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Disabled => "",
            Self::Cr => "CR",
            Self::CrLf => "CRLF",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("") {
            return Self::Disabled;
        }
        if s.eq_ignore_ascii_case("CR") {
            return Self::Cr;
        }
        if s.eq_ignore_ascii_case("CRLF") {
            return Self::CrLf;
        }
        Self::default()
    }
}

impl Default for ConnectionTcpCrSend {
    fn default() -> Self {
        Self::Disabled
    }
}

/// `ProxyWSockHook.h:1972`, through `ProxyInfo::parseType` (`:158`), which
/// lower-cases the value and compares it against a table — so it is
/// case-insensitive, and **anything unrecognised is no proxy at all**. A typo
/// is therefore a direct connection rather than an error, which is this file's
/// ordinary rule for an enumerated setting and is worth naming here because the
/// cost is different: the user believes they are behind a proxy and is not.
/// `none` is the spelling that says so on purpose.
///
/// `socks` and `socks5` are one type and `socks` is what upstream writes back —
/// `getTypeName` walks the same table and returns the first row for the type,
/// which is `socks` (`:181`).
///
/// **Five more spellings are in upstream's table and are not here**:
/// `ssl`, `none+ssl`, `http+ssl`, `socks+ssl`, `socks4+ssl`, `socks5+ssl` and
/// `telnet+ssl` parse to types the relay `switch` has no arm for, so they fall
/// to its `default: result = 0` (`:1822`) — reported as connected, with no
/// handshake done — because `SSLSocket.h` and `SSLLIB.h` are in the source tree,
/// included by nothing and listed in no build. Leaving them out puts them on the
/// unrecognised arm, which is the honest place for a spelling that has never
/// meant anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyType {
    /// `none`
    None,
    /// `http`
    Http,
    /// `telnet`
    Telnet,
    /// `socks4`
    Socks4,
    /// `socks`, `socks5` — the first is written back, the rest are aliases the
    /// file may hold because upstream's own table has them.
    Socks5,
}

impl ProxyType {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Http => "http",
            Self::Telnet => "telnet",
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks",
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
        if s.eq_ignore_ascii_case("http") {
            return Self::Http;
        }
        if s.eq_ignore_ascii_case("telnet") {
            return Self::Telnet;
        }
        if s.eq_ignore_ascii_case("socks4") {
            return Self::Socks4;
        }
        if s.eq_ignore_ascii_case("socks") || s.eq_ignore_ascii_case("socks5") {
            return Self::Socks5;
        }
        Self::default()
    }
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::None
    }
}

/// `ProxyWSockHook.h:1990`. Who turns the host name into an address — `local`
/// resolves here and fails if it cannot, `remote` never resolves here and sends
/// the name, `auto` resolves here and falls back to sending the name. Compared
/// case-insensitively with an `else` of `auto`.
///
/// Only the two SOCKS relays consult it; HTTP and the telnet proxy send the name
/// as text and have no choice to make. For SOCKS4 it is also what decides
/// whether the request is a 4a one, since that protocol expresses an unresolved
/// name by an address of `0.0.0.1` and a name after the user ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxySocksResolve {
    /// `auto`
    Auto,
    /// `local`
    Local,
    /// `remote`
    Remote,
}

impl ProxySocksResolve {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") {
            return Self::Auto;
        }
        if s.eq_ignore_ascii_case("local") {
            return Self::Local;
        }
        if s.eq_ignore_ascii_case("remote") {
            return Self::Remote;
        }
        Self::default()
    }
}

impl Default for ProxySocksResolve {
    fn default() -> Self {
        Self::Auto
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

/// `ttset.c:2006`. How raw Send file spaces writes. Anything unrecognised takes
/// `NoDelay`, including an empty present value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferRawSendDelayType {
    /// `NoDelay`
    NoDelay,
    /// `PerChar`
    PerChar,
    /// `PerLine`
    PerLine,
    /// `PerSendSize`
    PerSendSize,
}

impl TransferRawSendDelayType {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::NoDelay => "NoDelay",
            Self::PerChar => "PerChar",
            Self::PerLine => "PerLine",
            Self::PerSendSize => "PerSendSize",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("NoDelay") {
            return Self::NoDelay;
        }
        if s.eq_ignore_ascii_case("PerChar") {
            return Self::PerChar;
        }
        if s.eq_ignore_ascii_case("PerLine") {
            return Self::PerLine;
        }
        if s.eq_ignore_ascii_case("PerSendSize") {
            return Self::PerSendSize;
        }
        Self::default()
    }
}

impl Default for TransferRawSendDelayType {
    fn default() -> Self {
        Self::NoDelay
    }
}

/// `ttset.c:1555` passes the value through `IconName2IconId`. The ten known
/// names compare case-insensitively; anything else becomes `Default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TekIcon {
    /// `tterm`
    Tterm,
    /// `vt`
    Vt,
    /// `tek`
    Tek,
    /// `tterm_classic`
    TtermClassic,
    /// `vt_classic`
    VtClassic,
    /// `tterm_3d`
    Tterm3d,
    /// `vt_3d`
    Vt3d,
    /// `tterm_flat`
    TtermFlat,
    /// `vt_flat`
    VtFlat,
    /// `cygterm`
    Cygterm,
    /// `Default`
    Default,
}

impl TekIcon {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Tterm => "tterm",
            Self::Vt => "vt",
            Self::Tek => "tek",
            Self::TtermClassic => "tterm_classic",
            Self::VtClassic => "vt_classic",
            Self::Tterm3d => "tterm_3d",
            Self::Vt3d => "vt_3d",
            Self::TtermFlat => "tterm_flat",
            Self::VtFlat => "vt_flat",
            Self::Cygterm => "cygterm",
            Self::Default => "Default",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Default`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("tterm") {
            return Self::Tterm;
        }
        if s.eq_ignore_ascii_case("vt") {
            return Self::Vt;
        }
        if s.eq_ignore_ascii_case("tek") {
            return Self::Tek;
        }
        if s.eq_ignore_ascii_case("tterm_classic") {
            return Self::TtermClassic;
        }
        if s.eq_ignore_ascii_case("vt_classic") {
            return Self::VtClassic;
        }
        if s.eq_ignore_ascii_case("tterm_3d") {
            return Self::Tterm3d;
        }
        if s.eq_ignore_ascii_case("vt_3d") {
            return Self::Vt3d;
        }
        if s.eq_ignore_ascii_case("tterm_flat") {
            return Self::TtermFlat;
        }
        if s.eq_ignore_ascii_case("vt_flat") {
            return Self::VtFlat;
        }
        if s.eq_ignore_ascii_case("cygterm") {
            return Self::Cygterm;
        }
        if s.eq_ignore_ascii_case("Default") {
            return Self::Default;
        }
        Self::Default
    }
}

impl Default for TekIcon {
    fn default() -> Self {
        Self::Default
    }
}

/// `ttset.c:1550`, through `IconName2IconId`. The ten names compare
/// case-insensitively and anything else becomes `Default`. Sterna keeps its own
/// mark, so this is compatibility data for a shared Tera Term setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowIcon {
    /// `tterm`
    Tterm,
    /// `vt`
    Vt,
    /// `tek`
    Tek,
    /// `tterm_classic`
    TtermClassic,
    /// `vt_classic`
    VtClassic,
    /// `tterm_3d`
    Tterm3d,
    /// `vt_3d`
    Vt3d,
    /// `tterm_flat`
    TtermFlat,
    /// `vt_flat`
    VtFlat,
    /// `cygterm`
    Cygterm,
    /// `Default`
    Default,
}

impl WindowIcon {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Tterm => "tterm",
            Self::Vt => "vt",
            Self::Tek => "tek",
            Self::TtermClassic => "tterm_classic",
            Self::VtClassic => "vt_classic",
            Self::Tterm3d => "tterm_3d",
            Self::Vt3d => "vt_3d",
            Self::TtermFlat => "tterm_flat",
            Self::VtFlat => "vt_flat",
            Self::Cygterm => "cygterm",
            Self::Default => "Default",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Default`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("tterm") {
            return Self::Tterm;
        }
        if s.eq_ignore_ascii_case("vt") {
            return Self::Vt;
        }
        if s.eq_ignore_ascii_case("tek") {
            return Self::Tek;
        }
        if s.eq_ignore_ascii_case("tterm_classic") {
            return Self::TtermClassic;
        }
        if s.eq_ignore_ascii_case("vt_classic") {
            return Self::VtClassic;
        }
        if s.eq_ignore_ascii_case("tterm_3d") {
            return Self::Tterm3d;
        }
        if s.eq_ignore_ascii_case("vt_3d") {
            return Self::Vt3d;
        }
        if s.eq_ignore_ascii_case("tterm_flat") {
            return Self::TtermFlat;
        }
        if s.eq_ignore_ascii_case("vt_flat") {
            return Self::VtFlat;
        }
        if s.eq_ignore_ascii_case("cygterm") {
            return Self::Cygterm;
        }
        if s.eq_ignore_ascii_case("Default") {
            return Self::Default;
        }
        Self::Default
    }
}

impl Default for WindowIcon {
    fn default() -> Self {
        Self::Default
    }
}

/// `ttset.c:1501`. What the non-realtime broadcast dialog appends. The else arm
/// is `None`, so an unrecognised value and an absent key agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BroadcastSubmitKey {
    /// `None`
    None,
    /// `Enter`
    Enter,
    /// `CTRL+M`
    CtrlM,
}

impl BroadcastSubmitKey {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Enter => "Enter",
            Self::CtrlM => "CTRL+M",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("None") {
            return Self::None;
        }
        if s.eq_ignore_ascii_case("Enter") {
            return Self::Enter;
        }
        if s.eq_ignore_ascii_case("CTRL+M") {
            return Self::CtrlM;
        }
        Self::default()
    }
}

impl Default for BroadcastSubmitKey {
    fn default() -> Self {
        Self::None
    }
}

/// **`[Sterna]`**, because simultaneous panes are a Sterna window feature rather
/// than terminal state. The value says how many connections this window shows at
/// once: one terminal, two equal side-by-side panels, or four equal panels in a
/// 2x2 grid. A missing or unrecognised spelling returns to the single-panel
/// layout, so an edited file cannot leave the window without a usable view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowPanelLayout {
    /// `single`
    Single,
    /// `two`
    Two,
    /// `four`
    Four,
}

impl WindowPanelLayout {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Two => "two",
            Self::Four => "four",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Single`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("single") {
            return Self::Single;
        }
        if s.eq_ignore_ascii_case("two") {
            return Self::Two;
        }
        if s.eq_ignore_ascii_case("four") {
            return Self::Four;
        }
        Self::Single
    }
}

impl Default for WindowPanelLayout {
    fn default() -> Self {
        Self::Single
    }
}

/// Which edge the bar is on. Qt lets a toolbar be dragged to any of the four,
/// and this is where it opens.
///
/// **The right**, not the top, which is where the other bar is. A terminal's
/// rows are the scarce dimension — a window is usually far wider than the 80
/// columns it needs and exactly as tall as it can be — so a vertical bar costs
/// nothing that is being used, and the labels have room to be words rather than
/// abbreviations. An unrecognised spelling lands on the right as well, rather
/// than hiding the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowQuickButtonsArea {
    /// `right`
    Right,
    /// `top`
    Top,
    /// `bottom`
    Bottom,
    /// `left`
    Left,
}

impl WindowQuickButtonsArea {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }

    /// Case-insensitive, and **anything unrecognised is `Right`** — which
    /// is *not* this type's default. Upstream reads the key with a
    /// default string and then runs a chain of comparisons whose last
    /// arm catches everything, so an absent key and a misspelt value
    /// are two different settings.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("right") {
            return Self::Right;
        }
        if s.eq_ignore_ascii_case("top") {
            return Self::Top;
        }
        if s.eq_ignore_ascii_case("bottom") {
            return Self::Bottom;
        }
        if s.eq_ignore_ascii_case("left") {
            return Self::Left;
        }
        Self::Right
    }
}

impl Default for WindowQuickButtonsArea {
    fn default() -> Self {
        Self::Right
    }
}

/// Which of the four framing/burst combinations was used (`TelnetMode::of`).
/// `auto` is the shipped one: data until the first `IAC`, telnet after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentTelnetMode {
    /// `auto`
    Auto,
    /// `negotiate`
    Negotiate,
    /// `framed`
    Framed,
    /// `raw`
    Raw,
}

impl RecentTelnetMode {
    /// The INI's own spelling, which is what gets written back.
    pub fn as_ini(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Negotiate => "negotiate",
            Self::Framed => "framed",
            Self::Raw => "raw",
        }
    }

    /// Case-insensitive, and **anything unrecognised takes the default**
    /// rather than failing — which is how upstream spells most of its
    /// defaults, as the `else` branch of a chain of comparisons.
    pub fn from_ini(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") {
            return Self::Auto;
        }
        if s.eq_ignore_ascii_case("negotiate") {
            return Self::Negotiate;
        }
        if s.eq_ignore_ascii_case("framed") {
            return Self::Framed;
        }
        if s.eq_ignore_ascii_case("raw") {
            return Self::Raw;
        }
        Self::default()
    }
}

impl Default for RecentTelnetMode {
    fn default() -> Self {
        Self::Auto
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
    /// `ttset.c:1676`, part of `TermFlag` — the same trap as `ColorFlag`. Whether
    /// changing the terminal's size scrolls the page away and homes the cursor.
    ///
    /// Off, so the screen survives a resize and what moves is which lines the page
    /// covers (`buffer.c:5001`). Two things it does that its name does not say:
    /// with it **on** the clear happens even when the size did not change, because
    /// `BuffScroll` sits outside the `if (size changed)` block (`:5028`); and
    /// DECCOLM tests it and skips its own clear, since `ChangeTerminalSize` has
    /// already done one (`vtterm.c:2925`).
    pub terminal_clear_on_resize: bool,
    /// `ttset.c:1444`. Whether `ED 0` with the cursor already at the home position
    /// is treated as `ED 2`.
    ///
    /// **It is not the gate on `ED 2` its name suggests.** `CSScreenErase`'s `case 2`
    /// calls `BuffClearScreen` whatever this says (`vtterm.c:1740`), and a clear
    /// screen is a scroll into the history rather than an erase either way; what
    /// the key decides is only whether the `ESC [ H ESC [ J` pair — which many
    /// programs send in place of `ESC [ 2 J` — takes that path too. Turning it off
    /// leaves those programs erasing to the end of the screen, which is what the
    /// sequence literally asks for and loses the screen out of the history.
    pub terminal_home_erase_clears_screen: bool,
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
    /// `ttset.c:663`. What the terminal sends when the host asks it who it is with
    /// ENQ (`0x05`) — `vtterm.c:1076` writes it with `CommBinaryOut`, so the bytes
    /// go out raw, with no CR translation and no local echo.
    ///
    /// **The value is a hex string, not the answer itself.** `Hex2Str`
    /// (`ttlib.c:406`) copies bytes through and reads `$` as the lead of a two-digit
    /// escape, so `Answerback=VT100$0D` is nine bytes ending in a CR. Three quirks
    /// come with it, all from the same loop: `ConvHexChar` answers **0** for a digit
    /// that is not hex, so `$ZZ` is a NUL; a `$` with fewer than two digits behind
    /// it borrows `'0'` for each one it is missing, so a trailing `$` is also a NUL
    /// and `$A` is `0xA0`; and the result is arbitrary bytes rather than text, which
    /// is why this is held as the file's own spelling and decoded at the point of
    /// use rather than stored decoded.
    ///
    /// It is also the one setting in this file another setting **overwrites**:
    /// `ttset.c:1132` replaces it outright with B Plus's five-byte activation string
    /// when `BPAuto=on`, a hundred lines after reading it, so a file that sets both
    /// loses this one without a word. See `transfer.bplus_auto`.
    pub terminal_answerback: String,
    /// `ttset.c:1108`, `TF_BACKWRAP`. Whether a BS on the left margin steps back to
    /// the *previous* line rather than stopping dead. Off, `BackSpace`
    /// (`vtterm.c:662`) has an arm that does nothing at all; on, it moves to
    /// `CursorRightM` of the row above — the right *margin*, so a terminal with
    /// DECSLRM set lands inside the margins rather than at the last column.
    ///
    /// Only the arm that moves taps a BS into the log and the macro language's
    /// received-line buffer, so this key also decides whether a script's `wait` ever
    /// sees one at column zero.
    pub terminal_back_wrap: bool,
    /// `ttset.c:1343`, and it is read in two places that do not sound like the same
    /// setting. Off — which is how it ships — a tab is *like a printed character*:
    /// `Tab` (`vtterm.c:713`) takes a pending wrap first, so a tab arriving on a full
    /// line breaks the line before tabbing, and `CursorForwardTab` (`buffer.c:5228`)
    /// arms the pending wrap when it runs out of stops. On, both stop happening and a
    /// tab is only ever a cursor move, which is what a real VT does.
    ///
    /// CHT (`CSI Ps I`) is unaffected by the first half: it calls `CursorForwardTab`
    /// directly and never sees the wrap.
    pub terminal_vt_compat_tab: bool,
    /// `ttset.c:1717`, `ts.TabStopFlag`. Which sequences a *host* is allowed to move
    /// the tab stops with, as a comma list — `HTS7` is `ESC H`, `HTS8` is the 8-bit
    /// C1 at 0x88, `TBC0` is `CSI 0 g` and `TBC3` is `CSI 3 g`; `HTS` and `TBC` are
    /// each the pair. `on`/`all` and `off`/`none` assign the whole word.
    ///
    /// A `string` rather than a type of its own, for the same reason
    /// `terminal.iso2022_shifts` is one: it is a flag list and the parse lives beside
    /// the bits it names. Unlike that key, this one starts from `TABF_NONE` only in
    /// the *list* arm and the default applies whenever the value is absent **or**
    /// matches `on`.
    pub terminal_tab_stop_modify: String,
    /// `ttset.c:1756`, `TF_INVALIDDECRPSS`, and upstream's own comment is "(for
    /// testing)". `RequestStatusString` (`vtterm.c:4400`) flips the leading digit of
    /// the reply it was about to send, so a valid request answers `0$r` — "I do not
    /// recognise this" — and an invalid one answers `1$r` with an empty body. It is
    /// there to exercise the *host's* error handling, and the only setting in the
    /// terminal whose purpose is to lie.
    pub terminal_invalid_decrqss: bool,
    /// `ttset.c:1688`. The eight hex digits the tertiary DA (`CSI = c`) answers with,
    /// in a `DCS ! | … ST` (`vtterm.c:2829`).
    ///
    /// **Validated on read and the fallback is the default**: eight characters, every
    /// one a hex digit, upper-cased in place — anything else, including a nine-digit
    /// value, becomes `FFFFFFFF`. That is `ts.BSKey`'s shape rather than an enum's,
    /// so it is held as a string and checked at the point of use; a file keeps
    /// whatever it wrote, and the terminal answers with the valid form.
    pub terminal_uid: String,
    /// `ttset.c:1711`, `TF_LOCKTUID`, and the default is **on** — so DECSTUI
    /// (`DCS ! { … ST`) is refused as shipped. `vtterm.c:4565` is the whole of it:
    /// with the key off a host may set the UID above, with it on the sequence is
    /// read and dropped. The same eight-hex-digit validation applies there as
    /// applies to the file, in a second place.
    pub terminal_lock_uid: bool,
    /// `ttset.c:1101`, `TF_AUTOINVOKE`. Whether designating a character set into G0
    /// also invokes G0 into GL, so `ESC ( B` puts ASCII back on the wire's own bytes
    /// without an SI.
    ///
    /// Two things about `ESCSBCSSelect` (`vtterm.c:1409`) that reading the name would
    /// not give. The invoke is **outside** the switch that handled the final
    /// character, so an unrecognised designation like `ESC ( Z` still invokes; and it
    /// is *not* gated on `ts.ISO2022Flag`, unlike every other locking shift in the
    /// parser, so a terminal with `ISO2022ShiftFunction=off` still performs this one.
    pub terminal_auto_invoke: bool,
    /// `ttset.c:1789`. The ceiling on the buffer an OSC or DCS string is collected
    /// into — `vtterm.c:5265` doubles the buffer from `sizeof(ts.Title)` up to this
    /// and then silently **drops** every further byte, so a title longer than this
    /// arrives truncated and the sequence still terminates normally.
    ///
    /// It is the only bound on a string a host controls the length of, which is why
    /// it is not merely cosmetic.
    pub terminal_max_osc_buffer: i32,
    /// `ttset.c:1078`, `TF_ALLOWWRONGSEQUENCE`, default off. Its remaining upstream
    /// consumer accepts a non-regular Japanese coding sequence (`coding_pp.cpp:247`)
    /// and permits the otherwise-refused `KanjiOut=H`; CJK is deferred here, so the
    /// compatibility switch round-trips and acts on nothing for now.
    pub terminal_allow_wrong_sequence: bool,
    /// `ttset.c:1187`, `TF_ENABLESLINE`, default **on**. Gates DECSSDT/DECSASD
    /// (`vtterm.c:3827`). `tt-grid` deliberately has no third status-line buffer yet,
    /// so the key is retained until that parser feature exists.
    pub terminal_status_line_enabled: bool,
    /// `ttset.c:670` calls `GetKanjiCodeFromStr`, whose table is compared with
    /// case-sensitive `strcmp` (`ttlib_charset.cpp:147`). Empty and unknown values
    /// become UTF-8. Aliases are ordered with the spelling `GetKanjiCodeStr` writes
    /// first: EUC is written as `EUC-JP`, and CP1251 as `Windows-1251`.
    pub encoding_receive: EncodingReceive,
    /// `ttset.c:683`, the same exact table and UTF-8 fallback for transmitted text.
    pub encoding_send: EncodingSend,
    /// `ttset.c:675`; only the literal `7` selects seven-bit JIS Katakana, and the
    /// empty default and every other value select eight-bit.
    pub encoding_katakana_receive: EncodingKatakanaReceive,
    /// `ttset.c:688`, the transmit-side copy of the same switch.
    pub encoding_katakana_send: EncodingKatakanaSend,
    /// `ttset.c:696` and `makeoutputstring.cpp:72`; exact `@` selects the 1978 JIS
    /// designation and exact `B` plus everything else selects the 1983 one.
    pub encoding_kanji_in: EncodingKanjiIn,
    /// `ttset.c:701` and `makeoutputstring.cpp:83`; exact `B`, `J` and `H` are the
    /// three designations and an absent or unknown value takes `J`.
    pub encoding_kanji_out: EncodingKanjiOut,
    /// `ttset.c:1158`, `TF_CTRLINKANJI`, default on. It lets C0 controls interrupt a
    /// two-byte Japanese character; retained until those decoders exist here.
    pub encoding_ctrl_in_kanji: bool,
    /// `ttset.c:1194`, `TF_FIXEDJIS`, default off. The writer does not emit this
    /// legacy hand-edited key, but a changed generated setting must still be able to.
    pub encoding_fixed_jis: bool,
    /// `ttset.c:1957`, default off. Upstream's experimental fallback decodes bytes
    /// that are invalid UTF-8 as CP932; the decoder is part of the deferred CJK work.
    pub encoding_fallback_cp932: bool,
    /// `ttset.c:911`. Only `KOI8-R`, case-insensitively, selects that keyboard map;
    /// the `Windows` spelling and everything else select the Windows map.
    pub encoding_russian_keyboard: EncodingRussianKeyboard,
    /// `ttset.c:1537`. Values 0, 1 and 2 select Unicode-to-DEC, DEC-to-Unicode and
    /// no mapping. An out-of-range integer falls back to the first, not to the file
    /// default of 2, so `*` records the separate else arm.
    pub encoding_dec_special_direction: EncodingDecSpecialDirection,
    /// `ttset.c:1547`, assigned to a `WORD`. The low three bits choose the symbol
    /// groups mapped to DEC special graphics; unknown high bits survive a save.
    pub encoding_unicode_to_dec_special: i32,
    /// `ttset.c:1965`. Only 1 and 2 are valid, and either invalid end takes
    /// `GetDefaultUnicodeWidth()`, which is 1 in this upstream tree.
    pub encoding_ambiguous_width: i32,
    /// `ttset.c:1969`, default off. When enabled, Emoji-property characters use the
    /// explicit width below instead of their East Asian Width property.
    pub encoding_emoji_override: bool,
    /// `ttset.c:1970`, the same accept-only-1-or-2 rule and default 1 as ambiguous
    /// width. Width policy remains deliberately deferred with CJK.
    pub encoding_emoji_width: i32,
    /// `ttset.c:1198`, default on. This is Win32 IME enablement; Qt input-method
    /// integration remains deferred with CJK, so the compatibility key is carried.
    pub ime_enabled: bool,
    /// `ttset.c:1201`, default on. Selects inline composition in upstream and acts
    /// on nothing until the input-method surface exists here.
    pub ime_inline: bool,
    /// `ttset.c:1684`, `WF_IMECURSORCHANGE`, default off. Changes the cursor while
    /// an IME composition is active; the setting is retained with that deferred UI.
    pub ime_cursor_related: bool,
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
    /// `ttset.c:1176`. When a word starts on a non-delimiter, changing between a
    /// one-cell and a multi-cell character ends it (`buffer.c:4479`). On by default,
    /// so double-clicking `abc北京def` selects one of the three width runs rather than
    /// the whole string. A run that starts on a delimiter is unaffected: upstream's
    /// other arm still groups consecutive copies of that same character.
    pub keyboard_width_delimits_word: bool,
    /// `ttset.c:1598`, default off. With it on, a PC key that has no explicit
    /// `KEYBOARD.CNF` entry sends nothing instead of falling back to Tera Term's
    /// built-in VT sequence (`keyboard.c:960` onward).
    pub keyboard_strict_mapping: bool,
    /// `ttset.c:1643`. What an enabled Meta key does to a one-byte character: `raw`
    /// sets bit 7 on the byte, `text` sets U+0080 on the character before encoding,
    /// and `off` prefixes ESC. `on` is a read-only alias of `raw`; the writer emits
    /// `raw` (`ttset.c:3012`).
    pub keyboard_meta_8bit: KeyboardMeta8bit,
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
    /// `ttset.c:775`. The pair applied to a cell carrying `AttrURL`: green on
    /// white as shipped. URL is its own attribute rather than an SGR colour, and
    /// an explicit SGR foreground or background still wins over the corresponding
    /// half later in `vtdisp.c:GetDrawAttr` (`:2499`, then `:2522`).
    pub color_url: [u8; 6],
    /// `ttset.c:776`, the `CF_URLCOLOR` bit. This gates only the URL colour pair;
    /// URL detection and `URLUnderline` are independent, so turning it off leaves a
    /// detected URL underlined in the ordinary text colour.
    pub color_url_enabled: bool,
    /// `ttset.c:780`, the `FF_URLUNDERLINE` bit. Independent of both the URL colour
    /// and whether a double-click is allowed to open one.
    pub color_url_underline: bool,
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
    /// `ttset.c:797`. Sixteen `(legacy index,r,g,b)` groups in one `MAX_PATH`
    /// buffer. This stays a string because upstream accepts partial lists, repeated
    /// and wrapped IDs, and byte-wrapped channels; the behavioral parse belongs by
    /// the terminal palette in `tt-session`. The default is upstream's exact colour
    /// values with only its alignment whitespace removed.
    pub color_ansi_palette: String,
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
    /// `ttset.c:868`, `FF_BOLD`. Whether SGR 1 selects a bold *font*, independently
    /// of `color.bold_enabled`, which decides whether it selects the bold colour
    /// pair. Both ship on, but either may be disabled alone — a bold cell can be
    /// blue in a regular face or bold in the normal text colour.
    pub color_bold_font: bool,
    /// `ttset.c:782`, `FF_UNDERLINE`. The font half of SGR 4, independent of
    /// `color.underline_enabled`'s magenta colour pair. Off keeps the underline
    /// attribute in the grid and changes only how it is drawn.
    pub color_underline_font: bool,
    /// `ttset.c:1335`, `CF_USETEXTCOLOR`. A compatibility escape for applications
    /// which assume a black terminal: after applying explicit SGR colours,
    /// `GetDrawAttr` (`vtdisp.c:2542`) replaces an invisible same-colour pair with
    /// the configured normal pair when both indices match and the foreground is
    /// black (0), white (7), or bright white (15). Under reverse video it uses the
    /// configured reverse pair — even when `color.reverse_enabled` is off.
    pub color_use_text_color: bool,
    /// `ttset.c:1561`. Bold, blink, underline and URL colours are pairs of their
    /// own; on, their configured background half is ignored and the normal text
    /// background is used instead. Reverse swaps that normal background into the
    /// foreground (`vtdisp.c:2453`).
    pub color_use_normal_background: bool,
    /// `ttset.c:718`, and the default is again the `else` branch.
    pub cursor_shape: CursorShape,
    /// `ttset.c:1227`.
    pub cursor_nonblinking: bool,
    /// `ttset.c:1231`. Whether a window without keyboard focus keeps a hollow
    /// cursor on the live screen. `CaretKillFocus` (`vtdisp.c:1872`) draws a
    /// full-cell outline regardless of the configured active cursor shape; off, an
    /// unfocused window has no cursor at all.
    pub cursor_show_unfocused: bool,
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
    /// `ttset.c:1564`. Whether output arriving while the user has scrolled back
    /// leaves the view where it is.
    ///
    /// **Off**, which means the opposite of what a modern terminal does: every line
    /// the host prints yanks the view to the cursor, because `MoveCursor` and
    /// `MoveRight` call `DispScrollToCursor` unconditionally (`buffer.c:3794`,
    /// `:3805`). Scrolling back through a boot log on a device that is still
    /// talking is the case it ruins, and this port had upstream's `on` behaviour
    /// hardcoded before there was a key to read.
    pub window_auto_scroll_only_at_bottom: bool,
    /// `ttset.c:1273`. How many scrolled lines upstream lets accumulate before it
    /// repaints (`vtdisp.c:3132`) — a coalescing governor, counted in lines rather
    /// than in time.
    ///
    /// Read and written and acting on nothing, said here rather than discovered:
    /// the equivalent is `TerminalView`'s 8 ms frame floor, which measures the same
    /// thing in the unit a compositor cares about. Carried so a file round-trips,
    /// the way `bell.notify_sound` is.
    pub window_scroll_threshold: i32,
    /// `ttset.c:1466`. `255` is fully opaque and `0` fully transparent; upstream
    /// applies this value when the window loses focus (`vtwin.cpp:1537`).
    ///
    /// **The `max(0, …)`/`min(255, …)` pair on the next two lines is dead code**, and
    /// reading it as the rule is how this row came to say `int_clamp(0..255)`.
    /// `ts.AlphaBlendInactive` is a `BYTE`, so the assignment has already narrowed
    /// the value and neither test can ever fire. It decides the answer for the one
    /// value a person writes here: `AlphaBlend=-1` is `(UINT)-1`, which the `BYTE`
    /// makes **255** — an opaque window — where the clamp reads a bare -1 as below
    /// the floor and gives 0, a window nobody can see. `AlphaBlend=256` is 0 for the
    /// same reason, not 255.
    pub window_opacity_inactive: i32,
    /// `ttset.c:1470`, with the same dead clamp behind the same narrowing. Its
    /// fallback is the *loaded inactive opacity*, not the constant 255:
    /// `AlphaBlend=120` without this key makes both window states 120. An empty
    /// value inherits too; a non-numeric one is zero under `GetPrivateProfileInt`'s
    /// own rules. Applied at startup, after settings changes and whenever the window
    /// gains focus (`vtwin.cpp:780`, `:1357`, `:1539`).
    pub window_opacity_active: i32,
    /// `ttset.c:1339`, a six-bit word rather than an enum: 1 shows the endpoint,
    /// 2 the process-wide session number, 4 the `VT`/`TEK` suffix, 8 puts the
    /// endpoint before the configured/remote title, 16 includes a TCP port and 32
    /// includes a serial speed (`ttwinman.c:79`). The shipped 13 is 1|4|8:
    /// `<endpoint> - <title> VT`.
    ///
    /// Kept as one word because that is what the file exposes and unknown bits 6–15
    /// round-trip through upstream unchanged. Upstream's dialog presents the
    /// low six as checkboxes; the generated first-pass dialog exposes the word
    /// directly until it grows a bit-field widget.
    pub window_title_format: i32,
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
    /// `ttset.c:1769`. The Win32 `LOGFONT` rasterisation request. Anything unknown
    /// falls to `default`; comparisons are case-insensitive.
    pub font_quality: FontQuality,
    /// `ttset.c:1988`, default **on**. Upstream fits glyphs whose natural advance
    /// does not match the cell into the cell (`vtdisp.c:2902`).
    pub font_draw_resized: bool,
    /// `ttset.c:2017`, parsed by `VTDrawFromIni` (`vtdraw.cpp:63`). These are the
    /// three Win32 text APIs upstream can select; an unknown spelling returns to
    /// `Auto`. Qt always draws Unicode glyphs, so this remains compatibility data.
    pub font_draw_api: FontDrawApi,
    /// `ttset.c:2021`. Zero asks the Windows renderer for the process ANSI code
    /// page; a positive value overrides it. It has no effect on Qt's Unicode text
    /// path, but preserving it matters to a settings file shared with Tera Term.
    pub font_draw_ansi_code_page: i32,
    /// `ttset.c:1346`, the first field of `VTFontSpace`. Upstream adds the left and
    /// right values to the cell width and offsets the glyph by the left value.
    pub font_space_left: i32,
    /// The second field of `VTFontSpace`; it adds space after the glyph.
    pub font_space_right: i32,
    /// The third field of `VTFontSpace`; it offsets the glyph downward and adds to
    /// the cell height (`vtdisp.c:2838`). Negative spacing remains meaningful.
    pub font_space_top: i32,
    /// The fourth field of `VTFontSpace`; it adds space below the glyph.
    pub font_space_bottom: i32,
    /// `ttset.c:1640`, default off and marked "test". Its only consumer is inside a
    /// `#if 0` block at `vtwin.cpp:2718`, so carrying it without runtime behavior is
    /// faithful to current upstream rather than an omission.
    pub font_scaling: bool,
    /// `ttset.c:1523`. **On**, and it was left zeroed here for a while, which
    /// silently disabled every mouse mode and made DECRQM answer "permanently
    /// reset" for all of them.
    pub mouse_tracking: bool,
    /// `ttset.c:1591`. Holding Ctrl suppresses the report so text can still be
    /// selected in a full-screen application.
    pub mouse_ctrl_disables_tracking: bool,
    /// `ttset.c:1515`. Gates `DECSET 7786`, and is what a reset restores it to.
    pub mouse_wheel_to_cursor: bool,
    /// `ttset.c:1594`. Holding Ctrl cancels the translation above, so the wheel
    /// scrolls the terminal's own history instead of sending arrow keys to the
    /// full-screen application in front of it (`vtterm.c:5847`). The pair to
    /// `mouse.ctrl_disables_tracking`, and on for the same reason.
    pub mouse_ctrl_disables_wheel_to_cursor: bool,
    /// `ttset.c:1276`. How many lines one notch of the wheel moves.
    ///
    /// **Only when the notch arrives alone.** `vtwin.cpp:2539` multiplies under
    /// `line == 1`, where `line` is `abs(zDelta)/WHEEL_DELTA` — so a flick fast
    /// enough to coalesce two notches into one message scrolls two lines rather
    /// than six, and the setting stops applying exactly when the user is scrolling
    /// hardest. Not a clamp either: the guard is `> 0`, so `MouseWheelScrollLine=0`
    /// is one line per notch and so is a negative value.
    ///
    /// It is also the step for something with no other name: with the pointer over
    /// the title bar the wheel changes the window's opacity, by this many units of
    /// 255 (`vtwin.cpp:2500`). One setting, two meanings, the way `TelEcho` and
    /// `ts.BSKey` each have two.
    pub mouse_wheel_scroll_line: i32,
    /// `ttset.c:771`. URL recognition, colouring and underlining happen regardless;
    /// this controls only the hand cursor and the double-click that launches one
    /// (`vtwin.cpp:2426`, `buffer.c:4411`). It ships off.
    pub mouse_clickable_url: bool,
    /// `ttset.c:1460`. The ordinary pointer over the terminal: `ARROW`, `IBEAM`,
    /// `CROSS` or `HAND`, compared case-insensitively by `SetMouseCursor`
    /// (`vtwin.cpp:159`). `IBEAM` is the shipped default.
    ///
    /// A `string`, deliberately, rather than an enum. The reader copies the file's
    /// spelling verbatim into `MouseCursorName`, and `SetMouseCursor` simply returns
    /// without changing the current pointer when none of the four names matches.
    /// Normalising an unknown value to the default would therefore change both a
    /// shared file and the live no-op behavior upstream gives it.
    pub mouse_cursor: String,
    /// `ttset.c:1760`. An empty string uses the operating system's URL handler.
    /// A configured executable is tried only for HTTP, HTTPS and FTP; SFTP, TFTP,
    /// NEWS and MMS still go straight to the system handler (`buffer.c:4084`).
    pub url_browser: String,
    /// `ttset.c:1762`. Prepended to the URL when `url.browser` is used; ignored for
    /// the four schemes that always use the system handler.
    pub url_browser_args: String,
    /// `ttset.c:1792`. Read, written and documented, but never consulted anywhere
    /// in upstream's current source: continued display lines are joined according
    /// to `AttrLineContinued` regardless. Carried so the file round-trips; it acts
    /// on nothing here for the same reason it acts on nothing there.
    pub url_join_split: bool,
    /// `ttset.c:1794`. Upstream keeps only the first byte (a backslash by default),
    /// but — like `JoinSplitURL` itself — no current code reads the result. Carried
    /// in the file and deliberately not given invented behaviour.
    pub url_join_split_ignore_eol_char: String,
    /// `ttset.c:1162`, default off. When enabled, Shift+Escape cycles through the
    /// receive-display modes selected by `debug.modes`; TTL's `setdebug` can select
    /// one directly without changing this keyboard gate.
    pub debug_enabled: bool,
    /// `ttset.c:1798`. `all` permits normal, hex and no-output modes; otherwise a
    /// comma-separated list of `normal`, `hex` and `noout` selects the cycle. `on`
    /// is an alias for all, while `off`, `none`, or a list with no recognised word
    /// also disables `Debug` upstream. Kept raw because order and duplicate words
    /// are part of the file even though only the resulting mask affects cycling.
    pub debug_modes: String,
    /// `ttset.c:1112`, read with an **empty** default and compared down an
    /// `_stricmp` chain that tests only `off` and `visual` — so the `on` spelling
    /// below matches nothing and lands on the same `else` the absent key does, which
    /// is why both give the same variant here. Ninth member of the family
    /// `AGENTS.md` keeps returning to.
    pub bell_mode: BellMode,
    /// `ttset.c:1121`, `PF_BEEPONCONNECT`. **A TCP/IP connection only**: both places
    /// it is read test `PortType==IdTCPIP` first (`vtwin.cpp:3018`, `:3658`), so a
    /// serial console opening and closing is silent however this is set. It also
    /// bypasses `RingBell` entirely — always audible, never the visual bell, and
    /// never governed by the four below.
    pub bell_on_connect: bool,
    /// `ttset.c:1125`. How long the screen stays inverted for a visual bell, in
    /// milliseconds — `int_min`, since it floors at 1 rather than taking the default.
    pub bell_visual_wait_ms: i32,
    /// `ttset.c:1781`. How many bells inside `bell.over_used_time` seconds are
    /// allowed before the governor starts suppressing. **Off by one against the
    /// manual**: `teraterm-term.html` says five bells are permitted and six sound,
    /// because `RingBell`'s inner `if` decides the *next* bell's fate and the switch
    /// that makes the noise sits outside it (`vtterm.c:5800`).
    pub bell_over_used_count: i32,
    /// `ttset.c:1783`. The window the count is measured over, in seconds. A gap
    /// longer than this refills the count.
    pub bell_over_used_time: i32,
    /// `ttset.c:1785`. How long the terminal stays silent once the count is used up,
    /// in seconds — and it is **quiet** time, not elapsed time. Every bell arriving
    /// during the suppression pushes the deadline out again (`vtterm.c:5796` assigns
    /// `now` in the arm that decides it is suppressed), so a host beeping steadily
    /// is silenced until it stops and for this long afterwards. The manual reads as
    /// though it were a fixed delay; the code is the specification and this follows
    /// the code, because a governor that let a runaway through every five seconds
    /// would not do the job it exists for.
    pub bell_suppress_time: i32,
    /// `ttset.c:1996`. Whether the *notification* makes a sound — upstream's tray
    /// balloon (`vtwin.cpp:725`, `Notify2SetSound`), not the terminal's bell. Read
    /// and written and acting on nothing here, because there is no notification
    /// surface yet; it is in this section because a user looking for "the sound
    /// settings" will look here.
    pub bell_notify_sound: bool,
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
    /// terminal forward cannot start a selection by accident. Read and written and
    /// acting on nothing yet: Qt delivers no `WM_MOUSEACTIVATE`, so the equivalent
    /// is a first-click filter the view does not have.
    pub clipboard_select_on_activate: bool,
    /// `ttset.c:1449`. On, only the left button starts a selection — and a middle
    /// or right button coming up over a standing selection does **not** copy it
    /// (`vtwin.cpp:819`), which is the half of the setting its name does not say
    /// and the bug it was added to fix.
    pub clipboard_select_only_by_lbutton: bool,
    /// `ttset.c:1954`, `ts.SelectStartDelay`, in milliseconds. How long the button
    /// is held before a drag counts as a selection rather than as a click. Read and
    /// written and acting on nothing yet; it ships at 0, which is what the view
    /// does.
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
    /// as well (`vtwin.cpp:2645` tests both bits). Half honoured: the suppression
    /// is there and the menu is not, so setting this gives a right button that does
    /// nothing rather than one that offers a choice.
    pub clipboard_confirm_paste_rbutton: bool,
    /// `ttset.c:1431`, `CPF_CONFIRM_CHANGEPASTE`. **On.** A paste holding a line
    /// break is shown in a dialog first and can be edited there
    /// (`clipboar.c:126`), because a newline pasted into a shell runs whatever
    /// came before it.
    pub clipboard_confirm_paste: bool,
    /// `ttset.c:1434`, `CPF_CONFIRM_CHANGEPASTE_CR`. The same confirmation for
    /// "paste and send a CR", where the newline is the one being *added* rather
    /// than one already in the text. Only consulted on that path, so a plain paste
    /// of text with no break is never confirmed by it — and that path is upstream's
    /// `Paste<CR>` menu item, which this shell has no command for, so the key is
    /// read and written and acts on nothing yet.
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
    /// that is a third bound rather than one of the other two. Read and written and
    /// acting on nothing yet: pacing a paste means handing the send path a schedule,
    /// and `Session::paste` queues the whole thing.
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
    /// `ttset.c:1742`, `ts.CtrlFlag & CSF_CBMASK`. The two bits are independent:
    /// `read` lets OSC 52 ask for the local clipboard, `write` lets it replace the
    /// clipboard, and `on`/`readwrite` sets both. Anything else is off — including
    /// an empty value — and the writer canonicalises the two-bit form to `on`.
    ///
    /// Off by default because a remote process reading the clipboard can disclose a
    /// password or token which never went near this terminal, while writing it can
    /// replace text the user is about to paste somewhere else. `/OSC52=` overrides
    /// this setting for one launch through the same four-state value.
    pub clipboard_remote_access: ClipboardRemoteAccess,
    /// `ttset.c:1753`, `GetOnOff(…, TRUE)`. Upstream raises a balloon for accepted
    /// and rejected reads and writes. It does not change the permission above: with
    /// access off (the default), this being on is what makes a rejected attempt
    /// visible instead of silently dropping it.
    pub clipboard_remote_notify: bool,
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
    /// **`ttset.c:1298`, and it is the reason `Telnet=off` is not a raw socket.**
    /// `commlib.c:323` copies this onto the connection unconditionally, beside a
    /// `cv->TelFlag` that comes from `Telnet=`; then `ttcmn.c:590` reads
    /// `!cv->TelFlag && cv->TelAutoDetect` and turns the framing on at the first
    /// `0xFF` byte. So the shipped defaults give a TCP session that starts as data
    /// and becomes telnet the moment anything sends an `IAC`, whatever `Telnet=`
    /// says — which is why TTSSH clears it by hand (`ttxssh.c:981`) with a comment
    /// saying the line "should not be needed". It is: an SSH stream is full of
    /// `0xFF`.
    ///
    /// The two keys together are four states rather than two, and `TelnetMode` in
    /// `tt-conn` carries all four. See `tt_session::open::telnet_params`.
    pub connection_telnet_auto_detect: bool,
    /// `ttset.c:1304`. **Not "echo locally" — it is "let the ECHO option decide".**
    /// With it off, `WILL ECHO` and `WONT ECHO` change nothing (`telnet.c:411`,
    /// `:497` both test it first) and the opening burst simply asks the server to
    /// echo. With it on, the negotiated state *assigns* `ts.LocalEcho` — server
    /// echoing means local echo off — and the burst runs `TelChangeEcho` instead,
    /// which asks the server to echo only if local echo is currently off and asks it
    /// **not** to (`DONT ECHO`) if local echo is on. The setting and the mode are one
    /// variable, the same shape as DECBKM and `ts.BSKey`.
    pub connection_telnet_echo: bool,
    /// `ttset.c:1307`, which ORs `LOG_TEL` into `ts.LogFlag` rather than keeping a
    /// field of its own. `TELNET.LOG` in the log directory, truncated at every
    /// connection (`telnet.c:127`, `CREATE_ALWAYS`).
    ///
    /// **It records only what Tera Term sends.** Every one of the eight
    /// `TelWriteLog` calls sits directly after a `CommRawOut`, and nothing on the
    /// receive path logs at all — so the `>` that leads each line has no inbound
    /// counterpart and a file that looks like a negotiation trace is one half of the
    /// conversation.
    pub connection_telnet_log: bool,
    /// `ttset.c:1314`, in **seconds**, zero meaning no keepalive. An `IAC NOP` for a
    /// firewall that would otherwise drop an idle session.
    ///
    /// Two things the name does not say. It is a **quiet** period, not a period:
    /// `telnet.c:913` compares against `cv.LastSendTime`, which `commlib.c:1062`
    /// stamps on every telnet send including the NOP itself, so a session that is
    /// being typed at never sends one. And it runs only where the opening burst ran
    /// — `TelStartKeepAliveThread` is called inside `vtwin.cpp:3666`'s
    /// `TCPPort == TelPort` arm — so a telnet-framed connection to a port that is not
    /// the telnet port gets no keepalive at all.
    pub connection_telnet_keepalive: i32,
    /// `ttset.c:1322`. Local echo for a TCP connection that is **not** speaking
    /// telnet, which is a separate key because the telnet one is negotiated and this
    /// one cannot be.
    ///
    /// It does not sit beside `LocalEcho`; it *overwrites* it. `vtwin.cpp:3696`
    /// assigns `ts.LocalEcho = ts.TCPLocalEcho` when the connection opens and
    /// `:3589` puts `ts.LocalEcho_ini` back when it closes — upstream keeps a
    /// pristine copy of the file's value precisely because the connection spends the
    /// live one. Off means "leave the terminal's own setting alone", so this is one
    /// of the settings where 0 is not a value.
    pub connection_tcp_local_echo: bool,
    /// `ttset.c:1325`, the same override for the line ending, and the empty spelling
    /// is the one that means "do not override" — `Temp[0] = 0` is exactly what the
    /// writer emits for it (`:2820`), so the disabled state round-trips as an empty
    /// value rather than as a missing key.
    ///
    /// The `else` and the default are the same arm here, unlike
    /// `window.title_change`: an unrecognised spelling is disabled too.
    pub connection_tcp_cr_send: ConnectionTcpCrSend,
    /// `ttset.c:1154`, on by default, and it is `PortFlag` rather than a field.
    /// Whether closing the window or choosing Disconnect asks first. **TCP only** —
    /// both tests are `cv.PortType==IdTCPIP` (`vtwin.cpp:1668`, `:4448`), so a serial
    /// session closes without a word however this is set.
    pub connection_confirm_disconnect: bool,
    /// `ttset.c:972`, off by default. Whether a host that was connected to is
    /// remembered in the New Connection dialog's list (`vtwin.cpp:3849`).
    ///
    /// Upstream's writer spells the key `Historylist` (`:2521`) where its reader
    /// spells it `HistoryList`. Harmless — `GetPrivateProfile*` matches key names
    /// case-insensitively, which `ini-audit/` measured rather than assumed — and it
    /// is not the only one: `Metakey`, `XmodemRcvCommand`, `YmodemRcvCommand` and
    /// `ZmodemRcvCommand` are written in a case their own readers do not use either.
    pub connection_history_list: bool,
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
    /// `ttset.c:1610`, off by default. When a connection ends and the window is
    /// staying open, scroll the live page into the history and home the cursor
    /// (`vtwin.cpp:3029`, `:4513`). This is Tera Term's ordinary Clear screen
    /// operation, not an erase: the old page remains available in scrollback.
    ///
    /// It sits after the auto-close decision. A network session with
    /// `connection.auto_win_close` on closes its window instead, while serial and
    /// local-pty sessions never take that network-only branch.
    pub connection_clear_screen_on_close: bool,
    /// `ttset.c:1457`, in seconds, zero meaning the stack's own timeout. `/TIMEOUT=`
    /// refuses a negative value rather than clamping it.
    pub connection_timeout: i32,
    /// `ttset.c:1520`, on by default — the New Connection dialog at startup, which
    /// `/DS` suppresses and `/ES` asks for.
    pub connection_host_dialog_on_startup: bool,
    /// `ttset.c:1673`, default **on**. A TCP connection begins in local line mode
    /// until telnet negotiation says otherwise (`commlib.c:341`); serial is never
    /// affected. The async transport currently sends keystrokes immediately, so the
    /// key is carried pending a negotiated line buffer.
    pub connection_line_mode: bool,
    /// `ProxyWSockHook.h:1972`, through `ProxyInfo::parseType` (`:158`), which
    /// lower-cases the value and compares it against a table — so it is
    /// case-insensitive, and **anything unrecognised is no proxy at all**. A typo
    /// is therefore a direct connection rather than an error, which is this file's
    /// ordinary rule for an enumerated setting and is worth naming here because the
    /// cost is different: the user believes they are behind a proxy and is not.
    /// `none` is the spelling that says so on purpose.
    ///
    /// `socks` and `socks5` are one type and `socks` is what upstream writes back —
    /// `getTypeName` walks the same table and returns the first row for the type,
    /// which is `socks` (`:181`).
    ///
    /// **Five more spellings are in upstream's table and are not here**:
    /// `ssl`, `none+ssl`, `http+ssl`, `socks+ssl`, `socks4+ssl`, `socks5+ssl` and
    /// `telnet+ssl` parse to types the relay `switch` has no arm for, so they fall
    /// to its `default: result = 0` (`:1822`) — reported as connected, with no
    /// handshake done — because `SSLSocket.h` and `SSLLIB.h` are in the source tree,
    /// included by nothing and listed in no build. Leaving them out puts them on the
    /// unrecognised arm, which is the honest place for a spelling that has never
    /// meant anything.
    pub proxy_type: ProxyType,
    /// `ProxyWSockHook.h:1976`. Empty is no proxy however `proxy.type` is set —
    /// `_load` demotes a type it has no host for rather than dialling an empty name.
    pub proxy_host: String,
    /// `ProxyWSockHook.h:1980`, read with `getInteger`'s default of **0** and
    /// assigned to an `unsigned short`, which is why this is `uint16` rather than a
    /// bounded `int`: `ProxyPort=65537` is port 1.
    ///
    /// **Zero means the type's default here and disables the relay upstream**, which
    /// is the first of the four defects in `tt-conn/src/proxy.rs`. `getPort()`
    /// (`:442`) supplies 1080 for either SOCKS, 23 for the telnet proxy and 80 for
    /// HTTP, and the connect hook uses it for the address (`:1770`) — and then the
    /// guard deciding whether to speak the protocol at all tests the raw stored port
    /// instead (`:1792`). Leaving the dialog's port box empty is enough to reach it.
    pub proxy_port: i32,
    /// `ProxyWSockHook.h:1981`. The proxy's own credentials, not the host's: Basic
    /// authentication for HTTP, RFC 1929 for SOCKS5, and the user ID field for
    /// SOCKS4 — which is not a password at all, being what the proxy hands to the
    /// client's identd.
    ///
    /// An empty value and an absent key mean the same thing here, which is not this
    /// file's usual rule. Upstream keeps them apart as `""` and NULL, and the
    /// distinction reaches exactly one place: a SOCKS5 method list that offers
    /// username/password with two empty strings in hand. No server accepts that, so
    /// nothing observable is lost by treating both as "no credentials".
    pub proxy_user: String,
    /// `ProxyWSockHook.h:1983`, and it is stored in plain text, as upstream stores
    /// it. Upstream reads it only when `ProxyUser` is set and then uses it without
    /// checking — `begin_relay_http` reaches `strlen(proxy.pass)` under a test of
    /// the *user* alone (`:1275`), so a username with no password is a NULL
    /// dereference on the first HTTP proxy connection, and the dialog produces
    /// exactly that pair from an empty password box (`:1013`). Here an absent
    /// password is the empty string, which is what `user:` means to Basic.
    pub proxy_pass: String,
    /// `ProxyWSockHook.h:1987`, in seconds. It bounds every send and receive of the
    /// handshake, as a `select` timeout there and as a socket timeout here, and
    /// **zero is upstream's "wait forever"** — a null `timeval` — which is kept.
    ///
    /// The floor is a divergence with no cost: a negative value makes upstream's
    /// `timeval` invalid, so `select` fails with `EINVAL` and every proxy connection
    /// breaks. Below zero takes the default instead.
    pub proxy_timeout: i32,
    /// `ProxyWSockHook.h:1990`. Who turns the host name into an address — `local`
    /// resolves here and fails if it cannot, `remote` never resolves here and sends
    /// the name, `auto` resolves here and falls back to sending the name. Compared
    /// case-insensitively with an `else` of `auto`.
    ///
    /// Only the two SOCKS relays consult it; HTTP and the telnet proxy send the name
    /// as text and have no choice to make. For SOCKS4 it is also what decides
    /// whether the request is a 4a one, since that protocol expresses an unresolved
    /// name by an address of `0.0.0.1` and a name after the user ID.
    pub proxy_socks_resolve: ProxySocksResolve,
    /// `ProxyWSockHook.h:1999`. The five strings the `telnet` proxy type watches for
    /// and answers, which exist because that "protocol" is a person's session with a
    /// terminal server and every vendor's prompts are different.
    ///
    /// Each is matched as a substring of **one line**: `wait_for_prompt` (`:1228`)
    /// reads to a newline and then looks for each of the five in what it has, so a
    /// prompt split across two lines is never found and a line holding two of them
    /// takes the first. The trailing space in this one is part of the value.
    pub proxy_telnet_hostname_prompt: String,
    /// `ProxyWSockHook.h:2000`. Answered with `proxy.user`.
    pub proxy_telnet_username_prompt: String,
    /// `ProxyWSockHook.h:2001`. Answered with `proxy.pass`.
    pub proxy_telnet_password_prompt: String,
    /// `ProxyWSockHook.h:2002`. Seeing this ends the relay and starts the session.
    pub proxy_telnet_connected_message: String,
    /// `ProxyWSockHook.h:2003`. Seeing this ends the relay as a refusal.
    pub proxy_telnet_error_message: String,
    /// `ProxyWSockHook.h:2005`, and the only diagnostic there is for a handshake
    /// that fails: everything the relay does happens before the terminal has a
    /// session, so a refusal reaches the user as one sentence in a box and nothing
    /// else. Empty is no trace.
    ///
    /// `TTProxy/Logger.h` appends two kinds of record — `send: [ 05 01 00 ]` for the
    /// two SOCKS relays and `send: "…"` for HTTP and the telnet proxy — and Sterna
    /// writes the same two, so a trace taken here reads beside one taken from Tera
    /// Term. **The credentials are in it**, Base64 being no more than a spelling.
    ///
    /// A relative name resolves against `ts.LogDirW` (`TTProxy.h:198`), which is the
    /// **program's** log directory and not the terminal's; no key in this file moves
    /// it. The one departure is when the file appears: upstream opens it as it reads
    /// this key, so the key alone creates an empty file in a session that never
    /// connects, and here the first handshake with something to say creates it.
    pub proxy_debug_log: String,
    /// `ttset.c:1291`, a wide string whose empty default means no automatic macro.
    /// `CVTWindow::Startup` (`vtwin.cpp:1413`) consumes it once when the window
    /// starts; a leading `*` makes TTPMACRO put up its file picker
    /// (`ttmmain.cpp:285`). `/M` can replace it and a `/D=` topic clears it, so a
    /// terminal launched by a macro does not recursively launch another one.
    pub macro_startup_file: String,
    /// `ttset.c:575`. Upstream parses the first two components to decide which
    /// compatibility migrations to apply, and its writer stamps the running Tera
    /// Term version. Sterna preserves the source metadata without claiming that it
    /// is that program; 5.7.0 is the current upstream fallback this schema targets.
    pub settings_source_version: String,
    /// `ttset.c:1489` delegates this read to `GetUILanguageFileFullW`, whose own
    /// fallback is `lang\Default.lng` (`common/ttlib_static_cpp.cpp:1041`). The
    /// default catalog contains font choices only, so absent translations fall back
    /// to the source-language text embedded in the application. Upstream resolves a
    /// relative value beside its executable; the shell does the platform-specific
    /// installed-data resolution after this compatible value has been read.
    pub settings_language_file: String,
    /// `ttset.c:1999`, default **on**. Before Setup > Save setup overwrites the
    /// active file, upstream copies its old bytes to a sibling named
    /// `YYYYMMDDTHHMMSS+zzzz_TERATERM.INI` (`vtwin.cpp:4738`,
    /// `ttlib_static_cpp.cpp:1432`). A first save has no old file to copy, and the
    /// smaller close-time geometry save does not take this path.
    pub settings_auto_backup: bool,
    /// `ttset.c:916`, narrowed to a `WORD` by the assignment as `MaxComPort` is.
    ///
    /// **The bound is a different setting and is not a clamp**: `:1223` resets the
    /// port to 1 when it is below 1 or above `MaxComPort`, which is read at `:1218`
    /// — after this key but before the check. That is a cross-field rule with an
    /// order to it, which no row here can carry, so it lives in `Settings::normalize`
    /// beside `Debug`/`DebugModes`. The narrowing has to be the row's, though,
    /// because the reset compares the *narrowed* value: `ComPort=65538` is port 2
    /// upstream rather than a number above every ceiling.
    ///
    /// What a number means on Linux is a separate question: this port opens a device
    /// path, so `/C=1` is resolved against enumeration rather than against `COM1`.
    pub serial_com_port: i32,
    /// `ttset.c:919`. Unbounded, and the hardware decides what it can do with it —
    /// `tt-conn` reads the setting back after `tcsetattr` for exactly that reason.
    ///
    /// **The default is 115200 and upstream's is 9600** — the first deliberate
    /// deviation, and `docs/deviations.md` has the reason: nothing this program is
    /// pointed at ships a 9600 console any more, so upstream's default is one more
    /// thing to change before the first useful connection. The key, its bounds and
    /// what a value in the file means are unchanged, so a `TERATERM.INI` that says
    /// `BaudRate=9600` still opens at 9600 in both programs.
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
    /// `ttset.c:1218`, floored at 4 and capped at `MAXCOMPORT` (4096,
    /// `tttypes.h:908`). This is the setting `/C=` is bounded against, which is why
    /// the parser takes it as an argument.
    ///
    /// **A clamp at both ends and not a range**, and the narrowing in front of it is
    /// load-bearing rather than pedantry. `ts.MaxComPort` is a `WORD`, so the
    /// assignment wraps before either test runs: `MaxComPort=-1` — which is what
    /// somebody writes meaning "no limit" — is 65535 and then 4096, where an
    /// `int_clamp` would read the -1 as below the floor and answer **4**, the other
    /// end of the range. `int(4..4096)` was wrong a second way as well, since below
    /// the floor it gave the default of 256 rather than upstream's 4.
    pub serial_max_com_port: i32,
    /// **`ttset.c:2034`, and the default is a sentinel rather than a value.** Read
    /// with a default of `-1` and then derived from the flow control: RTS becomes
    /// Handshake under `FlowCtrl=hard` and Enable under anything else. This is the
    /// `TCPPort` trap the right way up — `FlowCtrl` is read at `:943`, eleven
    /// hundred lines earlier, so the derivation really does see the file's own
    /// value rather than an initialiser.
    ///
    /// The numbers are Win32 `DCB` fields, not a table of Tera Term's own: 0
    /// disable, 1 enable, 2 handshake, 3 toggle — and only RTS offers the fourth
    /// (`serial_pp.cpp:74`). An out-of-range number is where this gets dangerous.
    /// `CommResetSerial` puts it straight into the `DCB` and **does not check what
    /// `SetCommState` says about it** (`commlib.c:240`), so `FlowCtrlRTS=9` makes
    /// Windows reject the whole structure and the port keeps whatever it had —
    /// every serial setting in the file silently discarded, baud included, with no
    /// message. Not reproduced: `tt-session`'s `serial_params` reads anything it
    /// does not know as Enable.
    ///
    /// Held as the sentinel rather than resolved on the way in, the same call
    /// `connection.terminal_speed` makes for the same reason — the schema cannot
    /// say "the default is another setting", so the resolution is in
    /// `serial_params`. **Upstream resolves at load and writes the concrete number
    /// back**, so its own save pins the line and changing the flow control
    /// afterwards no longer moves it; this port keeps the `-1`, which a real Tera
    /// Term reads the same way it reads an absent key.
    pub serial_rts: i32,
    /// `ttset.c:2042`, the same sentinel as `serial.rts` against a different arm:
    /// DTR becomes Handshake under `FlowCtrl=dsrdtr` and Enable otherwise. There is
    /// no toggle for DTR — `serial_pp.cpp:75` lists three — and Win32 has no
    /// `DTR_CONTROL_TOGGLE` to list.
    pub serial_dtr: i32,
    /// `ttset.c:1147`, `GetOnOff(…, TRUE)`. Purge whatever the driver has already
    /// buffered when the port opens, rather than delivering it as the session's
    /// first bytes. Off is a real choice on a console server: it is how you see
    /// what the far end said before you got there, and upstream marks the port
    /// readable straight away for it (`commlib.c:476`'s `cv->RRQ`).
    ///
    /// It gates the purge on **open** only. Control > Reset port purges whatever
    /// this says (`vtwin.cpp:4913` passes TRUE outright), so the setting is not the
    /// answer to "does resetting the port clear it".
    pub serial_clear_buffer_on_open: bool,
    /// `ttset.c:1286`, milliseconds. How long `CommSendBreak` holds the line at
    /// space — Control > Send break, and a macro's `sendbreak`, which reaches the
    /// same place through DDE (`ttdde.c:801` posts the menu command).
    ///
    /// One second is a long break and it is deliberate: a Sun PROM wants one, and
    /// `commlib.c:1176` says "pause for 1 sec" in a comment beside the parameter.
    /// This port had **three** durations and none of them was the file's — 300 ms
    /// in `MainWindow.cpp`, 250 ms in `tt-macro`, and whatever a caller of
    /// `tt_session_send_break` passed.
    pub serial_break_time: i32,
    /// `ttset.c:1086`, `GetOnOff(…, TRUE)`. Reopen the port by itself when the
    /// adapter comes back — the USB-serial cable somebody unplugged, which is the
    /// whole reason the setting exists.
    ///
    /// Upstream drives it from `WM_DEVICECHANGE` (`vtwin.cpp:311`), so the Linux
    /// half is a udev monitor and is not built yet; the four keys below describe a
    /// state machine this port carries and does not yet run.
    pub serial_auto_reconnect: bool,
    /// `ttset.c:1088`, milliseconds. The wait between the device arriving and the
    /// reopen, for the case where the arrival named the port it was about.
    pub serial_auto_reconnect_delay: i32,
    /// `ttset.c:1090`, milliseconds, and **"illegal" is about the notification and
    /// not about a value.** Some drivers send only `DBT_DEVTYP_DEVICEINTERFACE` and
    /// never the `DBT_DEVTYP_PORT` that would say *which* port arrived
    /// (`vtwin.cpp:335`), so this is the longer wait taken when the port number is
    /// unknown and the reopen is a guess.
    pub serial_auto_reconnect_delay_unknown_port: i32,
    /// `ttset.c:1092`, milliseconds between one failed reopen and the next.
    pub serial_auto_reconnect_retry_interval: i32,
    /// `ttset.c:1094`. Retries **after** the first attempt, so three is four tries —
    /// and unlike `BeepOverUsedCount` the name is honest about it. Two details are
    /// not in the name: an attempt where the port is still absent costs a retry
    /// without opening anything (`vtwin.cpp:475`'s `CheckComPort` guard), and the
    /// *last* attempt is the one allowed to raise the error box, because the
    /// suppression tests `retry_left_ != 0` (`:481`).
    ///
    /// The four above are `WORD` in `tttypes.h:602`, so upstream truncates them to
    /// 16 bits: a two-minute retry interval written as `120000` is 54464 ms there
    /// and 120000 here. Not reproduced — the schema has no type for it and the
    /// divergence only exists for values nobody means.
    pub serial_auto_reconnect_retries: i32,
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
    /// `ttset.c:1063`. A file-dialog mask such as `*.txt;*.log`. Upstream uses it
    /// for raw Send file and every protocol's send picker.
    pub transfer_send_filter: String,
    /// `ttset.c:2029`. The corresponding mask for raw Receive file. Protocols that
    /// carry their own filename do not ask for one and therefore do not use it.
    pub transfer_receive_filter: String,
    /// `ttset.c:1068`. The destination directory offered by drag-and-drop SCP.
    /// `scp` is one of the macro host's explicitly unsupported transport commands,
    /// so this is retained for the file and the future implementation.
    pub transfer_scp_send_dir: String,
    /// `ttset.c:1191`, default **on**. The boosted raw sender is used only for a
    /// binary serial send at 115200 baud or faster with no echo, telnet framing or
    /// bracketed paste (`filesys.cpp:365`). It does not govern X/Y/ZMODEM or Kermit.
    pub transfer_raw_high_speed: bool,
    /// `ttset.c:1511`, default **on**. Whether dropping files opens the choice
    /// between raw send, SCP and cancel. The shell has no file-drop target yet.
    pub transfer_confirm_file_drop: bool,
    /// `ttset.c:2006`. How raw Send file spaces writes. Anything unrecognised takes
    /// `NoDelay`, including an empty present value.
    pub transfer_raw_send_delay_type: TransferRawSendDelayType,
    /// `ttset.c:2011`, in milliseconds. The delay selected above; zero is a real
    /// value and means no wait.
    pub transfer_raw_send_delay_tick: i32,
    /// `ttset.c:2012`. The chunk size for `PerSendSize`, and the raw sender's read
    /// size. Upstream applies no range at settings-load time.
    pub transfer_raw_send_size: i32,
    /// `ttset.c:2013`. Use Tera Term 4's sequential file reader rather than loading
    /// chunks into memory before sending them. Off is the newer sender.
    pub transfer_raw_send_sequential: bool,
    /// `ttset.c:2014`. Skip raw Send file's option page and use the remembered
    /// binary and delay settings directly.
    pub transfer_raw_send_skip_dialog: bool,
    /// `ttset.c:2030`. The corresponding switch for raw Receive file.
    pub transfer_raw_receive_skip_dialog: bool,
    /// `ttset.c:1237`, assigned to the 16-bit `ts.PassThruDelay` and written
    /// unsigned. It is the delay before controller-mode pass-through printing starts.
    pub printer_passthrough_delay: i32,
    /// `ttset.c:1241`. The raw printer device used for pass-through mode; empty lets
    /// upstream choose its ordinary printer path.
    pub printer_passthrough_port: String,
    /// `ttset.c:1245`, `TF_PRINTERCTRL`, default off. Gates the printer control
    /// sequences themselves, independently of whether a printer device was named.
    pub printer_control_sequences: bool,
    /// `ttset.c:1255`. Margins are hundredths of an inch, left, right, top and
    /// bottom. `GetNthNum` makes a missing field zero in a present value.
    pub printer_margin_left: i32,
    /// The second field of `PrnMargin`, under the same `ttset.c:1255` rule.
    pub printer_margin_right: i32,
    /// The third field of `PrnMargin`, under the same `ttset.c:1255` rule.
    pub printer_margin_top: i32,
    /// The fourth field of `PrnMargin`, under the same `ttset.c:1255` rule.
    pub printer_margin_bottom: i32,
    /// `ttset.c:1263`, default off. Converts a form feed to a newline while printing.
    pub printer_convert_form_feed: bool,
    /// `ttset.c:1368`, first field. A positive value overrides the printer's native
    /// horizontal pixels per inch for VT printing; zero uses the device value.
    pub printer_vt_ppi_x: i32,
    /// The second field of `VTPPI`, the vertical printer resolution under the same
    /// rule. Printing is Stage 3 work, so both fields currently round-trip only.
    pub printer_vt_ppi_y: i32,
    /// `ttset.c:603`. The TEK window's x/y position, using the same `CW_USEDEFAULT`
    /// sentinel and `GetNthNum` zero-for-a-missing-field rule as `VTPos`.
    pub tek_x: i32,
    /// The second field of `TEKPos`, under the same `ttset.c:603` rule.
    pub tek_y: i32,
    /// `ttset.c:789`, black on white. The pair remains a pair even though no TEK
    /// painter consumes it here.
    pub tek_color: [u8; 6],
    /// `ttset.c:860`, default off. Enables colour emulation in the dropped TEK parser.
    pub tek_color_emulation: bool,
    /// `ttset.c:1295`. The byte a TEK GIN mouse report uses, with no load-time range.
    pub tek_gin_mouse_code: i32,
    /// `ttset.c:1555` passes the value through `IconName2IconId`. The ten known
    /// names compare case-insensitively; anything else becomes `Default`.
    pub tek_icon: TekIcon,
    /// `ttset.c:1374`. Both positive values override the printer's native pixels
    /// per inch for a TEK print; zero in either axis means use the device values.
    pub tek_ppi_x: i32,
    /// The second field of `TEKPPI`, under the same `ttset.c:1374` rule.
    pub tek_ppi_y: i32,
    /// `ttset.c:1527`. The string `on` means 2; every other present value goes
    /// through `atoi`, then narrows into a Win32 `WORD`. This controls an old
    /// maximised-window sizing workaround and has no corresponding Qt defect.
    pub window_maximized_bug_tweak: i32,
    /// `ttset.c:1550`, through `IconName2IconId`. The ten names compare
    /// case-insensitively and anything else becomes `Default`. Sterna keeps its own
    /// mark, so this is compatibility data for a shared Tera Term setup.
    pub window_icon: WindowIcon,
    /// `ttset.c:728`. No title bar, which `/H` also asks for. `/I` and `/V` —
    /// minimised and invisible — have no keys at all: `_ReadIniFile` zeroes both at
    /// `:554` and never reads one, so they are command-line-only.
    pub window_hide_title: bool,
    /// `ttset.c:731`. Hide the ordinary menu bar. When it is hidden, upstream opens
    /// the same menus as a popup on Ctrl+left-click (`vtwin.cpp:863`); this is not a
    /// choice between two different menus. `HideTitle` also removes the menu bar,
    /// independently of this key (`vtwin.cpp:3461`).
    pub window_popup_menu: bool,
    /// `ttset.c:1179`, default **on**. The gate on Ctrl+left-click opening the full
    /// menu while the bar is hidden. It does not decide whether the bar is hidden —
    /// that is `window.popup_menu`, or `window.hide_title` as a side effect.
    pub window_popup_menu_enabled: bool,
    /// `ttset.c:1183`, default **on**. With the bar hidden, upstream adds "Show menu
    /// bar" to the Win32 system menu (`vtwin.cpp:3509`). Qt cannot add application
    /// actions to a compositor-owned system menu, so the shell puts the recovery
    /// action in the Ctrl+left-click popup instead.
    pub window_show_menu_enabled: bool,
    /// `ttset.c:1380`, default **on**. Adds the dynamic Window menu, whose entries
    /// are every open VT and TEK window (`vtwin.cpp:1116`). This process owns one
    /// terminal window and TEK is out of scope, so it is read and written and acts
    /// on nothing until Stage 3 gives the shell multiple sessions to list.
    pub window_window_menu: bool,
    /// `ttset.c:608`, default off. Upstream updates `ts.VTPos` on every move but
    /// writes it only under this switch — both during Save setup (`:2109`) and on
    /// window close (`SaveVTPos`, `:3340`). The switch itself is read-only upstream:
    /// `_WriteIniFile` never writes the key, so a user enables it by hand. This port
    /// exposes it through the generated dialog and writes the same upstream key.
    pub window_save_position: bool,
    /// `ttset.c:598`, first half. `CW_USEDEFAULT` is `INT_MIN`, so an absent key asks
    /// the window manager to place the window rather than meaning coordinate zero.
    /// **Conditionally written**: with `SaveVTWinPos=off`, `_WriteIniFile` leaves an
    /// existing `VTPos` line byte-for-byte alone (`ttset.c:2109`).
    pub window_x: i32,
    /// `ttset.c:600`, second half of the same pair and the same sentinel.
    pub window_y: i32,
    /// `ttset.c:706`, default off. Upstream switches between its VT and TEK windows
    /// when either parser sees the other terminal's sequence. TEK is deliberately
    /// out of scope here, so the key round-trips and acts on nothing.
    pub window_auto_vt_tek_switch: bool,
    /// `ttset.c:1476`. The trailing space is in upstream's **reader literal** and
    /// the writer at `:2250` omits it. Backticks retain that space in the schema;
    /// `write-key` reproduces the writer. This is a Windows Cygwin install path, not
    /// the local pty's working directory, so it has no runtime effect here.
    pub connection_cygwin_directory: String,
    /// `ttset.c:1483`. The external editor used by View log. Sterna has no View log
    /// command yet, so the Windows default is preserved and the key round-trips.
    pub log_viewer: String,
    /// `ttset.c:1485`; the allocated-string helper's null fallback becomes empty.
    /// Arguments for `log.viewer`, carried until that command exists.
    pub log_viewer_args: String,
    /// `ttset.c:1493`, default off. Whether the broadcast dialog records commands.
    /// Broadcast is a multi-window feature and this shell owns one terminal today.
    pub broadcast_command_history: bool,
    /// `ttset.c:1497`, default **on**. Whether this terminal accepts a command from
    /// another Tera Term window. There is no broadcast channel in Sterna yet.
    pub broadcast_accept: bool,
    /// `ttset.c:1501`. What the non-realtime broadcast dialog appends. The else arm
    /// is `None`, so an unrecognised value and an absent key agree.
    pub broadcast_submit_key: BroadcastSubmitKey,
    /// `ttset.c:1319`. Upstream's misspelling is the public key and is preserved.
    /// It caps the broadcast command history, which is not built yet.
    pub broadcast_max_history: i32,
    /// `ttset.c:1452`. Suppresses Alt+B while leaving Control > Send break alone.
    pub menu_disable_accelerator_send_break: bool,
    /// `ttset.c:1606`. Greys Control > Send break even when the transport supports
    /// it. This is independent of the accelerator switch above.
    pub menu_disable_send_break: bool,
    /// `ttset.c:1614`. Suppresses Alt+D while leaving File > Duplicate session.
    pub menu_disable_accelerator_duplicate: bool,
    /// `ttset.c:1617`, default **on**. Enables Alt+N for New connection.
    pub menu_accelerator_new_connection: bool,
    /// `ttset.c:1620`, default **on**. Enables Alt+G for Cygwin connection; Sterna's
    /// equivalent command is Local shell.
    pub menu_accelerator_local_shell: bool,
    /// `ttset.c:1624`. Greys Duplicate session.
    pub menu_disable_duplicate: bool,
    /// `ttset.c:1628`. While already connected, greys New connection rather than
    /// allowing the current line to be replaced.
    pub menu_disable_new_connection: bool,
    /// `ttset.c:1602`, default off. Upstream mirrors received text into shared
    /// memory only under this switch so `wait4all` can inspect every macro process.
    /// Sterna has no process-global DDE ring; the command remains explicitly
    /// unsupported and the compatibility key still round-trips.
    pub macro_wait4all: bool,
    /// `ttset.c:1714`, default **on**. Adds recent connections to the Windows taskbar
    /// jump list. Qt/Linux has no equivalent owned by this process.
    pub window_jump_list: bool,
    /// `ttset.c:1993`, default off. Requests square corners from Windows 11 DWM.
    /// Window decoration belongs to the compositor on Linux, so it is carried only.
    pub window_corner_dontround: bool,
    /// **`[Sterna]`**, because upstream has no toolbar and so no key to be
    /// compatible with. Whether the bar under the menu — port, connect, local echo —
    /// is shown. It exists as a setting rather than as chrome nobody can remove:
    /// `window.popup_menu` and `window.hide_title` are about the *menu*, so neither
    /// hides this, and Setup > Show toolbar writes it.
    pub window_toolbar: bool,
    /// **`[Sterna]`**, because simultaneous panes are a Sterna window feature rather
    /// than terminal state. The value says how many connections this window shows at
    /// once: one terminal, two equal side-by-side panels, or four equal panels in a
    /// 2x2 grid. A missing or unrecognised spelling returns to the single-panel
    /// layout, so an edited file cannot leave the window without a usable view.
    pub window_panel_layout: WindowPanelLayout,
    /// **`[Sterna]`** again, and these two are only the *bar*: the buttons on it are
    /// a list, which no schema row can be, and they live in a `[Sterna Buttons]`
    /// section of their own (`tt-config/src/buttons.rs`).
    ///
    /// On, but the bar is hidden while nobody has defined a button — an empty
    /// toolbar is chrome for nothing. So this is what Setup > Show quick buttons
    /// writes, not what decides whether the bar exists.
    pub window_quick_buttons: bool,
    /// Which edge the bar is on. Qt lets a toolbar be dragged to any of the four,
    /// and this is where it opens.
    ///
    /// **The right**, not the top, which is where the other bar is. A terminal's
    /// rows are the scarce dimension — a window is usually far wider than the 80
    /// columns it needs and exactly as tall as it can be — so a vertical bar costs
    /// nothing that is being used, and the labels have room to be words rather than
    /// abbreviations. An unrecognised spelling lands on the right as well, rather
    /// than hiding the bar.
    pub window_quick_buttons_area: WindowQuickButtonsArea,
    /// The device path, not a number: `ComPort` is upstream's and cannot spell
    /// `/dev/serial/by-id/usb-FTDI_…`. Written as the port was opened, so it is the
    /// `open_path` a stable symlink resolves from rather than the `ttyUSB<n>` that
    /// moves on replug.
    pub recent_serial_port: String,
    /// The SSH host the dialog was given — an alias out of `~/.ssh/config` as
    /// readily as a name, since that is what the dialog's combo box is filled from.
    /// Empty is the whole record's switch: nothing remembered, so the dialog opens
    /// as it does on a fresh install.
    pub recent_ssh_host: String,
    /// The user, where one was typed. Empty means "whatever `~/.ssh/config` says",
    /// which is not the same as an empty user name — so the absence round-trips
    /// rather than being resolved on the way in.
    pub recent_ssh_user: String,
    /// The port, where one was asked for. Zero is the same kind of absence as an
    /// empty user: it leaves the choice to `~/.ssh/config` and, failing that, to 22.
    pub recent_ssh_port: i32,
    /// The private key the dialog was given, if it was given one. `~/.ssh/config`
    /// and the agent are consulted regardless, so this is an addition to them.
    pub recent_ssh_identity: String,
    /// The pre-2020 algorithm switch, which is a property of the equipment at the
    /// far end rather than a preference — a console server that needed it once needs
    /// it every time, and that is the whole reason this record exists.
    pub recent_ssh_legacy: bool,
    /// The telnet host. Empty is this record's switch, as `SshHost` is its own.
    pub recent_telnet_host: String,
    /// The telnet port, kept separate from `[Tera Term] TCPPort` deliberately: that
    /// one is also what a bare host name on the command line opens, so remembering a
    /// console server's per-line port there would move an SSH connection to it.
    pub recent_telnet_port: i32,
    /// Which of the four framing/burst combinations was used (`TelnetMode::of`).
    /// `auto` is the shipped one: data until the first `IAC`, telnet after it.
    pub recent_telnet_mode: RecentTelnetMode,
    /// Whether starting Sterna may look for a new release. On, so an installed
    /// terminal learns about a signed update without anyone remembering to ask.
    pub updates_check_on_startup: bool,
    /// When the last look happened, as ISO-8601 UTC — the "once a day" half of it.
    /// Written when a check *starts*, so a release server that cannot be reached
    /// costs one request a day rather than one per launch. Empty means never, which
    /// is also what clearing it by hand means: check at the next start. Anything
    /// unparsable, and anything in the future — a clock moved back, an edited file —
    /// reads the same way, because the alternative is a terminal that never looks
    /// again.
    pub updates_last_check: String,
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
            terminal_clear_on_resize: false,
            terminal_home_erase_clears_screen: true,
            terminal_scrollback_enabled: true,
            terminal_scrollback_lines: 100,
            terminal_buffer_max_lines: 10000,
            terminal_iso2022_shifts: String::from("on"),
            terminal_title: String::from("Tera Term"),
            terminal_answerback: String::from(""),
            terminal_back_wrap: false,
            terminal_vt_compat_tab: false,
            terminal_tab_stop_modify: String::from("on"),
            terminal_invalid_decrqss: false,
            terminal_uid: String::from("FFFFFFFF"),
            terminal_lock_uid: true,
            terminal_auto_invoke: false,
            terminal_max_osc_buffer: 4096,
            terminal_allow_wrong_sequence: false,
            terminal_status_line_enabled: true,
            encoding_receive: EncodingReceive::default(),
            encoding_send: EncodingSend::default(),
            encoding_katakana_receive: EncodingKatakanaReceive::default(),
            encoding_katakana_send: EncodingKatakanaSend::default(),
            encoding_kanji_in: EncodingKanjiIn::default(),
            encoding_kanji_out: EncodingKanjiOut::default(),
            encoding_ctrl_in_kanji: true,
            encoding_fixed_jis: false,
            encoding_fallback_cp932: false,
            encoding_russian_keyboard: EncodingRussianKeyboard::default(),
            encoding_dec_special_direction: EncodingDecSpecialDirection::default(),
            encoding_unicode_to_dec_special: 3,
            encoding_ambiguous_width: 1,
            encoding_emoji_override: false,
            encoding_emoji_width: 1,
            ime_enabled: true,
            ime_inline: true,
            ime_cursor_related: false,
            keyboard_backspace: KeyboardBackspace::default(),
            keyboard_meta: KeyboardMeta::default(),
            keyboard_delete_sends_del: false,
            keyboard_disable_app_keypad: false,
            keyboard_disable_app_cursor: false,
            keyboard_word_delimiters: String::from("$20!\"#$24%&'()*+,-./:;<=>?@[\\]^`{|}~"),
            keyboard_width_delimits_word: true,
            keyboard_strict_mapping: false,
            keyboard_meta_8bit: KeyboardMeta8bit::default(),
            color_normal: [0, 0, 0, 255, 255, 255],
            color_bold: [0, 0, 255, 255, 255, 255],
            color_blink: [255, 0, 0, 255, 255, 255],
            color_underline: [255, 0, 255, 255, 255, 255],
            color_reverse: [255, 255, 255, 0, 0, 0],
            color_url: [0, 255, 0, 255, 255, 255],
            color_url_enabled: true,
            color_url_underline: true,
            color_bold_enabled: true,
            color_blink_enabled: true,
            color_reverse_enabled: false,
            color_underline_enabled: true,
            color_ansi_palette: String::from("0,0,0,0,1,255,0,0,2,0,255,0,3,255,255,0,4,0,0,255,5,255,0,255,6,0,255,255,7,255,255,255,8,128,128,128,9,128,0,0,10,0,128,0,11,128,128,0,12,0,0,128,13,128,0,128,14,0,128,128,15,192,192,192"),
            color_xterm_256: true,
            color_aixterm_16: false,
            color_pc_bold_16: false,
            color_ansi_enabled: true,
            color_bold_font: true,
            color_underline_font: true,
            color_use_text_color: false,
            color_use_normal_background: false,
            cursor_shape: CursorShape::default(),
            cursor_nonblinking: false,
            cursor_show_unfocused: true,
            window_change_allowed: true,
            window_report_allowed: true,
            window_cursor_ctrl_allowed: false,
            window_accept_8bit_ctrl: true,
            window_send_8bit_ctrl: false,
            window_alt_screen: true,
            window_remote_clears_buffer: true,
            window_auto_scroll_only_at_bottom: false,
            window_scroll_threshold: 12,
            window_opacity_inactive: 255,
            window_opacity_active: 255,
            window_title_format: 13,
            window_title_change: WindowTitleChange::default(),
            window_title_report: WindowTitleReport::default(),
            font_quality: FontQuality::default(),
            font_draw_resized: true,
            font_draw_api: FontDrawApi::default(),
            font_draw_ansi_code_page: 0,
            font_space_left: 0,
            font_space_right: 0,
            font_space_top: 0,
            font_space_bottom: 0,
            font_scaling: false,
            mouse_tracking: true,
            mouse_ctrl_disables_tracking: true,
            mouse_wheel_to_cursor: true,
            mouse_ctrl_disables_wheel_to_cursor: true,
            mouse_wheel_scroll_line: 3,
            mouse_clickable_url: false,
            mouse_cursor: String::from("IBEAM"),
            url_browser: String::from(""),
            url_browser_args: String::from(""),
            url_join_split: false,
            url_join_split_ignore_eol_char: String::from("\\"),
            debug_enabled: false,
            debug_modes: String::from("all"),
            bell_mode: BellMode::default(),
            bell_on_connect: false,
            bell_visual_wait_ms: 10,
            bell_over_used_count: 5,
            bell_over_used_time: 2,
            bell_suppress_time: 5,
            bell_notify_sound: true,
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
            clipboard_remote_access: ClipboardRemoteAccess::default(),
            clipboard_remote_notify: true,
            connection_port_type: ConnectionPortType::default(),
            connection_tcp_port: 23,
            connection_telnet: true,
            connection_telnet_port: 23,
            connection_telnet_binary: false,
            connection_telnet_auto_detect: true,
            connection_telnet_echo: false,
            connection_telnet_log: false,
            connection_telnet_keepalive: 300,
            connection_tcp_local_echo: false,
            connection_tcp_cr_send: ConnectionTcpCrSend::default(),
            connection_confirm_disconnect: true,
            connection_history_list: false,
            connection_term_type: String::from("xterm"),
            connection_terminal_speed: String::from("38400"),
            connection_auto_win_close: true,
            connection_clear_screen_on_close: false,
            connection_timeout: 0,
            connection_host_dialog_on_startup: true,
            connection_line_mode: true,
            proxy_type: ProxyType::default(),
            proxy_host: String::from(""),
            proxy_port: 0,
            proxy_user: String::from(""),
            proxy_pass: String::from(""),
            proxy_timeout: 10,
            proxy_socks_resolve: ProxySocksResolve::default(),
            proxy_telnet_hostname_prompt: String::from(">> Host name: "),
            proxy_telnet_username_prompt: String::from("Username:"),
            proxy_telnet_password_prompt: String::from("Password:"),
            proxy_telnet_connected_message: String::from("-- Connected to "),
            proxy_telnet_error_message: String::from("!!!!!!!!"),
            proxy_debug_log: String::from(""),
            macro_startup_file: String::from(""),
            settings_source_version: String::from("5.7.0"),
            settings_language_file: String::from("lang\\Default.lng"),
            settings_auto_backup: true,
            serial_com_port: 1,
            serial_baud: 115200,
            serial_data_bits: SerialDataBits::default(),
            serial_parity: SerialParity::default(),
            serial_stop_bits: SerialStopBits::default(),
            serial_flow: SerialFlow::default(),
            serial_delay_per_char: 0,
            serial_delay_per_line: 0,
            serial_wait_com: false,
            serial_max_com_port: 256,
            serial_rts: -1,
            serial_dtr: -1,
            serial_clear_buffer_on_open: true,
            serial_break_time: 1000,
            serial_auto_reconnect: true,
            serial_auto_reconnect_delay: 500,
            serial_auto_reconnect_delay_unknown_port: 2000,
            serial_auto_reconnect_retry_interval: 1000,
            serial_auto_reconnect_retries: 3,
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
            transfer_send_filter: String::from(""),
            transfer_receive_filter: String::from(""),
            transfer_scp_send_dir: String::from(""),
            transfer_raw_high_speed: true,
            transfer_confirm_file_drop: true,
            transfer_raw_send_delay_type: TransferRawSendDelayType::default(),
            transfer_raw_send_delay_tick: 0,
            transfer_raw_send_size: 4096,
            transfer_raw_send_sequential: false,
            transfer_raw_send_skip_dialog: false,
            transfer_raw_receive_skip_dialog: false,
            printer_passthrough_delay: 3,
            printer_passthrough_port: String::from(""),
            printer_control_sequences: false,
            printer_margin_left: 50,
            printer_margin_right: 50,
            printer_margin_top: 50,
            printer_margin_bottom: 50,
            printer_convert_form_feed: false,
            printer_vt_ppi_x: 0,
            printer_vt_ppi_y: 0,
            tek_x: -2147483648,
            tek_y: -2147483648,
            tek_color: [0, 0, 0, 255, 255, 255],
            tek_color_emulation: false,
            tek_gin_mouse_code: 32,
            tek_icon: TekIcon::default(),
            tek_ppi_x: 0,
            tek_ppi_y: 0,
            window_maximized_bug_tweak: 2,
            window_icon: WindowIcon::default(),
            window_hide_title: false,
            window_popup_menu: false,
            window_popup_menu_enabled: true,
            window_show_menu_enabled: true,
            window_window_menu: true,
            window_save_position: false,
            window_x: -2147483648,
            window_y: -2147483648,
            window_auto_vt_tek_switch: false,
            connection_cygwin_directory: String::from("c:\\cygwin"),
            log_viewer: String::from("notepad.exe"),
            log_viewer_args: String::from(""),
            broadcast_command_history: false,
            broadcast_accept: true,
            broadcast_submit_key: BroadcastSubmitKey::default(),
            broadcast_max_history: 99,
            menu_disable_accelerator_send_break: false,
            menu_disable_send_break: false,
            menu_disable_accelerator_duplicate: false,
            menu_accelerator_new_connection: true,
            menu_accelerator_local_shell: true,
            menu_disable_duplicate: false,
            menu_disable_new_connection: false,
            macro_wait4all: false,
            window_jump_list: true,
            window_corner_dontround: false,
            window_toolbar: true,
            window_panel_layout: WindowPanelLayout::default(),
            window_quick_buttons: true,
            window_quick_buttons_area: WindowQuickButtonsArea::default(),
            recent_serial_port: String::from(""),
            recent_ssh_host: String::from(""),
            recent_ssh_user: String::from(""),
            recent_ssh_port: 0,
            recent_ssh_identity: String::from(""),
            recent_ssh_legacy: false,
            recent_telnet_host: String::from(""),
            recent_telnet_port: 23,
            recent_telnet_mode: RecentTelnetMode::default(),
            updates_check_on_startup: true,
            updates_last_check: String::from(""),
        }
    }
}

impl Settings {
    /// Read every setting, taking the default for anything absent.
    pub fn load(ini: &Ini) -> Settings {
        let d = Settings::default();
        let mut settings = Settings {
            terminal_cols: crate::schema::ranged(
                crate::schema::nth_int_zero(
                    ini.get("Tera Term", "TerminalSize"),
                    0,
                    d.terminal_cols,
                ),
                d.terminal_cols,
                1,
                1000,
            ),
            terminal_rows: crate::schema::ranged(
                crate::schema::nth_int_zero(
                    ini.get("Tera Term", "TerminalSize"),
                    1,
                    d.terminal_rows,
                ),
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
            terminal_clear_on_resize: crate::schema::on_off(
                ini.get("Tera Term", "ClearOnResize"),
                false,
            ),
            terminal_home_erase_clears_screen: crate::schema::on_off(
                ini.get("Tera Term", "ScrollWindowClearScreen"),
                true,
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
            terminal_answerback: ini
                .get_or("Tera Term", "Answerback", &d.terminal_answerback)
                .to_string(),
            terminal_back_wrap: crate::schema::on_off(ini.get("Tera Term", "BackWrap"), false),
            terminal_vt_compat_tab: crate::schema::on_off(
                ini.get("Tera Term", "VTCompatTab"),
                false,
            ),
            terminal_tab_stop_modify: ini
                .get_or(
                    "Tera Term",
                    "TabStopModifySequence",
                    &d.terminal_tab_stop_modify,
                )
                .to_string(),
            terminal_invalid_decrqss: crate::schema::on_off(
                ini.get("Tera Term", "UseInvalidDECRQSSResponse"),
                false,
            ),
            terminal_uid: ini
                .get_or("Tera Term", "TerminalUID", &d.terminal_uid)
                .to_string(),
            terminal_lock_uid: crate::schema::on_off(ini.get("Tera Term", "LockTUID"), true),
            terminal_auto_invoke: crate::schema::on_off(ini.get("Tera Term", "AutoInvoke"), false),
            terminal_max_osc_buffer: ini.get_int(
                "Tera Term",
                "MaxOSCBufferSize",
                d.terminal_max_osc_buffer,
            ) as i32,
            terminal_allow_wrong_sequence: crate::schema::on_off(
                ini.get("Tera Term", "AllowWrongSequence"),
                false,
            ),
            terminal_status_line_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableStatusLine"),
                true,
            ),
            encoding_receive: match ini.get("Tera Term", "KanjiReceive") {
                Some(v) => EncodingReceive::from_ini(v),
                None => d.encoding_receive,
            },
            encoding_send: match ini.get("Tera Term", "KanjiSend") {
                Some(v) => EncodingSend::from_ini(v),
                None => d.encoding_send,
            },
            encoding_katakana_receive: match ini.get("Tera Term", "KatakanaReceive") {
                Some(v) => EncodingKatakanaReceive::from_ini(v),
                None => d.encoding_katakana_receive,
            },
            encoding_katakana_send: match ini.get("Tera Term", "KatakanaSend") {
                Some(v) => EncodingKatakanaSend::from_ini(v),
                None => d.encoding_katakana_send,
            },
            encoding_kanji_in: match ini.get("Tera Term", "KanjiIn") {
                Some(v) => EncodingKanjiIn::from_ini(v),
                None => d.encoding_kanji_in,
            },
            encoding_kanji_out: match ini.get("Tera Term", "KanjiOut") {
                Some(v) => EncodingKanjiOut::from_ini(v),
                None => d.encoding_kanji_out,
            },
            encoding_ctrl_in_kanji: crate::schema::on_off(
                ini.get("Tera Term", "CtrlInKanji"),
                true,
            ),
            encoding_fixed_jis: crate::schema::on_off(ini.get("Tera Term", "FixedJIS"), false),
            encoding_fallback_cp932: crate::schema::on_off(
                ini.get("Tera Term", "FallbackToCP932"),
                false,
            ),
            encoding_russian_keyboard: match ini.get("Tera Term", "RussKeyb") {
                Some(v) => EncodingRussianKeyboard::from_ini(v),
                None => d.encoding_russian_keyboard,
            },
            encoding_dec_special_direction: match ini.get("Tera Term", "DecSpMappingDir") {
                Some(v) => EncodingDecSpecialDirection::from_ini(v),
                None => d.encoding_dec_special_direction,
            },
            encoding_unicode_to_dec_special: crate::schema::word(ini.get_int(
                "Tera Term",
                "UnicodeToDecSpMapping",
                d.encoding_unicode_to_dec_special,
            ) as i32),
            encoding_ambiguous_width: crate::schema::validated(
                crate::schema::byte(ini.get_int(
                    "Tera Term",
                    "UnicodeAmbiguousWidth",
                    d.encoding_ambiguous_width,
                ) as i32),
                d.encoding_ambiguous_width,
                1,
                2,
            ),
            encoding_emoji_override: crate::schema::on_off(
                ini.get("Tera Term", "UnicodeEmojiOverride"),
                false,
            ),
            encoding_emoji_width: crate::schema::validated(
                crate::schema::byte(ini.get_int(
                    "Tera Term",
                    "UnicodeEmojiWidth",
                    d.encoding_emoji_width,
                ) as i32),
                d.encoding_emoji_width,
                1,
                2,
            ),
            ime_enabled: crate::schema::on_off(ini.get("Tera Term", "IME"), true),
            ime_inline: crate::schema::on_off(ini.get("Tera Term", "IMEInline"), true),
            ime_cursor_related: crate::schema::on_off(
                ini.get("Tera Term", "IMERelatedCursor"),
                false,
            ),
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
            keyboard_width_delimits_word: crate::schema::on_off(
                ini.get("Tera Term", "DelimDBCS"),
                true,
            ),
            keyboard_strict_mapping: crate::schema::on_off(
                ini.get("Tera Term", "StrictKeyMapping"),
                false,
            ),
            keyboard_meta_8bit: match ini.get("Tera Term", "Meta8Bit") {
                Some(v) => KeyboardMeta8bit::from_ini(v),
                None => d.keyboard_meta_8bit,
            },
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
            color_url: crate::schema::color2(ini.get("Tera Term", "URLColor"), d.color_url),
            color_url_enabled: crate::schema::on_off(ini.get("Tera Term", "EnableURLColor"), true),
            color_url_underline: crate::schema::on_off(ini.get("Tera Term", "URLUnderline"), true),
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
            color_ansi_palette: ini
                .get_or("Tera Term", "ANSIColor", &d.color_ansi_palette)
                .to_string(),
            color_xterm_256: crate::schema::on_off(ini.get("Tera Term", "Xterm256Color"), true),
            color_aixterm_16: crate::schema::on_off(ini.get("Tera Term", "Aixterm16Color"), false),
            color_pc_bold_16: crate::schema::on_off(ini.get("Tera Term", "PcBoldColor"), false),
            color_ansi_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableANSIColor"),
                true,
            ),
            color_bold_font: crate::schema::on_off(ini.get("Tera Term", "EnableBold"), true),
            color_underline_font: crate::schema::on_off(
                ini.get("Tera Term", "UnderlineAttrFont"),
                true,
            ),
            color_use_text_color: crate::schema::on_off(
                ini.get("Tera Term", "UseTextColor"),
                false,
            ),
            color_use_normal_background: crate::schema::on_off(
                ini.get("Tera Term", "UseNormalBGColor"),
                false,
            ),
            cursor_shape: match ini.get("Tera Term", "CursorShape") {
                Some(v) => CursorShape::from_ini(v),
                None => d.cursor_shape,
            },
            cursor_nonblinking: crate::schema::on_off(
                ini.get("Tera Term", "NonblinkingCursor"),
                false,
            ),
            cursor_show_unfocused: crate::schema::on_off(
                ini.get("Tera Term", "KillFocusCursor"),
                true,
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
            window_auto_scroll_only_at_bottom: crate::schema::on_off(
                ini.get("Tera Term", "AutoScrollOnlyInBottomLine"),
                false,
            ),
            window_scroll_threshold: ini.get_int(
                "Tera Term",
                "ScrollThreshold",
                d.window_scroll_threshold,
            ) as i32,
            window_opacity_inactive: crate::schema::byte(ini.get_int(
                "Tera Term",
                "AlphaBlend",
                d.window_opacity_inactive,
            ) as i32),
            window_opacity_active: crate::schema::byte(ini.get_int(
                "Tera Term",
                "AlphaBlendActive",
                d.window_opacity_active,
            ) as i32),
            window_title_format: crate::schema::word(ini.get_int(
                "Tera Term",
                "TitleFormat",
                d.window_title_format,
            ) as i32),
            window_title_change: match ini.get("Tera Term", "AcceptTitleChangeRequest") {
                Some(v) => WindowTitleChange::from_ini(v),
                None => d.window_title_change,
            },
            window_title_report: match ini.get("Tera Term", "TitleReportSequence") {
                Some(v) => WindowTitleReport::from_ini(v),
                None => d.window_title_report,
            },
            font_quality: match ini.get("Tera Term", "FontQuality") {
                Some(v) => FontQuality::from_ini(v),
                None => d.font_quality,
            },
            font_draw_resized: crate::schema::on_off(
                ini.get("Tera Term", "DrawingResizedFont"),
                true,
            ),
            font_draw_api: match ini.get("Tera Term", "VTDrawAPI") {
                Some(v) => FontDrawApi::from_ini(v),
                None => d.font_draw_api,
            },
            font_draw_ansi_code_page: ini.get_int(
                "Tera Term",
                "VTDrawACP",
                d.font_draw_ansi_code_page,
            ) as i32,
            font_space_left: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTFontSpace"),
                0,
                d.font_space_left,
            ),
            font_space_right: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTFontSpace"),
                1,
                d.font_space_right,
            ),
            font_space_top: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTFontSpace"),
                2,
                d.font_space_top,
            ),
            font_space_bottom: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTFontSpace"),
                3,
                d.font_space_bottom,
            ),
            font_scaling: crate::schema::on_off(ini.get("Tera Term", "FontScaling"), false),
            mouse_tracking: crate::schema::on_off(ini.get("Tera Term", "MouseEventTracking"), true),
            mouse_ctrl_disables_tracking: crate::schema::on_off(
                ini.get("Tera Term", "DisableMouseTrackingByCtrl"),
                true,
            ),
            mouse_wheel_to_cursor: crate::schema::on_off(
                ini.get("Tera Term", "TranslateWheelToCursor"),
                true,
            ),
            mouse_ctrl_disables_wheel_to_cursor: crate::schema::on_off(
                ini.get("Tera Term", "DisableWheelToCursorByCtrl"),
                true,
            ),
            mouse_wheel_scroll_line: ini.get_int(
                "Tera Term",
                "MouseWheelScrollLine",
                d.mouse_wheel_scroll_line,
            ) as i32,
            mouse_clickable_url: crate::schema::on_off(
                ini.get("Tera Term", "EnableClickableUrl"),
                false,
            ),
            mouse_cursor: ini
                .get_or("Tera Term", "MouseCursor", &d.mouse_cursor)
                .to_string(),
            url_browser: ini
                .get_or("Tera Term", "ClickableUrlBrowser", &d.url_browser)
                .to_string(),
            url_browser_args: ini
                .get_or("Tera Term", "ClickableUrlBrowserArg", &d.url_browser_args)
                .to_string(),
            url_join_split: crate::schema::on_off(ini.get("Tera Term", "JoinSplitURL"), false),
            url_join_split_ignore_eol_char: ini
                .get_or(
                    "Tera Term",
                    "JoinSplitURLIgnoreEOLChar",
                    &d.url_join_split_ignore_eol_char,
                )
                .to_string(),
            debug_enabled: crate::schema::on_off(ini.get("Tera Term", "Debug"), false),
            debug_modes: ini
                .get_or("Tera Term", "DebugModes", &d.debug_modes)
                .to_string(),
            bell_mode: match ini.get("Tera Term", "Beep") {
                Some(v) => BellMode::from_ini(v),
                None => d.bell_mode,
            },
            bell_on_connect: crate::schema::on_off(ini.get("Tera Term", "BeepOnConnect"), false),
            bell_visual_wait_ms: crate::schema::floored(
                ini.get_int("Tera Term", "BeepVBellWait", d.bell_visual_wait_ms) as i32,
                1,
            ),
            bell_over_used_count: ini.get_int(
                "Tera Term",
                "BeepOverUsedCount",
                d.bell_over_used_count,
            ) as i32,
            bell_over_used_time: ini.get_int("Tera Term", "BeepOverUsedTime", d.bell_over_used_time)
                as i32,
            bell_suppress_time: ini.get_int("Tera Term", "BeepSuppressTime", d.bell_suppress_time)
                as i32,
            bell_notify_sound: crate::schema::on_off(ini.get("Tera Term", "NotifySound"), true),
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
                crate::schema::nth_int_zero(
                    ini.get("Tera Term", "PasteDialogSize"),
                    0,
                    d.clipboard_paste_dialog_width,
                ),
                d.clipboard_paste_dialog_width,
                0,
                2147483647,
            ),
            clipboard_paste_dialog_height: crate::schema::ranged(
                crate::schema::nth_int_zero(
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
            clipboard_remote_access: match ini.get("Tera Term", "ClipboardAccessFromRemote") {
                Some(v) => ClipboardRemoteAccess::from_ini(v),
                None => d.clipboard_remote_access,
            },
            clipboard_remote_notify: crate::schema::on_off(
                ini.get("Tera Term", "NotifyClipboardAccess"),
                true,
            ),
            connection_port_type: match ini.get("Tera Term", "Port") {
                Some(v) => ConnectionPortType::from_ini(v),
                None => d.connection_port_type,
            },
            connection_tcp_port: crate::schema::word(ini.get_int(
                "Tera Term",
                "TCPPort",
                d.connection_tcp_port,
            ) as i32),
            connection_telnet: crate::schema::on_off(ini.get("Tera Term", "Telnet"), true),
            connection_telnet_port: crate::schema::word(ini.get_int(
                "Tera Term",
                "TelPort",
                d.connection_telnet_port,
            ) as i32),
            connection_telnet_binary: crate::schema::on_off(ini.get("Tera Term", "TelBin"), false),
            connection_telnet_auto_detect: crate::schema::on_off(
                ini.get("Tera Term", "TelAutoDetect"),
                true,
            ),
            connection_telnet_echo: crate::schema::on_off(ini.get("Tera Term", "TelEcho"), false),
            connection_telnet_log: crate::schema::on_off(ini.get("Tera Term", "TelLog"), false),
            connection_telnet_keepalive: crate::schema::word(ini.get_int(
                "Tera Term",
                "TelKeepAliveInterval",
                d.connection_telnet_keepalive,
            ) as i32),
            connection_tcp_local_echo: crate::schema::on_off(
                ini.get("Tera Term", "TCPLocalEcho"),
                false,
            ),
            connection_tcp_cr_send: match ini.get("Tera Term", "TCPCRSend") {
                Some(v) => ConnectionTcpCrSend::from_ini(v),
                None => d.connection_tcp_cr_send,
            },
            connection_confirm_disconnect: crate::schema::on_off(
                ini.get("Tera Term", "ConfirmDisconnect"),
                true,
            ),
            connection_history_list: crate::schema::on_off(
                ini.get("Tera Term", "HistoryList"),
                false,
            ),
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
            connection_clear_screen_on_close: crate::schema::on_off(
                ini.get("Tera Term", "ClearScreenOnCloseConnection"),
                false,
            ),
            connection_timeout: ini.get_int("Tera Term", "ConnectingTimeout", d.connection_timeout)
                as i32,
            connection_host_dialog_on_startup: crate::schema::on_off(
                ini.get("Tera Term", "HostDialogOnStartup"),
                true,
            ),
            connection_line_mode: crate::schema::on_off(
                ini.get("Tera Term", "EnableLineMode"),
                true,
            ),
            proxy_type: match ini.get("TTProxy", "ProxyType") {
                Some(v) => ProxyType::from_ini(v),
                None => d.proxy_type,
            },
            proxy_host: match ini.get("TTProxy", "ProxyHost") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_host.clone(),
            },
            proxy_port: crate::schema::word(
                ini.get_int("TTProxy", "ProxyPort", d.proxy_port) as i32
            ),
            proxy_user: match ini.get("TTProxy", "ProxyUser") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_user.clone(),
            },
            proxy_pass: match ini.get("TTProxy", "ProxyPass") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_pass.clone(),
            },
            proxy_timeout: crate::schema::ranged(
                ini.get_int("TTProxy", "ConnectionTimeout", d.proxy_timeout) as i32,
                d.proxy_timeout,
                0,
                2147483647,
            ),
            proxy_socks_resolve: match ini.get("TTProxy", "SocksResolve") {
                Some(v) => ProxySocksResolve::from_ini(v),
                None => d.proxy_socks_resolve,
            },
            proxy_telnet_hostname_prompt: match ini.get("TTProxy", "TelnetHostnamePrompt") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_telnet_hostname_prompt.clone(),
            },
            proxy_telnet_username_prompt: match ini.get("TTProxy", "TelnetUsernamePrompt") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_telnet_username_prompt.clone(),
            },
            proxy_telnet_password_prompt: match ini.get("TTProxy", "TelnetPasswordPrompt") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_telnet_password_prompt.clone(),
            },
            proxy_telnet_connected_message: match ini.get("TTProxy", "TelnetConnectedMessage") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_telnet_connected_message.clone(),
            },
            proxy_telnet_error_message: match ini.get("TTProxy", "TelnetErrorMessage") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_telnet_error_message.clone(),
            },
            proxy_debug_log: match ini.get("TTProxy", "DebugLog") {
                Some(v) => crate::esc::unescape(v),
                None => d.proxy_debug_log.clone(),
            },
            macro_startup_file: ini
                .get_or("Tera Term", "StartupMacro", &d.macro_startup_file)
                .to_string(),
            settings_source_version: ini
                .get_or("Tera Term", "Version", &d.settings_source_version)
                .to_string(),
            settings_language_file: ini
                .get_or("Tera Term", "UILanguageFile", &d.settings_language_file)
                .to_string(),
            settings_auto_backup: crate::schema::on_off(
                ini.get("Tera Term", "IniAutoBackup"),
                true,
            ),
            serial_com_port: crate::schema::word(ini.get_int(
                "Tera Term",
                "ComPort",
                d.serial_com_port,
            ) as i32),
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
            serial_delay_per_char: crate::schema::word(ini.get_int(
                "Tera Term",
                "DelayPerChar",
                d.serial_delay_per_char,
            ) as i32),
            serial_delay_per_line: crate::schema::word(ini.get_int(
                "Tera Term",
                "DelayPerLine",
                d.serial_delay_per_line,
            ) as i32),
            serial_wait_com: crate::schema::on_off(ini.get("Tera Term", "WaitCom"), false),
            serial_max_com_port: crate::schema::clamped(
                crate::schema::word(
                    ini.get_int("Tera Term", "MaxComPort", d.serial_max_com_port) as i32,
                ),
                4,
                4096,
            ),
            serial_rts: ini.get_int("Tera Term", "FlowCtrlRTS", d.serial_rts) as i32,
            serial_dtr: ini.get_int("Tera Term", "FlowCtrlDTR", d.serial_dtr) as i32,
            serial_clear_buffer_on_open: crate::schema::on_off(
                ini.get("Tera Term", "ClearComBuffOnOpen"),
                true,
            ),
            serial_break_time: ini.get_int("Tera Term", "SendBreakTime", d.serial_break_time)
                as i32,
            serial_auto_reconnect: crate::schema::on_off(
                ini.get("Tera Term", "AutoComPortReconnect"),
                true,
            ),
            serial_auto_reconnect_delay: crate::schema::word(ini.get_int(
                "Tera Term",
                "AutoComPortReconnectDelayNormal",
                d.serial_auto_reconnect_delay,
            ) as i32),
            serial_auto_reconnect_delay_unknown_port: crate::schema::word(ini.get_int(
                "Tera Term",
                "AutoComPortReconnectDelayIllegal",
                d.serial_auto_reconnect_delay_unknown_port,
            ) as i32),
            serial_auto_reconnect_retry_interval: crate::schema::word(ini.get_int(
                "Tera Term",
                "AutoComPortReconnectRetryInterval",
                d.serial_auto_reconnect_retry_interval,
            ) as i32),
            serial_auto_reconnect_retries: crate::schema::word(ini.get_int(
                "Tera Term",
                "AutoComPortReconnectRetryCount",
                d.serial_auto_reconnect_retries,
            ) as i32),
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
            log_rotate_size_type: crate::schema::word(ini.get_int(
                "Tera Term",
                "LogRotateSizeType",
                d.log_rotate_size_type,
            ) as i32),
            log_rotate_step: crate::schema::word(ini.get_int(
                "Tera Term",
                "LogRotateStep",
                d.log_rotate_step,
            ) as i32),
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
            transfer_send_filter: ini
                .get_or("Tera Term", "FileSendFilter", &d.transfer_send_filter)
                .to_string(),
            transfer_receive_filter: ini
                .get_or("Tera Term", "FileReceiveFilter", &d.transfer_receive_filter)
                .to_string(),
            transfer_scp_send_dir: ini
                .get_or("Tera Term", "ScpSendDir", &d.transfer_scp_send_dir)
                .to_string(),
            transfer_raw_high_speed: crate::schema::on_off(
                ini.get("Tera Term", "FileSendHighSpeedMode"),
                true,
            ),
            transfer_confirm_file_drop: crate::schema::on_off(
                ini.get("Tera Term", "ConfirmFileDragAndDrop"),
                true,
            ),
            transfer_raw_send_delay_type: match ini.get("Tera Term", "SendfileDelayType") {
                Some(v) => TransferRawSendDelayType::from_ini(v),
                None => d.transfer_raw_send_delay_type,
            },
            transfer_raw_send_delay_tick: crate::schema::word(ini.get_int(
                "Tera Term",
                "SendfileDelayTick",
                d.transfer_raw_send_delay_tick,
            ) as i32),
            transfer_raw_send_size: ini.get_int(
                "Tera Term",
                "SendfileSize",
                d.transfer_raw_send_size,
            ) as i32,
            transfer_raw_send_sequential: crate::schema::on_off(
                ini.get("Tera Term", "SendfileSequential"),
                false,
            ),
            transfer_raw_send_skip_dialog: crate::schema::on_off(
                ini.get("Tera Term", "SendfileSkipOptionDialog"),
                false,
            ),
            transfer_raw_receive_skip_dialog: crate::schema::on_off(
                ini.get("Tera Term", "ReceivefileSkipOptionDialog"),
                false,
            ),
            printer_passthrough_delay: crate::schema::word(ini.get_int(
                "Tera Term",
                "PassThruDelay",
                d.printer_passthrough_delay,
            ) as i32),
            printer_passthrough_port: ini
                .get_or("Tera Term", "PassThruPort", &d.printer_passthrough_port)
                .to_string(),
            printer_control_sequences: crate::schema::on_off(
                ini.get("Tera Term", "PrinterCtrlSequence"),
                false,
            ),
            printer_margin_left: crate::schema::nth_int_zero(
                ini.get("Tera Term", "PrnMargin"),
                0,
                d.printer_margin_left,
            ),
            printer_margin_right: crate::schema::nth_int_zero(
                ini.get("Tera Term", "PrnMargin"),
                1,
                d.printer_margin_right,
            ),
            printer_margin_top: crate::schema::nth_int_zero(
                ini.get("Tera Term", "PrnMargin"),
                2,
                d.printer_margin_top,
            ),
            printer_margin_bottom: crate::schema::nth_int_zero(
                ini.get("Tera Term", "PrnMargin"),
                3,
                d.printer_margin_bottom,
            ),
            printer_convert_form_feed: crate::schema::on_off(
                ini.get("Tera Term", "PrnConvFF"),
                false,
            ),
            printer_vt_ppi_x: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTPPI"),
                0,
                d.printer_vt_ppi_x,
            ),
            printer_vt_ppi_y: crate::schema::nth_int_zero(
                ini.get("Tera Term", "VTPPI"),
                1,
                d.printer_vt_ppi_y,
            ),
            tek_x: crate::schema::nth_int_zero(ini.get("Tera Term", "TEKPos"), 0, d.tek_x),
            tek_y: crate::schema::nth_int_zero(ini.get("Tera Term", "TEKPos"), 1, d.tek_y),
            tek_color: crate::schema::color2(ini.get("Tera Term", "TEKColor"), d.tek_color),
            tek_color_emulation: crate::schema::on_off(
                ini.get("Tera Term", "TEKColorEmulation"),
                false,
            ),
            tek_gin_mouse_code: ini.get_int("Tera Term", "TEKGINMouseCode", d.tek_gin_mouse_code)
                as i32,
            tek_icon: match ini.get("Tera Term", "TEKIcon") {
                Some(v) => TekIcon::from_ini(v),
                None => d.tek_icon,
            },
            tek_ppi_x: crate::schema::nth_int_zero(ini.get("Tera Term", "TEKPPI"), 0, d.tek_ppi_x),
            tek_ppi_y: crate::schema::nth_int_zero(ini.get("Tera Term", "TEKPPI"), 1, d.tek_ppi_y),
            window_maximized_bug_tweak: crate::schema::word(crate::schema::int_alias(
                ini.get("Tera Term", "MaximizedBugTweak"),
                d.window_maximized_bug_tweak,
                "on",
                2,
            )),
            window_icon: match ini.get("Tera Term", "VTIcon") {
                Some(v) => WindowIcon::from_ini(v),
                None => d.window_icon,
            },
            window_hide_title: crate::schema::on_off(ini.get("Tera Term", "HideTitle"), false),
            window_popup_menu: crate::schema::on_off(ini.get("Tera Term", "PopupMenu"), false),
            window_popup_menu_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnablePopupMenu"),
                true,
            ),
            window_show_menu_enabled: crate::schema::on_off(
                ini.get("Tera Term", "EnableShowMenu"),
                true,
            ),
            window_window_menu: crate::schema::on_off(ini.get("Tera Term", "WindowMenu"), true),
            window_save_position: crate::schema::on_off(
                ini.get("Tera Term", "SaveVTWinPos"),
                false,
            ),
            window_x: crate::schema::nth_int_zero(ini.get("Tera Term", "VTPos"), 0, d.window_x),
            window_y: crate::schema::nth_int_zero(ini.get("Tera Term", "VTPos"), 1, d.window_y),
            window_auto_vt_tek_switch: crate::schema::on_off(
                ini.get("Tera Term", "AutoWinSwitch"),
                false,
            ),
            connection_cygwin_directory: ini
                .get_or(
                    "Tera Term",
                    "CygwinDirectory ",
                    &d.connection_cygwin_directory,
                )
                .to_string(),
            log_viewer: ini
                .get_or("Tera Term", "ViewlogEditor", &d.log_viewer)
                .to_string(),
            log_viewer_args: ini
                .get_or("Tera Term", "ViewlogEditorArg", &d.log_viewer_args)
                .to_string(),
            broadcast_command_history: crate::schema::on_off(
                ini.get("Tera Term", "BroadcastCommandHistory"),
                false,
            ),
            broadcast_accept: crate::schema::on_off(ini.get("Tera Term", "AcceptBroadcast"), true),
            broadcast_submit_key: match ini.get("Tera Term", "BroadcastSubmitKey") {
                Some(v) => BroadcastSubmitKey::from_ini(v),
                None => d.broadcast_submit_key,
            },
            broadcast_max_history: crate::schema::word(ini.get_int(
                "Tera Term",
                "MaxBroadcatHistory",
                d.broadcast_max_history,
            ) as i32),
            menu_disable_accelerator_send_break: crate::schema::on_off(
                ini.get("Tera Term", "DisableAcceleratorSendBreak"),
                false,
            ),
            menu_disable_send_break: crate::schema::on_off(
                ini.get("Tera Term", "DisableMenuSendBreak"),
                false,
            ),
            menu_disable_accelerator_duplicate: crate::schema::on_off(
                ini.get("Tera Term", "DisableAcceleratorDuplicateSession"),
                false,
            ),
            menu_accelerator_new_connection: crate::schema::on_off(
                ini.get("Tera Term", "AcceleratorNewConnection"),
                true,
            ),
            menu_accelerator_local_shell: crate::schema::on_off(
                ini.get("Tera Term", "AcceleratorCygwinConnection"),
                true,
            ),
            menu_disable_duplicate: crate::schema::on_off(
                ini.get("Tera Term", "DisableMenuDuplicateSession"),
                false,
            ),
            menu_disable_new_connection: crate::schema::on_off(
                ini.get("Tera Term", "DisableMenuNewConnection"),
                false,
            ),
            macro_wait4all: crate::schema::on_off(
                ini.get("Tera Term", "Wait4allMacroCommand"),
                false,
            ),
            window_jump_list: crate::schema::on_off(ini.get("Tera Term", "JumpList"), true),
            window_corner_dontround: crate::schema::on_off(
                ini.get("Tera Term", "WindowCornerDontround"),
                false,
            ),
            window_toolbar: crate::schema::on_off(ini.get("Sterna", "Toolbar"), true),
            window_panel_layout: match ini.get("Sterna", "PanelLayout") {
                Some(v) => WindowPanelLayout::from_ini(v),
                None => d.window_panel_layout,
            },
            window_quick_buttons: crate::schema::on_off(ini.get("Sterna", "QuickButtons"), true),
            window_quick_buttons_area: match ini.get("Sterna", "QuickButtonsArea") {
                Some(v) => WindowQuickButtonsArea::from_ini(v),
                None => d.window_quick_buttons_area,
            },
            recent_serial_port: ini
                .get_or("Sterna", "SerialPort", &d.recent_serial_port)
                .to_string(),
            recent_ssh_host: ini
                .get_or("Sterna", "SshHost", &d.recent_ssh_host)
                .to_string(),
            recent_ssh_user: ini
                .get_or("Sterna", "SshUser", &d.recent_ssh_user)
                .to_string(),
            recent_ssh_port: crate::schema::word(
                ini.get_int("Sterna", "SshPort", d.recent_ssh_port) as i32,
            ),
            recent_ssh_identity: ini
                .get_or("Sterna", "SshIdentity", &d.recent_ssh_identity)
                .to_string(),
            recent_ssh_legacy: crate::schema::on_off(ini.get("Sterna", "SshLegacy"), false),
            recent_telnet_host: ini
                .get_or("Sterna", "TelnetHost", &d.recent_telnet_host)
                .to_string(),
            recent_telnet_port: crate::schema::word(ini.get_int(
                "Sterna",
                "TelnetPort",
                d.recent_telnet_port,
            ) as i32),
            recent_telnet_mode: match ini.get("Sterna", "TelnetMode") {
                Some(v) => RecentTelnetMode::from_ini(v),
                None => d.recent_telnet_mode,
            },
            updates_check_on_startup: crate::schema::on_off(
                ini.get("Sterna", "CheckUpdatesOnStartup"),
                true,
            ),
            updates_last_check: ini
                .get_or("Sterna", "LastUpdateCheck", &d.updates_last_check)
                .to_string(),
        };
        settings.window_opacity_active = crate::schema::byte(ini.get_int(
            "Tera Term",
            "AlphaBlendActive",
            settings.window_opacity_inactive,
        ) as i32);
        settings.normalize();
        settings
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
            "ClearOnResize",
            &if self.terminal_clear_on_resize {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ScrollWindowClearScreen",
            &if self.terminal_home_erase_clears_screen {
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
        ini.set("Tera Term", "Answerback", &self.terminal_answerback.clone());
        ini.set(
            "Tera Term",
            "BackWrap",
            &if self.terminal_back_wrap { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "VTCompatTab",
            &if self.terminal_vt_compat_tab {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TabStopModifySequence",
            &self.terminal_tab_stop_modify.clone(),
        );
        ini.set(
            "Tera Term",
            "UseInvalidDECRQSSResponse",
            &if self.terminal_invalid_decrqss {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set("Tera Term", "TerminalUID", &self.terminal_uid.clone());
        ini.set(
            "Tera Term",
            "LockTUID",
            &if self.terminal_lock_uid { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoInvoke",
            &if self.terminal_auto_invoke {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "MaxOSCBufferSize",
            &self.terminal_max_osc_buffer.to_string(),
        );
        ini.set(
            "Tera Term",
            "AllowWrongSequence",
            &if self.terminal_allow_wrong_sequence {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableStatusLine",
            &if self.terminal_status_line_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "KanjiReceive",
            &self.encoding_receive.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "KanjiSend",
            &self.encoding_send.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "KatakanaReceive",
            &self.encoding_katakana_receive.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "KatakanaSend",
            &self.encoding_katakana_send.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "KanjiIn",
            &self.encoding_kanji_in.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "KanjiOut",
            &self.encoding_kanji_out.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "CtrlInKanji",
            &if self.encoding_ctrl_in_kanji {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "FixedJIS",
            &if self.encoding_fixed_jis { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "FallbackToCP932",
            &if self.encoding_fallback_cp932 {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "RussKeyb",
            &self.encoding_russian_keyboard.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "DecSpMappingDir",
            &self.encoding_dec_special_direction.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "UnicodeToDecSpMapping",
            &self.encoding_unicode_to_dec_special.to_string(),
        );
        ini.set(
            "Tera Term",
            "UnicodeAmbiguousWidth",
            &self.encoding_ambiguous_width.to_string(),
        );
        ini.set(
            "Tera Term",
            "UnicodeEmojiOverride",
            &if self.encoding_emoji_override {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "UnicodeEmojiWidth",
            &self.encoding_emoji_width.to_string(),
        );
        ini.set(
            "Tera Term",
            "IME",
            &if self.ime_enabled { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "IMEInline",
            &if self.ime_inline { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "IMERelatedCursor",
            &if self.ime_cursor_related { "on" } else { "off" }.to_string(),
        );
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
            "DelimDBCS",
            &if self.keyboard_width_delimits_word {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "StrictKeyMapping",
            &if self.keyboard_strict_mapping {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Meta8Bit",
            &self.keyboard_meta_8bit.as_ini().to_string(),
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
            "URLColor",
            &crate::schema::color2_str(&self.color_url),
        );
        ini.set(
            "Tera Term",
            "EnableURLColor",
            &if self.color_url_enabled { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "URLUnderline",
            &if self.color_url_underline {
                "on"
            } else {
                "off"
            }
            .to_string(),
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
        ini.set("Tera Term", "ANSIColor", &self.color_ansi_palette.clone());
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
            "EnableBold",
            &if self.color_bold_font { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "UnderlineAttrFont",
            &if self.color_underline_font {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "UseTextColor",
            &if self.color_use_text_color {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "UseNormalBGColor",
            &if self.color_use_normal_background {
                "on"
            } else {
                "off"
            }
            .to_string(),
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
            "KillFocusCursor",
            &if self.cursor_show_unfocused {
                "on"
            } else {
                "off"
            }
            .to_string(),
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
            "AutoScrollOnlyInBottomLine",
            &if self.window_auto_scroll_only_at_bottom {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ScrollThreshold",
            &self.window_scroll_threshold.to_string(),
        );
        ini.set(
            "Tera Term",
            "AlphaBlend",
            &self.window_opacity_inactive.to_string(),
        );
        ini.set(
            "Tera Term",
            "AlphaBlendActive",
            &self.window_opacity_active.to_string(),
        );
        ini.set(
            "Tera Term",
            "TitleFormat",
            &self.window_title_format.to_string(),
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
            "FontQuality",
            &self.font_quality.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "DrawingResizedFont",
            &if self.font_draw_resized { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "VTDrawAPI",
            &self.font_draw_api.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "VTDrawACP",
            &self.font_draw_ansi_code_page.to_string(),
        );
        ini.set(
            "Tera Term",
            "VTFontSpace",
            &crate::schema::with_nth(ini.get("Tera Term", "VTFontSpace"), 0, self.font_space_left),
        );
        ini.set(
            "Tera Term",
            "VTFontSpace",
            &crate::schema::with_nth(
                ini.get("Tera Term", "VTFontSpace"),
                1,
                self.font_space_right,
            ),
        );
        ini.set(
            "Tera Term",
            "VTFontSpace",
            &crate::schema::with_nth(ini.get("Tera Term", "VTFontSpace"), 2, self.font_space_top),
        );
        ini.set(
            "Tera Term",
            "VTFontSpace",
            &crate::schema::with_nth(
                ini.get("Tera Term", "VTFontSpace"),
                3,
                self.font_space_bottom,
            ),
        );
        ini.set(
            "Tera Term",
            "FontScaling",
            &if self.font_scaling { "on" } else { "off" }.to_string(),
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
            "DisableWheelToCursorByCtrl",
            &if self.mouse_ctrl_disables_wheel_to_cursor {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "MouseWheelScrollLine",
            &self.mouse_wheel_scroll_line.to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableClickableUrl",
            &if self.mouse_clickable_url {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set("Tera Term", "MouseCursor", &self.mouse_cursor.clone());
        ini.set(
            "Tera Term",
            "ClickableUrlBrowser",
            &self.url_browser.clone(),
        );
        ini.set(
            "Tera Term",
            "ClickableUrlBrowserArg",
            &self.url_browser_args.clone(),
        );
        ini.set(
            "Tera Term",
            "JoinSplitURL",
            &if self.url_join_split { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "JoinSplitURLIgnoreEOLChar",
            &self.url_join_split_ignore_eol_char.clone(),
        );
        ini.set(
            "Tera Term",
            "Debug",
            &if self.debug_enabled { "on" } else { "off" }.to_string(),
        );
        ini.set("Tera Term", "DebugModes", &self.debug_modes.clone());
        ini.set("Tera Term", "Beep", &self.bell_mode.as_ini().to_string());
        ini.set(
            "Tera Term",
            "BeepOnConnect",
            &if self.bell_on_connect { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "BeepVBellWait",
            &self.bell_visual_wait_ms.to_string(),
        );
        ini.set(
            "Tera Term",
            "BeepOverUsedCount",
            &self.bell_over_used_count.to_string(),
        );
        ini.set(
            "Tera Term",
            "BeepOverUsedTime",
            &self.bell_over_used_time.to_string(),
        );
        ini.set(
            "Tera Term",
            "BeepSuppressTime",
            &self.bell_suppress_time.to_string(),
        );
        ini.set(
            "Tera Term",
            "NotifySound",
            &if self.bell_notify_sound { "on" } else { "off" }.to_string(),
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
            "ClipboardAccessFromRemote",
            &self.clipboard_remote_access.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "NotifyClipboardAccess",
            &if self.clipboard_remote_notify {
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
            "TelAutoDetect",
            &if self.connection_telnet_auto_detect {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TelEcho",
            &if self.connection_telnet_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TelLog",
            &if self.connection_telnet_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TelKeepAliveInterval",
            &self.connection_telnet_keepalive.to_string(),
        );
        ini.set(
            "Tera Term",
            "TCPLocalEcho",
            &if self.connection_tcp_local_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TCPCRSend",
            &self.connection_tcp_cr_send.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmDisconnect",
            &if self.connection_confirm_disconnect {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "HistoryList",
            &if self.connection_history_list {
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
            "ClearScreenOnCloseConnection",
            &if self.connection_clear_screen_on_close {
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
        ini.set(
            "Tera Term",
            "EnableLineMode",
            &if self.connection_line_mode {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "TTProxy",
            "ProxyType",
            &self.proxy_type.as_ini().to_string(),
        );
        ini.set("TTProxy", "ProxyHost", &crate::esc::quote(&self.proxy_host));
        ini.set("TTProxy", "ProxyPort", &self.proxy_port.to_string());
        ini.set("TTProxy", "ProxyUser", &crate::esc::quote(&self.proxy_user));
        ini.set("TTProxy", "ProxyPass", &crate::esc::quote(&self.proxy_pass));
        ini.set(
            "TTProxy",
            "ConnectionTimeout",
            &self.proxy_timeout.to_string(),
        );
        ini.set(
            "TTProxy",
            "SocksResolve",
            &self.proxy_socks_resolve.as_ini().to_string(),
        );
        ini.set(
            "TTProxy",
            "TelnetHostnamePrompt",
            &crate::esc::quote(&self.proxy_telnet_hostname_prompt),
        );
        ini.set(
            "TTProxy",
            "TelnetUsernamePrompt",
            &crate::esc::quote(&self.proxy_telnet_username_prompt),
        );
        ini.set(
            "TTProxy",
            "TelnetPasswordPrompt",
            &crate::esc::quote(&self.proxy_telnet_password_prompt),
        );
        ini.set(
            "TTProxy",
            "TelnetConnectedMessage",
            &crate::esc::quote(&self.proxy_telnet_connected_message),
        );
        ini.set(
            "TTProxy",
            "TelnetErrorMessage",
            &crate::esc::quote(&self.proxy_telnet_error_message),
        );
        ini.set(
            "TTProxy",
            "DebugLog",
            &crate::esc::quote(&self.proxy_debug_log),
        );
        ini.set(
            "Tera Term",
            "StartupMacro",
            &self.macro_startup_file.clone(),
        );
        ini.set(
            "Tera Term",
            "Version",
            &self.settings_source_version.clone(),
        );
        ini.set(
            "Tera Term",
            "UILanguageFile",
            &self.settings_language_file.clone(),
        );
        ini.set(
            "Tera Term",
            "IniAutoBackup",
            &if self.settings_auto_backup {
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
        ini.set("Tera Term", "FlowCtrlRTS", &self.serial_rts.to_string());
        ini.set("Tera Term", "FlowCtrlDTR", &self.serial_dtr.to_string());
        ini.set(
            "Tera Term",
            "ClearComBuffOnOpen",
            &if self.serial_clear_buffer_on_open {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "SendBreakTime",
            &self.serial_break_time.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoComPortReconnect",
            &if self.serial_auto_reconnect {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoComPortReconnectDelayNormal",
            &self.serial_auto_reconnect_delay.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoComPortReconnectDelayIllegal",
            &self.serial_auto_reconnect_delay_unknown_port.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoComPortReconnectRetryInterval",
            &self.serial_auto_reconnect_retry_interval.to_string(),
        );
        ini.set(
            "Tera Term",
            "AutoComPortReconnectRetryCount",
            &self.serial_auto_reconnect_retries.to_string(),
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
            "FileSendFilter",
            &self.transfer_send_filter.clone(),
        );
        ini.set(
            "Tera Term",
            "FileReceiveFilter",
            &self.transfer_receive_filter.clone(),
        );
        ini.set(
            "Tera Term",
            "ScpSendDir",
            &self.transfer_scp_send_dir.clone(),
        );
        ini.set(
            "Tera Term",
            "FileSendHighSpeedMode",
            &if self.transfer_raw_high_speed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ConfirmFileDragAndDrop",
            &if self.transfer_confirm_file_drop {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "SendfileDelayType",
            &self.transfer_raw_send_delay_type.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "SendfileDelayTick",
            &self.transfer_raw_send_delay_tick.to_string(),
        );
        ini.set(
            "Tera Term",
            "SendfileSize",
            &self.transfer_raw_send_size.to_string(),
        );
        ini.set(
            "Tera Term",
            "SendfileSequential",
            &if self.transfer_raw_send_sequential {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "SendfileSkipOptionDialog",
            &if self.transfer_raw_send_skip_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "ReceivefileSkipOptionDialog",
            &if self.transfer_raw_receive_skip_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "PassThruDelay",
            &self.printer_passthrough_delay.to_string(),
        );
        ini.set(
            "Tera Term",
            "PassThruPort",
            &self.printer_passthrough_port.clone(),
        );
        ini.set(
            "Tera Term",
            "PrinterCtrlSequence",
            &if self.printer_control_sequences {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "PrnMargin",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PrnMargin"),
                0,
                self.printer_margin_left,
            ),
        );
        ini.set(
            "Tera Term",
            "PrnMargin",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PrnMargin"),
                1,
                self.printer_margin_right,
            ),
        );
        ini.set(
            "Tera Term",
            "PrnMargin",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PrnMargin"),
                2,
                self.printer_margin_top,
            ),
        );
        ini.set(
            "Tera Term",
            "PrnMargin",
            &crate::schema::with_nth(
                ini.get("Tera Term", "PrnMargin"),
                3,
                self.printer_margin_bottom,
            ),
        );
        ini.set(
            "Tera Term",
            "PrnConvFF",
            &if self.printer_convert_form_feed {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "VTPPI",
            &crate::schema::with_nth(ini.get("Tera Term", "VTPPI"), 0, self.printer_vt_ppi_x),
        );
        ini.set(
            "Tera Term",
            "VTPPI",
            &crate::schema::with_nth(ini.get("Tera Term", "VTPPI"), 1, self.printer_vt_ppi_y),
        );
        ini.set(
            "Tera Term",
            "TEKPos",
            &crate::schema::with_nth(ini.get("Tera Term", "TEKPos"), 0, self.tek_x),
        );
        ini.set(
            "Tera Term",
            "TEKPos",
            &crate::schema::with_nth(ini.get("Tera Term", "TEKPos"), 1, self.tek_y),
        );
        ini.set(
            "Tera Term",
            "TEKColor",
            &crate::schema::color2_str(&self.tek_color),
        );
        ini.set(
            "Tera Term",
            "TEKColorEmulation",
            &if self.tek_color_emulation {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "TEKGINMouseCode",
            &self.tek_gin_mouse_code.to_string(),
        );
        ini.set("Tera Term", "TEKIcon", &self.tek_icon.as_ini().to_string());
        ini.set(
            "Tera Term",
            "TEKPPI",
            &crate::schema::with_nth(ini.get("Tera Term", "TEKPPI"), 0, self.tek_ppi_x),
        );
        ini.set(
            "Tera Term",
            "TEKPPI",
            &crate::schema::with_nth(ini.get("Tera Term", "TEKPPI"), 1, self.tek_ppi_y),
        );
        ini.set(
            "Tera Term",
            "MaximizedBugTweak",
            &self.window_maximized_bug_tweak.to_string(),
        );
        ini.set(
            "Tera Term",
            "VTIcon",
            &self.window_icon.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "HideTitle",
            &if self.window_hide_title { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "PopupMenu",
            &if self.window_popup_menu { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "EnablePopupMenu",
            &if self.window_popup_menu_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "EnableShowMenu",
            &if self.window_show_menu_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "WindowMenu",
            &if self.window_window_menu { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "SaveVTWinPos",
            &if self.window_save_position {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        if self.window_save_position {
            ini.set(
                "Tera Term",
                "VTPos",
                &crate::schema::with_nth(ini.get("Tera Term", "VTPos"), 0, self.window_x),
            );
        }
        if self.window_save_position {
            ini.set(
                "Tera Term",
                "VTPos",
                &crate::schema::with_nth(ini.get("Tera Term", "VTPos"), 1, self.window_y),
            );
        }
        ini.set(
            "Tera Term",
            "AutoWinSwitch",
            &if self.window_auto_vt_tek_switch {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "CygwinDirectory",
            &self.connection_cygwin_directory.clone(),
        );
        ini.set("Tera Term", "ViewlogEditor", &self.log_viewer.clone());
        ini.set(
            "Tera Term",
            "ViewlogEditorArg",
            &self.log_viewer_args.clone(),
        );
        ini.set(
            "Tera Term",
            "BroadcastCommandHistory",
            &if self.broadcast_command_history {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AcceptBroadcast",
            &if self.broadcast_accept { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "BroadcastSubmitKey",
            &self.broadcast_submit_key.as_ini().to_string(),
        );
        ini.set(
            "Tera Term",
            "MaxBroadcatHistory",
            &self.broadcast_max_history.to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableAcceleratorSendBreak",
            &if self.menu_disable_accelerator_send_break {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableMenuSendBreak",
            &if self.menu_disable_send_break {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableAcceleratorDuplicateSession",
            &if self.menu_disable_accelerator_duplicate {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AcceleratorNewConnection",
            &if self.menu_accelerator_new_connection {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "AcceleratorCygwinConnection",
            &if self.menu_accelerator_local_shell {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableMenuDuplicateSession",
            &if self.menu_disable_duplicate {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "DisableMenuNewConnection",
            &if self.menu_disable_new_connection {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Tera Term",
            "Wait4allMacroCommand",
            &if self.macro_wait4all { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "JumpList",
            &if self.window_jump_list { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Tera Term",
            "WindowCornerDontround",
            &if self.window_corner_dontround {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Sterna",
            "Toolbar",
            &if self.window_toolbar { "on" } else { "off" }.to_string(),
        );
        ini.set(
            "Sterna",
            "PanelLayout",
            &self.window_panel_layout.as_ini().to_string(),
        );
        ini.set(
            "Sterna",
            "QuickButtons",
            &if self.window_quick_buttons {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Sterna",
            "QuickButtonsArea",
            &self.window_quick_buttons_area.as_ini().to_string(),
        );
        ini.set("Sterna", "SerialPort", &self.recent_serial_port.clone());
        ini.set("Sterna", "SshHost", &self.recent_ssh_host.clone());
        ini.set("Sterna", "SshUser", &self.recent_ssh_user.clone());
        ini.set("Sterna", "SshPort", &self.recent_ssh_port.to_string());
        ini.set("Sterna", "SshIdentity", &self.recent_ssh_identity.clone());
        ini.set(
            "Sterna",
            "SshLegacy",
            &if self.recent_ssh_legacy { "on" } else { "off" }.to_string(),
        );
        ini.set("Sterna", "TelnetHost", &self.recent_telnet_host.clone());
        ini.set("Sterna", "TelnetPort", &self.recent_telnet_port.to_string());
        ini.set(
            "Sterna",
            "TelnetMode",
            &self.recent_telnet_mode.as_ini().to_string(),
        );
        ini.set(
            "Sterna",
            "CheckUpdatesOnStartup",
            &if self.updates_check_on_startup {
                "on"
            } else {
                "off"
            }
            .to_string(),
        );
        ini.set(
            "Sterna",
            "LastUpdateCheck",
            &self.updates_last_check.clone(),
        );
    }

    /// Write one setting back by its dotted name, leaving every other
    /// line alone. False for a name that is not in the schema.
    ///
    /// This is what persists a change nobody asked to save — the
    /// remembered connection, the window position on close — without
    /// pinning every other default into a file the user may share with
    /// a real Tera Term. [`Settings::store`] is Save setup.
    pub fn store_one(&self, ini: &mut Ini, name: &str) -> bool {
        match name {
            "terminal.cols" => {
                ini.set(
                    "Tera Term",
                    "TerminalSize",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "TerminalSize"),
                        0,
                        self.terminal_cols,
                    ),
                );
            }
            "terminal.rows" => {
                ini.set(
                    "Tera Term",
                    "TerminalSize",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "TerminalSize"),
                        1,
                        self.terminal_rows,
                    ),
                );
            }
            "terminal.id" => {
                ini.set(
                    "Tera Term",
                    "TerminalID",
                    &self.terminal_id.as_ini().to_string(),
                );
            }
            "terminal.cr_receive" => {
                ini.set(
                    "Tera Term",
                    "CRReceive",
                    &self.terminal_cr_receive.as_ini().to_string(),
                );
            }
            "terminal.cr_send" => {
                ini.set(
                    "Tera Term",
                    "CRSend",
                    &self.terminal_cr_send.as_ini().to_string(),
                );
            }
            "terminal.local_echo" => {
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
            }
            "terminal.size_follows_window" => {
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
            }
            "terminal.auto_win_resize" => {
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
            }
            "terminal.clear_on_resize" => {
                ini.set(
                    "Tera Term",
                    "ClearOnResize",
                    &if self.terminal_clear_on_resize {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.home_erase_clears_screen" => {
                ini.set(
                    "Tera Term",
                    "ScrollWindowClearScreen",
                    &if self.terminal_home_erase_clears_screen {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.scrollback_enabled" => {
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
            }
            "terminal.scrollback_lines" => {
                ini.set(
                    "Tera Term",
                    "ScrollBuffSize",
                    &self.terminal_scrollback_lines.to_string(),
                );
            }
            "terminal.buffer_max_lines" => {
                ini.set(
                    "Tera Term",
                    "MaxBuffSize",
                    &self.terminal_buffer_max_lines.to_string(),
                );
            }
            "terminal.iso2022_shifts" => {
                ini.set(
                    "Tera Term",
                    "ISO2022ShiftFunction",
                    &self.terminal_iso2022_shifts.clone(),
                );
            }
            "terminal.title" => {
                ini.set("Tera Term", "Title", &self.terminal_title.clone());
            }
            "terminal.answerback" => {
                ini.set("Tera Term", "Answerback", &self.terminal_answerback.clone());
            }
            "terminal.back_wrap" => {
                ini.set(
                    "Tera Term",
                    "BackWrap",
                    &if self.terminal_back_wrap { "on" } else { "off" }.to_string(),
                );
            }
            "terminal.vt_compat_tab" => {
                ini.set(
                    "Tera Term",
                    "VTCompatTab",
                    &if self.terminal_vt_compat_tab {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.tab_stop_modify" => {
                ini.set(
                    "Tera Term",
                    "TabStopModifySequence",
                    &self.terminal_tab_stop_modify.clone(),
                );
            }
            "terminal.invalid_decrqss" => {
                ini.set(
                    "Tera Term",
                    "UseInvalidDECRQSSResponse",
                    &if self.terminal_invalid_decrqss {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.uid" => {
                ini.set("Tera Term", "TerminalUID", &self.terminal_uid.clone());
            }
            "terminal.lock_uid" => {
                ini.set(
                    "Tera Term",
                    "LockTUID",
                    &if self.terminal_lock_uid { "on" } else { "off" }.to_string(),
                );
            }
            "terminal.auto_invoke" => {
                ini.set(
                    "Tera Term",
                    "AutoInvoke",
                    &if self.terminal_auto_invoke {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.max_osc_buffer" => {
                ini.set(
                    "Tera Term",
                    "MaxOSCBufferSize",
                    &self.terminal_max_osc_buffer.to_string(),
                );
            }
            "terminal.allow_wrong_sequence" => {
                ini.set(
                    "Tera Term",
                    "AllowWrongSequence",
                    &if self.terminal_allow_wrong_sequence {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "terminal.status_line_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnableStatusLine",
                    &if self.terminal_status_line_enabled {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "encoding.receive" => {
                ini.set(
                    "Tera Term",
                    "KanjiReceive",
                    &self.encoding_receive.as_ini().to_string(),
                );
            }
            "encoding.send" => {
                ini.set(
                    "Tera Term",
                    "KanjiSend",
                    &self.encoding_send.as_ini().to_string(),
                );
            }
            "encoding.katakana_receive" => {
                ini.set(
                    "Tera Term",
                    "KatakanaReceive",
                    &self.encoding_katakana_receive.as_ini().to_string(),
                );
            }
            "encoding.katakana_send" => {
                ini.set(
                    "Tera Term",
                    "KatakanaSend",
                    &self.encoding_katakana_send.as_ini().to_string(),
                );
            }
            "encoding.kanji_in" => {
                ini.set(
                    "Tera Term",
                    "KanjiIn",
                    &self.encoding_kanji_in.as_ini().to_string(),
                );
            }
            "encoding.kanji_out" => {
                ini.set(
                    "Tera Term",
                    "KanjiOut",
                    &self.encoding_kanji_out.as_ini().to_string(),
                );
            }
            "encoding.ctrl_in_kanji" => {
                ini.set(
                    "Tera Term",
                    "CtrlInKanji",
                    &if self.encoding_ctrl_in_kanji {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "encoding.fixed_jis" => {
                ini.set(
                    "Tera Term",
                    "FixedJIS",
                    &if self.encoding_fixed_jis { "on" } else { "off" }.to_string(),
                );
            }
            "encoding.fallback_cp932" => {
                ini.set(
                    "Tera Term",
                    "FallbackToCP932",
                    &if self.encoding_fallback_cp932 {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "encoding.russian_keyboard" => {
                ini.set(
                    "Tera Term",
                    "RussKeyb",
                    &self.encoding_russian_keyboard.as_ini().to_string(),
                );
            }
            "encoding.dec_special_direction" => {
                ini.set(
                    "Tera Term",
                    "DecSpMappingDir",
                    &self.encoding_dec_special_direction.as_ini().to_string(),
                );
            }
            "encoding.unicode_to_dec_special" => {
                ini.set(
                    "Tera Term",
                    "UnicodeToDecSpMapping",
                    &self.encoding_unicode_to_dec_special.to_string(),
                );
            }
            "encoding.ambiguous_width" => {
                ini.set(
                    "Tera Term",
                    "UnicodeAmbiguousWidth",
                    &self.encoding_ambiguous_width.to_string(),
                );
            }
            "encoding.emoji_override" => {
                ini.set(
                    "Tera Term",
                    "UnicodeEmojiOverride",
                    &if self.encoding_emoji_override {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "encoding.emoji_width" => {
                ini.set(
                    "Tera Term",
                    "UnicodeEmojiWidth",
                    &self.encoding_emoji_width.to_string(),
                );
            }
            "ime.enabled" => {
                ini.set(
                    "Tera Term",
                    "IME",
                    &if self.ime_enabled { "on" } else { "off" }.to_string(),
                );
            }
            "ime.inline" => {
                ini.set(
                    "Tera Term",
                    "IMEInline",
                    &if self.ime_inline { "on" } else { "off" }.to_string(),
                );
            }
            "ime.cursor_related" => {
                ini.set(
                    "Tera Term",
                    "IMERelatedCursor",
                    &if self.ime_cursor_related { "on" } else { "off" }.to_string(),
                );
            }
            "keyboard.backspace" => {
                ini.set(
                    "Tera Term",
                    "BSKey",
                    &self.keyboard_backspace.as_ini().to_string(),
                );
            }
            "keyboard.meta" => {
                ini.set(
                    "Tera Term",
                    "MetaKey",
                    &self.keyboard_meta.as_ini().to_string(),
                );
            }
            "keyboard.delete_sends_del" => {
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
            }
            "keyboard.disable_app_keypad" => {
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
            }
            "keyboard.disable_app_cursor" => {
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
            }
            "keyboard.word_delimiters" => {
                ini.set(
                    "Tera Term",
                    "DelimList",
                    &self.keyboard_word_delimiters.clone(),
                );
            }
            "keyboard.width_delimits_word" => {
                ini.set(
                    "Tera Term",
                    "DelimDBCS",
                    &if self.keyboard_width_delimits_word {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "keyboard.strict_mapping" => {
                ini.set(
                    "Tera Term",
                    "StrictKeyMapping",
                    &if self.keyboard_strict_mapping {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "keyboard.meta_8bit" => {
                ini.set(
                    "Tera Term",
                    "Meta8Bit",
                    &self.keyboard_meta_8bit.as_ini().to_string(),
                );
            }
            "color.normal" => {
                ini.set(
                    "Tera Term",
                    "VTColor",
                    &crate::schema::color2_str(&self.color_normal),
                );
            }
            "color.bold" => {
                ini.set(
                    "Tera Term",
                    "VTBoldColor",
                    &crate::schema::color2_str(&self.color_bold),
                );
            }
            "color.blink" => {
                ini.set(
                    "Tera Term",
                    "VTBlinkColor",
                    &crate::schema::color2_str(&self.color_blink),
                );
            }
            "color.underline" => {
                ini.set(
                    "Tera Term",
                    "VTUnderlineColor",
                    &crate::schema::color2_str(&self.color_underline),
                );
            }
            "color.reverse" => {
                ini.set(
                    "Tera Term",
                    "VTReverseColor",
                    &crate::schema::color2_str(&self.color_reverse),
                );
            }
            "color.url" => {
                ini.set(
                    "Tera Term",
                    "URLColor",
                    &crate::schema::color2_str(&self.color_url),
                );
            }
            "color.url_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnableURLColor",
                    &if self.color_url_enabled { "on" } else { "off" }.to_string(),
                );
            }
            "color.url_underline" => {
                ini.set(
                    "Tera Term",
                    "URLUnderline",
                    &if self.color_url_underline {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "color.bold_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnableBoldAttrColor",
                    &if self.color_bold_enabled { "on" } else { "off" }.to_string(),
                );
            }
            "color.blink_enabled" => {
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
            }
            "color.reverse_enabled" => {
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
            }
            "color.underline_enabled" => {
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
            }
            "color.ansi_palette" => {
                ini.set("Tera Term", "ANSIColor", &self.color_ansi_palette.clone());
            }
            "color.xterm_256" => {
                ini.set(
                    "Tera Term",
                    "Xterm256Color",
                    &if self.color_xterm_256 { "on" } else { "off" }.to_string(),
                );
            }
            "color.aixterm_16" => {
                ini.set(
                    "Tera Term",
                    "Aixterm16Color",
                    &if self.color_aixterm_16 { "on" } else { "off" }.to_string(),
                );
            }
            "color.pc_bold_16" => {
                ini.set(
                    "Tera Term",
                    "PcBoldColor",
                    &if self.color_pc_bold_16 { "on" } else { "off" }.to_string(),
                );
            }
            "color.ansi_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnableANSIColor",
                    &if self.color_ansi_enabled { "on" } else { "off" }.to_string(),
                );
            }
            "color.bold_font" => {
                ini.set(
                    "Tera Term",
                    "EnableBold",
                    &if self.color_bold_font { "on" } else { "off" }.to_string(),
                );
            }
            "color.underline_font" => {
                ini.set(
                    "Tera Term",
                    "UnderlineAttrFont",
                    &if self.color_underline_font {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "color.use_text_color" => {
                ini.set(
                    "Tera Term",
                    "UseTextColor",
                    &if self.color_use_text_color {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "color.use_normal_background" => {
                ini.set(
                    "Tera Term",
                    "UseNormalBGColor",
                    &if self.color_use_normal_background {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "cursor.shape" => {
                ini.set(
                    "Tera Term",
                    "CursorShape",
                    &self.cursor_shape.as_ini().to_string(),
                );
            }
            "cursor.nonblinking" => {
                ini.set(
                    "Tera Term",
                    "NonblinkingCursor",
                    &if self.cursor_nonblinking { "on" } else { "off" }.to_string(),
                );
            }
            "cursor.show_unfocused" => {
                ini.set(
                    "Tera Term",
                    "KillFocusCursor",
                    &if self.cursor_show_unfocused {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.change_allowed" => {
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
            }
            "window.report_allowed" => {
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
            }
            "window.cursor_ctrl_allowed" => {
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
            }
            "window.accept_8bit_ctrl" => {
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
            }
            "window.send_8bit_ctrl" => {
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
            }
            "window.alt_screen" => {
                ini.set(
                    "Tera Term",
                    "AlternateScreenBuffer",
                    &if self.window_alt_screen { "on" } else { "off" }.to_string(),
                );
            }
            "window.remote_clears_buffer" => {
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
            }
            "window.auto_scroll_only_at_bottom" => {
                ini.set(
                    "Tera Term",
                    "AutoScrollOnlyInBottomLine",
                    &if self.window_auto_scroll_only_at_bottom {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.scroll_threshold" => {
                ini.set(
                    "Tera Term",
                    "ScrollThreshold",
                    &self.window_scroll_threshold.to_string(),
                );
            }
            "window.opacity_inactive" => {
                ini.set(
                    "Tera Term",
                    "AlphaBlend",
                    &self.window_opacity_inactive.to_string(),
                );
            }
            "window.opacity_active" => {
                ini.set(
                    "Tera Term",
                    "AlphaBlendActive",
                    &self.window_opacity_active.to_string(),
                );
            }
            "window.title_format" => {
                ini.set(
                    "Tera Term",
                    "TitleFormat",
                    &self.window_title_format.to_string(),
                );
            }
            "window.title_change" => {
                ini.set(
                    "Tera Term",
                    "AcceptTitleChangeRequest",
                    &self.window_title_change.as_ini().to_string(),
                );
            }
            "window.title_report" => {
                ini.set(
                    "Tera Term",
                    "TitleReportSequence",
                    &self.window_title_report.as_ini().to_string(),
                );
            }
            "font.quality" => {
                ini.set(
                    "Tera Term",
                    "FontQuality",
                    &self.font_quality.as_ini().to_string(),
                );
            }
            "font.draw_resized" => {
                ini.set(
                    "Tera Term",
                    "DrawingResizedFont",
                    &if self.font_draw_resized { "on" } else { "off" }.to_string(),
                );
            }
            "font.draw_api" => {
                ini.set(
                    "Tera Term",
                    "VTDrawAPI",
                    &self.font_draw_api.as_ini().to_string(),
                );
            }
            "font.draw_ansi_code_page" => {
                ini.set(
                    "Tera Term",
                    "VTDrawACP",
                    &self.font_draw_ansi_code_page.to_string(),
                );
            }
            "font.space_left" => {
                ini.set(
                    "Tera Term",
                    "VTFontSpace",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTFontSpace"),
                        0,
                        self.font_space_left,
                    ),
                );
            }
            "font.space_right" => {
                ini.set(
                    "Tera Term",
                    "VTFontSpace",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTFontSpace"),
                        1,
                        self.font_space_right,
                    ),
                );
            }
            "font.space_top" => {
                ini.set(
                    "Tera Term",
                    "VTFontSpace",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTFontSpace"),
                        2,
                        self.font_space_top,
                    ),
                );
            }
            "font.space_bottom" => {
                ini.set(
                    "Tera Term",
                    "VTFontSpace",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTFontSpace"),
                        3,
                        self.font_space_bottom,
                    ),
                );
            }
            "font.scaling" => {
                ini.set(
                    "Tera Term",
                    "FontScaling",
                    &if self.font_scaling { "on" } else { "off" }.to_string(),
                );
            }
            "mouse.tracking" => {
                ini.set(
                    "Tera Term",
                    "MouseEventTracking",
                    &if self.mouse_tracking { "on" } else { "off" }.to_string(),
                );
            }
            "mouse.ctrl_disables_tracking" => {
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
            }
            "mouse.wheel_to_cursor" => {
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
            }
            "mouse.ctrl_disables_wheel_to_cursor" => {
                ini.set(
                    "Tera Term",
                    "DisableWheelToCursorByCtrl",
                    &if self.mouse_ctrl_disables_wheel_to_cursor {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "mouse.wheel_scroll_line" => {
                ini.set(
                    "Tera Term",
                    "MouseWheelScrollLine",
                    &self.mouse_wheel_scroll_line.to_string(),
                );
            }
            "mouse.clickable_url" => {
                ini.set(
                    "Tera Term",
                    "EnableClickableUrl",
                    &if self.mouse_clickable_url {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "mouse.cursor" => {
                ini.set("Tera Term", "MouseCursor", &self.mouse_cursor.clone());
            }
            "url.browser" => {
                ini.set(
                    "Tera Term",
                    "ClickableUrlBrowser",
                    &self.url_browser.clone(),
                );
            }
            "url.browser_args" => {
                ini.set(
                    "Tera Term",
                    "ClickableUrlBrowserArg",
                    &self.url_browser_args.clone(),
                );
            }
            "url.join_split" => {
                ini.set(
                    "Tera Term",
                    "JoinSplitURL",
                    &if self.url_join_split { "on" } else { "off" }.to_string(),
                );
            }
            "url.join_split_ignore_eol_char" => {
                ini.set(
                    "Tera Term",
                    "JoinSplitURLIgnoreEOLChar",
                    &self.url_join_split_ignore_eol_char.clone(),
                );
            }
            "debug.enabled" => {
                ini.set(
                    "Tera Term",
                    "Debug",
                    &if self.debug_enabled { "on" } else { "off" }.to_string(),
                );
            }
            "debug.modes" => {
                ini.set("Tera Term", "DebugModes", &self.debug_modes.clone());
            }
            "bell.mode" => {
                ini.set("Tera Term", "Beep", &self.bell_mode.as_ini().to_string());
            }
            "bell.on_connect" => {
                ini.set(
                    "Tera Term",
                    "BeepOnConnect",
                    &if self.bell_on_connect { "on" } else { "off" }.to_string(),
                );
            }
            "bell.visual_wait_ms" => {
                ini.set(
                    "Tera Term",
                    "BeepVBellWait",
                    &self.bell_visual_wait_ms.to_string(),
                );
            }
            "bell.over_used_count" => {
                ini.set(
                    "Tera Term",
                    "BeepOverUsedCount",
                    &self.bell_over_used_count.to_string(),
                );
            }
            "bell.over_used_time" => {
                ini.set(
                    "Tera Term",
                    "BeepOverUsedTime",
                    &self.bell_over_used_time.to_string(),
                );
            }
            "bell.suppress_time" => {
                ini.set(
                    "Tera Term",
                    "BeepSuppressTime",
                    &self.bell_suppress_time.to_string(),
                );
            }
            "bell.notify_sound" => {
                ini.set(
                    "Tera Term",
                    "NotifySound",
                    &if self.bell_notify_sound { "on" } else { "off" }.to_string(),
                );
            }
            "clipboard.continued_line_copy" => {
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
            }
            "clipboard.auto_copy" => {
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
            }
            "clipboard.select_on_activate" => {
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
            }
            "clipboard.select_only_by_lbutton" => {
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
            }
            "clipboard.select_start_delay" => {
                ini.set(
                    "Tera Term",
                    "MouseSelectStartDelay",
                    &self.clipboard_select_start_delay.to_string(),
                );
            }
            "clipboard.paste_rbutton_disabled" => {
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
            }
            "clipboard.paste_mbutton_disabled" => {
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
            }
            "clipboard.confirm_paste_rbutton" => {
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
            }
            "clipboard.confirm_paste" => {
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
            }
            "clipboard.confirm_paste_cr" => {
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
            }
            "clipboard.confirm_paste_dictionary" => {
                ini.set(
                    "Tera Term",
                    "ConfirmChangePasteStringFile",
                    &self.clipboard_confirm_paste_dictionary.clone(),
                );
            }
            "clipboard.trim_trailing_newline" => {
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
            }
            "clipboard.paste_delay_per_line" => {
                ini.set(
                    "Tera Term",
                    "PasteDelayPerLine",
                    &self.clipboard_paste_delay_per_line.to_string(),
                );
            }
            "clipboard.paste_dialog_width" => {
                ini.set(
                    "Tera Term",
                    "PasteDialogSize",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PasteDialogSize"),
                        0,
                        self.clipboard_paste_dialog_width,
                    ),
                );
            }
            "clipboard.paste_dialog_height" => {
                ini.set(
                    "Tera Term",
                    "PasteDialogSize",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PasteDialogSize"),
                        1,
                        self.clipboard_paste_dialog_height,
                    ),
                );
            }
            "clipboard.bracketed" => {
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
            }
            "clipboard.bracketed_control_only" => {
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
            }
            "clipboard.remote_access" => {
                ini.set(
                    "Tera Term",
                    "ClipboardAccessFromRemote",
                    &self.clipboard_remote_access.as_ini().to_string(),
                );
            }
            "clipboard.remote_notify" => {
                ini.set(
                    "Tera Term",
                    "NotifyClipboardAccess",
                    &if self.clipboard_remote_notify {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.port_type" => {
                ini.set(
                    "Tera Term",
                    "Port",
                    &self.connection_port_type.as_ini().to_string(),
                );
            }
            "connection.tcp_port" => {
                ini.set(
                    "Tera Term",
                    "TCPPort",
                    &self.connection_tcp_port.to_string(),
                );
            }
            "connection.telnet" => {
                ini.set(
                    "Tera Term",
                    "Telnet",
                    &if self.connection_telnet { "on" } else { "off" }.to_string(),
                );
            }
            "connection.telnet_port" => {
                ini.set(
                    "Tera Term",
                    "TelPort",
                    &self.connection_telnet_port.to_string(),
                );
            }
            "connection.telnet_binary" => {
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
            }
            "connection.telnet_auto_detect" => {
                ini.set(
                    "Tera Term",
                    "TelAutoDetect",
                    &if self.connection_telnet_auto_detect {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.telnet_echo" => {
                ini.set(
                    "Tera Term",
                    "TelEcho",
                    &if self.connection_telnet_echo {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.telnet_log" => {
                ini.set(
                    "Tera Term",
                    "TelLog",
                    &if self.connection_telnet_log {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.telnet_keepalive" => {
                ini.set(
                    "Tera Term",
                    "TelKeepAliveInterval",
                    &self.connection_telnet_keepalive.to_string(),
                );
            }
            "connection.tcp_local_echo" => {
                ini.set(
                    "Tera Term",
                    "TCPLocalEcho",
                    &if self.connection_tcp_local_echo {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.tcp_cr_send" => {
                ini.set(
                    "Tera Term",
                    "TCPCRSend",
                    &self.connection_tcp_cr_send.as_ini().to_string(),
                );
            }
            "connection.confirm_disconnect" => {
                ini.set(
                    "Tera Term",
                    "ConfirmDisconnect",
                    &if self.connection_confirm_disconnect {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.history_list" => {
                ini.set(
                    "Tera Term",
                    "HistoryList",
                    &if self.connection_history_list {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.term_type" => {
                ini.set("Tera Term", "TermType", &self.connection_term_type.clone());
            }
            "connection.terminal_speed" => {
                ini.set(
                    "Tera Term",
                    "TerminalSpeed",
                    &self.connection_terminal_speed.clone(),
                );
            }
            "connection.auto_win_close" => {
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
            }
            "connection.clear_screen_on_close" => {
                ini.set(
                    "Tera Term",
                    "ClearScreenOnCloseConnection",
                    &if self.connection_clear_screen_on_close {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.timeout" => {
                ini.set(
                    "Tera Term",
                    "ConnectingTimeout",
                    &self.connection_timeout.to_string(),
                );
            }
            "connection.host_dialog_on_startup" => {
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
            }
            "connection.line_mode" => {
                ini.set(
                    "Tera Term",
                    "EnableLineMode",
                    &if self.connection_line_mode {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "proxy.type" => {
                ini.set(
                    "TTProxy",
                    "ProxyType",
                    &self.proxy_type.as_ini().to_string(),
                );
            }
            "proxy.host" => {
                ini.set("TTProxy", "ProxyHost", &crate::esc::quote(&self.proxy_host));
            }
            "proxy.port" => {
                ini.set("TTProxy", "ProxyPort", &self.proxy_port.to_string());
            }
            "proxy.user" => {
                ini.set("TTProxy", "ProxyUser", &crate::esc::quote(&self.proxy_user));
            }
            "proxy.pass" => {
                ini.set("TTProxy", "ProxyPass", &crate::esc::quote(&self.proxy_pass));
            }
            "proxy.timeout" => {
                ini.set(
                    "TTProxy",
                    "ConnectionTimeout",
                    &self.proxy_timeout.to_string(),
                );
            }
            "proxy.socks_resolve" => {
                ini.set(
                    "TTProxy",
                    "SocksResolve",
                    &self.proxy_socks_resolve.as_ini().to_string(),
                );
            }
            "proxy.telnet_hostname_prompt" => {
                ini.set(
                    "TTProxy",
                    "TelnetHostnamePrompt",
                    &crate::esc::quote(&self.proxy_telnet_hostname_prompt),
                );
            }
            "proxy.telnet_username_prompt" => {
                ini.set(
                    "TTProxy",
                    "TelnetUsernamePrompt",
                    &crate::esc::quote(&self.proxy_telnet_username_prompt),
                );
            }
            "proxy.telnet_password_prompt" => {
                ini.set(
                    "TTProxy",
                    "TelnetPasswordPrompt",
                    &crate::esc::quote(&self.proxy_telnet_password_prompt),
                );
            }
            "proxy.telnet_connected_message" => {
                ini.set(
                    "TTProxy",
                    "TelnetConnectedMessage",
                    &crate::esc::quote(&self.proxy_telnet_connected_message),
                );
            }
            "proxy.telnet_error_message" => {
                ini.set(
                    "TTProxy",
                    "TelnetErrorMessage",
                    &crate::esc::quote(&self.proxy_telnet_error_message),
                );
            }
            "proxy.debug_log" => {
                ini.set(
                    "TTProxy",
                    "DebugLog",
                    &crate::esc::quote(&self.proxy_debug_log),
                );
            }
            "macro.startup_file" => {
                ini.set(
                    "Tera Term",
                    "StartupMacro",
                    &self.macro_startup_file.clone(),
                );
            }
            "settings.source_version" => {
                ini.set(
                    "Tera Term",
                    "Version",
                    &self.settings_source_version.clone(),
                );
            }
            "settings.language_file" => {
                ini.set(
                    "Tera Term",
                    "UILanguageFile",
                    &self.settings_language_file.clone(),
                );
            }
            "settings.auto_backup" => {
                ini.set(
                    "Tera Term",
                    "IniAutoBackup",
                    &if self.settings_auto_backup {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "serial.com_port" => {
                ini.set("Tera Term", "ComPort", &self.serial_com_port.to_string());
            }
            "serial.baud" => {
                ini.set("Tera Term", "BaudRate", &self.serial_baud.to_string());
            }
            "serial.data_bits" => {
                ini.set(
                    "Tera Term",
                    "DataBit",
                    &self.serial_data_bits.as_ini().to_string(),
                );
            }
            "serial.parity" => {
                ini.set(
                    "Tera Term",
                    "Parity",
                    &self.serial_parity.as_ini().to_string(),
                );
            }
            "serial.stop_bits" => {
                ini.set(
                    "Tera Term",
                    "StopBit",
                    &self.serial_stop_bits.as_ini().to_string(),
                );
            }
            "serial.flow" => {
                ini.set(
                    "Tera Term",
                    "FlowCtrl",
                    &self.serial_flow.as_ini().to_string(),
                );
            }
            "serial.delay_per_char" => {
                ini.set(
                    "Tera Term",
                    "DelayPerChar",
                    &self.serial_delay_per_char.to_string(),
                );
            }
            "serial.delay_per_line" => {
                ini.set(
                    "Tera Term",
                    "DelayPerLine",
                    &self.serial_delay_per_line.to_string(),
                );
            }
            "serial.wait_com" => {
                ini.set(
                    "Tera Term",
                    "WaitCom",
                    &if self.serial_wait_com { "on" } else { "off" }.to_string(),
                );
            }
            "serial.max_com_port" => {
                ini.set(
                    "Tera Term",
                    "MaxComPort",
                    &self.serial_max_com_port.to_string(),
                );
            }
            "serial.rts" => {
                ini.set("Tera Term", "FlowCtrlRTS", &self.serial_rts.to_string());
            }
            "serial.dtr" => {
                ini.set("Tera Term", "FlowCtrlDTR", &self.serial_dtr.to_string());
            }
            "serial.clear_buffer_on_open" => {
                ini.set(
                    "Tera Term",
                    "ClearComBuffOnOpen",
                    &if self.serial_clear_buffer_on_open {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "serial.break_time" => {
                ini.set(
                    "Tera Term",
                    "SendBreakTime",
                    &self.serial_break_time.to_string(),
                );
            }
            "serial.auto_reconnect" => {
                ini.set(
                    "Tera Term",
                    "AutoComPortReconnect",
                    &if self.serial_auto_reconnect {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "serial.auto_reconnect_delay" => {
                ini.set(
                    "Tera Term",
                    "AutoComPortReconnectDelayNormal",
                    &self.serial_auto_reconnect_delay.to_string(),
                );
            }
            "serial.auto_reconnect_delay_unknown_port" => {
                ini.set(
                    "Tera Term",
                    "AutoComPortReconnectDelayIllegal",
                    &self.serial_auto_reconnect_delay_unknown_port.to_string(),
                );
            }
            "serial.auto_reconnect_retry_interval" => {
                ini.set(
                    "Tera Term",
                    "AutoComPortReconnectRetryInterval",
                    &self.serial_auto_reconnect_retry_interval.to_string(),
                );
            }
            "serial.auto_reconnect_retries" => {
                ini.set(
                    "Tera Term",
                    "AutoComPortReconnectRetryCount",
                    &self.serial_auto_reconnect_retries.to_string(),
                );
            }
            "log.auto_start" => {
                ini.set(
                    "Tera Term",
                    "LogAutoStart",
                    &if self.log_auto_start { "on" } else { "off" }.to_string(),
                );
            }
            "log.binary" => {
                ini.set(
                    "Tera Term",
                    "LogBinary",
                    &if self.log_binary { "on" } else { "off" }.to_string(),
                );
            }
            "log.append" => {
                ini.set(
                    "Tera Term",
                    "LogAppend",
                    &if self.log_append { "on" } else { "off" }.to_string(),
                );
            }
            "log.plain_text" => {
                ini.set(
                    "Tera Term",
                    "LogTypePlainText",
                    &if self.log_plain_text { "on" } else { "off" }.to_string(),
                );
            }
            "log.timestamp" => {
                ini.set(
                    "Tera Term",
                    "LogTimestamp",
                    &if self.log_timestamp { "on" } else { "off" }.to_string(),
                );
            }
            "log.timestamp_type" => {
                ini.set(
                    "Tera Term",
                    "LogTimestampType",
                    &self.log_timestamp_type.as_ini().to_string(),
                );
            }
            "log.timestamp_utc" => {
                ini.set(
                    "Tera Term",
                    "LogTimestampUTC",
                    &if self.log_timestamp_utc { "on" } else { "off" }.to_string(),
                );
            }
            "log.timestamp_format" => {
                ini.set(
                    "Tera Term",
                    "LogTimestampFormat",
                    &self.log_timestamp_format.clone(),
                );
            }
            "log.default_name" => {
                ini.set(
                    "Tera Term",
                    "LogDefaultName",
                    &self.log_default_name.clone(),
                );
            }
            "log.default_path" => {
                ini.set(
                    "Tera Term",
                    "LogDefaultPath",
                    &self.log_default_path.clone(),
                );
            }
            "log.rotate" => {
                ini.set("Tera Term", "LogRotate", &self.log_rotate.to_string());
            }
            "log.rotate_size" => {
                ini.set(
                    "Tera Term",
                    "LogRotateSize",
                    &self.log_rotate_size.to_string(),
                );
            }
            "log.rotate_size_type" => {
                ini.set(
                    "Tera Term",
                    "LogRotateSizeType",
                    &self.log_rotate_size_type.to_string(),
                );
            }
            "log.rotate_step" => {
                ini.set(
                    "Tera Term",
                    "LogRotateStep",
                    &self.log_rotate_step.to_string(),
                );
            }
            "log.hide_dialog" => {
                ini.set(
                    "Tera Term",
                    "LogHideDialog",
                    &if self.log_hide_dialog { "on" } else { "off" }.to_string(),
                );
            }
            "log.include_screen_buffer" => {
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
            }
            "log.lock_exclusive" => {
                ini.set(
                    "Tera Term",
                    "LogLockExclusive",
                    &if self.log_lock_exclusive { "on" } else { "off" }.to_string(),
                );
            }
            "log.deferred_write" => {
                ini.set(
                    "Tera Term",
                    "DeferredLogWriteMode",
                    &if self.log_deferred_write { "on" } else { "off" }.to_string(),
                );
            }
            "transfer.dir" => {
                ini.set("Tera Term", "FileDir", &self.transfer_dir.clone());
            }
            "transfer.binary" => {
                ini.set(
                    "Tera Term",
                    "TransBin",
                    &if self.transfer_binary { "on" } else { "off" }.to_string(),
                );
            }
            "transfer.auto_rename" => {
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
            }
            "transfer.hide_dialog" => {
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
            }
            "transfer.xmodem_opt" => {
                ini.set(
                    "Tera Term",
                    "XmodemOpt",
                    &self.transfer_xmodem_opt.as_ini().to_string(),
                );
            }
            "transfer.xmodem_binary" => {
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
            }
            "transfer.xmodem_rcv_command" => {
                ini.set(
                    "Tera Term",
                    "XModemRcvCommand",
                    &self.transfer_xmodem_rcv_command.clone(),
                );
            }
            "transfer.xmodem_log" => {
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
            }
            "transfer.xmodem_timeout_init" => {
                ini.set(
                    "Tera Term",
                    "XmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "XmodemTimeouts"),
                        0,
                        self.transfer_xmodem_timeout_init,
                    ),
                );
            }
            "transfer.xmodem_timeout_init_crc" => {
                ini.set(
                    "Tera Term",
                    "XmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "XmodemTimeouts"),
                        1,
                        self.transfer_xmodem_timeout_init_crc,
                    ),
                );
            }
            "transfer.xmodem_timeout_short" => {
                ini.set(
                    "Tera Term",
                    "XmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "XmodemTimeouts"),
                        2,
                        self.transfer_xmodem_timeout_short,
                    ),
                );
            }
            "transfer.xmodem_timeout_long" => {
                ini.set(
                    "Tera Term",
                    "XmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "XmodemTimeouts"),
                        3,
                        self.transfer_xmodem_timeout_long,
                    ),
                );
            }
            "transfer.xmodem_timeout_vlong" => {
                ini.set(
                    "Tera Term",
                    "XmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "XmodemTimeouts"),
                        4,
                        self.transfer_xmodem_timeout_vlong,
                    ),
                );
            }
            "transfer.ymodem_rcv_command" => {
                ini.set(
                    "Tera Term",
                    "YModemRcvCommand",
                    &self.transfer_ymodem_rcv_command.clone(),
                );
            }
            "transfer.ymodem_log" => {
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
            }
            "transfer.ymodem_timeout_init" => {
                ini.set(
                    "Tera Term",
                    "YmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "YmodemTimeouts"),
                        0,
                        self.transfer_ymodem_timeout_init,
                    ),
                );
            }
            "transfer.ymodem_timeout_init_crc" => {
                ini.set(
                    "Tera Term",
                    "YmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "YmodemTimeouts"),
                        1,
                        self.transfer_ymodem_timeout_init_crc,
                    ),
                );
            }
            "transfer.ymodem_timeout_short" => {
                ini.set(
                    "Tera Term",
                    "YmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "YmodemTimeouts"),
                        2,
                        self.transfer_ymodem_timeout_short,
                    ),
                );
            }
            "transfer.ymodem_timeout_long" => {
                ini.set(
                    "Tera Term",
                    "YmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "YmodemTimeouts"),
                        3,
                        self.transfer_ymodem_timeout_long,
                    ),
                );
            }
            "transfer.ymodem_timeout_vlong" => {
                ini.set(
                    "Tera Term",
                    "YmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "YmodemTimeouts"),
                        4,
                        self.transfer_ymodem_timeout_vlong,
                    ),
                );
            }
            "transfer.zmodem_auto" => {
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
            }
            "transfer.zmodem_data_len" => {
                ini.set(
                    "Tera Term",
                    "ZmodemDataLen",
                    &self.transfer_zmodem_data_len.to_string(),
                );
            }
            "transfer.zmodem_win_size" => {
                ini.set(
                    "Tera Term",
                    "ZmodemWinSize",
                    &self.transfer_zmodem_win_size.to_string(),
                );
            }
            "transfer.zmodem_escape_ctl" => {
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
            }
            "transfer.zmodem_log" => {
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
            }
            "transfer.zmodem_rcv_command" => {
                ini.set(
                    "Tera Term",
                    "ZModemRcvCommand",
                    &self.transfer_zmodem_rcv_command.clone(),
                );
            }
            "transfer.zmodem_timeout_normal" => {
                ini.set(
                    "Tera Term",
                    "ZmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "ZmodemTimeouts"),
                        0,
                        self.transfer_zmodem_timeout_normal,
                    ),
                );
            }
            "transfer.zmodem_timeout_tcpip" => {
                ini.set(
                    "Tera Term",
                    "ZmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "ZmodemTimeouts"),
                        1,
                        self.transfer_zmodem_timeout_tcpip,
                    ),
                );
            }
            "transfer.zmodem_timeout_init" => {
                ini.set(
                    "Tera Term",
                    "ZmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "ZmodemTimeouts"),
                        2,
                        self.transfer_zmodem_timeout_init,
                    ),
                );
            }
            "transfer.zmodem_timeout_fin" => {
                ini.set(
                    "Tera Term",
                    "ZmodemTimeouts",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "ZmodemTimeouts"),
                        3,
                        self.transfer_zmodem_timeout_fin,
                    ),
                );
            }
            "transfer.kermit_long_packet" => {
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
            }
            "transfer.kermit_file_attr" => {
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
            }
            "transfer.kermit_log" => {
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
            }
            "transfer.bplus_auto" => {
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
            }
            "transfer.bplus_escape_ctl" => {
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
            }
            "transfer.bplus_log" => {
                ini.set(
                    "Tera Term",
                    "BPLog",
                    &if self.transfer_bplus_log { "on" } else { "off" }.to_string(),
                );
            }
            "transfer.quickvan_win_size" => {
                ini.set(
                    "Tera Term",
                    "QVWinSize",
                    &self.transfer_quickvan_win_size.to_string(),
                );
            }
            "transfer.quickvan_log" => {
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
            }
            "transfer.raw_autostop" => {
                ini.set(
                    "Tera Term",
                    "ReceivefileAutoStopWaitTime",
                    &self.transfer_raw_autostop.to_string(),
                );
            }
            "transfer.send_filter" => {
                ini.set(
                    "Tera Term",
                    "FileSendFilter",
                    &self.transfer_send_filter.clone(),
                );
            }
            "transfer.receive_filter" => {
                ini.set(
                    "Tera Term",
                    "FileReceiveFilter",
                    &self.transfer_receive_filter.clone(),
                );
            }
            "transfer.scp_send_dir" => {
                ini.set(
                    "Tera Term",
                    "ScpSendDir",
                    &self.transfer_scp_send_dir.clone(),
                );
            }
            "transfer.raw_high_speed" => {
                ini.set(
                    "Tera Term",
                    "FileSendHighSpeedMode",
                    &if self.transfer_raw_high_speed {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "transfer.confirm_file_drop" => {
                ini.set(
                    "Tera Term",
                    "ConfirmFileDragAndDrop",
                    &if self.transfer_confirm_file_drop {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "transfer.raw_send_delay_type" => {
                ini.set(
                    "Tera Term",
                    "SendfileDelayType",
                    &self.transfer_raw_send_delay_type.as_ini().to_string(),
                );
            }
            "transfer.raw_send_delay_tick" => {
                ini.set(
                    "Tera Term",
                    "SendfileDelayTick",
                    &self.transfer_raw_send_delay_tick.to_string(),
                );
            }
            "transfer.raw_send_size" => {
                ini.set(
                    "Tera Term",
                    "SendfileSize",
                    &self.transfer_raw_send_size.to_string(),
                );
            }
            "transfer.raw_send_sequential" => {
                ini.set(
                    "Tera Term",
                    "SendfileSequential",
                    &if self.transfer_raw_send_sequential {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "transfer.raw_send_skip_dialog" => {
                ini.set(
                    "Tera Term",
                    "SendfileSkipOptionDialog",
                    &if self.transfer_raw_send_skip_dialog {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "transfer.raw_receive_skip_dialog" => {
                ini.set(
                    "Tera Term",
                    "ReceivefileSkipOptionDialog",
                    &if self.transfer_raw_receive_skip_dialog {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "printer.passthrough_delay" => {
                ini.set(
                    "Tera Term",
                    "PassThruDelay",
                    &self.printer_passthrough_delay.to_string(),
                );
            }
            "printer.passthrough_port" => {
                ini.set(
                    "Tera Term",
                    "PassThruPort",
                    &self.printer_passthrough_port.clone(),
                );
            }
            "printer.control_sequences" => {
                ini.set(
                    "Tera Term",
                    "PrinterCtrlSequence",
                    &if self.printer_control_sequences {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "printer.margin_left" => {
                ini.set(
                    "Tera Term",
                    "PrnMargin",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PrnMargin"),
                        0,
                        self.printer_margin_left,
                    ),
                );
            }
            "printer.margin_right" => {
                ini.set(
                    "Tera Term",
                    "PrnMargin",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PrnMargin"),
                        1,
                        self.printer_margin_right,
                    ),
                );
            }
            "printer.margin_top" => {
                ini.set(
                    "Tera Term",
                    "PrnMargin",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PrnMargin"),
                        2,
                        self.printer_margin_top,
                    ),
                );
            }
            "printer.margin_bottom" => {
                ini.set(
                    "Tera Term",
                    "PrnMargin",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "PrnMargin"),
                        3,
                        self.printer_margin_bottom,
                    ),
                );
            }
            "printer.convert_form_feed" => {
                ini.set(
                    "Tera Term",
                    "PrnConvFF",
                    &if self.printer_convert_form_feed {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "printer.vt_ppi_x" => {
                ini.set(
                    "Tera Term",
                    "VTPPI",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTPPI"),
                        0,
                        self.printer_vt_ppi_x,
                    ),
                );
            }
            "printer.vt_ppi_y" => {
                ini.set(
                    "Tera Term",
                    "VTPPI",
                    &crate::schema::with_nth(
                        ini.get("Tera Term", "VTPPI"),
                        1,
                        self.printer_vt_ppi_y,
                    ),
                );
            }
            "tek.x" => {
                ini.set(
                    "Tera Term",
                    "TEKPos",
                    &crate::schema::with_nth(ini.get("Tera Term", "TEKPos"), 0, self.tek_x),
                );
            }
            "tek.y" => {
                ini.set(
                    "Tera Term",
                    "TEKPos",
                    &crate::schema::with_nth(ini.get("Tera Term", "TEKPos"), 1, self.tek_y),
                );
            }
            "tek.color" => {
                ini.set(
                    "Tera Term",
                    "TEKColor",
                    &crate::schema::color2_str(&self.tek_color),
                );
            }
            "tek.color_emulation" => {
                ini.set(
                    "Tera Term",
                    "TEKColorEmulation",
                    &if self.tek_color_emulation {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "tek.gin_mouse_code" => {
                ini.set(
                    "Tera Term",
                    "TEKGINMouseCode",
                    &self.tek_gin_mouse_code.to_string(),
                );
            }
            "tek.icon" => {
                ini.set("Tera Term", "TEKIcon", &self.tek_icon.as_ini().to_string());
            }
            "tek.ppi_x" => {
                ini.set(
                    "Tera Term",
                    "TEKPPI",
                    &crate::schema::with_nth(ini.get("Tera Term", "TEKPPI"), 0, self.tek_ppi_x),
                );
            }
            "tek.ppi_y" => {
                ini.set(
                    "Tera Term",
                    "TEKPPI",
                    &crate::schema::with_nth(ini.get("Tera Term", "TEKPPI"), 1, self.tek_ppi_y),
                );
            }
            "window.maximized_bug_tweak" => {
                ini.set(
                    "Tera Term",
                    "MaximizedBugTweak",
                    &self.window_maximized_bug_tweak.to_string(),
                );
            }
            "window.icon" => {
                ini.set(
                    "Tera Term",
                    "VTIcon",
                    &self.window_icon.as_ini().to_string(),
                );
            }
            "window.hide_title" => {
                ini.set(
                    "Tera Term",
                    "HideTitle",
                    &if self.window_hide_title { "on" } else { "off" }.to_string(),
                );
            }
            "window.popup_menu" => {
                ini.set(
                    "Tera Term",
                    "PopupMenu",
                    &if self.window_popup_menu { "on" } else { "off" }.to_string(),
                );
            }
            "window.popup_menu_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnablePopupMenu",
                    &if self.window_popup_menu_enabled {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.show_menu_enabled" => {
                ini.set(
                    "Tera Term",
                    "EnableShowMenu",
                    &if self.window_show_menu_enabled {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.window_menu" => {
                ini.set(
                    "Tera Term",
                    "WindowMenu",
                    &if self.window_window_menu { "on" } else { "off" }.to_string(),
                );
            }
            "window.save_position" => {
                ini.set(
                    "Tera Term",
                    "SaveVTWinPos",
                    &if self.window_save_position {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.x" => {
                if self.window_save_position {
                    ini.set(
                        "Tera Term",
                        "VTPos",
                        &crate::schema::with_nth(ini.get("Tera Term", "VTPos"), 0, self.window_x),
                    );
                }
            }
            "window.y" => {
                if self.window_save_position {
                    ini.set(
                        "Tera Term",
                        "VTPos",
                        &crate::schema::with_nth(ini.get("Tera Term", "VTPos"), 1, self.window_y),
                    );
                }
            }
            "window.auto_vt_tek_switch" => {
                ini.set(
                    "Tera Term",
                    "AutoWinSwitch",
                    &if self.window_auto_vt_tek_switch {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "connection.cygwin_directory" => {
                ini.set(
                    "Tera Term",
                    "CygwinDirectory",
                    &self.connection_cygwin_directory.clone(),
                );
            }
            "log.viewer" => {
                ini.set("Tera Term", "ViewlogEditor", &self.log_viewer.clone());
            }
            "log.viewer_args" => {
                ini.set(
                    "Tera Term",
                    "ViewlogEditorArg",
                    &self.log_viewer_args.clone(),
                );
            }
            "broadcast.command_history" => {
                ini.set(
                    "Tera Term",
                    "BroadcastCommandHistory",
                    &if self.broadcast_command_history {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "broadcast.accept" => {
                ini.set(
                    "Tera Term",
                    "AcceptBroadcast",
                    &if self.broadcast_accept { "on" } else { "off" }.to_string(),
                );
            }
            "broadcast.submit_key" => {
                ini.set(
                    "Tera Term",
                    "BroadcastSubmitKey",
                    &self.broadcast_submit_key.as_ini().to_string(),
                );
            }
            "broadcast.max_history" => {
                ini.set(
                    "Tera Term",
                    "MaxBroadcatHistory",
                    &self.broadcast_max_history.to_string(),
                );
            }
            "menu.disable_accelerator_send_break" => {
                ini.set(
                    "Tera Term",
                    "DisableAcceleratorSendBreak",
                    &if self.menu_disable_accelerator_send_break {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.disable_send_break" => {
                ini.set(
                    "Tera Term",
                    "DisableMenuSendBreak",
                    &if self.menu_disable_send_break {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.disable_accelerator_duplicate" => {
                ini.set(
                    "Tera Term",
                    "DisableAcceleratorDuplicateSession",
                    &if self.menu_disable_accelerator_duplicate {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.accelerator_new_connection" => {
                ini.set(
                    "Tera Term",
                    "AcceleratorNewConnection",
                    &if self.menu_accelerator_new_connection {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.accelerator_local_shell" => {
                ini.set(
                    "Tera Term",
                    "AcceleratorCygwinConnection",
                    &if self.menu_accelerator_local_shell {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.disable_duplicate" => {
                ini.set(
                    "Tera Term",
                    "DisableMenuDuplicateSession",
                    &if self.menu_disable_duplicate {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "menu.disable_new_connection" => {
                ini.set(
                    "Tera Term",
                    "DisableMenuNewConnection",
                    &if self.menu_disable_new_connection {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "macro.wait4all" => {
                ini.set(
                    "Tera Term",
                    "Wait4allMacroCommand",
                    &if self.macro_wait4all { "on" } else { "off" }.to_string(),
                );
            }
            "window.jump_list" => {
                ini.set(
                    "Tera Term",
                    "JumpList",
                    &if self.window_jump_list { "on" } else { "off" }.to_string(),
                );
            }
            "window.corner_dontround" => {
                ini.set(
                    "Tera Term",
                    "WindowCornerDontround",
                    &if self.window_corner_dontround {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.toolbar" => {
                ini.set(
                    "Sterna",
                    "Toolbar",
                    &if self.window_toolbar { "on" } else { "off" }.to_string(),
                );
            }
            "window.panel_layout" => {
                ini.set(
                    "Sterna",
                    "PanelLayout",
                    &self.window_panel_layout.as_ini().to_string(),
                );
            }
            "window.quick_buttons" => {
                ini.set(
                    "Sterna",
                    "QuickButtons",
                    &if self.window_quick_buttons {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "window.quick_buttons_area" => {
                ini.set(
                    "Sterna",
                    "QuickButtonsArea",
                    &self.window_quick_buttons_area.as_ini().to_string(),
                );
            }
            "recent.serial_port" => {
                ini.set("Sterna", "SerialPort", &self.recent_serial_port.clone());
            }
            "recent.ssh_host" => {
                ini.set("Sterna", "SshHost", &self.recent_ssh_host.clone());
            }
            "recent.ssh_user" => {
                ini.set("Sterna", "SshUser", &self.recent_ssh_user.clone());
            }
            "recent.ssh_port" => {
                ini.set("Sterna", "SshPort", &self.recent_ssh_port.to_string());
            }
            "recent.ssh_identity" => {
                ini.set("Sterna", "SshIdentity", &self.recent_ssh_identity.clone());
            }
            "recent.ssh_legacy" => {
                ini.set(
                    "Sterna",
                    "SshLegacy",
                    &if self.recent_ssh_legacy { "on" } else { "off" }.to_string(),
                );
            }
            "recent.telnet_host" => {
                ini.set("Sterna", "TelnetHost", &self.recent_telnet_host.clone());
            }
            "recent.telnet_port" => {
                ini.set("Sterna", "TelnetPort", &self.recent_telnet_port.to_string());
            }
            "recent.telnet_mode" => {
                ini.set(
                    "Sterna",
                    "TelnetMode",
                    &self.recent_telnet_mode.as_ini().to_string(),
                );
            }
            "updates.check_on_startup" => {
                ini.set(
                    "Sterna",
                    "CheckUpdatesOnStartup",
                    &if self.updates_check_on_startup {
                        "on"
                    } else {
                        "off"
                    }
                    .to_string(),
                );
            }
            "updates.last_check" => {
                ini.set(
                    "Sterna",
                    "LastUpdateCheck",
                    &self.updates_last_check.clone(),
                );
            }
            _ => return false,
        }
        true
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
            "terminal.clear_on_resize" => if self.terminal_clear_on_resize {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.home_erase_clears_screen" => if self.terminal_home_erase_clears_screen {
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
            "terminal.answerback" => self.terminal_answerback.clone(),
            "terminal.back_wrap" => if self.terminal_back_wrap { "on" } else { "off" }.to_string(),
            "terminal.vt_compat_tab" => if self.terminal_vt_compat_tab {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.tab_stop_modify" => self.terminal_tab_stop_modify.clone(),
            "terminal.invalid_decrqss" => if self.terminal_invalid_decrqss {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.uid" => self.terminal_uid.clone(),
            "terminal.lock_uid" => if self.terminal_lock_uid { "on" } else { "off" }.to_string(),
            "terminal.auto_invoke" => if self.terminal_auto_invoke {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.max_osc_buffer" => self.terminal_max_osc_buffer.to_string(),
            "terminal.allow_wrong_sequence" => if self.terminal_allow_wrong_sequence {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "terminal.status_line_enabled" => if self.terminal_status_line_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "encoding.receive" => self.encoding_receive.as_ini().to_string(),
            "encoding.send" => self.encoding_send.as_ini().to_string(),
            "encoding.katakana_receive" => self.encoding_katakana_receive.as_ini().to_string(),
            "encoding.katakana_send" => self.encoding_katakana_send.as_ini().to_string(),
            "encoding.kanji_in" => self.encoding_kanji_in.as_ini().to_string(),
            "encoding.kanji_out" => self.encoding_kanji_out.as_ini().to_string(),
            "encoding.ctrl_in_kanji" => if self.encoding_ctrl_in_kanji {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "encoding.fixed_jis" => if self.encoding_fixed_jis { "on" } else { "off" }.to_string(),
            "encoding.fallback_cp932" => if self.encoding_fallback_cp932 {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "encoding.russian_keyboard" => self.encoding_russian_keyboard.as_ini().to_string(),
            "encoding.dec_special_direction" => {
                self.encoding_dec_special_direction.as_ini().to_string()
            }
            "encoding.unicode_to_dec_special" => self.encoding_unicode_to_dec_special.to_string(),
            "encoding.ambiguous_width" => self.encoding_ambiguous_width.to_string(),
            "encoding.emoji_override" => if self.encoding_emoji_override {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "encoding.emoji_width" => self.encoding_emoji_width.to_string(),
            "ime.enabled" => if self.ime_enabled { "on" } else { "off" }.to_string(),
            "ime.inline" => if self.ime_inline { "on" } else { "off" }.to_string(),
            "ime.cursor_related" => if self.ime_cursor_related { "on" } else { "off" }.to_string(),
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
            "keyboard.width_delimits_word" => if self.keyboard_width_delimits_word {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "keyboard.strict_mapping" => if self.keyboard_strict_mapping {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "keyboard.meta_8bit" => self.keyboard_meta_8bit.as_ini().to_string(),
            "color.normal" => crate::schema::color2_str(&self.color_normal),
            "color.bold" => crate::schema::color2_str(&self.color_bold),
            "color.blink" => crate::schema::color2_str(&self.color_blink),
            "color.underline" => crate::schema::color2_str(&self.color_underline),
            "color.reverse" => crate::schema::color2_str(&self.color_reverse),
            "color.url" => crate::schema::color2_str(&self.color_url),
            "color.url_enabled" => if self.color_url_enabled { "on" } else { "off" }.to_string(),
            "color.url_underline" => if self.color_url_underline {
                "on"
            } else {
                "off"
            }
            .to_string(),
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
            "color.ansi_palette" => self.color_ansi_palette.clone(),
            "color.xterm_256" => if self.color_xterm_256 { "on" } else { "off" }.to_string(),
            "color.aixterm_16" => if self.color_aixterm_16 { "on" } else { "off" }.to_string(),
            "color.pc_bold_16" => if self.color_pc_bold_16 { "on" } else { "off" }.to_string(),
            "color.ansi_enabled" => if self.color_ansi_enabled { "on" } else { "off" }.to_string(),
            "color.bold_font" => if self.color_bold_font { "on" } else { "off" }.to_string(),
            "color.underline_font" => if self.color_underline_font {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "color.use_text_color" => if self.color_use_text_color {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "color.use_normal_background" => if self.color_use_normal_background {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "cursor.shape" => self.cursor_shape.as_ini().to_string(),
            "cursor.nonblinking" => if self.cursor_nonblinking { "on" } else { "off" }.to_string(),
            "cursor.show_unfocused" => if self.cursor_show_unfocused {
                "on"
            } else {
                "off"
            }
            .to_string(),
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
            "window.auto_scroll_only_at_bottom" => if self.window_auto_scroll_only_at_bottom {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.scroll_threshold" => self.window_scroll_threshold.to_string(),
            "window.opacity_inactive" => self.window_opacity_inactive.to_string(),
            "window.opacity_active" => self.window_opacity_active.to_string(),
            "window.title_format" => self.window_title_format.to_string(),
            "window.title_change" => self.window_title_change.as_ini().to_string(),
            "window.title_report" => self.window_title_report.as_ini().to_string(),
            "font.quality" => self.font_quality.as_ini().to_string(),
            "font.draw_resized" => if self.font_draw_resized { "on" } else { "off" }.to_string(),
            "font.draw_api" => self.font_draw_api.as_ini().to_string(),
            "font.draw_ansi_code_page" => self.font_draw_ansi_code_page.to_string(),
            "font.space_left" => self.font_space_left.to_string(),
            "font.space_right" => self.font_space_right.to_string(),
            "font.space_top" => self.font_space_top.to_string(),
            "font.space_bottom" => self.font_space_bottom.to_string(),
            "font.scaling" => if self.font_scaling { "on" } else { "off" }.to_string(),
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
            "mouse.ctrl_disables_wheel_to_cursor" => if self.mouse_ctrl_disables_wheel_to_cursor {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "mouse.wheel_scroll_line" => self.mouse_wheel_scroll_line.to_string(),
            "mouse.clickable_url" => if self.mouse_clickable_url {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "mouse.cursor" => self.mouse_cursor.clone(),
            "url.browser" => self.url_browser.clone(),
            "url.browser_args" => self.url_browser_args.clone(),
            "url.join_split" => if self.url_join_split { "on" } else { "off" }.to_string(),
            "url.join_split_ignore_eol_char" => self.url_join_split_ignore_eol_char.clone(),
            "debug.enabled" => if self.debug_enabled { "on" } else { "off" }.to_string(),
            "debug.modes" => self.debug_modes.clone(),
            "bell.mode" => self.bell_mode.as_ini().to_string(),
            "bell.on_connect" => if self.bell_on_connect { "on" } else { "off" }.to_string(),
            "bell.visual_wait_ms" => self.bell_visual_wait_ms.to_string(),
            "bell.over_used_count" => self.bell_over_used_count.to_string(),
            "bell.over_used_time" => self.bell_over_used_time.to_string(),
            "bell.suppress_time" => self.bell_suppress_time.to_string(),
            "bell.notify_sound" => if self.bell_notify_sound { "on" } else { "off" }.to_string(),
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
            "clipboard.remote_access" => self.clipboard_remote_access.as_ini().to_string(),
            "clipboard.remote_notify" => if self.clipboard_remote_notify {
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
            "connection.telnet_auto_detect" => if self.connection_telnet_auto_detect {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.telnet_echo" => if self.connection_telnet_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.telnet_log" => if self.connection_telnet_log {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.telnet_keepalive" => self.connection_telnet_keepalive.to_string(),
            "connection.tcp_local_echo" => if self.connection_tcp_local_echo {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.tcp_cr_send" => self.connection_tcp_cr_send.as_ini().to_string(),
            "connection.confirm_disconnect" => if self.connection_confirm_disconnect {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.history_list" => if self.connection_history_list {
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
            "connection.clear_screen_on_close" => if self.connection_clear_screen_on_close {
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
            "connection.line_mode" => if self.connection_line_mode {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "proxy.type" => self.proxy_type.as_ini().to_string(),
            "proxy.host" => self.proxy_host.clone(),
            "proxy.port" => self.proxy_port.to_string(),
            "proxy.user" => self.proxy_user.clone(),
            "proxy.pass" => self.proxy_pass.clone(),
            "proxy.timeout" => self.proxy_timeout.to_string(),
            "proxy.socks_resolve" => self.proxy_socks_resolve.as_ini().to_string(),
            "proxy.telnet_hostname_prompt" => self.proxy_telnet_hostname_prompt.clone(),
            "proxy.telnet_username_prompt" => self.proxy_telnet_username_prompt.clone(),
            "proxy.telnet_password_prompt" => self.proxy_telnet_password_prompt.clone(),
            "proxy.telnet_connected_message" => self.proxy_telnet_connected_message.clone(),
            "proxy.telnet_error_message" => self.proxy_telnet_error_message.clone(),
            "proxy.debug_log" => self.proxy_debug_log.clone(),
            "macro.startup_file" => self.macro_startup_file.clone(),
            "settings.source_version" => self.settings_source_version.clone(),
            "settings.language_file" => self.settings_language_file.clone(),
            "settings.auto_backup" => if self.settings_auto_backup {
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
            "serial.rts" => self.serial_rts.to_string(),
            "serial.dtr" => self.serial_dtr.to_string(),
            "serial.clear_buffer_on_open" => if self.serial_clear_buffer_on_open {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "serial.break_time" => self.serial_break_time.to_string(),
            "serial.auto_reconnect" => if self.serial_auto_reconnect {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "serial.auto_reconnect_delay" => self.serial_auto_reconnect_delay.to_string(),
            "serial.auto_reconnect_delay_unknown_port" => {
                self.serial_auto_reconnect_delay_unknown_port.to_string()
            }
            "serial.auto_reconnect_retry_interval" => {
                self.serial_auto_reconnect_retry_interval.to_string()
            }
            "serial.auto_reconnect_retries" => self.serial_auto_reconnect_retries.to_string(),
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
            "transfer.send_filter" => self.transfer_send_filter.clone(),
            "transfer.receive_filter" => self.transfer_receive_filter.clone(),
            "transfer.scp_send_dir" => self.transfer_scp_send_dir.clone(),
            "transfer.raw_high_speed" => if self.transfer_raw_high_speed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.confirm_file_drop" => if self.transfer_confirm_file_drop {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.raw_send_delay_type" => {
                self.transfer_raw_send_delay_type.as_ini().to_string()
            }
            "transfer.raw_send_delay_tick" => self.transfer_raw_send_delay_tick.to_string(),
            "transfer.raw_send_size" => self.transfer_raw_send_size.to_string(),
            "transfer.raw_send_sequential" => if self.transfer_raw_send_sequential {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.raw_send_skip_dialog" => if self.transfer_raw_send_skip_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "transfer.raw_receive_skip_dialog" => if self.transfer_raw_receive_skip_dialog {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "printer.passthrough_delay" => self.printer_passthrough_delay.to_string(),
            "printer.passthrough_port" => self.printer_passthrough_port.clone(),
            "printer.control_sequences" => if self.printer_control_sequences {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "printer.margin_left" => self.printer_margin_left.to_string(),
            "printer.margin_right" => self.printer_margin_right.to_string(),
            "printer.margin_top" => self.printer_margin_top.to_string(),
            "printer.margin_bottom" => self.printer_margin_bottom.to_string(),
            "printer.convert_form_feed" => if self.printer_convert_form_feed {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "printer.vt_ppi_x" => self.printer_vt_ppi_x.to_string(),
            "printer.vt_ppi_y" => self.printer_vt_ppi_y.to_string(),
            "tek.x" => self.tek_x.to_string(),
            "tek.y" => self.tek_y.to_string(),
            "tek.color" => crate::schema::color2_str(&self.tek_color),
            "tek.color_emulation" => if self.tek_color_emulation {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "tek.gin_mouse_code" => self.tek_gin_mouse_code.to_string(),
            "tek.icon" => self.tek_icon.as_ini().to_string(),
            "tek.ppi_x" => self.tek_ppi_x.to_string(),
            "tek.ppi_y" => self.tek_ppi_y.to_string(),
            "window.maximized_bug_tweak" => self.window_maximized_bug_tweak.to_string(),
            "window.icon" => self.window_icon.as_ini().to_string(),
            "window.hide_title" => if self.window_hide_title { "on" } else { "off" }.to_string(),
            "window.popup_menu" => if self.window_popup_menu { "on" } else { "off" }.to_string(),
            "window.popup_menu_enabled" => if self.window_popup_menu_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.show_menu_enabled" => if self.window_show_menu_enabled {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.window_menu" => if self.window_window_menu { "on" } else { "off" }.to_string(),
            "window.save_position" => if self.window_save_position {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.x" => self.window_x.to_string(),
            "window.y" => self.window_y.to_string(),
            "window.auto_vt_tek_switch" => if self.window_auto_vt_tek_switch {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "connection.cygwin_directory" => self.connection_cygwin_directory.clone(),
            "log.viewer" => self.log_viewer.clone(),
            "log.viewer_args" => self.log_viewer_args.clone(),
            "broadcast.command_history" => if self.broadcast_command_history {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "broadcast.accept" => if self.broadcast_accept { "on" } else { "off" }.to_string(),
            "broadcast.submit_key" => self.broadcast_submit_key.as_ini().to_string(),
            "broadcast.max_history" => self.broadcast_max_history.to_string(),
            "menu.disable_accelerator_send_break" => if self.menu_disable_accelerator_send_break {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.disable_send_break" => if self.menu_disable_send_break {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.disable_accelerator_duplicate" => if self.menu_disable_accelerator_duplicate {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.accelerator_new_connection" => if self.menu_accelerator_new_connection {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.accelerator_local_shell" => if self.menu_accelerator_local_shell {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.disable_duplicate" => if self.menu_disable_duplicate {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "menu.disable_new_connection" => if self.menu_disable_new_connection {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "macro.wait4all" => if self.macro_wait4all { "on" } else { "off" }.to_string(),
            "window.jump_list" => if self.window_jump_list { "on" } else { "off" }.to_string(),
            "window.corner_dontround" => if self.window_corner_dontround {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.toolbar" => if self.window_toolbar { "on" } else { "off" }.to_string(),
            "window.panel_layout" => self.window_panel_layout.as_ini().to_string(),
            "window.quick_buttons" => if self.window_quick_buttons {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "window.quick_buttons_area" => self.window_quick_buttons_area.as_ini().to_string(),
            "recent.serial_port" => self.recent_serial_port.clone(),
            "recent.ssh_host" => self.recent_ssh_host.clone(),
            "recent.ssh_user" => self.recent_ssh_user.clone(),
            "recent.ssh_port" => self.recent_ssh_port.to_string(),
            "recent.ssh_identity" => self.recent_ssh_identity.clone(),
            "recent.ssh_legacy" => if self.recent_ssh_legacy { "on" } else { "off" }.to_string(),
            "recent.telnet_host" => self.recent_telnet_host.clone(),
            "recent.telnet_port" => self.recent_telnet_port.to_string(),
            "recent.telnet_mode" => self.recent_telnet_mode.as_ini().to_string(),
            "updates.check_on_startup" => if self.updates_check_on_startup {
                "on"
            } else {
                "off"
            }
            .to_string(),
            "updates.last_check" => self.updates_last_check.clone(),
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
            "terminal.clear_on_resize" => {
                self.terminal_clear_on_resize = crate::schema::on_off(Some(value), false)
            }
            "terminal.home_erase_clears_screen" => {
                self.terminal_home_erase_clears_screen = crate::schema::on_off(Some(value), true)
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
            "terminal.answerback" => self.terminal_answerback = value.to_string(),
            "terminal.back_wrap" => {
                self.terminal_back_wrap = crate::schema::on_off(Some(value), false)
            }
            "terminal.vt_compat_tab" => {
                self.terminal_vt_compat_tab = crate::schema::on_off(Some(value), false)
            }
            "terminal.tab_stop_modify" => self.terminal_tab_stop_modify = value.to_string(),
            "terminal.invalid_decrqss" => {
                self.terminal_invalid_decrqss = crate::schema::on_off(Some(value), false)
            }
            "terminal.uid" => self.terminal_uid = value.to_string(),
            "terminal.lock_uid" => {
                self.terminal_lock_uid = crate::schema::on_off(Some(value), true)
            }
            "terminal.auto_invoke" => {
                self.terminal_auto_invoke = crate::schema::on_off(Some(value), false)
            }
            "terminal.max_osc_buffer" => {
                self.terminal_max_osc_buffer =
                    crate::schema::int(value, self.terminal_max_osc_buffer)
            }
            "terminal.allow_wrong_sequence" => {
                self.terminal_allow_wrong_sequence = crate::schema::on_off(Some(value), false)
            }
            "terminal.status_line_enabled" => {
                self.terminal_status_line_enabled = crate::schema::on_off(Some(value), true)
            }
            "encoding.receive" => self.encoding_receive = EncodingReceive::from_ini(value),
            "encoding.send" => self.encoding_send = EncodingSend::from_ini(value),
            "encoding.katakana_receive" => {
                self.encoding_katakana_receive = EncodingKatakanaReceive::from_ini(value)
            }
            "encoding.katakana_send" => {
                self.encoding_katakana_send = EncodingKatakanaSend::from_ini(value)
            }
            "encoding.kanji_in" => self.encoding_kanji_in = EncodingKanjiIn::from_ini(value),
            "encoding.kanji_out" => self.encoding_kanji_out = EncodingKanjiOut::from_ini(value),
            "encoding.ctrl_in_kanji" => {
                self.encoding_ctrl_in_kanji = crate::schema::on_off(Some(value), true)
            }
            "encoding.fixed_jis" => {
                self.encoding_fixed_jis = crate::schema::on_off(Some(value), false)
            }
            "encoding.fallback_cp932" => {
                self.encoding_fallback_cp932 = crate::schema::on_off(Some(value), false)
            }
            "encoding.russian_keyboard" => {
                self.encoding_russian_keyboard = EncodingRussianKeyboard::from_ini(value)
            }
            "encoding.dec_special_direction" => {
                self.encoding_dec_special_direction = EncodingDecSpecialDirection::from_ini(value)
            }
            "encoding.unicode_to_dec_special" => {
                self.encoding_unicode_to_dec_special = crate::schema::word(crate::schema::int(
                    value,
                    self.encoding_unicode_to_dec_special,
                ))
            }
            "encoding.ambiguous_width" => {
                self.encoding_ambiguous_width = crate::schema::validated(
                    crate::schema::byte(crate::schema::int(value, self.encoding_ambiguous_width)),
                    1,
                    1,
                    2,
                )
            }
            "encoding.emoji_override" => {
                self.encoding_emoji_override = crate::schema::on_off(Some(value), false)
            }
            "encoding.emoji_width" => {
                self.encoding_emoji_width = crate::schema::validated(
                    crate::schema::byte(crate::schema::int(value, self.encoding_emoji_width)),
                    1,
                    1,
                    2,
                )
            }
            "ime.enabled" => self.ime_enabled = crate::schema::on_off(Some(value), true),
            "ime.inline" => self.ime_inline = crate::schema::on_off(Some(value), true),
            "ime.cursor_related" => {
                self.ime_cursor_related = crate::schema::on_off(Some(value), false)
            }
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
            "keyboard.width_delimits_word" => {
                self.keyboard_width_delimits_word = crate::schema::on_off(Some(value), true)
            }
            "keyboard.strict_mapping" => {
                self.keyboard_strict_mapping = crate::schema::on_off(Some(value), false)
            }
            "keyboard.meta_8bit" => self.keyboard_meta_8bit = KeyboardMeta8bit::from_ini(value),
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
            "color.url" => self.color_url = crate::schema::color2(Some(value), self.color_url),
            "color.url_enabled" => {
                self.color_url_enabled = crate::schema::on_off(Some(value), true)
            }
            "color.url_underline" => {
                self.color_url_underline = crate::schema::on_off(Some(value), true)
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
            "color.ansi_palette" => self.color_ansi_palette = value.to_string(),
            "color.xterm_256" => self.color_xterm_256 = crate::schema::on_off(Some(value), true),
            "color.aixterm_16" => self.color_aixterm_16 = crate::schema::on_off(Some(value), false),
            "color.pc_bold_16" => self.color_pc_bold_16 = crate::schema::on_off(Some(value), false),
            "color.ansi_enabled" => {
                self.color_ansi_enabled = crate::schema::on_off(Some(value), true)
            }
            "color.bold_font" => self.color_bold_font = crate::schema::on_off(Some(value), true),
            "color.underline_font" => {
                self.color_underline_font = crate::schema::on_off(Some(value), true)
            }
            "color.use_text_color" => {
                self.color_use_text_color = crate::schema::on_off(Some(value), false)
            }
            "color.use_normal_background" => {
                self.color_use_normal_background = crate::schema::on_off(Some(value), false)
            }
            "cursor.shape" => self.cursor_shape = CursorShape::from_ini(value),
            "cursor.nonblinking" => {
                self.cursor_nonblinking = crate::schema::on_off(Some(value), false)
            }
            "cursor.show_unfocused" => {
                self.cursor_show_unfocused = crate::schema::on_off(Some(value), true)
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
            "window.auto_scroll_only_at_bottom" => {
                self.window_auto_scroll_only_at_bottom = crate::schema::on_off(Some(value), false)
            }
            "window.scroll_threshold" => {
                self.window_scroll_threshold =
                    crate::schema::int(value, self.window_scroll_threshold)
            }
            "window.opacity_inactive" => {
                self.window_opacity_inactive =
                    crate::schema::byte(crate::schema::int(value, self.window_opacity_inactive))
            }
            "window.opacity_active" => {
                self.window_opacity_active =
                    crate::schema::byte(crate::schema::int(value, self.window_opacity_active))
            }
            "window.title_format" => {
                self.window_title_format =
                    crate::schema::word(crate::schema::int(value, self.window_title_format))
            }
            "window.title_change" => self.window_title_change = WindowTitleChange::from_ini(value),
            "window.title_report" => self.window_title_report = WindowTitleReport::from_ini(value),
            "font.quality" => self.font_quality = FontQuality::from_ini(value),
            "font.draw_resized" => {
                self.font_draw_resized = crate::schema::on_off(Some(value), true)
            }
            "font.draw_api" => self.font_draw_api = FontDrawApi::from_ini(value),
            "font.draw_ansi_code_page" => {
                self.font_draw_ansi_code_page =
                    crate::schema::int(value, self.font_draw_ansi_code_page)
            }
            "font.space_left" => {
                self.font_space_left = crate::schema::int(value, self.font_space_left)
            }
            "font.space_right" => {
                self.font_space_right = crate::schema::int(value, self.font_space_right)
            }
            "font.space_top" => {
                self.font_space_top = crate::schema::int(value, self.font_space_top)
            }
            "font.space_bottom" => {
                self.font_space_bottom = crate::schema::int(value, self.font_space_bottom)
            }
            "font.scaling" => self.font_scaling = crate::schema::on_off(Some(value), false),
            "mouse.tracking" => self.mouse_tracking = crate::schema::on_off(Some(value), true),
            "mouse.ctrl_disables_tracking" => {
                self.mouse_ctrl_disables_tracking = crate::schema::on_off(Some(value), true)
            }
            "mouse.wheel_to_cursor" => {
                self.mouse_wheel_to_cursor = crate::schema::on_off(Some(value), true)
            }
            "mouse.ctrl_disables_wheel_to_cursor" => {
                self.mouse_ctrl_disables_wheel_to_cursor = crate::schema::on_off(Some(value), true)
            }
            "mouse.wheel_scroll_line" => {
                self.mouse_wheel_scroll_line =
                    crate::schema::int(value, self.mouse_wheel_scroll_line)
            }
            "mouse.clickable_url" => {
                self.mouse_clickable_url = crate::schema::on_off(Some(value), false)
            }
            "mouse.cursor" => self.mouse_cursor = value.to_string(),
            "url.browser" => self.url_browser = value.to_string(),
            "url.browser_args" => self.url_browser_args = value.to_string(),
            "url.join_split" => self.url_join_split = crate::schema::on_off(Some(value), false),
            "url.join_split_ignore_eol_char" => {
                self.url_join_split_ignore_eol_char = value.to_string()
            }
            "debug.enabled" => self.debug_enabled = crate::schema::on_off(Some(value), false),
            "debug.modes" => self.debug_modes = value.to_string(),
            "bell.mode" => self.bell_mode = BellMode::from_ini(value),
            "bell.on_connect" => self.bell_on_connect = crate::schema::on_off(Some(value), false),
            "bell.visual_wait_ms" => {
                self.bell_visual_wait_ms =
                    crate::schema::floored(crate::schema::int(value, self.bell_visual_wait_ms), 1)
            }
            "bell.over_used_count" => {
                self.bell_over_used_count = crate::schema::int(value, self.bell_over_used_count)
            }
            "bell.over_used_time" => {
                self.bell_over_used_time = crate::schema::int(value, self.bell_over_used_time)
            }
            "bell.suppress_time" => {
                self.bell_suppress_time = crate::schema::int(value, self.bell_suppress_time)
            }
            "bell.notify_sound" => {
                self.bell_notify_sound = crate::schema::on_off(Some(value), true)
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
            "clipboard.remote_access" => {
                self.clipboard_remote_access = ClipboardRemoteAccess::from_ini(value)
            }
            "clipboard.remote_notify" => {
                self.clipboard_remote_notify = crate::schema::on_off(Some(value), true)
            }
            "connection.port_type" => {
                self.connection_port_type = ConnectionPortType::from_ini(value)
            }
            "connection.tcp_port" => {
                self.connection_tcp_port =
                    crate::schema::word(crate::schema::int(value, self.connection_tcp_port))
            }
            "connection.telnet" => {
                self.connection_telnet = crate::schema::on_off(Some(value), true)
            }
            "connection.telnet_port" => {
                self.connection_telnet_port =
                    crate::schema::word(crate::schema::int(value, self.connection_telnet_port))
            }
            "connection.telnet_binary" => {
                self.connection_telnet_binary = crate::schema::on_off(Some(value), false)
            }
            "connection.telnet_auto_detect" => {
                self.connection_telnet_auto_detect = crate::schema::on_off(Some(value), true)
            }
            "connection.telnet_echo" => {
                self.connection_telnet_echo = crate::schema::on_off(Some(value), false)
            }
            "connection.telnet_log" => {
                self.connection_telnet_log = crate::schema::on_off(Some(value), false)
            }
            "connection.telnet_keepalive" => {
                self.connection_telnet_keepalive =
                    crate::schema::word(crate::schema::int(value, self.connection_telnet_keepalive))
            }
            "connection.tcp_local_echo" => {
                self.connection_tcp_local_echo = crate::schema::on_off(Some(value), false)
            }
            "connection.tcp_cr_send" => {
                self.connection_tcp_cr_send = ConnectionTcpCrSend::from_ini(value)
            }
            "connection.confirm_disconnect" => {
                self.connection_confirm_disconnect = crate::schema::on_off(Some(value), true)
            }
            "connection.history_list" => {
                self.connection_history_list = crate::schema::on_off(Some(value), false)
            }
            "connection.term_type" => self.connection_term_type = value.to_string(),
            "connection.terminal_speed" => self.connection_terminal_speed = value.to_string(),
            "connection.auto_win_close" => {
                self.connection_auto_win_close = crate::schema::on_off(Some(value), true)
            }
            "connection.clear_screen_on_close" => {
                self.connection_clear_screen_on_close = crate::schema::on_off(Some(value), false)
            }
            "connection.timeout" => {
                self.connection_timeout = crate::schema::int(value, self.connection_timeout)
            }
            "connection.host_dialog_on_startup" => {
                self.connection_host_dialog_on_startup = crate::schema::on_off(Some(value), true)
            }
            "connection.line_mode" => {
                self.connection_line_mode = crate::schema::on_off(Some(value), true)
            }
            "proxy.type" => self.proxy_type = ProxyType::from_ini(value),
            "proxy.host" => self.proxy_host = value.to_string(),
            "proxy.port" => {
                self.proxy_port = crate::schema::word(crate::schema::int(value, self.proxy_port))
            }
            "proxy.user" => self.proxy_user = value.to_string(),
            "proxy.pass" => self.proxy_pass = value.to_string(),
            "proxy.timeout" => {
                self.proxy_timeout = crate::schema::ranged(
                    crate::schema::int(value, self.proxy_timeout),
                    10,
                    0,
                    2147483647,
                )
            }
            "proxy.socks_resolve" => self.proxy_socks_resolve = ProxySocksResolve::from_ini(value),
            "proxy.telnet_hostname_prompt" => self.proxy_telnet_hostname_prompt = value.to_string(),
            "proxy.telnet_username_prompt" => self.proxy_telnet_username_prompt = value.to_string(),
            "proxy.telnet_password_prompt" => self.proxy_telnet_password_prompt = value.to_string(),
            "proxy.telnet_connected_message" => {
                self.proxy_telnet_connected_message = value.to_string()
            }
            "proxy.telnet_error_message" => self.proxy_telnet_error_message = value.to_string(),
            "proxy.debug_log" => self.proxy_debug_log = value.to_string(),
            "macro.startup_file" => self.macro_startup_file = value.to_string(),
            "settings.source_version" => self.settings_source_version = value.to_string(),
            "settings.language_file" => self.settings_language_file = value.to_string(),
            "settings.auto_backup" => {
                self.settings_auto_backup = crate::schema::on_off(Some(value), true)
            }
            "serial.com_port" => {
                self.serial_com_port =
                    crate::schema::word(crate::schema::int(value, self.serial_com_port))
            }
            "serial.baud" => self.serial_baud = crate::schema::int(value, self.serial_baud),
            "serial.data_bits" => self.serial_data_bits = SerialDataBits::from_ini(value),
            "serial.parity" => self.serial_parity = SerialParity::from_ini(value),
            "serial.stop_bits" => self.serial_stop_bits = SerialStopBits::from_ini(value),
            "serial.flow" => self.serial_flow = SerialFlow::from_ini(value),
            "serial.delay_per_char" => {
                self.serial_delay_per_char =
                    crate::schema::word(crate::schema::int(value, self.serial_delay_per_char))
            }
            "serial.delay_per_line" => {
                self.serial_delay_per_line =
                    crate::schema::word(crate::schema::int(value, self.serial_delay_per_line))
            }
            "serial.wait_com" => self.serial_wait_com = crate::schema::on_off(Some(value), false),
            "serial.max_com_port" => {
                self.serial_max_com_port = crate::schema::clamped(
                    crate::schema::word(crate::schema::int(value, self.serial_max_com_port)),
                    4,
                    4096,
                )
            }
            "serial.rts" => self.serial_rts = crate::schema::int(value, self.serial_rts),
            "serial.dtr" => self.serial_dtr = crate::schema::int(value, self.serial_dtr),
            "serial.clear_buffer_on_open" => {
                self.serial_clear_buffer_on_open = crate::schema::on_off(Some(value), true)
            }
            "serial.break_time" => {
                self.serial_break_time = crate::schema::int(value, self.serial_break_time)
            }
            "serial.auto_reconnect" => {
                self.serial_auto_reconnect = crate::schema::on_off(Some(value), true)
            }
            "serial.auto_reconnect_delay" => {
                self.serial_auto_reconnect_delay =
                    crate::schema::word(crate::schema::int(value, self.serial_auto_reconnect_delay))
            }
            "serial.auto_reconnect_delay_unknown_port" => {
                self.serial_auto_reconnect_delay_unknown_port = crate::schema::word(
                    crate::schema::int(value, self.serial_auto_reconnect_delay_unknown_port),
                )
            }
            "serial.auto_reconnect_retry_interval" => {
                self.serial_auto_reconnect_retry_interval = crate::schema::word(crate::schema::int(
                    value,
                    self.serial_auto_reconnect_retry_interval,
                ))
            }
            "serial.auto_reconnect_retries" => {
                self.serial_auto_reconnect_retries = crate::schema::word(crate::schema::int(
                    value,
                    self.serial_auto_reconnect_retries,
                ))
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
                self.log_rotate_size_type =
                    crate::schema::word(crate::schema::int(value, self.log_rotate_size_type))
            }
            "log.rotate_step" => {
                self.log_rotate_step =
                    crate::schema::word(crate::schema::int(value, self.log_rotate_step))
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
            "transfer.send_filter" => self.transfer_send_filter = value.to_string(),
            "transfer.receive_filter" => self.transfer_receive_filter = value.to_string(),
            "transfer.scp_send_dir" => self.transfer_scp_send_dir = value.to_string(),
            "transfer.raw_high_speed" => {
                self.transfer_raw_high_speed = crate::schema::on_off(Some(value), true)
            }
            "transfer.confirm_file_drop" => {
                self.transfer_confirm_file_drop = crate::schema::on_off(Some(value), true)
            }
            "transfer.raw_send_delay_type" => {
                self.transfer_raw_send_delay_type = TransferRawSendDelayType::from_ini(value)
            }
            "transfer.raw_send_delay_tick" => {
                self.transfer_raw_send_delay_tick = crate::schema::word(crate::schema::int(
                    value,
                    self.transfer_raw_send_delay_tick,
                ))
            }
            "transfer.raw_send_size" => {
                self.transfer_raw_send_size = crate::schema::int(value, self.transfer_raw_send_size)
            }
            "transfer.raw_send_sequential" => {
                self.transfer_raw_send_sequential = crate::schema::on_off(Some(value), false)
            }
            "transfer.raw_send_skip_dialog" => {
                self.transfer_raw_send_skip_dialog = crate::schema::on_off(Some(value), false)
            }
            "transfer.raw_receive_skip_dialog" => {
                self.transfer_raw_receive_skip_dialog = crate::schema::on_off(Some(value), false)
            }
            "printer.passthrough_delay" => {
                self.printer_passthrough_delay =
                    crate::schema::word(crate::schema::int(value, self.printer_passthrough_delay))
            }
            "printer.passthrough_port" => self.printer_passthrough_port = value.to_string(),
            "printer.control_sequences" => {
                self.printer_control_sequences = crate::schema::on_off(Some(value), false)
            }
            "printer.margin_left" => {
                self.printer_margin_left = crate::schema::int(value, self.printer_margin_left)
            }
            "printer.margin_right" => {
                self.printer_margin_right = crate::schema::int(value, self.printer_margin_right)
            }
            "printer.margin_top" => {
                self.printer_margin_top = crate::schema::int(value, self.printer_margin_top)
            }
            "printer.margin_bottom" => {
                self.printer_margin_bottom = crate::schema::int(value, self.printer_margin_bottom)
            }
            "printer.convert_form_feed" => {
                self.printer_convert_form_feed = crate::schema::on_off(Some(value), false)
            }
            "printer.vt_ppi_x" => {
                self.printer_vt_ppi_x = crate::schema::int(value, self.printer_vt_ppi_x)
            }
            "printer.vt_ppi_y" => {
                self.printer_vt_ppi_y = crate::schema::int(value, self.printer_vt_ppi_y)
            }
            "tek.x" => self.tek_x = crate::schema::int(value, self.tek_x),
            "tek.y" => self.tek_y = crate::schema::int(value, self.tek_y),
            "tek.color" => self.tek_color = crate::schema::color2(Some(value), self.tek_color),
            "tek.color_emulation" => {
                self.tek_color_emulation = crate::schema::on_off(Some(value), false)
            }
            "tek.gin_mouse_code" => {
                self.tek_gin_mouse_code = crate::schema::int(value, self.tek_gin_mouse_code)
            }
            "tek.icon" => self.tek_icon = TekIcon::from_ini(value),
            "tek.ppi_x" => self.tek_ppi_x = crate::schema::int(value, self.tek_ppi_x),
            "tek.ppi_y" => self.tek_ppi_y = crate::schema::int(value, self.tek_ppi_y),
            "window.maximized_bug_tweak" => {
                self.window_maximized_bug_tweak = crate::schema::word(crate::schema::int_alias(
                    Some(value),
                    self.window_maximized_bug_tweak,
                    "on",
                    2,
                ))
            }
            "window.icon" => self.window_icon = WindowIcon::from_ini(value),
            "window.hide_title" => {
                self.window_hide_title = crate::schema::on_off(Some(value), false)
            }
            "window.popup_menu" => {
                self.window_popup_menu = crate::schema::on_off(Some(value), false)
            }
            "window.popup_menu_enabled" => {
                self.window_popup_menu_enabled = crate::schema::on_off(Some(value), true)
            }
            "window.show_menu_enabled" => {
                self.window_show_menu_enabled = crate::schema::on_off(Some(value), true)
            }
            "window.window_menu" => {
                self.window_window_menu = crate::schema::on_off(Some(value), true)
            }
            "window.save_position" => {
                self.window_save_position = crate::schema::on_off(Some(value), false)
            }
            "window.x" => self.window_x = crate::schema::int(value, self.window_x),
            "window.y" => self.window_y = crate::schema::int(value, self.window_y),
            "window.auto_vt_tek_switch" => {
                self.window_auto_vt_tek_switch = crate::schema::on_off(Some(value), false)
            }
            "connection.cygwin_directory" => self.connection_cygwin_directory = value.to_string(),
            "log.viewer" => self.log_viewer = value.to_string(),
            "log.viewer_args" => self.log_viewer_args = value.to_string(),
            "broadcast.command_history" => {
                self.broadcast_command_history = crate::schema::on_off(Some(value), false)
            }
            "broadcast.accept" => self.broadcast_accept = crate::schema::on_off(Some(value), true),
            "broadcast.submit_key" => {
                self.broadcast_submit_key = BroadcastSubmitKey::from_ini(value)
            }
            "broadcast.max_history" => {
                self.broadcast_max_history =
                    crate::schema::word(crate::schema::int(value, self.broadcast_max_history))
            }
            "menu.disable_accelerator_send_break" => {
                self.menu_disable_accelerator_send_break = crate::schema::on_off(Some(value), false)
            }
            "menu.disable_send_break" => {
                self.menu_disable_send_break = crate::schema::on_off(Some(value), false)
            }
            "menu.disable_accelerator_duplicate" => {
                self.menu_disable_accelerator_duplicate = crate::schema::on_off(Some(value), false)
            }
            "menu.accelerator_new_connection" => {
                self.menu_accelerator_new_connection = crate::schema::on_off(Some(value), true)
            }
            "menu.accelerator_local_shell" => {
                self.menu_accelerator_local_shell = crate::schema::on_off(Some(value), true)
            }
            "menu.disable_duplicate" => {
                self.menu_disable_duplicate = crate::schema::on_off(Some(value), false)
            }
            "menu.disable_new_connection" => {
                self.menu_disable_new_connection = crate::schema::on_off(Some(value), false)
            }
            "macro.wait4all" => self.macro_wait4all = crate::schema::on_off(Some(value), false),
            "window.jump_list" => self.window_jump_list = crate::schema::on_off(Some(value), true),
            "window.corner_dontround" => {
                self.window_corner_dontround = crate::schema::on_off(Some(value), false)
            }
            "window.toolbar" => self.window_toolbar = crate::schema::on_off(Some(value), true),
            "window.panel_layout" => self.window_panel_layout = WindowPanelLayout::from_ini(value),
            "window.quick_buttons" => {
                self.window_quick_buttons = crate::schema::on_off(Some(value), true)
            }
            "window.quick_buttons_area" => {
                self.window_quick_buttons_area = WindowQuickButtonsArea::from_ini(value)
            }
            "recent.serial_port" => self.recent_serial_port = value.to_string(),
            "recent.ssh_host" => self.recent_ssh_host = value.to_string(),
            "recent.ssh_user" => self.recent_ssh_user = value.to_string(),
            "recent.ssh_port" => {
                self.recent_ssh_port =
                    crate::schema::word(crate::schema::int(value, self.recent_ssh_port))
            }
            "recent.ssh_identity" => self.recent_ssh_identity = value.to_string(),
            "recent.ssh_legacy" => {
                self.recent_ssh_legacy = crate::schema::on_off(Some(value), false)
            }
            "recent.telnet_host" => self.recent_telnet_host = value.to_string(),
            "recent.telnet_port" => {
                self.recent_telnet_port =
                    crate::schema::word(crate::schema::int(value, self.recent_telnet_port))
            }
            "recent.telnet_mode" => self.recent_telnet_mode = RecentTelnetMode::from_ini(value),
            "updates.check_on_startup" => {
                self.updates_check_on_startup = crate::schema::on_off(Some(value), true)
            }
            "updates.last_check" => self.updates_last_check = value.to_string(),
            _ => return false,
        }
        self.normalize();
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
        name: "terminal.clear_on_resize",
        page: "terminal",
        section: "Tera Term",
        key: "ClearOnResize",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TAB_GENERAL_CLEAR_ON_RESIZE"),
        doc: "`ttset.c:1676`, part of `TermFlag` — the same trap as `ColorFlag`. Whether changing the terminal's size scrolls the page away and homes the cursor.  Off, so the screen survives a resize and what moves is which lines the page covers (`buffer.c:5001`). Two things it does that its name does not say: with it **on** the clear happens even when the size did not change, because `BuffScroll` sits outside the `if (size changed)` block (`:5028`); and DECCOLM tests it and skips its own clear, since `ChangeTerminalSize` has already done one (`vtterm.c:2925`).",
    },
    Field {
        name: "terminal.home_erase_clears_screen",
        page: "terminal",
        section: "Tera Term",
        key: "ScrollWindowClearScreen",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1444`. Whether `ED 0` with the cursor already at the home position is treated as `ED 2`.  **It is not the gate on `ED 2` its name suggests.** `CSScreenErase`'s `case 2` calls `BuffClearScreen` whatever this says (`vtterm.c:1740`), and a clear screen is a scroll into the history rather than an erase either way; what the key decides is only whether the `ESC [ H ESC [ J` pair — which many programs send in place of `ESC [ 2 J` — takes that path too. Turning it off leaves those programs erasing to the end of the screen, which is what the sequence literally asks for and loses the screen out of the history.",
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
        name: "terminal.answerback",
        page: "terminal",
        section: "Tera Term",
        key: "Answerback",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:663`. What the terminal sends when the host asks it who it is with ENQ (`0x05`) — `vtterm.c:1076` writes it with `CommBinaryOut`, so the bytes go out raw, with no CR translation and no local echo.  **The value is a hex string, not the answer itself.** `Hex2Str` (`ttlib.c:406`) copies bytes through and reads `$` as the lead of a two-digit escape, so `Answerback=VT100$0D` is nine bytes ending in a CR. Three quirks come with it, all from the same loop: `ConvHexChar` answers **0** for a digit that is not hex, so `$ZZ` is a NUL; a `$` with fewer than two digits behind it borrows `'0'` for each one it is missing, so a trailing `$` is also a NUL and `$A` is `0xA0`; and the result is arbitrary bytes rather than text, which is why this is held as the file's own spelling and decoded at the point of use rather than stored decoded.  It is also the one setting in this file another setting **overwrites**: `ttset.c:1132` replaces it outright with B Plus's five-byte activation string when `BPAuto=on`, a hundred lines after reading it, so a file that sets both loses this one without a word. See `transfer.bplus_auto`.",
    },
    Field {
        name: "terminal.back_wrap",
        page: "terminal",
        section: "Tera Term",
        key: "BackWrap",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1108`, `TF_BACKWRAP`. Whether a BS on the left margin steps back to the *previous* line rather than stopping dead. Off, `BackSpace` (`vtterm.c:662`) has an arm that does nothing at all; on, it moves to `CursorRightM` of the row above — the right *margin*, so a terminal with DECSLRM set lands inside the margins rather than at the last column.  Only the arm that moves taps a BS into the log and the macro language's received-line buffer, so this key also decides whether a script's `wait` ever sees one at column zero.",
    },
    Field {
        name: "terminal.vt_compat_tab",
        page: "terminal",
        section: "Tera Term",
        key: "VTCompatTab",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1343`, and it is read in two places that do not sound like the same setting. Off — which is how it ships — a tab is *like a printed character*: `Tab` (`vtterm.c:713`) takes a pending wrap first, so a tab arriving on a full line breaks the line before tabbing, and `CursorForwardTab` (`buffer.c:5228`) arms the pending wrap when it runs out of stops. On, both stop happening and a tab is only ever a cursor move, which is what a real VT does.  CHT (`CSI Ps I`) is unaffected by the first half: it calls `CursorForwardTab` directly and never sees the wrap.",
    },
    Field {
        name: "terminal.tab_stop_modify",
        page: "terminal",
        section: "Tera Term",
        key: "TabStopModifySequence",
        kind: Kind::Str,
        default: "on",
        label: None,
        doc: "`ttset.c:1717`, `ts.TabStopFlag`. Which sequences a *host* is allowed to move the tab stops with, as a comma list — `HTS7` is `ESC H`, `HTS8` is the 8-bit C1 at 0x88, `TBC0` is `CSI 0 g` and `TBC3` is `CSI 3 g`; `HTS` and `TBC` are each the pair. `on`/`all` and `off`/`none` assign the whole word.  A `string` rather than a type of its own, for the same reason `terminal.iso2022_shifts` is one: it is a flag list and the parse lives beside the bits it names. Unlike that key, this one starts from `TABF_NONE` only in the *list* arm and the default applies whenever the value is absent **or** matches `on`.",
    },
    Field {
        name: "terminal.invalid_decrqss",
        page: "terminal",
        section: "Tera Term",
        key: "UseInvalidDECRQSSResponse",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1756`, `TF_INVALIDDECRPSS`, and upstream's own comment is \"(for testing)\". `RequestStatusString` (`vtterm.c:4400`) flips the leading digit of the reply it was about to send, so a valid request answers `0$r` — \"I do not recognise this\" — and an invalid one answers `1$r` with an empty body. It is there to exercise the *host's* error handling, and the only setting in the terminal whose purpose is to lie.",
    },
    Field {
        name: "terminal.uid",
        page: "terminal",
        section: "Tera Term",
        key: "TerminalUID",
        kind: Kind::Str,
        default: "FFFFFFFF",
        label: None,
        doc: "`ttset.c:1688`. The eight hex digits the tertiary DA (`CSI = c`) answers with, in a `DCS ! | … ST` (`vtterm.c:2829`).  **Validated on read and the fallback is the default**: eight characters, every one a hex digit, upper-cased in place — anything else, including a nine-digit value, becomes `FFFFFFFF`. That is `ts.BSKey`'s shape rather than an enum's, so it is held as a string and checked at the point of use; a file keeps whatever it wrote, and the terminal answers with the valid form.",
    },
    Field {
        name: "terminal.lock_uid",
        page: "terminal",
        section: "Tera Term",
        key: "LockTUID",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1711`, `TF_LOCKTUID`, and the default is **on** — so DECSTUI (`DCS ! { … ST`) is refused as shipped. `vtterm.c:4565` is the whole of it: with the key off a host may set the UID above, with it on the sequence is read and dropped. The same eight-hex-digit validation applies there as applies to the file, in a second place.",
    },
    Field {
        name: "terminal.auto_invoke",
        page: "terminal",
        section: "Tera Term",
        key: "AutoInvoke",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1101`, `TF_AUTOINVOKE`. Whether designating a character set into G0 also invokes G0 into GL, so `ESC ( B` puts ASCII back on the wire's own bytes without an SI.  Two things about `ESCSBCSSelect` (`vtterm.c:1409`) that reading the name would not give. The invoke is **outside** the switch that handled the final character, so an unrecognised designation like `ESC ( Z` still invokes; and it is *not* gated on `ts.ISO2022Flag`, unlike every other locking shift in the parser, so a terminal with `ISO2022ShiftFunction=off` still performs this one.",
    },
    Field {
        name: "terminal.max_osc_buffer",
        page: "terminal",
        section: "Tera Term",
        key: "MaxOSCBufferSize",
        kind: Kind::Int,
        default: "4096",
        label: None,
        doc: "`ttset.c:1789`. The ceiling on the buffer an OSC or DCS string is collected into — `vtterm.c:5265` doubles the buffer from `sizeof(ts.Title)` up to this and then silently **drops** every further byte, so a title longer than this arrives truncated and the sequence still terminates normally.  It is the only bound on a string a host controls the length of, which is why it is not merely cosmetic.",
    },
    Field {
        name: "terminal.allow_wrong_sequence",
        page: "terminal",
        section: "Tera Term",
        key: "AllowWrongSequence",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1078`, `TF_ALLOWWRONGSEQUENCE`, default off. Its remaining upstream consumer accepts a non-regular Japanese coding sequence (`coding_pp.cpp:247`) and permits the otherwise-refused `KanjiOut=H`; CJK is deferred here, so the compatibility switch round-trips and acts on nothing for now.",
    },
    Field {
        name: "terminal.status_line_enabled",
        page: "terminal",
        section: "Tera Term",
        key: "EnableStatusLine",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1187`, `TF_ENABLESLINE`, default **on**. Gates DECSSDT/DECSASD (`vtterm.c:3827`). `tt-grid` deliberately has no third status-line buffer yet, so the key is retained until that parser feature exists.",
    },
    Field {
        name: "encoding.receive",
        page: "encoding",
        section: "Tera Term",
        key: "KanjiReceive",
        kind: Kind::Enum(&["UTF-8", "ISO8859-1", "ISO8859-2", "ISO8859-3", "ISO8859-4", "ISO8859-5", "ISO8859-6", "ISO8859-7", "ISO8859-8", "ISO8859-9", "ISO8859-10", "ISO8859-11", "ISO8859-13", "ISO8859-14", "ISO8859-15", "ISO8859-16", "SJIS", "EUC-JP", "JIS", "Windows-1251", "KOI8-R", "CP866", "KS5601", "GB2312", "BIG5", "CP437", "CP737", "CP775", "CP850", "CP852", "CP855", "CP857", "CP860", "CP861", "CP862", "CP863", "CP864", "CP865", "CP869", "CP874", "CP1250", "CP1252", "CP1253", "CP1254", "CP1255", "CP1256", "CP1257", "CP1258"]),
        default: "UTF-8",
        label: Some("DLG_TERMK_KANJI"),
        doc: "`ttset.c:670` calls `GetKanjiCodeFromStr`, whose table is compared with case-sensitive `strcmp` (`ttlib_charset.cpp:147`). Empty and unknown values become UTF-8. Aliases are ordered with the spelling `GetKanjiCodeStr` writes first: EUC is written as `EUC-JP`, and CP1251 as `Windows-1251`.",
    },
    Field {
        name: "encoding.send",
        page: "encoding",
        section: "Tera Term",
        key: "KanjiSend",
        kind: Kind::Enum(&["UTF-8", "ISO8859-1", "ISO8859-2", "ISO8859-3", "ISO8859-4", "ISO8859-5", "ISO8859-6", "ISO8859-7", "ISO8859-8", "ISO8859-9", "ISO8859-10", "ISO8859-11", "ISO8859-13", "ISO8859-14", "ISO8859-15", "ISO8859-16", "SJIS", "EUC-JP", "JIS", "Windows-1251", "KOI8-R", "CP866", "KS5601", "GB2312", "BIG5", "CP437", "CP737", "CP775", "CP850", "CP852", "CP855", "CP857", "CP860", "CP861", "CP862", "CP863", "CP864", "CP865", "CP869", "CP874", "CP1250", "CP1252", "CP1253", "CP1254", "CP1255", "CP1256", "CP1257", "CP1258"]),
        default: "UTF-8",
        label: Some("DLG_TERMK_KANJISEND"),
        doc: "`ttset.c:683`, the same exact table and UTF-8 fallback for transmitted text.",
    },
    Field {
        name: "encoding.katakana_receive",
        page: "encoding",
        section: "Tera Term",
        key: "KatakanaReceive",
        kind: Kind::Enum(&["7", "8"]),
        default: "8",
        label: None,
        doc: "`ttset.c:675`; only the literal `7` selects seven-bit JIS Katakana, and the empty default and every other value select eight-bit.",
    },
    Field {
        name: "encoding.katakana_send",
        page: "encoding",
        section: "Tera Term",
        key: "KatakanaSend",
        kind: Kind::Enum(&["7", "8"]),
        default: "8",
        label: None,
        doc: "`ttset.c:688`, the transmit-side copy of the same switch.",
    },
    Field {
        name: "encoding.kanji_in",
        page: "encoding",
        section: "Tera Term",
        key: "KanjiIn",
        kind: Kind::Enum(&["@", "B"]),
        default: "B",
        label: Some("DLG_TERM_KIN"),
        doc: "`ttset.c:696` and `makeoutputstring.cpp:72`; exact `@` selects the 1978 JIS designation and exact `B` plus everything else selects the 1983 one.",
    },
    Field {
        name: "encoding.kanji_out",
        page: "encoding",
        section: "Tera Term",
        key: "KanjiOut",
        kind: Kind::Enum(&["B", "J", "H"]),
        default: "J",
        label: Some("DLG_TERM_KOUT"),
        doc: "`ttset.c:701` and `makeoutputstring.cpp:83`; exact `B`, `J` and `H` are the three designations and an absent or unknown value takes `J`.",
    },
    Field {
        name: "encoding.ctrl_in_kanji",
        page: "encoding",
        section: "Tera Term",
        key: "CtrlInKanji",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1158`, `TF_CTRLINKANJI`, default on. It lets C0 controls interrupt a two-byte Japanese character; retained until those decoders exist here.",
    },
    Field {
        name: "encoding.fixed_jis",
        page: "encoding",
        section: "Tera Term",
        key: "FixedJIS",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1194`, `TF_FIXEDJIS`, default off. The writer does not emit this legacy hand-edited key, but a changed generated setting must still be able to.",
    },
    Field {
        name: "encoding.fallback_cp932",
        page: "encoding",
        section: "Tera Term",
        key: "FallbackToCP932",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1957`, default off. Upstream's experimental fallback decodes bytes that are invalid UTF-8 as CP932; the decoder is part of the deferred CJK work.",
    },
    Field {
        name: "encoding.russian_keyboard",
        page: "encoding",
        section: "Tera Term",
        key: "RussKeyb",
        kind: Kind::Enum(&["KOI8-R", "Windows"]),
        default: "Windows",
        label: None,
        doc: "`ttset.c:911`. Only `KOI8-R`, case-insensitively, selects that keyboard map; the `Windows` spelling and everything else select the Windows map.",
    },
    Field {
        name: "encoding.dec_special_direction",
        page: "encoding",
        section: "Tera Term",
        key: "DecSpMappingDir",
        kind: Kind::Enum(&["0", "1", "2"]),
        default: "2",
        label: None,
        doc: "`ttset.c:1537`. Values 0, 1 and 2 select Unicode-to-DEC, DEC-to-Unicode and no mapping. An out-of-range integer falls back to the first, not to the file default of 2, so `*` records the separate else arm.",
    },
    Field {
        name: "encoding.unicode_to_dec_special",
        page: "encoding",
        section: "Tera Term",
        key: "UnicodeToDecSpMapping",
        kind: Kind::IntWord,
        default: "3",
        label: None,
        doc: "`ttset.c:1547`, assigned to a `WORD`. The low three bits choose the symbol groups mapped to DEC special graphics; unknown high bits survive a save.",
    },
    Field {
        name: "encoding.ambiguous_width",
        page: "encoding",
        section: "Tera Term",
        key: "UnicodeAmbiguousWidth",
        kind: Kind::IntRange(1, 2),
        default: "1",
        label: None,
        doc: "`ttset.c:1965`. Only 1 and 2 are valid, and either invalid end takes `GetDefaultUnicodeWidth()`, which is 1 in this upstream tree.",
    },
    Field {
        name: "encoding.emoji_override",
        page: "encoding",
        section: "Tera Term",
        key: "UnicodeEmojiOverride",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1969`, default off. When enabled, Emoji-property characters use the explicit width below instead of their East Asian Width property.",
    },
    Field {
        name: "encoding.emoji_width",
        page: "encoding",
        section: "Tera Term",
        key: "UnicodeEmojiWidth",
        kind: Kind::IntRange(1, 2),
        default: "1",
        label: None,
        doc: "`ttset.c:1970`, the same accept-only-1-or-2 rule and default 1 as ambiguous width. Width policy remains deliberately deferred with CJK.",
    },
    Field {
        name: "ime.enabled",
        page: "ime",
        section: "Tera Term",
        key: "IME",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1198`, default on. This is Win32 IME enablement; Qt input-method integration remains deferred with CJK, so the compatibility key is carried.",
    },
    Field {
        name: "ime.inline",
        page: "ime",
        section: "Tera Term",
        key: "IMEInline",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1201`, default on. Selects inline composition in upstream and acts on nothing until the input-method surface exists here.",
    },
    Field {
        name: "ime.cursor_related",
        page: "ime",
        section: "Tera Term",
        key: "IMERelatedCursor",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TAB_GENERAL_CURSOR_CHANGE_IME"),
        doc: "`ttset.c:1684`, `WF_IMECURSORCHANGE`, default off. Changes the cursor while an IME composition is active; the setting is retained with that deferred UI.",
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
        name: "keyboard.width_delimits_word",
        page: "keyboard",
        section: "Tera Term",
        key: "DelimDBCS",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1176`. When a word starts on a non-delimiter, changing between a one-cell and a multi-cell character ends it (`buffer.c:4479`). On by default, so double-clicking `abc北京def` selects one of the three width runs rather than the whole string. A run that starts on a delimiter is unaffected: upstream's other arm still groups consecutive copies of that same character.",
    },
    Field {
        name: "keyboard.strict_mapping",
        page: "keyboard",
        section: "Tera Term",
        key: "StrictKeyMapping",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1598`, default off. With it on, a PC key that has no explicit `KEYBOARD.CNF` entry sends nothing instead of falling back to Tera Term's built-in VT sequence (`keyboard.c:960` onward).",
    },
    Field {
        name: "keyboard.meta_8bit",
        page: "keyboard",
        section: "Tera Term",
        key: "Meta8Bit",
        kind: Kind::Enum(&["off", "raw", "text"]),
        default: "off",
        label: None,
        doc: "`ttset.c:1643`. What an enabled Meta key does to a one-byte character: `raw` sets bit 7 on the byte, `text` sets U+0080 on the character before encoding, and `off` prefixes ESC. `on` is a read-only alias of `raw`; the writer emits `raw` (`ttset.c:3012`).",
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
        name: "color.url",
        page: "color",
        section: "Tera Term",
        key: "URLColor",
        kind: Kind::Color2,
        default: "0,255,0,255,255,255",
        label: Some("DLG_WIN_URL"),
        doc: "`ttset.c:775`. The pair applied to a cell carrying `AttrURL`: green on white as shipped. URL is its own attribute rather than an SGR colour, and an explicit SGR foreground or background still wins over the corresponding half later in `vtdisp.c:GetDrawAttr` (`:2499`, then `:2522`).",
    },
    Field {
        name: "color.url_enabled",
        page: "color",
        section: "Tera Term",
        key: "EnableURLColor",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TAB_VISUAL_URL_COLOR"),
        doc: "`ttset.c:776`, the `CF_URLCOLOR` bit. This gates only the URL colour pair; URL detection and `URLUnderline` are independent, so turning it off leaves a detected URL underlined in the ordinary text colour.",
    },
    Field {
        name: "color.url_underline",
        page: "color",
        section: "Tera Term",
        key: "URLUnderline",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TAB_VISUAL_URL_FONT"),
        doc: "`ttset.c:780`, the `FF_URLUNDERLINE` bit. Independent of both the URL colour and whether a double-click is allowed to open one.",
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
        name: "color.ansi_palette",
        page: "color",
        section: "Tera Term",
        key: "ANSIColor",
        kind: Kind::Str,
        default: "0,0,0,0,1,255,0,0,2,0,255,0,3,255,255,0,4,0,0,255,5,255,0,255,6,0,255,255,7,255,255,255,8,128,128,128,9,128,0,0,10,0,128,0,11,128,128,0,12,0,0,128,13,128,0,128,14,0,128,128,15,192,192,192",
        label: None,
        doc: "`ttset.c:797`. Sixteen `(legacy index,r,g,b)` groups in one `MAX_PATH` buffer. This stays a string because upstream accepts partial lists, repeated and wrapped IDs, and byte-wrapped channels; the behavioral parse belongs by the terminal palette in `tt-session`. The default is upstream's exact colour values with only its alignment whitespace removed.",
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
        name: "color.bold_font",
        page: "color",
        section: "Tera Term",
        key: "EnableBold",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TAB_VISUAL_BOLD_FONT"),
        doc: "`ttset.c:868`, `FF_BOLD`. Whether SGR 1 selects a bold *font*, independently of `color.bold_enabled`, which decides whether it selects the bold colour pair. Both ship on, but either may be disabled alone — a bold cell can be blue in a regular face or bold in the normal text colour.",
    },
    Field {
        name: "color.underline_font",
        page: "color",
        section: "Tera Term",
        key: "UnderlineAttrFont",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TAB_VISUAL_UNDERLINE_FONT"),
        doc: "`ttset.c:782`, `FF_UNDERLINE`. The font half of SGR 4, independent of `color.underline_enabled`'s magenta colour pair. Off keeps the underline attribute in the grid and changes only how it is drawn.",
    },
    Field {
        name: "color.use_text_color",
        page: "color",
        section: "Tera Term",
        key: "UseTextColor",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1335`, `CF_USETEXTCOLOR`. A compatibility escape for applications which assume a black terminal: after applying explicit SGR colours, `GetDrawAttr` (`vtdisp.c:2542`) replaces an invisible same-colour pair with the configured normal pair when both indices match and the foreground is black (0), white (7), or bright white (15). Under reverse video it uses the configured reverse pair — even when `color.reverse_enabled` is off.",
    },
    Field {
        name: "color.use_normal_background",
        page: "color",
        section: "Tera Term",
        key: "UseNormalBGColor",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_WIN_ALWAYSBG"),
        doc: "`ttset.c:1561`. Bold, blink, underline and URL colours are pairs of their own; on, their configured background half is ignored and the normal text background is used instead. Reverse swaps that normal background into the foreground (`vtdisp.c:2453`).",
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
        name: "cursor.show_unfocused",
        page: "cursor",
        section: "Tera Term",
        key: "KillFocusCursor",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1231`. Whether a window without keyboard focus keeps a hollow cursor on the live screen. `CaretKillFocus` (`vtdisp.c:1872`) draws a full-cell outline regardless of the configured active cursor shape; off, an unfocused window has no cursor at all.",
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
        name: "window.auto_scroll_only_at_bottom",
        page: "window",
        section: "Tera Term",
        key: "AutoScrollOnlyInBottomLine",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TAB_GENERAL_AUTOSCROLL_ONLY_IN_BOTTOM_LINE"),
        doc: "`ttset.c:1564`. Whether output arriving while the user has scrolled back leaves the view where it is.  **Off**, which means the opposite of what a modern terminal does: every line the host prints yanks the view to the cursor, because `MoveCursor` and `MoveRight` call `DispScrollToCursor` unconditionally (`buffer.c:3794`, `:3805`). Scrolling back through a boot log on a device that is still talking is the case it ruins, and this port had upstream's `on` behaviour hardcoded before there was a key to read.",
    },
    Field {
        name: "window.scroll_threshold",
        page: "window",
        section: "Tera Term",
        key: "ScrollThreshold",
        kind: Kind::Int,
        default: "12",
        label: None,
        doc: "`ttset.c:1273`. How many scrolled lines upstream lets accumulate before it repaints (`vtdisp.c:3132`) — a coalescing governor, counted in lines rather than in time.  Read and written and acting on nothing, said here rather than discovered: the equivalent is `TerminalView`'s 8 ms frame floor, which measures the same thing in the unit a compositor cares about. Carried so a file round-trips, the way `bell.notify_sound` is.",
    },
    Field {
        name: "window.opacity_inactive",
        page: "window",
        section: "Tera Term",
        key: "AlphaBlend",
        kind: Kind::IntByte,
        default: "255",
        label: Some("DLG_TAB_VISUAL_ALPHA_INACTIVE"),
        doc: "`ttset.c:1466`. `255` is fully opaque and `0` fully transparent; upstream applies this value when the window loses focus (`vtwin.cpp:1537`).  **The `max(0, …)`/`min(255, …)` pair on the next two lines is dead code**, and reading it as the rule is how this row came to say `int_clamp(0..255)`. `ts.AlphaBlendInactive` is a `BYTE`, so the assignment has already narrowed the value and neither test can ever fire. It decides the answer for the one value a person writes here: `AlphaBlend=-1` is `(UINT)-1`, which the `BYTE` makes **255** — an opaque window — where the clamp reads a bare -1 as below the floor and gives 0, a window nobody can see. `AlphaBlend=256` is 0 for the same reason, not 255.",
    },
    Field {
        name: "window.opacity_active",
        page: "window",
        section: "Tera Term",
        key: "AlphaBlendActive",
        kind: Kind::IntByte,
        default: "255",
        label: Some("DLG_TAB_VISUAL_ALPHA_ACTIVE"),
        doc: "`ttset.c:1470`, with the same dead clamp behind the same narrowing. Its fallback is the *loaded inactive opacity*, not the constant 255: `AlphaBlend=120` without this key makes both window states 120. An empty value inherits too; a non-numeric one is zero under `GetPrivateProfileInt`'s own rules. Applied at startup, after settings changes and whenever the window gains focus (`vtwin.cpp:780`, `:1357`, `:1539`).",
    },
    Field {
        name: "window.title_format",
        page: "window",
        section: "Tera Term",
        key: "TitleFormat",
        kind: Kind::IntWord,
        default: "13",
        label: Some("DLG_TAB_GENERAL_TITLEFMT_GROUP"),
        doc: "`ttset.c:1339`, a six-bit word rather than an enum: 1 shows the endpoint, 2 the process-wide session number, 4 the `VT`/`TEK` suffix, 8 puts the endpoint before the configured/remote title, 16 includes a TCP port and 32 includes a serial speed (`ttwinman.c:79`). The shipped 13 is 1|4|8: `<endpoint> - <title> VT`.  Kept as one word because that is what the file exposes and unknown bits 6–15 round-trip through upstream unchanged. Upstream's dialog presents the low six as checkboxes; the generated first-pass dialog exposes the word directly until it grows a bit-field widget.",
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
        name: "font.quality",
        page: "font",
        section: "Tera Term",
        key: "FontQuality",
        kind: Kind::Enum(&["default", "nonantialiased", "antialiased", "cleartype"]),
        default: "default",
        label: Some("DLG_TAB_VISUAL_FONT_QUALITY"),
        doc: "`ttset.c:1769`. The Win32 `LOGFONT` rasterisation request. Anything unknown falls to `default`; comparisons are case-insensitive.",
    },
    Field {
        name: "font.draw_resized",
        page: "font",
        section: "Tera Term",
        key: "DrawingResizedFont",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1988`, default **on**. Upstream fits glyphs whose natural advance does not match the cell into the cell (`vtdisp.c:2902`).",
    },
    Field {
        name: "font.draw_api",
        page: "font",
        section: "Tera Term",
        key: "VTDrawAPI",
        kind: Kind::Enum(&["Auto", "Unicode", "ANSI"]),
        default: "Auto",
        label: None,
        doc: "`ttset.c:2017`, parsed by `VTDrawFromIni` (`vtdraw.cpp:63`). These are the three Win32 text APIs upstream can select; an unknown spelling returns to `Auto`. Qt always draws Unicode glyphs, so this remains compatibility data.",
    },
    Field {
        name: "font.draw_ansi_code_page",
        page: "font",
        section: "Tera Term",
        key: "VTDrawACP",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:2021`. Zero asks the Windows renderer for the process ANSI code page; a positive value overrides it. It has no effect on Qt's Unicode text path, but preserving it matters to a settings file shared with Tera Term.",
    },
    Field {
        name: "font.space_left",
        page: "font",
        section: "Tera Term",
        key: "VTFontSpace",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1346`, the first field of `VTFontSpace`. Upstream adds the left and right values to the cell width and offsets the glyph by the left value.",
    },
    Field {
        name: "font.space_right",
        page: "font",
        section: "Tera Term",
        key: "VTFontSpace",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "The second field of `VTFontSpace`; it adds space after the glyph.",
    },
    Field {
        name: "font.space_top",
        page: "font",
        section: "Tera Term",
        key: "VTFontSpace",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "The third field of `VTFontSpace`; it offsets the glyph downward and adds to the cell height (`vtdisp.c:2838`). Negative spacing remains meaningful.",
    },
    Field {
        name: "font.space_bottom",
        page: "font",
        section: "Tera Term",
        key: "VTFontSpace",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "The fourth field of `VTFontSpace`; it adds space below the glyph.",
    },
    Field {
        name: "font.scaling",
        page: "font",
        section: "Tera Term",
        key: "FontScaling",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1640`, default off and marked \"test\". Its only consumer is inside a `#if 0` block at `vtwin.cpp:2718`, so carrying it without runtime behavior is faithful to current upstream rather than an omission.",
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
        name: "mouse.ctrl_disables_wheel_to_cursor",
        page: "mouse",
        section: "Tera Term",
        key: "DisableWheelToCursorByCtrl",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1594`. Holding Ctrl cancels the translation above, so the wheel scrolls the terminal's own history instead of sending arrow keys to the full-screen application in front of it (`vtterm.c:5847`). The pair to `mouse.ctrl_disables_tracking`, and on for the same reason.",
    },
    Field {
        name: "mouse.wheel_scroll_line",
        page: "mouse",
        section: "Tera Term",
        key: "MouseWheelScrollLine",
        kind: Kind::Int,
        default: "3",
        label: Some("DLG_TAB_GENERAL_MOUSEWHEEL_SCROLL_LINE"),
        doc: "`ttset.c:1276`. How many lines one notch of the wheel moves.  **Only when the notch arrives alone.** `vtwin.cpp:2539` multiplies under `line == 1`, where `line` is `abs(zDelta)/WHEEL_DELTA` — so a flick fast enough to coalesce two notches into one message scrolls two lines rather than six, and the setting stops applying exactly when the user is scrolling hardest. Not a clamp either: the guard is `> 0`, so `MouseWheelScrollLine=0` is one line per notch and so is a negative value.  It is also the step for something with no other name: with the pointer over the title bar the wheel changes the window's opacity, by this many units of 255 (`vtwin.cpp:2500`). One setting, two meanings, the way `TelEcho` and `ts.BSKey` each have two.",
    },
    Field {
        name: "mouse.clickable_url",
        page: "mouse",
        section: "Tera Term",
        key: "EnableClickableUrl",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_TAB_GENERAL_CLICKURL"),
        doc: "`ttset.c:771`. URL recognition, colouring and underlining happen regardless; this controls only the hand cursor and the double-click that launches one (`vtwin.cpp:2426`, `buffer.c:4411`). It ships off.",
    },
    Field {
        name: "mouse.cursor",
        page: "mouse",
        section: "Tera Term",
        key: "MouseCursor",
        kind: Kind::Str,
        default: "IBEAM",
        label: Some("DLG_TAB_VISUAL_MOUSE"),
        doc: "`ttset.c:1460`. The ordinary pointer over the terminal: `ARROW`, `IBEAM`, `CROSS` or `HAND`, compared case-insensitively by `SetMouseCursor` (`vtwin.cpp:159`). `IBEAM` is the shipped default.  A `string`, deliberately, rather than an enum. The reader copies the file's spelling verbatim into `MouseCursorName`, and `SetMouseCursor` simply returns without changing the current pointer when none of the four names matches. Normalising an unknown value to the default would therefore change both a shared file and the live no-op behavior upstream gives it.",
    },
    Field {
        name: "url.browser",
        page: "url",
        section: "Tera Term",
        key: "ClickableUrlBrowser",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1760`. An empty string uses the operating system's URL handler. A configured executable is tried only for HTTP, HTTPS and FTP; SFTP, TFTP, NEWS and MMS still go straight to the system handler (`buffer.c:4084`).",
    },
    Field {
        name: "url.browser_args",
        page: "url",
        section: "Tera Term",
        key: "ClickableUrlBrowserArg",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1762`. Prepended to the URL when `url.browser` is used; ignored for the four schemes that always use the system handler.",
    },
    Field {
        name: "url.join_split",
        page: "url",
        section: "Tera Term",
        key: "JoinSplitURL",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1792`. Read, written and documented, but never consulted anywhere in upstream's current source: continued display lines are joined according to `AttrLineContinued` regardless. Carried so the file round-trips; it acts on nothing here for the same reason it acts on nothing there.",
    },
    Field {
        name: "url.join_split_ignore_eol_char",
        page: "url",
        section: "Tera Term",
        key: "JoinSplitURLIgnoreEOLChar",
        kind: Kind::Str,
        default: "\\",
        label: None,
        doc: "`ttset.c:1794`. Upstream keeps only the first byte (a backslash by default), but — like `JoinSplitURL` itself — no current code reads the result. Carried in the file and deliberately not given invented behaviour.",
    },
    Field {
        name: "debug.enabled",
        page: "debug",
        section: "Tera Term",
        key: "Debug",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1162`, default off. When enabled, Shift+Escape cycles through the receive-display modes selected by `debug.modes`; TTL's `setdebug` can select one directly without changing this keyboard gate.",
    },
    Field {
        name: "debug.modes",
        page: "debug",
        section: "Tera Term",
        key: "DebugModes",
        kind: Kind::Str,
        default: "all",
        label: None,
        doc: "`ttset.c:1798`. `all` permits normal, hex and no-output modes; otherwise a comma-separated list of `normal`, `hex` and `noout` selects the cycle. `on` is an alias for all, while `off`, `none`, or a list with no recognised word also disables `Debug` upstream. Kept raw because order and duplicate words are part of the file even though only the resulting mask affects cycling.",
    },
    Field {
        name: "bell.mode",
        page: "bell",
        section: "Tera Term",
        key: "Beep",
        kind: Kind::Enum(&["off", "visual", "on"]),
        default: "on",
        label: None,
        doc: "`ttset.c:1112`, read with an **empty** default and compared down an `_stricmp` chain that tests only `off` and `visual` — so the `on` spelling below matches nothing and lands on the same `else` the absent key does, which is why both give the same variant here. Ninth member of the family `AGENTS.md` keeps returning to.",
    },
    Field {
        name: "bell.on_connect",
        page: "bell",
        section: "Tera Term",
        key: "BeepOnConnect",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1121`, `PF_BEEPONCONNECT`. **A TCP/IP connection only**: both places it is read test `PortType==IdTCPIP` first (`vtwin.cpp:3018`, `:3658`), so a serial console opening and closing is silent however this is set. It also bypasses `RingBell` entirely — always audible, never the visual bell, and never governed by the four below.",
    },
    Field {
        name: "bell.visual_wait_ms",
        page: "bell",
        section: "Tera Term",
        key: "BeepVBellWait",
        kind: Kind::IntMin(1),
        default: "10",
        label: None,
        doc: "`ttset.c:1125`. How long the screen stays inverted for a visual bell, in milliseconds — `int_min`, since it floors at 1 rather than taking the default.",
    },
    Field {
        name: "bell.over_used_count",
        page: "bell",
        section: "Tera Term",
        key: "BeepOverUsedCount",
        kind: Kind::Int,
        default: "5",
        label: None,
        doc: "`ttset.c:1781`. How many bells inside `bell.over_used_time` seconds are allowed before the governor starts suppressing. **Off by one against the manual**: `teraterm-term.html` says five bells are permitted and six sound, because `RingBell`'s inner `if` decides the *next* bell's fate and the switch that makes the noise sits outside it (`vtterm.c:5800`).",
    },
    Field {
        name: "bell.over_used_time",
        page: "bell",
        section: "Tera Term",
        key: "BeepOverUsedTime",
        kind: Kind::Int,
        default: "2",
        label: None,
        doc: "`ttset.c:1783`. The window the count is measured over, in seconds. A gap longer than this refills the count.",
    },
    Field {
        name: "bell.suppress_time",
        page: "bell",
        section: "Tera Term",
        key: "BeepSuppressTime",
        kind: Kind::Int,
        default: "5",
        label: None,
        doc: "`ttset.c:1785`. How long the terminal stays silent once the count is used up, in seconds — and it is **quiet** time, not elapsed time. Every bell arriving during the suppression pushes the deadline out again (`vtterm.c:5796` assigns `now` in the arm that decides it is suppressed), so a host beeping steadily is silenced until it stops and for this long afterwards. The manual reads as though it were a fixed delay; the code is the specification and this follows the code, because a governor that let a runaway through every five seconds would not do the job it exists for.",
    },
    Field {
        name: "bell.notify_sound",
        page: "bell",
        section: "Tera Term",
        key: "NotifySound",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1996`. Whether the *notification* makes a sound — upstream's tray balloon (`vtwin.cpp:725`, `Notify2SetSound`), not the terminal's bell. Read and written and acting on nothing here, because there is no notification surface yet; it is in this section because a user looking for \"the sound settings\" will look here.",
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
        doc: "`ttset.c:1280`, `ts.SelOnActive`. Off **eats** the click that activates the window (`vtwin.cpp:2387` returns `MA_ACTIVATEANDEAT`), so bringing the terminal forward cannot start a selection by accident. Read and written and acting on nothing yet: Qt delivers no `WM_MOUSEACTIVATE`, so the equivalent is a first-click filter the view does not have.",
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
        doc: "`ttset.c:1954`, `ts.SelectStartDelay`, in milliseconds. How long the button is held before a drag counts as a selection rather than as a click. Read and written and acting on nothing yet; it ships at 0, which is what the view does.",
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
        doc: "`ttset.c:1428`, `CPF_CONFIRM_RBUTTON`. The right button raises a menu with Paste on it instead of pasting, and the button-up paste is then suppressed as well (`vtwin.cpp:2645` tests both bits). Half honoured: the suppression is there and the menu is not, so setting this gives a right button that does nothing rather than one that offers a choice.",
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
        doc: "`ttset.c:1434`, `CPF_CONFIRM_CHANGEPASTE_CR`. The same confirmation for \"paste and send a CR\", where the newline is the one being *added* rather than one already in the text. Only consulted on that path, so a plain paste of text with no break is never confirmed by it — and that path is upstream's `Paste<CR>` menu item, which this shell has no command for, so the key is read and written and acts on nothing yet.",
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
        doc: "`ttset.c:1633`, milliseconds between the lines of a paste — for a host with no flow control that drops what arrives while it is still echoing. The only setting in the file clamped at **both** ends; see `int_clamp` above for why that is a third bound rather than one of the other two. Read and written and acting on nothing yet: pacing a paste means handing the send path a schedule, and `Session::paste` queues the whole thing.",
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
        name: "clipboard.remote_access",
        page: "clipboard",
        section: "Tera Term",
        key: "ClipboardAccessFromRemote",
        kind: Kind::Enum(&["on", "read", "write", "off"]),
        default: "off",
        label: Some("DLG_TAB_SEQUENCE_CLIPBOARD_ACCESS"),
        doc: "`ttset.c:1742`, `ts.CtrlFlag & CSF_CBMASK`. The two bits are independent: `read` lets OSC 52 ask for the local clipboard, `write` lets it replace the clipboard, and `on`/`readwrite` sets both. Anything else is off — including an empty value — and the writer canonicalises the two-bit form to `on`.  Off by default because a remote process reading the clipboard can disclose a password or token which never went near this terminal, while writing it can replace text the user is about to paste somewhere else. `/OSC52=` overrides this setting for one launch through the same four-state value.",
    },
    Field {
        name: "clipboard.remote_notify",
        page: "clipboard",
        section: "Tera Term",
        key: "NotifyClipboardAccess",
        kind: Kind::Bool,
        default: "on",
        label: Some("DLG_TAB_SEQUENCE_CLIPBOARD_NOTIFY"),
        doc: "`ttset.c:1753`, `GetOnOff(…, TRUE)`. Upstream raises a balloon for accepted and rejected reads and writes. It does not change the permission above: with access off (the default), this being on is what makes a rejected attempt visible instead of silently dropping it.",
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
        kind: Kind::IntWord,
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
        kind: Kind::IntWord,
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
        name: "connection.telnet_auto_detect",
        page: "connection",
        section: "Tera Term",
        key: "TelAutoDetect",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "**`ttset.c:1298`, and it is the reason `Telnet=off` is not a raw socket.** `commlib.c:323` copies this onto the connection unconditionally, beside a `cv->TelFlag` that comes from `Telnet=`; then `ttcmn.c:590` reads `!cv->TelFlag && cv->TelAutoDetect` and turns the framing on at the first `0xFF` byte. So the shipped defaults give a TCP session that starts as data and becomes telnet the moment anything sends an `IAC`, whatever `Telnet=` says — which is why TTSSH clears it by hand (`ttxssh.c:981`) with a comment saying the line \"should not be needed\". It is: an SSH stream is full of `0xFF`.  The two keys together are four states rather than two, and `TelnetMode` in `tt-conn` carries all four. See `tt_session::open::telnet_params`.",
    },
    Field {
        name: "connection.telnet_echo",
        page: "connection",
        section: "Tera Term",
        key: "TelEcho",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1304`. **Not \"echo locally\" — it is \"let the ECHO option decide\".** With it off, `WILL ECHO` and `WONT ECHO` change nothing (`telnet.c:411`, `:497` both test it first) and the opening burst simply asks the server to echo. With it on, the negotiated state *assigns* `ts.LocalEcho` — server echoing means local echo off — and the burst runs `TelChangeEcho` instead, which asks the server to echo only if local echo is currently off and asks it **not** to (`DONT ECHO`) if local echo is on. The setting and the mode are one variable, the same shape as DECBKM and `ts.BSKey`.",
    },
    Field {
        name: "connection.telnet_log",
        page: "connection",
        section: "Tera Term",
        key: "TelLog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1307`, which ORs `LOG_TEL` into `ts.LogFlag` rather than keeping a field of its own. `TELNET.LOG` in the log directory, truncated at every connection (`telnet.c:127`, `CREATE_ALWAYS`).  **It records only what Tera Term sends.** Every one of the eight `TelWriteLog` calls sits directly after a `CommRawOut`, and nothing on the receive path logs at all — so the `>` that leads each line has no inbound counterpart and a file that looks like a negotiation trace is one half of the conversation.",
    },
    Field {
        name: "connection.telnet_keepalive",
        page: "connection",
        section: "Tera Term",
        key: "TelKeepAliveInterval",
        kind: Kind::IntWord,
        default: "300",
        label: None,
        doc: "`ttset.c:1314`, in **seconds**, zero meaning no keepalive. An `IAC NOP` for a firewall that would otherwise drop an idle session.  Two things the name does not say. It is a **quiet** period, not a period: `telnet.c:913` compares against `cv.LastSendTime`, which `commlib.c:1062` stamps on every telnet send including the NOP itself, so a session that is being typed at never sends one. And it runs only where the opening burst ran — `TelStartKeepAliveThread` is called inside `vtwin.cpp:3666`'s `TCPPort == TelPort` arm — so a telnet-framed connection to a port that is not the telnet port gets no keepalive at all.",
    },
    Field {
        name: "connection.tcp_local_echo",
        page: "connection",
        section: "Tera Term",
        key: "TCPLocalEcho",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1322`. Local echo for a TCP connection that is **not** speaking telnet, which is a separate key because the telnet one is negotiated and this one cannot be.  It does not sit beside `LocalEcho`; it *overwrites* it. `vtwin.cpp:3696` assigns `ts.LocalEcho = ts.TCPLocalEcho` when the connection opens and `:3589` puts `ts.LocalEcho_ini` back when it closes — upstream keeps a pristine copy of the file's value precisely because the connection spends the live one. Off means \"leave the terminal's own setting alone\", so this is one of the settings where 0 is not a value.",
    },
    Field {
        name: "connection.tcp_cr_send",
        page: "connection",
        section: "Tera Term",
        key: "TCPCRSend",
        kind: Kind::Enum(&["", "CR", "CRLF"]),
        default: "",
        label: None,
        doc: "`ttset.c:1325`, the same override for the line ending, and the empty spelling is the one that means \"do not override\" — `Temp[0] = 0` is exactly what the writer emits for it (`:2820`), so the disabled state round-trips as an empty value rather than as a missing key.  The `else` and the default are the same arm here, unlike `window.title_change`: an unrecognised spelling is disabled too.",
    },
    Field {
        name: "connection.confirm_disconnect",
        page: "connection",
        section: "Tera Term",
        key: "ConfirmDisconnect",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1154`, on by default, and it is `PortFlag` rather than a field. Whether closing the window or choosing Disconnect asks first. **TCP only** — both tests are `cv.PortType==IdTCPIP` (`vtwin.cpp:1668`, `:4448`), so a serial session closes without a word however this is set.",
    },
    Field {
        name: "connection.history_list",
        page: "connection",
        section: "Tera Term",
        key: "HistoryList",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:972`, off by default. Whether a host that was connected to is remembered in the New Connection dialog's list (`vtwin.cpp:3849`).  Upstream's writer spells the key `Historylist` (`:2521`) where its reader spells it `HistoryList`. Harmless — `GetPrivateProfile*` matches key names case-insensitively, which `ini-audit/` measured rather than assumed — and it is not the only one: `Metakey`, `XmodemRcvCommand`, `YmodemRcvCommand` and `ZmodemRcvCommand` are written in a case their own readers do not use either.",
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
        name: "connection.clear_screen_on_close",
        page: "connection",
        section: "Tera Term",
        key: "ClearScreenOnCloseConnection",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1610`, off by default. When a connection ends and the window is staying open, scroll the live page into the history and home the cursor (`vtwin.cpp:3029`, `:4513`). This is Tera Term's ordinary Clear screen operation, not an erase: the old page remains available in scrollback.  It sits after the auto-close decision. A network session with `connection.auto_win_close` on closes its window instead, while serial and local-pty sessions never take that network-only branch.",
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
        name: "connection.line_mode",
        page: "connection",
        section: "Tera Term",
        key: "EnableLineMode",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1673`, default **on**. A TCP connection begins in local line mode until telnet negotiation says otherwise (`commlib.c:341`); serial is never affected. The async transport currently sends keystrokes immediately, so the key is carried pending a negotiated line buffer.",
    },
    Field {
        name: "proxy.type",
        page: "proxy",
        section: "TTProxy",
        key: "ProxyType",
        kind: Kind::Enum(&["none", "http", "telnet", "socks4", "socks"]),
        default: "none",
        label: None,
        doc: "`ProxyWSockHook.h:1972`, through `ProxyInfo::parseType` (`:158`), which lower-cases the value and compares it against a table — so it is case-insensitive, and **anything unrecognised is no proxy at all**. A typo is therefore a direct connection rather than an error, which is this file's ordinary rule for an enumerated setting and is worth naming here because the cost is different: the user believes they are behind a proxy and is not. `none` is the spelling that says so on purpose.  `socks` and `socks5` are one type and `socks` is what upstream writes back — `getTypeName` walks the same table and returns the first row for the type, which is `socks` (`:181`).  **Five more spellings are in upstream's table and are not here**: `ssl`, `none+ssl`, `http+ssl`, `socks+ssl`, `socks4+ssl`, `socks5+ssl` and `telnet+ssl` parse to types the relay `switch` has no arm for, so they fall to its `default: result = 0` (`:1822`) — reported as connected, with no handshake done — because `SSLSocket.h` and `SSLLIB.h` are in the source tree, included by nothing and listed in no build. Leaving them out puts them on the unrecognised arm, which is the honest place for a spelling that has never meant anything.",
    },
    Field {
        name: "proxy.host",
        page: "proxy",
        section: "TTProxy",
        key: "ProxyHost",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ProxyWSockHook.h:1976`. Empty is no proxy however `proxy.type` is set — `_load` demotes a type it has no host for rather than dialling an empty name.",
    },
    Field {
        name: "proxy.port",
        page: "proxy",
        section: "TTProxy",
        key: "ProxyPort",
        kind: Kind::IntWord,
        default: "0",
        label: None,
        doc: "`ProxyWSockHook.h:1980`, read with `getInteger`'s default of **0** and assigned to an `unsigned short`, which is why this is `uint16` rather than a bounded `int`: `ProxyPort=65537` is port 1.  **Zero means the type's default here and disables the relay upstream**, which is the first of the four defects in `tt-conn/src/proxy.rs`. `getPort()` (`:442`) supplies 1080 for either SOCKS, 23 for the telnet proxy and 80 for HTTP, and the connect hook uses it for the address (`:1770`) — and then the guard deciding whether to speak the protocol at all tests the raw stored port instead (`:1792`). Leaving the dialog's port box empty is enough to reach it.",
    },
    Field {
        name: "proxy.user",
        page: "proxy",
        section: "TTProxy",
        key: "ProxyUser",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ProxyWSockHook.h:1981`. The proxy's own credentials, not the host's: Basic authentication for HTTP, RFC 1929 for SOCKS5, and the user ID field for SOCKS4 — which is not a password at all, being what the proxy hands to the client's identd.  An empty value and an absent key mean the same thing here, which is not this file's usual rule. Upstream keeps them apart as `\"\"` and NULL, and the distinction reaches exactly one place: a SOCKS5 method list that offers username/password with two empty strings in hand. No server accepts that, so nothing observable is lost by treating both as \"no credentials\".",
    },
    Field {
        name: "proxy.pass",
        page: "proxy",
        section: "TTProxy",
        key: "ProxyPass",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ProxyWSockHook.h:1983`, and it is stored in plain text, as upstream stores it. Upstream reads it only when `ProxyUser` is set and then uses it without checking — `begin_relay_http` reaches `strlen(proxy.pass)` under a test of the *user* alone (`:1275`), so a username with no password is a NULL dereference on the first HTTP proxy connection, and the dialog produces exactly that pair from an empty password box (`:1013`). Here an absent password is the empty string, which is what `user:` means to Basic.",
    },
    Field {
        name: "proxy.timeout",
        page: "proxy",
        section: "TTProxy",
        key: "ConnectionTimeout",
        kind: Kind::IntRange(0, 2147483647),
        default: "10",
        label: None,
        doc: "`ProxyWSockHook.h:1987`, in seconds. It bounds every send and receive of the handshake, as a `select` timeout there and as a socket timeout here, and **zero is upstream's \"wait forever\"** — a null `timeval` — which is kept.  The floor is a divergence with no cost: a negative value makes upstream's `timeval` invalid, so `select` fails with `EINVAL` and every proxy connection breaks. Below zero takes the default instead.",
    },
    Field {
        name: "proxy.socks_resolve",
        page: "proxy",
        section: "TTProxy",
        key: "SocksResolve",
        kind: Kind::Enum(&["auto", "local", "remote"]),
        default: "auto",
        label: None,
        doc: "`ProxyWSockHook.h:1990`. Who turns the host name into an address — `local` resolves here and fails if it cannot, `remote` never resolves here and sends the name, `auto` resolves here and falls back to sending the name. Compared case-insensitively with an `else` of `auto`.  Only the two SOCKS relays consult it; HTTP and the telnet proxy send the name as text and have no choice to make. For SOCKS4 it is also what decides whether the request is a 4a one, since that protocol expresses an unresolved name by an address of `0.0.0.1` and a name after the user ID.",
    },
    Field {
        name: "proxy.telnet_hostname_prompt",
        page: "proxy",
        section: "TTProxy",
        key: "TelnetHostnamePrompt",
        kind: Kind::Str,
        default: ">> Host name: ",
        label: None,
        doc: "`ProxyWSockHook.h:1999`. The five strings the `telnet` proxy type watches for and answers, which exist because that \"protocol\" is a person's session with a terminal server and every vendor's prompts are different.  Each is matched as a substring of **one line**: `wait_for_prompt` (`:1228`) reads to a newline and then looks for each of the five in what it has, so a prompt split across two lines is never found and a line holding two of them takes the first. The trailing space in this one is part of the value.",
    },
    Field {
        name: "proxy.telnet_username_prompt",
        page: "proxy",
        section: "TTProxy",
        key: "TelnetUsernamePrompt",
        kind: Kind::Str,
        default: "Username:",
        label: None,
        doc: "`ProxyWSockHook.h:2000`. Answered with `proxy.user`.",
    },
    Field {
        name: "proxy.telnet_password_prompt",
        page: "proxy",
        section: "TTProxy",
        key: "TelnetPasswordPrompt",
        kind: Kind::Str,
        default: "Password:",
        label: None,
        doc: "`ProxyWSockHook.h:2001`. Answered with `proxy.pass`.",
    },
    Field {
        name: "proxy.telnet_connected_message",
        page: "proxy",
        section: "TTProxy",
        key: "TelnetConnectedMessage",
        kind: Kind::Str,
        default: "-- Connected to ",
        label: None,
        doc: "`ProxyWSockHook.h:2002`. Seeing this ends the relay and starts the session.",
    },
    Field {
        name: "proxy.telnet_error_message",
        page: "proxy",
        section: "TTProxy",
        key: "TelnetErrorMessage",
        kind: Kind::Str,
        default: "!!!!!!!!",
        label: None,
        doc: "`ProxyWSockHook.h:2003`. Seeing this ends the relay as a refusal.",
    },
    Field {
        name: "proxy.debug_log",
        page: "proxy",
        section: "TTProxy",
        key: "DebugLog",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ProxyWSockHook.h:2005`, and the only diagnostic there is for a handshake that fails: everything the relay does happens before the terminal has a session, so a refusal reaches the user as one sentence in a box and nothing else. Empty is no trace.  `TTProxy/Logger.h` appends two kinds of record — `send: [ 05 01 00 ]` for the two SOCKS relays and `send: \"…\"` for HTTP and the telnet proxy — and Sterna writes the same two, so a trace taken here reads beside one taken from Tera Term. **The credentials are in it**, Base64 being no more than a spelling.  A relative name resolves against `ts.LogDirW` (`TTProxy.h:198`), which is the **program's** log directory and not the terminal's; no key in this file moves it. The one departure is when the file appears: upstream opens it as it reads this key, so the key alone creates an empty file in a session that never connects, and here the first handshake with something to say creates it.",
    },
    Field {
        name: "macro.startup_file",
        page: "macro",
        section: "Tera Term",
        key: "StartupMacro",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1291`, a wide string whose empty default means no automatic macro. `CVTWindow::Startup` (`vtwin.cpp:1413`) consumes it once when the window starts; a leading `*` makes TTPMACRO put up its file picker (`ttmmain.cpp:285`). `/M` can replace it and a `/D=` topic clears it, so a terminal launched by a macro does not recursively launch another one.",
    },
    Field {
        name: "settings.source_version",
        page: "settings",
        section: "Tera Term",
        key: "Version",
        kind: Kind::Str,
        default: "5.7.0",
        label: None,
        doc: "`ttset.c:575`. Upstream parses the first two components to decide which compatibility migrations to apply, and its writer stamps the running Tera Term version. Sterna preserves the source metadata without claiming that it is that program; 5.7.0 is the current upstream fallback this schema targets.",
    },
    Field {
        name: "settings.language_file",
        page: "settings",
        section: "Tera Term",
        key: "UILanguageFile",
        kind: Kind::Str,
        default: "lang\\Default.lng",
        label: Some("DLG_GEN_LANG_UI"),
        doc: "`ttset.c:1489` delegates this read to `GetUILanguageFileFullW`, whose own fallback is `lang\\Default.lng` (`common/ttlib_static_cpp.cpp:1041`). The default catalog contains font choices only, so absent translations fall back to the source-language text embedded in the application. Upstream resolves a relative value beside its executable; the shell does the platform-specific installed-data resolution after this compatible value has been read.",
    },
    Field {
        name: "settings.auto_backup",
        page: "settings",
        section: "Tera Term",
        key: "IniAutoBackup",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1999`, default **on**. Before Setup > Save setup overwrites the active file, upstream copies its old bytes to a sibling named `YYYYMMDDTHHMMSS+zzzz_TERATERM.INI` (`vtwin.cpp:4738`, `ttlib_static_cpp.cpp:1432`). A first save has no old file to copy, and the smaller close-time geometry save does not take this path.",
    },
    Field {
        name: "serial.com_port",
        page: "serial",
        section: "Tera Term",
        key: "ComPort",
        kind: Kind::IntWord,
        default: "1",
        label: Some("DLG_SERIAL_PORT"),
        doc: "`ttset.c:916`, narrowed to a `WORD` by the assignment as `MaxComPort` is.  **The bound is a different setting and is not a clamp**: `:1223` resets the port to 1 when it is below 1 or above `MaxComPort`, which is read at `:1218` — after this key but before the check. That is a cross-field rule with an order to it, which no row here can carry, so it lives in `Settings::normalize` beside `Debug`/`DebugModes`. The narrowing has to be the row's, though, because the reset compares the *narrowed* value: `ComPort=65538` is port 2 upstream rather than a number above every ceiling.  What a number means on Linux is a separate question: this port opens a device path, so `/C=1` is resolved against enumeration rather than against `COM1`.",
    },
    Field {
        name: "serial.baud",
        page: "serial",
        section: "Tera Term",
        key: "BaudRate",
        kind: Kind::Int,
        default: "115200",
        label: Some("DLG_SERIAL_BAUD"),
        doc: "`ttset.c:919`. Unbounded, and the hardware decides what it can do with it — `tt-conn` reads the setting back after `tcsetattr` for exactly that reason.  **The default is 115200 and upstream's is 9600** — the first deliberate deviation, and `docs/deviations.md` has the reason: nothing this program is pointed at ships a 9600 console any more, so upstream's default is one more thing to change before the first useful connection. The key, its bounds and what a value in the file means are unchanged, so a `TERATERM.INI` that says `BaudRate=9600` still opens at 9600 in both programs.",
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
        kind: Kind::IntWord,
        default: "0",
        label: Some("DLG_SERIAL_DELAYCHAR"),
        doc: "`ttset.c:951`, milliseconds between characters — for a device that cannot keep up with a paste.",
    },
    Field {
        name: "serial.delay_per_line",
        page: "serial",
        section: "Tera Term",
        key: "DelayPerLine",
        kind: Kind::IntWord,
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
        kind: Kind::IntClamp(4, 4096),
        default: "256",
        label: None,
        doc: "`ttset.c:1218`, floored at 4 and capped at `MAXCOMPORT` (4096, `tttypes.h:908`). This is the setting `/C=` is bounded against, which is why the parser takes it as an argument.  **A clamp at both ends and not a range**, and the narrowing in front of it is load-bearing rather than pedantry. `ts.MaxComPort` is a `WORD`, so the assignment wraps before either test runs: `MaxComPort=-1` — which is what somebody writes meaning \"no limit\" — is 65535 and then 4096, where an `int_clamp` would read the -1 as below the floor and answer **4**, the other end of the range. `int(4..4096)` was wrong a second way as well, since below the floor it gave the default of 256 rather than upstream's 4.",
    },
    Field {
        name: "serial.rts",
        page: "serial",
        section: "Tera Term",
        key: "FlowCtrlRTS",
        kind: Kind::Int,
        default: "-1",
        label: None,
        doc: "**`ttset.c:2034`, and the default is a sentinel rather than a value.** Read with a default of `-1` and then derived from the flow control: RTS becomes Handshake under `FlowCtrl=hard` and Enable under anything else. This is the `TCPPort` trap the right way up — `FlowCtrl` is read at `:943`, eleven hundred lines earlier, so the derivation really does see the file's own value rather than an initialiser.  The numbers are Win32 `DCB` fields, not a table of Tera Term's own: 0 disable, 1 enable, 2 handshake, 3 toggle — and only RTS offers the fourth (`serial_pp.cpp:74`). An out-of-range number is where this gets dangerous. `CommResetSerial` puts it straight into the `DCB` and **does not check what `SetCommState` says about it** (`commlib.c:240`), so `FlowCtrlRTS=9` makes Windows reject the whole structure and the port keeps whatever it had — every serial setting in the file silently discarded, baud included, with no message. Not reproduced: `tt-session`'s `serial_params` reads anything it does not know as Enable.  Held as the sentinel rather than resolved on the way in, the same call `connection.terminal_speed` makes for the same reason — the schema cannot say \"the default is another setting\", so the resolution is in `serial_params`. **Upstream resolves at load and writes the concrete number back**, so its own save pins the line and changing the flow control afterwards no longer moves it; this port keeps the `-1`, which a real Tera Term reads the same way it reads an absent key.",
    },
    Field {
        name: "serial.dtr",
        page: "serial",
        section: "Tera Term",
        key: "FlowCtrlDTR",
        kind: Kind::Int,
        default: "-1",
        label: None,
        doc: "`ttset.c:2042`, the same sentinel as `serial.rts` against a different arm: DTR becomes Handshake under `FlowCtrl=dsrdtr` and Enable otherwise. There is no toggle for DTR — `serial_pp.cpp:75` lists three — and Win32 has no `DTR_CONTROL_TOGGLE` to list.",
    },
    Field {
        name: "serial.clear_buffer_on_open",
        page: "serial",
        section: "Tera Term",
        key: "ClearComBuffOnOpen",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1147`, `GetOnOff(…, TRUE)`. Purge whatever the driver has already buffered when the port opens, rather than delivering it as the session's first bytes. Off is a real choice on a console server: it is how you see what the far end said before you got there, and upstream marks the port readable straight away for it (`commlib.c:476`'s `cv->RRQ`).  It gates the purge on **open** only. Control > Reset port purges whatever this says (`vtwin.cpp:4913` passes TRUE outright), so the setting is not the answer to \"does resetting the port clear it\".",
    },
    Field {
        name: "serial.break_time",
        page: "serial",
        section: "Tera Term",
        key: "SendBreakTime",
        kind: Kind::Int,
        default: "1000",
        label: None,
        doc: "`ttset.c:1286`, milliseconds. How long `CommSendBreak` holds the line at space — Control > Send break, and a macro's `sendbreak`, which reaches the same place through DDE (`ttdde.c:801` posts the menu command).  One second is a long break and it is deliberate: a Sun PROM wants one, and `commlib.c:1176` says \"pause for 1 sec\" in a comment beside the parameter. This port had **three** durations and none of them was the file's — 300 ms in `MainWindow.cpp`, 250 ms in `tt-macro`, and whatever a caller of `tt_session_send_break` passed.",
    },
    Field {
        name: "serial.auto_reconnect",
        page: "serial",
        section: "Tera Term",
        key: "AutoComPortReconnect",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1086`, `GetOnOff(…, TRUE)`. Reopen the port by itself when the adapter comes back — the USB-serial cable somebody unplugged, which is the whole reason the setting exists.  Upstream drives it from `WM_DEVICECHANGE` (`vtwin.cpp:311`), so the Linux half is a udev monitor and is not built yet; the four keys below describe a state machine this port carries and does not yet run.",
    },
    Field {
        name: "serial.auto_reconnect_delay",
        page: "serial",
        section: "Tera Term",
        key: "AutoComPortReconnectDelayNormal",
        kind: Kind::IntWord,
        default: "500",
        label: None,
        doc: "`ttset.c:1088`, milliseconds. The wait between the device arriving and the reopen, for the case where the arrival named the port it was about.",
    },
    Field {
        name: "serial.auto_reconnect_delay_unknown_port",
        page: "serial",
        section: "Tera Term",
        key: "AutoComPortReconnectDelayIllegal",
        kind: Kind::IntWord,
        default: "2000",
        label: None,
        doc: "`ttset.c:1090`, milliseconds, and **\"illegal\" is about the notification and not about a value.** Some drivers send only `DBT_DEVTYP_DEVICEINTERFACE` and never the `DBT_DEVTYP_PORT` that would say *which* port arrived (`vtwin.cpp:335`), so this is the longer wait taken when the port number is unknown and the reopen is a guess.",
    },
    Field {
        name: "serial.auto_reconnect_retry_interval",
        page: "serial",
        section: "Tera Term",
        key: "AutoComPortReconnectRetryInterval",
        kind: Kind::IntWord,
        default: "1000",
        label: None,
        doc: "`ttset.c:1092`, milliseconds between one failed reopen and the next.",
    },
    Field {
        name: "serial.auto_reconnect_retries",
        page: "serial",
        section: "Tera Term",
        key: "AutoComPortReconnectRetryCount",
        kind: Kind::IntWord,
        default: "3",
        label: None,
        doc: "`ttset.c:1094`. Retries **after** the first attempt, so three is four tries — and unlike `BeepOverUsedCount` the name is honest about it. Two details are not in the name: an attempt where the port is still absent costs a retry without opening anything (`vtwin.cpp:475`'s `CheckComPort` guard), and the *last* attempt is the one allowed to raise the error box, because the suppression tests `retry_left_ != 0` (`:481`).  The four above are `WORD` in `tttypes.h:602`, so upstream truncates them to 16 bits: a two-minute retry interval written as `120000` is 54464 ms there and 120000 here. Not reproduced — the schema has no type for it and the divergence only exists for values nobody means.",
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
        kind: Kind::IntWord,
        default: "0",
        label: None,
        doc: "`ttset.c:1031`. 0 bytes, 1 KB, 2 MB (`log_pp.cpp:72`) — the unit the *dialog* shows `LogRotateSize` in, and nothing else. It is stored so that reopening the dialog offers the number the user typed rather than its expansion.",
    },
    Field {
        name: "log.rotate_step",
        page: "log",
        section: "Tera Term",
        key: "LogRotateStep",
        kind: Kind::IntWord,
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
        name: "transfer.send_filter",
        page: "transfer",
        section: "Tera Term",
        key: "FileSendFilter",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1063`. A file-dialog mask such as `*.txt;*.log`. Upstream uses it for raw Send file and every protocol's send picker.",
    },
    Field {
        name: "transfer.receive_filter",
        page: "transfer",
        section: "Tera Term",
        key: "FileReceiveFilter",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:2029`. The corresponding mask for raw Receive file. Protocols that carry their own filename do not ask for one and therefore do not use it.",
    },
    Field {
        name: "transfer.scp_send_dir",
        page: "transfer",
        section: "Tera Term",
        key: "ScpSendDir",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1068`. The destination directory offered by drag-and-drop SCP. `scp` is one of the macro host's explicitly unsupported transport commands, so this is retained for the file and the future implementation.",
    },
    Field {
        name: "transfer.raw_high_speed",
        page: "transfer",
        section: "Tera Term",
        key: "FileSendHighSpeedMode",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1191`, default **on**. The boosted raw sender is used only for a binary serial send at 115200 baud or faster with no echo, telnet framing or bracketed paste (`filesys.cpp:365`). It does not govern X/Y/ZMODEM or Kermit.",
    },
    Field {
        name: "transfer.confirm_file_drop",
        page: "transfer",
        section: "Tera Term",
        key: "ConfirmFileDragAndDrop",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1511`, default **on**. Whether dropping files opens the choice between raw send, SCP and cancel. The shell has no file-drop target yet.",
    },
    Field {
        name: "transfer.raw_send_delay_type",
        page: "transfer",
        section: "Tera Term",
        key: "SendfileDelayType",
        kind: Kind::Enum(&["NoDelay", "PerChar", "PerLine", "PerSendSize"]),
        default: "NoDelay",
        label: None,
        doc: "`ttset.c:2006`. How raw Send file spaces writes. Anything unrecognised takes `NoDelay`, including an empty present value.",
    },
    Field {
        name: "transfer.raw_send_delay_tick",
        page: "transfer",
        section: "Tera Term",
        key: "SendfileDelayTick",
        kind: Kind::IntWord,
        default: "0",
        label: None,
        doc: "`ttset.c:2011`, in milliseconds. The delay selected above; zero is a real value and means no wait.",
    },
    Field {
        name: "transfer.raw_send_size",
        page: "transfer",
        section: "Tera Term",
        key: "SendfileSize",
        kind: Kind::Int,
        default: "4096",
        label: None,
        doc: "`ttset.c:2012`. The chunk size for `PerSendSize`, and the raw sender's read size. Upstream applies no range at settings-load time.",
    },
    Field {
        name: "transfer.raw_send_sequential",
        page: "transfer",
        section: "Tera Term",
        key: "SendfileSequential",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:2013`. Use Tera Term 4's sequential file reader rather than loading chunks into memory before sending them. Off is the newer sender.",
    },
    Field {
        name: "transfer.raw_send_skip_dialog",
        page: "transfer",
        section: "Tera Term",
        key: "SendfileSkipOptionDialog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:2014`. Skip raw Send file's option page and use the remembered binary and delay settings directly.",
    },
    Field {
        name: "transfer.raw_receive_skip_dialog",
        page: "transfer",
        section: "Tera Term",
        key: "ReceivefileSkipOptionDialog",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:2030`. The corresponding switch for raw Receive file.",
    },
    Field {
        name: "printer.passthrough_delay",
        page: "printer",
        section: "Tera Term",
        key: "PassThruDelay",
        kind: Kind::IntWord,
        default: "3",
        label: None,
        doc: "`ttset.c:1237`, assigned to the 16-bit `ts.PassThruDelay` and written unsigned. It is the delay before controller-mode pass-through printing starts.",
    },
    Field {
        name: "printer.passthrough_port",
        page: "printer",
        section: "Tera Term",
        key: "PassThruPort",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1241`. The raw printer device used for pass-through mode; empty lets upstream choose its ordinary printer path.",
    },
    Field {
        name: "printer.control_sequences",
        page: "printer",
        section: "Tera Term",
        key: "PrinterCtrlSequence",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1245`, `TF_PRINTERCTRL`, default off. Gates the printer control sequences themselves, independently of whether a printer device was named.",
    },
    Field {
        name: "printer.margin_left",
        page: "printer",
        section: "Tera Term",
        key: "PrnMargin",
        kind: Kind::Int,
        default: "50",
        label: None,
        doc: "`ttset.c:1255`. Margins are hundredths of an inch, left, right, top and bottom. `GetNthNum` makes a missing field zero in a present value.",
    },
    Field {
        name: "printer.margin_right",
        page: "printer",
        section: "Tera Term",
        key: "PrnMargin",
        kind: Kind::Int,
        default: "50",
        label: None,
        doc: "The second field of `PrnMargin`, under the same `ttset.c:1255` rule.",
    },
    Field {
        name: "printer.margin_top",
        page: "printer",
        section: "Tera Term",
        key: "PrnMargin",
        kind: Kind::Int,
        default: "50",
        label: None,
        doc: "The third field of `PrnMargin`, under the same `ttset.c:1255` rule.",
    },
    Field {
        name: "printer.margin_bottom",
        page: "printer",
        section: "Tera Term",
        key: "PrnMargin",
        kind: Kind::Int,
        default: "50",
        label: None,
        doc: "The fourth field of `PrnMargin`, under the same `ttset.c:1255` rule.",
    },
    Field {
        name: "printer.convert_form_feed",
        page: "printer",
        section: "Tera Term",
        key: "PrnConvFF",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1263`, default off. Converts a form feed to a newline while printing.",
    },
    Field {
        name: "printer.vt_ppi_x",
        page: "printer",
        section: "Tera Term",
        key: "VTPPI",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1368`, first field. A positive value overrides the printer's native horizontal pixels per inch for VT printing; zero uses the device value.",
    },
    Field {
        name: "printer.vt_ppi_y",
        page: "printer",
        section: "Tera Term",
        key: "VTPPI",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "The second field of `VTPPI`, the vertical printer resolution under the same rule. Printing is Stage 3 work, so both fields currently round-trip only.",
    },
    Field {
        name: "tek.x",
        page: "tek",
        section: "Tera Term",
        key: "TEKPos",
        kind: Kind::Int,
        default: "-2147483648",
        label: None,
        doc: "`ttset.c:603`. The TEK window's x/y position, using the same `CW_USEDEFAULT` sentinel and `GetNthNum` zero-for-a-missing-field rule as `VTPos`.",
    },
    Field {
        name: "tek.y",
        page: "tek",
        section: "Tera Term",
        key: "TEKPos",
        kind: Kind::Int,
        default: "-2147483648",
        label: None,
        doc: "The second field of `TEKPos`, under the same `ttset.c:603` rule.",
    },
    Field {
        name: "tek.color",
        page: "tek",
        section: "Tera Term",
        key: "TEKColor",
        kind: Kind::Color2,
        default: "0,0,0,255,255,255",
        label: None,
        doc: "`ttset.c:789`, black on white. The pair remains a pair even though no TEK painter consumes it here.",
    },
    Field {
        name: "tek.color_emulation",
        page: "tek",
        section: "Tera Term",
        key: "TEKColorEmulation",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:860`, default off. Enables colour emulation in the dropped TEK parser.",
    },
    Field {
        name: "tek.gin_mouse_code",
        page: "tek",
        section: "Tera Term",
        key: "TEKGINMouseCode",
        kind: Kind::Int,
        default: "32",
        label: None,
        doc: "`ttset.c:1295`. The byte a TEK GIN mouse report uses, with no load-time range.",
    },
    Field {
        name: "tek.icon",
        page: "tek",
        section: "Tera Term",
        key: "TEKIcon",
        kind: Kind::Enum(&["tterm", "vt", "tek", "tterm_classic", "vt_classic", "tterm_3d", "vt_3d", "tterm_flat", "vt_flat", "cygterm", "Default"]),
        default: "Default",
        label: None,
        doc: "`ttset.c:1555` passes the value through `IconName2IconId`. The ten known names compare case-insensitively; anything else becomes `Default`.",
    },
    Field {
        name: "tek.ppi_x",
        page: "tek",
        section: "Tera Term",
        key: "TEKPPI",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "`ttset.c:1374`. Both positive values override the printer's native pixels per inch for a TEK print; zero in either axis means use the device values.",
    },
    Field {
        name: "tek.ppi_y",
        page: "tek",
        section: "Tera Term",
        key: "TEKPPI",
        kind: Kind::Int,
        default: "0",
        label: None,
        doc: "The second field of `TEKPPI`, under the same `ttset.c:1374` rule.",
    },
    Field {
        name: "window.maximized_bug_tweak",
        page: "window",
        section: "Tera Term",
        key: "MaximizedBugTweak",
        kind: Kind::IntWord,
        default: "2",
        label: None,
        doc: "`ttset.c:1527`. The string `on` means 2; every other present value goes through `atoi`, then narrows into a Win32 `WORD`. This controls an old maximised-window sizing workaround and has no corresponding Qt defect.",
    },
    Field {
        name: "window.icon",
        page: "window",
        section: "Tera Term",
        key: "VTIcon",
        kind: Kind::Enum(&["tterm", "vt", "tek", "tterm_classic", "vt_classic", "tterm_3d", "vt_3d", "tterm_flat", "vt_flat", "cygterm", "Default"]),
        default: "Default",
        label: None,
        doc: "`ttset.c:1550`, through `IconName2IconId`. The ten names compare case-insensitively and anything else becomes `Default`. Sterna keeps its own mark, so this is compatibility data for a shared Tera Term setup.",
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
    Field {
        name: "window.popup_menu",
        page: "window",
        section: "Tera Term",
        key: "PopupMenu",
        kind: Kind::Bool,
        default: "off",
        label: Some("DLG_WIN_HIDEMENU"),
        doc: "`ttset.c:731`. Hide the ordinary menu bar. When it is hidden, upstream opens the same menus as a popup on Ctrl+left-click (`vtwin.cpp:863`); this is not a choice between two different menus. `HideTitle` also removes the menu bar, independently of this key (`vtwin.cpp:3461`).",
    },
    Field {
        name: "window.popup_menu_enabled",
        page: "window",
        section: "Tera Term",
        key: "EnablePopupMenu",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1179`, default **on**. The gate on Ctrl+left-click opening the full menu while the bar is hidden. It does not decide whether the bar is hidden — that is `window.popup_menu`, or `window.hide_title` as a side effect.",
    },
    Field {
        name: "window.show_menu_enabled",
        page: "window",
        section: "Tera Term",
        key: "EnableShowMenu",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1183`, default **on**. With the bar hidden, upstream adds \"Show menu bar\" to the Win32 system menu (`vtwin.cpp:3509`). Qt cannot add application actions to a compositor-owned system menu, so the shell puts the recovery action in the Ctrl+left-click popup instead.",
    },
    Field {
        name: "window.window_menu",
        page: "window",
        section: "Tera Term",
        key: "WindowMenu",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1380`, default **on**. Adds the dynamic Window menu, whose entries are every open VT and TEK window (`vtwin.cpp:1116`). This process owns one terminal window and TEK is out of scope, so it is read and written and acts on nothing until Stage 3 gives the shell multiple sessions to list.",
    },
    Field {
        name: "window.save_position",
        page: "window",
        section: "Tera Term",
        key: "SaveVTWinPos",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:608`, default off. Upstream updates `ts.VTPos` on every move but writes it only under this switch — both during Save setup (`:2109`) and on window close (`SaveVTPos`, `:3340`). The switch itself is read-only upstream: `_WriteIniFile` never writes the key, so a user enables it by hand. This port exposes it through the generated dialog and writes the same upstream key.",
    },
    Field {
        name: "window.x",
        page: "window",
        section: "Tera Term",
        key: "VTPos",
        kind: Kind::Int,
        default: "-2147483648",
        label: None,
        doc: "`ttset.c:598`, first half. `CW_USEDEFAULT` is `INT_MIN`, so an absent key asks the window manager to place the window rather than meaning coordinate zero. **Conditionally written**: with `SaveVTWinPos=off`, `_WriteIniFile` leaves an existing `VTPos` line byte-for-byte alone (`ttset.c:2109`).",
    },
    Field {
        name: "window.y",
        page: "window",
        section: "Tera Term",
        key: "VTPos",
        kind: Kind::Int,
        default: "-2147483648",
        label: None,
        doc: "`ttset.c:600`, second half of the same pair and the same sentinel.",
    },
    Field {
        name: "window.auto_vt_tek_switch",
        page: "window",
        section: "Tera Term",
        key: "AutoWinSwitch",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:706`, default off. Upstream switches between its VT and TEK windows when either parser sees the other terminal's sequence. TEK is deliberately out of scope here, so the key round-trips and acts on nothing.",
    },
    Field {
        name: "connection.cygwin_directory",
        page: "connection",
        section: "Tera Term",
        key: "CygwinDirectory ",
        kind: Kind::Str,
        default: "c:\\cygwin",
        label: None,
        doc: "`ttset.c:1476`. The trailing space is in upstream's **reader literal** and the writer at `:2250` omits it. Backticks retain that space in the schema; `write-key` reproduces the writer. This is a Windows Cygwin install path, not the local pty's working directory, so it has no runtime effect here.",
    },
    Field {
        name: "log.viewer",
        page: "log",
        section: "Tera Term",
        key: "ViewlogEditor",
        kind: Kind::Str,
        default: "notepad.exe",
        label: None,
        doc: "`ttset.c:1483`. The external editor used by View log. Sterna has no View log command yet, so the Windows default is preserved and the key round-trips.",
    },
    Field {
        name: "log.viewer_args",
        page: "log",
        section: "Tera Term",
        key: "ViewlogEditorArg",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "`ttset.c:1485`; the allocated-string helper's null fallback becomes empty. Arguments for `log.viewer`, carried until that command exists.",
    },
    Field {
        name: "broadcast.command_history",
        page: "broadcast",
        section: "Tera Term",
        key: "BroadcastCommandHistory",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1493`, default off. Whether the broadcast dialog records commands. Broadcast is a multi-window feature and this shell owns one terminal today.",
    },
    Field {
        name: "broadcast.accept",
        page: "broadcast",
        section: "Tera Term",
        key: "AcceptBroadcast",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1497`, default **on**. Whether this terminal accepts a command from another Tera Term window. There is no broadcast channel in Sterna yet.",
    },
    Field {
        name: "broadcast.submit_key",
        page: "broadcast",
        section: "Tera Term",
        key: "BroadcastSubmitKey",
        kind: Kind::Enum(&["None", "Enter", "CTRL+M"]),
        default: "None",
        label: None,
        doc: "`ttset.c:1501`. What the non-realtime broadcast dialog appends. The else arm is `None`, so an unrecognised value and an absent key agree.",
    },
    Field {
        name: "broadcast.max_history",
        page: "broadcast",
        section: "Tera Term",
        key: "MaxBroadcatHistory",
        kind: Kind::IntWord,
        default: "99",
        label: None,
        doc: "`ttset.c:1319`. Upstream's misspelling is the public key and is preserved. It caps the broadcast command history, which is not built yet.",
    },
    Field {
        name: "menu.disable_accelerator_send_break",
        page: "menu",
        section: "Tera Term",
        key: "DisableAcceleratorSendBreak",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1452`. Suppresses Alt+B while leaving Control > Send break alone.",
    },
    Field {
        name: "menu.disable_send_break",
        page: "menu",
        section: "Tera Term",
        key: "DisableMenuSendBreak",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1606`. Greys Control > Send break even when the transport supports it. This is independent of the accelerator switch above.",
    },
    Field {
        name: "menu.disable_accelerator_duplicate",
        page: "menu",
        section: "Tera Term",
        key: "DisableAcceleratorDuplicateSession",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1614`. Suppresses Alt+D while leaving File > Duplicate session.",
    },
    Field {
        name: "menu.accelerator_new_connection",
        page: "menu",
        section: "Tera Term",
        key: "AcceleratorNewConnection",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1617`, default **on**. Enables Alt+N for New connection.",
    },
    Field {
        name: "menu.accelerator_local_shell",
        page: "menu",
        section: "Tera Term",
        key: "AcceleratorCygwinConnection",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1620`, default **on**. Enables Alt+G for Cygwin connection; Sterna's equivalent command is Local shell.",
    },
    Field {
        name: "menu.disable_duplicate",
        page: "menu",
        section: "Tera Term",
        key: "DisableMenuDuplicateSession",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1624`. Greys Duplicate session.",
    },
    Field {
        name: "menu.disable_new_connection",
        page: "menu",
        section: "Tera Term",
        key: "DisableMenuNewConnection",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1628`. While already connected, greys New connection rather than allowing the current line to be replaced.",
    },
    Field {
        name: "macro.wait4all",
        page: "macro",
        section: "Tera Term",
        key: "Wait4allMacroCommand",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1602`, default off. Upstream mirrors received text into shared memory only under this switch so `wait4all` can inspect every macro process. Sterna has no process-global DDE ring; the command remains explicitly unsupported and the compatibility key still round-trips.",
    },
    Field {
        name: "window.jump_list",
        page: "window",
        section: "Tera Term",
        key: "JumpList",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "`ttset.c:1714`, default **on**. Adds recent connections to the Windows taskbar jump list. Qt/Linux has no equivalent owned by this process.",
    },
    Field {
        name: "window.corner_dontround",
        page: "window",
        section: "Tera Term",
        key: "WindowCornerDontround",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "`ttset.c:1993`, default off. Requests square corners from Windows 11 DWM. Window decoration belongs to the compositor on Linux, so it is carried only.",
    },
    Field {
        name: "window.toolbar",
        page: "window",
        section: "Sterna",
        key: "Toolbar",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "**`[Sterna]`**, because upstream has no toolbar and so no key to be compatible with. Whether the bar under the menu — port, connect, local echo — is shown. It exists as a setting rather than as chrome nobody can remove: `window.popup_menu` and `window.hide_title` are about the *menu*, so neither hides this, and Setup > Show toolbar writes it.",
    },
    Field {
        name: "window.panel_layout",
        page: "window",
        section: "Sterna",
        key: "PanelLayout",
        kind: Kind::Enum(&["single", "two", "four"]),
        default: "single",
        label: None,
        doc: "**`[Sterna]`**, because simultaneous panes are a Sterna window feature rather than terminal state. The value says how many connections this window shows at once: one terminal, two equal side-by-side panels, or four equal panels in a 2x2 grid. A missing or unrecognised spelling returns to the single-panel layout, so an edited file cannot leave the window without a usable view.",
    },
    Field {
        name: "window.quick_buttons",
        page: "window",
        section: "Sterna",
        key: "QuickButtons",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "**`[Sterna]`** again, and these two are only the *bar*: the buttons on it are a list, which no schema row can be, and they live in a `[Sterna Buttons]` section of their own (`tt-config/src/buttons.rs`).  On, but the bar is hidden while nobody has defined a button — an empty toolbar is chrome for nothing. So this is what Setup > Show quick buttons writes, not what decides whether the bar exists.",
    },
    Field {
        name: "window.quick_buttons_area",
        page: "window",
        section: "Sterna",
        key: "QuickButtonsArea",
        kind: Kind::Enum(&["right", "top", "bottom", "left"]),
        default: "right",
        label: None,
        doc: "Which edge the bar is on. Qt lets a toolbar be dragged to any of the four, and this is where it opens.  **The right**, not the top, which is where the other bar is. A terminal's rows are the scarce dimension — a window is usually far wider than the 80 columns it needs and exactly as tall as it can be — so a vertical bar costs nothing that is being used, and the labels have room to be words rather than abbreviations. An unrecognised spelling lands on the right as well, rather than hiding the bar.",
    },
    Field {
        name: "recent.serial_port",
        page: "recent",
        section: "Sterna",
        key: "SerialPort",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "The device path, not a number: `ComPort` is upstream's and cannot spell `/dev/serial/by-id/usb-FTDI_…`. Written as the port was opened, so it is the `open_path` a stable symlink resolves from rather than the `ttyUSB<n>` that moves on replug.",
    },
    Field {
        name: "recent.ssh_host",
        page: "recent",
        section: "Sterna",
        key: "SshHost",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "The SSH host the dialog was given — an alias out of `~/.ssh/config` as readily as a name, since that is what the dialog's combo box is filled from. Empty is the whole record's switch: nothing remembered, so the dialog opens as it does on a fresh install.",
    },
    Field {
        name: "recent.ssh_user",
        page: "recent",
        section: "Sterna",
        key: "SshUser",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "The user, where one was typed. Empty means \"whatever `~/.ssh/config` says\", which is not the same as an empty user name — so the absence round-trips rather than being resolved on the way in.",
    },
    Field {
        name: "recent.ssh_port",
        page: "recent",
        section: "Sterna",
        key: "SshPort",
        kind: Kind::IntWord,
        default: "0",
        label: None,
        doc: "The port, where one was asked for. Zero is the same kind of absence as an empty user: it leaves the choice to `~/.ssh/config` and, failing that, to 22.",
    },
    Field {
        name: "recent.ssh_identity",
        page: "recent",
        section: "Sterna",
        key: "SshIdentity",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "The private key the dialog was given, if it was given one. `~/.ssh/config` and the agent are consulted regardless, so this is an addition to them.",
    },
    Field {
        name: "recent.ssh_legacy",
        page: "recent",
        section: "Sterna",
        key: "SshLegacy",
        kind: Kind::Bool,
        default: "off",
        label: None,
        doc: "The pre-2020 algorithm switch, which is a property of the equipment at the far end rather than a preference — a console server that needed it once needs it every time, and that is the whole reason this record exists.",
    },
    Field {
        name: "recent.telnet_host",
        page: "recent",
        section: "Sterna",
        key: "TelnetHost",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "The telnet host. Empty is this record's switch, as `SshHost` is its own.",
    },
    Field {
        name: "recent.telnet_port",
        page: "recent",
        section: "Sterna",
        key: "TelnetPort",
        kind: Kind::IntWord,
        default: "23",
        label: None,
        doc: "The telnet port, kept separate from `[Tera Term] TCPPort` deliberately: that one is also what a bare host name on the command line opens, so remembering a console server's per-line port there would move an SSH connection to it.",
    },
    Field {
        name: "recent.telnet_mode",
        page: "recent",
        section: "Sterna",
        key: "TelnetMode",
        kind: Kind::Enum(&["auto", "negotiate", "framed", "raw"]),
        default: "auto",
        label: None,
        doc: "Which of the four framing/burst combinations was used (`TelnetMode::of`). `auto` is the shipped one: data until the first `IAC`, telnet after it.",
    },
    Field {
        name: "updates.check_on_startup",
        page: "updates",
        section: "Sterna",
        key: "CheckUpdatesOnStartup",
        kind: Kind::Bool,
        default: "on",
        label: None,
        doc: "Whether starting Sterna may look for a new release. On, so an installed terminal learns about a signed update without anyone remembering to ask.",
    },
    Field {
        name: "updates.last_check",
        page: "updates",
        section: "Sterna",
        key: "LastUpdateCheck",
        kind: Kind::Str,
        default: "",
        label: None,
        doc: "When the last look happened, as ISO-8601 UTC — the \"once a day\" half of it. Written when a check *starts*, so a release server that cannot be reached costs one request a day rather than one per launch. Empty means never, which is also what clearing it by hand means: check at the next start. Anything unparsable, and anything in the future — a clock moved back, an edited file — reads the same way, because the alternative is a terminal that never looks again.",
    },
];
