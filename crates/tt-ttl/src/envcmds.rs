//! The environment, the clipboard, `exec`, and the three questions a macro
//! asks about the machine it is standing on.
//!
//! Everything here runs inside `ttpmacro.exe` rather than over DDE — except
//! `gethostname`, which is the odd one out and asks the terminal. So this is
//! the family where Win32 shows through hardest after `pathcmds`, and four of
//! the twelve commands have no Linux equivalent at all to be faithful to.
//! What each one does about that is on the command.
//!
//! Four things are worth knowing before the commands:
//!
//! - **`getspecialfolder` cannot fail.** `ttmlib.c:249` throws away the return
//!   of `GetSpecialFolderAlloc` and returns a literal 1, so the documented
//!   "0 when the command fails" never happens — and an unrecognised folder
//!   type reaches `strncpy_s` as a NULL source. Both are reproduced as far as
//!   a macro can see them: `result` is always 1, an unknown type is the empty
//!   string. Written up as an upstream defect in `docs/upstream-bugs.md`.
//! - **`getver` deliberately answers Tera Term's version**, not this port's.
//!   See [`ScriptHost::version`].
//! - **`clipb2var` reads out of a buffer the *previous* call filled.** The
//!   offset argument does not re-read the clipboard; only offset 0 does. The
//!   cache is a `static` in `ttl_gui.cpp` and a field here.
//! - **Two of them never check for end of line** — `var2clipb` and, when the
//!   feature is compiled in, `outputdebugstring`. Trailing junk is accepted
//!   where every neighbouring command calls it a syntax error.

#[cfg(unix)]
use std::path::PathBuf;
#[cfg(not(windows))]
use std::process::Command;

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::lexer::MAX_STR_LEN;
use crate::rsv::Rsv;
use crate::vars::VarType;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn env_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::Clipb2Var => self.cmd_clipb2var(host),
            Rsv::Var2Clipb => self.cmd_var2clipb(host),
            Rsv::Exec => self.cmd_exec(),
            Rsv::ExpandEnv => self.cmd_expand_env(),
            Rsv::GetEnv => self.cmd_get_env(),
            Rsv::SetEnv => self.cmd_set_env(),
            Rsv::GetSpecialFolder => self.cmd_get_special_folder(),
            Rsv::GetVer => self.cmd_get_ver(host),
            Rsv::GetHostname => self.cmd_get_hostname(host),
            Rsv::GetIPv4Addr => self.cmd_get_ip_addr(host, false),
            Rsv::GetIPv6Addr => self.cmd_get_ip_addr(host, true),
            #[cfg(feature = "outputdebugstring")]
            Rsv::OutputDebugString => self.cmd_output_debug_string(),
            _ => return None,
        })
    }

    // ---- the clipboard ----

    /// `clipb2var <strvar> [<offset>]` (`ttl_gui.cpp:55`).
    ///
    /// The offset is in units of `MaxStrLen - 1` = 511 bytes, and it indexes
    /// **the cached copy**, not the clipboard: only an offset of 0 re-reads.
    /// A script walking a long clipboard therefore has to start at 0 and count
    /// up, which is what the documentation's example does and why it warns
    /// about it.
    ///
    /// `result` is 0 for a clipboard that could not be read, 1 for a whole
    /// string and 2 for a truncated one. The documented 3, "could not allocate
    /// a memory", is never set by any path.
    ///
    /// Two edges fall out of the arithmetic rather than out of any branch
    /// written for them, and both are reproduced: an **empty** clipboard is
    /// `result` 0, because the guard is `offset * 511 < len` and `0 < 0` is
    /// false; and an offset given before any successful offset-0 call finds a
    /// NULL cache and is also 0.
    fn cmd_clipb2var(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let offset = if self.lx.parameter_given() {
            expr::get_int_val(&mut self.lx, &mut self.vars)?
        } else {
            0
        };
        self.end_of_line()?;

        if offset == 0 {
            self.clipboard = host.clipboard_text().map(|s| {
                // Upstream stores it with `_strdup` and measures it with
                // `strlen`, so the clipboard is a C string here too.
                let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                s[..end].to_vec()
            });
            if self.clipboard.is_none() {
                self.set_result(0);
                return Ok(());
            }
        }

        // `Num * (MaxStrLen - 1)` is `int` arithmetic upstream and overflows
        // for a large enough offset; done in 64 bits it simply fails the test,
        // which is the same answer without the undefined behaviour.
        let chunk = (MAX_STR_LEN - 1) as i64;
        let start = i64::from(offset) * chunk;
        let cached = self.clipboard.as_deref().unwrap_or(b"");
        if self.clipboard.is_none() || offset < 0 || start >= cached.len() as i64 {
            self.set_result(0);
            return Ok(());
        }

        let rest = &cached[start as usize..];
        let truncated = rest.len() > chunk as usize;
        let piece = &rest[..rest.len().min(chunk as usize)];
        let piece = piece.to_vec();
        self.set_result(if truncated { 2 } else { 1 });
        self.vars.set_str(target, &piece);
        Ok(())
    }

    /// `var2clipb <strval>` (`ttl_gui.cpp:110`).
    ///
    /// **No end-of-line check**, alone in its neighbourhood: `var2clipb 'x' y`
    /// is accepted and `y` is never looked at.
    fn cmd_var2clipb(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let text = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let ok = host.set_clipboard_text(&text);
        self.set_result(i32::from(ok));
        Ok(())
    }

    // ---- the environment ----

    /// `getenv <envname> <strvar>` — a name that is not set reads as `""`,
    /// which is indistinguishable from one set to the empty string.
    fn cmd_get_env(&mut self) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let val = get_env(&name).unwrap_or_default();
        self.vars.set_str(target, &val);
        Ok(())
    }

    /// `setenv <envname> <strval>` — the macro's own environment, which is
    /// this process's, so it is inherited by anything [`exec`] starts and by
    /// nothing else.
    ///
    /// `_wputenv_s` reports `EINVAL` for a name that is empty or holds an `=`;
    /// upstream discards the return, so the command is silent either way and
    /// so is this one. Rust's `set_var` would panic on the same names, which
    /// is why they are filtered rather than passed on.
    ///
    /// [`exec`]: Interp::cmd_exec
    fn cmd_set_env(&mut self) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let val = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        set_env(&name, &val);
        Ok(())
    }

    /// `expandenv <strvar> [<strval>]` — `%NAME%` substitution, Win32's.
    ///
    /// With one argument the variable is expanded in place; with two, the
    /// second is expanded into the first.
    fn cmd_expand_env(&mut self) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let src = if self.lx.parameter_given() {
            let s = expr::get_str_val(&mut self.lx, &mut self.vars)?;
            self.end_of_line()?;
            s
        } else {
            self.vars.str_at(target).to_vec()
        };
        let expanded = expand_env(&src);
        self.vars.set_str(target, &expanded);
        Ok(())
    }

    /// `getspecialfolder <strvar> <foldertype>` (`ttl.cpp:2720`).
    ///
    /// Sixteen Windows shell folders, of which ten have something an XDG
    /// desktop would call the same thing and six do not. A name with no
    /// answer here — and an unrecognised name — is the empty string, which is
    /// what upstream produces for an unrecognised one too. See [`special_folder`].
    ///
    /// `result` is 1 whatever happens, because `GetSpecialFolder` returns a
    /// literal 1.
    fn cmd_get_special_folder(&mut self) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let kind = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let folder = special_folder(&kind).unwrap_or_default();
        self.vars.set_str(target, &folder);
        self.set_result(1);
        Ok(())
    }

    /// `getver <strvar> [<version>]` (`ttl.cpp:2960`).
    ///
    /// Without a version to compare against, `result` is left alone — one of
    /// the few commands in the language that does not touch it.
    ///
    /// The order matters and is upstream's: a `<version>` that is not `M.m`
    /// sets `result` to -2 and **returns before the variable is written**, so
    /// a script that checked the string rather than `result` sees whatever was
    /// in it before.
    fn cmd_get_ver(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let compare = if self.lx.parameter_given() {
            let s = expr::get_str_val(&mut self.lx, &mut self.vars)?;
            let Some(v) = scan_version(&s) else {
                self.set_result(-2);
                return Ok(());
            };
            Some(v)
        } else {
            None
        };
        self.end_of_line()?;

        let (major, minor) = host.version();
        self.vars
            .set_str(target, format!("{major}.{minor}").as_bytes());
        if let Some((cmp_major, cmp_minor)) = compare {
            let ours = major * 10000 + minor;
            let theirs = cmp_major * 10000 + cmp_minor;
            self.set_result(match ours.cmp(&theirs) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            });
        }
        Ok(())
    }

    // ---- the machine ----

    /// `gethostname <strvar>` — the only command in this file that asks the
    /// terminal. Arguments first, then the link check, which is upstream's
    /// order and means `gethostname 'literal'` is a syntax error with no
    /// connection where `gethostname v` is a link error.
    fn cmd_get_hostname(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;
        let name = host.hostname()?;
        self.vars.set_str(target, &name);
        Ok(())
    }

    /// `getipv4addr <strary> <intvar>` / `getipv6addr <strary> <intvar>`.
    ///
    /// `result` is 1 when every address fitted, 0 when the array was too
    /// small — and the count variable still gets the *full* count, so a script
    /// can size the array and ask again — and -1 when the machine could not be
    /// asked at all.
    fn cmd_get_ip_addr(&mut self, host: &mut dyn ScriptHost, v6: bool) -> TtlResult<()> {
        let array = expr::get_ary_var(&mut self.lx, &mut self.vars, VarType::StrArray)?;
        let count = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        let Some(addrs) = host.local_ip_addresses(v6) else {
            self.set_result(-1);
            self.vars.set_int(count, 0);
            return Ok(());
        };

        let size = self.vars.array_len(array);
        for (i, addr) in addrs.iter().enumerate() {
            if i < size {
                if let Ok(slot) = self.vars.elem(array, i as i32) {
                    self.vars.set_str(slot, addr);
                }
            }
        }
        self.set_result(i32::from(addrs.len() <= size));
        self.vars.set_int(count, addrs.len() as i32);
        Ok(())
    }

    /// `exec <command line> [<show> [<wait> [<current directory>]]]`
    /// (`ttl.cpp:1121`).
    ///
    /// `result` is -1 if the program could not be started, the exit code if
    /// `<wait>` was given and non-zero, and 0 otherwise. Note the code tests
    /// `if (wait)` where the documentation says "if it is 1" — any non-zero
    /// value waits.
    ///
    /// On Windows this uses `CreateProcessW` directly, including the original
    /// command-line string and `STARTUPINFO.wShowWindow`. Three details need
    /// an explicit cross-platform answer:
    ///
    /// - **`<show>` has no Unix counterpart.** It is applied on Windows and
    ///   validated but dropped on Unix. An unrecognised word is still
    ///   `ErrSyntax`, because that is where a typo in a working script would
    ///   be caught.
    /// - **The command line is one string, and Windows splits it.** Splitting
    ///   it with `CommandLineToArgvW`'s rules is the Unix approximation:
    ///   `CreateProcess` runs a program, not a shell, so a script that wanted
    ///   globbing or a pipe already had to write `cmd /c ...` and will have to
    ///   write `sh -c ...` here. See [`split_command_line`].
    /// - **A child killed by a signal has no Windows equivalent.** It reports
    ///   `128 + signal`, which is every Unix shell's convention and at least
    ///   recognisable; -1 is taken already and means the program never ran.
    ///
    /// `<current directory>` is passed through unresolved, so a relative one
    /// is relative to the *process's* directory and not to the macro's
    /// `CurrentDir` — the two are deliberately different upstream and this
    /// command uses neither `GetAbsPath` nor `CurrentDir`.
    fn cmd_exec(&mut self) -> TtlResult<()> {
        let cmdline = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let mut mode = ExecShow::Show;
        let mut wait = 0;
        let mut dir = Vec::new();
        if self.lx.parameter_given() {
            let word = expr::get_str_val(&mut self.lx, &mut self.vars)?;
            mode = ExecShow::parse(&word).ok_or(TtlError::Syntax)?;
            if self.lx.parameter_given() {
                wait = expr::get_int_val(&mut self.lx, &mut self.vars)?;
                if self.lx.parameter_given() {
                    dir = expr::get_str_val(&mut self.lx, &mut self.vars)?;
                }
            }
        }
        if cmdline.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.end_of_line()?;

        #[cfg(windows)]
        let result = exec_windows(&cmdline, mode, wait != 0, &dir);
        #[cfg(not(windows))]
        let result = {
            let _ = mode;
            exec_portable(&cmdline, wait != 0, &dir)
        };
        self.set_result(result);
        Ok(())
    }

    /// `outputdebugstring <strval>` — **not in a shipping Tera Term.**
    ///
    /// `OUTPUTDEBUGSTRING_ENABLE` is commented out at `ttmparse.h:36`, so both
    /// the reserved word and the command are compiled out and a macro using it
    /// gets the syntax error an unknown word gets. The feature flag here is
    /// that `#if`, off by default for the same reason; enabling it writes to
    /// stderr, which is this platform's debugger channel.
    ///
    /// Two upstream sloppinesses come with it, both visible: there is **no end
    /// of line check**, and a failed `GetStrVal` is reported as `ErrSyntax`
    /// whatever it actually was, so a type mismatch is renamed on its way out.
    #[cfg(feature = "outputdebugstring")]
    fn cmd_output_debug_string(&mut self) -> TtlResult<()> {
        let s = expr::get_str_val(&mut self.lx, &mut self.vars).map_err(|_| TtlError::Syntax)?;
        eprintln!("{}", String::from_utf8_lossy(&s));
        Ok(())
    }
}

/// One environment variable, as bytes.
///
/// `OsString`'s encoded form is the platform's own — the bytes themselves on
/// unix, WTF-8 on Windows — which is what a TTL string wants either way. Going
/// through `String` would lose a name a shell can set and this cannot print.
fn get_env(name: &[u8]) -> Option<Vec<u8>> {
    let name = String::from_utf8(name.to_vec()).ok()?;
    std::env::var_os(name).map(|v| v.into_encoded_bytes())
}

fn set_env(name: &[u8], val: &[u8]) {
    // `set_var` panics on these; `_wputenv_s` returns `EINVAL` and upstream
    // ignores it, so the observable behaviour to reproduce is "nothing".
    if name.is_empty() || name.contains(&b'=') || name.contains(&0) || val.contains(&0) {
        return;
    }
    let (Ok(name), Ok(val)) = (
        String::from_utf8(name.to_vec()),
        String::from_utf8(val.to_vec()),
    ) else {
        return;
    };
    std::env::set_var(name, val);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecShow {
    Hide,
    Minimize,
    Maximize,
    Show,
}

impl ExecShow {
    fn parse(word: &[u8]) -> Option<Self> {
        match word.to_ascii_lowercase().as_slice() {
            b"hide" => Some(Self::Hide),
            b"minimize" => Some(Self::Minimize),
            b"maximize" => Some(Self::Maximize),
            b"show" => Some(Self::Show),
            _ => None,
        }
    }

    #[cfg(windows)]
    fn win32(self) -> u16 {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_SHOW,
        };

        match self {
            Self::Hide => SW_HIDE as u16,
            Self::Minimize => SW_MINIMIZE as u16,
            Self::Maximize => SW_MAXIMIZE as u16,
            Self::Show => SW_SHOW as u16,
        }
    }
}

#[cfg(windows)]
fn exec_windows(cmdline: &[u8], mode: ExecShow, wait: bool, dir: &[u8]) -> i32 {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, WaitForSingleObject, INFINITE, NORMAL_PRIORITY_CLASS,
        PROCESS_INFORMATION, STARTF_USESHOWWINDOW, STARTUPINFOW,
    };

    // `wc::fromUtf8`, used upstream, replaces malformed UTF-8 rather than
    // treating TTL bytes as the active Windows code page.
    let mut command: Vec<u16> = String::from_utf8_lossy(cmdline).encode_utf16().collect();
    command.push(0);
    let mut directory: Vec<u16> = String::from_utf8_lossy(dir).encode_utf16().collect();
    directory.push(0);

    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        wShowWindow: mode.win32(),
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    let current_dir = if dir.is_empty() {
        null()
    } else {
        directory.as_ptr()
    };

    // SAFETY: both UTF-16 buffers are NUL-terminated and remain live and
    // mutable for the call. The security pointers and environment are absent,
    // and both output structures have the layout required by Win32.
    let started = unsafe {
        CreateProcessW(
            null(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            NORMAL_PRIORITY_CLASS,
            null(),
            current_dir,
            &startup,
            &mut process,
        )
    };
    if started == 0 {
        return -1;
    }

    let result = if wait {
        let mut code = 0;
        // Upstream waits without a timeout and treats the DWORD exit code as
        // the macro language's signed `int`.
        unsafe {
            WaitForSingleObject(process.hProcess, INFINITE);
            GetExitCodeProcess(process.hProcess, &mut code);
        }
        code as i32
    } else {
        0
    };

    // SAFETY: successful CreateProcessW returned two owned, live handles.
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    result
}

#[cfg(not(windows))]
fn exec_portable(cmdline: &[u8], wait: bool, dir: &[u8]) -> i32 {
    let argv = split_command_line(cmdline);
    let Some((program, args)) = argv.split_first() else {
        return -1;
    };
    let mut cmd = Command::new(bytes_to_os(program));
    cmd.args(args.iter().map(|a| bytes_to_os(a)));
    if !dir.is_empty() {
        cmd.current_dir(bytes_to_os(dir));
    }

    if wait {
        match cmd.status() {
            Ok(st) => exit_code(st),
            Err(_) => -1,
        }
    } else {
        match cmd.spawn() {
            // The child is not reaped. Upstream closes both handles and walks
            // away too; a frontend that runs long-lived macros will want a
            // `SIGCHLD` handler, which is not the interpreter's.
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

#[cfg(not(windows))]
fn bytes_to_os(b: &[u8]) -> std::ffi::OsString {
    // Safety: the bytes came out of a TTL string, which on Unix is exactly an
    // `OsStr`'s encoded form.
    unsafe { std::ffi::OsString::from_encoded_bytes_unchecked(b.to_vec()) }
}

#[cfg(not(windows))]
fn exit_code(st: std::process::ExitStatus) -> i32 {
    match st.code() {
        Some(c) => c,
        // Killed by a signal. See `cmd_exec` for why this is not -1.
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                st.signal().map_or(-1, |s| 128 + s)
            }
            #[cfg(not(unix))]
            {
                -1
            }
        }
    }
}

/// `ExpandEnvironmentStrings` — replace every `%NAME%` with its value.
///
/// A name that is not set is left alone, percent signs and all, which is what
/// the documentation's own example shows. Expansion is not recursive: scanning
/// resumes after whatever was substituted.
///
/// Windows asks `ExpandEnvironmentStringsW` directly, as upstream does. Unix
/// has no environment-string API; its small equivalent follows the same
/// delimiter rule, including that the closing `%` of an unknown name is not
/// reused as the opening delimiter of a second name.
#[cfg(not(windows))]
fn expand_env(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'%' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        match src[i + 1..].iter().position(|&b| b == b'%') {
            None => {
                // No closing delimiter: the rest is literal.
                out.extend_from_slice(&src[i..]);
                break;
            }
            Some(rel) => {
                let end = i + 1 + rel;
                match get_env(&src[i + 1..end]) {
                    Some(val) => {
                        out.extend_from_slice(&val);
                        i = end + 1;
                    }
                    // **The closing percent is not consumed.** Windows emits
                    // the opener and the name and then resumes scanning *at*
                    // the delimiter, so it opens the next name:
                    // `%UNSET%KNOWN%` is `%UNSET` followed by `KNOWN`'s value,
                    // not the whole string left alone. This file recorded the
                    // opposite for a while, and every test agreed with it
                    // because the only case that can tell them apart is two
                    // names in a row with the first one unset.
                    None => {
                        out.extend_from_slice(&src[i..end]);
                        i = end;
                    }
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn expand_env(src: &[u8]) -> Vec<u8> {
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

    let mut source = crate::source::to_upstream_wide(src);
    source.push(0);
    // SAFETY: `source` is live and NUL-terminated; a NULL destination asks
    // Win32 for the required size, including its terminating NUL.
    let needed = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), null_mut(), 0) };
    if needed == 0 {
        return Vec::new();
    }
    let mut expanded = vec![0u16; needed as usize];
    // SAFETY: `expanded` has exactly the size returned above and neither live
    // buffer aliases the other.
    let written =
        unsafe { ExpandEnvironmentStringsW(source.as_ptr(), expanded.as_mut_ptr(), needed) };
    if written == 0 || written > needed {
        return Vec::new();
    }
    let end = expanded
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(written as usize);
    crate::source::from_upstream_wide(&expanded[..end])
}

/// `sscanf(s, "%d.%d")`, and it must fill both fields.
///
/// `%d` skips leading whitespace and takes an optional sign; the literal `.`
/// does not skip anything. Trailing text is ignored, so `'4.56 or so'` is a
/// perfectly good version and `'4 . 56'` is not.
fn scan_version(s: &[u8]) -> Option<(i32, i32)> {
    let (major, rest) = scan_int(s)?;
    let rest = rest.strip_prefix(b".")?;
    let (minor, _) = scan_int(rest)?;
    Some((major, minor))
}

fn scan_int(s: &[u8]) -> Option<(i32, &[u8])> {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let negative = match s.get(i) {
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
    let mut val: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        val = (val * 10 + i64::from(s[i] - b'0')).min(i64::from(i32::MAX) + 1);
        i += 1;
    }
    if i == start {
        return None;
    }
    let val = if negative { -val } else { val };
    Some((
        val.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        &s[i..],
    ))
}

/// `CommandLineToArgvW`'s rules, which is how the child of a `CreateProcess`
/// with a NULL application name reads the string it was handed.
///
/// Backslashes only escape in front of a quote: `2n` of them become `n` and
/// the quote opens or closes a run, `2n+1` become `n` and the quote is a
/// literal. Everything else is a plain byte, so a Windows path in a script
/// survives being read here even though it will not resolve.
///
/// `argv[0]` has its own rules and they are not the others': no backslash
/// escaping at all, and the word simply ends at the first space unless it
/// opened with a quote.
#[cfg(any(not(windows), test))]
fn split_command_line(cmdline: &[u8]) -> Vec<Vec<u8>> {
    let mut argv = Vec::new();
    let mut i = 0;
    let n = cmdline.len();

    // argv[0].
    while i < n && (cmdline[i] == b' ' || cmdline[i] == b'\t') {
        i += 1;
    }
    if i < n {
        let mut first = Vec::new();
        if cmdline[i] == b'"' {
            i += 1;
            while i < n && cmdline[i] != b'"' {
                first.push(cmdline[i]);
                i += 1;
            }
            i += 1; // the closing quote, or one past the end
        } else {
            while i < n && cmdline[i] != b' ' && cmdline[i] != b'\t' {
                first.push(cmdline[i]);
                i += 1;
            }
        }
        argv.push(first);
    }

    // ...and the rest.
    let mut cur = Vec::new();
    let mut in_arg = false;
    let mut in_quotes = false;
    while i < n {
        let c = cmdline[i];
        if !in_arg && (c == b' ' || c == b'\t') {
            i += 1;
            continue;
        }
        in_arg = true;
        match c {
            b'\\' => {
                let mut slashes = 0;
                while i < n && cmdline[i] == b'\\' {
                    slashes += 1;
                    i += 1;
                }
                if i < n && cmdline[i] == b'"' {
                    cur.extend(std::iter::repeat_n(b'\\', slashes / 2));
                    if slashes % 2 == 1 {
                        cur.push(b'"');
                    } else {
                        in_quotes = !in_quotes;
                    }
                    i += 1;
                } else {
                    cur.extend(std::iter::repeat_n(b'\\', slashes));
                }
            }
            b'"' => {
                in_quotes = !in_quotes;
                i += 1;
            }
            b' ' | b'\t' if !in_quotes => {
                argv.push(std::mem::take(&mut cur));
                in_arg = false;
                i += 1;
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    if in_arg {
        argv.push(cur);
    }
    argv
}

/// One of `getspecialfolder`'s sixteen names. `None` is a name with no
/// counterpart, or a name that is not one of the sixteen — upstream cannot
/// tell those apart either.
///
/// Six have no answer and say so rather than inventing one:
/// `AllUsersDesktop`, `Favorites`, `NetHood`, `PrintHood`, `Recent`, and
/// `SendTo`. `Recent` is the near miss — the desktop does keep one, but as a
/// *file* (`recently-used.xbel`), and the command's contract is a directory.
#[cfg(unix)]
fn special_folder(kind: &[u8]) -> Option<Vec<u8>> {
    let data_home = xdg_dir("XDG_DATA_HOME", ".local/share")?;
    let config_home = xdg_dir("XDG_CONFIG_HOME", ".config")?;
    let path = match kind.to_ascii_lowercase().as_slice() {
        b"desktop" => user_dir("XDG_DESKTOP_DIR", "Desktop")?,
        b"mydocuments" => user_dir("XDG_DOCUMENTS_DIR", "Documents")?,
        b"templates" => user_dir("XDG_TEMPLATES_DIR", "Templates")?,
        b"fonts" => data_home.join("fonts"),
        b"programs" | b"startmenu" => data_home.join("applications"),
        b"startup" => config_home.join("autostart"),
        b"allusersprograms" | b"allusersstartmenu" => PathBuf::from("/usr/share/applications"),
        b"allusersstartup" => PathBuf::from("/etc/xdg/autostart"),
        _ => return None,
    };
    Some(crate::files::path_to_bytes(&path))
}

/// The same table as `GetSpecialFolderAlloc` (`ttmlib.c:207`), using the
/// known-folder API its Vista-and-later branch resolves dynamically.
#[cfg(windows)]
fn special_folder(kind: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_CommonPrograms, FOLDERID_CommonStartMenu, FOLDERID_CommonStartup,
        FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Favorites, FOLDERID_Fonts, FOLDERID_NetHood,
        FOLDERID_PrintHood, FOLDERID_Programs, FOLDERID_PublicDesktop, FOLDERID_Recent,
        FOLDERID_SendTo, FOLDERID_StartMenu, FOLDERID_Startup, FOLDERID_Templates,
    };

    let folder = match kind.to_ascii_lowercase().as_slice() {
        b"allusersdesktop" => &FOLDERID_PublicDesktop,
        b"allusersstartmenu" => &FOLDERID_CommonStartMenu,
        b"allusersprograms" => &FOLDERID_CommonPrograms,
        b"allusersstartup" => &FOLDERID_CommonStartup,
        b"desktop" => &FOLDERID_Desktop,
        b"favorites" => &FOLDERID_Favorites,
        b"fonts" => &FOLDERID_Fonts,
        b"mydocuments" => &FOLDERID_Documents,
        b"nethood" => &FOLDERID_NetHood,
        b"printhood" => &FOLDERID_PrintHood,
        b"programs" => &FOLDERID_Programs,
        b"recent" => &FOLDERID_Recent,
        b"sendto" => &FOLDERID_SendTo,
        b"startmenu" => &FOLDERID_StartMenu,
        b"startup" => &FOLDERID_Startup,
        b"templates" => &FOLDERID_Templates,
        _ => return None,
    };
    known_folder(folder)
}

#[cfg(not(any(unix, windows)))]
fn special_folder(_kind: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
fn known_folder(folder: &windows_sys::core::GUID) -> Option<Vec<u8>> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

    let mut path = std::ptr::null_mut();
    // SAFETY: `folder` and the output pointer are live, the flags are zero as
    // upstream's call specifies, and a null token means the current user.
    let result = unsafe { SHGetKnownFolderPath(folder, 0, std::ptr::null_mut(), &mut path) };
    if result < 0 || path.is_null() {
        // The API contract permits task memory even on failure.
        // SAFETY: CoTaskMemFree accepts null and the returned pointer.
        unsafe { CoTaskMemFree(path.cast()) };
        return None;
    }

    // SAFETY: a successful call returns a NUL-terminated UTF-16 string which
    // stays live until freed below.
    let len = unsafe {
        let mut len = 0;
        while *path.add(len) != 0 {
            len += 1;
        }
        len
    };
    // SAFETY: `len` was found inside the API-owned NUL-terminated allocation.
    let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(path, len) });
    // SAFETY: the pointer came from SHGetKnownFolderPath and is freed once.
    unsafe { CoTaskMemFree(path.cast()) };
    Some(value.into_bytes())
}

#[cfg(unix)]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

#[cfg(unix)]
fn xdg_dir(var: &str, default: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => Some(home()?.join(default)),
    }
}

/// One of the user directories, which live in `~/.config/user-dirs.dirs`
/// rather than in the environment — the session exports them only sometimes,
/// so the file is the authority and the environment is the override.
#[cfg(unix)]
fn user_dir(var: &str, default: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(var) {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    let home = home()?;
    let file = xdg_dir("XDG_CONFIG_HOME", ".config")?.join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(&file) {
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix(var) else {
                continue;
            };
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let val = rest.trim().trim_matches('"');
            // The file is sourced by a shell, so `$HOME` is spelled out.
            let val = match val.strip_prefix("$HOME") {
                Some(tail) => home.join(tail.trim_start_matches('/')),
                None => PathBuf::from(val),
            };
            if !val.as_os_str().is_empty() {
                return Some(val);
            }
        }
    }
    Some(home.join(default))
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::special_folder;
    use super::{expand_env, scan_version, split_command_line};
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        run_with(&mut host, src);
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

    // ---- the clipboard ----

    #[test]
    fn clipb2var_reports_one_for_a_whole_string_and_zero_for_no_clipboard() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard = Some(b"hello".to_vec());
        run_with(&mut host, "clipb2var s\ndispstr result '|' s");
        assert_eq!(host.output, b"1|hello");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard = None;
        run_with(&mut host, "s = 'kept'\nclipb2var s\ndispstr result '|' s");
        assert_eq!(host.output, b"0|kept", "the variable is left alone");
    }

    #[test]
    fn an_empty_clipboard_is_zero_because_the_guard_is_a_less_than() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard = Some(Vec::new());
        run_with(&mut host, "clipb2var s\ndispstr result");
        assert_eq!(host.output, b"0");
    }

    #[test]
    fn the_offset_walks_the_copy_the_last_offset_zero_took() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard = Some(vec![b'x'; 511 + 3]);
        run_with(
            &mut host,
            "clipb2var a\n\
             r1 = result\n\
             clipb2var b 1\n\
             dispstr r1 '|' result '|'\n\
             strlen a\n\
             dispstr result '|'\n\
             strlen b\n\
             dispstr result",
        );
        assert_eq!(host.output, b"2|1|511|3");
        // The clipboard was read once, by the offset-0 call.
        assert_eq!(host.terminal, vec!["clipb2var"]);
    }

    #[test]
    fn an_offset_with_no_cache_behind_it_is_zero() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard = Some(b"hello".to_vec());
        run_with(&mut host, "clipb2var s 1\ndispstr result");
        assert_eq!(host.output, b"0");
        assert_eq!(host.terminal, Vec::<String>::new(), "and never read");

        // ...and so is a negative one, and one past the end.
        let h = run("clipb2var s (-1)\ndispstr result");
        assert_eq!(h.output, b"0");
    }

    #[test]
    fn var2clipb_reports_whether_it_wrote_and_never_checks_end_of_line() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard_writable = true;
        run_with(&mut host, "var2clipb 'text'\ndispstr result");
        assert_eq!(host.output, b"1");
        assert_eq!(host.clipboard, Some(b"text".to_vec()));

        let h = run("var2clipb 'text'\ndispstr result");
        assert_eq!(h.output, b"0", "a clipboard that could not be opened");

        // `ttl_gui.cpp:110` has no `GetFirstChar()` check, alone in the family.
        let h = run("var2clipb 'text' and then some\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
    }

    // ---- the environment ----

    #[test]
    fn getenv_and_setenv_are_the_macros_own_environment() {
        assert_eq!(
            out("setenv 'STERNA_TTL_TEST' 'yes'\ngetenv 'STERNA_TTL_TEST' v\ndispstr v"),
            "yes"
        );
        assert_eq!(
            out("getenv 'STERNA_NOT_SET_ANYWHERE' v\ndispstr '['v']'"),
            "[]",
            "an unset name is the empty string, not an error"
        );
        // A name `_wputenv_s` would refuse is silently nothing here too.
        assert!(run("setenv '' 'x'\nsetenv 'A=B' 'x'").errors.is_empty());
    }

    #[test]
    fn expandenv_replaces_what_it_knows_and_leaves_what_it_does_not() {
        std::env::set_var("STERNA_TTL_EXPAND", "VALUE");
        assert_eq!(
            expand_env(b"a%STERNA_TTL_EXPAND%b"),
            b"aVALUEb".to_vec(),
            "substituted"
        );
        assert_eq!(
            expand_env(b"%STERNA_NOT_SET%\\x"),
            b"%STERNA_NOT_SET%\\x".to_vec(),
            "the documentation's own example"
        );
        assert_eq!(expand_env(b"50%"), b"50%".to_vec(), "no closing delimiter");
        assert_eq!(expand_env(b"%%"), b"%%".to_vec(), "an empty name is unset");
        assert_eq!(
            expand_env(b"%STERNA_TTL_EXPAND%%STERNA_TTL_EXPAND%"),
            b"VALUEVALUE".to_vec(),
        );
        // The one case that can tell the two delimiter rules apart, and the
        // reason this was wrong until a native Windows run said so: everything
        // above passes whether or not the closing percent is consumed.
        assert_eq!(
            expand_env(b"%STERNA_NOT_SET%STERNA_TTL_EXPAND%"),
            b"%STERNA_NOT_SETVALUE".to_vec(),
            "an unknown name's closing percent opens the next name"
        );
    }

    #[cfg(windows)]
    #[test]
    fn expandenv_crosses_win32s_utf16_boundary_like_upstream() {
        std::env::set_var("STERNA_TTL_UNICODE", "café");
        assert_eq!(expand_env(b"%STERNA_TTL_UNICODE%"), "café".as_bytes());
        assert_eq!(expand_env(b"bad \xFF byte"), b"bad ? byte");
    }

    #[test]
    fn expandenv_takes_one_argument_or_two() {
        std::env::set_var("STERNA_TTL_EXPAND2", "V");
        assert_eq!(
            out("s = 'a%STERNA_TTL_EXPAND2%b'\nexpandenv s\ndispstr s"),
            "aVb"
        );
        assert_eq!(
            out("expandenv s 'a%STERNA_TTL_EXPAND2%b'\ndispstr s"),
            "aVb"
        );
        assert_eq!(err_of("expandenv 'literal'"), TtlError::Syntax);
    }

    // ---- getver ----

    #[test]
    fn getver_answers_tera_terms_version_and_compares_against_it() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.version = (4, 60);
        run_with(&mut host, "getver v\ndispstr v");
        assert_eq!(host.output, b"4.60");

        let cmp = |arg: &str| {
            let mut host = RecordingHost::new();
            host.linked = true;
            host.version = (4, 60);
            run_with(&mut host, &format!("getver v {arg}\ndispstr result"));
            String::from_utf8_lossy(&host.output).into_owned()
        };
        assert_eq!(cmp("'4.56'"), "1", "we are newer");
        assert_eq!(cmp("'4.60'"), "0");
        assert_eq!(cmp("'5.0'"), "-1", "we are older");
        assert_eq!(cmp("'4'"), "-2", "not two fields");
        assert_eq!(cmp("'abc'"), "-2");
    }

    #[test]
    fn a_bad_version_returns_before_the_variable_is_written() {
        let mut host = RecordingHost::new();
        host.linked = true;
        run_with(
            &mut host,
            "v = 'untouched'\ngetver v 'nonsense'\ndispstr result '|' v",
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(host.output, b"-2|untouched");
    }

    #[test]
    fn getver_without_a_version_leaves_result_alone() {
        assert_eq!(out("result = 42\ngetver v\ndispstr result"), "42");
    }

    #[test]
    fn the_version_scanner_is_sscanfs() {
        assert_eq!(scan_version(b"4.56"), Some((4, 56)));
        assert_eq!(scan_version(b"  4.56 or so"), Some((4, 56)), "and trailing");
        assert_eq!(scan_version(b"4. 56"), Some((4, 56)), "%d skips space");
        assert_eq!(scan_version(b"4 . 56"), None, "a literal '.' does not");
        assert_eq!(scan_version(b"-1.-2"), Some((-1, -2)));
        assert_eq!(scan_version(b"4."), None);
        assert_eq!(scan_version(b""), None);
    }

    // ---- the machine ----

    #[test]
    fn gethostname_wants_a_terminal_and_asks_it() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.hostname = b"router.example".to_vec();
        run_with(&mut host, "gethostname h\ndispstr h");
        assert_eq!(host.output, b"router.example");

        let mut host = RecordingHost::new();
        run_with(&mut host, "gethostname h");
        assert_eq!(host.errors.first().map(|e| e.0), Some(TtlError::LinkFirst));

        // Arguments are checked first, so this one never reaches the link.
        assert_eq!(err_of("gethostname 'literal'"), TtlError::Syntax);
    }

    #[test]
    fn the_ip_commands_fill_what_fits_and_count_what_did_not() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.ipv4 = Some(vec![b"192.0.2.1".to_vec(), b"198.51.100.7".to_vec()]);
        run_with(
            &mut host,
            "strdim a 4\ngetipv4addr a n\ndispstr result '|' n '|' a[0] '|' a[1]",
        );
        assert_eq!(host.output, b"1|2|192.0.2.1|198.51.100.7");

        // Too small: `result` 0, and the count is still the whole count.
        let mut host = RecordingHost::new();
        host.linked = true;
        host.ipv4 = Some(vec![b"192.0.2.1".to_vec(), b"198.51.100.7".to_vec()]);
        run_with(
            &mut host,
            "strdim a 1\ngetipv4addr a n\ndispstr result '|' n",
        );
        assert_eq!(host.output, b"0|2");

        // A machine that cannot say at all.
        let h = run("strdim a 4\ngetipv6addr a n\ndispstr result '|' n");
        assert_eq!(h.output, b"-1|0");
    }

    #[test]
    fn the_ip_commands_want_an_array_that_already_exists() {
        assert_eq!(err_of("getipv4addr a n"), TtlError::VarNotInit);
        assert_eq!(
            err_of("intdim a 4\ngetipv4addr a n"),
            TtlError::TypeMismatch
        );
    }

    // ---- getspecialfolder ----

    #[test]
    fn getspecialfolder_always_reports_success() {
        // `ttmlib.c:249` returns a literal 1, so the documented 0 is dead.
        assert_eq!(out("getspecialfolder s 'Desktop'\ndispstr result"), "1");
        assert_eq!(
            out("getspecialfolder s 'NoSuchFolder'\ndispstr result"),
            "1"
        );
        assert_eq!(
            out("getspecialfolder s 'NoSuchFolder'\ndispstr '['s']'"),
            "[]",
            "and an unknown name is the empty string"
        );
    }

    #[cfg(unix)]
    #[test]
    fn getspecialfolder_matches_its_name_without_regard_to_case() {
        // `XDG_DATA_HOME` rather than `HOME`: the tests share a process, and
        // moving `HOME` under the others would be a needless way to make one
        // of them fail somewhere else.
        std::env::set_var("XDG_DATA_HOME", "/somewhere/share");
        assert_eq!(
            out("getspecialfolder s 'FONTS'\ndispstr s"),
            "/somewhere/share/fonts"
        );
        assert_eq!(
            out("getspecialfolder s 'programs'\ndispstr s"),
            "/somewhere/share/applications"
        );
        std::env::remove_var("XDG_DATA_HOME");
        // The two that are fixed paths need no environment at all.
        assert_eq!(
            out("getspecialfolder s 'AllUsersStartup'\ndispstr s"),
            "/etc/xdg/autostart"
        );
        assert_eq!(
            out("getspecialfolder s 'AllUsersPrograms'\ndispstr s"),
            "/usr/share/applications"
        );
    }

    #[cfg(windows)]
    #[test]
    fn getspecialfolder_uses_all_sixteen_windows_shell_folders() {
        for name in [
            "AllUsersDesktop",
            "AllUsersStartMenu",
            "AllUsersPrograms",
            "AllUsersStartup",
            "Desktop",
            "Favorites",
            "Fonts",
            "MyDocuments",
            "NetHood",
            "PrintHood",
            "Programs",
            "Recent",
            "SendTo",
            "StartMenu",
            "Startup",
            "Templates",
        ] {
            let folder = special_folder(name.as_bytes()).unwrap_or_else(|| panic!("{name}"));
            let folder = String::from_utf8(folder).expect(name);
            assert!(
                folder.as_bytes().get(1) == Some(&b':') || folder.starts_with(r"\\"),
                "{name}: {folder:?}"
            );
        }
    }

    // ---- exec ----

    #[test]
    fn exec_runs_a_program_and_can_wait_for_it() {
        #[cfg(unix)]
        let (success, failure) = ("/bin/true", "/bin/false");
        #[cfg(windows)]
        let (success, failure) = ("cmd.exe /c exit 0", "cmd.exe /c exit 7");

        assert_eq!(
            out(&format!("exec '{success}' 'show' 1\ndispstr result")),
            "0"
        );
        assert_eq!(
            out(&format!("exec '{failure}' 'show' 1\ndispstr result")),
            if cfg!(windows) { "7" } else { "1" }
        );
        assert_eq!(
            out("exec 'sterna-no-such-program-18327' 'show' 1\ndispstr result"),
            "-1"
        );
        // Without a wait, only "did it start" is reported.
        assert_eq!(out(&format!("exec '{success}'\ndispstr result")), "0");
        assert_eq!(
            out("exec 'sterna-no-such-program-18327'\ndispstr result"),
            "-1"
        );
    }

    #[test]
    fn exec_validates_the_show_word_on_every_platform() {
        #[cfg(unix)]
        let success = "/bin/true";
        #[cfg(windows)]
        let success = "cmd.exe /c exit 0";
        assert_eq!(
            out(&format!("exec '{success}' 'HIDE'\ndispstr result")),
            "0"
        );
        assert_eq!(
            err_of(&format!("exec '{success}' 'sideways'")),
            TtlError::Syntax
        );
        assert_eq!(err_of("exec ''"), TtlError::Syntax);
    }

    #[cfg(windows)]
    #[test]
    fn exec_passes_the_show_word_through_windows_startup_info() {
        use windows_sys::Win32::System::Threading::{
            GetStartupInfoW, STARTF_USESHOWWINDOW, STARTUPINFOW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_SHOW,
        };

        const CHILD_OUTPUT: &str = "STERNA_TTL_EXEC_STARTUP_INFO";
        if let Some(output) = std::env::var_os(CHILD_OUTPUT) {
            let mut startup = STARTUPINFOW::default();
            // SAFETY: GetStartupInfoW fills the live structure and has no
            // failure return. This is the child side of the smoke test.
            unsafe { GetStartupInfoW(&mut startup) };
            std::fs::write(
                output,
                format!("{} {}", startup.dwFlags, startup.wShowWindow),
            )
            .unwrap();
            return;
        }

        let exe = std::env::current_exe().unwrap();
        let output = std::env::temp_dir().join(format!(
            "tt-ttl-exec-startup-info-{}.txt",
            std::process::id()
        ));
        std::env::set_var(CHILD_OUTPUT, &output);
        let command = format!(
            "\"{}\" --exact envcmds::tests::exec_passes_the_show_word_through_windows_startup_info --nocapture",
            exe.display()
        );

        for (word, expected) in [
            (None, SW_SHOW),
            (Some("hide"), SW_HIDE),
            (Some("minimize"), SW_MINIMIZE),
            (Some("maximize"), SW_MAXIMIZE),
            (Some("show"), SW_SHOW),
        ] {
            let _ = std::fs::remove_file(&output);
            let source = match word {
                Some(word) => format!("exec '{command}' '{word}' 1\ndispstr result"),
                None => format!("exec '{command}'\ndispstr result"),
            };
            assert_eq!(out(&source), "0");
            let fields = (0..100)
                .find_map(|_| {
                    let fields = std::fs::read_to_string(&output).ok().and_then(|text| {
                        let fields = text
                            .split_whitespace()
                            .map(str::parse)
                            .collect::<Result<Vec<u32>, _>>()
                            .ok()?;
                        (fields.len() == 2).then_some(fields)
                    });
                    if fields.is_none() {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    fields
                })
                .unwrap_or_else(|| panic!("child did not write {}", output.display()));
            let [flags, show] = fields.as_slice() else {
                unreachable!()
            };
            assert_ne!(*flags & STARTF_USESHOWWINDOW, 0, "{word:?}");
            assert_eq!(*show, expected as u32, "{word:?}");
        }

        std::env::remove_var(CHILD_OUTPUT);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn exec_splits_its_command_line_the_way_the_child_would() {
        assert_eq!(split_command_line(b"prog a b"), [&b"prog"[..], b"a", b"b"]);
        assert_eq!(
            split_command_line(b"\"c:\\my dir\\prog.exe\" a"),
            [&b"c:\\my dir\\prog.exe"[..], b"a"]
        );
        assert_eq!(
            split_command_line(b"prog \"one arg\" two"),
            [&b"prog"[..], b"one arg", b"two"]
        );
        // 2n backslashes then a quote: n backslashes, and the quote is a
        // delimiter. 2n+1: n backslashes and a literal quote.
        assert_eq!(
            split_command_line(b"prog a\\\\\"b c\""),
            [&b"prog"[..], b"a\\b c"]
        );
        assert_eq!(
            split_command_line(b"prog a\\\\\\\"b"),
            [&b"prog"[..], b"a\\\"b"]
        );
        // A backslash away from a quote is just a backslash — a Windows path
        // in an old script survives being read, even though it will not open.
        assert_eq!(
            split_command_line(b"prog c:\\temp\\x"),
            [&b"prog"[..], b"c:\\temp\\x"]
        );
        assert_eq!(split_command_line(b"   "), Vec::<Vec<u8>>::new());
        // argv[0] does not escape, and an unterminated quote runs to the end.
        assert_eq!(split_command_line(b"\"prog"), [b"prog".to_vec()]);
    }

    #[test]
    fn exec_takes_a_directory_to_start_in() {
        let dir = std::env::temp_dir().join("tt-ttl-exec-cwd");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker.txt"), b"here").unwrap();
        #[cfg(unix)]
        let command = "/bin/sh -c \"test -f marker.txt\"";
        #[cfg(windows)]
        let command = "cmd.exe /c if not exist marker.txt exit 7";
        let src = format!(
            "exec '{command}' 'show' 1 '{}'\ndispstr result",
            dir.display()
        );
        assert_eq!(out(&src), "0");
    }
}
