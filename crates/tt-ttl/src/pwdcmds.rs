//! The eight password commands.
//!
//! Four operations — set, get, is, del — over two storage formats, which is
//! why every name comes in a plain and a `2` variant. The formats themselves
//! are in [`crate::pwd`]; what is here is the argument lists and the four
//! different opinions these commands have about reporting failure.
//!
//! **They do not agree about anything.** `setpassword` rejects an empty
//! argument with a syntax error and `getpassword` returns quietly from the
//! same condition without even writing `result`. `delpassword` never writes
//! `result` at all, so a script cannot find out whether it deleted anything.
//! `getpassword` treats the Close button as "end the macro" and `getpassword2`
//! discards the dialog's answer entirely. All four are reproduced; a macro that
//! tests `result` after `delpassword` is reading whatever the previous command
//! left there, and it has always been.
//!
//! **The v1 store is not secure and this port does not pretend otherwise.**
//! `setpassword` takes no key, and `getpassword` needs none either — see
//! [`crate::pwd`]. It is here so that a `password.dat` written by a real Tera
//! Term still opens. New scripts want the `2` commands.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::{DialogEnd, ScriptHost};
use crate::interp::Interp;
use crate::pwd;
use crate::rsv::Rsv;

use std::path::Path;

/// The INI section both v1 commands read and write (`ttl.cpp:2615`).
const SECTION: &str = "Password";

/// `OpenInpDlg`'s caption for the prompt `getpassword` puts up when the file
/// has no entry yet. English in the source, and not translated: `ttl_gui.cpp`
/// passes the literal.
const PROMPT_TITLE: &[u8] = b"Enter password";

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn password_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::SetPassword => self.cmd_set_password(host),
            Rsv::SetPassword2 => self.cmd_set_password2(),
            Rsv::GetPassword => self.cmd_get_password(host),
            Rsv::GetPassword2 => self.cmd_get_password2(host),
            Rsv::IsPassword => self.cmd_is_password(),
            Rsv::IsPassword2 => self.cmd_is_password2(),
            Rsv::DelPassword => self.cmd_del_password(),
            Rsv::DelPassword2 => self.cmd_del_password2(),
            _ => return None,
        })
    }

    /// `setpassword <filename> <password name> <strval>` → `result` 1 on
    /// success.
    ///
    /// **An empty password is refused**, and upstream's comment says why: it is
    /// refused for the same reason `getpassword` refuses one, which is that an
    /// empty entry is how the file says "no password here". So there would be
    /// no way to tell a stored empty password from an absent one.
    fn cmd_set_password(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let (file, key, pass) = self.three_strings()?;
        if file.is_empty() || key.is_empty() || pass.is_empty() {
            return Err(TtlError::Syntax);
        }
        let Some(path) = self.files.abs_path(&file) else {
            self.set_result(0);
            return Ok(());
        };
        let enc = pwd::v1_encrypt(&pass, || host.random_u32());
        let ok = write_ini(&path, &key, Some(&enc));
        self.set_result(ok as i32);
        Ok(())
    }

    /// `setpassword2 <filename> <password name> <strval> <encryptstr>` →
    /// `result` 1 on success.
    ///
    /// The fourth argument is the key the password is actually encrypted
    /// under, and nothing stores it: a macro that loses it has lost the
    /// password. That is the trade the `2` commands make.
    fn cmd_set_password2(&mut self) -> TtlResult<()> {
        let (file, key, pass) = self.three_strings()?;
        let secret = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if file.is_empty() || key.is_empty() || pass.is_empty() || secret.is_empty() {
            return Err(TtlError::Syntax);
        }
        let ok = self
            .files
            .abs_path(&file)
            .is_some_and(|p| pwd::v2_set(&p, &key, &pass, &secret));
        self.set_result(ok as i32);
        Ok(())
    }

    /// `getpassword <filename> <password name> <strvar>` → the password in
    /// `<strvar>`, and `result` 1 if there is one.
    ///
    /// **When the file has no entry the command asks the user for one** and
    /// stores what they type, so a macro's first run is a prompt and every run
    /// after it is silent. Escape answers with an empty string — the buffer is
    /// initialised here, unlike `inputbox`'s — and the window's Close button
    /// **ends the macro**, which is the one place a dialog does that.
    ///
    /// **A record can be silently unreadable, and this is upstream's bug.** The
    /// obfuscated form is printable ASCII including `'` and `"`, and it goes
    /// into an INI file — where `GetPrivateProfileString` strips one matched
    /// pair of surrounding quotes. So roughly one record in four thousand comes
    /// back two characters short, fails the complement check inside
    /// [`v1_decrypt`](crate::pwd::v1_decrypt), and yields the **empty password
    /// with `result` 1**: success, and nothing in the variable. Reproduced,
    /// because the alternative is a terminal that reads files a real Tera Term
    /// cannot and writes files it then misreads.
    ///
    /// Two paths return without writing `result` at all — an empty filename and
    /// an empty key name — so a script that tests it is reading the previous
    /// command's answer.
    fn cmd_get_password(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let file = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let key = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        self.vars.set_str(var, b"");
        if file.is_empty() || key.is_empty() {
            return Ok(());
        }
        let Some(path) = self.files.abs_path(&file) else {
            return Ok(());
        };

        let stored = read_ini(&path, &key);
        let (password, ok) = if stored.is_empty() {
            let typed = match host.input_box(&key, PROMPT_TITLE, b"", true)? {
                DialogEnd::Ok(s) => s,
                DialogEnd::Cancel => Vec::new(),
                DialogEnd::Closed => {
                    // `TTLStatus = IdTTLEnd`, and `result` is left alone.
                    self.ended = true;
                    return Ok(());
                }
            };
            if typed.is_empty() {
                (Vec::new(), false)
            } else {
                let enc = pwd::v1_encrypt(&typed, || host.random_u32());
                let ok = write_ini(&path, &key, Some(&enc));
                (typed, ok)
            }
        } else {
            // `result` is 1 whatever `Decrypt` made of it, including nothing.
            (pwd::v1_decrypt(&stored), true)
        };

        if ok {
            self.vars.set_str(var, &password);
        }
        self.set_result(ok as i32);
        Ok(())
    }

    /// `getpassword2 <filename> <password name> <strvar> <encryptstr>`.
    ///
    /// The same shape as `getpassword` with two differences, both upstream's.
    /// The Close button is **not** handled — `OpenInpDlg`'s return value is
    /// discarded (`ttl_gui.cpp:293`), so closing the window is an empty
    /// password and the macro carries on. And a wrong `<encryptstr>` for an
    /// entry that *does* exist is `result` 0 with the variable left empty,
    /// rather than a prompt: the entry is found by its key hash, which the
    /// `<encryptstr>` has no part in.
    ///
    /// **Upstream writes through an uninitialised variable id here.**
    /// `SetStrVal(PassStr, "")` sits between `GetStrVar` and the error check
    /// (`ttl_gui.cpp:272`), and `GetStrVar` returns without touching its
    /// out-parameter when an earlier argument already failed — so
    /// `getpassword2 1 2 3 4` reaches `SetStrVal` with a stack value and
    /// `free()`s whatever pointer it finds there. Not reproducible and not
    /// worth reproducing; the clear happens after the arguments parse.
    fn cmd_get_password2(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let file = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let key = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let secret = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        self.vars.set_str(var, b"");
        if file.is_empty() || key.is_empty() || secret.is_empty() {
            return Err(TtlError::Syntax);
        }
        let Some(path) = self.files.abs_path(&file) else {
            self.set_result(0);
            return Ok(());
        };

        let found = if pwd::v2_is(&path, &key) {
            pwd::v2_get(&path, &key, &secret)
        } else {
            let typed = match host.input_box(&key, PROMPT_TITLE, b"", true)? {
                DialogEnd::Ok(s) => s,
                // Closing the window is not distinguished from an empty entry.
                DialogEnd::Cancel | DialogEnd::Closed => Vec::new(),
            };
            (!typed.is_empty() && pwd::v2_set(&path, &key, &typed, &secret)).then_some(typed)
        };

        if let Some(password) = &found {
            self.vars.set_str(var, password);
        }
        self.set_result(found.is_some() as i32);
        Ok(())
    }

    /// `ispassword <filename> <password name>` → `result` 1 if the file has an
    /// entry under that name.
    ///
    /// The test is on the *value*, not the key: `GetPrivateProfileString`
    /// answers an empty string both for a key that is absent and for one
    /// written as `name=`, and upstream compares against empty. So an entry
    /// with nothing after the `=` reads as no entry, which is the same reason
    /// `setpassword` refuses to store an empty password.
    fn cmd_is_password(&mut self) -> TtlResult<()> {
        let (file, key) = self.two_strings()?;
        if file.is_empty() || key.is_empty() {
            return Err(TtlError::Syntax);
        }
        let found = self
            .files
            .abs_path(&file)
            .is_some_and(|p| !read_ini(&p, &key).is_empty());
        self.set_result(found as i32);
        Ok(())
    }

    /// `ispassword2 <filename> <password name>`.
    ///
    /// No `<encryptstr>`: the key name is stored as a PBKDF2 hash of itself,
    /// which can be tested without being able to read anything.
    fn cmd_is_password2(&mut self) -> TtlResult<()> {
        let (file, key) = self.two_strings()?;
        if file.is_empty() || key.is_empty() {
            return Err(TtlError::Syntax);
        }
        let found = self
            .files
            .abs_path(&file)
            .is_some_and(|p| pwd::v2_is(&p, &key));
        self.set_result(found as i32);
        Ok(())
    }

    /// `delpassword <filename> <password name>`. An **empty** name deletes
    /// every v1 entry, which is `WritePrivateProfileString` with a NULL key —
    /// it removes the whole `[Password]` section.
    ///
    /// Both arguments are mandatory to the parser even though the second may be
    /// blank, and **nothing is written to `result`** on any path, success
    /// included. A missing file is not an error either: upstream tests for it
    /// and returns.
    fn cmd_del_password(&mut self) -> TtlResult<()> {
        let (file, key) = self.two_strings()?;
        if file.is_empty() {
            return Ok(());
        }
        let Some(path) = self.files.abs_path(&file) else {
            return Ok(());
        };
        if !path.is_file() {
            return Ok(());
        }
        write_ini(&path, &key, None);
        Ok(())
    }

    /// `delpassword2 <filename> <password name>`, with the same silence about
    /// what happened and one difference: an empty filename is a **syntax
    /// error** here where v1 returns quietly.
    fn cmd_del_password2(&mut self) -> TtlResult<()> {
        let (file, key) = self.two_strings()?;
        if file.is_empty() {
            return Err(TtlError::Syntax);
        }
        if let Some(p) = self.files.abs_path(&file) {
            pwd::v2_del(&p, &key);
        }
        Ok(())
    }

    /// Two strings and the end of the line, which is six of the eight.
    fn two_strings(&mut self) -> TtlResult<(Vec<u8>, Vec<u8>)> {
        let a = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let b = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        Ok((a, b))
    }

    /// Three strings. `setpassword2` reads a fourth before checking the line
    /// has ended, so this one does not check.
    fn three_strings(&mut self) -> TtlResult<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let a = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let b = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let c = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        Ok((a, b, c))
    }
}

/// `GetPrivateProfileStringW(L"Password", key, L"", ..., file)`.
///
/// Byte strings become text the way `wc::fromUtf8` does it — invalid UTF-8 is
/// replaced rather than rejected, because `MultiByteToWideChar` without
/// `MB_ERR_INVALID_CHARS` substitutes U+FFFD and carries on.
fn read_ini(path: &Path, key: &[u8]) -> Vec<u8> {
    let Ok(ini) = tt_config::Ini::load(path) else {
        return Vec::new();
    };
    ini.get(SECTION, &String::from_utf8_lossy(key))
        .unwrap_or_default()
        .as_bytes()
        .to_vec()
}

/// `WritePrivateProfileStringW`. `None` for the value deletes — the key if
/// there is one, the whole section if the key is empty, which is upstream's
/// NULL in both positions.
///
/// The file is read, edited and written back, so a `[Password]` section can sit
/// in the same file as anything else and everything else survives.
fn write_ini(path: &Path, key: &[u8], value: Option<&[u8]>) -> bool {
    let mut ini = tt_config::Ini::load(path).unwrap_or_default();
    let key = String::from_utf8_lossy(key).into_owned();
    match value {
        Some(v) => {
            if !ini.set(SECTION, &key, &String::from_utf8_lossy(v)) {
                return false;
            }
        }
        None if key.is_empty() => ini.remove_section(SECTION),
        None => ini.remove(SECTION, &key),
    }
    ini.save(path).is_ok()
}

#[cfg(test)]
mod tests {
    use crate::host::{DialogEnd, RecordingHost};
    use crate::interp::Interp;
    use crate::TtlError;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tt-ttl-pwdcmds-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn run_in(dir: &Path, host: &mut RecordingHost, src: &str) {
        let name = dir.join("t.ttl").to_string_lossy().into_owned();
        let mut it = Interp::new(name, src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(dir: &Path, src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        run_in(dir, &mut host, src);
        host
    }

    fn ok(dir: &Path, src: &str) -> RecordingHost {
        let h = run(dir, src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        h
    }

    fn err_of(dir: &Path, src: &str) -> TtlError {
        let h = run(dir, src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn v1_stores_a_password_and_reads_it_back() {
        let d = scratch("v1");
        let h = ok(
            &d,
            "setpassword 'p.dat' 'acct' 'hunter2'\n\
             dispstr result';'\n\
             ispassword 'p.dat' 'acct'\ndispstr result';'\n\
             ispassword 'p.dat' 'other'\ndispstr result';'\n\
             getpassword 'p.dat' 'acct' pw\ndispstr result';'pw",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "1;1;0;1;hunter2");

        // The file is an INI file with one section, and the value is printable
        // ASCII — which is what lets it live in one. A file this port creates
        // carries a UTF-8 BOM where upstream's `WritePrivateProfileStringW`
        // would have written the ANSI codepage; `ini-audit/` measured Win32
        // reading a BOM'd file correctly, and it is the same deliberate
        // divergence `tt-config` already makes for the settings file.
        let raw = String::from_utf8(std::fs::read(d.join("p.dat")).unwrap()).unwrap();
        assert!(raw.starts_with("\u{feff}[Password]"), "{raw:?}");
        assert!(raw.contains("acct="), "{raw:?}");
        assert!(!raw.contains("hunter2"), "stored in the clear: {raw:?}");
    }

    #[test]
    fn v1_prompts_when_the_file_has_no_entry_and_stores_the_answer() {
        let d = scratch("v1ask");
        let mut host = RecordingHost::new();
        host.input_replies
            .push_back(DialogEnd::Ok(b"typed-in".to_vec()));
        run_in(
            &d,
            &mut host,
            "getpassword 'p.dat' 'acct' pw\ndispstr result';'pw",
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(String::from_utf8_lossy(&host.output), "1;typed-in");
        // The prompt's text is the key name and its title is the literal.
        assert_eq!(
            host.dialogs,
            vec![r#"passwordbox "acct" "Enter password" """#]
        );

        // ...and the second run does not ask.
        let h = ok(&d, "getpassword 'p.dat' 'acct' pw\ndispstr pw");
        assert_eq!(String::from_utf8_lossy(&h.output), "typed-in");
        assert!(h.dialogs.is_empty());
    }

    #[test]
    fn v1_takes_an_empty_answer_as_no_password_and_close_as_the_end() {
        let d = scratch("v1esc");
        // Escape, which this dialog answers with an empty string because its
        // buffer is initialised — unlike `inputbox`'s.
        let mut host = RecordingHost::new();
        host.input_replies.push_back(DialogEnd::Cancel);
        run_in(
            &d,
            &mut host,
            "result = 9\ngetpassword 'p.dat' 'acct' pw\ndispstr result';'pw'!'",
        );
        assert_eq!(String::from_utf8_lossy(&host.output), "0;!");
        assert!(
            !d.join("p.dat").exists(),
            "nothing should have been written"
        );

        // The Close button ends the macro where it stands, which no other
        // dialog command does.
        let mut host = RecordingHost::new();
        host.input_replies.push_back(DialogEnd::Closed);
        run_in(
            &d,
            &mut host,
            "getpassword 'p.dat' 'acct' pw\ndispstr 'not reached'",
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert!(host.output.is_empty());
    }

    #[test]
    fn v1_deletes_one_entry_or_the_whole_section() {
        let d = scratch("v1del");
        ok(
            &d,
            "setpassword 'p.dat' 'a' 'one'\nsetpassword 'p.dat' 'b' 'two'",
        );
        let h = ok(
            &d,
            "delpassword 'p.dat' 'a'\n\
             ispassword 'p.dat' 'a'\ndispstr result';'\n\
             ispassword 'p.dat' 'b'\ndispstr result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "0;1");

        // An empty name takes the section with it.
        let h = ok(
            &d,
            "delpassword 'p.dat' ''\nispassword 'p.dat' 'b'\ndispstr result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "0");
        let raw = std::fs::read_to_string(d.join("p.dat")).unwrap();
        assert!(!raw.contains("[Password]"), "{raw:?}");
    }

    #[test]
    fn delpassword_says_nothing_about_what_it_did() {
        let d = scratch("v1delq");
        // No `result` on any path, so a script testing it reads the last
        // command's answer. Both the missing file and the successful delete.
        let h = ok(
            &d,
            "result = 7\ndelpassword 'missing.dat' 'a'\ndispstr result';'\n\
             setpassword 'p.dat' 'a' 'x'\nresult = 8\n\
             delpassword 'p.dat' 'a'\ndispstr result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "7;8");
        // An empty filename is silent for v1 and a syntax error for v2.
        assert!(ok(&d, "delpassword '' 'a'").errors.is_empty());
        assert_eq!(err_of(&d, "delpassword2 '' 'a'"), TtlError::Syntax);
    }

    #[test]
    fn the_empty_argument_rules_are_upstreams_and_disagree_with_each_other() {
        let d = scratch("empty");
        // `setpassword` refuses an empty anything, including the password.
        assert_eq!(err_of(&d, "setpassword '' 'a' 'p'"), TtlError::Syntax);
        assert_eq!(err_of(&d, "setpassword 'f' '' 'p'"), TtlError::Syntax);
        assert_eq!(err_of(&d, "setpassword 'f' 'a' ''"), TtlError::Syntax);
        assert_eq!(err_of(&d, "ispassword 'f' ''"), TtlError::Syntax);
        assert_eq!(err_of(&d, "setpassword2 'f' 'a' 'p' ''"), TtlError::Syntax);

        // `getpassword` returns quietly from the same condition — and does not
        // write `result`, so the 9 survives.
        let h = ok(
            &d,
            "result = 9\npw = 'kept'\ngetpassword '' 'a' pw\ndispstr result';'pw'!'",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "9;!");
        let h = ok(&d, "result = 9\ngetpassword 'f.dat' '' pw\ndispstr result");
        assert_eq!(String::from_utf8_lossy(&h.output), "9");

        // The variable is cleared before any of those checks, though.
        assert_eq!(err_of(&d, "getpassword2 'f' '' pw 'k'"), TtlError::Syntax);
    }

    #[test]
    fn v2_stores_under_a_key_the_macro_has_to_keep() {
        let d = scratch("v2");
        let h = ok(
            &d,
            "setpassword2 'p2.dat' 'acct' 'hunter2' 'master'\ndispstr result';'\n\
             ispassword2 'p2.dat' 'acct'\ndispstr result';'\n\
             getpassword2 'p2.dat' 'acct' pw 'master'\ndispstr result';'pw';'\n\
             getpassword2 'p2.dat' 'acct' pw2 'wrong'\ndispstr result';'pw2'!'",
        );
        // The wrong <encryptstr> is `result` 0 and an empty variable — not a
        // prompt, because the entry was found by its key hash.
        assert_eq!(String::from_utf8_lossy(&h.output), "1;1;1;hunter2;0;!");
        assert!(
            h.dialogs.is_empty(),
            "should not have prompted: {:?}",
            h.dialogs
        );
    }

    #[test]
    fn v2_prompts_only_when_the_key_is_absent() {
        let d = scratch("v2ask");
        let mut host = RecordingHost::new();
        host.input_replies
            .push_back(DialogEnd::Ok(b"typed-in".to_vec()));
        run_in(
            &d,
            &mut host,
            "getpassword2 'p2.dat' 'acct' pw 'master'\ndispstr result';'pw",
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(String::from_utf8_lossy(&host.output), "1;typed-in");
        assert_eq!(
            host.dialogs,
            vec![r#"passwordbox "acct" "Enter password" """#]
        );

        // Closing the window is not told apart from typing nothing — the
        // dialog's answer is discarded upstream — so the macro carries on.
        let mut host = RecordingHost::new();
        host.input_replies.push_back(DialogEnd::Closed);
        run_in(
            &d,
            &mut host,
            "getpassword2 'p2.dat' 'other' pw 'master'\ndispstr result';'pw'!'",
        );
        assert_eq!(String::from_utf8_lossy(&host.output), "0;!");
    }

    #[test]
    fn v2_deletes_by_key_and_wholesale() {
        let d = scratch("v2del");
        ok(
            &d,
            "setpassword2 'p2.dat' 'a' 'one' 'k'\n\
             setpassword2 'p2.dat' 'b' 'two' 'k'\n\
             delpassword2 'p2.dat' 'a'",
        );
        let h = ok(
            &d,
            "ispassword2 'p2.dat' 'a'\ndispstr result';'\n\
             ispassword2 'p2.dat' 'b'\ndispstr result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "0;1");

        ok(&d, "delpassword2 'p2.dat' ''");
        let h = ok(&d, "ispassword2 'p2.dat' 'b'\ndispstr result");
        assert_eq!(String::from_utf8_lossy(&h.output), "0");
    }

    #[test]
    fn the_two_formats_share_a_file_without_seeing_each_other() {
        // Both command families take a filename and the documentation uses
        // `password.dat` for both, so one file holding an INI section and a run
        // of v2 records has to work.
        let d = scratch("both");
        let h = ok(
            &d,
            "setpassword 'p.dat' 'acct' 'v1pass'\n\
             setpassword2 'p.dat' 'acct' 'v2pass' 'master'\n\
             getpassword 'p.dat' 'acct' a\n\
             getpassword2 'p.dat' 'acct' b 'master'\n\
             dispstr a';'b\n\
             ispassword 'p.dat' 'acct'\ndispstr ';'result\n\
             ispassword2 'p.dat' 'acct'\ndispstr ';'result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "v1pass;v2pass;1;1");

        // Deleting all of one leaves the other standing.
        let h = ok(
            &d,
            "delpassword2 'p.dat' ''\n\
             ispassword 'p.dat' 'acct'\ndispstr result';'\n\
             ispassword2 'p.dat' 'acct'\ndispstr result",
        );
        assert_eq!(String::from_utf8_lossy(&h.output), "1;0");
    }

    #[test]
    fn a_record_the_ini_layer_quotes_is_lost_and_that_is_upstreams() {
        // The obfuscated form is printable ASCII and includes both quote
        // characters, and `GetPrivateProfileString` strips one matched pair. A
        // record that happens to begin and end with the same quote therefore
        // comes back two characters short, fails the complement check, and is
        // reported as a success with an empty password.
        let d = scratch("quoted");
        std::fs::write(
            d.join("p.dat"),
            b"[Password]\r\nacct=\"not a real record\"\r\n",
        )
        .unwrap();
        let h = ok(&d, "getpassword 'p.dat' 'acct' pw\ndispstr result';'pw'!'");
        assert_eq!(String::from_utf8_lossy(&h.output), "1;!");
        // ...and the entry still counts as present, so a macro cannot even
        // detect the state by asking.
        let h = ok(&d, "ispassword 'p.dat' 'acct'\ndispstr result");
        assert_eq!(String::from_utf8_lossy(&h.output), "1");
    }
}
