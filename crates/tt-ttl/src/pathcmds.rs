//! The file commands that name a path rather than hold a handle, and the
//! directory walk.
//!
//! This is the half of the family where Windows shows through hardest.
//! Upstream calls `CopyFileW`, `MoveFileW`, `GetFileAttributesW` and
//! `FindFirstFileW` and reports their answers more or less unchanged, and
//! three of those have no exact Linux equivalent:
//!
//! - **File attributes are a Win32 bit field.** `getfileattr` hands
//!   `GetFileAttributes`'s return value straight to `result`, so the values a
//!   script tests against are `FILE_ATTRIBUTE_*`. What can be answered from a
//!   POSIX stat is answered; see [`Interp::cmd_get_file_attr`].
//! - **`FindFirstFile` globs, and the glob is Win32's.** `*.*` matches
//!   everything there, dot or no dot, and the walk includes `.` and `..`.
//!   Both are reproduced, because a script that loops over `*.*` was written
//!   against them.
//! - **Paths are compared case-insensitively** — `filecopy`, `filerename` and
//!   `fileconcat` each refuse to work on one file under two names, using
//!   `_stricmp`. Here the comparison is exact, because on this filesystem two
//!   spellings that differ in case are two files.
//!
//! One thing is *not* reproduced. `GetFileNamePosU8` rejects any path with a
//! `:` in it after the drive letter (`ttlib_static_cpp.cpp:741`), which makes
//! `GetAbsPath` fail and the command report a path error. A colon is a drive
//! separator and an alternate-data-stream marker on Windows and an ordinary
//! character here, so reproducing it would make `/tmp/a:b` unopenable for no
//! reason a user of this port could discover.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::files::path_to_bytes;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::rsv::Rsv;

/// `NumDirHandle` (`ttl.cpp:92`) — eight directory walks at once.
pub const NUM_DIR_HANDLE: usize = 8;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn path_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::FileDelete => self.cmd_file_delete(),
            Rsv::FileCopy => self.cmd_file_copy(),
            Rsv::FileRename => self.cmd_file_rename(),
            Rsv::FileConcat => self.cmd_file_concat(),
            Rsv::FileSearch => self.cmd_exists(false),
            Rsv::FolderSearch => self.cmd_exists(true),
            Rsv::FileStat => self.cmd_file_stat(host),
            Rsv::GetFileAttr => self.cmd_get_file_attr(),
            Rsv::SetFileAttr => self.cmd_set_file_attr(),
            Rsv::FolderCreate => self.cmd_folder(true),
            Rsv::FolderDelete => self.cmd_folder(false),
            Rsv::FindFirst => self.cmd_find_first(),
            Rsv::FindNext => self.cmd_find_next(),
            Rsv::FindClose => self.cmd_find_close(),
            Rsv::Basename => self.cmd_base_or_dir_name(true),
            Rsv::Dirname => self.cmd_base_or_dir_name(false),
            Rsv::MakePath => self.cmd_make_path(),
            _ => return None,
        })
    }

    /// One filename argument, which must not be empty, and end of line.
    ///
    /// `bad` is what `result` becomes when the argument list is wrong: every
    /// command in this file sets one before it returns the error, and they are
    /// not all the same number.
    fn one_name(&mut self, bad: i32) -> TtlResult<Vec<u8>> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if name.is_empty() || self.lx.first_char() != 0 {
            self.set_result(bad);
            return Err(TtlError::Syntax);
        }
        Ok(name)
    }

    /// Two filename arguments, neither empty, and end of line.
    fn two_names(&mut self, bad: i32) -> TtlResult<(Vec<u8>, Vec<u8>)> {
        let a = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let b = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if a.is_empty() || b.is_empty() || self.lx.first_char() != 0 {
            self.set_result(bad);
            return Err(TtlError::Syntax);
        }
        Ok((a, b))
    }

    /// `filedelete <name>` — `result` 0 gone, -1 not.
    ///
    /// A path that made no sense and a delete that failed report the same -1,
    /// which is upstream's and means "it is still there" either way.
    fn cmd_file_delete(&mut self) -> TtlResult<()> {
        let name = self.one_name(1)?;
        let ok = self
            .files
            .abs_path(&name)
            .is_some_and(|p| fs::remove_file(p).is_ok());
        self.set_result(if ok { 0 } else { -1 });
        Ok(())
    }

    /// `filecopy <from> <to>` — `result` 0 copied, -1/-2 a path, -3 the same
    /// file twice, -4 the copy itself.
    fn cmd_file_copy(&mut self) -> TtlResult<()> {
        let (a, b) = self.two_names(1)?;
        let Some(src) = self.files.abs_path(&a) else {
            self.set_result(-1);
            return Ok(());
        };
        let Some(dst) = self.files.abs_path(&b) else {
            self.set_result(-2);
            return Ok(());
        };
        if src == dst {
            self.set_result(-3);
            return Ok(());
        }
        self.set_result(if fs::copy(src, dst).is_ok() { 0 } else { -4 });
        Ok(())
    }

    /// `filerename <from> <to>` — `result` 0 renamed, 2 the same name twice,
    /// -1/-2 a path, -3 the rename itself.
    ///
    /// The same-name test runs on the names **as written**, before either is
    /// resolved — the opposite order from `filecopy`, which resolves first. So
    /// `filerename 'a' './a'` renames a file onto itself where
    /// `filecopy 'a' './a'` reports -3.
    fn cmd_file_rename(&mut self) -> TtlResult<()> {
        let (a, b) = self.two_names(1)?;
        if a == b {
            self.set_result(2);
            return Ok(());
        }
        let Some(src) = self.files.abs_path(&a) else {
            self.set_result(-1);
            return Ok(());
        };
        let Some(dst) = self.files.abs_path(&b) else {
            self.set_result(-2);
            return Ok(());
        };
        self.set_result(if fs::rename(src, dst).is_ok() { 0 } else { -3 });
        Ok(())
    }

    /// `fileconcat <to> <from>` — append the second file to the first.
    ///
    /// `result` is 0 done, 2 the same file twice, 3 the destination would not
    /// open, -4 a read failed, -5 a write did, -1/-2 a path.
    ///
    /// **A source that does not exist reports success.** Upstream opens it
    /// with `OPEN_EXISTING` and, if that fails, skips the copy loop with
    /// `result` still 0 — so `fileconcat 'log' 'nothing-here'` says it worked
    /// and the destination is untouched, or created empty if it was missing.
    /// Reproduced.
    fn cmd_file_concat(&mut self) -> TtlResult<()> {
        let (a, b) = self.two_names(1)?;
        let Some(dst) = self.files.abs_path(&a) else {
            self.set_result(-1);
            return Ok(());
        };
        let Some(src) = self.files.abs_path(&b) else {
            self.set_result(-2);
            return Ok(());
        };
        if dst == src {
            self.set_result(2);
            return Ok(());
        }
        let Ok(mut out) = fs::OpenOptions::new().create(true).append(true).open(dst) else {
            self.set_result(3);
            return Ok(());
        };
        let mut code = 0;
        if let Ok(mut input) = fs::File::open(src) {
            // Upstream's buffer is thirteen bytes and its loop ends on a short
            // read; the size is arbitrary and the shape is `read until short`,
            // which is what this is.
            let mut buf = [0u8; 8192];
            loop {
                match input.read(&mut buf) {
                    Err(_) => {
                        code = -4;
                        break;
                    }
                    Ok(0) => break,
                    Ok(n) => {
                        if out.write_all(&buf[..n]).is_err() {
                            code = -5;
                            break;
                        }
                    }
                }
            }
        }
        self.set_result(code);
        Ok(())
    }

    /// `filesearch` and `foldersearch` — `result` 1 if it is there.
    ///
    /// `foldersearch` answers 0 for a file that exists, which its own
    /// documentation warns about: the pair is "does this exist" and "is this a
    /// directory", not "file" and "folder".
    fn cmd_exists(&mut self, dir_only: bool) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if name.is_empty() || self.lx.first_char() != 0 {
            return Err(TtlError::Syntax);
        }
        let found = self.files.abs_path(&name).is_some_and(|p| {
            if dir_only {
                p.is_dir()
            } else {
                p.symlink_metadata().is_ok()
            }
        });
        self.set_result(i32::from(found));
        Ok(())
    }

    /// `filestat <name> [<size>] [<mtime>] [<drive>]` — `result` 0 or -1.
    ///
    /// Every output is optional and they are taken in order, so a macro that
    /// wants the time must also take the size.
    ///
    /// The drive letter is the resolved path's first character, upper-cased if
    /// it is a letter and `?` if it is not. That is exactly upstream's rule,
    /// and on this platform it means every `filestat` answers `?` — which is
    /// the truthful answer for a filesystem with no drives.
    fn cmd_file_stat(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if name.is_empty() {
            return Err(TtlError::Syntax);
        }
        let stat = self
            .files
            .abs_path(&name)
            .and_then(|p| fs::metadata(&p).ok().map(|m| (p, m)));
        let Some((path, md)) = stat else {
            self.set_result(-1);
            return Ok(());
        };

        if self.lx.parameter_given() {
            let var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
            // Upstream narrows to `int` too, so a file over 2 GB wraps.
            self.vars.set_int(var, md.len() as i32);
        }
        if self.lx.parameter_given() {
            let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
            let secs = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let s = host.format_time(secs);
            self.vars.set_str(var, s.as_bytes());
        }
        if self.lx.parameter_given() {
            let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
            let d = path_to_bytes(&path).first().copied().unwrap_or(b'?');
            let d = if d.is_ascii_alphabetic() {
                d.to_ascii_uppercase()
            } else {
                b'?'
            };
            self.vars.set_str(var, &[d]);
        }
        self.set_result(0);
        Ok(())
    }

    /// `getfileattr <name>` — `result` is a Win32 `FILE_ATTRIBUTE_*` bit
    /// field, or -1 for a path that is not there.
    ///
    /// Three of the bits have a POSIX answer and the rest do not:
    /// `READONLY` ($1) when nothing may write the file, `DIRECTORY` ($10), and
    /// `NORMAL` ($80) when neither of those applies — which is what `NORMAL`
    /// means on Windows too, "no other attribute set". `HIDDEN` ($2) is a
    /// leading dot, which is this platform's own convention for the same
    /// thing. `SYSTEM`, `ARCHIVE`, `TEMPORARY` and the rest describe
    /// bookkeeping NTFS does and nothing here does, so they are never set.
    fn cmd_get_file_attr(&mut self) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let attr = self
            .files
            .abs_path(&name)
            .and_then(|p| fs::symlink_metadata(&p).ok().map(|m| (p, m)))
            .map(|(p, md)| {
                let mut a = 0i32;
                if md.permissions().readonly() {
                    a |= 0x1;
                }
                if p.file_name()
                    .is_some_and(|n| path_to_bytes(Path::new(n)).starts_with(b"."))
                {
                    a |= 0x2;
                }
                if md.is_dir() {
                    a |= 0x10;
                }
                if a == 0 {
                    a = 0x80;
                }
                a
            })
            .unwrap_or(-1);
        self.set_result(attr);
        Ok(())
    }

    /// `setfileattr <name> <attributes>` — `result` 1 done, 0 not.
    ///
    /// Only `READONLY` ($1) can be acted on: it clears or restores the write
    /// bits. The rest are accepted and ignored, which is the same thing
    /// `SetFileAttributes` does with a bit the filesystem underneath does not
    /// keep — and it still reports success, so a script cannot tell.
    fn cmd_set_file_attr(&mut self) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let attrs = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let ok = (|| {
            let p = self.files.abs_path(&name)?;
            let md = fs::metadata(&p).ok()?;
            let mut perm = md.permissions();
            perm.set_readonly(attrs & 0x1 != 0);
            fs::set_permissions(&p, perm).ok()
        })()
        .is_some();
        self.set_result(i32::from(ok));
        Ok(())
    }

    /// `foldercreate` / `folderdelete` — `result` 0 done, 2 not, -1 a path.
    ///
    /// Neither is recursive: `CreateDirectory` makes one level and
    /// `RemoveDirectory` refuses a directory with anything in it.
    fn cmd_folder(&mut self, create: bool) -> TtlResult<()> {
        let name = self.one_name(1)?;
        let Some(p) = self.files.abs_path(&name) else {
            self.set_result(-1);
            return Ok(());
        };
        let ok = if create {
            fs::create_dir(p).is_ok()
        } else {
            fs::remove_dir(p).is_ok()
        };
        self.set_result(if ok { 0 } else { 2 });
        Ok(())
    }

    /// `findfirst <intvar> <pattern> <strvar>`.
    ///
    /// An empty pattern becomes `*.*`, which on Win32 matches every name
    /// whether or not it has a dot — a rule left over from 8.3 filenames and
    /// the reason the default works at all. The walk also yields `.` and `..`,
    /// as `FindFirstFile` does; a script looping over `*.*` has always had to
    /// skip them.
    ///
    /// The handle is the index of a free slot, or -1 when all eight are taken
    /// or the pattern matched nothing; `result` says which, and the name
    /// variable is emptied on failure.
    fn cmd_find_first(&mut self) -> TtlResult<()> {
        let dh_var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        let mut pattern = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let name_var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if pattern.is_empty() {
            pattern = b"*.*".to_vec();
        }

        let names = self
            .files
            .abs_path(&pattern)
            .map(|p| find_matches(&p))
            .unwrap_or_default();

        let slot = if names.is_empty() {
            None
        } else {
            self.finds.iter().position(|s| s.is_none())
        };
        match slot {
            Some(i) => {
                let mut rest = names.into_iter();
                let first = rest.next().unwrap_or_default();
                self.finds[i] = Some(rest.collect());
                self.vars.set_int(dh_var, i as i32);
                self.vars.set_str(name_var, &first);
                self.set_result(1);
            }
            None => {
                self.vars.set_int(dh_var, -1);
                self.vars.set_str(name_var, b"");
                self.set_result(0);
            }
        }
        Ok(())
    }

    /// `findnext <handle> <strvar>` — `result` 1 while there is a name left.
    ///
    /// Running out does **not** free the handle; only `findclose` does, which
    /// is upstream and is why a loop that never closes runs out of the eight.
    fn cmd_find_next(&mut self) -> TtlResult<()> {
        let dh = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let next = self.find_slot(dh).and_then(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.remove(0))
            }
        });
        match next {
            Some(n) => {
                self.vars.set_str(var, &n);
                self.set_result(1);
            }
            None => {
                self.vars.set_str(var, b"");
                self.set_result(0);
            }
        }
        Ok(())
    }

    fn cmd_find_close(&mut self) -> TtlResult<()> {
        let dh = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if dh >= 0 && (dh as usize) < NUM_DIR_HANDLE {
            self.finds[dh as usize] = None;
        }
        Ok(())
    }

    fn find_slot(&mut self, dh: i32) -> Option<&mut Vec<Vec<u8>>> {
        if dh < 0 || dh as usize >= NUM_DIR_HANDLE {
            return None;
        }
        self.finds[dh as usize].as_mut()
    }

    /// `basename <strvar> <path>` / `dirname <strvar> <path>`.
    ///
    /// Upstream scans for `\` and knows about a drive letter, and its
    /// `DeleteSlash` strips only backslashes — so ported literally,
    /// `dirname '/a/b/'` and `dirname 'c:\a\b\'` would answer differently for
    /// no reason but the character. These use this platform's separator and
    /// its rules, which is what the same code compiled for it would mean. The
    /// documented examples hold with `/` for `\`, and the two places the
    /// answers differ are at the root — `dirname '/a'` is `/` where upstream's
    /// `dirname 'c:\a'` is `c:` — and on a trailing separator, where
    /// `basename '/a/'` is `a` here and empty upstream.
    fn cmd_base_or_dir_name(&mut self, base: bool) -> TtlResult<()> {
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let src = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let out = match bytes_to_path(&src) {
            None => Vec::new(),
            Some(p) => {
                let part = if base {
                    p.file_name().map(PathBuf::from)
                } else {
                    p.parent().map(PathBuf::from)
                };
                part.map(|q| path_to_bytes(&q)).unwrap_or_default()
            }
        };
        self.vars.set_str(var, &out);
        Ok(())
    }

    /// `makepath <strvar> <dir> <name>` — join with a separator if there is
    /// not one already.
    ///
    /// A literal concatenation, as upstream's `AppendSlash` plus `strcat` is,
    /// and not a path join: an absolute `<name>` is appended rather than
    /// replacing `<dir>`.
    fn cmd_make_path(&mut self) -> TtlResult<()> {
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        let mut dir = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let sep = std::path::MAIN_SEPARATOR as u8;
        if dir.last() != Some(&sep) {
            dir.push(sep);
        }
        dir.extend_from_slice(&name);
        self.vars.set_str(var, &dir);
        Ok(())
    }
}

/// The names in `pattern`'s directory that match its last component, `.` and
/// `..` included, in whatever order the filesystem gives them.
fn find_matches(pattern: &Path) -> Vec<Vec<u8>> {
    let (dir, glob) = match pattern.file_name() {
        Some(f) => (
            pattern.parent().unwrap_or(Path::new(".")).to_path_buf(),
            path_to_bytes(Path::new(f)),
        ),
        None => (pattern.to_path_buf(), b"*".to_vec()),
    };
    let mut out = Vec::new();
    for dot in [&b"."[..], &b".."[..]] {
        if wildcard_match(&glob, dot) {
            out.push(dot.to_vec());
        }
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let n = path_to_bytes(Path::new(&e.file_name()));
        if wildcard_match(&glob, &n) {
            out.push(n);
        }
    }
    out
}

/// `*` and `?`, as `FindFirstFile` reads them.
///
/// `*.*` is special-cased to "anything", which is what Win32 makes of it and
/// what every macro that walks a directory relies on. The comparison is exact
/// rather than case-insensitive, for the same reason the same-file tests are.
fn wildcard_match(pat: &[u8], name: &[u8]) -> bool {
    if pat == b"*.*" || pat == b"*" {
        return true;
    }
    // Iterative backtracking: no recursion, so a pathological pattern costs
    // time and not stack.
    let (mut p, mut n) = (0usize, 0usize);
    let (mut star, mut retry) = (None, 0usize);
    while n < name.len() {
        if p < pat.len() && (pat[p] == b'?' || pat[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pat.len() && pat[p] == b'*' {
            star = Some(p);
            retry = n;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    p == pat.len()
}

fn bytes_to_path(b: &[u8]) -> Option<PathBuf> {
    if b.is_empty() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(b)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(b).ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::wildcard_match;
    use crate::host::RecordingHost;
    use crate::interp::Interp;

    fn run_in(dir: &std::path::Path, src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        let name = dir.join("t.ttl").to_string_lossy().into_owned();
        let mut it = Interp::new(name, src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tt-ttl-paths-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_glob_is_win32s_and_star_dot_star_means_everything() {
        assert!(wildcard_match(b"*.*", b"no-dot-here"));
        assert!(wildcard_match(b"*.txt", b"a.txt"));
        assert!(!wildcard_match(b"*.txt", b"a.txtx"));
        assert!(wildcard_match(b"a?c", b"abc"));
        assert!(!wildcard_match(b"a?c", b"ac"));
        assert!(wildcard_match(b"a*b*c", b"axxbyyc"));
        assert!(!wildcard_match(b"a*b*c", b"axxbyy"));
        assert!(wildcard_match(b"*", b""));
    }

    #[test]
    fn delete_copy_and_rename_report_their_own_numbers() {
        let d = scratch("crud");
        std::fs::write(d.join("a.txt"), b"hello").unwrap();
        let h = run_in(
            &d,
            "filecopy 'a.txt' 'b.txt'\ndispstr result\n\
             filerename 'b.txt' 'c.txt'\ndispstr result\n\
             filedelete 'c.txt'\ndispstr result\n\
             filedelete 'c.txt'\ndispstr result\n\
             filecopy 'a.txt' 'a.txt'\ndispstr result\n\
             filerename 'a.txt' 'a.txt'\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "000-1-32");
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"hello");
    }

    #[test]
    fn fileconcat_appends_and_calls_a_missing_source_a_success() {
        let d = scratch("concat");
        std::fs::write(d.join("a.txt"), b"one").unwrap();
        std::fs::write(d.join("b.txt"), b"two").unwrap();
        let h = run_in(
            &d,
            "fileconcat 'a.txt' 'b.txt'\ndispstr result\n\
             fileconcat 'a.txt' 'gone.txt'\ndispstr result\n\
             fileconcat 'a.txt' 'a.txt'\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "002");
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"onetwo");
    }

    #[test]
    fn filesearch_finds_anything_and_foldersearch_only_directories() {
        let d = scratch("search");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        let h = run_in(
            &d,
            "filesearch 'a.txt'\ndispstr result\n\
             filesearch 'sub'\ndispstr result\n\
             foldersearch 'sub'\ndispstr result\n\
             foldersearch 'a.txt'\ndispstr result\n\
             filesearch 'nope'\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "11100");
    }

    #[test]
    fn folders_are_made_and_removed_one_level_at_a_time() {
        let d = scratch("folder");
        let h = run_in(
            &d,
            "foldercreate 'one'\ndispstr result\n\
             foldercreate 'two/three'\ndispstr result\n\
             folderdelete 'one'\ndispstr result\n\
             folderdelete 'one'\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            String::from_utf8_lossy(&h.output),
            "0202",
            "made, refused (two/ is missing), removed, refused (already gone)"
        );
        assert!(!d.join("one").exists());
    }

    #[test]
    fn filestat_gives_the_size_the_time_and_a_question_mark_for_the_drive() {
        let d = scratch("stat");
        std::fs::write(d.join("a.bin"), b"0123456789").unwrap();
        let h = run_in(
            &d,
            "filestat 'a.bin' size time drive\ndispstr result '|' size '|' drive '|' time\n\
             filestat 'gone' size\ndispstr '|' result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        let got = String::from_utf8_lossy(&h.output).into_owned();
        assert!(got.starts_with("0|10|?|"), "{got}");
        assert!(got.ends_with("|-1"), "{got}");
        // The time is `%Y-%m-%d %H:%M:%S` and nothing else.
        let time = &got["0|10|?|".len()..got.len() - "|-1".len()];
        assert_eq!(time.len(), 19, "{time:?}");
        assert_eq!(&time[4..5], "-");
        assert_eq!(&time[10..11], " ");
    }

    #[test]
    fn getfileattr_answers_the_win32_bits_it_can() {
        let d = scratch("attr");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        std::fs::write(d.join(".hidden"), b"x").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        let h = run_in(
            &d,
            "getfileattr 'a.txt'\ndispstr result '|'\n\
             getfileattr '.hidden'\ndispstr result '|'\n\
             getfileattr 'sub'\ndispstr result '|'\n\
             getfileattr 'gone'\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        // $80 NORMAL, $2 HIDDEN, $10 DIRECTORY, -1 missing.
        assert_eq!(String::from_utf8_lossy(&h.output), "128|2|16|-1");
    }

    #[test]
    fn setfileattr_can_do_read_only_and_says_yes_to_the_rest() {
        let d = scratch("setattr");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        let h = run_in(
            &d,
            "setfileattr 'a.txt' 1\ndispstr result\ngetfileattr 'a.txt'\ndispstr '|' result\n\
             setfileattr 'a.txt' 32\ndispstr '|' result\ngetfileattr 'a.txt'\ndispstr '|' result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "1|1|1|128");
        // Leave the tree removable. `set_readonly(false)` would make it world
        // writable, so put back exactly the owner's write bit.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = std::fs::Permissions::from_mode(0o644);
            std::fs::set_permissions(d.join("a.txt"), p).unwrap();
        }
    }

    #[test]
    fn findfirst_walks_a_pattern_and_findclose_gives_the_handle_back() {
        let d = scratch("find");
        for n in ["a.txt", "b.txt", "c.log"] {
            std::fs::write(d.join(n), b"x").unwrap();
        }
        let h = run_in(
            &d,
            "n = 0\n\
             findfirst dh '*.txt' name\n\
             while result\n  n = n + 1\n  findnext dh name\nendwhile\n\
             findclose dh\n\
             dispstr dh '|' n",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "0|2");
    }

    #[test]
    fn a_pattern_that_matches_nothing_hands_back_minus_one() {
        let d = scratch("find-none");
        let h = run_in(
            &d,
            "findfirst dh '*.nope' name\ndispstr result '|' dh '|' name",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "0|-1|");
    }

    #[test]
    fn the_walk_includes_dot_and_dot_dot_as_win32_does() {
        let d = scratch("find-dots");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        let h = run_in(
            &d,
            "n = 0\nfindfirst dh '*.*' name\n\
             while result\n  n = n + 1\n  findnext dh name\nendwhile\n\
             findclose dh\ndispstr n",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        // The macro is never written to disk — only named — so the directory
        // holds a.txt and nothing else.
        assert_eq!(h.output, b"3", "a.txt, plus . and ..");
    }

    #[test]
    fn the_ninth_walk_at_once_is_refused() {
        let d = scratch("find-handles");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        let mut src = String::new();
        for _ in 0..9 {
            src += "findfirst dh '*.txt' name\ndispstr dh ' '\n";
        }
        let h = run_in(&d, &src);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "0 1 2 3 4 5 6 7 -1 ");
    }

    #[test]
    fn basename_dirname_and_makepath_agree_with_the_documented_examples() {
        let d = scratch("names");
        let h = run_in(
            &d,
            "basename s '/teraterm/test.txt'\ndispstr s '|'\n\
             dirname s '/teraterm/test.txt'\ndispstr s '|'\n\
             dirname s '/teraterm'\ndispstr s '|'\n\
             makepath s '/teraterm' 'test.txt'\ndispstr s '|'\n\
             makepath s '/teraterm/' 'test.txt'\ndispstr s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            String::from_utf8_lossy(&h.output),
            "test.txt|/teraterm|/|/teraterm/test.txt|/teraterm/test.txt"
        );
    }
}
