//! Turn one blocking Windows byte stream into a bounded queue and an event.
//!
//! ConPTY pipes and cloned TCP sockets are both synchronous handles. Their
//! worker is allowed to block; the frontend is not. One 8-KiB message is
//! returned per read so a burst stays split over event-loop turns exactly as
//! it is on Unix, while the bounded queue pushes back at 1 MiB.

use std::io::{ErrorKind, Read};
use std::os::windows::io::RawHandle;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::windows_event::ManualEvent;

const QUEUE_DEPTH: usize = 128;

pub(crate) struct WindowsReader {
    read: Receiver<Message>,
    wake: Arc<ManualEvent>,
    held: Option<Message>,
    ended: bool,
}

enum Message {
    Data(Vec<u8>),
    End,
}

impl WindowsReader {
    pub(crate) fn start(
        mut reader: Box<dyn Read + Send>,
        thread_name: &str,
    ) -> Result<WindowsReader> {
        let wake = Arc::new(ManualEvent::new()?);
        let (tx, rx) = std::sync::mpsc::sync_channel(QUEUE_DEPTH);
        let worker_wake = Arc::clone(&wake);
        std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            if tx.send(Message::End).is_ok() {
                                worker_wake.signal();
                            }
                            return;
                        }
                        Ok(n) => {
                            if tx.send(Message::Data(buf[..n].to_vec())).is_err() {
                                return;
                            }
                            worker_wake.signal();
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => {
                            if tx.send(Message::End).is_ok() {
                                worker_wake.signal();
                            }
                            return;
                        }
                    }
                }
            })
            .map_err(Error::from_io)?;

        Ok(WindowsReader {
            read: rx,
            wake,
            held: None,
            ended: false,
        })
    }

    pub(crate) fn read(&mut self, data: &mut Vec<u8>) -> Result<usize> {
        self.wake.reset();
        if self.ended {
            return Err(Error::Disconnected);
        }

        let message = match self.held.take() {
            Some(message) => message,
            None => match self.read.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) => return Ok(0),
                Err(TryRecvError::Disconnected) => {
                    self.ended = true;
                    return Err(Error::Disconnected);
                }
            },
        };

        match message {
            Message::End => {
                self.ended = true;
                Err(Error::Disconnected)
            }
            Message::Data(bytes) => {
                let n = bytes.len();
                data.extend_from_slice(&bytes);

                // A queued second message needs another edge because reset
                // coalesced the worker's earlier signals. Holding it preserves
                // order: the next message can be EOF after the final bytes.
                self.held = match self.read.try_recv() {
                    Ok(next) => Some(next),
                    Err(TryRecvError::Disconnected) => Some(Message::End),
                    Err(TryRecvError::Empty) => None,
                };
                if self.held.is_some() {
                    self.wake.signal();
                }
                Ok(n)
            }
        }
    }

    pub(crate) fn wait_handle(&self) -> RawHandle {
        self.wake.handle()
    }
}
