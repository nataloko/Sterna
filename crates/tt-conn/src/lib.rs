//! tt-conn — the connection layer.
//!
//! Serial first, because it is the differentiator: `minicom` and `picocom`
//! have no GUI and no scripting, PuTTY has serial but neither scripting nor
//! file transfer, and the one tool that covers this ground is closed and paid.
//! SSH (`russh`), telnet and a local pty follow.
//!
//! The serial layer is built against the requirement in Tera Term's
//! `commlib.c` rather than against a generic idea of what a serial port does.
//! That is why MARK/SPACE parity, DSR flow control and break *detection* are
//! here at all — see `../README.md` and `PLAN.md`'s spike 4 result for what
//! that requirement turned out to be, and which parts of it Linux cannot
//! express.
//!
//! ```no_run
//! use tt_conn::serial::{enumerate, SerialConn, SerialParams};
//!
//! let ports = enumerate()?;
//! let port = ports.first().expect("no serial ports");
//! // open_path(), not device: /dev/ttyUSB<n> is assigned in attach order.
//! let mut conn = SerialConn::open(port.open_path(), &SerialParams::default())?;
//!
//! let (mut data, mut events) = (Vec::new(), Vec::new());
//! conn.read(&mut data, &mut events)?;
//! # Ok::<(), tt_conn::Error>(())
//! ```

pub mod error;
pub mod serial;

pub use error::{Error, Result};
