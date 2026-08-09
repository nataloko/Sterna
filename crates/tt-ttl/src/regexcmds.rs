//! `regexoption`, `strmatch`, `strreplace` and `waitregex`.
//!
//! The engine and the three settings are in [`crate::regex`]; this is the
//! commands that reach for them.
//!
//! **Three of the four report success three different ways.** `strmatch`'s
//! `result` is the **byte position** the match started at, counting from one;
//! `waitregex`'s is the **index of the pattern** that matched, counting from
//! one; and `strreplace`'s is 1, 0 or -1. All three come out of the same
//! function upstream, which returns a position, and the difference is what each
//! caller does with it.

use crate::error::{TtlError, TtlResult};
use crate::expr::{self, Eval};
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::regex::{self, Enc, Syn};
use crate::rsv::Rsv;
use crate::vars::{VarRef, VarType};
use crate::wait::MAX_WAIT;

use onig::RegexOptions;

/// `regexoption`'s encoding names (`ttl.cpp:3602`), each with and without its
/// `ENCODING_` prefix. Extracted from the `_stricmp` chain rather than
/// transcribed — the table is fifty-odd names long and a diff is the only way
/// to check one of those.
const ENCODINGS: &[(&str, Enc)] = &[
    ("ASCII", Enc::Ascii),
    ("ISO_8859_1", Enc::Iso8859_1),
    ("ISO_8859_2", Enc::Iso8859_2),
    ("ISO_8859_3", Enc::Iso8859_3),
    ("ISO_8859_4", Enc::Iso8859_4),
    ("ISO_8859_5", Enc::Iso8859_5),
    ("ISO_8859_6", Enc::Iso8859_6),
    ("ISO_8859_7", Enc::Iso8859_7),
    ("ISO_8859_8", Enc::Iso8859_8),
    ("ISO_8859_9", Enc::Iso8859_9),
    ("ISO_8859_10", Enc::Iso8859_10),
    // No `ISO_8859_12`, which is not an omission: it was abandoned before it
    // was ever standardised and Oniguruma has no such encoding either.
    ("ISO_8859_11", Enc::Iso8859_11),
    ("ISO_8859_13", Enc::Iso8859_13),
    ("ISO_8859_14", Enc::Iso8859_14),
    ("ISO_8859_15", Enc::Iso8859_15),
    ("ISO_8859_16", Enc::Iso8859_16),
    ("UTF8", Enc::Utf8),
    ("UTF16_BE", Enc::Utf16Be),
    ("UTF16_LE", Enc::Utf16Le),
    ("UTF32_BE", Enc::Utf32Be),
    ("UTF32_LE", Enc::Utf32Le),
    ("EUC_JP", Enc::EucJp),
    ("EUC_TW", Enc::EucTw),
    ("EUC_KR", Enc::EucKr),
    ("EUC_CN", Enc::EucCn),
    ("SJIS", Enc::Sjis),
    // `CP932` is a second spelling of the same encoding, and the only name in
    // the table that has one.
    ("CP932", Enc::Sjis),
    ("KOI8_R", Enc::Koi8R),
    ("CP1251", Enc::Cp1251),
    ("BIG5", Enc::Big5),
    ("GB18030", Enc::Gb18030),
];

/// The syntax names (`ttl.cpp:3754`). `SYNTAX_DEFAULT` is the one entry with
/// **no** bare form — there is no `regexoption 'DEFAULT'`.
const SYNTAXES: &[(&str, Syn, bool)] = &[
    ("DEFAULT", Syn::Default, false),
    ("ASIS", Syn::Asis, true),
    ("POSIX_BASIC", Syn::PosixBasic, true),
    ("POSIX_EXTENDED", Syn::PosixExtended, true),
    ("EMACS", Syn::Emacs, true),
    ("GREP", Syn::Grep, true),
    ("GNU_REGEX", Syn::GnuRegex, true),
    ("JAVA", Syn::Java, true),
    ("PERL", Syn::Perl, true),
    ("PERL_NG", Syn::PerlNg, true),
    ("RUBY", Syn::Ruby, true),
];

/// The option names, all of which also have a bare form except `OPTION_NONE`.
const OPTIONS: &[(&str, RegexOptions)] = &[
    ("SINGLELINE", RegexOptions::REGEX_OPTION_SINGLELINE),
    ("MULTILINE", RegexOptions::REGEX_OPTION_MULTILINE),
    ("IGNORECASE", RegexOptions::REGEX_OPTION_IGNORECASE),
    ("EXTEND", RegexOptions::REGEX_OPTION_EXTEND),
    ("FIND_LONGEST", RegexOptions::REGEX_OPTION_FIND_LONGEST),
    ("FIND_NOT_EMPTY", RegexOptions::REGEX_OPTION_FIND_NOT_EMPTY),
    (
        "NEGATE_SINGLELINE",
        RegexOptions::REGEX_OPTION_NEGATE_SINGLELINE,
    ),
    (
        "DONT_CAPTURE_GROUP",
        RegexOptions::REGEX_OPTION_DONT_CAPTURE_GROUP,
    ),
    ("CAPTURE_GROUP", RegexOptions::REGEX_OPTION_CAPTURE_GROUP),
];

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn regex_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::RegexOption => self.cmd_regex_option(),
            Rsv::StrMatch => self.cmd_str_match(),
            Rsv::StrReplace => self.cmd_str_replace(),
            Rsv::WaitRegex => self.cmd_wait_regex(host),
            _ => return None,
        })
    }

    /// `regexoption [<option> ...]` — set the encoding, the syntax and the
    /// options for every later match. No `result`.
    ///
    /// **Each of the three is left alone unless this call mentioned it**, so
    /// `regexoption 'IGNORECASE'` keeps whatever encoding was in force and
    /// `regexoption 'ASCII'` keeps the options. With no arguments at all
    /// nothing changes; there is no "put it back to the defaults".
    ///
    /// **Naming an encoding or a syntax twice is a syntax error and naming an
    /// option twice is not**, because the options are ORed together. The one
    /// exception is `OPTION_NONE`, which refuses to follow anything — and the
    /// check is one-sided, so `regexoption 'OPTION_NONE' 'IGNORECASE'` is
    /// accepted and turns `IGNORECASE` **on** while
    /// `regexoption 'IGNORECASE' 'OPTION_NONE'` is refused. Reproduced;
    /// upstream's own test is written the accepted way round.
    fn cmd_regex_option(&mut self) -> TtlResult<()> {
        let mut enc: Option<Enc> = None;
        let mut syn: Option<Syn> = None;
        let mut opt = RegexOptions::REGEX_OPTION_NONE;
        let mut said_none = false;

        while self.lx.parameter_given() {
            let word = expr::get_str_val(&mut self.lx, &mut self.vars)?;

            if let Some(e) = lookup(
                &word,
                "ENCODING_",
                ENCODINGS.iter().map(|&(n, v)| (n, v, true)),
            ) {
                if enc.replace(e).is_some() {
                    return Err(TtlError::Syntax);
                }
            } else if let Some(s) = lookup(
                &word,
                "SYNTAX_",
                SYNTAXES.iter().map(|&(n, v, bare)| (n, v, bare)),
            ) {
                if syn.replace(s).is_some() {
                    return Err(TtlError::Syntax);
                }
            } else if eq(&word, "OPTION_NONE") {
                if opt != RegexOptions::REGEX_OPTION_NONE || said_none {
                    return Err(TtlError::Syntax);
                }
                said_none = true;
            } else if let Some(o) =
                lookup(&word, "OPTION_", OPTIONS.iter().map(|&(n, v)| (n, v, true)))
            {
                opt |= o;
            } else {
                return Err(TtlError::Syntax);
            }
        }

        if let Some(e) = enc {
            self.rx.enc = e;
        }
        if let Some(s) = syn {
            self.rx.syn = s;
        }
        if opt != RegexOptions::REGEX_OPTION_NONE || said_none {
            self.rx.opt = opt;
        }
        Ok(())
    }

    /// `strmatch <string> <pattern>` → `result` is the **byte position** the
    /// match started at, counting from one, or 0.
    ///
    /// A position of one is therefore a match at the start, which is also the
    /// only truthy value a macro can rely on: `if result then` is true for any
    /// match. A pattern Oniguruma refuses is 0 as well, indistinguishable from
    /// not matching — `strreplace` is the only command here that tells them
    /// apart.
    fn cmd_str_match(&mut self) -> TtlResult<()> {
        let target = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let pattern = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        let found = match regex::search(&self.rx, &pattern, &target) {
            Ok(Some(m)) => {
                self.record_match(&m);
                m.at + 1
            }
            _ => 0,
        };
        self.set_result(found as i32);
        Ok(())
    }

    /// `strreplace <strvar> <pos> <oldstr> <newstr>` — replace the first match
    /// at or after `<pos>`, in place. `result` is 1, 0 for no match, -1 for a
    /// pattern Oniguruma refused.
    ///
    /// **The length of what is being replaced is read back out of the
    /// `matchstr` variable**, not from the match — upstream looks the variable
    /// up by name and measures its string (`ttl.cpp:5085`), having written it a
    /// dozen lines earlier. The route is kept because it is the route, but the
    /// arm where the lookup fails and `result` is 0 is **dead**: `matchstr` is
    /// one of the variables `InitTTL` creates (`ttl.cpp:202`) and TTL fixes a
    /// variable's type at its first assignment, so it is a string for the whole
    /// run and there is no way for a macro to make it anything else.
    ///
    /// `<pos>` is one-based and counts **bytes**; outside the string it is
    /// `result` 0. The search runs on the tail from `<pos>` on, so `^` anchors
    /// there rather than at the start of the whole string.
    fn cmd_str_replace(&mut self) -> TtlResult<()> {
        let dest = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let pos = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let old = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let new = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        let src = self.vars.str_at(dest).to_vec();
        if pos > src.len() as i32 || pos <= 0 {
            self.set_result(0);
            return Ok(());
        }
        let pos = pos as usize - 1;

        let m = match regex::search(&self.rx, &old, &src[pos..]) {
            Ok(Some(m)) => m,
            Ok(None) => {
                self.set_result(0);
                return Ok(());
            }
            Err(_) => {
                self.set_result(-1);
                return Ok(());
            }
        };
        self.record_match(&m);
        let at = pos + m.at;

        // Upstream measures the match by looking `matchstr` up again. It has
        // just been written, so normally this is the match — but a macro that
        // made `matchstr` an integer takes the failure arm instead.
        let Some((id, VarType::String)) = self.vars.find(b"matchstr") else {
            self.set_result(0);
            return Ok(());
        };
        let len = self.vars.str_at(VarRef::Scalar(id)).len();

        let mut out = src[..at].to_vec();
        out.extend_from_slice(&new);
        if at + len <= src.len() {
            out.extend_from_slice(&src[at + len..]);
        }
        self.vars.set_str(dest, &out);
        self.set_result(1);
        Ok(())
    }

    /// `waitregex <string> [<string> ...]` — `wait` with the patterns read as
    /// regular expressions. `result` is which one matched, counting from one,
    /// or 0 for a timeout.
    ///
    /// **It matches a line at a time, not a byte at a time.** `wait` feeds each
    /// arriving byte to every pattern; this waits for a **newline** and then
    /// runs the patterns over the line that just ended (`ttmdde.c:721`), which
    /// is what lets `^` and `$` mean anything. A last attempt is made when the
    /// read loop stops, so output with no trailing newline still gets one look.
    ///
    /// The matched line goes into `inputstr`, which plain `wait` does not do —
    /// only `waitln` does. And the line has **not** had its terminator removed,
    /// because the newline that triggered the attempt has not been added yet:
    /// a CRLF line therefore leaves a trailing CR in `inputstr`.
    ///
    /// An **empty** line never matches, whatever the pattern
    /// (`FindRegexString`'s `RecvLnPtr == 0` guard), so a `waitregex 'x*'` does
    /// not fire on a blank line the way it would on an empty string.
    fn cmd_wait_regex(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let mut patterns = Vec::new();
        for _ in 0..MAX_WAIT {
            match self.lx.string() {
                Ok(Some(s)) => patterns.push(s),
                Ok(None) => match expr::get_expression(&mut self.lx, &mut self.vars) {
                    Ok(Some(Eval::Str(r))) => patterns.push(self.vars.str_at(r).to_vec()),
                    Ok(Some(_)) => return Err(TtlError::TypeMismatch),
                    Ok(None) => break,
                    Err(e) => return Err(e),
                },
                Err(e) => return Err(e),
            }
        }
        self.end_of_line()?;
        if !host.linked() {
            return Err(TtlError::LinkFirst);
        }
        // As with `wait`, no patterns at all is not an error and not a wait.
        if patterns.is_empty() {
            return Ok(());
        }

        let deadline = self.deadline();
        let mut found = 0;
        while let Some(b) = self.read_byte(host, deadline) {
            if b == 0x0a {
                if let Some(i) = self.match_recv_line(&patterns) {
                    found = i;
                    break;
                }
            }
            self.recv_line.put(b);
        }
        // The read loop ends on a timeout, and upstream tries once more against
        // whatever arrived without a newline behind it.
        if found == 0 {
            if let Some(i) = self.match_recv_line(&patterns) {
                found = i;
            }
        }
        self.set_result(found as i32);
        Ok(())
    }

    /// `FindRegexString` — try each pattern against the line so far, lowest
    /// first, and on a match put the line in `inputstr` and empty the buffer.
    fn match_recv_line(&mut self, patterns: &[Vec<u8>]) -> Option<usize> {
        if self.recv_line.is_empty() {
            return None;
        }
        let line = self.recv_line.peek().to_vec();
        for (i, p) in patterns.iter().enumerate() {
            if let Ok(Some(m)) = regex::search(&self.rx, p, &line) {
                self.record_match(&m);
                self.recv_line.clear();
                self.set_input_str(&line);
                return Some(i + 1);
            }
        }
        None
    }

    /// `SetMatchStr` and `SetGroupMatchStr` — the whole match into `matchstr`
    /// and the groups into `groupmatchstr1` upwards.
    ///
    /// The nine group variables are cleared first and **only when there was a
    /// match**, so a failed `strmatch` leaves the previous one's groups where
    /// they were. Each is written only if the macro still has it as a string:
    /// they are ordinary variables, and one that has been assigned an integer
    /// simply stops being told anything.
    fn record_match(&mut self, m: &regex::Found) {
        self.set_str_variable(b"matchstr", &m.whole);
        for i in 1..=9 {
            self.set_str_variable(format!("groupmatchstr{i}").as_bytes(), b"");
        }
        for (i, g) in m.groups.iter().enumerate() {
            self.set_str_variable(format!("groupmatchstr{}", i + 1).as_bytes(), g);
        }
    }

    /// Write a named string variable, if it is still one.
    fn set_str_variable(&mut self, name: &[u8], value: &[u8]) {
        if let Some((id, VarType::String)) = self.vars.find(name) {
            self.vars.set_str(VarRef::Scalar(id), value);
        }
    }
}

/// Case-insensitive comparison, which is `_stricmp` for names that are ASCII.
fn eq(word: &[u8], name: &str) -> bool {
    word.eq_ignore_ascii_case(name.as_bytes())
}

/// Find `word` in a table, accepting `<prefix><name>` always and the bare
/// `<name>` when the entry allows it.
fn lookup<T: Copy>(
    word: &[u8],
    prefix: &str,
    table: impl Iterator<Item = (&'static str, T, bool)>,
) -> Option<T> {
    for (name, value, bare) in table {
        if eq(word, &format!("{prefix}{name}")) || (bare && eq(word, name)) {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    fn err_of(src: &str) -> TtlError {
        let h = run(src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn strmatch_reports_where_it_matched_and_fills_the_groups() {
        assert_eq!(
            out("strmatch 'abcdef' 'cd'\ndispstr result';'matchstr"),
            "3;cd"
        );
        assert_eq!(
            out("strmatch 'user@host' '^(\\w+)@(\\w+)$'\n\
                 dispstr result';'matchstr';'groupmatchstr1';'groupmatchstr2"),
            "1;user@host;user;host"
        );
        // No match is 0, and so is a pattern Oniguruma refused.
        assert_eq!(out("strmatch 'abc' 'zzz'\ndispstr result"), "0");
        assert_eq!(out("strmatch 'abc' '(unclosed'\ndispstr result"), "0");
        assert_eq!(err_of("strmatch 'abc'"), TtlError::Syntax);
    }

    #[test]
    fn a_failed_match_leaves_the_previous_ones_groups_alone() {
        // The clear is inside the "did it match" arm upstream, so the groups
        // are the last successful match's until another one succeeds.
        assert_eq!(
            out("strmatch 'a1' '(\\d)'\nstrmatch 'zz' '(\\d)'\n\
                 dispstr result';'groupmatchstr1"),
            "0;1"
        );
        // A match with fewer groups does clear the ones it does not fill.
        assert_eq!(
            out("strmatch 'a1' '(\\d)'\nstrmatch 'b' 'b'\n\
                 dispstr groupmatchstr1'!'"),
            "!"
        );
    }

    #[test]
    fn the_default_dialect_has_backreferences_and_look_around() {
        assert_eq!(out("strmatch 'abab' '(ab)\\1'\ndispstr result"), "1");
        assert_eq!(
            out("strmatch 'login: root' '(?<=login: )\\w+'\ndispstr matchstr"),
            "root"
        );
    }

    #[test]
    fn regexoption_changes_only_what_it_names() {
        // Options persist across a call that only names an encoding.
        assert_eq!(
            out("regexoption 'IGNORECASE'\n\
                 regexoption 'ASCII'\n\
                 strmatch 'ABC' 'abc'\ndispstr result"),
            "1"
        );
        // ...and an encoding persists across a call that only names an option.
        assert_eq!(
            out("regexoption 'OPTION_NONE'\nstrmatch 'ABC' 'abc'\ndispstr result"),
            "0"
        );
        // No arguments at all changes nothing.
        assert_eq!(
            out("regexoption 'IGNORECASE'\nregexoption\n\
                 strmatch 'ABC' 'abc'\ndispstr result"),
            "1"
        );
    }

    #[test]
    fn regexoption_refuses_a_repeat_except_where_it_ors() {
        assert_eq!(err_of("regexoption 'ASCII' 'UTF8'"), TtlError::Syntax);
        assert_eq!(err_of("regexoption 'RUBY' 'PERL'"), TtlError::Syntax);
        assert_eq!(err_of("regexoption 'NOT_A_THING'"), TtlError::Syntax);
        // Options are ORed, so naming two — or one twice — is fine.
        assert_eq!(
            out("regexoption 'IGNORECASE' 'EXTEND' 'IGNORECASE'\n\
                 strmatch 'ABC' 'a b c'\ndispstr result"),
            "1"
        );
        // `OPTION_NONE` refuses to follow another option...
        assert_eq!(
            err_of("regexoption 'IGNORECASE' 'OPTION_NONE'"),
            TtlError::Syntax
        );
        // ...but the test is one-sided, so the other order is accepted and the
        // option that follows is turned on.
        assert_eq!(
            out("regexoption 'OPTION_NONE' 'IGNORECASE'\n\
                 strmatch 'ABC' 'abc'\ndispstr result"),
            "1"
        );
        // `SYNTAX_DEFAULT` has no bare spelling.
        assert_eq!(err_of("regexoption 'DEFAULT'"), TtlError::Syntax);
        assert_eq!(out("regexoption 'SYNTAX_DEFAULT'\ndispstr 'ok'"), "ok");
    }

    #[test]
    fn regexoption_really_does_swap_the_engine() {
        // ASIS has no metacharacters, so the dot is a dot.
        assert_eq!(
            out(
                "regexoption 'ASIS'\nstrmatch 'abc' 'a.c'\ndispstr result';'\n\
                 strmatch 'a.c' 'a.c'\ndispstr result"
            ),
            "0;1"
        );
        // POSIX basic has no alternation.
        assert_eq!(
            out("regexoption 'POSIX_BASIC'\nstrmatch 'b' 'a|b'\ndispstr result"),
            "0"
        );
        assert_eq!(
            out("regexoption 'ENCODING_CP932'\nstrmatch 'x' 'x'\ndispstr result"),
            "1"
        );
    }

    #[test]
    fn strreplace_replaces_the_first_match_from_a_position() {
        assert_eq!(
            out("s = 'one two one'\nstrreplace s 1 'one' 'ONE'\ndispstr result';'s"),
            "1;ONE two one"
        );
        // The position skips the first one; it counts from 1 and is bytes.
        assert_eq!(
            out("s = 'one two one'\nstrreplace s 4 'one' 'ONE'\ndispstr result';'s"),
            "1;one two ONE"
        );
        // A regular expression, not a literal.
        assert_eq!(
            out("s = 'a1b22c'\nstrreplace s 1 '[0-9]+' '#'\ndispstr s"),
            "a#b22c"
        );
        // No match, and a position outside the string.
        assert_eq!(
            out("s = 'abc'\nstrreplace s 1 'zzz' 'x'\ndispstr result';'s"),
            "0;abc"
        );
        assert_eq!(
            out("s = 'abc'\nstrreplace s 0 'a' 'x'\ndispstr result';'s"),
            "0;abc"
        );
        assert_eq!(
            out("s = 'abc'\nstrreplace s 9 'a' 'x'\ndispstr result';'s"),
            "0;abc"
        );
        // A pattern Oniguruma refused is -1, which is the one place the two
        // kinds of failure are told apart.
        assert_eq!(
            out("s = 'abc'\nstrreplace s 1 '(unclosed' 'x'\ndispstr result"),
            "-1"
        );
    }

    #[test]
    fn strreplace_measures_the_match_by_reading_matchstr_back() {
        // Upstream takes the length from the variable rather than from the
        // match, and its "the variable is not a string" arm cannot be reached:
        // `matchstr` is predefined as a string and a TTL variable's type is
        // fixed at its first assignment, so this is what trying costs.
        assert_eq!(err_of("matchstr = 3"), TtlError::TypeMismatch);

        // What the route does mean is that `matchstr` is left holding the text
        // that was replaced, which is the same thing `strmatch` would have put
        // there.
        assert_eq!(
            out("s = 'one two'\nstrreplace s 1 'o\\w+' 'X'\ndispstr s';'matchstr"),
            "X two;one"
        );
    }

    #[test]
    fn strreplace_anchors_at_the_position_rather_than_at_the_string() {
        // The search runs on the tail, so `^` means "at <pos>".
        assert_eq!(
            out("s = 'abcabc'\nstrreplace s 4 '^abc' 'X'\ndispstr result';'s"),
            "1;abcX"
        );
    }

    #[test]
    fn waitregex_matches_a_line_at_a_time_and_fills_inputstr() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input = b"noise\r\nlogin: root\r\nmore\r\n".to_vec().into();
        let mut it = Interp::new(
            "t.ttl",
            b"timeout = 1\nwaitregex '^login: (\\w+)$' 'zzz'\n\
              dispstr result';'groupmatchstr1"
                .to_vec(),
            &mut host,
        );
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        // `$` matched before the CR because the pattern is applied to the line
        // as it stands — which still has its CR, since the LF that triggered
        // the attempt has not been added.
        assert_eq!(String::from_utf8_lossy(&host.output), "0;");
    }

    #[test]
    fn waitregex_reports_the_pattern_index_not_the_position() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input = b"noise\nlogin: root\n".to_vec().into();
        let mut it = Interp::new(
            "t.ttl",
            b"timeout = 1\nwaitregex 'zzz' 'login: (\\w+)'\n\
              dispstr result';'inputstr';'groupmatchstr1"
                .to_vec(),
            &mut host,
        );
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        // 2 is the pattern, not the byte offset — `strmatch` would say 1.
        assert_eq!(String::from_utf8_lossy(&host.output), "2;login: root;root");
    }

    #[test]
    fn waitregex_gets_one_last_look_at_a_line_with_no_newline_behind_it() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input = b"password: ".to_vec().into();
        let mut it = Interp::new(
            "t.ttl",
            b"mtimeout = 30\nwaitregex 'password: $'\ndispstr result';'inputstr".to_vec(),
            &mut host,
        );
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(String::from_utf8_lossy(&host.output), "1;password: ");
    }

    #[test]
    fn waitregex_wants_a_terminal_and_takes_no_patterns_quietly() {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", b"waitregex 'x'".to_vec(), &mut host);
        it.run(&mut host);
        assert_eq!(host.errors.first().map(|e| e.0), Some(TtlError::LinkFirst));

        assert_eq!(out("result = 7\nwaitregex\ndispstr result"), "7");
        assert_eq!(err_of("waitregex 3"), TtlError::TypeMismatch);
    }
}
