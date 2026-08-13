//! A native wait for Win32 serial ports, over an **overlapped** COM handle.
//!
//! The handle is not waitable for received bytes, but `WaitCommEvent` is: one
//! worker owns a duplicate handle, publishes exactly one notice, and waits for
//! the reader to acknowledge it before arming the next wait. This is the same
//! handshake as Tera Term's `CommThread` and `ReadEnd` event
//! (`commlib.c:638`).
//!
//! **Every operation here is overlapped, and that is not a style choice.** A
//! COM handle opened without `FILE_FLAG_OVERLAPPED` gets `FO_SYNCHRONOUS_IO`,
//! which makes the I/O manager serialise `ReadFile` and `WriteFile` on the
//! file object — and a duplicated handle shares the file object, so the
//! worker's pending wait is in the same queue. A blocking `WaitCommEvent`
//! then holds every write behind it until a byte happens to arrive. See the
//! trap in `AGENTS.md`: what that looks like is a window which connects
//! cleanly and freezes on the first keystroke.
//!
//! The comm API wrappers — `SetCommState`, `PurgeComm`, `EscapeCommFunction`,
//! `GetCommModemStatus`, `ClearCommError` — are synchronous whatever the
//! handle is, so `serialport-rs`'s `COMPort` still owns the DCB, the pins and
//! the purge. Only its `Read`/`Write` impls are unusable here: they pass a
//! null `OVERLAPPED`, which is a bug against an overlapped handle rather than
//! a slow path.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use serialport::COMPort;
use windows_sys::Win32::Devices::Communication::{
    ClearCommError, GetCommState, SetCommMask, SetCommState, SetupComm, WaitCommEvent, CE_BREAK,
    COMSTAT, DCB, EVENPARITY, EV_BREAK, EV_ERR, EV_RXCHAR, MARKPARITY, NOPARITY, ODDPARITY,
    ONESTOPBIT, SPACEPARITY, TWOSTOPBITS,
};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_NAME, ERROR_IO_PENDING,
    ERROR_OPERATION_ABORTED, ERROR_PATH_NOT_FOUND, ERROR_SHARING_VIOLATION, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

use crate::error::{Error, Result};
use crate::windows_event::ManualEvent;

use super::{DataBits, FlowControl, Parity, PinControl, SerialParams, StopBits};

const INPUT_QUEUE: u32 = 64 * 1024;
const OUTPUT_QUEUE: u32 = 4 * 1024;
const XON_LIMIT: u16 = 768;
const XOFF_LIMIT: u16 = 3328;
const CONTROLLED_FLAGS: u32 = 0x7fff;

pub(super) struct QueueStatus {
    pub(super) bytes: u32,
    pub(super) broken: bool,
}

/// Open a COM port overlapped, without losing Win32's reason for failure.
///
/// `serialport-rs` maps missing, busy and access-denied handles to one
/// `NoDevice` variant and retains only a localized message. There is no sound
/// way to recover the distinction afterwards, so preserve `GetLastError` at
/// the same `CreateFileW` boundary the crate uses — and add
/// `FILE_FLAG_OVERLAPPED`, which the crate does not, because the wait worker
/// and the writes have to reach the driver independently.
pub(super) fn open(path: &str) -> Result<COMPort> {
    let mut name = Vec::with_capacity(path.len() + 5);
    if !path.starts_with('\\') {
        name.extend(r"\\.\".encode_utf16());
    }
    name.extend(path.encode_utf16());
    name.push(0);

    // SAFETY: `name` is NUL-terminated and live; no security/template handles
    // are supplied. COM ports are exclusive, matching upstream and the crate.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let source = std::io::Error::last_os_error();
        return Err(match source.raw_os_error().map(|e| e as u32) {
            Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION) => Error::Busy {
                path: path.to_string(),
            },
            Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_INVALID_NAME) => {
                Error::Disconnected
            }
            _ => Error::Open {
                path: path.to_string(),
                source,
            },
        });
    }

    // SAFETY: CreateFileW returned a uniquely owned handle and COMPort takes
    // ownership of it. Its cached name is irrelevant; SerialConn keeps `path`.
    Ok(unsafe { COMPort::from_raw_handle(handle as RawHandle) })
}

/// Apply the DCB in one operation, then ask the driver what actually stuck.
///
/// `serialport-rs` cannot express MARK/SPACE parity, DSR flow control, the pin
/// modes or custom XON/XOFF bytes. Calling its setters one at a time also
/// leaves a half-mutated port when the fifth setting is invalid. Upstream
/// builds one zeroed DCB and calls `SetCommState`; do the same here.
pub(super) fn apply(port: &COMPort, params: &SerialParams) -> Result<()> {
    let expected = build_dcb(params)?;
    let handle = port.as_raw_handle() as HANDLE;

    // A recommendation which drivers may ignore, just as upstream ignores
    // SetupComm's return value. The DCB below remains authoritative.
    // SAFETY: `port` owns a live communications handle.
    let _ = unsafe { SetupComm(handle, INPUT_QUEUE, OUTPUT_QUEUE) };
    // SAFETY: the handle and DCB remain live for the call.
    if unsafe { SetCommState(handle, &expected) } == 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }

    let mut actual = DCB {
        DCBlength: std::mem::size_of::<DCB>() as u32,
        ..DCB::default()
    };
    // SAFETY: the handle and output DCB remain live for the call.
    if unsafe { GetCommState(handle, &mut actual) } == 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    verify_dcb(&expected, &actual)
}

/// The two events the data path's overlapped operations complete on.
///
/// One per direction, reused across operations rather than created per call:
/// each operation is driven to completion or cancelled before its function
/// returns, so an event is never shared by two live requests. A write is a
/// keystroke, and a `CreateEventW` per keystroke buys nothing.
pub(super) struct SerialIo {
    read: OwnedHandle,
    write: OwnedHandle,
}

impl SerialIo {
    pub(super) fn new() -> Result<SerialIo> {
        Ok(SerialIo {
            read: event()?,
            write: event()?,
        })
    }
}

fn event() -> Result<OwnedHandle> {
    // SAFETY: unnamed event, default security, manual reset, initially quiet.
    // The kernel resets it when an overlapped operation is queued on it.
    let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if handle.is_null() {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    // SAFETY: `handle` is fresh and transferred exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// One overlapped write, bounded by the caller's deadline.
///
/// The deadline is ours rather than `COMMTIMEOUTS`', which is what lets the
/// port's read policy stay untouched: a write no longer has to borrow and
/// restore connection-wide state, so a failed write cannot leak a timeout into
/// the next read.
pub(super) fn write(
    port: &COMPort,
    io: &SerialIo,
    data: &[u8],
    timeout: Duration,
) -> Result<usize> {
    let handle = port.as_raw_handle() as HANDLE;
    let event = io.write.as_raw_handle() as HANDLE;
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..OVERLAPPED::default()
    };
    // SAFETY: `data`, `overlapped` and both handles outlive the call, which
    // does not return while the kernel may still be writing to `overlapped`.
    let started = unsafe {
        WriteFile(
            handle,
            data.as_ptr(),
            data.len() as u32,
            std::ptr::null_mut(),
            &mut overlapped,
        )
    } != 0;
    // SAFETY: as above.
    unsafe { finish(handle, &mut overlapped, event, started, Some(timeout)) }
}

/// One overlapped read.
///
/// **Zero is a timeout, not an end of file.** A COM handle has no EOF: an
/// expired `ReadTotalTimeoutConstant` completes the request successfully with
/// nothing transferred, so a caller which reads zero as a disconnect drops the
/// session on any quiet line. `serialport-rs` hid this by translating it to
/// `ErrorKind::TimedOut`; reading the handle directly does not.
pub(super) fn read(
    port: &COMPort,
    io: &SerialIo,
    buf: &mut [u8],
    timeout: Duration,
) -> Result<usize> {
    let handle = port.as_raw_handle() as HANDLE;
    let event = io.read.as_raw_handle() as HANDLE;
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..OVERLAPPED::default()
    };
    // SAFETY: `buf`, `overlapped` and both handles outlive the call, which
    // does not return while the kernel may still be writing to either.
    let started = unsafe {
        ReadFile(
            handle,
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null_mut(),
            &mut overlapped,
        )
    } != 0;
    // The driver's own read timeout should end this first. The wait is given a
    // margin over it anyway, because the point of the overlapped handle is
    // that no single misbehaving request can hold the frontend's thread.
    let bound = timeout.saturating_mul(2) + Duration::from_secs(1);
    // SAFETY: as above.
    unsafe { finish(handle, &mut overlapped, event, started, Some(bound)) }
}

/// Wait for one overlapped operation, and never leave it pending.
///
/// A cancelled request still owns `overlapped` until the kernel completes it,
/// so a timeout cancels and then *reaps* rather than returning — the stack
/// frame holding the `OVERLAPPED` is about to go away. `ERROR_OPERATION_ABORTED`
/// is the expected answer there and reports whatever got out beforehand, which
/// for a write is a short count the caller retries from.
///
/// # Safety
///
/// `overlapped` must be live for the whole call and must not be reachable by
/// any other pending request, and `handle` must be the handle the operation
/// was issued on.
unsafe fn finish(
    handle: HANDLE,
    overlapped: *mut OVERLAPPED,
    event: HANDLE,
    started: bool,
    timeout: Option<Duration>,
) -> Result<usize> {
    if !started {
        let pending = std::io::Error::last_os_error();
        if pending.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(Error::from_io(pending));
        }
        let ms = timeout.map_or(INFINITE, wait_ms);
        if WaitForSingleObject(event, ms) != WAIT_OBJECT_0 {
            CancelIoEx(handle, overlapped);
            return Ok(reap(handle, overlapped));
        }
    }

    let mut moved = 0u32;
    if GetOverlappedResult(handle, overlapped, &mut moved, 1) == 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(ERROR_OPERATION_ABORTED as i32) {
            return Err(Error::from_io(e));
        }
    }
    Ok(moved as usize)
}

/// Collect a cancelled operation's completion, discarding its outcome.
///
/// # Safety
///
/// As [`finish`], and the operation must already have been cancelled.
unsafe fn reap(handle: HANDLE, overlapped: *mut OVERLAPPED) -> usize {
    let mut moved = 0u32;
    let _ = GetOverlappedResult(handle, overlapped, &mut moved, 1);
    moved as usize
}

/// `WaitForSingleObject`'s milliseconds, never accidentally `INFINITE`.
fn wait_ms(timeout: Duration) -> u32 {
    timeout.as_millis().clamp(1, u32::MAX as u128 - 1) as u32
}

/// Output depth, retaining a break which `ClearCommError` would otherwise eat.
pub(super) fn output_queue(port: &COMPort) -> Result<QueueStatus> {
    let mut errors = 0;
    let mut status = COMSTAT::default();
    // SAFETY: the handle and both output structures are live.
    if unsafe { ClearCommError(port.as_raw_handle() as HANDLE, &mut errors, &mut status) } == 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    Ok(QueueStatus {
        bytes: status.cbOutQue,
        broken: errors & CE_BREAK != 0,
    })
}

fn build_dcb(params: &SerialParams) -> Result<DCB> {
    if params.dtr == PinControl::Toggle {
        return Err(Error::Unsupported("DTR toggle control".into()));
    }

    let mut dcb = DCB {
        DCBlength: std::mem::size_of::<DCB>() as u32,
        BaudRate: params.baud,
        ByteSize: match params.data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        },
        Parity: match params.parity {
            Parity::None => NOPARITY,
            Parity::Odd => ODDPARITY,
            Parity::Even => EVENPARITY,
            Parity::Mark => MARKPARITY,
            Parity::Space => SPACEPARITY,
        },
        StopBits: match params.stop_bits {
            StopBits::One => ONESTOPBIT,
            StopBits::Two => TWOSTOPBITS,
        },
        XonChar: params.xon as i8,
        XoffChar: params.xoff as i8,
        ..DCB::default()
    };

    set_flag(&mut dcb, 0, true); // fBinary
    set_flag(&mut dcb, 1, params.parity != Parity::None); // fParity
    set_field(&mut dcb, 4, 2, params.dtr as u32); // fDtrControl
    set_field(&mut dcb, 12, 2, params.rts as u32); // fRtsControl
    match params.flow {
        FlowControl::None => {}
        FlowControl::XonXoff => {
            set_flag(&mut dcb, 8, true); // fOutX
            set_flag(&mut dcb, 9, true); // fInX
            dcb.XonLim = XON_LIMIT;
            dcb.XoffLim = XOFF_LIMIT;
        }
        FlowControl::RtsCts => set_flag(&mut dcb, 2, true), // fOutxCtsFlow
        FlowControl::DsrDtr => set_flag(&mut dcb, 3, true), // fOutxDsrFlow
    }
    Ok(dcb)
}

fn set_flag(dcb: &mut DCB, bit: u32, on: bool) {
    if on {
        dcb._bitfield |= 1 << bit;
    } else {
        dcb._bitfield &= !(1 << bit);
    }
}

fn set_field(dcb: &mut DCB, bit: u32, width: u32, value: u32) {
    let mask = ((1 << width) - 1) << bit;
    dcb._bitfield = (dcb._bitfield & !mask) | ((value << bit) & mask);
}

fn verify_dcb(expected: &DCB, actual: &DCB) -> Result<()> {
    macro_rules! same {
        ($field:ident) => {
            if actual.$field != expected.$field {
                return Err(Error::Unsupported(format!(
                    "COM driver kept {}={} instead of {}",
                    stringify!($field),
                    actual.$field,
                    expected.$field
                )));
            }
        };
    }

    same!(BaudRate);
    same!(ByteSize);
    same!(Parity);
    same!(StopBits);
    if actual._bitfield & CONTROLLED_FLAGS != expected._bitfield & CONTROLLED_FLAGS {
        return Err(Error::Unsupported(format!(
            "COM driver kept control flags {:#06x} instead of {:#06x}",
            actual._bitfield & CONTROLLED_FLAGS,
            expected._bitfield & CONTROLLED_FLAGS
        )));
    }
    if expected._bitfield & ((1 << 8) | (1 << 9)) != 0 {
        same!(XonChar);
        same!(XoffChar);
        same!(XonLim);
        same!(XoffLim);
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct Notice {
    pub(super) receive: bool,
    pub(super) broken: bool,
    pub(super) worker: bool,
}

enum Message {
    Notice(Notice),
    End,
}

pub(super) struct WindowsSerialWake {
    notices: Receiver<Message>,
    acknowledge: SyncSender<bool>,
    wake: Arc<ManualEvent>,
    held: Option<Message>,
    pending_break: bool,
    /// Borrowed from the port. `SerialConn::drop` cancels while it is live.
    cancel_handle: usize,
}

impl WindowsSerialWake {
    pub(super) fn start(port: &COMPort) -> Result<WindowsSerialWake> {
        let clone = port.try_clone_native().map_err(Error::from)?;
        let cancel_handle = port.as_raw_handle() as usize;
        let worker_handle = clone.as_raw_handle() as usize;
        let mask = EV_RXCHAR | EV_ERR | EV_BREAK;
        // SAFETY: both raw handles belong to live `COMPort`s. The clone moves
        // into the worker and the original outlives `WindowsSerialWake`.
        if unsafe { SetCommMask(worker_handle as HANDLE, mask) } == 0 {
            return Err(Error::from_io(std::io::Error::last_os_error()));
        }

        // Capacity one plus the acknowledgement is deliberate: a modal
        // dialog may stop the frontend for minutes, and line events must
        // apply backpressure instead of becoming an unbounded queue.
        let (notice_tx, notice_rx) = std::sync::mpsc::sync_channel(1);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        let wake = Arc::new(ManualEvent::new()?);
        let worker_wake = Arc::clone(&wake);
        let wait_event = event()?;
        std::thread::Builder::new()
            .name("sterna-serial-wait".into())
            .spawn(move || {
                let mut more_buffered = false;
                loop {
                    let notice = if more_buffered {
                        Notice {
                            receive: true,
                            broken: false,
                            worker: true,
                        }
                    } else {
                        let mut events = 0;
                        let mut overlapped = OVERLAPPED {
                            hEvent: wait_event.as_raw_handle() as HANDLE,
                            ..OVERLAPPED::default()
                        };
                        // The wait is overlapped so that it sits in the
                        // kernel without holding the file object: a
                        // synchronous one would queue every write on this
                        // port behind it until a byte arrived.
                        // SAFETY: `clone` keeps the COM handle live, and
                        // `overlapped` and its event outlive the operation —
                        // `finish` does not return while either is in use.
                        let started = unsafe {
                            WaitCommEvent(
                                worker_handle as HANDLE,
                                &mut events,
                                &mut overlapped,
                            )
                        } != 0;
                        // SAFETY: as above. No deadline: the wait ends when
                        // the line does something or when `cancel` clears the
                        // mask, and the thread exists in order to block.
                        if unsafe {
                            finish(
                                worker_handle as HANDLE,
                                &mut overlapped,
                                wait_event.as_raw_handle() as HANDLE,
                                started,
                                None,
                            )
                        }
                        .is_err()
                        {
                            publish_end(&notice_tx, &worker_wake);
                            break;
                        }
                        // `SetCommMask(handle, 0)` is the documented way to
                        // cancel a WaitCommEvent; it completes with an empty
                        // mask rather than reporting a disconnect.
                        if events == 0 {
                            break;
                        }

                        let mut errors = 0;
                        if events & EV_ERR != 0
                            // SAFETY: the handle and output pointer are live;
                            // no COMSTAT snapshot is needed here.
                            && unsafe {
                                ClearCommError(
                                    worker_handle as HANDLE,
                                    &mut errors,
                                    std::ptr::null_mut(),
                                )
                            } == 0
                        {
                            publish_end(&notice_tx, &worker_wake);
                            break;
                        }
                        Notice {
                            receive: events & EV_RXCHAR != 0,
                            broken: events & EV_BREAK != 0 || errors & CE_BREAK != 0,
                            worker: true,
                        }
                    };

                    if notice_tx.send(Message::Notice(notice)).is_err() {
                        break;
                    }
                    worker_wake.signal();
                    more_buffered = match ack_rx.recv() {
                        Ok(more) => more,
                        Err(_) => break,
                    };
                }
                drop(clone);
            })
            .map_err(Error::from_io)?;

        Ok(WindowsSerialWake {
            notices: notice_rx,
            acknowledge: ack_tx,
            wake,
            held: None,
            pending_break: false,
            cancel_handle,
        })
    }

    pub(super) fn take(&mut self) -> Result<Option<Notice>> {
        self.wake.reset();
        if self.pending_break {
            self.pending_break = false;
            if self.held.is_none() {
                self.held = match self.notices.try_recv() {
                    Ok(message) => Some(message),
                    Err(TryRecvError::Disconnected) => Some(Message::End),
                    Err(TryRecvError::Empty) => None,
                };
            }
            if self.held.is_some() {
                self.wake.signal();
            }
            return Ok(Some(Notice {
                receive: false,
                broken: true,
                worker: false,
            }));
        }

        let message = match self.held.take() {
            Some(message) => Ok(message),
            None => self.notices.try_recv(),
        };
        match message {
            Ok(Message::Notice(notice)) => Ok(Some(notice)),
            Ok(Message::End) | Err(TryRecvError::Disconnected) => Err(Error::Disconnected),
            Err(TryRecvError::Empty) => Ok(None),
        }
    }

    pub(super) fn record_break(&mut self) {
        self.pending_break = true;
        self.wake.signal();
    }

    /// Let the worker arm its next native wait. `more` synthesises another
    /// receive notice when the frontend filled its whole input buffer.
    pub(super) fn acknowledge(&self, more: bool) -> Result<()> {
        self.acknowledge.send(more).map_err(|_| Error::Disconnected)
    }

    pub(super) fn wait_handle(&self) -> RawHandle {
        self.wake.handle()
    }

    pub(super) fn cancel(&self) {
        // SAFETY: SerialConn calls this before dropping the original port.
        // SetCommMask with zero completes the worker's pending WaitCommEvent.
        let _ = unsafe { SetCommMask(self.cancel_handle as HANDLE, 0) };
    }
}

fn publish_end(tx: &SyncSender<Message>, wake: &ManualEvent) {
    if tx.send(Message::End).is_ok() {
        wake.signal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(dcb: &DCB, bit: u32, width: u32) -> u32 {
        (dcb._bitfield >> bit) & ((1 << width) - 1)
    }

    #[test]
    fn dcb_carries_every_setting_the_portable_crate_cannot() {
        let params = SerialParams {
            baud: 115_200,
            data_bits: DataBits::Seven,
            parity: Parity::Mark,
            stop_bits: StopBits::Two,
            flow: FlowControl::XonXoff,
            xon: 0x80,
            xoff: 0xff,
            dtr: PinControl::Handshake,
            rts: PinControl::Toggle,
            ..SerialParams::default()
        };
        let dcb = build_dcb(&params).unwrap();

        assert_eq!(dcb.BaudRate, 115_200);
        assert_eq!(dcb.ByteSize, 7);
        assert_eq!(dcb.Parity, MARKPARITY);
        assert_eq!(dcb.StopBits, TWOSTOPBITS);
        assert_eq!(dcb.XonChar, -128);
        assert_eq!(dcb.XoffChar, -1);
        assert_eq!(dcb.XonLim, XON_LIMIT);
        assert_eq!(dcb.XoffLim, XOFF_LIMIT);
        assert_eq!(field(&dcb, 0, 1), 1, "fBinary");
        assert_eq!(field(&dcb, 1, 1), 1, "fParity");
        assert_eq!(field(&dcb, 4, 2), 2, "fDtrControl");
        assert_eq!(field(&dcb, 8, 1), 1, "fOutX");
        assert_eq!(field(&dcb, 9, 1), 1, "fInX");
        assert_eq!(field(&dcb, 12, 2), 3, "fRtsControl");
    }

    #[test]
    fn cts_and_dsr_flow_are_distinct_dcb_bits() {
        let mut params = SerialParams {
            flow: FlowControl::RtsCts,
            ..SerialParams::default()
        };
        let cts = build_dcb(&params).unwrap();
        assert_eq!(field(&cts, 2, 1), 1, "fOutxCtsFlow");
        assert_eq!(field(&cts, 3, 1), 0, "fOutxDsrFlow");

        params.flow = FlowControl::DsrDtr;
        let dsr = build_dcb(&params).unwrap();
        assert_eq!(field(&dsr, 2, 1), 0, "fOutxCtsFlow");
        assert_eq!(field(&dsr, 3, 1), 1, "fOutxDsrFlow");
    }

    #[test]
    fn dtr_toggle_fails_before_touching_the_port() {
        let params = SerialParams {
            dtr: PinControl::Toggle,
            ..SerialParams::default()
        };
        assert!(matches!(build_dcb(&params), Err(Error::Unsupported(_))));
    }

    /// A deadline must never round to `INFINITE`, which is what an unbounded
    /// wait on the frontend's own thread is spelt as.
    #[test]
    fn a_wait_is_bounded_and_monotonic() {
        assert_eq!(wait_ms(Duration::ZERO), 1);
        assert_eq!(wait_ms(Duration::from_nanos(1)), 1);
        assert_eq!(wait_ms(Duration::from_millis(25)), 25);
        assert_eq!(wait_ms(Duration::from_millis(u32::MAX as u64)), u32::MAX - 1);
        assert_ne!(wait_ms(Duration::MAX), INFINITE);
    }

    #[test]
    fn a_polled_break_does_not_consume_a_worker_notice() {
        let (notice_tx, notice_rx) = std::sync::mpsc::sync_channel(1);
        let (ack_tx, _ack_rx) = std::sync::mpsc::sync_channel(1);
        let wake = Arc::new(ManualEvent::new().unwrap());
        notice_tx
            .send(Message::Notice(Notice {
                receive: true,
                broken: false,
                worker: true,
            }))
            .unwrap();
        let mut serial = WindowsSerialWake {
            notices: notice_rx,
            acknowledge: ack_tx,
            wake,
            held: None,
            pending_break: false,
            cancel_handle: 0,
        };

        serial.record_break();
        let polled = serial.take().unwrap().unwrap();
        assert!(polled.broken);
        assert!(!polled.receive);
        assert!(!polled.worker);

        let worker = serial.take().unwrap().unwrap();
        assert!(worker.receive);
        assert!(!worker.broken);
        assert!(worker.worker);
    }
}
