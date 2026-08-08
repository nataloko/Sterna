//! Telnet over TCP.
//!
//! Third transport, and the one that matters most after serial for the reason
//! `PLAN.md` gives: a terminal server puts one TCP port on each serial line,
//! and reaching those ports is the same job as reaching the cable. That also
//! shapes the defaults — see [`TelnetMode`], where "raw" is a first-class
//! choice rather than a degraded one.
//!
//! The protocol is in [`protocol`], with no socket in it, so the parts that
//! break — option negotiation, IAC framing, a command split across two reads —
//! are tested against byte strings rather than against a server.

pub mod protocol;

pub use protocol::{TelnetEvent, TelnetMode, TelnetParams};

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::transport::{Transport, TransportEvent};
use protocol::Telnet;

/// A telnet (or raw TCP) connection.
pub struct TelnetConn {
    socket: TcpStream,
    telnet: Telnet,
    describe: String,
    /// Read scratch, kept so a busy line does not allocate per read.
    buf: Vec<u8>,
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
        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|e| Error::Ssh(format!("cannot resolve {host}: {e}")))?
            .collect();
        if addrs.is_empty() {
            return Err(Error::Ssh(format!("{host} resolved to no addresses")));
        }

        // Every address in turn, which is what makes a dual-stack host work
        // when only one family is routable — a single `connect` to the first
        // AAAA is how "it works from the shell but not from the GUI" happens.
        let mut last = None;
        let mut socket = None;
        for addr in &addrs {
            match TcpStream::connect_timeout(addr, timeout) {
                Ok(s) => {
                    socket = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let socket = match socket {
            Some(s) => s,
            None => {
                let e = last.expect("at least one address was tried");
                return Err(Error::Ssh(format!("cannot connect to {host}:{port}: {e}")));
            }
        };

        // Nagle off: a terminal sends one keystroke at a time and 40 ms of
        // coalescing on every one is exactly the lag people describe as "the
        // GUI feels slow".
        let _ = socket.set_nodelay(true);
        // A read timeout rather than non-blocking, so `read` behaves like the
        // serial one: quiet is `Ok(0)` and cheap.
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(Error::from_io)?;

        let mut conn = TelnetConn {
            socket,
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

    /// Whether the far end agreed to echo. See [`protocol`] for why nothing
    /// here acts on it.
    pub fn server_echoes(&self) -> bool {
        self.telnet.server_echoes()
    }

    /// `IAC AYT` — "are you there". Upstream binds it to a menu item, and it
    /// is the only way to tell a wedged session from a quiet one.
    pub fn are_you_there(&mut self) -> Result<()> {
        self.telnet.queue_are_you_there();
        self.flush_reply()
    }

    fn flush_reply(&mut self) -> Result<()> {
        if !self.telnet.has_reply() {
            return Ok(());
        }
        let reply = self.telnet.take_reply();
        // Protocol replies are tiny and must not be half-written: a truncated
        // subnegotiation desynchronises the far end for the rest of the
        // session. `write_all` is right here and wrong for terminal output.
        self.socket.write_all(&reply).map_err(Error::from_io)
    }
}

impl Transport for TelnetConn {
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize> {
        let before = data.len();
        let n = match self.socket.read(&mut self.buf) {
            Ok(0) => return Err(Error::Disconnected),
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => 0,
            Err(e) if e.kind() == ErrorKind::Interrupted => 0,
            Err(e) => return Err(Error::from_io(e)),
        };

        let mut telnet_events = Vec::new();
        let chunk = self.buf[..n].to_vec();
        self.telnet.feed(&chunk, data, &mut telnet_events);
        for e in telnet_events {
            events.push(match e {
                TelnetEvent::Break => TransportEvent::Break,
                TelnetEvent::Resize { cols, rows } => TransportEvent::Resize { cols, rows },
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

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.telnet.resize(cols, rows);
        self.flush_reply()
    }

    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        use std::os::unix::io::AsRawFd;
        Some(self.socket.as_raw_fd())
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}
