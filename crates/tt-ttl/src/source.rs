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
//! **The no-BOM branch is upstream's ANSI code page on Windows.** `LoadFileU8C`
//! tries `CP_ACP` first, with `MB_ERR_INVALID_CHARS`, and keeps the original
//! bytes when that conversion fails. On a Japanese Windows that makes a
//! Shift-JIS macro read correctly and a UTF-8 one that happens to be valid
//! CP932 read as mojibake; on a Western one the same file is CP1252, where
//! almost every byte sequence is valid. The encoding of a macro file is
//! therefore a property of the Windows machine that runs it. Windows follows
//! that rule; Unix, which has no ACP to ask, keeps a BOM-less file unchanged.

/// `LoadFileU8C` — a macro file's bytes as the parser should see them.
pub fn decode(raw: &[u8]) -> Vec<u8> {
    let mut out = match raw {
        // UTF-8 BOM: dropped, and nothing else is touched.
        [0xEF, 0xBB, 0xBF, rest @ ..] => rest.to_vec(),
        [0xFF, 0xFE, rest @ ..] => from_utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => from_utf16(rest, u16::from_be_bytes),
        _ => from_ansi(raw),
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

#[cfg(not(windows))]
fn from_ansi(raw: &[u8]) -> Vec<u8> {
    raw.to_vec()
}

/// `ToU8A` — convert the bytes Windows accepts in its active ANSI code page.
///
/// `LoadRawFile` NUL-terminates the input and `ToU8A` uses `strlen`, so bytes
/// after the first NUL do not get a vote on whether the conversion succeeds.
/// If `MultiByteToWideChar` refuses the bytes, upstream tries its permissive
/// UTF-8 decoder only as a check and then retains the original buffer. Keeping
/// the bytes here is therefore the observable fallback too.
#[cfg(windows)]
fn from_ansi(raw: &[u8]) -> Vec<u8> {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let text = &raw[..end];
    // SAFETY: GetACP takes no pointers and only reads the process locale.
    let code_page = unsafe { windows_sys::Win32::Globalization::GetACP() };
    from_code_page(text, code_page).unwrap_or_else(|| text.to_vec())
}

#[cfg(windows)]
fn from_code_page(raw: &[u8], code_page: u32) -> Option<Vec<u8>> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Globalization::{
        MultiByteToWideChar, WideCharToMultiByte, CP_UTF8, MB_ERR_INVALID_CHARS,
    };

    if raw.is_empty() {
        return Some(Vec::new());
    }
    if code_page == CP_UTF8 {
        // `_MultiByteToWideChar` deliberately uses Tera Term's own permissive
        // decoder for CP_UTF8 rather than the Win32 function below.
        return Some(from_upstream_utf8(raw));
    }
    let raw_len = i32::try_from(raw.len()).ok()?;
    // SAFETY: `raw` is alive for the call, its explicit length is checked
    // above, and a NULL output asks Win32 for the required allocation size.
    let wide_len = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            raw.as_ptr(),
            raw_len,
            null_mut(),
            0,
        )
    };
    if wide_len == 0 {
        return None;
    }
    let mut wide = vec![0u16; wide_len as usize];
    // SAFETY: `wide` has exactly the capacity Win32 just requested and both
    // slices remain alive and unaliased for the call.
    if unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            raw.as_ptr(),
            raw_len,
            wide.as_mut_ptr(),
            wide_len,
        )
    } != wide_len
    {
        return None;
    }

    // `ToU8A` next calls `_WideCharToMultiByte(..., CP_UTF8, ...)`.
    // SAFETY: `wide` is a fully initialised UTF-16 buffer; NULL output and
    // default-character pointers are required for CP_UTF8.
    let utf8_len = unsafe {
        WideCharToMultiByte(
            CP_UTF8,
            0,
            wide.as_ptr(),
            wide_len,
            null_mut(),
            0,
            null(),
            null_mut(),
        )
    };
    if utf8_len == 0 {
        return None;
    }
    let mut utf8 = vec![0u8; utf8_len as usize];
    // SAFETY: `utf8` has the size reported by the preceding call and all
    // pointers refer to live, non-overlapping buffers.
    if unsafe {
        WideCharToMultiByte(
            CP_UTF8,
            0,
            wide.as_ptr(),
            wide_len,
            utf8.as_mut_ptr(),
            utf8_len,
            null(),
            null_mut(),
        )
    } != utf8_len
    {
        return None;
    }
    Some(utf8)
}

/// `UTF8ToWideChar` followed by `WideCharToMBCP(CP_UTF8)`.
///
/// This differs from both strict UTF-8 and Win32's replacement behavior: an
/// invalid byte becomes an ASCII `?`, while adjacent UTF-8 encodings of a high
/// and low surrogate are joined by the UTF-16 intermediate representation.
#[cfg(windows)]
fn from_upstream_utf8(raw: &[u8]) -> Vec<u8> {
    from_upstream_wide(&to_upstream_wide(raw))
}

/// Tera Term's permissive `UTF8ToWideChar`, shared with Win32 APIs which take
/// the parser's UTF-8 strings through a UTF-16 boundary.
#[cfg(windows)]
pub(crate) fn to_upstream_wide(raw: &[u8]) -> Vec<u16> {
    fn continuation(byte: u8) -> bool {
        byte & 0xC0 == 0x80
    }

    let mut wide = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        let c1 = raw[at];
        let decoded = if c1 <= 0x7F {
            Some((u32::from(c1), 1))
        } else if (0xC2..=0xDF).contains(&c1) && at + 1 < raw.len() {
            let c2 = raw[at + 1];
            continuation(c2).then_some((u32::from(c1 & 0x1F) << 6 | u32::from(c2 & 0x3F), 2))
        } else if (0xE0..=0xEF).contains(&c1) && at + 2 < raw.len() {
            let c2 = raw[at + 1];
            let c3 = raw[at + 2];
            ((c1 & 0x0F != 0 || c2 & 0x20 != 0) && continuation(c2) && continuation(c3)).then_some(
                (
                    u32::from(c1 & 0x0F) << 12 | u32::from(c2 & 0x3F) << 6 | u32::from(c3 & 0x3F),
                    3,
                ),
            )
        } else if (0xF0..=0xF7).contains(&c1) && at + 3 < raw.len() {
            let c2 = raw[at + 1];
            let c3 = raw[at + 2];
            let c4 = raw[at + 3];
            ((c1 & 0x07 != 0 || c2 & 0x30 != 0)
                && continuation(c2)
                && continuation(c3)
                && continuation(c4))
            .then_some((
                u32::from(c1 & 0x07) << 18
                    | u32::from(c2 & 0x3F) << 12
                    | u32::from(c3 & 0x3F) << 6
                    | u32::from(c4 & 0x3F),
                4,
            ))
        } else {
            None
        };

        let (codepoint, used) = decoded.unwrap_or((u32::from(b'?'), 1));
        at += used;
        if codepoint < 0x1_0000 {
            wide.push(codepoint as u16);
        } else if codepoint <= 0x10_FFFF {
            let pair = codepoint - 0x1_0000;
            wide.push(0xD800 | (pair >> 10) as u16);
            wide.push(0xDC00 | (pair & 0x3FF) as u16);
        } else {
            wide.push(u16::from(b'?'));
        }
    }

    wide
}

/// Tera Term's `WideCharToMBCP(CP_UTF8)`, including its `?` for an unpaired
/// surrogate rather than Rust's replacement character.
pub(crate) fn from_upstream_wide(wide: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(wide.len());
    let mut at = 0;
    while at < wide.len() {
        let first = wide[at];
        let (codepoint, used) = if (0xD800..=0xDBFF).contains(&first)
            && wide
                .get(at + 1)
                .is_some_and(|u| (0xDC00..=0xDFFF).contains(u))
        {
            let second = wide[at + 1];
            (
                0x1_0000 + (u32::from(first - 0xD800) << 10) + u32::from(second - 0xDC00),
                2,
            )
        } else if (0xD800..=0xDFFF).contains(&first) {
            (u32::from(b'?'), 1)
        } else {
            (u32::from(first), 1)
        };
        at += used;
        let ch = char::from_u32(codepoint).unwrap_or('?');
        let mut encoded = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
    }
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
    // `ToU8W` uses Tera Term's own `WideCharToMBCP(CP_UTF8)`, not the Win32
    // function its name resembles. An unpaired surrogate is therefore `?`.
    from_upstream_wide(&units)
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
        // Two thirds of a BOM is not a BOM: it takes the platform's ordinary
        // no-BOM branch rather than losing those first two bytes.
        assert_eq!(decode(b"\xEF\xBBx"), from_ansi(b"\xEF\xBBx"));
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
    fn an_unpaired_surrogate_becomes_upstreams_question_mark() {
        assert_eq!(decode(b"\xFF\xFE\x00\xD8A\x00"), b"?A");
    }

    /// The odd byte is the low half of its unit in *both* orders, because
    /// upstream's byte-swap loop never reaches it.
    #[test]
    fn a_truncated_utf16_file_keeps_its_half_character() {
        assert_eq!(decode(b"\xFF\xFEA\x00\x42"), "A\u{42}".as_bytes());
        assert_eq!(decode(b"\xFE\xFF\x00A\x42"), "A\u{42}".as_bytes());
    }

    #[cfg(not(windows))]
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

    #[cfg(windows)]
    fn encode_without_best_fit(text: &str, code_page: u32) -> Option<Vec<u8>> {
        use std::ptr::{null, null_mut};
        use windows_sys::core::BOOL;
        use windows_sys::Win32::Globalization::{
            WideCharToMultiByte, CP_UTF8, WC_NO_BEST_FIT_CHARS,
        };

        if code_page == CP_UTF8 {
            return Some(text.as_bytes().to_vec());
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut used_default: BOOL = 0;
        // SAFETY: `wide` is live for the call; NULL output requests its size.
        let len = unsafe {
            WideCharToMultiByte(
                code_page,
                WC_NO_BEST_FIT_CHARS,
                wide.as_ptr(),
                wide.len() as i32,
                null_mut(),
                0,
                null(),
                &mut used_default,
            )
        };
        if len == 0 || used_default != 0 {
            return None;
        }
        let mut encoded = vec![0u8; len as usize];
        used_default = 0;
        // SAFETY: `encoded` has the size Win32 just returned and both slices
        // remain alive and unaliased for the call.
        let written = unsafe {
            WideCharToMultiByte(
                code_page,
                WC_NO_BEST_FIT_CHARS,
                wide.as_ptr(),
                wide.len() as i32,
                encoded.as_mut_ptr(),
                len,
                null(),
                &mut used_default,
            )
        };
        (written == len && used_default == 0).then_some(encoded)
    }

    #[cfg(windows)]
    #[test]
    fn a_file_with_no_bom_uses_the_active_windows_code_page() {
        use windows_sys::Win32::Globalization::GetACP;

        // SAFETY: GetACP takes no pointers and only reads the process locale.
        let code_page = unsafe { GetACP() };
        let (text, encoded) = [
            "café",
            "こんにちは",
            "中文",
            "한국어",
            "Привет",
            "Γειά",
            "שלום",
            "مرحبا",
            "สวัสดี",
        ]
        .into_iter()
        .find_map(|text| encode_without_best_fit(text, code_page).map(|bytes| (text, bytes)))
        .unwrap_or_else(|| panic!("no test text is representable in Windows CP{code_page}"));
        assert_eq!(decode(&encoded), text.as_bytes());
    }

    #[cfg(windows)]
    #[test]
    fn a_rejected_ansi_sequence_keeps_its_original_bytes() {
        // A lone CP932 lead byte is rejected with MB_ERR_INVALID_CHARS. This
        // tests the fallback independently of the machine's active code page.
        assert_eq!(from_code_page(b"x\x82", 932), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_utf8_active_code_page_uses_upstreams_replacement_rules() {
        use windows_sys::Win32::Globalization::CP_UTF8;

        assert_eq!(
            from_code_page(b"a\xFF\xF0\x90\x80\x80", CP_UTF8),
            Some(b"a?\xF0\x90\x80\x80".to_vec())
        );
        // Two individually invalid surrogate encodings become a valid pair
        // when upstream takes the detour through UTF-16.
        assert_eq!(
            from_code_page(b"\xED\xA0\x80\xED\xB0\x80", CP_UTF8),
            Some(b"\xF0\x90\x80\x80".to_vec())
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
