//! `AutoComPortReconnect`'s state machine — upstream's `SerialReconnect`
//! (`vtwin.cpp:221`).
//!
//! Power-cycle a board and a USB adapter's node leaves `/dev` and comes back a
//! few seconds later. Upstream reopens the port by itself when that happens,
//! and it has carried the four timings for it since Tera Term 5; this port has
//! carried the settings and not the behaviour.
//!
//! It is here rather than in the frontend for the same reason [`crate::bell`]
//! is here rather than in `tt-vt`: it needs a clock, and it needs something the
//! layer above cannot see. The parameters to reopen with are the **port's** —
//! `setbaud` moves them, and `Session::reset_serial` says why that is right —
//! so they exist only until the transport is dropped. A frontend remembering
//! what it passed to `connect` would bring a session back at the speed in the
//! settings file rather than the speed it was using.
//!
//! **Nothing here opens anything to find out whether the port is back.** That
//! is [`tt_conn::serial::present`], and the reason is in `serial::inuse`'s
//! module docs: opening a port raises DTR for the life of the descriptor, so a
//! probe on a timer resets an Arduino-style board once a tick. Upstream's own
//! guard is the same shape — `CheckComPort` enumerates, it does not probe.
//!
//! ## Where this differs from upstream, deliberately
//!
//! * **Arrival is polled, not notified.** Upstream registers for
//!   `WM_DEVICECHANGE` (`vtwin.cpp:250`). Linux's equivalent is a udev monitor;
//!   asking [`tt_conn::serial::present`] once a second costs a `stat` and needs
//!   no dependency. Forced by the platform, so it is a port rather than a
//!   deviation.
//! * **`delay_unknown` is chosen by the path, not by the message.** See
//!   [`ReopenLimits::delay_unknown`].
//! * **A node that leaves again while we are settling goes back to waiting.**
//!   Upstream gives up entirely (`vtwin.cpp:411`); a supply that bounces on the
//!   way up is exactly the case this feature exists for.

use std::time::{Duration, Instant};

/// The four timings, out of the settings.
#[derive(Clone, Copy, Debug)]
pub struct ReopenLimits {
    /// `AutoComPortReconnectDelayNormal`, `ttset.c:1088`. How long to leave the
    /// node alone after it appears, before the first open.
    pub delay: Duration,
    /// `AutoComPortReconnectDelayIllegal`, `ttset.c:1090` — the longer wait for
    /// when the reopen is a guess.
    ///
    /// Upstream's doubt is about the *notification*: some drivers send only
    /// `DBT_DEVTYP_DEVICEINTERFACE` and never the `DBT_DEVTYP_PORT` that would
    /// say which port arrived (`vtwin.cpp:335`), so the reopen is a guess and
    /// gets the longer wait. There is no such message here, and the same doubt
    /// turns out to have a **mechanical** Linux spelling rather than only an
    /// analogous one:
    ///
    /// `devtmpfs` creates `/dev/ttyUSB0` the instant the driver binds. udev's
    /// rules run afterwards — they apply the group and mode, and only then make
    /// the `/dev/serial/by-path/…` symlink. So a **bare node appearing means
    /// udev has not finished**, and opening it there routinely fails with
    /// `EACCES` (the `dialout` group is not on it yet) or `EBUSY` (something is
    /// still probing it). A **`by-path` name appearing means udev has
    /// finished.** The two waits are therefore exactly "I saw a device" and "I
    /// saw the device, set up" — which is what the two mean on Windows.
    ///
    /// It carries the attach-order doubt as well: `/dev/ttyUSB<n>` is assigned
    /// in the order adapters attach, so a bare node that came back need not be
    /// the one that left.
    ///
    /// Windows always takes the short [`ReopenLimits::delay`]: `QueryDosDeviceW`
    /// names the exact port, so nothing there is a guess. That is the one place
    /// this port is *less* uncertain than upstream, and spending two seconds
    /// pretending otherwise would buy nothing.
    pub delay_unknown: Duration,
    /// `AutoComPortReconnectRetryInterval`, `ttset.c:1092`. Between one failed
    /// open and the next.
    pub retry_interval: Duration,
    /// `AutoComPortReconnectRetryCount`, `ttset.c:1094`, and it counts retries
    /// **after** the first attempt — so the shipped 3 is four tries.
    pub retries: u32,
}

impl Default for ReopenLimits {
    /// `vtwin.cpp:224-227`'s four `#define`s, which are also the schema's
    /// defaults.
    fn default() -> Self {
        ReopenLimits {
            delay: Duration::from_millis(500),
            delay_unknown: Duration::from_millis(2000),
            retry_interval: Duration::from_millis(1000),
            retries: 3,
        }
    }
}

/// What the caller should do about this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReopenAction {
    /// Keep waiting.
    Nothing,
    /// Try to open the port now, and report back with [`Reopen::attempted`].
    Attempt,
    /// The retries are spent. The machine is idle again; tell the user.
    GiveUp,
}

/// How often to look for a node that is not there yet, and how long before
/// that slows down.
///
/// Not settings, and deliberately: this is a *resolution*, not a preference —
/// nobody wants to notice their adapter more slowly — and a key the schema
/// invents is a permanent promise in a file Tera Term will never read. The
/// wait is indefinite, so it does have to stop being a twice-a-second wakeup
/// eventually; after half a minute it is slower than the session tick that
/// `AGENTS.md` already accepts as running for the life of the window.
const POLL_FAST: Duration = Duration::from_millis(500);
const POLL_SLOW: Duration = Duration::from_secs(2);
const SLOW_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    /// The node is not there. Indefinite — upstream waits for a device-arrival
    /// message for ever, and a board switched off overnight should still be
    /// there in the morning.
    ///
    /// `since` is when this wait began, which is what the poll backs off from.
    /// `next` is when to look again, and it is an **instant rather than an
    /// interval** for the same reason the other two states hold one: a frontend
    /// re-reads [`Reopen::deadline`] whenever anything else happens to the
    /// session and restarts its timer with the answer. An interval measured
    /// from *now* is a fresh full wait each time, so anything that asks often
    /// enough postpones the look for ever — and `Session::mouse` asks on every
    /// mouse-move event, which is faster than [`POLL_SLOW`] by three orders of
    /// magnitude. The symptom is an adapter that comes back and is not noticed
    /// until the pointer stops moving.
    Waiting {
        since: Instant,
        next: Instant,
    },
    /// The node is back; leave it alone until this instant.
    Settling(Instant),
    /// An open failed; try again at this instant.
    Retrying(Instant),
}

/// Where a port was, and what is left of the budget for getting it back.
#[derive(Clone, Debug)]
pub struct Reopen {
    target: Option<tt_conn::ReopenTarget>,
    state: State,
    /// Counts down. `retries` at arming, so it is attempts-after-the-first.
    left: u32,
}

impl Default for Reopen {
    fn default() -> Self {
        Reopen {
            target: None,
            state: State::Idle,
            left: 0,
        }
    }
}

impl Reopen {
    /// Start watching for a port that went away — upstream's
    /// `SetAutoConnectPort` (`vtwin.cpp:427`).
    ///
    /// Always starts in `Waiting`, even when the node is still there: one rule
    /// and no branch, and the first [`Reopen::poll`] promotes it in the same
    /// tick if the node has not actually gone. A read can report the device
    /// disconnected for reasons that leave the node in place, and this way that
    /// case takes the ordinary settle-then-open path rather than a second one.
    pub fn arm(&mut self, now: Instant, target: tt_conn::ReopenTarget, limits: &ReopenLimits) {
        self.left = limits.retries;
        self.state = State::Waiting {
            since: now,
            next: now + POLL_FAST,
        };
        self.target = Some(target);
    }

    /// Stop, whatever state it is in. Every connect, every deliberate
    /// disconnect, and turning the setting off.
    pub fn cancel(&mut self) {
        *self = Reopen::default();
    }

    pub fn is_armed(&self) -> bool {
        self.target.is_some()
    }

    /// The path being watched, for a status line.
    pub fn waiting_for(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.path.as_str())
    }

    /// What to reopen with. `None` when nothing is armed.
    pub fn target(&self) -> Option<&tt_conn::ReopenTarget> {
        self.target.as_ref()
    }

    /// How long until there is something to do, so a frontend can sleep for
    /// exactly that long instead of asking on a timer of its own choosing.
    ///
    /// This is why the machine is not driven from `Session::tick`: that timer
    /// is `Qt::VeryCoarseTimer`, which rounds to whole seconds, and the shipped
    /// settle delay is 500 ms. A setting the program rounds to twice its value
    /// is worse than a setting it does not have. The transfer deadline
    /// (`Session::transfer_deadline`) is the same arrangement — the core owns
    /// the instant, the frontend owns the timer.
    ///
    /// `None` when nothing is armed. `Duration::ZERO` means now.
    ///
    /// **Idempotent in every state**, which is what makes it safe for a
    /// frontend to ask again after anything at all and restart its timer with
    /// the answer — see [`State::Waiting`] for what asking a relative question
    /// costs.
    pub fn deadline(&self, now: Instant) -> Option<Duration> {
        self.target.as_ref()?;
        let at = match self.state {
            State::Idle => return None,
            State::Waiting { next, .. } => next,
            State::Settling(at) | State::Retrying(at) => at,
        };
        Some(at.saturating_duration_since(now))
    }

    /// How long to leave a node that is not there before looking again. Fast
    /// while something is likely to be happening, slower once the wait has
    /// turned into an overnight one.
    fn poll_interval(since: Instant, now: Instant) -> Duration {
        if now.saturating_duration_since(since) < SLOW_AFTER {
            POLL_FAST
        } else {
            POLL_SLOW
        }
    }

    /// Step the machine. `present` is [`tt_conn::serial::present`] for
    /// [`Reopen::waiting_for`] — passed in rather than asked here so that the
    /// tests are about the algorithm, the way [`crate::bell`]'s `now` is.
    pub fn poll(&mut self, now: Instant, present: bool, limits: &ReopenLimits) -> ReopenAction {
        let Some(target) = self.target.as_ref() else {
            return ReopenAction::Nothing;
        };
        match self.state {
            State::Idle => ReopenAction::Nothing,
            State::Waiting { since, next } => {
                if !present {
                    // Only once the look was due, so an early or spurious call
                    // reads the node and does not push the next one out.
                    if now >= next {
                        self.state = State::Waiting {
                            since,
                            next: now + Self::poll_interval(since, now),
                        };
                    }
                    return ReopenAction::Nothing;
                }
                // **`cfg!(windows)` first, and it is not a shortcut.**
                // [`tt_conn::serial::is_stable_path`] answers false for every
                // Windows path *correctly* — there is no topology spelling of a
                // `COM<n>` for it to recognise, and `PortInfo::stable_id` needs
                // that answer. But the question here is a different one: is this
                // reopen a guess? On Windows it never is, because
                // `QueryDosDeviceW` named the exact port, so the doubt
                // [`ReopenLimits::delay_unknown`] is for — udev's, and the
                // attach-order doubt behind `/dev/ttyUSB<n>` — has no Windows
                // half. Reading the one answer as the other spent the
                // guess-length two seconds on every Windows reopen, which is the
                // opposite of what that field's own documentation promises.
                let delay = if cfg!(windows) || tt_conn::serial::is_stable_path(&target.path) {
                    limits.delay
                } else {
                    limits.delay_unknown
                };
                self.state = State::Settling(now + delay);
                ReopenAction::Nothing
            }
            State::Settling(at) => {
                if !present {
                    // It bounced. Back to the start of the wait, and this costs
                    // nothing: the budget is for opens that failed, and no open
                    // has been tried. The poll goes back to its fast rate for
                    // the same reason — something is happening.
                    self.state = State::Waiting {
                        since: now,
                        next: now + POLL_FAST,
                    };
                    return ReopenAction::Nothing;
                }
                if now < at {
                    return ReopenAction::Nothing;
                }
                ReopenAction::Attempt
            }
            State::Retrying(at) => {
                if now < at {
                    return ReopenAction::Nothing;
                }
                ReopenAction::Attempt
            }
        }
    }

    /// Report what an [`ReopenAction::Attempt`] did.
    ///
    /// `opened == false` also covers "the node had gone again by the time we
    /// tried", which upstream charges a retry for without opening anything —
    /// `OpenSerial`'s `CheckComPort` guard falls through to the same
    /// `retry_left_--` (`vtwin.cpp:477`, `:519`).
    pub fn attempted(&mut self, now: Instant, opened: bool, limits: &ReopenLimits) -> ReopenAction {
        if opened {
            self.cancel();
            return ReopenAction::Nothing;
        }
        if self.left == 0 {
            self.cancel();
            return ReopenAction::GiveUp;
        }
        self.left -= 1;
        self.state = State::Retrying(now + limits.retry_interval);
        ReopenAction::Nothing
    }

    /// Is this the last attempt — the one upstream lets raise its error box
    /// (`vtwin.cpp:481` tests `retry_left_ != 0`)?
    ///
    /// Nothing modal happens here, but it is the same edge: the message is worth
    /// showing once, when there is no further attempt coming.
    pub fn is_last_attempt(&self) -> bool {
        self.left == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tt_conn::serial::SerialParams;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn target(path: &str) -> tt_conn::ReopenTarget {
        tt_conn::ReopenTarget {
            path: path.into(),
            params: SerialParams::default(),
        }
    }

    /// The stable name, so the short delay applies and the arithmetic is about
    /// the retries rather than about which wait was picked. On Windows the
    /// short delay applies to every path, so there it is simply a path — which
    /// is why every test below it runs on both.
    const STABLE: &str = "/dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0";

    #[test]
    fn nothing_happens_until_it_is_armed() {
        let base = Instant::now();
        let mut r = Reopen::default();
        assert!(!r.is_armed());
        assert_eq!(
            r.poll(base, true, &ReopenLimits::default()),
            ReopenAction::Nothing
        );
    }

    #[test]
    fn the_wait_for_the_node_is_indefinite() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);

        // A whole day of the board being switched off.
        for hour in 0..24 {
            assert_eq!(
                r.poll(at(base, hour * 3_600_000), false, &limits),
                ReopenAction::Nothing
            );
        }
        assert!(r.is_armed());
        assert_eq!(r.waiting_for(), Some(STABLE));

        // And it is still ready when the node comes back.
        assert_eq!(
            r.poll(at(base, 86_400_000), true, &limits),
            ReopenAction::Nothing,
            "the node is back; the settle wait starts"
        );
        assert_eq!(
            r.poll(at(base, 86_400_600), true, &limits),
            ReopenAction::Attempt
        );
    }

    #[test]
    fn the_node_is_left_alone_for_the_settling_delay() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);

        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 499), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);
    }

    /// A kernel name may be a different adapter than the one that left, so it
    /// takes the longer wait. The topology name cannot be, and takes the short
    /// one — checked above.
    ///
    /// **Unix only, because the doubt is udev's.** `/dev/ttyUSB0` can appear
    /// before the rules that make it openable have run, and it is assigned in
    /// attach order besides. Neither is true of a `COM<n>`, which is what the
    /// Windows half below pins.
    #[cfg(unix)]
    #[test]
    fn a_kernel_name_waits_longer_than_a_topology_name() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target("/dev/ttyUSB0"), &limits);

        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 1999), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 2000), true, &limits), ReopenAction::Attempt);
    }

    /// ...and the other side of it: **Windows takes the short wait for every
    /// path**, which is the one place this port is less uncertain than
    /// upstream. `is_stable_path` cannot say so — it has no topology spelling
    /// of a `COM<n>` to recognise — so without the `cfg!(windows)` arm in
    /// `poll` every Windows reopen spends the guess-length two seconds on a
    /// port `QueryDosDeviceW` named exactly.
    #[cfg(windows)]
    #[test]
    fn windows_takes_the_short_wait_for_a_kernel_name() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target("COM3"), &limits);

        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 499), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);
    }

    #[test]
    fn a_successful_open_ends_it() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);
        r.poll(base, true, &limits);
        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);

        assert_eq!(
            r.attempted(at(base, 500), true, &limits),
            ReopenAction::Nothing
        );
        assert!(!r.is_armed());
        assert_eq!(r.waiting_for(), None);
    }

    /// Three retries is four attempts, which is what the setting's name means
    /// and what `ttset.c:1094`'s note in the schema says.
    #[test]
    fn three_retries_is_four_attempts() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);
        r.poll(base, true, &limits);

        let mut now = at(base, 500);
        let mut attempts = 0;
        loop {
            match r.poll(now, true, &limits) {
                ReopenAction::Attempt => {
                    attempts += 1;
                    assert_eq!(r.is_last_attempt(), attempts == 4);
                    if r.attempted(now, false, &limits) == ReopenAction::GiveUp {
                        break;
                    }
                }
                ReopenAction::Nothing => {}
                ReopenAction::GiveUp => panic!("give up comes from `attempted`"),
            }
            now = at(
                base,
                (now.saturating_duration_since(base).as_millis() as u64) + 100,
            );
            assert!(
                now < at(base, 60_000),
                "it should have given up long before this"
            );
        }
        assert_eq!(attempts, 4);
        assert!(!r.is_armed(), "giving up disarms it");
    }

    /// Zero retries is one attempt, not none.
    #[test]
    fn no_retries_still_tries_once() {
        let base = Instant::now();
        let limits = ReopenLimits {
            retries: 0,
            ..ReopenLimits::default()
        };
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);
        r.poll(base, true, &limits);

        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);
        assert!(r.is_last_attempt());
        assert_eq!(
            r.attempted(at(base, 500), false, &limits),
            ReopenAction::GiveUp
        );
        assert!(!r.is_armed());
    }

    /// An attempt where the node has gone again costs a retry without anything
    /// having been opened — upstream's `CheckComPort` guard.
    #[test]
    fn an_attempt_with_the_node_gone_still_costs_a_retry() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);
        r.poll(base, true, &limits);
        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);

        // The caller found `present()` false and reports it as a failed
        // attempt without having opened anything.
        r.attempted(at(base, 500), false, &limits);
        assert!(!r.is_last_attempt(), "one of three retries is spent");
        assert_eq!(
            r.poll(at(base, 1499), false, &limits),
            ReopenAction::Nothing
        );
        assert_eq!(
            r.poll(at(base, 1500), false, &limits),
            ReopenAction::Attempt,
            "the retry timer does not consult the node — the attempt does"
        );
    }

    /// A supply that bounces on the way up is the case this feature is for, so
    /// a node that leaves again while settling costs nothing. Upstream gives
    /// up instead (`vtwin.cpp:411`).
    #[test]
    fn a_node_that_bounces_goes_back_to_waiting_and_keeps_its_budget() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);

        for i in 0..5 {
            let t = 1000 * i;
            assert_eq!(r.poll(at(base, t), true, &limits), ReopenAction::Nothing);
            assert_eq!(
                r.poll(at(base, t + 100), false, &limits),
                ReopenAction::Nothing,
                "gone again before the settle ran out"
            );
        }
        assert!(r.is_armed());

        // Settled at last, and with the whole budget still there.
        assert_eq!(r.poll(at(base, 6000), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 6500), true, &limits), ReopenAction::Attempt);
        assert!(!r.is_last_attempt());
    }

    #[test]
    fn cancelling_forgets_the_port() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);
        r.cancel();
        assert!(!r.is_armed());
        assert_eq!(r.waiting_for(), None);
        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
    }

    /// The parameters travel with the path, because they are the port's own and
    /// not the settings file's.
    #[test]
    fn the_speed_it_was_using_is_what_comes_back() {
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(
            Instant::now(),
            tt_conn::ReopenTarget {
                path: STABLE.into(),
                params: SerialParams {
                    baud: 921_600,
                    ..SerialParams::default()
                },
            },
            &limits,
        );
        assert_eq!(r.target().expect("armed").params.baud, 921_600);
    }

    /// The frontend sleeps for exactly this, so it has to be the real wait and
    /// not a rounded one — which is the whole reason the machine is not driven
    /// from the once-a-second session tick.
    #[test]
    fn the_deadline_is_what_a_frontend_should_sleep_for() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        assert_eq!(r.deadline(base), None, "nothing armed, nothing to wait for");

        r.arm(base, target(STABLE), &limits);
        assert_eq!(r.deadline(base), Some(POLL_FAST));

        // The node arrives: now there is a real instant to wait until, and it
        // is the settle delay rather than a poll.
        r.poll(base, true, &limits);
        assert_eq!(r.deadline(base), Some(Duration::from_millis(500)));
        assert_eq!(r.deadline(at(base, 200)), Some(Duration::from_millis(300)));
        assert_eq!(
            r.deadline(at(base, 900)),
            Some(Duration::ZERO),
            "past the deadline is now, not a negative"
        );
    }

    /// An indefinite wait must not stay a twice-a-second wakeup for ever.
    ///
    /// Driven the way a frontend drives it — look when the deadline says to,
    /// and ask for the next one — because the back-off is a property of the
    /// looking rather than of the asking.
    #[test]
    fn the_poll_backs_off_once_the_wait_is_long() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);

        let mut ms = 0u64;
        let mut last = POLL_FAST;
        while ms < 29_999 {
            let d = r.deadline(at(base, ms)).expect("armed");
            ms += d.as_millis() as u64;
            r.poll(at(base, ms), false, &limits);
            last = d;
        }
        assert_eq!(last, POLL_FAST, "under half a minute it is the fast rate");
        assert_eq!(
            r.deadline(at(base, ms)),
            Some(POLL_SLOW),
            "and then it is not"
        );

        // ...and a node that appears and goes again is activity, so the fast
        // rate comes back with it.
        r.poll(at(base, 40_000), true, &limits);
        r.poll(at(base, 40_100), false, &limits);
        assert_eq!(r.deadline(at(base, 40_100)), Some(POLL_FAST));
    }

    /// The deadline is a question a frontend asks again after *anything* — it
    /// re-reads it and restarts one timer, and `Session::mouse` re-reads it on
    /// every mouse-move event. So asking must not move the answer: an interval
    /// measured from now would make a moving pointer over a waiting terminal
    /// postpone the look for ever, and the adapter would come back unnoticed
    /// until the pointer stopped.
    #[test]
    fn asking_for_the_deadline_does_not_move_it() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(base, target(STABLE), &limits);

        // A pointer moving over the terminal, once a frame, for the whole wait.
        for ms in 0..500 {
            assert_eq!(
                r.deadline(at(base, ms)),
                Some(Duration::from_millis(500 - ms)),
                "the answer counts down rather than starting again"
            );
        }
        assert_eq!(r.deadline(at(base, 500)), Some(Duration::ZERO));

        // ...and the same for a wait long enough to have backed off, which is
        // where an interval-shaped answer starves outright: 2 s of poll against
        // a question asked every few milliseconds.
        let mut ms = 0u64;
        while ms < 40_000 {
            let d = r.deadline(at(base, ms)).expect("armed");
            ms += d.as_millis() as u64;
            r.poll(at(base, ms), false, &limits);
        }
        let due = r.deadline(at(base, ms)).expect("armed");
        assert_eq!(due, POLL_SLOW);
        for step in 0..due.as_millis() as u64 {
            assert_eq!(
                r.deadline(at(base, ms + step)),
                Some(due - Duration::from_millis(step))
            );
        }
    }
}
