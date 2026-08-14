//! Who has a serial port open, asked without opening it.
//!
//! A picker that lists every port and says nothing about which are taken sends
//! the user to a modal error after the fact. Tera Term answers the same
//! question from a bitmap in the shared memory its windows map
//! (`CheckCOMFlag`, `ttpcmn/ttcmn.c:73`), which knows about Tera Term and
//! nothing else; on Linux the kernel already knows the whole answer, so ask it.
//!
//! **Nothing here opens a device, and that is the constraint the design turns
//! on.** Opening a port raises DTR for as long as the descriptor lives and
//! drops it on close — measured on the FTDI rig, where `ttyUSB0`'s DTR is
//! wired to `ttyUSB1`'s DSR — so a picker that probed by opening would reset
//! an Arduino-style board and drop a modem's carrier every time somebody
//! opened the dropdown. `flock`-style probes are out for the same reason: they
//! need a descriptor first.
//!
//! Two sources, because neither sees everything:
//!
//! * **`/proc/locks`** is world-readable and lists one line per lock, so it
//!   names holders belonging to *any* user — including root's. Every port this
//!   program opens is in there, since `serialport-rs` takes an exclusive
//!   `flock` as well as `TIOCEXCL` (`posix/tty.rs:131`), and so is anything
//!   built on pyserial. It cannot see a holder that took no lock.
//! * **`/proc/<pid>/fd`** sees every descriptor of every process this user may
//!   read, lock or no lock — a stray `cat`, a `screen`. It cannot see another
//!   user's processes, so a root-owned holder that took no lock is invisible
//!   to both sources; the open then fails with [`crate::Error::Busy`], which
//!   is the honest fallback and the reason this is advice rather than a gate.
//!
//! The two are unioned. Matching is by device identity rather than by name:
//! `/dev/ttyUSB0`, `/dev/serial/by-path/…` and `/dev/serial/by-id/…` are three
//! names for one node, and the picker stores the `by-path` one, so a
//! string comparison would answer "free" for the port it is looking at.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// What has a device open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    /// `/proc/<pid>/comm` — the program's own name, `minicom`, `sterna`. Empty
    /// when the process left between being seen and being named.
    pub program: String,
}

/// One answer per path, in the order given; `None` where nothing visible holds
/// it.
///
/// The cost is one walk whatever the number of paths, so ask about all of them
/// at once. It is a few milliseconds on a desktop with six hundred processes —
/// fine for a dropdown opening, and deliberately never on the connect path.
pub fn holders(paths: &[&str]) -> Vec<Option<Holder>> {
    holders_under(Path::new("/proc"), paths)
}

/// [`holders`] against a `/proc` somewhere else, which is how it is tested
/// without root and without hardware — the same shape as
/// [`SshConfig::from_files`](crate::ssh::SshConfig::from_files) beside
/// `user_default`.
///
/// Linux only. Nothing else has `/proc`: macOS would need the work `lsof` does
/// through `proc_pidinfo`, and Windows has no non-destructive question to ask
/// at all, which is why the claims a window publishes for its own ports are a
/// separate mechanism rather than a fallback for this one.
pub fn holders_under(proc_root: &Path, paths: &[&str]) -> Vec<Option<Holder>> {
    let mut out = vec![None; paths.len()];
    if !cfg!(target_os = "linux") {
        return out;
    }

    // The identity of each device, twice over: `/proc/locks` names the inode
    // the lock is on, an open descriptor names the character device it reaches.
    let ids: Vec<Option<DeviceId>> = paths.iter().map(|p| DeviceId::of(Path::new(p))).collect();
    if ids.iter().all(Option::is_none) {
        return out;
    }

    let mut found: HashMap<u32, u32> = HashMap::new(); // index -> pid
    locked(proc_root, &ids, &mut found);
    opened(proc_root, &ids, &mut found);

    for (i, slot) in out.iter_mut().enumerate() {
        if let Some(pid) = found.get(&(i as u32)) {
            *slot = Some(Holder {
                pid: *pid,
                program: program_name(proc_root, *pid),
            });
        }
    }
    out
}

/// The two ways one device node is identified in `/proc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceId {
    /// The filesystem the node lives on, and its inode there — what
    /// `/proc/locks` prints.
    dev: u64,
    ino: u64,
    /// The character device the node *is* — what a descriptor stats as, and
    /// what makes every name for one port compare equal.
    rdev: u64,
}

impl DeviceId {
    fn of(path: &Path) -> Option<DeviceId> {
        // `metadata` follows symlinks, which is the point: the picker's
        // `by-path` name has to answer as the node it points at.
        let md = fs::metadata(path).ok()?;
        Some(DeviceId::of_metadata(&md))
    }

    fn of_metadata(md: &fs::Metadata) -> DeviceId {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            DeviceId {
                dev: md.dev(),
                ino: md.ino(),
                rdev: md.rdev(),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = md;
            DeviceId {
                dev: 0,
                ino: 0,
                rdev: 0,
            }
        }
    }
}

/// `/proc/locks`, which is the only source that can see another user's holder.
///
/// ```text
/// 1: FLOCK  ADVISORY  WRITE 4321 00:07:1543 0 EOF
///                           ^pid ^dev:inode
/// ```
///
/// The device is major and minor in hexadecimal and the inode in decimal.
/// POSIX locks are read too: a program is free to use either, and a lock of
/// any kind on a serial node means somebody is on that line.
fn locked(proc_root: &Path, ids: &[Option<DeviceId>], found: &mut HashMap<u32, u32>) {
    let Ok(text) = fs::read_to_string(proc_root.join("locks")) else {
        return;
    };
    for line in text.lines() {
        let Some((pid, dev, ino)) = parse_lock(line) else {
            continue;
        };
        for (i, id) in ids.iter().enumerate() {
            if id.is_some_and(|id| id.dev == dev && id.ino == ino) {
                found.entry(i as u32).or_insert(pid);
            }
        }
    }
}

/// One `/proc/locks` line: the pid, and the `dev`/`inode` it is held on.
///
/// A line whose fields are missing or unparseable is skipped rather than
/// guessed at — `->` continuation lines for blocked waiters are shaped
/// differently, and a kernel is free to add a lock type this does not know.
fn parse_lock(line: &str) -> Option<(u32, u64, u64)> {
    let mut fields = line.split_whitespace();
    // `1:` or, for a waiter, `1: ->`. The offsets below are counted from the
    // lock type, so drop the leading number and any arrow.
    let mut field = fields.next()?;
    if !field.ends_with(':') {
        return None;
    }
    field = fields.next()?;
    if field == "->" {
        field = fields.next()?;
    }
    // field: the lock type (FLOCK/POSIX/OFDLCK/…), then ADVISORY/MANDATORY,
    // then READ/WRITE, then the pid, then dev:dev:inode.
    let _kind = field;
    let _mode = fields.next()?;
    let _access = fields.next()?;
    let pid: i64 = fields.next()?.parse().ok()?;
    let mut who = fields.next()?.split(':');
    let major = u64::from_str_radix(who.next()?, 16).ok()?;
    let minor = u64::from_str_radix(who.next()?, 16).ok()?;
    let ino: u64 = who.next()?.parse().ok()?;
    // A lock with no owning process — an OFD lock can print `-1` — is a real
    // lock but names nobody, so report it against pid 0 rather than dropping
    // it: the port is still taken.
    let pid = u32::try_from(pid).unwrap_or(0);
    Some((pid, makedev(major, minor), ino))
}

/// `makedev(3)`'s encoding, which is what `stat` reports in `st_dev`.
fn makedev(major: u64, minor: u64) -> u64 {
    ((major & 0xfff) << 8) | (minor & 0xff) | ((major & !0xfff) << 32) | ((minor & !0xff) << 12)
}

/// Every descriptor of every process this user may read.
///
/// An unreadable `/proc/<pid>/fd` is another user's process and is skipped in
/// silence: on an ordinary desktop that is two thirds of them, and it is not an
/// error, it is the permission model. `stat` on the entry follows to the device
/// rather than reading the link, so a descriptor opened through any of the
/// node's names matches.
fn opened(proc_root: &Path, ids: &[Option<DeviceId>], found: &mut HashMap<u32, u32>) {
    let Ok(entries) = fs::read_dir(proc_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(md) = fs::metadata(fd.path()) else {
                continue;
            };
            let id = DeviceId::of_metadata(&md);
            for (i, want) in ids.iter().enumerate() {
                if want.is_some_and(|want| want.rdev == id.rdev) {
                    found.entry(i as u32).or_insert(pid);
                }
            }
        }
    }
}

/// `/proc/<pid>/comm`, which is world-readable — so a holder this process
/// could not have found on its own can still be named once `/proc/locks` has
/// pointed at it.
fn program_name(proc_root: &Path, pid: u32) -> String {
    fs::read_to_string(proc_root.join(pid.to_string()).join("comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch `/proc`, the shape `ssh::config`'s tests use.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("tt-inuse-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn write(&self, name: &str, body: &str) {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }

        /// A process holding `device` on descriptor 3.
        #[cfg(unix)]
        fn holder(&self, pid: u32, program: &str, device: &str) {
            let fd = self.0.join(pid.to_string()).join("fd");
            fs::create_dir_all(&fd).unwrap();
            std::os::unix::fs::symlink(device, fd.join("3")).unwrap();
            self.write(&format!("{pid}/comm"), &format!("{program}\n"));
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_lock_line_names_its_pid_and_its_inode() {
        assert_eq!(
            parse_lock("1: FLOCK  ADVISORY  WRITE 4321 00:07:1543 0 EOF"),
            Some((4321, makedev(0, 7), 1543))
        );
        // POSIX locks count too, and the byte range is not this module's
        // business.
        assert_eq!(
            parse_lock("2: POSIX  ADVISORY  WRITE 120435 00:1d:7651 0 0"),
            Some((120435, makedev(0, 0x1d), 7651))
        );
        // A blocked waiter is printed with an arrow before the type.
        assert_eq!(
            parse_lock("3: -> POSIX  ADVISORY  WRITE 99 00:07:1543 0 EOF"),
            Some((99, makedev(0, 7), 1543))
        );
        // An OFD lock owned by no process is still a held lock.
        assert_eq!(
            parse_lock("4: OFDLCK ADVISORY  READ  -1 00:07:1543 0 EOF"),
            Some((0, makedev(0, 7), 1543))
        );
        // Junk is skipped rather than guessed at.
        assert_eq!(parse_lock(""), None);
        assert_eq!(parse_lock("nonsense"), None);
        assert_eq!(parse_lock("5: FLOCK ADVISORY WRITE notapid 00:07:1"), None);
    }

    /// The descriptor sweep, against a `/proc` built for the purpose. `/dev/null`
    /// stands in for the port: it is a real device node on every machine, so
    /// the `st_rdev` comparison is the real one.
    #[test]
    #[cfg(unix)]
    fn an_open_descriptor_names_the_program_holding_it() {
        let scratch = Scratch::new("fd");
        scratch.holder(1234, "minicom", "/dev/null");

        let answer = holders_under(&scratch.0, &["/dev/null", "/dev/zero"]);
        if !cfg!(target_os = "linux") {
            assert_eq!(answer, vec![None, None], "only Linux has /proc");
            return;
        }
        assert_eq!(
            answer[0],
            Some(Holder {
                pid: 1234,
                program: "minicom".into()
            })
        );
        assert_eq!(answer[1], None, "nothing holds /dev/zero");
    }

    /// The lock table is the source that reaches another user's processes, so
    /// it must work with no readable `fd` directory at all.
    #[test]
    #[cfg(unix)]
    fn a_lock_is_enough_on_its_own() {
        let scratch = Scratch::new("locks");
        let md = fs::metadata("/dev/null").unwrap();
        let id = DeviceId::of_metadata(&md);
        let (major, minor) = (id.dev >> 8 & 0xfff, id.dev & 0xff);
        scratch.write(
            "locks",
            &format!(
                "1: FLOCK  ADVISORY  WRITE 4321 {major:02x}:{minor:02x}:{} 0 EOF\n",
                id.ino
            ),
        );
        scratch.write("4321/comm", "sterna\n");

        let answer = holders_under(&scratch.0, &["/dev/null"]);
        if !cfg!(target_os = "linux") {
            assert_eq!(answer, vec![None]);
            return;
        }
        assert_eq!(
            answer[0],
            Some(Holder {
                pid: 4321,
                program: "sterna".into()
            })
        );
    }

    /// A path that names nothing must not be reported as held, and must not
    /// stop the paths beside it from being answered.
    #[test]
    fn a_missing_device_is_not_in_use() {
        let answer = holders(&["/dev/tt-inuse-nonexistent"]);
        assert_eq!(answer, vec![None]);
        assert_eq!(holders(&[]).len(), 0);
    }
}
