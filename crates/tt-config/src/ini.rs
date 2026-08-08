//! `TERATERM.INI`, read and written the way `GetPrivateProfile*` does it.
//!
//! Not an INI parser. *That* INI parser — the Win32 one, measured under Wine
//! and recorded in `ini-audit/win32.txt`, quirks and all. Every rule below has
//! a case in `ini-audit/cases.txt` and `tests/win32.rs` replays the whole
//! battery against this file, so the citations are checkable rather than
//! decorative.
//!
//! The reason for the fidelity is that this code's first act on a new user's
//! machine is to read the `TERATERM.INI` they already have and write it back.
//! A generic crate gets at least four of these rules wrong — duplicate keys,
//! quote stripping, empty values and comments — and each one is a setting the
//! user never changed, changing.
//!
//! The file is kept as **lines**, not as a map, because that is what a faithful
//! write needs: comments survive, ordering survives, an existing key keeps its
//! own spelling, and everything the caller did not ask about is left alone.

/// How the file was encoded, so that writing it back does not convert it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-16 little-endian with a BOM, which is what Tera Term 5 writes.
    Utf16Le,
    /// UTF-8 with a BOM.
    Utf8Bom,
    /// No BOM, and valid UTF-8.
    Utf8,
    /// No BOM and not valid UTF-8, so it is held as Latin-1 — which is not a
    /// guess about what the bytes *mean*, it is the one decoding that maps
    /// every byte to a distinct character and back again. See [`Ini::parse`].
    Latin1,
}

/// What a line turns out to be.
///
/// Deliberately not an `Entry`/`Comment` split: to a *lookup* there are no
/// comments at all. `;A=1` is an entry whose key is `;A`, and asking for it by
/// that name returns `1` — only enumeration skips it. Asking for `A` misses
/// because the names differ, which is why the distinction stays invisible until
/// something lists the keys.
enum Line<'a> {
    /// `[name]`. Text after the `]` is ignored, and the name is trimmed.
    Section(&'a str),
    /// A line with an `=` in it. Key and value are trimmed; the value has not
    /// been unquoted yet.
    Entry { key: &'a str, value: &'a str },
    /// A blank line, or one with no `=` — which is not an entry at all,
    /// neither found by a lookup nor listed by one.
    Other,
}

fn classify(line: &str) -> Line<'_> {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix('[') {
        return match rest.find(']') {
            // An unterminated `[section` is not a header, and having no `=`
            // it is not an entry either — so it disappears, and the keys under
            // it belong to whatever section was open before.
            None => Line::Other,
            Some(end) => Line::Section(rest[..end].trim()),
        };
    }
    match t.find('=') {
        None => Line::Other,
        Some(eq) => Line::Entry {
            key: t[..eq].trim(),
            value: t[eq + 1..].trim(),
        },
    }
}

/// Discard one matched pair of surrounding quotes.
///
/// MSDN documents this and `PLAN.md` assumed the opposite. Single and double
/// both count, but only as a *pair*: `"value` and `value"` and `"value'` all
/// keep their quote, and `""value""` loses exactly one pair. Stripping
/// unconditionally mangles a value that legitimately starts with a quote;
/// stripping nothing puts literal quotes into every quoted setting.
fn unquote(value: &str) -> &str {
    let b = value.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Win32 matches names case-insensitively. ASCII-only here: settings keys are
/// ASCII, and the locale-aware folding Win32 does for anything else is not
/// reproducible off Windows anyway.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// One INI file.
pub struct Ini {
    lines: Vec<String>,
    encoding: Encoding,
    /// What to end a line the *writer* adds with. Taken from the file rather
    /// than fixed, which is a deliberate divergence — see [`Ini::to_bytes`].
    eol: String,
}

impl Ini {
    /// Parse bytes, honouring the BOM.
    ///
    /// A UTF-16 BOM is decoded as UTF-16 and a UTF-8 BOM as UTF-8. Without a
    /// BOM, valid UTF-8 is taken as UTF-8 — where Win32 would use the
    /// machine's ANSI codepage, so such a file reads correctly here and comes
    /// back as mojibake on Windows. On Linux there is no ANSI codepage to
    /// consult and UTF-8 is what a file has actually been edited as.
    ///
    /// **Anything else is held as Latin-1**, which is not a claim about what
    /// the bytes mean. A Japanese Tera Term 4 wrote Shift-JIS and Latin-1 will
    /// render it as nonsense — but it is the only decoding under which every
    /// byte survives and comes back unchanged, so the file can be written back
    /// without the settings nobody touched being quietly rewritten. Decoding
    /// lossily instead would turn each of those bytes into U+FFFD and destroy
    /// them on the first save.
    pub fn parse(bytes: &[u8]) -> Ini {
        let (text, encoding) = decode(bytes);
        let eol = if text.contains("\r\n") {
            "\r\n"
        } else if text.contains('\n') {
            "\n"
        } else if text.contains('\r') {
            "\r"
        } else {
            "\r\n"
        };
        Ini {
            lines: split_lines(&text),
            encoding,
            eol: eol.to_string(),
        }
    }

    /// Read a file. A missing one parses as empty, because "the user has no
    /// `TERATERM.INI` yet" and "the user's `TERATERM.INI` is empty" call for
    /// exactly the same behaviour and only one of them is worth an error.
    pub fn load(path: &std::path::Path) -> std::io::Result<Ini> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Ini::parse(&bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ini::new()),
            Err(e) => Err(e),
        }
    }

    /// Write the file, replacing it via a temporary in the same directory.
    ///
    /// Settings are written on exit, which is exactly when a machine loses
    /// power or a session is killed — and a half-written `TERATERM.INI` is a
    /// terminal that comes up with someone else's defaults and no explanation.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let tmp = path.with_extension("ini.tmp");
        std::fs::write(&tmp, self.to_bytes())?;
        std::fs::rename(&tmp, path)
    }

    /// An empty file that will be written as UTF-8 with a BOM.
    ///
    /// The BOM is what makes a Tera Term on Windows read it as UTF-8 rather
    /// than in its ANSI codepage, so it is not decoration.
    pub fn new() -> Ini {
        Ini {
            lines: Vec::new(),
            encoding: Encoding::Utf8Bom,
            eol: "\r\n".to_string(),
        }
    }

    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// The file's bytes, ready to write.
    ///
    /// The encoding and the BOM are whatever they were. **Line endings that
    /// were already there are kept**, which Win32 does not do — measured under
    /// Wine, writing one key to an LF-only file rewrites every line as CRLF.
    /// Preserving them costs nothing (a Tera Term on Windows parses LF-only
    /// files fine, which is also measured) and rewriting a Linux user's whole
    /// file because one setting changed is the sort of thing that shows up in
    /// a diff and looks like corruption.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut text = String::new();
        for line in &self.lines {
            text.push_str(line);
            text.push_str(&self.eol);
        }
        match self.encoding {
            Encoding::Utf16Le => {
                let mut out = vec![0xFF, 0xFE];
                for unit in text.encode_utf16() {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
                out
            }
            Encoding::Utf8Bom => {
                let mut out = vec![0xEF, 0xBB, 0xBF];
                out.extend_from_slice(text.as_bytes());
                out
            }
            Encoding::Utf8 => text.into_bytes(),
            Encoding::Latin1 => text.chars().map(|c| c as u8).collect(),
        }
    }

    // --- reading ------------------------------------------------------------

    /// The value of `key` in `section`, or `None`.
    ///
    /// **The first match wins**, for both the key and the section: a second
    /// `[Tera Term]` block is not merged with the first, and a key that appears
    /// only in it is invisible. Getting this backwards reads a different
    /// setting than Tera Term does from the same file.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let range = self.section_body(section)?;
        for line in &self.lines[range] {
            if let Line::Entry { key: k, value } = classify(line) {
                if same(k, key.trim()) {
                    // Borrowed out of the line, so a caller can compare
                    // without allocating — which the schema does per key.
                    let start = value.as_ptr() as usize - line.as_ptr() as usize;
                    let unquoted = unquote(&line[start..start + value.len()]);
                    return Some(unquoted);
                }
            }
        }
        None
    }

    /// `GetPrivateProfileString`: the value, or `default` when there is none.
    ///
    /// Kept separate from [`get`](Ini::get) because the distinction is
    /// load-bearing — `Key=` yields an **empty string, not the default**, and
    /// upstream leans on it. `ttset.c:877` reads `BSKey` with an empty fallback
    /// and only the literal `DEL` takes the other arm, so an empty value means
    /// backspace sends BS. A reader that collapsed empty into absent would
    /// change that setting for everyone who has the key but no value.
    pub fn get_or<'a>(&'a self, section: &str, key: &str, default: &'a str) -> &'a str {
        self.get(section, key).unwrap_or(default)
    }

    /// `GetPrivateProfileInt`, which is a different parser from the string one
    /// and agrees with it about almost nothing.
    ///
    /// Leading whitespace and a sign are accepted, `0x` is hex, trailing junk
    /// is ignored, and **a value that does not parse at all is zero rather than
    /// the default** — only a missing key gets the default. The result is
    /// unsigned, so `-5` comes back as 4294967291 and the caller is expected
    /// to know that.
    pub fn get_int(&self, section: &str, key: &str, default: i32) -> u32 {
        match self.get(section, key) {
            // An empty value is the one case that falls back, unlike the
            // string call.
            None => default as u32,
            Some(v) if v.trim().is_empty() => default as u32,
            Some(v) => parse_int(v.trim()),
        }
    }

    /// The keys of `section`, in file order, duplicates included.
    ///
    /// This is where a comment finally becomes one: a key beginning with `;` is
    /// skipped here and nowhere else.
    pub fn keys(&self, section: &str) -> Vec<String> {
        let Some(range) = self.section_body(section) else {
            return Vec::new();
        };
        self.lines[range]
            .iter()
            .filter_map(|line| match classify(line) {
                Line::Entry { key, .. } if !key.starts_with(';') => Some(key.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Every section name, in file order, duplicates included.
    ///
    /// The unnamed section that holds keys written before any header is *not*
    /// listed, though its keys are reachable through `""`.
    pub fn sections(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|line| match classify(line) {
                Line::Section(name) if !name.is_empty() => Some(name.to_string()),
                _ => None,
            })
            .collect()
    }

    /// The line range holding `section`'s entries, header excluded.
    ///
    /// An empty `section` means the keys before the first header. A literal
    /// `[]` in the file starts a section that can never be named, so its keys
    /// are unreachable — measured, and reproduced rather than tidied up,
    /// because a file containing one is a file whose author already lost those
    /// settings on Windows.
    fn section_body(&self, section: &str) -> Option<std::ops::Range<usize>> {
        let want = section.trim();
        let mut start: Option<usize> = None;
        for (i, line) in self.lines.iter().enumerate() {
            let Line::Section(name) = classify(line) else {
                continue;
            };
            if let Some(s) = start {
                return Some(s..i);
            }
            if want.is_empty() {
                // The unnamed run is whatever comes before the first header,
                // whatever that header is called — including `[]`.
                return Some(0..i);
            }
            if same(name, want) {
                start = Some(i + 1);
            }
        }
        match start {
            Some(s) => Some(s..self.lines.len()),
            None if want.is_empty() => Some(0..self.lines.len()),
            None => None,
        }
    }

    // --- writing ------------------------------------------------------------

    /// Set `key` in `section`, creating either if needed.
    ///
    /// An existing key is updated **in place and keeps its own spelling** — a
    /// file holding `KeyName` written through as `keyname` still says
    /// `KeyName`. A new key goes at the end of its section and a new section at
    /// the end of the file. Only the *first* of a duplicated key is touched,
    /// which leaves the second unreachable exactly as it already was.
    ///
    /// Returns false, changing nothing, if the value contains a line ending.
    /// Win32 writes it raw and splits the file in two; upstream escapes such
    /// values itself (`Str2HexW`, for `DelimList`), so refusing is both safer
    /// and closer to how the settings are actually stored.
    pub fn set(&mut self, section: &str, key: &str, value: &str) -> bool {
        if value.contains('\n') || value.contains('\r') {
            return false;
        }
        let key = key.trim();
        // Latin-1 is a container for bytes that were already there, not one
        // to put new text into: anything above U+00FF has no representation
        // and would be silently dropped by the cast in `to_bytes`. A file with
        // no BOM has the opposite problem — it round-trips fine here and reads
        // as mojibake on Windows. Both are answered by upgrading to UTF-8 with
        // a BOM, which is the one encoding both sides agree about, and only
        // when the alternative is losing what was asked for.
        if (!key.is_ascii() || !value.is_ascii())
            && matches!(self.encoding, Encoding::Utf8 | Encoding::Latin1)
        {
            self.encoding = Encoding::Utf8Bom;
        }
        if let Some(range) = self.section_body(section) {
            let mut end = range.start;
            for i in range.clone() {
                if let Line::Entry { key: k, .. } = classify(&self.lines[i]) {
                    if same(k, key) {
                        let spelling = k.to_string();
                        self.lines[i] = format!("{spelling}={value}");
                        return true;
                    }
                }
                // Trailing blank lines belong after the section, not inside
                // it, or every write pushes the key further down the file.
                if !self.lines[i].trim().is_empty() {
                    end = i + 1;
                }
            }
            self.lines.insert(end, format!("{key}={value}"));
            return true;
        }
        self.lines.push(format!("[{}]", section.trim()));
        self.lines.push(format!("{key}={value}"));
        true
    }

    /// Delete `key` from `section`. A no-op when it is not there.
    pub fn remove(&mut self, section: &str, key: &str) {
        let Some(range) = self.section_body(section) else {
            return;
        };
        let key = key.trim();
        for i in range {
            if let Line::Entry { key: k, .. } = classify(&self.lines[i]) {
                if same(k, key) {
                    self.lines.remove(i);
                    return;
                }
            }
        }
    }

    /// Delete a whole section, header and all.
    pub fn remove_section(&mut self, section: &str) {
        let Some(range) = self.section_body(section) else {
            return;
        };
        // The header is the line above the body, unless the body is the
        // unnamed run at the top of the file.
        let from = if range.start > 0 { range.start - 1 } else { 0 };
        self.lines.drain(from..range.end);
    }
}

impl Default for Ini {
    fn default() -> Self {
        Ini::new()
    }
}

// --- the pieces that are their own small parsers ----------------------------

fn decode(bytes: &[u8]) -> (String, Encoding) {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return (String::from_utf16_lossy(&units), Encoding::Utf16Le);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (
            String::from_utf8_lossy(&bytes[3..]).into_owned(),
            Encoding::Utf8Bom,
        );
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), Encoding::Utf8),
        // Latin-1 is a byte-for-byte round trip, which is the entire reason
        // for choosing it over a lossy decode or a guess.
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), Encoding::Latin1),
    }
}

/// Split on CR LF, LF **or** a bare CR. All three parse on Windows, and a
/// settings file that has been through a Linux editor or a Mac from 2001 is
/// not a hypothetical.
fn split_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut current));
            }
            '\n' => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `GetPrivateProfileInt`'s number parser, for a value already in hand.
/// The schema's `int` needs it for a value typed into a dialog rather than
/// read from a file, and there must be exactly one of these.
pub(crate) fn parse_int_public(s: &str) -> u32 {
    parse_int(s)
}

/// `GetPrivateProfileInt`'s number parser, wrapping like the original.
fn parse_int(s: &str) -> u32 {
    let s = s.trim_start();
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(rest) => (16, rest),
        None => (10, s),
    };
    let mut value: u32 = 0;
    for c in digits.chars() {
        match c.to_digit(radix) {
            // Trailing junk ends the number rather than failing it, and
            // leading junk therefore yields zero rather than the default.
            None => break,
            Some(d) => value = value.wrapping_mul(radix).wrapping_add(d),
        }
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}
