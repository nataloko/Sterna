//! Arbitrary bytes into the terminal: does anything panic, and does the grid
//! stay self-consistent?
//!
//! Flat bytes rather than a structured input, because libFuzzer's mutators are
//! built for exactly this shape — and so is the thing being modelled. A serial
//! line at the wrong baud rate, a device that reboots mid-escape-sequence and a
//! hostile SSH server all deliver the same thing: a byte string nobody chose.
//!
//! The stream is fed in one call. `vt_chunks` is the target that moves the
//! boundaries around.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(e) = tt_fuzz::vt_stream(tt_fuzz::config(), &[data]) {
        panic!("{e}");
    }
});
