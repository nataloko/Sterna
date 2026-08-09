//! The command line, tokenised the way Tera Term tokenises one.
//!
//! `GetParam` and `DequoteParam` (`ttlib.c:879`, `:917`) live here rather than
//! in the crate that first needed them because there are three callers and one
//! of them ships: `ttpmacro`'s launcher (`tt-ttl`), `_ParseParam` (`ttset.c`,
//! this crate) and TTXSSH's hook over it. Upstream puts `_ParseParam` in the
//! same DLL as the INI reader for the same reason — the two are one file
//! format's worth of front door, and the first thing the parser does with a
//! `/F=` is read a settings file.
//!
//! **The tokeniser is upstream's, not the C runtime's.** A backslash is an
//! ordinary character (which it has to be, since these are Windows paths), a
//! `""` inside a quoted run is one literal quote, and an unquoted `;` **ends
//! the command line** — everything after it is a comment. Reaching for
//! `CommandLineToArgvW` semantics gives a parser that agrees on every example
//! in the documentation and disagrees on the first path with a space in it.

pub mod ssh;

use crate::{
    ConnectionPortType, SerialDataBits, SerialFlow, SerialParity, SerialStopBits, Settings,
};

/// `GetParam` (`ttlib.c:879`) — one token and what is left after it.
///
/// Quotes are *kept*: upstream splits with them still in place and takes them
/// out afterwards with [`dequote_param`], which is why a token can come back
/// looking like `"a b"`. `None` is upstream's NULL — the end of the line, or an
/// unquoted `;`, which is a comment and ends it early.
///
/// `size` is the caller's buffer, counted the way the function counts it: a
/// token is truncated at `size - 1` characters. `_ParseParam` and `ttpmacro`
/// both pass 512; TTXSSH allocates one as long as the line and so never
/// truncates.
pub fn get_param(param: &[u8], size: usize) -> Option<(Vec<u8>, &[u8])> {
    let mut i = 0;
    while matches!(param.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    match param.get(i) {
        None | Some(b';') => return None,
        _ => {}
    }

    let mut buf = Vec::new();
    let mut quoted = false;
    while let Some(&b) = param.get(i) {
        if !quoted && matches!(b, b';' | b' ' | b'\t') {
            break;
        }
        if b == b'"' {
            if param.get(i + 1) != Some(&b'"') {
                quoted = !quoted;
            } else {
                // `""`: the first quote is copied here and the second by the
                // unconditional copy below, so a doubled quote survives
                // tokenising whole and does not toggle the state.
                push_capped(&mut buf, b'"', size);
                i += 1;
            }
        }
        push_capped(&mut buf, param[i], size);
        i += 1;
    }
    // Upstream drops a trailing `;` here — `if (!quoted && buff[i-1] == ';')`.
    // It cannot fire: a `;` only reaches the buffer while `quoted`, and nothing
    // between that copy and the loop test can clear the flag. Transcribed as a
    // comment rather than as an unreachable branch, and it is also where the
    // function reads `buff[-1]` if it is ever called with a size of 1.
    Some((buf, &param[i..]))
}

fn push_capped(buf: &mut Vec<u8>, b: u8, size: usize) {
    if buf.len() + 1 < size {
        buf.push(b);
    }
}

/// `DequoteParam` (`ttlib.c:917`) — take the quotes back out.
///
/// A quote toggles the state and vanishes; a `""` *inside* a quoted run is one
/// literal quote. So `"a b"` is `a b`, `""` is the empty string — which
/// `params_array.bat` passes deliberately — and `""""` is a single `"`.
///
/// No length cap: every caller dequotes in place into the buffer
/// [`get_param`] just filled, and taking quotes out cannot lengthen a token.
pub fn dequote_param(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut quoted = false;
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'"' {
            out.push(src[i]);
            i += 1;
            continue;
        }
        i += 1;
        if quoted && src.get(i) == Some(&b'"') {
            out.push(b'"');
            i += 1;
        } else {
            quoted = !quoted;
        }
    }
    out
}

/// Every token of a command line, dequoted — the loop both `_ParseParam` and
/// `TTXParseParam` open with, minus the first term.
///
/// "The first term shuld be executable filename of Tera Term", says the comment
/// above the untested `GetParam` that discards it. A line consisting of nothing
/// but that term yields no tokens.
pub fn tokens(line: &[u8], size: usize) -> Vec<Vec<u8>> {
    token_spans(line, size)
        .into_iter()
        .map(|(_, t)| t)
        .collect()
}

/// The same, each token paired with the span of the line it came out of —
/// **including the whitespace in front of it**, because that is the span
/// TTXSSH blanks when it takes an option away (`ttxssh.c:1521`).
///
/// `cur` in upstream's loop is where the *previous* token ended, not where this
/// one begins, so `wmemset(cur, ' ', next-cur)` covers the separator too. That
/// is what makes `OPTION_REPLACE` safe: it writes the new text at `cur + 1`,
/// one character in, and relies on there having been at least one space.
pub fn token_spans(line: &[u8], size: usize) -> Vec<(std::ops::Range<usize>, Vec<u8>)> {
    let mut out = Vec::new();
    let mut cur = match get_param(line, size) {
        Some((_, rest)) => line.len() - rest.len(),
        None => return out,
    };
    while let Some((tok, rest)) = get_param(&line[cur..], size) {
        let next = line.len() - rest.len();
        out.push((cur..next, dequote_param(&tok)));
        cur = next;
    }
    out
}

/// `MaxStrLen` (`ttset.c:75`) — the buffer `_ParseParam` reads a token into,
/// with `_countof` passed correctly, so a token is truncated at 511 characters.
///
/// `ttpmacro` defines the same constant at the same value (`ttmdef.h:34`) and
/// passes `sizeof` instead, which is the overflow listed in `PLAN.md`.
const MAX_STR_LEN: usize = 512;

/// `TopicName` is `char[21]` at `ttdde.c`'s end of the wire, so `/D=` keeps
/// twenty characters. **Not the same as `ttpmacro`'s ten** (`ttmdlg.cpp:62`):
/// the launcher's buffer is `wchar_t[11]` and the terminal's is 21 bytes of
/// ACP, so the two ends of one topic name truncate differently.
const MAX_TOPIC_LEN: usize = 20;

/// `TitleBuffSize` (`tttypes.h:269`) — `/W=` keeps 49 characters of a title.
const TITLE_LEN: usize = 49;

/// `MAX_PATH`, which is what `ts.MulticastName` is.
const MULTICAST_NAME_LEN: usize = 259;

/// `ts.HostName` is `char[1024]` (`tttypes.h:301`).
const HOST_NAME_LEN: usize = 1023;

/// `MAXCOMPORT` (`tttypes.h:908`) — the ceiling `ts.MaxComPort` is itself
/// clamped to, and so the widest `/C=` that can ever be accepted.
pub const MAX_COM_PORT: u16 = 4096;

/// `ts.MaxComPort`'s own default (`ttset.c:1218`) — 256, floored at 4 and
/// capped at [`MAX_COM_PORT`]. `/C=` is bounded against this setting rather
/// than against the hardware, so a port number above it is **dropped** and the
/// command line still selects the serial transport.
pub const DEFAULT_MAX_COM_PORT: u16 = 256;

/// `ts.PortType` (`tttypes.h:100`) — which transport the command line chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    TcpIp,
    Serial,
    File,
    NamedPipe,
}

/// `ts.MacroFNW` after a command line has had its say, which has three states
/// and not two.
///
/// [`MacroArg::Cleared`] is the one that is easy to miss: `StartupMacro` is an
/// INI setting (`ttset.c:1291`), and a `/D=` with a topic in it **frees the
/// name unconditionally** (`ttset.c:3963`) — so a terminal launched by a macro
/// does not also run the startup macro. Without that a macro would open a
/// window that opens another macro.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MacroArg {
    /// No `/M`, so whatever the settings file said stands.
    #[default]
    Unset,
    /// A `/D=` topic was given: forget the settings file's macro too.
    Cleared,
    /// `/M`, or `/M=` with nothing or a `*` after it — ask which macro to run.
    Prompt,
    /// `/M=<name>`, before [`file_path`] has been applied to it.
    File(Vec<u8>),
}

/// `ts.CtrlFlag & CSF_CBMASK` (`tttypes.h:223`) — what OSC 52 may do.
///
/// `CSF_CBNONE` is 0 and `/OSC52=` clears the mask before matching, so a value
/// upstream does not recognise is not a fifth state: `off` and `nonsense` both
/// arrive here as [`ClipboardAccess::None`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardAccess {
    None,
    Read,
    Write,
    ReadWrite,
}

/// `ts.ProtocolFamily` — `/4` and `/6`, which are `AF_INET` and `AF_INET6`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolFamily {
    V4,
    V6,
}

/// The four serial settings `/C…` can name, spelled the way
/// `serial_conf_databit` and friends spell them (`ttset.c:87`).
///
/// These are the INI's own spellings too — `ttset.c:924` reads `Parity=`,
/// `DataBit=`, `StopBit=` and `FlowCtrl=` through the same table — and a value
/// that matches nothing leaves the setting alone rather than taking a default,
/// because `SerialPortConfconvertStr2Id` returns FALSE without storing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataBits {
    Seven,
    Eight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    None,
    Odd,
    Even,
    Mark,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopBits {
    One,
    Two,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowControl {
    /// `IdFlowX`, spelled `x`.
    XonXoff,
    /// `IdFlowHard`, spelled `hard` **or** `rtscts` — the one alias in any of
    /// the four tables (`ttset.c:111`).
    Hardware,
    None,
    /// `IdFlowHardDsrDtr`, spelled `dsrdtr`.
    DsrDtr,
}

/// What a Tera Term command line asked for — `_ParseParam` (`ttset.c:3654`),
/// with nothing dropped.
///
/// Every field is what the line *said*, not what a running terminal would make
/// of it: paths are unresolved (see [`file_path`], which is upstream's
/// `GetFilePath` and needs directories this crate has no opinion about), and
/// the four settings whose subsystems this port has not built keep their
/// strings. A parser that quietly discarded the options it could not apply
/// would be indistinguishable from one that had a bug in them.
///
/// **Two passes, because a `/F=` has to win before anything is layered on
/// it.** The first pass looks for a settings file and stops at the first one it
/// finds; upstream reads that file *inside* the loop, so every later option is
/// applied over the new contents rather than the old. [`CommandLine::parse`]
/// only records the name — reading it is the caller's, which is also the only
/// way `connect`'s "did the setup file change?" test (`ttdde.c:620`) can work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLine {
    /// The line as handed over, first term and all.
    pub raw: Vec<u8>,

    /// `/F=` — the settings file, from the first pass. `GetFilePath` against
    /// the home directory with a default extension of `.INI`.
    pub setup_file: Option<Vec<u8>>,
    /// `/D=` — the DDE topic, twenty characters of it. This port has no DDE;
    /// what still matters is [`MacroArg::Cleared`], and that `connect` passes
    /// NULL here (`ttdde.c:617`) so a `/D=` inside a `connect` string does
    /// nothing at all.
    pub dde_topic: Option<Vec<u8>>,

    /// `ParamPort` — which of the four transports the options settled on.
    /// `None` when the line named none, in which case the settings file's
    /// `Port=` stands.
    pub port_type: Option<PortType>,
    /// `ts.HostName`, which `_ParseParam` **always clears first** — a host name
    /// is never inherited from the settings file. Holds `/R=`'s path when the
    /// port type is [`PortType::File`], and the mangled `\\.\pipe\…` form when
    /// it is [`PortType::NamedPipe`].
    pub host_name: Vec<u8>,
    /// `ParamTCP`, if it was not zero. `/P=`, or a bare token straight after
    /// the host name, or the `:port` a host name carried.
    pub tcp_port: Option<u16>,
    /// `ParamTel`, if it was given at all — `/T=0` and `/T=1`. `/T=2` is
    /// TTSSH's and Tera Term ignores it.
    pub telnet: Option<bool>,
    /// `ParamBin` — `/B`, telnet binary mode.
    pub telnet_binary: Option<bool>,
    /// `ParamCom`, if it was in range. `/C=`.
    pub com_port: Option<u16>,
    /// `ParamBaud` — `/SPEED=`, or `/BAUD=` which is the same thing spelled
    /// the way it was spelled before 4.60.
    pub baud: Option<u32>,

    pub data_bits: Option<DataBits>,
    pub parity: Option<Parity>,
    pub stop_bits: Option<StopBits>,
    pub flow: Option<FlowControl>,
    /// `/CDELAYPERCHAR=`, in milliseconds, as a `WORD`.
    pub delay_per_char: Option<u16>,
    /// `/CDELAYPERLINE=`.
    pub delay_per_line: Option<u16>,
    /// `/WAITCOM` — open the serial port when it turns up rather than failing.
    pub wait_com: bool,

    /// `/AUTOWINCLOSE=on|off`, and **anything that is not `on` is off** — this
    /// arm is a plain `_wcsicmp` against `on` with an `else`, not `GetOnOff`,
    /// so `/AUTOWINCLOSE=1` switches it *off*.
    pub auto_win_close: Option<bool>,
    /// `/DS` (suppress) and `/ES` (show) — the New Connection dialog.
    pub host_dialog_on_startup: Option<bool>,
    /// `/E` — turn `TCPLocalEcho` and `TCPCRSend` off for this session.
    pub disable_tcp_echo_cr: bool,
    /// `/FD=` — the file transfer directory. Upstream applies it **only if the
    /// folder exists** (`DoesFolderExistW`, `ttset.c:3794`) and silently drops
    /// it otherwise; that check is the applier's, since it is the one thing
    /// here that needs a filesystem.
    pub file_dir: Option<Vec<u8>>,
    /// `/H` — no title bar.
    pub hide_title: bool,
    /// `/I` — start minimised.
    pub minimize: bool,
    /// `/V` — no window at all. Note that `ttpmacro`'s `/V` means something
    /// else again; these are two programs' command lines.
    pub hide_window: bool,
    /// `/K=` — the keyboard file, default extension `.CNF`.
    pub key_cnf_file: Option<Vec<u8>>,
    /// `/KR=` and `/KT=` — the receive and send character sets, **unresolved**.
    /// `GetKanjiCodeFromStr` (`ttlib_charset.cpp:147`) is a case-*sensitive*
    /// match over a 55-entry table with a default of UTF-8, and neither
    /// `tt-charset` nor this schema has the identifiers yet; resolving here
    /// would mean a second copy of that table to keep in step.
    pub kanji_recv: Option<Vec<u8>>,
    pub kanji_send: Option<Vec<u8>>,
    /// `/L=` — the log file, resolved against the *log* directory rather than
    /// the home one, and with no default extension.
    pub log_file: Option<Vec<u8>>,
    /// `/NOLOG` — no automatic logging, and **a deliberate divergence when
    /// `/L=` is given too.**
    ///
    /// Upstream's arm clears `LogAutoStart` and the *ANSI* copy of the name,
    /// `ts.LogFN` (`ttset.c:3850`) — but the wide `ts.LogFNW` is the one that
    /// counts, and `vtwin.cpp:3631` starts logging when
    /// `ts.LogAutoStart || ts.LogFNW != NULL`. So
    /// `ttermpro /L=out.log /NOLOG` **logs to `out.log`**, which is the one
    /// thing the option exists to prevent; `teraterm.html` says only "start
    /// Tera Term without logging". Here `/NOLOG` wins, as the manual says, and
    /// a consumer that has both must let it — the twenty-fifth upstream defect
    /// on file, and the second where the code and the documentation disagree
    /// rather than the code and this port.
    pub no_log: bool,
    /// `/MN=` — the name this window answers to for `sendmulticast`.
    pub multicast_name: Option<Vec<u8>>,
    /// `/M=`, `/M`, and the third state a `/D=` topic puts it in.
    pub macro_file: MacroArg,
    /// `ts.ComAutoConnect`, which starts **true** on every call — so a command
    /// line always overrides whatever the settings file said — and is turned
    /// off by a macro and back on by an explicit `/C=`.
    pub com_auto_connect: bool,
    /// `/OSC52=`.
    pub clipboard: Option<ClipboardAccess>,
    /// `/TEKICON=` and `/VTICON=` — icon names, unresolved, for the same
    /// reason as the character sets: `IconName2IconId` (`ttset.c:256`) is ten
    /// names and a default, and this port has no icon table to map them onto.
    pub tek_icon: Option<Vec<u8>>,
    pub vt_icon: Option<Vec<u8>>,
    /// `/THEME=` — a background theme file, which also switches `BGEnable` on.
    pub theme_file: Option<Vec<u8>>,
    /// `/W=` — the window title, 49 characters of it.
    pub title: Option<Vec<u8>>,
    /// `/X=` and `/Y=`, each as given.
    ///
    /// Upstream pairs them: setting one puts the other at 0 **if it is still
    /// `CW_USEDEFAULT`** (`ttset.c:3917`), because a real coordinate in one
    /// axis and "wherever you like" in the other is not a position Windows
    /// will take. That test is against `ts.VTPos`, which comes out of the
    /// settings file — so whether `/X=100` moves the window vertically depends
    /// on a `VTPos=` the user may have. It belongs to the applier, which has
    /// the settings; deciding it here would be right only for a default file.
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    /// `/4` and `/6`.
    pub protocol_family: Option<ProtocolFamily>,
    /// `/DUPLICATE`.
    pub duplicate_session: bool,
    /// `/TIMEOUT=`, in seconds. Negative values are ignored rather than
    /// clamped, and a non-numeric one is ignored too.
    pub connecting_timeout: Option<i32>,
}

impl Default for CommandLine {
    /// An empty command line, which is **not** all-zero: `ComAutoConnect` is
    /// assigned TRUE at the top of `_ParseParam` before a token is read
    /// (`ttset.c:3672`), so "nothing was asked for" means auto-connect on.
    fn default() -> Self {
        CommandLine {
            raw: Vec::new(),
            setup_file: None,
            dde_topic: None,
            port_type: None,
            host_name: Vec::new(),
            tcp_port: None,
            telnet: None,
            telnet_binary: None,
            com_port: None,
            baud: None,
            data_bits: None,
            parity: None,
            stop_bits: None,
            flow: None,
            delay_per_char: None,
            delay_per_line: None,
            wait_com: false,
            auto_win_close: None,
            host_dialog_on_startup: None,
            disable_tcp_echo_cr: false,
            file_dir: None,
            hide_title: false,
            minimize: false,
            hide_window: false,
            key_cnf_file: None,
            kanji_recv: None,
            kanji_send: None,
            log_file: None,
            no_log: false,
            multicast_name: None,
            macro_file: MacroArg::Unset,
            com_auto_connect: true,
            clipboard: None,
            tek_icon: None,
            vt_icon: None,
            theme_file: None,
            title: None,
            window_x: None,
            window_y: None,
            protocol_family: None,
            duplicate_session: false,
            connecting_timeout: None,
        }
    }
}

impl CommandLine {
    /// `_ParseParam` over a whole command line, first term and all.
    ///
    /// `max_com_port` is `ts.MaxComPort`, which `/C=` is bounded against —
    /// pass [`DEFAULT_MAX_COM_PORT`] unless a settings file has said otherwise.
    /// It is a *setting*, so the same command line can select COM 300 on one
    /// machine and silently no port at all on another.
    pub fn parse(line: &[u8], max_com_port: u16) -> CommandLine {
        CommandLine::parse_inner(line, max_com_port, true)
    }

    /// The argument of a macro's `connect`, which is a command line with no
    /// program in it.
    ///
    /// `ttdde.c:617` prepends a literal `"a "` — "`a` = dummy exe name" — for
    /// exactly the reason this function exists: `_ParseParam` throws its first
    /// token away, so `connect 'myhost'` would otherwise connect to nothing at
    /// all. It also passes **NULL** for the DDE topic, which is not a detail:
    /// with no buffer to write into, `/D=` is ignored *and* the startup macro
    /// survives.
    pub fn parse_argument(arg: &[u8], max_com_port: u16) -> CommandLine {
        let mut line = b"a ".to_vec();
        line.extend_from_slice(arg);
        CommandLine::parse_inner(&line, max_com_port, false)
    }

    /// Write what was asked for into the settings, leaving the rest alone.
    ///
    /// This is the half of `_ParseParam` that is an assignment to `ts`, and it
    /// is separate here for a reason upstream does not have: the schema does
    /// not hold every setting the parser can name yet, and a parser that
    /// silently dropped the difference would be indistinguishable from one with
    /// a bug in those options. What [`CommandLine`] holds is the whole answer;
    /// what this writes is the part the file can carry today.
    ///
    /// **Deliberately not applied**, each because the thing underneath does not
    /// exist rather than because the option was ignored:
    ///
    /// - `host_name` and `tcp_port`'s companion — a host name is **not a
    ///   setting**. `ts.HostName` has no INI key and `_ParseParam` clears it on
    ///   every call, so where to connect is the session's and not the file's.
    ///   `tcp_port` *is* a setting and is written.
    /// - `/KR=`, `/KT=`, `/VTICON=`, `/TEKICON=`, `/THEME=`, `/K=`, `/MN=`,
    ///   `/M`, `/D=`, `/L=`, `/R=` — no character-set identifiers, no icon
    ///   table, no background themes, no `KEYBOARD.CNF`, no tab bar, no startup
    ///   macro path, no DDE, and no `ts.LogFN` (which upstream has no key for
    ///   either).
    /// - `/4`, `/6`, `/I`, `/V`, `/DUPLICATE`, `/E` — upstream has no key for
    ///   any of these. `ProtocolFamily` is command-line-only, and `Minimize`
    ///   and `HideWindow` are zeroed at `ttset.c:554` and never read.
    /// - `/X=` and `/Y=`, because their companion rule tests `ts.VTPos` against
    ///   `CW_USEDEFAULT` and this schema has no window position yet.
    /// - `/FD=` is applied **only if the directory exists**, which is upstream
    ///   (`DoesFolderExistW`) and the one thing here that needs a filesystem.
    pub fn apply(&self, settings: &mut Settings) {
        if let Some(p) = self.port_type {
            // The file holds two of the four, and a replay-file or named-pipe
            // session is written down as `tcpip` — which is upstream's own
            // writer, `(PortType==IdSerial)?"serial":"tcpip"`.
            settings.connection_port_type = match p {
                PortType::Serial => ConnectionPortType::Serial,
                _ => ConnectionPortType::TcpIp,
            };
        }
        if let Some(p) = self.tcp_port {
            settings.connection_tcp_port = i32::from(p);
        }
        if let Some(t) = self.telnet {
            settings.connection_telnet = t;
        }
        if let Some(b) = self.telnet_binary {
            settings.connection_telnet_binary = b;
        }
        if let Some(c) = self.auto_win_close {
            settings.connection_auto_win_close = c;
        }
        if let Some(t) = self.connecting_timeout {
            settings.connection_timeout = t;
        }
        if let Some(d) = self.host_dialog_on_startup {
            settings.connection_host_dialog_on_startup = d;
        }

        if let Some(c) = self.com_port {
            settings.serial_com_port = i32::from(c);
        }
        if let Some(b) = self.baud {
            settings.serial_baud = b as i32;
        }
        if let Some(d) = self.data_bits {
            settings.serial_data_bits = match d {
                DataBits::Seven => SerialDataBits::Seven,
                DataBits::Eight => SerialDataBits::Eight,
            };
        }
        if let Some(p) = self.parity {
            settings.serial_parity = match p {
                Parity::None => SerialParity::None,
                Parity::Odd => SerialParity::Odd,
                Parity::Even => SerialParity::Even,
                Parity::Mark => SerialParity::Mark,
                Parity::Space => SerialParity::Space,
            };
        }
        if let Some(s) = self.stop_bits {
            settings.serial_stop_bits = match s {
                StopBits::One => SerialStopBits::One,
                StopBits::Two => SerialStopBits::Two,
            };
        }
        if let Some(f) = self.flow {
            settings.serial_flow = match f {
                FlowControl::None => SerialFlow::None,
                FlowControl::XonXoff => SerialFlow::XonXoff,
                FlowControl::Hardware => SerialFlow::Hardware,
                FlowControl::DsrDtr => SerialFlow::DsrDtr,
            };
        }
        if let Some(d) = self.delay_per_char {
            settings.serial_delay_per_char = i32::from(d);
        }
        if let Some(d) = self.delay_per_line {
            settings.serial_delay_per_line = i32::from(d);
        }
        if self.wait_com {
            settings.serial_wait_com = true;
        }

        // `/NOLOG` clears `ts.LogFN` as well, which is not a setting — the
        // caller has to forget the name it was given separately.
        if self.no_log {
            settings.log_auto_start = false;
        }
        if let Some(dir) = &self.file_dir {
            let dir = String::from_utf8_lossy(dir).into_owned();
            if std::path::Path::new(&dir).is_dir() {
                settings.transfer_dir = dir;
            }
        }
        if self.hide_title {
            settings.window_hide_title = true;
        }
        if let Some(t) = &self.title {
            settings.terminal_title = String::from_utf8_lossy(t).into_owned();
        }
    }

    /// `_ParseParam(Param, ts, DDETopic)`, where `topic` is whether that third
    /// argument was a buffer rather than NULL. Two arms test it.
    fn parse_inner(line: &[u8], max_com_port: u16, topic: bool) -> CommandLine {
        // "Set AutoConnect true as default (2008.2.16 by steven)" is in
        // `Default`, with the rest of what an empty line means.
        let mut cmd = CommandLine {
            raw: line.to_vec(),
            ..Default::default()
        };
        let toks = tokens(line, MAX_STR_LEN);

        // First pass: the settings file, and stop at the first one. Upstream
        // reads it here, so everything the second pass does lands on top of
        // the new contents.
        for t in &toks {
            if let Some(v) = after_ci(t, b"/F=") {
                cmd.setup_file = Some(v.to_vec());
                break;
            }
        }

        // `ParamPort` and friends: locals upstream, because the port type has
        // to be decided by all the options together before any of it is
        // applied.
        let mut param_tel: Option<bool> = None;
        let mut param_bin: Option<bool> = None;
        let mut param_tcp: u16 = 0;
        let mut param_com: u16 = 0;
        let mut host_name_flag = false;
        let mut just_after_host = false;

        for t in &toks {
            if host_name_flag {
                just_after_host = true;
                host_name_flag = false;
            }
            cmd.arm(
                t,
                &mut param_tel,
                &mut param_bin,
                &mut param_tcp,
                &mut param_com,
                &mut host_name_flag,
                just_after_host,
                max_com_port,
                topic,
            );
            just_after_host = false;
        }

        if cmd.dde_topic.as_ref().is_some_and(|t| !t.is_empty()) {
            cmd.macro_file = MacroArg::Cleared;
        }

        // A host name only has its `telnet://` and `:port` taken apart once
        // everything else has decided this is a TCP/IP session — so
        // `/C=1 myhost:23` leaves the colon in the host name, which is a
        // serial session with a curious host name and not an error.
        if !cmd.host_name.is_empty() && cmd.port_type == Some(PortType::TcpIp) {
            let (host, port) = parse_host_name(&cmd.host_name);
            cmd.host_name = host;
            if let Some(p) = port {
                param_tcp = p;
            }
        }

        match cmd.port_type {
            Some(PortType::TcpIp) => {
                cmd.tcp_port = (param_tcp != 0).then_some(param_tcp);
                cmd.telnet = param_tel;
                cmd.telnet_binary = param_bin;
            }
            Some(PortType::Serial) => {
                if param_com > 0 {
                    cmd.com_port = Some(param_com);
                    // "Don't display new connection dialog if COM port is
                    // specified explicitly (2006.9.15 maya)" — and this runs
                    // after the loop, so `/M=x /C=1` connects anyway.
                    cmd.com_auto_connect = true;
                }
            }
            Some(PortType::NamedPipe) => {
                cmd.host_name = pipe_name(&cmd.host_name);
                cmd.com_port = None;
            }
            Some(PortType::File) | None => {}
        }
        cmd
    }

    /// One token, in upstream's order — an `if`/`else if` chain, so the first
    /// arm that matches wins and a token that matches nothing is silently
    /// ignored. There is no diagnostic for a misspelt option.
    #[allow(clippy::too_many_arguments)]
    fn arm(
        &mut self,
        t: &[u8],
        param_tel: &mut Option<bool>,
        param_bin: &mut Option<bool>,
        param_tcp: &mut u16,
        param_com: &mut u16,
        host_name_flag: &mut bool,
        just_after_host: bool,
        max_com_port: u16,
        topic: bool,
    ) {
        if let Some(v) = after_ci(t, b"/AUTOWINCLOSE=") {
            self.auto_win_close = Some(eq_ci(v, b"on"));
        } else if let Some(v) = after_ci(t, b"/SPEED=") {
            self.port_type = Some(PortType::Serial);
            self.baud = Some(wtoi(v) as u32);
        } else if let Some(v) = after_ci(t, b"/BAUD=") {
            self.port_type = Some(PortType::Serial);
            self.baud = Some(wtoi(v) as u32);
        } else if eq_ci(t, b"/B") {
            self.port_type = Some(PortType::TcpIp);
            *param_bin = Some(true);
        } else if let Some(v) = after_ci(t, b"/C=") {
            self.port_type = Some(PortType::Serial);
            let n = wtoi(v);
            // Out of range is *dropped*, not clamped, and the serial port type
            // stays selected — so `/C=999` on a stock setup opens the New
            // Connection dialog on serial rather than failing.
            *param_com = match (1..=i32::from(max_com_port)).contains(&n) {
                true => n as u16,
                false => 0,
            };
        } else if let Some(v) = after_ci(t, b"/CDATABIT=") {
            self.port_type = Some(PortType::Serial);
            self.data_bits = match v {
                b"7" => Some(DataBits::Seven),
                b"8" => Some(DataBits::Eight),
                _ => self.data_bits,
            };
        } else if let Some(v) = after_ci(t, b"/CPARITY=") {
            self.port_type = Some(PortType::Serial);
            self.parity = match lower(v).as_slice() {
                b"none" => Some(Parity::None),
                b"odd" => Some(Parity::Odd),
                b"even" => Some(Parity::Even),
                b"mark" => Some(Parity::Mark),
                b"space" => Some(Parity::Space),
                _ => self.parity,
            };
        } else if let Some(v) = after_ci(t, b"/CSTOPBIT=") {
            self.port_type = Some(PortType::Serial);
            self.stop_bits = match v {
                b"1" => Some(StopBits::One),
                b"2" => Some(StopBits::Two),
                _ => self.stop_bits,
            };
        } else if let Some(v) = after_ci(t, b"/CFLOWCTRL=") {
            self.port_type = Some(PortType::Serial);
            self.flow = match lower(v).as_slice() {
                b"x" => Some(FlowControl::XonXoff),
                // `rtscts` is a second spelling of the same value, and the only
                // alias in any of the four tables.
                b"hard" | b"rtscts" => Some(FlowControl::Hardware),
                b"none" => Some(FlowControl::None),
                b"dsrdtr" => Some(FlowControl::DsrDtr),
                _ => self.flow,
            };
        } else if let Some(v) = after_ci(t, b"/CDELAYPERCHAR=") {
            self.port_type = Some(PortType::Serial);
            self.delay_per_char = Some(wtoi(v) as u16);
        } else if let Some(v) = after_ci(t, b"/CDELAYPERLINE=") {
            self.port_type = Some(PortType::Serial);
            self.delay_per_line = Some(wtoi(v) as u16);
        } else if eq_ci(t, b"/WAITCOM") {
            // Note the absent `ParamPort = IdSerial`: `/WAITCOM` on its own
            // waits for a port it has not selected.
            self.wait_com = true;
        } else if let Some(v) = after_ci(t, b"/D=") {
            if topic {
                self.dde_topic = Some(truncate(v, MAX_TOPIC_LEN));
            }
        } else if eq_ci(t, b"/DS") {
            self.host_dialog_on_startup = Some(false);
        } else if eq_ci(t, b"/E") {
            self.disable_tcp_echo_cr = true;
        } else if eq_ci(t, b"/ES") {
            self.host_dialog_on_startup = Some(true);
        } else if let Some(v) = after_ci(t, b"/FD=") {
            self.file_dir = Some(v.to_vec());
        } else if eq_ci(t, b"/H") {
            self.hide_title = true;
        } else if eq_ci(t, b"/I") {
            self.minimize = true;
        } else if let Some(v) = after_ci(t, b"/K=") {
            self.key_cnf_file = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/KR=") {
            self.kanji_recv = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/KT=") {
            self.kanji_send = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/L=") {
            self.log_file = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/MN=") {
            self.multicast_name = Some(truncate(v, MULTICAST_NAME_LEN));
        } else if let Some(v) = after_ci(t, b"/M=") {
            self.macro_file = match v {
                b"" | b"*" => MacroArg::Prompt,
                _ => MacroArg::File(v.to_vec()),
            };
            // "Disable auto connect to serial when macro mode (2006.9.15 maya)"
            self.com_auto_connect = false;
        } else if eq_ci(t, b"/M") {
            self.macro_file = MacroArg::Prompt;
            self.com_auto_connect = false;
        } else if eq_ci(t, b"/NOLOG") {
            self.no_log = true;
        } else if let Some(v) = after_ci(t, b"/OSC52=") {
            self.clipboard = Some(match lower(v).as_slice() {
                b"on" | b"readwrite" => ClipboardAccess::ReadWrite,
                b"read" => ClipboardAccess::Read,
                b"write" => ClipboardAccess::Write,
                // `off`, and anything else, because the mask is cleared first.
                _ => ClipboardAccess::None,
            });
        } else if let Some(v) = after_ci(t, b"/P=") {
            self.port_type = Some(PortType::TcpIp);
            *param_tcp = crate::services::parse_port_name(v) as u16;
        } else if eq_ci(t, b"/PIPE") || eq_ci(t, b"/NAMEDPIPE") {
            self.port_type = Some(PortType::NamedPipe);
        } else if let Some(v) = after_ci(t, b"/R=") {
            // A replay file goes in the *host name*, which is how `IdFile`
            // says where to read from. An empty one is not a session.
            if !v.is_empty() {
                self.host_name = truncate(v, HOST_NAME_LEN);
                self.port_type = Some(PortType::File);
            }
        } else if eq_ci(t, b"/T=0") {
            self.port_type = Some(PortType::TcpIp);
            *param_tel = Some(false);
        } else if eq_ci(t, b"/T=1") {
            self.port_type = Some(PortType::TcpIp);
            *param_tel = Some(true);
        } else if let Some(v) = after_ci(t, b"/TEKICON=") {
            self.tek_icon = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/THEME=") {
            self.theme_file = Some(v.to_vec());
        } else if let Some(v) = after_ci(t, b"/VTICON=") {
            self.vt_icon = Some(v.to_vec());
        } else if eq_ci(t, b"/V") {
            self.hide_window = true;
        } else if let Some(v) = after_ci(t, b"/W=") {
            self.title = Some(truncate(v, TITLE_LEN));
        } else if let Some(v) = after_ci(t, b"/X=") {
            self.window_x = crate::services::scanf_int(v).or(self.window_x);
        } else if let Some(v) = after_ci(t, b"/Y=") {
            self.window_y = crate::services::scanf_int(v).or(self.window_y);
        } else if eq_ci(t, b"/4") {
            self.protocol_family = Some(ProtocolFamily::V4);
        } else if eq_ci(t, b"/6") {
            self.protocol_family = Some(ProtocolFamily::V6);
        } else if eq_ci(t, b"/DUPLICATE") {
            self.duplicate_session = true;
        } else if let Some(v) = after_ci(t, b"/TIMEOUT=") {
            if let Some(n) = crate::services::scanf_int(v) {
                if n >= 0 {
                    self.connecting_timeout = Some(n);
                }
            }
        } else if t.first() != Some(&b'/') && !t.is_empty() {
            let port = crate::services::parse_port_name(t);
            if just_after_host && port > 0 {
                *param_tcp = port as u16;
            } else {
                self.host_name = truncate(t, HOST_NAME_LEN);
                if self.port_type != Some(PortType::NamedPipe) {
                    self.port_type = Some(PortType::TcpIp);
                    *host_name_flag = true;
                }
            }
        }
    }
}

/// `ParseHostName` (`ttset.c:3473`) — a host name with a scheme, brackets or a
/// port on it, taken apart.
///
/// Returns the host and the port, where `None` means upstream would not have
/// written one: it assigns `*port` only when it found a `:`, or when the name
/// was a `telnet://` URL with no port and so gets 23.
///
/// The five forms this has to handle are in a comment above the function, and
/// `tn3270://` is there because Windows registers Tera Term for that scheme
/// too. Only the *scheme* is checked case-insensitively; the trailing `/` is
/// dropped whether or not there is a path after it, which there must not be.
pub fn parse_host_name(host: &[u8]) -> (Vec<u8>, Option<u16>) {
    let mut s = host.to_vec();
    let mut is_handler = false;
    if starts_with_ci(&s, b"telnet://") || starts_with_ci(&s, b"tn3270://") {
        s.drain(..9);
        if s.last() == Some(&b'/') {
            s.pop();
        }
        is_handler = true;
    }

    // A bracketed IPv6 literal: both brackets come out and the search for a
    // port starts *after* the address, which is the whole point of them.
    let mut from = 0;
    if s.first() == Some(&b'[') {
        if let Some(close) = s.iter().position(|&b| b == b']') {
            s.remove(close);
            s.remove(0);
            from = close.saturating_sub(1);
        }
    }

    match s[from..].iter().position(|&b| b == b':') {
        Some(colon) => {
            let at = from + colon;
            let port = crate::services::parse_port_name(&s[at + 1..]) as u16;
            s.truncate(at);
            (s, Some(port))
        }
        // "telnet://host" with no port is 23 — a URL Windows handed over knows
        // which service it meant.
        None if is_handler => (s, Some(23)),
        None => (s, None),
    }
}

/// The `IdNamedPipe` arm of `_ParseParam`'s closing switch (`ttset.c:3996`) —
/// `host\pipename` written out as a UNC path.
///
/// A name that already starts with a backslash is taken as complete. Anything
/// else is split at its first backslash into a machine and a pipe name, and a
/// name with no backslash at all is a pipe on this machine.
fn pipe_name(host: &[u8]) -> Vec<u8> {
    if host.is_empty() || host.first() == Some(&b'\\') {
        return host.to_vec();
    }
    let mut out = br"\\".to_vec();
    match host.iter().position(|&b| b == b'\\') {
        Some(i) => {
            out.extend_from_slice(&host[..i]);
            out.extend_from_slice(br"\pipe\");
            out.extend_from_slice(&host[i + 1..]);
        }
        None => {
            out.extend_from_slice(br".\pipe\");
            out.extend_from_slice(host);
        }
    }
    out
}

/// `GetFilePath` (`ttset.c:3573`) — the full path a command line's file name
/// means.
///
/// `default_dir` is prepended to a relative name — upstream's `IsRelativePathW`
/// then `awcscats(&full_path, default_path, L"\\", command_line, NULL)` — and
/// `default_ext` is appended when the *file part* has no dot in it, which is
/// not the same as having no extension. The directories differ per option:
/// `/F=`, `/K=`, `/M=`, `/R=` and `/THEME=` resolve against `ts.HomeDirW` and
/// `/L=` against `GetTermLogDir`, so this takes them rather than knowing them.
///
/// The normalisation upstream does with `hGetFullPathNameW` is left to the
/// caller: it resolves against the process's current directory, which is a
/// thing a parser should not read.
pub fn file_path(given: &[u8], default_dir: Option<&[u8]>, default_ext: Option<&[u8]>) -> Vec<u8> {
    if given.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(dir) = default_dir.filter(|_| is_relative(given)) {
        out.extend_from_slice(dir);
        out.push(std::path::MAIN_SEPARATOR as u8);
    }
    out.extend_from_slice(given);
    if let Some(ext) = default_ext {
        let file = match out.iter().rposition(|&b| b == b'/' || b == b'\\') {
            Some(i) => &out[i + 1..],
            None => &out[..],
        };
        if !file.contains(&b'.') {
            out.extend_from_slice(ext);
        }
    }
    out
}

/// `IsRelativePathW`, for the two path shapes this has to tell apart.
fn is_relative(p: &[u8]) -> bool {
    !(p.first() == Some(&b'/')
        || p.first() == Some(&b'\\')
        || (p.len() >= 2 && p[1] == b':')
        || p.starts_with(b"~"))
}

/// `_wcsnicmp(t, prefix, n) == 0` — and the rest of the token, which is what
/// every `&Temp[n]` in `_ParseParam` is.
fn after_ci<'a>(t: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    match t.len() >= prefix.len() && t[..prefix.len()].eq_ignore_ascii_case(prefix) {
        true => Some(&t[prefix.len()..]),
        false => None,
    }
}

fn starts_with_ci(t: &[u8], prefix: &[u8]) -> bool {
    after_ci(t, prefix).is_some()
}

/// `_wcsicmp(t, s) == 0`.
fn eq_ci(t: &[u8], s: &[u8]) -> bool {
    t.eq_ignore_ascii_case(s)
}

fn lower(s: &[u8]) -> Vec<u8> {
    s.to_ascii_lowercase()
}

/// `_wtoi` — like [`crate::services::scanf_int`] but with nothing to say about
/// a string that held no number, which is how every arm that uses it treats
/// one: `/SPEED=fast` is baud zero.
fn wtoi(s: &[u8]) -> i32 {
    crate::services::scanf_int(s).unwrap_or(0)
}

/// Upstream counts `wchar_t` or bytes of ACP depending on the field; this
/// counts bytes and backs off a UTF-8 continuation, so a cut value is still
/// text. Half a character is a thing no truncation should produce.
fn truncate(s: &[u8], n: usize) -> Vec<u8> {
    if s.len() <= n {
        return s.to_vec();
    }
    let mut end = n;
    while end > 0 && s[end] & 0xC0 == 0x80 {
        end -= 1;
    }
    s[..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 512 is `MaxStrLen`, which both `ttset.c:75` and `ttmdef.h:34` define for
    /// themselves at the same value.
    const MAX: usize = 512;

    #[test]
    fn a_doubled_quote_is_one_literal_quote() {
        assert_eq!(dequote_param(br#""a b""#), b"a b");
        assert_eq!(dequote_param(br#""""#), b"");
        assert_eq!(dequote_param(br#""""""#), br#"""#);
        // Outside a quoted run, `""` is still just the toggle twice.
        assert_eq!(dequote_param(br#"a""b"#), b"ab");
        assert_eq!(dequote_param(br#""a""b""#), br#"a"b"#);
        // A quote in the middle of a word opens a run like any other.
        assert_eq!(dequote_param(br#"a"b c"d"#), b"ab cd");
    }

    /// `GetParam` hands the quotes on rather than removing them, which is what
    /// makes the doubled-quote rule work at all.
    #[test]
    fn get_param_keeps_the_quotes_for_dequote_param() {
        let (tok, rest) = get_param(br#""a b" c"#, MAX).unwrap();
        assert_eq!(tok, br#""a b""#);
        assert_eq!(rest, b" c");
        assert_eq!(dequote_param(&tok), b"a b");
        // An unterminated quote runs to the end of the line.
        let (tok, rest) = get_param(br#""a b c"#, MAX).unwrap();
        assert_eq!(tok, br#""a b c"#);
        assert_eq!(rest, b"");
    }

    #[test]
    fn the_tokeniser_is_not_the_c_runtimes() {
        // A backslash is an ordinary character, which it has to be.
        assert_eq!(tokens(br"tt c:\dir\m.ttl", MAX), [br"c:\dir\m.ttl"]);
        // An unquoted `;` ends the line, and everything after it is lost.
        assert_eq!(tokens(b"tt a ; b c", MAX), [b"a"]);
        // A quoted one is just a character.
        assert_eq!(
            tokens(br#"tt "a;b" c"#, MAX),
            [b"a;b".to_vec(), b"c".to_vec()]
        );
        // Tabs separate, and a run of separators is one.
        assert_eq!(tokens(b"tt \t a  b", MAX), [b"a", b"b"]);
        // The first term is the executable and is discarded.
        assert!(tokens(b"ttermpro.exe", MAX).is_empty());
        assert!(tokens(b"", MAX).is_empty());
    }

    #[test]
    fn a_token_stops_one_short_of_the_buffer() {
        let long = vec![b'x'; 600];
        let line = [b"tt ".as_slice(), &long].concat();
        assert_eq!(tokens(&line, MAX)[0].len(), MAX - 1);
        // The size is the caller's, not a constant of the tokeniser.
        assert_eq!(tokens(&line, 4)[0], b"xxx");
    }

    /// `_ParseParam` with `ts.MaxComPort` at its own default.
    fn parse(line: &str) -> CommandLine {
        CommandLine::parse(line.as_bytes(), DEFAULT_MAX_COM_PORT)
    }

    fn host(cmd: &CommandLine) -> String {
        String::from_utf8_lossy(&cmd.host_name).into_owned()
    }

    fn text(v: &Option<Vec<u8>>) -> String {
        String::from_utf8_lossy(v.as_deref().unwrap_or_default()).into_owned()
    }

    /// The shape of the whole thing: a host name selects TCP/IP, a `:port`
    /// comes off it, and nothing else was asked for.
    #[test]
    fn a_bare_host_name_is_a_tcp_session() {
        let cmd = parse("ttermpro.exe myhost");
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));
        assert_eq!(host(&cmd), "myhost");
        assert_eq!(cmd.tcp_port, None);
        assert_eq!(
            cmd,
            CommandLine {
                raw: cmd.raw.clone(),
                port_type: Some(PortType::TcpIp),
                host_name: b"myhost".to_vec(),
                ..Default::default()
            }
        );

        let cmd = parse("ttermpro.exe myhost:2222");
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost".into(), Some(2222)));
        // The port may be a service name, out of upstream's own table.
        let cmd = parse("ttermpro.exe myhost:ssh");
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost".into(), Some(22)));
        // ...or a separate token, but only the very next one.
        let cmd = parse("ttermpro.exe myhost 2222");
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost".into(), Some(2222)));
    }

    /// `JustAfterHost` is one token wide and is cleared by anything that is not
    /// a port, so a second bare word is a **second host name** rather than an
    /// error — the last one wins.
    #[test]
    fn only_the_token_straight_after_the_host_can_be_its_port() {
        let cmd = parse("ttermpro.exe myhost /4 2222");
        assert_eq!(host(&cmd), "2222");
        assert_eq!(cmd.tcp_port, None);
        assert_eq!(cmd.protocol_family, Some(ProtocolFamily::V4));

        let cmd = parse("ttermpro.exe first second");
        assert_eq!(host(&cmd), "second");
        // A port token is consumed, so the host after it starts the dance over.
        let cmd = parse("ttermpro.exe first 23 second 22");
        assert_eq!((host(&cmd), cmd.tcp_port), ("second".into(), Some(22)));
    }

    /// The four transports, and the guards each has on it.
    #[test]
    fn the_port_type_is_decided_by_all_the_options_together() {
        let cmd = parse("tt /C=3 /SPEED=115200");
        assert_eq!(cmd.port_type, Some(PortType::Serial));
        assert_eq!((cmd.com_port, cmd.baud), (Some(3), Some(115200)));
        assert!(cmd.com_auto_connect);

        // `/BAUD=` is the same option under its pre-4.60 name.
        assert_eq!(parse("tt /BAUD=9600").baud, Some(9600));

        // Out of range is dropped, and serial stays selected.
        let cmd = parse("tt /C=9999");
        assert_eq!(cmd.port_type, Some(PortType::Serial));
        assert_eq!(cmd.com_port, None);
        assert_eq!(parse("tt /C=0").com_port, None);
        // The bound is a *setting*, so the same line differs per machine.
        assert_eq!(CommandLine::parse(b"tt /C=300", 256).com_port, None);
        assert_eq!(CommandLine::parse(b"tt /C=300", 1024).com_port, Some(300));

        let cmd = parse("tt /R=session.log");
        assert_eq!(cmd.port_type, Some(PortType::File));
        assert_eq!(host(&cmd), "session.log");
        // An empty replay name is not a session at all.
        assert_eq!(parse("tt /R=").port_type, None);

        // **A bare host name cancels the serial selection**, because its arm
        // assigns `ParamPort = IdTCPIP` outright. So `/C=1 myhost` is a TCP
        // session with no COM port at all, and the order of the two is what
        // decides — which is worth knowing before writing a launcher script.
        let cmd = parse("tt /C=1 myhost:23");
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost".into(), Some(23)));
        assert_eq!(cmd.com_port, None);

        // The other way round the serial option wins, and then the host name is
        // never taken apart, so the colon stays in it.
        let cmd = parse("tt myhost:23 /C=1");
        assert_eq!(cmd.port_type, Some(PortType::Serial));
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost:23".into(), None));
        assert_eq!(cmd.com_port, Some(1));
    }

    #[test]
    fn a_named_pipe_is_written_out_as_a_unc_path() {
        let cmd = parse(r"tt /PIPE mypipe");
        assert_eq!(cmd.port_type, Some(PortType::NamedPipe));
        assert_eq!(host(&cmd), r"\\.\pipe\mypipe");
        // A machine name before the first backslash.
        assert_eq!(host(&parse(r"tt /PIPE box\mypipe")), r"\\box\pipe\mypipe");
        // Already a UNC path: left alone.
        assert_eq!(host(&parse(r"tt /PIPE \\box\pipe\x")), r"\\box\pipe\x");
        // `/NAMEDPIPE` is kept for compatibility and means the same.
        assert_eq!(parse("tt /NAMEDPIPE").port_type, Some(PortType::NamedPipe));
    }

    /// Telnet and binary mode are three-state upstream — given on, given off,
    /// not given — and only the first two touch the settings file's value.
    #[test]
    fn telnet_is_three_state_and_t_equals_2_is_not_tera_terms() {
        assert_eq!(parse("tt /T=0 h").telnet, Some(false));
        assert_eq!(parse("tt /T=1 h").telnet, Some(true));
        assert_eq!(parse("tt h").telnet, None);
        // `/T=2` is TTSSH's spelling for "SSH", and Tera Term ignores it —
        // silently, like every other option it does not recognise.
        let cmd = parse("tt /T=2 h");
        assert_eq!(cmd.telnet, None);
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));
        assert_eq!(parse("tt /B h").telnet_binary, Some(true));
        assert_eq!(parse("tt h").telnet_binary, None);
    }

    /// The other half: what `apply` puts into the settings, and what it
    /// deliberately leaves for the caller.
    #[test]
    fn apply_writes_the_settings_the_file_can_hold() {
        let cmd = parse(
            "tt /C=3 /SPEED=115200 /CDATABIT=7 /CPARITY=odd /CSTOPBIT=2 \
             /CFLOWCTRL=hard /CDELAYPERCHAR=5 /CDELAYPERLINE=50 /WAITCOM",
        );
        let mut s = Settings::default();
        cmd.apply(&mut s);
        assert_eq!(s.connection_port_type, ConnectionPortType::Serial);
        assert_eq!((s.serial_com_port, s.serial_baud), (3, 115_200));
        assert_eq!(s.serial_data_bits, SerialDataBits::Seven);
        assert_eq!(s.serial_parity, SerialParity::Odd);
        assert_eq!(s.serial_stop_bits, SerialStopBits::Two);
        assert_eq!(s.serial_flow, SerialFlow::Hardware);
        assert_eq!((s.serial_delay_per_char, s.serial_delay_per_line), (5, 50));
        assert!(s.serial_wait_com);

        let cmd = parse("tt /T=1 /B /AUTOWINCLOSE=off /TIMEOUT=30 /DS /H /W=Title myhost:2222");
        let mut s = Settings::default();
        cmd.apply(&mut s);
        assert_eq!(s.connection_port_type, ConnectionPortType::TcpIp);
        assert_eq!(s.connection_tcp_port, 2222);
        assert!(s.connection_telnet && s.connection_telnet_binary);
        assert!(!s.connection_auto_win_close);
        assert_eq!(s.connection_timeout, 30);
        assert!(!s.connection_host_dialog_on_startup);
        assert!(s.window_hide_title);
        assert_eq!(s.terminal_title, "Title");
        // The host name is not a setting and never was: `ts.HostName` has no
        // key, so where to connect stays on the command line.
        assert_eq!(cmd.host_name, b"myhost");
    }

    /// An option that was not given must not overwrite the file's value, which
    /// is the whole reason every field is an `Option`.
    #[test]
    fn apply_leaves_alone_what_the_line_did_not_mention() {
        let mut s = Settings {
            serial_baud: 4800,
            connection_tcp_port: 2222,
            serial_flow: SerialFlow::XonXoff,
            ..Default::default()
        };
        let before = s.clone();
        parse("tt /H").apply(&mut s);
        assert_eq!(s.serial_baud, before.serial_baud);
        assert_eq!(s.connection_tcp_port, before.connection_tcp_port);
        assert_eq!(s.serial_flow, before.serial_flow);
        // ...and a value the serial tables do not have is not a value.
        parse("tt /CFLOWCTRL=maybe").apply(&mut s);
        assert_eq!(s.serial_flow, SerialFlow::XonXoff);
    }

    /// The two transports the file cannot name are written down as `tcpip`,
    /// which is upstream's own writer rather than a shortcut here.
    #[test]
    fn a_replay_or_pipe_session_is_recorded_as_tcpip() {
        for line in ["tt /R=session.log", "tt /PIPE mypipe"] {
            let cmd = parse(line);
            let mut s = Settings::default();
            cmd.apply(&mut s);
            assert_eq!(s.connection_port_type, ConnectionPortType::TcpIp, "{line}");
            // ...while the command line keeps the whole answer.
            assert_ne!(cmd.port_type, Some(PortType::TcpIp), "{line}");
        }
    }

    /// `/NOLOG` and `/FD=`, the two that are not a plain assignment.
    #[test]
    fn nolog_and_a_directory_that_has_to_exist() {
        let mut s = Settings {
            log_auto_start: true,
            ..Default::default()
        };
        parse("tt /NOLOG").apply(&mut s);
        assert!(!s.log_auto_start);

        // **`/L=` with `/NOLOG` is where this port follows the manual instead.**
        // Both are visible here, so a consumer can see the conflict; upstream
        // resolves it the wrong way — `/NOLOG` clears the ANSI copy of the name
        // and `vtwin.cpp:3631` tests the wide one, so it logs anyway.
        let cmd = parse("tt /L=out.log /NOLOG");
        assert!(cmd.no_log);
        assert_eq!(text(&cmd.log_file), "out.log");
        let mut s = Settings {
            log_auto_start: true,
            ..Default::default()
        };
        cmd.apply(&mut s);
        assert!(!s.log_auto_start, "the manual: `without logging`");

        // `/FD=` is applied only if the folder is there, which is upstream's
        // `DoesFolderExistW` and is why this one arm reads the filesystem.
        let mut s = Settings::default();
        parse("tt /FD=/nonexistent-directory-for-a-test").apply(&mut s);
        assert_eq!(s.transfer_dir, "");
        let tmp = std::env::temp_dir();
        parse(&format!("tt /FD={}", tmp.display())).apply(&mut s);
        assert_eq!(s.transfer_dir, tmp.display().to_string());
    }

    /// A `/D=` topic frees the startup macro, so a terminal opened by a macro
    /// does not open another one.
    #[test]
    fn a_dde_topic_cancels_the_startup_macro() {
        assert_eq!(
            parse("tt /M=login.ttl").macro_file,
            MacroArg::File(b"login.ttl".to_vec())
        );
        assert_eq!(parse("tt /M").macro_file, MacroArg::Prompt);
        assert_eq!(parse("tt /M=").macro_file, MacroArg::Prompt);
        assert_eq!(parse("tt /M=*").macro_file, MacroArg::Prompt);
        assert!(!parse("tt /M=login.ttl").com_auto_connect);
        // ...unless a COM port was named as well, because that runs afterwards.
        assert!(parse("tt /M=login.ttl /C=1").com_auto_connect);

        let cmd = parse("tt /D=TERATERM01 /M=login.ttl");
        assert_eq!(text(&cmd.dde_topic), "TERATERM01");
        assert_eq!(cmd.macro_file, MacroArg::Cleared);
        // An empty topic is not a topic.
        assert_eq!(
            parse("tt /D= /M=x").macro_file,
            MacroArg::File(b"x".to_vec())
        );
        // Twenty characters, where `ttpmacro`'s own `/D=` keeps ten.
        assert_eq!(
            text(&parse("tt /D=012345678901234567890123").dde_topic),
            "01234567890123456789"
        );
    }

    /// `connect`'s argument is a command line with no program in it, which is
    /// why `ttdde.c` puts a dummy one there before parsing.
    #[test]
    fn a_connect_argument_keeps_its_first_word() {
        let cmd = CommandLine::parse_argument(b"myhost:22 /nossh", DEFAULT_MAX_COM_PORT);
        assert_eq!((host(&cmd), cmd.tcp_port), ("myhost".into(), Some(22)));
        // Parsed as a whole line instead, the host would be eaten as the exe.
        assert_eq!(host(&parse("myhost:22")), "");
        // `connect` passes NULL for the topic, so a `/D=` in it does nothing —
        // including not cancelling the macro.
        let cmd = CommandLine::parse_argument(b"/D=TOPIC /M=x", DEFAULT_MAX_COM_PORT);
        assert_eq!(cmd.dde_topic, None);
        assert_eq!(cmd.macro_file, MacroArg::File(b"x".to_vec()));
    }

    /// The serial options, whose spellings are the settings file's own.
    #[test]
    fn the_serial_options_read_through_upstreams_tables() {
        let cmd = parse("tt /CDATABIT=7 /CPARITY=EVEN /CSTOPBIT=2 /CFLOWCTRL=rtscts");
        assert_eq!(cmd.data_bits, Some(DataBits::Seven));
        assert_eq!(cmd.parity, Some(Parity::Even));
        assert_eq!(cmd.stop_bits, Some(StopBits::Two));
        assert_eq!(cmd.flow, Some(FlowControl::Hardware));
        assert_eq!(cmd.port_type, Some(PortType::Serial));
        // `hard` and `rtscts` are one value under two names.
        assert_eq!(
            parse("tt /CFLOWCTRL=hard").flow,
            Some(FlowControl::Hardware)
        );
        assert_eq!(
            parse("tt /CFLOWCTRL=dsrdtr").flow,
            Some(FlowControl::DsrDtr)
        );

        // A value the table does not have leaves the setting alone — and still
        // selects the serial transport.
        let cmd = parse("tt /CDATABIT=9 /CPARITY=maybe");
        assert_eq!((cmd.data_bits, cmd.parity), (None, None));
        assert_eq!(cmd.port_type, Some(PortType::Serial));

        let cmd = parse("tt /CDELAYPERCHAR=5 /CDELAYPERLINE=100 /WAITCOM");
        assert_eq!(
            (cmd.delay_per_char, cmd.delay_per_line),
            (Some(5), Some(100))
        );
        assert!(cmd.wait_com);
        // `/WAITCOM` alone does not select serial, which is upstream's.
        assert_eq!(parse("tt /WAITCOM").port_type, None);
    }

    /// `/AUTOWINCLOSE=` is not `GetOnOff`: it tests for `on` and everything
    /// else is off, so the value that means *on* for a settings key means
    /// *off* here.
    #[test]
    fn auto_win_close_is_on_or_anything_else() {
        assert_eq!(parse("tt /AUTOWINCLOSE=on").auto_win_close, Some(true));
        assert_eq!(parse("tt /AUTOWINCLOSE=ON").auto_win_close, Some(true));
        assert_eq!(parse("tt /AUTOWINCLOSE=off").auto_win_close, Some(false));
        assert_eq!(parse("tt /AUTOWINCLOSE=1").auto_win_close, Some(false));
        assert_eq!(parse("tt").auto_win_close, None);
    }

    /// `off` and a value nobody recognises are the same thing, because the mask
    /// is cleared before the match and `CSF_CBNONE` is zero.
    #[test]
    fn osc52_has_four_states_and_not_five() {
        assert_eq!(
            parse("tt /OSC52=on").clipboard,
            Some(ClipboardAccess::ReadWrite)
        );
        assert_eq!(
            parse("tt /OSC52=readwrite").clipboard,
            Some(ClipboardAccess::ReadWrite)
        );
        assert_eq!(
            parse("tt /OSC52=Read").clipboard,
            Some(ClipboardAccess::Read)
        );
        assert_eq!(
            parse("tt /OSC52=WRITE").clipboard,
            Some(ClipboardAccess::Write)
        );
        assert_eq!(
            parse("tt /OSC52=off").clipboard,
            Some(ClipboardAccess::None)
        );
        assert_eq!(
            parse("tt /OSC52=nonsense").clipboard,
            Some(ClipboardAccess::None)
        );
        assert_eq!(parse("tt").clipboard, None);
    }

    /// The flags, and the two pairs where one spelling is a prefix of another.
    #[test]
    fn the_switches_that_are_prefixes_of_each_other() {
        let cmd = parse("tt /H /I /V /E /DS /DUPLICATE /NOLOG");
        assert!(cmd.hide_title && cmd.minimize && cmd.hide_window);
        assert!(cmd.disable_tcp_echo_cr && cmd.duplicate_session && cmd.no_log);
        assert_eq!(cmd.host_dialog_on_startup, Some(false));
        // `/ES` is not `/E` with a stray S, and `/VTICON=` is not `/V`.
        let cmd = parse("tt /ES /VTICON=vt /TEKICON=tek");
        assert_eq!(cmd.host_dialog_on_startup, Some(true));
        assert!(!cmd.disable_tcp_echo_cr && !cmd.hide_window);
        assert_eq!(text(&cmd.vt_icon), "vt");
        assert_eq!(text(&cmd.tek_icon), "tek");
        // Case never matters, for any of them.
        assert!(parse("tt /h /duplicate").hide_title);
    }

    /// The numbers, and what a number that is not one does.
    #[test]
    fn a_position_and_a_timeout_are_ignored_when_they_do_not_parse() {
        let cmd = parse("tt /X=100 /Y=-20 /TIMEOUT=30");
        assert_eq!((cmd.window_x, cmd.window_y), (Some(100), Some(-20)));
        assert_eq!(cmd.connecting_timeout, Some(30));
        // `swscanf` returning 0 means the setting is left alone.
        let cmd = parse("tt /X=left /TIMEOUT=soon");
        assert_eq!((cmd.window_x, cmd.connecting_timeout), (None, None));
        // A negative timeout is refused rather than clamped.
        assert_eq!(parse("tt /TIMEOUT=-1").connecting_timeout, None);
        assert_eq!(parse("tt /TIMEOUT=0").connecting_timeout, Some(0));
        // `/SPEED=` has no such test: a baud rate that is not a number is 0.
        assert_eq!(parse("tt /SPEED=fast").baud, Some(0));
    }

    /// The first pass stops at the first `/F=`, and its value is a name rather
    /// than a resolved path — the caller reads the file, because whether it
    /// changed is what decides a re-read.
    #[test]
    fn the_first_setup_file_wins_and_is_not_resolved() {
        let cmd = parse("tt /F=first.ini /F=second.ini myhost");
        assert_eq!(text(&cmd.setup_file), "first.ini");
        assert_eq!(host(&cmd), "myhost");
        assert_eq!(parse("tt myhost").setup_file, None);
        // A quoted path with a space in it survives, which is the whole reason
        // the tokeniser is upstream's.
        let cmd = parse(r#"tt "/F=C:\Program Files\teraterm\my.ini""#);
        assert_eq!(text(&cmd.setup_file), r"C:\Program Files\teraterm\my.ini");
    }

    /// The paths, and the two arguments `GetFilePath` takes.
    #[test]
    fn get_file_path_adds_a_directory_and_an_extension() {
        assert_eq!(
            file_path(b"my.ini", Some(b"/home/nata"), Some(b".INI")),
            b"/home/nata/my.ini"
        );
        // No dot anywhere in the *file part* means the extension goes on.
        assert_eq!(
            file_path(b"my", Some(b"/etc"), Some(b".INI")),
            b"/etc/my.INI"
        );
        // A dot in a directory further up is not the file's.
        assert_eq!(
            file_path(b"tt/my", Some(b"/a.b"), Some(b".INI")),
            b"/a.b/tt/my.INI"
        );
        // A trailing dot counts, so nothing is added.
        assert_eq!(file_path(b"my.", Some(b"/etc"), Some(b".INI")), b"/etc/my.");
        // An absolute path is left where it is, on either platform's spelling.
        assert_eq!(
            file_path(b"/etc/my.ini", Some(b"/home"), None),
            b"/etc/my.ini"
        );
        assert_eq!(
            file_path(br"c:\tt\my.ini", Some(b"/home"), None),
            br"c:\tt\my.ini"
        );
        assert!(file_path(b"", Some(b"/home"), None).is_empty());
    }

    /// `ParseHostName`, which is five forms and one default.
    #[test]
    fn a_host_name_can_carry_a_scheme_a_port_and_brackets() {
        assert_eq!(parse_host_name(b"host"), (b"host".to_vec(), None));
        assert_eq!(parse_host_name(b"host:23"), (b"host".to_vec(), Some(23)));
        // A scheme Windows registered Tera Term for implies port 23.
        assert_eq!(
            parse_host_name(b"telnet://host/"),
            (b"host".to_vec(), Some(23))
        );
        assert_eq!(
            parse_host_name(b"TELNET://host"),
            (b"host".to_vec(), Some(23))
        );
        assert_eq!(
            parse_host_name(b"tn3270://host/"),
            (b"host".to_vec(), Some(23))
        );
        // ...but an explicit port in the URL beats it.
        assert_eq!(
            parse_host_name(b"telnet://host:2323/"),
            (b"host".to_vec(), Some(2323))
        );
        assert_eq!(
            parse_host_name(b"telnet://host:finger/"),
            (b"host".to_vec(), Some(79))
        );
        // A bracketed IPv6 literal loses its brackets, and the port search
        // starts after the address rather than at its first colon.
        assert_eq!(parse_host_name(b"[3ffe::1]"), (b"3ffe::1".to_vec(), None));
        assert_eq!(
            parse_host_name(b"[3ffe::1]:23"),
            (b"3ffe::1".to_vec(), Some(23))
        );
        assert_eq!(
            parse_host_name(b"telnet://[3ffe::1]:23/"),
            (b"3ffe::1".to_vec(), Some(23))
        );
        // An unbracketed one is cut at its first colon, which is upstream and
        // is why the brackets are not optional.
        assert_eq!(parse_host_name(b"3ffe::1"), (b"3ffe".to_vec(), Some(0)));
        // `telnet://` with nothing after it is an empty host on port 23.
        assert_eq!(parse_host_name(b"telnet://"), (b"".to_vec(), Some(23)));
    }

    /// The whole of it at once, from `teraterm.hlp`'s own examples.
    #[test]
    fn a_command_line_out_of_the_documentation() {
        let cmd = parse(r#"ttermpro myhost /nossh /T=1 /W="My Session" /L=out.log /FD=C:\tmp"#);
        assert_eq!(host(&cmd), "myhost");
        assert_eq!(cmd.telnet, Some(true));
        assert_eq!(text(&cmd.title), "My Session");
        assert_eq!(text(&cmd.log_file), "out.log");
        assert_eq!(text(&cmd.file_dir), r"C:\tmp");
        // `/nossh` is TTSSH's and Tera Term itself does not know it.
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));

        let cmd = parse("ttermpro /C=1 /SPEED=9600 /CPARITY=none /CDATABIT=8 /CSTOPBIT=1");
        assert_eq!(cmd.port_type, Some(PortType::Serial));
        assert_eq!((cmd.com_port, cmd.baud), (Some(1), Some(9600)));

        // A title of more than 49 characters is cut, and the multicast name at
        // MAX_PATH.
        let long = "x".repeat(60);
        assert_eq!(text(&parse(&format!("tt /W={long}")).title).len(), 49);
        let long = "y".repeat(300);
        assert_eq!(
            text(&parse(&format!("tt /MN={long}")).multicast_name).len(),
            259
        );
    }
}
