//! What the protocols read before they start, and where each default comes
//! from.
//!
//! Every value here is `ttpset/ttset.c`'s, cited by line. That discipline is
//! not decoration: `AGENTS.md` lists five settings whose real default is an
//! `else` branch or a flag word built up hundreds of lines from where it is
//! declared, and a transfer configured from the wrong ones is a transfer that
//! times out on its first packet for a reason nobody would think to look for.
//! Zeroing `TTTSet` instead — the obvious thing — sets every timeout to zero.

use std::path::PathBuf;
use std::time::Duration;

/// What the connection underneath is, which the protocols branch on.
///
/// Not cosmetic and not derivable: `xmodem.c:347`, `zmodem.c:788` and
/// `ymodem.c:417` each pick a different timeout set from it, zmodem caps its
/// block size at 1024 on a network link and scales it off the baud rate
/// otherwise, and `kermit.c:1213` uses it to decide whether the 8th bit needs
/// quoting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// A real serial port. `baud` feeds zmodem's block-size ladder and
    /// `seven_bit` makes kermit quote the high bit.
    Serial { baud: u32, seven_bit: bool },
    /// Telnet or SSH — anything where the layer below already guarantees
    /// delivery. Takes the TCP timeout branch, which by default means zmodem
    /// does not time out at all and leaves detecting a dead peer to the
    /// socket.
    Network,
}

impl Link {
    /// A local pty, which is neither.
    ///
    /// It reports as serial deliberately. The link is reliable, so `Network`
    /// looks right — but `Network` also selects `ZmodemTimeOutTCPIP`, whose
    /// default is 0 meaning *never time out*, and a local `sz` that dies
    /// mid-transfer would then hang the transfer for ever with nothing to
    /// notice it. The nominal baud only picks a block size, and 115200 picks
    /// the largest one. This is also the branch `xfer/`'s interop suite
    /// actually exercised, over exactly this kind of pty.
    pub fn local_pty() -> Link {
        Link::Serial {
            baud: 115_200,
            seven_bit: false,
        }
    }

    pub(crate) fn port_type(self) -> i32 {
        match self {
            // IdSerial / IdTCPIP, `tttypes.h`.
            Link::Serial { .. } => 1,
            Link::Network => 2,
        }
    }
}

/// The per-protocol timeouts, in seconds.
///
/// Upstream stores each set as one comma-separated INI value and floors every
/// field at 1 on read (`ttset.c:1821` onward) — except `ZmodemTimeOutTCPIP`,
/// which floors at 0 because 0 is meaningful there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// `XmodemTimeOut` fields 1-5, `ttset.c:1821`.
    pub xmodem: [i32; 5],
    /// `YmodemTimeOut` fields 1-5, `ttset.c:1839`.
    pub ymodem: [i32; 5],
    /// `ZmodemTimeOut`: normal, **tcpip**, init, fin — `ttset.c:1857`.
    /// The second is 0 by default, and 0 means no timeout at all.
    pub zmodem: [i32; 4],
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts {
            // init, initCRC, short, long, vlong
            xmodem: [10, 3, 10, 20, 60],
            ymodem: [10, 3, 10, 20, 60],
            // normal, tcpip, init, fin
            zmodem: [10, 0, 10, 3],
        }
    }
}

/// Which protocols write a transfer log. Upstream's `LogFlag` bits
/// (`tttypes.h:120`), every one of them `GetOnOff(..., FALSE)` — so in an INI
/// only the literal `on` switches one on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogFlags {
    pub kermit: bool,
    pub xmodem: bool,
    pub zmodem: bool,
    pub bplus: bool,
    pub quickvan: bool,
    pub ymodem: bool,
}

impl LogFlags {
    pub(crate) fn bits(self) -> i32 {
        (self.kermit as i32) << 1
            | (self.xmodem as i32) << 2
            | (self.zmodem as i32) << 3
            | (self.bplus as i32) << 4
            | (self.quickvan as i32) << 5
            | (self.ymodem as i32) << 6
    }
}

/// `ts.FTFlag` (`tttypes.h:129`), minus the two auto-activation bits.
///
/// `FT_ZAUTO` and `FT_BPAUTO` are not here on purpose: they are the
/// *terminal's* business — whether the VT engine watches the stream for a
/// zmodem trigger and starts a transfer — and not a running transfer's. What
/// they turn into once a transfer does start is [`Job::ZModem { auto: true }`]
/// (crate::Job), which is a different thing said in the right place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quirks {
    /// `ZmodemEscCtl` — escape control characters, for a link that eats them.
    pub zmodem_escape_ctl: bool,
    /// `BPEscCtl`, the same for B-Plus.
    pub bplus_escape_ctl: bool,
    /// `AutoFileRename`: on receive, never overwrite — add `.1`, `.2`.
    pub auto_rename: bool,
}

impl Quirks {
    pub(crate) fn bits(self) -> i32 {
        (self.zmodem_escape_ctl as i32)
            | (self.bplus_escape_ctl as i32) << 2
            | (self.auto_rename as i32) << 4
    }
}

/// Everything a transfer is configured with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub link: Link,
    pub timeouts: Timeouts,
    /// `ZmodemDataLen`, `ttset.c:1400`. The subpacket size when sending;
    /// `zmodem.c:780` floors it at 64.
    pub zmodem_data_len: i32,
    /// `ZmodemWinSize`, `ttset.c:1403`.
    pub zmodem_win_size: i32,
    /// `QVWinSize`, `ttset.c:1270`.
    pub quickvan_win_size: i32,
    /// `KmtLongPacket`, `ttset.c:1207`.
    pub kermit_long_packet: bool,
    /// `KmtFileAttr`, `ttset.c:1209`.
    pub kermit_file_attr: bool,
    pub quirks: Quirks,
    pub log: LogFlags,
    /// Where a protocol log goes. `None` is the working directory.
    pub log_dir: Option<PathBuf>,
    /// Whether a received file may replace one that is already there.
    ///
    /// Upstream's receive dialog defaults this on and offers a checkbox;
    /// [`Quirks::auto_rename`] is the setting that makes the answer moot.
    pub overwrite: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            link: Link::Serial {
                baud: 115_200,
                seven_bit: false,
            },
            timeouts: Timeouts::default(),
            zmodem_data_len: 1024,
            zmodem_win_size: 32767,
            quickvan_win_size: 8,
            kermit_long_packet: false,
            kermit_file_attr: false,
            quirks: Quirks::default(),
            log: LogFlags::default(),
            log_dir: None,
            overwrite: true,
        }
    }
}

/// XMODEM's block format. `xmodem.h`.
///
/// **The default is checksum**, which is upstream's and is not the obvious
/// choice: `ttset.c:1039` reads `XmodemOpt` with an *empty* default and tests
/// it against `crc`, `1k` and `1ksum`, so anything else — including the
/// `checksum` its own writer emits — falls to the `else` arm. Picking CRC here
/// because it is the better format would give a `Job` built from
/// `Default::default()` a different block size from one built from an
/// untouched `TERATERM.INI`, which is the kind of disagreement nobody finds
/// until a peer that only speaks checksum refuses the transfer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XmodemOpt {
    /// 128-byte blocks with an 8-bit checksum: the original, and the only
    /// thing some very old peers understand.
    #[default]
    Checksum,
    /// 128-byte blocks with a CRC. What a receiver asks for by sending `C`.
    Crc,
    /// 1K blocks with a CRC.
    Crc1K,
    /// 1K blocks with a checksum.
    Checksum1K,
}

impl XmodemOpt {
    pub(crate) fn value(self) -> i32 {
        match self {
            XmodemOpt::Checksum => 1,
            XmodemOpt::Crc => 2,
            XmodemOpt::Crc1K => 3,
            XmodemOpt::Checksum1K => 4,
        }
    }
}

/// YMODEM's block format. `ymodem.h`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YmodemOpt {
    /// 1K blocks. `filesys_proto.cpp:1409` hardcodes this, and it is the only
    /// value the sender's packet builder has a case for — leave it unset and
    /// `YSendPacket` falls through its switch to `assert(0)`.
    #[default]
    K1,
    /// YMODEM-g: streaming, no per-block ACK. Needs an error-free link.
    G,
    /// One file only, no batch terminator.
    Single,
}

impl YmodemOpt {
    pub(crate) fn value(self) -> i32 {
        match self {
            YmodemOpt::K1 => 1,
            YmodemOpt::G => 2,
            YmodemOpt::Single => 3,
        }
    }
}

/// Which side of a Kermit conversation to be. `kermit.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KermitMode {
    /// Wait for the peer to send.
    Receive,
    /// Ask the peer for a file by name (`GET`).
    Get,
    Send,
    /// Tell a remote server to exit server mode.
    Finish,
}

impl KermitMode {
    pub(crate) fn value(self) -> i32 {
        match self {
            KermitMode::Receive => 1,
            KermitMode::Get => 2,
            KermitMode::Send => 3,
            KermitMode::Finish => 4,
        }
    }
}

/// Which direction, for the protocols that only have two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Send,
    Receive,
}

/// A transfer to run.
///
/// One variant per protocol rather than a protocol plus a bag of options,
/// because the options are not interchangeable: XMODEM has a text flag and no
/// filename, Kermit has four modes and no direction, Raw has neither and a
/// stop timer instead. Flattening them into one struct produces fields that
/// are meaningless five-sixths of the time, which is how `YMODEM_OPT` ends up
/// unset and asserting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    XModem {
        dir: Direction,
        opt: XmodemOpt,
        /// Text mode: CRLF translation, and the file is padded to a block
        /// boundary with `^Z` rather than NUL.
        text: bool,
    },
    YModem {
        dir: Direction,
        opt: YmodemOpt,
    },
    ZModem {
        dir: Direction,
        /// Binary: no end-of-line translation. Almost always what is wanted.
        binary: bool,
        /// The peer's `ZRQINIT` has already gone past in the terminal stream
        /// and was swallowed by it. `ZInit` pushes `ZPAD ZDLE B 0 0` back into
        /// the receive buffer so the protocol sees its own trigger.
        auto: bool,
    },
    Kermit {
        mode: KermitMode,
    },
    BPlus {
        dir: Direction,
        auto: bool,
    },
    QuickVan {
        dir: Direction,
    },
    /// Not a protocol: read the connection into a file until it goes quiet.
    /// Receive only — upstream has no raw send here, and says so in `raw.h`.
    Raw {
        autostop: Duration,
    },
}

impl Job {
    pub(crate) fn protocol(self) -> i32 {
        match self {
            Job::XModem { .. } => 0,
            Job::YModem { .. } => 1,
            Job::ZModem { .. } => 2,
            Job::Kermit { .. } => 3,
            Job::BPlus { .. } => 4,
            Job::QuickVan { .. } => 5,
            Job::Raw { .. } => 6,
        }
    }

    /// Whether files leave this machine. Kermit's `Get` and `Finish` are
    /// neither send nor receive in the usual sense; `Get` ends with a file
    /// arriving, so it counts as a receive.
    pub fn direction(self) -> Direction {
        match self {
            Job::XModem { dir, .. }
            | Job::YModem { dir, .. }
            | Job::ZModem { dir, .. }
            | Job::BPlus { dir, .. }
            | Job::QuickVan { dir } => dir,
            Job::Kermit { mode } => match mode {
                KermitMode::Send => Direction::Send,
                _ => Direction::Receive,
            },
            Job::Raw { .. } => Direction::Receive,
        }
    }

    /// `SetOpt`'s mode word.
    ///
    /// **Kermit's is a `KMT_MODE_T`, not the `OpId_t` that goes into
    /// `fv->OpId`**, and the two enums overlap misleadingly: `OpKmtRcv == 3 ==
    /// IdKmtSend`, so passing the wrong one asks Kermit to send when you meant
    /// receive and produces a silent stall with bytes flowing in both
    /// directions.
    pub(crate) fn mode(self) -> i32 {
        match self {
            Job::XModem { dir, .. } | Job::YModem { dir, .. } => match dir {
                Direction::Receive => 1,
                Direction::Send => 2,
            },
            Job::ZModem { dir, auto, .. } => match (dir, auto) {
                (Direction::Receive, false) => 1,
                (Direction::Send, false) => 2,
                (Direction::Receive, true) => 3,
                (Direction::Send, true) => 4,
            },
            Job::Kermit { mode } => mode.value(),
            Job::BPlus { dir, auto } => match (dir, auto) {
                _ if auto => 3,
                (Direction::Receive, _) => 1,
                (Direction::Send, _) => 2,
            },
            Job::QuickVan { dir } => match dir {
                Direction::Receive => 1,
                Direction::Send => 2,
            },
            Job::Raw { .. } => 0,
        }
    }

    /// Whether a *receive* has to be told a name, because this job's does not
    /// come off the wire.
    ///
    /// Three do, for three different reasons — XMODEM carries no filename at
    /// all, `raw.c:80` writes into whatever it is handed, and a Kermit `GET`
    /// is asking the peer for a name rather than being told one. All three
    /// read it through `GetNextFname`, which is why a receive puts it in the
    /// *send* list.
    pub(crate) fn needs_name(self) -> bool {
        matches!(
            self,
            Job::XModem { .. }
                | Job::Raw { .. }
                | Job::Kermit {
                    mode: KermitMode::Get
                }
        )
    }

    /// `SetOpt`'s option word, which every protocol spells differently.
    pub(crate) fn opt(self) -> i32 {
        match self {
            Job::XModem { opt, .. } => opt.value(),
            Job::YModem { opt, .. } => opt.value(),
            Job::ZModem { binary, .. } => binary as i32,
            _ => 0,
        }
    }
}
