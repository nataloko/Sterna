//! One error type for every transport, and one place that decides what
//! "the device went away" looks like.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// The far end is gone: the adapter was unplugged, the socket closed, the
    /// pty's child exited. The frontend shows this differently from a
    /// transient failure, so it is a variant rather than a kind of `Io`.
    Disconnected,
    /// The port exists but something else holds it. By far the most common
    /// serial failure — `minicom` left running in another terminal, a
    /// ModemManager probe, a second window of our own — and the one where a
    /// wrong message wastes the most of the user's time.
    Busy {
        path: String,
    },
    /// The port exists but we are not allowed to open it. On Linux this
    /// usually means the user is not in `dialout`.
    PermissionDenied {
        path: String,
    },
    /// The port exists and cannot be opened for some other reason.
    Open {
        path: String,
        source: std::io::Error,
    },
    /// A setting this platform cannot express. Carries what was asked for, so
    /// the message can say so rather than "invalid argument".
    Unsupported(String),
    /// The SSH protocol failed: the socket, the banner, key exchange, opening
    /// the channel. Carries text because there is nothing structured a
    /// frontend can do with it beyond showing it.
    Ssh(String),
    /// The far end is not who the files say it is, or is who they say must be
    /// refused. Separate from [`Ssh`](Error::Ssh) because a frontend must
    /// **not** offer to retry: this is the one failure where the right
    /// affordance is no affordance.
    HostKey(String),
    /// The proxy in front of the host refused, failed, or is not a proxy.
    /// Separate from [`Ssh`](Error::Ssh) because the two send the user to
    /// different settings: this one is `[TTProxy]`, and saying "SSH failed"
    /// about a SOCKS server that answered `REP 2` sends them to the wrong
    /// dialog.
    Proxy(String),
    /// Every authentication method either failed or was not on offer.
    /// `offered` is what the server said it would still accept, which is the
    /// only thing that makes the message actionable — "the server wants
    /// publickey" beats "authentication failed".
    Auth {
        offered: Vec<String>,
    },
    Io(std::io::Error),
}

impl Error {
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Error::Disconnected)
    }

    /// Classify an `io::Error` from a read or write on an open port.
    ///
    /// `serialport-rs` maps an unplugged USB adapter to **`BrokenPipe` with
    /// `raw_os_error() == None`** rather than to the `EIO`/`ENXIO` the kernel
    /// actually returns — an undocumented crate detail, and the reason this
    /// lives in one function instead of at each call site. The raw errnos are
    /// checked too, because a port opened by any other route reports those.
    ///
    /// **Win32 says the same thing in numbers Rust has no `ErrorKind` for**, so
    /// that half needs [`windows_device_gone`] rather than `e.kind()`: an
    /// unplugged adapter answers `ERROR_BAD_COMMAND`, which arrives as
    /// `Uncategorized` and would fall through to [`Error::Io`]. What that looks
    /// like is a window that goes on believing it is connected until somebody
    /// types, and then says `os error 22` instead of reconnecting.
    pub fn from_io(e: std::io::Error) -> Error {
        use std::io::ErrorKind::*;
        #[cfg(unix)]
        if matches!(
            e.raw_os_error(),
            Some(libc::EIO) | Some(libc::ENXIO) | Some(libc::ENODEV)
        ) {
            return Error::Disconnected;
        }
        #[cfg(windows)]
        if windows_device_gone(e.raw_os_error()) {
            return Error::Disconnected;
        }
        match e.kind() {
            BrokenPipe | NotFound | ConnectionReset | ConnectionAborted => Error::Disconnected,
            _ => Error::Io(e),
        }
    }

    /// Work out why an open failed, given the path that was tried.
    ///
    /// **`serialport-rs` reports a busy port as `ErrorKind::NoDevice` with no
    /// errno** — the message reads "Device or resource busy" but the kind says
    /// the device is missing. Mapping that straight through tells someone with
    /// `minicom` open in another window that their adapter was unplugged, and
    /// sends them to check the cable. The same shape of wart as the
    /// `BrokenPipe`-on-disconnect mapping spike 4 found, so it is handled in
    /// the same one place.
    ///
    /// The discriminator is deliberately *not* the message text alone: whether
    /// the device node still exists separates "gone" from "there but
    /// unavailable" without depending on a string the crate is free to
    /// reword. The text only chooses between the two remaining reasons.
    #[cfg(unix)]
    pub fn from_open(path: &str, e: serialport::Error) -> Error {
        let io = std::io::Error::from(e);
        #[cfg(unix)]
        match io.raw_os_error() {
            Some(libc::EBUSY) => {
                return Error::Busy {
                    path: path.to_string(),
                }
            }
            Some(libc::EACCES) | Some(libc::EPERM) => {
                return Error::PermissionDenied {
                    path: path.to_string(),
                }
            }
            Some(libc::ENOENT) | Some(libc::ENODEV) | Some(libc::ENXIO) => {
                return Error::Disconnected
            }
            _ => {}
        }

        // No errno: the crate raised this itself. If the node is gone the
        // device really did leave.
        //
        // **Deliberately not `serial::present`**, which is a stricter test and
        // a different question. That one asks "is there a serial port node
        // here" so that a reopen loop can wait for one; this asks "is there
        // anything here to blame at all", and answering it with the strict
        // test would turn `cannot open /home/me/notaport: Is a directory` —
        // which says what to fix — into "the device disconnected", which does
        // not. The two agree on the case that matters to both: a path with
        // nothing at it.
        if !std::path::Path::new(path).exists() {
            return Error::Disconnected;
        }
        let text = io.to_string().to_ascii_lowercase();
        if text.contains("busy") {
            Error::Busy {
                path: path.to_string(),
            }
        } else if text.contains("permission") || text.contains("denied") {
            Error::PermissionDenied {
                path: path.to_string(),
            }
        } else {
            Error::Open {
                path: path.to_string(),
                source: io,
            }
        }
    }
}

/// The Win32 errors that mean **the device behind this handle has gone**.
///
/// Unix says it in three errnos and Rust gives two of them an `ErrorKind`;
/// Windows says it in seven numbers and gives none of them one, so every answer
/// here arrives as `ErrorKind::Uncategorized` and has to be recognised by value.
/// `ERROR_BAD_COMMAND` is the one an FTDI or CDC adapter gives after a surprise
/// removal, and the one that reached a user as `os error 22`.
///
/// The list is deliberately short: each entry is a driver's statement about the
/// *device*, never about the request, because the cost of a wrong entry is a
/// working session dropped. `ERROR_INVALID_HANDLE` is left out for that reason —
/// it is what a bug in this program looks like, and swallowing it as a
/// disconnect would hide one.
#[cfg(any(windows, test))]
mod device_gone {
    /// The device object is gone; the I/O manager fails the request rather than
    /// the driver refusing it.
    pub(super) const ACCESS_DENIED: i32 = 5;
    /// "The device does not recognize the command" — a stale COM handle.
    pub(super) const BAD_COMMAND: i32 = 22;
    /// "A device attached to the system is not functioning."
    pub(super) const GEN_FAILURE: i32 = 31;
    /// "The specified device is no longer available."
    pub(super) const DEV_NOT_EXIST: i32 = 55;
    pub(super) const NO_SUCH_DEVICE: i32 = 433;
    pub(super) const DEVICE_NOT_CONNECTED: i32 = 1167;
    pub(super) const DEVICE_REMOVED: i32 = 1617;

    /// The numbers are literals so that the list compiles — and is tested —
    /// where the tests run, which is not Windows. This is the diff that says
    /// they are the real ones, made by the compiler rather than by reading.
    #[cfg(windows)]
    const _: () = {
        use windows_sys::Win32::Foundation as win;
        assert!(ACCESS_DENIED as u32 == win::ERROR_ACCESS_DENIED);
        assert!(BAD_COMMAND as u32 == win::ERROR_BAD_COMMAND);
        assert!(GEN_FAILURE as u32 == win::ERROR_GEN_FAILURE);
        assert!(DEV_NOT_EXIST as u32 == win::ERROR_DEV_NOT_EXIST);
        assert!(NO_SUCH_DEVICE as u32 == win::ERROR_NO_SUCH_DEVICE);
        assert!(DEVICE_NOT_CONNECTED as u32 == win::ERROR_DEVICE_NOT_CONNECTED);
        assert!(DEVICE_REMOVED as u32 == win::ERROR_DEVICE_REMOVED);
    };
}

/// Whether a Win32 error code says the device has left. See [`device_gone`].
#[cfg(any(windows, test))]
pub(crate) fn windows_device_gone(code: Option<i32>) -> bool {
    use device_gone::*;
    matches!(
        code,
        Some(
            ACCESS_DENIED
                | BAD_COMMAND
                | GEN_FAILURE
                | DEV_NOT_EXIST
                | NO_SUCH_DEVICE
                | DEVICE_NOT_CONNECTED
                | DEVICE_REMOVED
        )
    )
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Disconnected => write!(f, "the device disconnected"),
            Error::Busy { path } => write!(f, "{path} is in use by another program"),
            Error::PermissionDenied { path } => {
                write!(
                    f,
                    "no permission to open {path} (is the user in `dialout`?)"
                )
            }
            Error::Open { path, source } => write!(f, "cannot open {path}: {source}"),
            Error::Unsupported(what) => write!(f, "not supported on this platform: {what}"),
            Error::Ssh(what) => write!(f, "{what}"),
            Error::HostKey(what) => write!(f, "{what}"),
            Error::Proxy(what) => write!(f, "{what}"),
            Error::Auth { offered } if offered.is_empty() => {
                write!(
                    f,
                    "authentication failed and the server offered no other method"
                )
            }
            Error::Auth { offered } => {
                write!(
                    f,
                    "authentication failed; the server accepts {}",
                    offered.join(", ")
                )
            }
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Open { source, .. } => Some(source),
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::from_io(e)
    }
}

impl From<serialport::Error> for Error {
    fn from(e: serialport::Error) -> Error {
        match e.kind() {
            serialport::ErrorKind::NoDevice => Error::Disconnected,
            _ => Error::from_io(std::io::Error::from(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows half of [`Error::from_io`], asserted on the list rather than
    /// through the function: the arm that consults it is `#[cfg(windows)]` and
    /// is not compiled where these tests run.
    #[test]
    fn a_removed_windows_device_is_a_disconnection() {
        // The one a user reported. Everything else here shares its shape: the
        // handle is still open and the device behind it is not.
        assert!(windows_device_gone(Some(22)), "ERROR_BAD_COMMAND");
        assert!(windows_device_gone(Some(5)), "ERROR_ACCESS_DENIED");
        assert!(windows_device_gone(Some(31)), "ERROR_GEN_FAILURE");
        assert!(windows_device_gone(Some(55)), "ERROR_DEV_NOT_EXIST");
        assert!(windows_device_gone(Some(433)), "ERROR_NO_SUCH_DEVICE");
        assert!(
            windows_device_gone(Some(1167)),
            "ERROR_DEVICE_NOT_CONNECTED"
        );
        assert!(windows_device_gone(Some(1617)), "ERROR_DEVICE_REMOVED");
    }

    /// The other half, and the one that costs a live session if it is wrong.
    #[test]
    fn an_ordinary_windows_failure_is_not_a_disconnection() {
        assert!(!windows_device_gone(None), "no code at all");
        assert!(
            !windows_device_gone(Some(6)),
            "ERROR_INVALID_HANDLE is a bug in this program, not an unplugged cable"
        );
        assert!(
            !windows_device_gone(Some(995)),
            "ERROR_OPERATION_ABORTED is a cancelled request, which `finish` expects"
        );
        assert!(
            !windows_device_gone(Some(121)),
            "ERROR_SEM_TIMEOUT is a quiet line"
        );
        assert!(
            !windows_device_gone(Some(87)),
            "ERROR_INVALID_PARAMETER is a setting this port cannot take"
        );
    }
}
