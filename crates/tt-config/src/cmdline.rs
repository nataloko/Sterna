//! The command line, tokenised the way Tera Term tokenises one.
//!
//! `GetParam` and `DequoteParam` (`ttlib.c:879`, `:917`) live here rather than
//! in the crate that first needed them because there are three callers and one
//! of them ships: `ttpmacro`'s launcher (`tt-ttl`), `_ParseParam` (`ttset.c`,
//! this crate) and TTXSSH's hook over it. Upstream puts `_ParseParam` in the
//! same DLL as the INI reader for the same reason — the two are one file
//! format's worth of front door, and the first thing the parser does with a
//! `/F=` is read a settings file.
//!
//! **The tokeniser is upstream's, not the C runtime's.** A backslash is an
//! ordinary character (which it has to be, since these are Windows paths), a
//! `""` inside a quoted run is one literal quote, and an unquoted `;` **ends
//! the command line** — everything after it is a comment. Reaching for
//! `CommandLineToArgvW` semantics gives a parser that agrees on every example
//! in the documentation and disagrees on the first path with a space in it.

/// `GetParam` (`ttlib.c:879`) — one token and what is left after it.
///
/// Quotes are *kept*: upstream splits with them still in place and takes them
/// out afterwards with [`dequote_param`], which is why a token can come back
/// looking like `"a b"`. `None` is upstream's NULL — the end of the line, or an
/// unquoted `;`, which is a comment and ends it early.
///
/// `size` is the caller's buffer, counted the way the function counts it: a
/// token is truncated at `size - 1` characters. `_ParseParam` and `ttpmacro`
/// both pass 512; TTXSSH allocates one as long as the line and so never
/// truncates.
pub fn get_param(param: &[u8], size: usize) -> Option<(Vec<u8>, &[u8])> {
    let mut i = 0;
    while matches!(param.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    match param.get(i) {
        None | Some(b';') => return None,
        _ => {}
    }

    let mut buf = Vec::new();
    let mut quoted = false;
    while let Some(&b) = param.get(i) {
        if !quoted && matches!(b, b';' | b' ' | b'\t') {
            break;
        }
        if b == b'"' {
            if param.get(i + 1) != Some(&b'"') {
                quoted = !quoted;
            } else {
                // `""`: the first quote is copied here and the second by the
                // unconditional copy below, so a doubled quote survives
                // tokenising whole and does not toggle the state.
                push_capped(&mut buf, b'"', size);
                i += 1;
            }
        }
        push_capped(&mut buf, param[i], size);
        i += 1;
    }
    // Upstream drops a trailing `;` here — `if (!quoted && buff[i-1] == ';')`.
    // It cannot fire: a `;` only reaches the buffer while `quoted`, and nothing
    // between that copy and the loop test can clear the flag. Transcribed as a
    // comment rather than as an unreachable branch, and it is also where the
    // function reads `buff[-1]` if it is ever called with a size of 1.
    Some((buf, &param[i..]))
}

fn push_capped(buf: &mut Vec<u8>, b: u8, size: usize) {
    if buf.len() + 1 < size {
        buf.push(b);
    }
}

/// `DequoteParam` (`ttlib.c:917`) — take the quotes back out.
///
/// A quote toggles the state and vanishes; a `""` *inside* a quoted run is one
/// literal quote. So `"a b"` is `a b`, `""` is the empty string — which
/// `params_array.bat` passes deliberately — and `""""` is a single `"`.
///
/// No length cap: every caller dequotes in place into the buffer
/// [`get_param`] just filled, and taking quotes out cannot lengthen a token.
pub fn dequote_param(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut quoted = false;
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'"' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        i += 1;
        if quoted && src.get(i) == Some(&b'"') {
            out.push(b'"');
            i += 1;
        } else {
            quoted = !quoted;
        }
    }
    out
}

/// Every token of a command line, dequoted — the loop both `_ParseParam` and
/// `TTXParseParam` open with, minus the first term.
///
/// "The first term shuld be executable filename of Tera Term", says the comment
/// above the untested `GetParam` that discards it. A line consisting of nothing
/// but that term yields no tokens.
pub fn tokens(line: &[u8], size: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur = match get_param(line, size) {
        Some((_, rest)) => rest,
        None => return out,
    };
    while let Some((tok, rest)) = get_param(cur, size) {
        cur = rest;
        out.push(dequote_param(&tok));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 512 is `MaxStrLen`, which both `ttset.c:75` and `ttmdef.h:34` define for
    /// themselves at the same value.
    const MAX: usize = 512;

    #[test]
    fn a_doubled_quote_is_one_literal_quote() {
        assert_eq!(dequote_param(br#""a b""#), b"a b");
        assert_eq!(dequote_param(br#""""#), b"");
        assert_eq!(dequote_param(br#""""""#), br#"""#);
        // Outside a quoted run, `""` is still just the toggle twice.
        assert_eq!(dequote_param(br#"a""b"#), b"ab");
        assert_eq!(dequote_param(br#""a""b""#), br#"a"b"#);
        // A quote in the middle of a word opens a run like any other.
        assert_eq!(dequote_param(br#"a"b c"d"#), b"ab cd");
    }

    /// `GetParam` hands the quotes on rather than removing them, which is what
    /// makes the doubled-quote rule work at all.
    #[test]
    fn get_param_keeps_the_quotes_for_dequote_param() {
        let (tok, rest) = get_param(br#""a b" c"#, MAX).unwrap();
        assert_eq!(tok, br#""a b""#);
        assert_eq!(rest, b" c");
        assert_eq!(dequote_param(&tok), b"a b");
        // An unterminated quote runs to the end of the line.
        let (tok, rest) = get_param(br#""a b c"#, MAX).unwrap();
        assert_eq!(tok, br#""a b c"#);
        assert_eq!(rest, b"");
    }

    #[test]
    fn the_tokeniser_is_not_the_c_runtimes() {
        // A backslash is an ordinary character, which it has to be.
        assert_eq!(tokens(br"tt c:\dir\m.ttl", MAX), [br"c:\dir\m.ttl"]);
        // An unquoted `;` ends the line, and everything after it is lost.
        assert_eq!(tokens(b"tt a ; b c", MAX), [b"a"]);
        // A quoted one is just a character.
        assert_eq!(
            tokens(br#"tt "a;b" c"#, MAX),
            [b"a;b".to_vec(), b"c".to_vec()]
        );
        // Tabs separate, and a run of separators is one.
        assert_eq!(tokens(b"tt \t a  b", MAX), [b"a", b"b"]);
        // The first term is the executable and is discarded.
        assert!(tokens(b"ttermpro.exe", MAX).is_empty());
        assert!(tokens(b"", MAX).is_empty());
    }

    #[test]
    fn a_token_stops_one_short_of_the_buffer() {
        let long = vec![b'x'; 600];
        let line = [b"tt ".as_slice(), &long].concat();
        assert_eq!(tokens(&line, MAX)[0].len(), MAX - 1);
        // The size is the caller's, not a constant of the tokeniser.
        assert_eq!(tokens(&line, 4)[0], b"xxx");
    }
}
