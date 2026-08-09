//! The third command line, which belongs to a program that is not Tera Term.
//!
//! `cygconnect`'s argument is **CygTerm's** command line, not Tera Term's:
//! `ttl.cpp:73` spells the launcher `cyglaunch -o`, and `TTLConnect` builds
//! `cyglaunch -o /D=<topic> <the macro's string>` (`:635`). `cyglaunch` joins
//! its own arguments back into one line (`cyglaunch.c:83`) and hands it to
//! `cygterm.exe`, whose `get_args` (`cygterm.cpp:317`) is the loop reproduced
//! here. So the options a macro writes are read by a Cygwin program that spawns
//! a shell on a pty — which is what [`tt_conn::pty`] is, and why the mapping
//! onto it is a mapping rather than an invention.
//!
//! Three of the ten options describe a *terminal emulator to launch* rather
//! than the shell — `-t`, `-p`, `-o` — and there is nothing to launch here: the
//! terminal is the caller. They are parsed and kept so that a line carrying them
//! is not misread, and the consumer ignores them.
//!
//! Two things about the tokenising are worth saying out loud, and they are not
//! the same tokenising:
//!
//! - **Nothing is discarded from the front.** `_ParseParam` throws its first
//!   token away and `super::CommandLine::parse_argument` puts a dummy there to
//!   feed it; this is the mirror image, because upstream's `argv[0]` is
//!   `cygterm.exe` itself and every token the macro wrote comes after it.
//! - **The line and the `-s` string are split by different rules, upstream as
//!   well as here.** The line is split by cygwin's C runtime, which cannot be
//!   reached from this side and which leaves a backslash alone — that is what
//!   makes the manual's own `-d C:\ -nocd -nols` example three options rather
//!   than two ([`split_line`]). The shell string is split by [`get_argv`], the
//!   function in `cygterm.cpp` itself, where a backslash **is** an escape.
//!   Using one for both looks tidier and gets the documented example wrong.

use crate::services::scanf_int;

/// What a CygTerm command line asked for.
///
/// The defaults are the ones a CygTerm with **no configuration file** starts
/// from, which is the state this port is always in: `cygterm.cfg` ships
/// `LOGIN_SHELL = Yes` and no `HOME_CHDIR` at all, and the structure is zeroed
/// before either is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CygTerm {
    /// `-s 'shell'` — the command line of the shell to run, unsplit. `None` is
    /// upstream's `AUTO`, which is also what `-s AUTO` means explicitly: the
    /// account's own shell out of `/etc/passwd` (`cygterm.cpp:238`).
    pub shell: Option<Vec<u8>>,
    /// `-ls` / `-nols` / `+ls`. On by default.
    pub login_shell: bool,
    /// `-cd` / `-nocd` / `+cd` — start in the user's home. Off by default, so
    /// a shell inherits the launcher's directory unless it is asked not to.
    pub home_chdir: bool,
    /// `-d 'directory'`. Setting it clears [`home_chdir`](CygTerm::home_chdir),
    /// which is `cygterm.cpp:1336` and not the option loop.
    pub change_dir: Option<Vec<u8>>,
    /// `-v 'NAME=value'`, in the order given, with a repeated name replacing
    /// its earlier value.
    pub env: Vec<(Vec<u8>, Vec<u8>)>,
    /// `-dumb` — `$TERM` for the shell, which the option sets to `dumb`.
    /// `None` leaves it to whoever opens the pty.
    pub term_type: Option<Vec<u8>>,
    /// `-A` / `-a` — CygTerm's ssh-agent proxy, which is a Pageant bridge and
    /// has no meaning against a real `ssh-agent`. Parsed, never acted on.
    pub agent_proxy: bool,
    /// `-t 'terminal-emulator'` — the command line CygTerm would have launched.
    /// Nothing to launch here.
    pub terminal: Option<Vec<u8>>,
    /// `-p <port>` — connect to a listener on localhost instead of launching a
    /// terminal. A whole second transport, and not one a macro can ask this
    /// port for.
    pub port: Option<i32>,
    /// `-o 'option'` — appended to the terminal emulator's command line, which
    /// is how the DDE topic reaches `ttermpro`.
    pub term_option: Option<Vec<u8>>,
    /// `-debug`.
    pub debug: bool,
}

impl Default for CygTerm {
    fn default() -> CygTerm {
        CygTerm {
            shell: None,
            login_shell: true,
            home_chdir: false,
            change_dir: None,
            env: Vec::new(),
            term_type: None,
            agent_proxy: false,
            terminal: None,
            port: None,
            term_option: None,
            debug: false,
        }
    }
}

/// `get_args` (`cygterm.cpp:317`) over a whole line.
///
/// Every arm is upstream's, including the three `+` spellings that are not in
/// the documentation and the one case-insensitive comparison (`AUTO`). An
/// option whose value is missing **ends the parse** where upstream's `break`
/// does — `-t`, `-s`, `-d` and `-o` all test `*++argv == NULL` — while `-p` and
/// `-v` look ahead instead and simply carry on.
pub fn parse(line: &[u8]) -> CygTerm {
    parse_args(&split_line(line))
}

/// The same over arguments something else has already split.
pub fn parse_args(args: &[Vec<u8>]) -> CygTerm {
    let mut cfg = CygTerm::default();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        i += 1;
        // `strcmp`, so every one of these is case-sensitive.
        match arg.as_slice() {
            b"-t" => match args.get(i) {
                Some(v) => {
                    i += 1;
                    cfg.terminal = Some(v.clone());
                }
                None => break,
            },
            b"-p" => {
                if let Some(v) = args.get(i) {
                    i += 1;
                    // `atoi`, which is 0 for anything that is not a number —
                    // and `cl_port > 0` is the test that uses it, so a 0 is
                    // the same as no option at all.
                    cfg.port = Some(scanf_int(v).unwrap_or(0));
                }
            }
            b"-dumb" => cfg.term_type = Some(b"dumb".to_vec()),
            b"-s" => match args.get(i) {
                Some(v) => {
                    i += 1;
                    // `strcasecmp(*argv, "AUTO")` — the one comparison in the
                    // loop that is not case-sensitive, and it means "leave the
                    // shell alone" rather than naming one.
                    if !v.eq_ignore_ascii_case(b"AUTO") {
                        cfg.shell = Some(v.clone());
                    }
                }
                None => break,
            },
            b"-cd" => cfg.home_chdir = true,
            b"-nocd" | b"+cd" => cfg.home_chdir = false,
            b"-ls" => cfg.login_shell = true,
            b"-nols" | b"+ls" => cfg.login_shell = false,
            b"-A" => cfg.agent_proxy = true,
            b"-a" => cfg.agent_proxy = false,
            b"-v" => {
                if let Some(v) = args.get(i) {
                    i += 1;
                    add_env(&mut cfg.env, v);
                }
            }
            b"-d" => match args.get(i) {
                Some(v) => {
                    i += 1;
                    cfg.change_dir = Some(quote_cut(v, CHANGE_DIR_LEN));
                }
                None => break,
            },
            b"-o" => match args.get(i) {
                Some(v) => {
                    i += 1;
                    cfg.term_option = Some(v.clone());
                }
                None => break,
            },
            b"-debug" => cfg.debug = true,
            // Anything else is skipped in silence, including a bare word: the
            // loop has no `else`, so `cygconnect 'myhost'` asks for nothing at
            // all rather than failing.
            _ => {}
        }
    }
    // `cygterm.cpp:1336`, which runs after the option loop rather than in it:
    // an explicit directory outranks `-cd` whichever order they were given in.
    // The `CHERE_INVOKING=y` it also sets is Cygwin's own `/etc/profile`
    // convention and means nothing to a Linux login shell, so it is not
    // carried over.
    if cfg.change_dir.is_some() {
        cfg.home_chdir = false;
    }
    cfg
}

/// `char change_dir[256]` (`cygterm.cpp:379`), which `quote_cut` truncates to.
const CHANGE_DIR_LEN: usize = 256;

/// `quote_cut` (`cygterm.cpp:304`) — copy without the quotes.
///
/// **Every** `"` is dropped, not a matched pair: `a"b` is `ab` and `"a b"` is
/// `a b`. That is the opposite of `GetPrivateProfileString`'s matched-pair rule
/// and of `DequoteParam`'s, so the three quoting rules in this crate disagree
/// with one another — which is what happens when three programs each write
/// their own.
fn quote_cut(src: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    for &b in src {
        if out.len() + 1 >= len {
            break;
        }
        if b != b'"' {
            out.push(b);
        }
    }
    out
}

/// `env_add1` / `env_add` (`cygterm_cfg.cpp:92`, `:42`).
///
/// An empty argument and an empty name are both ignored, and a repeated name
/// replaces the value it had. Two things upstream does here are **not**
/// reproduced, and both are defects rather than behaviour:
///
/// - A name with **no `=`** gives `env_add` a NULL value and it calls
///   `strdup(NULL)`. `cygconnect '-v FOO'` is a macro reaching a null
///   dereference in a program it launched; here it is ignored.
/// - Replacing the **first** variable drops every variable after it —
///   `pr_data->envp = e` with `e->next` still NULL (`:67`), so `-v A=1 -v B=2
///   -v A=3` loses `B`. The list is kept whole here.
fn add_env(env: &mut Vec<(Vec<u8>, Vec<u8>)>, name_value: &[u8]) {
    if name_value.is_empty() {
        return;
    }
    let Some(eq) = name_value.iter().position(|&b| b == b'=') else {
        return;
    };
    let (name, value) = (&name_value[..eq], &name_value[eq + 1..]);
    if name.is_empty() {
        return;
    }
    if let Some(slot) = env.iter_mut().find(|(n, _)| n == name) {
        slot.1 = value.to_vec();
        return;
    }
    env.push((name.to_vec(), value.to_vec()));
}

/// The command line into arguments, the way the runtime under `cygterm.exe`
/// does it.
///
/// **Not [`get_argv`].** The line reaches `main` through `CreateProcess` and
/// cygwin's own `build_argv`, so the rules are Windows-shaped: a backslash is
/// an ordinary character, which it has to be, since the option that carries a
/// path is `-d` and the manual's example for it is `-d C:\`. Both quote
/// characters are honoured — cygwin handles `'` as well as `"` for a command
/// line that came from a program which is not itself cygwin, and every example
/// in the CygTerm documentation is written with single quotes.
///
/// It is a reproduction of a runtime rather than of a function in the tree, so
/// it is the loosest thing in this module; the two places it could be wrong —
/// a backslash before a quote, and an unbalanced quote — are the places nobody
/// writes on purpose.
pub fn split_line(line: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let (mut sq, mut dq) = (false, false);
    let mut i = 0;
    loop {
        while matches!(line.get(i), Some(b) if is_space(*b)) {
            i += 1;
        }
        if i >= line.len() {
            return out;
        }
        let mut tok = Vec::new();
        while let Some(&c) = line.get(i) {
            i += 1;
            if is_space(c) && !sq && !dq {
                break;
            }
            match c {
                b'\'' if !dq => sq = !sq,
                b'"' if !sq => dq = !dq,
                _ => tok.push(c),
            }
        }
        out.push(tok);
    }
}

/// `get_argv` (`cygterm.cpp:869`) — a string into arguments.
///
/// A quote toggles and vanishes, a backslash escapes the next character
/// whatever it is, and an unquoted run of whitespace separates. Unlike
/// [`super::get_param`] a backslash is *not* an ordinary character, which is
/// the right way round for a program whose paths use forward slashes.
///
/// `max` is upstream's `maxc` and bounds the count at `max - 1`; the shell
/// string is split with 32. Nothing is reported when it overflows — the comment
/// in the function says so ("not to judge syntax errors") — and an unclosed
/// quote is not an error either.
pub fn get_argv(s: &[u8], max: usize) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    // Declared outside the token loop upstream, so an unclosed quote or a
    // trailing backslash carries into the next token.
    let (mut esc, mut sq, mut dq) = (false, false, false);
    let mut i = 0;
    while out.len() + 1 < max {
        while matches!(s.get(i), Some(b) if is_space(*b)) {
            i += 1;
        }
        if i >= s.len() {
            break;
        }
        let mut tok = Vec::new();
        while let Some(&c) = s.get(i) {
            i += 1;
            if is_space(c) && !esc && !sq && !dq {
                break;
            }
            if c == b'\'' && !esc && !dq {
                sq = !sq;
            } else if c == b'"' && !esc && !sq {
                dq = !dq;
            } else if c == b'\\' && !esc {
                esc = true;
            } else {
                esc = false;
                tok.push(c);
            }
        }
        // Pushed even when it is empty, which `''` produces: upstream writes
        // the terminator and moves on to the next slot.
        out.push(tok);
    }
    out
}

/// `isascii(c) && isspace(c)` — the C set, which includes the vertical tab that
/// `u8::is_ascii_whitespace` leaves out.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        strings(get_argv(s.as_bytes(), 32))
    }

    fn line(s: &str) -> Vec<String> {
        strings(split_line(s.as_bytes()))
    }

    fn strings(v: Vec<Vec<u8>>) -> Vec<String> {
        v.into_iter()
            .map(|t| String::from_utf8_lossy(&t).into_owned())
            .collect()
    }

    #[test]
    fn a_line_splits_on_whitespace_and_the_quotes_come_off() {
        assert_eq!(argv("  -nols  -d /tmp "), ["-nols", "-d", "/tmp"]);
        assert_eq!(argv("-s '/bin/sh -l'"), ["-s", "/bin/sh -l"]);
        assert_eq!(argv(r#"-s "a b" c"#), ["-s", "a b", "c"]);
        // A quote in the middle of a word, which is where the two quoting
        // rules in this crate differ: this one takes it out.
        assert_eq!(argv("a'b'c"), ["abc"]);
    }

    /// The line's splitter agrees with `get_argv` about quotes and disagrees
    /// about the backslash, which is the whole reason there are two.
    #[test]
    fn the_line_keeps_its_backslashes() {
        assert_eq!(line(r"-d C:\ -nocd"), ["-d", r"C:\", "-nocd"]);
        assert_eq!(argv(r"-d C:\ -nocd"), ["-d", "C: -nocd"]);
        assert_eq!(line("-s '/bin/sh -l'"), ["-s", "/bin/sh -l"]);
        assert_eq!(line(r#"-d "/a b""#), ["-d", "/a b"]);
        // A quote inside the other kind is an ordinary character.
        assert_eq!(line(r#"'"x"'"#), [r#""x""#]);
    }

    /// A backslash escapes anything, including a quote and a space — the one
    /// place this tokeniser and Tera Term's disagree completely.
    #[test]
    fn a_backslash_escapes_the_next_character() {
        assert_eq!(argv(r"a\ b"), ["a b"]);
        assert_eq!(argv(r"\'a\'"), ["'a'"]);
        assert_eq!(argv(r"a\\b"), [r"a\b"]);
    }

    /// The count is bounded and the overflow is silent.
    #[test]
    fn the_argument_count_is_capped_at_one_below_the_limit() {
        let line = "a b c d e";
        assert_eq!(get_argv(line.as_bytes(), 3).len(), 2);
        assert_eq!(get_argv(line.as_bytes(), 32).len(), 5);
    }

    /// An empty token is a token: `''` is a shell string of nothing, and
    /// upstream stores it rather than skipping it.
    #[test]
    fn an_empty_quoted_run_is_an_argument() {
        assert_eq!(argv("-s '' -nols"), ["-s", "", "-nols"]);
    }

    #[test]
    fn nothing_asked_for_is_a_login_shell_in_the_current_directory() {
        let c = parse(b"");
        assert_eq!(c, CygTerm::default());
        assert!(c.login_shell, "cygterm.cfg ships LOGIN_SHELL = Yes");
        assert!(!c.home_chdir, "and no HOME_CHDIR at all");
        assert_eq!(c.shell, None);
    }

    /// The documentation's own example, which is the case that decides which
    /// splitter the line gets.
    #[test]
    fn the_documented_example_parses() {
        let c = parse(br"-d C:\ -nocd -nols");
        assert_eq!(c.change_dir.as_deref(), Some(&br"C:\"[..]));
        assert!(!c.login_shell);
        assert!(!c.home_chdir);
    }

    #[test]
    fn the_shell_is_a_string_and_auto_is_not_one() {
        assert_eq!(
            parse(b"-s /bin/dash").shell.as_deref(),
            Some(&b"/bin/dash"[..])
        );
        assert_eq!(parse(b"-s 'AUTO'").shell, None, "AUTO leaves it alone");
        assert_eq!(parse(b"-s auto").shell, None, "and the test is case-blind");
        assert_eq!(
            parse(b"-s '/bin/sh -c ls'").shell.as_deref(),
            Some(&b"/bin/sh -c ls"[..]),
            "kept whole; splitting it is the consumer's"
        );
    }

    /// `-cd` and `-nocd` are last-wins, and `-d` outranks both wherever it is.
    #[test]
    fn a_directory_outranks_the_home_flag_in_either_order() {
        assert!(parse(b"-nocd -cd").home_chdir);
        assert!(!parse(b"-cd -nocd").home_chdir);
        assert!(!parse(b"-cd -d /tmp").home_chdir);
        let c = parse(b"-d /tmp -cd");
        assert!(!c.home_chdir, "the clear happens after the loop");
        assert_eq!(c.change_dir.as_deref(), Some(&b"/tmp"[..]));
    }

    /// The undocumented `+` spellings, which are in the code and not in the
    /// manual.
    #[test]
    fn plus_means_off() {
        assert!(!parse(b"+ls").login_shell);
        assert!(!parse(b"-cd +cd").home_chdir);
    }

    #[test]
    fn the_environment_keeps_its_order_and_replaces_by_name() {
        let c = parse(b"-v A=1 -v B=2 -v A=3");
        assert_eq!(
            c.env,
            [
                (b"A".to_vec(), b"3".to_vec()),
                (b"B".to_vec(), b"2".to_vec())
            ],
            "B survives, which upstream's own list does not"
        );
        assert_eq!(parse(b"-v A=").env, [(b"A".to_vec(), Vec::new())]);
    }

    /// The two `-v` arguments that reach nothing: upstream dereferences NULL
    /// for the first and ignores the second.
    #[test]
    fn a_variable_with_no_value_and_one_with_no_name_are_both_ignored() {
        assert!(parse(b"-v FOO").env.is_empty());
        assert!(parse(b"-v =x").env.is_empty());
        assert!(parse(b"-v ''").env.is_empty());
    }

    #[test]
    fn dumb_is_a_terminal_type_and_the_rest_are_kept_unused() {
        assert_eq!(parse(b"-dumb").term_type.as_deref(), Some(&b"dumb"[..]));
        let c = parse(b"-t 'ttermpro.exe %s %d' -p 20000 -o /D=1234 -debug -A");
        assert_eq!(c.terminal.as_deref(), Some(&b"ttermpro.exe %s %d"[..]));
        assert_eq!(c.port, Some(20000));
        assert_eq!(c.term_option.as_deref(), Some(&b"/D=1234"[..]));
        assert!(c.debug);
        assert!(c.agent_proxy);
    }

    /// An option that takes a value takes **whatever is next**, flag-shaped or
    /// not — there is no "that looks like another option" test anywhere in the
    /// loop, so `-s -nols` names a shell called `-nols` and loses the flag.
    ///
    /// Upstream's four `break`s on a missing value are transcribed and are not
    /// observable: an option with nothing after it is the last token, and the
    /// loop was ending anyway.
    #[test]
    fn an_option_that_wants_a_value_eats_the_next_token_whatever_it_is() {
        assert_eq!(parse(b"-s -nols").shell.as_deref(), Some(&b"-nols"[..]));
        assert!(
            parse(b"-s -nols").login_shell,
            "the flag was eaten as a name"
        );
        assert!(parse(b"-p -nols").login_shell);
        assert!(parse(b"-v -nols").login_shell);
        // And a trailing one is simply nothing, with what came before it kept.
        assert!(!parse(b"-nols -s").login_shell);
        assert!(!parse(b"-nols -v").login_shell);
    }

    /// Case-sensitive, and no complaint about a word it does not know.
    #[test]
    fn an_unknown_word_is_skipped_in_silence() {
        assert_eq!(parse(b"-NOLS myhost -zz"), CygTerm::default());
    }

    /// Every quote goes, not a matched pair.
    #[test]
    fn a_directory_loses_every_quote_it_has() {
        // The tokeniser takes the outer pair off, and `quote_cut` takes the
        // doubled inner ones. Both run, in that order.
        assert_eq!(
            parse(br#"-d '"/a b"'"#).change_dir.as_deref(),
            Some(&b"/a b"[..])
        );
    }
}
