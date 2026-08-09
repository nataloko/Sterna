//! A request's way to the thread that owns the terminal.
//!
//! The same bargain, and the same shape, as `tt_macro::channel`: a
//! [`Session`] is touched only from the thread that owns it, so the listener —
//! which is a thread of its own, blocked in `accept` — sends a **job** and
//! waits for the answer. The frontend runs the job in its own event loop, on
//! its own thread, where a dialog is an ordinary dialog and a repaint happens
//! on time.
//!
//! It is a second copy of that plumbing rather than a shared one, and the
//! reason is the trait object in the job's signature. `tt-macro`'s job takes a
//! `&mut dyn MacroUi` and this one takes a `&mut dyn CtlHost`, because the two
//! ask a frontend for different things — a macro asks for dialogs, a control
//! socket asks for macros. Making one generic over the other would put a type
//! parameter through the clearest type in `tt-macro` to save a hundred lines
//! of pipe. What is shared is the *contract*, which is written down in both:
//! a wakeup descriptor the toolkit waits on, and a `service` call that runs
//! what is waiting.
//!
//! Nothing here is on a hot path. A macro's `wait` reads the ring thousands of
//! times a second and is deliberately not a job; a control request arrives when
//! somebody's script runs, which is a handful of times a session.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, SendError, Sender, TryRecvError};
use std::sync::Arc;

use tt_session::Session;

use crate::host::CtlHost;

/// Something to do on the frontend's thread, on behalf of a request.
pub type Job = Box<dyn FnOnce(&mut Session, &mut dyn CtlHost) + Send>;

/// The listener's end. Cloned once per accepted connection.
#[derive(Clone)]
pub struct CtlSender {
    tx: Sender<Job>,
    wake: Arc<Waker>,
}

/// The frontend's end.
pub struct CtlReceiver {
    rx: Receiver<Job>,
    wake: Arc<Waker>,
}

/// Build a pair. The frontend keeps the receiver and hands the sender to the
/// listener it starts.
pub fn channel() -> std::io::Result<(CtlSender, CtlReceiver)> {
    let (tx, rx) = mpsc::channel();
    let wake = Arc::new(Waker::new()?);
    Ok((
        CtlSender {
            tx,
            wake: wake.clone(),
        },
        CtlReceiver { rx, wake },
    ))
}

impl CtlSender {
    /// Run `f` on the frontend's thread and wait for what it returns.
    ///
    /// `None` is the window having gone — either it never took the job or it
    /// died holding it, and the two are the same thing to a client. Every
    /// caller turns it into [`RpcError::GONE`](crate::RpcError::GONE) and then
    /// closes the connection, because a socket whose window has closed has
    /// nothing left to answer.
    pub fn call<T, F>(&self, f: F) -> Option<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session, &mut dyn CtlHost) -> T + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let job: Job = Box::new(move |s, h| {
            let _ = reply_tx.send(f(s, h));
        });
        self.post(job).ok()?;
        reply_rx.recv().ok()
    }

    /// Send without waiting — for a notification, which by definition has no
    /// answer to carry back.
    pub fn post(&self, job: Job) -> Result<(), SendError<Job>> {
        self.tx.send(job)?;
        self.wake.wake();
        Ok(())
    }
}

impl CtlReceiver {
    /// A descriptor that becomes readable when there is work.
    ///
    /// The same bargain as [`Session::poll_fd`] and `tt_macro`'s: the toolkit
    /// waits on it and calls [`service`](CtlReceiver::service) when it fires,
    /// so a window with nobody talking to it costs nothing and needs no timer.
    #[cfg(unix)]
    pub fn poll_fd(&self) -> std::os::unix::io::RawFd {
        self.wake.poll_fd()
    }

    /// Run everything waiting. Returns how many jobs ran.
    ///
    /// **This can start a macro and it can close the window**, so a frontend
    /// has to be able to survive both happening inside it — the same care
    /// `tt_macro_service` needs, for the same reason.
    pub fn service(&self, session: &mut Session, host: &mut dyn CtlHost) -> usize {
        self.wake.drain();
        let mut n = 0;
        loop {
            match self.rx.try_recv() {
                Ok(job) => {
                    job(session, host);
                    n += 1;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return n,
            }
        }
    }
}

/// A descriptor that can be made readable from another thread.
///
/// `UnixStream::pair` rather than a pipe, for the reason `tt_macro`'s says:
/// it is in `std`. Stage 3 replaces the type with an event object and nothing
/// outside it knows which it is.
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
        // A full pipe is a wakeup nobody has answered yet, so a failed write
        // is not a lost one.
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
    use crate::host::NullHost;
    use tt_vt::Config;

    #[test]
    fn a_call_runs_on_the_other_side_and_comes_back() {
        let (tx, rx) = channel().unwrap();
        let thread = std::thread::spawn(move || tx.call(|s, _| s.grid().cols()));
        let mut session = Session::new(Config {
            cols: 132,
            ..Config::default()
        });
        let mut host = NullHost;
        let mut ran = 0;
        while ran == 0 {
            ran = rx.service(&mut session, &mut host);
        }
        assert_eq!(thread.join().unwrap(), Some(132));
    }

    /// A window that closes while a request is in flight unblocks the
    /// connection rather than hanging it, which is what the socket's `GONE`
    /// is made of.
    #[test]
    fn a_dropped_frontend_ends_the_call() {
        let (tx, rx) = channel().unwrap();
        drop(rx);
        assert_eq!(tx.call(|_, _| 1u8), None);
    }
}
