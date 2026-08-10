//! A native wait for synchronous Win32 serial ports.
//!
//! `serialport-rs` opens a COM handle for synchronous I/O. The handle itself
//! is not waitable for received bytes, but `WaitCommEvent` is: one worker owns
//! a duplicate handle, publishes exactly one notice, and waits for the reader
//! to acknowledge it before arming the next wait. This is the same handshake
//! as Tera Term's `CommThread` and `ReadEnd` event (`commlib.c:638`).

use std::io::Write;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::sync::Arc;

use serialport::COMPort;
use windows_sys::Win32::Devices::Communication::{
    ClearCommError, GetCommState, GetCommTimeouts, SetCommMask, SetCommState, SetCommTimeouts,
    SetupComm, WaitCommEvent, CE_BREAK, COMMTIMEOUTS, COMSTAT, DCB, EVENPARITY, EV_BREAK, EV_ERR,
    EV_RXCHAR, MARKPARITY, NOPARITY, ODDPARITY, ONESTOPBIT, SPACEPARITY, TWOSTOPBITS,
};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_NAME, ERROR_PATH_NOT_FOUND,
    ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};

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

/// Open a COM port without losing Win32's reason for failure.
///
/// `serialport-rs` maps missing, busy and access-denied handles to one
/// `NoDevice` variant and retains only a localized message. There is no sound
/// way to recover the distinction afterwards, so preserve `GetLastError` at
/// the same `CreateFileW` boundary the crate uses.
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
            FILE_ATTRIBUTE_NORMAL,
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

/// One synchronous write with the caller's deadline, without changing the
/// read timeout cached by `serialport-rs`.
pub(super) fn write(
    port: &mut COMPort,
    data: &[u8],
    timeout: std::time::Duration,
) -> Result<usize> {
    let handle = port.as_raw_handle() as HANDLE;
    let mut original = COMMTIMEOUTS::default();
    // SAFETY: the handle and output structure are live.
    if unsafe { GetCommTimeouts(handle, &mut original) } == 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }
    let mut temporary = original;
    temporary.WriteTotalTimeoutMultiplier = 0;
    temporary.WriteTotalTimeoutConstant = timeout_constant(timeout);
    // SAFETY: the handle and input structure are live.
    if unsafe { SetCommTimeouts(handle, &temporary) } == 0 {
        return Err(Error::from_io(std::io::Error::last_os_error()));
    }

    let written = port.write(data).map_err(Error::from_io);
    // Always put the read and ordinary write policy back, including after a
    // failed WriteFile. A temporary timeout leaking into the next read is a
    // connection-wide state bug, not just a slow write.
    // SAFETY: as above.
    let restored = if unsafe { SetCommTimeouts(handle, &original) } == 0 {
        Err(Error::from_io(std::io::Error::last_os_error()))
    } else {
        Ok(())
    };
    match (written, restored) {
        (Err(e), _) => Err(e),
        (Ok(_), Err(e)) => Err(e),
        (Ok(n), Ok(())) => Ok(n),
    }
}

fn timeout_constant(timeout: std::time::Duration) -> u32 {
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
                        // SAFETY: `clone` keeps the synchronous COM handle
                        // live, and a null OVERLAPPED requests a blocking wait.
                        if unsafe {
                            WaitCommEvent(
                                worker_handle as HANDLE,
                                &mut events,
                                std::ptr::null_mut(),
                            )
                        } == 0
                        {
                            publish_end(&notice_tx, &worker_wake);
                            break;
                        }
                        // `SetCommMask(handle, 0)` is the documented way to
                        // cancel a synchronous WaitCommEvent; it returns with
                        // an empty mask rather than reporting a disconnect.
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
        // SetCommMask with zero wakes a blocking synchronous WaitCommEvent.
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

    #[test]
    fn write_timeout_is_bounded_and_monotonic() {
        assert_eq!(timeout_constant(std::time::Duration::ZERO), 1);
        assert_eq!(timeout_constant(std::time::Duration::from_nanos(1)), 1);
        assert_eq!(timeout_constant(std::time::Duration::from_millis(25)), 25);
        assert_eq!(
            timeout_constant(std::time::Duration::from_millis(u32::MAX as u64)),
            u32::MAX - 1
        );
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
