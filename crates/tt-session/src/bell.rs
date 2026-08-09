//! `RingBell`'s governor — `vtterm.c:5791`.
//!
//! Four settings stand between a BEL and a noise, and they exist for one case
//! the documentation names outright: a binary file shown by mistake is
//! thousands of BELs, and a terminal that honours every one of them cannot be
//! used until it stops. So the terminal counts bells inside a window, and when
//! there have been too many it goes quiet.
//!
//! It is here rather than in `tt-vt` because it needs a clock, and `Vt` has
//! none — being a function of its bytes is what lets the differential suite
//! and the fuzzers compare it against real Tera Term at all. Upstream keeps the
//! same three variables in `vtterm.c` and reads `GetTickCount()`.

use std::time::{Duration, Instant};

/// The three numbers the governor is built from, out of the settings.
#[derive(Clone, Copy, Debug)]
pub struct BellLimits {
    /// `ts.BeepOverUsedCount` — how many bells a window allows.
    pub count: u32,
    /// `ts.BeepOverUsedTime`, in seconds. The window the count is spent over.
    pub over_used: Duration,
    /// `ts.BeepSuppressTime`, in seconds. How long the terminal stays quiet
    /// once the count is gone — see [`BellGovernor::ring`] for the fact that it
    /// is quiet time rather than elapsed time.
    pub suppress: Duration,
}

impl Default for BellLimits {
    fn default() -> Self {
        BellLimits {
            count: 5,
            over_used: Duration::from_secs(2),
            suppress: Duration::from_secs(5),
        }
    }
}

/// `BeepStartTime`, `BeepSuppressTime` and `BeepOverUsedCount`, which upstream
/// keeps as three file statics.
#[derive(Clone, Debug, Default)]
pub struct BellGovernor {
    /// When the current over-used window opened. `None` is upstream's
    /// initialiser, which back-dates the clock by exactly the window
    /// (`vtterm.c:350`) so that the first bell after a reset always sounds.
    start: Option<Instant>,
    /// When suppression last saw a bell. `None` means not suppressing, which
    /// is the same back-dating one line up.
    suppress: Option<Instant>,
    /// Bells left in the window, counting down.
    left: u32,
}

impl BellGovernor {
    /// Step the governor once and say whether this bell is heard.
    ///
    /// Two things about this are surprising and both are upstream's:
    ///
    /// * **The bell that trips the limit still sounds.** The inner test decides
    ///   the *next* one's fate — it sets the suppression clock and falls
    ///   through to the noise, because the switch that makes it sits outside
    ///   the `if` (`vtterm.c:5800`). So a `count` of 5 is heard six times, and
    ///   `teraterm-term.html`'s worked example is off by one against its own
    ///   code.
    /// * **Suppression measures quiet, not elapsed time.** The arm that finds
    ///   itself suppressed assigns `now` to the clock it just tested
    ///   (`vtterm.c:5797`), so every bell arriving during the silence pushes
    ///   the end of it further out. A host beeping steadily is silenced until
    ///   it stops and for `suppress` afterwards — which the manual's "for ten
    ///   seconds" does not say, and which is the behaviour that actually does
    ///   the job the feature exists for.
    ///
    /// `now` is passed in rather than read here so that a burst arriving in one
    /// read is stepped against one instant, the way it would be against one
    /// `GetTickCount()` — and so the tests can be about the algorithm.
    pub fn ring(&mut self, now: Instant, limits: &BellLimits) -> bool {
        if self
            .suppress
            .is_some_and(|t| now.saturating_duration_since(t) < limits.suppress)
        {
            self.suppress = Some(now);
            return false;
        }
        if self
            .start
            .is_some_and(|t| now.saturating_duration_since(t) < limits.over_used)
        {
            if self.left <= 1 {
                self.suppress = Some(now);
            } else {
                self.left -= 1;
            }
        } else {
            self.start = Some(now);
            self.left = limits.count;
        }
        true
    }

    /// `ResetTerminal`'s block at `vtterm.c:348`, which puts both clocks back
    /// far enough that nothing is suppressed.
    pub fn reset(&mut self) {
        *self = BellGovernor::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// The documented example, and the off-by-one in it: five is the setting,
    /// six bells are heard.
    #[test]
    fn the_bell_that_trips_the_limit_is_still_heard() {
        let base = Instant::now();
        let limits = BellLimits::default();
        let mut g = BellGovernor::default();
        let heard: Vec<bool> = (0..8).map(|i| g.ring(at(base, i * 10), &limits)).collect();
        assert_eq!(
            heard,
            [true, true, true, true, true, true, false, false],
            "count=5 permits six"
        );
    }

    /// A gap longer than the window refills the count, which is what makes a
    /// beep every few seconds go on working for ever.
    #[test]
    fn a_quiet_gap_refills_the_count() {
        let base = Instant::now();
        let limits = BellLimits::default();
        let mut g = BellGovernor::default();
        for i in 0..6 {
            assert!(g.ring(at(base, i * 10), &limits));
        }
        assert!(!g.ring(at(base, 100), &limits));
        // Past the suppression, and past the over-used window with it.
        assert!(g.ring(at(base, 6_000), &limits));
    }

    /// The half the manual does not describe: bells during the silence push
    /// the end of it out, so a runaway stays silenced however long it runs.
    #[test]
    fn suppression_is_quiet_time_rather_than_elapsed_time() {
        let base = Instant::now();
        let limits = BellLimits::default();
        let mut g = BellGovernor::default();
        for i in 0..6 {
            assert!(g.ring(at(base, i * 10), &limits));
        }
        // One bell a second, for twenty seconds — four times the five-second
        // suppression. None of them is heard.
        for s in 1..=20 {
            assert!(!g.ring(at(base, s * 1000), &limits), "second {s}");
        }
        // And the silence still has its full length to run from the last one:
        // this bell at 24 s is refused *and* restarts the five seconds, so the
        // next one is not heard until 29.
        assert!(!g.ring(at(base, 24_000), &limits));
        assert!(!g.ring(at(base, 28_000), &limits));
        assert!(g.ring(at(base, 33_000), &limits));
    }

    /// `BeepSuppressTime=0` is a governor that suppresses nothing — the
    /// comparison is `<`, so a zero window is never open. Same for the
    /// over-used window, which then refills on every bell.
    #[test]
    fn zero_windows_disable_the_governor() {
        let base = Instant::now();
        let limits = BellLimits {
            count: 5,
            over_used: Duration::ZERO,
            suppress: Duration::ZERO,
        };
        let mut g = BellGovernor::default();
        for i in 0..50 {
            assert!(g.ring(at(base, i), &limits));
        }
    }

    /// `BeepOverUsedCount=0` is not "no bells": the first one opens the window
    /// and the second trips the limit and is heard anyway, so two get through.
    #[test]
    fn a_count_of_zero_still_lets_two_through() {
        let base = Instant::now();
        let limits = BellLimits {
            count: 0,
            ..BellLimits::default()
        };
        let mut g = BellGovernor::default();
        assert!(g.ring(at(base, 0), &limits));
        assert!(g.ring(at(base, 1), &limits));
        assert!(!g.ring(at(base, 2), &limits));
    }

    #[test]
    fn a_reset_puts_the_clocks_back() {
        let base = Instant::now();
        let limits = BellLimits::default();
        let mut g = BellGovernor::default();
        for i in 0..7 {
            g.ring(at(base, i * 10), &limits);
        }
        assert!(!g.ring(at(base, 100), &limits));
        g.reset();
        assert!(g.ring(at(base, 101), &limits));
    }
}
