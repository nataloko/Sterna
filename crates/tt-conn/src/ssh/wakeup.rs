//! Wake an ordinary frontend from an async SSH worker.
//!
//! A transport exposes its platform's native wakeup because the Qt shell has
//! no timer in its event loop and does not want one — a serial console is
//! quiet almost all the time, and a poll on a timer burns a wakeup per frame
//! forever to discover nothing happened.
//!
//! SSH does not. `russh` is `tokio`, its readable thing is a task, and there
//! is no native object to hand out. On Unix a self-pipe is manufactured; on
//! Windows it is a manual-reset event. The worker signals either one whenever
//! the terminal has something to collect.
//!
//! The alternative — the shell running its own tokio reactor, or polling on a
//! timer — was rejected for the same reason the timer was rejected everywhere
//! else. This keeps the whole async story inside `tt-conn`, which is where
//! `PLAN.md` said it belonged.

#[cfg(unix)]
use std::os::unix::io::RawFd;
#[cfg(windows)]
use std::os::windows::io::RawHandle;

use crate::error::Result;
#[cfg(windows)]
use crate::windows_event::ManualEvent;

/// The classic self-pipe. Both ends non-blocking, both `O_CLOEXEC`.
#[cfg(unix)]
pub(crate) struct Wakeup {
    read: RawFd,
    write: RawFd,
}

#[cfg(unix)]
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

#[cfg(unix)]
impl Drop for Wakeup {
    fn drop(&mut self) {
        // SAFETY: both are ours and are closed exactly once.
        unsafe {
            libc::close(self.read);
            libc::close(self.write);
        }
    }
}

/// A manual-reset event, the Win32 counterpart of the Unix self-pipe.
///
/// Manual reset preserves the self-pipe's coalescing: ten worker messages can
/// produce one frontend wakeup, and the event stays signalled until `drain`.
#[cfg(windows)]
pub(crate) struct Wakeup {
    event: ManualEvent,
}

#[cfg(windows)]
impl Wakeup {
    pub(crate) fn new() -> Result<Wakeup> {
        Ok(Wakeup {
            event: ManualEvent::new()?,
        })
    }

    pub(crate) fn handle(&self) -> RawHandle {
        self.event.handle()
    }

    pub(crate) fn signal(&self) {
        self.event.signal();
    }

    pub(crate) fn drain(&self) {
        // Reset before the caller examines its queue. A signal racing after
        // this reset remains set; one racing before it has already published
        // its state and the caller will consume that state now.
        self.event.reset();
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) struct Wakeup;

#[cfg(not(any(unix, windows)))]
impl Wakeup {
    pub(crate) fn new() -> Result<Wakeup> {
        Ok(Wakeup)
    }

    pub(crate) fn signal(&self) {}

    pub(crate) fn drain(&self) {}
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    #[test]
    fn event_wakes_and_drain_resets_it() {
        let wake = Wakeup::new().unwrap();
        // SAFETY: `wake` owns a live waitable event for the duration.
        unsafe {
            assert_eq!(WaitForSingleObject(wake.handle(), 0), WAIT_TIMEOUT);
            wake.signal();
            assert_eq!(WaitForSingleObject(wake.handle(), 0), WAIT_OBJECT_0);
            wake.drain();
            assert_eq!(WaitForSingleObject(wake.handle(), 0), WAIT_TIMEOUT);
        }
    }
}
