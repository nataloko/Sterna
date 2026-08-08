//! Matching the far end's output — `ttmdde.c`'s `Wait`, `Wait2`, `WaitN` and
//! the received-line buffer.
//!
//! These live in the *macro* process upstream, not the terminal: the byte
//! stream is pulled across DDE one byte at a time and matched here. That is
//! exactly the shape the port wants, so the matcher comes over unchanged and
//! only the source of the bytes moves behind [`crate::ScriptHost`].
//!
//! All of it is byte-oriented and none of it knows about encodings. A `wait`
//! for a multi-byte character matches its bytes, and a partial character at the
//! end of a read is not a special case here — there is no decoding to be
//! partway through.

use crate::lexer::MAX_STR_LEN;

/// `wait` takes up to ten strings.
pub const MAX_WAIT: usize = 10;

/// One `wait` pattern and how much of it has matched so far.
#[derive(Debug, Clone, Default)]
struct Pattern {
    bytes: Vec<u8>,
    matched: usize,
    active: bool,
}

/// The state of a `wait`: up to ten patterns, matched a byte at a time.
#[derive(Debug, Clone, Default)]
pub struct WaitSet {
    pats: [Pattern; MAX_WAIT],
}

impl WaitSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// `ClearWait`.
    pub fn clear(&mut self) {
        for p in &mut self.pats {
            *p = Pattern::default();
        }
    }

    /// `SetWait` — install the 1-based `index`th pattern.
    pub fn set(&mut self, index: usize, bytes: &[u8]) {
        let p = &mut self.pats[index - 1];
        p.bytes = bytes.to_vec();
        p.matched = 0;
        p.active = true;
    }

    /// Whether the 1-based `index`th pattern is exactly `bytes` — `CmpWait`,
    /// which `waitln` uses to ask "was it the newline that matched?".
    pub fn is(&self, index: usize, bytes: &[u8]) -> bool {
        let p = &self.pats[index - 1];
        p.active && p.bytes == bytes
    }

    pub fn any(&self) -> bool {
        self.pats.iter().any(|p| p.active)
    }

    /// Feed one byte. Returns the 1-based index of a pattern that completed.
    ///
    /// Two details are upstream's and both are visible from a macro. The scan
    /// runs from the tenth pattern down to the first and overwrites its answer,
    /// so when several complete on the same byte the **lowest-numbered wins**.
    /// And an *empty* pattern completes on the first byte that arrives, because
    /// the "have I matched all of it" test is `matched == len` with both zero.
    pub fn feed(&mut self, b: u8) -> Option<usize> {
        let mut found = None;
        for i in (0..MAX_WAIT).rev() {
            let p = &mut self.pats[i];
            if !p.active {
                continue;
            }
            if p.bytes.get(p.matched) == Some(&b) {
                p.matched += 1;
            } else if p.matched > 0 {
                // Back off to the longest prefix that is also a suffix of what
                // has been seen — done by search rather than by a precomputed
                // table, which is upstream's and is why a long pattern costs.
                let mut j = p.matched as isize - 1;
                while j >= 0 {
                    let ju = j as usize;
                    if p.bytes[ju] == b
                        && (ju == 0 || p.bytes[..ju] == p.bytes[p.matched - ju..p.matched])
                    {
                        break;
                    }
                    j -= 1;
                }
                p.matched = if j >= 0 { j as usize + 1 } else { 0 };
            }
            if p.matched == p.bytes.len() {
                found = Some(i + 1);
            }
        }
        found
    }
}

/// `RecvLnBuff` — the bytes seen since the last newline, and what `inputstr`
/// is filled from.
///
/// It is 511 bytes and silently stops growing, so a `waitln` on a line longer
/// than that gets a truncated `inputstr` and no indication of it.
#[derive(Debug, Clone)]
pub struct RecvLine {
    buf: Vec<u8>,
    last: u8,
    /// `RecvLnClear`. On while an ordinary `wait` runs, off while `waitn` does
    /// — because `waitn` counts bytes and must not lose them at a line break.
    pub clear_on_newline: bool,
}

impl Default for RecvLine {
    fn default() -> Self {
        Self {
            buf: Vec::new(),
            last: 0,
            clear_on_newline: true,
        }
    }
}

impl RecvLine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// `PutRecvLnBuff` — the clear happens on the byte *after* a newline, not
    /// on the newline itself, so the terminator stays in the line it ended.
    pub fn put(&mut self, b: u8) {
        if self.last == 0x0a && self.clear_on_newline {
            self.buf.clear();
        }
        if self.buf.len() < MAX_STR_LEN - 1 {
            self.buf.push(b);
        }
        self.last = b;
    }

    /// `GetRecvLnBuff` — the line without its terminator, and the buffer is
    /// emptied. A CR is only dropped when it came immediately before the LF.
    pub fn take(&mut self) -> Vec<u8> {
        let mut out = std::mem::take(&mut self.buf);
        if out.last() == Some(&0x0a) {
            out.pop();
            if out.last() == Some(&0x0d) {
                out.pop();
            }
        }
        out
    }

    /// Read the line without emptying the buffer.
    pub fn peek(&self) -> &[u8] {
        &self.buf
    }
}

/// `waitrecv`'s state — a sliding window with a substring expected at a fixed
/// position in it.
///
/// It succeeds only when the window is **full**, so `waitrecv 'ok' 10 1` reads
/// ten bytes whatever happens; the substring being present early does not end
/// the wait, it only decides what the answer will be when the tenth byte lands.
#[derive(Debug, Clone, Default)]
pub struct WaitRecv {
    sub: Vec<u8>,
    len: usize,
    /// 1-based position in the window at which `sub` must appear.
    pos: usize,
    window: Vec<u8>,
    found: bool,
}

impl WaitRecv {
    /// `SetWait2`. The length grows to fit the substring and the position is
    /// clamped so that the substring can fit at it — neither is an error.
    ///
    /// An empty substring starts out *found*, so the command degenerates into
    /// "read exactly this many bytes".
    pub fn set(sub: &[u8], len: i32, pos: i32) -> Self {
        let sub = sub[..sub.len().min(MAX_STR_LEN - 1)].to_vec();
        let mut window_len = if len < 1 {
            0
        } else {
            (len as usize).min(MAX_STR_LEN - 1)
        };
        window_len = window_len.max(sub.len());

        let max_pos = window_len - sub.len() + 1;
        let pos = if pos < 1 {
            1
        } else {
            (pos as usize).min(max_pos)
        };

        WaitRecv {
            found: sub.is_empty(),
            sub,
            len: window_len,
            pos,
            window: Vec::new(),
        }
    }

    pub fn done(&self) -> bool {
        self.found && self.window.len() == self.len
    }

    pub fn found(&self) -> bool {
        self.found
    }

    pub fn window(&self) -> &[u8] {
        &self.window
    }

    /// Feed one byte. Returns whether the wait is now satisfied.
    pub fn feed(&mut self, b: u8) -> bool {
        if self.window.len() >= self.len {
            if self.len == 0 {
                return self.done();
            }
            self.window.remove(0);
        }
        self.window.push(b);

        if !self.found && self.window.len() >= self.pos + self.sub.len() - 1 {
            let at = self.pos - 1;
            self.found = self.window[at..at + self.sub.len()] == self.sub[..];
        }
        self.done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pats: &[&[u8]], input: &[u8]) -> Option<(usize, usize)> {
        let mut w = WaitSet::new();
        for (i, p) in pats.iter().enumerate() {
            w.set(i + 1, p);
        }
        for (n, &b) in input.iter().enumerate() {
            if let Some(i) = w.feed(b) {
                return Some((i, n + 1));
            }
        }
        None
    }

    #[test]
    fn a_pattern_matches_where_it_appears() {
        assert_eq!(run(&[b"login:"], b"xx login: yy"), Some((1, 9)));
        assert_eq!(run(&[b"nope"], b"xx login: yy"), None);
    }

    #[test]
    fn the_lowest_numbered_pattern_wins_a_tie() {
        // Both complete on the same byte; upstream scans downwards and the last
        // write is the one that survives.
        assert_eq!(run(&[b"ab", b"b"], b"ab"), Some((1, 2)));
        assert_eq!(run(&[b"b", b"ab"], b"ab"), Some((1, 2)));
    }

    #[test]
    fn a_failed_partial_match_backs_off_rather_than_restarting() {
        // "aab" against "aaab": the naive reset to zero would miss it.
        assert_eq!(run(&[b"aab"], b"aaab"), Some((1, 4)));
        // ...and the overlap has to be a real border, not just a repeated byte.
        assert_eq!(run(&[b"abab"], b"abaabab"), Some((1, 7)));
    }

    #[test]
    fn an_empty_pattern_matches_the_very_first_byte() {
        assert_eq!(run(&[b""], b"x"), Some((1, 1)));
    }

    #[test]
    fn the_line_buffer_clears_on_the_byte_after_the_newline() {
        let mut r = RecvLine::new();
        for &b in b"one\r\ntwo" {
            r.put(b);
        }
        assert_eq!(r.peek(), b"two");
        // Taking it strips the terminator, and only a CR that touched the LF.
        let mut r = RecvLine::new();
        for &b in b"one\r\n" {
            r.put(b);
        }
        assert_eq!(r.take(), b"one");
        let mut r = RecvLine::new();
        for &b in b"a\rb\n" {
            r.put(b);
        }
        assert_eq!(r.take(), b"a\rb");
    }

    #[test]
    fn the_line_buffer_stops_at_511_bytes_without_saying_so() {
        let mut r = RecvLine::new();
        for _ in 0..1000 {
            r.put(b'x');
        }
        assert_eq!(r.len(), MAX_STR_LEN - 1);
    }

    #[test]
    fn waitrecv_wants_the_substring_in_place_and_the_window_full() {
        let mut w = WaitRecv::set(b"ok", 4, 2);
        for &b in b"xok" {
            assert!(!w.feed(b), "not full yet");
        }
        assert!(w.feed(b'y'));
        assert_eq!(w.window(), b"xoky");

        // The position is measured inside the window and the window slides, so
        // a substring in the wrong place now can be in the right place once
        // more bytes have pushed it along.
        let mut w = WaitRecv::set(b"ok", 4, 1);
        for &b in b"xoky" {
            w.feed(b);
        }
        assert!(!w.found(), "\"ok\" is at position 2 of \"xoky\"");
        w.feed(b'o');
        assert!(w.found(), "the window is now \"okyo\"");

        // One that never lands at the position is never found.
        let mut w = WaitRecv::set(b"zz", 4, 1);
        for &b in b"abcdefgh" {
            w.feed(b);
        }
        assert!(!w.found());
    }

    #[test]
    fn waitrecv_grows_its_window_to_fit_and_clamps_its_position() {
        // A window shorter than the substring is widened to it.
        let mut w = WaitRecv::set(b"abc", 1, 1);
        assert!(!w.feed(b'a'));
        assert!(!w.feed(b'b'));
        assert!(w.feed(b'c'));

        // A position past the last place it could fit is pulled back to it.
        let mut w = WaitRecv::set(b"ab", 3, 99);
        for &b in b"xab" {
            w.feed(b);
        }
        assert!(w.done());
    }

    #[test]
    fn an_empty_substring_makes_waitrecv_a_byte_count() {
        let mut w = WaitRecv::set(b"", 3, 1);
        assert!(w.found());
        assert!(!w.feed(b'a'));
        assert!(!w.feed(b'b'));
        assert!(w.feed(b'c'));
    }
}
