//! Decoding the `PARMRK` input stream.
//!
//! Linux has no out-of-band way to tell a caller that a BREAK arrived: with
//! the default termios a break is delivered as a single `0x00`, which is
//! indistinguishable from a device sending a NUL. `PARMRK` (with `IGNPAR` and
//! `BRKINT` off) escapes the stream instead:
//!
//! | On the wire | Delivered |
//! |---|---|
//! | BREAK | `FF 00 00` |
//! | byte `b` with a parity or framing error | `FF 00 b` |
//! | a genuine `FF` | `FF FF` |
//! | anything else | itself |
//!
//! So the escaping is what makes the distinction possible, and undoing it is
//! what makes the port usable for binary data again — a file transfer that
//! saw doubled `FF` bytes would corrupt every eighth-bit-set byte it touched.
//!
//! Spike 4 established that the flag survives every `serialport-rs` call that
//! rewrites termios, which is why this is a decoder and not a fork.

/// Something that arrived on the wire but is not data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialEvent {
    /// A line break. Tera Term surfaces this to the session; a host running
    /// `getty` uses it to cycle baud rates, and Solaris consoles use it to
    /// drop to the PROM.
    Break,
    /// A byte that arrived with a parity or framing error. The byte is
    /// included because it is usually still readable and dropping it silently
    /// is worse than showing it.
    BadByte(u8),
}

/// The escape state, kept across reads because a marker can straddle two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    /// Seen `FF`.
    Escape,
    /// Seen `FF 00`.
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct Parmrk {
    state: State,
}

impl Parmrk {
    pub fn new() -> Self {
        Parmrk::default()
    }

    /// Decode `input`, appending data to `data` and anything else to `events`.
    ///
    /// Both outputs are appended to rather than replaced, so a caller can
    /// reuse one pair of buffers for the life of the connection.
    pub fn feed(&mut self, input: &[u8], data: &mut Vec<u8>, events: &mut Vec<SerialEvent>) {
        for &b in input {
            match self.state {
                State::Ground => {
                    if b == 0xff {
                        self.state = State::Escape;
                    } else {
                        data.push(b);
                    }
                }
                State::Escape => match b {
                    // `FF FF` is the escape for a real 0xFF.
                    0xff => {
                        self.state = State::Ground;
                        data.push(0xff);
                    }
                    0x00 => self.state = State::Error,
                    // Not a form the kernel produces. Passing both bytes
                    // through beats dropping data on a stream we do not fully
                    // understand.
                    other => {
                        self.state = State::Ground;
                        data.push(0xff);
                        data.push(other);
                    }
                },
                State::Error => {
                    self.state = State::Ground;
                    // A framing error whose byte is zero *is* the break: the
                    // line was held at space for longer than a character.
                    if b == 0x00 {
                        events.push(SerialEvent::Break);
                    } else {
                        events.push(SerialEvent::BadByte(b));
                    }
                }
            }
        }
    }

    /// True when a marker is half-decoded, so the caller knows a short read
    /// left something pending rather than ending cleanly.
    pub fn is_mid_marker(&self) -> bool {
        self.state != State::Ground
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(chunks: &[&[u8]]) -> (Vec<u8>, Vec<SerialEvent>) {
        let mut p = Parmrk::new();
        let (mut data, mut events) = (Vec::new(), Vec::new());
        for c in chunks {
            p.feed(c, &mut data, &mut events);
        }
        (data, events)
    }

    #[test]
    fn plain_bytes_pass_through() {
        let (data, events) = decode(&[b"hello"]);
        assert_eq!(data, b"hello");
        assert!(events.is_empty());
    }

    #[test]
    fn a_break_is_not_a_nul() {
        // The whole point: `FF 00 00` is a break, a bare `00` is data.
        let (data, events) = decode(&[&[b'X', 0xff, 0x00, 0x00, 0x00, b'Y']]);
        assert_eq!(data, b"X\x00Y");
        assert_eq!(events, vec![SerialEvent::Break]);
    }

    #[test]
    fn a_doubled_ff_is_one_byte() {
        let (data, events) = decode(&[&[0xff, 0xff, 0x41, 0xff, 0xff]]);
        assert_eq!(data, vec![0xff, 0x41, 0xff]);
        assert!(events.is_empty());
    }

    #[test]
    fn a_framing_error_keeps_its_byte() {
        let (data, events) = decode(&[&[0xff, 0x00, 0x41]]);
        assert!(data.is_empty());
        assert_eq!(events, vec![SerialEvent::BadByte(0x41)]);
    }

    #[test]
    fn a_marker_split_across_reads_still_decodes() {
        // A three-byte marker can arrive one byte per read; a decoder that
        // reset per call would turn a break into two NULs and an FF.
        let (data, events) = decode(&[&[0xff], &[0x00], &[0x00], b"ok"]);
        assert_eq!(data, b"ok");
        assert_eq!(events, vec![SerialEvent::Break]);
    }

    #[test]
    fn an_unknown_escape_keeps_both_bytes() {
        let (data, events) = decode(&[&[0xff, 0x41]]);
        assert_eq!(data, vec![0xff, 0x41]);
        assert!(events.is_empty());
    }

    #[test]
    fn a_trailing_escape_is_reported_as_pending() {
        let mut p = Parmrk::new();
        let (mut data, mut events) = (Vec::new(), Vec::new());
        p.feed(&[b'a', 0xff], &mut data, &mut events);
        assert_eq!(data, b"a");
        assert!(p.is_mid_marker());
        p.feed(&[0xff], &mut data, &mut events);
        assert_eq!(data, vec![b'a', 0xff]);
        assert!(!p.is_mid_marker());
    }
}
