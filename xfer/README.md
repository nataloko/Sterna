# termitta xfer — Stage 0 spike 2

Runs Tera Term's **real** file-transfer protocols on Linux, and proves they
interoperate with the reference Unix implementations.

```sh
make && ./run_tests.sh
```
```
=== xfer interop: Tera Term protocol C vs lrzsz / G-Kermit ===
  ok   xmodem-recv    ok   xmodem-send
  ok   ymodem-recv    ok   ymodem-send
  ok   zmodem-recv    ok   zmodem-send
  ok   zmodem-recv-1m ok   zmodem-send-1m
  ok   kermit-recv    ok   kermit-send
10 passed, 0 failed
```

## Why this exists

`PLAN.md` proposes **vendoring** `ttpfile/`'s 9,777 lines of C rather than
rewriting six file-transfer protocols, two of which (B-Plus, Quick-VAN) have no
surviving counterparty anywhere to test a rewrite against. That plan is only
credible if the C actually builds and speaks the protocols off Windows.

It does. **8,409 lines compile unmodified** and every protocol with a
counterparty passes interop in both directions.

## How the protocols attach to a host

`ttpfile` talks to the rest of Tera Term through three vtables and nothing else,
which is why this works at all:

| Seam | Defined in | What we supply |
|---|---|---|
| `TComm` | `filesys_proto.h` | `src/comm_fd.c` — a pty, socket or serial fd |
| `TFileIO` | `filesys_io.h` | `src/fileio_posix.c` — replaces `filesys_win32.cpp` |
| `TFileVarProto` | `filesys_proto.h` | `src/host.c` — services + the `InfoOp` progress vtable |

Beyond those, the protocol sources reference only **six** symbols from outside
`ttpfile`: `SetTimer`, `KillTimer`, `ProtoEnd`, `TTMessageBoxW`, and the CRT's
`_atoi64`/`ctime_s`. That number is the real finding — the protocols are very
nearly free-standing, and this is the shape `tt-xfer` will expose to the Rust
core.

The lifecycle is `Create` → `SetOpt`* → `Init` → `Parse` until it returns FALSE
→ `Destroy`. `src/main.c` is that loop; `filesys_proto.cpp` upstream is the same
loop plus dialogs.

## Usage

```sh
xfer --proto x|y|z|kermit|bplus|quickvan
     --send FILE... | --recv DIR [--recv-name NAME]
     --pty 'CMD' | --serial DEV [--baud N] | --fd N
     [--limit SECONDS] [-v]
```

`--pty` spawns a peer on a pty and is what the test suite uses. `--serial`
drives a real port — with the FTDI loopback rig (see `CLAUDE.md`) you can run a
transfer over actual wire:

```sh
./build/xfer --proto z --send big.bin --serial /dev/ttyUSB0 &
rz -b < /dev/ttyUSB1 > /dev/ttyUSB1
```

## Traps

Each of these cost real debugging time, and each looks like something else.

- **`FTSetTimeOut` re-arms a deadline; it is not a value.** The protocols call
  it with the *same* number on every packet in order to reset the timer. Read it
  as a change-of-value signal and a stale deadline fires mid-transfer — which
  presents as a flaky failure on large files, not as a timeout bug. It was a
  1-in-3 ymodem flake here.
- **`KMT_MODE` takes `KMT_MODE_T`, not `OpId_t`.** The enums overlap
  misleadingly: `OpKmtRcv == 3 == IdKmtSend`, so passing the OpId asks kermit to
  send when you meant receive, and you get a silent stall with bytes flowing.
- **`YMODEM_OPT` must be `Yopt1K`**, which `filesys_proto.cpp:1409` hardcodes.
  Leave it unset and `YOpt == 0` falls through `YSendPacket`'s switch into
  `assert(0)` — a core dump, not an error return.
- **A pty peer's stderr lands in the protocol stream.** `ymodem.c` answers an
  unexpected byte with `assert(0)`, so `2>/dev/null` on the peer command is
  load-bearing. Real serial links do not have this problem; ptys do.
- **Settings must mirror `ttset.c`.** A zeroed `TTTSet` sets every timeout to 0
  and every transfer aborts on its first wait. `settings_defaults()` in
  `main.c` carries the ~20 fields the protocols read, with line references. Same
  hazard as the oracle's, and see `CLAUDE.md` ground rule 4.
- **XMODEM has no filename and no length.** Receiving needs `--recv-name`,
  because in Tera Term the receive dialog supplies it via `FileNames[]`. And the
  received file is padded to a whole 128/1024-byte block, so a byte-exact
  comparison against the original will fail — that is the protocol, not a bug.
- **Kermit uppercases filenames** ("common form"), so `payload.bin` arrives as
  `PAYLOAD.BIN`. Correct on both sides.
- **`rb` exits without ACKing the end of a YMODEM batch.** After the closing
  null block, lrzsz's `rb` just leaves. Tera Term's `ymodem.c` sets
  `fv->Success = TRUE` only on that final ACK (`ymodem.c:851`), so the transfer
  completes, the file is byte-correct, and the protocol still reports failure —
  intermittently, depending on whether `rb` gets scheduled before or after we
  poll. It presented as a ~2-in-10 flake with `in=10` instead of `in=11`:
  **exactly one byte short.**
  The harness now detects peer-close (EOF/EIO, which `Read1Byte` cannot express
  — it returns 0 for both "nothing yet" and "gone forever") and exits with code
  **4** instead of spinning to the wall-clock limit. `run_tests.sh` accepts 4
  *only* together with a byte-identical file. Do not widen that: the file
  comparison is the real assertion, and code 4 on its own means the peer
  vanished mid-transfer.
- **C-Kermit is the wrong counterparty for a pty.** It sees a tty and drops into
  interactive command mode. G-Kermit (`gkermit`) speaks the protocol on stdio
  and is what the suite uses.

## Not covered

- **B-Plus and Quick-VAN** compile and are wired up, but have no counterparty to
  test against — that is precisely why `PLAN.md` keeps them as vendored C marked
  best-effort rather than rewriting them.
- **Only the happy path is exercised.** No induced line noise, no cancellation,
  no resume, no disk-full. Those want a fault-injecting transport, which is a
  Stage 2 job.
- `protolog.cpp` is compiled in but the suite never enables `LogFlag`.

## Dependencies

`gcc`, `make`, and for the interop suite `lrzsz` and `gkermit`. Shares
`../winshim` — the Win32 surface these protocols need turned out to be a
subset of what the VT engine already needed, plus `MessageBox` and MSVC's
`stat`.
