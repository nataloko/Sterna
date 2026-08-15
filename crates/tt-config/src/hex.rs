//! `Hex2Str` (`ttlib.c:406`) — the escape two settings are stored in.
//!
//! `Answerback` and `DelimList` are read out of the INI as text and then run
//! through this, which copies bytes straight through and reads `$` as the lead
//! of a two-digit hexadecimal escape. It is how a setting whose value is
//! arbitrary bytes survives a file format that has no way to write a control
//! character, a space at either end, or a line break.
//!
//! It lives here rather than beside either setting because both of them are
//! *file* values, and because the three ways it surprises a reader are
//! properties of the escape rather than of what the bytes are for.

/// `ConvHexChar` (`ttlib.c:390`), which answers **0** rather than failing.
fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - 0x30,
        b'A'..=b'F' => b - 0x37,
        b'a'..=b'f' => b - 0x57,
        _ => 0,
    }
}

/// Decode `s` the way `Hex2Str` does, stopping at `max` bytes.
///
/// Three behaviours are upstream's and none of them is an error:
///
/// * A digit that is not hexadecimal reads as zero, so `$ZZ` is a NUL.
/// * A `$` with fewer than two characters behind it borrows `'0'` for each one
///   it is missing — so a trailing `$` is a NUL and `$A` is `0xA0`, rather than
///   either being left alone or refused.
/// * `$` is the only escape. There is no way to write a literal one except as
///   `$24`, which is why upstream's own `DelimList` default opens with
///   `$20!"#$24%…`.
///
/// `max` is the C buffer this filled: 32 for `Answerback` (`tttypes.h:350`),
/// unbounded for `DelimList`, which upstream allocates instead. Anything past
/// it is dropped silently, as it is there.
///
/// The one place this cannot be upstream is the input: a `String` here is
/// UTF-8 where upstream holds the ANSI code page, so a value with a character
/// above U+007F in it decodes to different bytes. That difference belongs to
/// the file's encoding rather than to this function, and `ini::Encoding` is
/// where it is decided.
pub fn hex_decode(s: &str, max: usize) -> Vec<u8> {
    let src = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() && out.len() < max {
        let mut b = src[i];
        if b == b'$' {
            i += 1;
            let hi = if i < src.len() { src[i] } else { b'0' };
            i += 1;
            let lo = if i < src.len() { src[i] } else { b'0' };
            b = (hex_digit(hi) << 4) + hex_digit(lo);
        }
        out.push(b);
        i += 1;
    }
    out
}

/// `Hex2StrW` (`ttlib_static_cpp.cpp:837`) — the same escape decoded to *text*
/// rather than to bytes.
///
/// Upstream has two of these and the difference is not cosmetic: `Answerback`
/// goes on the wire and is bytes, while `DelimList` is compared against what is
/// on the screen and is characters. `$41` is the letter A either way; `$E9` is
/// one byte here and the character U+00E9 there, and there is no length cap
/// because upstream allocates instead of filling a fixed buffer.
///
/// That allocation is where its own defect is — see `docs/upstream-bugs.md`.
pub fn hex_decode_str(s: &str) -> String {
    let src: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < src.len() {
        let mut c = src[i];
        if c == '$' {
            i += 1;
            let hi = if i < src.len() { src[i] } else { '0' };
            i += 1;
            let lo = if i < src.len() { src[i] } else { '0' };
            let v = (hex_digit(hi as u32 as u8) as u32) << 4 | hex_digit(lo as u32 as u8) as u32;
            // Always below 0x100, so this cannot be a surrogate or out of
            // range — `expect` rather than a fallback that would hide a
            // change to `hex_digit`.
            c = char::from_u32(v).expect("a byte is always a codepoint");
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The escape [`hex_decode_str`] undoes, applied to text that has to survive a
/// round trip through an INI value.
///
/// Upstream has no encoder for this — its two settings are written by a dialog
/// that escapes as it goes (`Str2HexW`, for `DelimList`) — so the rule here is
/// this port's, and it is the smallest one that round-trips: escape `$`,
/// because it is the lead character, and escape anything the file format
/// cannot carry. That is the C0 controls and DEL, which `Ini::set` refuses a
/// line ending among, plus a space at either end, which
/// `GetPrivateProfileString` trims off before the caller ever sees it.
///
/// Everything else is left alone, so a value stays readable in the file — the
/// point of storing `show version$0D` rather than a wall of hexadecimal.
pub fn hex_escape_str(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let edge = i == 0 || i + 1 == chars.len();
        if c == '$' || c < ' ' || c == '\x7f' || (c == ' ' && edge) {
            out.push_str(&format!("${:02X}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(hex_decode("VT100", 32), b"VT100");
    }

    #[test]
    fn dollar_leads_two_hex_digits() {
        assert_eq!(hex_decode("VT100$0D", 32), b"VT100\r");
        assert_eq!(hex_decode("$41$42", 32), b"AB");
        // Case does not matter, either way round.
        assert_eq!(hex_decode("$0d$0A", 32), b"\r\n");
    }

    #[test]
    fn a_dollar_that_runs_out_borrows_zeroes() {
        // Both of these are `AGENTS.md`-shaped: the obvious guess is that an
        // incomplete escape is left as text, and it is a NUL byte instead.
        assert_eq!(hex_decode("$", 32), b"\0");
        assert_eq!(hex_decode("x$", 32), b"x\0");
        assert_eq!(hex_decode("$A", 32), b"\xa0");
    }

    #[test]
    fn a_digit_that_is_not_hex_is_zero() {
        assert_eq!(hex_decode("$ZZ", 32), b"\0");
        assert_eq!(hex_decode("$4Z", 32), b"\x40");
    }

    #[test]
    fn the_delimiter_default_decodes_to_a_space_and_a_dollar() {
        // `ttset.c:1167`'s own default, which is the reason this escape exists
        // at all: the list has to start with a space and to contain a `$`.
        let d = hex_decode("$20!\"#$24%&'()*+,-./:;<=>?@[\\]^`{|}~", usize::MAX);
        assert_eq!(d, b" !\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~");
    }

    /// The wide decoder is the same escape and a different result type, which
    /// is the whole of why upstream has two.
    #[test]
    fn the_wide_decoder_gives_characters_not_bytes() {
        assert_eq!(hex_decode_str("$20!\"#$24%"), " !\"#$%");
        // One byte in the answerback, one *character* here.
        assert_eq!(hex_decode_str("$E9"), "\u{e9}");
        assert_eq!(hex_decode("$E9", 32), b"\xe9");
        // The same borrowing at the end, and the same zero for a bad digit.
        assert_eq!(hex_decode_str("$"), "\0");
        assert_eq!(hex_decode_str("$ZZ"), "\0");
        // ...and no cap, because upstream allocates rather than filling a
        // buffer.
        assert_eq!(hex_decode_str(&"a".repeat(600)).len(), 600);
        // Text on either side of an escape survives as text.
        assert_eq!(hex_decode_str("日$20本"), "日 本");
    }

    #[test]
    fn the_buffer_is_a_hard_stop() {
        assert_eq!(hex_decode("abcdef", 3), b"abc");
        // The escape is counted as the one byte it produces, not as three.
        assert_eq!(hex_decode("$41$42$43", 2), b"AB");
    }

    #[test]
    fn the_encoder_escapes_only_what_has_to_be() {
        // The ordinary case is meant to stay legible in the file.
        assert_eq!(hex_escape_str("show version\r"), "show version$0D");
        assert_eq!(hex_escape_str("a\tb\nc"), "a$09b$0Ac");
        assert_eq!(hex_escape_str("$41"), "$2441");
        // A space in the middle is a space; one at either end would be
        // trimmed back off by the reader, so it is not left as one.
        assert_eq!(hex_escape_str(" a b "), "$20a b$20");
        assert_eq!(hex_escape_str(" "), "$20");
        // Text above ASCII is left alone — the file's encoding carries it,
        // and a two-digit escape could not spell it anyway.
        assert_eq!(hex_escape_str("日本"), "日本");
    }

    #[test]
    fn the_encoder_round_trips_through_the_decoder() {
        for s in [
            "show version\r",
            "$",
            "$$$",
            " leading and trailing ",
            "\x00\x1b\x7f",
            "日本$41\r\n",
            "",
        ] {
            assert_eq!(hex_decode_str(&hex_escape_str(s)), s, "round trip of {s:?}");
        }
    }
}
