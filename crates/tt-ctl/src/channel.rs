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

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::sync::mpsc::{self, Receiver, SendError, Sender, TryRecvError};
use std::sync::Arc;

use tt_session::Session;

use crate::host::CtlHost;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent};

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

    /// The Windows event that becomes signalled when there is work.
    ///
    /// Borrowed for this receiver's lifetime; the frontend must not close it.
    #[cfg(windows)]
    pub fn wait_handle(&self) -> RawHandle {
        self.wake.wait_handle()
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

/// A native wakeup that can be signalled from another thread.
///
/// Unix uses `UnixStream::pair` rather than a pipe, for the reason
/// `tt_macro`'s says: it is in `std`. Windows uses a manual-reset event.
/// Nothing outside the type knows which one it is.
struct Waker {
    #[cfg(unix)]
    read: std::os::unix::net::UnixStream,
    #[cfg(unix)]
    write: std::sync::Mutex<std::os::unix::net::UnixStream>,
    #[cfg(windows)]
    event: OwnedHandle,
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

    #[cfg(windows)]
    fn new() -> std::io::Result<Waker> {
        // SAFETY: unnamed event, default security, manual reset, initially
        // quiet. A non-null handle is transferred into `OwnedHandle` once.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `event` is freshly created and uniquely owned.
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        Ok(Waker { event })
    }

    #[cfg(windows)]
    fn wake(&self) {
        // SAFETY: the event stays live for this borrow. Multiple sets
        // deliberately coalesce into one pending frontend wakeup.
        unsafe {
            SetEvent(self.event.as_raw_handle());
        }
    }

    #[cfg(windows)]
    fn drain(&self) {
        // Reset before reading the queue so a later post remains signalled.
        // SAFETY: the event stays live for this borrow.
        unsafe {
            ResetEvent(self.event.as_raw_handle());
        }
    }

    #[cfg(windows)]
    fn wait_handle(&self) -> RawHandle {
        self.event.as_raw_handle()
    }

    #[cfg(not(any(unix, windows)))]
    fn new() -> std::io::Result<Waker> {
        Ok(Waker {})
    }
    #[cfg(not(any(unix, windows)))]
    fn wake(&self) {}
    #[cfg(not(any(unix, windows)))]
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

    #[cfg(windows)]
    #[test]
    fn event_wakes_on_a_job_and_goes_quiet_when_serviced() {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::WaitForSingleObject;

        let (tx, rx) = channel().unwrap();
        // SAFETY: the receiver owns a live waitable event throughout.
        unsafe {
            assert_eq!(WaitForSingleObject(rx.wait_handle(), 0), WAIT_TIMEOUT);
        }
        tx.post(Box::new(|_, _| {})).unwrap();
        // SAFETY: as above.
        unsafe {
            assert_eq!(WaitForSingleObject(rx.wait_handle(), 0), WAIT_OBJECT_0);
        }
        let mut session = Session::new(Config::default());
        let mut host = NullHost;
        assert_eq!(rx.service(&mut session, &mut host), 1);
        // SAFETY: as above.
        unsafe {
            assert_eq!(WaitForSingleObject(rx.wait_handle(), 0), WAIT_TIMEOUT);
        }
    }
}
