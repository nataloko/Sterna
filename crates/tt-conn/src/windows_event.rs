//! A private manual-reset event for worker-backed Windows transports.
//!
//! The worker publishes bytes or state first and signals second. The frontend
//! resets before collecting that work, so a racing publication leaves either
//! visible state or a signalled event. This is the Win32 counterpart of the
//! non-blocking self-pipe used on Unix.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent};

pub(crate) struct ManualEvent {
    event: OwnedHandle,
}

impl ManualEvent {
    pub(crate) fn new() -> std::io::Result<ManualEvent> {
        // SAFETY: unnamed event, default security, manual reset, initially
        // quiet. A non-null handle is uniquely owned by the returned object.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `event` is fresh and transferred exactly once.
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        Ok(ManualEvent { event })
    }

    pub(crate) fn handle(&self) -> RawHandle {
        self.event.as_raw_handle()
    }

    pub(crate) fn signal(&self) {
        // SAFETY: the event stays live for this borrow. Multiple sets
        // deliberately coalesce into one pending frontend wakeup.
        unsafe {
            SetEvent(self.event.as_raw_handle());
        }
    }

    pub(crate) fn reset(&self) {
        // SAFETY: the event stays live for this borrow.
        unsafe {
            ResetEvent(self.event.as_raw_handle());
        }
    }
}
