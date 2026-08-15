//! Telnet over TCP.
//!
//! Third transport, and the one that matters most after serial for the reason
//! `docs/history.md` gives: a terminal server puts one TCP port on each serial line,
//! and reaching those ports is the same job as reaching the cable. That also
//! shapes the defaults — see [`TelnetMode`], where "raw" is a first-class
//! choice rather than a degraded one.
//!
//! The protocol is in [`protocol`], with no socket in it, so the parts that
//! break — option negotiation, IAC framing, a command split across two reads —
//! are tested against byte strings rather than against a server.

pub mod protocol;

pub use protocol::{TelnetEvent, TelnetMode, TelnetParams};

use std::fs::File;
#[cfg(not(windows))]
use std::io::Read;
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
#[cfg(windows)]
use std::os::windows::io::RawHandle;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::transport::{Transport, TransportEvent};
#[cfg(windows)]
use crate::windows_reader::WindowsReader;
use protocol::Telnet;

/// A telnet (or raw TCP) connection.
pub struct TelnetConn {
    socket: TcpStream,
    /// A blocking clone of `socket`, kept off the frontend thread. The
    /// original remains synchronous for timed writes and protocol replies.
    #[cfg(windows)]
    reader: WindowsReader,
    telnet: Telnet,
    describe: String,
    /// Read scratch, kept so a busy line does not allocate per read.
    buf: Vec<u8>,
    /// `cv.LastSendTime` — when anything last went out, which is what the
    /// keepalive measures against. Stamped by every send including the NOP,
    /// exactly as `commlib.c:1062` stamps it inside `CommSend`.
    last_send: Instant,
    /// `ts.TelKeepAliveInterval`, and `None` where upstream would not have
    /// started the thread at all. See [`TelnetConn::poll_keepalive`].
    keepalive: Option<Duration>,
    /// Which of the four [`TelnetMode`]s this opened in, kept because two
    /// settings above depend on it after the negotiation is under way.
    mode: TelnetMode,
    /// `TELNET.LOG`, open only when `TelLog` asked for it.
    log: Option<File>,
}

impl TelnetConn {
    /// Connect, with a timeout on the socket rather than on the whole call.
    ///
    /// `params.mode` decides how much of the protocol is spoken;
    /// [`TelnetMode::for_port`] gives upstream's rule if there is no reason to
    /// override it.
    pub fn connect(
        host: &str,
        port: u16,
        params: &TelnetParams,
        timeout: Duration,
    ) -> Result<TelnetConn> {
        // Through the proxy when the file names one, and straight to the host
        // when it does not — including trying every address the name has,
        // which is what makes a dual-stack host work when only one family is
        // routable. `dial` also sets Nagle off: a terminal sends one keystroke
        // at a time and 40 ms of coalescing on every one is exactly the lag
        // people describe as "the GUI feels slow".
        let socket = crate::proxy::dial(params.proxy.as_deref(), host, port, timeout)?;
        // A read timeout rather than non-blocking, so Unix `read` behaves like
        // the serial one: quiet is `Ok(0)` and cheap. Windows blocks a worker
        // on a cloned socket instead; a timeout there would turn an idle line
        // into a 20 Hz wakeup and make timeout indistinguishable from EOF to
        // the generic worker.
        #[cfg(not(windows))]
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(Error::from_io)?;
        #[cfg(windows)]
        let reader = WindowsReader::start(
            Box::new(socket.try_clone().map_err(Error::from_io)?),
            "sterna-telnet-read",
        )?;

        let mut conn = TelnetConn {
            socket,
            #[cfg(windows)]
            reader,
            // `CREATE_ALWAYS` (`telnet.c:129`) — the log is this connection's,
            // not a running one, so a failure to open it is not a failure to
            // connect. Upstream does not check either.
            log: params.log.as_deref().and_then(open_log),
            // Upstream starts the keepalive thread inside the arm that sends
            // the opening burst (`vtwin.cpp:3688`), so a telnet-framed session
            // at a port that is not the telnet port gets no NOPs however the
            // interval is set. Reproduced rather than tidied: the setting is
            // about a telnet server, and a console server's per-line port has
            // its own idea of what an idle line means.
            keepalive: match params.mode {
                TelnetMode::Negotiate => params.keepalive.filter(|d| !d.is_zero()),
                _ => None,
            },
            last_send: Instant::now(),
            mode: params.mode,
            telnet: Telnet::new(params.clone()),
            describe: if port == 23 {
                host.to_string()
            } else {
                format!("{host}:{port}")
            },
            buf: vec![0u8; 8192],
        };
        // The opening burst, where there is one.
        conn.flush_reply()?;
        Ok(conn)
    }

    /// Whether the far end agreed to echo.
    ///
    /// The negotiated state, always tracked. What acts on it is
    /// [`TransportEvent::LocalEcho`], and only when `TelEcho` is on — see
    /// [`TelnetParams::echo_negotiates`].
    pub fn server_echoes(&self) -> bool {
        self.telnet.server_echoes()
    }

    /// `TelKeepAliveThread` (`telnet.c:904`), without the thread.
    ///
    /// Upstream polls this every 100 ms from a thread of its own and sends an
    /// `IAC NOP` when the line has been quiet for the interval. Here the caller
    /// owns the clock, because a socket that nothing is arriving on wakes
    /// nothing: the frontend's read notifier never fires on an idle link, which
    /// is exactly the link a keepalive exists for. Call it from a timer.
    ///
    /// **The interval is a quiet period, not a period.** The comparison is
    /// against the last time anything went out — `cv.LastSendTime`, which
    /// `commlib.c:1062` stamps inside `CommSend` for every telnet send, the NOP
    /// included — so a session being typed at sends none at all, and one that
    /// is idle sends one every interval.
    pub fn poll_keepalive(&mut self) -> Result<()> {
        let Some(interval) = self.keepalive else {
            return Ok(());
        };
        if self.last_send.elapsed() < interval {
            return Ok(());
        }
        self.telnet.queue_nop();
        self.flush_reply()
    }

    fn write_log(&mut self) {
        let Some(file) = self.log.as_mut() else {
            return;
        };
        let text = self.telnet.take_log();
        if !text.is_empty() {
            let _ = file.write_all(text.as_bytes());
        }
    }

    /// `IAC AYT` — "are you there". Upstream binds it to a menu item, and it
    /// is the only way to tell a wedged session from a quiet one.
    pub fn are_you_there(&mut self) -> Result<()> {
        self.telnet.queue_are_you_there();
        self.flush_reply()
    }

    fn flush_reply(&mut self) -> Result<()> {
        self.write_log();
        if !self.telnet.has_reply() {
            return Ok(());
        }
        let reply = self.telnet.take_reply();
        self.last_send = Instant::now();
        // Protocol replies are tiny and must not be half-written: a truncated
        // subnegotiation desynchronises the far end for the rest of the
        // session. `write_all` is right here and wrong for terminal output.
        self.socket.write_all(&reply).map_err(Error::from_io)
    }
}

/// `TELNET.LOG`, truncated. `None` if it cannot be opened, which is not an
/// error anybody is told about — upstream ignores the handle's value too.
fn open_log(path: &Path) -> Option<File> {
    File::create(path).ok()
}

impl Transport for TelnetConn {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        let before = data.len();
        #[cfg(not(windows))]
        let n = match self.socket.read(&mut self.buf) {
            Ok(0) => return Err(Error::Disconnected),
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => 0,
            Err(e) if e.kind() == ErrorKind::Interrupted => 0,
            Err(e) => return Err(Error::from_io(e)),
        };
        #[cfg(windows)]
        let n = {
            self.buf.clear();
            self.reader.read(&mut self.buf)?
        };

        let mut telnet_events = Vec::new();
        let chunk = self.buf[..n].to_vec();
        self.telnet.feed(&chunk, data, &mut telnet_events);
        for e in telnet_events {
            events.push(match e {
                TelnetEvent::Break => TransportEvent::Break,
                TelnetEvent::Resize { cols, rows } => TransportEvent::Resize { cols, rows },
                TelnetEvent::LocalEcho(on) => TransportEvent::LocalEcho(on),
            });
        }
        // Negotiation replies go out here rather than waiting for the caller
        // to write something: a server that asked for the window size and got
        // silence will sit on it.
        self.flush_reply()?;
        Ok(data.len() - before)
    }

    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        let mut escaped = Vec::with_capacity(data.len());
        self.telnet.encode(data, &mut escaped);
        let _ = self.socket.set_write_timeout(Some(timeout));
        // Stamped before the write rather than after it, and unconditionally:
        // `CommSend` stamps `cv.LastSendTime` at the top of the function, ahead
        // of the send that may fail (`commlib.c:1062`). A keepalive is about
        // the line having gone quiet, and an attempt is not quiet.
        self.last_send = Instant::now();
        match self.socket.write(&escaped) {
            // Reporting *escaped* bytes written would be a lie the caller
            // would act on — it retries from the count. A partial write of an
            // escaped stream cannot be mapped back to an input offset, so the
            // whole thing goes or none of it does.
            Ok(n) if n == escaped.len() => Ok(data.len()),
            Ok(_) => Ok(0),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(0),
            Err(e) => Err(Error::from_io(e)),
        }
    }

    /// `IAC BRK`, which a console server turns into a real break on the serial
    /// port behind it. The one place telnet does something SSH cannot.
    fn send_break(&mut self, _dur: Duration) -> Result<()> {
        self.telnet.queue_break();
        self.flush_reply()
    }

    fn supports_break(&self) -> bool {
        true
    }

    fn tick(&mut self) -> Result<()> {
        self.poll_keepalive()
    }

    /// True for everything but a session that opened with the burst — which is
    /// upstream's `else`, not a separate test.
    fn tcp_without_telnet(&self) -> bool {
        self.mode != TelnetMode::Negotiate
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.telnet.resize(cols, rows);
        self.flush_reply()
    }

    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        use std::os::unix::io::AsRawFd;
        Some(self.socket.as_raw_fd())
    }

    #[cfg(windows)]
    fn wait_handle(&self) -> Option<RawHandle> {
        Some(self.reader.wait_handle())
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

#[cfg(windows)]
impl Drop for TelnetConn {
    fn drop(&mut self) {
        // Dropping the original socket is not enough: the blocking reader
        // owns a clone. Shutdown applies to the underlying connection and
        // wakes that clone before its receiver and event are dropped.
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    #[test]
    fn reader_event_orders_idle_data_and_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            release_rx.recv().unwrap();
            socket.write_all(b"telnet-worker-ok").unwrap();
            socket.shutdown(std::net::Shutdown::Write).unwrap();
        });

        let mut conn = TelnetConn::connect(
            "127.0.0.1",
            addr.port(),
            &TelnetParams {
                mode: TelnetMode::Raw,
                ..TelnetParams::default()
            },
            Duration::from_secs(5),
        )
        .unwrap();
        let handle = conn.wait_handle().unwrap();

        // A quiet socket does not wake the frontend just to report no bytes.
        // SAFETY: `conn` owns this event for the duration of the wait.
        assert_eq!(unsafe { WaitForSingleObject(handle, 0) }, WAIT_TIMEOUT);
        release_tx.send(()).unwrap();

        // SAFETY: as above.
        assert_eq!(unsafe { WaitForSingleObject(handle, 5000) }, WAIT_OBJECT_0);
        let (mut data, mut events) = (Vec::new(), Vec::new());
        assert_eq!(conn.read(&mut data, &mut events).unwrap(), 16);
        assert_eq!(data, b"telnet-worker-ok");
        assert!(events.is_empty());

        // The worker can observe data and EOF before the frontend drains
        // either. Reading the data re-signals its manual-reset event so the
        // final bytes are never hidden behind the close.
        // SAFETY: as above.
        assert_eq!(unsafe { WaitForSingleObject(handle, 5000) }, WAIT_OBJECT_0);
        assert!(matches!(
            conn.read(&mut data, &mut events),
            Err(Error::Disconnected)
        ));

        server.join().unwrap();
    }
}
