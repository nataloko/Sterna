//! The same bytes split two ways must leave the same terminal.
//!
//! The input is a list of chunks, which is what a read loop actually hands the
//! engine — the kernel decides where the boundaries fall, and it does not care
//! that a UTF-8 sequence or a `DECSET` was halfway through. `tt-vt` carries
//! `pending_c2` and `utf8_left` across the call for that reason, and `vte`
//! carries its parser state; both are invisible to any test that feeds a whole
//! file at once.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|chunks: Vec<Vec<u8>>| {
    let borrowed: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    if let Err(e) = tt_fuzz::vt_stream(tt_fuzz::config(), &borrowed) {
        panic!("{e}");
    }
    if let Err(e) = tt_fuzz::vt_chunking(tt_fuzz::config(), &borrowed) {
        panic!("{e}");
    }
});
