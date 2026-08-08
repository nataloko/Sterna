//! A self-pipe, so an async transport can be waited on by a frontend that is
//! not async.
//!
//! [`Transport::poll_fd`](crate::Transport::poll_fd) exists because the Qt
//! shell has no timer in its event loop and does not want one — a serial
//! console is quiet almost all the time, and a poll on a timer burns a wakeup
//! per frame forever to discover nothing happened. A serial port satisfies
//! that trivially: it *is* a descriptor.
//!
//! SSH does not. `russh` is `tokio`, its readable thing is a task, and there
//! is no fd to hand out. So one is manufactured: a pipe whose read end goes to
//! the frontend and whose write end the worker thread pokes whenever the
//! terminal has something to collect.
//!
//! The alternative — the shell running its own tokio reactor, or polling on a
//! timer — was rejected for the same reason the timer was rejected everywhere
//! else. This keeps the whole async story inside `tt-conn`, which is where
//! `PLAN.md` said it belonged.

use std::os::unix::io::RawFd;

use crate::error::Result;

/// The classic self-pipe. Both ends non-blocking, both `O_CLOEXEC`.
pub(crate) struct Wakeup {
    read: RawFd,
    write: RawFd,
}

impl Wakeup {
    pub(crate) fn new() -> Result<Wakeup> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: `fds` is a two-element array, which is what pipe2 writes.
        let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Wakeup {
            read: fds[0],
            write: fds[1],
        })
    }

    pub(crate) fn fd(&self) -> RawFd {
        self.read
    }

    /// Make the read end readable. Cheap enough to call per event, and safe to
    /// call from any thread.
    ///
    /// **A full pipe is not an error.** `EAGAIN` here means the frontend has
    /// not drained the previous pokes, so the descriptor is *already* readable
    /// and the wakeup this call would have delivered is redundant. Treating it
    /// as a failure would turn "the UI is busy" into "the connection broke".
    pub(crate) fn signal(&self) {
        let byte = 1u8;
        // SAFETY: writing one byte from a live local into a non-blocking fd we
        // own. The result is deliberately ignored; see above.
        unsafe {
            libc::write(self.write, std::ptr::addr_of!(byte).cast(), 1);
        }
    }

    /// Consume every pending poke.
    ///
    /// Called at the top of `read`, before looking at the buffer, so that a
    /// byte arriving between the drain and the lock leaves the pipe readable
    /// and produces one extra wakeup. The other order loses it: drain after
    /// the buffer is emptied and the new byte's poke is thrown away with it,
    /// and the frontend sleeps on data that has already arrived.
    pub(crate) fn drain(&self) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: reading into a live local from a non-blocking fd we own.
            let n = unsafe { libc::read(self.read, buf.as_mut_ptr().cast(), buf.len()) };
            if n < buf.len() as isize {
                break;
            }
        }
    }
}

impl Drop for Wakeup {
    fn drop(&mut self) {
        // SAFETY: both are ours and are closed exactly once.
        unsafe {
            libc::close(self.read);
            libc::close(self.write);
        }
    }
}
