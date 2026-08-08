//! A file transfer running over the session's own connection.
//!
//! `tt-xfer` is transport-agnostic on purpose — it is fed bytes and asked for
//! bytes — and this is the piece that decides where those bytes come from.
//! While a transfer is up the session's reader hands its input to the protocol
//! instead of to the VT engine, and the protocol's output goes out through the
//! same queue a keystroke would.
//!
//! **The terminal is deaf and mute for the duration**, which is upstream's
//! behaviour too: `filesys_proto.cpp` runs its transfer behind a modal dialog.
//! Letting a keystroke through would put a stray byte in the middle of a
//! packet, and letting the protocol's traffic reach the parser would paint a
//! screenful of `**\x18B00`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tt_config::Settings;
use tt_conn::{LinkKind, Result};
use tt_xfer::{Job, Link, Options, Progress, Transfer};

/// What a frontend needs to draw a progress dialog.
///
/// A snapshot rather than a borrow of the [`Transfer`], because the frontend
/// is across the C ABI and cannot hold one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferStatus {
    /// "ZMODEM", "Kermit" — the protocol's own word for itself.
    pub protocol: String,
    /// The file in flight, as the protocol named it. Empty before the first
    /// one is opened.
    pub file: String,
    pub sending: bool,
    pub progress: Progress,
}

/// How a finished transfer finished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOutcome {
    pub success: bool,
    pub cancelled: bool,
    /// What the protocol said when it failed — "Cannot create file". Upstream
    /// puts this in a message box, and it is often the only account of the
    /// failure there is.
    pub message: Option<String>,
    /// Bytes moved, for the status line.
    pub bytes: i64,
    pub elapsed: Duration,
}

pub(crate) struct Running {
    pub(crate) xfer: Transfer,
    pub(crate) sending: bool,
}

impl Running {
    pub(crate) fn status(&self) -> TransferStatus {
        TransferStatus {
            protocol: self.xfer.protocol_name().unwrap_or_default(),
            file: self.xfer.file_name().unwrap_or_default(),
            sending: self.sending,
            progress: self.xfer.progress(),
        }
    }

    pub(crate) fn outcome(&self) -> TransferOutcome {
        let p = self.xfer.progress();
        TransferOutcome {
            success: self.xfer.succeeded(),
            cancelled: self.xfer.was_cancelled(),
            message: self.xfer.message(),
            bytes: p.bytes,
            elapsed: p.elapsed,
        }
    }
}

/// The transfer options implied by a settings file and a connection.
///
/// The mirror of [`vt_config`](crate::vt_config), and the same argument: the
/// values a transfer runs on are settings, so they come from the settings
/// rather than from a second list. What is not in the schema yet keeps
/// `tt-xfer`'s defaults, which are upstream's with a `ttset.c` citation each —
/// so a value is never invented here, only sourced from one place or the
/// other.
pub fn xfer_options(_settings: &Settings, link: Link) -> Options {
    Options {
        link,
        ..Options::default()
    }
}

/// What the connection underneath is, in the terms the protocols branch on.
pub fn link_of(kind: LinkKind) -> Link {
    match kind {
        LinkKind::Serial { baud, seven_bit } => Link::Serial { baud, seven_bit },
        LinkKind::Network => Link::Network,
        // Reliable, but with no socket underneath to notice a dead child —
        // and the network branch means "no timeout at all". See
        // `Link::local_pty`.
        LinkKind::LocalPty => Link::local_pty(),
    }
}

impl crate::Session {
    /// Start sending files over the connection.
    ///
    /// Fails if there is no connection or a transfer is already running —
    /// both of which a frontend should have greyed the menu out for, but
    /// neither of which it can be trusted to have.
    pub fn send_files<P: AsRef<Path>>(
        &mut self,
        job: Job,
        files: &[P],
        opts: &Options,
    ) -> std::result::Result<(), TransferError> {
        self.check_can_start()?;
        let paths: Vec<PathBuf> = files.iter().map(|p| p.as_ref().to_path_buf()).collect();
        let xfer = Transfer::send(job, &paths, opts)?;
        self.begin(xfer, true);
        Ok(())
    }

    /// Start receiving into `dir`.
    ///
    /// `name` is XMODEM's alone: its wire format carries no filename, so there
    /// is nothing to derive a destination from and the caller supplies one.
    pub fn receive_files(
        &mut self,
        job: Job,
        dir: &Path,
        name: Option<&str>,
        opts: &Options,
    ) -> std::result::Result<(), TransferError> {
        self.check_can_start()?;
        let xfer = Transfer::receive(job, dir, name, opts)?;
        self.begin(xfer, false);
        Ok(())
    }

    fn check_can_start(&self) -> std::result::Result<(), TransferError> {
        if self.conn.is_none() {
            return Err(TransferError::NotConnected);
        }
        if self.xfer.is_some() {
            return Err(TransferError::AlreadyRunning);
        }
        Ok(())
    }

    fn begin(&mut self, xfer: Transfer, sending: bool) {
        // Anything the user typed that has not gone out yet must not be
        // interleaved with the protocol's first packet.
        self.pending.clear();
        self.xfer = Some(Running { xfer, sending });
    }

    /// Whether a transfer is running, and how it is doing.
    pub fn transfer(&self) -> Option<TransferStatus> {
        self.xfer.as_ref().map(Running::status)
    }

    /// The options a transfer would run on right now: this session's settings,
    /// and what the connection underneath actually is.
    ///
    /// With nothing connected the link reports as a local pty, which is the
    /// conservative answer — it always has a timeout. Starting a transfer with
    /// no connection fails anyway.
    pub fn transfer_options(&self) -> Options {
        let link = self
            .conn
            .as_ref()
            .map_or(Link::local_pty(), |c| link_of(c.link_kind()));
        xfer_options(&self.settings, link)
    }

    /// Ask the running transfer to stop.
    ///
    /// It does not stop here: the protocol sends its cancel sequence and
    /// finishes on its own terms — ZMODEM arms a 500 ms timer and ends on
    /// that. The frontend keeps pumping and waits for
    /// [`Event::TransferDone`](crate::Event::TransferDone).
    pub fn cancel_transfer(&mut self) {
        if let Some(running) = self.xfer.as_mut() {
            running.xfer.cancel();
        }
    }

    /// How long the caller may sleep before the transfer needs attention.
    ///
    /// **A frontend that only wakes on the descriptor will stall a transfer.**
    /// The protocols retry by timeout — an XMODEM receiver that hears nothing
    /// re-sends its `NAK` after ten seconds — and a quiet line produces no
    /// wakeup at all, so nothing would ever fire it. Arm a timer for this.
    pub fn transfer_deadline(&self) -> Option<Duration> {
        self.xfer.as_ref().and_then(|r| r.xfer.wait_hint())
    }
}

/// Why a transfer could not be started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    NotConnected,
    AlreadyRunning,
    Protocol(tt_xfer::Error),
}

impl From<tt_xfer::Error> for TransferError {
    fn from(e: tt_xfer::Error) -> TransferError {
        TransferError::Protocol(e)
    }
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::NotConnected => write!(f, "not connected"),
            TransferError::AlreadyRunning => write!(f, "a transfer is already running"),
            TransferError::Protocol(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransferError {}

impl crate::Session {
    /// Give the running transfer the bytes that just arrived, and take back
    /// whatever it wants written. Returns whether the transfer finished.
    ///
    /// Called from [`pump`](crate::Session::pump) in place of feeding the VT.
    pub(crate) fn pump_transfer(&mut self, incoming: &[u8]) -> Result<bool> {
        // Taken out for the duration rather than borrowed, because writing
        // what it produces goes back through the session — and a transfer
        // cannot be started from inside its own pump, so nothing can observe
        // the gap.
        let Some(mut running) = self.xfer.take() else {
            return Ok(false);
        };

        let mut at = 0;
        loop {
            if at < incoming.len() {
                at += running.xfer.feed(&incoming[at..]);
            }
            running.xfer.poll();
            running.xfer.take_output(&mut self.pending);
            // A 64 KB queue and a protocol that has not drained it: parsing is
            // what empties it, and the poll above is the parse. Round again
            // rather than dropping the tail on the floor.
            if at >= incoming.len() || running.xfer.is_done() {
                break;
            }
        }

        // A timeout that is already due fires now. The frontend is supposed to
        // arm a timer from `transfer_deadline`, but a busy line reaches here
        // often enough that waiting for the timer would add latency to every
        // retry — and a frontend that forgot still limps rather than hanging.
        if !running.xfer.is_done() && running.xfer.wait_hint() == Some(Duration::ZERO) {
            running.xfer.fire_timeout();
            running.xfer.take_output(&mut self.pending);
        }

        self.flush_pending()?;

        if running.xfer.is_done() {
            let outcome = running.outcome();
            self.events
                .push(crate::Event::TransferDone(Box::new(outcome)));
            Ok(true)
        } else {
            let status = running.status();
            self.events
                .push(crate::Event::TransferProgress(Box::new(status)));
            self.xfer = Some(running);
            Ok(false)
        }
    }

    /// The transport went away under a running transfer.
    pub(crate) fn transfer_disconnected(&mut self) {
        let Some(mut running) = self.xfer.take() else {
            return;
        };
        // Not the same as cancelling, and the protocols distinguish it: each
        // tests `cv->Ready` and the ones that cannot finish call `ProtoEnd`
        // there and then, because they know Parse will not run again.
        running.xfer.disconnected();
        running.xfer.poll();
        let outcome = running.outcome();
        self.events
            .push(crate::Event::TransferDone(Box::new(outcome)));
    }
}
