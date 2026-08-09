//! `ttpmacro`'s own command line — `ParseParam` (`ttmdlg.cpp:82`), the
//! tokeniser under it (`ttlib.c:879`) and the file name it settles on.
//!
//! This is the launcher, not the language. Everything a macro can see of how
//! it was started — `paramcnt`, `param1`..`param9`, `params[]` — is decided
//! here before [`crate::Interp`] runs a line, and two of upstream's own test
//! scripts (`macroparam.ttl`, `params_array.ttl`) are *about* this file rather
//! than about TTL. Their `.bat` wrappers are the specification:
//!
//! ```text
//! ttpmacro.exe macrofile /vxx /ixx /V /i test1      paramcnt 6, params[2]=/vxx
//! ttpmacro.exe /V /i macrofile /v /I test2          paramcnt 4, params[2]=/v
//! ttpmacro.exe /I macrofile test3 /Vxx /ixx /V /i   paramcnt 6, params[2]=test3
//! ttpmacro.exe /i macrofile test4 /V /Vxx /ixx      paramcnt 5, params[2]=test4
//! ```
//!
//! **A switch is only a switch before the macro's name.** `ParseParam` tests
//! for `/D=`, `/I`, `/S` and `/V` inside `if (ParamCnt == 0)` and nowhere else,
//! so the `/V` in test 1 is an ordinary parameter and the `/V` in test 2 is the
//! option — same spelling, same case, opposite meaning, and the only difference
//! is which side of the filename it fell on. There is no `--` and no way to
//! escape one; putting the filename first is how a macro is given a `/V` of its
//! own.
//!
//! **The tokeniser is upstream's, not the C runtime's.** `GetParam` is nothing
//! like `CommandLineToArgvW`: a backslash is an ordinary character (which it
//! has to be, since these are Windows paths), `""` inside a quoted run is one
//! literal quote, and an unquoted `;` **ends the command line** — everything
//! after it is a comment. Reaching for `argv` semantics here gives a parser
//! that agrees on the four `.bat` lines and disagrees on the first path with a
//! space in it.
//!
//! **There are two entry points because there are two platforms.**
//! [`CmdLine::parse`] takes a whole command line, which is what
//! `GetCommandLineW` hands `ParseParam` and what Stage 3 will use. On Unix the
//! shell has already split and unquoted the arguments before `execve`, so
//! [`CmdLine::from_args`] takes them split and runs only the half of
//! `ParseParam` that is left — the switch scan and the counting. Running the
//! tokeniser over a joined `argv` instead would quote-process the text twice
//! and turn `'param 7'` back into two parameters. What that costs is `params[0]`,
//! which upstream sets to the command line *as typed*: the original spacing and
//! quoting do not survive `execve`, so what goes there is the arguments joined
//! by a space, which is `/proc/self/cmdline` with its NULs replaced.

/// `MaxStrLen` (`ttmdef.h:34`) — the buffer `ParseParam` reads each token into,
/// so a token is truncated at 511 characters.
///
/// Upstream does not truncate there: the loop calls
/// `GetParam(Temp, sizeof(Temp), cur)` where `Temp` is `wchar_t[512]`, so the
/// size is passed in *bytes* where the function counts `wchar_t`, and a token
/// of more than 511 characters is written up to 511 `wchar_t` past the end of a
/// stack array. The first call, outside the loop, passes `_countof` and is
/// right. Truncating at what the code meant is the only sane reproduction —
/// see `PLAN.md`, where it is listed with the other `ttpmacro` defects.
const MAX_STR_LEN: usize = 512;

/// `TopicName` is `wchar_t[11]` (`ttmdlg.cpp:62`), so `/D=` keeps ten
/// characters and drops the rest without saying so.
const MAX_TOPIC_LEN: usize = 10;

/// What the launcher made of its arguments.
///
/// Upstream spreads this across four globals in `ttmdlg.cpp` and one more in
/// `ttmmain.cpp`; the shape is the same. `args` is `Params[2..=ParamCnt]` —
/// index 0 is the raw command line and index 1 is the macro's own name, and
/// neither comes out of the loop that fills the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmdLine {
    /// `Params[0]` — the command line, whole and untokenised. A macro reads it
    /// as `params[0]`, which is the only way it can see the switches the
    /// launcher ate.
    pub raw: Vec<u8>,
    /// `FileName` — the first argument that was not a switch. Empty when there
    /// was none, which is one of the two states [`CmdLine::needs_prompt`]
    /// answers to.
    pub file_name: Vec<u8>,
    /// `TopicName` — the DDE topic from `/D=`, ten characters of it. Kept
    /// because a startup macro is launched with one; this port has no DDE and
    /// nothing reads it yet.
    pub topic: Vec<u8>,
    /// `SleepFlag` — `/S`, which parks the macro until the terminal is ready
    /// rather than starting it immediately.
    pub sleep: bool,
    /// `IOption` — `/I`, start minimised.
    pub i_option: bool,
    /// `VOption` — `/V`, do not show the control window at all. It beats `/I`:
    /// `ttmacro.cpp:149` tests it first.
    pub v_option: bool,
    /// `Params[2..=ParamCnt]` — everything after the macro's name, plus any
    /// switch-shaped word that came after it.
    pub args: Vec<Vec<u8>>,
    /// Whether a non-switch argument has been seen — upstream's `ParamCnt > 0`,
    /// which is the test that stops looking for switches. Not the same as
    /// `!file_name.is_empty()`: a literal `""` names the macro as far as the
    /// counting goes, and everything after it is a parameter.
    named: bool,
}

impl CmdLine {
    /// `ParseParam` over a whole command line, first term and all.
    ///
    /// The first token is the executable and is thrown away, exactly as
    /// upstream's untested `GetParam` before the loop does.
    pub fn parse(line: &[u8]) -> CmdLine {
        let mut cmd = CmdLine {
            raw: line.to_vec(),
            ..Default::default()
        };
        // "the first term shuld be executable filename of TTMACRO"
        let mut cur = match get_param(line) {
            Some((_, rest)) => rest,
            None => return cmd,
        };
        while let Some((tok, rest)) = get_param(cur) {
            cur = rest;
            cmd.push(&dequote_param(&tok));
        }
        cmd
    }

    /// The same, from arguments the platform has already split — Unix `argv`
    /// with `argv[0]` dropped.
    ///
    /// No tokenising and no dequoting: the shell did both, and doing them again
    /// would eat a quote the user escaped to get one.
    pub fn from_args<I, S>(args: I) -> CmdLine
    where
        I: IntoIterator<Item = S>,
        S: AsRef<[u8]>,
    {
        let args: Vec<Vec<u8>> = args.into_iter().map(|a| a.as_ref().to_vec()).collect();
        let mut cmd = CmdLine {
            raw: args.join(&b' '),
            ..Default::default()
        };
        for a in &args {
            cmd.push(a);
        }
        cmd
    }

    /// One token, after `DequoteParam` — the body of `ParseParam`'s loop.
    fn push(&mut self, tok: &[u8]) {
        // `if (ParamCnt == 0)`: a switch is only a switch until the macro has
        // been named.
        if !self.named {
            if tok.len() >= 3 && tok[..3].eq_ignore_ascii_case(b"/D=") {
                self.topic = truncate(&tok[3..], MAX_TOPIC_LEN);
                return;
            }
            if tok.eq_ignore_ascii_case(b"/I") {
                self.i_option = true;
                return;
            }
            if tok.eq_ignore_ascii_case(b"/S") {
                self.sleep = true;
                return;
            }
            if tok.eq_ignore_ascii_case(b"/V") {
                self.v_option = true;
                return;
            }
            self.file_name = tok.to_vec();
            self.named = true;
            return;
        }
        self.args.push(tok.to_vec());
    }

    /// `ParamCnt` — the arguments *including* the macro's own name.
    ///
    /// Zero when nothing was passed at all, which `InitTTL` then bumps to one
    /// (`ttl.cpp:214`); [`crate::Interp`] does that where upstream does, so
    /// this is the launcher's count and not the variable's.
    pub fn param_cnt(&self) -> usize {
        if self.named {
            1 + self.args.len()
        } else {
            0
        }
    }

    /// Whether the launcher would put up its file-open dialog
    /// (`ttmmain.cpp:283`): no filename, or one that is literally `*`.
    pub fn needs_prompt(&self) -> bool {
        matches!(self.file_name.first(), None | Some(b'*'))
    }

    /// `FitTTLFileName` (`ttmmain.cpp:253`) — the macro's path with `.TTL`
    /// fitted onto its last component.
    pub fn fitted_file_name(&self) -> Vec<u8> {
        let (dir, file) = split_file_name(&self.file_name);
        let mut out = dir.to_vec();
        out.extend_from_slice(&fit_file_name(file, b".TTL"));
        out
    }

    /// `ShortName` — the same, with the directory dropped. This is what a macro
    /// gets as `param1` and `params[1]`, so a script that reports its own name
    /// reports a bare filename however it was launched.
    pub fn short_name(&self) -> Vec<u8> {
        fit_file_name(split_file_name(&self.file_name).1, b".TTL")
    }
}

/// `ShortName` for a macro that was opened without a command line — the path
/// [`crate::Interp::new`] was handed, cut down the same way.
pub fn short_name_of(path: &[u8]) -> Vec<u8> {
    fit_file_name(split_file_name(path).1, b".TTL")
}

/// `GetParam` (`ttlib.c:879`) — one token and what is left after it.
///
/// Quotes are *kept*: upstream splits with them still in place and takes them
/// out afterwards with [`dequote_param`], which is why a token can come back
/// looking like `"a b"`. `None` is upstream's NULL — the end of the line, or an
/// unquoted `;`, which is a comment and ends it early.
fn get_param(param: &[u8]) -> Option<(Vec<u8>, &[u8])> {
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
                push_capped(&mut buf, b'"');
                i += 1;
            }
        }
        push_capped(&mut buf, param[i]);
        i += 1;
    }
    // Upstream drops a trailing `;` here — `if (!quoted && buff[i-1] == ';')`.
    // It cannot fire: a `;` only reaches the buffer while `quoted`, and nothing
    // between that copy and the loop test can clear the flag. Transcribed as a
    // comment rather than as an unreachable branch, and it is also where the
    // function reads `buff[-1]` if it is ever called with a size of 1.
    Some((buf, &param[i..]))
}

fn push_capped(buf: &mut Vec<u8>, b: u8) {
    if buf.len() < MAX_STR_LEN - 1 {
        buf.push(b);
    }
}

/// `DequoteParam` (`ttlib.c:917`) — take the quotes back out.
///
/// A quote toggles the state and vanishes; a `""` *inside* a quoted run is one
/// literal quote. So `"a b"` is `a b`, `""` is the empty string — which
/// `params_array.bat` passes deliberately — and `""""` is a single `"`.
fn dequote_param(src: &[u8]) -> Vec<u8> {
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

/// `GetFileNamePosW` (`ttlib_static_cpp.cpp:768`) — a path split into the
/// directory part, slash included, and the name.
///
/// Both separators count, because both work on Windows and one of them is the
/// only one that works here. A `:` after the first two characters makes the
/// whole path invalid upstream, which leaves `ShortName` empty; that is
/// reproduced, since a macro named `a:b` is a macro upstream refuses to open.
fn split_file_name(path: &[u8]) -> (&[u8], &[u8]) {
    let start = if path.len() >= 2 && path[1] == b':' {
        2
    } else {
        0
    };
    let start = start + usize::from(matches!(path.get(start), Some(b'\\' | b'/')));
    let mut pos = start;
    for (i, &b) in path.iter().enumerate().skip(start) {
        match b {
            b':' => return (b"", b""),
            b'/' | b'\\' => pos = i + 1,
            _ => {}
        }
    }
    path.split_at(pos)
}

/// `FitFileNameW` (`ttlib_static_cpp.cpp:1248`) — a bare filename, made into
/// one Windows will take.
///
/// Two rules, and the second is the one a macro notices: a name that starts
/// with a dot gets an underscore in front of it, and a name with **no dot at
/// all** gets the default extension. So `ttpmacro m` runs `m.TTL` and reports
/// `m.TTL` as `param1`, and `ttpmacro m.` runs `m.` — the test is for a dot
/// anywhere, not for an extension.
fn fit_file_name(name: &[u8], def_ext: &[u8]) -> Vec<u8> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(name.len() + def_ext.len() + 1);
    if name[0] == b'.' {
        out.push(b'_');
    }
    out.extend_from_slice(name);
    if !name.contains(&b'.') {
        out.extend_from_slice(def_ext);
    }
    out
}

/// Upstream counts `wchar_t`; this counts bytes, and backs off a continuation
/// byte so a cut topic is still text. A DDE topic is `TERATERM` and a number,
/// so the difference is theoretical — but half a character is a thing no
/// truncation should produce.
fn truncate(s: &[u8], n: usize) -> Vec<u8> {
    if s.len() <= n {
        return s.to_vec();
    }
    let mut end = n;
    while end > 0 && s[end] & 0xC0 == 0x80 {
        end -= 1;
    }
    s[..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cmd: &CmdLine) -> Vec<String> {
        cmd.args
            .iter()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect()
    }

    fn name(cmd: &CmdLine) -> String {
        String::from_utf8_lossy(&cmd.file_name).into_owned()
    }

    /// The four lines in `macroparam.bat`, and the counts the script itself
    /// asserts on. This is the whole reason the module exists.
    #[test]
    fn macroparam_bat_reads_the_way_its_script_says() {
        let cmd = CmdLine::parse(b"ttpmacro.exe macrofile /vxx /ixx /V /i test1");
        assert_eq!(cmd.param_cnt(), 6);
        assert_eq!(name(&cmd), "macrofile");
        assert_eq!(args(&cmd), ["/vxx", "/ixx", "/V", "/i", "test1"]);
        assert!(!cmd.v_option && !cmd.i_option);

        let cmd = CmdLine::parse(b"ttpmacro.exe /V /i macrofile /v /I test2");
        assert_eq!(cmd.param_cnt(), 4);
        assert_eq!(args(&cmd), ["/v", "/I", "test2"]);
        assert!(cmd.v_option && cmd.i_option);

        let cmd = CmdLine::parse(b"ttpmacro.exe /I macrofile test3 /Vxx /ixx /V /i");
        assert_eq!(cmd.param_cnt(), 6);
        assert_eq!(args(&cmd), ["test3", "/Vxx", "/ixx", "/V", "/i"]);
        assert!(!cmd.v_option && cmd.i_option);

        let cmd = CmdLine::parse(b"ttpmacro.exe /i macrofile test4 /V /Vxx /ixx");
        assert_eq!(cmd.param_cnt(), 5);
        assert_eq!(args(&cmd), ["test4", "/V", "/Vxx", "/ixx"]);
    }

    /// `params_array.bat`, which adds a quoted argument with a space in it, an
    /// empty one and a numeric one.
    #[test]
    fn params_array_bat_keeps_the_space_and_the_empty_string() {
        let cmd = CmdLine::parse(
            br#"ttpmacro.exe "params_array.ttl" /vxx /ixx /V /i test1 "param 7" "" param9 10 eleven"#,
        );
        assert_eq!(cmd.param_cnt(), 11);
        assert_eq!(name(&cmd), "params_array.ttl");
        assert_eq!(
            args(&cmd),
            ["/vxx", "/ixx", "/V", "/i", "test1", "param 7", "", "param9", "10", "eleven"]
        );
    }

    /// A switch after the filename is a parameter; the same word before it is
    /// the switch. Case does not matter, but position does.
    #[test]
    fn a_switch_is_only_a_switch_before_the_filename() {
        let cmd = CmdLine::parse(b"tt /s /d=topic m.ttl /s /d=other");
        assert!(cmd.sleep);
        assert_eq!(String::from_utf8_lossy(&cmd.topic), "topic");
        assert_eq!(args(&cmd), ["/s", "/d=other"]);
        // `/D=` keeps ten characters of its topic and drops the rest.
        let cmd = CmdLine::parse(b"tt /D=0123456789abc m.ttl");
        assert_eq!(String::from_utf8_lossy(&cmd.topic), "0123456789");
    }

    /// A near miss is not a switch: `/vxx` is a parameter, and so is `/V=1`.
    #[test]
    fn only_the_exact_spellings_are_switches() {
        let cmd = CmdLine::parse(b"tt /vxx /V=1 m.ttl");
        assert!(!cmd.v_option);
        assert_eq!(name(&cmd), "/vxx");
        assert_eq!(args(&cmd), ["/V=1", "m.ttl"]);
    }

    #[test]
    fn the_tokeniser_is_not_the_c_runtimes() {
        // A backslash is an ordinary character, which it has to be.
        let cmd = CmdLine::parse(br"tt c:\dir\m.ttl");
        assert_eq!(name(&cmd), r"c:\dir\m.ttl");
        // An unquoted `;` ends the line, and everything after it is lost.
        let cmd = CmdLine::parse(b"tt m.ttl a ; b c");
        assert_eq!(args(&cmd), ["a"]);
        // A quoted one is just a character.
        let cmd = CmdLine::parse(br#"tt m.ttl "a;b" c"#);
        assert_eq!(args(&cmd), ["a;b", "c"]);
        // Tabs separate, and a run of separators is one.
        let cmd = CmdLine::parse(b"tt \t m.ttl \t\t a  b");
        assert_eq!(args(&cmd), ["a", "b"]);
    }

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
        let (tok, rest) = get_param(br#""a b" c"#).unwrap();
        assert_eq!(tok, br#""a b""#);
        assert_eq!(rest, b" c");
        assert_eq!(dequote_param(&tok), b"a b");
        // An unterminated quote runs to the end of the line.
        let (tok, rest) = get_param(br#""a b c"#).unwrap();
        assert_eq!(tok, br#""a b c"#);
        assert_eq!(rest, b"");
    }

    #[test]
    fn a_token_stops_at_511_characters() {
        let long = vec![b'x'; 600];
        let line = [b"tt m.ttl ".as_slice(), &long].concat();
        let cmd = CmdLine::parse(&line);
        assert_eq!(cmd.args[0].len(), MAX_STR_LEN - 1);
    }

    #[test]
    fn nothing_at_all_is_a_prompt_and_a_count_of_zero() {
        let cmd = CmdLine::parse(b"ttpmacro.exe");
        assert_eq!(cmd.param_cnt(), 0);
        assert!(cmd.needs_prompt());
        assert_eq!(cmd.raw, b"ttpmacro.exe");
        // A `*` is the other way of asking for the dialog.
        assert!(CmdLine::parse(b"tt *").needs_prompt());
        assert!(!CmdLine::parse(b"tt m.ttl").needs_prompt());
        // Switches alone still leave nothing to run.
        let cmd = CmdLine::parse(b"tt /V /S");
        assert_eq!(cmd.param_cnt(), 0);
        assert!(cmd.needs_prompt());
    }

    #[test]
    fn the_default_extension_goes_on_a_name_with_no_dot() {
        let cmd = CmdLine::parse(b"tt m");
        assert_eq!(cmd.short_name(), b"m.TTL");
        assert_eq!(cmd.fitted_file_name(), b"m.TTL");
        // A dot anywhere is enough, and it need not be an extension.
        assert_eq!(CmdLine::parse(b"tt m.ttl").short_name(), b"m.ttl");
        assert_eq!(CmdLine::parse(b"tt m.").short_name(), b"m.");
        assert_eq!(CmdLine::parse(b"tt a.b.c").short_name(), b"a.b.c");
        // A leading dot is illegal, so an underscore goes in front of it.
        assert_eq!(CmdLine::parse(b"tt .ttl").short_name(), b"_.ttl");
    }

    #[test]
    fn short_name_drops_the_directory_and_fitted_keeps_it() {
        let cmd = CmdLine::parse(br"tt c:\dir\m");
        assert_eq!(cmd.short_name(), b"m.TTL");
        assert_eq!(cmd.fitted_file_name(), br"c:\dir\m.TTL");
        let cmd = CmdLine::parse(b"tt /home/nata/m.ttl");
        assert_eq!(cmd.short_name(), b"m.ttl");
        assert_eq!(cmd.fitted_file_name(), b"/home/nata/m.ttl");
        // A colon at position 1 is a drive letter, whatever the letter is.
        assert_eq!(CmdLine::parse(b"tt a:b").short_name(), b"b.TTL");
        // Anywhere else it makes the path invalid, and upstream then has no
        // name at all to report.
        assert_eq!(CmdLine::parse(b"tt ab:c").short_name(), b"");
    }

    /// The Unix half: the shell has split and unquoted already, so a quote that
    /// survived into `argv` is one the user meant to keep.
    #[test]
    fn from_args_does_not_unquote_a_second_time() {
        let cmd = CmdLine::from_args([
            "/V",
            "m.ttl",
            "/V",
            "param 7",
            "",
            r#""quoted""#,
            "a;b",
        ]);
        assert!(cmd.v_option);
        assert_eq!(name(&cmd), "m.ttl");
        assert_eq!(cmd.param_cnt(), 6);
        assert_eq!(args(&cmd), ["/V", "param 7", "", r#""quoted""#, "a;b"]);
        // `params[0]` is the arguments joined, which is all `execve` left.
        assert_eq!(cmd.raw, br#"/V m.ttl /V param 7  "quoted" a;b"#);
    }

    #[test]
    fn both_entry_points_agree_when_nothing_needs_quoting() {
        let split = CmdLine::from_args(["/V", "/i", "m.ttl", "/v", "/I", "test2"]);
        let whole = CmdLine::parse(b"ttpmacro.exe /V /i m.ttl /v /I test2");
        assert_eq!(split.args, whole.args);
        assert_eq!(split.file_name, whole.file_name);
        assert_eq!(split.param_cnt(), whole.param_cnt());
        assert_eq!((split.v_option, split.i_option), (true, true));
    }
}
