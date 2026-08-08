# tt-conn

The connection layer. **Serial first**, because that is the differentiator:
`minicom` and `picocom` have no GUI and no scripting, `cutecom` and `moserial`
are toys, PuTTY has serial but neither scripting nor file transfer, and the one
tool that covers this ground — SecureCRT — is closed and paid. SSH (`russh`),
telnet and a local pty follow.

```sh
cargo test -p tt-conn                                    # unit tests only
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-conn -- --test-threads=1              # and the hardware ones
```

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

`SerialConn` holds a `TTYPort`, not a `Box<dyn SerialPort>`. The raw-fd patch
layer needs `AsRawFd` and the trait object does not provide it, so the split is
at the type level whether or not the API admits it. Better to admit it than to
find out at the point where MARK parity has to work.

## Still to come

Async. `PLAN.md` puts `tokio` under `tt-conn`, and `russh` will require it, but
inventing the async shape before the second transport exists would be guessing.
The seam is the byte-stream API above; a runtime goes behind it.
