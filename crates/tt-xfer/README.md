# tt-xfer — X/Y/ZMODEM, Kermit, B-Plus and Quick-VAN

```sh
cargo test -p tt-xfer          # 9 unit + 12 interop, against lrzsz and gkermit
cargo check -p tt-xfer --target x86_64-pc-windows-gnu
```

The protocols are Tera Term's, vendored verbatim under `vendor/ttpfile/` and
compiled in by `build.rs`. This crate is the **host** they attach to: the three
vtables in `csrc/tt_xfer.c`, and the loop that drives them in `src/lib.rs`.
Nothing is reimplemented. `PLAN.md`'s spike 2 is the argument for that, and the
short version is that B-Plus was CompuServe's and Quick-VAN was NIFTY-Serve's,
both services are gone, and a rewrite of either could only ever be checked
against a reading of the code it replaced.

## The shape

Upstream's equivalent is `teraterm/filesys_proto.cpp`. It is the same three
vtables plus a modal dialog, a Win32 message pump and a file-scope `FileVar`.
Here:

| Seam | Where |
|---|---|
| `TComm` — BinaryOut / Read1Byte / Insert1Byte / FlashReceiveBuf | `csrc/tt_xfer.c`, over a real `TComVar` |
| `TFileIO` — 14 file ops | `csrc/fileio_posix.c` / `fileio_windows.c`, replacing `filesys_win32.cpp` |
| `TFileVarProto` — services + the `InfoOp` progress vtable | `csrc/tt_xfer.c` |
| `SetTimer` / `KillTimer` / `ProtoEnd` / `TTMessageBoxW` | the same, as globals |

**A transfer does not own a connection.** It is fed bytes and asked for bytes,
because it runs over the terminal's own link — the reader that normally feeds
the VT engine hands its bytes here instead while a transfer is up. That is the
one thing `xfer/`, which drove the same C from a file descriptor, could not
test.

The build has one explicit platform seam. POSIX compiles the small Win32/CRT
shim which the vendored sources have always used here; Windows compiles
against the real SDK and selects a wide-path `TFileIO` backend, so a UTF-8
filename is not reinterpreted through the process ANSI code page. The vendored
sources are unchanged on both sides. `csrc/platform.h` also redirects their
window timers and message boxes into the per-transfer host: this library has no
`HWND`, and an error in the core must not open a second dialog behind Qt's.
The three native archives are linked whole: every protocol constructor is
selected at runtime from the host archive, and MinGW's one-pass archive scan
otherwise skips them before it learns their names. `protolog.cpp` also calls
back into the C archive for its path conversions, while the protocols call
back into the host; merely reversing the archives moves the unresolved symbol
rather than fixing the cycle.

**The comm side is `TComVar`-shaped, not merely vtable-shaped**, because three
places in the protocol sources reach past the vtable: `raw.c:152` drains
`cv->InBuff` itself, `bplus.c:885` waits for `cv->OutBuffCount` to reach zero,
and every protocol tests `cv->Ready` before deciding it cannot finish. `raw.c`
is compiled in partly to keep that honest.

## Traps

Each of these cost real time here, and each looks like something else.

- **`Insert1Byte` puts the byte at the front of the *receive* buffer.**
  `filesys_proto.h:61` comments it as "1byte送信" — send one byte — and the
  implementation it maps to, `CommInsert1Byte` (`ttcmn.c:532`), does the
  opposite. It exists for auto-start: the terminal has already swallowed
  `ZPAD ZDLE B 0 0` out of the stream, so `ZInit` pushes those five bytes back
  for the protocol to read. Send them instead and a zmodem header goes to the
  peer from the wrong direction. **`xfer/`'s spike had this backwards** and
  never noticed, because nothing there ran an auto-start mode.
- **`Read1Byte` is `CommReadRawByte`, not `CommRead1Byte`, and `BinaryOut` is
  `CommRawOut`, not `CommBinaryOut`.** The difference in both cases is the
  telnet codec. Upstream runs one buffer for the terminal and the transfer, so
  the unescaping happens on the way past; here `tt-conn`'s telnet transport
  has already done it. Doing it twice eats one `0xFF` of every escaped pair —
  invisible on text, fatal on every binary transfer to a terminal server.
- **No protocol closes the received file.** XMODEM's EOT arm sets `Success`,
  ACKs and returns FALSE (`xmodem.c:444`); `Destroy` frees its state without
  touching the file. Upstream gets away with it because `ProtoEnd` tears the
  whole `FileVar` down a moment later. A library cannot: the caller is
  entitled to report "done" and let the user open the file, and with stdio
  buffering that file is short by up to 4 KB. `tt_xfer_parse` closes it when
  Parse returns FALSE. The symptom was a 4106-byte payload arriving as exactly
  4096 bytes, which reads as a truncated transfer and is not one.
- **`fv->Success` is not monotonic and must not be latched.** A YMODEM batch
  sets it per file, so a latched first TRUE reports a batch that failed on its
  second file as a success.
- **ZMODEM answers a cancel with `Success = TRUE`.** `zmodem.c:1047` sets it on
  any `ZFIN`, and cancelling provokes one — so the protocol's own verdict on a
  transfer the user stopped halfway is "fine, thanks". `Transfer::succeeded`
  subtracts the cancel.
- **Cancelling is not a state change, it is a state change plus a timer.**
  ZMODEM sends `ZCAN`, arms 500 ms through `SetTimer` (`zmodem.c:1586`) and
  finishes when that fires. A host that ignores the timer leaves the transfer
  waiting for a peer it has already told to go away — a hang at exactly the
  moment the user asked for it to stop.
- **`FTSetTimeOut` re-arms a deadline; it is not a value.** The protocols call
  it with the *same* number on every packet in order to reset the timer. Read
  it as a change-of-value signal and a stale deadline fires mid-transfer,
  which presents as a flaky failure on large files rather than as a timeout
  bug. It was a one-in-three ymodem flake in `xfer/`.
- **`KMT_MODE` takes `KMT_MODE_T`, not `OpId_t`.** The enums overlap
  misleadingly: `OpKmtRcv == 3 == IdKmtSend`, so passing the OpId asks Kermit
  to send when you asked it to receive, and the failure is a silent stall with
  bytes flowing in both directions.
- **`YMODEM_OPT` must be set.** `filesys_proto.cpp:1409` hardcodes `Yopt1K`,
  and it is the only value `YSendPacket` has a case for — leaving it zero
  falls through the switch to `assert(0)`, which is a crash, not an error.
- **One `Parse` per wakeup moves one packet.** `Transfer::poll` loops while the
  protocol is making progress (consuming input or producing output) and stops
  when it is not. A caller that fed 8 KB and parsed once would move a file at
  one packet per turn of the event loop.
- **`Link::Network` means zmodem never times out.** It selects
  `ZmodemTimeOutTCPIP`, whose default is 0, on the assumption that the socket
  will notice a dead peer. That is right for telnet and SSH and wrong for a
  local pty, which is why `Link::local_pty()` reports as serial.
- **The peer shares the pty in the interop tests, so its stderr is redirected.**
  A warning it prints lands in the protocol stream, and `ymodem.c` meets an
  unexpected byte with `assert(0)`. That redirect is load-bearing.

## Not covered

- **B-Plus and Quick-VAN have no counterparty to test against.** They compile,
  they are wired, and they are best-effort — which is the same position
  upstream is in.
- **Only the happy path and cancellation.** No induced line noise, no resume,
  no disk-full. Those want a fault-injecting transport.
- **The `.lng` lookup for protocol messages.** `TTMessageBoxW` records the
  English default; translating it is the frontend's job.
