//! The sixteen file handles, and the directory relative paths hang off.
//!
//! Upstream this is four file-scope arrays in `ttl.cpp` (`FHandle`, `FPointer`
//! and their `Handle*` accessors, `ttl.cpp:95-142`) plus `CurrentDir` in
//! `ttmlib.c`. It is the macro's own state rather than the terminal's, so it
//! stays in the interpreter and does not go through [`ScriptHost`] — unlike
//! `include`, which is the host's because loading a *macro* means sniffing a
//! BOM and falling back to a codepage, and that is a decision with real files
//! behind it. `fileread` has no such decision: it reads bytes.
//!
//! [`ScriptHost`]: crate::ScriptHost

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// `NumFHandle` (`ttl.cpp:95`). Sixteen, and the seventeenth `fileopen`
/// answers -1.
pub const NUM_FHANDLE: usize = 16;

/// One open file, and the mark `filemarkptr` left in it.
#[derive(Debug)]
struct Slot {
    file: File,
    /// `FPointer[i]` — zeroed when the handle is allocated, so a
    /// `fileseekback` with no preceding `filemarkptr` goes to the start.
    mark: u64,
}

/// The handle table and the current directory.
#[derive(Debug)]
pub struct Files {
    slots: [Option<Slot>; NUM_FHANDLE],
    /// `CurrentDir`. `TTLStart` seeds it from the macro's own directory when
    /// the macro was named by an absolute path, and from the process's
    /// otherwise (`ttl.cpp:267-282`).
    cur_dir: PathBuf,
}

impl Files {
    pub fn new(macro_path: &str) -> Files {
        let dir = Path::new(macro_path);
        let cur_dir = if dir.is_absolute() {
            dir.parent().unwrap_or(Path::new("")).to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default()
        };
        Files {
            slots: Default::default(),
            cur_dir,
        }
    }

    /// `TTMGetDir`.
    pub fn cur_dir(&self) -> &Path {
        &self.cur_dir
    }

    /// `TTMSetDir` — move the macro's directory, resolving a relative argument
    /// against where it already is.
    ///
    /// Upstream does it by `SetCurrentDirectory` twice and reading the answer
    /// back, which both resolves the relative path and canonicalises the
    /// result; a directory that does not exist leaves `CurrentDir` alone,
    /// because the second `SetCurrentDirectory` fails and the read-back
    /// returns the first. Reproduced without touching the process's own
    /// directory, which upstream can only get away with because it restores it
    /// three lines later and has no second thread to race.
    pub fn set_dir(&mut self, dir: &[u8]) {
        let Some(p) = bytes_to_path(dir) else { return };
        let joined = self.cur_dir.join(p);
        if let Ok(c) = joined.canonicalize() {
            if c.is_dir() {
                self.cur_dir = c;
            }
        }
    }

    /// `GetAbsPath` (`ttmlib.c:176`) — resolve against the current directory.
    ///
    /// `None` is upstream's `FALSE`, which every caller treats as "the
    /// operation quietly did not happen". Here it means a path that is not a
    /// path: empty, or bytes that are not valid UTF-8 on a platform whose
    /// paths are.
    pub fn abs_path(&self, name: &[u8]) -> Option<PathBuf> {
        let p = bytes_to_path(name)?;
        Some(self.cur_dir.join(p))
    }

    /// `HandlePut` — the lowest free slot, or -1 when all sixteen are taken.
    pub fn put(&mut self, file: File) -> i32 {
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.is_none() {
                *s = Some(Slot { file, mark: 0 });
                return i as i32;
            }
        }
        -1
    }

    /// `HandleGet`. Out of range is `None`, which behaves throughout as
    /// upstream's `INVALID_HANDLE_VALUE` does: every operation on it quietly
    /// fails, and a read reports end of file.
    ///
    /// **Upstream's bounds are wrong in two places and neither is reproduced.**
    /// `HandleGet` tests `_countof(FHandle) < fhi` where it means `<=`, so
    /// handle 16 reads one element past the array; `HandleFree` tests nothing
    /// at all, so `fileclose 99999` writes `INVALID_HANDLE_VALUE` at an index
    /// the script chose; and `FPointer[fhi]` in `filemarkptr` and
    /// `fileseekback` is unchecked the same way. All three are out-of-bounds
    /// accesses with no result a macro can observe, which is the same reason
    /// the three in `strtrim`, `strsplit` and `GetFactor` are not reproduced
    /// either.
    fn slot(&mut self, fhi: i32) -> Option<&mut Slot> {
        if fhi < 0 || fhi as usize >= NUM_FHANDLE {
            return None;
        }
        self.slots[fhi as usize].as_mut()
    }

    /// `CloseHandle` + `HandleFree`. Closing a handle that is not open is not
    /// an error — `CloseHandle(INVALID_HANDLE_VALUE)` merely fails.
    pub fn close(&mut self, fhi: i32) {
        if fhi >= 0 && (fhi as usize) < NUM_FHANDLE {
            self.slots[fhi as usize] = None;
        }
    }

    /// `win16_lread` of one byte. `None` is end of file *or* a handle that was
    /// never open, which upstream also cannot tell apart: both return -1.
    pub fn read_byte(&mut self, fhi: i32) -> Option<u8> {
        let slot = self.slot(fhi)?;
        let mut b = [0u8; 1];
        match slot.file.read(&mut b) {
            Ok(1) => Some(b[0]),
            _ => None,
        }
    }

    /// `win16_lwrite`. A failed write is silent, as upstream's is.
    pub fn write(&mut self, fhi: i32, bytes: &[u8]) {
        if let Some(slot) = self.slot(fhi) {
            let _ = slot.file.write_all(bytes);
        }
    }

    /// `win16_llseek`. `origin` is `fileseek`'s: 0 start, 1 current, 2 end.
    /// Anything else is upstream's `FILE_BEGIN` default.
    pub fn seek(&mut self, fhi: i32, offset: i64, origin: i32) -> Option<u64> {
        let slot = self.slot(fhi)?;
        let from = match origin {
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => SeekFrom::Start(offset.max(0) as u64),
        };
        slot.file.seek(from).ok()
    }

    /// Where the pointer is now, without moving it.
    pub fn tell(&mut self, fhi: i32) -> Option<u64> {
        let slot = self.slot(fhi)?;
        slot.file.stream_position().ok()
    }

    /// `filemarkptr`. Upstream stores 0 when the seek fails, with a `// ?`
    /// beside it.
    pub fn mark(&mut self, fhi: i32) {
        let pos = self.tell(fhi).unwrap_or(0);
        if let Some(slot) = self.slot(fhi) {
            slot.mark = pos;
        }
    }

    /// `fileseekback` — back to the mark.
    pub fn seek_back(&mut self, fhi: i32) {
        let Some(slot) = self.slot(fhi) else { return };
        let mark = slot.mark;
        let _ = slot.file.seek(SeekFrom::Start(mark));
    }

    /// `filelock`'s `LockFile` over the whole file, once.
    pub fn try_lock(&mut self, fhi: i32) -> bool {
        match self.slot(fhi) {
            Some(slot) => try_lock(&slot.file),
            None => false,
        }
    }

    /// `fileunlock`'s `UnlockFile`.
    pub fn unlock(&mut self, fhi: i32) -> bool {
        match self.slot(fhi) {
            Some(slot) => unlock(&slot.file),
            None => false,
        }
    }
}

#[cfg(not(windows))]
fn try_lock(file: &File) -> bool {
    file.try_lock().is_ok()
}

#[cfg(not(windows))]
fn unlock(file: &File) -> bool {
    file.unlock().is_ok()
}

#[cfg(windows)]
fn try_lock(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::LockFile;

    // SAFETY: `file` owns a live handle for the duration of the call. The
    // offset and length are exactly TTLFileLock's five LockFile arguments.
    unsafe { LockFile(file.as_raw_handle(), 0, 0, u32::MAX, u32::MAX) != 0 }
}

#[cfg(windows)]
fn unlock(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFile;

    // SAFETY: as above; Windows requires the unlock range to exactly match
    // the range passed to LockFile.
    unsafe { UnlockFile(file.as_raw_handle(), 0, 0, u32::MAX, u32::MAX) != 0 }
}

/// A TTL string is bytes; a path on this platform may not be.
///
/// Empty is `None` because `GetFileNamePosU8` fails on it, which is
/// `GetAbsPath` returning `FALSE`.
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

/// A path back into a TTL string, for `getdir`.
pub fn path_to_bytes(p: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        p.to_string_lossy().into_owned().into_bytes()
    }
}
