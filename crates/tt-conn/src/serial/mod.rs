//! The serial transport — the differentiator, and the first one built.
//!
//! Written against the requirement in Tera Term's
//! `teraterm/teraterm/commlib.c` (the DCB fields it sets, the
//! `EscapeCommFunction` calls it makes) rather than against a generic
//! wishlist. That is why MARK/SPACE parity and DSR flow control are here at
//! all: nothing else on Linux offers them, and they are why people still keep
//! a Windows VM for console work.

pub mod parmrk;
pub use parmrk::SerialEvent;

#[cfg(unix)]
mod linux;
#[cfg(windows)]
mod windows;

mod enumerate;
pub use enumerate::{enumerate, number_of_port, port_by_number, PortInfo, UsbInfo};

mod inuse;
pub use inuse::{holders, holders_under, Holder};

// The port's own `Read`/`Write` are the Unix data path. Windows drives
// overlapped `ReadFile`/`WriteFile` on the handle instead, because the crate's
// impls pass a null `OVERLAPPED`.
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::io::RawHandle;
use std::time::{Duration, Instant};

use serialport::SerialPort;

use crate::error::{Error, Result};

/// The platform's concrete port type.
///
/// **Concrete, not `Box<dyn SerialPort>`, and that is the design.** The raw-fd
/// patch layer needs `AsRawFd`, which the trait object does not implement, so
/// the split exists whether or not the API admits it — spike 4's conclusion
/// was to make it explicit and thin rather than to pretend the portable trait
/// suffices. Hiding it would mean discovering it at the point where MARK
/// parity has to work.
#[cfg(unix)]
type NativePort = serialport::TTYPort;
#[cfg(windows)]
type NativePort = serialport::COMPort;

/// `ts.DataBit`, widened. Tera Term's dialog offers 7 and 8; the hardware
/// does 5 and 6 as well and old teletype gear needs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    Eight,
}

/// `ts.Parity` — `commlib.c:182`, all five values including the two Linux
/// needs `CMSPAR` for.
///
/// `repr(u8)` here and on the two enums below: the C ABI names these
/// variants directly rather than keeping a second copy. See `tt-ffi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
    /// The parity bit is always 1.
    Mark,
    /// The parity bit is always 0.
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StopBits {
    #[default]
    One,
    Two,
}

/// `ts.Flow` — `commlib.c:204`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FlowControl {
    #[default]
    None,
    /// `IdFlowX`. The characters are configurable; the thresholds are not,
    /// on Linux.
    XonXoff,
    /// `IdFlowHard`, `fOutxCtsFlow`.
    RtsCts,
    /// `IdFlowHardDsrDtr`, `fOutxDsrFlow`. **Linux has no such termios bit**,
    /// so writes are gated in userspace — see [`SerialConn::write`].
    DsrDtr,
}

/// `dcb.fDtrControl` / `dcb.fRtsControl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PinControl {
    /// `*_CONTROL_DISABLE` — hold the line low.
    Disable,
    /// `*_CONTROL_ENABLE` — hold it high. Most devices expect this.
    #[default]
    Enable,
    /// `*_CONTROL_HANDSHAKE` — the driver raises and lowers it as its buffer
    /// fills. On Linux only RTS can do this, and only as part of `CRTSCTS`.
    Handshake,
    /// `RTS_CONTROL_TOGGLE` — RTS up while transmitting and down otherwise,
    /// which is half-duplex RS-485 keying. **RTS only**: Win32 has no
    /// `DTR_CONTROL_TOGGLE` and upstream's DTR list has three entries to RTS's
    /// four (`serial_pp.cpp:74`).
    ///
    /// Linux does this through `TIOCSRS485` rather than through termios, and
    /// whether it exists at all is the driver's answer, not the kernel's — the
    /// FTDI Quad RS232-HS on the rig here answers `ENOTTY` to even the *get*,
    /// so there is nothing to test an implementation against. So the line is
    /// left where the kernel put it on open rather than driven by hand, which
    /// on a port with no RS-485 support is the same place `Enable` leaves it.
    /// The mapping is written down here for whoever has an 8250 to try it on.
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialParams {
    pub baud: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow: FlowControl,
    /// `dcb.XonChar` / `dcb.XoffChar`. Tera Term hardcodes the standard pair
    /// in `CommResetSerial`, but `TTTSet` carries them, so they are settable.
    pub xon: u8,
    pub xoff: u8,
    pub dtr: PinControl,
    pub rts: PinControl,
    /// Escape the input stream so a line break arrives as a
    /// [`SerialEvent::Break`] instead of only a `0x00`. Unix pays one
    /// `FF`-doubling pass over the input for `PARMRK`; Windows receives the
    /// line event separately from the bytes through `WaitCommEvent`.
    pub detect_break: bool,
    /// How long a read waits before returning empty. Short enough that a
    /// disconnect is noticed promptly, long enough not to spin.
    pub read_timeout: Duration,
}

impl Default for SerialParams {
    /// Tera Term's own defaults where it has them: 8N1, no flow control, DTR
    /// and RTS asserted, and the standard XON/XOFF pair.
    ///
    /// **The speed is 115200 where upstream's is 9600** — deliberate, and the
    /// same value the settings schema defaults `BaudRate` to; see
    /// `docs/deviations.md`. Nothing else here deviates.
    fn default() -> Self {
        SerialParams {
            baud: 115_200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow: FlowControl::None,
            xon: 0x11,
            xoff: 0x13,
            dtr: PinControl::Enable,
            rts: PinControl::Enable,
            detect_break: true,
            read_timeout: Duration::from_millis(50),
        }
    }
}

/// The modem status lines, as one snapshot. `GetCommModemStatus` returns all
/// four at once and so does `TIOCMGET`; reading them one at a time can catch
/// a device mid-transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModemLines {
    pub cts: bool,
    pub dsr: bool,
    /// Ring indicator.
    pub ri: bool,
    /// Carrier detect.
    pub cd: bool,
}

/// An open serial port.
pub struct SerialConn {
    port: NativePort,
    #[cfg(windows)]
    wake: Option<windows::WindowsSerialWake>,
    /// The completion events for the data path. Windows only, because only
    /// there is the port opened overlapped — see [`windows`] for why it has
    /// to be.
    #[cfg(windows)]
    io: windows::SerialIo,
    params: SerialParams,
    #[cfg(unix)]
    decoder: parmrk::Parmrk,
    path: String,
    /// Set once a read or write has reported the device gone, so later calls
    /// fail the same way instead of returning a confusing second error.
    dead: bool,
}

impl SerialConn {
    /// Open `path` and apply `params`.
    ///
    /// `path` may be a `/dev/serial/by-path/…` symlink, and for anything on
    /// USB it should be: `ttyUSB<n>` is assigned in attach order, so
    /// reconnecting by that name can land on a different physical port.
    pub fn open(path: &str, params: &SerialParams) -> Result<Self> {
        #[cfg(unix)]
        let port = {
            let builder = serialport::new(path, params.baud).timeout(params.read_timeout);
            NativePort::open(&builder).map_err(|e| Error::from_open(path, e))?
        };
        #[cfg(windows)]
        let port = windows::open(path)?;

        let mut conn = SerialConn {
            port,
            #[cfg(windows)]
            wake: None,
            #[cfg(windows)]
            io: windows::SerialIo::new()?,
            params: *params,
            #[cfg(unix)]
            decoder: parmrk::Parmrk::new(),
            path: path.to_string(),
            dead: false,
        };
        conn.apply(params)?;
        #[cfg(windows)]
        {
            conn.wake = Some(windows::WindowsSerialWake::start(&conn.port)?);
        }
        Ok(conn)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn params(&self) -> &SerialParams {
        &self.params
    }

    /// Apply a full parameter set to an already-open port — the settings
    /// dialog's "OK", and `CommResetSerial`'s job.
    ///
    /// On Unix, order matters: the crate's setters run before the raw-fd
    /// patches because they would otherwise clear `CMSPAR` on the way past.
    /// Windows instead builds one complete DCB and reads it back, both because
    /// the portable setters cannot express half its fields and so a rejected
    /// field cannot leave the preceding setters applied piecemeal.
    pub fn apply(&mut self, params: &SerialParams) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            self.port.set_baud_rate(params.baud)?;
            self.port.set_stop_bits(match params.stop_bits {
                StopBits::One => serialport::StopBits::One,
                StopBits::Two => serialport::StopBits::Two,
            })?;
            self.port.set_parity(match params.parity {
                Parity::Odd | Parity::Mark => serialport::Parity::Odd,
                Parity::Even | Parity::Space => serialport::Parity::Even,
                // MARK and SPACE are set through CMSPAR below; asking the
                // crate for Odd/Even first gets PARENB on and the bit in the
                // right place, and CMSPAR then overrides what it means.
                Parity::None => serialport::Parity::None,
            })?;
            self.port.set_flow_control(match params.flow {
                FlowControl::RtsCts => serialport::FlowControl::Hardware,
                FlowControl::XonXoff => serialport::FlowControl::Software,
                // DSR/DTR gets no kernel help; `write` gates it in userspace.
                FlowControl::None | FlowControl::DsrDtr => serialport::FlowControl::None,
            })?;
            self.port.set_timeout(params.read_timeout)?;
            let fd = self.port.as_raw_fd();
            linux::set_data_bits(
                fd,
                match params.data_bits {
                    DataBits::Five => 5,
                    DataBits::Six => 6,
                    DataBits::Seven => 7,
                    DataBits::Eight => 8,
                },
            )?;
            linux::set_stick_parity(
                fd,
                match params.parity {
                    Parity::Mark => Some(true),
                    Parity::Space => Some(false),
                    _ => None,
                },
            )?;
            linux::set_parmrk(fd, params.detect_break)?;
            linux::set_xon_xoff_chars(fd, params.xon, params.xoff)?;
            // RTS is the driver's while CRTSCTS is on, so only drive it by
            // hand when it is ours. `Toggle` is the driver too, where one has
            // it at all.
            if !matches!(params.rts, PinControl::Handshake | PinControl::Toggle)
                && params.flow != FlowControl::RtsCts
            {
                self.port
                    .write_request_to_send(params.rts == PinControl::Enable)?;
            }
            self.port
                .write_data_terminal_ready(params.dtr == PinControl::Enable)?;
        }
        #[cfg(windows)]
        {
            windows::apply(&self.port, params)?;
            self.port.set_timeout(params.read_timeout)?;
        }

        self.params = *params;
        Ok(())
    }

    /// Read whatever is available, appending decoded bytes to `data` and
    /// out-of-band events to `events`.
    ///
    /// Returns the number of **data** bytes appended. A timeout is not an
    /// error — it is the normal way a quiet line reports itself — so it comes
    /// back as `Ok(0)`.
    pub fn read(&mut self, data: &mut Vec<u8>, events: &mut Vec<SerialEvent>) -> Result<usize> {
        if self.dead {
            return Err(Error::Disconnected);
        }
        #[cfg(windows)]
        {
            let Some(notice) = self
                .wake
                .as_mut()
                .expect("an open Windows serial port has a wakeup")
                .take()?
            else {
                return Ok(0);
            };
            if notice.broken && self.params.detect_break {
                events.push(SerialEvent::Break);
            }
            if !notice.receive {
                if notice.worker {
                    self.wake
                        .as_ref()
                        .expect("the wakeup is still present")
                        .acknowledge(false)?;
                }
                return Ok(0);
            }
        }

        #[cfg(unix)]
        let mut buf = [0u8; 4096];
        // Tera Term's own input buffer is 64 KiB. Draining that much avoids a
        // `ClearCommError`/COMSTAT peek here: that tempting queue-length check
        // would clear a break which arrived while the worker was waiting for
        // this read to finish.
        #[cfg(windows)]
        let mut buf = [0u8; 64 * 1024];
        #[cfg(unix)]
        let n = match self.port.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Ok(0),
            Err(e) => {
                let e = Error::from_io(e);
                self.dead = e.is_disconnected();
                return Err(e);
            }
        };
        // A zero-length read on a character device is EOF, which for a serial
        // port means the far end is gone. **Windows is the opposite** — there
        // a completed empty read is the driver's read timeout and a COM handle
        // has no EOF at all, so the two cases cannot share this branch.
        #[cfg(unix)]
        if n == 0 {
            self.dead = true;
            return Err(Error::Disconnected);
        }

        #[cfg(windows)]
        let n = match windows::read(&self.port, &self.io, &mut buf, self.params.read_timeout) {
            Ok(0) => {
                self.acknowledge_windows_read(false)?;
                return Ok(0);
            }
            Ok(n) => n,
            Err(e) => {
                let _ = self.acknowledge_windows_read(false);
                self.dead = e.is_disconnected();
                return Err(e);
            }
        };

        let before = data.len();
        #[cfg(unix)]
        if self.params.detect_break {
            self.decoder.feed(&buf[..n], data, events);
        } else {
            data.extend_from_slice(&buf[..n]);
        }
        // Win32 reports line errors through WaitCommEvent/ClearCommError; its
        // bytes are not Linux PARMRK escapes. Feeding an ordinary 0xFF into
        // that decoder would hold it forever while waiting for two more bytes.
        #[cfg(windows)]
        data.extend_from_slice(&buf[..n]);

        #[cfg(windows)]
        {
            // A full read may have stopped at our buffer rather than at the
            // driver's queue. Ask the worker for one immediate follow-up; a
            // rare exactly-64-KiB burst then pays one ordinary read timeout,
            // while a larger burst cannot be stranded without a new event.
            let more = n == buf.len();
            self.acknowledge_windows_read(more)?;
        }
        Ok(data.len() - before)
    }

    #[cfg(windows)]
    fn acknowledge_windows_read(&self, more: bool) -> Result<()> {
        self.wake
            .as_ref()
            .expect("an open Windows serial port has a wakeup")
            .acknowledge(more)
    }

    /// Write `data` for at most `timeout`, honouring DSR flow control if it is
    /// on.
    ///
    /// Linux has `CRTSCTS` and `IXON`/`IXOFF` and **no DSR flow-control bit at
    /// all** — `commlib.c:219`'s `fOutxDsrFlow` has no kernel equivalent, and
    /// that is a kernel limitation rather than a missing crate feature. So
    /// when the mode is on, the write is gated in userspace: poll DSR, send
    /// only while it is asserted, and give up after `timeout` rather than
    /// blocking a UI thread forever. Win32 needs no gate — `fOutxDsrFlow` is
    /// the driver's there — and spends `timeout` on the overlapped wait in
    /// [`windows::write`] instead. That wait is the *outer* bound: the driver
    /// has a write deadline of its own, because `serialport`'s `set_timeout`
    /// writes the port's read timeout into `WriteTotalTimeoutConstant` as well,
    /// so a stalled write usually ends there first and comes back as a short
    /// count for the caller to retry.
    pub fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        if self.dead {
            return Err(Error::Disconnected);
        }
        #[cfg(windows)]
        {
            match windows::write(&self.port, &self.io, data, timeout) {
                Ok(n) => Ok(n),
                Err(e) => {
                    self.dead = e.is_disconnected();
                    Err(e)
                }
            }
        }

        #[cfg(unix)]
        {
            if self.params.flow != FlowControl::DsrDtr {
                return self.write_raw(data);
            }

            let deadline = Instant::now() + timeout;
            let mut sent = 0;
            while sent < data.len() {
                if self.modem_lines()?.dsr {
                    // One chunk at a time, so DSR is re-checked often enough to
                    // stop within a buffer of the far end deasserting it.
                    let end = (sent + 64).min(data.len());
                    sent += self.write_raw(&data[sent..end])?;
                    continue;
                }
                if Instant::now() >= deadline {
                    break;
                }
                // 2 ms is well under a character time at any baud rate a DSR-flow
                // device runs at, and cheap enough to poll.
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(sent)
        }
    }

    /// The ungated write, for a caller which has already decided it may send.
    ///
    /// Unix only: Windows has no ungated form, because there every write needs
    /// the deadline that bounds its overlapped wait. What reaches this on Unix
    /// is the DSR gate's own chunking, which must not recurse through it.
    #[cfg(unix)]
    fn write_raw(&mut self, data: &[u8]) -> Result<usize> {
        match self.port.write(data) {
            Ok(n) => Ok(n),
            Err(e) => {
                let e = Error::from_io(e);
                self.dead = e.is_disconnected();
                Err(e)
            }
        }
    }

    /// Wait for the driver to finish sending, for at most `timeout`. Returns
    /// whether the queue actually emptied.
    ///
    /// **This takes a timeout because the obvious implementation hangs.**
    /// `tcdrain` on Unix and `FlushFileBuffers` on Windows — which are what
    /// `serialport-rs`'s `flush` calls — wait for the output queue to empty,
    /// and flow control can hold that off indefinitely: drop CTS on the far
    /// end and a flush never returns. On a GUI thread that is a frozen
    /// application, and it is not a rare state, it is what a device asserting
    /// backpressure looks like. So the queue depth is polled instead, and the
    /// caller decides how long to care.
    pub fn flush(&mut self, timeout: Duration) -> Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.port.as_raw_fd();
            let deadline = Instant::now() + timeout;
            loop {
                if linux::output_queue_len(fd)? == 0 {
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        #[cfg(windows)]
        {
            let deadline = Instant::now() + timeout;
            loop {
                let status = windows::output_queue(&self.port)?;
                if status.broken {
                    self.wake
                        .as_mut()
                        .expect("an open Windows serial port has a wakeup")
                        .record_break();
                }
                if status.bytes == 0 {
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// `CommSendBreak` — hold the line at space for `dur`, then release it.
    ///
    /// Latched, matching `SetCommBreak`/`ClearCommBreak` rather than
    /// `tcsendbreak`'s fixed quarter-second, because Tera Term's macro
    /// language exposes the duration and devices differ on what they want.
    pub fn send_break(&mut self, dur: Duration) -> Result<()> {
        self.port.set_break()?;
        std::thread::sleep(dur);
        self.port.clear_break()?;
        Ok(())
    }

    pub fn modem_lines(&mut self) -> Result<ModemLines> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let bits = linux::modem_bits(self.port.as_raw_fd())?;
            Ok(ModemLines {
                cts: bits & libc::TIOCM_CTS != 0,
                dsr: bits & libc::TIOCM_DSR != 0,
                ri: bits & libc::TIOCM_RI != 0,
                cd: bits & libc::TIOCM_CD != 0,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(ModemLines {
                cts: self.port.read_clear_to_send()?,
                dsr: self.port.read_data_set_ready()?,
                ri: self.port.read_ring_indicator()?,
                cd: self.port.read_carrier_detect()?,
            })
        }
    }

    pub fn set_dtr(&mut self, on: bool) -> Result<()> {
        self.port.write_data_terminal_ready(on).map_err(Error::from)
    }

    pub fn set_rts(&mut self, on: bool) -> Result<()> {
        self.port.write_request_to_send(on).map_err(Error::from)
    }

    /// `CommLock` — tell the far end to stop or start sending, by whichever
    /// means the current flow control implies (`commlib.c:1186`).
    ///
    /// Note it is *not* symmetric with the flow-control setting: with no flow
    /// control at all Tera Term still sends the XOFF byte, on the theory that
    /// a device which ignores it is no worse off.
    pub fn lock(&mut self, lock: bool) -> Result<()> {
        match self.params.flow {
            FlowControl::RtsCts => self.set_rts(!lock),
            FlowControl::DsrDtr => self.set_dtr(!lock),
            FlowControl::None | FlowControl::XonXoff => {
                let b = if lock {
                    self.params.xoff
                } else {
                    self.params.xon
                };
                // Through `write`, not the raw form: this arm runs only for
                // the two modes with no DSR gate, so there is nothing for the
                // ordinary path to hold the byte back for — and on Windows
                // the ordinary path is the only one that has a deadline.
                self.write(&[b], Duration::from_millis(500))?;
                self.flush(Duration::from_millis(500)).map(|_| ())
            }
        }
    }

    /// Discard whatever the driver has buffered in either direction —
    /// `PurgeComm`'s `PURGE_*CLEAR` (`commlib.c:162`).
    ///
    /// Deliberately not part of [`apply`](SerialConn::apply), because it is
    /// not part of `CommResetSerial` either: that takes it as an *argument*,
    /// so the caller decides. `ClearComBuffOnOpen` is only ever that argument
    /// at open — Control > Reset port passes TRUE whatever the setting says
    /// (`vtwin.cpp:4913`). The reason someone turns it off is a console
    /// server, where what the driver buffered is what the far end said before
    /// anybody was watching, and often the only copy.
    pub fn clear(&mut self, input: bool, output: bool) -> Result<()> {
        let what = match (input, output) {
            (true, true) => serialport::ClearBuffer::All,
            (true, false) => serialport::ClearBuffer::Input,
            (false, true) => serialport::ClearBuffer::Output,
            (false, false) => return Ok(()),
        };
        self.port.clear(what).map_err(Error::from)
    }
}

impl crate::transport::Transport for SerialConn {
    fn link_kind(&self) -> crate::transport::LinkKind {
        crate::transport::LinkKind::Serial {
            baud: self.params.baud,
            seven_bit: self.params.data_bits == DataBits::Seven,
        }
    }

    fn read(
        &mut self,
        data: &mut Vec<u8>,
        events: &mut Vec<crate::transport::TransportEvent>,
    ) -> Result<usize> {
        // The decoder speaks SerialEvent; widening happens here rather than
        // in the decoder, so the serial layer stays usable on its own.
        let mut raw = Vec::new();
        let n = SerialConn::read(self, data, &mut raw)?;
        events.extend(raw.into_iter().map(crate::transport::TransportEvent::from));
        Ok(n)
    }

    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize> {
        SerialConn::write(self, data, timeout)
    }

    fn send_break(&mut self, dur: Duration) -> Result<()> {
        SerialConn::send_break(self, dur)
    }

    /// A serial line has no idea how big the window is, and nothing to tell.
    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<()> {
        Ok(())
    }

    /// The port's own descriptor. `serialport-rs` opens the tty and we already
    /// reach through to it for the patch layer, so there is nothing to
    /// construct here — the same escape hatch, used for waiting instead of for
    /// `termios`.
    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        use std::os::unix::io::AsRawFd;
        // A dead port's fd is still open until `Drop` runs, but a frontend
        // waiting on it would spin: EOF on a character device reads ready
        // forever. Say no instead, and let the disconnect event do its job.
        if self.dead {
            return None;
        }
        Some(self.port.as_raw_fd())
    }

    #[cfg(windows)]
    fn wait_handle(&self) -> Option<RawHandle> {
        if self.dead {
            return None;
        }
        self.wake
            .as_ref()
            .map(windows::WindowsSerialWake::wait_handle)
    }

    /// The one transport that answers this, and the whole reason it is asked.
    fn as_serial(&mut self) -> Option<&mut SerialConn> {
        Some(self)
    }

    fn describe(&self) -> String {
        format!("{} {}", self.path, self.params.baud)
    }

    fn serial_path(&self) -> Option<&str> {
        Some(&self.path)
    }
}

impl Drop for SerialConn {
    /// `CommClose` drops DTR on the way out (`commlib.c:848`), which is how a
    /// modem is told to hang up. Errors are ignored because the usual reason
    /// for one here is the adapter having already been unplugged.
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(wake) = &self.wake {
            wake.cancel();
        }
        let _ = self.port.write_data_terminal_ready(false);
    }
}
