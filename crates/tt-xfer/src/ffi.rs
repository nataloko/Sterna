//! The declarations for `csrc/tt_xfer.h`.
//!
//! Hand-written rather than bindgen-generated: the surface is twenty
//! functions and one struct, it changes when we change it, and a generated
//! binding would be a second build-time dependency to keep a header in step
//! with. `tt-ffi` has the opposite problem and the opposite answer — there the
//! header is the product, so it is generated and committed.

use std::os::raw::{c_char, c_double, c_int, c_uint};

#[repr(C)]
pub struct TtXfer {
    _opaque: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TtXferOpts {
    pub port_type: c_int,
    pub baud: c_int,
    pub data_bit_7: c_int,

    pub xmodem_timeout_init: c_int,
    pub xmodem_timeout_init_crc: c_int,
    pub xmodem_timeout_short: c_int,
    pub xmodem_timeout_long: c_int,
    pub xmodem_timeout_vlong: c_int,
    pub ymodem_timeout_init: c_int,
    pub ymodem_timeout_init_crc: c_int,
    pub ymodem_timeout_short: c_int,
    pub ymodem_timeout_long: c_int,
    pub ymodem_timeout_vlong: c_int,
    pub zmodem_timeout_normal: c_int,
    pub zmodem_timeout_tcpip: c_int,
    pub zmodem_timeout_init: c_int,
    pub zmodem_timeout_fin: c_int,
    pub zmodem_data_len: c_int,
    pub zmodem_win_size: c_int,
    pub qv_win_size: c_int,

    pub ft_flag: c_int,
    pub kermit_opt: c_int,
    pub log_flag: c_int,
    pub log_dir: *const c_char,

    pub mode: c_int,
    pub opt: c_int,
    pub text_flag: c_int,
    pub autostop_sec: c_int,

    pub overwrite: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TtXferProgress {
    pub bytes: i64,
    pub packets: i64,
    pub done: i64,
    pub total: i64,
    pub percent: i32,
    pub elapsed_ms: u32,
}

pub const STATE_DONE: c_uint = 1;
pub const STATE_SUCCESS: c_uint = 2;
pub const STATE_ENDED: c_uint = 4;
pub const STATE_CANCELLED: c_uint = 8;

extern "C" {
    pub fn tt_xfer_create(proto: c_int, dir: c_int, opts: *const TtXferOpts) -> *mut TtXfer;
    pub fn tt_xfer_destroy(x: *mut TtXfer);
    pub fn tt_xfer_add_send_file(x: *mut TtXfer, path: *const c_char) -> c_int;
    pub fn tt_xfer_set_recv_dir(x: *mut TtXfer, dir: *const c_char) -> c_int;
    pub fn tt_xfer_init(x: *mut TtXfer) -> c_int;
    pub fn tt_xfer_parse(x: *mut TtXfer) -> c_int;
    pub fn tt_xfer_timeout(x: *mut TtXfer);
    pub fn tt_xfer_cancel(x: *mut TtXfer);
    pub fn tt_xfer_state(x: *const TtXfer) -> c_uint;
    pub fn tt_xfer_timeout_remaining(x: *const TtXfer) -> c_double;
    pub fn tt_xfer_push_rx(x: *mut TtXfer, data: *const u8, len: usize) -> usize;
    pub fn tt_xfer_rx_pending(x: *const TtXfer) -> usize;
    pub fn tt_xfer_take_tx(x: *mut TtXfer, out: *mut u8, cap: usize) -> usize;
    pub fn tt_xfer_tx_pending(x: *const TtXfer) -> usize;
    pub fn tt_xfer_set_ready(x: *mut TtXfer, ready: c_int);
    pub fn tt_xfer_progress(x: *const TtXfer) -> *const TtXferProgress;
    pub fn tt_xfer_proto_name(x: *const TtXfer) -> *const c_char;
    pub fn tt_xfer_file_name(x: *const TtXfer) -> *const c_char;
    pub fn tt_xfer_message(x: *const TtXfer) -> *const c_char;
}
