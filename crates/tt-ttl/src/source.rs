//! Turning a macro *file* into the bytes the parser reads — `fileread.cpp`'s
//! `LoadFileU8C`, which `ttmbuff.c:147` calls for the file the user named and
//! for every `include`.
//!
//! Upstream's parser never sees a file. `LoadMacroFile` hands the bytes to
//! `LoadFileU8W`, which sniffs a byte-order mark, converts the whole thing to
//! UTF-8 and NUL-terminates it; `Buff[]` is always UTF-8 by the time
//! `GetRawLine` reads a byte out of it. So this is not a decision the port gets
//! to make somewhere else: it belongs between the file and the buffer, which is
//! [`crate::buffer::Buffers`], and it applies to an included file exactly as it
//! applies to the first one.
//!
//! **The no-BOM branch is upstream's ANSI code page, and this port does not
//! have one.** `LoadFileU8C` tries `CP_ACP` *first* and only falls back to
//! UTF-8 when the conversion fails — with `MB_ERR_INVALID_CHARS` set, so it
//! fails only on bytes the code page cannot spell. On a Japanese Windows that
//! makes a Shift-JIS macro read correctly and a UTF-8 one that happens to be
//! valid CP932 read as mojibake; on a Western one the same file is CP1252,
//! where *every* byte sequence is valid, so a UTF-8 macro is mangled and there
//! is no fallback at all. The encoding of a macro file is therefore a property
//! of the machine that runs it, which is not something to reproduce on Linux
//! where there is no such setting. A file with no BOM is passed through
//! unchanged: valid UTF-8 is already what the parser wants, and anything else
//! reaches the host as the bytes the file held. See `PLAN.md` — Stage 3 puts
//! the code-page branch back on Windows, where it means something.

/// `LoadFileU8C` — a macro file's bytes as the parser should see them.
pub fn decode(raw: &[u8]) -> Vec<u8> {
    let mut out = match raw {
        // UTF-8 BOM: dropped, and nothing else is touched.
        [0xEF, 0xBB, 0xBF, rest @ ..] => rest.to_vec(),
        [0xFF, 0xFE, rest @ ..] => from_utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => from_utf16(rest, u16::from_be_bytes),
        _ => raw.to_vec(),
    };

    // Every branch of `LoadFileU8C` finishes with `*_len = strlen(buf)+1`, so
    // the macro ends at its first NUL however it was encoded. That is not an
    // edge case worth skipping: it is exactly what happens to a UTF-16 file
    // saved *without* a BOM, which falls to the no-BOM branch and stops after
    // one character. A macro that appears to be one line long is the symptom.
    let end = out.iter().position(|&b| b == 0).unwrap_or(out.len());
    out.truncate(end);
    out
}

/// The two UTF-16 branches, which differ only in how a pair is read.
///
/// Upstream converts through a NUL-terminated `wchar_t *`, and `LoadRawFile`
/// appends **three** zero bytes for it — the third one so that a file with an
/// odd length still terminates. So a trailing half-unit is a character whose
/// missing byte is zero, and for the big-endian branch that byte is the *low*
/// one: the swap loop only reaches whole pairs, and what it leaves behind is
/// then read little-endian like everything else. Handing the odd byte to
/// `from_be_bytes` as the high half would put the character somewhere else
/// entirely, which is the sort of divergence that only shows up in a file
/// somebody truncated.
fn from_utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> Vec<u8> {
    let mut units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| read([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    if units.len() == bytes.len() / 2 {
        if let [odd] = bytes.chunks_exact(2).remainder() {
            units.push(*odd as u16);
        }
    }
    // `WideCharToMultiByte(CP_UTF8, 0, ...)` — with no `WC_ERR_INVALID_CHARS`,
    // an unpaired surrogate becomes U+FFFD rather than failing the file.
    String::from_utf16_lossy(&units).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_utf8_bom_is_dropped_and_the_rest_kept() {
        assert_eq!(
            decode(b"\xEF\xBB\xBFmessagebox 'a' 'b'"),
            b"messagebox 'a' 'b'"
        );
        // Two thirds of a BOM is not a BOM.
        assert_eq!(decode(b"\xEF\xBBx"), b"\xEF\xBBx");
    }

    #[test]
    fn both_utf16_byte_orders_become_utf8() {
        let le: Vec<u8> = b"\xFF\xFE"
            .iter()
            .copied()
            .chain("ab\u{3042}".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let be: Vec<u8> = b"\xFE\xFF"
            .iter()
            .copied()
            .chain("ab\u{3042}".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode(&le), "ab\u{3042}".as_bytes());
        assert_eq!(decode(&be), "ab\u{3042}".as_bytes());
    }

    #[test]
    fn a_bom_on_its_own_is_an_empty_macro() {
        assert_eq!(decode(b"\xFF\xFE"), b"");
        assert_eq!(decode(b"\xFE\xFF"), b"");
        assert_eq!(decode(b"\xEF\xBB\xBF"), b"");
    }

    #[test]
    fn an_unpaired_surrogate_is_replaced_rather_than_refused() {
        assert_eq!(decode(b"\xFF\xFE\x00\xD8A\x00"), "\u{FFFD}A".as_bytes());
    }

    /// The odd byte is the low half of its unit in *both* orders, because
    /// upstream's byte-swap loop never reaches it.
    #[test]
    fn a_truncated_utf16_file_keeps_its_half_character() {
        assert_eq!(decode(b"\xFF\xFEA\x00\x42"), "A\u{42}".as_bytes());
        assert_eq!(decode(b"\xFE\xFF\x00A\x42"), "A\u{42}".as_bytes());
    }

    #[test]
    fn a_file_with_no_bom_is_its_own_bytes() {
        // Shift-JIS, which upstream would have read through the code page.
        assert_eq!(
            decode(b"messagebox '\x82\xB1\x82\xF1'"),
            b"messagebox '\x82\xB1\x82\xF1'"
        );
        assert_eq!(
            decode("messagebox 'こん'".as_bytes()),
            "messagebox 'こん'".as_bytes()
        );
    }

    /// The macro stops at its first NUL whatever the encoding — which is what
    /// makes a BOM-less UTF-16 file a one-character macro.
    #[test]
    fn the_macro_ends_at_its_first_nul() {
        assert_eq!(decode(b"a\x00b"), b"a");
        let le: Vec<u8> = "ab".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&le), b"a");
    }
}
