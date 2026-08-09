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
use tt_config::cmdline::{CommandLine, PortType};
use tt_config::Settings;
use tt_config::{ConnectionPortType, SerialDataBits, SerialFlow, SerialParity, SerialStopBits};
use tt_conn::pty::PtyParams;
use tt_conn::serial::{port_by_number, DataBits, FlowControl, Parity, SerialParams, StopBits};
use tt_conn::ssh::SshParams;
use tt_conn::telnet::{TelnetMode, TelnetParams};

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
        ..SerialParams::default()
    }
}

/// `ts` → [`TelnetParams`].
///
/// `Telnet=off`, which is what `/T=0` sets, is **raw** — that is why raw is a
/// first-class mode rather than a fallback. On it, the mode is
/// [`TelnetMode::for_port`], which is upstream's own rule and not a guess: the
/// opening burst goes out only when the port is 23 (`vtwin.cpp:3666`), because a
/// terminal server's per-line port is not a telnet server and five bytes of
/// negotiation would land in somebody's serial console.
pub fn telnet_params(s: &Settings, port: u16, cols: u16, rows: u16) -> TelnetParams {
    TelnetParams {
        mode: match s.connection_telnet {
            true => TelnetMode::for_port(port),
            false => TelnetMode::Raw,
        },
        binary: s.connection_telnet_binary,
        cols,
        rows,
        ..TelnetParams::default()
    }
}

/// `ts` plus TTSSH's options → [`SshParams`].
pub fn ssh_params(ssh: &SshOptions, s: &Settings, host: &str, cols: u16, rows: u16) -> SshParams {
    let port = match s.connection_tcp_port == s.connection_telnet_port {
        // No port was chosen for a protocol — see `Target::of`.
        true => SSH_PORT,
        false => s.connection_tcp_port.clamp(0, i32::from(u16::MAX)) as u16,
    };
    let user = ssh.username.as_ref().map(|u| text(u)).unwrap_or_default();
    SshParams {
        cols,
        rows,
        identities: ssh
            .key_file
            .as_ref()
            .map(|k| vec![PathBuf::from(text(k))])
            .unwrap_or_default(),
        ..SshParams::new(host, port, user)
    }
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
        // Not 23, so no opening burst — a terminal server's per-line port is
        // not a telnet server, which is upstream's rule and not a guess.
        assert_eq!(params.mode, TelnetMode::Auto);
        assert!(!params.binary);

        let Startup::Open(Target::Telnet { params, .. }) = startup("ttermpro myhost") else {
            panic!("expected telnet");
        };
        assert_eq!(params.mode, TelnetMode::Negotiate, "port 23 does negotiate");

        // `/T=0` is raw rather than telnet, which is the whole reason raw is a
        // first-class mode.
        let Startup::Open(Target::Telnet { params, .. }) = startup("ttermpro myhost /T=0") else {
            panic!("expected telnet");
        };
        assert_eq!(params.mode, TelnetMode::Raw);

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

        // A port that *was* asked for wins, which is what keeps the divergence
        // narrow: it only fires when nothing chose one.
        let Startup::Open(Target::Ssh { params, .. }) = startup("ttermpro /ssh myhost:2222") else {
            panic!("expected ssh");
        };
        assert_eq!(params.port, 2222);
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
}
