//! The seam every connection presents to the core.
//!
//! Serial, SSH, telnet and a local pty have almost nothing in common
//! internally, and the terminal needs almost nothing from them: bytes in,
//! bytes out, and the handful of things that are *not* bytes — a line break,
//! a byte that arrived corrupted, the far end going away.
//!
//! Keeping that list short is deliberate. Everything specific to a transport
//! — baud rates, host keys, window titles from a pty — is reached through the
//! concrete type before it is boxed, so the trait does not grow a method per
//! protocol.

use std::time::Duration;

use crate::error::Result;
use crate::serial::SerialEvent;

/// Something that arrived on a connection but is not data.
///
/// Deliberately not `SerialEvent`: a break is a serial concept, but a
/// *terminal* has to surface it whatever carried it — telnet has its own
/// `BRK` command and SSH has `break` requests, and both mean the same thing
/// to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    /// A line break.
    Break,
    /// A byte that arrived with a parity or framing error. Kept rather than
    /// dropped: it is usually still readable, and silently losing it makes a
    /// bad cable look like a bad program.
    BadByte(u8),
    /// The **far end** says the terminal should be this size.
    ///
    /// Backwards from the usual direction, and real: telnet's NAWS is defined
    /// client-to-server, and a console server sends it the other way to say
    /// what the equipment behind it actually is. Upstream honours it
    /// (`telnet.c:298`), so a window that ignores it is a window drawing 80
    /// columns at a device that said 132.
    Resize { cols: u16, rows: u16 },
    /// The link negotiated who echoes, and the terminal should now do this.
    ///
    /// Telnet's alone, and only when `TelEcho` is on — see
    /// [`TelnetParams::echo_negotiates`](crate::telnet::TelnetParams::echo_negotiates).
    /// It is an event rather than a state to poll because upstream *assigns*
    /// `ts.LocalEcho` at the two points the option settles, and SRM assigns the
    /// same variable from the wire (`vtterm.c:2053`); a transport that
    /// re-asserted its answer on every read would undo a host's `ESC [ 12 h`
    /// a moment after it arrived.
    LocalEcho(bool),
}

impl From<SerialEvent> for TransportEvent {
    fn from(e: SerialEvent) -> TransportEvent {
        match e {
            SerialEvent::Break => TransportEvent::Break,
            SerialEvent::BadByte(b) => TransportEvent::BadByte(b),
        }
    }
}

/// What is underneath a connection, in the only terms anything above it needs.
///
/// Not "which transport" — that would be a list to extend every time one is
/// added. It is the two properties a protocol running over the link actually
/// branches on: whether delivery is already guaranteed, and how fast the line
/// is. See [`Transport::link_kind`] for who asks and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// A real serial port: bytes can be lost, and the rate is known.
    Serial { baud: u32, seven_bit: bool },
    /// Telnet, SSH — something that retransmits for us, and where a stalled
    /// link is the socket's problem to notice.
    Network,
    /// A local pty. Reliable like a network link, but with no socket
    /// underneath to notice a dead child, so a transfer over it still wants a
    /// timeout.
    LocalPty,
}

/// A byte stream the terminal can talk over.
///
/// `Send` because the frontend will drive this from somewhere other than its
/// UI thread — reads block, and a blocking read on a UI thread is a frozen
/// window.
pub trait Transport: Send {
    /// Read whatever is available, appending data to `data` and anything else
    /// to `events`, and return the number of **data** bytes appended.
    ///
    /// A quiet line is `Ok(0)`, not an error: silence is the normal state of
    /// a serial console and must not look like a failure. Both buffers are
    /// appended to, so one pair can serve the life of the connection.
    fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<TransportEvent>) -> Result<usize>;

    /// Write as much of `data` as it can within `timeout`, returning how much
    /// went. A short write is normal — flow control is entitled to hold the
    /// line — and it is the caller's business to retry the rest.
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize>;

    /// Send a line break, where the transport has one.
    fn send_break(&mut self, _dur: Duration) -> Result<()> {
        Ok(())
    }

    /// Whether [`send_break`](Transport::send_break) will do anything.
    ///
    /// Separate from trying it and failing, because a frontend needs to know
    /// *before* it draws the menu. Offering "Send break" on a connection that
    /// cannot is offering an error message, and a break is the kind of thing
    /// someone reaches for when a console has stopped answering — which is
    /// exactly the wrong moment to find out.
    fn supports_break(&self) -> bool {
        true
    }

    /// Tell the far end the window changed size. Meaningless on a serial
    /// line, which is why it defaults to doing nothing; a pty and SSH both
    /// need it.
    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<()> {
        Ok(())
    }

    /// A chance to do something the clock asks for rather than the wire.
    ///
    /// Called from a timer, not from the read loop, and that is the point: a
    /// transport whose only wakeup is "bytes arrived" cannot act on a link
    /// having gone *quiet*. Telnet's keepalive is the one caller so far, and an
    /// idle link is exactly the link it exists for.
    ///
    /// Cheap enough to call at any rate the frontend likes; upstream's own
    /// keepalive thread wakes ten times a second.
    fn tick(&mut self) -> Result<()> {
        Ok(())
    }

    /// Whether `TCPLocalEcho` and `TCPCRSend` apply to this connection.
    ///
    /// Upstream's condition is the `else` of the arm that sends telnet's
    /// opening burst (`vtwin.cpp:3690`): a TCP session that is **not** a telnet
    /// session, which is a raw socket or a telnet-framed console port, and not
    /// SSH — TTSSH sets `ts.DisableTCPEchoCR` on the way in (`ttxssh.c:971`)
    /// precisely so its sessions are excluded.
    ///
    /// It is on the trait rather than resolved by the caller because the two
    /// settings are applied where the connection is attached, and by then the
    /// concrete type is gone.
    fn tcp_without_telnet(&self) -> bool {
        false
    }

    /// A descriptor that becomes readable when there is something to
    /// [`read`](Transport::read), for a frontend that would rather wait in its
    /// own event loop than poll ours.
    ///
    /// This exists because the alternative is bad in both directions. A UI
    /// thread that calls `read` directly blocks for the transport's read
    /// timeout on every quiet line — which is the *normal* state of a serial
    /// console — and one that polls on a timer burns a wakeup per frame
    /// forever to discover nothing happened. Handing out the descriptor lets
    /// the toolkit do what it is already good at: Qt's `QSocketNotifier`,
    /// `poll(2)`, `epoll`, whatever the frontend runs on.
    ///
    /// Two caveats, both of which the caller must be able to take:
    ///
    /// - **Readable does not promise bytes.** `read` may still return `Ok(0)`
    ///   — the break decoder can be holding a partial `PARMRK` escape whose
    ///   remaining bytes have not arrived. Treat readiness as a wakeup, not as
    ///   a guarantee.
    /// - **The descriptor is borrowed and dies with the transport.** It is
    ///   only valid while this `Transport` is alive; a frontend that caches it
    ///   across a reconnect is watching a closed or recycled fd.
    ///
    /// `None` means this is not the target's native wait primitive. Windows
    /// uses [`wait_handle`](Transport::wait_handle) instead.
    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        None
    }

    /// A waitable event that is signalled when there is something to
    /// [`read`](Transport::read), for a Windows frontend.
    ///
    /// This is the native spelling of [`poll_fd`](Transport::poll_fd), with
    /// the same borrowed lifetime and wakeup-not-bytes contract. It is an
    /// event `HANDLE`, not an ordinary file handle: wait on it with
    /// `WaitForSingleObject`, `QWinEventNotifier`, or an equivalent event-loop
    /// primitive. The transport owns it and resets it at the start of `read`.
    #[cfg(windows)]
    fn wait_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        None
    }

    /// What kind of link this is, for the one caller that has to know.
    ///
    /// This is the exception the rest of this trait argues against, and it
    /// earns its place: Tera Term's file-transfer protocols branch on it
    /// directly. `xmodem.c:347`, `ymodem.c:417` and `zmodem.c:788` each pick a
    /// different timeout set from `cv->PortType`, ZMODEM caps its block size
    /// at 1 KB on a network link and scales it off the baud rate otherwise,
    /// and `kermit.c:1213` uses it to decide whether the eighth bit needs
    /// quoting. Getting it wrong is not cosmetic — the network branch means
    /// "no timeout at all", which on a link that can go quiet is a transfer
    /// that hangs for ever.
    ///
    /// One question, answered by every transport, rather than a method per
    /// protocol. The session cannot reach the concrete type: it holds a
    /// `Box<dyn Transport>` from the moment it is connected.
    fn link_kind(&self) -> LinkKind {
        LinkKind::Network
    }

    /// The serial port underneath, when there is one.
    ///
    /// The trait's one downcast, and it is here for the same reason
    /// [`link_kind`](Transport::link_kind) is: the session cannot reach the
    /// concrete type once it holds a box. The alternative is four more
    /// methods — DTR, RTS, the speed, the modem lines — that three transports
    /// out of four would implement only to decline, and a fifth that returns
    /// something meaningless off a socket.
    ///
    /// It stays one method because the *commands* are serial-only by
    /// definition rather than by omission: `setdtr`, `setrts`, `setbaud`,
    /// `setflowctrl` and `getmodemstatus` are each guarded by
    /// `cv.PortType != IdSerial` in `ttdde.c` and do nothing at all otherwise.
    /// `None` is that guard, and it covers "not open" at the same time.
    fn as_serial(&mut self) -> Option<&mut crate::serial::SerialConn> {
        None
    }

    /// A short name for the status line — `/dev/ttyUSB0`, `user@host`.
    fn describe(&self) -> String;

    /// Why the connection ended, when the transport knows something the word
    /// "disconnected" does not say.
    ///
    /// Called once, after a read or write reports
    /// [`Disconnected`](crate::Error::Disconnected) and before the transport
    /// is dropped. Most have nothing to add — an unplugged adapter and a
    /// closed socket are exactly what they look like — but a local shell does:
    /// "bash exited with status 1" is a different message from "the device
    /// disconnected", and it is the one that says whether anything went wrong.
    fn closing_note(&mut self) -> Option<String> {
        None
    }
}
