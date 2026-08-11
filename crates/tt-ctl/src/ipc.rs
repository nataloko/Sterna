//! The local byte stream under the control protocol.
//!
//! Unix has a filesystem-named stream socket. Windows has a byte-mode named
//! pipe: the same `Read`/`Write` contract, but no socket file and no listening
//! handle that accepts more than one client. Keeping that difference here
//! stops the JSON-RPC client and server from becoming two platform ports.

use std::io::{self, Read, Write};
use std::path::Path;

/// One connected client or server end.
pub(crate) struct Stream {
    #[cfg(unix)]
    inner: std::os::unix::net::UnixStream,
    #[cfg(windows)]
    inner: std::fs::File,
    #[cfg(windows)]
    server: bool,
}

impl Stream {
    pub(crate) fn connect(path: &Path) -> io::Result<Stream> {
        #[cfg(unix)]
        {
            std::os::unix::net::UnixStream::connect(path).map(|inner| Stream { inner })
        }
        #[cfg(windows)]
        {
            windows::connect(path)
        }
    }

    pub(crate) fn try_clone(&self) -> io::Result<Stream> {
        Ok(Stream {
            inner: self.inner.try_clone()?,
            #[cfg(windows)]
            server: self.server,
        })
    }

    pub(crate) fn shutdown(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.inner.shutdown(std::net::Shutdown::Both)
        }
        #[cfg(windows)]
        {
            windows::disconnect(self)
        }
    }

    pub(crate) fn peer_is_us(&self) -> bool {
        #[cfg(unix)]
        {
            unix::peer_is_us(&self.inner)
        }
        #[cfg(windows)]
        {
            windows::peer_is_us(self)
        }
    }

    /// The same question with the reason kept. Windows only, because it is the
    /// only side where the check is several calls that can each fail.
    ///
    /// For the tests: the accept loop has to refuse either way and this
    /// library has no channel to complain on, so the reason is of use exactly
    /// where it can be acted on.
    #[cfg(all(windows, test))]
    pub(crate) fn peer_check(&self) -> io::Result<bool> {
        windows::peer_check(self)
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A bound endpoint.
#[derive(Debug)]
pub(crate) struct Listener {
    #[cfg(unix)]
    inner: std::os::unix::net::UnixListener,
    #[cfg(windows)]
    path: std::path::PathBuf,
    #[cfg(windows)]
    next: Option<std::fs::File>,
}

impl Listener {
    pub(crate) fn bind(path: &Path) -> io::Result<Listener> {
        #[cfg(unix)]
        {
            std::os::unix::net::UnixListener::bind(path).map(|inner| Listener { inner })
        }
        #[cfg(windows)]
        {
            windows::bind(path)
        }
    }

    pub(crate) fn accept(&mut self) -> io::Result<Stream> {
        #[cfg(unix)]
        {
            self.inner.accept().map(|(inner, _)| Stream { inner })
        }
        #[cfg(windows)]
        {
            windows::accept(self)
        }
    }

    #[cfg(unix)]
    pub(crate) fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        use std::os::unix::io::AsRawFd;
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
mod unix {
    use std::os::unix::io::AsRawFd;

    pub(super) fn peer_is_us(stream: &std::os::unix::net::UnixStream) -> bool {
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `cred` and `len` are the size the option expects, and the fd
        // is borrowed from a live stream.
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut libc::ucred as *mut libc::c_void,
                &mut len,
            )
        };
        // A failure is not a pass. Root is refused along with every other uid.
        rc == 0 && cred.uid == unsafe { libc::getuid() }
    }
}

#[cfg(windows)]
mod windows {
    use super::{Listener, Stream};
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, RevertToSelf, TokenUser, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient,
        WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };

    const BUFFER: u32 = 64 * 1024;
    const CONNECT_WAIT_MS: u32 = 250;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn instance(path: &Path, first: bool) -> io::Result<std::fs::File> {
        let path = wide(path);
        let open_flags = if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
        // SAFETY: the name is NUL-terminated for the duration of the call;
        // the returned handle is either invalid or newly owned.
        let handle = unsafe {
            CreateNamedPipeW(
                path.as_ptr(),
                PIPE_ACCESS_DUPLEX | open_flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let e = io::Error::last_os_error();
            // `FILE_FLAG_FIRST_PIPE_INSTANCE` reports an existing server as
            // access denied. At this boundary it is an address collision,
            // which is the useful answer to a `/D=` the user duplicated.
            if first && e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another window owns this named pipe",
                ));
            }
            return Err(e);
        }
        // SAFETY: `handle` was returned owned by CreateNamedPipeW.
        Ok(unsafe { std::fs::File::from_raw_handle(handle) })
    }

    pub(super) fn bind(path: &Path) -> io::Result<Listener> {
        Ok(Listener {
            path: path.to_path_buf(),
            next: Some(instance(path, true)?),
        })
    }

    pub(super) fn accept(listener: &mut Listener) -> io::Result<Stream> {
        let connected = listener
            .next
            .take()
            .ok_or_else(|| io::Error::other("named-pipe listener has stopped"))?;
        // A client may open the instance between CreateNamedPipe and this
        // call; ERROR_PIPE_CONNECTED is that successful race.
        let ok = unsafe { ConnectNamedPipe(connected.as_raw_handle(), std::ptr::null_mut()) };
        if ok == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
                return Err(e);
            }
        }
        // Put the next instance in the namespace before the connection thread
        // starts. A client that arrives in the tiny create/connect gap waits
        // in `connect` rather than seeing a false "no window".
        listener.next = Some(instance(&listener.path, false)?);
        Ok(Stream {
            inner: connected,
            server: true,
        })
    }

    pub(super) fn connect(path: &Path) -> io::Result<Stream> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
            {
                Ok(inner) => {
                    return Ok(Stream {
                        inner,
                        server: false,
                    })
                }
                Err(e)
                    if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                        && Instant::now() < deadline =>
                {
                    let path = wide(path);
                    // SAFETY: the name is NUL-terminated. A timeout is not an
                    // error of its own; the following open reports the useful
                    // Windows error if no instance became available.
                    unsafe {
                        WaitNamedPipeW(path.as_ptr(), CONNECT_WAIT_MS);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub(super) fn disconnect(stream: &Stream) -> io::Result<()> {
        if !stream.server {
            return Ok(());
        }
        // SAFETY: this is a live server-side pipe handle. Disconnecting a
        // duplicate disconnects the pipe instance and wakes a blocked reader.
        if unsafe { DisconnectNamedPipe(stream.inner.as_raw_handle()) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    struct Token(OwnedHandle);

    impl Token {
        fn process() -> io::Result<Token> {
            let mut token = std::ptr::null_mut();
            let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: OpenProcessToken returned a newly owned handle.
            Ok(Token(unsafe { OwnedHandle::from_raw_handle(token) }))
        }

        fn thread() -> io::Result<Token> {
            let mut token = std::ptr::null_mut();
            let ok = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: OpenThreadToken returned a newly owned handle.
            Ok(Token(unsafe { OwnedHandle::from_raw_handle(token) }))
        }

        fn user(&self) -> io::Result<TokenUser> {
            let mut needed = 0u32;
            unsafe {
                GetTokenInformation(
                    self.0.as_raw_handle(),
                    TokenUser,
                    std::ptr::null_mut(),
                    0,
                    &mut needed,
                );
            }
            if needed == 0 {
                return Err(io::Error::last_os_error());
            }
            // `usize` gives the buffer enough alignment for TOKEN_USER.
            let words = (needed as usize).div_ceil(std::mem::size_of::<usize>());
            let mut storage = vec![0usize; words];
            let ok = unsafe {
                GetTokenInformation(
                    self.0.as_raw_handle(),
                    TokenUser,
                    storage.as_mut_ptr().cast::<c_void>(),
                    needed,
                    &mut needed,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(TokenUser { storage })
        }
    }

    struct TokenUser {
        storage: Vec<usize>,
    }

    impl TokenUser {
        fn sid(&self) -> *mut c_void {
            // SAFETY: GetTokenInformation filled this aligned allocation with
            // TOKEN_USER and its SID remains inside `storage`.
            unsafe { (*(self.storage.as_ptr().cast::<TOKEN_USER>())).User.Sid }
        }
    }

    /// The identity check with its reason kept, which is the form the tests
    /// use and the form a diagnosis needs.
    ///
    /// A refusal is a `false` and a broken check is an `Err`, and from outside
    /// they had looked the same — so "this peer is not us", which is the
    /// answer that closes the connection, was also what a Win32 call failing
    /// for some unrelated reason produced. On Unix neither can really happen;
    /// this is Windows' half, and it is the half nothing had run.
    pub(super) fn peer_check(stream: &Stream) -> io::Result<bool> {
        if !stream.server {
            return Err(io::Error::other("not the server end of a pipe"));
        }
        // SAFETY: a live server-side pipe handle whose client has connected.
        if unsafe { ImpersonateNamedPipeClient(stream.inner.as_raw_handle()) } == 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(e.kind(), format!("impersonate: {e}")));
        }
        let same = (|| -> io::Result<bool> {
            let peer = Token::thread()
                .map_err(|e| io::Error::new(e.kind(), format!("the client's token: {e}")))?
                .user()
                .map_err(|e| io::Error::new(e.kind(), format!("the client's user: {e}")))?;
            let us = Token::process()
                .map_err(|e| io::Error::new(e.kind(), format!("our token: {e}")))?
                .user()
                .map_err(|e| io::Error::new(e.kind(), format!("our user: {e}")))?;
            Ok(unsafe { EqualSid(peer.sid(), us.sid()) != 0 })
        })();
        // Never let a failed identity check leave the accept thread running as
        // the client. A failed revert is therefore also a refusal.
        if unsafe { RevertToSelf() } == 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(e.kind(), format!("revert: {e}")));
        }
        same
    }

    pub(super) fn peer_is_us(stream: &Stream) -> bool {
        peer_check(stream).unwrap_or(false)
    }

    pub(crate) fn list(pattern: &Path) -> io::Result<Vec<PathBuf>> {
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES};
        use windows_sys::Win32::Storage::FileSystem::{
            FindClose, FindFirstFileW, FindNextFileW, WIN32_FIND_DATAW,
        };

        let wide_pattern = wide(pattern);
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        let find = unsafe { FindFirstFileW(wide_pattern.as_ptr(), &mut data) };
        if find == INVALID_HANDLE_VALUE {
            let e = io::Error::last_os_error();
            // A pattern that matches nothing is not an error, and the pipe
            // namespace does not spell it the way a directory does: an empty
            // `\\.\pipe` answers `ERROR_NO_MORE_FILES` from *FindFirstFile*,
            // where a real directory answers `ERROR_FILE_NOT_FOUND`. Taking
            // only the second turns "no window is listening" — the ordinary
            // state of a machine with no terminal open — into a hard failure
            // of every client that has to look.
            if matches!(
                e.raw_os_error(),
                Some(x) if x == ERROR_FILE_NOT_FOUND as i32 || x == ERROR_NO_MORE_FILES as i32
            ) {
                return Ok(Vec::new());
            }
            return Err(e);
        }
        let mut out = Vec::new();
        loop {
            let len = data
                .cFileName
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(data.cFileName.len());
            let name = String::from_utf16_lossy(&data.cFileName[..len]);
            out.push(PathBuf::from(r"\\.\pipe").join(name));
            if unsafe { FindNextFileW(find, &mut data) } == 0 {
                let e = io::Error::last_os_error();
                unsafe { FindClose(find) };
                return if e.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    Ok(out)
                } else {
                    Err(e)
                };
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use windows::list as list_named_pipes;
