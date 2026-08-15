# serial-audit — Stage 0 spike 4

Does `serialport-rs` cover what a Tera Term successor needs from a serial port,
and where it doesn't, is a raw-fd patch enough or do we need our own layer?

**Answer: adopt it, plus a thin patch layer.** Full findings are in `docs/history.md`
under "Spike 4 result". This crate is what produced them, and it stays as the
regression test for the patch layer once `tt-conn` exists.

The audit is written against the requirement in Tera Term's
`teraterm/teraterm/commlib.c` — the DCB fields it actually sets, the
`EscapeCommFunction` calls it actually makes — rather than a generic wishlist.
That is the whole reason it found the gaps it did: `fOutxDsrFlow` and
MARK/SPACE parity are not things you would think to test for otherwise.

## Hardware it needs

An FTDI Quad RS232-HS with **`/dev/ttyUSB0` and `/dev/ttyUSB1` wired
back-to-back on data *and* control lines** (TX↔RX, DTR↔DSR, RTS↔CTS). Without
the control-line wiring the modem-line and flow-control tests are meaningless
rather than failing, so check the loom before believing a red result.

Port paths are hardcoded as `A`/`B` consts at the top of each binary.

## Running

```sh
cargo run --bin serial-audit   # capability audit vs commlib.c
cargo run --bin rawpatch       # can the gaps be patched through the raw fd?
cargo run --bin hotplug [secs] # needs a human to pull the cable
```

`hotplug` prompts, then waits: unplug the adapter, wait ~5 s, plug it back in.
It exits early once removal, re-attach and the open-port error have all been
seen.

Needs `libudev-dev` (or `systemd-devel` on Fedora) for enumeration.

## What each one establishes

- **`serial-audit`** — enumeration and USB metadata, the line settings
  `commlib.c` sets, baud rates 300→3000000 verified by driver readback, modem
  lines cross-checked over the loopback, latched break, both flow-control
  modes, timeout semantics.
- **`rawpatch`** — the decisive one. Whether `CMSPAR` (MARK/SPACE parity),
  `PARMRK` (telling an incoming break from a real `0x00`) and `VSTART`/`VSTOP`
  can be set through `AsRawFd`, **and whether they survive subsequent
  `serialport-rs` calls**. They do — which is what makes a patch layer viable
  instead of a fork.
- **`hotplug`** — removal and re-attach via re-enumeration, and what an
  already-open port does when the device is yanked (`ErrorKind::BrokenPipe`,
  with `raw_os_error()` unhelpfully `None`).

## Known non-findings

Two gaps are *not* `serialport-rs`'s fault and no patch layer fixes them:

- **DSR/DTR flow control** (`commlib.c:219`, `fOutxDsrFlow`) — Linux termios has
  no DSR flow-control bit at all. Must be emulated in userspace: poll DSR, gate
  writes.
- **XON/XOFF thresholds** (`XonLim`/`XoffLim`) — the kernel owns its buffer
  watermarks. The *characters* are settable; the limits are not.
