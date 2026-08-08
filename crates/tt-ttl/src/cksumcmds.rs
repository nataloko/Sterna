//! The checksums — `checksum8`, `checksum16`, `checksum32`, `crc16`, `crc32`
//! and the five `*file` forms beside them.
//!
//! Ten commands and five functions, all of them twenty lines of `ttl.cpp`
//! (`:661-730`) with no host in sight. Upstream's documentation prints the C
//! for `crc32` verbatim on the command's own page, which is as close to a
//! specification as this language gets, and the other four are the same shape.
//!
//! Two things they do not share with the rest of the language:
//!
//! - **The string forms write no `result` at all** and the file forms write 0
//!   or -1. So `if result = -1` after a `crc32` is reading whatever the line
//!   before it left there, which is the same trap `sendfile` has.
//! - **The answer is stored in a signed integer.** A `crc32` above 0x7FFFFFFF
//!   lands in the variable as a negative number, and the documentation's own
//!   example prints it with `sprintf '0x%08X'` — which reinterprets the bits
//!   and so hides it. A macro that compares one against zero does not.

use std::fs;

use crate::error::TtlResult;
use crate::expr;
use crate::interp::Interp;
use crate::rsv::Rsv;

/// Which of the five, and it is the only difference between the ten commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sum {
    /// A byte sum, kept to 8, 16 or 32 bits.
    Bytes(u32),
    /// The 16-bit CRC, reflected, seeded and finalised with 0xFFFF.
    /// Upstream's comment calls it CRC-16-CCITT and it is not; see
    /// [`checksum`].
    Crc16,
    /// CRC-32, the Ethernet FCS one.
    Crc32,
}

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn checksum_command(&mut self, w: Rsv) -> Option<TtlResult<()>> {
        let (kind, file) = match w {
            Rsv::Checksum8 => (Sum::Bytes(8), false),
            Rsv::Checksum8File => (Sum::Bytes(8), true),
            Rsv::Checksum16 => (Sum::Bytes(16), false),
            Rsv::Checksum16File => (Sum::Bytes(16), true),
            Rsv::Checksum32 => (Sum::Bytes(32), false),
            Rsv::Checksum32File => (Sum::Bytes(32), true),
            Rsv::Crc16 => (Sum::Crc16, false),
            Rsv::Crc16File => (Sum::Crc16, true),
            Rsv::Crc32 => (Sum::Crc32, false),
            Rsv::Crc32File => (Sum::Crc32, true),
            _ => return None,
        };
        Some(self.cmd_checksum(kind, file))
    }

    /// `<command> <intvar> <string>` and `<command>file <intvar> <filename>`.
    ///
    /// The empty-argument arm is upstream's `if (Str[0]==0) return Err;` and it
    /// returns before *everything*: the variable keeps whatever it had, and the
    /// file forms do not even write the -1 they write for a file they could not
    /// open.
    ///
    /// It is a C-string test, and so is the `strlen` that bounds the bytes
    /// checksummed, but neither can differ from emptiness here: a TTL string
    /// cannot contain a NUL — `#0` is rejected by the tokeniser and the
    /// variable store cuts at one — so there is nothing for the two to
    /// disagree about.
    fn cmd_checksum(&mut self, kind: Sum, file: bool) -> TtlResult<()> {
        let target = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        let arg = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        if arg.is_empty() {
            return Ok(());
        }

        let data = if file {
            let read = self.files.abs_path(&arg).and_then(|p| fs::read(&p).ok());
            // A file that is there and empty fails as loudly as one that is
            // not: `CreateFileMapping` of a zero-length file returns
            // `ERROR_FILE_INVALID`, so upstream's `goto error` runs and the
            // variable is left alone. Reproduced — a script testing `result`
            // is entitled to the same answer for the same file.
            match read {
                Some(d) if !d.is_empty() => d,
                _ => {
                    self.set_result(-1);
                    return Ok(());
                }
            }
        } else {
            arg
        };

        let sum = checksum(kind, &data);
        self.vars.set_int(target, sum as i32);
        if file {
            self.set_result(0);
        }
        Ok(())
    }
}

/// The five functions, transcribed from `ttl.cpp:661-730`.
///
/// Both CRCs are the reflected form: the polynomial is written back to front,
/// the register shifts right, and the seed and the final XOR are all-ones.
/// That makes `crc32` the Ethernet FCS and `crc16` CRC-16/X-25 — which is
/// *not* the CRC-16-CCITT the comment beside it claims, since the usual
/// CCITT-FALSE runs left and does not invert its output. The name is
/// upstream's; the arithmetic is what matters and it is reproduced.
fn checksum(kind: Sum, data: &[u8]) -> u32 {
    match kind {
        // `unsigned long` is 32 bits on Win32, so the sum wraps there before
        // the mask is applied. Anything narrower is the mask's business.
        Sum::Bytes(bits) => {
            let sum = data.iter().fold(0u32, |a, &b| a.wrapping_add(u32::from(b)));
            match bits {
                8 => sum & 0xFF,
                16 => sum & 0xFFFF,
                _ => sum,
            }
        }
        Sum::Crc16 => reflected_crc(data, 0xFFFF, 0x8408),
        Sum::Crc32 => reflected_crc(data, 0xFFFF_FFFF, 0xEDB8_8320),
    }
}

fn reflected_crc(data: &[u8], seed: u32, poly: u32) -> u32 {
    let mut r = seed;
    for &b in data {
        r ^= u32::from(b);
        for _ in 0..8 {
            r = if r & 1 == 1 { (r >> 1) ^ poly } else { r >> 1 };
        }
    }
    r ^ seed
}

#[cfg(test)]
mod tests {
    use super::{checksum, Sum};
    use crate::host::RecordingHost;
    use crate::interp::Interp;
    use crate::TtlError;

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    #[test]
    fn crc32_is_the_ethernet_one() {
        // The standard check vector: CRC-32 of "123456789" is 0xCBF43926.
        assert_eq!(checksum(Sum::Crc32, b"123456789"), 0xCBF4_3926);
        // And CRC-16/X-25 of the same, which is what the reflected 0x8408 with
        // an inverted output actually is — not the CCITT the comment names.
        assert_eq!(checksum(Sum::Crc16, b"123456789"), 0x906E);
        assert_eq!(checksum(Sum::Crc32, b""), 0, "the empty message");
    }

    #[test]
    fn the_three_sums_are_one_addition_under_three_masks() {
        let s = b"\xFF\xFF\x02";
        assert_eq!(checksum(Sum::Bytes(32), s), 0x200);
        assert_eq!(checksum(Sum::Bytes(16), s), 0x200);
        assert_eq!(checksum(Sum::Bytes(8), s), 0x00);
    }

    #[test]
    fn the_string_forms_write_the_variable_and_nothing_else() {
        assert_eq!(out("checksum8 v 'abc'\ndispstr v"), "38", "294 & 0xFF");
        assert_eq!(out("checksum16 v 'abc'\ndispstr v"), "294");
        assert_eq!(
            out("result = 7\ncrc16 v 'x'\ndispstr result"),
            "7",
            "no result is written, so the line before it still shows"
        );
    }

    #[test]
    fn a_crc_above_the_sign_bit_is_stored_negative() {
        // 0xCBF43926 as a signed 32-bit integer. The documentation's own
        // example prints it with `%08X`, which is why nobody notices.
        assert_eq!(out("crc32 v '123456789'\ndispstr v"), "-873187034");
    }

    #[test]
    fn an_empty_argument_returns_before_anything_is_written() {
        assert_eq!(
            out("v = 42\nresult = 7\nchecksum8 v ''\ndispstr v '|' result"),
            "42|7"
        );
        assert_eq!(
            out("v = 42\nresult = 7\ncrc32file v ''\ndispstr v '|' result"),
            "42|7",
            "not even the -1 a missing file would get"
        );
        // There is no third case: `#0` is a syntax error in the tokeniser, so
        // a TTL string cannot begin with the NUL upstream's test looks for.
        assert_eq!(run("checksum8 v #0'abc'").errors[0].0, TtlError::Syntax);
    }

    #[test]
    fn the_file_forms_report_minus_one_for_a_file_they_cannot_read() {
        let h = run("crc32file v '/nonexistent/nope'\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"-1");
    }

    #[test]
    fn a_file_is_checksummed_and_an_empty_one_reports_failure() {
        let dir = std::env::temp_dir().join("tt-ttl-cksum");
        let _ = std::fs::create_dir_all(&dir);
        let full = dir.join("full.bin");
        let empty = dir.join("empty.bin");
        std::fs::write(&full, b"123456789").unwrap();
        std::fs::write(&empty, b"").unwrap();

        let src = format!(
            "crc32file v '{}'\ndispstr result '|' v",
            full.to_string_lossy()
        );
        assert_eq!(out(&src), "0|-873187034");

        // Zero bytes is `ERROR_FILE_INVALID` from `CreateFileMapping`, so the
        // variable is untouched and `result` is -1.
        let src = format!(
            "v = 42\ncrc32file v '{}'\ndispstr result '|' v",
            empty.to_string_lossy()
        );
        assert_eq!(out(&src), "-1|42");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_argument_shapes_are_checked() {
        let h = run("crc32 'not a variable' 'x'");
        assert_eq!(h.errors[0].0, TtlError::Syntax);
        let h = run("crc32 v 'x' extra");
        assert_eq!(h.errors[0].0, TtlError::Syntax);
    }
}
