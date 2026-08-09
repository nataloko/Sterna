//! The link between a running macro and the terminal it is driving.
//!
//! Upstream this is a *process* boundary: `ttpmacro.exe` and `ttermpro.exe`
//! talk over DDE, and `teraterm/ttdde.c` is the terminal's half of the
//! conversation. Here the macro is [`tt_ttl`] on a thread of its own and the
//! boundary is this module — but the shape it has to keep is the same one,
//! because the language was written against it.
//!
//! Two things cross. Bytes come **from** the terminal through [`MacroLink`], a
//! ring the session fills and the macro drains; everything else goes **to** the
//! terminal as a method call on the host. The ring is separate from the session
//! lock on purpose: a macro sitting in `wait` asks for a byte thousands of times
//! a second, and making each of those take the lock that the UI thread needs to
//! paint would be a frame rate decided by a script.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// `InBuffSize` (`tttypes.h:788`) — 64 KiB, and the reason a macro that stops
/// reading does not stop the terminal.
pub const MACRO_BUF_SIZE: usize = 64 * 1024;

/// The bytes a linked macro has not read yet.
///
/// **Full drops the oldest byte** (`ttdde.c:107`), which is the opposite of
/// what a queue usually does and is the right way round here: a macro that has
/// fallen behind wants the prompt that just arrived, not the one from four
/// screens ago, and the alternative — blocking the parser until a script gets
/// around to reading — would let a stalled macro freeze the window.
///
/// Cloning shares the ring; that is the point.
#[derive(Clone, Debug, Default)]
pub struct MacroLink(Arc<Mutex<Ring>>);

#[derive(Debug, Default)]
struct Ring {
    buf: VecDeque<u8>,
    /// How many bytes have been dropped for want of room since the link was
    /// made. Nothing upstream counts them — a macro cannot tell — but a
    /// silently lossy buffer is worth being able to see from a test.
    dropped: u64,
}

impl MacroLink {
    pub fn new() -> MacroLink {
        MacroLink::default()
    }

    /// Everything the tap collected this pump. Called by the session.
    pub fn push(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut r = self.0.lock().unwrap();
        for &b in bytes {
            if r.buf.len() == MACRO_BUF_SIZE {
                r.buf.pop_front();
                r.dropped += 1;
            }
            r.buf.push_back(b);
        }
    }

    /// One byte, or `None` when the macro has caught up. This is
    /// `ScriptHost::read_byte`'s whole implementation.
    pub fn pop(&self) -> Option<u8> {
        self.0.lock().unwrap().buf.pop_front()
    }

    /// Throw away what has not been read — `flushrecv`, and what linking does.
    pub fn clear(&self) {
        let mut r = self.0.lock().unwrap();
        r.buf.clear();
    }

    /// How many bytes are waiting.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes lost to a full ring since the link was made.
    pub fn dropped(&self) -> u64 {
        self.0.lock().unwrap().dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_come_out_in_the_order_they_went_in() {
        let link = MacroLink::new();
        assert!(link.is_empty());
        assert_eq!(link.pop(), None);
        link.push(b"abc");
        link.push(b"de");
        assert_eq!(link.len(), 5);
        let got: Vec<u8> = std::iter::from_fn(|| link.pop()).collect();
        assert_eq!(got, b"abcde");
        assert_eq!(link.dropped(), 0);
    }

    /// The oldest goes, not the newest — a macro that has fallen behind wants
    /// what just arrived.
    #[test]
    fn a_full_ring_drops_the_oldest_byte() {
        let link = MacroLink::new();
        link.push(&vec![b'x'; MACRO_BUF_SIZE]);
        assert_eq!(link.len(), MACRO_BUF_SIZE);
        link.push(b"yz");
        assert_eq!(link.len(), MACRO_BUF_SIZE);
        assert_eq!(link.dropped(), 2);
        // The two survivors are at the *end*.
        let all: Vec<u8> = std::iter::from_fn(|| link.pop()).collect();
        assert_eq!(&all[all.len() - 2..], b"yz");
        assert_eq!(all[0], b'x');
    }

    #[test]
    fn a_clone_is_the_same_ring() {
        let a = MacroLink::new();
        let b = a.clone();
        a.push(b"hello");
        assert_eq!(b.len(), 5);
        b.clear();
        assert!(a.is_empty());
    }
}
