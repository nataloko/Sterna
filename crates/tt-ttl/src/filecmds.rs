//! The file commands that take a handle, and the directory they hang off.
//!
//! Nothing here goes through [`ScriptHost`](crate::ScriptHost): the handle
//! table is the macro's own state, and reading bytes out of a file is not a
//! decision the way loading a *macro* is. Windows uses upstream's exact
//! `LockFile`/`UnlockFile` range; Unix uses its advisory whole-file lock. The
//! distinction is visible: Windows prevents overlapping I/O, while Unix keeps
//! only other well-behaved programs out.
//!
//! Two behaviours are deliberately **not** upstream's, both in `filelock`, and
//! both are called out on that command.

use std::fs::OpenOptions;
use std::time::{Duration, Instant};

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::files::path_to_bytes;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::lexer::MAX_STR_LEN;
use crate::rsv::Rsv;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn file_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::FileOpen => self.cmd_file_open(),
            Rsv::FileCreate => self.cmd_file_create(),
            Rsv::FileClose => self.cmd_file_close(),
            Rsv::FileRead => self.cmd_file_read(),
            Rsv::FileReadln => self.cmd_file_readln(),
            Rsv::FileWrite => self.cmd_file_write(false),
            Rsv::FileWriteLn => self.cmd_file_write(true),
            Rsv::FileSeek => self.cmd_file_seek(),
            Rsv::FileSeekBack => self.cmd_file_seek_back(),
            Rsv::FileMarkPtr => self.cmd_file_mark_ptr(),
            Rsv::FileStrSeek => self.cmd_file_str_seek(),
            Rsv::FileStrSeek2 => self.cmd_file_str_seek2(),
            Rsv::FileLock => self.cmd_file_lock(host),
            Rsv::FileUnLock => self.cmd_file_unlock(),
            Rsv::FileTruncate => self.cmd_file_truncate(),
            Rsv::GetDir => self.cmd_get_dir(),
            Rsv::SetDir => self.cmd_set_dir(),
            _ => return None,
        })
    }

    /// The first argument of every handle command: an integer, unvalidated.
    fn handle(&mut self) -> TtlResult<i32> {
        expr::get_int_val(&mut self.lx, &mut self.vars)
    }

    /// `fileopen <intvar> <name> <append> [<readonly>]`.
    ///
    /// Never an error: a file that cannot be opened puts **-1** in the
    /// variable and the macro carries on. That is upstream's, and the
    /// documentation goes out of its way to say that 4.102 and 4.103 got it
    /// wrong the other way and were changed back.
    fn cmd_file_open(&mut self) -> TtlResult<()> {
        let var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let append = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let readonly = if self.lx.parameter_given() {
            expr::get_int_val(&mut self.lx, &mut self.vars)? != 0
        } else {
            false
        };
        if name.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.end_of_line()?;

        let fhi = (|| {
            let path = self.files.abs_path(&name)?;
            let mut opts = OpenOptions::new();
            if readonly {
                // `OPEN_EXISTING` — read-only does not create.
                opts.read(true);
            } else {
                // `OPEN_ALWAYS` — open, or create and then open.
                opts.read(true).write(true).create(true);
            }
            let file = opts.open(path).ok()?;
            Some(self.files.put(file))
        })()
        .unwrap_or(-1);

        self.vars.set_int(var, fhi);
        if fhi >= 0 && append != 0 {
            self.files.seek(fhi, 0, 2);
        }
        Ok(())
    }

    /// `filecreate <intvar> <name>` — truncate or make, and open.
    ///
    /// `result` has three values and they are not a scale: 0 created, 2 could
    /// not open, -1 the path made no sense. A full handle table is *not* one
    /// of them — upstream sets `result` before it calls `HandlePut`, so a
    /// seventeenth `filecreate` reports success and hands back -1.
    fn cmd_file_create(&mut self) -> TtlResult<()> {
        let var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if name.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.end_of_line()?;

        let Some(path) = self.files.abs_path(&name) else {
            self.vars.set_int(var, -1);
            self.set_result(-1);
            return Ok(());
        };
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path);
        let fhi = match opened {
            Ok(file) => {
                self.set_result(0);
                self.files.put(file)
            }
            Err(_) => {
                self.set_result(2);
                -1
            }
        };
        self.vars.set_int(var, fhi);
        Ok(())
    }

    fn cmd_file_close(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        self.end_of_line()?;
        self.files.close(fhi);
        Ok(())
    }

    /// `fileread <handle> <count> <strvar>` — `result` is 1 if it hit the end.
    ///
    /// The count is bounded at both ends and a bad one is a *syntax* error,
    /// checked after the arguments are parsed rather than while they are.
    fn cmd_file_read(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        let count = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        if count < 1 || count > MAX_STR_LEN as i32 - 1 {
            return Err(TtlError::Syntax);
        }

        let mut out = Vec::with_capacity(count as usize);
        let mut eof = false;
        for _ in 0..count {
            match self.files.read_byte(fhi) {
                Some(b) => out.push(b),
                None => {
                    eof = true;
                    break;
                }
            }
        }
        self.set_result(i32::from(eof));
        self.vars.set_str(var, &out);
        Ok(())
    }

    /// `filereadln <handle> <strvar>`.
    ///
    /// A line ends at LF, or at CR — and a CR takes the LF after it only if
    /// that is what follows, seeking back one byte when it is not. `result` is
    /// 1 only when the read found *nothing at all*, so the last line of a file
    /// with no trailing newline reports 0 and the read after it reports 1.
    fn cmd_file_readln(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        let mut out = Vec::new();
        let mut eof = true;
        while let Some(b) = self.files.read_byte(fhi) {
            eof = false;
            match b {
                0x0d => {
                    match self.files.read_byte(fhi) {
                        Some(0x0a) | None => {}
                        Some(_) => {
                            self.files.seek(fhi, -1, 1);
                        }
                    }
                    break;
                }
                0x0a => break,
                _ => {
                    if out.len() < MAX_STR_LEN - 1 {
                        out.push(b);
                    }
                }
            }
        }
        self.set_result(i32::from(eof));
        self.vars.set_str(var, &out);
        Ok(())
    }

    /// `filewrite` / `filewriteln`.
    ///
    /// The argument is a string, or — if reading it as one is a *type*
    /// mismatch rather than a syntax error — an integer whose low byte is
    /// written. Upstream backs the line pointer up and parses the whole
    /// argument a second time to find out; so does this, because the
    /// distinction is which error the first attempt gave.
    fn cmd_file_write(&mut self, add_crlf: bool) -> TtlResult<()> {
        let fhi = self.handle()?;
        let p = self.lx.ptr;
        let bytes = match expr::get_str_val(&mut self.lx, &mut self.vars) {
            Ok(s) => s,
            Err(TtlError::TypeMismatch) => {
                self.lx.ptr = p;
                let v = expr::get_int_val(&mut self.lx, &mut self.vars)?;
                vec![v as u8]
            }
            Err(e) => return Err(e),
        };
        self.end_of_line()?;
        self.files.write(fhi, &bytes);
        if add_crlf {
            self.files.write(fhi, b"\r\n");
        }
        Ok(())
    }

    fn cmd_file_seek(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        let offset = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let origin = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        self.files.seek(fhi, offset as i64, origin);
        Ok(())
    }

    fn cmd_file_seek_back(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        self.end_of_line()?;
        self.files.seek_back(fhi);
        Ok(())
    }

    fn cmd_file_mark_ptr(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        self.end_of_line()?;
        self.files.mark(fhi);
        Ok(())
    }

    /// `filestrseek <handle> <string>` — forwards from where the pointer is.
    ///
    /// On a hit the pointer is left just past the match and `result` is 1; on
    /// a miss it goes back where it started and `result` is 0.
    ///
    /// **The matcher is not the one `wait` uses**, and the difference is a
    /// bug. `wait` backs off to the longest prefix of the pattern that is also
    /// a suffix of what it has seen; this one backs off to *nothing*, then
    /// takes the current byte as a possible first character. So it cannot find
    /// a pattern whose own prefix overlaps it: `aab` is not found in `aaab`,
    /// though it is plainly there. Reproduced, because a script that has been
    /// searching its logs successfully for twenty years is searching them the
    /// way this searches them.
    fn cmd_file_str_seek(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        let pat = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if pat.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.end_of_line()?;

        let Some(start) = self.files.tell(fhi) else {
            return Ok(());
        };
        let mut i = 0usize;
        while i != pat.len() {
            let Some(b) = self.files.read_byte(fhi) else {
                break;
            };
            if b == pat[i] {
                i += 1;
            } else if i > 0 {
                i = usize::from(b == pat[0]);
            }
        }
        if i == pat.len() {
            self.set_result(1);
        } else {
            self.set_result(0);
            self.files.seek(fhi, start as i64, 0);
        }
        Ok(())
    }

    /// `filestrseek2 <handle> <string>` — backwards from where the pointer is.
    ///
    /// Reads a byte and then seeks back two, so it walks the file in reverse
    /// one byte at a time and compares against the pattern in reverse too. On
    /// a hit the pointer is left just *before* the match; when the match runs
    /// into the start of the file the seek underflows and upstream puts the
    /// pointer at zero rather than leaving it wherever a failed seek left it.
    fn cmd_file_str_seek2(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        let pat = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if pat.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.end_of_line()?;

        let Some(start) = self.files.tell(fhi) else {
            return Ok(());
        };
        let mut i = 0usize;
        // Upstream's `pos2`, and the -1 is not a sentinel of ours: a failed
        // `llseek` answers `INVALID_SET_FILE_POINTER`, which lands in a
        // `long int` as -1 and is compared back against the macro as -1. It
        // matters twice — it makes `Last` true on the next turn, and it is
        // what the hit path tests to decide whether to park the pointer at
        // zero — so it has to be the *last* seek's answer and not a flag that
        // stays set.
        let mut pos = start as i64;
        loop {
            let last = pos <= 0;
            let read = self.files.read_byte(fhi);
            pos = match self.files.seek(fhi, -2, 1) {
                Some(p) => p as i64,
                None => -1,
            };
            if let Some(b) = read {
                if b == pat[pat.len() - 1 - i] {
                    i += 1;
                } else if i > 0 {
                    i = usize::from(b == pat[pat.len() - 1]);
                }
            }
            if last || i == pat.len() {
                break;
            }
        }
        if i == pat.len() {
            if pos < 0 {
                self.files.seek(fhi, 0, 0);
            }
            self.set_result(1);
        } else {
            self.set_result(0);
            self.files.seek(fhi, start as i64, 0);
        }
        Ok(())
    }

    /// `filelock <handle> [<timeout>]` — `result` 0 locked, 1 not.
    ///
    /// **Two divergences, and this is the one command in the port that has
    /// any.** Upstream's timeout arithmetic is broken twice over
    /// (`ttl.cpp:1586`): `timeoutI` is never initialised, so a bare
    /// `filelock fh` — the form the documentation calls the way to wait for
    /// ever — spins for however long a stale stack slot says; and the loop
    /// then compares against `timeout * 1000` having already multiplied by
    /// 1000 when it assigned it, so `filelock fh 5` waits five *million*
    /// seconds rather than five. The line above both of them reads
    /// `timeout = -1;  // infinite`, which is the intent, and the
    /// documentation's table agrees with the intent.
    ///
    /// So: no argument waits for ever, 0 returns at once, and N waits N
    /// seconds. Reproducing an uninitialised read is not possible in safe Rust
    /// and there is nothing to be faithful *to*; the 1000× is the same
    /// expression and goes with it.
    ///
    /// Upstream also omits the end-of-line check here that every sibling has,
    /// so `filelock 1 2 junk` is accepted. That one *is* reproduced: it costs
    /// nothing and a script may lean on it.
    fn cmd_file_lock(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let fhi = self.handle()?;
        let timeout = if self.lx.parameter_given() {
            Some(expr::get_int_val(&mut self.lx, &mut self.vars)?.max(0) as u64)
        } else {
            None
        };

        let deadline = timeout.map(|s| Instant::now() + Duration::from_secs(s));
        let mut locked = false;
        loop {
            if self.files.try_lock(fhi) {
                locked = true;
                break;
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                break;
            }
            host.sleep(Duration::from_millis(1));
        }
        self.set_result(i32::from(!locked));
        Ok(())
    }

    fn cmd_file_unlock(&mut self) -> TtlResult<()> {
        let fhi = self.handle()?;
        self.end_of_line()?;
        let ok = self.files.unlock(fhi);
        self.set_result(i32::from(!ok));
        Ok(())
    }

    /// `filetruncate <name> <size>` — `result` 0 done, -1 not.
    ///
    /// It creates the file if it is missing, and growing one leaves the new
    /// bytes undefined on Windows and zero here; upstream documents the
    /// contents as undefined, so both are within it.
    ///
    /// The size argument is optional to the *parser* and required in fact: a
    /// `filetruncate 'f'` is `ErrSyntax`, reached by the same `goto end` that
    /// sets `result` to -1 on the way past.
    fn cmd_file_truncate(&mut self) -> TtlResult<()> {
        let name = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if name.is_empty() {
            return Err(TtlError::Syntax);
        }
        if !self.lx.parameter_given() {
            self.set_result(-1);
            return Err(TtlError::Syntax);
        }
        let size = expr::get_int_val(&mut self.lx, &mut self.vars)?;

        let done = (|| {
            let path = self.files.abs_path(&name)?;
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .ok()?;
            f.set_len(size.max(0) as u64).ok()
        })()
        .is_some();
        self.set_result(if done { 0 } else { -1 });
        Ok(())
    }

    fn cmd_get_dir(&mut self) -> TtlResult<()> {
        let var = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        let dir = path_to_bytes(self.files.cur_dir());
        self.vars.set_str(var, &dir);
        Ok(())
    }

    /// `setdir <dir>`. A directory that does not exist is not an error and
    /// leaves the current one where it was.
    fn cmd_set_dir(&mut self) -> TtlResult<()> {
        let dir = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        self.files.set_dir(&dir);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;
    use std::io::Read;

    /// Run a macro with the current directory set to a scratch tree.
    ///
    /// The macro is named by an absolute path inside it, which is what makes
    /// `Files::new` seed `CurrentDir` there — the same rule `TTLStart` uses.
    fn run_in(dir: &std::path::Path, src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        let name = dir.join("t.ttl").to_string_lossy().into_owned();
        let mut it = Interp::new(name, src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tt-ttl-files-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(p: &std::path::Path) -> Vec<u8> {
        let mut v = Vec::new();
        std::fs::File::open(p).unwrap().read_to_end(&mut v).unwrap();
        v
    }

    #[test]
    fn a_file_is_written_and_read_back_a_line_at_a_time() {
        let d = scratch("rw");
        let h = run_in(
            &d,
            "filecreate fh 'a.txt'\n\
             filewriteln fh 'one'\n\
             filewriteln fh 'two'\n\
             fileclose fh\n\
             fileopen fh 'a.txt' 0\n\
             filereadln fh s\ndispstr s'|'\n\
             filereadln fh s\ndispstr s'|'\n\
             filereadln fh s\ndispstr result'|'s\n\
             fileclose fh",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "one|two|1|");
        assert_eq!(read(&d.join("a.txt")), b"one\r\ntwo\r\n");
    }

    #[test]
    fn the_last_line_without_a_newline_still_reports_zero() {
        let d = scratch("tail");
        std::fs::write(d.join("a.txt"), b"one\ntwo").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 0\n\
             filereadln fh s\n\
             filereadln fh s\ndispstr result'|'s'|'\n\
             filereadln fh s\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "0|two|1");
    }

    #[test]
    fn a_bare_cr_ends_a_line_and_the_byte_after_it_is_not_eaten() {
        let d = scratch("cr");
        std::fs::write(d.join("a.txt"), b"one\rtwo\r\nx").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 0\n\
             filereadln fh s\ndispstr s'|'\n\
             filereadln fh s\ndispstr s'|'\n\
             filereadln fh s\ndispstr s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "one|two|x");
    }

    #[test]
    fn fileopen_answers_minus_one_rather_than_failing() {
        let d = scratch("missing");
        let h = run_in(&d, "fileopen fh 'nope.txt' 0 1\ndispstr fh");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"-1", "read-only does not create");

        // Without the read-only flag the same name is created.
        let h = run_in(&d, "fileopen fh 'nope.txt' 0\ndispstr fh");
        assert_eq!(h.output, b"0");
        assert!(d.join("nope.txt").exists());
    }

    #[test]
    fn append_puts_the_pointer_at_the_end() {
        let d = scratch("append");
        std::fs::write(d.join("a.txt"), b"old").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 1\nfilewrite fh 'new'\nfileclose fh",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(read(&d.join("a.txt")), b"oldnew");
    }

    #[test]
    fn filewrite_takes_an_integers_low_byte() {
        let d = scratch("byte");
        let h = run_in(
            &d,
            "filecreate fh 'b.bin'\nfilewrite fh 'A'\nfilewrite fh 0\nfilewrite fh $141\nfileclose fh",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(read(&d.join("b.bin")), b"A\x00\x41");
    }

    #[test]
    fn fileread_takes_a_count_and_reports_the_end() {
        let d = scratch("read");
        std::fs::write(d.join("a.bin"), b"abcde").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.bin' 0\n\
             fileread fh 3 s\ndispstr result s '|'\n\
             fileread fh 3 s\ndispstr result s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "0abc|1de");
    }

    #[test]
    fn a_read_count_outside_one_to_511_is_a_syntax_error() {
        let d = scratch("count");
        std::fs::write(d.join("a.bin"), b"abc").unwrap();
        for n in ["0", "512"] {
            let h = run_in(&d, &format!("fileopen fh 'a.bin' 0\nfileread fh {n} s"));
            assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::Syntax), "{n}");
        }
    }

    #[test]
    fn seek_mark_and_seekback_move_the_pointer() {
        let d = scratch("seek");
        std::fs::write(d.join("a.bin"), b"0123456789").unwrap();
        // `fileseek fh (-3) 2` needs its brackets: without them the first
        // argument is the whole expression `fh - 3`, the origin becomes the
        // offset and the command runs out of parameters.
        let h = run_in(
            &d,
            "fileopen fh 'a.bin' 0\n\
             fileseek fh 4 0\nfilemarkptr fh\n\
             fileread fh 2 s\ndispstr s'|'\n\
             fileseek fh (-3) 2\nfileread fh 3 s\ndispstr s'|'\n\
             fileseekback fh\nfileread fh 2 s\ndispstr s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "45|789|45");
    }

    #[test]
    fn seekback_with_no_mark_goes_to_the_start() {
        let d = scratch("nomark");
        std::fs::write(d.join("a.bin"), b"abcdef").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.bin' 0\nfileseek fh 3 0\nfileseekback fh\nfileread fh 2 s\ndispstr s",
        );
        assert_eq!(h.output, b"ab");
    }

    #[test]
    fn filestrseek_leaves_the_pointer_past_the_match_or_where_it_was() {
        let d = scratch("strseek");
        std::fs::write(d.join("a.txt"), b"hello abc world").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 0\n\
             filestrseek fh 'abc'\ndispstr result\n\
             filereadln fh s\ndispstr '|'s'|'\n\
             fileseek fh 0 0\n\
             filestrseek fh 'zzz'\ndispstr result\n\
             fileread fh 5 s\ndispstr '|'s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "1| world|0|hello");
    }

    #[test]
    fn filestrseek_cannot_find_a_pattern_that_overlaps_itself() {
        // Upstream's back-off, reproduced. `aab` is in `aaab` at offset 1 and
        // this search does not find it; `wait`'s matcher would.
        let d = scratch("overlap");
        std::fs::write(d.join("a.txt"), b"aaab").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 0\nfilestrseek fh 'aab'\ndispstr result",
        );
        assert_eq!(h.output, b"0");
    }

    #[test]
    fn filestrseek2_searches_backwards_and_stops_just_before_the_match() {
        let d = scratch("strseek2");
        std::fs::write(d.join("a.txt"), b"one abc two abc three").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 1\n\
             filestrseek2 fh 'abc'\ndispstr result'|'\n\
             fileread fh 9 s\ndispstr s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "1| abc thre");
    }

    #[test]
    fn filestrseek2_finds_a_match_at_the_very_start_and_parks_at_zero() {
        let d = scratch("strseek2-0");
        std::fs::write(d.join("a.txt"), b"abcdef").unwrap();
        let h = run_in(
            &d,
            "fileopen fh 'a.txt' 1\nfilestrseek2 fh 'abc'\ndispstr result'|'\nfileread fh 3 s\ndispstr s",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "1|abc");
    }

    #[test]
    fn filetruncate_makes_a_file_the_size_it_is_told() {
        let d = scratch("trunc");
        std::fs::write(d.join("a.bin"), b"0123456789").unwrap();
        let h = run_in(&d, "filetruncate 'a.bin' 4\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"0");
        assert_eq!(read(&d.join("a.bin")), b"0123");

        // No size at all is a syntax error, and `result` is -1 on the way out.
        let h = run_in(&d, "filetruncate 'a.bin'");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::Syntax));
    }

    #[test]
    fn getdir_starts_at_the_macros_own_directory_and_setdir_moves_it() {
        let d = scratch("dir");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        let h = run_in(&d, "getdir s\ndispstr s");
        let seen = String::from_utf8_lossy(&h.output).into_owned();
        assert_eq!(
            std::path::Path::new(&seen).canonicalize().unwrap(),
            d.canonicalize().unwrap()
        );

        // A relative setdir resolves against where it already is, and a file
        // opened afterwards lands in the new directory.
        let h = run_in(&d, "setdir 'sub'\nfilecreate fh 'x.txt'\nfileclose fh");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert!(d.join("sub/x.txt").exists());
    }

    #[test]
    fn a_setdir_that_goes_nowhere_leaves_the_directory_alone() {
        let d = scratch("nodir");
        let h = run_in(
            &d,
            "setdir 'no-such-place'\nfilecreate fh 'y.txt'\nfileclose fh",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert!(d.join("y.txt").exists());
    }

    #[test]
    fn the_seventeenth_handle_is_minus_one_and_nothing_breaks() {
        let d = scratch("handles");
        let mut src = String::new();
        for i in 0..17 {
            src += &format!("fileopen fh 'f{i}.txt' 0\ndispstr fh' '\n");
        }
        let h = run_in(&d, &src);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        let got = String::from_utf8_lossy(&h.output).into_owned();
        assert!(got.ends_with("15 -1 "), "{got}");
    }

    #[test]
    fn an_out_of_range_handle_reads_as_end_of_file_and_writes_nowhere() {
        let d = scratch("badhandle");
        let h = run_in(
            &d,
            "fileread 99 3 s\ndispstr result'|'s'|'\n\
             filewrite 99 'x'\nfileclose 99\nfileseekback 99\nfilemarkptr 99\n\
             fileunlock 99\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(String::from_utf8_lossy(&h.output), "1||1");
    }

    #[test]
    fn filelock_returns_at_once_when_it_is_told_to() {
        let d = scratch("lock");
        let h = run_in(
            &d,
            "filecreate fh 'l.txt'\nfilelock fh 0\ndispstr result\nfileunlock fh\ndispstr result",
        );
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"00");
    }
}
