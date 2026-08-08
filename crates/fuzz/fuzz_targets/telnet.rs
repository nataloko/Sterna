//! The telnet decoder, over chunked input.
//!
//! Everything it reads comes from the far end, including the subnegotiation
//! payloads it parses for `NAWS` and `TERMINAL-TYPE`, so the whole surface is
//! remote input. Chunked because `after_cr` and the subnegotiation buffer both
//! live across reads, and a `CR` ending one read with its `NUL` starting the
//! next is the ordinary case on a telnet connection.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tt_conn::telnet::protocol::TelnetParams;

fuzz_target!(|chunks: Vec<Vec<u8>>| {
    let borrowed: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    // Negotiate rather than the default Auto: it is the mode that answers, so
    // it reaches the option state machine on the first byte instead of after
    // the first `IAC`.
    let params = TelnetParams {
        mode: tt_conn::telnet::TelnetMode::Negotiate,
        ..TelnetParams::default()
    };
    if let Err(e) = tt_fuzz::telnet_chunking(params, &borrowed) {
        panic!("{e}");
    }
});
