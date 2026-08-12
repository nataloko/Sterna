//! The SSH transport: `russh` on a worker thread, behind the same synchronous
//! [`Transport`] every other connection presents.
//!
//! Two shapes had to be decided here, and `PLAN.md` deferred both until there
//! was a second transport to decide them against. There is one now.
//!
//! **Async lives inside `tt-conn`, not above it.** The terminal core, the C
//! ABI and the Qt shell are all synchronous, and a terminal needs nothing from
//! a connection but bytes. So the tokio runtime is a private implementation
//! detail of this module: one thread, one current-thread runtime, and a
//! self-pipe (`wakeup.rs`) on Unix so the frontend can wait on SSH exactly the
//! way it waits on a serial port. Windows has no pollable descriptor and keeps
//! the same state machine behind a polling boundary until the shell grows its
//! native-event notifier. The alternative — making the shell async — would
//! have spread `russh`'s runtime through three layers that have no use for it.
//!
//! **Connecting is a state machine the caller drives, not a callback.** SSH
//! asks questions: is this host key acceptable, what is the password, what
//! does the server's keyboard-interactive prompt want. A callback would have
//! to be `Send`, would run on the worker thread, and would leave a Qt frontend
//! blocking its own worker while it tried to raise a modal dialog from the
//! wrong thread. Instead [`SshConnect::poll`] returns the question, the caller
//! answers it whenever it likes, and the worker waits. The same drained-event
//! shape `tt-session` already uses, for the same reason.
//!
//! What is deliberately not here: RFC 4335 `break` (russh does not implement
//! the request, so [`send_break`](Transport::send_break) reports it as
//! unsupported rather than pretending), agent *forwarding*, port forwarding,
//! X11, and SSH-1 — the last of those permanently, per `PLAN.md`.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{load_secret_key, Algorithm, HashAlg, PrivateKeyWithHashAlg};
use russh::{cipher, client, kex, mac, ChannelMsg, MethodKind, Preferred, Pty};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;

use super::known_hosts::{HostKeyRef, KnownHosts, Verdict};
use super::wakeup::Wakeup;
use crate::error::{Error, Result};
use crate::transport::{Transport, TransportEvent};

/// How much unread output to hold before letting the SSH window close.
///
/// Backpressure has to exist somewhere: a `cat` of a large file arrives faster
/// than a terminal can paint it, and an unbounded queue turns that into
/// unbounded memory. Stopping at the mark leaves the bytes in the server's
/// hands, which is where a flow-controlled protocol is supposed to keep them.
const HIGH_WATER: usize = 1 << 20;

/// What to connect to, and what to try when the far end asks who we are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// `$TERM` for the remote side — `ts.TermType`, which TTSSH puts straight
    /// into the `pty-req` (`ssh.c:8593`) rather than keeping one of its own.
    ///
    /// The default here is `xterm-256color`, which matches what the engine
    /// actually implements; claiming more is how a remote `vim` ends up
    /// drawing sequences nothing here parses. **Upstream's own default is
    /// plain `xterm`** (`ttset.c:961`), and a session opened from settings
    /// gets that instead — this constant is for a caller with no file.
    pub term: String,
    pub cols: u16,
    pub rows: u16,
    /// Private keys to try, in order. Empty means the OpenSSH defaults —
    /// see [`default_identities`].
    pub identities: Vec<PathBuf>,
    /// Ask `ssh-agent` first. On a Linux desktop this is usually the only
    /// thing needed, and it is the difference between "my setup just works"
    /// and "configure it twice".
    pub use_agent: bool,
    /// Offer the pre-2020 algorithms as well.
    ///
    /// Off by default, and that is a deliberate cost. Spike 5's first finding
    /// was that russh keeps SHA-1 key exchange, CBC ciphers and `ssh-rsa`
    /// host keys out of its default preference list — correct posture, and
    /// also the reason a console server from 2012 will not answer. PuTTY and
    /// SecureCRT both solve this with a per-connection switch; this is ours.
    pub legacy: bool,
    /// Give up if the socket and key exchange have not finished in this long.
    /// Generous by default: spike 5 could not test it, but key exchange on an
    /// underpowered embedded CPU is a named risk in `PLAN.md`.
    pub connect_timeout: Duration,
    /// Send a keepalive after this much silence. `None` is OpenSSH's default
    /// (none at all); a NAT between here and a console server is the usual
    /// reason to want one.
    pub keepalive: Option<Duration>,
    pub known_hosts: KnownHosts,
    /// What to do about a host key that is not already trusted.
    pub host_key_policy: HostKeyPolicy,
    /// `[TTProxy]`, when the file configures one.
    ///
    /// TTSSH has no proxy of its own and needs none: `TTProxy` hooks Winsock
    /// underneath it, so an SSH session goes through a configured proxy
    /// upstream without either plugin knowing about the other. Here the two
    /// are in one process and the seam is explicit — the handshake happens on
    /// a blocking socket and russh is handed the connected stream.
    /// Boxed for the reason [`TelnetParams::proxy`] is.
    ///
    /// [`TelnetParams::proxy`]: crate::telnet::TelnetParams::proxy
    pub proxy: Option<Box<crate::proxy::ProxyParams>>,
}

/// `StrictHostKeyChecking`, as a decision the transport can take on its own.
///
/// This lives here rather than in the frontend because the frontend would
/// otherwise have to reimplement it — and because `Strict` has to *refuse*
/// before the prompt is raised, not after the user answers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Ask about anything not already trusted. What a GUI does, and OpenSSH's
    /// `ask`.
    #[default]
    Ask,
    /// `accept-new`: record a first-seen host without asking, and refuse a
    /// changed one without asking either. OpenSSH's default since 8.5.
    AcceptNew,
    /// `yes`: refuse anything not already in the files. Never prompts.
    Strict,
    /// `no`: connect to anything. A new host is still recorded; a **changed**
    /// key is accepted and *not* recorded, because overwriting the old entry
    /// would destroy the only evidence that it changed.
    AcceptAny,
}

impl SshParams {
    pub fn new(host: impl Into<String>, port: u16, user: impl Into<String>) -> SshParams {
        SshParams {
            host: host.into(),
            port,
            user: user.into(),
            term: "xterm-256color".to_string(),
            cols: 80,
            rows: 24,
            identities: Vec::new(),
            use_agent: true,
            legacy: false,
            connect_timeout: Duration::from_secs(30),
            keepalive: None,
            known_hosts: KnownHosts::user_default(),
            host_key_policy: HostKeyPolicy::Ask,
            proxy: None,
        }
    }

    /// Build the parameters `~/.ssh/config` already describes for `alias`.
    ///
    /// This is the adoption lever in one function: a user who can type
    /// `ssh myrouter` gets the same connection here without re-entering the
    /// user, the port, the key or the fact that it is old equipment.
    /// `user` and `port` override the file, because something typed into a
    /// dialog is more specific than a pattern in a file.
    ///
    /// One deliberate simplification: `IdentitiesOnly yes` turns the agent
    /// off entirely, where OpenSSH still lets the agent hold a *listed* key.
    /// The narrower reading costs a user with an agent-held listed key one
    /// passphrase prompt; the wider one would offer keys they said not to.
    pub fn from_config(
        config: &super::config::SshConfig,
        alias: &str,
        user: Option<&str>,
        port: Option<u16>,
    ) -> SshParams {
        use super::config::StrictHostKeyChecking as S;

        let r = config.resolve(alias, user);
        let mut p = SshParams::new(
            r.host_name.clone(),
            port.or(r.port).unwrap_or(22),
            r.user
                .clone()
                .or_else(|| std::env::var("USER").ok())
                .unwrap_or_default(),
        );
        p.identities = r.identity_files.clone();
        p.use_agent = r.use_agent && !r.identities_only;
        p.legacy = r.legacy;
        if let Some(t) = r.connect_timeout {
            p.connect_timeout = t;
        }
        p.keepalive = r.server_alive_interval;
        if !r.user_known_hosts_files.is_empty() {
            p.known_hosts = KnownHosts::with_files(r.user_known_hosts_files.clone());
        }
        p.known_hosts = p.known_hosts.hashing(r.hash_known_hosts);
        p.host_key_policy = match r.strict_host_key_checking {
            Some(S::Yes) => HostKeyPolicy::Strict,
            Some(S::AcceptNew) => HostKeyPolicy::AcceptNew,
            Some(S::No) => HostKeyPolicy::AcceptAny,
            Some(S::Ask) | None => HostKeyPolicy::Ask,
        };
        p
    }

    fn describe(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

/// `~/.ssh/id_*`, strongest first.
///
/// OpenSSH's own order starts at `id_rsa` for historical reasons; there is no
/// value in reproducing that, because trying a stronger key first costs one
/// round trip at most and every server that takes `id_rsa` takes Ed25519 too.
pub fn default_identities() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|n| ssh.join(n))
        .filter(|p| p.exists())
        .collect()
}

/// The far end's host key, and what the `known_hosts` files made of it.
#[derive(Clone, Debug)]
pub struct HostKeyPrompt {
    pub host: String,
    pub port: u16,
    /// The key's own type name, as `known_hosts` records it — `ssh-ed25519`,
    /// `ssh-rsa`. **Not** the negotiated signature algorithm.
    pub algorithm: String,
    /// `SHA256:…`, the form every other client prints.
    pub fingerprint: String,
    pub verdict: Verdict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Connect, and write the key down so the next connection is silent.
    AcceptAndSave,
    /// Connect this once and record nothing. What "yes, but I am on a strange
    /// network" means.
    AcceptOnce,
    Refuse,
}

/// One line of something the user has to type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    /// Whether to show what is typed. The server chooses; a keyboard-
    /// interactive challenge may legitimately want an echoed answer.
    pub echo: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthPromptKind {
    Password,
    KeyboardInteractive,
    /// The passphrase for a private key file. Ours to ask, not the server's,
    /// and the only prompt that names a local path.
    Passphrase(PathBuf),
}

/// A question that has to reach the user before authentication can continue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthPrompt {
    pub kind: AuthPromptKind,
    /// The server's own wording, where it sent any. Empty is normal.
    pub name: String,
    pub instruction: String,
    pub prompts: Vec<Prompt>,
}

/// What the connection wants next.
pub enum Step {
    /// Nothing to do. Wait for the descriptor to become readable again.
    Working,
    HostKey(HostKeyPrompt),
    Auth(AuthPrompt),
    /// Authenticated, the pty is allocated and the shell is running.
    Ready(SshConn),
    Failed(Error),
}

// ---------------------------------------------------------------------------
// Shared state between the worker thread and the caller.
// ---------------------------------------------------------------------------

enum Pending {
    HostKey(HostKeyPrompt),
    Auth(AuthPrompt),
}

enum Phase {
    Connecting,
    Running,
    /// Over. `Some` when there is something to say about why.
    Ended(Option<Error>),
}

struct State {
    inbound: VecDeque<u8>,
    pending: Option<Pending>,
    phase: Phase,
}

struct Shared {
    wake: Wakeup,
    state: Mutex<State>,
    /// Told when the reader has taken enough for the worker to resume.
    drained: Notify,
    /// Set when the caller has gone away, so the worker stops asking
    /// questions nobody will answer.
    cancelled: AtomicBool,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock here means a panic while holding it, which this
        // module has no path to. Recovering the guard is still better than
        // taking the whole terminal down with it.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, bytes: &[u8]) {
        self.lock().inbound.extend(bytes);
        self.wake.signal();
    }

    fn prompt(&self, p: Pending) {
        self.lock().pending = Some(p);
        self.wake.signal();
    }

    fn set_phase(&self, phase: Phase) {
        self.lock().phase = phase;
        self.wake.signal();
    }

    fn buffered(&self) -> usize {
        self.lock().inbound.len()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

enum Cmd {
    Data(Vec<u8>),
    Resize(u16, u16),
    Disconnect,
}

// ---------------------------------------------------------------------------
// The caller's side.
// ---------------------------------------------------------------------------

/// A connection being set up. Poll it; answer what it asks.
pub struct SshConnect {
    shared: Arc<Shared>,
    cmd: UnboundedSender<Cmd>,
    host_key: UnboundedSender<HostKeyDecision>,
    answers: UnboundedSender<Vec<String>>,
    describe: String,
    spent: bool,
}

impl SshConnect {
    /// Start connecting. Returns immediately; nothing has happened yet.
    pub fn start(params: SshParams) -> Result<SshConnect> {
        let shared = Arc::new(Shared {
            wake: Wakeup::new()?,
            state: Mutex::new(State {
                inbound: VecDeque::new(),
                pending: None,
                phase: Phase::Connecting,
            }),
            drained: Notify::new(),
            cancelled: AtomicBool::new(false),
        });
        let (cmd, cmd_rx) = unbounded_channel();
        let (host_key, host_key_rx) = unbounded_channel();
        let (answers, answers_rx) = unbounded_channel();
        let describe = params.describe();

        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("tt-ssh".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        worker_shared
                            .set_phase(Phase::Ended(Some(Error::Ssh(format!("no runtime: {e}")))));
                        return;
                    }
                };
                rt.block_on(run(
                    params,
                    Arc::clone(&worker_shared),
                    cmd_rx,
                    host_key_rx,
                    answers_rx,
                ));
            })
            .map_err(|e| Error::Ssh(format!("cannot start the SSH thread: {e}")))?;

        Ok(SshConnect {
            shared,
            cmd,
            host_key,
            answers,
            describe,
            spent: false,
        })
    }

    /// The descriptor to wait on. **The same one the resulting [`SshConn`]
    /// hands out**, so a frontend registers its notifier once and keeps it
    /// across the handover rather than swapping it at the moment the session
    /// starts producing output.
    #[cfg(unix)]
    pub fn poll_fd(&self) -> std::os::unix::io::RawFd {
        self.shared.wake.fd()
    }

    /// The Windows event to wait on. The resulting [`SshConn`] borrows the
    /// same event, so a frontend can keep one notifier across the handover.
    #[cfg(windows)]
    pub fn wait_handle(&self) -> std::os::windows::io::RawHandle {
        self.shared.wake.handle()
    }

    /// What the connection needs next. Never blocks.
    ///
    /// A spent handle — one that has already yielded [`Step::Ready`] or
    /// [`Step::Failed`] — reports `Working` forever and should be dropped.
    pub fn poll(&mut self) -> Step {
        if self.spent {
            return Step::Working;
        }
        self.shared.wake.drain();
        let mut state = self.shared.lock();
        if let Some(p) = state.pending.take() {
            return match p {
                Pending::HostKey(p) => Step::HostKey(p),
                Pending::Auth(p) => Step::Auth(p),
            };
        }
        match std::mem::replace(&mut state.phase, Phase::Connecting) {
            Phase::Connecting => Step::Working,
            Phase::Running => {
                state.phase = Phase::Running;
                drop(state);
                self.spent = true;
                Step::Ready(SshConn {
                    shared: Arc::clone(&self.shared),
                    cmd: self.cmd.clone(),
                    describe: self.describe.clone(),
                })
            }
            Phase::Ended(e) => {
                state.phase = Phase::Ended(None);
                drop(state);
                self.spent = true;
                Step::Failed(e.unwrap_or_else(|| {
                    Error::Ssh("the connection closed before the session started".into())
                }))
            }
        }
    }

    /// Answer a [`Step::HostKey`].
    pub fn answer_host_key(&self, decision: HostKeyDecision) {
        let _ = self.host_key.send(decision);
    }

    /// Answer a [`Step::Auth`], one string per [`Prompt`] in the order asked.
    pub fn answer_auth(&self, answers: Vec<String>) {
        let _ = self.answers.send(answers);
    }

    pub fn describe(&self) -> &str {
        &self.describe
    }
}

impl Drop for SshConnect {
    fn drop(&mut self) {
        if !self.spent {
            // The worker may be inside a key exchange that cannot be
            // interrupted; it will notice at its next await and unwind. The
            // thread outlives this handle by at most `connect_timeout`, and
            // holds nothing the frontend can see.
            self.shared.cancelled.store(true, Ordering::Relaxed);
            let _ = self.cmd.send(Cmd::Disconnect);
        }
    }
}

/// A running SSH session, as a byte stream.
pub struct SshConn {
    shared: Arc<Shared>,
    cmd: UnboundedSender<Cmd>,
    describe: String,
}

impl Transport for SshConn {
    fn read(&mut self, data: &mut Vec<u8>, _events: &mut Vec<TransportEvent>) -> Result<usize> {
        // Drain the pipe *before* looking at the buffer. The other order loses
        // a wakeup: a byte arriving in between would have its poke thrown away
        // along with the ones already handled, and the frontend would sleep on
        // data that had already arrived.
        self.shared.wake.drain();

        let mut state = self.shared.lock();
        let n = state.inbound.len();
        if n > 0 {
            data.extend(state.inbound.drain(..));
            drop(state);
            // The worker stops pulling from the channel above HIGH_WATER;
            // this is what starts it again.
            self.shared.drained.notify_one();
            return Ok(n);
        }
        match &mut state.phase {
            // Report the reason once, then behave like any other hang-up. The
            // session drops the transport on `Disconnected` and would never
            // ask again, but a caller that keeps it must not get the same
            // error forever.
            Phase::Ended(e @ Some(_)) => Err(e.take().unwrap()),
            Phase::Ended(None) => Err(Error::Disconnected),
            _ => Ok(0),
        }
    }

    /// Queues `data` and reports all of it written.
    ///
    /// The timeout is unused, and that is not a shortcut: what backs up on an
    /// SSH connection is the *server's* receive window, which is metered
    /// inside `russh`, and there is nothing useful for a caller to do with a
    /// short write of a keystroke. Everything the terminal sends is small —
    /// keys, a mouse report, a paste — and the queue is the transport's.
    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<usize> {
        if self.cmd.send(Cmd::Data(data.to_vec())).is_err() {
            return Err(Error::Disconnected);
        }
        Ok(data.len())
    }

    /// Not available over SSH.
    ///
    /// RFC 4335 defines a `break` channel request and `russh` does not
    /// implement it — there is no `send_break` on a channel and no message
    /// for it. Returning `Ok(())` would be worse than saying so: on a console
    /// server reached over SSH, a break is a real function with a real
    /// consequence, and silently not sending one looks like the far end
    /// ignoring it.
    fn send_break(&mut self, _dur: Duration) -> Result<()> {
        Err(Error::Unsupported(
            "a line break over SSH (RFC 4335) — russh does not implement the request".into(),
        ))
    }

    fn supports_break(&self) -> bool {
        false
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let _ = self.cmd.send(Cmd::Resize(cols, rows));
        Ok(())
    }

    #[cfg(unix)]
    fn poll_fd(&self) -> Option<std::os::unix::io::RawFd> {
        Some(self.shared.wake.fd())
    }

    #[cfg(windows)]
    fn wait_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        Some(self.shared.wake.handle())
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

impl Drop for SshConn {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Relaxed);
        let _ = self.cmd.send(Cmd::Disconnect);
    }
}

// ---------------------------------------------------------------------------
// The worker.
// ---------------------------------------------------------------------------

/// The `check_server_key` half of the conversation.
struct Handler {
    shared: Arc<Shared>,
    known_hosts: KnownHosts,
    policy: HostKeyPolicy,
    host: String,
    port: u16,
    decisions: UnboundedReceiver<HostKeyDecision>,
}

impl Handler {
    /// Write the key down, and treat failure as a note rather than a refusal.
    ///
    /// The user — or the policy — said connect. Failing the connection because
    /// `known_hosts` is on a read-only filesystem would be answering a
    /// different question from the one that was asked.
    fn record(&self, key: HostKeyRef<'_>) {
        if let Err(e) = self.known_hosts.learn(&self.host, self.port, key) {
            self.shared
                .push(format!("\r\ncould not record the host key: {e}\r\n").as_bytes());
        }
    }

    fn refuse(&self, why: String) {
        self.shared
            .set_phase(Phase::Ended(Some(Error::HostKey(why))));
    }
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let blob = key.to_bytes().map_err(|_| russh::Error::CouldNotReadKey)?;
        // The algorithm name comes out of the blob rather than from
        // `key.algorithm()`, because the two disagree exactly where it
        // matters: a host key verified with `rsa-sha2-512` signatures is still
        // written down as `ssh-rsa`, and RFC 8332 leaves the blob's own type
        // string alone. Taking the negotiated name would report every RSA host
        // in the file as unknown.
        let algorithm = algorithm_from_blob(&blob).unwrap_or("").to_string();
        let recorded = HostKeyRef {
            algorithm: &algorithm,
            blob: &blob,
        };

        let verdict = match self.known_hosts.check(&self.host, self.port, recorded) {
            Ok(v) => v,
            Err(e) => {
                self.shared
                    .set_phase(Phase::Ended(Some(Error::HostKey(format!(
                        "cannot read known_hosts: {e}"
                    )))));
                return Ok(false);
            }
        };
        if verdict.is_trusted() {
            return Ok(true);
        }
        // Revocation is not a policy question. Nothing may click through it,
        // and no `StrictHostKeyChecking no` in a config file overrides it.
        if let Verdict::Revoked(site) = &verdict {
            self.refuse(format!(
                "the host key for {} is revoked at {site}",
                self.host
            ));
            return Ok(false);
        }

        let changed = matches!(verdict, Verdict::Changed { .. });
        match self.policy {
            // Refuses *before* the prompt, which is the whole point of it —
            // asking and then ignoring the answer would be worse than not
            // asking.
            HostKeyPolicy::Strict => {
                self.refuse(format!(
                    "{} is not in the known_hosts files and StrictHostKeyChecking is yes",
                    self.host
                ));
                return Ok(false);
            }
            HostKeyPolicy::AcceptNew if changed => {
                self.refuse(format!(
                    "the host key for {} has changed; StrictHostKeyChecking is accept-new",
                    self.host
                ));
                return Ok(false);
            }
            HostKeyPolicy::AcceptNew => {
                self.record(recorded);
                return Ok(true);
            }
            // A changed key is deliberately *not* recorded: overwriting the
            // old entry would destroy the only evidence that it changed.
            HostKeyPolicy::AcceptAny => {
                if !changed {
                    self.record(recorded);
                }
                return Ok(true);
            }
            HostKeyPolicy::Ask => {}
        }

        self.shared.prompt(Pending::HostKey(HostKeyPrompt {
            host: self.host.clone(),
            port: self.port,
            algorithm: algorithm.clone(),
            fingerprint: recorded.fingerprint(),
            verdict,
        }));
        match self.decisions.recv().await {
            Some(HostKeyDecision::AcceptAndSave) => {
                self.record(recorded);
                Ok(true)
            }
            Some(HostKeyDecision::AcceptOnce) => Ok(true),
            // `None` is the caller going away mid-prompt, which is a refusal.
            Some(HostKeyDecision::Refuse) | None => Ok(false),
        }
    }

    /// Servers that print a banner mean it to be read — it is where a console
    /// server says which port you are about to land on. It goes into the
    /// stream ahead of the session, which is where OpenSSH puts it too.
    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> std::result::Result<(), Self::Error> {
        self.shared.push(banner.as_bytes());
        Ok(())
    }
}

/// The leading `string` of an SSH public key blob is its type name.
fn algorithm_from_blob(blob: &[u8]) -> Option<&str> {
    let len = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    std::str::from_utf8(blob.get(4..4 + len)?).ok()
}

async fn run(
    params: SshParams,
    shared: Arc<Shared>,
    cmd_rx: UnboundedReceiver<Cmd>,
    host_key_rx: UnboundedReceiver<HostKeyDecision>,
    answers_rx: UnboundedReceiver<Vec<String>>,
) {
    match connect(&params, &shared, host_key_rx, answers_rx).await {
        Ok((handle, channel)) => {
            shared.set_phase(Phase::Running);
            let reason = pump(&shared, handle, channel, cmd_rx).await;
            shared.set_phase(Phase::Ended(reason));
        }
        Err(e) => {
            // `check_server_key` may already have set a better reason — a
            // revoked key, an unreadable file — and russh reports the refusal
            // it caused as a generic failure. Keep the specific one.
            let mut state = shared.lock();
            if !matches!(state.phase, Phase::Ended(Some(_))) {
                state.phase = Phase::Ended(Some(e));
            }
            drop(state);
            shared.wake.signal();
        }
    }
}

type Session = (client::Handle<Handler>, russh::Channel<client::Msg>);

async fn connect(
    params: &SshParams,
    shared: &Arc<Shared>,
    host_key_rx: UnboundedReceiver<HostKeyDecision>,
    mut answers_rx: UnboundedReceiver<Vec<String>>,
) -> Result<Session> {
    let config = Arc::new(client::Config {
        preferred: preferred(params.legacy),
        keepalive_interval: params.keepalive,
        // Silence is the normal state of a console. An inactivity timeout
        // would hang up on a session that is simply waiting for the operator.
        inactivity_timeout: None,
        nodelay: true,
        ..Default::default()
    });

    let handler = Handler {
        shared: Arc::clone(shared),
        known_hosts: params.known_hosts.clone(),
        policy: params.host_key_policy,
        host: params.host.clone(),
        port: params.port,
        decisions: host_key_rx,
    };

    // A proxy is dialled and spoken to on a **blocking** socket and only then
    // handed over, so the four relays have one implementation rather than a
    // synchronous one for telnet and an async one here. That matters more than
    // it looks: they are where the wire format is, and two copies of a wire
    // format drift. `spawn_blocking` keeps the wait off the runtime's worker,
    // and the whole thing is inside the same timeout the direct path has.
    let mut handle = match &params.proxy {
        Some(proxy) if proxy.is_active() => {
            let (proxy, host, port) = ((**proxy).clone(), params.host.clone(), params.port);
            let timeout = params.connect_timeout;
            let dialling = tokio::task::spawn_blocking(move || {
                crate::proxy::dial(Some(&proxy), &host, port, timeout)
            });
            let connecting = async {
                let socket = match dialling.await {
                    Ok(r) => r?,
                    Err(e) => {
                        return Err(Error::Proxy(format!("the proxy dial did not finish: {e}")))
                    }
                };
                // russh wants a tokio stream, and `from_std` insists the
                // descriptor is already non-blocking — without this it panics
                // rather than misbehaving, but only once something reads.
                socket.set_nonblocking(true).map_err(Error::from_io)?;
                let socket = tokio::net::TcpStream::from_std(socket).map_err(Error::from_io)?;
                client::connect_stream(config, socket, handler)
                    .await
                    .map_err(|e| Error::Ssh(format!("{e}")))
            };
            match tokio::time::timeout(params.connect_timeout, connecting).await {
                Ok(Ok(h)) => h,
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(Error::Ssh(format!(
                        "no answer from {}:{} through the proxy after {:?}",
                        params.host, params.port, params.connect_timeout
                    )))
                }
            }
        }
        _ => {
            let connecting = client::connect(config, (params.host.as_str(), params.port), handler);
            match tokio::time::timeout(params.connect_timeout, connecting).await {
                Ok(Ok(h)) => h,
                Ok(Err(e)) => return Err(Error::Ssh(format!("{e}"))),
                Err(_) => {
                    return Err(Error::Ssh(format!(
                        "no answer from {}:{} after {:?}",
                        params.host, params.port, params.connect_timeout
                    )))
                }
            }
        }
    };

    authenticate(params, shared, &mut handle, &mut answers_rx).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| Error::Ssh(format!("cannot open a session channel: {e}")))?;
    channel
        .request_pty(
            true,
            &params.term,
            params.cols as u32,
            params.rows as u32,
            0,
            0,
            // OpenSSH sends the line speeds and leaves the rest to the
            // server's defaults; some embedded servers read them and most
            // ignore them.
            &[(Pty::TTY_OP_ISPEED, 115_200), (Pty::TTY_OP_OSPEED, 115_200)],
        )
        .await
        .map_err(|e| Error::Ssh(format!("the server refused a pty: {e}")))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| Error::Ssh(format!("the server refused a shell: {e}")))?;

    Ok((handle, channel))
}

/// The algorithms to offer.
///
/// Spike 5's second finding decided the shape: embedded servers offer very
/// little, and narrow in *different* directions — Dropbear declined nine of
/// the algorithms tested, none of them the ones an old Cisco declines. So the
/// client offers broadly and lets the server choose, rather than hard-coding
/// a modern list and calling the result a compatibility problem.
fn preferred(legacy: bool) -> Preferred {
    let base = Preferred::DEFAULT;
    if !legacy {
        return base;
    }
    let mut kex = base.kex.to_vec();
    kex.extend_from_slice(&[kex::DH_G14_SHA1, kex::DH_GEX_SHA1, kex::DH_G1_SHA1]);
    let mut key = base.key.to_vec();
    // `ssh-rsa` — `Algorithm::Rsa { hash: None }`, an RSA host key with SHA-1
    // signatures — is already last in russh's default list, so only DSA is
    // missing. It is behind the `dsa` feature and appears nowhere by default.
    key.push(Algorithm::Dsa);
    let mut cipher = base.cipher.to_vec();
    cipher.extend_from_slice(&[
        cipher::AES_128_CBC,
        cipher::AES_192_CBC,
        cipher::AES_256_CBC,
        cipher::TRIPLE_DES_CBC,
    ]);
    let mut mac = base.mac.to_vec();
    mac.extend_from_slice(&[mac::HMAC_SHA1_ETM, mac::HMAC_SHA1]);
    Preferred {
        kex: Cow::Owned(kex),
        key: Cow::Owned(key),
        cipher: Cow::Owned(cipher),
        mac: Cow::Owned(mac),
        compression: base.compression,
    }
}

/// Try what the server will take, in the order that costs the user least.
///
/// Agent first, then key files, then the interactive methods — because the
/// first two need nothing typed. The server's `remaining_methods` drives the
/// order rather than a fixed list: a device that only does
/// `keyboard-interactive` should not be asked for a password it will reject.
async fn authenticate(
    params: &SshParams,
    shared: &Arc<Shared>,
    handle: &mut client::Handle<Handler>,
    answers: &mut UnboundedReceiver<Vec<String>>,
) -> Result<()> {
    // `none` is not a shortcut past authentication — it is how a client asks
    // what the server accepts. Some appliances do answer it with success.
    let mut offered = match handle.authenticate_none(&params.user).await {
        Ok(r) if r.success() => return Ok(()),
        Ok(russh::client::AuthResult::Failure {
            remaining_methods, ..
        }) => remaining_methods,
        Ok(_) => unreachable!("AuthResult has two variants"),
        Err(e) => return Err(Error::Ssh(format!("{e}"))),
    };

    // Three layers, and each `None` means something different: the call
    // failed, the server did not advertise `server-sig-algs`, or it did and
    // wants SHA-1. All three collapse to the same fallback — plain `ssh-rsa`
    // — which is what a server that says nothing must be assumed to want.
    let rsa_hash = handle
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();

    if offered.contains(&MethodKind::PublicKey) {
        if params.use_agent {
            if let Some(r) = try_agent(params, handle, rsa_hash).await? {
                match r {
                    Outcome::Ok => return Ok(()),
                    Outcome::More(m) => offered = m,
                }
            }
        }
        let identities = if params.identities.is_empty() {
            default_identities()
        } else {
            params.identities.clone()
        };
        for path in identities {
            if shared.is_cancelled() {
                return Err(Error::Ssh("cancelled".into()));
            }
            match try_identity(params, shared, handle, answers, &path, rsa_hash).await? {
                Some(Outcome::Ok) => return Ok(()),
                Some(Outcome::More(m)) => offered = m,
                None => {}
            }
            if !offered.contains(&MethodKind::PublicKey) {
                break;
            }
        }
    }

    if offered.contains(&MethodKind::KeyboardInteractive) {
        match try_keyboard_interactive(params, shared, handle, answers).await? {
            Outcome::Ok => return Ok(()),
            Outcome::More(m) => offered = m,
        }
    }

    if offered.contains(&MethodKind::Password) {
        // OpenSSH's `NumberOfPasswordPrompts`. Three, then stop: a device that
        // counts failures should not be walked into a lockout by a UI that
        // will ask forever.
        for _ in 0..3 {
            let answer = ask(
                shared,
                answers,
                AuthPrompt {
                    kind: AuthPromptKind::Password,
                    name: String::new(),
                    instruction: String::new(),
                    prompts: vec![Prompt {
                        text: format!("{}@{}'s password: ", params.user, params.host),
                        echo: false,
                    }],
                },
            )
            .await?;
            let password = answer.into_iter().next().unwrap_or_default();
            match handle.authenticate_password(&params.user, password).await {
                Ok(r) if r.success() => return Ok(()),
                Ok(russh::client::AuthResult::Failure {
                    remaining_methods, ..
                }) => offered = remaining_methods,
                Ok(_) => unreachable!("AuthResult has two variants"),
                Err(e) => return Err(Error::Ssh(format!("{e}"))),
            }
            if !offered.contains(&MethodKind::Password) {
                break;
            }
        }
    }

    Err(Error::Auth {
        offered: offered.iter().map(method_name).collect(),
    })
}

/// Either authenticated, or not — with what the server says it will still take.
enum Outcome {
    Ok,
    More(russh::MethodSet),
}

fn method_name(m: &MethodKind) -> String {
    match m {
        MethodKind::None => "none",
        MethodKind::Password => "password",
        MethodKind::PublicKey => "publickey",
        MethodKind::HostBased => "hostbased",
        MethodKind::KeyboardInteractive => "keyboard-interactive",
    }
    .to_string()
}

/// `Ok(None)` when there is no agent or it holds nothing, which is not a
/// failure — it is the normal state on a machine that uses key files.
async fn try_agent(
    params: &SshParams,
    handle: &mut client::Handle<Handler>,
    rsa_hash: Option<HashAlg>,
) -> Result<Option<Outcome>> {
    #[cfg(unix)]
    let agent = russh::keys::agent::client::AgentClient::connect_env().await;
    #[cfg(windows)]
    let agent = russh::keys::agent::client::AgentClient::connect_pageant().await;
    #[cfg(not(any(unix, windows)))]
    return Ok(None);

    let Ok(agent) = agent else {
        return Ok(None);
    };
    try_agent_client(params, handle, rsa_hash, agent).await
}

async fn try_agent_client<S>(
    params: &SshParams,
    handle: &mut client::Handle<Handler>,
    rsa_hash: Option<HashAlg>,
    mut agent: russh::keys::agent::client::AgentClient<S>,
) -> Result<Option<Outcome>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let Ok(identities) = agent.request_identities().await else {
        return Ok(None);
    };
    let mut last = None;
    for id in identities {
        let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = id else {
            // Certificates are Stage 2's; skip rather than fail.
            continue;
        };
        match handle
            .authenticate_publickey_with(&params.user, key, rsa_hash, &mut agent)
            .await
        {
            Ok(r) if r.success() => return Ok(Some(Outcome::Ok)),
            Ok(russh::client::AuthResult::Failure {
                remaining_methods, ..
            }) => {
                if !remaining_methods.contains(&MethodKind::PublicKey) {
                    return Ok(Some(Outcome::More(remaining_methods)));
                }
                last = Some(Outcome::More(remaining_methods));
            }
            Ok(_) => unreachable!("AuthResult has two variants"),
            // A broken agent is not a broken connection; fall through to the
            // key files.
            Err(_) => return Ok(last),
        }
    }
    Ok(last)
}

/// `Ok(None)` when the file is missing or unreadable — one absent `id_rsa` in
/// the default list must not end authentication.
async fn try_identity(
    params: &SshParams,
    shared: &Arc<Shared>,
    handle: &mut client::Handle<Handler>,
    answers: &mut UnboundedReceiver<Vec<String>>,
    path: &PathBuf,
    rsa_hash: Option<HashAlg>,
) -> Result<Option<Outcome>> {
    let key = match load_secret_key(path, None) {
        Ok(k) => k,
        Err(russh::keys::Error::KeyIsEncrypted) => {
            let answer = ask(
                shared,
                answers,
                AuthPrompt {
                    kind: AuthPromptKind::Passphrase(path.clone()),
                    name: String::new(),
                    instruction: String::new(),
                    prompts: vec![Prompt {
                        text: format!("passphrase for {}: ", path.display()),
                        echo: false,
                    }],
                },
            )
            .await?;
            let passphrase = answer.into_iter().next().unwrap_or_default();
            match load_secret_key(path, Some(&passphrase)) {
                Ok(k) => k,
                Err(e) => {
                    shared.push(format!("\r\n{}: {e}\r\n", path.display()).as_bytes());
                    return Ok(None);
                }
            }
        }
        Err(_) => return Ok(None),
    };

    let signer = PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash);
    match handle.authenticate_publickey(&params.user, signer).await {
        Ok(r) if r.success() => Ok(Some(Outcome::Ok)),
        Ok(russh::client::AuthResult::Failure {
            remaining_methods, ..
        }) => Ok(Some(Outcome::More(remaining_methods))),
        Ok(_) => unreachable!("AuthResult has two variants"),
        Err(e) => Err(Error::Ssh(format!("{e}"))),
    }
}

async fn try_keyboard_interactive(
    params: &SshParams,
    shared: &Arc<Shared>,
    handle: &mut client::Handle<Handler>,
    answers: &mut UnboundedReceiver<Vec<String>>,
) -> Result<Outcome> {
    use russh::client::KeyboardInteractiveAuthResponse as Response;

    let mut response = handle
        .authenticate_keyboard_interactive_start(&params.user, None)
        .await
        .map_err(|e| Error::Ssh(format!("{e}")))?;
    loop {
        match response {
            Response::Success => return Ok(Outcome::Ok),
            Response::Failure {
                remaining_methods, ..
            } => return Ok(Outcome::More(remaining_methods)),
            Response::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                // A server is allowed to send an empty request — it is how
                // some devices display a notice — and the reply is an empty
                // list, not a prompt.
                let answer = if prompts.is_empty() {
                    if !instructions.is_empty() {
                        shared.push(instructions.replace('\n', "\r\n").as_bytes());
                    }
                    Vec::new()
                } else {
                    ask(
                        shared,
                        answers,
                        AuthPrompt {
                            kind: AuthPromptKind::KeyboardInteractive,
                            name,
                            instruction: instructions,
                            prompts: prompts
                                .iter()
                                .map(|p| Prompt {
                                    text: p.prompt.clone(),
                                    echo: p.echo,
                                })
                                .collect(),
                        },
                    )
                    .await?
                };
                response = handle
                    .authenticate_keyboard_interactive_respond(answer)
                    .await
                    .map_err(|e| Error::Ssh(format!("{e}")))?;
            }
        }
    }
}

/// Publish a prompt and wait for the caller's answer.
async fn ask(
    shared: &Arc<Shared>,
    answers: &mut UnboundedReceiver<Vec<String>>,
    prompt: AuthPrompt,
) -> Result<Vec<String>> {
    let wanted = prompt.prompts.len();
    shared.prompt(Pending::Auth(prompt));
    match answers.recv().await {
        Some(mut a) => {
            // The protocol requires one response per prompt. A frontend that
            // sends the wrong number would otherwise desynchronise the
            // exchange in a way that reads as a server bug.
            a.resize(wanted, String::new());
            Ok(a)
        }
        None => Err(Error::Ssh("cancelled".into())),
    }
}

/// The session, once it is up: bytes out, bytes in, and the two things that
/// are neither.
async fn pump(
    shared: &Arc<Shared>,
    handle: client::Handle<Handler>,
    channel: russh::Channel<client::Msg>,
    mut cmd_rx: UnboundedReceiver<Cmd>,
) -> Option<Error> {
    let (mut read, write) = channel.split();
    loop {
        // Above the mark, stop taking from the channel. russh's own flow
        // control then stops advertising window space, and the backlog stays
        // on the server rather than in this process.
        let full = shared.buffered() >= HIGH_WATER;
        tokio::select! {
            msg = read.wait(), if !full => match msg {
                Some(ChannelMsg::Data { data }) => shared.push(&data),
                // Stderr. A terminal has one screen, and this is where the
                // remote side's diagnostics belong — which is also where
                // OpenSSH puts them.
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => shared.push(&data),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            },
            _ = shared.drained.notified(), if full => {}
            cmd = cmd_rx.recv() => match cmd {
                Some(Cmd::Data(bytes)) => {
                    if write.data_bytes(bytes).await.is_err() {
                        break;
                    }
                }
                Some(Cmd::Resize(cols, rows)) => {
                    let _ = write.window_change(cols as u32, rows as u32, 0, 0).await;
                }
                Some(Cmd::Disconnect) | None => {
                    let _ = write.eof().await;
                    break;
                }
            },
        }
    }
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The legacy switch is the whole of spike 5's first finding, and the
    /// failure mode if it silently does nothing is a device that will not
    /// answer and a user with no way to find out why.
    #[test]
    fn legacy_mode_offers_what_old_gear_has_and_the_default_does_not() {
        let modern = preferred(false);
        let legacy = preferred(true);

        for name in [kex::DH_G1_SHA1, kex::DH_G14_SHA1, kex::DH_GEX_SHA1] {
            assert!(!modern.kex.contains(&name), "{name:?} in the modern list");
            assert!(legacy.kex.contains(&name), "{name:?} missing from legacy");
        }
        for name in [cipher::TRIPLE_DES_CBC, cipher::AES_128_CBC] {
            assert!(
                !modern.cipher.contains(&name),
                "{name:?} in the modern list"
            );
            assert!(
                legacy.cipher.contains(&name),
                "{name:?} missing from legacy"
            );
        }
        assert!(!modern.mac.contains(&mac::HMAC_SHA1));
        assert!(legacy.mac.contains(&mac::HMAC_SHA1));

        // `ssh-rsa` — an RSA key with SHA-1 signatures — is already in russh's
        // default list, and `ssh-dss` is not in either until the switch is on.
        let ssh_rsa = Algorithm::Rsa { hash: None };
        assert!(
            modern.key.contains(&ssh_rsa),
            "ssh-rsa dropped from default"
        );
        assert!(!modern.key.contains(&Algorithm::Dsa));
        assert!(legacy.key.contains(&Algorithm::Dsa));

        // Broad, not replaced: a server that speaks curve25519 must still get
        // it when the switch is on, or turning legacy on to reach one device
        // would downgrade every other connection.
        assert!(legacy.kex.contains(&kex::CURVE25519));
        assert!(legacy.cipher.contains(&cipher::CHACHA20_POLY1305));
    }

    #[test]
    fn the_algorithm_name_comes_from_the_blob() {
        // An RSA host key verified with `rsa-sha2-512` signatures is recorded
        // as `ssh-rsa`, and the blob is the only place that stays true.
        let mut blob = Vec::new();
        blob.extend_from_slice(&7u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-rsa");
        blob.extend_from_slice(b"...the rest of the key...");
        assert_eq!(algorithm_from_blob(&blob), Some("ssh-rsa"));

        // A truncated or non-UTF-8 blob is not a panic.
        assert_eq!(algorithm_from_blob(&[0, 0, 0]), None);
        assert_eq!(algorithm_from_blob(&[0, 0, 0, 99, b'x']), None);
    }

    #[test]
    fn a_config_alias_becomes_a_connection() {
        // The adoption lever, end to end: what `ssh myrouter` would do.
        let config = super::super::config::SshConfig::parse(
            "Host myrouter\n\
             \x20 HostName 10.0.0.1\n\
             \x20 User admin\n\
             \x20 Port 2222\n\
             \x20 IdentityFile /keys/router\n\
             \x20 IdentitiesOnly yes\n\
             \x20 KexAlgorithms +diffie-hellman-group14-sha1\n\
             \x20 StrictHostKeyChecking accept-new\n\
             \x20 ConnectTimeout 5\n\
             \x20 ServerAliveInterval 30\n",
            std::path::Path::new("/home/nobody/.ssh/config"),
        );
        let p = SshParams::from_config(&config, "myrouter", None, None);
        assert_eq!(p.host, "10.0.0.1");
        assert_eq!(p.port, 2222);
        assert_eq!(p.user, "admin");
        assert_eq!(p.identities, vec![PathBuf::from("/keys/router")]);
        // IdentitiesOnly: do not offer whatever the agent happens to hold.
        assert!(!p.use_agent);
        // The config already said this is old equipment.
        assert!(p.legacy);
        assert_eq!(p.host_key_policy, HostKeyPolicy::AcceptNew);
        assert_eq!(p.connect_timeout, Duration::from_secs(5));
        assert_eq!(p.keepalive, Some(Duration::from_secs(30)));
    }

    #[test]
    fn what_was_typed_overrides_the_file() {
        let config = super::super::config::SshConfig::parse(
            "Host r\n  HostName 10.0.0.1\n  User admin\n  Port 2222\n",
            std::path::Path::new("/home/nobody/.ssh/config"),
        );
        let p = SshParams::from_config(&config, "r", Some("root"), Some(2022));
        assert_eq!(p.user, "root");
        assert_eq!(p.port, 2022);
        // ...but not the host name, which is what the alias is *for*.
        assert_eq!(p.host, "10.0.0.1");
    }

    #[test]
    fn a_host_with_no_config_still_works() {
        let config = super::super::config::SshConfig::default();
        let p = SshParams::from_config(&config, "plain.example.com", Some("nata"), None);
        assert_eq!(p.host, "plain.example.com");
        assert_eq!(p.port, 22);
        assert_eq!(p.host_key_policy, HostKeyPolicy::Ask);
        assert!(p.use_agent);
        assert!(!p.legacy);
    }

    #[test]
    fn a_default_port_is_left_out_of_the_description() {
        assert_eq!(SshParams::new("host", 22, "nata").describe(), "nata@host");
        assert_eq!(
            SshParams::new("host", 2222, "nata").describe(),
            "nata@host:2222"
        );
    }
}
