//! The engine's properties, stated once, for both the fuzzer and the tests.
//!
//! Everything in the differential suite answers *is this what Tera Term does?*
//! and needs a case somebody wrote. These answer a different and much cheaper
//! question — *is the engine still self-consistent?* — and a machine can ask it
//! about any byte string at all. That matters because the parser's whole job is
//! reading untrusted bytes off a wire: a serial console at the wrong baud rate
//! and a hostile SSH server produce the same thing, which is a stream nobody
//! chose.
//!
//! Two properties, and the second is the interesting one:
//!
//! - **[`vt_stream`]** — no panic, and [`Grid::check_invariants`] holds after
//!   every chunk. A stream that panics has no dump to diff, so this is ground
//!   `run_diff.sh` cannot reach at all.
//! - **[`vt_chunking`]** — where the chunk boundaries fall must not change the
//!   result. This is not a theoretical property. Bytes arrive from a socket or
//!   a serial port in whatever sizes the kernel felt like, so *every* stream is
//!   already a chunked stream, and the engine keeps real state across the
//!   boundary: `vte`'s parser position, and `tt-vt`'s own `pending_c2` /
//!   `utf8_left`, which exist precisely because a UTF-8 sequence can be cut in
//!   half. A bug here is invisible to every test that feeds a whole file.
//!
//! Both are shared by `crates/fuzz/` (libFuzzer, nightly) and by this crate's
//! own tests (stable, in CI, replaying the differential corpus and every crash
//! the fuzzer has ever found). The fuzzer explores; the tests are what stop a
//! fixed bug coming back.

use tt_conn::telnet::protocol::{Telnet, TelnetParams};
use tt_vt::{Config, Vt};

/// Feed `chunks` to a fresh terminal and check the grid after each one.
///
/// Returns the violation rather than panicking, so a caller can attach its own
/// context — the fuzz target has the input bytes, the corpus test has the file
/// name, and neither is worth threading in here.
pub fn vt_stream(config: Config, chunks: &[&[u8]]) -> Result<(), String> {
    let mut vt = Vt::new(config);
    vt.grid()
        .check_invariants()
        .map_err(|e| format!("before any input: {e}"))?;
    for (i, chunk) in chunks.iter().enumerate() {
        vt.feed(chunk);
        vt.grid()
            .check_invariants()
            .map_err(|e| format!("after chunk {i} ({} bytes): {e}", chunk.len()))?;
        // Replies are drained by the session in real use. Draining here keeps
        // a long fuzz run from being a memory test instead of a parser test.
        vt.take_reply();
    }
    Ok(())
}

/// Feed `chunks` and additionally require that no wide character is left
/// half-written.
///
/// Separate from [`vt_stream`] because it holds over a narrower set of streams:
/// Tera Term's rectangular copy and its screen restore both leave orphaned
/// halves, and both are reproduced. See
/// [`Grid::check_wide_pairs`](tt_grid::Grid::check_wide_pairs). Feed this
/// anything without a `$ v` or a `? 1047`-family switch in it and the pairing
/// is ours to keep.
pub fn vt_wide_pairs(config: Config, chunks: &[&[u8]]) -> Result<(), String> {
    let mut vt = Vt::new(config);
    for (i, chunk) in chunks.iter().enumerate() {
        vt.feed(chunk);
        vt.grid()
            .check_invariants()
            .map_err(|e| format!("after chunk {i}: {e}"))?;
        vt.grid()
            .check_wide_pairs()
            .map_err(|e| format!("after chunk {i}: {e}"))?;
        vt.take_reply();
    }
    Ok(())
}

/// The same bytes, split two different ways, must leave the same terminal.
///
/// `chunks` is compared against one call holding all of it. Replies are
/// accumulated rather than drained, because a reply emitted at the wrong point
/// in the stream is exactly the kind of thing a boundary bug causes.
pub fn vt_chunking(config: Config, chunks: &[&[u8]]) -> Result<(), String> {
    let whole: Vec<u8> = chunks.concat();

    let mut one = Vt::new(config.clone());
    one.feed(&whole);

    let mut split = Vt::new(config);
    for chunk in chunks {
        split.feed(chunk);
    }

    vt_diff(&one, &split).map_or(Ok(()), |d| {
        let sizes: Vec<usize> = chunks.iter().map(|c| c.len()).collect();
        Err(format!("chunked as {sizes:?}: {d}"))
    })
}

/// The first difference between two terminals, named. `None` if they agree.
///
/// Named rather than a bare bool because the useful output of a failing
/// chunking property is *which* piece of state drifted: a cell says the grid,
/// a reply says the parser answered twice or not at all, and a mode says a
/// `DECSET` was cut in half and lost.
pub fn vt_diff(a: &Vt, b: &Vt) -> Option<String> {
    let (ga, gb) = (a.grid(), b.grid());
    if (ga.cols(), ga.rows()) != (gb.cols(), gb.rows()) {
        return Some(format!(
            "size {}x{} vs {}x{}",
            ga.cols(),
            ga.rows(),
            gb.cols(),
            gb.rows()
        ));
    }
    if ga.cursor != gb.cursor {
        return Some(format!("cursor {:?} vs {:?}", ga.cursor, gb.cursor));
    }
    if ga.pen != gb.pen {
        return Some(format!("pen {:?} vs {:?}", ga.pen, gb.pen));
    }
    for y in 0..ga.rows() {
        for x in 0..ga.cols() {
            let (ca, cb) = (ga.line(y)[x], gb.line(y)[x]);
            if ca != cb {
                return Some(format!("cell {x},{y}: {ca:?} vs {cb:?}"));
            }
        }
    }
    if ga.scrolled_off() != gb.scrolled_off() {
        return Some(format!(
            "scrolled off {} vs {}",
            ga.scrolled_off(),
            gb.scrolled_off()
        ));
    }
    if ga.scrollback_len() != gb.scrollback_len() {
        return Some(format!(
            "scrollback {} vs {} lines",
            ga.scrollback_len(),
            gb.scrollback_len()
        ));
    }
    for i in 0..ga.scrollback_len() {
        if ga.scrollback_line(i) != gb.scrollback_line(i) {
            return Some(format!("scrollback line {i} differs"));
        }
    }
    if a.reply() != b.reply() {
        return Some(format!(
            "reply {:?} vs {:?}",
            String::from_utf8_lossy(a.reply()),
            String::from_utf8_lossy(b.reply())
        ));
    }
    if a.title() != b.title() {
        return Some(format!("title {:?} vs {:?}", a.title(), b.title()));
    }

    // The modes, which are what a cut-in-half `DECSET` loses. Listed rather
    // than derived: `Vt` has no way to compare itself and should not grow one
    // just for a test, and a mode missing from this list is a mode the
    // property is silently not checking.
    let modes = |v: &Vt| -> Vec<(&'static str, String)> {
        vec![
            ("key modes", format!("{:?}", v.key_modes())),
            ("mouse tracking", format!("{:?}", v.mouse_tracking())),
            ("mouse encoding", format!("{:?}", v.mouse_encoding())),
            ("focus reporting", format!("{}", v.focus_reporting())),
            ("DECCKM", format!("{}", v.application_cursor_keys())),
            ("DECNKM", format!("{}", v.application_keypad())),
            ("DECTCEM", format!("{}", v.cursor_visible())),
            ("bracketed paste", format!("{}", v.bracketed_paste())),
            ("DECSCNM", format!("{}", v.reverse_video())),
            ("KAM", format!("{}", v.keyboard_enabled())),
            ("SRM", format!("{}", v.local_echo())),
            ("LNM", format!("{}", v.newline_mode())),
            ("DECBKM", format!("{}", v.backspace_sends_bs())),
            ("wheel to cursor", format!("{}", v.wheel_to_cursor())),
        ]
    };
    for ((name, x), (_, y)) in modes(a).into_iter().zip(modes(b)) {
        if x != y {
            return Some(format!("{name}: {x} vs {y}"));
        }
    }
    None
}

/// Telnet's version of both properties at once: no panic, and the same
/// decoded stream however the reads fall.
///
/// `Telnet::feed` already loops a byte at a time, so chunk equivalence is
/// nearly free — but "nearly" is doing work, since `after_cr` and the
/// subnegotiation buffer both live across calls, and a `CR` at the end of one
/// read with its `NUL` at the start of the next is the ordinary case on a
/// telnet connection rather than an exotic one.
pub fn telnet_chunking(params: TelnetParams, chunks: &[&[u8]]) -> Result<(), String> {
    let whole: Vec<u8> = chunks.concat();

    let mut one = Telnet::new(params.clone());
    let (mut data_one, mut events_one) = (Vec::new(), Vec::new());
    one.feed(&whole, &mut data_one, &mut events_one);

    let mut split = Telnet::new(params);
    let (mut data_split, mut events_split) = (Vec::new(), Vec::new());
    for chunk in chunks {
        split.feed(chunk, &mut data_split, &mut events_split);
    }

    if data_one != data_split {
        return Err(format!(
            "decoded {} bytes whole, {} bytes chunked",
            data_one.len(),
            data_split.len()
        ));
    }
    if events_one != events_split {
        return Err(format!("events {events_one:?} vs {events_split:?}"));
    }
    if one.take_reply() != split.take_reply() {
        return Err("negotiation replies differ".to_string());
    }
    Ok(())
}

/// Split `bytes` into chunks of the sizes in `sizes`, cycling through them.
///
/// A fuzz target gets its chunking from the input; a test has to invent one,
/// and this is what it invents from. Zero sizes are skipped rather than
/// looping forever, and the tail is whatever is left.
pub fn chunk<'a>(bytes: &'a [u8], sizes: &[usize]) -> Vec<&'a [u8]> {
    let sizes: Vec<usize> = sizes.iter().copied().filter(|&n| n > 0).collect();
    if sizes.is_empty() {
        return vec![bytes];
    }
    let mut out = Vec::new();
    let mut rest = bytes;
    let mut i = 0;
    while !rest.is_empty() {
        let n = sizes[i % sizes.len()].min(rest.len());
        let (head, tail) = rest.split_at(n);
        out.push(head);
        rest = tail;
        i += 1;
    }
    out
}

/// The configuration the properties run under: the shipping defaults, with the
/// grid small enough that a fuzz iteration is cheap and wrapping, scrolling and
/// the margins are all reachable within a few bytes.
///
/// The scrollback is deliberately shallow for the same reason — a fuzzer that
/// has to emit ten thousand line feeds to reach the eviction path will never
/// reach it.
pub fn config() -> Config {
    Config {
        cols: 20,
        rows: 6,
        scrollback_max: 8,
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_covers_the_input_exactly() {
        let bytes = b"0123456789";
        for sizes in [&[1usize][..], &[3, 4][..], &[100][..], &[0, 2][..], &[][..]] {
            let chunks = chunk(bytes, sizes);
            assert_eq!(chunks.concat(), bytes, "sizes {sizes:?}");
            assert!(chunks.iter().all(|c| !c.is_empty()), "sizes {sizes:?}");
        }
    }

    /// The regression this file exists for: `rewrite_c1` holds a `0xC2` back
    /// across a call, so a two-byte C1 control cut in half must still be one
    /// control. `U+008D` on a VT100 is a carriage return.
    #[test]
    fn a_c1_control_split_across_chunks_is_still_one_control() {
        vt_chunking(config(), &[b"abc\xc2", b"\x8dX"]).unwrap();
    }

    /// `vte` 0.15.0 loses a byte when it resumes a two-byte sequence, so
    /// `tt-vt` holds partial sequences back rather than handing them over.
    /// This is that bug, minimised: `é` cut by the boundary, then exactly one
    /// ASCII byte, then another multi-byte lead. `advance_partial_utf8` prints
    /// the `é`, reports `valid_up_to()` — three — as consumed, and the `a`
    /// is gone.
    #[test]
    fn a_two_byte_sequence_resumed_does_not_eat_the_byte_after_it() {
        vt_chunking(config(), &[b"\xc3", b"\xa9a\xe4\xb8\x80"]).unwrap();
        vt_chunking(config(), &[b"\xcc", b"\x81a\xc3\xa9"]).unwrap();

        // And the shape it was found in, which is the same bug with a
        // combining mark in front of it.
        let bytes = b" \xcc\x81 \xe4\xb8\x80";
        vt_chunking(config(), &chunk(bytes, &[2, 3, 2])).unwrap();
    }

    /// And the other half of it: the `0x80` inside an em dash is a
    /// continuation byte, not a bare C1, whichever chunk it lands in.
    #[test]
    fn an_em_dash_split_anywhere_is_still_an_em_dash() {
        for n in 0..4 {
            let bytes = b"a\xe2\x80\x94b";
            vt_chunking(config(), &chunk(bytes, &[n.max(1)])).unwrap();
        }
    }

    #[test]
    fn an_escape_sequence_split_anywhere_still_runs() {
        let bytes = b"\x1b[31;1mred\x1b[?1049h\x1b[8;10;30t\x1b[?1049lend";
        for n in 1..bytes.len() {
            vt_chunking(config(), &chunk(bytes, &[n])).unwrap();
            vt_stream(config(), &chunk(bytes, &[n])).unwrap();
        }
    }

    #[test]
    fn telnet_negotiation_split_anywhere_decodes_the_same() {
        // IAC DO TERMTYPE, then data with an escaped IAC and a CR NUL.
        let bytes = b"\xff\xfd\x18hello\xff\xffworld\r\0done";
        for n in 1..bytes.len() {
            telnet_chunking(TelnetParams::default(), &chunk(bytes, &[n])).unwrap();
        }
    }
}
