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
    /// Upstream's guess is about the *notification*: some drivers send only
    /// `DBT_DEVTYP_DEVICEINTERFACE` and never the `DBT_DEVTYP_PORT` that would
    /// say which port arrived (`vtwin.cpp:335`). There is no such message here,
    /// so the same doubt is reached by the Linux fact instead: a port opened by
    /// its kernel name — `/dev/ttyUSB0`, or any `COM<n>` — may not be the
    /// device that left, because `ttyUSB<n>` is assigned in attach order. A
    /// port opened by its `/dev/serial/…` topology name is the same socket it
    /// always was and takes [`ReopenLimits::delay`].
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    /// The node is not there. Indefinite — upstream waits for a device-arrival
    /// message for ever, and a board switched off overnight should still be
    /// there in the morning.
    Waiting,
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
    pub fn arm(&mut self, target: tt_conn::ReopenTarget, limits: &ReopenLimits) {
        self.left = limits.retries;
        self.state = State::Waiting;
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

    /// Step the machine. `present` is [`tt_conn::serial::present`] for
    /// [`Reopen::waiting_for`] — passed in rather than asked here so that the
    /// tests are about the algorithm, the way [`crate::bell`]'s `now` is.
    pub fn poll(&mut self, now: Instant, present: bool, limits: &ReopenLimits) -> ReopenAction {
        let Some(target) = self.target.as_ref() else {
            return ReopenAction::Nothing;
        };
        match self.state {
            State::Idle => ReopenAction::Nothing,
            State::Waiting => {
                if !present {
                    return ReopenAction::Nothing;
                }
                let delay = if tt_conn::serial::is_stable_path(&target.path) {
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
                    // has been tried.
                    self.state = State::Waiting;
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
    /// the retries rather than about which wait was picked.
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
        r.arm(target(STABLE), &limits);

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
        r.arm(target(STABLE), &limits);

        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 499), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 500), true, &limits), ReopenAction::Attempt);
    }

    /// A kernel name may be a different adapter than the one that left, so it
    /// takes the longer wait. The topology name cannot be, and takes the short
    /// one — checked above.
    #[test]
    fn a_kernel_name_waits_longer_than_a_topology_name() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(target("/dev/ttyUSB0"), &limits);

        assert_eq!(r.poll(base, true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 1999), true, &limits), ReopenAction::Nothing);
        assert_eq!(r.poll(at(base, 2000), true, &limits), ReopenAction::Attempt);
    }

    #[test]
    fn a_successful_open_ends_it() {
        let base = Instant::now();
        let limits = ReopenLimits::default();
        let mut r = Reopen::default();
        r.arm(target(STABLE), &limits);
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
        r.arm(target(STABLE), &limits);
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
        r.arm(target(STABLE), &limits);
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
        r.arm(target(STABLE), &limits);
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
        r.arm(target(STABLE), &limits);

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
        r.arm(target(STABLE), &limits);
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
}
