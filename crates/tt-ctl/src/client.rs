//! The other end, for the two binaries and for anything else in the tree.
//!
//! Deliberately small: a client is a socket, a line out and a line back, and
//! the point of choosing JSON-RPC over a Unix socket is that nobody *needs*
//! this type. `printf | nc -U` is a client. What this adds is the id
//! bookkeeping and turning an error object back into a Rust error, which is
//! the part a shell script does with `jq` and a Rust caller should not do by
//! hand.
//!
//! **A call is synchronous and the connection is not shared.** Requests are
//! answered in order on one connection, and every method here waits for its
//! own reply before returning — so an id is only ever outstanding one at a
//! time and matching them is an assertion rather than a table. A caller that
//! wants two things at once opens two connections, which costs a file
//! descriptor and no code.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use serde_json::Value;

use crate::proto::{RpcError, MAX_LINE};

/// What went wrong, from a caller's point of view.
#[derive(Debug)]
pub enum Error {
    /// The socket: not there, refused, hung up mid-request.
    Io(std::io::Error),
    /// The window answered, and the answer was no.
    Rpc(RpcError),
    /// The window answered with something that is not a response.
    Protocol(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Rpc(e) => match &e.data {
                Some(Value::String(d)) => write!(f, "{} ({})", e.message, d),
                _ => write!(f, "{}", e.message),
            },
            Error::Protocol(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One connection to one window.
pub struct Client {
    out: UnixStream,
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl Client {
    /// Connect to an explicit path.
    pub fn connect(path: &Path) -> Result<Client> {
        let out = UnixStream::connect(path)?;
        let reader = BufReader::new(out.try_clone()?);
        Ok(Client {
            out,
            reader,
            next_id: 1,
        })
    }

    /// Connect to whichever window `name` resolves to — see [`addr::resolve`],
    /// which is where "there are two and you did not say which" is refused.
    ///
    /// [`addr::resolve`]: crate::addr::resolve
    pub fn open(name: Option<&str>) -> Result<Client> {
        Client::connect(&crate::addr::resolve(name)?)
    }

    /// Call a method and return its result, or its error.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let reply = self.send(&req)?;
        let reply = reply.ok_or_else(|| Error::Protocol("no answer".into()))?;
        // Out of order can only mean a window answering somebody else's
        // question on this connection, which would be a bug on that side; say
        // so rather than returning it as if it were ours.
        match reply.get("id") {
            Some(Value::Number(n)) if n.as_u64() == Some(id) => {}
            other => {
                return Err(Error::Protocol(format!(
                    "answer to {other:?}, expected {id}"
                )))
            }
        }
        if let Some(e) = reply.get("error") {
            let e: RpcError = serde_json::from_value(e.clone())
                .map_err(|e| Error::Protocol(format!("bad error object: {e}")))?;
            return Err(Error::Rpc(e));
        }
        reply
            .get("result")
            .cloned()
            .ok_or_else(|| Error::Protocol("neither result nor error".into()))
    }

    /// Send a notification — no id, no answer, no way to know whether it
    /// worked. For `macro.stop` from something that is not waiting.
    pub fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.out
            .write_all(format!("{req}\n").as_bytes())
            .map_err(Error::Io)
    }

    /// Write a line as given and read whatever comes back.
    ///
    /// For `ttctl --raw`, and for the tests that are about what a malformed
    /// request does — which cannot be asked through [`call`](Client::call),
    /// since it only builds well-formed ones.
    pub fn raw(&mut self, line: &str) -> Result<Value> {
        self.out.write_all(line.as_bytes())?;
        if !line.ends_with('\n') {
            self.out.write_all(b"\n")?;
        }
        self.read_line()?
            .ok_or_else(|| Error::Protocol("no answer".into()))
    }

    fn send(&mut self, req: &Value) -> Result<Option<Value>> {
        self.out.write_all(format!("{req}\n").as_bytes())?;
        self.read_line()
    }

    fn read_line(&mut self) -> Result<Option<Value>> {
        let mut line = Vec::new();
        // The same ceiling the server applies, for the same reason and in the
        // other direction: a client should not be made to allocate by whatever
        // it connected to either.
        let n = (&mut self.reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the window hung up",
            )));
        }
        if n > MAX_LINE {
            return Err(Error::Protocol("answer too long".into()));
        }
        let v: Value =
            serde_json::from_slice(&line).map_err(|e| Error::Protocol(format!("not JSON: {e}")))?;
        Ok(Some(v))
    }
}
