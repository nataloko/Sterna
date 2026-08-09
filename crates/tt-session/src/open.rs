//! What a command line says to open — `OnCommStart` (`vtwin.cpp:3708`).
//!
//! The parser is in `tt-config` and the transports are in `tt-conn`; this is the
//! one crate that holds both, so the join lives here for the same reason
//! [`crate::settings`] does.
//!
//! **Three outcomes, not one.** Upstream's startup is a single `if` and it is
//! easy to miss that two of its arms open nothing:
//!
//! ```c
//! if (((ts.PortType!=IdSerial) && (ts.HostName[0]==0)) ||
//!     ((ts.PortType==IdSerial) && (ts.ComAutoConnect == FALSE))) {
//!         if (ts.HostDialogOnStartup) OnFileNewConnection();
//!         else SetDdeComReady(0);
//! } else CommOpen(…);
//! ```
//!
//! So a TCP session is decided by whether there is a **host name** — not by the
//! port type — and a serial one by `ComAutoConnect`, which `/M` turns off and an
//! explicit `/C=` turns back on. `/DS` and `/ES` then choose between the dialog
//! and an empty window. [`Startup`] is those three answers.
//!
//! Everything else is read from [`Settings`], **after**
//! [`CommandLine::apply`](tt_config::cmdline::CommandLine::apply) — which is
//! upstream's order too, since `_ParseParam` writes into `ts` and `CommOpen`
//! reads `ts` back. The command line is consulted for exactly the three things
//! the file cannot hold: the host name, which has no key at all; the port type,
//! because the file holds two of its four values; and TTSSH's options.

use std::path::PathBuf;
use std::time::Duration;

use tt_config::cmdline::ssh::{AuthMethod, SshOptions};
use tt_config::cmdline::{cygterm, CommandLine, PortType};
use tt_config::Settings;
use tt_config::{ConnectionPortType, SerialDataBits, SerialFlow, SerialParity, SerialStopBits};
use tt_conn::pty::PtyParams;
use tt_conn::serial::{
    port_by_number, DataBits, FlowControl, Parity, PinControl, SerialParams, StopBits,
};
use tt_conn::ssh::SshParams;
use tt_conn::telnet::{TelnetMode, TelnetParams};
use tt_conn::Transport;

/// SSH's own port, for the divergence described on [`Target::of`].
const SSH_PORT: u16 = 22;

/// What to do at startup, once a command line and a settings file have both had
/// their say.
#[derive(Clone, Debug, PartialEq)]
pub enum Startup {
    /// `CommOpen` — connect to this.
    Open(Target),
    /// `OnFileNewConnection` — nothing was named, and `HostDialogOnStartup` is
    /// on, so ask.
    Dialog,
    /// `SetDdeComReady(0)` — nothing was named and the dialog is suppressed, so
    /// sit there with a terminal and no connection. `/DS` is how a macro that
    /// will `connect` for itself starts up.
    Idle,
    /// The line named a transport this port does not have, with the reason.
    /// Upstream would have opened it; saying so beats opening something else.
    Unsupported(&'static str),
}

/// A connection to open, in the terms `tt-conn` takes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Serial {
        /// The device, resolved from `/C=<n>` through enumeration — see
        /// `tt_conn::serial::port_by_number` for why a number means the nth
        /// port rather than `/dev/ttyS<n-1>`.
        path: String,
        params: SerialParams,
    },
    Telnet {
        host: String,
        port: u16,
        params: TelnetParams,
        timeout: Duration,
    },
    /// SSH, which **this does not open**: the host key and the password are
    /// prompts, and prompts belong to whoever owns a window. The frontend
    /// already drives that state machine; this only says what to drive it with.
    Ssh {
        params: SshParams,
        /// Whether a port was *asked* for — `ts.TCPPort != ts.TelPort`, which
        /// is upstream's own idiom and the test described on [`Target::of`].
        ///
        /// False means `params.port` is this port's fallback rather than
        /// anybody's choice, so a consumer with a better answer should use it:
        /// `~/.ssh/config`'s `Port` is one, and it is why `sterna myrouter`
        /// reaches an alias on 2222 today. Nothing in `tt-conn` reads the
        /// config, so the fallback stays a real 22 rather than a zero meaning
        /// "ask somebody".
        port_chosen: bool,
        /// `/passwd=`, or a URL's. Held rather than used, so a frontend can
        /// decide whether an automatic login is allowed to skip its own prompt.
        password: Option<String>,
        /// `/auth=`, unmapped — `tt-conn` chooses its own order (agent, then
        /// keys, then password) and forcing one method is a later decision.
        method: Option<AuthMethod>,
        /// `/ask4passwd`, which turns an automatic login back into a prompt.
        ask_password: bool,
        /// `/nosecuritywarning`. Loud on purpose: it skips the `known_hosts`
        /// check.
        no_known_hosts_check: bool,
    },
    /// A local shell — `cygconnect`'s answer here, and the `--shell` flag's.
    /// No command line names it: upstream launches `cyglaunch.exe`.
    Shell(Box<PtyParams>),
}

impl Startup {
    /// `OnCommStart`, over a command line that has already been applied to
    /// `settings`.
    ///
    /// `cols` and `rows` are the terminal's, which the transports need for
    /// `NAWS` and for the pty's `winsize`; they come from the session rather
    /// than from the settings because a window that has been resized since
    /// startup is the truth.
    pub fn of(
        cmd: &CommandLine,
        ssh: &SshOptions,
        settings: &Settings,
        cols: u16,
        rows: u16,
    ) -> Startup {
        let port_type = cmd
            .port_type
            .unwrap_or(match settings.connection_port_type {
                ConnectionPortType::Serial => PortType::Serial,
                ConnectionPortType::TcpIp => PortType::TcpIp,
            });

        // The two arms that open nothing. Note which test goes with which
        // transport: a host name for anything but serial, `ComAutoConnect` for
        // serial — so `/C=1 /M=x` opens the dialog and `myhost /M=x` connects.
        let named = match port_type {
            PortType::Serial => cmd.com_auto_connect,
            _ => !cmd.host_name.is_empty(),
        };
        if !named {
            return match settings.connection_host_dialog_on_startup {
                true => Startup::Dialog,
                false => Startup::Idle,
            };
        }

        match Target::of(cmd, ssh, settings, cols, rows) {
            Ok(t) => Startup::Open(t),
            Err(why) => Startup::Unsupported(why),
        }
    }

    /// The same for a macro's `connect` — `CmdConnect` (`ttdde.c:608`), which
    /// is the startup path with a string in front of it.
    ///
    /// Upstream's terminal parses the macro's argument **into `ts`** and then
    /// posts `WM_USER_COMMSTART`, which is the very message a startup command
    /// line ends at. So this takes `settings` by `&mut`: the line's `/BAUD=`,
    /// `/T=` and `/F=` are settings, they outlive the connection they were
    /// given for, and a second `connect` with none of them keeps the first's.
    ///
    /// **Both parsers run, and TTSSH's runs first** — `LoadTTSET`
    /// (`ttsetup.c:47`) re-installs `_ParseParam` and then calls
    /// `TTXGetSetupHooks`, so the plugin re-hooks the pointer `ttdde.c` is
    /// about to call through. Reading `ttdde.c` alone suggests a `connect`
    /// cannot open an SSH session; it can, and that is most of what the
    /// command is used for.
    ///
    /// Two things upstream does around this are **not** here, both because
    /// they need something this port has not got:
    ///
    /// - A `/F=` that names a *different* settings file posts
    ///   `IdCmdRestoreSetup`, which re-reads it and re-applies it to the
    ///   display. Nothing here knows where the settings came from; the parse
    ///   still records the file, so a caller that does can act on it.
    /// - `cv.NoMsg = 1` suppresses the connection's error dialogs for the
    ///   duration, because the macro is the one being told. There is no dialog
    ///   here to suppress.
    pub fn of_connect(
        arg: &[u8],
        settings: &mut Settings,
        cols: u16,
        rows: u16,
    ) -> (Startup, CommandLine) {
        let max = settings.serial_max_com_port.clamp(0, i32::from(u16::MAX)) as u16;
        let (cmd, ssh) = tt_config::cmdline::ssh::parse_both_argument(arg, max);
        cmd.apply(settings);
        let startup = Startup::of(&cmd, &ssh, settings, cols, rows);
        (startup, cmd)
    }
}

impl Target {
    /// The transport and its parameters, or why there is none.
    ///
    /// **One deliberate divergence, and it is about a port.** TTSSH never sets
    /// `ts.TCPPort` — only its half of the New Connection dialog does
    /// (`ttxssh.c:1347`) — so upstream's `ttermpro /ssh myhost` connects to
    /// whatever `TCPPort=` holds, which on a fresh install is **23**: an SSH
    /// client on the telnet port, a connection that cannot succeed. Here, SSH
    /// with no port asked for uses 22.
    ///
    /// The test for "no port was asked for" is upstream's own idiom rather than
    /// a new one: `ts.TCPPort == ts.TelPort` is exactly how `vtwin.cpp:3666`
    /// decides whether a TCP port was chosen for a protocol or merely inherited,
    /// and it is why the telnet opening burst is not sent to a terminal server's
    /// per-line port. A user who has ever connected by SSH from the dialog has
    /// `TCPPort=22` in their file already, so this only changes what a fresh
    /// setup does — and it changes it from certain failure.
    pub fn of(
        cmd: &CommandLine,
        ssh: &SshOptions,
        s: &Settings,
        cols: u16,
        rows: u16,
    ) -> Result<Target, &'static str> {
        let port_type = cmd.port_type.unwrap_or(match s.connection_port_type {
            ConnectionPortType::Serial => PortType::Serial,
            ConnectionPortType::TcpIp => PortType::TcpIp,
        });
        match port_type {
            PortType::Serial => {
                let n = s.serial_com_port.clamp(0, i32::from(u16::MAX)) as u16;
                let port = port_by_number(n)
                    .map_err(|_| "the serial ports could not be enumerated")?
                    .ok_or("there is no serial port with that number")?;
                Ok(Target::Serial {
                    path: port.open_path().to_string(),
                    params: serial_params(s),
                })
            }
            PortType::TcpIp => {
                let host = String::from_utf8_lossy(&cmd.host_name).into_owned();
                let timeout = timeout(s);
                if ssh.enabled == Some(true) {
                    return Ok(Target::Ssh {
                        params: ssh_params(ssh, s, &host, cols, rows),
                        port_chosen: s.connection_tcp_port != s.connection_telnet_port,
                        password: ssh.password.as_ref().map(|p| text(p)),
                        method: ssh.auth_method,
                        ask_password: ssh.ask_password,
                        no_known_hosts_check: ssh.no_known_hosts_check,
                    });
                }
                let port = s.connection_tcp_port.clamp(0, i32::from(u16::MAX)) as u16;
                Ok(Target::Telnet {
                    host,
                    port,
                    params: telnet_params(s, port, cols, rows),
                    timeout,
                })
            }
            // Both are real upstream transports and neither is built. `/R=`
            // replays a captured session, which is a Stage 4 feature; a named
            // pipe is Windows.
            PortType::File => Err("replaying a captured session is not implemented"),
            PortType::NamedPipe => Err("named pipes are a Windows transport"),
        }
    }
}

impl Target {
    /// `cygconnect`'s argument, which is **CygTerm's** command line rather than
    /// Tera Term's — see [`tt_config::cmdline::cygterm`] for why, and for the
    /// options themselves.
    ///
    /// A local shell is this port's answer to Cygwin, and the mapping is closer
    /// than it looks: `exec_shell` (`cygterm.cpp:905`) forks, sets `$TERM` and
    /// the `-v` variables, changes directory, and executes the shell with a
    /// leading `-` on `argv[0]` for a login shell. That is [`PtyParams`] field
    /// for field.
    ///
    /// **The default is the launcher's directory, not the user's home.** With
    /// no `-cd` and no `-d`, `home_chdir` is false and CygTerm never calls
    /// `chdir`, so the shell starts where the terminal was started — which is
    /// the process's own directory here, and *not* what `PtyParams::cwd`'s
    /// `None` means.
    ///
    /// `settings` supplies nothing: CygTerm is a separate program and reads
    /// `cygterm.cfg`, not `TERATERM.INI`.
    pub fn cygterm(arg: &[u8], cols: u16, rows: u16) -> Target {
        let cfg = cygterm::parse(arg);
        let base = PtyParams {
            cols,
            rows,
            ..PtyParams::default()
        };
        let argv = match &cfg.shell {
            // `get_argv(argv, 32, cmd_shell)` — the shell string is split by
            // upstream's own splitter, and 32 is its cap.
            Some(s) => cygterm::get_argv(s, 32)
                .into_iter()
                .map(|a| String::from_utf8_lossy(&a).into_owned())
                .collect(),
            // `AUTO`, or nothing said: the account's shell, which
            // `get_username_and_shell` reads out of `/etc/passwd` and
            // `PtyParams` answers with an empty `argv`.
            None => Vec::new(),
        };
        let cwd = match (&cfg.change_dir, cfg.home_chdir) {
            // `chdir` failing is reported and then ignored (`cygterm.cpp:962`),
            // so a directory that is not there must not stop the shell — and
            // it would, since the child cannot report a failed `chdir` back
            // through a pty that is already open.
            (Some(d), _) => {
                let path = PathBuf::from(String::from_utf8_lossy(d).into_owned());
                match path.is_dir() {
                    true => Some(path),
                    false => working_dir(),
                }
            }
            (None, true) => None,
            (None, false) => working_dir(),
        };
        Target::Shell(Box::new(PtyParams {
            argv,
            cwd,
            env: cfg
                .env
                .iter()
                .map(|(n, v)| {
                    (
                        String::from_utf8_lossy(n).into_owned(),
                        String::from_utf8_lossy(v).into_owned(),
                    )
                })
                .collect(),
            // `-dumb` is the only thing that names a terminal type, and it
            // names the one that turns the negotiation off. Anything else
            // keeps ours: `cygterm.cfg`'s `TERM_TYPE = vt100` describes what
            // *CygTerm's* telnet link can do, not what this terminal is.
            term: match &cfg.term_type {
                Some(t) => String::from_utf8_lossy(t).into_owned(),
                None => base.term,
            },
            login_shell: cfg.login_shell,
            ..base
        }))
    }

    /// Open it — `CommOpen`, for the three transports that can be opened
    /// without asking the user anything.
    ///
    /// **[`Target::Ssh`] is not one of them, and that is the whole shape of
    /// this.** An SSH connection may need a host key confirmed or a password
    /// typed, so it is a state machine the caller pumps while it owns a window;
    /// `tt-ffi`'s `tt_ssh_connect` family exists for exactly that and the Qt
    /// shell already drives it. Returning an error here is better than a
    /// blocking `open` that could only answer such a prompt by inventing a
    /// policy — and upstream agrees about *where* the prompt goes: TTSSH puts
    /// its dialogs on the terminal's thread while the macro that asked sleeps.
    pub fn open(&self) -> tt_conn::Result<Box<dyn Transport>> {
        match self {
            Target::Serial { path, params } => {
                Ok(Box::new(tt_conn::serial::SerialConn::open(path, params)?))
            }
            Target::Telnet {
                host,
                port,
                params,
                timeout,
            } => Ok(Box::new(tt_conn::telnet::TelnetConn::connect(
                host, *port, params, *timeout,
            )?)),
            Target::Shell(params) => Ok(Box::new(tt_conn::pty::PtyConn::open(params)?)),
            Target::Ssh { .. } => Err(tt_conn::Error::Unsupported(
                "an SSH connection has prompts, so it is driven by the caller rather than opened \
                 here — see tt_ssh_connect"
                    .into(),
            )),
        }
    }
}

/// `ts` → [`SerialParams`], the four enumerated settings and the delays.
///
/// The delays are **not** here: they are a property of how bytes are *sent*,
/// which `tt-conn` does not implement yet, and putting them in the open call
/// would look like they were being honoured.
pub fn serial_params(s: &Settings) -> SerialParams {
    SerialParams {
        baud: s.serial_baud.max(0) as u32,
        data_bits: match s.serial_data_bits {
            SerialDataBits::Seven => DataBits::Seven,
            SerialDataBits::Eight => DataBits::Eight,
        },
        parity: match s.serial_parity {
            SerialParity::None => Parity::None,
            SerialParity::Odd => Parity::Odd,
            SerialParity::Even => Parity::Even,
            SerialParity::Mark => Parity::Mark,
            SerialParity::Space => Parity::Space,
        },
        stop_bits: match s.serial_stop_bits {
            SerialStopBits::One => StopBits::One,
            SerialStopBits::Two => StopBits::Two,
        },
        flow: match s.serial_flow {
            SerialFlow::None => FlowControl::None,
            SerialFlow::XonXoff => FlowControl::XonXoff,
            SerialFlow::Hardware => FlowControl::RtsCts,
            SerialFlow::DsrDtr => FlowControl::DsrDtr,
        },
        rts: pin_control(s.serial_rts, s.serial_flow == SerialFlow::Hardware),
        dtr: pin_control(s.serial_dtr, s.serial_flow == SerialFlow::DsrDtr),
        ..SerialParams::default()
    }
}

/// `FlowCtrlRTS` / `FlowCtrlDTR` → [`PinControl`], resolving the sentinel.
///
/// `-1` is the default both are read with (`ttset.c:2034`, `:2042`) and it is
/// not a `DCB` value: it means "derive from the flow control", which is why
/// the resolution is here rather than in the schema — the answer depends on
/// another setting, the same reason `connection.terminal_speed` is parsed here
/// too. `handshaking` is whether the flow control is the one *this* line does,
/// which is `hard` for RTS and `dsrdtr` for DTR.
///
/// Anything outside 0..=3 reads as `Enable`, and that is a divergence worth
/// stating: upstream puts the number straight into the `DCB` and never checks
/// what `SetCommState` made of it (`commlib.c:240`), so a stray `FlowCtrlRTS=9`
/// makes Windows reject the whole structure and the port silently keeps the
/// baud, parity and stop bits it already had. Reproducing that would mean
/// reproducing a bug whose symptom is every serial setting in the file going
/// missing at once.
fn pin_control(value: i32, handshaking: bool) -> PinControl {
    match value {
        -1 if handshaking => PinControl::Handshake,
        -1 => PinControl::Enable,
        0 => PinControl::Disable,
        2 => PinControl::Handshake,
        3 => PinControl::Toggle,
        _ => PinControl::Enable,
    }
}

/// `ts` → [`TelnetParams`].
///
/// **`Telnet=off` is not a raw socket**, which is what this used to say and is
/// the trap the whole family turns on: `TelAutoDetect` is a key of its own, it
/// ships on, and `ttcmn.c:590` turns the framing on at the first `0xFF`
/// whatever `Telnet=` said. Two keys and the port test make four modes — see
/// [`TelnetMode::of`], which is where the table is.
///
/// The port test is `ts.TCPPort == ts.TelPort` rather than a literal 23
/// (`vtwin.cpp:3666`): the burst goes out only at the port telnet was chosen
/// for, because a terminal server's per-line port is not a telnet server and
/// five bytes of negotiation would land in somebody's serial console.
///
pub fn telnet_params(s: &Settings, port: u16, cols: u16, rows: u16) -> TelnetParams {
    TelnetParams {
        mode: TelnetMode::of(
            s.connection_telnet,
            s.connection_telnet_auto_detect,
            i32::from(port) == s.connection_telnet_port,
        ),
        binary: s.connection_telnet_binary,
        term_type: s.connection_term_type.clone(),
        speed: terminal_speed(s),
        cols,
        rows,
        echo_negotiates: s.connection_telnet_echo,
        // What `TelChangeEcho` reads is the terminal's *live* `ts.LocalEcho`,
        // but nothing has connected yet at the moment the burst is built, so
        // the file's value is the terminal's value. A session that has since
        // seen an SRM would differ, and upstream has the same gap: the burst is
        // built once, at open.
        local_echo: s.terminal_local_echo,
        keepalive: match s.connection_telnet_keepalive {
            n if n > 0 => Some(Duration::from_secs(n as u64)),
            _ => None,
        },
        // `telnet.c:128` puts it in `ts.LogDirW`, so it lands wherever the
        // session log would — including the surprise that an empty
        // `LogDefaultPath` sends it to the file-*transfer* directory. See
        // [`crate::logname::term_log_dir`].
        log: s
            .connection_telnet_log
            .then(|| crate::logname::term_log_dir(s).join("TELNET.LOG")),
        // Every field is named now, and there is no `..default()` to fall
        // through to — so a field added to `TelnetParams` is a compile error
        // here rather than a setting the file silently cannot reach.
    }
}

/// `TerminalSpeed` — `ttset.c:1937`, which is one number or two.
///
/// **The output speed's default is the input speed, not 38400.** `GetNthNum`
/// answers 0 for a field that is not there (`ttlib_static_cpp.cpp:1182`) and
/// the `i > 0` below it then assigns `ts->TerminalInputSpeed`, so
/// `TerminalSpeed=57600` is 57600 in both directions rather than 57600 one way
/// and the default the other. That relationship is why the schema holds this
/// as a string: it has no way to spell "the default is the other field".
fn terminal_speed(s: &Settings) -> (u32, u32) {
    let field = |n: usize| -> i32 {
        s.connection_terminal_speed
            .split(',')
            .nth(n)
            .map(str::trim)
            .and_then(|f| f.parse::<i32>().ok())
            .unwrap_or(0)
    };
    let input = match field(0) {
        n if n > 0 => n as u32,
        _ => 38400,
    };
    let output = match field(1) {
        n if n > 0 => n as u32,
        _ => input,
    };
    (input, output)
}

/// `ts` plus TTSSH's options → [`SshParams`].
pub fn ssh_params(ssh: &SshOptions, s: &Settings, host: &str, cols: u16, rows: u16) -> SshParams {
    let port = match s.connection_tcp_port == s.connection_telnet_port {
        // No port was chosen for a protocol — see `Target::of`.
        true => SSH_PORT,
        false => s.connection_tcp_port.clamp(0, i32::from(u16::MAX)) as u16,
    };
    let user = ssh.username.as_ref().map(|u| text(u)).unwrap_or_default();
    let mut p = SshParams {
        cols,
        rows,
        // The same key telnet answers `TERMINAL-TYPE` with: TTSSH puts
        // `ts.TermType` straight into the `pty-req` (`ssh.c:8593`) rather than
        // having one of its own, so a user who sets it gets it on both
        // transports — which is the reason it is on `connection` and not on a
        // telnet page.
        term: s.connection_term_type.clone(),
        identities: ssh
            .key_file
            .as_ref()
            .map(|k| vec![PathBuf::from(text(k))])
            .unwrap_or_default(),
        ..SshParams::new(host, port, user)
    };
    // `/TIMEOUT=` is `ts.ConnectingTimeout`, and it is a *TCP connect* timeout
    // rather than a telnet one — the socket is Tera Term's own however TTSSH
    // uses it afterwards. Zero is "let the stack decide", which for telnet
    // becomes a number longer than any SYN budget; here it leaves the
    // transport's own, because 300 seconds of an unanswered key exchange is
    // not what anybody meant by leaving the option out.
    if s.connection_timeout > 0 {
        p.connect_timeout = timeout(s);
    }
    p
}

/// `ts.ConnectingTimeout`, in seconds, where **zero means "let the stack
/// decide"** — which a `connect_timeout` cannot express, so it becomes a number
/// longer than any stack's own SYN retry budget.
fn timeout(s: &Settings) -> Duration {
    match s.connection_timeout {
        n if n > 0 => Duration::from_secs(n as u64),
        _ => Duration::from_secs(300),
    }
}

fn text(v: &[u8]) -> String {
    String::from_utf8_lossy(v).into_owned()
}

/// Where the process is, or `None` — which [`PtyParams`] reads as the home
/// directory, and which is the better answer when there is no current
/// directory to inherit because it has been deleted underneath us.
fn working_dir() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_config::cmdline::DEFAULT_MAX_COM_PORT;

    /// A command line, applied, and then resolved — which is the order the
    /// frontend has to use and the order upstream uses.
    fn startup(line: &str) -> Startup {
        let (cmd, ssh) = tt_config::cmdline::ssh::parse_both(line.as_bytes(), DEFAULT_MAX_COM_PORT);
        let mut s = Settings::default();
        cmd.apply(&mut s);
        Startup::of(&cmd, &ssh, &s, 80, 24)
    }

    /// The three answers, and which test belongs to which transport.
    #[test]
    fn a_line_that_names_nothing_asks_rather_than_connecting() {
        assert_eq!(startup("ttermpro"), Startup::Dialog);
        // `/DS` is how a session that will `connect` for itself starts.
        assert_eq!(startup("ttermpro /DS"), Startup::Idle);
        // A TCP session is decided by the host name...
        assert_eq!(startup("ttermpro /T=1 /P=23"), Startup::Dialog);
        // ...and a serial one by `ComAutoConnect`, which `/M` turns off.
        assert!(matches!(
            startup("ttermpro /C=1"),
            Startup::Open(Target::Serial { .. }) | Startup::Unsupported(_)
        ));
        // ...and which an explicit, *in-range* `/C=` turns back on — in either
        // order, because that runs after the loop rather than in it. A `/C=`
        // above `MaxComPort` is dropped, so it does not.
        assert_eq!(startup("ttermpro /C=999 /M=x /DS"), Startup::Idle);
        assert!(!matches!(startup("ttermpro /M=x /C=1 /DS"), Startup::Idle));
        assert!(!matches!(startup("ttermpro /C=1 /M=x /DS"), Startup::Idle));
        // A host name with a macro still connects: `ComAutoConnect` is only
        // consulted for serial.
        assert!(matches!(
            startup("ttermpro myhost /M=x"),
            Startup::Open(Target::Telnet { .. })
        ));
    }

    #[test]
    fn a_host_name_and_a_port_become_a_telnet_target() {
        let Startup::Open(Target::Telnet {
            host,
            port,
            params,
            timeout,
        }) = startup("ttermpro myhost:2323 /TIMEOUT=5")
        else {
            panic!("expected telnet");
        };
        assert_eq!((host.as_str(), port), ("myhost", 2323));
        assert_eq!(timeout, Duration::from_secs(5));
        assert_eq!((params.cols, params.rows), (80, 24));
        // Not the telnet port, so no opening burst — a terminal server's
        // per-line port is not a telnet server, which is upstream's rule and
        // not a guess. The framing is still on, because `Telnet=` is.
        assert_eq!(params.mode, TelnetMode::Framed);
        assert!(!params.binary);

        let Startup::Open(Target::Telnet { params, .. }) = startup("ttermpro myhost") else {
            panic!("expected telnet");
        };
        assert_eq!(params.mode, TelnetMode::Negotiate, "port 23 does negotiate");

        // **`/T=0` is not raw**, which is the trap the whole family turns on:
        // it clears `ts.Telnet` and says nothing about `TelAutoDetect`, which
        // ships on — so `ttcmn.c:590` still turns the framing on at the first
        // `0xFF`. Raw needs the second key as well.
        let Startup::Open(Target::Telnet { params, .. }) = startup("ttermpro myhost /T=0") else {
            panic!("expected telnet");
        };
        assert_eq!(params.mode, TelnetMode::Auto);

        let off = Settings {
            connection_telnet: false,
            connection_telnet_auto_detect: false,
            ..Settings::default()
        };
        assert_eq!(
            telnet_params(&off, 23, 80, 24).mode,
            TelnetMode::Raw,
            "and only then is every byte data"
        );

        // `/B` asks for binary in the opening burst.
        let Startup::Open(Target::Telnet { params, .. }) = startup("ttermpro myhost /B") else {
            panic!("expected telnet");
        };
        assert!(params.binary);

        // A timeout of zero is "let the stack decide", which is not five
        // seconds and not immediate.
        let Startup::Open(Target::Telnet { timeout, .. }) = startup("ttermpro myhost") else {
            panic!("expected telnet");
        };
        assert!(timeout > Duration::from_secs(60));
    }

    /// **The divergence**: upstream would send SSH to the telnet port.
    #[test]
    fn ssh_with_no_port_is_22_and_not_the_files_23() {
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro /ssh /user=me myhost")
        else {
            panic!("expected ssh");
        };
        assert_eq!(params.port, 22, "upstream would connect to 23 and fail");
        assert_eq!(
            (params.host.as_str(), params.user.as_str()),
            ("myhost", "me")
        );

        // ...and it says so, because a consumer that reads `~/.ssh/config` has
        // a better fallback than this one and needs to know it may use it.
        let Startup::Open(Target::Ssh { port_chosen, .. }) = startup("ttermpro /ssh myhost") else {
            panic!("expected ssh");
        };
        assert!(!port_chosen);

        // A port that *was* asked for wins, which is what keeps the divergence
        // narrow: it only fires when nothing chose one.
        let Startup::Open(Target::Ssh {
            params,
            port_chosen,
            ..
        }) = startup("ttermpro /ssh myhost:2222")
        else {
            panic!("expected ssh");
        };
        assert_eq!((params.port, port_chosen), (2222, true));
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro /ssh /P=2222 myhost")
        else {
            panic!("expected ssh");
        };
        assert_eq!(params.port, 2222);

        // A URL brings its own `:22`, so it never reaches the special case.
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro ssh://me@myhost/") else {
            panic!("expected ssh");
        };
        assert_eq!((params.port, params.user.as_str()), (22, "me"));
    }

    #[test]
    fn the_ssh_options_a_frontend_has_to_act_on_survive() {
        let Startup::Open(Target::Ssh {
            params,
            password,
            method,
            ask_password,
            no_known_hosts_check,
            ..
        }) = startup(
            "ttermpro /ssh /auth=publickey /user=me /passwd=pw /keyfile=/tmp/k \
             /nosecuritywarning myhost",
        )
        else {
            panic!("expected ssh");
        };
        assert_eq!(password.as_deref(), Some("pw"));
        assert_eq!(method, Some(AuthMethod::PublicKey));
        assert!(!ask_password && no_known_hosts_check);
        assert_eq!(params.identities, [PathBuf::from("/tmp/k")]);
        assert!(startup("ttermpro /ssh /ask4passwd h").eq(&startup("ttermpro /ssh /ask4passwd h")));

        // `/TIMEOUT=` is a TCP connect timeout, so it reaches SSH too — and
        // leaving it out leaves the transport's own rather than the five
        // minutes "let the stack decide" means for telnet.
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro /ssh /TIMEOUT=7 h")
        else {
            panic!("expected ssh");
        };
        assert_eq!(params.connect_timeout, Duration::from_secs(7));
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro /ssh h") else {
            panic!("expected ssh");
        };
        assert!(params.connect_timeout < Duration::from_secs(300));
    }

    /// The serial parameters come from the settings, which the command line has
    /// already been applied to — so this is one path and not two.
    #[test]
    fn the_serial_parameters_come_through_the_settings() {
        let (cmd, ssh) = tt_config::cmdline::ssh::parse_both(
            b"ttermpro /C=1 /SPEED=115200 /CPARITY=even /CDATABIT=7 /CSTOPBIT=2 /CFLOWCTRL=rtscts",
            DEFAULT_MAX_COM_PORT,
        );
        let mut s = Settings::default();
        cmd.apply(&mut s);
        let params = serial_params(&s);
        assert_eq!(params.baud, 115_200);
        assert_eq!(params.data_bits, DataBits::Seven);
        assert_eq!(params.parity, Parity::Even);
        assert_eq!(params.stop_bits, StopBits::Two);
        assert_eq!(params.flow, FlowControl::RtsCts);
        // Whether there *is* a port with that number is the rig's business, not
        // the resolution's — but the answer must be one of the two, never a
        // silently different device.
        match Startup::of(&cmd, &ssh, &s, 80, 24) {
            Startup::Open(Target::Serial { path, params }) => {
                assert!(path.starts_with("/dev/"));
                assert_eq!(params.baud, 115_200);
            }
            Startup::Unsupported(why) => assert!(why.contains("no serial port")),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// The control lines' `-1` is not a value — it is "derive from the flow
    /// control", and each derives from a *different* one of the four.
    #[test]
    fn the_control_lines_derive_from_the_flow_control_they_belong_to() {
        let with = |flow| {
            let s = Settings {
                serial_flow: flow,
                ..Settings::default()
            };
            let p = serial_params(&s);
            (p.rts, p.dtr)
        };
        // Neither line is the flow control's, so both are simply asserted.
        assert_eq!(
            with(SerialFlow::None),
            (PinControl::Enable, PinControl::Enable)
        );
        assert_eq!(
            with(SerialFlow::XonXoff),
            (PinControl::Enable, PinControl::Enable)
        );
        // `hard` is RTS/CTS, so RTS becomes the driver's and DTR does not.
        assert_eq!(
            with(SerialFlow::Hardware),
            (PinControl::Handshake, PinControl::Enable)
        );
        // ...and `dsrdtr` the other way round, which is the half that is easy
        // to write as one condition and get wrong for both.
        assert_eq!(
            with(SerialFlow::DsrDtr),
            (PinControl::Enable, PinControl::Handshake)
        );

        // A number in the file overrides the derivation, including one that
        // contradicts the flow control — upstream's dialog writes both fields
        // independently and this is what a saved file looks like.
        let mut s = Settings {
            serial_flow: SerialFlow::Hardware,
            serial_rts: 0,
            ..Settings::default()
        };
        assert_eq!(serial_params(&s).rts, PinControl::Disable);

        // Toggle is RTS's fourth value and DTR has no such thing; nothing here
        // stops the file naming it for DTR, and `tt-conn` treats it as the
        // driver's line either way.
        s.serial_rts = 3;
        assert_eq!(serial_params(&s).rts, PinControl::Toggle);

        // And the one Win32 would refuse. Upstream hands 9 to `SetCommState`
        // and ignores the failure, which discards every other serial setting
        // with it; this reads it as Enable and keeps the rest of the file.
        s.serial_rts = 9;
        assert_eq!(serial_params(&s).rts, PinControl::Enable);
    }

    /// What the terminal claims to be, on both transports — and the speed's
    /// second field, whose default is its first.
    #[test]
    fn the_terminal_type_and_speed_come_out_of_the_file() {
        let of = |bytes: &[u8]| Settings::load(&tt_config::Ini::parse(bytes));

        let d = telnet_params(&Settings::default(), 23, 80, 24);
        assert_eq!(d.term_type, "xterm", "upstream's, not this crate's");
        assert_eq!(d.speed, (38400, 38400));

        let s = of(b"[Tera Term]\r\nTermType=vt220\r\nTerminalSpeed=57600\r\n");
        let p = telnet_params(&s, 23, 80, 24);
        assert_eq!(p.term_type, "vt220");
        assert_eq!(
            p.speed,
            (57600, 57600),
            "the output speed's default is the input speed"
        );
        // ...and the same key reaches SSH, because TTSSH has none of its own.
        let ssh = ssh_params(&SshOptions::default(), &s, "h", 80, 24);
        assert_eq!(ssh.term, "vt220");

        let two = telnet_params(
            &of(b"[Tera Term]\r\nTerminalSpeed=57600,19200\r\n"),
            23,
            80,
            24,
        );
        assert_eq!(two.speed, (57600, 19200));

        // Zero or less is the default for the first field and the first field
        // for the second, so this is not a terminal at 0 baud.
        let zero = telnet_params(&of(b"[Tera Term]\r\nTerminalSpeed=0,0\r\n"), 23, 80, 24);
        assert_eq!(zero.speed, (38400, 38400));
    }

    /// The two transports upstream has and this does not say so, rather than
    /// opening something else.
    #[test]
    fn a_transport_this_port_does_not_have_says_which() {
        assert!(matches!(
            startup("ttermpro /R=session.log"),
            Startup::Unsupported(w) if w.contains("replaying")
        ));
        assert!(matches!(
            startup(r"ttermpro /PIPE mypipe"),
            Startup::Unsupported(w) if w.contains("named pipes")
        ));
    }

    /// The opener, on the one transport that needs nothing but a fork — and the
    /// one that must refuse.
    #[test]
    fn a_shell_opens_and_ssh_says_who_should_open_it() {
        let shell = Target::Shell(Box::new(PtyParams {
            argv: vec!["sh".into(), "-c".into(), "exit 0".into()],
            login_shell: false,
            ..Default::default()
        }));
        assert!(shell.open().is_ok(), "a local shell needs no prompt");

        let Startup::Open(ssh) = startup("ttermpro /ssh myhost") else {
            panic!("expected ssh");
        };
        let why = match ssh.open() {
            Err(e) => e.to_string(),
            Ok(_) => panic!("ssh must not be opened here"),
        };
        assert!(why.contains("tt_ssh_connect"), "{why}");
    }

    /// The settings file decides when the command line says nothing, which is
    /// the case a shortcut with only a `/F=` in it relies on.
    #[test]
    fn the_file_decides_the_transport_when_the_line_does_not() {
        let cmd = CommandLine::parse(b"ttermpro", DEFAULT_MAX_COM_PORT);
        let mut s = Settings {
            connection_port_type: ConnectionPortType::Serial,
            ..Default::default()
        };
        // Serial, no `/M`, so `ComAutoConnect` is on and it connects with the
        // file's own `ComPort=`.
        assert!(!matches!(
            Startup::of(&cmd, &SshOptions::default(), &s, 80, 24),
            Startup::Dialog
        ));
        s.connection_port_type = ConnectionPortType::TcpIp;
        // TCP with no host name, though, has nothing to connect to.
        assert_eq!(
            Startup::of(&cmd, &SshOptions::default(), &s, 80, 24),
            Startup::Dialog
        );
    }

    // ---- what a macro's `connect` opens ----

    fn connect(arg: &str, s: &mut Settings) -> Startup {
        Startup::of_connect(arg.as_bytes(), s, 80, 24).0
    }

    /// The whole point of the dummy first token: a macro's argument has no
    /// program name in it, and `_ParseParam` throws its first token away.
    #[test]
    fn a_bare_host_name_is_a_host_name_and_not_a_discarded_program() {
        let mut s = Settings::default();
        let Startup::Open(Target::Telnet { host, port, .. }) = connect("myhost:2323", &mut s)
        else {
            panic!("expected telnet");
        };
        assert_eq!((host.as_str(), port), ("myhost", 2323));
    }

    /// TTSSH's half runs for a `connect` too, which reading `ttdde.c` alone
    /// would not tell you — and it is what most of the documentation's own
    /// examples for the command need.
    #[test]
    fn the_plugins_half_of_the_line_is_parsed_as_well() {
        let mut s = Settings::default();
        let Startup::Open(Target::Ssh { params, .. }) =
            connect("myhost /ssh /auth=password /user=alice", &mut s)
        else {
            panic!("expected ssh");
        };
        assert_eq!((params.host.as_str(), params.port), ("myhost", 22));
        assert_eq!(params.user, "alice");
    }

    /// The argument is written into the settings and stays there, which is
    /// `ParseParam(commandline, &ts, NULL)` and is why this takes `&mut`.
    #[test]
    fn what_the_line_set_outlives_the_connection_it_was_given_for() {
        let mut s = Settings::default();
        assert_eq!(s.serial_baud, 9600);
        // `/C=` with no port to open still applied the speed on its way past.
        connect("/C=1 /BAUD=115200", &mut s);
        assert_eq!(s.serial_baud, 115200);
        // And a second `connect` that says nothing about it keeps it.
        connect("myhost", &mut s);
        assert_eq!(s.serial_baud, 115200);
    }

    /// `connect ''` names nothing, so it is the dialog or an idle terminal —
    /// the same two arms a bare `ttermpro` gets, decided by the same setting.
    #[test]
    fn a_connect_with_nothing_in_it_asks_rather_than_opening() {
        let mut s = Settings::default();
        assert_eq!(connect("", &mut s), Startup::Dialog);
        s.connection_host_dialog_on_startup = false;
        assert_eq!(connect("", &mut s), Startup::Idle);
    }

    /// `MaxComPort` is the file's, so the same `connect` line opens a port on
    /// one machine and nothing on another. Upstream reads it out of `ts` at the
    /// same moment.
    #[test]
    fn the_com_port_bound_comes_from_the_settings() {
        let mut s = Settings {
            connection_host_dialog_on_startup: false,
            ..Default::default()
        };
        // Above the default bound, so the option is dropped — and with it the
        // auto-connect that an in-range `/C=` would have turned back on after
        // the `/M=`.
        assert_eq!(connect("/C=300 /M=x", &mut s), Startup::Idle);
        s.serial_max_com_port = 512;
        assert_ne!(connect("/C=300 /M=x", &mut s), Startup::Idle);
    }

    // ---- and what `cygconnect` opens ----

    fn pty(arg: &str) -> PtyParams {
        match Target::cygterm(arg.as_bytes(), 100, 30) {
            Target::Shell(p) => *p,
            other => panic!("expected a shell, got {other:?}"),
        }
    }

    #[test]
    fn cygconnect_with_no_arguments_is_a_login_shell_where_we_are() {
        let p = pty("");
        assert!(p.argv.is_empty(), "the account's own shell");
        assert!(p.login_shell, "cygterm.cfg ships LOGIN_SHELL = Yes");
        assert_eq!(
            p.cwd,
            std::env::current_dir().ok(),
            "not the home directory"
        );
        assert_eq!((p.cols, p.rows), (100, 30));
    }

    /// The two splitters, in the order they run: the line's takes the outer
    /// quotes off and `get_argv` takes the inner ones, which is how a shell
    /// command with an argument that has a space in it survives at all.
    #[test]
    fn the_shell_string_is_split_and_the_flags_are_carried() {
        let p = pty("-s \"'/bin/sh' -c 'echo hi'\" -nols -dumb -v FOO=bar");
        assert_eq!(p.argv, ["/bin/sh", "-c", "echo hi"]);
        assert!(!p.login_shell);
        assert_eq!(p.term, "dumb");
        assert_eq!(p.env, [("FOO".to_string(), "bar".to_string())]);
    }

    /// `-cd` is the *only* way to ask for the home directory, and `-d` outranks
    /// it. A directory that is not there is ignored rather than fatal, because
    /// upstream's `chdir` failure is a message and not a refusal.
    #[test]
    fn the_directory_options_decide_where_the_shell_starts() {
        assert_eq!(pty("-cd").cwd, None, "None is the home directory");
        assert_eq!(pty("-d /tmp").cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(pty("-cd -d /tmp").cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(
            pty("-d /no/such/directory").cwd,
            std::env::current_dir().ok(),
            "a directory that is not there is not a failed connection"
        );
    }

    /// It opens, which is the thing a macro's `cygconnect` needs and the one
    /// transport that needs nothing installed to prove it.
    #[test]
    fn what_cygconnect_produces_can_be_opened() {
        let target = Target::cygterm(b"-s '/bin/sh -c :' -nols", 80, 24);
        assert!(target.open().is_ok());
    }
}
