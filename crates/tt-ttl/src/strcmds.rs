//! The string and integer commands — the half of TTL that needs no terminal.
//!
//! Two conventions run through all of them and are worth stating once.
//!
//! **Positions are 1-based**, in `strcopy`, `strscan`, `strinsert`,
//! `strremove` and `strreplace` alike. Zero is not the first character; it is
//! usually an error, and in `strscan` it is the answer meaning "not found".
//!
//! **The answer comes back in `result`** for the commands that compute one
//! about a string (`strlen`, `strscan`, `strcompare`, `strsplit`), and in a
//! variable named as the *first* argument for the commands that build one.
//! Which of the two a command uses is not guessable from its name, so each
//! says which below.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::lexer::MAX_STR_LEN;
use crate::rsv::Rsv;
use crate::vars::{VarRef, VarType};

/// `strsplit` and `strjoin` work through `groupmatchstr1` to `groupmatchstr9`.
const MAX_GROUP: usize = 9;

/// Every command that writes a string writes it through a `TStrVal`, so the
/// result is capped at 511 bytes wherever it came from.
fn capped(mut s: Vec<u8>) -> Vec<u8> {
    s.truncate(MAX_STR_LEN - 1);
    s
}

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn string_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::StrLen => self.cmd_strlen(),
            Rsv::StrScan => self.cmd_strscan(),
            Rsv::StrCompare => self.cmd_strcompare(),
            Rsv::StrConcat => self.cmd_strconcat(),
            Rsv::StrCopy => self.cmd_strcopy(),
            Rsv::StrInsert => self.cmd_strinsert(),
            Rsv::StrRemove => self.cmd_strremove(),
            Rsv::StrTrim => self.cmd_strtrim(),
            Rsv::StrSplit => self.cmd_strsplit(),
            Rsv::StrJoin => self.cmd_strjoin(),
            Rsv::StrSpecial => self.cmd_strspecial(),
            Rsv::ToLower => self.cmd_case(false),
            Rsv::ToUpper => self.cmd_case(true),
            Rsv::Int2Str => self.cmd_int2str(),
            Rsv::Str2Int => self.cmd_str2int(),
            Rsv::Code2Str => self.cmd_code2str(),
            Rsv::Str2Code => self.cmd_str2code(),
            Rsv::RotateL => self.cmd_rotate(false),
            Rsv::RotateR => self.cmd_rotate(true),
            Rsv::Random => self.cmd_random(host),
            _ => return None,
        })
    }

    fn str_val(&mut self) -> TtlResult<Vec<u8>> {
        expr::get_str_val(&mut self.lx, &mut self.vars)
    }

    fn int_val(&mut self) -> TtlResult<i32> {
        expr::get_int_val(&mut self.lx, &mut self.vars)
    }

    fn str_var(&mut self) -> TtlResult<VarRef> {
        expr::get_str_var(&mut self.lx, &mut self.vars)
    }

    fn int_var(&mut self) -> TtlResult<VarRef> {
        expr::get_int_var(&mut self.lx, &mut self.vars)
    }

    /// `strlen <string>` → `result`, in **bytes**.
    ///
    /// Not characters: the engine has no idea what encoding the string is in,
    /// and a UTF-8 macro counting a non-ASCII string gets its byte length.
    fn cmd_strlen(&mut self) -> TtlResult<()> {
        let s = self.str_val()?;
        self.end_of_line()?;
        self.set_result(s.len() as i32);
        Ok(())
    }

    /// `strscan <string> <substring>` → `result` is the 1-based position, or 0.
    ///
    /// An empty argument on either side answers 0 rather than 1, which is
    /// tested for explicitly upstream and is not what `strstr` would have said.
    fn cmd_strscan(&mut self) -> TtlResult<()> {
        let hay = self.str_val()?;
        let needle = self.str_val()?;
        self.end_of_line()?;
        if hay.is_empty() || needle.is_empty() {
            self.set_result(0);
            return Ok(());
        }
        let pos = hay
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i as i32 + 1)
            .unwrap_or(0);
        self.set_result(pos);
        Ok(())
    }

    /// `strcompare <a> <b>` → `result` is -1, 0 or 1.
    ///
    /// `strcmp`, so the comparison is byte-wise and unsigned; there is no
    /// locale and no case folding.
    fn cmd_strcompare(&mut self) -> TtlResult<()> {
        let a = self.str_val()?;
        let b = self.str_val()?;
        self.end_of_line()?;
        self.set_result(match a.cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
        Ok(())
    }

    /// `strconcat <strvar> <string>` — append, in place.
    fn cmd_strconcat(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let add = self.str_val()?;
        self.end_of_line()?;
        let mut s = self.vars.str_at(target).to_vec();
        s.extend_from_slice(&add);
        self.vars.set_str(target, &capped(s));
        Ok(())
    }

    /// `strcopy <string> <from> <len> <strvar>` — a substring, by 1-based
    /// position and length.
    ///
    /// Nothing here is an error: a `from` below 1 becomes 1, a `len` past the
    /// end is shortened, and a `from` past the end gives the empty string.
    fn cmd_strcopy(&mut self) -> TtlResult<()> {
        let src = self.str_val()?;
        let from = self.int_val()?;
        let len = self.int_val()?;
        let target = self.str_var()?;
        self.end_of_line()?;

        let from = from.max(1) as usize;
        let avail = (src.len() + 1).saturating_sub(from);
        let len = (len.max(0) as usize).min(avail);
        // Upstream copies zero bytes from a pointer past the end of the string,
        // which is harmless there and not expressible here.
        let at = (from - 1).min(src.len());
        let out = src[at..at + len].to_vec();
        self.vars.set_str(target, &out);
        Ok(())
    }

    /// `strinsert <strvar> <index> <string>` — insert before the 1-based index.
    ///
    /// `index` may be one past the end, which appends; anything outside that is
    /// a syntax error rather than a clamp, and so is a result over 511 bytes.
    fn cmd_strinsert(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let index = self.int_val()?;
        let add = self.str_val()?;
        self.end_of_line()?;

        let s = self.vars.str_at(target).to_vec();
        if index <= 0 || index as usize > s.len() + 1 {
            return Err(TtlError::Syntax);
        }
        if s.len() + add.len() + 1 > MAX_STR_LEN {
            return Err(TtlError::Syntax);
        }
        let at = index as usize - 1;
        let mut out = s[..at].to_vec();
        out.extend_from_slice(&add);
        out.extend_from_slice(&s[at..]);
        self.vars.set_str(target, &out);
        Ok(())
    }

    /// `strremove <strvar> <index> <len>` — cut, by 1-based index.
    fn cmd_strremove(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let index = self.int_val()?;
        let len = self.int_val()?;
        self.end_of_line()?;

        let s = self.vars.str_at(target).to_vec();
        if len <= 0 || index <= 0 || (index as usize - 1 + len as usize) > s.len() {
            return Err(TtlError::Syntax);
        }
        let at = index as usize - 1;
        let mut out = s[..at].to_vec();
        out.extend_from_slice(&s[at + len as usize..]);
        self.vars.set_str(target, &out);
        Ok(())
    }

    /// `strtrim <strvar> <chars>` — strip any of `chars` from both ends.
    ///
    /// Upstream builds a 256-entry table and indexes it with a *signed* `char`,
    /// so a trim character above 0x7F reads before the start of the table.
    /// Here the byte value is used, which is what the table was for.
    fn cmd_strtrim(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let chars = self.str_val()?;
        self.end_of_line()?;

        let mut table = [false; 256];
        for &c in &chars {
            table[c as usize] = true;
        }
        let s = self.vars.str_at(target).to_vec();
        let start = s
            .iter()
            .position(|&c| !table[c as usize])
            .unwrap_or(s.len());
        let end = s
            .iter()
            .rposition(|&c| !table[c as usize])
            .map(|i| i + 1)
            .unwrap_or(0);
        let out = if start < end { &s[start..end] } else { b"" };
        self.vars.set_str(target, out);
        Ok(())
    }

    /// `strsplit <string> <delimiter> [count]` → `groupmatchstr1..9`, and
    /// `result` is how many fields there were.
    ///
    /// The delimiter is exactly one character; more or fewer is a syntax error.
    /// With `count` given, the last field keeps the whole remainder including
    /// any further delimiters — with it omitted, the remainder is thrown away
    /// instead, and `result` can come back as 10 with only nine fields stored.
    fn cmd_strsplit(&mut self) -> TtlResult<()> {
        let src = self.str_val()?;
        let delim = self.str_val()?;
        let (max_var, omit) = if self.lx.parameter_given() {
            (self.int_val()?, 0usize)
        } else {
            (MAX_GROUP as i32, 1usize)
        };
        self.end_of_line()?;

        if !(1..=MAX_GROUP as i32).contains(&max_var) {
            return Err(TtlError::Syntax);
        }
        if delim.len() != 1 {
            return Err(TtlError::Syntax);
        }
        let max_var = max_var as usize;

        let mut fields: Vec<Vec<u8>> = vec![Vec::new()];
        let mut count = 1usize;
        for &b in &src {
            if count >= max_var + omit {
                break;
            }
            if b == delim[0] {
                count += 1;
                fields.push(Vec::new());
            } else {
                fields.last_mut().unwrap().push(b);
            }
        }
        // The remainder goes into the last field only when a count was given.
        if omit == 0 {
            let consumed: usize = fields.iter().map(|f| f.len()).sum::<usize>() + count - 1;
            fields
                .last_mut()
                .unwrap()
                .extend_from_slice(&src[consumed..]);
        }

        for i in 1..=MAX_GROUP {
            let val = fields.get(i - 1).cloned().unwrap_or_default();
            self.set_group_match(i, &val);
        }
        self.set_result(count as i32);
        Ok(())
    }

    /// `strjoin <strvar> <delimiter> [count]` — the reverse, out of
    /// `groupmatchstr1..count`.
    ///
    /// The target is overwritten, not appended to, and a delimiter goes between
    /// every pair — including around groups that were left empty.
    fn cmd_strjoin(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let delim = self.str_val()?;
        let max_var = if self.lx.parameter_given() {
            self.int_val()?
        } else {
            MAX_GROUP as i32
        };
        self.end_of_line()?;
        if !(1..=MAX_GROUP as i32).contains(&max_var) {
            return Err(TtlError::Syntax);
        }

        let mut out = Vec::new();
        for i in 1..=max_var as usize {
            let name = format!("groupmatchstr{i}");
            match self.vars.find(name.as_bytes()) {
                Some((id, VarType::String)) => {
                    let s = self.vars.str_at(VarRef::Scalar(id)).to_vec();
                    out.extend_from_slice(&s);
                }
                Some(_) => return Err(TtlError::Syntax),
                None => continue,
            }
            if i < max_var as usize {
                out.extend_from_slice(&delim);
            }
        }
        self.vars.set_str(target, &capped(out));
        Ok(())
    }

    /// `strspecial <strvar> [string]` — expand `\n`, `\t`, `\\` and `\0`.
    ///
    /// With no second argument it rewrites the variable in place. A backslash
    /// before anything else is kept as a backslash, and `\0` ends the string —
    /// the value is a C string, so there is nothing after a NUL.
    fn cmd_strspecial(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let src = if self.lx.parameter_given() {
            let s = self.str_val()?;
            self.end_of_line()?;
            s
        } else {
            self.vars.str_at(target).to_vec()
        };

        let out = restore_new_line(&src);
        self.vars.set_str(target, &out);
        Ok(())
    }

    /// `tolower <strvar> <string>` / `toupper` — ASCII only, by design.
    ///
    /// Upstream compares against `'A'`/`'Z'` directly rather than calling
    /// `tolower`, so no locale gets a say and no byte above 0x7F is touched.
    fn cmd_case(&mut self, upper: bool) -> TtlResult<()> {
        let target = self.str_var()?;
        let mut s = self.str_val()?;
        self.end_of_line()?;
        for b in s.iter_mut() {
            if upper {
                if b.is_ascii_lowercase() {
                    *b -= 0x20;
                }
            } else if b.is_ascii_uppercase() {
                *b += 0x20;
            }
        }
        self.vars.set_str(target, &s);
        Ok(())
    }

    /// `int2str <strvar> <integer>` — decimal, signed.
    fn cmd_int2str(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let n = self.int_val()?;
        self.end_of_line()?;
        self.vars.set_str(target, n.to_string().as_bytes());
        Ok(())
    }

    /// `str2int <intvar> <string>` → the number, and `result` is 1 or 0.
    ///
    /// Decimal, or hexadecimal with either `0x` or TTL's own `$` prefix.
    /// Parsing stops at the first byte that does not fit and still counts as
    /// success, so `str2int n '12abc'` is 12 with `result` 1.
    fn cmd_str2int(&mut self) -> TtlResult<()> {
        let target = self.int_var()?;
        let s = self.str_val()?;
        self.end_of_line()?;

        let (digits, radix) = if s.first() == Some(&b'$') {
            (&s[1..], 16)
        } else if s.len() >= 2 && s[0] == b'0' && s[1].eq_ignore_ascii_case(&b'x') {
            (&s[2..], 16)
        } else {
            (&s[..], 10)
        };

        match scan_int(digits, radix) {
            Some(n) => {
                self.vars.set_int(target, n);
                self.set_result(1);
            }
            None => {
                self.vars.set_int(target, 0);
                self.set_result(0);
            }
        }
        Ok(())
    }

    /// `code2str <strvar> <integer>` — the four bytes of the number, most
    /// significant first, with leading zero bytes dropped.
    ///
    /// A zero byte in the *middle* is not dropped but does end the string, so
    /// `code2str s $01000041` is one byte long.
    fn cmd_code2str(&mut self) -> TtlResult<()> {
        let target = self.str_var()?;
        let n = self.int_val()?;
        self.end_of_line()?;
        let bytes = (n as u32).to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        self.vars.set_str(target, &bytes[first..]);
        Ok(())
    }

    /// `str2code <intvar> <string>` — the *last* four bytes of the string, most
    /// significant first. A shorter string is not padded; a longer one loses
    /// its beginning.
    fn cmd_str2code(&mut self) -> TtlResult<()> {
        let target = self.int_var()?;
        let s = self.str_val()?;
        self.end_of_line()?;
        let take = s.len().min(4);
        let mut n: u32 = 0;
        for &b in &s[s.len() - take..] {
            n = n.wrapping_mul(256).wrapping_add(b as u32);
        }
        self.vars.set_int(target, n as i32);
        Ok(())
    }

    /// `rotateleft <intvar> <value> <count>` / `rotateright` — a true rotate,
    /// 32 bits wide, with the count reduced modulo 32 in either direction.
    fn cmd_rotate(&mut self, right: bool) -> TtlResult<()> {
        let target = self.int_var()?;
        let x = self.int_val()?;
        let n = self.int_val()?;
        self.end_of_line()?;

        let n = if right { n.wrapping_neg() } else { n };
        let n = n.rem_euclid(32) as u32;
        let v = (x as u32).rotate_left(n);
        self.vars.set_int(target, v as i32);
        Ok(())
    }

    /// `random <intvar> <max>` — uniform in 0..=max, `max` at least 1.
    ///
    /// The rejection loop is upstream's and is the reason this is not a plain
    /// modulo: throwing away the last partial block of the 32-bit range is what
    /// keeps the small values from being likelier than the large ones. The
    /// entropy comes from the host so a test can be repeatable.
    fn cmd_random(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = self.int_var()?;
        let max = self.int_val()?;
        if self.lx.first_char() != 0 || max <= 0 {
            return Err(TtlError::Syntax);
        }

        let range = (max as u32).wrapping_add(1);
        // 2**32 % range == (2**32 - range) % range
        let min = range.wrapping_neg() % range;
        let mut n = host.random_u32();
        while n < min {
            n = host.random_u32();
        }
        self.vars.set_int(target, (n % range) as i32);
        Ok(())
    }

    /// `SetGroupMatchStr` — write `groupmatchstrN`, if it is still a string.
    pub(crate) fn set_group_match(&mut self, no: usize, val: &[u8]) {
        let name = format!("groupmatchstr{no}");
        if let Some((id, VarType::String)) = self.vars.find(name.as_bytes()) {
            self.vars.set_str(VarRef::Scalar(id), val);
        }
    }
}

/// `RestoreNewLine` (`common/ttlib.c:619`) — expand `\n`, `\t`, `\\` and `\0`.
///
/// `strspecial` is the command for it, and the `<special>` argument that
/// `messagebox`, `statusbox`, `yesnobox` and `inputbox` all carry runs the same
/// function over their message. A backslash before anything else stays a
/// backslash, including a trailing one — upstream reads the byte after it,
/// which is the terminator, and takes the `default:` arm.
///
/// The cut at the first NUL is upstream's callers rather than upstream's
/// function: it writes the NUL into the buffer and keeps going, and then every
/// caller hands the buffer to something that treats it as a C string. Doing it
/// here makes that one rule instead of five.
pub(crate) fn restore_new_line(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'\\' && i + 1 < src.len() {
            let (byte, step) = match src[i + 1] {
                b'\\' => (b'\\', 2),
                b'n' => (b'\n', 2),
                b't' => (b'\t', 2),
                b'0' => (0u8, 2),
                _ => (b'\\', 1),
            };
            if byte == 0 {
                break;
            }
            out.push(byte);
            i += step;
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

/// `sscanf("%d")` / `sscanf("%i")` on a hex string, in the part TTL uses.
///
/// Leading whitespace and a sign are allowed, parsing stops at the first byte
/// that is not a digit of the radix, and the answer is `None` only when there
/// was no digit at all — which is what makes `result` 0.
pub(crate) fn scan_int(s: &[u8], radix: u32) -> Option<i32> {
    let mut i = 0;
    while i < s.len() && (s[i] as char).is_whitespace() {
        i += 1;
    }
    let neg = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut n: u32 = 0;
    while i < s.len() {
        let Some(d) = (s[i] as char).to_digit(radix) else {
            break;
        };
        n = n.wrapping_mul(radix).wrapping_add(d);
        i += 1;
    }
    if i == start {
        return None;
    }
    Some(if neg {
        (n as i32).wrapping_neg()
    } else {
        n as i32
    })
}

#[cfg(test)]
mod tests {
    use crate::host::RecordingHost;
    use crate::interp::Interp;

    fn out(src: &str) -> String {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        assert!(
            host.errors.is_empty(),
            "unexpected errors: {:?}",
            host.errors
        );
        String::from_utf8_lossy(&host.output).into_owned()
    }

    fn err(src: &str) -> crate::TtlError {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        assert!(!host.errors.is_empty(), "expected an error");
        host.errors[0].0
    }

    #[test]
    fn strlen_counts_bytes() {
        assert_eq!(out("strlen 'abc'\ndispstr result"), "3");
        assert_eq!(out("strlen ''\ndispstr result"), "0");
        // Two bytes for the pound sign, and TTL says two.
        assert_eq!(out("strlen '£'\ndispstr result"), "2");
    }

    #[test]
    fn strscan_is_one_based_and_zero_for_absent() {
        assert_eq!(out("strscan 'abcabc' 'ca'\ndispstr result"), "3");
        assert_eq!(out("strscan 'abc' 'z'\ndispstr result"), "0");
        // An empty needle answers 0, where `strstr` would have said 1.
        assert_eq!(out("strscan 'abc' ''\ndispstr result"), "0");
        assert_eq!(out("strscan '' 'a'\ndispstr result"), "0");
    }

    #[test]
    fn strcompare_is_bytewise_and_three_valued() {
        assert_eq!(out("strcompare 'a' 'a'\ndispstr result"), "0");
        assert_eq!(out("strcompare 'a' 'b'\ndispstr result"), "-1");
        assert_eq!(out("strcompare 'b' 'a'\ndispstr result"), "1");
        assert_eq!(out("strcompare 'A' 'a'\ndispstr result"), "-1");
        assert_eq!(out("strcompare 'ab' 'a'\ndispstr result"), "1");
    }

    #[test]
    fn strconcat_appends_in_place() {
        assert_eq!(out("s = 'ab'\nstrconcat s 'cd'\ndispstr s"), "abcd");
    }

    #[test]
    fn strcopy_clamps_instead_of_failing() {
        assert_eq!(out("strcopy 'abcdef' 2 3 d\ndispstr d"), "bcd");
        assert_eq!(out("strcopy 'abcdef' 0 3 d\ndispstr d"), "abc");
        assert_eq!(out("strcopy 'abcdef' 5 99 d\ndispstr d"), "ef");
        assert_eq!(out("strcopy 'abcdef' 99 3 d\ndispstr d"), "");
        assert_eq!(out("strcopy 'abcdef' 2 0-1 d\ndispstr d"), "");
    }

    #[test]
    fn strinsert_takes_one_past_the_end_and_nothing_further() {
        assert_eq!(out("s = 'ad'\nstrinsert s 2 'bc'\ndispstr s"), "abcd");
        assert_eq!(out("s = 'ab'\nstrinsert s 3 'c'\ndispstr s"), "abc");
        assert_eq!(err("s = 'ab'\nstrinsert s 4 'c'"), crate::TtlError::Syntax);
        assert_eq!(err("s = 'ab'\nstrinsert s 0 'c'"), crate::TtlError::Syntax);
    }

    #[test]
    fn strremove_cuts_by_index_and_length() {
        assert_eq!(out("s = 'abcdef'\nstrremove s 2 3\ndispstr s"), "aef");
        assert_eq!(out("s = 'abc'\nstrremove s 1 3\ndispstr s"), "");
        assert_eq!(err("s = 'abc'\nstrremove s 2 3"), crate::TtlError::Syntax);
        assert_eq!(err("s = 'abc'\nstrremove s 1 0"), crate::TtlError::Syntax);
    }

    #[test]
    fn strtrim_takes_a_set_of_characters_from_both_ends() {
        assert_eq!(out("s = '  ab  '\nstrtrim s ' '\ndispstr '['s']'"), "[ab]");
        assert_eq!(out("s = 'xyaybx'\nstrtrim s 'xy'\ndispstr s"), "ayb");
        assert_eq!(out("s = 'aaa'\nstrtrim s 'a'\ndispstr '['s']'"), "[]");
        assert_eq!(out("s = 'ab'\nstrtrim s ''\ndispstr s"), "ab");
    }

    #[test]
    fn strsplit_fills_the_group_variables() {
        let src =
            "strsplit 'a,b,c' ','\ndispstr result groupmatchstr1 groupmatchstr2 groupmatchstr3";
        assert_eq!(out(src), "3abc");
        // Consecutive delimiters make empty fields; they are not collapsed.
        assert_eq!(
            out("strsplit 'a,,b' ','\ndispstr result '['groupmatchstr2']'"),
            "3[]"
        );
        assert_eq!(err("strsplit 'a,b' ',,'"), crate::TtlError::Syntax);
        assert_eq!(err("strsplit 'a,b' ',' 0"), crate::TtlError::Syntax);
    }

    #[test]
    fn a_count_keeps_the_remainder_and_omitting_it_throws_it_away() {
        assert_eq!(
            out("strsplit 'a,b,c,d' ',' 2\ndispstr result '|'groupmatchstr2"),
            "2|b,c,d"
        );
        // Ten fields with no count: nine are kept, `result` still says ten.
        let ten = "strsplit '1,2,3,4,5,6,7,8,9,10' ','\ndispstr result '|'groupmatchstr9";
        assert_eq!(out(ten), "10|9");
    }

    #[test]
    fn strjoin_puts_them_back() {
        assert_eq!(
            out("strsplit 'a,b,c' ','\nstrjoin s '-' 3\ndispstr s"),
            "a-b-c"
        );
        // Groups past the split are empty, and still get their delimiters.
        assert_eq!(
            out("strsplit 'a,b' ','\nstrjoin s '-' 4\ndispstr s"),
            "a-b--"
        );
    }

    #[test]
    fn strspecial_expands_the_four_escapes_and_keeps_the_rest() {
        assert_eq!(
            out("s = 'a\\nb'\nstrspecial s\nstrlen s\ndispstr result"),
            "3"
        );
        assert_eq!(
            out("s = 'a\\tb'\nstrspecial s\nstrscan s #9\ndispstr result"),
            "2"
        );
        assert_eq!(out("s = 'a\\\\b'\nstrspecial s\ndispstr s"), "a\\b");
        // An unknown escape keeps its backslash, and the letter after it.
        assert_eq!(out("s = 'a\\qb'\nstrspecial s\ndispstr s"), "a\\qb");
        // `\0` is a NUL, and a TTL string ends at one.
        assert_eq!(out("s = 'ab\\0cd'\nstrspecial s\ndispstr s"), "ab");
        // With a second argument it reads from there instead.
        assert_eq!(
            out("s = ''\nstrspecial s 'x\\ny'\nstrlen s\ndispstr result"),
            "3"
        );
    }

    #[test]
    fn case_conversion_is_ascii_only() {
        assert_eq!(out("tolower d 'AbC1'\ndispstr d"), "abc1");
        assert_eq!(out("toupper d 'AbC1'\ndispstr d"), "ABC1");
        // A byte above 0x7F is left alone, whatever it might mean.
        assert_eq!(out("toupper d #200\nstr2code n d\ndispstr n"), "200");
    }

    #[test]
    fn int2str_and_str2int_round_trip() {
        assert_eq!(out("int2str s 0-42\ndispstr s"), "-42");
        assert_eq!(out("str2int n '42'\ndispstr n result"), "421");
        assert_eq!(out("str2int n '0x1f'\ndispstr n result"), "311");
        assert_eq!(out("str2int n '$1f'\ndispstr n result"), "311");
        assert_eq!(out("str2int n 'zz'\ndispstr n result"), "00");
        // A trailing tail is not an error; the digits it did read still count.
        assert_eq!(out("str2int n '12abc'\ndispstr n result"), "121");
    }

    #[test]
    fn code2str_and_str2code_are_big_endian() {
        assert_eq!(out("code2str s $414243\ndispstr s"), "ABC");
        assert_eq!(out("str2code n 'ABC'\ndispstr n"), "4276803");
        // Only the last four bytes survive the trip back.
        assert_eq!(out("str2code n 'ABCDE'\ncode2str s n\ndispstr s"), "BCDE");
        // A zero byte in the middle ends the string it produced.
        assert_eq!(out("code2str s $01000041\nstrlen s\ndispstr result"), "1");
    }

    #[test]
    fn rotate_wraps_the_bits_round_rather_than_dropping_them() {
        assert_eq!(out("rotateleft n 1 1\ndispstr n"), "2");
        assert_eq!(out("rotateright n 1 1\ndispstr n"), (i32::MIN).to_string());
        assert_eq!(out("rotateleft n 1 32\ndispstr n"), "1");
        assert_eq!(out("rotateleft n 1 33\ndispstr n"), "2");
        assert_eq!(out("rotateright n 2 33\ndispstr n"), "1");
    }

    #[test]
    fn random_stays_inside_its_range_and_asks_the_host_for_entropy() {
        assert_eq!(err("random n 0"), crate::TtlError::Syntax);
        let mut host = RecordingHost::new();
        // The default host counts up, which is enough to show the modulo.
        let mut it = Interp::new(
            "t.ttl",
            b"for i 1 5\nrandom n 3\ndispstr n\nnext".to_vec(),
            &mut host,
        );
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        for b in &host.output {
            assert!((b'0'..=b'3').contains(b), "out of range: {}", *b as char);
        }
    }
}
