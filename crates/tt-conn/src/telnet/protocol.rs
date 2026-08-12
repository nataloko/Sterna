//! Telnet, as bytes in and bytes out — no socket, so it can be tested
//! exhaustively.
//!
//! Ported from Tera Term's `telnet.c` and the IAC handling in `ttpcmn/ttcmn.c`,
//! which are two files rather than one for a reason worth knowing: **the
//! framing and the negotiation live in different places upstream**, and the
//! framing runs first. `ttcmn.c` strips `IAC IAC` to one `0xFF`, swallows the
//! `NUL` after a `CR`, and hands everything after an `IAC` to `telnet.c` a byte
//! at a time. Reading `telnet.c` alone gives a parser that doubles `0xFF` and
//! passes `CR NUL` through.
//!
//! What is here is what a console server needs. One thing upstream has is
//! deliberately absent, and it is an **opt-in setting there too**, so its
//! absence is not a behaviour difference by default:
//!
//! - **LINEMODE.** `cv->TelLineMode` is gated by `ts->EnableLineMode`
//!   (`commlib.c:341`) and starts false. Local line editing is a terminal's
//!   job rather than a byte buffer's, and the buffer upstream uses would have
//!   to be re-decided against the grid.
//!
//! Local echo *is* here, and it is one of the places a setting and a protocol
//! state turn out to be the same variable — see
//! [`TelnetParams::echo_negotiates`], which is off by default, so a stock Tera
//! Term typing at a server that never says `WILL ECHO` shows nothing either.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

// Commands. RFC 854's names, which are also upstream's except that its
// `WILLTEL`/`DOTEL` spellings exist only to dodge Win32 macros.
pub const IAC: u8 = 255;
pub const DONT: u8 = 254;
pub const DO: u8 = 253;
pub const WONT: u8 = 252;
pub const WILL: u8 = 251;
pub const SB: u8 = 250;
pub const GA: u8 = 249;
pub const SE: u8 = 240;
pub const NOP: u8 = 241;
pub const DM: u8 = 242;
pub const BRK: u8 = 243;
pub const AYT: u8 = 246;

// Options, of the thirty-five upstream knows.
pub const OPT_BINARY: u8 = 0;
pub const OPT_ECHO: u8 = 1;
pub const OPT_SGA: u8 = 3;
pub const OPT_TERMTYPE: u8 = 24;
pub const OPT_NAWS: u8 = 31;
pub const OPT_TERMSPEED: u8 = 32;

/// `telnet.h`'s `MaxTelOpt`. Anything above it is refused flat — `DONT` for a
/// `WILL`, `WONT` for a `DO` — without a table entry.
///
/// Reproduced rather than widened. It means `NEW-ENVIRON` (39) and `CHARSET`
/// (42) are declined, which is what a server sees from Tera Term today, and
/// accepting them here would be inventing behaviour rather than porting it.
pub const MAX_OPTION: u8 = 34;

/// Something that arrived and is not data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelnetEvent {
    /// `IAC BRK`. The same thing a serial line break means to the host, which
    /// is why it becomes a [`TransportEvent::Break`](crate::TransportEvent).
    Break,
    /// The far end sent `NAWS` at *us*.
    ///
    /// RFC 1073 defines NAWS as client-to-server, and upstream accepts it in
    /// both directions anyway (`telnet.c:298`) — with the status check
    /// commented out, so it acts on the subnegotiation even when NAWS was
    /// never agreed. That is how a console server tells a client what size the
    /// far end really is, and it is reproduced including the laxity.
    Resize { cols: u16, rows: u16 },
    /// The `ECHO` negotiation moved, and [`TelnetParams::echo_negotiates`] says
    /// it decides local echo. True means *the terminal* should echo.
    ///
    /// Emitted only where upstream assigns `ts.LocalEcho`: the tail of
    /// `ParseTelWill` and of `ParseTelWont` (`telnet.c:411`, `:497`), on `ECHO`
    /// alone, and only when the option settled at Yes or No — a half-negotiated
    /// `WantYes` leaves the setting where it was.
    LocalEcho(bool),
}

/// How much of the protocol to speak.
///
/// Upstream spells this as two settings and a rule, and the three of them make
/// **four** states rather than two:
///
/// - `ts.Telnet` becomes `cv->TelFlag` at open (`commlib.c:340`), which is what
///   turns IAC framing on. It says nothing about the negotiation.
/// - `ts.TelAutoDetect` becomes `cv->TelAutoDetect` beside it (`:323`),
///   unconditionally, and `ttcmn.c:590` turns the framing on at the first
///   `0xFF` when `TelFlag` is clear. So it only matters when `Telnet=off`.
/// - The opening burst goes out **only when the port is the telnet port**
///   (`vtwin.cpp:3666`, `ts.TCPPort == ts.TelPort`). That rule matters: a
///   console server with one TCP port per serial line is not a telnet server,
///   and opening at it with `WILL TERMTYPE` puts five bytes of protocol into
///   somebody's serial console.
///
/// Use [`TelnetMode::of`] rather than assembling it by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TelnetMode {
    /// Every byte is data, `0xFF` included. What a port that streams binary
    /// needs, and the only mode that cannot corrupt one.
    ///
    /// `Telnet=off` **and** `TelAutoDetect=off`, which is two keys away from
    /// the shipped defaults.
    Raw,
    /// Data until the first `IAC` arrives, and telnet from then on.
    ///
    /// `Telnet=off` with `TelAutoDetect` left alone — so it is what `/T=0`
    /// really gets, not [`Raw`](TelnetMode::Raw).
    #[default]
    Auto,
    /// Telnet from the first byte, and not a word offered.
    ///
    /// `Telnet=on` at a port that is not the telnet port. Framing without
    /// negotiation looks like a contradiction and is the common case on a
    /// terminal server: the framing has to be on for `IAC IAC` and `CR NUL` to
    /// mean what the far end intends by them, while the burst would be noise
    /// on a line that is really a serial cable.
    Framed,
    /// Telnet from the first byte, opening with the negotiation upstream
    /// opens with. `Telnet=on` at the telnet port.
    Negotiate,
}

impl TelnetMode {
    /// The four states, from the two settings and the port test.
    ///
    /// `telnet` is `ts.Telnet`, `auto_detect` is `ts.TelAutoDetect`, and
    /// `telnet_port` is `ts.TCPPort == ts.TelPort` — the same comparison
    /// `vtwin.cpp:3666` makes, which is a test of whether a port was chosen for
    /// a protocol rather than a literal 23.
    pub fn of(telnet: bool, auto_detect: bool, telnet_port: bool) -> TelnetMode {
        match (telnet, auto_detect, telnet_port) {
            (true, _, true) => TelnetMode::Negotiate,
            (true, _, false) => TelnetMode::Framed,
            (false, true, _) => TelnetMode::Auto,
            (false, false, _) => TelnetMode::Raw,
        }
    }

    /// What upstream's shipped settings would do at `port`: negotiate on 23,
    /// frame everywhere else.
    ///
    /// Both keys default on, so this is `of(true, true, port == 23)` — and the
    /// answer for a port that is not 23 is [`Framed`](TelnetMode::Framed),
    /// **not** [`Auto`](TelnetMode::Auto). It used to be `Auto` here, which
    /// silently passed a `CR NUL` through to the terminal for as long as a host
    /// went without sending an `IAC`.
    pub fn for_port(port: u16) -> TelnetMode {
        TelnetMode::of(true, true, port == 23)
    }

    /// Whether IAC framing is on from the first byte.
    fn active_at_open(self) -> bool {
        matches!(self, TelnetMode::Framed | TelnetMode::Negotiate)
    }
}

/// RFC 1143's "Q method", which is what `telnet.c` implements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OptState {
    #[default]
    No,
    Yes,
    WantNo,
    WantYes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Queue {
    #[default]
    Empty,
    Opposite,
}

#[derive(Clone, Copy, Debug, Default)]
struct Opt {
    /// Whether to agree when asked. Upstream's `Accept`, set once in
    /// `InitTelnet`.
    accept: bool,
    state: OptState,
    queue: Queue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Data.
    Data,
    /// After an `IAC` in the data stream.
    Iac,
    /// After `IAC SB`.
    Sub,
    Will,
    Wont,
    Do,
    Dont,
}

/// What the terminal tells the far end about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelnetParams {
    pub mode: TelnetMode,
    /// `ts.TermType`, sent for `TERMINAL-TYPE`. **Upstream's default is plain
    /// `xterm`** (`ttset.c:961`); the one below is this crate's, for a caller
    /// with no settings behind it.
    pub term_type: String,
    /// `ts.TerminalInputSpeed` / `OutputSpeed`, sent for `TERMINAL-SPEED`.
    /// Upstream defaults both to 38400 (`ttset.c:1941`), and the *output*
    /// speed's real default is the input one — see
    /// `tt_session::open::terminal_speed`.
    pub speed: (u32, u32),
    pub cols: u16,
    pub rows: u16,
    /// Offer `BINARY` in the opening burst. Upstream's `ts.TelBin`, which
    /// defaults **off** (`ttset.c:1301`) — it will agree if asked, but it does
    /// not ask.
    pub binary: bool,
    /// `ts.TelEcho` (`ttset.c:1304`), off by default. Whether the `ECHO`
    /// option and the local-echo setting are one variable.
    ///
    /// Off — the shipped state — means the burst asks the server to echo and
    /// the answer changes nothing locally, so a server that declines leaves a
    /// terminal that shows nothing as you type. On makes the negotiation drive
    /// local echo ([`TelnetEvent::LocalEcho`]) and makes the burst ask
    /// according to [`local_echo`](TelnetParams::local_echo) rather than
    /// asking flat.
    pub echo_negotiates: bool,
    /// `ts.LocalEcho` as the terminal currently has it, read only by
    /// `TelChangeEcho` (`telnet.c:845`) and so only when
    /// [`echo_negotiates`](TelnetParams::echo_negotiates) is on.
    pub local_echo: bool,
    /// `ts.TelKeepAliveInterval` (`ttset.c:1314`), 300 seconds by default, zero
    /// meaning none. See [`TelnetConn::poll_keepalive`](super::TelnetConn::poll_keepalive)
    /// — nothing in this module has a clock.
    pub keepalive: Option<Duration>,
    /// `TelLog` (`ttset.c:1307`), as the file it names rather than as a flag:
    /// upstream's `TELNET.LOG` in `ts.LogDirW`, truncated at every connection
    /// (`telnet.c:127`, `CREATE_ALWAYS`).
    ///
    /// One field rather than a bool and a path, so there is no state where the
    /// log is on and has nowhere to go. Nothing here opens it — the transcript
    /// accumulates and [`TelnetConn`](super::TelnetConn) writes it.
    pub log: Option<PathBuf>,
    /// `[TTProxy]`, when the file configures one. It is a property of the
    /// *connection* rather than of the protocol, so nothing in this module
    /// reads it — [`TelnetConn::connect`](super::TelnetConn::connect) hands it
    /// to [`crate::proxy::dial`] and everything below that point is the same
    /// socket it would otherwise have opened.
    ///
    /// Upstream has no field for this at all: `TTProxy` hooks `connect(2)`, so
    /// the terminal never knows. See `crate::proxy` for what that costs it.
    ///
    /// Boxed because it is large and almost always absent: the two params
    /// structs are moved through `Target` and `Startup`, and a quarter of a
    /// kilobyte of prompt strings nobody configured would ride along.
    pub proxy: Option<Box<crate::proxy::ProxyParams>>,
}

impl Default for TelnetParams {
    fn default() -> TelnetParams {
        TelnetParams {
            mode: TelnetMode::Auto,
            term_type: "xterm-256color".to_string(),
            speed: (38400, 38400),
            cols: 80,
            rows: 24,
            binary: false,
            echo_negotiates: false,
            local_echo: false,
            keepalive: Some(Duration::from_secs(300)),
            log: None,
            proxy: None,
        }
    }
}

/// The protocol, as a state machine over bytes.
pub struct Telnet {
    params: TelnetParams,
    /// False in [`TelnetMode::Raw`], and in [`TelnetMode::Auto`] until the
    /// first `IAC`.
    active: bool,
    /// The transcript `TelLog` asks for, drained by [`Telnet::take_log`].
    log: String,
    state: State,
    /// `ttcmn.c`'s `TelCRFlag`: a `NUL` immediately after a `CR` is the
    /// line-ending escape and not data.
    after_cr: bool,
    sub: Vec<u8>,
    sub_iac: bool,
    mine: [Opt; 256],
    his: [Opt; 256],
    /// Bytes to send back, drained with [`take_reply`](Telnet::take_reply).
    out: Vec<u8>,
    binary_send: bool,
    binary_recv: bool,
    /// The size last sent, so a resize that changes nothing sends nothing.
    sent_size: Option<(u16, u16)>,
}

impl Telnet {
    pub fn new(params: TelnetParams) -> Telnet {
        let mut t = Telnet {
            active: params.mode.active_at_open(),
            log: String::new(),
            state: State::Data,
            after_cr: false,
            sub: Vec::new(),
            sub_iac: false,
            mine: [Opt::default(); 256],
            his: [Opt::default(); 256],
            out: Vec::new(),
            binary_send: false,
            binary_recv: false,
            sent_size: None,
            params,
        };
        // `InitTelnet`'s table, exactly. Note ECHO is only ever *his* and
        // TERMTYPE/TERMSPEED only ever *mine*: a client does not echo for a
        // server, and a server has no terminal type to report.
        for o in [OPT_BINARY, OPT_SGA] {
            t.mine[o as usize].accept = true;
            t.his[o as usize].accept = true;
        }
        t.his[OPT_ECHO as usize].accept = true;
        t.mine[OPT_TERMTYPE as usize].accept = true;
        t.mine[OPT_TERMSPEED as usize].accept = true;
        t.mine[OPT_NAWS as usize].accept = true;
        t.his[OPT_NAWS as usize].accept = true;

        if t.params.mode == TelnetMode::Negotiate {
            t.open();
        }
        t
    }

    /// `vtwin.cpp:3669`'s burst, in its order. Order is not cosmetic: a server
    /// answers in the order it was asked, and TERMTYPE first is what makes the
    /// `SB TERMTYPE SEND` arrive before the shell starts.
    fn open(&mut self) {
        self.enable_mine(OPT_TERMTYPE);
        self.enable_his(OPT_SGA);
        self.enable_mine(OPT_SGA);
        // `vtwin.cpp:3675`. `ts.TelEcho` defaults off, which takes the `else`
        // — asking the server to echo, flat. On, it is `TelChangeEcho`
        // (`telnet.c:845`): ask the server to echo only if the terminal is not
        // already doing it, and ask it to **stop** if the terminal is. The two
        // ends of one variable, so a `LocalEcho=on` file opens with
        // `DONT ECHO`, which is the opposite request from the default's.
        match (self.params.echo_negotiates, self.params.local_echo) {
            (true, true) => self.disable_his(OPT_ECHO),
            _ => self.enable_his(OPT_ECHO),
        }
        self.enable_mine(OPT_NAWS);
        if self.params.binary {
            self.enable_mine(OPT_BINARY);
            self.enable_his(OPT_BINARY);
        }
        self.enable_mine(OPT_TERMSPEED);
    }

    /// Bytes to send. Empty most of the time.
    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    pub fn has_reply(&self) -> bool {
        !self.out.is_empty()
    }

    /// Whether the far end agreed to echo. False means nothing is echoing, and
    /// what to do about that is a settings question — see the module docs.
    pub fn server_echoes(&self) -> bool {
        self.his[OPT_ECHO as usize].state == OptState::Yes
    }

    /// Whether `BINARY` was agreed for what we send. When false, a `CR` on the
    /// wire has to be followed by `NUL`.
    pub fn binary_send(&self) -> bool {
        self.binary_send
    }

    /// Decode `bytes`, appending data to `data` and anything else to `events`.
    pub fn feed(&mut self, bytes: &[u8], data: &mut Vec<u8>, events: &mut Vec<TelnetEvent>) {
        for &b in bytes {
            self.byte(b, data, events);
        }
    }

    fn byte(&mut self, b: u8, data: &mut Vec<u8>, events: &mut Vec<TelnetEvent>) {
        // `ttcmn.c:572`, and it runs *before* the IAC check: the flag is
        // cleared whatever the byte is, so `CR IAC …` processes the IAC and
        // only `CR NUL` loses the NUL.
        if self.after_cr {
            self.after_cr = false;
            if b == 0 {
                return;
            }
        }

        if !self.active {
            if self.params.mode == TelnetMode::Auto && b == IAC {
                // `ttcmn.c:590` — the first IAC on a TCP port turns telnet on.
                self.active = true;
            } else {
                data.push(b);
                return;
            }
        }

        match self.state {
            State::Data => match b {
                IAC => self.state = State::Iac,
                0x0D if !self.binary_recv => {
                    // The CR itself is data; only a NUL after it is not.
                    self.after_cr = true;
                    data.push(b);
                }
                _ => data.push(b),
            },
            State::Iac => match b {
                // A doubled IAC is one data byte, and `after_cr` must not be
                // set by it — `ttcmn.c` unescapes before the CR test.
                IAC => {
                    data.push(IAC);
                    self.state = State::Data;
                }
                SB => {
                    self.sub.clear();
                    self.sub_iac = false;
                    self.state = State::Sub;
                }
                WILL => self.state = State::Will,
                WONT => self.state = State::Wont,
                DO => self.state = State::Do,
                DONT => self.state = State::Dont,
                BRK => {
                    events.push(TelnetEvent::Break);
                    self.state = State::Data;
                }
                // NOP, DM, IP, AO, AYT, EC, EL, GA, SE and anything else all
                // go back to idle without a reply, which is `ParseTelIAC`.
                // Upstream does not answer an AYT; neither do we.
                _ => self.state = State::Data,
            },
            State::Sub => self.sub_byte(b, events),
            State::Will => {
                self.will(b);
                self.echo_settled(b, events);
                self.state = State::Data;
            }
            State::Wont => {
                self.wont(b);
                self.echo_settled(b, events);
                self.state = State::Data;
            }
            State::Do => {
                self.do_(b);
                self.state = State::Data;
            }
            State::Dont => {
                self.dont(b);
                self.state = State::Data;
            }
        }
    }

    fn sub_byte(&mut self, b: u8, events: &mut Vec<TelnetEvent>) {
        if self.sub_iac {
            self.sub_iac = false;
            match b {
                SE => {
                    self.end_sub(events);
                    self.state = State::Data;
                    return;
                }
                // A doubled IAC inside a subnegotiation is the data byte 255.
                // Falls through to the push below, which is upstream's comment
                // at `telnet.c:325` made structural.
                IAC => {}
                // `IAC <anything else>` inside SB: upstream abandons the
                // subnegotiation and re-runs the byte as a command.
                other => {
                    self.sub.clear();
                    self.state = State::Iac;
                    let mut sink = Vec::new();
                    self.byte(other, &mut sink, events);
                    return;
                }
            }
        } else if b == IAC {
            self.sub_iac = true;
            return;
        }
        // Upstream's buffer is 50 bytes and it stops storing past that while
        // still consuming to the terminator; a longer subnegotiation is
        // truncated, not a parse error.
        if self.sub.len() < 50 {
            self.sub.push(b);
        }
    }

    fn end_sub(&mut self, events: &mut Vec<TelnetEvent>) {
        let sub = std::mem::take(&mut self.sub);
        // `telnet.c:276` — a subnegotiation with no parameter is dropped.
        if sub.len() <= 1 {
            return;
        }
        match sub[0] {
            OPT_TERMTYPE if self.mine[OPT_TERMTYPE as usize].state == OptState::Yes => {
                // 1 is SEND. Anything else is not a request for our name.
                if sub[1] == 1 {
                    let mut reply = vec![IAC, SB, OPT_TERMTYPE, 0];
                    reply.extend_from_slice(self.params.term_type.as_bytes());
                    reply.extend_from_slice(&[IAC, SE]);
                    self.emit(&reply);
                }
            }
            OPT_TERMSPEED if self.mine[OPT_TERMSPEED as usize].state == OptState::Yes => {
                if sub[1] == 1 {
                    let mut reply = vec![IAC, SB, OPT_TERMSPEED, 0];
                    let mut text = String::new();
                    let _ = write!(text, "{},{}", self.params.speed.0, self.params.speed.1);
                    reply.extend_from_slice(text.as_bytes());
                    reply.extend_from_slice(&[IAC, SE]);
                    self.emit(&reply);
                }
            }
            // No status check, which is upstream's — the test is commented
            // out at `telnet.c:299`. A server that sends NAWS without
            // negotiating it is still telling us something true.
            OPT_NAWS if sub.len() >= 5 => {
                events.push(TelnetEvent::Resize {
                    cols: u16::from(sub[1]) << 8 | u16::from(sub[2]),
                    rows: u16::from(sub[3]) << 8 | u16::from(sub[4]),
                });
            }
            _ => {}
        }
    }

    fn send(&mut self, command: u8, option: u8) {
        self.emit(&[IAC, command, option]);
    }

    /// Queue a protocol record, and log it if `TelLog` asked.
    ///
    /// One function because upstream's `TelWriteLog` sits directly after a
    /// `CommRawOut` at all eight of its call sites and never anywhere else —
    /// which is the whole reason `TELNET.LOG` holds only what Tera Term said.
    /// Data written by the terminal does not come through here.
    fn emit(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
        if self.params.log.is_none() {
            return;
        }
        // `TelWriteLog` (`telnet.c:169`): CRLF, a `>`, then each byte as a
        // space and two upper-case hex digits. There is no trailing newline —
        // the next record's own CRLF ends the line, so the file has a blank
        // first line and no final one.
        self.log.push_str("\r\n>");
        for b in bytes {
            let _ = write!(self.log, " {b:02X}");
        }
    }

    /// Take the `TELNET.LOG` transcript accumulated so far. Empty unless
    /// [`TelnetParams::log`] is on.
    pub fn take_log(&mut self) -> String {
        std::mem::take(&mut self.log)
    }

    // --- the Q method -------------------------------------------------------

    fn will(&mut self, o: u8) {
        if o > MAX_OPTION {
            self.send(DONT, o);
        } else {
            let opt = self.his[o as usize];
            match opt.state {
                OptState::No => {
                    if opt.accept {
                        self.send(DO, o);
                        self.his[o as usize].state = OptState::Yes;
                    } else {
                        self.send(DONT, o);
                    }
                }
                OptState::WantNo => match opt.queue {
                    Queue::Empty => self.his[o as usize].state = OptState::No,
                    Queue::Opposite => self.his[o as usize].state = OptState::Yes,
                },
                OptState::WantYes => match opt.queue {
                    Queue::Empty => self.his[o as usize].state = OptState::Yes,
                    Queue::Opposite => {
                        self.his[o as usize].state = OptState::WantNo;
                        self.his[o as usize].queue = Queue::Empty;
                        self.send(DONT, o);
                    }
                },
                OptState::Yes => {}
            }
        }
        if o == OPT_BINARY {
            self.binary_recv = self.his[OPT_BINARY as usize].state == OptState::Yes;
        }
    }

    fn wont(&mut self, o: u8) {
        if o > MAX_OPTION {
            self.send(DONT, o);
        } else {
            let opt = self.his[o as usize];
            match opt.state {
                OptState::Yes => {
                    self.his[o as usize].state = OptState::No;
                    self.send(DONT, o);
                }
                OptState::WantNo => match opt.queue {
                    Queue::Empty => self.his[o as usize].state = OptState::No,
                    Queue::Opposite => {
                        self.his[o as usize].state = OptState::WantYes;
                        self.his[o as usize].queue = Queue::Empty;
                        self.send(DO, o);
                    }
                },
                OptState::WantYes => {
                    self.his[o as usize].state = OptState::No;
                    self.his[o as usize].queue = Queue::Empty;
                }
                OptState::No => {}
            }
        }
        if o == OPT_BINARY {
            self.binary_recv = self.his[OPT_BINARY as usize].state == OptState::Yes;
        }
    }

    fn do_(&mut self, o: u8) {
        if o > MAX_OPTION {
            self.send(WONT, o);
        } else {
            let opt = self.mine[o as usize];
            match opt.state {
                OptState::No => {
                    if opt.accept {
                        self.send(WILL, o);
                        self.mine[o as usize].state = OptState::Yes;
                    } else {
                        self.send(WONT, o);
                    }
                }
                OptState::WantNo => match opt.queue {
                    Queue::Empty => self.mine[o as usize].state = OptState::No,
                    Queue::Opposite => self.mine[o as usize].state = OptState::Yes,
                },
                OptState::WantYes => match opt.queue {
                    Queue::Empty => self.mine[o as usize].state = OptState::Yes,
                    Queue::Opposite => {
                        self.mine[o as usize].state = OptState::WantNo;
                        self.mine[o as usize].queue = Queue::Empty;
                        self.send(WONT, o);
                    }
                },
                OptState::Yes => {}
            }
        }
        match o {
            OPT_BINARY => self.binary_send = self.mine[OPT_BINARY as usize].state == OptState::Yes,
            // The size goes out the moment NAWS is agreed, not on the next
            // resize — a server that asked wants it now.
            OPT_NAWS if self.mine[OPT_NAWS as usize].state == OptState::Yes => self.send_size(),
            _ => {}
        }
    }

    fn dont(&mut self, o: u8) {
        if o > MAX_OPTION {
            self.send(WONT, o);
        } else {
            let opt = self.mine[o as usize];
            match opt.state {
                OptState::Yes => {
                    self.mine[o as usize].state = OptState::No;
                    self.send(WONT, o);
                }
                OptState::WantNo => match opt.queue {
                    Queue::Empty => self.mine[o as usize].state = OptState::No,
                    Queue::Opposite => {
                        self.mine[o as usize].state = OptState::WantYes;
                        self.mine[o as usize].queue = Queue::Empty;
                        self.send(WILL, o);
                    }
                },
                OptState::WantYes => {
                    self.mine[o as usize].state = OptState::No;
                    self.mine[o as usize].queue = Queue::Empty;
                }
                OptState::No => {}
            }
        }
        if o == OPT_BINARY {
            self.binary_send = self.mine[OPT_BINARY as usize].state == OptState::Yes;
        }
    }

    fn enable_mine(&mut self, o: u8) {
        if o > MAX_OPTION {
            return;
        }
        match self.mine[o as usize].state {
            OptState::No => {
                self.mine[o as usize].state = OptState::WantYes;
                self.send(WILL, o);
            }
            OptState::WantNo if self.mine[o as usize].queue == Queue::Empty => {
                self.mine[o as usize].queue = Queue::Opposite
            }
            OptState::WantYes if self.mine[o as usize].queue == Queue::Opposite => {
                self.mine[o as usize].queue = Queue::Empty
            }
            _ => {}
        }
    }

    fn enable_his(&mut self, o: u8) {
        if o > MAX_OPTION {
            return;
        }
        match self.his[o as usize].state {
            OptState::No => {
                self.his[o as usize].state = OptState::WantYes;
                self.send(DO, o);
            }
            OptState::WantNo if self.his[o as usize].queue == Queue::Empty => {
                self.his[o as usize].queue = Queue::Opposite
            }
            OptState::WantYes if self.his[o as usize].queue == Queue::Opposite => {
                self.his[o as usize].queue = Queue::Empty
            }
            _ => {}
        }
    }

    /// The `case ECHO:` that ends both `ParseTelWill` and `ParseTelWont`
    /// (`telnet.c:411`, `:497`) — identical in the two, and outside the
    /// `b <= MaxTelOpt` guard above it, so it runs on the option's state
    /// whatever the state machine did with it.
    fn echo_settled(&mut self, o: u8, events: &mut Vec<TelnetEvent>) {
        if o != OPT_ECHO || !self.params.echo_negotiates {
            return;
        }
        match self.his[OPT_ECHO as usize].state {
            // The far end echoes, so the terminal must not.
            OptState::Yes => events.push(TelnetEvent::LocalEcho(false)),
            OptState::No => events.push(TelnetEvent::LocalEcho(true)),
            // Mid-negotiation. Upstream's `default:` leaves `ts.LocalEcho`
            // alone rather than guessing at the answer.
            _ => {}
        }
    }

    /// `TelDisableHisOpt` (`telnet.c:735`), the mirror of [`Telnet::enable_his`]
    /// and reached from one caller: `TelChangeEcho` when local echo is already
    /// on.
    fn disable_his(&mut self, o: u8) {
        if o > MAX_OPTION {
            return;
        }
        match self.his[o as usize].state {
            OptState::Yes => {
                self.his[o as usize].state = OptState::WantNo;
                self.send(DONT, o);
            }
            OptState::WantNo if self.his[o as usize].queue == Queue::Opposite => {
                self.his[o as usize].queue = Queue::Empty
            }
            OptState::WantYes if self.his[o as usize].queue == Queue::Empty => {
                self.his[o as usize].queue = Queue::Opposite
            }
            _ => {}
        }
    }

    // --- what we send -------------------------------------------------------

    /// Tell the far end the window changed size, if it agreed to hear it.
    ///
    /// Silent when nothing changed, which is `TelInformWinSize`'s own guard: a
    /// window being dragged emits a resize per pixel row and each one would
    /// otherwise be nine bytes on the wire.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.params.cols = cols;
        self.params.rows = rows;
        if self.mine[OPT_NAWS as usize].state == OptState::Yes
            && self.sent_size != Some((cols, rows))
        {
            self.send_size();
        }
    }

    fn send_size(&mut self) {
        let (cols, rows) = (self.params.cols, self.params.rows);
        let mut record = vec![IAC, SB, OPT_NAWS];
        // Each byte of the size is escaped on its own: a terminal 255 columns
        // wide would otherwise put a bare IAC inside the subnegotiation and
        // end it early.
        for byte in [(cols >> 8) as u8, cols as u8, (rows >> 8) as u8, rows as u8] {
            if byte == IAC {
                record.push(IAC);
            }
            record.push(byte);
        }
        record.extend_from_slice(&[IAC, SE]);
        self.emit(&record);
        self.sent_size = Some((cols, rows));
    }

    /// `IAC BRK`, which is what a telnet console server turns into a real
    /// line break on the serial port behind it.
    pub fn queue_break(&mut self) {
        self.emit(&[IAC, BRK]);
    }

    /// `IAC AYT`.
    pub fn queue_are_you_there(&mut self) {
        self.emit(&[IAC, AYT]);
    }

    /// `IAC NOP`, upstream's keepalive.
    pub fn queue_nop(&mut self) {
        self.emit(&[IAC, NOP]);
    }

    /// Escape outbound data: `CommBinaryOut`, `ttcmn.c:633`.
    ///
    /// Two substitutions and both matter. `0xFF` doubles or the far end reads
    /// it as a command. A `CR` gains a `NUL` unless `BINARY` was agreed —
    /// RFC 854 says a bare `CR` in NVT ASCII is not a line ending, and a
    /// server that follows it treats `CR` alone as undefined.
    pub fn encode(&self, data: &[u8], out: &mut Vec<u8>) {
        if !self.active {
            out.extend_from_slice(data);
            return;
        }
        for &b in data {
            out.push(b);
            if b == 0x0D && !self.binary_send {
                out.push(0);
            } else if b == IAC {
                out.push(IAC);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed bytes and collect everything that came of them.
    fn run(t: &mut Telnet, bytes: &[u8]) -> (Vec<u8>, Vec<TelnetEvent>, Vec<u8>) {
        let (mut data, mut events) = (Vec::new(), Vec::new());
        t.feed(bytes, &mut data, &mut events);
        (data, events, t.take_reply())
    }

    /// Telnet on, with nothing asked for yet — the state a *server*-initiated
    /// negotiation actually starts from, and the only one in which a reply to
    /// a `DO` is visible. After the opening burst most options are already
    /// `WantYes`, so the server's answer completes them in silence; a test
    /// that expects a reply there is testing the wrong state.
    fn passive() -> Telnet {
        let mut t = Telnet::new(TelnetParams {
            term_type: "vt100".into(),
            ..TelnetParams::default()
        });
        // Auto turns on at the first IAC; a NOP is the cheapest one.
        let (mut data, mut events) = (Vec::new(), Vec::new());
        t.feed(&[IAC, NOP], &mut data, &mut events);
        t
    }

    fn negotiating() -> Telnet {
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            term_type: "vt100".into(),
            speed: (38400, 38400),
            cols: 80,
            rows: 24,
            binary: false,
            ..TelnetParams::default()
        });
        t.take_reply(); // discard the opening burst
        t
    }

    #[test]
    fn the_opening_burst_is_upstreams_in_upstreams_order() {
        // A server answers in the order it was asked, and TERMTYPE first is
        // what gets `SB TERMTYPE SEND` in before the shell starts.
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            ..TelnetParams::default()
        });
        assert_eq!(
            t.take_reply(),
            vec![
                IAC,
                WILL,
                OPT_TERMTYPE,
                IAC,
                DO,
                OPT_SGA,
                IAC,
                WILL,
                OPT_SGA,
                IAC,
                DO,
                OPT_ECHO,
                IAC,
                WILL,
                OPT_NAWS,
                IAC,
                WILL,
                OPT_TERMSPEED,
            ]
        );
    }

    #[test]
    fn binary_is_offered_only_when_asked_for() {
        // `ts.TelBin` defaults off: upstream agrees to BINARY if the server
        // proposes it, and never proposes it itself.
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            binary: true,
            ..TelnetParams::default()
        });
        let burst = t.take_reply();
        assert!(burst.windows(3).any(|w| w == [IAC, WILL, OPT_BINARY]));
        assert!(burst.windows(3).any(|w| w == [IAC, DO, OPT_BINARY]));
    }

    #[test]
    fn raw_mode_never_looks_at_a_byte() {
        // The mode a console server on a per-line port needs: an 0xFF in a
        // binary stream is data, and eating it corrupts a firmware upload.
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Raw,
            ..TelnetParams::default()
        });
        let (data, events, reply) = run(&mut t, &[IAC, DO, OPT_NAWS, 0x0D, 0x00, IAC, IAC]);
        assert_eq!(data, vec![IAC, DO, OPT_NAWS, 0x0D, 0x00, IAC, IAC]);
        assert!(events.is_empty());
        assert!(reply.is_empty());
    }

    #[test]
    fn auto_mode_is_raw_until_the_first_iac() {
        let mut t = Telnet::new(TelnetParams::default());
        // `0x0D 0x00` before any IAC is two data bytes — telnet is not on yet,
        // so the NUL is not an escape.
        let (data, _, reply) = run(&mut t, b"hi\x0d\x00");
        assert_eq!(data, b"hi\x0d\x00");
        assert!(reply.is_empty());
        // And from the first IAC on, it is telnet.
        let (data, _, reply) = run(&mut t, &[IAC, DO, OPT_TERMTYPE, b'x']);
        assert_eq!(data, b"x");
        assert_eq!(reply, vec![IAC, WILL, OPT_TERMTYPE]);
    }

    #[test]
    fn a_doubled_iac_is_one_data_byte() {
        let mut t = negotiating();
        let (data, _, _) = run(&mut t, &[b'a', IAC, IAC, b'b']);
        assert_eq!(data, vec![b'a', 0xFF, b'b']);
    }

    #[test]
    fn a_nul_after_a_carriage_return_is_the_escape_and_not_data() {
        let mut t = negotiating();
        let (data, _, _) = run(&mut t, b"a\x0d\x00b");
        assert_eq!(data, b"a\x0db");
        // But CR LF is two real bytes, and both reach the terminal.
        let (data, _, _) = run(&mut t, b"a\x0d\x0ab");
        assert_eq!(data, b"a\x0d\x0ab");
        // ...and a NUL that is not after a CR is data.
        let (data, _, _) = run(&mut t, b"a\x00b");
        assert_eq!(data, b"a\x00b");
    }

    #[test]
    fn a_carriage_return_does_not_swallow_a_following_command() {
        // `ttcmn.c` clears the CR flag whatever the next byte is, so only a
        // NUL is lost. Getting this wrong drops the IAC and leaves the
        // negotiation one byte out of step for the rest of the session.
        let mut t = passive();
        let (data, _, reply) = run(&mut t, &[0x0D, IAC, DO, OPT_TERMTYPE]);
        assert_eq!(data, vec![0x0D]);
        assert_eq!(reply, vec![IAC, WILL, OPT_TERMTYPE]);
    }

    #[test]
    fn a_split_command_survives_the_read_boundary() {
        // Three reads, one command. A parser that reset per call would treat
        // the option byte as data.
        let mut t = passive();
        let (data, _, reply) = run(&mut t, &[IAC]);
        assert!(data.is_empty() && reply.is_empty());
        let (_, _, reply) = run(&mut t, &[DO]);
        assert!(reply.is_empty());
        let (_, _, reply) = run(&mut t, &[OPT_TERMTYPE]);
        assert_eq!(reply, vec![IAC, WILL, OPT_TERMTYPE]);
    }

    #[test]
    fn an_option_above_the_table_is_refused_flat() {
        // `MaxTelOpt` is 34, so NEW-ENVIRON (39) and CHARSET (42) are
        // declined. Reproduced rather than widened: this is what a server
        // sees from Tera Term today.
        let mut t = negotiating();
        let (_, _, reply) = run(&mut t, &[IAC, WILL, 39, IAC, DO, 42]);
        assert_eq!(reply, vec![IAC, DONT, 39, IAC, WONT, 42]);
    }

    #[test]
    fn an_option_we_do_not_accept_is_refused() {
        let mut t = negotiating();
        // ECHO is only ever *his*: a client does not echo for a server.
        let (_, _, reply) = run(&mut t, &[IAC, DO, OPT_ECHO]);
        assert_eq!(reply, vec![IAC, WONT, OPT_ECHO]);
    }

    #[test]
    fn agreeing_twice_says_nothing_the_second_time() {
        // The whole point of the Q method: a naive implementation answers
        // every WILL with a DO and two of them loop forever.
        let mut t = passive();
        let (_, _, first) = run(&mut t, &[IAC, WILL, OPT_SGA]);
        assert_eq!(first, vec![IAC, DO, OPT_SGA]);
        let (_, _, second) = run(&mut t, &[IAC, WILL, OPT_SGA]);
        assert!(second.is_empty(), "answered a second WILL: {second:?}");
    }

    #[test]
    fn an_answer_to_our_own_request_is_not_answered_again() {
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            ..TelnetParams::default()
        });
        t.take_reply();
        // We asked WILL TERMTYPE; the server's DO completes it silently.
        let (_, _, reply) = run(&mut t, &[IAC, DO, OPT_TERMTYPE]);
        assert!(reply.is_empty(), "{reply:?}");
    }

    #[test]
    fn terminal_type_is_answered_only_after_it_is_agreed() {
        let mut t = negotiating();
        // Before agreement, a SEND gets nothing.
        let (_, _, reply) = run(&mut t, &[IAC, SB, OPT_TERMTYPE, 1, IAC, SE]);
        assert!(reply.is_empty(), "{reply:?}");

        run(&mut t, &[IAC, DO, OPT_TERMTYPE]);
        let (_, _, reply) = run(&mut t, &[IAC, SB, OPT_TERMTYPE, 1, IAC, SE]);
        let mut want = vec![IAC, SB, OPT_TERMTYPE, 0];
        want.extend_from_slice(b"vt100");
        want.extend_from_slice(&[IAC, SE]);
        assert_eq!(reply, want);
    }

    #[test]
    fn terminal_speed_is_two_decimal_numbers() {
        let mut t = negotiating();
        run(&mut t, &[IAC, DO, OPT_TERMSPEED]);
        let (_, _, reply) = run(&mut t, &[IAC, SB, OPT_TERMSPEED, 1, IAC, SE]);
        let mut want = vec![IAC, SB, OPT_TERMSPEED, 0];
        want.extend_from_slice(b"38400,38400");
        want.extend_from_slice(&[IAC, SE]);
        assert_eq!(reply, want);
    }

    #[test]
    fn the_window_size_goes_out_as_soon_as_naws_is_agreed() {
        let mut t = negotiating();
        let (_, _, reply) = run(&mut t, &[IAC, DO, OPT_NAWS]);
        assert_eq!(
            reply,
            vec![IAC, SB, OPT_NAWS, 0, 80, 0, 24, IAC, SE],
            "a server that asked for NAWS wants the size now, not next resize"
        );
    }

    #[test]
    fn a_size_byte_of_255_is_escaped() {
        // A 255-column terminal would otherwise put a bare IAC inside the
        // subnegotiation and end it early.
        let mut t = negotiating();
        t.resize(255, 24);
        let (_, _, reply) = run(&mut t, &[IAC, DO, OPT_NAWS]);
        assert_eq!(reply, vec![IAC, SB, OPT_NAWS, 0, IAC, IAC, 0, 24, IAC, SE]);
    }

    #[test]
    fn a_resize_that_changes_nothing_sends_nothing() {
        // A window being dragged emits a resize per pixel row.
        let mut t = negotiating();
        run(&mut t, &[IAC, DO, OPT_NAWS]);
        t.resize(100, 40);
        assert!(!t.take_reply().is_empty());
        t.resize(100, 40);
        assert!(t.take_reply().is_empty());
    }

    #[test]
    fn a_resize_before_naws_is_agreed_is_not_sent() {
        let mut t = negotiating();
        t.resize(100, 40);
        assert!(t.take_reply().is_empty());
        // ...and the agreed size is the current one, not the startup one.
        let (_, _, reply) = run(&mut t, &[IAC, DO, OPT_NAWS]);
        assert_eq!(reply, vec![IAC, SB, OPT_NAWS, 0, 100, 0, 40, IAC, SE]);
    }

    #[test]
    fn the_server_may_send_naws_at_us() {
        // RFC 1073 says client-to-server; upstream accepts it both ways and
        // acts on it even when NAWS was never agreed (`telnet.c:299`, with
        // the status test commented out). This is how a console server tells
        // a client what the far end really is.
        let mut t = negotiating();
        let (_, events, _) = run(&mut t, &[IAC, SB, OPT_NAWS, 0, 132, 0, 43, IAC, SE]);
        assert_eq!(
            events,
            vec![TelnetEvent::Resize {
                cols: 132,
                rows: 43
            }]
        );
    }

    #[test]
    fn a_doubled_iac_inside_a_subnegotiation_is_data() {
        let mut t = negotiating();
        // 255 columns arriving from the far end, escaped.
        let (_, events, _) = run(&mut t, &[IAC, SB, OPT_NAWS, 0, IAC, IAC, 0, 24, IAC, SE]);
        assert_eq!(
            events,
            vec![TelnetEvent::Resize {
                cols: 255,
                rows: 24
            }]
        );
    }

    #[test]
    fn a_subnegotiation_with_no_parameter_is_dropped() {
        let mut t = negotiating();
        let (_, events, reply) = run(&mut t, &[IAC, SB, OPT_NAWS, IAC, SE, b'x']);
        assert!(events.is_empty());
        assert!(reply.is_empty());
    }

    #[test]
    fn an_unterminated_subnegotiation_does_not_eat_the_rest_of_the_session() {
        // `IAC <not SE>` inside SB abandons it and runs the byte as a command,
        // which is what stops a server that forgets its SE from turning every
        // subsequent byte into subnegotiation payload.
        let mut t = passive();
        let (data, _, reply) = run(&mut t, &[IAC, SB, OPT_NAWS, 0, 80, IAC, DO, OPT_SGA, b'x']);
        assert_eq!(reply, vec![IAC, WILL, OPT_SGA]);
        assert_eq!(data, b"x");
    }

    #[test]
    fn a_break_is_an_event_and_not_data() {
        let mut t = negotiating();
        let (data, events, _) = run(&mut t, &[b'a', IAC, BRK, b'b']);
        assert_eq!(data, b"ab");
        assert_eq!(events, vec![TelnetEvent::Break]);
    }

    #[test]
    fn the_commands_with_nothing_to_say_are_swallowed() {
        // NOP, DM, GA, AYT and the editing commands all return to idle
        // without a reply — `ParseTelIAC`. Upstream does not answer an AYT
        // and neither do we; answering would be inventing behaviour.
        let mut t = negotiating();
        let (data, events, reply) =
            run(&mut t, &[b'a', IAC, NOP, IAC, GA, IAC, AYT, IAC, DM, b'b']);
        assert_eq!(data, b"ab");
        assert!(events.is_empty());
        assert!(reply.is_empty());
    }

    #[test]
    fn outbound_carriage_returns_gain_a_nul_until_binary_is_agreed() {
        let mut t = negotiating();
        let mut out = Vec::new();
        t.encode(b"a\x0db", &mut out);
        assert_eq!(out, b"a\x0d\x00b");

        // And stop once BINARY is on, which is the whole reason to ask for it.
        run(&mut t, &[IAC, DO, OPT_BINARY]);
        out.clear();
        t.encode(b"a\x0db", &mut out);
        assert_eq!(out, b"a\x0db");
    }

    #[test]
    fn outbound_iac_doubles() {
        let t = negotiating();
        let mut out = Vec::new();
        t.encode(&[0xFF, b'x'], &mut out);
        assert_eq!(out, vec![0xFF, 0xFF, b'x']);
    }

    #[test]
    fn raw_mode_escapes_nothing_on_the_way_out_either() {
        let t = Telnet::new(TelnetParams {
            mode: TelnetMode::Raw,
            ..TelnetParams::default()
        });
        let mut out = Vec::new();
        t.encode(&[0xFF, 0x0D], &mut out);
        assert_eq!(out, vec![0xFF, 0x0D]);
    }

    #[test]
    fn binary_agreed_one_way_does_not_affect_the_other() {
        // `TelBinSend` is `MyOpt[BINARY]` and `TelBinRecv` is `HisOpt[BINARY]`.
        // Conflating them makes a half-binary link either double its NULs or
        // eat the far end's.
        let mut t = negotiating();
        run(&mut t, &[IAC, WILL, OPT_BINARY]); // his only
        let mut out = Vec::new();
        t.encode(b"\x0d", &mut out);
        assert_eq!(out, b"\x0d\x00", "our direction is still NVT");
        // ...and his direction stopped treating CR NUL as an escape.
        let (data, _, _) = run(&mut t, b"\x0d\x00");
        assert_eq!(data, b"\x0d\x00");
    }

    // --- the four modes -----------------------------------------------------

    #[test]
    fn the_two_keys_and_the_port_make_four_modes() {
        use TelnetMode::*;
        // The shipped file, at the telnet port and away from it.
        assert_eq!(TelnetMode::of(true, true, true), Negotiate);
        assert_eq!(TelnetMode::of(true, true, false), Framed);
        // `Telnet=off` is not raw — `TelAutoDetect` is a separate key and it
        // ships on, so the framing still arrives with the first `IAC`.
        assert_eq!(TelnetMode::of(false, true, true), Auto);
        assert_eq!(TelnetMode::of(false, false, true), Raw);
        // Auto-detect is dead once the framing is on from byte one.
        assert_eq!(TelnetMode::of(true, false, false), Framed);
    }

    #[test]
    fn framed_swallows_the_nul_after_a_cr_with_no_iac_in_sight() {
        // The whole difference between `Framed` and `Auto`, and the reason
        // `for_port` no longer answers `Auto` for a port that is not 23: a host
        // that has not yet sent an `IAC` is still a host whose `CR NUL` is a
        // line ending rather than two characters.
        let mut framed = Telnet::new(TelnetParams {
            mode: TelnetMode::Framed,
            ..TelnetParams::default()
        });
        assert!(framed.take_reply().is_empty(), "and not a word offered");
        let (data, _, _) = run(&mut framed, b"a\x0d\x00b");
        assert_eq!(data, b"a\x0db");

        let mut auto = Telnet::new(TelnetParams::default());
        let (data, _, _) = run(&mut auto, b"a\x0d\x00b");
        assert_eq!(data, b"a\x0d\x00b", "still plain data");
    }

    // --- TelEcho ------------------------------------------------------------

    /// `TelEcho` on, so the option and the setting are one variable.
    fn echoing(local_echo: bool) -> Telnet {
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            echo_negotiates: true,
            local_echo,
            ..TelnetParams::default()
        });
        t.take_reply();
        t
    }

    #[test]
    fn tel_echo_off_leaves_local_echo_alone() {
        // `ttset.c:1304` ships it off, and then `WILL ECHO` changes nothing
        // locally — the burst asked the server to echo and the answer is the
        // server's business.
        let mut t = negotiating();
        let (_, events, _) = run(&mut t, &[IAC, WILL, OPT_ECHO]);
        assert!(t.server_echoes());
        assert_eq!(events, vec![], "no LocalEcho without TelEcho");
    }

    #[test]
    fn tel_echo_on_makes_the_negotiation_decide() {
        let mut t = echoing(false);
        let (_, events, _) = run(&mut t, &[IAC, WILL, OPT_ECHO]);
        assert_eq!(events, vec![TelnetEvent::LocalEcho(false)], "he echoes");
        let (_, events, _) = run(&mut t, &[IAC, WONT, OPT_ECHO]);
        assert_eq!(events, vec![TelnetEvent::LocalEcho(true)], "so we must");
    }

    #[test]
    fn tel_echo_reverses_the_bursts_echo_request() {
        // `TelChangeEcho` (`telnet.c:845`): with local echo already on, the
        // opening burst asks the server **not** to echo. Nothing else in the
        // burst moves, so this is one byte's difference in five records and
        // the opposite request.
        let ask = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            echo_negotiates: true,
            local_echo: false,
            ..TelnetParams::default()
        })
        .take_reply();
        assert!(window(&ask, &[IAC, DO, OPT_ECHO]));

        let refuse = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            echo_negotiates: true,
            local_echo: true,
            ..TelnetParams::default()
        })
        .take_reply();
        assert!(!window(&refuse, &[IAC, DO, OPT_ECHO]));
        // ...and `DONT` on an option that was never `Yes` is silent, which is
        // the Q method: `TelDisableHisOpt`'s only arm that sends is `Yes`.
        assert!(!window(&refuse, &[IAC, DONT, OPT_ECHO]));
        assert_eq!(refuse.len(), ask.len() - 3, "exactly the ECHO record short");
    }

    fn window(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // --- TelLog -------------------------------------------------------------

    #[test]
    fn the_log_records_what_was_sent_and_nothing_that_arrived() {
        // Every `TelWriteLog` call sits after a `CommRawOut`, so the file is
        // one half of the conversation — a `>` per record and no inbound line
        // to pair it with. Reading it as a negotiation trace is the mistake it
        // invites.
        let mut t = Telnet::new(TelnetParams {
            mode: TelnetMode::Negotiate,
            log: Some(PathBuf::from("TELNET.LOG")),
            ..TelnetParams::default()
        });
        t.take_reply();
        let burst = t.take_log();
        assert!(burst.starts_with("\r\n> FF FB 18"), "{burst:?}");
        assert_eq!(burst.matches(">").count(), 6, "six records in the burst");

        // A `DO` we refuse logs the `WONT` that goes back and not the `DO`.
        run(&mut t, &[IAC, DO, OPT_ECHO]);
        assert_eq!(t.take_log(), "\r\n> FF FC 01");
        // A `WILL` that completes an option we already asked for sends
        // nothing, so it logs nothing either.
        run(&mut t, &[IAC, WILL, OPT_SGA]);
        assert_eq!(t.take_log(), "");
    }

    #[test]
    fn the_log_is_empty_unless_it_was_asked_for() {
        let mut t = negotiating();
        run(&mut t, &[IAC, DO, OPT_ECHO]);
        assert_eq!(t.take_log(), "");
    }
}
