//! A local shell on a pty — the fourth transport, and the last of Stage 1's.
//!
//! Upstream reaches a local shell by *not* being a terminal for it:
//! `cygwin/cygterm` is a separate program that forks a shell onto a pty and
//! then bridges it to Tera Term over a **loopback telnet socket**
//! (`cygterm.cpp:1083` onward implements ECHO, SGA, TERMINAL-TYPE and NAWS by
//! hand). That indirection existed because Tera Term is a Windows program that
//! could not fork; here the pty is a transport like any other and the telnet
//! detour is deleted.
//!
//! Two of upstream's decisions are kept, because they are about how a shell
//! should start rather than about Win32: a **login shell** by default
//! (`cygterm.cfg`'s `LOGIN_SHELL = Yes`, implemented at `cygterm.cpp:988` by
//! rewriting `argv[0]` to `-bash`), and an explicitly set `TERM`. The value
//! differs — upstream says `vt100`, we say `xterm-256color` — because that is
//! a claim about the engine behind it, and ours does 256 colour, truecolor and
//! xterm mouse tracking. Underclaiming costs the user `ls --color` and a mouse
//! that does nothing in `vim`.
//!
//! `portable-pty` supplies the parts that are genuinely hard and genuinely
//! platform-specific — `openpty`, the `setsid`/`TIOCSCTTY` dance in the child,
//! and ConPTY when Stage 3 arrives. The byte-level read and write are ours,
//! for the reason [`PtyConn::read`] gives.

#[cfg(unix)]
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::error::{Error, Result};
use crate::transport::{Transport, TransportEvent};

/// What to run, and what to tell it about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtyParams {
    /// The command and its arguments. **Empty means the user's login shell**,
    /// which is the case the connect menu uses; anything else is run as
    /// itself.
    pub argv: Vec<String>,
    /// Working directory. `None` means the user's home, which is where a
    /// terminal that was launched from a desktop menu should start.
    pub cwd: Option<PathBuf>,
    /// Extra environment, applied over the inherited one.
    pub env: Vec<(String, String)>,
    /// `$TERM`. Set explicitly and never inherited — see [`PtyConn::open`].
    pub term: String,
    /// Start the shell as a login shell, so it reads `~/.profile` and friends.
    ///
    /// **Only meaningful when `argv` is empty.** The trick is to pass `-bash`
    /// as `argv[0]`, and `argv[0]` is also the name looked up on `PATH`, so a
    /// command named by the caller cannot have one without the other.
    pub login_shell: bool,
    /// The initial window size, in cells. The child sees it as `TIOCGWINSZ`
    /// from the first instruction it executes, which is what stops a shell
    /// from drawing its prompt 80 columns wide in a 132-column window.
    pub cols: u16,
    pub rows: u16,
}

impl Default for PtyParams {
    fn default() -> PtyParams {
        PtyParams {
            argv: Vec::new(),
            cwd: None,
            env: Vec::new(),
            term: "xterm-256color".to_string(),
            login_shell: true,
            cols: 80,
            rows: 24,
        }
    }
}

/// How the child ended.
///
/// Its own type rather than `portable_pty::ExitStatus` so the dependency stays
/// an implementation detail — the same reason the serial layer does not
/// re-export `serialport`'s types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtyExit {
    pub code: u32,
    /// The signal name, when one killed it. `Some` here outranks `code`: a
    /// shell killed by `SIGSEGV` has no meaningful exit code, and reporting
    /// the 1 that stands in for one reads as an ordinary failure.
    pub signal: Option<String>,
}

impl std::fmt::Display for PtyExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.signal {
            Some(sig) => write!(f, "killed by {sig}"),
            None => write!(f, "exited with status {}", self.code),
        }
    }
}

/// A child process on the far end of a pty.
pub struct PtyConn {
    /// `Option` only so [`Drop`] can close it *before* reaping — closing the
    /// master is what hangs the line up, and reaping before that would wait
    /// for a shell nothing had told to leave.
    master: Option<Box<dyn MasterPty>>,
    child: Box<dyn Child + Send + Sync>,
    /// The master's descriptor, cached because reads and writes go through it
    /// directly. Borrowed from `master` and invalid once that is dropped.
    #[cfg(unix)]
    fd: std::os::unix::io::RawFd,
    describe: String,
    tty_name: Option<PathBuf>,
    exit: Option<PtyExit>,
    dead: bool,
}

impl PtyConn {
    /// Fork a child onto a new pty.
    ///
    /// The environment is inherited and then corrected in three places, each
    /// of which is wrong by default in a way that is invisible until it isn't:
    ///
    /// - **`TERM` is ours, not the parent's.** A terminal launched from
    ///   another terminal inherits *that* terminal's `TERM` and would hand it
    ///   to the shell, so applications would negotiate against the wrong
    ///   engine — and one launched from a desktop menu inherits no `TERM` at
    ///   all, which makes everything assume a teleprinter.
    /// - **`COLORTERM`** says truecolor, because the engine takes `SGR 38;2`.
    /// - **`LINES` and `COLUMNS` are removed.** They are a snapshot, the pty's
    ///   `winsize` is the truth, and a stale pair inherited from a differently
    ///   sized parent survives every resize.
    pub fn open(params: &PtyParams) -> Result<PtyConn> {
        let size = PtySize {
            rows: params.rows.max(1),
            cols: params.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| Error::Unsupported(format!("cannot open a pty: {e}")))?;

        let shell = shell_for(params);
        let mut cmd = if params.argv.is_empty() {
            if params.login_shell {
                // `new_default_prog` is the only route to a `-bash` argv[0],
                // because an explicit argv[0] is also what gets looked up on
                // PATH. It resolves the shell from the builder's own `SHELL`,
                // which is `shell_for`'s answer by the time it is read.
                CommandBuilder::new_default_prog()
            } else {
                CommandBuilder::new(&shell)
            }
        } else {
            CommandBuilder::from_argv(params.argv.iter().map(Into::into).collect())
        };

        cmd.env("TERM", &params.term);
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "sterna");
        cmd.env_remove("LINES");
        cmd.env_remove("COLUMNS");
        for (k, v) in &params.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &params.cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd).map_err(|e| Error::Open {
            path: describe_argv(&params.argv, &shell),
            source: std::io::Error::other(e.to_string()),
        })?;

        // **Drop the slave, and do it here.** We hold one end of the pty and
        // the child holds the other; keeping ours open means the master never
        // sees the hangup when the child exits, so the shell dies and the
        // window sits there forever waiting for output from nobody. It is the
        // oldest trap in pty programming and it fails as a hang rather than as
        // an error.
        drop(pair.slave);

        let master = pair.master;
        #[cfg(unix)]
        let tty_name = master.tty_name();
        #[cfg(not(unix))]
        let tty_name = None;

        #[cfg(unix)]
        let fd = {
            let fd = master.as_raw_fd().ok_or_else(|| {
                Error::Unsupported("this pty has no descriptor to wait on".into())
            })?;
            set_nonblocking(fd)?;
            fd
        };

        Ok(PtyConn {
            master: Some(master),
            child,
            #[cfg(unix)]
            fd,
            describe: describe_argv(&params.argv, &shell),
            tty_name,
            exit: None,
            dead: false,
        })
    }

    /// The slave's path — `/dev/pts/7`. Worth surfacing: it is what a user
    /// needs to send something to this window from elsewhere, and what
    /// `who`/`w` will show.
    pub fn tty_name(&self) -> Option<&Path> {
        self.tty_name.as_deref()
    }

    /// The child's process id, while it has one.
    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// How the child ended, once it has. Never blocks.
    ///
    /// Read this **after** a read returns [`Error::Disconnected`]: the status
    /// is what turns "the connection dropped" into "the shell exited with
    /// status 1", which is the difference between a mystery and a message.
    pub fn exit_status(&mut self) -> Option<PtyExit> {
        self.reap();
        self.exit.clone()
    }

    /// True once the child's fate is settled — collected, or unknowable.
    fn reap(&mut self) -> bool {
        if self.exit.is_some() {
            return true;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                self.exit = Some(PtyExit {
                    code: status.exit_code(),
                    signal: status.signal().map(str::to_string),
                });
                true
            }
            Ok(None) => false,
            // It cannot be waited for at all — already collected elsewhere, or
            // never ours. Asking again would only spend the deadline.
            Err(_) => true,
        }
    }

    /// Mark the connection dead and collect the child, so the caller's next
    /// question about *why* has an answer ready.
    ///
    /// **The wait is not belt-and-braces; there is a real race and it is
    /// small.** The kernel closes a dying process's file descriptors in
    /// `exit_files()` — which is what makes the master read `EIO` — *before*
    /// `exit_notify()` makes it waitable. So the read can learn the shell is
    /// gone microseconds before `try_wait` can say how, and without this the
    /// window says "Disconnected" instead of "bash exited with status 4", some
    /// of the time. An intermittently missing explanation is worse than none:
    /// it teaches the reader that the message means something it does not.
    ///
    /// Bounded like `Drop`'s, and for the same reason. A child that closed the
    /// slave and kept running — a daemon detaching — is the case that spends
    /// the whole deadline, once, at the end of a connection.
    #[cfg(unix)]
    fn died(&mut self) -> Error {
        self.dead = true;
        self.wait_briefly();
        Error::Disconnected
    }
}

#[cfg(unix)]
impl PtyConn {
    /// Read whatever is there, or nothing.
    ///
    /// **This does not go through `portable-pty`'s reader, and the reason is a
    /// silent one.** Its unix `Read` impl maps `EIO` — which is how a pty
    /// master reports that the last slave closed, i.e. that the child is gone
    /// — to `Ok(0)`, so that `read_to_string` terminates. Our `Ok(0)` already
    /// means "the line is quiet", the state a terminal spends nearly all its
    /// time in. Taking that mapping would collapse the two: the window would
    /// never learn the shell exited, and because a hung-up descriptor is
    /// *permanently* readable, the frontend's `QSocketNotifier` would fire
    /// forever against a read that never returns anything. A dead shell would
    /// present as a terminal at 100% CPU.
    fn read_once(&mut self, data: &mut Vec<u8>) -> Result<usize> {
        let mut buf = [0u8; 8192];
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n > 0 {
            let n = n as usize;
            data.extend_from_slice(&buf[..n]);
            return Ok(n);
        }
        if n == 0 {
            // Not reachable on Linux — a pty master gives EIO rather than EOF
            // — but a zero-length read is EOF everywhere it does happen.
            return Err(self.died());
        }
        let e = std::io::Error::last_os_error();
        match e.kind() {
            ErrorKind::WouldBlock | ErrorKind::Interrupted => Ok(0),
            _ => {
                let e = Error::from_io(e);
                if e.is_disconnected() {
                    return Err(self.died());
                }
                Err(e)
            }
        }
    }

    fn wait_writable(&self, timeout: Duration) -> bool {
        let mut pfd = libc::pollfd {
            fd: self.fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        let ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        unsafe { libc::poll(&mut pfd, 1, ms) > 0 }
    }
}

impl Transport for PtyConn {
    fn link_kind(&self) -> crate::transport::LinkKind {
        crate::transport::LinkKind::LocalPty
    }

    fn read(&mut self, data: &mut Vec<u8>, _events: &mut Vec<TransportEvent>) -> Result<usize> {
        if self.dead {
            return Err(Error::Disconnected);
        }
        #[cfg(unix)]
        {
            self.read_once(data)
        }
        #[cfg(not(unix))]
        {
            let _ = data;
            Err(Error::Unsupported("a local shell on this platform".into()))
        }
    }

    #[cfg(unix)]
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        if self.dead {
            return Err(Error::Disconnected);
        }
        if data.is_empty() {
            return Ok(0);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let n =
                unsafe { libc::write(self.fd, data.as_ptr() as *const libc::c_void, data.len()) };
            if n >= 0 {
                return Ok(n as usize);
            }
            let e = std::io::Error::last_os_error();
            match e.kind() {
                ErrorKind::Interrupted => continue,
                // The pty's buffer is a few kilobytes and a paste is not, so a
                // short write here is routine rather than exceptional. The
                // session retries the remainder; see `pending_out`.
                ErrorKind::WouldBlock => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() || !self.wait_writable(left) {
                        return Ok(0);
                    }
                }
                _ => {
                    let e = Error::from_io(e);
                    if e.is_disconnected() {
                        return Err(self.died());
                    }
                    return Err(e);
                }
            }
        }
    }

    #[cfg(not(unix))]
    fn write(&mut self, _data: &[u8], _timeout: Duration) -> Result<usize> {
        Err(Error::Unsupported("a local shell on this platform".into()))
    }

    /// A pty has no break to send. There is no wire and no far end — the
    /// "device" is a process on this machine, and the thing a break stands in
    /// for is `SIGINT`, which reaches the child as `Ctrl+C` through the line
    /// discipline like any other keystroke.
    fn send_break(&mut self, _dur: Duration) -> Result<()> {
        Err(Error::Unsupported(
            "a line break on a local shell — a pty has no line to break".into(),
        ))
    }

    fn supports_break(&self) -> bool {
        false
    }

    /// `TIOCSWINSZ`, which also delivers `SIGWINCH` to the foreground process
    /// group. Both halves matter: the ioctl is what `stty size` reads, and the
    /// signal is what makes a running `vim` redraw instead of waiting for the
    /// next keystroke.
    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let Some(master) = self.master.as_ref() else {
            return Ok(());
        };
        master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
    }

    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        Some(self.fd)
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }

    /// "bash exited with status 1", which is the whole difference between a
    /// shell that finished and one that fell over.
    fn closing_note(&mut self) -> Option<String> {
        let exit = self.exit_status()?;
        Some(format!("{} {exit}", self.describe))
    }
}

impl Drop for PtyConn {
    /// Hang up, then collect the body.
    ///
    /// Closing the master is what a terminal window closing *means*: the
    /// kernel sends `SIGHUP` to the pty's foreground process group, the shell
    /// leaves, and it hangs up its own jobs on the way out. Only then is there
    /// anything to reap — and reaping matters, because `std::process::Child`
    /// does not do it on drop, so a session that opens and closes local shells
    /// all day would leave one zombie behind per shell.
    ///
    /// Both waits are bounded. A shell that ignores `SIGHUP` gets `SIGKILL`; a
    /// process that survives even that is stuck in the kernel, and leaving a
    /// zombie is better than hanging the window that is trying to close.
    fn drop(&mut self) {
        self.master.take();
        if self.wait_briefly() {
            return;
        }
        let _ = self.child.kill();
        self.wait_briefly();
    }
}

impl PtyConn {
    /// Poll for the child for a fifth of a second, keeping its status. True if
    /// its fate was settled inside that.
    ///
    /// A millisecond between tries rather than five: the race this closes on
    /// the read path is microseconds wide, and sleeping through it five times
    /// over would put the cost where there is no need for one.
    fn wait_briefly(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_millis(200);
        loop {
            if self.reap() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Which shell to run: `params.env`'s `SHELL` if the caller set one, then the
/// process's, then the password database. The same order `cygterm.cfg`'s
/// `SHELL = auto` means, with the caller's override in front of it — asking
/// for a shell in `env` and getting a different one would be a quiet lie.
fn shell_for(params: &PtyParams) -> String {
    match params.env.iter().find(|(k, _)| k == "SHELL") {
        Some((_, v)) => v.clone(),
        None => CommandBuilder::new_default_prog().get_shell(),
    }
}

/// What the status line calls this connection.
///
/// The basename for a plain shell — a status line reading `bash` beats one
/// reading `/usr/bin/bash` — and the whole command line when there is one,
/// because `sh -c 'journalctl -f'` and `sh` are not the same session.
fn describe_argv(argv: &[String], shell: &str) -> String {
    match argv.split_first() {
        None => basename(shell),
        Some((prog, [])) => basename(prog),
        Some((prog, args)) => format!("{} {}", basename(prog), args.join(" ")),
    }
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

#[cfg(unix)]
fn set_nonblocking(fd: std::os::unix::io::RawFd) -> Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_shell_is_named_by_its_basename() {
        assert_eq!(describe_argv(&[], "/usr/bin/bash"), "bash");
        assert_eq!(describe_argv(&["/bin/bash".into()], "/bin/sh"), "bash");
    }

    #[test]
    fn a_command_keeps_its_arguments() {
        assert_eq!(
            describe_argv(&["/bin/sh".into(), "-c".into(), "true".into()], "/bin/sh"),
            "sh -c true"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_default_shell_is_a_path() {
        assert!(shell_for(&PtyParams::default()).starts_with('/'));
    }

    #[test]
    fn a_windows_path_is_named_by_its_basename() {
        assert_eq!(basename(r"C:\Windows\System32\cmd.exe"), "cmd.exe");
    }

    /// A caller that names a shell in `env` gets that shell, rather than the
    /// one the process happens to have been launched from.
    #[test]
    fn an_explicit_shell_wins() {
        let params = PtyParams {
            env: vec![("SHELL".into(), "/bin/zsh".into())],
            ..PtyParams::default()
        };
        assert_eq!(shell_for(&params), "/bin/zsh");
    }
}
