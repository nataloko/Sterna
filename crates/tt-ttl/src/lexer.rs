//! The line tokeniser, ported from `ttmparse.cpp`.
//!
//! TTL is parsed a line at a time and re-parsed on every execution of that
//! line — there is no AST, and a `while` loop lexes its condition again each
//! time round. That is upstream's design and it is kept, because the language
//! leans on it: `LinePtr` is rewound by half a dozen callers to try a second
//! reading of the same text, and a command decides what a token means only
//! after it has seen it.
//!
//! Everything here works in **bytes**, not `char`s. A `#` escape can produce
//! any byte from 1 to 255, so a TTL string is not necessarily UTF-8 — and it
//! must not be, since `send` puts it on the wire unchanged.

use crate::error::{TtlError, TtlResult};
use crate::rsv::{Rsv, RESERVED};

/// `ttmdef.h:33` — an identifier keeps its first 31 bytes and drops the rest.
pub const MAX_NAME_LEN: usize = 32;
/// `ttmdef.h:34` — a string value is capped at 511 bytes, silently.
pub const MAX_STR_LEN: usize = 512;
/// `ttmdef.h:35` — and a source line at 1023.
pub const MAX_LINE_LEN: usize = 1024;

/// The current line, and where in it the parser has got to.
///
/// Upstream keeps this in four globals (`LineBuff`, `LinePtr`, `LineLen`,
/// `LineParsePtr`) plus the `commenting` static; the C-style comment state has
/// to outlive the line because `/*` may not be closed until several lines on.
#[derive(Debug, Default, Clone)]
pub struct Lexer {
    buf: Vec<u8>,
    /// How far the parser has read. Callers rewind it freely.
    pub ptr: usize,
    /// Where the current token started, for the error report's highlight.
    pub parse_ptr: usize,
    commenting: bool,
}

impl Lexer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a raw line, truncated the way `GetRawLine` truncates it.
    pub fn set_line(&mut self, line: &[u8]) {
        self.buf.clear();
        self.buf
            .extend_from_slice(&line[..line.len().min(MAX_LINE_LEN - 1)]);
        self.ptr = 0;
        self.parse_ptr = 0;
    }

    pub fn line(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// The byte at `i`, or 0 past the end.
    ///
    /// Upstream reads `LineBuff[LinePtr + 1]` without a bounds test in the
    /// comment scanner and gets away with it because `LineBuff` is a 1024-byte
    /// array that `GetRawLine` memsets. Returning 0 is that behaviour.
    fn at(&self, i: usize) -> u8 {
        self.buf.get(i).copied().unwrap_or(0)
    }

    /// Step back one byte. Upstream writes `LinePtr--` on a `WORD`, which on an
    /// empty line wraps to 65535 and is then harmlessly past the end; saturating
    /// lands on the same outcome without the wrap.
    pub fn back(&mut self) {
        self.ptr = self.ptr.saturating_sub(1);
    }

    /// `UpdateLineParsePtr` — remember where this token began.
    pub fn mark(&mut self) {
        self.parse_ptr = self.ptr;
    }

    /// Whether a `/*` is still open at the end of the line, and clear the flag.
    ///
    /// `IsCommentClosed` deliberately resets as it reports, so the commands
    /// before the comment on that line still run.
    pub fn comment_closed(&mut self) -> bool {
        let closed = !self.commenting;
        self.commenting = false;
        closed
    }

    /// `GetFirstChar` — skip whitespace and comments, consume and return the
    /// next significant byte, or 0 at end of line.
    ///
    /// A `;` starts a comment to end of line and reads as 0 **without** being
    /// consumed, which is why so many callers follow up with [`Lexer::back`].
    pub fn first_char(&mut self) -> u8 {
        let mut b = if self.ptr < self.len() {
            self.at(self.ptr)
        } else {
            return 0;
        };

        self.skip_blanks(&mut b);

        if self.commenting {
            while self.ptr < self.len() {
                if self.at(self.ptr) == b'*' && self.at(self.ptr + 1) == b'/' {
                    self.commenting = false;
                    self.ptr += 2;
                    break;
                }
                self.ptr += 1;
            }
            // No close on this line either: the rest of it is comment.
            if self.commenting {
                return 0;
            }
            b = if self.ptr < self.len() {
                self.at(self.ptr)
            } else {
                0
            };
            self.skip_blanks(&mut b);
        }

        // A line may hold several comments, so this loops while `/` follows one.
        loop {
            if self.at(self.ptr) == b'/' && self.at(self.ptr + 1) == b'*' {
                let mut unterminated = true;
                self.ptr += 2;
                while self.ptr < self.len() {
                    if self.at(self.ptr) == b'*' && self.at(self.ptr + 1) == b'/' {
                        self.ptr += 2;
                        unterminated = false;
                        break;
                    }
                    self.ptr += 1;
                }

                b = if self.ptr < self.len() {
                    self.at(self.ptr)
                } else {
                    0
                };
                self.skip_blanks(&mut b);

                if unterminated {
                    self.commenting = true;
                }
            } else {
                break;
            }
            if b != b'/' {
                break;
            }
        }

        if b > b' ' && b != b';' {
            self.ptr += 1;
            return b;
        }
        0
    }

    fn skip_blanks(&mut self, b: &mut u8) {
        while self.ptr < self.len() && (*b == b' ' || *b == b'\t') {
            self.ptr += 1;
            if self.ptr < self.len() {
                *b = self.at(self.ptr);
            }
        }
    }

    /// `CheckParameterGiven` — is there anything left on the line?
    pub fn parameter_given(&mut self) -> bool {
        let p = self.ptr;
        let given = self.first_char() != 0;
        self.ptr = p;
        given
    }

    /// `GetIdentifier` — `[A-Za-z_][0-9A-Za-z_]*`, kept to 31 bytes.
    ///
    /// The tail past 31 bytes is consumed and dropped, not rejected, so two
    /// names that agree for 31 bytes are one variable.
    pub fn identifier(&mut self) -> Option<Vec<u8>> {
        let b = self.first_char();
        if b == 0 {
            return None;
        }
        if !(b.is_ascii_alphabetic() || b == b'_') {
            self.back();
            return None;
        }

        let mut name = vec![b];
        let mut b = self.at(self.ptr);
        while self.ptr < self.len() && (b.is_ascii_alphanumeric() || b == b'_') {
            if name.len() < MAX_NAME_LEN - 1 {
                name.push(b);
            }
            self.ptr += 1;
            if self.ptr < self.len() {
                b = self.at(self.ptr);
            }
        }
        Some(name)
    }

    /// `GetLabelName` — like an identifier, but the first byte may be anything
    /// `first_char` will return. `goto 1st` is a legal jump to `:1st`.
    pub fn label_name(&mut self) -> Option<Vec<u8>> {
        let b = self.first_char();
        if b == 0 {
            return None;
        }
        let mut name = vec![b];
        let mut b = self.at(self.ptr);
        while self.ptr < self.len() && (b.is_ascii_alphanumeric() || b == b'_') {
            if name.len() < MAX_NAME_LEN - 1 {
                name.push(b);
            }
            self.ptr += 1;
            if self.ptr < self.len() {
                b = self.at(self.ptr);
            }
        }
        Some(name)
    }

    /// `GetReservedWord` — an identifier, but only if the table knows it.
    pub fn reserved_word(&mut self) -> Option<Rsv> {
        let p = self.ptr;
        let name = self.identifier()?;
        match check_reserved(&name) {
            Some(w) => Some(w),
            None => {
                self.ptr = p;
                None
            }
        }
    }

    /// `GetOperator` — punctuation first, then the four operators that are
    /// spelled as words. Anything else rewinds and fails.
    pub fn operator(&mut self) -> Option<Rsv> {
        let p = self.ptr;
        let b = self.first_char();
        let w = match b {
            0 => return None,
            b'*' => Rsv::Mul,
            b'+' => Rsv::Plus,
            b'-' => Rsv::Minus,
            b'/' => Rsv::Div,
            b'%' => Rsv::Mod,
            b'=' => {
                if self.ptr < self.len() && self.at(self.ptr) == b'=' {
                    self.ptr += 1;
                }
                Rsv::Eq
            }
            b'<' => {
                let mut w = Rsv::Lt;
                if self.ptr < self.len() {
                    let c = self.at(self.ptr);
                    self.ptr += 1;
                    match c {
                        b'=' => w = Rsv::Le,
                        b'>' => w = Rsv::Ne,
                        b'<' => w = Rsv::ALShift,
                        _ => self.ptr -= 1,
                    }
                }
                w
            }
            b'>' => {
                let mut w = Rsv::Gt;
                if self.ptr < self.len() {
                    let c = self.at(self.ptr);
                    self.ptr += 1;
                    match c {
                        b'=' => w = Rsv::Ge,
                        b'>' => {
                            w = Rsv::ARShift;
                            if self.ptr < self.len() && self.at(self.ptr) == b'>' {
                                w = Rsv::LRShift;
                                self.ptr += 1;
                            }
                        }
                        _ => self.ptr -= 1,
                    }
                }
                w
            }
            b'&' => {
                if self.ptr < self.len() && self.at(self.ptr) == b'&' {
                    self.ptr += 1;
                    Rsv::LAnd
                } else {
                    Rsv::BAnd
                }
            }
            b'|' => {
                if self.ptr < self.len() && self.at(self.ptr) == b'|' {
                    self.ptr += 1;
                    Rsv::LOr
                } else {
                    Rsv::BOr
                }
            }
            b'^' => Rsv::BXor,
            b'~' => Rsv::BNot,
            b'!' => {
                if self.ptr < self.len() && self.at(self.ptr) == b'=' {
                    self.ptr += 1;
                    Rsv::Ne
                } else {
                    Rsv::LNot
                }
            }
            _ => {
                self.back();
                match self.reserved_word() {
                    Some(w) if w.is_operator() => w,
                    _ => {
                        self.ptr = p;
                        return None;
                    }
                }
            }
        };
        Some(w)
    }

    /// `GetNumber` — a decimal or `$`-prefixed hexadecimal literal.
    ///
    /// A bare `$` is the number zero, not an error, because the hex loop tests
    /// nothing before it runs. Overflow wraps, as it does in C.
    pub fn number(&mut self) -> Option<i32> {
        let b = self.first_char();
        if b == 0 {
            return None;
        }
        let mut num: i32 = 0;
        if b.is_ascii_digit() {
            num = (b - b'0') as i32;
            let mut b = self.at(self.ptr);
            while self.ptr < self.len() && b.is_ascii_digit() {
                num = num.wrapping_mul(10).wrapping_add((b - b'0') as i32);
                self.ptr += 1;
                if self.ptr < self.len() {
                    b = self.at(self.ptr);
                }
            }
        } else if b == b'$' {
            let mut b = self.at(self.ptr);
            while self.ptr < self.len() && b.is_ascii_hexdigit() {
                let d = if b.is_ascii_alphabetic() {
                    (b | 0x20) - b'a' + 10
                } else {
                    b - b'0'
                };
                num = num.wrapping_mul(16).wrapping_add(d as i32);
                self.ptr += 1;
                if self.ptr < self.len() {
                    b = self.at(self.ptr);
                }
            }
        } else {
            self.back();
            return None;
        }
        Some(num)
    }

    /// `GetString` — a run of `"..."`, `'...'` and `#nn` pieces, joined with
    /// nothing between them.
    ///
    /// The pieces have to abut: the next piece is the byte at `LinePtr`, read
    /// without skipping whitespace, so `"a"#13"b"` is one string and `"a" "b"`
    /// is a string followed by a syntax error.
    ///
    /// `Ok(None)` means "this is not a string, nothing was consumed".
    pub fn string(&mut self) -> TtlResult<Option<Vec<u8>>> {
        let q = self.first_char();
        if q == 0 {
            return Ok(None);
        }
        self.back();
        if q != b'"' && q != b'\'' && q != b'#' {
            return Ok(None);
        }

        let mut out = Vec::new();
        let mut q = q;
        while q == b'"' || q == b'\'' || q == b'#' {
            self.ptr += 1;
            match q {
                b'"' | b'\'' => self.quoted_str(&mut out, q)?,
                _ => self.char_by_code(&mut out)?,
            }
            q = self.at(self.ptr);
        }
        Ok(Some(out))
    }

    fn quoted_str(&mut self, out: &mut Vec<u8>, q: u8) -> TtlResult<()> {
        let mut b = if self.ptr < self.len() {
            self.at(self.ptr)
        } else {
            0
        };
        while self.ptr < self.len() && (b >= b' ' || b == b'\t') && b != q {
            if out.len() < MAX_STR_LEN - 1 {
                out.push(b);
            }
            self.ptr += 1;
            if self.ptr < self.len() {
                b = self.at(self.ptr);
            }
        }
        if b == q {
            if self.ptr < self.len() {
                self.ptr += 1;
            }
            Ok(())
        } else {
            Err(TtlError::Syntax)
        }
    }

    /// `GetCharByCode` — `#65`, `#$41`. Zero and anything past 255 are errors,
    /// so there is no way to write a NUL into a TTL string.
    fn char_by_code(&mut self, out: &mut Vec<u8>) -> TtlResult<()> {
        let mut b = if self.ptr < self.len() {
            self.at(self.ptr)
        } else {
            0
        };
        if !b.is_ascii_digit() && b != b'$' {
            return Err(TtlError::Syntax);
        }

        let mut n: u16 = 0;
        if b != b'$' {
            while self.ptr < self.len() && b.is_ascii_digit() {
                n = n.wrapping_mul(10).wrapping_add((b - b'0') as u16);
                self.ptr += 1;
                if self.ptr < self.len() {
                    b = self.at(self.ptr);
                }
            }
        } else {
            self.ptr += 1;
            if self.ptr < self.len() {
                b = self.at(self.ptr);
            }
            while self.ptr < self.len() && b.is_ascii_hexdigit() {
                let d = if b.is_ascii_alphabetic() {
                    (b | 0x20) - b'a' + 10
                } else {
                    b - b'0'
                };
                n = n.wrapping_mul(16).wrapping_add(d as u16);
                self.ptr += 1;
                if self.ptr < self.len() {
                    b = self.at(self.ptr);
                }
            }
        }

        if n == 0 || n > 255 {
            return Err(TtlError::Syntax);
        }
        if out.len() < MAX_STR_LEN - 1 {
            out.push(n as u8);
        }
        Ok(())
    }
}

/// `CheckReservedWord` — case-insensitive, ASCII, and total on the table.
pub fn check_reserved(name: &[u8]) -> Option<Rsv> {
    let name = std::str::from_utf8(name).ok()?;
    RESERVED
        .binary_search_by(|(n, _)| {
            let n = n.as_bytes();
            let m = name.as_bytes();
            let lim = n.len().min(m.len());
            for i in 0..lim {
                let c = n[i].cmp(&m[i].to_ascii_lowercase());
                if c != std::cmp::Ordering::Equal {
                    return c;
                }
            }
            n.len().cmp(&m.len())
        })
        .ok()
        .map(|i| RESERVED[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str) -> Lexer {
        let mut l = Lexer::new();
        l.set_line(s.as_bytes());
        l
    }

    #[test]
    fn the_table_is_sorted_so_the_search_is_valid() {
        for w in RESERVED.windows(2) {
            assert!(w[0].0 < w[1].0, "{} then {}", w[0].0, w[1].0);
        }
    }

    #[test]
    fn reserved_words_are_case_insensitive() {
        assert_eq!(check_reserved(b"SendLn"), Some(Rsv::SendLn));
        assert_eq!(check_reserved(b"sendln"), Some(Rsv::SendLn));
        assert_eq!(check_reserved(b"SENDLN"), Some(Rsv::SendLn));
        assert_eq!(check_reserved(b"sendl"), None);
        assert_eq!(check_reserved(b"sendlnx"), None);
    }

    #[test]
    fn setspeed_and_setbaud_are_one_command() {
        assert_eq!(check_reserved(b"setspeed"), check_reserved(b"setbaud"));
    }

    #[test]
    fn a_semicolon_ends_the_line() {
        let mut l = lex("a ; b");
        assert_eq!(l.identifier().as_deref(), Some(&b"a"[..]));
        assert_eq!(l.first_char(), 0);
    }

    #[test]
    fn c_comments_are_skipped_inline_and_across_lines() {
        let mut l = lex("a /* one */ /* two */ b");
        assert_eq!(l.identifier().as_deref(), Some(&b"a"[..]));
        assert_eq!(l.identifier().as_deref(), Some(&b"b"[..]));
        assert!(l.comment_closed());

        let mut l = lex("a /* open");
        assert_eq!(l.identifier().as_deref(), Some(&b"a"[..]));
        assert_eq!(l.first_char(), 0);
        // The flag has to survive the line; `comment_closed` clears as it reports.
        l.set_line(b"still comment */ b");
        assert_eq!(l.identifier().as_deref(), Some(&b"b"[..]));
    }

    #[test]
    fn an_identifier_keeps_31_bytes_and_swallows_the_rest() {
        let long = "a".repeat(40);
        let mut l = lex(&long);
        let name = l.identifier().unwrap();
        assert_eq!(name.len(), MAX_NAME_LEN - 1);
        assert_eq!(l.first_char(), 0, "the tail is consumed, not left behind");
    }

    #[test]
    fn numbers_take_decimal_and_dollar_hex() {
        assert_eq!(lex("42").number(), Some(42));
        assert_eq!(lex("$ff").number(), Some(255));
        assert_eq!(lex("$FF").number(), Some(255));
        // A bare `$` is zero: the hex loop tests nothing before it runs.
        assert_eq!(lex("$").number(), Some(0));
        assert_eq!(lex("x").number(), None);
    }

    #[test]
    fn strings_concatenate_only_when_the_pieces_abut() {
        assert_eq!(
            lex(r#""ab""#).string().unwrap().as_deref(),
            Some(&b"ab"[..])
        );
        assert_eq!(
            lex("\"a\"#13#10\"b\"").string().unwrap().as_deref(),
            Some(&b"a\r\nb"[..])
        );
        let mut l = lex(r#""a" "b""#);
        assert_eq!(l.string().unwrap().as_deref(), Some(&b"a"[..]));
        assert_eq!(l.string().unwrap().as_deref(), Some(&b"b"[..]));
    }

    #[test]
    fn a_char_code_cannot_be_nul_or_past_255() {
        assert_eq!(lex("#0").string(), Err(TtlError::Syntax));
        assert_eq!(lex("#256").string(), Err(TtlError::Syntax));
        assert_eq!(lex("#255").string().unwrap().as_deref(), Some(&b"\xff"[..]));
    }

    #[test]
    fn an_unterminated_quote_is_a_syntax_error() {
        assert_eq!(lex(r#""abc"#).string(), Err(TtlError::Syntax));
    }

    #[test]
    fn operators_prefer_the_longest_match() {
        assert_eq!(lex("<=").operator(), Some(Rsv::Le));
        assert_eq!(lex("<>").operator(), Some(Rsv::Ne));
        assert_eq!(lex("<<").operator(), Some(Rsv::ALShift));
        assert_eq!(lex("<").operator(), Some(Rsv::Lt));
        assert_eq!(lex(">>>").operator(), Some(Rsv::LRShift));
        assert_eq!(lex(">>").operator(), Some(Rsv::ARShift));
        assert_eq!(lex("!=").operator(), Some(Rsv::Ne));
        assert_eq!(lex("!").operator(), Some(Rsv::LNot));
        assert_eq!(lex("&&").operator(), Some(Rsv::LAnd));
        assert_eq!(lex("and").operator(), Some(Rsv::BAnd));
        // A command is not an operator, and the rewind has to be complete.
        let mut l = lex("sendln");
        assert_eq!(l.operator(), None);
        assert_eq!(l.reserved_word(), Some(Rsv::SendLn));
    }

    #[test]
    fn a_line_longer_than_the_buffer_is_truncated_not_split() {
        let mut l = Lexer::new();
        l.set_line(&vec![b'x'; MAX_LINE_LEN * 2]);
        assert_eq!(l.len(), MAX_LINE_LEN - 1);
    }
}
