//! What a connection has moved, and for how long.
//!
//! Nothing upstream counts anything, so there is no `ttset.c` line to
//! transcribe here and no `TERATERM.INI` key behind it — this is deviation 21,
//! and `docs/counters.md` is what a user reads. The question it answers is the
//! first one anybody asks of a link that is not working: *is anything coming
//! out of this thing at all*, which today takes a session log and a text
//! editor.
//!
//! It lives on [`Session`](crate::Session) rather than in the frontend because
//! the numbers are a fact about the connection, not about a widget: the status
//! strip, `ttctl status` and a duplicated session all have to agree, and a
//! rate each of them worked out by differencing its own two polls would give
//! three answers.
//!
//! The rate is the only part with a shape worth arguing about — see
//! [`CounterState::rates`].

use std::time::{Duration, Instant};

/// How long a bucket collects before it becomes a rate.
const WINDOW: Duration = Duration::from_secs(1);

/// How quiet a line has to be before its rate reads zero. Longer than
/// [`WINDOW`] on purpose: at exactly one window a rate would flicker to zero
/// between two reads that both saw traffic.
const STALE: Duration = Duration::from_secs(2);

/// What this connection has moved, and for how long. A snapshot — see
/// [`Session::counters`](crate::Session::counters).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Bytes the transport handed over, before the input stream filter: a Lua
    /// filter that rewrites the stream has not changed what the cable carried.
    /// A file transfer's traffic is in here, which is deliberately not what
    /// the session log does — see [`CounterState::record_in`].
    pub bytes_in: u64,
    /// Bytes the transport accepted, after the output stream filter.
    pub bytes_out: u64,
    /// Line endings received: `CR`, `LF` or `CR LF` counts one each.
    ///
    /// A count of the *stream*, so a bare `CR` inside an escape sequence
    /// counts. The alternative is to ask the parser how many line feeds it
    /// executed, which is the fully honest answer and needs a counter in
    /// `tt-vt` — the reason it is not built that way is written down in
    /// `docs/deviations.md`. What this must never be is `Grid::scrolled_off`:
    /// that counts lines which left the page, so it reads zero on a session
    /// that never filled the screen.
    pub lines_in: u64,
    /// Breaks received, from whichever transport can produce one.
    pub breaks: u64,
    /// Bytes per second over the last complete second — zero once the line has
    /// been quiet for two, and zero from the moment it ended.
    pub rate_in: u64,
    pub rate_out: u64,
    /// How long the connection has been up, or how long the last one lasted.
    /// `None` until something connects.
    pub connected_for: Option<Duration>,
    /// False means the numbers above belong to a connection that has ended.
    pub live: bool,
}

/// The counts, plus what the rate is worked out from.
///
/// Everything in here is reset by a connect and frozen by a disconnect. That
/// is the opposite of what the terminal does — [`Session::connect`] keeps the
/// scrollback on purpose — and it is right for a counter: a byte total
/// spanning three reconnects is not a number anybody can use, and the readout
/// names the connect time beside it so the total always says what it covers.
#[derive(Clone, Debug)]
pub(crate) struct CounterState {
    bytes_in: u64,
    bytes_out: u64,
    lines_in: u64,
    breaks: u64,
    /// A `CR` ended the last read, so an `LF` opening the next one is the
    /// second half of a `CR LF` and not a line of its own.
    last_was_cr: bool,
    /// When the open bucket started collecting.
    window_at: Instant,
    window_in: u64,
    window_out: u64,
    /// What the last bucket to close worked out to.
    rate_in: u64,
    rate_out: u64,
    /// When the connection ended, so the clock stops.
    ///
    /// A second instant rather than clearing `Session::connected_at`, which
    /// `sync_log_epoch` reads for the `ElapsedConnection` log timestamp and
    /// which therefore cannot be touched here.
    stopped_at: Option<Instant>,
}

impl Default for CounterState {
    fn default() -> Self {
        CounterState::new(Instant::now())
    }
}

impl CounterState {
    fn new(at: Instant) -> CounterState {
        CounterState {
            bytes_in: 0,
            bytes_out: 0,
            lines_in: 0,
            breaks: 0,
            last_was_cr: false,
            window_at: at,
            window_in: 0,
            window_out: 0,
            rate_in: 0,
            rate_out: 0,
            stopped_at: None,
        }
    }

    /// A connection opened. Everything starts again from here.
    pub(crate) fn restart(&mut self, at: Instant) {
        *self = CounterState::new(at);
    }

    /// A connection ended: the clock stops here and both rates read zero from
    /// this moment, while every total stays as it was.
    ///
    /// The first call wins: the three teardown paths reach
    /// [`Session::connection_closed`] once each, but a second stop would move
    /// a clock that has already been read.
    pub(crate) fn stop(&mut self, at: Instant) {
        if self.stopped_at.is_none() {
            self.stopped_at = Some(at);
        }
    }

    /// Bytes arrived, and the line endings in them.
    ///
    /// Counted at the transport read rather than beside `log_bytes_in`, which
    /// is the opposite of the session log's choice and deliberate: the pump's
    /// transfer arm `continue`s before the log ever sees a byte, so a ZMODEM
    /// download is invisible to the log in either mode. A counter whose whole
    /// job is "is anything coming out of this thing" must not go blank for the
    /// one case where a great deal is.
    pub(crate) fn record_in(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        for &b in bytes {
            match b {
                b'\r' => {
                    self.lines_in += 1;
                    self.last_was_cr = true;
                }
                b'\n' => {
                    if !self.last_was_cr {
                        self.lines_in += 1;
                    }
                    self.last_was_cr = false;
                }
                _ => self.last_was_cr = false,
            }
        }
        let n = bytes.len() as u64;
        self.bytes_in += n;
        // The clock is read here and not at the top, so a quiet line's `Ok(0)`
        // read — the common case, on every session, several times a second —
        // costs the emptiness test and nothing else.
        self.roll(Instant::now());
        self.window_in += n;
    }

    /// Bytes went out. `n` is what the transport accepted, so a short write
    /// under flow control counts what went rather than what was asked for.
    pub(crate) fn record_out(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let n = n as u64;
        self.bytes_out += n;
        self.roll(Instant::now());
        self.window_out += n;
    }

    pub(crate) fn record_break(&mut self) {
        self.breaks += 1;
    }

    /// Close the open bucket if it has been collecting for long enough.
    fn roll(&mut self, now: Instant) {
        let age = now.saturating_duration_since(self.window_at);
        if age < WINDOW {
            return;
        }
        // A bucket that stayed open past the staleness bound belongs to a line
        // that went quiet, whatever trickled through it. Publishing
        // `bytes / age` here would smear a burst that ended nine seconds ago
        // across the nine quiet seconds after it.
        let (rate_in, rate_out) = if age >= STALE {
            (0, 0)
        } else {
            (
                per_second(self.window_in, age),
                per_second(self.window_out, age),
            )
        };
        self.rate_in = rate_in;
        self.rate_out = rate_out;
        self.window_at = now;
        self.window_in = 0;
        self.window_out = 0;
    }

    /// The two rates, decayed on **read** and without mutating anything.
    ///
    /// Decaying on read is the load-bearing half. An idle line never calls
    /// [`pump`](crate::Session::pump), so a rate published when a bucket closed
    /// and never revisited would sit on the screen saying 12 MB/s at a console
    /// that has been silent all afternoon. The same rule as [`roll`], applied
    /// where the answer is asked for — which is also what keeps the getter
    /// `&self`, and with it the C ABI's `const TtSession *`.
    fn rates(&self, now: Instant) -> (u64, u64) {
        // A connection that has ended is quiet, whatever its last bucket held.
        // Without this the bucket goes on decaying against the wall clock for
        // the two seconds after the line dropped, so a dimmed field can still
        // be claiming 1.2 MB/s of a cable nobody is holding — and the three
        // tests asserting a dead line reads zero pass only while the
        // disconnect lands inside the first window. Everything else about a
        // stopped connection is frozen; a rate is the one number for which
        // frozen and zero are the same answer.
        if self.stopped_at.is_some() {
            return (0, 0);
        }
        let age = now.saturating_duration_since(self.window_at);
        if age >= STALE {
            (0, 0)
        } else if age >= WINDOW {
            (
                per_second(self.window_in, age),
                per_second(self.window_out, age),
            )
        } else {
            (self.rate_in, self.rate_out)
        }
    }

    /// The snapshot. `connected_at` is the session's, because the log epoch
    /// owns it.
    pub(crate) fn snapshot(&self, connected_at: Option<Instant>, live: bool) -> Counters {
        let now = Instant::now();
        let (rate_in, rate_out) = self.rates(now);
        let connected_for = match (connected_at, self.stopped_at) {
            (None, _) => None,
            (Some(at), Some(end)) => Some(end.saturating_duration_since(at)),
            (Some(at), None) => Some(now.saturating_duration_since(at)),
        };
        Counters {
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
            lines_in: self.lines_in,
            breaks: self.breaks,
            rate_in,
            rate_out,
            connected_for,
            live,
        }
    }
}

/// Integer throughout: the core has no business rounding a byte count through
/// a float, and `u128` is what keeps a terabyte in a millisecond from wrapping.
fn per_second(bytes: u64, over: Duration) -> u64 {
    let ms = over.as_millis().max(1);
    u64::try_from(u128::from(bytes) * 1000 / ms).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No `sleep` anywhere in here. Every case drives the clock by hand, which
    /// is both faster and the only way to assert the staleness bound without a
    /// two-second test.
    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn line_endings_count_once_each() {
        let mut c = CounterState::new(Instant::now());
        c.record_in(b"a\rb\nc\r\nd");
        assert_eq!(c.lines_in, 3);
        assert_eq!(c.bytes_in, 8);
    }

    #[test]
    fn a_cr_lf_split_across_two_reads_is_one_line() {
        let mut c = CounterState::new(Instant::now());
        c.record_in(b"hello\r");
        c.record_in(b"\nworld");
        assert_eq!(c.lines_in, 1);
    }

    #[test]
    fn a_lone_cr_at_the_end_of_a_read_still_counts() {
        let mut c = CounterState::new(Instant::now());
        c.record_in(b"prompt\r");
        c.record_in(b"more");
        assert_eq!(c.lines_in, 1);
    }

    #[test]
    fn an_empty_read_changes_nothing() {
        let mut c = CounterState::new(Instant::now());
        c.record_in(b"\r");
        let before = c.clone();
        c.record_in(b"");
        c.record_out(0);
        assert_eq!(c.bytes_in, before.bytes_in);
        assert_eq!(c.lines_in, before.lines_in);
        assert_eq!(c.window_at, before.window_at);
    }

    #[test]
    fn a_closed_window_publishes_its_average() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        c.window_in = 1000;
        c.roll(at(base, 1000));
        assert_eq!(c.rate_in, 1000);
        assert_eq!(c.window_in, 0);
    }

    #[test]
    fn a_burst_is_reported_from_the_open_window_and_then_decays() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        c.window_in = 4000;
        // Inside the window: the last closed bucket's answer, which is zero.
        assert_eq!(c.rates(at(base, 500)), (0, 0));
        // Past it: the burst, averaged over how long it has been sitting there.
        assert_eq!(c.rates(at(base, 1000)), (4000, 0));
        assert_eq!(c.rates(at(base, 1600)), (2500, 0));
        // Past the staleness bound: quiet.
        assert_eq!(c.rates(at(base, 2000)), (0, 0));
        assert_eq!(c.rates(at(base, 9000)), (0, 0));
    }

    #[test]
    fn a_byte_after_a_long_silence_publishes_zero_rather_than_a_smear() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        c.window_in = 4000;
        c.roll(at(base, 30_000));
        assert_eq!(
            c.rate_in, 0,
            "a bucket left open for half a minute is quiet"
        );
        assert_eq!(c.window_at, at(base, 30_000));
    }

    #[test]
    fn the_clock_runs_and_then_freezes() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        assert!(c.snapshot(None, false).connected_for.is_none());

        let running = c.snapshot(Some(base), true).connected_for.unwrap();
        assert!(running >= Duration::ZERO);

        c.stop(at(base, 5000));
        assert_eq!(
            c.snapshot(Some(base), false).connected_for,
            Some(Duration::from_millis(5000))
        );
        // Idempotent: three teardown paths reach `stop`, and a second call
        // would move a clock somebody has already read.
        c.stop(at(base, 9000));
        assert_eq!(
            c.snapshot(Some(base), false).connected_for,
            Some(Duration::from_millis(5000))
        );
    }

    /// A stopped connection reads zero however busy its last second was, and
    /// it does so at once rather than two seconds later. The rate is read at
    /// the moment somebody asks, so without this the answer depends on how
    /// long ago the line dropped — which is a clock the caller cannot see.
    #[test]
    fn a_stopped_connection_is_quiet_whatever_it_was_doing() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        c.bytes_in = 4000;
        c.window_in = 4000;
        c.roll(at(base, 1000));
        assert_eq!(c.rates(at(base, 1000)), (4000, 0), "still running");

        c.stop(at(base, 1000));
        assert_eq!(c.rates(at(base, 1000)), (0, 0));
        // And the open bucket cannot come back either, at any distance.
        assert_eq!(c.rates(at(base, 1500)), (0, 0));
        let s = c.snapshot(Some(base), false);
        assert_eq!((s.rate_in, s.rate_out), (0, 0));
        assert_eq!(s.bytes_in, 4000, "the totals are untouched by the freeze");
    }

    #[test]
    fn a_restart_forgets_everything() {
        let base = Instant::now();
        let mut c = CounterState::new(base);
        c.record_in(b"hello\r\n");
        c.record_out(4);
        c.record_break();
        c.stop(at(base, 1000));

        c.restart(at(base, 2000));
        let s = c.snapshot(Some(at(base, 2000)), true);
        assert_eq!(
            (s.bytes_in, s.bytes_out, s.lines_in, s.breaks),
            (0, 0, 0, 0)
        );
        assert_eq!((s.rate_in, s.rate_out), (0, 0));
        assert!(s.connected_for.is_some(), "the new connection is running");
    }

    #[test]
    fn per_second_does_not_wrap() {
        assert_eq!(per_second(0, Duration::from_secs(1)), 0);
        assert_eq!(per_second(100, Duration::from_millis(250)), 400);
        // A zero-length window is a divide by zero waiting to happen.
        assert_eq!(per_second(7, Duration::ZERO), 7000);
        assert_eq!(per_second(u64::MAX, Duration::from_millis(1)), u64::MAX);
    }
}
