# tt-conn

The connection layer. **Serial first**, because that is the differentiator:
`minicom` and `picocom` have no GUI and no scripting, `cutecom` and `moserial`
are toys, PuTTY has serial but neither scripting nor file transfer, and the one
tool that covers this ground — SecureCRT — is closed and paid. SSH (`russh`),
telnet and a local pty (`portable-pty`) follow. **All four transports are
built**; see the sections below for each one's decisions.

```sh
cargo test -p tt-conn                    # unit tests, plus the whole pty suite
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-conn -- --test-threads=1              # and the hardware ones

cd ../../ssh-audit && ./servers.sh start                 # :2222 sshd, :2223 dropbear
D=$XDG_RUNTIME_DIR/sterna-ssh-audit
TT_SSH_HOST=127.0.0.1 TT_SSH_PORT=2222 TT_SSH_USER=$USER TT_SSH_KEY=$D/id_ed25519 \
  TT_SSH_PW_USER=sterna-test TT_SSH_PASS=spike5-not-a-secret \
  cargo test -p tt-conn --test ssh -- --test-threads=1   # and the SSH ones
cd ../../ssh-audit && ./servers.sh stop                  # removes the account
```

`--test-threads=1` for the SSH ones too, but for a different reason: nothing is
shared, the *server* simply refuses to be hammered. OpenSSH's `MaxStartups`
defaults to `10:30:100` and dropbear's ceiling is lower, so running them in
parallel produces a scatter of unrelated failures that all pass on their own.

The pty suite is the exception to all of that on POSIX: it needs no rig, no
server and no variables, so `cargo test -p tt-conn` runs all nineteen cases
there. Windows has a separate `pty_windows` suite against `cmd.exe`; `/bin/sh`,
`poll(2)` and signals are not portable tests. It must run on native Windows:
Wine 9's console host rejects ConPTY's `--inheritcursor` mode before a child
can produce output.

Without those two variables the hardware tests **skip loudly** rather than pass
quietly, so a machine with no rig still gets a green `cargo test` without
pretending the serial layer was exercised. They need two ports wired
back-to-back on data *and* control lines (TX↔RX, DTR↔DSR, RTS↔CTS) — the dev
container has an FTDI Quad RS232-HS looped exactly that way. `--test-threads=1`
because there is one rig and the tests take turns on it.

## Built against `commlib.c`, not against an idea of a serial port

Every setting here exists because Tera Term's `teraterm/teraterm/commlib.c` sets
it — the DCB fields it fills in, the `EscapeCommFunction` calls it makes. That
is why MARK/SPACE parity, DSR flow control and break *detection* are here at
all; none of them is something you would think to build otherwise, and all
three are why people still keep a Windows VM for console work.

| `commlib.c` | Here | On Linux |
|---|---|---|
| `dcb.BaudRate` | `SerialParams::baud` | exact, 300 → 3000000, non-standard rates included |
| `dcb.Parity` incl. MARK/SPACE | `Parity` | needs `CMSPAR` on the raw fd; `serialport-rs` has no enum for it |
| `dcb.ByteSize`, `StopBits` | `DataBits`, `StopBits` | 7 and 8 always; 5 and 6 are adapter-dependent, see below |
| `fOutxCtsFlow` | `FlowControl::RtsCts` | `CRTSCTS` |
| `fOutxDsrFlow` | `FlowControl::DsrDtr` | **no kernel support at all** — emulated in `write` |
| `fOutX`/`fInX`, `XonChar`/`XoffChar` | `FlowControl::XonXoff`, `xon`/`xoff` | `IXON`/`IXOFF` + `VSTART`/`VSTOP` |
| `XonLim` 768 / `XoffLim` 3328 | — | **not expressible**; the kernel owns its watermarks |
| `fDtrControl`, `fRtsControl` | `PinControl` | `Handshake` only for RTS, only as part of `CRTSCTS` |
| `SetCommBreak`/`ClearCommBreak` | `send_break(dur)` | latched, not `tcsendbreak`'s fixed quarter-second |
| `GetCommModemStatus` | `modem_lines()` | one `TIOCMGET` |
| `CommLock` | `lock()` | XOFF byte, or drop RTS/DTR per the flow mode |
| `PurgeComm` | `clear()` | |
| close drops DTR | `Drop` | how a modem is told to hang up |

On Windows those rows are one DCB rather than a chain of portable setters.
That matters for MARK/SPACE, `fOutxDsrFlow`, independent DTR/RTS control and
the XON/XOFF characters and thresholds, none of which the crate API can fully
name. The DCB is applied once and read back field by field; a driver which
accepts the call but silently keeps a different data size or line mode is an
error rather than a successful-looking wrong port.

The handle is opened directly too. `serialport-rs` merges Windows' missing and
exclusive-use failures into `NoDevice` and discards `GetLastError`, after which
no locale-independent classifier is possible. Keeping the code at
`CreateFileW` lets a second window say “COM3 is in use” instead of telling the
user that their adapter was unplugged.

## The four things that are not obvious

**A break is not a NUL, and by default Linux says it is.** With default termios
a line break arrives as a single `0x00`, indistinguishable from a device
sending one. `PARMRK` escapes the input stream instead — a break becomes
`FF 00 00`, a real `FF` becomes `FF FF` — and `serial::parmrk` decodes it back.
Undoing the escaping matters as much as detecting the break: a file transfer
over a port with doubled `FF` bytes would corrupt every one of them.

**DSR flow control has no kernel bit.** Not a `serialport-rs` gap — Linux
termios has `CRTSCTS` and `IXON`/`IXOFF` and nothing for DSR. `write` polls DSR
and gates the output in 64-byte chunks, and returns short on a deadline rather
than blocking, because the alternative is a frozen UI whenever a device
deasserts the line.

**`flush` takes a timeout because the obvious implementation hangs.** `tcdrain`
waits for the output queue to empty, and flow control can hold that off
indefinitely — drop CTS on the far end and a flush never returns. That is not a
rare state; it is what backpressure looks like. The queue depth is polled
(`TIOCOUTQ`) instead, and the caller decides how long to care.

**`/dev/ttyUSB<n>` is not an identity.** It is assigned in attach order, so
unplugging two adapters and replugging them the other way round swaps their
names. The USB serial number is not the answer either: the FTDI Quad reports
`serial = None` for every port, and even when there is one it names the
*adapter*, not which of its four ports you meant. `PortInfo::open_path()`
returns a `/dev/serial/by-path/…` name, which encodes the USB topology plus the
interface number — so a socket on a hub keeps its name across a replug, and
across swapping in an identical adapter.

## Two places `serialport-rs` says something it does not mean

Both are wrapped in exactly one place, in `error.rs`, so there is one thing to
fix if the crate changes.

- **A disconnect arrives as `BrokenPipe` with `raw_os_error() == None`**, not
  the `EIO`/`ENXIO` the kernel returns. Found by spike 4.
- **A *busy* port arrives as `ErrorKind::NoDevice`**, message "Device or
  resource busy", no errno. Mapping that straight through tells someone with
  `minicom` open in another window that their adapter was unplugged, and sends
  them off to check the cable — for the single most common serial failure there
  is. `Error::from_open` separates the two by asking whether the device node
  still exists, rather than by matching the message text, which the crate is
  free to reword.

## `tcsetattr` succeeding does not mean the driver did it

Measured on the FTDI Quad: `CS6` is refused with `EINVAL`, which is fine — and
**`CS5` is accepted and then ignored**, with the adapter still transmitting
eight bits. `tcsetattr` returns success if it could apply *any* part of the
request, so its return value proves nothing on its own.
`linux::set_data_bits` therefore reads the setting back and reports
`Unsupported` when it did not take. Without that the settings dialog would say
five data bits while the wire carried eight, and the corruption would look like
a cabling fault.

This also corrects a claim in `PLAN.md`'s spike 4 result: "5–8 data bits" came
from the `serialport-rs` *enum* covering four values, not from any of them
reaching the wire. Seven does — `seven_data_bits_reach_the_wire` proves it by
transmitting at seven and receiving at eight, where the stop bit lands in bit 7
and turns `0x25` into `0xA5`. **Do not pick a probe byte with bit 7 set for
that test**: `0xA5` sent at seven bits also reads back as `0xA5`, and the test
then passes whatever the port is doing. That cost a wrong conclusion here
before it was noticed.

## Why the type is concrete

`SerialConn` holds a `NativePort` — `TTYPort` on unix, `COMPort` on Windows —
not a `Box<dyn SerialPort>`. The raw-fd patch layer needs `AsRawFd` and the
trait object does not provide it, so the split is at the type level whether or
not the API admits it. Spike 4's conclusion was to make it explicit and thin
rather than to pretend the portable trait suffices; better that than finding
out at the point where MARK parity has to work.

The crate cross-compiles for `x86_64-pc-windows-gnu` today and CI checks it on
a real Windows runner, so the Linux-only parts stay behind `cfg` rather than
accumulating until Stage 3. What is *behind* those `cfg`s on Windows is mostly
unwritten: `fOutxDsrFlow` is native there and inverts this whole design, and
that is Stage 3's problem.

## Telnet

Third transport, and after serial the one that matters most: a terminal server
puts one TCP port on each serial line, so reaching those ports is the same job
as reaching the cable.

That shapes the modes, and `TelnetMode::Raw` is a **first-class choice rather
than a degraded one**. An `0xFF` in a firmware upload is data; a client that
eats it corrupts the transfer. `Auto` is upstream's `TelAutoDetect` — raw until
the first `IAC`. `Negotiate` opens with upstream's burst, and upstream sends it
only when the port is 23 (`vtwin.cpp:3666`), which is not an oversight: opening
at a console server with `WILL TERMINAL-TYPE` puts five bytes of protocol into
somebody's serial console.

The protocol is in `telnet/protocol.rs` with no socket in it, so the parts that
break — option negotiation, IAC framing, a command split across two reads — are
tested against byte strings. **The framing and the negotiation are two files
upstream and the framing runs first**: `ttcmn.c` unescapes `IAC IAC` and
swallows the `NUL` after a `CR` before `telnet.c` sees anything. Reading
`telnet.c` alone gives a parser that doubles every `0xFF`.

Two things upstream has are absent, and both are **opt-in settings there too**,
so neither is a default behaviour difference: local echo (`ts.TelEcho`, off)
and LINEMODE (`ts.EnableLineMode`, off).

Those unit tests prove the port matches upstream's C and nothing about whether
upstream matches the world. `telnet-audit/` is what closes that: GNU inetutils'
`telnetd` behind a fifteen-line inetd, plus a raw echo. It opens with four
options above upstream's `MaxTelOpt`, so the refusal path runs first in every
session.

## SSH

`russh` on a worker thread, behind the same synchronous `Transport` a serial
port presents. Two shapes had to be decided here, and `PLAN.md` deferred both
until there was a second transport to decide them against.

**Async lives inside `tt-conn`, not above it.** The terminal core, the C ABI and
the Qt shell are all synchronous, and a terminal wants nothing from a connection
but bytes. So the tokio runtime is private to `ssh/conn.rs`: one thread and one
current-thread runtime. Unix adds a self-pipe (`ssh/wakeup.rs`) so a frontend
can wait on SSH exactly the way it waits on a serial port. Windows uses an
owned manual-reset event instead; `QWinEventNotifier` waits on the borrowed
handle, and the same event spans connection setup and the running transport.
The alternative — an async shell — would spread `russh`'s runtime through
three layers with no use for it.

**Connecting is a state machine the caller drives.** `SshConnect::poll` returns
the question — host key, password, keyboard-interactive challenge, key
passphrase — and the worker waits for an answer. A callback would have to be
`Send`, would run on the worker thread, and would leave a Qt frontend trying to
raise a modal dialog from the wrong one.

Authentication follows the server's `remaining_methods` rather than a fixed
list: agent, then key files, then what has to be typed. A device that only does
`keyboard-interactive` is never asked for a password it will reject.
The agent is `SSH_AUTH_SOCK` on Unix and Pageant on Windows, both through
russh's own transports; an absent or broken agent falls through to key files.

`SshParams::legacy` is spike 5's first finding as a switch. russh keeps SHA-1
key exchange, CBC ciphers and `ssh-rsa` host keys out of its default preference
list — correct posture, and the reason a console server from 2012 will not
answer. It *widens* the offer rather than replacing it, because spike 5's second
finding was that embedded servers are narrow in different directions from each
other.

### `known_hosts` and `ssh_config` are written here, not adopted

Both existing implementations get `known_hosts` wrong in ways that are invisible
until they matter, and both fail in the same direction — reporting what an
untouched file reports, *unknown host*:

- **`russh::keys::known_hosts` splits the line on one space and reads the second
  field as the key type**, so an `@revoked` entry parses as a host named
  `@revoked` and matches nothing. A key the user explicitly revoked comes back
  as unknown and the prompt offers to accept it. No wildcards either.
- **Tera Term's `hosts.c` has the wildcards and the negation** but no hashed
  entries at all — `|1|` appears nowhere in it — which on Debian and Ubuntu is
  every line in the file. Its matcher is also case-sensitive.

`ssh_config` has no adoptable reader at all, and one trap of its own: **the
first value wins, not the last.** Nearly every other config format does the
opposite, and getting it backwards does not fail loudly — it silently applies
the wrong user or key to hosts that had a perfectly good specific block.
`IdentityFile` is the exception and accumulates.

`Match exec` never matches, deliberately. Resolving a config would otherwise run
an arbitrary shell command every time a connect dialog enumerates hosts.
Keywords that are not acted on are *reported* through `Resolved::unsupported`
rather than dropped, because a silently ignored `ProxyJump` is a connection to
the wrong machine.

### What is not here

- **A line break.** RFC 4335 defines a `break` channel request and `russh` does
  not implement it, so `send_break` returns `Unsupported` and `supports_break`
  is false. Returning `Ok(())` would be worse: on a console server reached over
  SSH a break is a real function, and silently not sending one looks like the
  far end ignoring it.
- Port forwarding, agent forwarding, X11, `ProxyJump`, certificates, and SSH-1
  — the last permanently, per `PLAN.md`.

## The local pty

Fourth transport, and the one upstream reaches by *not* being a terminal for it.
`cygwin/cygterm` is a separate program that forks a shell onto a pty and bridges
it back to Tera Term over a **loopback telnet socket** — `cygterm.cpp:1083`
onward implements ECHO, SGA, TERMINAL-TYPE and NAWS by hand. That existed
because a Windows program cannot fork. Here a pty is a transport like any other
and the detour is deleted.

Two of upstream's decisions survive it, because they are about how a shell
should *start* rather than about Win32:

- **A login shell by default** — `cygterm.cfg`'s `LOGIN_SHELL = Yes`,
  implemented at `cygterm.cpp:988` by rewriting `argv[0]` to `-bash`. It is what
  makes `~/.profile` run.
- **`TERM` set explicitly.** The value does not survive: upstream says `vt100`,
  we say `xterm-256color`, because that is a claim about the engine behind it
  and ours does 256 colour, truecolor and xterm mouse tracking. Underclaiming
  costs the user `ls --color` and a mouse that does nothing in `vim`.

Three environment variables are corrected on the way in, each wrong by default
in a way that is invisible until it isn't. `TERM` is **never inherited** — a
window launched from another terminal would otherwise hand the shell *that*
terminal's name, and one launched from a desktop menu would hand it nothing at
all. `COLORTERM` says truecolor. `LINES` and `COLUMNS` are *removed*: they are a
snapshot, the pty's `winsize` is the truth, and a stale pair inherited from a
differently sized parent survives every resize.

### The two traps, both of which fail as silence

**The slave end has to be dropped, and immediately.** We hold one end of the pty
and the child holds the other; keeping ours open means the master never sees the
hangup when the child exits. The shell dies and the window sits there forever
waiting for output from nobody — no error, no data, nothing to debug.

**`portable-pty`'s own reader maps `EIO` to `Ok(0)`**, so that `read_to_string`
terminates. `EIO` is how a pty master reports that the last slave closed, i.e.
that the child is gone, and `Ok(0)` is already this crate's word for "the line is
quiet" — the state a terminal spends nearly all its time in. Taking that mapping
collapses the two: the window never learns the shell exited, and because a
hung-up descriptor is *permanently* readable, the frontend's `QSocketNotifier`
fires forever against a read that never returns anything. **A dead shell would
present as a terminal at 100% CPU.** So the byte-level read and write are ours,
straight on the master's descriptor, and `EIO` means disconnected.

**ConPTY's anonymous pipes are synchronous.** `portable-pty` creates them with
`CreatePipe`, so a `ReadFile` or `WriteFile` on the frontend thread can block
for as long as the child does. Windows gives each direction a worker instead.
The reader feeds a bounded 128×8 KiB queue; when a dialog stops the frontend,
the queue fills and backpressure returns to ConPTY rather than memory growing
without a ceiling. A manual-reset event wakes `QWinEventNotifier`, and EOF is
queued behind the final bytes and re-signalled if both coalesced into one
wakeup. The writer has a small bounded queue and reports full as a short write,
which lets the session's existing pending-output timer retry it.

`portable-pty` also requests `PSEUDOCONSOLE_INHERIT_CURSOR`. ConPTY begins by
asking the terminal for `CSI 6 n` and waits for the cursor report; a raw test
has to answer it, while `Session`'s VT engine already does. Wine 9 launches its
console host with that mode but the host rejects the internal
`--inheritcursor` switch and closes the pipe empty. The worker/event unit test
runs there; the real `cmd.exe` cases remain native-Windows tests.

### What it adds to the seam

`Transport::closing_note` — asked once, after a disconnect and before the
transport is dropped, because a pty's exit status dies with the child handle.
Every other transport returns `None`: an unplugged adapter and a closed socket
are what they look like. A local shell is not, and "bash exited with status 1"
is the message that says whether anything went wrong.

`Drop` hangs up and then collects. Closing the master is what a terminal window
closing *means* — the kernel sends `SIGHUP` to the foreground process group —
and only then is there anything to reap. Reaping matters: `std::process::Child`
does not do it on drop, so a session that opens and closes local shells all day
would leave one zombie per shell. Both waits are bounded, and a shell that
ignores `SIGHUP` gets `SIGKILL`.

`supports_break()` is false. A pty has no line to break; the thing a break
stands in for is `SIGINT`, which arrives as `Ctrl+C` through the line discipline
like any other keystroke.

### The cost of adopting `portable-pty`

It drags in **`serial2`, a second serial-port crate**, unconditionally and with
no feature to switch it off — the crate carries a "serial port as a pty" mode
nothing here uses. Twenty-seven packages in total. Accepted rather than fixed,
because what it supplies is the part that is genuinely hard and genuinely
platform-specific: the `setsid`/`TIOCSCTTY` dance in the forked child, and
ConPTY. The Windows backend now constructs and resizes and has a byte reader,
writer and frontend event around its synchronous pipes. A native Windows run
is still required before the `cmd.exe` path is called proven. Writing either
backend again to save a dependency would be the wrong trade in the direction
the project keeps choosing against.
