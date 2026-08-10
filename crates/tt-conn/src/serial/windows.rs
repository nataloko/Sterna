//! A native wait for synchronous Win32 serial ports.
//!
//! `serialport-rs` opens a COM handle for synchronous I/O. The handle itself
//! is not waitable for received bytes, but `WaitCommEvent` is: one worker owns
//! a duplicate handle, publishes exactly one notice, and waits for the reader
//! to acknowledge it before arming the next wait. This is the same handshake
//! as Tera Term's `CommThread` and `ReadEnd` event (`commlib.c:638`).

use std::os::windows::io::{AsRawHandle, RawHandle};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::sync::Arc;

use serialport::COMPort;
use windows_sys::Win32::Devices::Communication::{
    ClearCommError, SetCommMask, WaitCommEvent, CE_BREAK, EV_BREAK, EV_ERR, EV_RXCHAR,
};
use windows_sys::Win32::Foundation::HANDLE;

use crate::error::{Error, Result};
use crate::windows_event::ManualEvent;

#[derive(Clone, Copy)]
pub(super) struct Notice {
    pub(super) receive: bool,
    pub(super) broken: bool,
}

enum Message {
    Notice(Notice),
    End,
}

pub(super) struct WindowsSerialWake {
    notices: Receiver<Message>,
    acknowledge: SyncSender<bool>,
    wake: Arc<ManualEvent>,
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
            cancel_handle,
        })
    }

    pub(super) fn take(&mut self) -> Result<Option<Notice>> {
        self.wake.reset();
        match self.notices.try_recv() {
            Ok(Message::Notice(notice)) => Ok(Some(notice)),
            Ok(Message::End) | Err(TryRecvError::Disconnected) => Err(Error::Disconnected),
            Err(TryRecvError::Empty) => Ok(None),
        }
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
