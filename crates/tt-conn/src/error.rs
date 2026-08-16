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
    pub fn from_io(e: std::io::Error) -> Error {
        use std::io::ErrorKind::*;
        #[cfg(unix)]
        if matches!(
            e.raw_os_error(),
            Some(libc::EIO) | Some(libc::ENXIO) | Some(libc::ENODEV)
        ) {
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
        // device really did leave. Asked through `serial::present` so that
        // this and the auto-reopen loop cannot disagree about what "the node
        // is there" means — and it opens nothing, which is what lets the
        // reopen loop ask it on a timer.
        if !crate::serial::present(path) {
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
