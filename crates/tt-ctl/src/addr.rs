//! Where the socket is, what it is called, and how a client finds one.
//!
//! DDE had a name service. A Tera Term window registers the topic
//! `TERATERM<hwnd-in-hex>` (`ttdde.c:208`), and the only reason a macro can
//! find its window is that the window *launched* it and put the topic on its
//! command line: `TTPMACRO /D=<topic>` (`:1497`). A `ttpmacro.exe` started by
//! hand has no topic and reaches whichever Tera Term answers a wildcard
//! connect.
//!
//! There is no name service on a local byte stream. On Unix the runtime
//! directory is one; on Windows the named-pipe namespace is enumerable. Each
//! window binds an endpoint named for the process id unless `/D=` overrides
//! it — the same command line upstream uses for the same purpose, doing the
//! same job through a different mechanism.
//!
//! **The address is part of the access control.** Unix uses a `0700` runtime
//! directory, a uid-bearing `/tmp` fallback and a `0600` socket. Windows puts
//! the login-session id in the pipe name and rejects remote clients; both then
//! verify the accepted peer's uid or token SID. That is belt and braces for a
//! good reason: anything that reaches this endpoint can type at whatever the
//! window is connected to.
//!
//! **A client given no name refuses to guess between two windows**, which is
//! where this diverges from upstream. `DdeConnect` with a wildcard picks
//! whichever conversation answers first, so `ttpmacro login.ttl` with two Tera
//! Terms open logs into an arbitrary one of them. The macro that command runs
//! usually types a password; picking wrong is not an outcome worth the
//! convenience, so [`resolve`] names the candidates and fails. `STERNA_CTL`,
//! `--to` and `/D=` are the three ways to say which.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

use crate::ipc::{Listener, Stream};

/// The environment variable holding the full path of one window's socket.
///
/// Set in the environment of anything the window starts — the local shell most
/// of all, so that a script running *inside* the terminal can drive the window
/// it is running in without being told where it is. That is the one thing DDE
/// could not do at all.
pub const ENV: &str = "STERNA_CTL";

/// The longest a socket name may be, matching `TopicName[21]` at `ttdde.c:70`.
///
/// Not a limit anything here needs — it is so that a `/D=` written for
/// upstream, where the topic is truncated to twenty characters at both ends,
/// names the same socket in both.
pub const MAX_NAME: usize = 20;

/// The Unix socket directory, or the Windows named-pipe namespace.
///
/// On Unix, `$XDG_RUNTIME_DIR` when the session has one, which is the correct home for
/// a socket that should not outlive the login. The `/tmp` fallback is for a
/// session that has none — a bare `ssh` login, or a container — and carries
/// the uid because `/tmp` is shared and the name must not be another user's to
/// create. Windows returns `\\.\pipe`; there is no directory to create.
#[cfg(unix)]
pub fn dir() -> io::Result<PathBuf> {
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d).join("sterna"),
        // SAFETY: `getuid` cannot fail and takes no arguments.
        _ => PathBuf::from(format!("/tmp/sterna-{}", unsafe { libc::getuid() })),
    };
    if !base.is_dir() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&base)?;
    }
    Ok(base)
}

#[cfg(windows)]
pub fn dir() -> io::Result<PathBuf> {
    Ok(PathBuf::from(r"\\.\pipe"))
}

/// Whether a name may be used as a file name in [`dir`].
///
/// A `/D=` topic comes off a command line, so it is a string somebody else
/// wrote: without this, `/D=../../.ssh/config` names a socket outside the
/// directory that is the whole access control. Upstream's own topics are eight
/// hexadecimal digits, so nothing that works there is refused here.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The path a name resolves to.
pub fn path_of(name: &str) -> io::Result<PathBuf> {
    if !valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("bad socket name {name:?}: letters, digits, - and _, at most {MAX_NAME}"),
        ));
    }
    #[cfg(unix)]
    {
        Ok(dir()?.join(format!("{name}.sock")))
    }
    #[cfg(windows)]
    {
        Ok(dir()?.join(format!("{}{name}", pipe_prefix()?)))
    }
}

#[cfg(windows)]
fn pipe_prefix() -> io::Result<String> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;

    let mut session = 0u32;
    let ok = unsafe { ProcessIdToSessionId(std::process::id(), &mut session) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(format!("sterna-{session}-"))
}

/// The `/D=` name carried by an endpoint returned by [`list`] or [`live`].
pub fn name_of(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        path.file_stem().map(|s| s.to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let leaf = path.file_name()?.to_string_lossy();
        leaf.strip_prefix(&pipe_prefix().ok()?).map(str::to_owned)
    }
}

/// This process's default name — its pid, which is what a window uses when no
/// `/D=` named it.
pub fn default_name() -> String {
    std::process::id().to_string()
}

/// Bind a listener at `path`, taking over a socket file nobody is listening on.
///
/// A Unix socket file outlives the process that bound it, and the process that
/// bound it is a terminal emulator — so it is killed, it crashes, the machine
/// loses power, and the file is still there. `bind` on an existing path is
/// `EADDRINUSE` whether or not anybody is behind it, so the only way to tell
/// the two apart is to try to connect: a refusal means the file is a leftover
/// and can be removed, and a success means somebody is already there and this
/// really is a collision.
///
/// The race — another window binding between the unlink and the bind — leaves
/// one of the two without a socket and is reported as the error it is, rather
/// than being retried. Two windows choose the same name only when both were
/// given the same `/D=`.
#[cfg(unix)]
pub(crate) fn bind(path: &Path) -> io::Result<Listener> {
    if path.exists() {
        match Stream::connect(path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("{} is another window's socket", path.display()),
                ))
            }
            // Nobody home. Anything else — a permission error, a path that is
            // not a socket — is left alone, so that a mistake in the name is
            // not answered by deleting the user's file.
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                std::fs::remove_file(path)?;
            }
            Err(e) => return Err(e),
        }
    }
    let listener = Listener::bind(path)?;
    // The directory is already `0700`; this is what is left if it is not.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

#[cfg(windows)]
pub(crate) fn bind(path: &Path) -> io::Result<Listener> {
    Listener::bind(path)
}

/// Every Unix socket file or Windows named pipe, sorted by name.
#[cfg(windows)]
pub fn list() -> io::Result<Vec<PathBuf>> {
    let pattern = dir()?.join(format!("{}*", pipe_prefix()?));
    crate::ipc::list_named_pipes(&pattern)
}

#[cfg(unix)]
pub fn list() -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let dir = match std::fs::read_dir(dir()?) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "sock") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Every socket that answers, with the leftovers removed on the way past.
///
/// Pruning here rather than in a reaper because this is the only code that
/// learns a socket is dead, and a directory that fills up with dead names is
/// what makes [`resolve`]'s "exactly one" refuse a session that has exactly
/// one window.
pub fn live() -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in list()? {
        match Stream::connect(&path) {
            Ok(_) => out.push(path),
            #[cfg(unix)]
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                // Best effort: another client may be pruning the same one, and
                // a socket in somebody else's directory is not ours to remove.
                let _ = std::fs::remove_file(&path);
            }
            // A socket that refuses for any other reason is somebody's, and
            // saying so is better than hiding it.
            Err(_) => out.push(path),
        }
    }
    Ok(out)
}

/// Which window a client means.
///
/// In order: what it was told, then `$STERNA_CTL`, then the only live endpoint
/// there is. A name containing a path separator is taken as an explicit Unix
/// socket path or Windows pipe path.
pub fn resolve(name: Option<&str>) -> io::Result<PathBuf> {
    if let Some(n) = name {
        return if n.contains('/') || n.contains('\\') {
            Ok(PathBuf::from(n))
        } else {
            path_of(n)
        };
    }
    if let Some(p) = std::env::var_os(ENV) {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let live = live()?;
    match live.len() {
        1 => Ok(live.into_iter().next().unwrap()),
        0 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no Sterna window is listening in {}", dir()?.display()),
        )),
        _ => {
            let names: Vec<String> = live.iter().filter_map(|p| name_of(p)).collect();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} windows are listening ({}); name one",
                    names.len(),
                    names.join(", ")
                ),
            ))
        }
    }
}

/// Whether the process on the other end of an accepted connection is us.
///
/// The directory's mode already keeps other users out, so this is the second
/// of the two locks rather than the only one — and it is what makes a
/// misconfigured `/tmp`, or a directory somebody has been generous with, still
/// safe. `root` is refused along with everyone else: a socket that types at a
/// production router should not be a way for a daemon running as root to do it
/// by accident.
///
/// Unix compares `SO_PEERCRED`'s uid. Windows impersonates the named-pipe
/// client long enough to compare its token user SID with the server's, then
/// reverts before any request is parsed.
pub(crate) fn peer_is_us(stream: &Stream) -> bool {
    stream.peer_is_us()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The tests bind real sockets, so they need a directory of their own —
    /// otherwise a run inside a live session would prune, or collide with, the
    /// user's own windows.
    struct Scratch {
        _dir: tempfile::TempDir,
        prev: Option<std::ffi::OsString>,
    }

    impl Scratch {
        fn new() -> Scratch {
            let dir = tempfile::tempdir().unwrap();
            let prev = std::env::var_os("XDG_RUNTIME_DIR");
            std::env::set_var("XDG_RUNTIME_DIR", dir.path());
            Scratch { _dir: dir, prev }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            match &self.prev {
                Some(p) => std::env::set_var("XDG_RUNTIME_DIR", p),
                None => std::env::remove_var("XDG_RUNTIME_DIR"),
            }
        }
    }

    // One test at a time: they share the process's environment, which is what
    // `XDG_RUNTIME_DIR` is.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn a_name_may_not_escape_the_directory() {
        assert!(valid_name("4321"));
        assert!(valid_name("A1B2C3D4"));
        assert!(!valid_name("../etc/passwd"));
        assert!(!valid_name("has/slash"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"x".repeat(MAX_NAME + 1)));
    }

    #[test]
    fn the_directory_is_private() {
        let _g = lock();
        let _s = Scratch::new();
        let d = dir().unwrap();
        let mode = std::fs::metadata(&d).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_socket_is_private_too() {
        let _g = lock();
        let _s = Scratch::new();
        let p = path_of("one").unwrap();
        let _l = bind(&p).unwrap();
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    /// The failure this exists for: the window died and left its file behind.
    #[test]
    fn binding_takes_over_a_socket_nobody_is_listening_on() {
        let _g = lock();
        let _s = Scratch::new();
        let p = path_of("stale").unwrap();
        drop(bind(&p).unwrap());
        assert!(p.exists(), "the file outlives the listener");
        let _l = bind(&p).expect("a leftover is not a collision");
    }

    #[test]
    fn binding_over_a_live_socket_is_refused() {
        let _g = lock();
        let _s = Scratch::new();
        let p = path_of("live").unwrap();
        let _l = bind(&p).unwrap();
        assert_eq!(
            bind(&p).unwrap_err().kind(),
            io::ErrorKind::AddrInUse,
            "two windows cannot share a name"
        );
    }

    #[test]
    fn a_dead_socket_is_pruned_and_a_live_one_is_found() {
        let _g = lock();
        let _s = Scratch::new();
        let dead = path_of("dead").unwrap();
        drop(bind(&dead).unwrap());
        let alive = path_of("alive").unwrap();
        let _l = bind(&alive).unwrap();
        assert_eq!(live().unwrap(), vec![alive.clone()]);
        assert!(!dead.exists(), "the leftover is removed on the way past");
        assert_eq!(resolve(None).unwrap(), alive);
    }

    #[test]
    fn two_windows_are_named_rather_than_guessed_between() {
        let _g = lock();
        let _s = Scratch::new();
        let _a = bind(&path_of("aaa").unwrap()).unwrap();
        let _b = bind(&path_of("bbb").unwrap()).unwrap();
        let e = resolve(None).unwrap_err();
        assert!(e.to_string().contains("aaa"), "{e}");
        assert!(e.to_string().contains("bbb"), "{e}");
    }

    #[test]
    fn nothing_listening_says_so() {
        let _g = lock();
        let _s = Scratch::new();
        assert_eq!(resolve(None).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_name_with_a_slash_in_it_is_a_path() {
        let _g = lock();
        let _s = Scratch::new();
        assert_eq!(
            resolve(Some("/run/somewhere/x.sock")).unwrap(),
            PathBuf::from("/run/somewhere/x.sock")
        );
    }

    #[test]
    fn the_environment_names_a_window_when_nothing_else_does() {
        let _g = lock();
        let _s = Scratch::new();
        let _l = bind(&path_of("env").unwrap()).unwrap();
        std::env::set_var(ENV, "/somewhere/else.sock");
        let got = resolve(None).unwrap();
        std::env::remove_var(ENV);
        assert_eq!(got, PathBuf::from("/somewhere/else.sock"));
    }

    #[test]
    fn our_own_connection_passes_the_credential_check() {
        let _g = lock();
        let _s = Scratch::new();
        let p = path_of("cred").unwrap();
        let mut l = bind(&p).unwrap();
        let _client = Stream::connect(&p).unwrap();
        let server = l.accept().unwrap();
        assert!(peer_is_us(&server));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn name() -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!("a{:x}{n:x}", std::process::id())
    }

    #[test]
    fn names_cannot_escape_the_pipe_namespace() {
        assert!(valid_name("A1B2C3D4"));
        assert!(!valid_name(r"..\other"));
        assert!(!valid_name("has/slash"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"x".repeat(MAX_NAME + 1)));
    }

    #[test]
    fn a_pipe_name_round_trips_and_is_enumerated() {
        let name = name();
        let path = path_of(&name).unwrap();
        let _listener = bind(&path).unwrap();
        assert_eq!(name_of(&path).as_deref(), Some(name.as_str()));
        assert!(list().unwrap().contains(&path));
    }

    #[test]
    fn two_windows_cannot_own_one_pipe() {
        let path = path_of(&name()).unwrap();
        let _listener = bind(&path).unwrap();
        assert_eq!(bind(&path).unwrap_err().kind(), io::ErrorKind::AddrInUse);
    }

    /// Through `peer_check` rather than `peer_is_us`, so that a Win32 call
    /// which failed says which one and why instead of arriving as a bare
    /// "this peer is not us" — the same answer a correct refusal gives.
    #[test]
    fn our_own_connection_passes_the_token_check() {
        let path = path_of(&name()).unwrap();
        let mut listener = bind(&path).unwrap();
        let _client = Stream::connect(&path).unwrap();
        let server = listener.accept().unwrap();
        assert!(server.peer_check().expect("the token check ran"));
    }
}
