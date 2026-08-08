//! The raw-fd patch layer — the four things `commlib.c` needs that
//! `serialport-rs` does not expose.
//!
//! Spike 4's decisive finding is what makes this a *layer* rather than a fork:
//! `serialport-rs` reads termios, changes the fields it owns, and writes it
//! back, so foreign flags survive `set_baud_rate`, `set_parity`,
//! `set_flow_control`, `set_timeout` and `write_request_to_send`. Had it
//! rewritten termios wholesale, every crate call would silently have undone
//! these and adoption would have meant maintaining a fork.
//!
//! Because the escape hatch is `AsRawFd`, it exists only on the concrete
//! `TTYPort`: `Box<dyn SerialPort>` does not implement it. That is why the
//! serial layer is platform-split at the type level rather than hiding behind
//! the portable trait.

use std::os::unix::io::RawFd;

use crate::error::{Error, Result};

/// Linux's fifth parity bit. Absent from the `libc` crate's exports, and the
/// only way to reach MARK and SPACE parity — which `commlib.c:194-200` sets
/// and which a good deal of industrial kit still speaks.
const CMSPAR: libc::tcflag_t = 0o10000000000;

pub fn get(fd: RawFd) -> Result<libc::termios> {
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) != 0 {
            return Err(Error::from_io(std::io::Error::last_os_error()));
        }
        Ok(t)
    }
}

pub fn set(fd: RawFd, t: &libc::termios) -> Result<()> {
    // TCSANOW, not TCSADRAIN: a settings change must not block behind output
    // the far end is not draining, which is exactly the state a user is in
    // when they reach for the settings dialog.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, t) } != 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    Ok(())
}

/// MARK and SPACE parity, which `serialport-rs` has no enum for.
///
/// `CMSPAR` with `PARODD` is MARK (the parity bit is always 1); without it,
/// SPACE (always 0). Clearing `CMSPAR` hands parity back to the crate.
pub fn set_stick_parity(fd: RawFd, mark: Option<bool>) -> Result<()> {
    let mut t = get(fd)?;
    match mark {
        None => t.c_cflag &= !CMSPAR,
        Some(mark) => {
            t.c_cflag |= libc::PARENB | CMSPAR;
            if mark {
                t.c_cflag |= libc::PARODD;
            } else {
                t.c_cflag &= !libc::PARODD;
            }
        }
    }
    set(fd, &t)
}

/// Escape the input stream so a BREAK is distinguishable from a `0x00`.
///
/// `IGNBRK` and `BRKINT` must both be off or the kernel swallows the break or
/// turns it into `SIGINT`; `INPCK` must be on or errors are never marked in the
/// first place. See [`super::parmrk`] for what the escaped stream looks like.
pub fn set_parmrk(fd: RawFd, on: bool) -> Result<()> {
    let mut t = get(fd)?;
    if on {
        t.c_iflag |= libc::PARMRK | libc::INPCK;
        t.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::IGNPAR);
    } else {
        t.c_iflag &= !libc::PARMRK;
    }
    set(fd, &t)
}

/// The XON and XOFF *characters* — `ts.XonChar`/`ts.XoffChar`.
///
/// The matching *thresholds* (`XonLim` 768, `XoffLim` 3328 in `commlib.c:107`)
/// have no Linux equivalent: the kernel owns its buffer watermarks. That is a
/// real behavioural difference from Windows, not an omission here.
pub fn set_xon_xoff_chars(fd: RawFd, xon: u8, xoff: u8) -> Result<()> {
    let mut t = get(fd)?;
    t.c_cc[libc::VSTART] = xon;
    t.c_cc[libc::VSTOP] = xoff;
    set(fd, &t)
}

/// Set the character size, and **check the driver actually did it**.
///
/// `commlib.c`'s DCB path offers only 7 and 8; 5 and 6 are here because other
/// adapters do support them and a serial terminal meets teletype gear that
/// Tera Term's dialog never had to.
///
/// The read-back is not defensive programming, it is the whole point.
/// Measured on an FTDI Quad RS232-HS: `CS6` is refused outright with `EINVAL`,
/// which is fine — but **`CS5` is accepted by `tcsetattr` and then ignored**,
/// and the adapter keeps transmitting eight bits. Without this check the
/// settings dialog would report 5 data bits while the wire carried 8, and the
/// resulting corruption would look like a cabling fault. `tcsetattr` succeeds
/// if it could apply *any* of what it was asked, so its return value alone
/// says nothing.
pub fn set_data_bits(fd: RawFd, bits: u8) -> Result<()> {
    let mask = match bits {
        5 => libc::CS5,
        6 => libc::CS6,
        7 => libc::CS7,
        8 => libc::CS8,
        n => return Err(Error::Unsupported(format!("{n} data bits"))),
    };
    let mut t = get(fd)?;
    t.c_cflag = (t.c_cflag & !libc::CSIZE) | mask;
    set(fd, &t)?;

    if get(fd)?.c_cflag & libc::CSIZE != mask {
        return Err(Error::Unsupported(format!(
            "{bits} data bits (the driver accepted the request and ignored it)"
        )));
    }
    Ok(())
}

/// Bytes still queued for transmission — `TIOCOUTQ`.
///
/// The bounded alternative to `tcdrain`, which has no timeout and never
/// returns while flow control is holding the line. See
/// [`super::SerialConn::flush`].
pub fn output_queue_len(fd: RawFd) -> Result<usize> {
    let mut pending: libc::c_int = 0;
    if unsafe { libc::ioctl(fd, libc::TIOCOUTQ, &mut pending) } != 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    Ok(pending.max(0) as usize)
}

/// Read the modem status lines in one `ioctl`, rather than four crate calls
/// that each do their own.
pub fn modem_bits(fd: RawFd) -> Result<i32> {
    let mut bits: libc::c_int = 0;
    if unsafe { libc::ioctl(fd, libc::TIOCMGET, &mut bits) } != 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    Ok(bits)
}
