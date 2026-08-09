//! tt-xfer — X/Y/ZMODEM, Kermit, B-Plus and Quick-VAN.
//!
//! The protocols themselves are Tera Term's, vendored verbatim under
//! `vendor/ttpfile/` and compiled in. This crate is the host they attach to:
//! the three vtables in `csrc/tt_xfer.c`, and the loop that drives them here.
//! Nothing is reimplemented, which is the point — B-Plus was CompuServe's and
//! Quick-VAN was NIFTY-Serve's, both services are gone, and a rewrite of
//! either could only ever be checked against a reading of the code it
//! replaced.
//!
//! **A transfer does not own a connection.** It is fed bytes and asked for
//! bytes, because it runs over the terminal's own link: the same reader that
//! feeds the VT engine hands its bytes here instead while a transfer is up.
//!
//! ```no_run
//! use std::time::Duration;
//! use tt_xfer::{Direction, Job, Options, Transfer};
//!
//! let mut xfer = Transfer::send(
//!     Job::ZModem { dir: Direction::Send, binary: true, auto: false },
//!     ["report.bin"],
//!     &Options::default(),
//! )?;
//!
//! let mut out = Vec::new();
//! while !xfer.is_done() {
//!     // ...read from the connection into `chunk`, then:
//!     let chunk: &[u8] = &[];
//!     xfer.feed(chunk);
//!     xfer.poll();
//!     out.clear();
//!     xfer.take_output(&mut out);
//!     // ...write `out` to the connection, and wait xfer.wait_hint().
//! }
//! assert!(xfer.succeeded());
//! # Ok::<(), tt_xfer::Error>(())
//! ```

use std::ffi::{CStr, CString};
use std::path::Path;
use std::time::Duration;

mod ffi;
mod options;

pub use options::{
    Direction, Job, KermitMode, Link, LogFlags, Options, Quirks, Timeouts, XmodemOpt, YmodemOpt,
};

/// Why a transfer could not be set up. Failures *during* a transfer are not
/// errors in this sense — the protocol reports them by finishing without
/// success, and [`Transfer::message`] is what it has to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A path with an interior NUL, or one that is not valid UTF-8. The
    /// protocols take UTF-8 paths and nothing else.
    BadPath(String),
    /// Sending with no files, or receiving with no directory.
    NothingToDo,
    /// `Create` or `Init` refused. Out of memory, or an option combination the
    /// protocol will not take.
    Refused(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadPath(p) => write!(f, "path is not usable: {p}"),
            Error::NothingToDo => write!(f, "no files to send and no directory to receive into"),
            Error::Refused(what) => write!(f, "the protocol refused to start: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// Where a transfer has got to. Every field is something the protocol told
/// its progress dialog; none is inferred.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Bytes of file content moved so far.
    pub bytes: i64,
    /// Packets sent or received, for the protocols that count them.
    pub packets: i64,
    /// Position within the current file, and its size. `total` is 0 when the
    /// size is not known — XMODEM never learns it, and ZMODEM only does if
    /// the sender said.
    pub done: i64,
    pub total: i64,
    /// The whole-percent high-water mark upstream keeps, or -1 when the
    /// protocol has said there is no meaningful bar to draw.
    pub percent: i32,
    pub elapsed: Duration,
}

/// A running transfer.
///
/// Not `Sync`: the C side keeps its current instance in a thread-local so that
/// the globals Tera Term's protocols reach for — `MessageBox`, `ProtoEnd`,
/// `SetTimer` — land on the right transfer. It **is** `Send`, so a session can
/// own one on whichever thread it pumps from.
pub struct Transfer {
    raw: *mut ffi::TtXfer,
    inited: bool,
    /// Held for the lifetime of the transfer: `TtXferOpts::log_dir` is a
    /// borrowed pointer and the C side reads it during `create`.
    _log_dir: Option<CString>,
}

// Safe: everything the handle reaches is owned by the handle, and the one
// piece of shared state on the C side is thread-local. Two transfers on two
// threads do not touch each other; the same transfer on two threads at once is
// what `!Sync` forbids.
unsafe impl Send for Transfer {}

impl Transfer {
    /// Send one or more files.
    pub fn send<I, P>(job: Job, files: I, opts: &Options) -> Result<Transfer, Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let files: Vec<CString> = files
            .into_iter()
            .map(|p| cstr(p.as_ref()))
            .collect::<Result<_, _>>()?;
        if files.is_empty() {
            return Err(Error::NothingToDo);
        }
        let mut t = Transfer::create(job, opts)?;
        for f in &files {
            // SAFETY: `t.raw` is live and `f` outlives the call, which copies.
            if unsafe { ffi::tt_xfer_add_send_file(t.raw, f.as_ptr()) } == 0 {
                return Err(Error::Refused("could not record the file list"));
            }
        }
        t.start()?;
        Ok(t)
    }

    /// Receive into `dir`.
    ///
    /// `name` is for the three jobs whose destination does not come off the
    /// wire, and it means something different in each:
    ///
    /// - **XMODEM**, whose format carries no filename at all, so there is
    ///   nothing to derive a destination from.
    /// - **Raw**, which is not a protocol: `raw.c:80` opens `GetNextFname`'s
    ///   answer for writing and pours the line into it.
    /// - **A Kermit `GET`**, where it is not a destination at all but the
    ///   *remote* name to ask for — `kermit.c:1159` takes it from the same
    ///   list and puts its **basename** in the `R` packet, so a directory in
    ///   it cannot reach the peer.
    ///
    /// Every other protocol takes the name its sender gives, and supplying one
    /// would override the peer's — which is what upstream's receive dialog
    /// does with the same field.
    pub fn receive(
        job: Job,
        dir: impl AsRef<Path>,
        name: Option<&str>,
        opts: &Options,
    ) -> Result<Transfer, Error> {
        let dir = dir.as_ref();
        let cdir = cstr(dir)?;
        let mut t = Transfer::create(job, opts)?;
        // SAFETY: `t.raw` is live; the call copies the string.
        if unsafe { ffi::tt_xfer_set_recv_dir(t.raw, cdir.as_ptr()) } == 0 {
            return Err(Error::Refused("could not record the receive directory"));
        }
        if job.needs_name() {
            let Some(name) = name else {
                return Err(Error::NothingToDo);
            };
            // All three reach for it through GetNextFname, which is the same
            // service the send side uses to walk its file list. An absolute
            // `name` replaces `dir` here, which is what `join` does and what a
            // macro that named a full path means.
            let target = cstr(&dir.join(name))?;
            // SAFETY: as above.
            if unsafe { ffi::tt_xfer_add_send_file(t.raw, target.as_ptr()) } == 0 {
                return Err(Error::Refused("could not record the destination"));
            }
        }
        t.start()?;
        Ok(t)
    }

    fn create(job: Job, opts: &Options) -> Result<Transfer, Error> {
        let log_dir = match &opts.log_dir {
            Some(p) => Some(cstr(p)?),
            None => None,
        };
        let (baud, seven) = match opts.link {
            Link::Serial { baud, seven_bit } => (baud as i32, seven_bit as i32),
            Link::Network => (0, 0),
        };
        let t = opts.timeouts;
        let autostop = match job {
            Job::Raw { autostop } => autostop.as_secs() as i32,
            _ => 0,
        };
        let c = ffi::TtXferOpts {
            port_type: opts.link.port_type(),
            baud,
            data_bit_7: seven,

            xmodem_timeout_init: t.xmodem[0],
            xmodem_timeout_init_crc: t.xmodem[1],
            xmodem_timeout_short: t.xmodem[2],
            xmodem_timeout_long: t.xmodem[3],
            xmodem_timeout_vlong: t.xmodem[4],
            ymodem_timeout_init: t.ymodem[0],
            ymodem_timeout_init_crc: t.ymodem[1],
            ymodem_timeout_short: t.ymodem[2],
            ymodem_timeout_long: t.ymodem[3],
            ymodem_timeout_vlong: t.ymodem[4],
            zmodem_timeout_normal: t.zmodem[0],
            zmodem_timeout_tcpip: t.zmodem[1],
            zmodem_timeout_init: t.zmodem[2],
            zmodem_timeout_fin: t.zmodem[3],
            zmodem_data_len: opts.zmodem_data_len,
            zmodem_win_size: opts.zmodem_win_size,
            qv_win_size: opts.quickvan_win_size,

            ft_flag: opts.quirks.bits(),
            kermit_opt: (opts.kermit_long_packet as i32) | (opts.kermit_file_attr as i32) << 1,
            log_flag: opts.log.bits(),
            log_dir: log_dir.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),

            mode: job.mode(),
            opt: job.opt(),
            text_flag: match job {
                Job::XModem { text, .. } => text as i32,
                _ => 0,
            },
            autostop_sec: autostop,
            overwrite: opts.overwrite as i32,
        };
        let dir = match job.direction() {
            Direction::Send => 0,
            Direction::Receive => 1,
        };
        // SAFETY: `c` and anything it points at outlive the call.
        let raw = unsafe { ffi::tt_xfer_create(job.protocol(), dir, &c) };
        if raw.is_null() {
            return Err(Error::Refused("Create returned nothing"));
        }
        Ok(Transfer {
            raw,
            inited: false,
            _log_dir: log_dir,
        })
    }

    fn start(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is live and not yet initialised.
        if unsafe { ffi::tt_xfer_init(self.raw) } == 0 {
            return Err(Error::Refused("Init failed"));
        }
        self.inited = true;
        Ok(())
    }

    /// Hand over bytes that arrived on the connection. Returns how many were
    /// taken; the receive buffer is Tera Term's own 64 KB, and a caller with
    /// more than that in hand must keep the rest and offer it again.
    pub fn feed(&mut self, data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        // SAFETY: `data` is a valid slice for the duration of the copy.
        unsafe { ffi::tt_xfer_push_rx(self.raw, data.as_ptr(), data.len()) }
    }

    /// Run the protocol until it is waiting for something.
    ///
    /// One `Parse` per call is not enough. `Parse` handles one packet, so a
    /// caller that fed 8 KB and parsed once would leave the rest of it in the
    /// buffer until the next wakeup and move a file at one packet per turn of
    /// the event loop. So this loops while the protocol is making progress —
    /// consuming input or producing output — and stops when it is not, which
    /// is either "waiting for the peer" or "the send buffer is full and needs
    /// draining". Both mean: return, so the caller can do its half.
    pub fn poll(&mut self) -> Progress {
        loop {
            if self.is_done() {
                break;
            }
            // SAFETY: `self.raw` is live and initialised for all of these.
            let (rx_before, tx_before) = unsafe {
                (
                    ffi::tt_xfer_rx_pending(self.raw),
                    ffi::tt_xfer_tx_pending(self.raw),
                )
            };
            unsafe { ffi::tt_xfer_parse(self.raw) };
            let (rx_after, tx_after) = unsafe {
                (
                    ffi::tt_xfer_rx_pending(self.raw),
                    ffi::tt_xfer_tx_pending(self.raw),
                )
            };
            if rx_after >= rx_before && tx_after <= tx_before {
                break;
            }
        }
        self.progress()
    }

    /// Tell the protocol its timeout elapsed, then let it act on that.
    ///
    /// Separate from [`poll`](Transfer::poll) because the waiting belongs to
    /// the caller: it is the one with an event loop, and it knows how long it
    /// actually slept. Call it when [`wait_hint`](Transfer::wait_hint) has
    /// run out.
    pub fn fire_timeout(&mut self) -> Progress {
        // SAFETY: `self.raw` is live and initialised.
        unsafe { ffi::tt_xfer_timeout(self.raw) };
        self.poll()
    }

    /// How long the caller may sleep before something needs doing.
    ///
    /// `None` means nothing is armed and the only thing that can wake the
    /// transfer is bytes arriving. `Some(Duration::ZERO)` means a timeout is
    /// already due and [`fire_timeout`](Transfer::fire_timeout) should be
    /// called now.
    pub fn wait_hint(&self) -> Option<Duration> {
        // SAFETY: `self.raw` is live.
        let remaining = unsafe { ffi::tt_xfer_timeout_remaining(self.raw) };
        if remaining < 0.0 {
            None
        } else {
            Some(Duration::from_secs_f64(remaining.max(0.0)))
        }
    }

    /// Take bytes the protocol wants written to the connection, appending to
    /// `out`. The caller writes them; a short write is its problem to retry,
    /// not ours, because it is the one that knows about flow control.
    pub fn take_output(&mut self, out: &mut Vec<u8>) -> usize {
        // SAFETY: `self.raw` is live.
        let pending = unsafe { ffi::tt_xfer_tx_pending(self.raw) };
        if pending == 0 {
            return 0;
        }
        let at = out.len();
        out.resize(at + pending, 0);
        // SAFETY: `out[at..]` is exactly `pending` writable bytes.
        let n = unsafe { ffi::tt_xfer_take_tx(self.raw, out[at..].as_mut_ptr(), pending) };
        out.truncate(at + n);
        n
    }

    /// Ask the protocol to abort. It sends whatever its cancel sequence is —
    /// five `CAN`s for XMODEM, a `ZCAN` for ZMODEM — so the caller must keep
    /// pumping afterwards until [`is_done`](Transfer::is_done). ZMODEM in
    /// particular arms a 500 ms timer and finishes on that.
    pub fn cancel(&mut self) {
        // SAFETY: `self.raw` is live and initialised.
        unsafe { ffi::tt_xfer_cancel(self.raw) };
    }

    /// The connection went away.
    ///
    /// This is not the same as cancelling and the protocols distinguish it:
    /// each tests `cv->Ready` before deciding whether it can still finish, and
    /// the ones that cannot call `ProtoEnd` there and then, because they know
    /// they will not be parsed again.
    pub fn disconnected(&mut self) {
        // SAFETY: `self.raw` is live.
        unsafe { ffi::tt_xfer_set_ready(self.raw, 0) };
    }

    pub fn progress(&self) -> Progress {
        // SAFETY: `self.raw` is live; the returned pointer is into it and is
        // read before anything can invalidate it.
        let p = unsafe { &*ffi::tt_xfer_progress(self.raw) };
        Progress {
            bytes: p.bytes,
            packets: p.packets,
            done: p.done,
            total: p.total,
            percent: p.percent,
            elapsed: Duration::from_millis(p.elapsed_ms as u64),
        }
    }

    /// Nothing more will happen without a call to
    /// [`fire_timeout`](Transfer::fire_timeout) or more input — or ever, if
    /// [`is_done`](Transfer::is_done).
    pub fn is_done(&self) -> bool {
        // SAFETY: `self.raw` is live.
        let state = unsafe { ffi::tt_xfer_state(self.raw) };
        state & (ffi::STATE_DONE | ffi::STATE_ENDED) != 0
    }

    /// Whether the transfer completed. Only meaningful once
    /// [`is_done`](Transfer::is_done).
    ///
    /// A cancelled transfer is never a success, whatever the protocol says.
    /// ZMODEM's cancel provokes the peer into a `ZFIN`, and `zmodem.c:1047`
    /// sets `Success` on any `ZFIN` — so the protocol's own answer for a
    /// transfer the user stopped halfway is "fine, thanks", which is not an
    /// answer to give the person who pressed cancel.
    pub fn succeeded(&self) -> bool {
        // SAFETY: `self.raw` is live.
        let state = unsafe { ffi::tt_xfer_state(self.raw) };
        state & ffi::STATE_SUCCESS != 0 && state & ffi::STATE_CANCELLED == 0
    }

    /// Whether [`cancel`](Transfer::cancel) was called on this transfer.
    pub fn was_cancelled(&self) -> bool {
        // SAFETY: `self.raw` is live.
        unsafe { ffi::tt_xfer_state(self.raw) & ffi::STATE_CANCELLED != 0 }
    }

    /// The protocol's name as it reports it — "ZMODEM", "Kermit". Available
    /// only after the first poll, because the protocols set it in `Init`.
    pub fn protocol_name(&self) -> Option<String> {
        // SAFETY: `self.raw` is live.
        unsafe { owned(ffi::tt_xfer_proto_name(self.raw)) }
    }

    /// The file currently in flight, as the protocol named it.
    pub fn file_name(&self) -> Option<String> {
        // SAFETY: `self.raw` is live.
        unsafe { owned(ffi::tt_xfer_file_name(self.raw)) }
    }

    /// What the protocol said when it failed — "Cannot create file",
    /// "Transfer failure". Upstream puts these in a message box; without
    /// somewhere to put them they are the only account of the failure there
    /// is, and it goes to stderr.
    pub fn message(&self) -> Option<String> {
        // SAFETY: `self.raw` is live.
        unsafe { owned(ffi::tt_xfer_message(self.raw)) }
    }
}

impl std::fmt::Debug for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transfer")
            .field("protocol", &self.protocol_name())
            .field("file", &self.file_name())
            .field("done", &self.is_done())
            .field("succeeded", &self.succeeded())
            .field("progress", &self.progress())
            .finish()
    }
}

impl Drop for Transfer {
    fn drop(&mut self) {
        // SAFETY: `self.raw` was returned by `tt_xfer_create` and is dropped
        // exactly once.
        unsafe { ffi::tt_xfer_destroy(self.raw) };
    }
}

/// SAFETY: `p` is NULL or a NUL-terminated string owned by the C side and
/// valid until the next call that replaces it, which cannot happen here.
unsafe fn owned(p: *const std::os::raw::c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

fn cstr(path: &Path) -> Result<CString, Error> {
    let s = path
        .to_str()
        .ok_or_else(|| Error::BadPath(path.display().to_string()))?;
    CString::new(s).map_err(|_| Error::BadPath(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kermit_mode_is_not_the_op_id() {
        // OpKmtRcv is 3 and so is IdKmtSend. Getting this backwards asks
        // Kermit to send when it was told to receive, and the failure is a
        // stall with bytes flowing rather than an error.
        assert_eq!(
            Job::Kermit {
                mode: KermitMode::Receive
            }
            .mode(),
            1
        );
        assert_eq!(
            Job::Kermit {
                mode: KermitMode::Send
            }
            .mode(),
            3
        );
    }

    /// The three receives whose name the caller supplies, and the reason each
    /// is in the list. Getting this wrong is silent: `GetNextFname` answers
    /// NULL and the protocol opens a file called nothing.
    #[test]
    fn only_three_receives_are_told_their_own_name() {
        assert!(Job::Raw {
            autostop: Duration::ZERO
        }
        .needs_name());
        assert!(Job::Kermit {
            mode: KermitMode::Get
        }
        .needs_name());
        // A Kermit receive is told by its sender, and a finish moves no file.
        assert!(!Job::Kermit {
            mode: KermitMode::Receive
        }
        .needs_name());
        assert!(!Job::ZModem {
            dir: Direction::Receive,
            binary: true,
            auto: false
        }
        .needs_name());
    }

    #[test]
    fn a_kermit_get_ends_with_a_file_arriving() {
        assert_eq!(
            Job::Kermit {
                mode: KermitMode::Get
            }
            .direction(),
            Direction::Receive
        );
    }

    #[test]
    fn zmodem_auto_is_a_mode_not_a_flag() {
        // IdZAutoR is 3 and IdZAutoS is 4 — separate modes, not a bit on top
        // of IdZReceive/IdZSend.
        let auto_recv = Job::ZModem {
            dir: Direction::Receive,
            binary: true,
            auto: true,
        };
        assert_eq!(auto_recv.mode(), 3);
    }

    #[test]
    fn flag_words_are_upstreams_bits() {
        let q = Quirks {
            zmodem_escape_ctl: true,
            bplus_escape_ctl: true,
            auto_rename: true,
        };
        // FT_ZESCCTL | FT_BPESCCTL | FT_RENAME, tttypes.h:129.
        assert_eq!(q.bits(), 1 | 4 | 16);

        let l = LogFlags {
            kermit: true,
            zmodem: true,
            ymodem: true,
            ..LogFlags::default()
        };
        // LOG_KMT | LOG_Z | LOG_Y, tttypes.h:120.
        assert_eq!(l.bits(), 2 | 8 | 64);
    }

    #[test]
    fn a_send_with_no_files_is_refused_before_it_starts() {
        let job = Job::ZModem {
            dir: Direction::Send,
            binary: true,
            auto: false,
        };
        let files: [&str; 0] = [];
        assert_eq!(
            Transfer::send(job, files, &Options::default()).unwrap_err(),
            Error::NothingToDo
        );
    }

    #[test]
    fn xmodem_receive_needs_a_name_because_the_wire_has_none() {
        let job = Job::XModem {
            dir: Direction::Receive,
            opt: XmodemOpt::Crc,
            text: false,
        };
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Transfer::receive(job, dir.path(), None, &Options::default()).unwrap_err(),
            Error::NothingToDo
        );
        assert!(Transfer::receive(job, dir.path(), Some("in.bin"), &Options::default()).is_ok());
    }

    #[test]
    fn a_protocol_names_itself_once_it_has_started() {
        let job = Job::ZModem {
            dir: Direction::Receive,
            binary: true,
            auto: false,
        };
        let dir = tempfile::tempdir().unwrap();
        let x = Transfer::receive(job, dir.path(), None, &Options::default()).unwrap();
        assert_eq!(x.protocol_name().as_deref(), Some("ZMODEM"));
    }

    #[test]
    fn the_receive_buffer_is_teraterms_own_size() {
        let job = Job::ZModem {
            dir: Direction::Receive,
            binary: true,
            auto: false,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut x = Transfer::receive(job, dir.path(), None, &Options::default()).unwrap();
        // InBuffSize is 64 KB (tttypes.h:788). Offering more takes what fits
        // and says so, rather than dropping the rest in silence.
        let big = vec![0u8; 100 * 1024];
        assert_eq!(x.feed(&big), 64 * 1024);
        assert_eq!(x.feed(&big), 0);
    }
}
