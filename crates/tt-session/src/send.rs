//! The paced send queue — upstream's `SendMem` (`teraterm/teraterm/sendmem.cpp`).
//!
//! Everything this port has sent so far went straight through: `send_text`,
//! `send_bytes` and `paste` each queue the whole thing and flush it. Upstream
//! has not done that since 2019. `SendMem` sits between a caller and the wire
//! and lets go of the bytes a piece at a time — a character, a line, or a fixed
//! chunk — with a wait in between, and it does not consider itself finished
//! until the transport's own queue has drained. Six callers go through it
//! there: a paste (`clipboar.c:200`), the macro's `send`/`sendln`/`sendfile`
//! and `sendkcode` (`ttdde.c:399`, `:407`, `:811`, `keyboard.c:1580`),
//! File > Send file (`vtwin.cpp:4312`) and a file drop (`vtwin.cpp:2048`).
//!
//! Its absence is why nine settings in this program's own schema said "At this
//! time, Sterna does not use this", and why `crates/tt-macro/src/host.rs`
//! refuses `sendfile`, `setserialdelaychar` and `setserialdelayline`.
//!
//! ## What is here and what is one layer up
//!
//! This module decides **what goes next and when**. It never touches a
//! transport, a parser or a setting: [`Sender::take`] hands a [`Piece`] back
//! and [`crate::Session::service_send`] is what encodes it, echoes it and
//! queues it. That is the same split [`crate::reopen`] makes with
//! `ReopenAction`, and it is what lets the tests here be about the algorithm.
//!
//! The clock is not here either, in the sense that matters: **every deadline is
//! an instant, never an interval measured from the moment it is asked for.**
//! `Session::rearm` re-reads [`Sender::deadline`] after anything at all happens
//! to a session — `Session::mouse` calls it on every mouse-move event — so a
//! state answering "20 ms from now" would get a fresh full wait sixty times a
//! second and never fire. `Reopen::Waiting` had exactly this and stopped
//! noticing a returning adapter while the pointer was moving.
//!
//! ## Two pacing layers, and this is only one of them
//!
//! Upstream has a **second**, entirely separate governor inside the serial
//! write itself (`commlib.c:1068`): `cv->DelayFlag && PortType==IdSerial`, per
//! character and per line, from `ts.DelayPerChar` and `ts.DelayPerLine`, and
//! suppressed for the duration of a protocol transfer. That one paces
//! everything a serial port sends, not just a queued job, and it is not this
//! module. Do not fold them together — a paste through this queue on a serial
//! port is subject to both, upstream and here.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tt_conn::Result;

/// How long to wait before offering the transport more, when it would not take
/// what it was last given.
///
/// The same 20 ms as the frontend's own retry timer (`shell/src/Session.cpp`'s
/// `kRetryIntervalMs`), and for the same reason: short enough that a device
/// releasing CTS is not noticed as lag, long enough that a genuinely wedged
/// line does not spin.
const BACKOFF: Duration = Duration::from_millis(20);

/// How much may be queued for the transport before this stops adding to it.
///
/// `OutBuffSize` (`tttypes.h:789`). Upstream's is a real fixed buffer and this
/// is a high-water mark on a `Vec`, so the number is a policy here rather than
/// a limit — but it is the policy a paced send has always had, and without one
/// a per-line job on a stalled line would read a whole file into memory twice.
pub const HIGH_WATER: usize = 16 * 1024;

/// A job's bytes, and which of the two send paths they belong to.
///
/// The distinction is `SendMemTextW` against `SendMemBinary`, and it is not
/// cosmetic: text goes through [`tt_vt::Vt::encode_text`], so a `CR` in it
/// becomes whatever `CRSend` and LNM say, and bytes do not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Line breaks are already one `CR` each — see
    /// [`crate::normalize_line_break_cr`], which is what upstream's
    /// `NormalizeLineBreakCR` does to a text file before it is queued
    /// (`sendmem.cpp:774`).
    Text(String),
    Bytes(Vec<u8>),
}

impl Body {
    /// How many bytes the whole job is, for a progress display. For
    /// [`Body::Text`] this is UTF-8 bytes rather than upstream's `wchar_t`
    /// count; nothing but the progress bar can tell.
    pub fn len(&self) -> usize {
        match self {
            Body::Text(s) => s.len(),
            Body::Bytes(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How much goes at once, and how long the queue waits afterwards.
///
/// `SendMemDelayType` (`sendmem.h:41`) and `SendMemInitDelay`. Upstream keeps
/// three separate `delay_per_*` fields and reads them in a fixed order — char
/// beats line beats chunk (`sendmem.cpp:396`, `:419`, `:459`) — so only one is
/// ever non-zero and the order is unreachable. One enum says the same thing
/// without the dead branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Everything at once, which is what every send in this program did before
    /// this module existed.
    None,
    /// One Unicode scalar, or one byte of a [`Body::Bytes`].
    PerChar(Duration),
    /// Up to and including the next line break.
    PerLine(Duration),
    /// A fixed number of bytes. `SENDMEM_DELAYTYPE_PER_SENDSIZE`.
    PerChunk { bytes: usize, wait: Duration },
}

impl Pace {
    /// The pace a delay type and a tick of zero really mean.
    ///
    /// Upstream stores the type and the tick separately and a tick of zero is a
    /// real value meaning no wait (`ttset.c:2011`) — so `PerLine` with a tick of
    /// zero is a job that pauses for nothing after every line, which is
    /// [`Pace::None`] with extra work. `clipboar.c:205` makes the same
    /// collapse by hand for a paste.
    pub fn of(kind: PaceKind, tick: Duration, chunk: usize) -> Pace {
        if tick.is_zero() {
            return Pace::None;
        }
        match kind {
            PaceKind::None => Pace::None,
            PaceKind::PerChar => Pace::PerChar(tick),
            PaceKind::PerLine => Pace::PerLine(tick),
            // A chunk of nothing would hand over nothing for ever. Upstream
            // reaches the same answer by a different route: `send_size_max` of
            // zero falls out of its `else if` into the send-everything arm
            // (`sendmem.cpp:459`).
            PaceKind::PerChunk if chunk == 0 => Pace::None,
            PaceKind::PerChunk => Pace::PerChunk {
                bytes: chunk,
                wait: tick,
            },
        }
    }
}

/// The delay type as the settings file spells it, before the tick is known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaceKind {
    #[default]
    None,
    PerChar,
    PerLine,
    PerChunk,
}

/// One thing to send.
#[derive(Clone, Debug)]
pub struct Job {
    pub body: Body,
    pub pace: Pace,
    /// Whether to put a copy through the receive parser as it goes.
    ///
    /// Captured when the job is queued rather than read live, which is
    /// upstream's `SendMemInitEcho`: changing `LocalEcho` halfway through a
    /// file send must not leave half of it echoed.
    pub echo: bool,
    /// What a progress display should call it — a file's path, or nothing for a
    /// paste. Not used for anything else.
    pub name: Option<String>,
}

impl Job {
    /// A job that sends `body` as fast as the transport will take it.
    pub fn new(body: Body) -> Job {
        Job {
            body,
            pace: Pace::None,
            echo: false,
            name: None,
        }
    }

    pub fn paced(mut self, pace: Pace) -> Job {
        self.pace = pace;
        self
    }

    pub fn echoed(mut self, echo: bool) -> Job {
        self.echo = echo;
        self
    }

    pub fn named(mut self, name: impl Into<String>) -> Job {
        self.name = Some(name.into());
        self
    }
}

/// What [`Sender::take`] handed over, for the caller to put on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text { text: String, echo: bool },
    Bytes { data: Vec<u8>, echo: bool },
}

/// Why a job stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendEnd {
    /// Every byte was handed over and the transport's queue drained.
    Finished,
    /// Somebody stopped it.
    Cancelled,
    /// The link went away underneath it.
    LinkLost,
}

/// A finished job, for the frontend that was showing its progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendOutcome {
    pub name: Option<String>,
    pub end: SendEnd,
    /// Bytes handed to the transport. Equal to `total` for
    /// [`SendEnd::Finished`].
    pub sent: usize,
    pub total: usize,
}

/// What a progress display needs, answered by polling rather than by an event.
///
/// An event per piece would be one event per character on a per-character
/// paced job, and the frontend is already being woken for the deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendProgress {
    pub name: Option<String>,
    pub sent: usize,
    pub total: usize,
    pub paused: bool,
    /// Jobs queued behind this one.
    pub queued: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Nothing queued.
    Idle,
    /// There is more to hand over and nothing is stopping it.
    Ready,
    /// Nothing until this instant — a pace's wait, or a transport that would
    /// not take the last piece.
    ///
    /// An instant and not a duration, for the reason in this module's own
    /// documentation.
    Waiting(Instant),
    /// A person stopped it. No deadline at all: nothing is going to happen
    /// until somebody says so, and a timer that fires to discover that is a
    /// timer that should not have been armed.
    Paused,
    /// Every byte is handed over; the job ends when the transport's queue is
    /// empty. `Instant` is when to look again, for the same reason as
    /// [`State::Waiting`].
    Draining(Instant),
}

/// The queue of jobs, the cursor into the running one, and when the next thing
/// is due.
#[derive(Debug, Default)]
pub struct Sender {
    queue: VecDeque<Job>,
    /// How far into `queue.front()`'s body the cursor is, in bytes. Always on a
    /// character boundary for a [`Body::Text`].
    at: usize,
    state: SendState,
}

/// `State` is not `Default`, and a `Sender::default()` is the state a session
/// spends nearly all of its life in.
#[derive(Debug)]
struct SendState(State);

impl Default for SendState {
    fn default() -> SendState {
        SendState(State::Idle)
    }
}

impl Sender {
    /// Queue one. It starts the moment the caller next services the sender,
    /// which is what `SendMemStart` does by setting `TalkStatus`.
    ///
    /// A second job **queues behind** the first rather than replacing it —
    /// upstream keeps a FIFO of them (`smptrPush`, `sendmem.cpp:107`) so a
    /// macro's `send` during a file send arrives after the file rather than
    /// interleaved with it.
    pub fn push(&mut self, job: Job) {
        let first = self.queue.is_empty();
        self.queue.push_back(job);
        if first {
            self.at = 0;
            self.state.0 = State::Ready;
        }
    }

    /// Whether anything is queued at all. The hot-path question: a session with
    /// nothing sending must pay nothing for this module.
    pub fn is_running(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.state.0, State::Paused)
    }

    /// Stop everything and say what to report. `None` when nothing was running.
    ///
    /// Every queued job goes, not only the running one: cancelling is a person
    /// saying stop, and leaving three more behind to start by themselves is not
    /// what they asked for.
    pub fn cancel(&mut self, end: SendEnd) -> Option<SendOutcome> {
        let job = self.queue.pop_front()?;
        let outcome = SendOutcome {
            name: job.name,
            end,
            sent: self.at,
            total: job.body.len(),
        };
        self.queue.clear();
        self.at = 0;
        self.state.0 = State::Idle;
        Some(outcome)
    }

    /// Hold, or let go. Upstream's `OnPause` (`sendmem.cpp:224`), which
    /// re-triggers the timer on release rather than working out what was left
    /// of the wait — so a pause of an hour costs the paused line one more
    /// interval and no more.
    pub fn set_paused(&mut self, paused: bool) {
        if !self.is_running() {
            return;
        }
        match (paused, self.state.0) {
            (true, State::Paused) => {}
            (true, _) => self.state.0 = State::Paused,
            (false, State::Paused) => self.state.0 = State::Ready,
            (false, _) => {}
        }
    }

    /// When there is next something to do. `None` when there is not, and
    /// `Duration::ZERO` for now.
    ///
    /// **Idempotent in every state**, which is the whole contract: a frontend
    /// re-reads this after anything at all and restarts one single-shot timer
    /// with the answer.
    pub fn deadline(&self, now: Instant) -> Option<Duration> {
        if self.queue.is_empty() {
            return None;
        }
        match self.state.0 {
            State::Idle | State::Paused => None,
            State::Ready => Some(Duration::ZERO),
            State::Waiting(at) | State::Draining(at) => Some(at.saturating_duration_since(now)),
        }
    }

    /// What the running job looks like from outside.
    pub fn progress(&self) -> Option<SendProgress> {
        let job = self.queue.front()?;
        Some(SendProgress {
            name: job.name.clone(),
            sent: self.at,
            total: job.body.len(),
            paused: self.is_paused(),
            queued: self.queue.len() - 1,
        })
    }

    /// Hand over the next piece, or nothing.
    ///
    /// `free` is how much the transport's queue will take — zero is a line that
    /// has not moved, and the sender waits rather than growing the queue, which
    /// is upstream's `GetBufferFreeSpece` returning zero (`sendmem.cpp:388`).
    ///
    /// Returns `None` in three different situations that all look the same from
    /// here: nothing queued, a wait that has not expired, and a job whose bytes
    /// are all handed over. [`Sender::drained`] tells the last one apart,
    /// because only the caller knows whether the transport has caught up.
    pub fn take(&mut self, now: Instant, free: usize) -> Option<Piece> {
        match self.state.0 {
            State::Idle | State::Paused | State::Draining(_) => return None,
            State::Waiting(at) if now < at => return None,
            State::Waiting(_) | State::Ready => {}
        }
        let job = self.queue.front()?;
        let left = job.body.len() - self.at;
        if left == 0 {
            // Handed over on an earlier call; it ends when the wire catches up.
            self.state.0 = State::Draining(now);
            return None;
        }
        if free == 0 {
            self.state.0 = State::Waiting(now + BACKOFF);
            return None;
        }

        let take = span(&job.body, self.at, job.pace, free);
        // A pace that could not fit even its smallest unit into the room going:
        // wait rather than send a fragment. Only `PerLine` and `PerChunk` can
        // ask for more than `free`, and a line longer than the whole buffer is
        // the one case upstream gets wrong — `sendmem.cpp:454` truncates the
        // length, clears the line detector and then returns without sending
        // any of it, so the truncation is dead and the detector is reset for
        // no reason. Waiting is what that code meant to do.
        let Some(take) = take else {
            self.state.0 = State::Waiting(now + BACKOFF);
            return None;
        };

        let piece = match &job.body {
            Body::Text(s) => Piece::Text {
                text: s[self.at..self.at + take].to_string(),
                echo: job.echo,
            },
            Body::Bytes(b) => Piece::Bytes {
                data: b[self.at..self.at + take].to_vec(),
                echo: job.echo,
            },
        };
        self.at += take;

        // The wait comes *after* a piece and not before the next one, and only
        // when something is left — upstream's `send_left != 0 && need_delay`
        // (`sendmem.cpp:515`). A job whose last line is exactly one line long
        // therefore does not sit through an interval it cannot use.
        let done = self.at == job.body.len();
        self.state.0 = match (done, job.pace) {
            (true, _) => State::Draining(now),
            (false, Pace::None) => State::Ready,
            (false, Pace::PerChar(w) | Pace::PerLine(w) | Pace::PerChunk { wait: w, .. }) => {
                State::Waiting(now + w)
            }
        };
        Some(piece)
    }

    /// The transport's queue is empty, so a job that had handed everything over
    /// is finished. `None` at any other time.
    ///
    /// Upstream will not end a job until `GetOutBuffInfo` says zero
    /// (`sendmem.cpp:373`), and the reason is visible in the progress dialog it
    /// is refreshing one line earlier: "sent" must mean *gone*, not *queued*.
    /// It matters more here, because the thing waiting on the answer is a
    /// macro's `sendfile` returning.
    pub fn drained(&mut self) -> Option<SendOutcome> {
        let State::Draining(_) = self.state.0 else {
            return None;
        };
        let job = self.queue.pop_front()?;
        let outcome = SendOutcome {
            name: job.name,
            end: SendEnd::Finished,
            sent: self.at,
            total: job.body.len(),
        };
        self.at = 0;
        self.state.0 = if self.queue.is_empty() {
            State::Idle
        } else {
            State::Ready
        };
        Some(outcome)
    }

    /// Come back to a draining job later — the transport still has bytes.
    pub fn still_draining(&mut self, now: Instant) {
        if matches!(self.state.0, State::Draining(_)) {
            self.state.0 = State::Draining(now + BACKOFF);
        }
    }
}

/// Why a send could not be started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendError {
    NotConnected,
    /// A file transfer owns the byte stream. Everything a person could type is
    /// refused while one is up, and a file is the largest stray byte there is.
    TransferRunning,
    /// The file could not be read. `String` is the operating system's reason,
    /// which is the only useful thing to say about it.
    Unreadable(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::NotConnected => write!(f, "not connected"),
            SendError::TransferRunning => write!(f, "a file transfer is running"),
            SendError::Unreadable(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SendError {}

/// What File > Send file line by line was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSend {
    /// `TransBin`. Text goes through [`tt_vt::Vt::encode_text`] with its line
    /// breaks already normalised; bytes go exactly as they are on disk.
    pub binary: bool,
    pub pace: Pace,
    /// `ts.LocalEcho` at the moment the job is queued.
    pub echo: bool,
}

impl Default for FileSend {
    fn default() -> FileSend {
        FileSend {
            binary: false,
            pace: Pace::None,
            echo: false,
        }
    }
}

/// What File > Send file would do right now, out of the settings file.
///
/// Upstream's own dialog is seeded the same way and writes back to the same
/// four keys (`vtwin.cpp:4290`), which is what `SendfileSkipOptionDialog` then
/// lets somebody skip. Those keys have been read, written and acted on by
/// nothing since this program had a settings file; this is where they start
/// meaning something.
pub fn file_send_defaults(s: &crate::Settings) -> FileSend {
    use tt_config::TransferRawSendDelayType as D;
    let kind = match s.transfer_raw_send_delay_type {
        D::NoDelay => PaceKind::None,
        D::PerChar => PaceKind::PerChar,
        D::PerLine => PaceKind::PerLine,
        D::PerSendSize => PaceKind::PerChunk,
    };
    FileSend {
        binary: s.transfer_binary,
        pace: Pace::of(
            kind,
            Duration::from_millis(s.transfer_raw_send_delay_tick.max(0) as u64),
            s.transfer_raw_send_size.max(0) as usize,
        ),
        echo: s.terminal_local_echo,
    }
}

impl crate::Session {
    /// Send a file through the terminal's own write path, a piece at a time.
    ///
    /// Upstream's `SendMemSendFile` (`sendmem.cpp:805`), which is what
    /// File > Send file does there and what the macro command `sendfile` runs.
    /// It is **not** a file transfer: nothing is framed, nothing is
    /// acknowledged, and the far end sees exactly what somebody typing it
    /// would have sent.
    ///
    /// The whole file is read into memory, which is upstream's `LoadFileWW` /
    /// `LoadFileBinary`. `SendfileSequential` is the setting that asks for the
    /// other behaviour and this port does not have a second sender to point it
    /// at; the schema says so.
    ///
    /// A text file is decoded as UTF-8, lossily, with a byte-order mark
    /// removed. Upstream decodes by BOM and then by the machine's ANSI code
    /// page, which is a Windows answer to a Windows question — `tt-ttl` keeps
    /// the ACP path because a `.ttl` is a Windows artefact, and a configuration
    /// somebody is about to paste into a console is not.
    pub fn send_file(
        &mut self,
        path: &std::path::Path,
        opts: &FileSend,
    ) -> std::result::Result<(), SendError> {
        self.check_can_send()?;
        let raw = std::fs::read(path).map_err(|e| SendError::Unreadable(e.to_string()))?;
        let body = if opts.binary {
            Body::Bytes(raw)
        } else {
            let text = String::from_utf8_lossy(&raw);
            Body::Text(crate::normalize_line_break_cr(
                text.strip_prefix('\u{feff}').unwrap_or(&text),
            ))
        };
        let job = Job {
            body,
            pace: opts.pace,
            echo: opts.echo,
            name: Some(path.display().to_string()),
        };
        self.push_send(job);
        Ok(())
    }

    /// Queue a job built by hand — a paste, or a macro's `send`.
    ///
    /// Refused for the same two reasons a file send is, and silently: the
    /// callers here are the paths a keystroke reaches, and upstream drops those
    /// rather than reporting them (`keyboard.c:1480`'s `TalkStatus` test).
    pub fn queue_send(&mut self, job: Job) -> std::result::Result<(), SendError> {
        self.check_can_send()?;
        self.push_send(job);
        Ok(())
    }

    fn check_can_send(&self) -> std::result::Result<(), SendError> {
        if self.xfer.is_some() {
            return Err(SendError::TransferRunning);
        }
        if self.conn.is_none() {
            return Err(SendError::NotConnected);
        }
        Ok(())
    }

    fn push_send(&mut self, job: Job) {
        self.sender.push(job);
    }

    /// Whether a queued send owns the wire.
    ///
    /// While one does, **typing is dropped** — upstream sets
    /// `TalkStatus = IdTalkSendMem` for the duration and `keyboard.c:1480`
    /// tests it before every key. A line typed into the middle of a
    /// configuration being pasted is a line the far end runs in the wrong
    /// place. A macro's `send` is not dropped; it queues behind, which is the
    /// one exception upstream makes too (`ttdde.c:460`).
    pub fn sending(&self) -> bool {
        self.sender.is_running()
    }

    /// What the running send is doing, for a progress display.
    pub fn send_progress(&self) -> Option<SendProgress> {
        self.sender.progress()
    }

    /// Hold the running send, or let it go again.
    pub fn pause_send(&mut self, paused: bool) {
        self.sender.set_paused(paused);
    }

    /// Stop it. Raises [`crate::Event::SendDone`] unless nothing was running.
    pub fn cancel_send(&mut self) {
        if let Some(outcome) = self.sender.cancel(SendEnd::Cancelled) {
            self.events.push(crate::Event::SendDone(Box::new(outcome)));
        }
    }

    /// End a send because the link did — called from `Session::line_went_away`,
    /// where every other thing that a dead connection ends is ended.
    pub(crate) fn send_link_lost(&mut self) {
        if let Some(outcome) = self.sender.cancel(SendEnd::LinkLost) {
            self.events.push(crate::Event::SendDone(Box::new(outcome)));
        }
    }

    /// How long the caller may sleep before the send queue needs attention.
    ///
    /// **The core owns the instant and the frontend owns the timer**, the same
    /// arrangement as [`crate::Session::transfer_deadline`] and
    /// [`crate::Session::reopen_deadline`], and for a sharper reason than
    /// either: a per-character pace of 1 ms cannot be expressed by the session
    /// tick at all, which is `Qt::VeryCoarseTimer` and rounds to whole seconds.
    ///
    /// Re-read it after every [`Session::service_send`](crate::Session::service_send)
    /// and after anything else that touches the session. It is idempotent —
    /// asking does not postpone the answer.
    pub fn send_deadline(&self) -> Option<Duration> {
        self.sender.deadline(Instant::now())
    }

    /// Hand the transport the next piece, if one is due.
    ///
    /// Call it when [`Session::send_deadline`](crate::Session::send_deadline)
    /// has elapsed. Doing it at any other time is harmless — the sender
    /// answers with nothing — which is what makes it safe to call from a pump
    /// that is not sure.
    pub fn service_send(&mut self) -> Result<()> {
        if !self.sender.is_running() {
            return Ok(());
        }
        let now = Instant::now();
        let free = HIGH_WATER.saturating_sub(self.pending.len());
        match self.sender.take(now, free) {
            Some(Piece::Text { text, echo }) => {
                let bytes = self.vt.encode_text(&text);
                if echo {
                    self.feed(&bytes);
                }
                self.queue(&bytes);
            }
            Some(Piece::Bytes { data, echo }) => {
                if echo {
                    self.feed(&data);
                }
                self.queue(&data);
            }
            // Either nothing is due, or the body is spent and the job is
            // waiting for the wire. Only this layer knows which, because only
            // this layer can see the transport's queue.
            None => {}
        }
        let out = self.flush_pending();
        // ...and the flush is what may have emptied it. Asking before would
        // add a whole `BACKOFF` to every job that fitted in one piece.
        if self.pending.is_empty() {
            if let Some(outcome) = self.sender.drained() {
                self.events.push(crate::Event::SendDone(Box::new(outcome)));
            }
        } else {
            self.sender.still_draining(now);
        }
        out
    }
}

/// How many bytes of `body` from `at` the pace wants to hand over, or `None`
/// when the unit it wants does not fit in `free`.
fn span(body: &Body, at: usize, pace: Pace, free: usize) -> Option<usize> {
    let left = body.len() - at;
    match pace {
        // Everything, in one piece. `free` has no say: an unpaced send is what
        // every caller in this program did before this module existed, and
        // `Session::flush_pending` already keeps whatever a short write left
        // behind. Cutting it into buffer-sized pieces would put a whole file
        // through the event queue to reach the same bytes.
        Pace::None => Some(left),
        Pace::PerChar(_) => match body {
            Body::Text(s) => {
                let n = s[at..].chars().next()?.len_utf8();
                (n <= free).then_some(n)
            }
            Body::Bytes(_) => (free >= 1).then_some(1),
        },
        Pace::PerLine(_) => {
            let n = line_span(body, at);
            (n <= free).then_some(n)
        }
        Pace::PerChunk { bytes, .. } => {
            // A chunk is a length and not a character boundary, so a text body
            // has to round *down* to one or the slice panics. Rounding down can
            // reach zero only for a chunk smaller than one character, which
            // `Pace::of` cannot produce and a hand-edited file can.
            let n = match body {
                Body::Text(s) => floor_char_boundary(s, at, bytes.min(left)),
                Body::Bytes(_) => bytes.min(left),
            };
            (n > 0 && n <= free).then_some(n)
        }
    }
}

/// Bytes from `at` up to and including the next line break.
///
/// A [`Body::Text`] has been through `NormalizeLineBreakCR`, so its only line
/// break is a bare `CR` and this is exact. A [`Body::Bytes`] has not been
/// through anything — and upstream reads one as `wchar_t` here regardless
/// (`sendmem.cpp:422` casts unconditionally), which pairs bytes into
/// characters that are not there. This takes an `LF` or a `CR`, whichever comes
/// first, which is what per-line pacing on a binary file has to mean if it is
/// to mean anything.
fn line_span(body: &Body, at: usize) -> usize {
    let rest: &[u8] = match body {
        Body::Text(s) => &s.as_bytes()[at..],
        Body::Bytes(b) => &b[at..],
    };
    rest.iter()
        .position(|&b| b == b'\r' || b == b'\n')
        .map_or(rest.len(), |i| i + 1)
}

/// The largest `n <= want` with `at + n` on a character boundary.
fn floor_char_boundary(s: &str, at: usize, want: usize) -> usize {
    let mut n = want;
    while n > 0 && !s.is_char_boundary(at + n) {
        n -= 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Job {
        Job::new(Body::Text(s.to_string()))
    }

    fn drain(sender: &mut Sender, now: Instant) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = now;
        // A bounded loop: a test that stops making progress should fail by
        // asserting the wrong answer, not by hanging a suite.
        for _ in 0..1000 {
            let Some(d) = sender.deadline(at) else { break };
            at += d;
            match sender.take(at, HIGH_WATER) {
                Some(Piece::Text { text, .. }) => out.push(text),
                Some(Piece::Bytes { data, .. }) => {
                    out.push(String::from_utf8_lossy(&data).into_owned())
                }
                None => {
                    if sender.drained().is_some() {
                        break;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn nothing_queued_has_no_deadline() {
        let s = Sender::default();
        assert_eq!(s.deadline(Instant::now()), None);
        assert!(!s.is_running());
        assert_eq!(s.progress(), None);
    }

    #[test]
    fn an_unpaced_job_goes_in_one_piece() {
        let mut s = Sender::default();
        s.push(text("one\rtwo\rthree\r"));
        assert_eq!(drain(&mut s, Instant::now()), vec!["one\rtwo\rthree\r"]);
        assert!(!s.is_running());
    }

    /// ...even when it is larger than the high-water mark, because an unpaced
    /// send is what every caller did before this module and `flush_pending`
    /// already keeps what a short write left behind.
    #[test]
    fn an_unpaced_job_is_not_cut_by_the_high_water_mark() {
        let mut s = Sender::default();
        let big = "x".repeat(HIGH_WATER * 3);
        s.push(text(&big));
        assert_eq!(drain(&mut s, Instant::now()), vec![big]);
    }

    #[test]
    fn per_line_hands_over_one_line_at_a_time() {
        let mut s = Sender::default();
        s.push(text("one\rtwo\rthree\r").paced(Pace::PerLine(Duration::from_millis(50))));
        assert_eq!(
            drain(&mut s, Instant::now()),
            vec!["one\r", "two\r", "three\r"]
        );
    }

    /// A last line with nothing after it is still a line.
    #[test]
    fn per_line_sends_a_final_line_with_no_break() {
        let mut s = Sender::default();
        s.push(text("one\rtwo").paced(Pace::PerLine(Duration::from_millis(50))));
        assert_eq!(drain(&mut s, Instant::now()), vec!["one\r", "two"]);
    }

    #[test]
    fn per_line_waits_exactly_the_interval_between_lines() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("a\rb\r").paced(Pace::PerLine(Duration::from_millis(50))));
        assert_eq!(s.deadline(now), Some(Duration::ZERO));
        assert!(s.take(now, HIGH_WATER).is_some());
        assert_eq!(s.deadline(now), Some(Duration::from_millis(50)));
        // Nothing comes out early...
        assert_eq!(s.take(now + Duration::from_millis(49), HIGH_WATER), None);
        // ...and the wait is not renewed by having been asked.
        assert_eq!(
            s.deadline(now + Duration::from_millis(49)),
            Some(Duration::from_millis(1))
        );
        assert!(s
            .take(now + Duration::from_millis(50), HIGH_WATER)
            .is_some());
    }

    /// The trap this whole module is shaped around: `Session::rearm` asks on
    /// every mouse-move event, so a deadline that answered "the full interval,
    /// from now" would never come due.
    #[test]
    fn asking_for_the_deadline_does_not_postpone_it() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("a\rb\r").paced(Pace::PerLine(Duration::from_millis(50))));
        s.take(now, HIGH_WATER);
        for i in 0..40u64 {
            let t = now + Duration::from_millis(i);
            assert_eq!(s.deadline(t), Some(Duration::from_millis(50 - i)));
        }
    }

    #[test]
    fn per_char_hands_over_one_scalar_at_a_time() {
        let mut s = Sender::default();
        s.push(text("aé\u{1f600}").paced(Pace::PerChar(Duration::from_millis(5))));
        assert_eq!(drain(&mut s, Instant::now()), vec!["a", "é", "\u{1f600}"]);
    }

    #[test]
    fn per_char_on_bytes_hands_over_one_byte_at_a_time() {
        let mut s = Sender::default();
        s.push(Job::new(Body::Bytes(vec![1, 2, 3])).paced(Pace::PerChar(Duration::from_millis(5))));
        let mut out = Vec::new();
        let mut at = Instant::now();
        while let Some(d) = s.deadline(at) {
            at += d;
            match s.take(at, HIGH_WATER) {
                Some(Piece::Bytes { data, .. }) => out.push(data),
                _ => {
                    s.drained();
                }
            }
        }
        assert_eq!(out, vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn per_chunk_hands_over_a_fixed_size() {
        let mut s = Sender::default();
        s.push(text("abcdefg").paced(Pace::PerChunk {
            bytes: 3,
            wait: Duration::from_millis(5),
        }));
        assert_eq!(drain(&mut s, Instant::now()), vec!["abc", "def", "g"]);
    }

    /// A chunk boundary in the middle of a character rounds down rather than
    /// panicking on the slice.
    #[test]
    fn a_chunk_never_splits_a_character() {
        let mut s = Sender::default();
        s.push(text("aéb").paced(Pace::PerChunk {
            bytes: 2,
            wait: Duration::from_millis(5),
        }));
        assert_eq!(drain(&mut s, Instant::now()), vec!["a", "é", "b"]);
    }

    /// Zero is a real value for the tick and means no wait at all
    /// (`ttset.c:2011`), so the pace collapses rather than becoming a job that
    /// pauses for nothing after every line.
    #[test]
    fn a_tick_of_zero_is_not_a_pace() {
        assert_eq!(Pace::of(PaceKind::PerLine, Duration::ZERO, 0), Pace::None);
        assert_eq!(Pace::of(PaceKind::PerChar, Duration::ZERO, 0), Pace::None);
        assert_eq!(
            Pace::of(PaceKind::PerChunk, Duration::from_millis(5), 0),
            Pace::None
        );
        assert_eq!(
            Pace::of(PaceKind::PerChunk, Duration::from_millis(5), 8),
            Pace::PerChunk {
                bytes: 8,
                wait: Duration::from_millis(5)
            }
        );
    }

    #[test]
    fn a_full_transport_is_waited_for_rather_than_queued_behind() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("one\rtwo\r").paced(Pace::PerLine(Duration::from_millis(5))));
        assert_eq!(s.take(now, 0), None);
        assert_eq!(s.deadline(now), Some(BACKOFF));
        // And it is still the first line that comes out when there is room.
        let t = now + BACKOFF;
        assert_eq!(
            s.take(t, HIGH_WATER),
            Some(Piece::Text {
                text: "one\r".into(),
                echo: false
            })
        );
    }

    /// A line longer than the room going waits for room rather than being cut
    /// in half — see the comment at the call site for what upstream does here.
    #[test]
    fn a_line_too_long_for_the_room_waits() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("aaaaaaaaaa\rb\r").paced(Pace::PerLine(Duration::from_millis(5))));
        assert_eq!(s.take(now, 4), None);
        assert_eq!(s.deadline(now), Some(BACKOFF));
        assert!(s.take(now + BACKOFF, 11).is_some());
    }

    #[test]
    fn a_job_is_not_finished_until_the_wire_has_caught_up() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("hello"));
        assert!(s.take(now, HIGH_WATER).is_some());
        assert!(s.is_running());
        // The caller has bytes left in its own queue, so it says so instead of
        // taking the outcome.
        s.still_draining(now);
        assert_eq!(s.deadline(now), Some(BACKOFF));
        assert!(s.is_running());
        let out = s.drained().unwrap();
        assert_eq!(out.end, SendEnd::Finished);
        assert_eq!(out.sent, 5);
        assert_eq!(out.total, 5);
        assert!(!s.is_running());
    }

    #[test]
    fn a_second_job_queues_behind_the_first() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("one\rtwo\r").paced(Pace::PerLine(Duration::from_millis(5))));
        s.push(text("three\r"));
        assert_eq!(s.progress().unwrap().queued, 1);
        let out = drain(&mut s, now);
        assert_eq!(out, vec!["one\r", "two\r"]);
        // ...and the next one starts on the following service.
        assert_eq!(s.progress().unwrap().queued, 0);
        assert_eq!(drain(&mut s, now), vec!["three\r"]);
        assert!(!s.is_running());
    }

    #[test]
    fn cancelling_takes_everything_queued() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("one\rtwo\rthree\r").paced(Pace::PerLine(Duration::from_millis(5))));
        s.push(text("and another"));
        s.take(now, HIGH_WATER);
        let out = s.cancel(SendEnd::Cancelled).unwrap();
        assert_eq!(out.end, SendEnd::Cancelled);
        assert_eq!(out.sent, 4);
        assert_eq!(out.total, 14);
        assert!(!s.is_running());
        assert_eq!(s.deadline(now), None);
        assert_eq!(s.cancel(SendEnd::Cancelled), None);
    }

    /// A pause has no deadline, so nothing wakes up to discover it is paused.
    #[test]
    fn a_paused_job_arms_nothing() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text("one\rtwo\r").paced(Pace::PerLine(Duration::from_millis(5))));
        s.set_paused(true);
        assert!(s.is_paused());
        assert_eq!(s.deadline(now), None);
        assert_eq!(s.take(now, HIGH_WATER), None);
        s.set_paused(false);
        assert!(!s.is_paused());
        assert_eq!(s.deadline(now), Some(Duration::ZERO));
        assert!(s.take(now, HIGH_WATER).is_some());
    }

    #[test]
    fn pausing_nothing_does_nothing() {
        let mut s = Sender::default();
        s.set_paused(true);
        assert!(!s.is_paused());
        assert_eq!(s.deadline(Instant::now()), None);
    }

    #[test]
    fn the_echo_flag_rides_every_piece_of_its_job() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(
            text("a\rb\r")
                .paced(Pace::PerLine(Duration::from_millis(5)))
                .echoed(true),
        );
        let Some(Piece::Text { echo, .. }) = s.take(now, HIGH_WATER) else {
            panic!("no piece");
        };
        assert!(echo);
    }

    #[test]
    fn progress_counts_bytes_handed_over() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(
            text("one\rtwo\r")
                .paced(Pace::PerLine(Duration::from_millis(5)))
                .named("/tmp/config.txt"),
        );
        let p = s.progress().unwrap();
        assert_eq!((p.sent, p.total), (0, 8));
        assert_eq!(p.name.as_deref(), Some("/tmp/config.txt"));
        s.take(now, HIGH_WATER);
        assert_eq!(s.progress().unwrap().sent, 4);
    }

    #[test]
    fn an_empty_job_finishes_without_sending_anything() {
        let now = Instant::now();
        let mut s = Sender::default();
        s.push(text(""));
        assert_eq!(s.take(now, HIGH_WATER), None);
        let out = s.drained().unwrap();
        assert_eq!(out.end, SendEnd::Finished);
        assert_eq!(out.total, 0);
        assert!(!s.is_running());
    }
}
