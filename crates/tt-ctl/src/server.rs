//! The listener, and one thread per client.
//!
//! Three kinds of thread meet here and the split is the point:
//!
//! - **The frontend's**, which owns the [`Session`](tt_session::Session) and
//!   never blocks on a socket. It calls [`Server::service`] when
//!   [`Server::poll_fd`] fires, and that is its whole involvement.
//! - **The accept thread**, which is blocked on the listener. Unix waits in
//!   `poll(2)` with a stop pipe; Windows wakes a named-pipe accept by making a
//!   private final connection after setting the stop flag.
//! - **A connection thread**, one per client, blocked on a read. It parses,
//!   posts a job, waits for the answer and writes it back.
//!
//! A connection thread blocking is the design rather than a cost: `macro.run`
//! with `wait` blocks for as long as the macro runs, and it does that without
//! the window noticing. Upstream's equivalent of that block is a whole second
//! process.
//!
//! **The accept loop waits on two descriptors rather than polling a flag on a
//! timer.** A terminal that wakes up ten times a second to ask whether it is
//! still wanted is the thing this project keeps out of its event loop; the
//! same rule applies to a thread that will spend its whole life idle.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::addr;
use crate::channel::{channel, CtlReceiver, CtlSender};
use crate::dispatch;
use crate::host::CtlHost;
use crate::ipc::{Listener as IpcListener, Stream};
use crate::proto::{self, Incoming, Response, RpcError, MAX_LINE};

/// A bound socket, before the accept thread has been started.
///
/// Separate from [`Server`] so that a frontend can find out that the address
/// is taken — the one failure that is about the *user's* setup rather than
/// about the machine — before it has spawned anything.
pub struct Listener {
    listener: IpcListener,
    path: PathBuf,
}

impl Listener {
    /// Bind this window's socket. `name` is `/D=`'s topic, or the pid.
    pub fn bind(name: &str) -> std::io::Result<Listener> {
        let path = addr::path_of(name)?;
        Ok(Listener {
            listener: addr::bind(&path)?,
            path,
        })
    }

    /// Bind an explicit path, for a test or for a caller with its own idea of
    /// where sockets live.
    pub fn bind_path(path: &Path) -> std::io::Result<Listener> {
        Ok(Listener {
            listener: addr::bind(path)?,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Start accepting. The frontend keeps the [`Server`] and services it.
    pub fn start(self) -> std::io::Result<Server> {
        let (tx, rx) = channel()?;
        let stop = Stop::new()?;
        let conns = Arc::new(Connections::default());
        let listener = self.listener;
        let accept_conns = conns.clone();
        let accept_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("ttctl-accept".into())
            .spawn(move || accept_loop(listener, tx, accept_stop, accept_conns))?;
        Ok(Server {
            rx,
            path: self.path,
            stop,
            conns,
            thread: Some(thread),
        })
    }
}

/// A running control endpoint. Dropping it closes it and unlinks a Unix socket.
pub struct Server {
    rx: CtlReceiver,
    path: PathBuf,
    stop: Stop,
    conns: Arc<Connections>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Bind and start in one step.
    pub fn start(name: &str) -> std::io::Result<Server> {
        Listener::bind(name)?.start()
    }

    /// Where it is listening. Goes into `$STERNA_CTL` for whatever the window
    /// launches, so a shell running *inside* the terminal can drive it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The descriptor the toolkit waits on. See [`CtlReceiver::poll_fd`].
    #[cfg(unix)]
    pub fn poll_fd(&self) -> std::os::unix::io::RawFd {
        self.rx.poll_fd()
    }

    /// Run whatever the clients have asked for. Returns how many ran.
    ///
    /// **This can start a macro, open a connection and close the window**, so
    /// a frontend must be able to survive all three happening inside it.
    pub fn service(&self, session: &mut tt_session::Session, host: &mut dyn CtlHost) -> usize {
        self.rx.service(session, host)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // In this order: tell the accept thread to stop, hang up on every
        // client so its thread's read returns, then take the Unix file away.
        // A Windows pipe leaves the namespace with its last handle.
        self.stop.set(&self.path);
        self.conns.shutdown_all();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn accept_loop(mut listener: IpcListener, tx: CtlSender, stop: Stop, conns: Arc<Connections>) {
    loop {
        if stop.is_set() {
            return;
        }
        #[cfg(unix)]
        if !wait_readable(&listener, &stop) {
            return;
        }
        let stream = match listener.accept() {
            Ok(s) => s,
            // `EINVAL` is the listener being torn down under us; anything else
            // transient (`ECONNABORTED`, a signal) is worth another turn, and
            // the stop check at the top of the loop is what makes that safe.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        // On Windows this may be the connection `Stop::set` made solely to
        // wake ConnectNamedPipe. It must not become a client thread.
        if stop.is_set() {
            return;
        }
        // The directory is `0700` and the socket `0600`, so this should be
        // unreachable. It is the second lock, and it is cheap.
        if !addr::peer_is_us(&stream) {
            continue;
        }
        let tx = tx.clone();
        let conns2 = conns.clone();
        let id = match conns.add(&stream) {
            Some(id) => id,
            None => continue,
        };
        let spawned = std::thread::Builder::new()
            .name("ttctl-conn".into())
            .spawn(move || {
                serve(stream, &tx);
                conns2.remove(id);
            });
        if spawned.is_err() {
            conns.remove(id);
        }
    }
}

/// One client, until it hangs up or asks for something impossible.
fn serve(stream: Stream, tx: &CtlSender) {
    let mut out = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
        // One line at a time with a ceiling on each, so a peer that never
        // sends a newline cannot make the buffer its own. `Take` is rebuilt
        // per line because its limit is consumed, not reset.
        let n = match (&mut reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => return,
            Ok(n) => n,
            Err(_) => return,
        };
        if n > MAX_LINE {
            // The framing is gone: there is no way to find where this line was
            // meant to end, so there is nothing to answer and nothing to
            // resynchronise to.
            let _ = out.write_all(
                Response::err(
                    Value::Null,
                    RpcError::new(RpcError::INVALID_REQUEST, "request too long"),
                )
                .line()
                .as_bytes(),
            );
            return;
        }
        let text = String::from_utf8_lossy(&line).into_owned();
        let (reply, keep) = handle(&text, tx);
        if let Some(r) = reply {
            if out.write_all(r.line().as_bytes()).is_err() {
                return;
            }
        }
        if !keep {
            return;
        }
    }
}

/// One line in, at most one line out, and whether the connection survives.
///
/// Split out from [`serve`] so the wire can be tested without a socket: this
/// is the whole of what a client sees.
fn handle(line: &str, tx: &CtlSender) -> (Option<Response>, bool) {
    let incoming = match proto::parse(line) {
        Some(i) => i,
        None => return (None, true),
    };
    let req = match incoming {
        Incoming::Bad(r) => return (Some(r), true),
        Incoming::Call(r) => r,
    };
    let id = req.id.clone();
    let result = dispatch::call(tx, &req.method, req.params);
    // A window that has gone cannot answer the next one either, so the
    // connection ends — with the error, so the client learns why rather than
    // seeing a bare hang-up.
    let keep = !matches!(&result, Err(e) if e.code == RpcError::GONE);
    match (id, result) {
        // A notification is answered by silence whether it worked or not,
        // which is §4.1. The work still happened.
        (None, _) => (None, keep),
        (Some(id), Ok(v)) => (Some(Response::ok(id, v)), keep),
        (Some(id), Err(e)) => (Some(Response::err(id, e)), keep),
    }
}

/// A pipe that says "stop", so the accept thread waits on two things and
/// wakes on either.
#[derive(Clone)]
struct Stop {
    flag: Arc<AtomicBool>,
    #[cfg(unix)]
    read: Arc<std::os::unix::net::UnixStream>,
    #[cfg(unix)]
    write: Arc<Mutex<std::os::unix::net::UnixStream>>,
}

impl Stop {
    #[cfg(unix)]
    fn new() -> std::io::Result<Stop> {
        let (read, write) = std::os::unix::net::UnixStream::pair()?;
        read.set_nonblocking(true)?;
        Ok(Stop {
            flag: Arc::new(AtomicBool::new(false)),
            read: Arc::new(read),
            write: Arc::new(Mutex::new(write)),
        })
    }

    #[cfg(windows)]
    fn new() -> std::io::Result<Stop> {
        Ok(Stop {
            flag: Arc::new(AtomicBool::new(false)),
        })
    }

    fn set(&self, _path: &Path) {
        self.flag.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        let _ = self.write.lock().unwrap().write(&[1]);
        #[cfg(windows)]
        let _ = Stream::connect(_path);
    }

    fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// Block until the listener has a connection or the stop pipe has a byte.
///
/// `false` means stop. A `poll` failure is also a stop: there is no error here
/// a retry would fix, and a spinning accept thread would be worse than a
/// missing socket.
#[cfg(unix)]
fn wait_readable(listener: &IpcListener, stop: &Stop) -> bool {
    use std::os::unix::io::AsRawFd;

    let mut fds = [
        libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: stop.read.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: two initialised `pollfd`s, and the descriptors are borrowed
        // from values that outlive the call.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if rc < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        if fds[1].revents != 0 || stop.is_set() {
            return false;
        }
        if fds[0].revents != 0 {
            return true;
        }
    }
}

/// The live connections, so that closing the window hangs up on all of them.
///
/// Without this a client sitting on an open connection keeps its thread — and
/// its socket — alive after the window has gone, which shows up as a test that
/// finishes and a process that does not.
#[derive(Default)]
struct Connections {
    next: AtomicU64,
    live: Mutex<HashMap<u64, Stream>>,
}

impl Connections {
    fn add(&self, stream: &Stream) -> Option<u64> {
        let clone = stream.try_clone().ok()?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.live.lock().ok()?.insert(id, clone);
        Some(id)
    }

    fn remove(&self, id: u64) {
        if let Ok(mut live) = self.live.lock() {
            live.remove(&id);
        }
    }

    fn shutdown_all(&self) {
        if let Ok(live) = self.live.lock() {
            for s in live.values() {
                let _ = s.shutdown();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::NullHost;
    use tt_session::Session;
    use tt_vt::Config;

    /// The frontend, in a test: service until the closure says it is done.
    fn pump<T>(server: &Server, session: &mut Session, mut done: impl FnMut() -> Option<T>) -> T {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            server.service(session, &mut NullHost);
            if let Some(v) = done() {
                return v;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the client never finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[cfg(unix)]
    fn socket() -> (Option<tempfile::TempDir>, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sock");
        (Some(dir), path)
    }

    #[cfg(windows)]
    fn socket() -> (Option<tempfile::TempDir>, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("t{:x}{n:x}", std::process::id());
        (None, crate::addr::path_of(&name).unwrap())
    }

    /// End to end over a real socket: a request in, a response out, and the
    /// work done on the frontend's thread.
    #[test]
    fn a_request_is_answered_over_the_socket() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        let mut session = Session::new(Config::default());
        session.feed(b"\x1b]0;hello\x07");

        let client = std::thread::spawn(move || {
            let mut c = crate::Client::connect(&path).unwrap();
            c.call("status", serde_json::json!({})).unwrap()
        });
        pump(&server, &mut session, || client.is_finished().then_some(()));
        let v = client.join().unwrap();
        assert_eq!(v["title"], serde_json::json!("hello"));
    }

    /// A bad line is answered and the connection carries on, because the
    /// framing is intact once a newline has been found.
    #[test]
    fn a_bad_line_does_not_end_the_conversation() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        let mut session = Session::new(Config::default());

        let client = std::thread::spawn(move || {
            let mut c = crate::Client::connect(&path).unwrap();
            let first = c.raw("nonsense\n").unwrap();
            let second = c.call("ping", serde_json::json!({})).unwrap();
            (first, second)
        });
        pump(&server, &mut session, || client.is_finished().then_some(()));
        let (first, second) = client.join().unwrap();
        assert_eq!(first["error"]["code"], serde_json::json!(RpcError::PARSE));
        assert_eq!(second["pid"], serde_json::json!(std::process::id()));
    }

    /// A notification is done and not answered.
    #[test]
    fn a_notification_gets_no_reply() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        let mut session = Session::new(Config::default());
        session.feed(b"hi");

        let client = std::thread::spawn(move || {
            let mut c = crate::Client::connect(&path).unwrap();
            c.notify("macro.stop", serde_json::json!({})).unwrap();
            // The next request proves the connection survived the silence.
            c.call("ping", serde_json::json!({})).unwrap()
        });
        pump(&server, &mut session, || client.is_finished().then_some(()));
        assert!(client.join().unwrap().get("pid").is_some());
    }

    /// The socket goes when the window does, and so does the accept thread —
    /// a leaked one would hold the file and make the next window's `bind`
    /// look like a collision.
    #[test]
    fn dropping_the_server_unlinks_the_socket() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        #[cfg(unix)]
        assert!(path.exists());
        #[cfg(windows)]
        assert!(crate::addr::list().unwrap().contains(&path));
        drop(server);
        #[cfg(unix)]
        assert!(!path.exists());
        #[cfg(windows)]
        assert!(!crate::addr::list().unwrap().contains(&path));
    }

    /// A client holding an open connection does not keep the window alive.
    #[test]
    fn closing_the_window_hangs_up_on_a_waiting_client() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        let mut client = crate::Client::connect(&path).unwrap();
        drop(server);
        // The read ends rather than blocking; what it reports is either the
        // hang-up or an error, and both are "the window has gone".
        assert!(client.call("ping", serde_json::json!({})).is_err());
    }

    #[test]
    fn a_line_longer_than_the_ceiling_ends_the_connection() {
        let (_dir, path) = socket();
        let server = Listener::bind_path(&path).unwrap().start().unwrap();
        let mut session = Session::new(Config::default());

        let client = std::thread::spawn(move || {
            let mut c = crate::Client::connect(&path).unwrap();
            let long = format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"send\",\"params\":{{\"text\":\"{}\"}}}}\n",
                "x".repeat(MAX_LINE)
            );
            let first = c.raw(&long);
            let second = c.call("ping", serde_json::json!({}));
            (first, second)
        });
        pump(&server, &mut session, || client.is_finished().then_some(()));
        let (first, second) = client.join().unwrap();
        assert_eq!(
            first.unwrap()["error"]["code"],
            serde_json::json!(RpcError::INVALID_REQUEST)
        );
        assert!(second.is_err(), "the connection is gone");
    }
}
