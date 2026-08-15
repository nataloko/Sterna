//! Reaching a host through a proxy — HTTP `CONNECT`, SOCKS4/4a, SOCKS5, and
//! the prompt-driven "telnet proxy" a terminal server puts in front of a line.
//!
//! Ported from Tera Term's `TTProxy/ProxyWSockHook.h`, which is a plugin that
//! **hooks Winsock**: it replaces `connect`, `gethostbyname`,
//! `WSAAsyncGetHostByName`, `WSAAsyncGetAddrInfo`, `send`, `recv` and four
//! more, so that the terminal below it goes on believing it dialled the host
//! directly. That shape is why the file is 2,155 lines for four protocols
//! worth about three hundred: most of it is remembering, behind the API, the
//! host name the terminal asked for — because by the time `connect(2)` runs
//! the name is already a `sockaddr` and the name is what a proxy needs.
//!
//! None of that survives here. The transports call [`dial`] and are handed a
//! connected socket, so the name never has to be recovered from an address it
//! was turned into. What is ported is the wire behaviour of the four relays,
//! byte for byte, including the parts that are only true of Tera Term.
//!
//! # The settings are a section of their own
//!
//! `[TTProxy]` in the same INI file, because the plugin hooks `ReadIniFile`
//! rather than adding keys to `[Tera Term]` (`TTProxy.h:63`). `ProxyType`,
//! `ProxyHost`, `ProxyPort`, `ProxyUser`, `ProxyPass`, `ConnectionTimeout`,
//! `SocksResolve`, the five `Telnet*` prompt strings and `DebugLog`.
//!
//! # There is no other way to see a handshake fail
//!
//! Everything above happens before the terminal has a session, so a refusal
//! reaches the user as one sentence in a message box and nothing else — no
//! screen, no session log, and a transport that never opened. `DebugLog` is
//! upstream's answer to that and [`Trace`] is this one: every byte of the
//! handshake, in the same format, so a trace taken here can be read beside one
//! taken from Tera Term against the same proxy.
//!
//! # What is deliberately not reproduced
//!
//! Four things, and each one is an upstream defect rather than a behaviour.
//! They are listed in `docs/upstream-bugs.md`; the
//! short form is here because this is the file somebody comparing the two
//! implementations will have open.
//!
//! - **An absent `ProxyPort` disables the relay entirely.**
//!   `ProxyInfo::getPort()` (`:442`) supplies 1080, 23 or 80 by type and the
//!   connect hook uses it for the *address* (`:1770`) — and then the guard
//!   that decides whether to speak the protocol at all tests the raw stored
//!   port instead (`:1792`). So a proxy whose port box was left blank is
//!   dialled correctly and then never spoken to, and the terminal talks
//!   telnet or SSH straight at a SOCKS server. [`ProxyParams::port`] is the
//!   default and it is used for both.
//! - **A username with no password crashes an HTTP proxy connection.** The
//!   dialog stores an empty password field as NULL (`:1013`), `_save` then
//!   deletes the key, and `begin_relay_http` reaches `strlen(proxy.pass)`
//!   under a test of `user` alone (`:1275`). Here the password is an
//!   `Option` and an absent one is an empty string, which is what Basic
//!   authentication means by it.
//! - **A short read is treated as a full one.** `recieveFromSocket` (`:1193`)
//!   is one `recv` whose own comment says the count may be less than asked
//!   for, and every SOCKS caller checks only for the error. A SOCKS4 reply
//!   split across two segments therefore has its result byte read out of
//!   uninitialised stack, which can read as 90 — granted — on a connection
//!   the proxy refused. `Wire::recv_exact` reads all of it.
//! - **`ProxyType=http+ssl` and its four siblings parse and then do
//!   nothing.** They are in the type table (`:139`) and the relay `switch`
//!   has no arm for them, so they fall to `default: result = 0` (`:1822`) —
//!   success, with no handshake — because `SSLSocket.h` and `SSLLIB.h` sit in
//!   the tree included by nothing and listed in no build. They are left out
//!   of the schema's spellings, so they take the unrecognised-value arm that
//!   every other value upstream does not know takes.
//!
//! One thing that *is* reproduced and looks like a defect: an unrecognised
//! `ProxyType` is no proxy at all (`parseType` returns `TYPE_NONE`), so a
//! typo is a direct connection rather than a refusal. That is the schema's
//! ordinary rule for an enumerated setting and it is upstream's; it is also
//! the one place in this file where the rule has a cost worth naming, which
//! is why `none` exists as a spelling that says so on purpose.

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use data_encoding::BASE64;

use crate::error::{Error, Result};

/// Which protocol the proxy speaks.
///
/// The spellings are `ProxyType`'s, and `socks` is `socks5` — upstream's table
/// has both (`ProxyWSockHook.h:139`). The `+ssl` spellings are deliberately
/// absent; see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyKind {
    /// `none`, and the value of an unrecognised `ProxyType`. Upstream keeps
    /// these apart as `TYPE_NONE_FORCE` and `TYPE_NONE` because a per-host
    /// `none://` URL has to override a configured default; with one setting
    /// and no URL table there is nothing to override.
    #[default]
    None,
    /// `http` — RFC 7231 `CONNECT`, with optional Basic authentication.
    Http,
    /// `telnet` — no protocol at all. Five configurable prompts, answered as
    /// a person would.
    Telnet,
    /// `socks4`, which becomes 4a when the name is not resolved locally.
    Socks4,
    /// `socks5`/`socks`, with optional username/password authentication.
    Socks5,
}

impl ProxyKind {
    /// `ProxyInfo::getPort()`'s table (`ProxyWSockHook.h:442`).
    pub fn default_port(self) -> u16 {
        match self {
            ProxyKind::Socks4 | ProxyKind::Socks5 => 1080,
            ProxyKind::Telnet => 23,
            ProxyKind::Http => 80,
            ProxyKind::None => 0,
        }
    }
}

/// `SocksResolve` — who turns the host name into an address.
///
/// It is read as a plain string comparison with an `else` that is `auto`
/// (`ProxyWSockHook.h:1990`), so a misspelling is `auto` rather than an error.
/// Only the two SOCKS relays consult it: HTTP and the telnet proxy send the
/// name as text and have no choice to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolve {
    /// Resolve here, and fall back to sending the name if that fails. The
    /// shipped answer.
    #[default]
    Auto,
    /// Resolve here, and fail if that does not work. Names never leave the
    /// machine.
    Local,
    /// Never resolve here — send the name and let the proxy do it. This is
    /// what people mean by "so the proxy sees the DNS", and for SOCKS4 it is
    /// what makes the request a 4a one.
    Remote,
}

/// The five strings the telnet proxy relay watches for and answers.
///
/// Each is matched as a **substring of one line**, which is upstream's
/// `wait_for_prompt` (`ProxyWSockHook.h:1228`): it reads until a newline and
/// then looks for each of the five in what it has, so a prompt split across
/// two lines is never found and a line matching two prompts takes the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelnetPrompts {
    /// Answered with `host:port`, or `[host]:port` for an IPv6 literal.
    pub hostname: String,
    /// Answered with `ProxyUser`.
    pub username: String,
    /// Answered with `ProxyPass`.
    pub password: String,
    /// Ends the relay successfully.
    pub connected: String,
    /// Ends it as a refusal.
    pub error: String,
}

impl Default for TelnetPrompts {
    /// `ProxyWSockHook.h:1999`. The leading `>> ` and the trailing space in
    /// the first are upstream's and are part of the string.
    fn default() -> TelnetPrompts {
        TelnetPrompts {
            hostname: ">> Host name: ".into(),
            username: "Username:".into(),
            password: "Password:".into(),
            connected: "-- Connected to ".into(),
            error: "!!!!!!!!".into(),
        }
    }
}

/// Everything `[TTProxy]` holds, as the relays need it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProxyParams {
    pub kind: ProxyKind,
    /// `ProxyHost`. Empty means no proxy however `kind` is set, which is
    /// upstream's `_load` refusing a type it has no host for (`:1977`).
    pub host: String,
    /// `ProxyPort`, and **0 means the type's default** rather than no relay.
    /// See the module documentation.
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub resolve: Resolve,
    /// `ConnectionTimeout`, in seconds, default 10. Zero is upstream's "wait
    /// forever" — it passes a null `timeval` to `select` — and is kept, since
    /// the handshake runs on a connect that the frontend already treats as
    /// blocking.
    pub timeout: Duration,
    pub prompts: TelnetPrompts,
    /// `DebugLog` — where to append a transcript of the handshake, or `None`
    /// for no transcript. See [`Trace`], which is what this opens.
    ///
    /// It must already be absolute: upstream resolves a relative name against
    /// the *program's* log directory (`TTProxy.h:198` hands the `Logger`
    /// `ts.LogDirW`), and that directory is a settings question rather than a
    /// transport one — `tt_session::logname::program_log_dir` answers it.
    pub debug_log: Option<PathBuf>,
}

impl ProxyParams {
    /// The port to dial, resolving 0 to the type's default.
    pub fn port(&self) -> u16 {
        if self.port != 0 {
            self.port
        } else {
            self.kind.default_port()
        }
    }

    /// Whether this describes a proxy that will actually be used.
    ///
    /// A type with no host is not one — `_load` demotes it to `TYPE_NONE`
    /// (`ProxyWSockHook.h:1977`) rather than trying to dial an empty name.
    pub fn is_active(&self) -> bool {
        self.kind != ProxyKind::None && !self.host.is_empty()
    }
}

// ---------------------------------------------------------------------------
// The handshake transcript
// ---------------------------------------------------------------------------

/// `DebugLog` — every byte of the handshake, in upstream's `Logger` format.
///
/// `TTProxy/Logger.h` writes two kinds of record and nothing else, each one a
/// line ending in CRLF:
///
/// ```text
/// send: [ 05 01 00 ]
/// recv: "HTTP/1.1 200 Connection established\r\n"
/// ```
///
/// Binary for the two SOCKS relays, quoted text for HTTP and the telnet proxy,
/// which is the division upstream draws by calling `sendToSocket` in one and
/// `sendToSocketFormat` in the other. The text form escapes `\n`, `\r`, `\t`,
/// `\` and `"` and passes every other byte through, so an escape sequence in a
/// terminal server's banner reaches the file raw — as it does upstream.
///
/// Three things about the file, all upstream's:
///
/// - **It is appended to, never truncated**, so a trace holds every attempt
///   since it was last deleted. There is no record between one handshake and
///   the next, which is a real cost when reading one and is what the file
///   looks like in Tera Term; a delimiter here would be a line no Tera Term
///   writes, in a file whose whole purpose is to be compared against one.
/// - **The credentials are in it.** A `Proxy-Authorization` header is Base64,
///   which is not encryption, and SOCKS5's are in the clear. Upstream's are
///   too — the trace is a thing you turn on to send somebody, so it is worth
///   knowing what is in it before you do.
/// - **It cannot fail.** A path that will not open leaves the handshake
///   untraced and connecting normally, which is `Logger::open` keeping its
///   `INVALID_HANDLE_VALUE` and every write testing for it.
///
/// The one departure is *when* the file appears: upstream opens it while
/// reading the INI file, so the key alone creates an empty file in a session
/// that never connects. Here it is opened by the first handshake that has
/// something to write into it.
pub struct Trace {
    file: RefCell<File>,
}

impl Trace {
    /// Open `path` for appending, or answer `None` and say nothing.
    ///
    /// `None` is also what an empty path gives, which is the `DebugLog=` a
    /// dialog leaves behind when the box is cleared.
    pub fn open(path: &Path) -> Option<Trace> {
        if path.as_os_str().is_empty() {
            return None;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(|file| Trace {
                file: RefCell::new(file),
            })
    }

    /// `Logger::debuglog_binary` — `label: [ xx xx ]`.
    fn bytes(&self, label: &str, data: &[u8]) {
        let mut line = format!("{label}: [");
        for b in data {
            line.push_str(&format!(" {b:02x}"));
        }
        line.push_str(" ]\r\n");
        self.put(line.as_bytes());
    }

    /// `Logger::debuglog_string` — `label: "…"`, with five escapes.
    ///
    /// Upstream is handed a C string, so it is bytes rather than characters
    /// and anything not one of the five goes through as it stands. A UTF-8
    /// banner therefore reaches the file as its own bytes, which is what makes
    /// the two programs' traces comparable.
    fn text(&self, label: &str, data: &[u8]) {
        let mut line = format!("{label}: \"").into_bytes();
        for &b in data {
            match b {
                b'\n' => line.extend_from_slice(b"\\n"),
                b'\r' => line.extend_from_slice(b"\\r"),
                b'\t' => line.extend_from_slice(b"\\t"),
                b'\\' => line.extend_from_slice(b"\\\\"),
                b'"' => line.extend_from_slice(b"\\\""),
                other => line.push(other),
            }
        }
        line.extend_from_slice(b"\"\r\n");
        self.put(&line);
    }

    /// A failed write is dropped rather than reported: a trace that breaks a
    /// connection is worse than no trace.
    fn put(&self, bytes: &[u8]) {
        let mut file = self.file.borrow_mut();
        let _ = file.write_all(bytes);
        let _ = file.flush();
    }
}

/// The socket plus the transcript, so that no relay can write a byte it did
/// not record or record one it did not write.
///
/// The alternative — a `trace` argument on each of the four I/O helpers — was
/// the same code with two ways to get it wrong. Each relay speaks in one of
/// the two record forms throughout, which is why the form is chosen by the
/// method rather than carried in the struct.
struct Wire<'a, S> {
    stream: &'a mut S,
    trace: Option<&'a Trace>,
}

impl<S: Read + Write> Wire<'_, S> {
    /// `sendToSocket` — one binary record, written before the send, so a send
    /// that then fails still leaves what it was trying to say.
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(t) = self.trace {
            t.bytes("send", bytes);
        }
        self.write_all(bytes)
    }

    /// `sendToSocketFormat` — one text record **per line**.
    ///
    /// Upstream calls it once per line and this builds the whole request
    /// before writing, so splitting here is what keeps the two files
    /// record-for-record identical: an HTTP `CONNECT` is four records in both,
    /// and a telnet proxy's answer is one.
    fn send_text(&mut self, text: &str) -> Result<()> {
        if let Some(t) = self.trace {
            for line in text.split_inclusive('\n') {
                t.text("send", line.as_bytes());
            }
        }
        self.write_all(text.as_bytes())
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.stream.write_all(bytes).map_err(proxy_io)?;
        self.stream.flush().map_err(proxy_io)
    }

    /// Fill `buf` completely, or fail.
    ///
    /// The difference from upstream's `recieveFromSocket`, which is one `recv`
    /// and may return fewer bytes than asked for while every caller behaves as
    /// if it did not. See the module documentation — and the trace shows it,
    /// because a record is written per underlying read exactly as upstream's
    /// is, so a reply that arrived in two segments is two records here and two
    /// records and a wrong answer there.
    fn recv_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut done = 0;
        while done < buf.len() {
            match self.stream.read(&mut buf[done..]) {
                Ok(0) => {
                    return Err(Error::Proxy(
                        "the proxy hung up in the middle of its reply".into(),
                    ))
                }
                Ok(n) => {
                    if let Some(t) = self.trace {
                        t.bytes("recv", &buf[done..done + n]);
                    }
                    done += n;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(proxy_io(e)),
            }
        }
        Ok(())
    }

    /// Upstream's `line_input` (`ProxyWSockHook.h:1201`): one byte at a time to
    /// the newline, which is the only way to read a line without consuming what
    /// comes after it.
    ///
    /// Its 1024-byte buffer is kept, and so is what it does when a line is
    /// longer: nothing. The line comes back truncated and the rest is read as
    /// the next one, which for a header the caller only compares against
    /// `"\r\n"` is the same answer.
    fn recv_line(&mut self) -> Result<String> {
        let mut out = Vec::with_capacity(64);
        let mut byte = [0u8; 1];
        while out.len() < 1023 {
            match self.stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    out.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(proxy_io(e)),
            }
        }
        // Upstream logs the assembled line rather than each byte, and only
        // once it has one: a read that ends the stream logs nothing.
        if let Some(t) = self.trace {
            if !out.is_empty() {
                t.text("recv", &out);
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

/// Connect to `host:port`, through `proxy` when there is one.
///
/// This is the one entry point: passing `None`, or a [`ProxyParams`] that is
/// not [`is_active`](ProxyParams::is_active), gives the ordinary direct
/// connection, so a transport has one call rather than a branch.
///
/// The returned socket has no read or write timeout set — the handshake's are
/// removed on the way out, because the caller's are different and on Windows
/// a cloned reader shares them.
pub fn dial(
    proxy: Option<&ProxyParams>,
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<TcpStream> {
    let params = match proxy {
        Some(p) if p.is_active() => p,
        _ => return tcp_connect(host, port, timeout),
    };

    let socket = tcp_connect(&params.host, params.port(), timeout).map_err(|e| {
        Error::Proxy(format!(
            "cannot reach the {} proxy at {}:{}: {e}",
            type_name(params.kind),
            params.host,
            params.port()
        ))
    })?;

    // Upstream's `select` bound on every send and receive of the handshake,
    // expressed as socket timeouts because there is no select here. Zero is
    // its "wait forever", which is a null `timeval` there and `None` here.
    let t = (!params.timeout.is_zero()).then_some(params.timeout);
    socket.set_read_timeout(t).map_err(Error::from_io)?;
    socket.set_write_timeout(t).map_err(Error::from_io)?;

    let mut stream = socket;
    handshake(&mut stream, params, host, port)?;

    stream.set_read_timeout(None).map_err(Error::from_io)?;
    stream.set_write_timeout(None).map_err(Error::from_io)?;
    Ok(stream)
}

/// Every address the name has, in turn.
///
/// The loop is what makes a dual-stack host work when only one family is
/// routable; a single connect to the first AAAA is how "it works from the
/// shell but not from the GUI" happens.
fn tcp_connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| Error::Ssh(format!("cannot resolve {host}: {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(Error::Ssh(format!("{host} resolved to no addresses")));
    }

    let mut last = None;
    for addr in &addrs {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(e) => last = Some(e),
        }
    }
    let e = last.expect("at least one address was tried");
    Err(Error::Ssh(format!("cannot connect to {host}:{port}: {e}")))
}

/// Speak the proxy's protocol on an already-connected stream.
///
/// Split out from [`dial`] so the relays can be driven from a byte buffer as
/// well as from a socket, which is how the wire format is asserted without a
/// server. `host`/`port` are the *real* destination.
///
/// [`ProxyParams::debug_log`] is opened here rather than in [`dial`], so a
/// caller driving a relay over anything else gets the transcript too.
pub fn handshake<S: Read + Write>(
    stream: &mut S,
    params: &ProxyParams,
    host: &str,
    port: u16,
) -> Result<()> {
    let trace = params.debug_log.as_deref().and_then(Trace::open);
    let wire = &mut Wire {
        stream,
        trace: trace.as_ref(),
    };
    match params.kind {
        ProxyKind::None => Ok(()),
        ProxyKind::Http => relay_http(wire, params, host, port),
        ProxyKind::Socks5 => relay_socks5(wire, params, host, port),
        ProxyKind::Socks4 => relay_socks4(wire, params, host, port),
        ProxyKind::Telnet => relay_telnet(wire, params, host, port),
    }
}

fn type_name(kind: ProxyKind) -> &'static str {
    match kind {
        ProxyKind::None => "none",
        ProxyKind::Http => "HTTP",
        ProxyKind::Telnet => "telnet",
        ProxyKind::Socks4 => "SOCKS4",
        ProxyKind::Socks5 => "SOCKS5",
    }
}

/// `host:port`, bracketing an IPv6 literal.
///
/// Upstream's test is `strchr(realhost, ':')` (`ProxyWSockHook.h:1269`) — a
/// colon anywhere means a literal, which is right because a host *name* can
/// not contain one.
fn authority(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

// ---------------------------------------------------------------------------
// HTTP CONNECT
// ---------------------------------------------------------------------------

fn relay_http<S: Read + Write>(
    wire: &mut Wire<'_, S>,
    params: &ProxyParams,
    host: &str,
    port: u16,
) -> Result<()> {
    let target = authority(host, port);
    let mut req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");

    // Upstream tests `user` alone and then reads `pass` unconditionally,
    // which is the NULL dereference in the module documentation. An absent
    // password is the empty string, which is what `user:` means to Basic.
    if let Some(user) = &params.user {
        let pass = params.pass.as_deref().unwrap_or("");
        let encoded = BASE64.encode(format!("{user}:{pass}").as_bytes());
        req.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    req.push_str("\r\n");
    wire.send_text(&req)?;

    let status = wire.recv_line()?;
    // `atoi(strchr(buf, ' '))` (`:1314`), which dereferences NULL when the
    // line has no space in it. The number is whatever follows the first one.
    let code = status
        .split_once(' ')
        .map(|(_, rest)| rest.trim_start())
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u16>().ok()
        })
        .ok_or_else(|| {
            Error::Proxy(format!(
                "the HTTP proxy answered something that is not a status line: {:?}",
                status.trim_end()
            ))
        })?;

    // Headers to the blank line. Upstream compares against `"\r\n"` exactly;
    // a bare LF is accepted here too, which costs nothing and is the
    // difference between working and timing out against a sloppy proxy.
    loop {
        let line = wire.recv_line()?;
        if line == "\r\n" || line == "\n" {
            break;
        }
        if line.is_empty() {
            return Err(Error::Proxy(
                "the HTTP proxy hung up in the middle of its headers".into(),
            ));
        }
    }

    if code != 200 {
        // Upstream's two messages, and its choice of which status codes get
        // which (`:1322`). Everything that is not an authentication failure
        // carries the code, because "prevented" without it is unactionable.
        return Err(Error::Proxy(match code {
            401 | 407 => "the HTTP proxy rejected the credentials".into(),
            _ => format!("the HTTP proxy refused the connection (status {code})"),
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SOCKS5 — RFC 1928
// ---------------------------------------------------------------------------

const SOCKS5_VERSION: u8 = 5;
const SOCKS5_REJECT: u8 = 0xFF;
const SOCKS5_CMD_CONNECT: u8 = 1;
const SOCKS5_ATYP_IPV4: u8 = 1;
const SOCKS5_ATYP_DOMAIN: u8 = 3;
const SOCKS5_ATYP_IPV6: u8 = 4;
const SOCKS5_AUTH_NOAUTH: u8 = 0;
const SOCKS5_AUTH_USERPASS: u8 = 2;
/// The username/password sub-negotiation's own version, RFC 1929.
const SOCKS5_AUTH_SUBNEGOVER: u8 = 1;

fn relay_socks5<S: Read + Write>(
    wire: &mut Wire<'_, S>,
    params: &ProxyParams,
    host: &str,
    port: u16,
) -> Result<()> {
    // The method list. Upstream offers username/password only when it has
    // **both** halves (`:1385`), so a half-configured proxy asks for no
    // authentication and is refused by the server rather than here.
    let mut hello = vec![SOCKS5_VERSION];
    match (&params.user, &params.pass) {
        (Some(_), Some(_)) => {
            hello.extend_from_slice(&[2, SOCKS5_AUTH_NOAUTH, SOCKS5_AUTH_USERPASS])
        }
        _ => hello.extend_from_slice(&[1, SOCKS5_AUTH_NOAUTH]),
    }
    wire.send_bytes(&hello)?;

    let mut reply = [0u8; 2];
    wire.recv_exact(&mut reply)?;
    if reply[0] != SOCKS5_VERSION || reply[1] == SOCKS5_REJECT {
        return Err(Error::Proxy(format!(
            "the SOCKS5 proxy accepted none of the offered authentication methods \
             (version {}, method {:#04x})",
            reply[0], reply[1]
        )));
    }

    match reply[1] {
        SOCKS5_AUTH_NOAUTH => {}
        SOCKS5_AUTH_USERPASS => {
            let user = params.user.as_deref().unwrap_or("");
            let pass = params.pass.as_deref().unwrap_or("");
            // One byte of length each, so neither can be longer than 255.
            // Upstream copies into a 256-byte stack buffer having checked
            // nothing; the refusal here is the same limit, said out loud.
            let (ulen, plen) = (user.len(), pass.len());
            if ulen > 255 || plen > 255 {
                return Err(Error::Proxy(
                    "a SOCKS5 username or password is longer than the protocol's 255 bytes".into(),
                ));
            }
            let mut auth = vec![SOCKS5_AUTH_SUBNEGOVER, ulen as u8];
            auth.extend_from_slice(user.as_bytes());
            auth.push(plen as u8);
            auth.extend_from_slice(pass.as_bytes());
            wire.send_bytes(&auth)?;

            let mut ok = [0u8; 2];
            wire.recv_exact(&mut ok)?;
            if ok[1] != 0 {
                return Err(Error::Proxy(
                    "the SOCKS5 proxy rejected the credentials".into(),
                ));
            }
        }
        other => {
            return Err(Error::Proxy(format!(
                "the SOCKS5 proxy chose authentication method {other:#04x}, which Sterna \
                 does not implement"
            )))
        }
    }

    let mut req = vec![SOCKS5_VERSION, SOCKS5_CMD_CONNECT, 0];
    match destination(params.resolve, host)? {
        Destination::V4(a) => {
            req.push(SOCKS5_ATYP_IPV4);
            req.extend_from_slice(&a.octets());
        }
        Destination::V6(a) => {
            req.push(SOCKS5_ATYP_IPV6);
            req.extend_from_slice(&a.octets());
        }
        Destination::Name(name) => {
            if name.len() > 255 {
                return Err(Error::Proxy(
                    "a SOCKS5 host name is longer than the protocol's 255 bytes".into(),
                ));
            }
            req.push(SOCKS5_ATYP_DOMAIN);
            req.push(name.len() as u8);
            req.extend_from_slice(name.as_bytes());
        }
    }
    req.push((port >> 8) as u8);
    req.push((port & 0xFF) as u8);
    wire.send_bytes(&req)?;

    let mut head = [0u8; 4];
    wire.recv_exact(&mut head)?;
    if head[0] != SOCKS5_VERSION || head[1] != 0 {
        return Err(Error::Proxy(format!(
            "{} (SOCKS5: VER {} REP {} ATYP {})",
            socks5_reason(head[1]),
            head[0],
            head[1],
            head[3]
        )));
    }
    // The bound address, which nothing here wants but which has to come off
    // the socket before the session's own bytes start.
    match head[3] {
        SOCKS5_ATYP_IPV4 => wire.recv_exact(&mut [0u8; 4 + 2])?,
        SOCKS5_ATYP_IPV6 => wire.recv_exact(&mut [0u8; 16 + 2])?,
        SOCKS5_ATYP_DOMAIN => {
            let mut len = [0u8; 1];
            wire.recv_exact(&mut len)?;
            let mut rest = vec![0u8; len[0] as usize + 2];
            wire.recv_exact(&mut rest)?;
        }
        other => {
            return Err(Error::Proxy(format!(
                "the SOCKS5 proxy replied with address type {other}, which is not one of \
                 the three"
            )))
        }
    }
    Ok(())
}

/// RFC 1928's reply codes, which upstream lists and then does not use — its
/// message is the same sentence for all of them with the numbers appended.
/// The numbers are still appended; the sentence now says which.
fn socks5_reason(rep: u8) -> &'static str {
    match rep {
        1 => "the SOCKS5 proxy failed",
        2 => "the SOCKS5 proxy's rules do not allow this connection",
        3 => "the SOCKS5 proxy cannot reach that network",
        4 => "the SOCKS5 proxy cannot reach that host",
        5 => "the host refused the connection",
        6 => "the SOCKS5 proxy timed out reaching the host",
        7 => "the SOCKS5 proxy does not support CONNECT",
        8 => "the SOCKS5 proxy does not support that address type",
        _ => "the SOCKS5 proxy refused the connection",
    }
}

// ---------------------------------------------------------------------------
// SOCKS4 and 4a
// ---------------------------------------------------------------------------

const SOCKS4_VERSION: u8 = 4;
const SOCKS4_CMD_CONNECT: u8 = 1;
const SOCKS4_REP_SUCCEEDED: u8 = 90;
const SOCKS4_REP_IDENT_FAIL: u8 = 92;
const SOCKS4_REP_USERID: u8 = 93;

fn relay_socks4<S: Read + Write>(
    wire: &mut Wire<'_, S>,
    params: &ProxyParams,
    host: &str,
    port: u16,
) -> Result<()> {
    let mut req = vec![
        SOCKS4_VERSION,
        SOCKS4_CMD_CONNECT,
        (port >> 8) as u8,
        (port & 0xFF) as u8,
    ];

    // SOCKS4 has no address type field, so an unresolved name is expressed by
    // an address of 0.0.0.x — the 4a extension — and the name goes after the
    // user ID. There is no IPv6 spelling at all.
    let name = match destination(params.resolve, host)? {
        Destination::V4(a) => {
            req.extend_from_slice(&a.octets());
            None
        }
        Destination::V6(_) => {
            return Err(Error::Proxy(format!(
                "{host} is IPv6 and SOCKS4 has no way to express one; use SOCKS5"
            )))
        }
        Destination::Name(name) => {
            req.extend_from_slice(&[0, 0, 0, 1]);
            Some(name)
        }
    };

    // The user ID, NUL-terminated, empty when there is none. It is not a
    // password: SOCKS4 authenticates by asking the client's identd.
    if let Some(user) = &params.user {
        req.extend_from_slice(user.as_bytes());
    }
    req.push(0);

    if let Some(name) = name {
        req.extend_from_slice(name.as_bytes());
        req.push(0);
    }
    wire.send_bytes(&req)?;

    let mut reply = [0u8; 8];
    wire.recv_exact(&mut reply)?;
    // `VN` is 0 in a reply, not 4 — the protocol's own asymmetry, and
    // upstream checks it.
    let complaint = if reply[0] != 0 {
        Some("the SOCKS4 proxy refused the connection")
    } else if reply[1] == SOCKS4_REP_IDENT_FAIL || reply[1] == SOCKS4_REP_USERID {
        Some("the SOCKS4 proxy rejected the user ID")
    } else if reply[1] != SOCKS4_REP_SUCCEEDED {
        Some("the SOCKS4 proxy refused the connection")
    } else {
        None
    };
    if let Some(what) = complaint {
        return Err(Error::Proxy(format!(
            "{what} (SOCKS4: VN {} CD {})",
            reply[0], reply[1]
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The telnet proxy
// ---------------------------------------------------------------------------

fn relay_telnet<S: Read + Write>(
    wire: &mut Wire<'_, S>,
    params: &ProxyParams,
    host: &str,
    port: u16,
) -> Result<()> {
    let p = &params.prompts;
    let table = [
        p.hostname.as_str(),
        p.username.as_str(),
        p.password.as_str(),
        p.connected.as_str(),
        p.error.as_str(),
    ];
    loop {
        match wait_for_prompt(wire, &table)? {
            0 => wire.send_text(&format!("{}\n", authority(host, port)))?,
            1 => wire.send_text(&format!("{}\n", params.user.as_deref().unwrap_or("")))?,
            2 => wire.send_text(&format!("{}\n", params.pass.as_deref().unwrap_or("")))?,
            3 => return Ok(()),
            _ => {
                return Err(Error::Proxy(
                    "the telnet proxy refused the connection".into(),
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

enum Destination {
    V4(Ipv4Addr),
    V6(std::net::Ipv6Addr),
    Name(String),
}

/// Decide what goes in the request's address field.
///
/// Upstream writes this twice, once per SOCKS version, and the two are not
/// quite the same shape: SOCKS5 calls `getaddrinfo` and takes whatever family
/// comes back, SOCKS4 tries `inet_addr` first and only then `gethostbyname`.
/// The observable rule is the same and it is this one.
fn destination(resolve: Resolve, host: &str) -> Result<Destination> {
    // A literal is a literal whatever the setting says — there is nothing to
    // resolve and nothing a proxy could do better.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(match ip {
            IpAddr::V4(a) => Destination::V4(a),
            IpAddr::V6(a) => Destination::V6(a),
        });
    }
    if resolve == Resolve::Remote {
        return Ok(Destination::Name(host.to_string()));
    }
    // Port 0 because only the address is wanted; `to_socket_addrs` needs one.
    match (host, 0u16).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(SocketAddr::V4(a)) => Ok(Destination::V4(*a.ip())),
            Some(SocketAddr::V6(a)) => Ok(Destination::V6(*a.ip())),
            None if resolve == Resolve::Local => Err(Error::Proxy(format!(
                "{host} resolved to no addresses, and SocksResolve=local forbids asking \
                 the proxy to resolve it"
            ))),
            None => Ok(Destination::Name(host.to_string())),
        },
        Err(e) if resolve == Resolve::Local => Err(Error::Proxy(format!(
            "cannot resolve {host}: {e}, and SocksResolve=local forbids asking the proxy \
             to resolve it"
        ))),
        Err(_) => Ok(Destination::Name(host.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Reading and writing
// ---------------------------------------------------------------------------

/// Read lines until one contains one of `prompts`, and answer with its index.
///
/// Upstream discards a line that matches nothing (`:1228`), so the search is
/// per line rather than over everything received. Reproduced: a proxy whose
/// banner wraps the prompt onto its own line still works, and one that splits
/// a prompt across two lines does not — which is upstream's behaviour and the
/// reason the five strings are configurable in the first place.
fn wait_for_prompt<S: Read + Write>(wire: &mut Wire<'_, S>, prompts: &[&str]) -> Result<usize> {
    loop {
        let line = wire.recv_line()?;
        if line.is_empty() {
            return Err(Error::Proxy(
                "the telnet proxy hung up before it said anything recognisable".into(),
            ));
        }
        if let Some(i) = prompts
            .iter()
            .position(|p| !p.is_empty() && line.contains(p))
        {
            return Ok(i);
        }
    }
}

/// A timed-out handshake is the proxy not answering, which is a different
/// thing to say from `io::Error`'s own words for it.
fn proxy_io(e: std::io::Error) -> Error {
    match e.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            Error::Proxy("the proxy stopped answering".into())
        }
        _ => Error::Proxy(format!("the connection to the proxy failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A stream that plays canned server bytes and records what was written,
    /// so the wire format can be asserted exactly.
    struct Mock {
        inbound: Cursor<Vec<u8>>,
        outbound: Vec<u8>,
    }

    impl Mock {
        fn new(server_says: &[u8]) -> Mock {
            Mock {
                inbound: Cursor::new(server_says.to_vec()),
                outbound: Vec::new(),
            }
        }
    }

    impl Read for Mock {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inbound.read(buf)
        }
    }

    impl Write for Mock {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.outbound.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn params(kind: ProxyKind) -> ProxyParams {
        ProxyParams {
            kind,
            host: "proxy.example".into(),
            timeout: Duration::from_secs(10),
            ..Default::default()
        }
    }

    #[test]
    fn default_ports_are_upstreams() {
        assert_eq!(ProxyKind::Socks4.default_port(), 1080);
        assert_eq!(ProxyKind::Socks5.default_port(), 1080);
        assert_eq!(ProxyKind::Telnet.default_port(), 23);
        assert_eq!(ProxyKind::Http.default_port(), 80);
    }

    /// The defect the module documentation names first: an absent port must
    /// give the type's default and still speak the protocol.
    #[test]
    fn an_absent_port_takes_the_default_rather_than_disabling_the_relay() {
        let mut p = params(ProxyKind::Socks5);
        p.port = 0;
        assert_eq!(p.port(), 1080);
        assert!(p.is_active());

        p.port = 3128;
        assert_eq!(p.port(), 3128);
    }

    #[test]
    fn a_type_with_no_host_is_not_a_proxy() {
        let mut p = params(ProxyKind::Http);
        p.host.clear();
        assert!(!p.is_active());
    }

    #[test]
    fn http_connect_is_byte_exact() {
        let mut s = Mock::new(b"HTTP/1.1 200 Connection established\r\nX-Via: p\r\n\r\n");
        handshake(&mut s, &params(ProxyKind::Http), "host.example", 22).unwrap();
        assert_eq!(
            String::from_utf8(s.outbound).unwrap(),
            "CONNECT host.example:22 HTTP/1.1\r\nHost: host.example:22\r\n\r\n"
        );
    }

    #[test]
    fn http_brackets_an_ipv6_literal() {
        let mut s = Mock::new(b"HTTP/1.1 200 OK\r\n\r\n");
        handshake(&mut s, &params(ProxyKind::Http), "2001:db8::1", 23).unwrap();
        let sent = String::from_utf8(s.outbound).unwrap();
        assert!(
            sent.starts_with("CONNECT [2001:db8::1]:23 HTTP/1.1\r\n"),
            "{sent}"
        );
        assert!(sent.contains("Host: [2001:db8::1]:23\r\n"), "{sent}");
    }

    /// Upstream reaches `strlen(NULL)` here. A missing password is the empty
    /// string, which is what `user:` means to Basic authentication.
    #[test]
    fn http_basic_auth_survives_a_missing_password() {
        let mut p = params(ProxyKind::Http);
        p.user = Some("bob".into());
        p.pass = None;
        let mut s = Mock::new(b"HTTP/1.1 200 OK\r\n\r\n");
        handshake(&mut s, &p, "host.example", 23).unwrap();
        let sent = String::from_utf8(s.outbound).unwrap();
        // base64("bob:")
        assert!(
            sent.contains("Proxy-Authorization: Basic Ym9iOg==\r\n"),
            "{sent}"
        );
    }

    #[test]
    fn http_basic_auth_encodes_both_halves() {
        let mut p = params(ProxyKind::Http);
        p.user = Some("Aladdin".into());
        p.pass = Some("open sesame".into());
        let mut s = Mock::new(b"HTTP/1.1 200 OK\r\n\r\n");
        handshake(&mut s, &p, "h", 23).unwrap();
        let sent = String::from_utf8(s.outbound).unwrap();
        assert!(
            sent.contains("Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==\r\n"),
            "{sent}"
        );
    }

    #[test]
    fn http_reports_the_status_it_was_refused_with() {
        let mut s = Mock::new(b"HTTP/1.1 403 Forbidden\r\n\r\n");
        let e = handshake(&mut s, &params(ProxyKind::Http), "h", 23).unwrap_err();
        assert!(format!("{e}").contains("403"), "{e}");

        let mut s = Mock::new(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n");
        let e = handshake(&mut s, &params(ProxyKind::Http), "h", 23).unwrap_err();
        assert!(format!("{e}").contains("credentials"), "{e}");
    }

    /// `atoi(strchr(buf, ' '))` faults on this line upstream.
    #[test]
    fn http_status_line_with_no_space_is_an_error_not_a_crash() {
        let mut s = Mock::new(b"garbage\r\n\r\n");
        let e = handshake(&mut s, &params(ProxyKind::Http), "h", 23).unwrap_err();
        assert!(format!("{e}").contains("status line"), "{e}");
    }

    #[test]
    fn socks5_no_auth_is_byte_exact() {
        // Method selection, then the connect reply with an IPv4 bound address.
        let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 127, 0, 0, 1, 0, 80]);
        handshake(&mut s, &params(ProxyKind::Socks5), "203.0.113.7", 22).unwrap();
        assert_eq!(
            s.outbound,
            vec![
                // VER, NMETHODS, NOAUTH
                5, 1, 0, //
                // VER, CONNECT, RSV, ATYP=IPv4, 203.0.113.7, port 22
                5, 1, 0, 1, 203, 0, 113, 7, 0, 22,
            ]
        );
    }

    #[test]
    fn socks5_offers_userpass_only_with_both_halves() {
        let mut p = params(ProxyKind::Socks5);
        p.user = Some("u".into());
        p.pass = None;
        let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &p, "203.0.113.7", 22).unwrap();
        assert_eq!(&s.outbound[..3], &[5, 1, 0]);

        p.pass = Some("p".into());
        let mut s = Mock::new(&[5, 2, 1, 0, 5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &p, "203.0.113.7", 22).unwrap();
        assert_eq!(&s.outbound[..4], &[5, 2, 0, 2]);
        // RFC 1929: version 1, then each half length-prefixed.
        assert_eq!(&s.outbound[4..9], &[1, 1, b'u', 1, b'p']);
    }

    #[test]
    fn socks5_sends_a_name_when_told_to_resolve_remotely() {
        let mut p = params(ProxyKind::Socks5);
        p.resolve = Resolve::Remote;
        let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &p, "host.example", 23).unwrap();
        assert_eq!(
            &s.outbound[3..],
            &[
                5,
                1,
                0,
                SOCKS5_ATYP_DOMAIN,
                12,
                b'h',
                b'o',
                b's',
                b't',
                b'.',
                b'e',
                b'x',
                b'a',
                b'm',
                b'p',
                b'l',
                b'e',
                0,
                23
            ]
        );
    }

    #[test]
    fn socks5_ipv6_literal_goes_out_as_one() {
        let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &params(ProxyKind::Socks5), "2001:db8::1", 22).unwrap();
        assert_eq!(s.outbound[3 + 3], SOCKS5_ATYP_IPV6);
        assert_eq!(
            &s.outbound[3 + 4..3 + 20],
            &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
        );
    }

    #[test]
    fn socks5_names_the_reply_code() {
        // REP 2, connection not allowed by ruleset.
        let mut s = Mock::new(&[5, 0, 5, 2, 0, 1, 0, 0, 0, 0, 0, 0]);
        let e = handshake(&mut s, &params(ProxyKind::Socks5), "203.0.113.7", 22).unwrap_err();
        let msg = format!("{e}");
        assert!(msg.contains("rules do not allow"), "{msg}");
        assert!(msg.contains("REP 2"), "{msg}");
    }

    #[test]
    fn socks5_rejected_methods_are_reported() {
        let mut s = Mock::new(&[5, 0xFF]);
        let e = handshake(&mut s, &params(ProxyKind::Socks5), "203.0.113.7", 22).unwrap_err();
        assert!(format!("{e}").contains("authentication methods"), "{e}");
    }

    /// A domain-name bound address in the reply has a length byte in front of
    /// it, and getting that wrong leaves bytes on the socket that the session
    /// then reads as terminal output.
    #[test]
    fn socks5_drains_a_domain_bound_address() {
        let mut s = Mock::new(&[
            5,
            0, // method selection
            5,
            0,
            0,
            SOCKS5_ATYP_DOMAIN,
            3,
            b'a',
            b'b',
            b'c',
            0,
            22,   // reply
            b'!', // the session's first byte
        ]);
        handshake(&mut s, &params(ProxyKind::Socks5), "203.0.113.7", 22).unwrap();
        let mut rest = Vec::new();
        s.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"!");
    }

    #[test]
    fn socks4_is_byte_exact() {
        let mut s = Mock::new(&[0, 90, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &params(ProxyKind::Socks4), "203.0.113.7", 22).unwrap();
        assert_eq!(s.outbound, vec![4, 1, 0, 22, 203, 0, 113, 7, 0]);
    }

    #[test]
    fn socks4_with_a_user_id() {
        let mut p = params(ProxyKind::Socks4);
        p.user = Some("bob".into());
        let mut s = Mock::new(&[0, 90, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &p, "203.0.113.7", 22).unwrap();
        assert_eq!(
            s.outbound,
            vec![4, 1, 0, 22, 203, 0, 113, 7, b'b', b'o', b'b', 0]
        );
    }

    /// 4a: the fake 0.0.0.1 address, then the name after the user ID.
    #[test]
    fn socks4a_when_the_name_is_left_to_the_proxy() {
        let mut p = params(ProxyKind::Socks4);
        p.resolve = Resolve::Remote;
        let mut s = Mock::new(&[0, 90, 0, 0, 0, 0, 0, 0]);
        handshake(&mut s, &p, "host.example", 23).unwrap();
        assert_eq!(
            s.outbound,
            vec![
                4, 1, 0, 23, 0, 0, 0, 1, 0, b'h', b'o', b's', b't', b'.', b'e', b'x', b'a', b'm',
                b'p', b'l', b'e', 0
            ]
        );
    }

    #[test]
    fn socks4_reports_the_code_it_was_refused_with() {
        let mut s = Mock::new(&[0, 91, 0, 0, 0, 0, 0, 0]);
        let e = handshake(&mut s, &params(ProxyKind::Socks4), "203.0.113.7", 22).unwrap_err();
        assert!(format!("{e}").contains("CD 91"), "{e}");

        let mut s = Mock::new(&[0, 93, 0, 0, 0, 0, 0, 0]);
        let e = handshake(&mut s, &params(ProxyKind::Socks4), "203.0.113.7", 22).unwrap_err();
        assert!(format!("{e}").contains("user ID"), "{e}");
    }

    #[test]
    fn socks4_refuses_ipv6_rather_than_sending_nonsense() {
        let mut s = Mock::new(&[0, 90, 0, 0, 0, 0, 0, 0]);
        let e = handshake(&mut s, &params(ProxyKind::Socks4), "2001:db8::1", 22).unwrap_err();
        assert!(format!("{e}").contains("SOCKS5"), "{e}");
    }

    /// The defect the module documentation names third. Upstream reads this
    /// reply with one `recv`, so a split one leaves `CD` uninitialised.
    #[test]
    fn a_reply_split_across_two_reads_is_still_read_whole() {
        /// Hands over one byte per `read`, which is what a segmented reply
        /// looks like from the socket's side.
        struct Dribble(Cursor<Vec<u8>>);
        impl Read for Dribble {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }
                self.0.read(&mut buf[..1])
            }
        }
        impl Write for Dribble {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // CD 91 — refused. Read one byte at a time, it must still be seen.
        let mut s = Dribble(Cursor::new(vec![0, 91, 0, 0, 0, 0, 0, 0]));
        let e = handshake(&mut s, &params(ProxyKind::Socks4), "203.0.113.7", 22).unwrap_err();
        assert!(format!("{e}").contains("CD 91"), "{e}");
    }

    #[test]
    fn telnet_proxy_answers_the_prompts_in_order() {
        let mut p = params(ProxyKind::Telnet);
        p.user = Some("bob".into());
        p.pass = Some("s3cret".into());
        let mut s = Mock::new(
            b"Terminal server 1.0\n\
              Username:\n\
              Password:\n\
              >> Host name: \n\
              -- Connected to host.example\n",
        );
        handshake(&mut s, &p, "host.example", 23).unwrap();
        assert_eq!(
            String::from_utf8(s.outbound).unwrap(),
            "bob\ns3cret\nhost.example:23\n"
        );
    }

    #[test]
    fn telnet_proxy_takes_the_error_string_as_a_refusal() {
        let mut s = Mock::new(b"banner\n!!!!!!!! no route\n");
        let e = handshake(&mut s, &params(ProxyKind::Telnet), "h", 23).unwrap_err();
        assert!(format!("{e}").contains("telnet proxy refused"), "{e}");
    }

    #[test]
    fn telnet_proxy_brackets_an_ipv6_literal() {
        let mut s = Mock::new(b">> Host name: \n-- Connected to x\n");
        handshake(&mut s, &params(ProxyKind::Telnet), "2001:db8::1", 22).unwrap();
        assert_eq!(String::from_utf8(s.outbound).unwrap(), "[2001:db8::1]:22\n");
    }

    #[test]
    fn a_proxy_that_hangs_up_mid_handshake_says_so() {
        let mut s = Mock::new(&[5]); // half a method-selection reply
        let e = handshake(&mut s, &params(ProxyKind::Socks5), "203.0.113.7", 22).unwrap_err();
        assert!(format!("{e}").contains("hung up"), "{e}");
    }

    #[test]
    fn a_literal_is_never_resolved_whatever_the_setting_says() {
        for r in [Resolve::Auto, Resolve::Local, Resolve::Remote] {
            match destination(r, "203.0.113.7").unwrap() {
                Destination::V4(a) => assert_eq!(a, Ipv4Addr::new(203, 0, 113, 7)),
                _ => panic!("a literal came back as something else under {r:?}"),
            }
        }
    }

    #[test]
    fn a_name_that_does_not_resolve_is_the_proxys_problem_unless_local() {
        let name = "no-such-host.invalid";
        match destination(Resolve::Auto, name).unwrap() {
            Destination::Name(n) => assert_eq!(n, name),
            _ => panic!("auto should fall back to sending the name"),
        }
        assert!(destination(Resolve::Local, name).is_err());
        match destination(Resolve::Remote, name).unwrap() {
            Destination::Name(n) => assert_eq!(n, name),
            _ => panic!("remote must never resolve"),
        }
    }

    // -----------------------------------------------------------------------
    // DebugLog
    // -----------------------------------------------------------------------

    /// Run a handshake with a trace switched on and hand back the file.
    fn traced(kind: ProxyKind, server_says: &[u8], edit: fn(&mut ProxyParams)) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy.log");
        let mut p = params(kind);
        p.debug_log = Some(path.clone());
        edit(&mut p);
        let mut s = Mock::new(server_says);
        let _ = handshake(&mut s, &p, "203.0.113.7", 22);
        std::fs::read_to_string(&path).expect("the trace was written")
    }

    /// The two SOCKS relays are `sendToSocket`/`recieveFromSocket` throughout,
    /// which is `Logger`'s binary record: `label: [ xx xx ]`, CRLF.
    #[test]
    fn a_socks5_trace_is_upstreams_binary_record() {
        let log = traced(
            ProxyKind::Socks5,
            &[5, 0, 5, 0, 0, 1, 127, 0, 0, 1, 0, 80],
            |_| {},
        );
        assert_eq!(
            log,
            "send: [ 05 01 00 ]\r\n\
             recv: [ 05 00 ]\r\n\
             send: [ 05 01 00 01 cb 00 71 07 00 16 ]\r\n\
             recv: [ 05 00 00 01 ]\r\n\
             recv: [ 7f 00 00 01 00 50 ]\r\n"
        );
    }

    /// HTTP is `sendToSocketFormat`/`line_input` throughout, which is the
    /// quoted-text record — and upstream sends the request a line at a time,
    /// so four records come out of what is one write here.
    #[test]
    fn an_http_trace_is_upstreams_text_record_one_line_at_a_time() {
        let log = traced(
            ProxyKind::Http,
            b"HTTP/1.1 200 Connection established\r\n\r\n",
            |p| p.user = Some("bob".into()),
        );
        assert_eq!(
            log,
            "send: \"CONNECT 203.0.113.7:22 HTTP/1.1\\r\\n\"\r\n\
             send: \"Host: 203.0.113.7:22\\r\\n\"\r\n\
             send: \"Proxy-Authorization: Basic Ym9iOg==\\r\\n\"\r\n\
             send: \"\\r\\n\"\r\n\
             recv: \"HTTP/1.1 200 Connection established\\r\\n\"\r\n\
             recv: \"\\r\\n\"\r\n"
        );
    }

    /// `Logger::debuglog_string`'s five escapes and nothing else: a byte that
    /// is not one of them goes through as it stands, including UTF-8 and
    /// including a control character a terminal server put in its banner.
    #[test]
    fn the_text_record_escapes_five_characters_and_passes_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("escapes.log");
        let trace = Trace::open(&path).expect("open");
        trace.text("recv", b"a\tb\\c\"d\x1b[0m\xc3\xa9\r\n");
        drop(trace);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"recv: \"a\\tb\\\\c\\\"d\x1b[0m\xc3\xa9\\r\\n\"\r\n"
        );
    }

    /// Upstream appends (`OPEN_ALWAYS` and a seek to the end), so a trace
    /// holds every attempt since it was last deleted rather than only the
    /// last one — which is the whole of what makes it useful for a proxy that
    /// fails intermittently.
    #[test]
    fn a_second_handshake_is_appended_rather_than_replacing_the_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("append.log");
        let mut p = params(ProxyKind::Socks5);
        p.debug_log = Some(path.clone());
        for _ in 0..2 {
            let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 127, 0, 0, 1, 0, 80]);
            handshake(&mut s, &p, "203.0.113.7", 22).unwrap();
        }
        let log = std::fs::read_to_string(&path).unwrap();
        assert_eq!(log.matches("send: [ 05 01 00 ]").count(), 2);
    }

    /// A trace that cannot be opened is `Logger::open` keeping its
    /// `INVALID_HANDLE_VALUE`: nothing is written and the connection is made
    /// anyway. A diagnostic that can break a session is not one.
    #[test]
    fn a_trace_that_cannot_be_opened_does_not_stop_the_handshake() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut p = params(ProxyKind::Socks5);
        // A directory that is not there, so the file cannot be created.
        p.debug_log = Some(dir.path().join("no/such/dir/proxy.log"));
        let mut s = Mock::new(&[5, 0, 5, 0, 0, 1, 127, 0, 0, 1, 0, 80]);
        handshake(&mut s, &p, "203.0.113.7", 22).expect("the handshake still runs");

        // And an empty path is the cleared dialog box, which is no trace
        // rather than a file called nothing.
        assert!(Trace::open(Path::new("")).is_none());
    }

    /// The record is written before the send, so the last thing a broken
    /// handshake tried to say is in the file — which is the case the trace
    /// exists for.
    #[test]
    fn a_refused_handshake_still_records_what_it_sent() {
        // CD 91: request rejected.
        let log = traced(ProxyKind::Socks4, &[0, 91, 0, 0, 0, 0, 0, 0], |_| {});
        assert!(
            log.starts_with("send: [ 04 01 00 16 cb 00 71 07 00 ]\r\n"),
            "{log}"
        );
        assert!(log.contains("recv: [ 00 5b 00 00 00 00 00 00 ]"), "{log}");
    }
}
