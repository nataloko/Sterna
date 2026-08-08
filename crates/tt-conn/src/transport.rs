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
}

impl From<SerialEvent> for TransportEvent {
    fn from(e: SerialEvent) -> TransportEvent {
        match e {
            SerialEvent::Break => TransportEvent::Break,
            SerialEvent::BadByte(b) => TransportEvent::BadByte(b),
        }
    }
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

    /// Tell the far end the window changed size. Meaningless on a serial
    /// line, which is why it defaults to doing nothing; a pty and SSH both
    /// need it.
    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<()> {
        Ok(())
    }

    /// A short name for the status line — `/dev/ttyUSB0`, `user@host`.
    fn describe(&self) -> String;
}
