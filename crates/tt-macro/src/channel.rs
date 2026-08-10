//! The macro thread's only way to reach the terminal.
//!
//! Upstream a macro is a second *process* and every one of these calls is a
//! DDE transaction. `PLAN.md` has said since Stage 2 that deleting that is the
//! point — 2,600 lines of glue and a whole class of races — but the thing it is
//! replaced with has to keep two of DDE's properties, because the language was
//! written against them:
//!
//! 1. **A host call blocks the macro and nothing else.** `messagebox` is modal
//!    to the script and invisible to the terminal, which keeps painting. That
//!    is why the interpreter has a thread: upstream had to park itself in
//!    `TTLStatus` because the macro and the window shared one, and here `wait`
//!    is an ordinary function that returns when it is done.
//! 2. **The terminal is only ever touched from the thread that owns it.** A
//!    `Session` behind a mutex would work until the day a macro held the lock
//!    through a modal dialog and the window stopped repainting — a frame rate
//!    decided by a script.
//!
//! So the macro thread never sees a [`Session`]. It sends a **job** — a closure
//! taking the session and the frontend — and blocks on the answer. The frontend
//! runs the job on its own thread, in its own event loop, where a modal dialog
//! is an ordinary modal dialog. What crosses is owned data in both directions;
//! nothing is borrowed across the boundary, which is what made the equivalent
//! in the SSH dialogs need a re-entrancy guard.
//!
//! Bytes do **not** come this way. They come through
//! [`tt_session::MacroLink`], a ring with a lock of its own, because a `wait`
//! asks for a byte thousands of times a second and none of those should queue
//! behind a repaint.

#[cfg(unix)]
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender, TryRecvError};
use std::sync::{mpsc, Arc};

use tt_session::Session;

use crate::ui::MacroUi;

/// Something to do on the frontend's thread, on behalf of a macro.
///
/// Both halves are handed over because a command needs one or the other and
/// occasionally both: `connect` is the session, `messagebox` is the frontend,
/// and `logopen` is the session with a filename the frontend may have to ask
/// for first.
pub type Job = Box<dyn FnOnce(&mut Session, &mut dyn MacroUi) + Send>;

/// The macro thread's end. Cloneable, though nothing needs two yet.
#[derive(Clone)]
pub struct MacroSender {
    tx: Sender<Job>,
    wake: Arc<Waker>,
    cancel: Arc<AtomicBool>,
}

/// The frontend's end.
pub struct MacroReceiver {
    rx: Receiver<Job>,
    wake: Arc<Waker>,
    cancel: Arc<AtomicBool>,
}

/// Build a pair. The frontend keeps the receiver and hands the sender to the
/// thread it starts.
pub fn channel() -> std::io::Result<(MacroSender, MacroReceiver)> {
    let (tx, rx) = mpsc::channel();
    let wake = Arc::new(Waker::new()?);
    let cancel = Arc::new(AtomicBool::new(false));
    Ok((
        MacroSender {
            tx,
            wake: wake.clone(),
            cancel: cancel.clone(),
        },
        MacroReceiver { rx, wake, cancel },
    ))
}

impl MacroSender {
    /// Run `f` on the frontend's thread and wait for what it returns.
    ///
    /// `None` means the frontend has gone — the window closed while a script
    /// was running, which is the one failure this can have. Every caller turns
    /// it into the end of the macro rather than into an error a script could
    /// catch, because there is nothing left to report to.
    pub fn call<T, F>(&self, f: F) -> Option<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session, &mut dyn MacroUi) -> T + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let job: Job = Box::new(move |s, ui| {
            // A failed send means the macro stopped waiting, which it only does
            // when it is being torn down. Dropping the answer is right.
            let _ = reply_tx.send(f(s, ui));
        });
        self.post(job).ok()?;
        // An error is the frontend having taken the job and then died without
        // answering, which is the same "gone" as a failed post.
        reply_rx.recv().ok()
    }

    /// Send without waiting. For the handful of things that have no answer and
    /// no ordering requirement against a later `call` — there is none today,
    /// and it exists so that adding one does not need a new mechanism.
    pub fn post(&self, job: Job) -> Result<(), SendError<Job>> {
        self.tx.send(job)?;
        self.wake.wake();
        Ok(())
    }

    /// Whether the user has asked the macro to stop.
    ///
    /// Read straight out of shared memory rather than through a job, because
    /// [`tt_ttl::ScriptHost::cancelled`] is asked once per *line* — routing
    /// that through the frontend would make the cost of running a script the
    /// cost of waking it.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl MacroReceiver {
    /// A descriptor that becomes readable when there is work.
    ///
    /// The same bargain as [`Session::poll_fd`]: the toolkit waits on it and
    /// calls [`service`](MacroReceiver::service) when it fires, so the frontend
    /// needs no timer and a quiet macro costs nothing.
    #[cfg(unix)]
    pub fn poll_fd(&self) -> std::os::unix::io::RawFd {
        self.wake.poll_fd()
    }

    /// Run everything waiting. Returns how many jobs ran.
    ///
    /// **This may show a dialog**, because a macro's `messagebox` is a job like
    /// any other — so it can take as long as the user does, and the event loop
    /// it spins is the frontend's own. That is the whole reason the macro is on
    /// another thread.
    pub fn service(&self, session: &mut Session, ui: &mut dyn MacroUi) -> usize {
        self.wake.drain();
        let mut n = 0;
        loop {
            match self.rx.try_recv() {
                Ok(job) => {
                    job(session, ui);
                    n += 1;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return n,
            }
        }
    }

    /// Ask the macro to stop at its next line — the End button on upstream's
    /// macro control window, and what closing the terminal does.
    ///
    /// It does not interrupt a `wait`; those poll it too, so the delay is one
    /// poll interval rather than one timeout.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// A descriptor that can be made readable from another thread.
///
/// `UnixStream::pair` rather than a pipe because it is in `std`: this crate
/// would otherwise want `libc` for one `pipe(2)`. Stage 3 replaces the whole
/// type with an event object; nothing outside it knows which it is.
struct Waker {
    #[cfg(unix)]
    read: std::os::unix::net::UnixStream,
    #[cfg(unix)]
    write: std::sync::Mutex<std::os::unix::net::UnixStream>,
}

impl Waker {
    #[cfg(unix)]
    fn new() -> std::io::Result<Waker> {
        let (read, write) = std::os::unix::net::UnixStream::pair()?;
        read.set_nonblocking(true)?;
        write.set_nonblocking(true)?;
        Ok(Waker {
            read,
            write: std::sync::Mutex::new(write),
        })
    }

    #[cfg(unix)]
    fn wake(&self) {
        // A full pipe is already a wakeup nobody has answered, so a failed
        // write is not a lost one.
        let _ = self.write.lock().unwrap().write(&[1]);
    }

    #[cfg(unix)]
    fn drain(&self) {
        let mut buf = [0u8; 64];
        while (&self.read).read(&mut buf).is_ok() {}
    }

    #[cfg(unix)]
    fn poll_fd(&self) -> std::os::unix::io::RawFd {
        use std::os::unix::io::AsRawFd;
        self.read.as_raw_fd()
    }

    #[cfg(not(unix))]
    fn new() -> std::io::Result<Waker> {
        Ok(Waker {})
    }
    #[cfg(not(unix))]
    fn wake(&self) {}
    #[cfg(not(unix))]
    fn drain(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::NullUi;
    use tt_vt::Config;

    /// The shape the whole crate rests on: a value computed on the frontend's
    /// thread, returned to the macro's.
    #[test]
    fn a_call_runs_on_the_other_side_and_comes_back() {
        let (tx, rx) = channel().unwrap();
        let thread = std::thread::spawn(move || tx.call(|s, _| s.grid().cols()));
        // The frontend, servicing until the job arrives.
        let mut session = Session::new(Config {
            cols: 132,
            ..Config::default()
        });
        let mut ui = NullUi;
        let mut ran = 0;
        while ran == 0 {
            ran = rx.service(&mut session, &mut ui);
        }
        assert_eq!(thread.join().unwrap(), Some(132));
    }

    /// A frontend that goes away unblocks the macro rather than hanging it.
    #[test]
    fn a_dropped_frontend_ends_the_call() {
        let (tx, rx) = channel().unwrap();
        drop(rx);
        assert_eq!(tx.call(|_, _| 1u8), None);
    }

    #[test]
    fn cancelling_is_visible_from_the_other_thread() {
        let (tx, rx) = channel().unwrap();
        assert!(!tx.cancelled());
        rx.cancel();
        assert!(tx.cancelled());
    }

    /// The descriptor is quiet until there is work and readable after.
    #[cfg(unix)]
    #[test]
    fn the_descriptor_wakes_on_a_job_and_goes_quiet_when_it_is_taken() {
        let (tx, rx) = channel().unwrap();
        assert!(!readable(rx.poll_fd()));
        let _ = tx.post(Box::new(|_, _| {}));
        assert!(readable(rx.poll_fd()));
        let mut session = Session::new(Config::default());
        rx.service(&mut session, &mut NullUi);
        assert!(!readable(rx.poll_fd()));
    }

    #[cfg(unix)]
    fn readable(fd: std::os::unix::io::RawFd) -> bool {
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut p, 1, 0) > 0 }
    }
}
