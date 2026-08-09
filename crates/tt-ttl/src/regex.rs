//! Oniguruma, and the three settings `regexoption` turns.
//!
//! `strmatch`, `strreplace` and `waitregex` all go through one function
//! upstream — `FindRegexStringOne` (`ttmdde.c:594`) — and it compiles the
//! pattern fresh on every call against three globals that `regexoption` sets.
//! That is reproduced exactly, globals included: they live on the [`Interp`]
//! rather than in a `static`, which is the only difference and is invisible to
//! a macro.
//!
//! **The engine is Oniguruma itself, through the `onig` crate.** Not a
//! lookalike: `regexoption` names eleven syntaxes and some thirty encodings
//! that no other engine has, and the commands are documented in terms of Ruby's
//! dialect, backreferences and look-around included. Reimplementing that would
//! be reimplementing Oniguruma. Tera Term vendors and builds the same library
//! (`libs/buildoniguruma.cmake`), so this is the rule about preferring real
//! upstream code over a stub, applied to a dependency.
//!
//! Two consequences are worth stating plainly. Oniguruma is C, so the core
//! crate now compiles a C library — `cc` only, with `default-features = false`
//! to skip `bindgen` and its `libclang`. And it backtracks, so a pattern with
//! nested quantifiers can be made to take exponential time; `waitregex` matches
//! against **what the far end sent**, which makes that reachable from the other
//! side of a connection. Upstream has exactly the same exposure and a macro
//! chooses its own patterns, so it is reproduced rather than fenced — but a
//! `waitregex` in an unattended script is a place to think about `timeout`.

use onig::{EncodedBytes, Regex, RegexOptions, SearchOptions, Syntax};

/// Everything `regexoption` sets, and what every match here reads.
///
/// The defaults are `ttmdde.c:61-63`: no options, UTF-8, **Ruby** syntax —
/// which is not the same as `ONIG_SYNTAX_DEFAULT` even though Oniguruma
/// happens to point that at Ruby too, because `regexoption 'SYNTAX_DEFAULT'`
/// is a distinct thing a macro can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegexConfig {
    pub opt: RegexOptions,
    pub enc: Enc,
    pub syn: Syn,
}

impl Default for RegexConfig {
    fn default() -> Self {
        RegexConfig {
            opt: RegexOptions::REGEX_OPTION_NONE,
            enc: Enc::Utf8,
            syn: Syn::Ruby,
        }
    }
}

/// The encodings `regexoption` knows, in `ttl.cpp:3602`'s order.
///
/// Kept as an enum rather than as the `OnigEncoding` pointer so that
/// [`RegexConfig`] stays plain data — a raw pointer on the interpreter would
/// make the whole thing awkward to move between threads for no gain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Enc {
    Ascii,
    Iso8859_1,
    Iso8859_2,
    Iso8859_3,
    Iso8859_4,
    Iso8859_5,
    Iso8859_6,
    Iso8859_7,
    Iso8859_8,
    Iso8859_9,
    Iso8859_10,
    Iso8859_11,
    Iso8859_13,
    Iso8859_14,
    Iso8859_15,
    Iso8859_16,
    Utf8,
    Utf16Be,
    Utf16Le,
    Utf32Be,
    Utf32Le,
    EucJp,
    EucTw,
    EucKr,
    EucCn,
    Sjis,
    Koi8R,
    Cp1251,
    Big5,
    Gb18030,
}

impl Enc {
    /// The `OnigEncoding` this names.
    fn onig(self) -> onig_sys::OnigEncoding {
        use onig_sys::*;
        let p: *mut OnigEncodingType = match self {
            Enc::Ascii => &raw mut OnigEncodingASCII,
            Enc::Iso8859_1 => &raw mut OnigEncodingISO_8859_1,
            Enc::Iso8859_2 => &raw mut OnigEncodingISO_8859_2,
            Enc::Iso8859_3 => &raw mut OnigEncodingISO_8859_3,
            Enc::Iso8859_4 => &raw mut OnigEncodingISO_8859_4,
            Enc::Iso8859_5 => &raw mut OnigEncodingISO_8859_5,
            Enc::Iso8859_6 => &raw mut OnigEncodingISO_8859_6,
            Enc::Iso8859_7 => &raw mut OnigEncodingISO_8859_7,
            Enc::Iso8859_8 => &raw mut OnigEncodingISO_8859_8,
            Enc::Iso8859_9 => &raw mut OnigEncodingISO_8859_9,
            Enc::Iso8859_10 => &raw mut OnigEncodingISO_8859_10,
            Enc::Iso8859_11 => &raw mut OnigEncodingISO_8859_11,
            Enc::Iso8859_13 => &raw mut OnigEncodingISO_8859_13,
            Enc::Iso8859_14 => &raw mut OnigEncodingISO_8859_14,
            Enc::Iso8859_15 => &raw mut OnigEncodingISO_8859_15,
            Enc::Iso8859_16 => &raw mut OnigEncodingISO_8859_16,
            Enc::Utf8 => &raw mut OnigEncodingUTF8,
            Enc::Utf16Be => &raw mut OnigEncodingUTF16_BE,
            Enc::Utf16Le => &raw mut OnigEncodingUTF16_LE,
            Enc::Utf32Be => &raw mut OnigEncodingUTF32_BE,
            Enc::Utf32Le => &raw mut OnigEncodingUTF32_LE,
            Enc::EucJp => &raw mut OnigEncodingEUC_JP,
            Enc::EucTw => &raw mut OnigEncodingEUC_TW,
            Enc::EucKr => &raw mut OnigEncodingEUC_KR,
            Enc::EucCn => &raw mut OnigEncodingEUC_CN,
            Enc::Sjis => &raw mut OnigEncodingSJIS,
            Enc::Koi8R => &raw mut OnigEncodingKOI8_R,
            Enc::Cp1251 => &raw mut OnigEncodingCP1251,
            Enc::Big5 => &raw mut OnigEncodingBIG5,
            Enc::Gb18030 => &raw mut OnigEncodingGB18030,
        };
        p
    }
}

/// The syntaxes `regexoption` knows (`ttl.cpp:3754`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syn {
    Default,
    Asis,
    PosixBasic,
    PosixExtended,
    Emacs,
    Grep,
    GnuRegex,
    Java,
    Perl,
    PerlNg,
    Ruby,
}

impl Syn {
    fn onig(self) -> &'static Syntax {
        match self {
            Syn::Default => Syntax::default(),
            Syn::Asis => Syntax::asis(),
            Syn::PosixBasic => Syntax::posix_basic(),
            Syn::PosixExtended => Syntax::posix_extended(),
            Syn::Emacs => Syntax::emacs(),
            Syn::Grep => Syntax::grep(),
            Syn::GnuRegex => Syntax::gnu_regex(),
            Syn::Java => Syntax::java(),
            Syn::Perl => Syntax::perl(),
            Syn::PerlNg => Syntax::perl_ng(),
            Syn::Ruby => Syntax::ruby(),
        }
    }
}

/// A pattern Oniguruma refused, carrying the message it gave.
///
/// Upstream prints that message to `stderr` and returns -1 (`ttmdde.c:611`),
/// which in a windowed program goes nowhere at all. It is kept here so a host
/// that wants to log it can, and because "which pattern and why" is the one
/// thing a macro author cannot find out from `result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadPattern(pub String);

impl std::fmt::Display for BadPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BadPattern {}

/// What a successful search found.
pub struct Found {
    /// Byte offset of the whole match, which is what `strmatch` reports as
    /// `result` after adding one.
    pub at: usize,
    /// `matchstr` — group 0.
    pub whole: Vec<u8>,
    /// `groupmatchstr1` upwards, in order and starting at group 1.
    pub groups: Vec<Vec<u8>>,
}

/// `FindRegexStringOne` — search `target` for `pattern`.
///
/// [`Err`] is upstream's -1, a pattern Oniguruma refused; the commands turn it
/// into `result` 0 or -1 depending which one asked. `Ok(None)` is
/// `ONIG_MISMATCH`.
///
/// **The pattern is compiled on every call.** Upstream does, down to
/// `onig_free` and `onig_end` afterwards, so a `waitregex` recompiles once per
/// line of output. Caching it would be a visible change the first time a macro
/// built a pattern from a variable inside a loop and expected the new one to
/// take effect — and the compile is not what makes a bad pattern slow anyway.
///
/// **A group that did not participate comes back empty**, which is upstream's
/// behaviour by accident rather than by design: it uses the region's `beg` and
/// `end` as indices into the target, and for such a group both are -1, so it
/// writes a NUL one byte *before* the buffer, reads the empty string that
/// leaves, and puts the byte back. An out-of-bounds write on a plain
/// `strmatch 'abc' '(x)?abc'`, and one more for the list; not reproduced,
/// since the empty string is what it produces anyway.
pub fn search(
    cfg: &RegexConfig,
    pattern: &[u8],
    target: &[u8],
) -> Result<Option<Found>, BadPattern> {
    let enc = cfg.enc.onig();
    let re = Regex::with_options_and_encoding(
        EncodedBytes::from_parts(pattern, enc),
        cfg.opt,
        cfg.syn.onig(),
    )
    .map_err(|e| BadPattern(e.to_string()))?;

    let mut region = onig::Region::new();
    let subject = EncodedBytes::from_parts(target, enc);
    let at = re.search_with_encoding(
        subject,
        0,
        target.len(),
        SearchOptions::SEARCH_OPTION_NONE,
        Some(&mut region),
    );
    let Some(at) = at else {
        return Ok(None);
    };

    let slice = |i: usize| -> Vec<u8> {
        match region.pos(i) {
            Some((b, e)) if b <= e && e <= target.len() => target[b..e].to_vec(),
            _ => Vec::new(),
        }
    };
    Ok(Some(Found {
        at,
        whole: slice(0),
        groups: (1..region.len()).map(slice).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(cfg: &RegexConfig, pat: &str, target: &str) -> Option<Found> {
        search(cfg, pat.as_bytes(), target.as_bytes()).unwrap()
    }

    #[test]
    fn the_default_dialect_is_ruby_and_has_what_ruby_has() {
        let cfg = RegexConfig::default();
        // Backreference.
        let m = find(&cfg, r"^(\w+)@(\w+)\1$", "ab@cdab").unwrap();
        assert_eq!(m.at, 0);
        assert_eq!(m.whole, b"ab@cdab");
        assert_eq!(m.groups, vec![b"ab".to_vec(), b"cd".to_vec()]);
        // Look-behind.
        let m = find(&cfg, r"(?<=login: )\w+", "xx login: root").unwrap();
        assert_eq!(m.at, 10);
        assert_eq!(m.whole, b"root");
        assert!(find(&cfg, r"^(\w+)@(\w+)\1$", "ab@cdef").is_none());
    }

    #[test]
    fn the_offset_is_bytes_and_is_what_strmatch_reports() {
        let cfg = RegexConfig::default();
        assert_eq!(find(&cfg, "cde", "abcdef").unwrap().at, 2);
        // Multi-byte: the offset counts bytes, so `strmatch` reports a byte
        // position and not a character one.
        assert_eq!(find(&cfg, "b", "\u{e9}\u{e9}b").unwrap().at, 4);
    }

    #[test]
    fn a_group_that_did_not_participate_is_empty_rather_than_missing() {
        let cfg = RegexConfig::default();
        let m = find(&cfg, "(x)?abc", "abc").unwrap();
        assert_eq!(m.whole, b"abc");
        assert_eq!(m.groups, vec![Vec::<u8>::new()]);
    }

    #[test]
    fn a_pattern_oniguruma_refuses_is_an_error_and_not_a_mismatch() {
        let cfg = RegexConfig::default();
        assert!(search(&cfg, b"(unclosed", b"x").is_err());
        assert!(search(&cfg, b"*", b"x").is_err());
    }

    #[test]
    fn the_options_do_what_their_names_say() {
        let mut cfg = RegexConfig::default();
        assert!(find(&cfg, "ABC", "abc").is_none());
        cfg = RegexConfig {
            opt: RegexOptions::REGEX_OPTION_IGNORECASE,
            ..RegexConfig::default()
        };
        assert!(find(&cfg, "ABC", "abc").is_some());

        // FIND_LONGEST takes the longest match rather than the leftmost-first.
        cfg = RegexConfig::default();
        assert_eq!(find(&cfg, "a|ab", "ab").unwrap().whole, b"a");
        cfg.opt = RegexOptions::REGEX_OPTION_FIND_LONGEST;
        assert_eq!(find(&cfg, "a|ab", "ab").unwrap().whole, b"ab");

        // DONT_CAPTURE_GROUP turns `(...)` into a plain group.
        cfg.opt = RegexOptions::REGEX_OPTION_DONT_CAPTURE_GROUP;
        assert!(find(&cfg, "(a)b", "ab").unwrap().groups.is_empty());
    }

    #[test]
    fn the_syntaxes_really_are_different_engines() {
        // POSIX basic has no alternation and no `+`: both are literals.
        let mut cfg = RegexConfig {
            syn: Syn::PosixBasic,
            ..RegexConfig::default()
        };
        assert_eq!(find(&cfg, "a+", "xa+y").unwrap().whole, b"a+");
        assert!(find(&cfg, "a|b", "b").is_none());
        assert_eq!(find(&cfg, "a|b", "a|b").unwrap().whole, b"a|b");

        // ASIS is no metacharacters at all, which is how a macro asks for a
        // literal search through a command that takes a pattern.
        cfg.syn = Syn::Asis;
        assert_eq!(find(&cfg, "a.c", "xa.cy").unwrap().whole, b"a.c");
        assert!(find(&cfg, "a.c", "abc").is_none());

        // ...and extended has them back.
        cfg.syn = Syn::PosixExtended;
        assert_eq!(find(&cfg, "a+", "xaay").unwrap().whole, b"aa");
    }

    #[test]
    fn a_non_utf8_encoding_matches_the_bytes_it_was_told_about() {
        let mut cfg = RegexConfig {
            enc: Enc::Sjis,
            ..RegexConfig::default()
        };
        // 0x83 0x41 is katakana A in Shift-JIS. Its second byte is an ASCII
        // 'A', so an engine reading the bytes as ASCII would match `A` inside
        // it; one told the encoding will not.
        let target = [0x83, 0x41, b'B'];
        assert!(search(&cfg, b"A", &target).unwrap().is_none());
        assert_eq!(search(&cfg, b"B", &target).unwrap().unwrap().at, 2);

        // The same bytes read as Latin-1 have no multi-byte characters, so the
        // `A` is there to be found.
        cfg.enc = Enc::Iso8859_1;
        assert_eq!(search(&cfg, b"A", &target).unwrap().unwrap().at, 1);
    }
}
