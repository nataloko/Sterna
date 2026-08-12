//! The `[TTProxy]` section's own string escaping, which no other section has.
//!
//! Every other setting in the file is written raw and read raw. The proxy's
//! are not, because the proxy is a plugin with its own INI layer: `YCL`'s
//! `IniFile::setString` C-escapes the value and wraps the result in double
//! quotes (`TTProxy/YCL/include/YCL/IniFile.h:258`), and `getString` runs
//! `StringUtil::unescape` over what comes back.
//!
//! **The quoting is load-bearing rather than decorative.**
//! `GetPrivateProfileString` trims whitespace around a value and strips one
//! matched pair of quotes, so `TelnetConnectedMessage=-- Connected to ` loses
//! the trailing space that makes it a prompt rather than a prefix of one, and
//! `TelnetConnectedMessage="-- Connected to "` does not. A writer that emits
//! the bare value produces a file whose own reader gives back a different
//! string — which is exactly what this port's round-trip test caught.
//!
//! The escaping is C's: the seven named control characters, three-digit octal
//! for the rest, and a backslash in front of `'`, `"`, `?` and `\`. Two things
//! about the decoder are worth knowing before assuming it is just C:
//!
//! - **`\x` is accepted and never produced.** `unescape` takes one or two hex
//!   digits after it; `escape` only ever emits octal. So a hand-written
//!   `\x1b` works and will come back as `\033`.
//! - **An escape that decodes to NUL is left exactly as written.** The test
//!   that decides whether anything was decoded is `ch != '\0'`
//!   (`StringUtil.h:255`), so `\000` and `\x00` pass through as those four
//!   characters rather than becoming a byte no INI file could hold anyway.
//!
//! An unrecognised escape is also left alone, backslash included, which is
//! what makes a Windows path in one of these values survive being read by a
//! Tera Term that never escaped it.

/// The seven control characters with a letter of their own, and the letters.
/// Upstream keeps them as two parallel strings (`StringUtil.h:40`); the order
/// is what pairs them.
const NAMED: [(u8, u8); 7] = [
    (0x07, b'a'),
    (0x08, b'b'),
    (0x0c, b'f'),
    (0x0a, b'n'),
    (0x0d, b'r'),
    (0x09, b't'),
    (0x0b, b'v'),
];

/// C-escape `value` and wrap it in double quotes, ready for [`Ini::set`].
///
/// [`Ini::set`]: crate::ini::Ini::set
pub fn quote(value: &str) -> String {
    // Bytes rather than characters, and assembled at the end: `u8 as char` is
    // a *codepoint* conversion, so it would turn every UTF-8 continuation
    // byte into a Latin-1 letter of its own.
    let mut out: Vec<u8> = Vec::with_capacity(value.len() + 2);
    out.push(b'"');
    for &b in value.as_bytes() {
        // Upstream's test is `'\0' < ch && ch < ' '` on a **signed** char, so
        // a byte with the top bit set is negative and takes neither arm —
        // which is what leaves UTF-8 alone here.
        let control = (1..0x20).contains(&b);
        if !control && !matches!(b, b'\'' | b'"' | b'?' | b'\\') {
            out.push(b);
            continue;
        }
        out.push(b'\\');
        match NAMED.iter().find(|(c, _)| *c == b) {
            Some((_, letter)) => out.push(*letter),
            None if control => {
                for shift in [6, 3, 0] {
                    out.push(b'0' + ((b >> shift) & 0x07));
                }
            }
            None => out.push(b),
        }
    }
    out.push(b'"');
    String::from_utf8_lossy(&out).into_owned()
}

/// Undo [`quote`]'s escaping. The surrounding quotes are gone by now —
/// `GetPrivateProfileString` strips them and so does [`Ini::get`].
///
/// [`Ini::get`]: crate::ini::Ini::get
pub fn unescape(value: &str) -> String {
    let src = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'\\' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        let Some(&next) = src.get(i + 1) else {
            // A trailing backslash is not an escape and is kept.
            out.push(b'\\');
            i += 1;
            continue;
        };
        let (decoded, used) = match next {
            b'\'' | b'"' | b'\\' | b'?' => (Some(next), 2),
            b'x' | b'X' => {
                let first = src.get(i + 2).and_then(|c| hex(*c));
                match first {
                    None => (None, 0),
                    Some(hi) => match src.get(i + 3).and_then(|c| hex(*c)) {
                        Some(lo) => (Some(hi << 4 | lo), 4),
                        None => (Some(hi), 3),
                    },
                }
            }
            b'0'..=b'7' => {
                let mut value = next - b'0';
                let mut used = 2;
                for offset in [i + 2, i + 3] {
                    match src.get(offset).and_then(|c| octal(*c)) {
                        Some(d) => {
                            value = value << 3 | d;
                            used += 1;
                        }
                        None => break,
                    }
                }
                (Some(value), used)
            }
            _ => match NAMED.iter().find(|(_, letter)| *letter == next) {
                Some((control, _)) => (Some(*control), 2),
                None => (None, 0),
            },
        };
        // `ch != '\0'` is upstream's "did that decode to anything" test, so an
        // escape whose value is zero is copied through as written.
        match decoded {
            Some(0) | None => {
                out.push(b'\\');
                i += 1;
            }
            Some(byte) => {
                out.push(byte);
                i += used;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

fn octal(c: u8) -> Option<u8> {
    (b'0'..=b'7').contains(&c).then(|| c - b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_value_is_only_quoted() {
        assert_eq!(quote("Username:"), "\"Username:\"");
        assert_eq!(unescape("Username:"), "Username:");
    }

    /// The reason any of this exists: without the quotes the file's own
    /// reader trims the space off the end.
    #[test]
    fn a_trailing_space_survives_the_quotes() {
        assert_eq!(quote(">> Host name: "), "\">> Host name: \"");
        assert_eq!(quote("-- Connected to "), "\"-- Connected to \"");
    }

    #[test]
    fn the_four_punctuation_escapes_round_trip() {
        for (plain, escaped) in [
            ("a\"b", "\"a\\\"b\""),
            ("a\\b", "\"a\\\\b\""),
            ("a'b", "\"a\\'b\""),
            ("a?b", "\"a\\?b\""),
        ] {
            assert_eq!(quote(plain), escaped, "escaping {plain:?}");
            let inner = &escaped[1..escaped.len() - 1];
            assert_eq!(unescape(inner), plain, "decoding {inner:?}");
        }
    }

    #[test]
    fn the_named_controls_are_letters_and_the_rest_are_octal() {
        assert_eq!(quote("a\tb\nc"), "\"a\\tb\\nc\"");
        assert_eq!(quote("\x1b["), "\"\\033[\"");
        assert_eq!(unescape("a\\tb\\nc"), "a\tb\nc");
        assert_eq!(unescape("\\033["), "\x1b[");
    }

    /// Accepted on the way in, never produced on the way out.
    #[test]
    fn hex_escapes_are_read_and_not_written() {
        assert_eq!(unescape("\\x1b"), "\x1b");
        assert_eq!(unescape("\\X1B"), "\x1b");
        // One digit is enough, which is upstream's `ch1`-only arm.
        assert_eq!(unescape("\\x7g"), "\u{7}g");
        assert_eq!(quote("\x1b"), "\"\\033\"");
    }

    /// `ch != '\0'` is the "did it decode" test upstream, so a zero escape is
    /// not an escape at all.
    #[test]
    fn an_escape_that_decodes_to_nul_is_left_alone() {
        assert_eq!(unescape("a\\000b"), "a\\000b");
        assert_eq!(unescape("a\\x00b"), "a\\x00b");
    }

    #[test]
    fn an_unknown_escape_keeps_its_backslash() {
        assert_eq!(unescape(r"C:\Users\q"), r"C:\Users\q");
        assert_eq!(unescape("trailing\\"), "trailing\\");
    }

    /// The other half of that, and the reason a hand-edited value is where
    /// this bites: `\t` **is** an escape, so a Windows path written into one
    /// of these keys without doubling its backslashes comes back with a tab
    /// in it. Upstream's writer always doubles them, so only a file somebody
    /// edited by hand can reach this.
    #[test]
    fn a_path_whose_next_letter_names_a_control_loses_it() {
        assert_eq!(unescape(r"C:\temp"), "C:\temp");
        assert_eq!(quote(r"C:\temp"), r#""C:\\temp""#);
        assert_eq!(unescape(r"C:\\temp"), r"C:\temp");
    }

    /// Octal takes at most three digits, so a fourth is a literal character.
    #[test]
    fn octal_stops_at_three_digits() {
        assert_eq!(unescape("\\0331"), "\x1b1");
        // Two digits, then a non-octal: the escape ends where the digits do.
        assert_eq!(unescape("\\33x"), "\x1bx");
    }

    #[test]
    fn utf8_passes_through_untouched() {
        // Upstream's signed-char test leaves every byte above 0x7f alone, and
        // a value written that way decodes back to the same text.
        let text = "café — ホスト";
        assert_eq!(quote(text), format!("\"{text}\""));
        assert_eq!(unescape(text), text);
    }

    /// A file hand-written with octal for each UTF-8 byte still comes back as
    /// the character, because the bytes are reassembled before the decode.
    #[test]
    fn octal_bytes_reassemble_into_a_character() {
        assert_eq!(unescape("caf\\303\\251"), "café");
    }
}
