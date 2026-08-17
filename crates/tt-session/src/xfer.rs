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
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tt_config::{Settings, TransferXmodemOpt};
use tt_conn::{LinkKind, Result};
use tt_xfer::{Job, Link, LogFlags, Options, Progress, Quirks, Timeouts, Transfer, XmodemOpt};

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

/// Somewhere for a finished transfer's outcome to be *waited* for, rather than
/// collected from the event queue.
///
/// The event is how a frontend hears; this is how a caller on another thread
/// does, and today that caller is a macro. Upstream the two are the same
/// mechanism seen from two processes: a transfer command parks `ttpmacro` in
/// `IdTTLWaitCmndResult` and `ProtoEnd` sends the answer back over DDE, so a
/// script blocks for as long as the transfer takes and hears exactly once how
/// it went. A macro thread here has the same shape and needs the same reply.
///
/// A clone shares the slot — the session keeps one end and the waiter the
/// other — and it is a one-shot: [`take`](TransferReply::take) empties it, and
/// the session drops its end when it fires.
#[derive(Clone, Debug, Default)]
pub struct TransferReply(Arc<(Mutex<Option<TransferOutcome>>, Condvar)>);

impl TransferReply {
    pub fn new() -> TransferReply {
        TransferReply::default()
    }

    /// The session's end: the transfer ended, this is how.
    pub fn post(&self, outcome: TransferOutcome) {
        let (slot, wake) = &*self.0;
        *slot.lock().unwrap() = Some(outcome);
        wake.notify_all();
    }

    /// The waiter's end: block for up to `timeout`, and take the outcome if it
    /// has arrived.
    ///
    /// A timeout rather than a plain wait so that the caller keeps the turn —
    /// a macro has an End button to answer and a frontend that may have gone,
    /// neither of which this can see.
    pub fn wait(&self, timeout: Duration) -> Option<TransferOutcome> {
        let (slot, wake) = &*self.0;
        let guard = slot.lock().unwrap();
        let (mut guard, _) = wake
            .wait_timeout_while(guard, timeout, |o| o.is_none())
            .unwrap();
        guard.take()
    }

    /// The outcome if it is already there, without waiting.
    pub fn take(&self) -> Option<TransferOutcome> {
        self.0 .0.lock().unwrap().take()
    }
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
///
/// `link` is not a setting and cannot be: upstream rebuilds its whole
/// `TTTSet` from the file because the port was opened from the file, while
/// here a session can be over anything. It is the same argument
/// `Session::reset_serial` makes about the live serial parameters.
pub fn xfer_options(s: &Settings, link: Link) -> Options {
    Options {
        link,
        timeouts: Timeouts {
            xmodem: [
                s.transfer_xmodem_timeout_init,
                s.transfer_xmodem_timeout_init_crc,
                s.transfer_xmodem_timeout_short,
                s.transfer_xmodem_timeout_long,
                s.transfer_xmodem_timeout_vlong,
            ],
            ymodem: [
                s.transfer_ymodem_timeout_init,
                s.transfer_ymodem_timeout_init_crc,
                s.transfer_ymodem_timeout_short,
                s.transfer_ymodem_timeout_long,
                s.transfer_ymodem_timeout_vlong,
            ],
            zmodem: [
                s.transfer_zmodem_timeout_normal,
                s.transfer_zmodem_timeout_tcpip,
                s.transfer_zmodem_timeout_init,
                s.transfer_zmodem_timeout_fin,
            ],
        },
        zmodem_data_len: s.transfer_zmodem_data_len,
        zmodem_win_size: s.transfer_zmodem_win_size,
        quickvan_win_size: s.transfer_quickvan_win_size,
        kermit_long_packet: s.transfer_kermit_long_packet,
        kermit_file_attr: s.transfer_kermit_file_attr,
        quirks: Quirks {
            zmodem_escape_ctl: s.transfer_zmodem_escape_ctl,
            bplus_escape_ctl: s.transfer_bplus_escape_ctl,
            auto_rename: s.transfer_auto_rename,
        },
        log: LogFlags {
            kermit: s.transfer_kermit_log,
            xmodem: s.transfer_xmodem_log,
            zmodem: s.transfer_zmodem_log,
            bplus: s.transfer_bplus_log,
            quickvan: s.transfer_quickvan_log,
            ymodem: s.transfer_ymodem_log,
        },
        // `ZMODEM.LOG` and its five siblings go in `ts.LogDirW`
        // (`ttpfile/zmodem.c:815`), which is the **program's** log directory
        // and not the transfer directory the files themselves land in, nor the
        // terminal log's. It takes no settings at all. See
        // [`crate::logname::program_log_dir`].
        log_dir: Some(crate::logname::program_log_dir()),
        // Not a setting: it is the receive dialog's own checkbox, and
        // `transfer.auto_rename` is the setting that makes the answer moot.
        overwrite: Options::default().overwrite,
    }
}

/// The job options a settings file implies, for the three a dialog would
/// otherwise have to invent.
///
/// Separate from [`xfer_options`] because these belong to the [`Job`] rather
/// than to the transfer — which is `tt-xfer`'s own division and the reason
/// XMODEM's block format is not a field of [`Options`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobDefaults {
    /// `XmodemOpt`. **Checksum**, not CRC — the default is the `else` branch
    /// of `ttset.c:1039`'s `_stricmp` chain.
    pub xmodem_opt: XmodemOpt,
    /// `XmodemBin`, which is **on** where `TransBin` below is off. XMODEM
    /// keeps its own flag and upstream ships the two disagreeing.
    pub xmodem_binary: bool,
    /// `TransBin`, the Binary checkbox every other protocol's dialog carries.
    pub binary: bool,
    /// `ZmodemAuto`: whether the terminal starts a receive on seeing a peer's
    /// `ZRQINIT` go past.
    pub zmodem_auto: bool,
    /// `BPAuto`, the same for B-Plus.
    pub bplus_auto: bool,
    /// `ReceivefileAutoStopWaitTime`, for [`Job::Raw`].
    pub raw_autostop: Duration,
}

pub fn job_defaults(s: &Settings) -> JobDefaults {
    JobDefaults {
        xmodem_opt: match s.transfer_xmodem_opt {
            TransferXmodemOpt::Checksum => XmodemOpt::Checksum,
            TransferXmodemOpt::Crc => XmodemOpt::Crc,
            TransferXmodemOpt::Crc1K => XmodemOpt::Crc1K,
            TransferXmodemOpt::Checksum1K => XmodemOpt::Checksum1K,
        },
        xmodem_binary: s.transfer_xmodem_binary,
        binary: s.transfer_binary,
        zmodem_auto: s.transfer_zmodem_auto,
        bplus_auto: s.transfer_bplus_auto,
        // Upstream's is in whole seconds and its own clock only starts at the
        // first byte received — see `Job::Raw`.
        raw_autostop: Duration::from_secs(s.transfer_raw_autostop.max(0) as u64),
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
    /// `name` is for the three jobs that are not told one by the far end —
    /// XMODEM, whose format carries no filename; a raw receive, which pours
    /// the line into whatever it is handed; and a Kermit `GET`, where it is
    /// the *remote* name being asked for. See [`Transfer::receive`].
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

    /// Post the *next* transfer's outcome to `reply` as well as pushing it as
    /// an event.
    ///
    /// Armed by whoever is about to start one and cleared when it fires, so a
    /// caller that blocks on a reply cannot be woken by somebody else's
    /// transfer. Starting one is the frontend's own thread either way, so
    /// arming and starting in the same breath is atomic against pumping.
    pub fn notify_transfer(&mut self, reply: TransferReply) {
        self.xfer_reply = Some(reply);
    }

    /// The one place a transfer ends. Both callers reach it.
    fn finish_transfer(&mut self, outcome: TransferOutcome) {
        // A queued send was held for the length of this transfer
        // (`Session::service_send`), so every deadline it was carrying has
        // passed. Putting them back on the clock is what stops a gated job
        // releasing its next line the instant the protocol lets go — and it has
        // to be here rather than at the end of the pump, because this is the one
        // place both endings reach.
        self.sender.rebase(std::time::Instant::now());
        if let Some(reply) = self.xfer_reply.take() {
            reply.post(outcome.clone());
        }
        self.events
            .push(crate::Event::TransferDone(Box::new(outcome)));
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
            self.finish_transfer(outcome);
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
        self.finish_transfer(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(success: bool) -> TransferOutcome {
        TransferOutcome {
            success,
            cancelled: false,
            message: None,
            bytes: 12,
            elapsed: Duration::from_millis(3),
        }
    }

    /// A clone is the same slot, and the outcome comes out exactly once.
    #[test]
    fn a_reply_carries_the_outcome_across_a_clone_and_empties() {
        let a = TransferReply::new();
        let b = a.clone();
        assert!(b.take().is_none());
        a.post(outcome(true));
        assert_eq!(b.take(), Some(outcome(true)));
        assert!(a.take().is_none());
    }

    /// Waiting for one that never comes gives the turn back rather than
    /// hanging — which is what lets a blocked macro still answer End.
    #[test]
    fn a_wait_with_nothing_posted_gives_up() {
        let r = TransferReply::new();
        let start = std::time::Instant::now();
        assert!(r.wait(Duration::from_millis(20)).is_none());
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn a_wait_returns_as_soon_as_the_other_side_posts() {
        let r = TransferReply::new();
        let poster = r.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            poster.post(outcome(false));
        });
        // Ten times longer than the post will take, so a wait that only woke
        // on its own timeout would be visible.
        let got = r.wait(Duration::from_secs(10));
        assert_eq!(got, Some(outcome(false)));
    }

    // --- the options a file implies ---------------------------------------

    fn from_ini(body: &str) -> Settings {
        Settings::load(&tt_config::Ini::parse(
            format!("[Tera Term]\r\n{body}").as_bytes(),
        ))
    }

    /// With nothing in the file, what comes out is what `tt-xfer` would have
    /// used anyway — which is the check that the two lists of upstream
    /// defaults still agree. They are transcribed independently, so this is
    /// the only thing that would notice one of them drifting.
    #[test]
    fn an_empty_file_gives_the_crates_own_defaults() {
        let link = Link::Network;
        assert_eq!(
            xfer_options(&Settings::default(), link),
            Options {
                link,
                // The one field that is in neither list: a protocol log's
                // directory is the program's own and comes from the
                // environment rather than from the file.
                log_dir: Some(crate::logname::program_log_dir()),
                ..Options::default()
            }
        );
    }

    #[test]
    fn the_file_decides_the_timeouts_and_the_window() {
        let s = from_ini(
            "XmodemTimeouts=1,2,3,4,5\r\n\
             ZmodemTimeouts=7,8,9,11\r\n\
             ZmodemWinSize=4096\r\n\
             ZmodemEscCtl=on\r\n\
             KmtLongPacket=on\r\n\
             ZmodemLog=on\r\n",
        );
        let o = xfer_options(&s, Link::Network);
        assert_eq!(o.timeouts.xmodem, [1, 2, 3, 4, 5]);
        assert_eq!(o.timeouts.zmodem, [7, 8, 9, 11]);
        assert_eq!(o.zmodem_win_size, 4096);
        assert!(o.quirks.zmodem_escape_ctl);
        assert!(o.kermit_long_packet);
        assert!(o.log.zmodem && !o.log.xmodem);
    }

    /// **A timeout floors at 1 rather than taking the default**, which is the
    /// opposite of what every other bounded int in the file does — and
    /// `ZmodemTimeouts`' second field floors at 0, because 0 is how "never
    /// time out" is spelt on a network link.
    #[test]
    fn a_zero_timeout_is_one_second_and_not_the_default() {
        let s = from_ini("XmodemTimeouts=0,0,0,0,0\r\nZmodemTimeouts=0,0,0,0\r\n");
        let o = xfer_options(&s, Link::Network);
        assert_eq!(o.timeouts.xmodem, [1, 1, 1, 1, 1], "ttset.c:1822");
        assert_eq!(o.timeouts.zmodem, [1, 0, 1, 1], "ttset.c:1861 floors at 0");
    }

    /// The XMODEM block format's default is the `else` branch of
    /// `ttset.c:1039`'s `_stricmp` chain — plain checksum — and the writer's
    /// own spelling for it, `checksum`, is not one the reader has an arm for.
    #[test]
    fn the_xmodem_default_is_checksum_and_its_spelling_round_trips() {
        assert_eq!(
            job_defaults(&Settings::default()).xmodem_opt,
            XmodemOpt::Checksum
        );
        assert_eq!(
            job_defaults(&from_ini("XmodemOpt=1k\r\n")).xmodem_opt,
            XmodemOpt::Crc1K
        );
        // What upstream writes, read back.
        assert_eq!(
            job_defaults(&from_ini("XmodemOpt=checksum\r\n")).xmodem_opt,
            XmodemOpt::Checksum
        );
        // ...and so does anything else, which is how it round-trips at all.
        assert_eq!(
            job_defaults(&from_ini("XmodemOpt=nonsense\r\n")).xmodem_opt,
            XmodemOpt::Checksum
        );
    }

    /// XMODEM's binary flag and everyone else's are two settings, and upstream
    /// ships them disagreeing: `XmodemBin` on, `TransBin` off.
    #[test]
    fn the_two_binary_flags_are_not_one_setting() {
        let d = job_defaults(&Settings::default());
        assert!(d.xmodem_binary, "ttset.c:1051");
        assert!(!d.binary, "ttset.c:975");
    }

    /// A protocol log does not go where the files go.
    ///
    /// `ZMODEM.LOG` is written to `ts.LogDirW` (`ttpfile/zmodem.c:815`), the
    /// program's own log directory, which no key in the file moves — so
    /// `FileDir` decides where a *received file* lands and has nothing to say
    /// about the transcript of receiving it.
    #[test]
    fn a_protocol_log_ignores_the_transfer_directory() {
        let expected = crate::logname::program_log_dir();
        assert_eq!(
            xfer_options(&Settings::default(), Link::Network).log_dir,
            Some(expected.clone())
        );
        assert_eq!(
            xfer_options(&from_ini("FileDir=/tmp\r\n"), Link::Network).log_dir,
            Some(expected)
        );
    }
}
