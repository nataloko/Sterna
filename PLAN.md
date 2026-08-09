# Sterna — plan and status

Canonical roadmap. Update the status markers as work lands; this file is the
thing a fresh session should read first, together with `CLAUDE.md`.

**Last updated:** 2026-08-09 · **Stage:** 1 complete, 2 in progress · **Commits:** 212

| | Stage 0 spike | Status |
|---|---|---|
| 0 | Repo bootstrap | ✅ done |
| 1 | Headless C oracle | ✅ **done** — 15,325 LOC of Tera Term builds on Linux, 18 tests green |
| 2 | `ttpfile` protocols on Linux | ✅ **done** — 8,409 lines build unmodified, 10/10 interop |
| 3 | Qt 6 IME reality check | ⏸ **deferred indefinitely** — CJK is out of scope (2026-08-07) |
| 4 | `serialport-rs` audit | ✅ **done** — adopt it, plus a thin patch layer; see below |
| 5 | `russh` compatibility sweep | ✅ **done** — algorithms and auth green; real-device behaviour an accepted risk |

**The "needs the user's desktop / hardware" premise was wrong.** Measured
2026-08-07: the dev container is a distrobox container on a Bluefin (Fedora
Silverblue 44) host and inherits the whole session — Qt 6 windows open on the
real desktop under both Wayland and Xwayland, the session bus is reachable, and
an FTDI Quad RS232-HS is present with `ttyUSB0`/`ttyUSB1` wired back-to-back on
data *and* control lines. See `CLAUDE.md` for the capability table and the three
places it still bites.

**All Stage 0 spikes are now resolved.** Of the three once called highest-risk:
spike 3 became a scope decision, spike 4 passed, and spike 5 was re-scoped —
see below — because the hardware it was waiting for does not exist here and
never will. What remains in Stage 0 is decisions and CI, not unknowns.

---

## Context

Tera Term is a *communications terminal* — it connects **out** to serial ports,
telnet, SSH and named pipes. Peer group is PuTTY, SecureCRT, minicom and YAT,
not GNOME Terminal or Alacritty. The Linux gap is real: `minicom`/`picocom` have
no scripting and no GUI, `cutecom`/`moserial` are toys, PuTTY has serial but
neither scripting nor file transfer, and the one tool that covers this ground —
SecureCRT — is closed and paid.

Tera Term is Windows-only structurally, not incidentally: **`_WIN32` appears
zero times** in 157k lines, because the code has never been asked to compile
elsewhere. 184 files include `<windows.h>`; 220 reference `HWND`; the thing that
looks like a portability layer (`common/tmfc.*`) is a thin `HWND` wrapper, 19
subclasses deep. An in-tree port would be a rewrite wearing a port's clothes.

**Two findings make this far cheaper than 157k LOC suggests, both now verified
by working code, not inspection:**

1. **The renderer seam is tiny.** `vtdisp.h` exports 75 functions; only two draw
   text (`DispStrA`/`DispStrW`). `vtterm.c` — 5,939 lines running the entire
   VT100/220/320/525 + xterm state machine — has **zero** Win32 tokens and makes
   **zero** drawing calls.
2. **The file-transfer protocols are already portable.** Win32 token counts:
   `xmodem.c` 0; `kermit.c`/`zmodem.c`/`ymodem.c`/`bplus.c`/`quickvan.c` 2 each,
   all `#include <windows.h>` pulling in `BOOL`.

**Goal:** a free, native, GUI serial + SSH terminal with real scripting and
legacy file transfer, on Windows and Linux — the ~20% of Tera Term that nothing
else on Linux does. **Not parity.**

---

## Decisions

### Locked

| Decision | Choice | Why |
|---|---|---|
| Scope | Focused successor | Parity is 3+ years; the niche is narrow and real |
| Core | Rust | `russh` deletes 62.6k LOC; `tokio` dissolves the `WSAAsyncSelect` problem |
| Shell | Qt 6 Widgets | Good on Windows *and* Linux, unlike GTK4; **76 dialogs make Widgets the load-bearing reason**; re-confirmed 2026-08-07, see below |
| Renderer | `QPainter` + glyph atlas, **no GPU** | 115200 baud is 11.5 KB/s; GPU spends the scarce resource on the non-bottleneck. **Measured 2026-08-07 on Qt 6.11.1: 255 fps full repaint, ~40x headroom** |
| Platforms | Windows + Linux | No macOS |
| Stage 1 focus | Serial, then SSH/telnet | The user's own daily-driver needs |
| Relationship to upstream | Fresh project that vendors specific subsystems | A fork means carrying 157k LOC we intend to delete |

**CJK is deferred indefinitely (2026-08-07).** Not on the roadmap: input
methods, the `.map`/`.tbl` charset depth, ambiguous-width policy, and the CJK
conformance corpus. This is worth recording honestly, because the toolkit was
chosen on the strength of "CJK IME on Linux decides it" — that argument is now
gone, and Qt 6 stands on its remaining merits, which are real but no longer
decisive. Revisit the toolkit only if something *else* about Qt disappoints; do
not reopen it on this ground alone.

Two things stay in scope regardless. **Wide and combining character handling in
the grid** — it arrives free with the oracle-driven port, and box drawing,
emoji and combining accents need it whether or not CJK does. And the **14 `.lng`
translation files**, which are donated work in 14 languages and cost nothing to
carry.

**Toolkit re-evaluated after the CJK deferral (2026-08-07) — conclusion
unchanged.** Recorded so it isn't redone from scratch. Dropping CJK removed an
*advantage* Qt held; it did not grant one to any alternative, and no pairwise
comparison flips: GTK4 always lost on Windows rather than on IME, the
Rust-native toolkits (egui, iced, Slint) lost on dialogs, native integration and
text layout, and a webview lost on RSS and startup. What changed is which
argument bears the weight — it is now the **76 dialogs and the 909-line
`TTTSet`**, which is a sturdier reason than IME ever was, since it is also
risk 2 on the list below.

**The sharper framing: this question is a proxy for "are we still shipping
Windows?"** Qt wins because it is strong on both platforms at once. If Windows
ever leaves scope, GTK4 and the Rust-native options become live again. That is
the trigger to watch — not CJK, and not toolkit fashion.

### Settled 2026-08-07 — nothing open

- **Project name: Sterna.** The working name `qtterm` collided with an existing
  Qt terminal and with `qtermwidget`, and tied the project to a toolkit the
  architecture deliberately treats as swappable. Sterna names the tern mascot
  and stays independent of the implementation. Upstream is
  <https://github.com/nataloko/Sterna>.
- **Licence: 3-clause BSD.** See `LICENSE`. It matches the vendored Tera Term
  code, so the shipped distribution carries one licence text rather than two,
  and it keeps the no-endorsement clause — the live one for a project that is
  explicitly not affiliated with the TeraTerm Project. MIT was the alternative
  and differs only in dropping that clause.
- **Qt licensing posture: LGPLv3, dynamically linked, no commercial licence.**
  The obligations that follow are small but real and constrain packaging, so
  they are recorded rather than rediscovered: **never static-link Qt**; ship it
  as separate shared libraries so a user can substitute their own build; and
  carry the LGPL text plus an offer of Qt's source. **This binds on both
  platforms**, because Linux is an AppImage and an AppImage bundles Qt — the
  "Fedora just depends on the distro's Qt" escape hatch went away with the rpm
  (see Stage 1). So ~30 MB of Qt rides in the Windows installer and a
  comparable weight of `libQt6*.so` in the image, which is the real price of the
  toolkit choice and belongs in the README's size numbers.
- **Vendoring clearance: done, and it corrected an assumption.** `ttpfile/*.c`
  and the 14 `.lng` files are clear under Tera Term's 3-clause BSD. But 45 of
  the 49 `.map`/`.tbl` tables are **generated from Unicode Consortium data**,
  not Tera Term's own work — so they carry the Unicode licence, and should be
  regenerated from the UCD rather than copied. Moot while CJK is deferred.
  Detail in `ATTRIBUTION.md`.
- **Upstream bug reports drafted** — three now, not one, each with before/after
  output measured from patched and unpatched builds rather than asserted. See
  `docs/upstream-bugs.md`. **Filing needs a GitHub account**, so it is the one
  Stage 0 item that still needs the user. One of the three is an
  attacker-controlled out-of-bounds write and should go first. **A sixth report
  has since been drafted against `vte`** rather than Tera Term —
  `docs/vte-bug.md`, silent data loss when a UTF-8 sequence is split across a
  read — and needs the same account.

---

## Architecture

One process, one core library. The frontend is replaceable because it only ever
sees a flat C ABI over POD types.

```
┌─ frontend: Qt 6 Widgets (C++) ──── swappable: Tauri / TUI / headless ─┐
│  QWidget grid + QPainter glyph atlas · .ui dialogs                    │
│  key + mouse events · menus · clipboard · font/colour config          │
└──────────────────────── C ABI (cbindgen) ─────────────────────────────┘
┌─ Sterna core (Rust cdylib) ───────────────────────────────────────────┐
│  tt-vt       VT100/220/320/525 + xterm state machine (over `vte`)     │
│  tt-grid     cells, scrollback, selection, BCE, wide/combining        │
│  tt-charset  DEC sets + line drawing (CJK tables deferred)            │
│  tt-conn     serial | ssh (russh) | telnet | pty | pipe    [tokio]    │
│  tt-session  the loop between the two, and the ABI's surface          │
│  tt-xfer     FFI → vendored C: x/y/zmodem, kermit, bplus, quickvan    │
│  tt-script   TTL interpreter + mlua over one shared command table     │
│  tt-config   INI (GetPrivateProfile-compatible) + KEYBOARD.CNF        │
│  tt-i18n     .lng loader                                              │
└───────────────────────────────────────────────────────────────────────┘
```

**Core → frontend:** a drained event queue plus a zero-copy read API —
`Damage { rows }` + `snapshot(row) -> &[Cell]`, `Cell` being POD
`{ text: [u32;N], fg, bg, attrs: u32, width_class: u8 }`; OSC/window requests
(title, bell, palette, cursor shape, mouse mode, clipboard); connection
lifecycle including **prompt-needed** (password, keyboard-interactive, host-key
verification); transfer progress; and the five script dialog requests
`ttpmacro/` already defines (`inpdlg`, `msgdlg`, `statdlg`, `ListDlg`, `errdlg`).

**Frontend → core:** `key_event(keysym, mods)` — **the core owns the keymap**,
because `KEYBOARD.CNF` is a compatibility artifact; `paste`, `selection_get`
(and `commit_preedit`, should CJK ever be revived — keep room for it, don't
build it); `resize(cols, rows)` + `set_cell_metrics(w_px, h_px)`;
`connect`/`disconnect`/`send_file`/`run_script`; `settings_get/set`; prompt and
dialog results.

**Never crosses:** `HWND`, `QWidget`, `HDC`, fonts, glyphs, or pixels beyond
`cell_w`/`cell_h`. The core knows pixel dimensions only for pixel-mode mouse
reporting and window-size escape sequences.

### The leverage point: one settings schema

`common/tttypes.h` is a **909-line `TTTSet`**, surfaced by ~13.8k LOC of dialog
code and 76 `DIALOG` templates across 30 `.rc` files. **Do not hand-port these.**

Define one declarative schema (key, type, INI section+name, default, range,
`.lng` label key, help anchor) and generate: the Rust `Settings` struct and INI
reader/writer, the Qt dialog pages plus a search box, the TTL
`setsetting`/`getsetting` and Lua accessors, and the docs table. That turns
~14k LOC of dialogs into a schema plus ~1.5–2k of codegen.

**This is the difference between the project finishing and not.** Build it in
Stage 2 while morale is high, not Stage 3 when it hurts.

---

## Disposition of the existing tree

| Asset | LOC | Disposition |
|---|---:|---|
| `ttpfile/*.c` protocols | 9,777 | **Vendor as C**, call via FFI behind `TFileIO`. Validated by spike 2 — builds and interoperates on Linux |
| 49 `.map`/`.tbl` charset tables | data | **Deferred with CJK.** If revived: vendor verbatim — they encode exact round-trip behaviour `encoding_rs` doesn't reproduce |
| 14 `.lng` files | 17,610 | **Vendor verbatim, keep the format** |
| `vtterm.c` + `buffer.c` | 12,082 | **Port to Rust** (~14–16k). Reused as specification and oracle |
| `ttssh2/` | 62,596 | **Delete** → `russh` |
| `vtdisp.c` + `vtwin.cpp` + dialogs + `.rc` | ~28,000 | **Delete** → Qt + generated dialogs |
| `ttpmacro/` | 16,472 | **Port to Rust** (~9–10k) |
| `TTProxy/` | 8,314 | **Delete**, reimplement in core (~1k Rust) |
| `ttptek`, `ttpmenu`, `susie_plugin`, `cygwin/` | ~11,000 | **Drop** |

Net: ~10k LOC of C carried forward, ~30k as executable specification, ~115k deleted.

---

## Stages

### ✅ Stage 0 — bootstrap + de-risking — **COMPLETE 2026-08-07**

Every spike resolved, every open decision settled, CI green on all of it. The
one item still needing a human is filing the upstream bug report, which needs a
GitHub account and blocks nothing.

**What Stage 0 bought:** the two subsystems that would have been the most
expensive surprises are now proven rather than assumed. Tera Term's VT engine
runs headless as a differential oracle (15,325 lines), and its file-transfer
protocols run and interoperate on Linux (8,409 lines) — so "vendor, don't
rewrite" is measured, not hoped. The serial and SSH layers were audited against
real hardware and real servers. The one risk that could not be closed is
old-device SSH *behaviour*, and it is recorded as accepted with a named
mitigation rather than left looking open.

Spike 1 delivered `oracle/` — see `oracle/README.md`. Result exceeded the plan:
**15,325 lines compile unmodified**, not the 12,082 estimated, because
`charset.cpp` and `unicode.cpp` came along free (and they carry the CJK width
and ISO-2022 behaviour, so that matters).

#### Spike 2 result — vendoring `ttpfile` is sound

`xfer/`, 2026-08-07. **8,409 lines of protocol C compile unmodified on Linux**
and interoperate in both directions with the reference implementations:
x/y/zmodem against `lrzsz`, kermit against G-Kermit, 10/10 including a 1 MB
zmodem transfer. A 64 KB zmodem send also ran over the **real FTDI serial wire**
at essentially full line rate, so this is not a pty artifact.

The entire Win32 portability gap was **three things** — `MB_*` constants,
`struct _stati64`, `_S_IFREG` — plus five Secure-CRT functions. All went into
the shared `winshim`, which turned out to already cover most of what
the protocols need; the VT engine had needed a superset.

Structurally the protocols attach through **three vtables and six external
symbols**, nothing more:

| Seam | Supplied by |
|---|---|
| `TComm` — BinaryOut / Read1Byte / Insert1Byte / FlashReceiveBuf | a pty, socket or serial fd |
| `TFileIO` — 14 file ops | a POSIX impl replacing `filesys_win32.cpp` |
| `TFileVarProto` — services + `InfoOp` progress | the host |

plus `SetTimer`, `KillTimer`, `ProtoEnd`, `TTMessageBoxW`, `_atoi64`, `ctime_s`.
**That is the shape `tt-xfer` exposes to the Rust core**, and it is small enough
that the FFI boundary is a morning's work rather than a subsystem.

Not covered: B-Plus and Quick-VAN compile and are wired but have no counterparty
anywhere to test against — which is exactly why they stay vendored and
best-effort. Only happy paths are exercised; line noise, cancellation, resume
and disk-full want a fault-injecting transport, which is Stage 2 work. See
`xfer/README.md` for the traps, several of which are silent-stall or core-dump
shaped.

#### `tt-conn` — the serial transport is built

`crates/tt-conn/`, 2026-08-08. Everything spike 4 specified, exercised by 15
tests against the FTDI loopback rig (they skip loudly without
`TT_SERIAL_A`/`TT_SERIAL_B`, so CI stays green without hardware rather than
pretending). Written against `commlib.c`'s DCB fields one by one, which is why
it has MARK/SPACE parity and DSR flow control at all.

Three things the tests found that the spike had not:

- **`serialport-rs` reports a *busy* port as `ErrorKind::NoDevice`** — message
  "Device or resource busy", no errno. The obvious mapping tells a user with
  `minicom` open in another window that their adapter was unplugged, for the
  single most common serial failure there is. Separated by asking whether the
  device node still exists, not by matching the crate's message text.
- **`tcsetattr` succeeding does not mean the driver applied it.** `CS6` is
  refused with `EINVAL`, which is fine; **`CS5` is accepted and ignored**, with
  eight bits still on the wire. The layer reads the setting back and refuses,
  because a settings dialog reporting five bits over an eight-bit wire produces
  corruption that looks like a cabling fault.
- **`flush` cannot be `tcdrain`.** Flow control holds the output queue
  indefinitely, so a flush from a UI thread with CTS low is a frozen
  application — and that is not an edge case, it is what backpressure looks
  like. `TIOCOUTQ` is polled against a deadline instead.

Deferred deliberately: async. `tokio` belongs here eventually and `russh` will
require it, but inventing the async shape before the second transport exists
would be guessing at a seam that is currently a byte-stream API.

#### Spike 4 result — adopt `serialport-rs`, with a thin patch layer

Audited 2026-08-07 against an FTDI Quad RS232-HS with two ports wired
back-to-back, and — this is the point — **against the requirement in
`commlib.c`**, not a generic wishlist. What Stage 1 needs works: enumeration
with USB vid/pid/manufacturer, 7/8 data bits, 1/2 stop bits, none/odd/even
parity, both flow-control modes, latched `set_break`/`clear_break` matching
`SetCommBreak` semantics, DTR→DSR and RTS→CTS both observable, honest timeouts,
and every baud rate exact on readback from 300 to 3000000 including a
non-standard 250000. Hotplug: removal and re-attach both seen by
re-enumeration.

**Correction (2026-08-08, while building `tt-conn`): this originally said "5–8
data bits", and that was an API claim wearing a hardware result's clothes.** The
audit checked that `serialport-rs`'s `DataBits` enum has four values, not that
any of them reached the wire. Measured properly against the same adapter: `CS6`
is refused with `EINVAL`, and **`CS5` is accepted by `tcsetattr` and then
ignored**, with eight bits still going out. `tcsetattr` reports success if it
could apply *any* of what it was asked, so `tt-conn` reads the setting back and
refuses rather than lying to the settings dialog. Seven and eight are real and
proven on the wire.

Five gaps, and how each lands:

| Gap | In `commlib.c` | Verdict |
|---|---|---|
| MARK / SPACE parity | `:194-200` | Patchable — `CMSPAR` on the raw fd |
| Incoming break vs. a real NUL | — | Patchable — `PARMRK` yields an `FF 00 00` marker |
| XON/XOFF characters | `XonChar`/`XoffChar` | Patchable — `VSTART`/`VSTOP` |
| XON/XOFF thresholds | `XonLim=768`, `XoffLim=3328` | **Not on Linux** — the kernel owns its watermarks |
| DSR/DTR flow control | `:219` `fOutxDsrFlow` | **Not a crate gap — Linux has no DSR flow-control bit.** Emulate: poll DSR, gate writes |

**The finding that makes the patch layer viable:** `serialport-rs` does not
clobber foreign termios changes. `PARMRK` survived `set_baud_rate`, `set_parity`,
`set_flow_control`, `set_timeout` and `write_request_to_send`. Had it rewritten
termios wholesale, every crate call would have silently undone our settings and
adoption would have meant a fork.

Two consequences for `tt-conn`:

- **The serial layer is platform-split at the type level regardless.** The raw-fd
  escape hatch lives on the concrete `TTYPort`; `Box<dyn SerialPort>` does not
  implement `AsRawFd`. Make that split explicit and thin rather than pretending
  the portable trait suffices.
- **The port picker must key on USB topology** (`/dev/serial/by-path/`), not on
  `ttyUSBn` and not on the serial number — the FTDI quad reports `serial=None`,
  and `ttyUSBn` is assignment-order dependent, so a reconnect can otherwise
  reattach to a different physical port. Disconnect surfaces as
  `ErrorKind::BrokenPipe` with `raw_os_error() == None`, an undocumented crate
  mapping rather than `EIO`/`ENXIO`; wrap it so there is one place to fix.

Untested: Windows, where `fOutxDsrFlow` exists natively and this inverts; and
the `CH340G_hw_flowctrl` case upstream carries, which needs a CH340 adapter.

### 🔵 Stage 1 — the Linux serial + SSH terminal (3–4 months, ~25–30k LOC)

Must be shippable and genuinely useful, not a demo.

**Every deliverable below is done as of 2026-08-08.** The stage is not
*declared* finished, because its own test is not a checklist: "the Wine
shortcut gets deleted and it's daily-driven for serial console work". That is
the user's call and it needs a week of real use, not another commit. Stage 2
has started in the meantime, at the settings schema — which is where the first
thing daily driving will complain about lives (`keyboard.backspace`, still
sending BS because that is what Tera Term does).

- `tt-vt` + `tt-grid`: VT100/220 + core xterm, SGR/256/truecolor, scrollback,
  selection, BCE, wide + combining chars. Ported **against the oracle**.
  ✅ **done for Stage 1's purposes** — 102 differential cases and 365 of
  esctest's 568, see below. Selection, the last piece, landed 2026-08-08: the
  grid's half of it is naming a line so a highlight can outlive the output
  under it, and the rest is the window's.
- `tt-conn`: **serial first** (the differentiator), then SSH2 via `russh`, then
  telnet, then local PTY via `portable-pty`. ✅ **done — all four transports** —
  the patch layer spike 4 specified is built and green on the loopback rig
  (`CMSPAR` parity, `PARMRK` break detection, `VSTART`/`VSTOP`, a userspace
  DSR-flow shim, by-path port identity); SSH is built on `russh` with a
  caller-driven prompt lifecycle, 15 integration tests against real OpenSSH and
  real dropbear, and its own `known_hosts` and `ssh_config` readers; telnet is
  ported from `telnet.c` plus `ttcmn.c`'s framing, with 28 unit tests and 10
  against a real `telnetd`; and the local pty is `portable-pty` with our own
  read path, 19 tests that need no rig at all. See `crates/tt-conn/README.md`
  and below.
- `tt-session`: the loop between engine and transport, and what the C ABI
  exports. ✅ **done** — 20 tests, three of them over the real wire. See
  `crates/tt-session/README.md`.
- `tt-ffi`: the flat C ABI, `cbindgen` over `tt-session`. ✅ **done** —
  `libsterna.so` plus a generated, committed, CI-gated header, exercised from
  C and C++ rather than from Rust. See `crates/tt-ffi/README.md` and below.
- Qt shell: one window, grid painter, clipboard, font/colour config,
  connect dialog, serial-port picker with live enumeration. ✅ **done for
  Stage 1's purposes** — all of that, plus scrollback with a scrollbar, wheel
  and `Shift+PageUp`, selection by character, word and line that survives the
  output scrolling under it, the SSH connect path with its host-key and
  authentication dialogs, telnet, and a local shell on a menu item. See
  `shell/README.md` and below.
- **`~/.ssh/config`, `~/.ssh/known_hosts`, `~/.ssh/id_*`** — Tera Term lacks
  this and it is a major Linux adoption lever. ✅ **done** — an alias typed into
  the connect dialog, or given on the command line, brings its user, port, key,
  `StrictHostKeyChecking` and legacy-algorithm settings with it. Both files are
  read and written to OpenSSH's own semantics, which meant writing both readers;
  see below.
- Session logging (timestamped, rotation). ✅ **done** — raw and text modes,
  `[time] ` line prefixes, generation rotation, and a live indicator in the
  window. The text tap is inside `tt-vt` at upstream's `FLogPutUTF32` seam
  rather than a second escape-sequence stripper beside the log.
- **AppImage. And only an AppImage** — decided 2026-08-08, ✅ **built the same
  day**; see `packaging/README.md`. No rpm, and by the same decision no deb
  later. One artifact, one thing to test, and no per-distro
  packaging to keep alive alongside the Windows installer while the project is
  one person. It also suits the machine this is being written for: the host is
  Bluefin, an image-based Fedora where layering an rpm is the awkward path and
  a self-contained binary is the ordinary one.

  **The cost lands on the Qt licence, and it is not zero.** The posture above
  assumed Linux would be an rpm depending on in-distro Qt, which costs nothing;
  an AppImage *bundles* Qt, so Linux now carries the same obligations Windows
  does — never static-link it, keep it as separate shared libraries a user can
  substitute, and ship the LGPL text plus an offer of Qt's source **inside the
  image**. That is a build step, not a note in a README, and
  `packaging/appimage/build.sh` does it.

  **Measured from the image on the desktop:** 37 MB on disk, 43 MB RSS / 33 MB
  PSS with a shell attached under Wayland, ~144 ms from exec to a mapped window
  — the last of which includes mounting the SquashFS, a cost the build tree does
  not pay. The base is `sterna-fedora`, so the **glibc floor is 2.43**: this
  image runs on Fedora 44 and not much else yet, which is deliberate and
  temporary. Reaching older distributions needs an older base *and* a Qt fetched
  separately, because the distributions that give reach also ship old Qt — the
  Ubuntu 24.04 container was rejected as a base for exactly that reason, its
  Qt 6.4.2 being the one that costs 62 MB of extra private memory under Wayland.

  **Two of the three ways this fails are silent**, both now in `CLAUDE.md`:
  linuxdeploy's `patchelf` predates `.relr.dyn` and corrupts every library it
  bundles, which presents as a segfault in the `_init` of whichever one the
  loader reaches first; and a Qt Wayland plugin with no shell integration binds
  the registry, maps no window, warns about nothing and never exits non-zero —
  which looks exactly like a working headless run and was briefly counted as
  one here.

- **A perf gate**, because "light" is the reason this project exists. ✅
  **done** — `bench/`, four shell numbers and three engine ones, an absolute
  floor in CI and a same-machine baseline locally. See `bench/README.md`, and
  the numbers below.

**Done when:** the Wine shortcut gets deleted and it's daily-driven for serial
console work.

Deliberately absent: file transfer, macros, tabs, Windows build, most settings.

#### The C ABI is the seam, and it is now real

`crates/tt-ffi/`, 2026-08-08. `libsterna.so` and a generated
`include/sterna.h`: session lifecycle, zero-copy row reads, the key and mouse
input paths, serial connect with live port enumeration, and the drained event
queue. Roughly forty functions, which is the whole of what a terminal window
needs.

**The design decision worth recording: the ABI takes its enums straight from
the core crates.** `#[repr(u32)]` on `tt_vt::Key` rather than a `TtKey` here
with a conversion table — so there is one list of the 55 key names, one list of
parity values, and no mapping function that can be quietly wrong about which
`F` key is which. The price is that reordering one of those Rust enums is an
ABI break with no other symptom, so the generated header is **committed and CI
fails on a diff**: the break becomes a review question instead of a runtime
mystery. Same reasoning as the differential suite — put the check where a
mistake is visible, not where it is plausible.

Three things it cost to find, all in `CLAUDE.md`:

- **cbindgen parses files, not crates, so privacy does not exist.** `tt-vt`'s
  private `locator_flag` module put `PIXEL`, `ONE_SHOT` and `FILTERED` into the
  public header, unprefixed, until they were excluded by name.
- **`Builder::with_crate` runs `cargo metadata` from inside a build script**,
  which can block on the package cache lock — and combined with `with_src` it
  parses the crate twice and emits every declaration twice.
- **`--test-threads=1` is per test binary.** Cargo runs the binaries
  concurrently regardless, so the command both READMEs gave for the hardware
  tests put `tt-conn`'s and `tt-session`'s suites on the same two ports at
  once. It failed as a flaky `tt-conn` rather than as an overbooked rig.

The test is `tests/abi.c`, compiled against the generated header with
`-Wall -Wextra -Werror -pedantic` and linked against the shared library, plus
the same header compiled as C++. A Rust test would prove the logic and nothing
about the seam: it never compiles the header, never links the library, and
cannot notice that a struct the frontend has to fill in is unreachable without
a Rust type.

Deliberately not exposed yet, each for a reason: the **settings surface**
(`TtConfig` carries six fields, not `tt_vt::Config`'s thirty — those are
`TERATERM.INI` keys and belong to Stage 2's generated schema, so transcribing
them now would be work done twice, the second time as a deletion),
**scrollback and selection** (`tt-session` has no viewport yet), and
**connects for transports that do not exist**.

#### The Qt shell exists, and its event loop has no timer in it

`shell/`, 2026-08-08. One window on the C ABI — grid painter, keyboard, mouse,
selection, clipboard, a serial connect dialog whose port list refreshes while
it is open, and a status line. Built and run in the `sterna-fedora`
container, so the Qt is 6.11.1, the one the desktop runs.

**The design decision worth recording is the event loop.** `tt_session_pump`
blocks for the transport's read timeout, which leaves two obvious shapes and
both are wrong: pumping from the UI thread freezes the window for as long as
the line is quiet — the *normal* state of a serial console — and pumping on a
timer instead spends a wakeup every frame, forever, to discover that nothing
arrived, on a terminal whose whole claim is being light.

So the core grew `tt_session_poll_fd`, a `QSocketNotifier` waits on it, and the
pump runs with a budget of **zero**, which reads exactly once and returns. A
burst arrives over several turns of the event loop and the window keeps
painting through it. Measured on the loopback rig at 115200: **zero CPU ticks
over five seconds** with a port open and idle, 65 MB RSS (in line with the
~60 MB Qt floor above), and **40 ms of CPU for 44 KB** of coloured output —
after which it returns to zero.

The one case a descriptor cannot cover is output the far end *refused*: flow
control holds the line, the write comes up short, and the remainder waits for a
pump that never comes, because a device asserting backpressure is not sending
anything to wake us with. `tt_session_pending_out` makes that visible and the
shell runs a 20 ms retry timer only while it is non-zero. Without it a stalled
write reads as dropped keystrokes.

Three more things the shell needed from the core, each because the frontend
should not have been holding it:

- **Return and Backspace are not `TtKey`s**, because `keyboard.c` handles
  `VK_RETURN` and `VK_BACK` in `KeyDown` rather than in the table `GetKeyStr`
  walks. Return needed nothing new at the seam — upstream marks it `IdText` so
  `OutControl` expands the CR by LNM, so `send_text` does the same and a shell
  sends `"\r"`. Backspace needed DECBKM's state, now readable.
- **The cdylib had no `DT_SONAME`**, which cargo does not add, so the shell
  built out of tree recorded a *relative* `DT_NEEDED` and ran only from its
  build directory.

**The colour model is `vtdisp.c:GetDrawAttr` ported, not invented.** Tera Term's
bold, blink and underline attributes each carry their own colour pair and which
applies is a priority chain, not a blend; an explicit SGR colour then overrides
whichever won. With upstream's defaults that means **black on white, blue bold
and magenta underline**, which is what Tera Term looks like out of the box.
Those values are all `TERATERM.INI` keys and become Stage 2's schema.

**`shell/tests/render_test.cpp` is the only thing that can check the painter.**
The differential suite proves the grid matches Tera Term and stops where cells
become pixels. `QWidget::grab()` re-renders offscreen — which is both the thing
worth testing and the only screenshot available here, GNOME's screenshot D-Bus
API having been locked down since 45. Ten cases assert on background fills,
which are the entire output of the colour model; glyphs are checked only for
ink present or absent, which is font-independent and catches the failure that
actually happens — the right codepoints rendering blank.

Three of those cases pin behaviour that looks like a painter bug and is an
upstream default, which is exactly the shape of thing someone later "fixes":
truecolor pure red resolving to *dark* red (the nearest-colour search flips
bright and dim when a full-colour mode is on, and 256-colour ships on),
`SGR 101` doing nothing at all (`Aixterm16Color` ships off, so 90-97 and
100-107 are ignored and the previous pen stands), and a cell's `fg`/`bg`
meaning a palette index only when its attribute bit says so.

#### The scrollback viewport, and why it is in the core

The grid had scrollback from the first landing and nothing could look at it.
The viewport that closed that is in `tt-session` rather than in the shell, for
one reason: **it has to be anchored to content, not to the bottom**, and only
the thing that owns the feed knows when the grid scrolled.

Anchoring to the bottom is the obvious implementation and it fails in exactly
the case anyone scrolls back on a serial console — reading a boot log on a
device that is still booting, where every line printed slides what you are
reading up by one. So `Grid` counts the lines that leave the page,
**monotonically rather than by length**, because once the scrollback is full its
length stops changing while the content still shifts by one per line; the
session moves the offset by that count.

Two shapes fell out of it that are worth keeping:

- **`row()` is viewport-relative**, rather than there being a second row
  function beside it. A painter cannot then pick the wrong one, and with the
  offset at zero — which is where it stays until something scrolls — it is the
  live screen exactly as before.
- **The cursor needs the opposite and gets its own accessor.** It belongs to
  the live screen, so scrolling back moves it *down* and off the bottom;
  painting `TtCursor::y` regardless would stamp a block onto a line of history
  and read as a prompt that is not there.

The frontend consequence is that the scrollbar follows the session rather than
the session following the scrollbar, and its own updates are signal-blocked —
otherwise every pump writes back into the session and the rounding fights the
offset the core just chose. Ten tests in `tt-session` and three render tests
cover it, the render ones because a frontend that re-read the offset wrongly
would undo the whole thing with no core test noticing.

#### And selection, which is the same argument one level up

The last Stage 1 item. A selection held in viewport rows is held in "wherever
this has slid to by now", so the honest thing the old code did was **drop it on
every scroll** — and the case people actually copy in is a line off a device
that is *still printing*, where the highlight would otherwise stay put while
the text walks up the screen underneath it.

So the core grew the one thing a frontend cannot work out for itself: a
**number for a line**. `top_line` is `Grid::scrolled_off`, which makes the top
of the live page always line *n* by construction, and `line_at(row)` /
`line(n)` are the two directions across it. A line that has been evicted, or
one not printed yet, comes back **absent** rather than making the caller
range-check first — a frontend holding an old number has to be able to ask.

The window's half is upstream's, and every piece of it looks arbitrary until
the original turns up:

- **Endpoints round to the nearest boundary between characters**
  (`buffer.c:GetCharCell`), which is what makes dragging across `abc` select
  `abc` rather than `ab`. Wide characters are taken or left whole.
- **`ts.DelimList`'s default word set** — a space and every ASCII punctuation
  mark **except** underscore — and `CheckDelimiterChar`'s two arms: starting on
  a delimiter takes the run of *that same character*, so double-clicking the
  gap between two columns of output selects the gap.
- **The anchor is the whole unit the drag started on**, not the point it
  started at, which is what lets a double-clicked word be dragged leftwards and
  keep its right-hand edge. Upstream keeps the same pair.

Two things it cost. **Qt has no triple-click event** — the second press arrives
as `mouseDoubleClickEvent` *instead of* a press, so a widget counting clicks in
its press handler never reaches three. And **the padding half of a wide
character always joins a word run**, so the leftward walk can stop on padding
whose lead it has just refused; stepping the wrong way there puts half a
character in the selection.

Six render tests cover it, driven through real `QMouseEvent`s rather than by
calling the handlers, because the click counting is part of what is being
tested.

#### First landing — the differential gate is live

`crates/` exists: `tt-grid` (cells, cursor, scroll region, scrollback, alternate
screen), `tt-charset` (ISO-2022 and DEC special graphics), `tt-vt` (the state
machine, `vte` for byte-level parsing), and `tt-dump` (a CLI that speaks the
oracle's argument set and dump format). `./run_diff.sh` feeds every case to
**both** engines and diffs them against each other. **103 cases: 102 matching
and one known divergence.**

**The design decision worth recording: the differential suite has no golden
files.** The oracle *is* the expectation, so a new case is an input file and
nothing else — nothing to bless, nothing that can quietly enshrine a wrong
answer. `oracle/run_tests.sh` keeps its goldens for the different job of
catching the oracle itself drifting when upstream is bumped. A case can carry an
`xfail` file naming a *known* divergence; it is reported but not fatal, and it
fails if the two ever agree, so the marker cannot outlive the bug.

Covered: cursor motion and clamping, ED/EL/ICH/DCH/ECH/IL/DL/SU/SD, scroll
regions, origin mode, insert mode, autowrap on and off, deferred wrap, tab
stops, DECSC/DECRC including the pen and the G-sets, DA/DSR replies, OSC titles,
all four `CRReceive` modes, wide characters at the margin, combining marks,
ISO-2022 designation and every locking and single shift, DEC special graphics,
256-colour and truecolor, 8-bit C1 controls, the alternate screen, DECSCA and
selective erase, the whole rectangular-area family (DECSACE, DECCARA, DECRARA,
DECFRA, DECERA, DECSERA, DECCRA), the XTWINOPS resize, left and right margins
(DECLRMM/DECSLRM) through every operation that reads them, DECALN, the soft
resets (DECSTR, DECSCL), DECRQSS, 8-bit control replies (S7C1T/S8C1T), every
private and ANSI mode via DECRQM, DECCOLM, and **mouse and focus reporting** —
all eight tracking modes and all five encodings, including DEC's locator.

**The VT engine is functionally complete for Stage 1.** What is left is
deliberate: Tek, printing, Japanese charset designations, and the XTWINOPS
operations that ask the display layer where the window is. See
`crates/README.md` for the list and the reason for each.

#### And what a second opinion found, once there was one

`esctest/`, 2026-08-08. The differential suite proves the port matches Tera
Term **on the cases somebody wrote**. It cannot say anything about ground the
cases never covered, and 93 hand-written cases is not much ground. esctest is
568 scenarios nobody here chose, and it found **ten real gaps** in an engine
that had been passing every gate for a week.

The method matters more than the list, because esctest measures against xterm
and this is a port of Tera Term. A failure is a question, not a verdict. So
`esctest/run_diff.sh` records each test's byte stream — esctest will write them
out — and feeds every one to **both** engines: if they agree, the failure is
Tera Term not being xterm and gets a written reason; if they disagree, it is
ours. **510 of 568 stimuli now produce identical output from both engines**,
and the 58 that do not are all colour or window queries the oracle answers from
a stub, so it cannot arbitrate them either way.

What that turned up, in order of how badly it would have bitten:

1. **HPR, VPR, HPB and VPB were missing entirely** (`CSI a`, `e`, `j`, `k`).
   Upstream has all four; they are the cursor moves measured against the *page*
   rather than the margins.
2. **VPA had lost origin mode.** `CSMoveToLineN` counts from the top margin;
   ours counted from the screen.
3. **A bare C1 byte was being swallowed.** In UTF-8 a lone `80..=9F` is invalid
   and Tera Term shows U+FFFD; `vte` executes it as a control and we dropped it.
   On a line that is not 8-bit clean this is the difference between a screen
   full of replacement characters and a screen that stays blank.
4. **DECSC did not save autowrap**, and **the alternate screen shared the main
   screen's save slot** — so a full-screen editor's `ESC 7` overwrote the
   position the shell underneath was going to come back to.
5. **LNM did nothing on receive.** With it set, a line feed returns the
   carriage as well (`vtterm.c:706`).
6. **`CSI ? Ps n` was answering the plain DSR reports.** Upstream reserves the
   private form for the locator (53 and 55) and ignores the rest.
7. **NEL went to the left margin** instead of to column zero.
8. **DECID (`ESC Z`) answered nothing**; it is Primary DA under another name.
9. **Insert mode shifted to the screen edge**, not to the right margin, so a
   character typed inside a margin pair shoved text out through it.
10. **The title reports and the title stack were absent** (`CSI 20`–`23 t`).

And an eleventh in the oracle, which is the same shape as the finding that
started this whole file: **`IdTitleReportEmpty` is 24, the whole
`WF_TITLEREPORT` mask**, so the shipped default sets both bits. The oracle had
read the name as "no bits" and had been standing in for a Tera Term with title
reporting switched off. The flag-word trap again, this time wearing a named
constant instead of a zero.

Each fix landed with a case in `oracle/cases/`, so the differential gate now
covers the ground esctest pointed at — 93 cases became 103.

#### Mouse reporting turned out to be differential-testable after all

The plan said it could not be: it turns *input events* into reports, and a
headless dump has no mouse. That was a failure of imagination about the
*harness*, not a fact about the problem. The oracle now takes directives inside
the byte stream —

```
ESC [ ? 1000 h ESC _ tt.mouse down 0 24 80 ESC \
```

— which the runner strips and executes between parses, on both engines. The
reports come out in the reply dump and are diffed like everything else.
Fourteen cases now cover it. Getting there fixed three more places the oracle
was lying: `ts.MouseEventTracking` and `ts.TranslateWheelToCursor` were left
zeroed when both ship on (the flag-word trap again, this time in a plain
`WORD`), `ShiftKey`/`ControlKey`/`AltKey` were defined as `BOOL` *variables*
when `keyboard.h` declares them as functions, and
`DispConvWinToScreen`/`DispConvScreenToWin` were empty stubs that never stored
through their out-parameters. None of it was reachable until something called
the mouse path.

**The lesson generalises: "not testable against the oracle" is worth
re-examining before it becomes a design constraint.** It paid off again
immediately — see the key table below, where the answer was the same shape.

#### And the key table, which is bytes *out*

`keyboard.c` now compiles into the oracle too (1,651 lines), so `tt.key <name>`
runs Tera Term's own `GetKeyStr()` and one case sweeps **55 keys across 10 mode
combinations**. The table was never transcribed.

Reaching it needed `src/keys.c` to `#include` the translation unit, because
`GetKeyStr` is `static` — upstream stays unmodified, and 200-odd escape
sequences stay untyped. The obvious alternative, driving the public
`KeyCodeSend()`, is worse: it routes through the delayed-send queue, so it
would have dragged an async subsystem into an answer decided before it.

Compiling it found two more places the oracle had been standing in for upstream
and getting it wrong: `keyboard.c` **owns** `AppliKeyMode`, `AppliCursorMode`,
`AppliEscapeMode`, `AutoRepeatMode` and `Send8BitMode`, which `stubs_manual.c`
had been defining — so `vtterm.c` set a mode and the real key table would never
have seen it — and `ShiftKey`/`ControlKey`/`AltKey` are upstream's, over
`GetAsyncKeyState`, rather than three booleans of ours.

#### What the harness caught, which is the point of having it

**The first run matched 18/18, which meant the corpus was too easy, not that the
port was done.** Every subsequent finding came from writing a harder case.

1. **`TermIDGetID()` never fails.** Case-sensitive `strcmp` against an UPPERCASE
   table, returning `IdVT100` for anything unrecognised — so `--term vt220` ran
   as a VT100 and `main.c`'s guard against that could never fire.
2. **A `ts->X = 0` at the top of `ttset.c` is an initialiser, not a default.**
   The big one. `ColorFlag`, `TermFlag`, `ISO2022Flag` and `WindowFlag` are each
   zeroed near `ttset.c:559` and then built up from per-key `GetOnOff(…, TRUE)`
   calls a thousand lines later. The oracle had taken the zeros, so it was
   reporting a Tera Term with **256-colour off, every ISO-2022 shift off, 8-bit
   controls off and the alternate screen off** — none of which is how it ships.
   Found while porting character sets, when SO and SI did nothing.
3. **A manual stub was lying.** `DispFindClosestColor` lives in the oracle's
   `stubs_manual.c` because `vtdisp.c` is not compiled; it held *xterm's*
   palette rather than Tera Term's and omitted the bright/dim flip the real one
   applies, so every truecolor SGR resolved to the wrong index. This is exactly
   the failure `CLAUDE.md` warns about — "every stub is a place the oracle can
   lie" — caught only because a Rust implementation disagreed with it.

Finding 2 is worth dwelling on: **the port was briefly being written against a
misconfigured oracle**, and the only reason it surfaced is that the differential
suite made a settings bug look like a parser bug and forced the question. A
golden-file-only suite would have blessed the wrong answer and moved on.

Then `./run_upstream.sh` pointed the same diff at **Tera Term's own exercisers**
and found two more upstream bugs in `buffer.c` — inside scripts upstream ships
to test exactly this behaviour, which had evidently been run by eye many times
without anyone diffing the buffer afterwards:

4. **`BuffGetAnyLineDataW` budgets output units with a column count.** A second,
   independent defect in the function `0001` already patches: `left` is seeded
   from a cell count but spent in `wchar_t` units, so any line with combining
   marks truncates at about half the width. More session-log data loss.
5. **ECH writes past the end of the line.** `CSI Ps X` clamps `Ps` to the
   terminal *width* and then writes that many cells *from the cursor*,
   overshooting into the next line — and off the end of the allocation on the
   last line. The parameter arrives in the byte stream, so this is an
   **attacker-controlled out-of-bounds write** in a program whose whole job is
   reading untrusted bytes. Reachable from upstream's own `bcetest.sh`.

A third arrived while implementing DECSED:

6. **`BuffSelectedErase*` index a line-relative pointer with an absolute buffer
   offset.** `CodeLineW = &CodeBuffW[LinePtr]` is the cursor's line; `j` is an
   absolute offset. So DECSED reads the protect bit from a cell roughly twice as
   far into the buffer, *writes* to it as well, and leaves the allocation
   entirely once the page has scrolled into the second half of the ring —
   confirmed under AddressSanitizer for both `CSI ? 0 J` and `CSI ? 1 J`. The
   same function also subtracts the start column from its end bound, so the
   cursor's own row erases nothing once the cursor is past mid-screen.

Reports for all four are drafted in `docs/upstream-bugs.md`; **file the two
memory-safety ones (ECH and DECSED) first**, and consider whether they want a
private report rather than public issues.

A fifth turned up as soon as the oracle could be driven with a mouse:

9. **`MakeMouseReportStr` builds the row's UTF-8 lead byte from the column.**
   In `DECSET 1005` mouse tracking, coordinates above 127 take a two-byte form;
   the row's branch reads `x` where it means `y`. Past row 96 the report carries
   the wrong row, or — when the column is small — the byte `0xC0`, which is not
   valid UTF-8 at all. The first bug found in `vtterm.c` rather than
   `buffer.c`.

Margins found two more, neither of them about margins:

7. **A plain HT takes the pending wrap before it tabs** (`vtterm.c:Tab`), so a
   tab arriving on a full line starts the next one. `CSI Ps I` (CHT) does not —
   it calls `CursorForwardTab` directly. Ours had been leaving the tab on the
   old row, which put the next character a row too high.
8. **A scroll region starting at row 0 fills the scrollback even when its
   bottom margin does not reach the last row.** `BuffScroll` slides the page
   and copies the rows below the region down to keep them in place. Nothing in
   the dump can see the scrollback, so case 69 reads it back through a resize.

Two more findings were the oracle's own, both of the same shape as finding 3 —
harness code reimplementing upstream logic and getting it wrong. Its
`disp_width()` treated only `'W'` as full-width and not `'F'`, so every
fullwidth form counted one column in the dump while `buffer.c` had given it two;
and it dumped its argv size rather than `NumOfColumns`/`NumOfLines`, so any
stream that resized the terminal was measured against the wrong width. **Both
were invisible until a Rust implementation disagreed**, which is the entire
argument for the differential suite over golden files.

Upstream behaviours reproduced deliberately, which will look like bugs to anyone
reading the Rust in isolation: G1 starts as DEC special graphics so a bare SO
draws lines; a single shift never ends in UTF-8 mode; C1 controls are masked to
C0 below VT220, making `U+008D` a carriage return; the nearest-colour search
flips bright and dim so truecolor red lands on index 1; and a line feed at the
bottom of the scroll region leaves pending-wrap set. All are in
`crates/README.md` with citations, alongside the known divergences — chiefly
that character width comes from the `unicode-width` crate rather than Tera
Term's own tables, which is fine until CJK is revived, and that DEL occupies a
cell upstream but not for us.

**One divergence is structural and worth flagging: Tera Term's CSI parser takes
intermediates and parameters in any order, and `vte` does not.**
`ControlSequence()` dispatches each byte on its numeric range alone, so
`ESC [ * 2 x` is a perfectly good DECSACE upstream; `vte` follows ECMA-48, where
an intermediate ends the parameter string, and drops the sequence. Upstream's own
`tests/#38168-deccara-range.sh` is written that way. Closing it means normalising
the byte stream before `vte` sees it — a small scanner, not a redesign — and it
is the first thing to reach for if a real device turns out to emit the same
shape.

#### SSH, and the two decisions this stage had deferred

`crates/tt-conn/src/ssh/`, 2026-08-08. `PLAN.md` deliberately left the async
shape open — "inventing it before the second transport exists would be guessing
at a seam that is currently a byte-stream API." The second transport exists now,
and it answered both questions the same way: **keep it inside `tt-conn`.**

**The tokio runtime is private to one module.** One thread, one current-thread
runtime, and a self-pipe so a frontend waits on SSH with the same
`QSocketNotifier` it uses for a serial port. The core, the C ABI and the shell
stay synchronous, which is what they wanted to be; the alternative was spreading
`russh`'s runtime through three layers that have no use for it. The descriptor
is the *same one* the session hands out afterwards, so the shell registers its
notifier once and keeps it across the handover.

**Connecting is a state machine the caller drives, not a callback.** `poll`
returns the question — host key, password, keyboard-interactive challenge, key
passphrase — and the worker waits. A callback would have to be `Send`, would run
on the worker thread, and would leave a Qt frontend raising a modal dialog from
the wrong one. This is the same drained-event shape `tt-session` already uses,
for the same reason, and it is now the pattern for anything interactive that
crosses the ABI.

Authentication follows the server's `remaining_methods` rather than a fixed
order: agent, then key files, then what has to be typed. Spike 5's two findings
are both load-bearing — legacy algorithms are a per-connection switch (finding
1), and it *widens* the offer rather than replacing it because embedded servers
are narrow in different directions (finding 2).

Fifteen integration tests run against real OpenSSH and real dropbear, plus the
C ABI driven from C and the shell's own event loop driven under `offscreen`.
All three are in CI, reusing `ssh-audit`'s servers.

**One capability is missing and says so: a line break.** RFC 4335 defines the
channel request and `russh` does not implement it, so `send_break` reports
`Unsupported` and a new `supports_break` keeps the menu item disabled. Returning
success would be worse — on a console server reached over SSH a break is a real
function, and it is what someone reaches for when the console has stopped
answering.

#### Both `known_hosts` readers on offer are wrong, in the same direction

Writing our own was not the plan. It became the plan after reading the two
candidates, because both fail *silently* and both fail as "unknown host" —
which is precisely the answer an untouched file gives, so a caller cannot tell.

- **`russh::keys::known_hosts` splits the line on a single space and reads the
  second field as the key type.** An `@revoked` or `@cert-authority` line
  therefore parses as a host pattern named `@revoked` and matches nothing. A key
  the user explicitly revoked comes back as unknown and the prompt offers to
  accept it. It has no wildcard or negation matching either.
- **Tera Term's `hosts.c:check_host_key` has the wildcards and the negation**
  (`:389`, over `matcher.c`) but **no hashed entries at all** — `|1|` appears
  nowhere in the file. Debian and Ubuntu ship `HashKnownHosts yes`, so on those
  machines that is every line. Its matcher is also case-sensitive, so a host
  reached by a differently-cased name is a host it has never seen.

Ours implements OpenSSH's semantics: comma-separated patterns with `*`/`?` and
`!`, hashed entries, both markers, `[host]:port`, several files in order. Five
verdicts rather than a bool, because the frontend has five things to say and
three of them are not "do you want to continue" — and because *revoked* must
outrank *trusted*, which means reading every file to the end rather than
stopping at the first accepting line.

`ssh_config` had no adoptable reader at all. Its own trap is worth recording
because it is the opposite of every intuition: **the first value wins, not the
last.** A `Host *` block at the top of a file overrides everything below it.
Getting it backwards does not fail loudly — it applies the wrong user or key to
hosts that had a perfectly good specific block, and the user's setup "just
doesn't work". `IdentityFile` is the one exception and accumulates.

`Match exec` never matches, deliberately: resolving a config would otherwise run
an arbitrary shell command every time the connect dialog enumerates hosts.
Keywords that are not acted on are *reported* rather than dropped, because a
silently ignored `ProxyJump` is a connection to the wrong machine.

#### Telnet, and why "raw" is a first-class mode

`crates/tt-conn/src/telnet/`, 2026-08-08. Ported from `telnet.c` **and**
`ttpcmn/ttcmn.c`, which is the first thing to know: the IAC framing and the
option negotiation live in different files upstream and the framing runs first.
`ttcmn.c` unescapes `IAC IAC`, swallows the `NUL` after a `CR`, and only then
hands bytes to `telnet.c`. Reading `telnet.c` alone gives a parser that doubles
every `0xFF` and passes `CR NUL` through to the terminal.

**The mode follows the port, and that is the design.** Upstream sends its
opening negotiation only when the port is 23 (`vtwin.cpp:3666`,
`ts.TCPPort == ts.TelPort`), which reads like an oversight and is not: a
terminal server puts one TCP port on each serial line, those ports are not
telnet servers, and opening at one with `WILL TERMINAL-TYPE` puts five bytes of
protocol into somebody's console. So `Raw` is a first-class choice — an `0xFF`
in a firmware upload is data, and a client that eats it corrupts the transfer —
`Auto` is upstream's `TelAutoDetect`, and `Negotiate` is the burst.

Two upstream behaviours are reproduced that look like bugs and are decisions:
`MaxTelOpt` is 34, so `NEW-ENVIRON` and `CHARSET` are refused flat; and NAWS is
acted on **in the direction RFC 1073 does not define**, with the "did we
negotiate this" test commented out at `telnet.c:299` — that is a console server
telling a client what the equipment behind it really is, and the shell honours
it by resizing the window.

Two are deliberately absent, and both are opt-in settings upstream too, so
neither is a default difference: local echo (`ts.TelEcho`, off) and LINEMODE
(`ts.EnableLineMode`, off).

#### The local pty, and the transport that knows why it ended

`crates/tt-conn/src/pty/`, 2026-08-08. The fourth transport, and the one
upstream reaches by *not* being a terminal for it: `cygwin/cygterm` is a
separate program that forks a shell onto a pty and bridges it back over a
**loopback telnet socket**, implementing ECHO, SGA, TERMINAL-TYPE and NAWS by
hand (`cygterm.cpp:1083` onward). That existed because a Windows program cannot
fork. Here the pty is a transport like any other and the detour is deleted —
which is the first place the port has *removed* a subsystem rather than
replacing one.

Two of upstream's decisions survive, because they are about how a shell should
start rather than about Win32: a **login shell** by default (`cygterm.cfg`'s
`LOGIN_SHELL = Yes`) and an explicitly set `TERM`. The value does not survive —
upstream says `vt100`, we say `xterm-256color` — because that is a claim about
the engine behind it, and underclaiming costs the user `ls --color` and a mouse
that does nothing in `vim`.

**The trap is that both failure modes are silent, and one of them is a busy
loop.** Holding the slave end open after the fork means the master never sees
the hangup, so the shell exits and the window waits forever on nobody. And
`portable-pty`'s own reader maps `EIO` — a pty master reporting that the child
is gone — to `Ok(0)`, which is already this project's word for "the line is
quiet". Taking that would collapse the two, and because a hung-up descriptor is
*permanently* readable, the frontend's notifier would fire forever against a
read returning nothing: **a dead shell presenting as a terminal at 100% CPU.**
So the byte-level read and write are ours, on the master's descriptor, and the
adoption is for the parts that are genuinely hard — the child-side
`setsid`/`TIOCSCTTY` dance, and ConPTY in Stage 3.

It also added the one thing the seam was missing. `Transport::closing_note` is
asked once, after a disconnect and **before the transport is dropped**, because
a pty's exit status dies with the child handle. Every other transport returns
`None`: an unplugged adapter and a closed socket are what they look like. A
local shell is not, and "bash exited with status 1" is the difference between a
window that explains itself and one that just goes quiet.

**And it is the first end-to-end suite that never skips.** Serial needs the
loopback rig, SSH needs a server, telnet needs a `telnetd`; a pty needs nothing,
so 19 transport tests, 7 session tests, the C ABI case and the Qt window's own
event-loop test all run on a fresh checkout and in CI. Measured with a shell
attached: 80 ms to start and paint `bash`'s prompt, then zero CPU ticks over the
next six seconds, at 72 MB RSS.

One cost, recorded rather than fixed: `portable-pty` drags in **`serial2`, a
second serial-port crate**, unconditionally and with no feature to switch it
off.

**`telnet-audit/` exists because the unit tests cannot close the loop.** They
are byte strings derived from upstream's C, so they prove the port matches
upstream and nothing about whether upstream matches the world. GNU inetutils'
`telnetd` behind a fifteen-line inetd closes it, and proves the point
immediately: it opens with `WILL AUTHENTICATION`, `WILL ENCRYPT`, `DO XDISPLOC`
and `DO NEW-ENVIRON`, four options above `MaxTelOpt`, so the refusal path runs
before anything else in every session.

### 🔵 Stage 2 — the differentiators (3–4 months, ~20k LOC)

- **File transfer**: FFI to the vendored C, all six protocols, interop-tested
  against `lrzsz` and `gkermit`. ✅ **done through the C ABI**, 2026-08-08 —
  `vendor/ttpfile/`, `crates/tt-xfer/`, the session wiring, the ABI surface
  and the Qt dialogs. See below.
- **TTL interpreter**: native Rust, **in-process on a thread** — deletes ~2,600
  LOC of DDE glue (`ttpmacro/ttmdde.c` + `teraterm/ttdde.c`) and a whole class
  of races. Target: the 53 `.ttl` scripts in `teraterm/tests/` pass.
  ✅ **the language is ported and upstream's own macros run**, 2026-08-09 —
  `crates/tt-ttl/`: the tokeniser, the variables, the
  eleven precedence levels, the control flow, the string and integer commands,
  `send`/`wait`/`waitln`/`waitn`/`waitrecv`/`recvln`/`pause`/`flushrecv`, the
  link and the connection, the serial control lines, all sixteen transfer
  commands, the whole file family, the eleven dialogs, the eight logging
  commands, the ten checksums, the terminal's odds and ends, the environment
  and clipboard, the clock, the `send*` variants, `scp`, the passwords and the
  regex family — **every one of the 231 reserved words has an arm**, and
  `crates/tt-ttl/tests/scripts.rs` runs upstream's own 53 macros against a
  golden transcript each — including the `ttpmacro` command line, so the three
  scripts that check their own answers now do it against real arguments. See
  below. `crates/tt-macro/` is the host that is a terminal rather than a
  recorder, 2026-08-09, including the transfers, `connect`/`cygconnect` and the
  serial control lines. **And the window runs one**, 2026-08-09 — the macro
  half of the C ABI, and `shell/src/Macro.cpp` answering what it asks: Control
  > Run macro, `/M=` on the command line, and the dialogs. See below. What is
  left is the commands listed at the bottom of `tt-macro/src/host.rs` and of
  `shell/src/Macro.cpp`, each of which wants a subsystem this port has not
  built. **The way in that is not a person clicking a menu now exists**: see
  the `ttctl` bullet above.
- **Lua via `mlua`** over the same `ScriptHost` command table (~500 LOC glue).
- `ttctl` JSON-RPC control socket replacing DDE. Keep a `ttpmacro script.ttl`
  CLI entry point so existing shortcuts and `.bat` wrappers keep working.
  ✅ **done**, 2026-08-09 — `crates/tt-ctl/`: the wire, the address, the
  dispatch, the two clients; the ctl half of the C ABI; and
  `shell/src/Control.cpp`, so a window binds one on startup and answers it.
  See below. ✅ **both command lines are parsed, and `ttermpro`'s opens a
  window**,
  2026-08-09 — `crates/tt-ttl/src/cmdline.rs` for `ttpmacro`'s and
  `crates/tt-config/src/cmdline/` for `ttermpro`'s, the second including TTSSH's
  plugin half; `tt-session`'s `open.rs` for what a line says to open, the
  command-line half of the C ABI, and `shell/src/main.cpp`, so
  `sterna /ssh /auth=publickey myhost` works as the shortcut it was converted
  from did. **A macro's `connect` opens one too**, 2026-08-09, through the same
  two parsers plus CygTerm's for `cygconnect`.
- **Settings schema + generated dialogs**, first pass. ✅ **done, first pass** —
  `crates/tt-config/` (60 settings: 39 for the terminal, 2026-08-08, plus the
  connection, serial, log and transfer ones the command line writes into,
  2026-08-09), the map onto a running terminal in `tt-session`, the schema as
  data over the C ABI, and a Qt dialog that builds itself from it. What remains
  is the *rest of the settings*, which is a line and a citation each. See below.
- `TERATERM.INI` and `KEYBOARD.CNF` readers. ✅ **`TERATERM.INI` done**, held
  against a real Win32 rather than against a reading of the documentation;
  `KEYBOARD.CNF` is an INI and reads with the same layer.

#### The settings schema, and an oracle for a file format

Started with the leverage point rather than with file transfer, because
`PLAN.md` has said since Stage 0 that this is "the difference between the
project finishing and not" and that it should be built while morale is high.

**The INI layer needed an oracle of its own, and the existing one could not
help.** Upstream calls Win32's `GetPrivateProfile*` directly — `ttpset/ttset.c`
and `common/inifile_com.cpp`, with no portable implementation anywhere in the
157k lines — so the headless oracle stubs those calls and takes every default,
which is exactly right for comparing *parsers* and useless for comparing *file
handling*. And "bug-compatible with `GetPrivateProfile*`" is a claim that has
to be true of a file the user already has: the first thing this code does on a
new machine is read their `TERATERM.INI` and write it back.

So `ini-audit/`: a battery of 104 cases as **data**, a mingw-w64 exerciser
compiled against the real API, run under Wine, and the answers recorded.
`crates/tt-config/tests/win32.rs` puts the same battery to the Rust
implementation and diffs. **98 match byte for byte**; the six that do not are
in `ini-audit/divergences.txt` with a reason each, and the gate fails in both
directions so a reason cannot outlive the behaviour it describes.

It corrected the plan on its first run. **This document said "no quote
stripping" and that is wrong** — a matched pair, single or double, is
discarded, which MSDN documents. Four more findings are in `CLAUDE.md`, and one
of them is the same shape as every settings trap already there: **`GetOnOff` is
default-biased** (`ttset.c:344`), so `Key=1` means *on* for a setting that ships
on and *off* for one that ships off. It also reads into a four-byte buffer, so
`offline` is `off`.

Wine is not Windows, and the writeup says so rather than hoping: two recorded
answers — that a write rewrites every line ending in the file, and normalises
`[ s ]` — are Wine's alone and are deliberately *not* reproduced. Re-run the
battery on Windows in Stage 3; `exercise.exe` compiles there natively.

**The schema itself is one line per setting** and generates the struct, the
defaults, the reader, the writer, name-addressed accessors and a metadata
table. `FIELDS` is the point of it: the dialog builds itself from that table,
`setsetting`/`getsetting` resolve through it, and the docs are printed from it,
so the list exists exactly once. A dialog *generated as C++* would be a second
copy to keep in step across two build systems; one that reads the metadata over
the C ABI has nothing to keep in step — which is a better answer than the one
this document originally sketched.

Every default carries the `ttset.c` line that proves it, because four of them
are `else` branches or flag words and each already has a trap written about it.
The generated file is committed and a test fails when it is stale, the same
arrangement as `tt-ffi`'s header.

39 settings of roughly 600. The machinery was the expensive part; adding a row
is a line and a citation.

#### And then it was wired, which is where the schema paid for itself

`tt-session/src/settings.rs`, the C ABI's settings surface and
`shell/src/SettingsDialog.cpp`, 2026-08-08. A setting typed into the dialog now
reaches the terminal, the painter and the file.

**The dialog holds no list of settings.** It walks `tt_settings_field` — a row
per setting, carrying the page, the INI section and key, the kind, an int's
bounds, the `.lng` label and the citation for the default — and builds a tab per
page and a widget per kind. This document originally sketched *generating* the
dialog as C++, and that is worse: a second copy of the list, living in the other
build system, that every schema change has to be pushed through. A table read at
runtime leaves nothing to keep in step, which is the difference between "adding
a setting is a line in a text file" and "adding a setting is a build".

Three decisions worth recording, each of which looks like a bug from one side:

- **Applying settings overwrites modes the host set**, because upstream keeps no
  second copy of them: `vtterm.c` reads `ts` at the point of use, so DECBKM
  assigns `ts.BSKey`, SRM assigns `ts.LocalEcho` and LNM assigns `ts.CRSend`.
  `Vt::set_config` is `CVTWindow::SetupTerm` (`vtwin.cpp:1383`) and refreshes
  exactly those — while deliberately leaving `LFMode` and `AcceptWheelToCursor`,
  which upstream *does* keep separately and `SetupTerm` does not touch.
- **The dialog writes only what changed.** Applying every field would pin all 39
  settings into the user's file the first time it was opened, and a pinned
  setting stops following upstream's default for ever.
- **The size resizes the *window*, not the grid** — the same path a telnet NAWS
  resize takes, because the view fits the terminal to the space it has.

And it found four things, all of the same family as every other settings trap:

1. **`TermWidthMax` is 1000 and `TermHeightMax` is 500**, one line apart in
   `tttypes.h`. The wrong one had become `tt-grid`'s column cap *and* the
   schema's documented range, so a 640-column terminal would have silently been
   500.
2. **`TerminalID` is the one enumerated setting compared with `strcmp`**, not
   `_stricmp` — and `TermIDGetID` never fails, so `TerminalID=vt320` in the
   wrong case is a VT100. The schema grew an `enum_exact` for it, and two names
   it had been missing entirely: `VT220`, and upstream's lower-case `dumb`.
3. **`ttset.c:615` bounds a size without clamping it**: at or below the floor
   takes the default, above the ceiling takes the ceiling. `TerminalSize=0,0` is
   80x24, and a schema that clamped would have given a one-column window.
4. **`ScrollBuffSize` is the whole buffer, page included** (`buffer.c:641`), and
   the grid was using its scrollback *depth* as the ceiling on the page height —
   two different upstream settings sharing one field. With the history switched
   off that made the terminal one row tall. Fixed by separating them, which also
   fixed line numbering with no scrollback at all: lines are counted off the
   page whether or not anything keeps them, or a held line number means nothing.

#### File transfer, and what a driver found that a spike could not

`vendor/ttpfile/`, `crates/tt-xfer/`, `tt-session/src/xfer.rs`, the ABI surface
and `shell/src/XferDialog.cpp`, 2026-08-08. `File > Send file...` moves a file
from the window's own connection to `rz` and back.

**The vendoring is real now**, which is a first for this tree: 33 files and
11,568 lines copied verbatim at a named upstream revision, every one carrying
its BSD notice, with `sync.sh --check` as the only guard they have — nothing
diffs them against anything, because they *are* the implementation.
`ATTRIBUTION.md` records it as the one piece of Tera Term the distribution
contains. `winshim/` moved out of `oracle/` on the way: a shipped crate must
not reach into the test harness for its build.

**`tt-xfer` is the host, not the protocols.** Upstream's equivalent is
`filesys_proto.cpp` — the same three vtables plus a modal dialog, a message
pump and a file-scope global. Here the vtables are 700 lines of C and the loop
is in Rust, and the comm side is written against `ttcmn.c` rather than
invented, because three places in the protocol sources reach past the vtable
into `TComVar` and would notice.

The spike drove the same C from a file descriptor. **A transfer over the
*terminal's own connection* is a different problem**, and it is the one that
found things:

- **`Insert1Byte` puts the byte at the front of the *receive* buffer.** The
  header comments it as "send one byte" and `CommInsert1Byte` does the
  opposite; it exists so ZMODEM auto-start can push back the trigger the
  terminal already swallowed. `xfer/`'s spike had it backwards and nothing
  noticed, because nothing there ran an auto-start mode.
- **`Read1Byte` must be the *raw* one, and so must `BinaryOut`.** The
  difference is the telnet codec, which upstream runs on the way past because
  one buffer serves the terminal and the transfer both. `tt-conn` has already
  done it, and doing it twice eats one `0xFF` of every escaped pair — fatal on
  every binary transfer to a terminal server, invisible on text.
- **No protocol closes the received file.** XMODEM's EOT arm sets `Success`,
  ACKs and returns FALSE; `Destroy` frees its state without touching the file.
  Upstream gets away with it because `ProtoEnd` tears the whole `FileVar` down
  a moment later — a library cannot, because the caller is entitled to report
  "done" and let the user open the file. The symptom was a 4,106-byte payload
  arriving as exactly 4,096: one stdio buffer short, which reads as a truncated
  transfer and is not one.
- **`FTSetTimeOut` and ZMODEM's 500 ms cancel timer are one timer.** Both are
  `IdProtoTimer` and the cancel *deliberately* displaces the read deadline. Two
  clocks looked tidier and meant a cancelled transfer waited out a ten-second
  timeout before it noticed.
- **ZMODEM's own verdict on a cancel is `Success`**, because the cancel
  provokes a `ZFIN` and `zmodem.c:1047` sets it on any `ZFIN`. Not an answer to
  give the person who pressed cancel.
- **The protocols throttle progress to ten updates a second**
  (`zmodem.c:197`), so a transfer that finishes in under a tenth of a second
  finishes having reported nothing. A frontend must not read `bytes == 0` as
  "not started" — a test here did, and was wrong.

`Transport` grew one method for this, `link_kind`, which is the exception its
own documentation argues against. It earns it: `cv->PortType` picks the timeout
set, caps ZMODEM's block size and decides whether Kermit quotes the eighth bit,
and the network branch means *no timeout at all* — right for a socket that will
notice a dead peer, wrong for a pty, where it is a transfer that hangs for ever.

**The window needed a second timer, and it is the only other one.** Everything
else in the shell runs off a descriptor; a transfer cannot, because the
protocols retry by *timeout* and a quiet line produces no wakeup at all. The
progress dialog is also modeless where upstream's is modal — not a style
preference, since the transfer is driven by the window's own event loop and a
dialog that blocked it would block the transfer it is showing.

Twelve interop cases against `lrzsz` and `gkermit` moved from a shell script
into `cargo test`, seven session-level cases, one in `abi.c` that sends a file
to `rz` from C, and three in `shell/tests/xfer_test.cpp` driven by Qt. B-Plus
and Quick-VAN still have no counterparty anywhere and stay best-effort, which
is also upstream's position — the protocol list says *untested* beside them
rather than letting somebody find out.

Two portability findings came out of building it on the *other* container's
compiler, both recorded in `CLAUDE.md`: upstream leans on `<windows.h>` for
`<stdlib.h>` and for the `SetTimer`/`KillTimer` declarations, which GCC 13
forgave and GCC 14 does not, and `ttcstd.h`'s `char8_t` guard is inverted so
the C++ has to be pinned at `gnu++17`.

#### The macro language, and the three decisions inside it

`crates/tt-ttl/`, 2026-08-08. `ttl.cpp`, `ttmparse.cpp` and `ttmbuff.c` — about
9,200 lines — ported far enough that a macro of arithmetic, strings and control
flow runs to completion with no terminal attached at all.

**The DDE deletion is the point, and it is structural rather than a line
count.** Upstream's engine is a second process reaching the terminal by message
passing; here it is a crate and the terminal is behind one trait. What that
buys is not 2,600 fewer lines but the disappearance of a state machine:
upstream's `wait` *cannot* block, because the macro and the window share a
thread, so it parks the macro in `TTLStatus` and lets the message loop drive it
back to life. The interpreter here has its own thread, a host call may simply
block, and there is no state for waiting — only for having finished. Every
`wait`-family command becomes an ordinary function that returns when it is done.

**`ScriptHost` is wide and shallow on purpose** — one method per thing a command
needs from outside, each with a refusing default. A host that implements half of
it is useful, and the other half answers "Unknown command" rather than
pretending to work. It is also what makes the tests possible: they are TTL source
and an expected output, with no terminal in the loop.

**File loading is the host's, including the encoding.** Upstream's `LoadFileU8W`
sniffs a BOM and falls back to the ANSI codepage, and four of the 53 test scripts
(`code_utf8.ttl`, `code_utf8-bom.ttl`, `code_utf16le-bom.ttl`, `code_cp932.ttl`)
exist because that is a real decision with real files behind it. It does not
belong in a parser.

Faithfulness cost the usual. Six behaviours are reproduced because scripts were
written against what TTL *does*:

1. **A TTL string is bytes and is a C string.** `#255` is a legal escape, so it
   need not be UTF-8 — and must not be, since `send` puts it on the wire
   unchanged. It also stops at its first NUL, which is not trivia: `strspecial`'s
   `\0` truncates the string it is in, and `code2str $01000041` is one byte long.
   Doing the cut in the variable store makes that one rule instead of two
   special cases.
2. **A string operand short-circuits the whole expression grammar.** Every
   precedence level returns immediately if its left operand is not an integer,
   so in `a + b` with `a` a string the `+ b` is never looked at and the caller
   reports a syntax error. That is why TTL has `strconcat` and no `+`.
3. **An expression cannot build a string, only name one** — upstream returns the
   variable id in the same `int` it returns numbers in.
4. **A block is skipped by executing it, not by seeking past it.** A counter
   suppresses the effect line by line, so a syntax error in a branch that never
   runs is still an error. The subtle consequence: `ElseFlag` increments
   *`EndIfFlag`* when it meets a nested `if`, because that `if`'s own `else`
   must not end the outer skip.
5. **`for` steps its variable *towards* the end value**, so a loop counts down as
   readily as up and `for i 3 3` runs exactly once.
6. **`strsplit` with no count answers 10 having stored 9.** The loop runs one
   field past the limit to discard the remainder, and `result` is the count it
   reached.

Three upstream out-of-bounds accesses are **not** reproduced, because none has
an observable result and all three are memory-safety bugs: `strtrim` indexes a
256-byte table with a signed `char`, so a trim character above 0x7F reads before
the start of it; `strsplit` reads one past its nine-element token array, handing
the garbage to a lookup for `groupmatchstr10` that does not exist; and
`GetFactor` returns a label's type beside an uninitialised value, which every
caller rejects on the type before looking. None is worth reporting upstream on
its own — they are not reachable as anything but a wrong answer nobody can see —
but they are recorded here in case a fourth turns up that is.

**The wait family is where the thread pays for itself.** Upstream's `wait` sets
`TTLStatus = IdTTLWait` and returns; the window's message loop calls `Wait()`
on every timer tick, so the command, the answer, the `inputstr` handling and
the timeout arm each live somewhere else. Here each is a function that reads
bytes until it is done, and four states became four loops. The matchers
themselves came over unchanged — they are in `ttmdde.c`, the *macro* side of the
DDE conversation, not in the terminal.

Three of their behaviours are upstream's and none is guessable: the pattern
scan runs from the tenth string down to the first and overwrites its answer, so
the **lowest-numbered pattern wins a tie**; a `waitln` that matched but never
saw the end of its line reports **0**, because the second phase's timeout
overwrites the result; and `waitrecv` succeeds only when its window is *full*,
with the position measured inside the last `len` bytes rather than from the
start of the stream.

It also turned up a sixth upstream defect, in `ttpmacro` this time rather than
the VT engine. `waitn` suppresses the received-line buffer's clear-on-newline so
that it can count bytes across line breaks, and restores it **only on the
success path** — `ttmmain.cpp`'s timeout arm sets `result` and `inputstr` and
never calls `ClearWaitN`. So after a `waitn` that times out, every later
`inputstr` in that run accumulates across lines instead of holding one. It is
reproduced, and it is deliberately *not* in `docs/upstream-bugs.md`: that file
holds defects proven by running two engines against each other, and this is a
reading of the source. Demonstrate it against a real `ttpmacro.exe` in Stage 3
before filing it.

#### The session, and where the process boundary was

`crates/tt-ttl/src/sesscmds.rs`, 2026-08-08. `connect`, `cygconnect`,
`disconnect`, `testlink`, `unlink`, `closett`, `setsync`, the six serial
control-line commands and all sixteen transfer commands.

Upstream these are the *thin* ones: `TTLCommCmd`, `TTLCommCmdInt` and
`TTLCommCmdFile` are three or four lines each, hand a one-byte opcode to
`SendCmnd`, and every scrap of behaviour is on the other side of the DDE
conversation in `teraterm/ttdde.c`. The thinness is the trap, and it hides
three things that are visible from a macro:

- **`SendCmnd` owns the link check**, so a command whose own body never
  mentions `Linked` still fails with `ErrLinkFirst`. It also runs *after* the
  arguments are parsed, so `sendbreak junk` is a syntax error where `send 'x'`
  with no terminal is a link error. Both orders are upstream's and both are
  reproduced.
- **`IdTTLWaitCmndEnd` and `IdTTLWaitCmndResult` are not the same wait.**
  `sendfile` takes the first, so it blocks until the file has gone and then
  writes **no `result` at all** — a macro that tests `result` after a
  `sendfile` is reading whatever the command before it left there.
- **`DDE_FNOTPROCESSED` reads to the macro as success.** It is what the
  terminal answers when the port is not serial, so `setdtr`, `setrts`,
  `setbaud` and `setflowctrl` are all silent no-ops over SSH rather than
  errors. The host declining quietly is that shape.

The one thing not reproduced is the process boundary. `TTLConnect` either
tells an existing Tera Term to connect or spawns a fresh `ttermpro.exe` and
links to it over DDE; in-process there is nothing to spawn, because the
terminal is the caller. What a macro observes is unchanged — `result` is read
back off the link and the connection afterwards, which is exactly the
three-value table `testlink` documents.

The XMODEM option folds in two different directions and neither is guessable:
on send only 1K survives and **everything else becomes CRC**, including a
literal 1 for checksum, because the checksum-or-CRC choice belongs to the
receiver; on receive 1K *means* CRC — upstream comments the arm "for
compatibility" — and anything unrecognised falls back to checksum rather than
to CRC.

**A seventh upstream defect**, and like the `waitn` one it is a reading of the
source rather than two engines disagreeing, so it is not in
`docs/upstream-bugs.md` either. `getmodemstatus`'s `result` is **always 0**,
including when it failed. The documentation promises 1 on failure and
`TTLGetModemStatus` has the arm that would set it, but the arm fires on a
non-zero return from `GetTTParam`, which returns `ErrLinkFirst` or nothing else
— and the `ErrLinkFirst` case was already taken three lines earlier. When the
terminal declines, `GetTTParam` leaves the caller's buffer alone and returns 0;
the buffer was `memset` to zero, `atoi("")` is 0, and the macro is told all
four control lines are low. `GetTTParam`'s own comment at `ttmdde.c:1067` says
the transaction failing *should* be an error and the `return 0` under it says
it is not. Reproduced; demonstrate it against a real `ttpmacro.exe` in Stage 3
before filing.

#### The file family, and the Windows underneath it

`crates/tt-ttl/src/files.rs`, `filecmds.rs` and `pathcmds.rs`, 2026-08-08.
Thirty-three commands: the sixteen handles and everything they do, the path
operations, the directory walk, and `basename`/`dirname`/`makepath`.

**None of it goes through `ScriptHost`.** The handle table is the macro's own
state — four file-scope arrays in `ttl.cpp` — and reading bytes out of a file
is not the decision that loading a *macro* is, where a BOM and a fallback
codepage have to be sniffed. The one seam that was needed is `format_time`,
because a wall clock is not in the standard library: no time zones and no
`strftime`, and this crate still has no dependencies. The default answers in
UTC by Hinnant's `civil_from_days`; a frontend that knows the user's zone
overrides it, and `getdate`/`gettime` will want the same seam.

Three answers are the platform's rather than upstream's, each argued at the
command:

1. **File attributes are a Win32 bit field** and `getfileattr` hands
   `GetFileAttributes`'s return value straight to `result`, so the values a
   script tests are `FILE_ATTRIBUTE_*`. `READONLY`, `HIDDEN` (a leading dot,
   which is this platform's own word for the same idea), `DIRECTORY` and
   `NORMAL` are answered from a POSIX stat; the NTFS bookkeeping bits never
   set. `setfileattr` can act on `READONLY` and accepts the rest silently,
   which is what `SetFileAttributes` does with a bit the filesystem does not
   keep.
2. **`basename` and `dirname` use this platform's separator.** Upstream's
   `DeleteSlash` strips backslashes only, so a literal port would answer
   differently for `/a/b/` and `c:\a\b\` for no reason but the character. The
   documented examples all hold with `/` for `\`; the two places the answers
   differ are at the root and on a trailing separator, and both are POSIX's.
3. **The same-file tests compare exactly**, where upstream uses `_stricmp`,
   because here two spellings that differ in case are two files.

`FindFirstFile`'s glob *is* reproduced, Win32 trivia included: `*.*` matches
every name whether or not it has a dot — an 8.3 leftover, and the reason
`findfirst` with an empty pattern works at all — and the walk yields `.` and
`..`. A script that loops over `*.*` was written against both.

`GetFileNamePosU8` rejects any path containing a colon (`ttlib_static_cpp.cpp:741`),
which makes `GetAbsPath` fail and the command report a path error. Not
reproduced: a colon is a drive separator and an alternate-data-stream marker on
Windows and an ordinary character here, so it would make `/tmp/a:b` unopenable
for no reason a user of this port could discover.

Two behaviours that look like bugs and are kept:

- **`filestrseek`'s matcher is not `wait`'s.** `wait` backs off to the longest
  prefix of the pattern that is also a suffix of what it has seen; this one
  backs off to nothing and then takes the current byte as a possible first
  character, so it cannot find a pattern that overlaps itself — `aab` is not
  found in `aaab`, though it plainly is there.
- **`fileconcat` calls a missing source a success.** It opens it with
  `OPEN_EXISTING`, and when that fails it skips the copy loop with `result`
  still 0.

**`filelock` is the one command in the port with deliberate divergences**, and
they are upstream's arithmetic rather than a preference. `timeoutI` is never
initialised, so a bare `filelock fh` — the form the documentation calls the way
to wait for ever — spins for however long a stale stack slot says; and the loop
then compares against `timeout * 1000` having already multiplied by 1000 when
it assigned it, so `filelock fh 5` waits five *million* seconds. The line above
both reads `timeout = -1;  // 無限大`, and the documentation's table agrees with
that intent. Implemented as documented: no argument waits for ever, 0 returns at
once, N waits N seconds. Reproducing an uninitialised read is not possible in
safe Rust and there is nothing to be faithful to; the 1000× is the same
expression and goes with it. Note also that a POSIX lock is advisory where a
Windows one is mandatory, so `filelock` keeps well-behaved programs out and
nothing else.

**Three more upstream out-of-bounds accesses**, all in the handle table, none
reproduced — same class and same reason as the three in `strtrim`, `strsplit`
and `GetFactor`. `HandleGet` tests `_countof(FHandle) < fhi` where it means
`<=`, so handle 16 reads one element past the array; `HandleFree` tests nothing
at all, so `fileclose 99999` writes `INVALID_HANDLE_VALUE` at an index the
script chose; and `FPointer[fhi]` is unchecked the same way in `filemarkptr`
and `fileseekback`. That makes six found by reading, and the tally is the point:
`ttl.cpp` bounds-checks its handle arrays in about half the places it indexes
them.

#### The dialogs, and one word the table had lost

`crates/tt-ttl/src/dlgcmds.rs`, 2026-08-09. `messagebox`, `yesnobox`,
`statusbox`, `closesbox`, `bringupbox`, `listbox`, `inputbox`, `passwordbox`,
`filenamebox`, `dirnamebox` and `setdlgpos` — one host method each, every one
of them blocking the way upstream's `DoModal` does.

**The seam is the same shape as `wait`'s and for the same reason.** Upstream's
dialogs are `ttpmacro.exe`'s own windows on the thread the macro runs on, so
each is an ordinary modal call and the interpreter simply waits. Here the
window belongs to the frontend, which answers by spinning its own event loop —
so the interpreter has to be off the UI thread, which it already had to be.

Three shapes in the family are not guessable from the command names:

- **Closing a dialog is not cancelling it.** Every one of these windows puts a
  "halt the script?" confirmation in front of its close button
  (`msgdlg.cpp:227`), so the close answer arrives only when the user has agreed
  to stop — which is why it ends the macro and Escape does not. The codes
  differ per dialog for a reason that is pure Win32 accounting: a plain
  message box has no No button, so its close path spends `IDCANCEL` where the
  yes/no one spends `IDCLOSE`.
- **`filenamebox` and `dirnamebox` guard on `inputstr` still being a string**
  and skip the dialog entirely if it is not — a step further than the silence
  every other `inputstr` writer has. Unreachable from a macro, since TTL will
  not retype a variable, and reproduced anyway.
- **`inputbox`'s third argument is read twice.** It is tried as a string and,
  on a type mismatch, the line is rewound and the `<special>` arm gets it. That
  is what makes `inputbox 'a' 'b' 1` the flag rather than a default of `"1"`.

**`filenamebox` was missing from the reserved-word table.** One name out of 214,
dropped when the table was transcribed from `ttmparse.cpp:245`, and invisible
because an unknown word is not an error — it is read as a variable, so the
command reported a syntax error rather than "unknown command". Found by
diffing the two tables mechanically, which is now worth doing after any
transcription of an upstream list.

Three more upstream defects, all found by reading and none in
`docs/upstream-bugs.md` for the usual reason:

1. **`inputbox` and `passwordbox` copy an uninitialised stack buffer into
   `inputstr` when the dialog is dismissed with Escape.** `CInpDlg` does not
   override `OnCancel`, so Escape reaches `TTCDialog::OnCancel`
   (`tmfc.cpp:312`) and ends the dialog with `IDCANCEL`; `TTLInputBox`
   (`ttl_gui.cpp:353`) declares `wchar_t input_string[MaxStrLen]` with no
   initialiser, tests only for `IDCLOSE`, and hands the buffer to `SetStrVal`
   on every other path. `TTLGetPassword` and `TTLGetPassword2` initialise the
   same buffer, which is what makes this an oversight rather than a
   convention. Not reproduced — the empty string is what the documentation
   implies and what the neighbouring commands produce.
2. **`filenamebox`'s two flag sets are each other's** (`ttl_gui.cpp:180`). A
   non-zero `<dialogtype>` opens the Save dialog with `OFN_FILEMUSTEXIST`,
   which is Win32's "only an existing name will do" and stops the user naming
   a new file; zero opens the Open dialog with `OFN_OVERWRITEPROMPT`, which an
   Open dialog has nothing to do with. Implemented as documented: a Save dialog
   that cannot save is not a behaviour a script can be written against.
3. **`listbox`'s `listboxsize=` test compares five characters**
   (`ttl_gui.cpp:486`): `_wcsnicmp(..., L"listboxsize=", 5)`, where the length
   should be 12. So any keyword starting `listb` enters that arm and must then
   parse as a size — which means a misspelling like `listbee=60x20` silently
   works and `listbee` alone is a syntax error rather than being tried as a
   selection index. Reproduced; it is harmless, and no real keyword collides.

#### The logging commands, and an error that is thrown away

`crates/tt-ttl/src/logcmds.rs`, 2026-08-09. `logopen`, `logclose`, `logpause`,
`logstart`, `logwrite`, `loginfo`, `logrotate` and `logautoclosemode`.

The log is the *terminal's*, not the macro's — upstream sends all eight over
DDE to `filesys_log.cpp`, which is the same log `File > Log` opens — so six of
them are the thin `TTLCommCmd*` shapes and carry `SendCmnd`'s link check
without mentioning it. `tt-session` already has the log (Stage 1); wiring these
methods to it is the frontend's job, not this crate's.

Four behaviours that are not what the command names suggest:

- **`logopen` reports success as 0.** The terminal answers the character `1`
  and `TTLLogOpen` inverts it, which is documented and is the opposite of every
  other command in the file.
- **`loginfo` answers -1 when nothing is logging**, and otherwise a five-bit
  flag word of what `logopen` was *given* — not what the log is doing. Pausing
  does not show up, and neither does the timestamp type. It travels as one
  character, `'0' + flags` (`filesys_log.cpp:856`), which is also how it
  carries a negative without a sign.
- **`logrotate` has no end-of-line check**, alone in the file, so
  `logrotate 'halt' and then some` runs and ignores the tail. Its keyword is
  also the one enumerated argument here compared with `strcmp`, so `'Halt'` is
  a syntax error, and its size suffix is an uppercase `K` or `M` only.
- **`logautoclosemode` is not `logautoclose`** and does not close the log when
  the *connection* goes: it closes it when the **macro** ends, off the DDE
  conversation disconnecting (`ttdde.c:1340`), and clears itself at the same
  time so it lasts exactly one run.

**A tenth upstream defect, and this one is reproduced.** `TTLLogOpen`
(`ttl.cpp:3243`) never tests the error from its three *mandatory* arguments.
It accumulates into a sticky `Err` like every command around it, but the first
test of that variable is after the *fourth* argument, the label the optional
arguments jump to checks only that the filename is non-empty and that nothing
is left on the line, and then `Err = GetTTParam(...)` overwrites whatever was
in it. So `logopen 'f' 1` — one argument short of the documented three —
opens a log, with `append` reading as the 0 its array was initialised with.
Reproduced, and it was the closest call in the file: a macro that has been
opening a log with two arguments has been working, the flags it silently gets
are the documented defaults, and turning it into a syntax error would break
that script for nobody's benefit.

**And a seventh out-of-bounds read**, same class as the six in the handle table
and `strtrim`: `logrotate 'size' ''` evaluates `Str2[len-1]` with `len` zero
(`ttl.cpp:3179`), reading the byte before a 512-byte stack buffer, and hands
the result to `isdigit` as a signed `char` — so a size argument ending in a
byte above 0x7F has `strtrim`'s problem as well. Not reproduced; an empty
argument is a syntax error here, which is what the non-digit arm would have
said anyway.

That is **twelve** found in `ttpmacro` by reading: `waitn`'s timeout arm,
`getmodemstatus`'s always-zero result, `logopen`'s discarded error,
`filenamebox`'s swapped flags, `inputbox`'s uninitialised buffer, and seven
out-of-bounds accesses. Three more arrived with the environment and the clock
below, for **fifteen**: `getspecialfolder`'s always-1 result, the NULL it
hands `strncpy_s` for a folder type it does not know, and `gettime`'s
timezone argument leaking into the process environment when the line has
trailing junk. Six more came with the passwords and one with the regular
expressions, for **twenty-two**, and one with the command line, for
**twenty-three** — see those sections. The last is the only one that is
reachable from outside the macro: every other is a wrong answer or a read of
memory nobody chose.

#### The checksums, and the terminal's odds and ends

`crates/tt-ttl/src/cksumcmds.rs` and `termcmds.rs`, 2026-08-09. Ten checksum
commands and seventeen thin ones.

The checksums are the only family in the language with **no host at all** —
twenty lines of arithmetic each, with the C for `crc32` printed verbatim on its
own documentation page, which is as close to a specification as TTL gets. Three
things fall out of transcribing them:

- **`crc16` is not the CRC-16-CCITT its comment claims.** The reflected 0x8408
  with an inverted output is CRC-16/X-25; CCITT-FALSE runs the other way and
  does not invert. The arithmetic is reproduced and the name is upstream's.
  Both CRCs are checked against the standard `"123456789"` vectors, which is
  the first oracle this port has had that is not Tera Term itself.
- **The answer is stored in a signed integer**, so any `crc32` above
  0x7FFFFFFF is negative in the variable. The documentation's own example
  prints it with `sprintf '0x%08X'`, which reinterprets the bits and hides it.
- **A zero-length file reports failure.** `CreateFileMapping` of an empty file
  returns `ERROR_FILE_INVALID`, so upstream's `goto error` runs, `result` is -1
  and the variable is untouched. Reproduced: a script that tests `result` is
  entitled to the same answer for the same file on either engine.

The seventeen thin ones — `beep`, `callmenu`, `changedir`, `clearscreen`,
`enablekeyb`, `loadkeymap`, `restoresetup`, `setdebug`, `setecho`, `settitle`,
`gettitle`, `showtt`, `show`, `getttpos`, `getttdir` and the two serial delays
— are where reading `ttl.cpp` alone is least sufficient, since every one of
them is two lines there and all the behaviour is in `ttdde.c`. Four things that
are not visible from the macro side:

1. **Three of these arguments are switched on their first character.**
   `CmdShowTT` (`ttdde.c:847`), `CmdClearScreen` (`:593`) and `CmdSetDebug`
   (`:834`) each read `ParamFileName[0]` of the decimal rendering, so
   `showtt 100` is `showtt 1`, `clearscreen 25` is `clearscreen 2`, and every
   negative value is the same `'-'` arm — which `showtt` has and the other two
   do not. A value with no arm does nothing and reports nothing. Reproduced, in
   the `from_code` on each enum so that exactly one place knows it.
2. **`changedir` and `setdir` move different directories**, and the names are
   the wrong way round for guessing: `changedir` is the *file transfer*
   directory that `sendfile` and `zmodemrecv` resolve against, `setdir` is the
   macro's own working directory.
3. **`show` is the macro's window and `showtt` is the terminal's.** Upstream's
   `show` is local rather than a DDE command for exactly that reason, and it is
   three-way on the sign where `showtt` is a ten-way table.
4. **`beep` and `getttdir` need no terminal**, alone in the family — both run
   inside `ttpmacro.exe`. `beep` also validates its argument properly, which
   the character-switched three do not: an unknown sound is `ErrSyntax`.

`getttdir` needs no host method either. Upstream reads
`GetModuleFileName(NULL)`, which is the running executable, and
`std::env::current_exe` is that exactly — a frontend gets its installation
directory and a test binary gets its own, which is the same answer upstream
would give in the same position.

#### The environment, the clipboard and `exec`

`crates/tt-ttl/src/envcmds.rs`, 2026-08-09. Twelve commands, eleven of which
run inside `ttpmacro.exe` rather than over DDE — `gethostname` is the odd one
out and asks the terminal.

Four of the twelve have no Linux counterpart to be faithful to, and what each
one does about that is the interesting part:

- **`exec`'s `<show>` is validated and then dropped.** It is `STARTUPINFO`'s
  `wShowWindow`, asking the *child* to open hidden or minimised, and there is
  no such request here. An unrecognised word is still `ErrSyntax`, because
  that is where a typo in a working script gets caught. The command line is
  split with `CommandLineToArgvW`'s rules and the first word run directly:
  `CreateProcess` runs a program and not a shell, so a script that wanted a
  pipe already had to write `cmd /c` and will have to write `sh -c` here.
- **`getspecialfolder` answers the nine XDG has and admits to the seven it
  does not.** `Favorites`, `NetHood`, `PrintHood`, `Recent`, `SendTo` and
  `AllUsersDesktop` are the empty string — which is also what upstream gives
  for a name it does not recognise.
- **`getipv4addr`/`getipv6addr` are `ScriptHost` methods** whose default
  answers "cannot retrieve", `result` -1. Enumerating interfaces is the one
  thing in the family that needs more than `std`, and `tt-ttl` has no
  dependencies; -1 is upstream's own answer when `WSAStartup` fails or
  `GetAdaptersAddresses` is missing, so the default is a state a real Tera
  Term can be in. The IPv6 rendering when a host does supply them is
  upstream's and is *not* RFC 5952: `myInetNtop` (`ttl.cpp:2499`) prints all
  sixteen bytes as `%02x` with a colon after every second one, so `::1` is
  `0000:0000:0000:0000:0000:0000:0000:0001`.
- **`outputdebugstring` is behind a Cargo feature that is off**, because
  `OUTPUTDEBUGSTRING_ENABLE` is commented out at `ttmparse.h:36`. Compiled
  out, upstream does not have the *reserved word* either, so a macro using it
  fails as a syntax error on a line that reads perfectly well — the trap
  `filenamebox` already cost four commits. Accepting it here would be this
  port quietly having a command Tera Term does not.

And one decision that is not a portability question at all: **`getver`
deliberately answers Tera Term's version, not Sterna's** — 5.7, on the
`ScriptHost` so a frontend can say otherwise. Its whole use is feature
gating (`getver v '4.56'`, then `if result >= 0`), so a version of its own
would fail every gate ever written and silently take the old branch.

**A thirteenth upstream defect, and a fourteenth.** `GetSpecialFolder`
(`ttmlib.c:249`) throws away `GetSpecialFolderAlloc`'s return and returns a
literal 1, so the documented "0 when the command fails" never happens — and
for an unrecognised folder type the same line hands `strncpy_s` a **NULL
source**, which is the CRT's invalid-parameter path rather than an empty
string. `result` is always 1 here too, because a script branching on it must
branch the same way; the NULL is not reproduced because there is nothing to
reproduce it with.

Three more laxities are reproduced rather than reported, because a script
could depend on them: `var2clipb` and `outputdebugstring` never check for end
of line, so `var2clipb 'x' junk` is accepted; `clipb2var`'s guard is
`offset * 511 < len`, so an **empty** clipboard is `result` 0 rather than 1;
and `clipb2var`'s documented `result` 3 is set by no path in the function.

#### The clock

`crates/tt-ttl/src/clockcmds.rs` and `strftime.rs`, 2026-08-09. Five commands
over a `strftime` written for the purpose — there is no calendar, no time zone
and no `strftime` in the standard library, and this crate has no dependencies.

**The conversions are MSVC's, not glibc's**, and they differ where it shows:
`%c` in the C locale is `08/09/26 14:30:00` on MSVC and
`Sun Aug  9 14:30:00 2026` on glibc. A macro that prints `%c` was written
against MSVC. The `#` flag is MSVC's too — `%#d` drops the leading zero,
`%#c` and `%#x` are long forms, and everything else ignores it. Only the
twenty-three conversions `isInvalidStrftimeCharW` (`ttlib_static_cpp.cpp:1894`)
lets through are implemented, and that is not an omission: `%F`, `%T`, `%e`
and `%s` never reach `strftime` because `getdate` rejects the format with
`result` 2 first.

`ScriptHost::format_time` became `strftime(secs, format, tz)` and `filestat`
now goes through it as well, so a frontend with a date library fixes both
zones at once. The default works in **UTC** and honours a fixed-offset POSIX
`TZ` (`JST-9` is UTC+9, since POSIX counts west) and nothing more: an Olson
name needs the database, and a `dst,start,end` tail needs a transition rule.
Anything not understood is UTC, which is what POSIX says an unparseable `TZ`
means anyway.

**A fifteenth upstream defect: the `<timezone>` argument leaks.** Upstream
applies it by putting it in the process environment and puts the old value
back on the way out — but the `GetFirstChar()` check sits *between* the two
(`ttl.cpp:2782` and `:2801`), so `gettime t '%H' 'UTC' junk` returns
`ErrSyntax` with `TZ` still overwritten and every later `localtime` in the run
is in the wrong zone. Not reproduced: the zone is an argument here and never
touches the environment, so there is nothing to leak. There is a second,
smaller one in the same restore — `_putenv_s("TZ", "")` *deletes* the variable
on Windows and *sets it to UTC* under POSIX, so a straight port of the restore
would have broken the macro's own zone for the rest of the run.

Three quirks are reproduced. Only the form **with** a format touches `result`,
so a script testing `result` after a bare `getdate` reads the previous
command's answer. `strftime` returning 0 is reported as `result` 1, "too
long" — and it also returns 0 for a format that produced nothing, so
`gettime t ''` is a length error that is really an empty one. And `setdate`
and `settime` read **fixed columns** (`Str[4] = 0`, then `sscanf "%u"`), never
looking at the separators: `1997/08/01` and `1997x08y01` are both accepted, a
piece with no digit ends the command silently, and nothing is ever reported.
Both are `ScriptHost` methods that do nothing by default, which is faithful
rather than a stub — `SetLocalTime` needs `SE_SYSTEMTIME_NAME`, so an
unelevated `ttpmacro.exe` silently does nothing too.

#### The send variants, the broadcasts and `scp`

`crates/tt-ttl/src/sendcmds.rs`, 2026-08-09. Twelve commands, in three groups
that are each a different kind of "not this terminal".

**`sendtext` and `sendbinary` are `send` with the guessing turned off.** All
three build the identical buffer from the identical argument list and hand it
to a different DDE command; the *terminal* decides (`ttdde.c:1215`). `send`
sniffs — text if there is no control byte below 0x20 other than CR/LF **and**
the bytes survive UTF-8 → UTF-16 → UTF-8, which for a byte string means "is
valid UTF-8" — and sends text re-encoded for the connection or binary
unchanged. The two newer commands exist because the guess is sometimes wrong.
So `ScriptHost::send` grew a `SendMode`, and the sniff itself is
`host::looks_like_text` in the *language* rather than a rule each host would
have to rediscover: two hosts disagreeing about it would make the same macro
send different bytes. `sendtext` on invalid UTF-8 sends **nothing at all**,
which is upstream's `ToWcharU8` returning NULL and no fallback.

**`sendbroadcast`, `sendmulticast` and `setmulticastname` address other
windows**, and `wait4all` reaches into other *macro processes* — `ttmdde.c:856`
walks a shared-memory table of every running `ttpmacro.exe` and snoops their
receive buffers until each has matched one of the patterns. Both are wholly a
frontend that owns several sessions, so all four are host methods and the
interpreter only parses. `scpsend`/`scprecv` are host methods for a different
reason: SCP is the SSH connection's own channel, not `tt-xfer`'s and not the
terminal's, and upstream does not wait for the transfer to finish — the
documentation's own example polls `ps` to find out.

**One thing is deliberately not reproduced.** `GetBroadcastString`
(`ttl.cpp:4031`) escapes `0x00` as `0x01 0x01` and `0x01` as `0x01 0x02`
because a DDE string ends at its first NUL. That is transport, not language:
there is no DDE here, and copying the escape would put literal `0x01` bytes
into everybody's broadcast.

Four quirks are reproduced. `sendlnbroadcast` ends with **CRLF** where
`sendln` ends with a bare CR, because `sendln` goes through the terminal's
newline setting and a broadcast does not. `setmulticastname` never checks for
end of line. `sendkcode`'s two arguments cross as four hex digits each, so
both are sixteen bits and `sendkcode 65536 1` is `sendkcode 0 1`. And
`scpsend 'f' 3` is **not** a type mismatch: the optional destination is
optional by discarding whatever `GetStrVal` reports, and the expression is
consumed on the way to the error that is thrown away, so the end-of-line check
finds nothing left and the file goes to the default destination.

`waitevent` is the one command in the group that is not a host method, because
it needs nothing new: its tests are on the **current** connection state rather
than on a transition (`ttmmain.cpp:609`), so `waitevent 4` on a connection
that is already closed returns at once. That reads as a bug and is what a
script means by it. `WakeupCondition &= 15` drops the bits above the four, so
`waitevent 16` waits for nothing and, with no timeout set, for ever — which is
what `ScriptHost::cancelled` exists for.

#### The passwords, and a store that is not one

`crates/tt-ttl/src/pwd.rs` and `pwdcmds.rs`, 2026-08-09. Eight commands over
two file formats, and the decision the plan had left open — keep `ttmenc.c` or
drop it — went **keep**, for a reason that is about files rather than about
cryptography.

**The v1 store is obfuscation and always was.** `Encrypt` takes no key: it
reads the password six bits at a time, writes each group next to the random
byte that masked it, and runs the lot through a rolling bias. Anything it
wrote can be read by anyone holding the file, and `Decrypt` — thirty lines
below it in the same source — is the program that does it. Dropping it would
have been defensible on its merits and is the wrong call anyway: `password.dat`
files full of it exist, `getpassword` is in twenty years of scripts, and a
successor that cannot open the user's own file is not one. So it is ported
exactly, documented as what it is, and new scripts are pointed at the `2`
commands.

**The v2 store is real and is byte-compatible.** AES-256-CTR under a key from
PBKDF2-HMAC-SHA512 at 210001 iterations, an HMAC-SHA512 over the record, the
key *name* stored only as its own PBKDF2 hash so the file does not say whose
passwords it holds — 381 bytes per record, base64'd to 508, one per line.
Which is what lets one file hold both formats at once: v1 lives in a
`[Password]` INI section and v2 lives in lines that are not INI at all, and the
documentation's own examples point both at `password.dat`.

Two details in there are quirks rather than choices, and getting either wrong
writes files nothing else opens. The HMAC key is derived from `EncSalt` **as
stored**, which by that point is its own ciphertext; and the three encrypted
fields are one continuous keystream — 203 bytes of NUL-padded password, then
the salt, then the MAC at offset 219 — because upstream pushes all three
through the same OpenSSL cipher BIO.

**This crate has dependencies now**, which it did not before. Six: the four
RustCrypto crates the format names, `getrandom` for the salts, and `tt-config`
for the INI layer — every one of them already in the lock file through
`russh`, at the version already resolved, so the shipped binary gains no code.
`getrandom` rather than `ScriptHost::random_u32` is deliberate: a salt a host
could make repeatable would not be a salt. The v1 obfuscation *does* take the
host's `random_u32`, because there the randomness is cosmetic and a repeatable
host makes a testable record. Unoptimised RustCrypto is about thirty times
slower than optimised, and 210001 iterations of it turned `cargo test -p
tt-ttl` from three seconds into thirty, so the workspace's dev profile now
builds those four crates at `opt-level = 3` and nothing else changes.

**The v1 vectors were not derived by reading.** `ttmenc.c` was compiled with a
deterministic `rand()` and run, and its output is the golden data in
`pwd.rs`'s tests — including the length table that pins `2·ceil(4n/3) + 2`,
which is where the first of the new defects came from.

**Six more upstream defects, for twenty-one.** Two are stack overflows in the
v1 codec. `Encrypt`'s output is `2·ceil(4n/3) + 2` characters and both callers
hand it a `char[512]` while accepting a password of up to 511, so **191
characters overflows** — 512 written plus a terminator, confirmed by running
it. `Decrypt` is the same shape from the other side: its working buffer is a
`char[512]` indexed by `strlen` of a value read out of the password file, and
its output is a `TStrVal` holding three-quarters of that, so a long enough
entry in a file the macro named overruns both. Two more are in the v2 layer:
`Encrypt2SetPassword`'s "has it changed?" test compares 203 bytes against a
buffer `strncpy_s` only NUL-*terminated*, reading uninitialised stack, and the
call that fills it writes `PassStr[203]` — one past the field, into the
record's own `EncSalt`; and `Encrypt2ProfileSearch` seeks to `Dpos` when
nothing matched, which is uninitialised if the file held no v2 record at all.
The fifth is the worst of them: `getpassword2` calls `SetStrVal` **between**
`GetStrVar` and the error check, and `GetStrVar` returns without touching its
out-parameter when an earlier argument already failed — so `getpassword2 1 2 3
4` reaches a function that bounds-checks nothing and `free()`s the pointer it
finds at a stack-valued index. None of the five is reproduced.

**The sixth is reproduced, because it is about files.** The obfuscated form is
printable ASCII and includes both quote characters, and it goes into an INI
file — where `GetPrivateProfileString` strips one matched pair of surrounding
quotes, which `ini-audit/` measured. So a record that happens to begin and end
with the same quote comes back two characters short, fails `Decrypt`'s
complement check, and `getpassword` reports **`result` 1 with an empty
password**: success, nothing in the variable, and `ispassword` still says the
entry is there. Roughly one record in four thousand. Reproducing it is the only
option that does not make this terminal write files it then misreads.

Four laxities are reproduced rather than reported, and they contradict each
other, which is the point. `setpassword` refuses an empty argument with a
syntax error; `getpassword` returns quietly from the same condition and does
not even write `result`, so a script testing it reads the previous command's
answer. `delpassword` never writes `result` on any path, success included.
`getpassword`'s Close button ends the macro — the only dialog that does —
where `getpassword2` discards the dialog's return value entirely, so closing
its window is indistinguishable from typing nothing. And `delpassword` is
silent about an empty filename where `delpassword2` calls it a syntax error.

One deviation is an improvement in a corner upstream cannot reach with its own
files: the v2 delete removes exactly `508 + 2` bytes at a computed offset, so a
password file somebody has run through an editor that changed the line endings
comes back corrupt. Reading the file into lines and writing it back has no such
dependency. A file this port *creates* also carries a UTF-8 BOM where
`WritePrivateProfileStringW` would have written the ANSI codepage — the same
deliberate divergence `tt-config` already makes, and `ini-audit/` measured
Win32 reading a BOM'd file correctly.

#### The regex family, and the dialect decision inside it

`crates/tt-ttl/src/regex.rs`, `regexcmds.rs` and `sprintf.rs`, 2026-08-09. Six
commands, and **every reserved word in the language now has an arm in the
dispatch** — the test that used to name whichever one the port had not reached
asserts that instead, by running each of the 231 words and failing on any that
still answers "unknown command".

**The dialect decision went to Oniguruma itself, through the `onig` crate.**
Decided 2026-08-09 with the user. The alternatives were a pure-Rust
Perl-flavoured engine (`fancy-regex`) or the linear-time `regex` crate, and
both lose the same thing: `regexoption` names **eleven syntaxes** — ASIS,
POSIX basic and extended, Emacs, grep, GNU, Java, Perl, Perl-NG, Ruby, default
— and some **thirty encodings**, and no other engine has them. `regex` also
has no backreferences and no look-around, which the documented patterns use.
Reimplementing that is reimplementing Oniguruma, and Tera Term vendors and
builds the same library, so this is the rule about preferring real upstream
code over a stub applied one level out.

Three things about it are worth writing down. `default-features = false` is
load-bearing: with the default on, `onig_sys` runs `bindgen` and the **build
then needs `libclang`**; without it the pre-generated bindings are used and
`cc` is the only requirement. Oniguruma is C, so the core crate now compiles a
C library — the first that ships rather than being test-only. And it
backtracks, so a pattern with nested quantifiers can be made to take
exponential time, and `waitregex` matches against **what the far end sent**.
Upstream has the identical exposure and a macro chooses its own patterns, so it
is reproduced rather than fenced; `timeout` is what an unattended script has.

**`sprintf` needed none of it.** Upstream validates each conversion spec with
Oniguruma — `^%[-+0 #]*([1-9][0-9]*|\*)?(?:\.([0-9]*|\*))?$`, recompiled on
every call — but the pattern is fixed and its two groups only ever answer "is
this a `*`", so the grammar is written out. What `sprintf` *does* need is C's
`printf`, because Rust has no `%g`, `%o` or `%a` and its `{:e}` prints `1.5e0`
where C prints `1.500000e+00`. All twenty conversions are written out and the
55 goldens come from a C program compiled and run, the same way the password
codec's did.

**A macro has no floating-point type**, which is why `%f` and its relatives
read a *string* and put it through `atof` — `sprintf '%f' '1.5'`, quotes
included, since without them the `1.5` is read as the integer 1 and refused.
`atof` answers 0 for anything it cannot read, so `sprintf '%f' 'abc'` prints
`0.000000` rather than failing.

The three commands report success three different ways, all from one upstream
function that returns a position: `strmatch`'s `result` is the **byte position**
of the match counting from one, `waitregex`'s is the **index of the pattern**,
and `sprintf`'s is **0 for success** — 1, 2, 3 and 4 name which argument went
wrong. `strreplace` is 1, 0 or -1 and is the only one of them that tells a
pattern Oniguruma refused apart from one that simply did not match.

Five quirks are reproduced. `regexoption` leaves each of its three settings
alone unless the call mentioned it, so there is no way back to the defaults;
naming an encoding or a syntax twice is a syntax error and naming an option
twice is not, because the options are ORed; and the one-sided check on
`OPTION_NONE` means `regexoption 'OPTION_NONE' 'IGNORECASE'` is accepted and
turns `IGNORECASE` **on** while the other order is refused. `waitregex` matches
a **line at a time** rather than a byte at a time — it waits for a newline and
then runs the patterns over the line that just ended, which is what makes `^`
and `$` mean anything — and the line still has its **CR** on it, because the LF
that triggered the attempt has not been added yet, so a pattern ending in `$`
does not match a CRLF line. An empty line never matches at all. And
`strreplace` measures what it is replacing by looking the `matchstr` *variable*
up by name rather than by using the match it just made.

**A twenty-second upstream defect.** `FindRegexStringOne` (`ttmdde.c:634`) uses
the match region's `beg` and `end` as indices into the target, and for a group
that did not participate both are **-1** — so it writes a NUL one byte *before*
the buffer, reads the empty string that leaves, and puts the byte back. A plain
`strmatch 'abc' '(x)?abc'` reaches it. Not reproduced: the empty string is what
it produces anyway.

#### And then the 53 scripts, which needed a definition of "pass" first

`crates/tt-ttl/tests/scripts.rs` and 53 goldens, 2026-08-09. This document has
said since Stage 0 that the target is "the 53 `.ttl` scripts in
`teraterm/tests/` pass", and the first thing that had to be built was the
sentence. **They are not self-checking**: they report to a human through
`messagebox`, several are deliberately full of errors so as to exercise the
error dialog, one asks the user to choose a file, most are Shift-JIS, and there
is no exit code to test in any of them. So "pass" is a **transcript** — every
send, every dialog, every action, and what the macro left on disk, in order —
against a golden per script, which is `oracle/`'s shape and carries `oracle/`'s
warning: every one was read before it was blessed.

Eight decisions make a run reproducible and they are all on the harness, where
somebody can disagree with them: the terminal starts connected, nothing ever
arrives from the far end so every `wait` is a timeout, the error dialog answers
**Continue** rather than Stop, the clock and the entropy and `HOME` are fixed,
each script gets its own directory, and the machine's own paths are substituted
out of the transcript so a golden is the same anywhere. The scripts stay in
`../teraterm`; a tree without that checkout skips the suite rather than failing
it.

**A harness that always presses the default button does not terminate.**
`gui_commands_test.ttl` loops on `yesnobox` until it has been told both yes and
no, on `inputbox` until the string is `ok`, on `listbox` until each of seven
items has been picked *and* the dialog then cancelled. The first blessing
produced a 57,000-line transcript that stopped on the line limit. So the harness
plays a **scripted user** per script — which is the honest model anyway, since
that is what the file is: a walkthrough for a person.

**And a transcript of host calls alone is blind to a third of them.**
`#32621.ttl` reads a file, rewrites every line through `strreplace` and writes
another one, and asks the host for nothing at all while doing it — its
transcript was the exit code. The directory is therefore snapshotted before and
after and the difference is part of the golden, which is what makes
`test_file.ttl` worth something: that one *is* self-checking, it puts up a
`messagebox` only when a file command misbehaves, and it comes out clean across
`fileopen`, `filerename`, `filecreate`, `fileconcat`, `filecopy`,
`filetruncate`, `filestat`, `filesearch`, `findfirst`/`findnext`,
`foldercreate`/`folderdelete`, `getfileattr`/`setfileattr` and the whole
password family.

Two scripts do not finish and both say so in their own golden.
`#35822-random.ttl` draws **eleven million** random numbers to check their
distribution — a benchmark, not a test — and stops at the line limit;
`array.ttl` fills a 100,000-element array and does complete, which is why the
limit sits between them.

Nothing in the 53 turned out to be a port defect. Three answers looked wrong on
first reading and each is upstream's: `#34838.ttl`'s three empty
`groupmatchstr` are the *clear* at `ttmdde.c:626`, which a successful match does
before it fills only the groups that exist; `comstat.ttl`'s three
`Variable not initialized` are `TTLFileStat`'s `goto end`, which returns -1
without ever parsing its output variables; and `oneline-if.ttl`'s
`Unknown command` for `if 1 hogefuga` is `ttl.cpp:6480`'s `else`, which is what
an identifier not followed by `=` gets. The rest are platform: `makepath` joins
with `/`, a `z:\name` argument is a filename containing a backslash, and
`filetruncate 'aa:\...'` succeeds here where Windows calls it an invalid path.

**Two of the 53 were not about the language and had nothing to run against.**
`macroparam.ttl` and `params_array.ttl` are about `params[]`, and the `.bat`
files beside them run `ttpmacro.exe` with `/V`, `/i` and `/vxx` switches that
the **launcher** eats before the macro sees what is left. Which arguments reach
`params[]` is `ttmdlg.cpp`'s `ParseParam`, so until that was ported both scripts
ran their `paramcnt == 1` arm and the golden recorded that they did. Done a
commit later — see the section after next — which turns them into the second and
third scripts here that check their own answers.

#### The macro file's own encoding, which is the machine's on Windows

`crates/tt-ttl/src/source.rs`, 2026-08-09. Four of the 53 scripts are named
`code_*.ttl` and exist to ask this question. Upstream's parser never sees a
file: `LoadMacroFile` goes through `fileread.cpp`'s `LoadFileU8C`, which sniffs
a byte-order mark and converts the whole thing to UTF-8 before `Buff[]` is
filled — so the decision belongs between the file and the buffer, and `include`
gets it for free. The three BOM branches are reproduced, and so are two things
that only show up in a damaged file: the macro **ends at its first NUL**, which
is what makes a BOM-less UTF-16 file a one-character macro, and a trailing
half-unit is a character whose missing byte is the *low* one in **both** byte
orders, because upstream's swap loop never reaches it.

**The no-BOM branch is deliberately not reproduced.** `LoadFileU8C` tries
`CP_ACP` *first* and falls back to UTF-8 only when the conversion fails, so the
encoding of a macro file is a property of the machine that runs it: on a
Japanese Windows a Shift-JIS macro reads correctly and a UTF-8 one that happens
to be valid CP932 is mojibake, and on a Western one every byte sequence is
valid CP1252 so a UTF-8 macro is mangled with no fallback at all. There is no
ANSI code page on Linux to be faithful to. A file with no BOM is passed through
unchanged, which is right for UTF-8 and leaves a Shift-JIS macro's bytes to
reach the host as they were written — visible in `code_cp932.txt`. Stage 3 puts
the code-page branch back on Windows, where it means something.

#### And the command line, which is the last thing the 53 were waiting for

`crates/tt-ttl/src/cmdline.rs`, 2026-08-09. `ParseParam` (`ttmdlg.cpp:82`) and
the tokeniser under it. Two of the 53 scripts — `macroparam.ttl` and
`params_array.ttl` — are not about TTL at all: they are about how `ttpmacro.exe`
reads its own arguments, and the `.bat` files beside them are a specification
written as five command lines and the `paramcnt` each should produce. Until this
landed both ran their "no parameters" arm and the suite recorded that they did.
They now run their real arms and come out clean, which makes them the second and
third scripts in the 53 that check their own answers rather than showing them to
a person.

**A switch is a switch only before the macro's name**, and that is the whole
content of the two scripts. `ParseParam` tests for `/D=`, `/I`, `/S` and `/V`
inside `if (ParamCnt == 0)` and nowhere else, so `ttpmacro m.ttl /V` passes `/V`
to the macro and `ttpmacro /V m.ttl` does not — same word, same case, opposite
meaning, decided by which side of the filename it fell on. There is no `--` and
no escape; putting the filename first is how a macro is given a `/V` of its own.

**`params[0]` is the whole command line**, which no documentation says and the
`param1`..`param9` form cannot express. `NewStrAryVar("params", ParamCnt+1)`
gives an array indexed from zero, `ttmmain.cpp:297` fills index 1 with the
macro's short name, and `ttl.cpp:243`'s loop starts at **0** and skips only
index 1 — so `Params[0]`, which `ParseParam` set to `GetCommandLineW()` before
tokenising anything, lands in `params[0]`. It is the only way a macro can see
the switches the launcher ate. The port had it as an unwritten hole with a
comment explaining why; the comment was wrong.

**And `param1` is `ShortName`, not the path.** `FitTTLFileName` cuts the
directory off and appends `.TTL` to a name with no dot anywhere in it — so
`ttpmacro /tmp/x/m` opens `/tmp/x/m.TTL` and tells the macro it is called
`m.TTL`. This port had been handing the whole path to `param1`, which showed up
in exactly one golden, as a dialog title.

**The tokeniser is upstream's own and is not `CommandLineToArgvW`.** A backslash
is an ordinary character, which it has to be for Windows paths; a `""` inside a
quoted run is one literal quote; and an unquoted `;` **ends the command line**,
with everything after it discarded as a comment. That last one is not a
plausible guess — it is `GetParam` returning NULL at `ttlib.c:888` — and it is
why `params_array.bat`'s `""` argument arrives as an empty string rather than as
two quote characters.

**Which leaves the platform question.** Upstream is handed one string and splits
it; Unix hands us `argv`, already split and unquoted by the shell. Running the
tokeniser over a joined `argv` would quote-process the text twice and turn
`'param 7'` back into two parameters, so there are two entry points instead:
`CmdLine::parse` for a whole command line, which is what Stage 3 and the `.bat`
transcriptions use, and `CmdLine::from_args` for `argv`, which runs only the
half of `ParseParam` that is left — the switch scan and the counting. What that
costs is `params[0]`: the original spacing and quoting do not survive `execve`,
so on Unix it holds the arguments joined by a space, which is
`/proc/self/cmdline` with its NULs replaced.

**A twenty-third defect in `ttpmacro`, and this one is reachable from outside.**
`ParseParam`'s loop calls `GetParam(Temp, sizeof(Temp), cur)` where `Temp` is
`wchar_t[512]` — the size is passed in *bytes* to a function that counts
`wchar_t`, so an argument longer than 511 characters is written up to 511
`wchar_t` (1022 bytes) past the end of a stack array. The call immediately above
the loop, for the executable's own name, passes `_countof` and is correct, which
is what makes it look like a typo rather than a pattern. It is a stack buffer
overflow driven by the command line, so it is reachable from a shortcut, a
`.bat` file or anything else that launches `ttpmacro.exe` — file it with the
rest in Stage 3. Not reproduced: the port truncates at 511, which is what the
code meant.

`GetParam` also has a trailing-`;` trim (`ttlib.c:909`) that cannot fire — a `;`
only reaches the buffer while `quoted`, and nothing between that copy and the
loop test can clear the flag — and which reads `buff[-1]` if the function is
ever called with a size of 1. Recorded rather than reported: neither is
reachable from any caller in the tree.

#### And then it was joined to a terminal, which changed what "wait" means

`crates/tt-macro/`, 2026-08-09, plus the tap in `tt-vt` and the ring in
`tt-session`. The language had been complete since the day before and had
nothing to drive; this is the join, and it turned up the largest single
surprise in the macro language.

**A macro does not read the wire.** `wait`, `waitln`, `waitregex` and `recvln`
match against `DDEPut1`'s buffer, and `DDEPut1` is fed from `OutputLogUTF32`
(`vtterm.c:448`) — *the same function that feeds the text session log*. What
reaches a macro is the characters the parser decided to **print**, re-encoded as
UTF-8, plus `CR`, `LF`, `BS` and `HT` at the moment those controls executed and
a `CR LF` where a line wrapped. Nothing else. So `wait 'ESC['` cannot match, a
character that was printed and then erased is in the stream anyway, and a
prompt dressed in colour is matched by a pattern written against the plain
text. A port that teed the transport's bytes — which is the obvious thing to
build, and was the plan — would have been wrong in a way no single test would
have caught, because the difference only shows on a host that emits escape
sequences, which is all of them.

Three details of it are worth having written down. `CheckEOLCheckLog`
(`checkeol.cpp:105`) **drops a lone CR** and turns `CR LF` into one `CR LF`, so
`abc\rdef` reaches a macro as `abcdef` while the screen shows `def`; that is
also the mechanism behind the `waitregex` trap already on file, since a line
arrives with its CR still attached. The parked space upstream writes in the last
column before a wide glyph wraps is **not** in the stream, because `vtterm.c:896`
writes it with `BuffPutUnicode` rather than `PutU32` — so a macro's copy of that
line is one column narrower than the screen's. And the tap is fed by
`CarriageReturn` and `LineFeed` themselves, which means `ts.CRReceive` changes
what a macro sees and not only what the screen does.

**The ring is 64 KiB and full drops the *oldest* byte** (`ttdde.c:107`,
`InBuffSize`). The opposite of what a queue usually does and the right way
round: a macro that has fallen behind wants the prompt that just arrived, and
blocking the parser until a script gets around to reading would let a stalled
macro freeze the window.

**The thread boundary is a job queue, not a mutex, and that was the decision.**
Upstream's macro is a second process and every host call is a DDE transaction;
the port keeps two properties of that and drops the rest. A host call blocks the
macro and nothing else — which is why the interpreter has a thread at all, and
why `wait` is an ordinary function here where upstream had to park itself in
`TTLStatus`. And the terminal is only ever touched from the thread that owns it:
an `Arc<Mutex<Session>>` would have worked until the day a macro held the lock
through a modal dialog and the window stopped repainting, which is a frame rate
decided by a script. So the macro thread sends a closure taking
`(&mut Session, &mut dyn MacroUi)` and blocks on the answer, the frontend runs
it in its own event loop, and **nothing is borrowed across the boundary** — the
rule the SSH host-key prompt already pays for breaking. Bytes bypass the queue
entirely through the ring, because a `wait` asks for one thousands of times a
second and none of those should wait behind a repaint.

Two things fell out of building it. `Session` needed a **raw** write
(`send_bytes`) beside `send_text`, because a TTL string is bytes and `send` is
documented to put them on the line unchanged — and the mode is not decoration:
upstream's text path runs them through `OutControl` (`ttcmn.c:800`), where a CR
becomes CR, CRLF, LF or `CR NUL` depending on `ts.CRSend` and on telnet's binary
mode, while the binary path writes what it was given. So `sendln 'go'` puts
`go\r` on the wire with the shipping default and `go\r\n` with `CRSend=CRLF`,
and `sendbinary` of the same string always puts `go\r`. Choosing one path for
both would break whichever half of the world the choice went against.

**And the text session log is now known to be missing three things.** Upstream
feeds it and the macro from one call, so a text log gets the `HT`, the `BS` and
the wrap's line break as well; this port's log tap predates the macro one and
has none of them. Deliberately left alone in the same commit — the log is an
artefact a user reads, changing it wants its own justification and its own
tests — but it is a divergence rather than a choice, and it is `LogOptions`'s
neighbourhood when somebody gets to it.

Twelve integration tests drive a real interpreter on a real thread against a
real session over a `MemoryTransport`, including the two that matter most: a
`waitregex` anchored with `$` failing where `\r$` succeeds, driven end to end
for the first time, and End releasing a macro blocked in a `wait` with no
timeout. What the host still refuses is listed at the bottom of
`crates/tt-macro/src/host.rs` with a reason each; the one that blocks real use
is `connect`, which wants the Tera Term command-line parser the CLI entry point
also wants.

**That parser is the next substantial piece, and it is bigger than it looks.**
`ttset.c`'s `_ParseParam` is 357 lines over two passes — the first finds `/F=`
and stops, because the settings file has to be read before anything is applied
on top of it — and TTXSSH adds another 389 in a plugin hook that *blanks* the
options it consumed before Tera Term sees them, which is where `/ssh`, `/user=`
and `/auth=` live. It also writes into about twenty settings this schema does
not have yet, so the honest shape is a `CommandLine` that parses everything and
an `apply` that changes what the schema can hold, rather than a parser that
silently drops half its input. `GetParam` and `DequoteParam` are already ported
in `tt-ttl/src/cmdline.rs` and would move to `tt-config` first — they are
`ttlib.c`'s, and upstream puts `_ParseParam` in the same DLL as the INI reader
for the same reason this port would.

#### And then the command line, both halves of it

`crates/tt-config/src/cmdline/`, 2026-08-09. All of the above, in the shape that
paragraph predicted: the tokeniser moved, `_ParseParam`'s 39 options over its two
passes, TTXSSH's 30 more, `ParseHostName`, `ParsePortName`, `GetFilePath`, and 21
new settings with `CommandLine::apply` to write into them. 66 tests.

**The two halves compose through a string, not a struct**, and that is the thing
a design would get wrong first. TTSSH hooks the parser, runs *first*, and blanks
what it consumed out of the line — so `ssh://user@host/` is rewritten **into** a
bare `host:22` token, and that is the only reason Tera Term's own parser can find
a host in an SSH URL. `ssh::parse` therefore returns the options *and the line it
left behind*; `ssh::parse_both` runs the pair in upstream's order.

`connect`'s argument needed one more thing: `ttdde.c:617` prepends a literal
`"a "` — "`a` = dummy exe name" — because `_ParseParam` discards its first token.
It also passes NULL for the DDE topic, which is not a detail either: with no
buffer, `/D=` is ignored *and* the startup macro survives. `parse_argument` is
those two facts.

Six things read as bugs and are the specification. **A bare host name cancels
`/C=`**, because its arm assigns `IdTCPIP` outright — so `/C=1 myhost` is a TCP
session with no COM port and reversing the two words changes the answer.
`/C=` is bounded against `ts.MaxComPort`, a *setting*, and out of range is
dropped rather than clamped, leaving serial selected with no port.
`/AUTOWINCLOSE=1` means **off**, because that arm is an `_wcsicmp` against `on`
with an `else` rather than `GetOnOff`. `/OSC52=off` and `/OSC52=nonsense` are the
same state. A `/D=` topic frees the startup macro, INI setting and all, so a
terminal a macro opened does not open another macro. And in TTSSH, **`-` leads a
switch** while `ssh` is matched case-*sensitively*, so `-ssh` works and `/SSH`
does nothing at all, silently, in both parsers.

The service-name table is transcribed from `servicenames.c` rather than left to
`getservbyname`: `/P=telnet` has to be 23 on Linux, on Windows and in 2003.
`ATTRIBUTION.md` records it as the second thing this distribution ships from
upstream.

**And a twenty-fifth upstream defect, the second where the code and the
documentation disagree rather than the code and this port.** `/NOLOG`'s arm
clears `ts.LogAutoStart` and the *ANSI* copy of the log filename, `ts.LogFN`
(`ttset.c:3850`) — but the wide `ts.LogFNW` is the one everything uses, and
`vtwin.cpp:3631` starts logging when `ts.LogAutoStart || ts.LogFNW != NULL`. So
`ttermpro /L=out.log /NOLOG` **logs to `out.log`**, which is the single thing the
option exists to prevent, and `teraterm.html` says only "start Tera Term without
logging". The port lets `/NOLOG` win — the same call as `logwrite`-while-paused,
and for the same reason: reproducing it would mean writing a file the user
explicitly asked not to have. Reachable from a shortcut, so it wants filing with
the rest.

**`/C=<n>` is the nth port the picker shows** — decided 2026-08-09, implemented
as `tt_conn::serial::port_by_number`. A number is a 1-based index into
`enumerate()`, which is sorted by device node, so `/C=1` on a command line and
the first entry in the port menu are the same thing. The alternative was a
literal `COM<n>` → `/dev/ttyS<n-1>` map: stable, and useless on the machine this
is developed on, which has four USB ports and no `ttyS0` worth opening.

It inherits the instability `enumerate`'s own docs already carry — `ttyUSB<n>` is
assigned in attach order, so replugging two adapters can swap which is `/C=1`.
That is the right trade for a *command line*, which chooses afresh every time;
anything that **remembers** a port still has to store the `by-path` id, and
`number_of_port` goes the other way for writing one back.

One open item left, and it is about the schema rather than about Linux:
`ComPort`'s bound is a *different setting* and is a reset to 1 rather than a
clamp (`ttset.c:1223`), which the schema has no way to express — so an
out-of-range `ComPort=` in a file survives here where upstream would reset it.

#### And what a command line says to open, which is three answers

`crates/tt-session/src/open.rs`, 2026-08-09 — `OnCommStart` (`vtwin.cpp:3708`),
the join between the parser and the transports, and the half both consumers
share.

**Upstream's startup is one `if` whose other two arms open nothing**, and that is
the part a reimplementation drops on the floor. A TCP session is decided by
whether there is a *host name* — not by the port type — and a serial one by
`ComAutoConnect`, which `/M` turns off and an in-range `/C=` turns back on in
either order, because that runs after the option loop rather than inside it.
`/DS` and `/ES` then choose between the New Connection dialog and an empty
window, which is how a session that will `connect` for itself starts up.
`Startup` is those three answers plus a fourth: the two transports upstream has
and this port does not — a replay file and a named pipe — say which, rather than
opening something else.

Everything but three things comes from the settings, *after* `apply`, which is
upstream's order since `_ParseParam` writes `ts` and `CommOpen` reads it back.
The three are the host name (no INI key at all), the port type (the file holds
two of its four values) and TTSSH's options.

`Target::open` does serial, telnet and a local shell. **SSH deliberately does
not**: a host key or a password is a prompt, so it stays a state machine the
caller pumps while it owns a window — `tt_ssh_connect` and the Qt shell's four
dialogs already do exactly that, and upstream agrees about where the prompt goes,
since TTSSH raises its dialogs on the terminal's thread while the macro that
asked sleeps.

**And one deliberate divergence, which is a port number.** TTSSH never assigns
`ts.TCPPort` — only its half of the New Connection dialog does (`ttxssh.c:1347`)
— so `ttermpro /ssh myhost` connects to whatever `TCPPort=` holds, and on a fresh
install that is **23**: an SSH client on the telnet port, a connection that cannot
succeed. SSH with no port asked for is 22 here. The test for "no port was asked
for" is upstream's own idiom rather than a new one — `TCPPort == TelPort` is how
`vtwin.cpp:3666` decides whether a port was chosen for a protocol or merely
inherited, and it is why the telnet opening burst is not sent to a terminal
server's per-line port — so a user who has ever connected by SSH from the dialog
already has 22 in their file and sees no change. Overruling this is a one-line
change in `Target::of`.

#### And then a window opened from one

`crates/tt-ffi`'s command-line half plus `shell/src/main.cpp`, 2026-08-09.
`sterna /ssh /auth=publickey myhost` works as the shortcut it was converted
from did.

**Three calls, in the order a frontend has to make them**: `tt_cmdline_parse`
over `argv` as the shell split it, `tt_cmdline_apply` to write the line into
the settings the file was just loaded into, and `tt_cmdline_startup` for
`OnCommStart`'s answer. The `TtStartup` it fills is exactly what the existing
`tt_session_connect_*` calls take, with SSH going to `tt_ssh_connect` because
it has prompts. Writing the C test found the order matters and is not
reversible: applying writes *into* the settings and nothing takes it back out,
so a second line over the first sees the first one's port.

**The two spellings are read one way or the other and never half of each**,
because on one point they disagree: a bare host name is telnet to `ttermpro`
and SSH to `sterna`, and each is right on its own side. An argument spelled
`/OPTION` switches to Tera Term's; anything led by `-` stays ours, which is
what keeps `--shell -- /bin/sh` working when its own positional arguments are
full of slashes. TTSSH's dash spellings (`-ssh`) therefore reach nobody, which
is stated rather than guessed at — `-` is Qt's option lead here.

`/F=` is why upstream parses twice and why this does: it names the settings
file, and `MaxComPort=` — which bounds `/C=` — is inside it. `/W=` and `/H` go
through the *settings* rather than the startup, so a file that sets them and a
line that sets them arrive at the same place; `Title=`'s default is upstream's
own product name and is read as "no opinion" rather than put in this program's
title bar. `/M=` and a mistyped `/ssh` option are said out loud rather than
ignored.

**And a trap that cost a debugging round, now in `CLAUDE.md`.** The diagnostic
under `/V` was written with `qWarning`, and Fedora builds Qt with journald — so
it goes to the journal rather than to stderr whenever stderr is not a terminal,
which is exactly how a windowless session is launched. It read as "the option
was never parsed"; the option had been parsed correctly all along. Anything the
user has to see uses `fprintf(stderr)`, which is what `QCommandLineParser` does
with its own errors.

`shell/tests/cmdline_test.cpp` needs nothing — the one case that connects opens
its own listening socket — so argv to a live connection is checked end to end
in CI.

And a `ttctl` socket now lets one of these be reached from outside the process
— see the section at the end of this stage.

#### And then a macro opened one, which is the same line through a third parser

`Startup::of_connect` and `Target::cygterm` in `tt-session`, `connect` and
`cygconnect` in `crates/tt-macro/src/host.rs`, 2026-08-09. A macro can now open
its own connection, which is the command every login script starts with.

**`connect`'s argument is a Tera Term command line and `cygconnect`'s is not.**
The first goes through both parsers and then through the same `OnCommStart` a
startup line ends at — upstream is literally that, `ttdde.c:608` parsing the
string into `ts` and posting `WM_USER_COMMSTART` to the window. The second is
**CygTerm's** command line (`ttl.cpp:73` spells the launcher `cyglaunch -o`),
which is a tenth parser's worth of options describing a shell to spawn on a
pty — so it maps onto `PtyParams` field for field rather than being ignored.
`crates/tt-config/src/cmdline/cygterm.rs` is that parser, and
`cygterm.cpp:905`'s `exec_shell` is what it is transcribed against: `-s` is the
command, `-ls` is the leading `-` on `argv[0]`, `-v` is the environment, and
`-d`/`-cd` are the working directory.

Three things were only findable by reading past the obvious file:

- **TTSSH's half runs for a `connect` too**, so `connect 'myhost /ssh'` works.
  `ttdde.c` calls through a *function pointer*, and `LoadTTSET`
  (`ttsetup.c:47`) re-installs `_ParseParam` and then calls `TTXGetSetupHooks`,
  which lets the plugin hook it again. Reading `ttdde.c` alone gives a `connect`
  that cannot open an SSH session, which is most of what the command is for.
- **CygTerm's default is the launcher's directory, not the user's home.**
  `home_chdir` is false with no `-cd`, and the shipped `cygterm.cfg` has no key
  for it, so an unqualified `cygconnect` inherits where the terminal was
  started. `PtyParams::cwd`'s `None` means *home*, so taking the default there
  would have been a divergence in the one case nobody writes an option for.
- **The line and the `-s` string are split by different rules**, upstream as
  well as here: the line is split by cygwin's C runtime, where a backslash is
  ordinary, and the shell string by `get_argv` in `cygterm.cpp` itself, where a
  backslash escapes. Using one splitter for both is tidier and gets the
  manual's own `-d C:\ -nocd -nols` example wrong — it becomes two options and
  a directory called `C: -nocd`.

**SSH is the one target that leaves the crate**, for the reason `Target::open`
already had: a host key or a password is a prompt and a prompt belongs to
whoever owns a window. `MacroUi::connect_ssh` is the seam, and its default is
`Ok(None)` rather than the refusal every other method there makes — a `connect`
that answered "Unknown command" would be the larger lie, where "the connection
did not come up" is an outcome the documentation already promises. Nothing else
about failure is reported either: `result` is 0, 1 or 2 and covers a refused
port, a name that does not resolve, a frontend with no SSH dialogs and a line
that named nothing.

Two gaps, both deliberate and both written down where the code is.
`Startup::Dialog` — `connect ''`, which upstream answers with the New
Connection dialog — is treated as the other arm of that same `if`,
`SetDdeComReady(0)`, because the dialog this wants is the whole four-transport
one and the shell's own is serial-only. And a `/L=` on a `connect` line does not
start a log: that is `OnCommOpen`'s (`vtwin.cpp:3631`), it is the frontend's
there and the frontend's here, and a second implementation behind `connect`
would be two of them.

`crates/tt-macro/tests/connect.rs` is nine tests that need nothing installed:
`cygconnect` opens a real `/bin/echo` on a real pty and the macro waits for what
it printed, and `connect` opens a TCP listener the test binds itself.

**And two more upstream defects, for twenty-seven** — both in CygTerm's
`env_add` (`cygterm_cfg.cpp:42`), both found by reading, neither reproduced.
`cygconnect '-v FOO'` — a variable with no `=` — reaches `strdup(NULL)`, because
`env_add1` passes NULL for the value and nothing checks it; and replacing the
*first* variable drops every variable after it, since the same-name arm assigns
`pr_data->envp = e` without carrying `e->next` over, so `-v A=1 -v B=2 -v A=3`
loses `B`. They are in a Cygwin-only program this port does not ship, which is
why they are here rather than in `docs/upstream-bugs.md`, and they want the
same demonstration in Stage 3 as the rest.

#### And then it could move a file, which needed something to wait on

`crates/tt-macro/src/host.rs`'s `Plan`, plus `TransferReply` in `tt-session`,
2026-08-09. Fifteen of the sixteen transfer commands, which is every one that
is a protocol.

**The command blocks, and that is the whole difficulty.** `Session::send_files`
returns as soon as the protocol has started, because a transfer is driven by
the frontend's pump; a macro is on another thread and has nothing to watch. So
`TransferReply` is a one-shot the session posts the outcome to alongside the
event, and the macro parks on a condvar until it arrives. That is upstream's
shape rather than a new one — a transfer command puts `ttpmacro` in
`IdTTLWaitCmndResult` and `ProtoEnd` answers over DDE — and it is the second
thing to cross this boundary without being a job, after the byte ring, for the
same reason: it is not work for the frontend to do.

Two details are not obvious. **Cancelling asks and then keeps waiting**: the
protocol sends its cancel sequence and ends on its own terms, which for ZMODEM
is a 500 ms timer, so End is followed by a wait rather than by a return. And a
transfer is the one blocking command that cannot notice a dead frontend on its
own — a `wait` polls a ring that goes quiet, but an outcome that is *posted*
never arrives at all — so it knocks every 250 ms, which is free against a
transfer's own traffic.

**The mapping is `filesys_proto.cpp`'s `*Start*` functions read for what they
do to `ts` before opening the dialog**, not for their signatures. Two things
are the same in every one: a relative filename is resolved against
`ts.FileDir`, and that same directory is where a protocol that names its own
file puts what arrives. The per-protocol answers are mostly settings rather
than arguments — `xmodemsend` has no binary flag, so `ts.XmodemBin` decides and
`ttset.c:1051` makes it binary; YMODEM is `Yopt1K` in both directions,
hardcoded with a comment saying so; and ZMODEM's receive flag does not matter
at all, because `zmodem.c:1008` overwrites it from the sender's own ZFILE
header.

It also corrected `tt-xfer`, which believed only XMODEM is told its own
destination. Three are, for three different reasons: XMODEM carries no filename,
`raw.c:80` writes into whatever `GetNextFname` hands it, and a Kermit `GET`'s
name is the **remote** one being asked for — `kermit.c:1160` takes its basename
before it goes in the `R` packet, so `kmtget` cannot name a remote directory
here or upstream. Without that, `kmtget` and `recvfile` would have opened a
file called nothing.

**`sendfile` is the one that is still refused, and it is not laziness.** It is
the File menu's, not `ttpfile`'s: `filesys.cpp:359` runs it a byte at a time
through the terminal's own write path, with bracketed paste, local echo and
DBCS decoding, and `raw.h` says outright that there is no raw *send* protocol.
It wants a `Session::send_file` that nothing has needed yet, because the shell
has no File menu either — one feature, two callers, and its own tests.

Four integration tests drive it on a real thread against a real session, using
the raw receive because it is the only protocol that needs no peer. The one
that matters is the wait: a `recvfile` with a one-second auto-stop takes a
second, and the `result` the script sends afterwards could not have been sent
before. The other three are the auto-stop that never fires, the cancel, and a
transfer that fails to start — which is `result` 0 rather than an error, and
returns at once.

#### The log a script drives, and the one place the manual won

`crates/tt-session/src/log.rs`, 2026-08-09. `logpause`, `logstart`, `logwrite`
and `logrotate` — four commands that were refused for want of a `SessionLog`
that could pause or take a write from anywhere but the tap. `loginfo` also
stopped answering every flag as false and reads the log's own options instead.

A pause **discards** rather than buffers, which is `logpause.html` and is
upstream in two places at once: `Log1Bin` drops the byte at the input for a
binary log (`filesys_log.cpp:1038`) and `LogToFile`'s drain loop drops it on
the way out for a text one (`:647`). Here there is no ring between the tap and
the file, so both are one test.

**`logwrite` while paused is a deliberate divergence, and it is the first one
where the code and the documentation disagree rather than the code and this
port.** `logwrite.html` says the string "can be written even while logging is
paused". `FLogWriteStr` (`:833`) puts the characters in the same ring the tap
fills and then calls `LogToFile`, whose drain loop is discarding — so upstream's
note falls into the gap it was written to explain. Following the code would
mean implementing the sentence the manual does not say, so this one follows the
manual and says so in three places. It is the twenty-fourth upstream defect on
file and the first outside `ttpmacro`; file it with the rest in Stage 3.

Two smaller things fell out. `logwrite` writes the string character for
character, so a `#13#10` puts a real CR in a log whose tap would have
normalised the line ending — that is `FLogPutUTF32_` and it is why upstream's
own examples pass `#13#10`. And none of the family is an error with no log
open: `FLogPause` and the three rotation setters all return on a NULL `LogVar`,
so a macro cannot tell.

#### And the control lines, which are a modem script's first four commands

`crates/tt-session/src/serial.rs`, 2026-08-09. `setdtr`, `setrts`,
`setbaud`/`setspeed`, `setflowctrl` and `getmodemstatus` — five commands that
had been refused since the language landed, for a structural reason rather than
a behavioural one: a `Session` holds a `Box<dyn Transport>`, and DTR is on
`SerialConn`.

**`Transport::as_serial` is the trait's one downcast, and it is upstream's
guard as well as the escape hatch.** Every one of these arms in `ttdde.c` opens
with `!cv.Open || cv.PortType != IdSerial`, and both halves of that are `None`
here. The alternative was four more trait methods that three transports out of
four would implement only to decline. What the guard rejects is not an error —
the terminal answers `DDE_FNOTPROCESSED`, which a macro reads as success — so a
login script written for a modem runs to the end over SSH, doing nothing, which
is what it does upstream.

Two divergences, both deliberate.

**`setflowctrl` is applied to the port, and upstream's is not.**
`CmdSetFlowCtrl` assigns `ts.Flow` and stops there (`ttdde.c:1002`): there is no
`CommResetSerial` under it, and no other path applies one, so upstream's
`setflowctrl 2` leaves the port running with whatever it was opened with until
something *else* resets it — a `setbaud`, or the serial dialog's OK. The
neighbouring `setbaud` remembers to call it, one case arm away, which is what
makes this an omission rather than a design. `setflowctrl.html` says flatly that
the command changes flow control. **Third place the port follows the manual
instead of the code**, after `logwrite` while paused and `/NOLOG`, and the
twenty-eighth defect on file: a script that turns handshaking on before a large
paste and does not get it drops bytes on a real cable, and the documented idiom
for its neighbours — `setflowctrl 3` so that `setdtr` passes its "flow control
is none" guard — otherwise opens the guard while the driver still has
`CRTSCTS`, leaving the pin driven by two things at once. Measured on the rig:
the by-hand clear does move RTS under `CRTSCTS`, so that is a fight rather than
a silent failure.

**And the reset edits the live parameters rather than rebuilding them from the
settings.** `CommResetSerial` builds the whole `DCB` out of `ts`, which is
correct upstream because the port was opened from that same struct. Here the two
can disagree: `Session::connect` takes any transport, and the shell's
`--port`/`--baud` line opens one from a `SerialParams` the settings never saw.
Rebuilding from the settings meant a `setbaud 19200` on a 115200 port also
quietly moving the parity, the data bits and the flow control to whatever the
file said — which is exactly how the first version failed its own test, by
resetting the line to the schema's default 9600 and then reporting that XON/XOFF
had not been applied. The upstream *shape* is kept: it is still one whole
`tcsetattr`, so DTR and RTS are re-asserted on the way past and a `setdtr 0`
before a `setbaud` does not survive it, which is `dcb.fDtrControl` doing the
same thing.

The rig is what makes this checkable, and four new cases in
`tt-session/tests/serial_loopback.rs` use all of it: DTR is wired to the other
port's DSR and RTS to its CTS, so the pins are asserted at the far end rather
than at the ioctl; `setbaud` is shown by a speed *mismatch*, because a test that
read the setting back could not tell it from a no-op; and `setflowctrl` is shown
by the far end sending XOFF and the near end's transmitter stopping until the
XON. `getmodemstatus` reads the far end's DTR back through the session.

`getipv4addr` and `getipv6addr` went with them, since they were on the same
list for the same kind of reason — `getifaddrs` in `tt-conn`, which already has
`libc` and owns the socket layer. Both of upstream's filters are transcribed
and they disagree with each other: IPv4 is one address per interface and drops
anything down or loopback, because `SIO_GET_INTERFACE_LIST` answers one entry
per interface; IPv6 takes every unicast address of every adapter with `::1`
included, asking only for Windows' `DNS_ELIGIBLE` — the one thing here with no
Linux equivalent, and so the one thing not reproduced. The rendering is
`myInetNtop`'s (`ttl.cpp:2499`) rather than RFC 5952: sixteen bytes of `%02x`
with a colon after every second one, always 39 characters, because a script has
been comparing against that string for a decade.

Three things are still refused, and the list at the bottom of
`tt-macro/src/host.rs` says so. `setserialdelaychar` and `setserialdelayline`
pace what is *sent*, and upstream paces it in `SendMem` — a queue between the
macro and the wire that this port does not have, with three other callers
waiting on it (a paste, `sendfile`, the File menu's send), so it wants building
once for all of them. And a `setbaud` does not repaint the status line: upstream
posts `WM_USER_CHANGETITLE` because the speed is in the title, `Session::
describe` carries the speed here too, and nothing asks it again — there is no
frontend running macros yet to notice, and it wants an event when there is.

**One bug fixed on the way past, in this port rather than upstream's.**
`setecho` wrote `LocalEcho` through `Session::set_setting`, which matches the
schema's *dotted* name — so it resolved to nothing, answered `false`, and the
command parsed, reported nothing and changed nothing. Every write through that
seam wants `terminal.local_echo`; a `debug_assert` on the `false` now says so,
and the test asserts against the terminal's mode, which is the only place it
shows.

#### And then the window ran one, which is the first callback in the ABI

`crates/tt-ffi/src/lib.rs` and `shell/src/Macro.cpp`, 2026-08-09. The language
was ported, the host was a real terminal, and none of it could be reached from
outside the process: `MainWindow` put a box up saying macros could not be
started yet. Now Control > Run macro starts one, `/M=` on a command line starts
one, and the eleven dialogs are Qt dialogs.

**The seam calls back into C, which nothing else here does, and the reason is
the mirror image of why SSH refuses to.** `tt_ssh_connect_poll` is a state
machine precisely because a callback would fire on a worker thread — the one
place a Qt frontend cannot raise a dialog. `TtMacroUi`'s twenty callbacks fire
from inside `tt_macro_service`, on the thread that *called* it, which is the
frontend's own: a modal dialog spinning a nested event loop is an ordinary
modal dialog, and the macro is parked on its own thread until it closes. That
is the whole of the design `tt-macro` was built for, arriving at the seam.

A null function pointer is not a crash and not a silent success. The Rust side
falls through to `NullUi` rather than to a hand-written refusal, so "this
frontend has not implemented that" stays the trait's own documented default —
the macro is told "Unknown command", which is the only refusal the language
has, and a frontend with three dialogs is useful.

Three things the wiring turned up, all in `CLAUDE.md`:

- **A macro that ends without asking for anything never wakes its frontend.**
  A `dispstr` on the last line is noticed and a bare `pause 1` is not, because
  a frontend has no timer to fall back on. The thread knocks once on its way
  out — and sets its flag *before* knocking, since `JoinHandle::is_finished`
  is still false at that point and reading it would make the frontend wait for
  a wakeup that had already happened. `tests/abi.c` polls for five seconds
  against a one-second macro, which fails outright without the knock.
- **The notifier is disabled across a service call.** It is level-triggered,
  so it fires again inside the dialog's own nested loop — the SSH prompt's
  re-entrancy, except that here it would open a second dialog inside the
  first.
- **`tt_macro_free` cannot detach the terminal**, because it is not given a
  session. `tt_session_unlink_macro` is the other half; without it the
  terminal goes on copying every character it prints into a ring nobody reads.

**Two divergences, both stated where they happen.** Qt gives Escape and the
title bar's close to a dialog's reject-role button, so a closed `yesnobox`
cannot be told from No — upstream ends the macro on the first and not the
second, and here both are No. And `enablekeyb 0` is released when the macro
ends: upstream puts `KeybEnabled` back only from Control > Reset terminal
(`vtwin.cpp:4874`), a menu item this port has not got, so a macro that died
between the two calls would leave a terminal nobody can type into.
`enablekeyb.html` describes the lock as lasting "while the macro is sending
the data", which makes this the fourth place the port follows the manual.

`shell/tests/macro_test.cpp` runs seven cases against the real event loop,
including one that drives `/bin/sh` — a macro typing at a shell and waiting
for what comes back, which is the tap, the ring, the macro's thread and the
window's notifier in one assertion. The dialogs are answered by a repeating
timer that fires *inside* the modal loop, which is the only way to test them
and is also the re-entrancy the guard above exists for.

#### And then it could be reached from outside, which is what DDE was for

`crates/tt-ctl/`, the ctl half of `crates/tt-ffi/src/lib.rs`, and
`shell/src/Control.cpp`, 2026-08-09. The last thing in this stage that had not
been built, and the one `PLAN.md` has named since Stage 0.

**The socket is not a replacement for DDE's command set.** That set is
`ttddecmnd.h`, ninety-odd commands, and it *is* the macro language — because
upstream's macro is a second process and had no other way to reach the
terminal. Here the macro is a thread inside the window, the language is
`tt-ttl`, and all of that glue was deleted in the first place. What was deleted
with it was the *reachability*: a running window could be asked for something by
a person clicking Control > Run macro, or by a `/M=` at startup, and that was
the entire list. So this replaces the reachability and nothing else — nine
methods, of which two start and stop a macro and the rest are the things a
script wants that are not a macro.

JSON-RPC 2.0, one object per line, on a Unix socket in
`$XDG_RUNTIME_DIR/sterna/`. The framing is chosen so that
`printf … | nc -U` is a client: if driving a window needs a library, the socket
has failed at the thing DDE was bad at. `serde_json` is a dependency and a
hand-rolled parser was rejected — the protocol is one this project did not
design, the parser is on a socket, and this is the wrong place in the tree to
save four crates.

**The directory is the name service DDE had and a Unix socket has not.** A
window binds `<name>.sock`; the name is a `/D=` topic or the pid, which is the
same command line upstream uses for the same purpose — `ttermpro` launches
`TTPMACRO /D=<hwnd-in-hex>` (`ttdde.c:1497`) precisely so the macro can find
the window that started it. It is also the access control: `0700` on the
directory, `0600` on the socket, `SO_PEERCRED` behind both. Anything that
reaches this can type at whatever the window is connected to.

**Four divergences, all stated where they happen.**

- **A client given no name refuses to guess between two windows.** A DDE
  wildcard connect takes whichever conversation answers first, so upstream's
  `ttpmacro login.ttl` with two Tera Terms open logs into an arbitrary one of
  them — and that macro usually types a password.
- **`connect` with nothing openable is refused rather than answered with the
  New Connection dialog**, and a `connect` that *will* open is queued to the
  next turn of the event loop. Both are the same rule: a modal dialog raised
  from inside `tt_ctl_service` parks the window on a box nobody is looking for,
  with the requester blocked behind it. Upstream opens the dialog, which is
  right when a person clicked.
- **`ttpmacro`'s `/V`, `/I` and `/S` do nothing.** All three describe the
  control window of a second process, and there is no second process: this
  `ttpmacro` parses the command line and then asks a window to run the file.
  `params[0]` is likewise the file and its parameters rather than the line as
  typed, because what the window can see is what was sent.

**One new ABI function and one new field**, both because the seam could not
otherwise carry a command line. `tt_cmdline_parse_line` is `parse_argument` —
the arm that prepends upstream's dummy program name and passes NULL for the DDE
topic — which is what a macro's `connect` and a socket's `connect` are both
given; the ABI could parse `argv` and nothing else. And `TtCmdLineInfo` gained
`dde_topic`, which had been parsed since the command line landed and had
nothing to be.

Four suites cover it and each proves something the others cannot:
`cargo test -p tt-ctl` is the wire and the address; `--test cli` runs both
binaries as subprocesses against a real session; `tests/abi.c` drives the C
side with `sprintf` and a `sockaddr_un` and no JSON library, which is as close
as this suite gets to the shell script the design is for; and
`shell/tests/control_test.cpp` runs it against a real `MainWindow`'s event
loop, which is the only place the notifier, the nested loops and the window's
own four callbacks meet.

### ⬜ Stage 3 — Windows parity (3–4 months, ~15k LOC)

Windows build, ConPTY, Win32 serial edge cases, NSIS installer. All 14 `.lng`
languages wired through unchanged. VT320/VT525 depth and DEC private modes.
Tabs and sessions; session duplication as an in-process concept rather than
`CreateFileMapping`. Built-in HTTP/SOCKS proxy replacing `TTProxy`. Printing.

### ⬜ Stage 4 — depth and polish (4–6 months)

DEC special graphics (line drawing — not CJK, and needed), macro reference docs,
Lua plugin API, sixel, self-updater. **No deb** — the AppImage-only decision in
Stage 1 covers this too.

**Realistic total to a credible replacement: 15–20 months solo with AI
assistance.** Full parity is 3+ years and should be explicitly renounced in the
README.

---

## Dropped permanently — say so in the README

| Thing | LOC | Why |
|---|---:|---|
| Tek 4010 (`ttptek` + `tekwin.cpp`) | ~2,900 | No one has a storage-tube workflow in 2026 |
| **TTX C plugin ABI** | — | `common/ttplugin.h` hooks are literal Winsock (`Pconnect`, `PWSAAsyncSelect`) and Win32 file-API function tables plus raw `HMENU`. Unportable by construction |
| Susie image plugins | 957 | A 1996 Win32 codec DLL ABI |
| DDE | 2,600 | → `ttctl` JSON-RPC; strictly better and cross-platform |
| SSH1 | — | Broken by design since 1998 |
| `ttpmenu.exe` | 4,831 | It's a launcher; the desktop has one |
| `cygterm` | 2,200 | Superseded by `portable-pty` (ConPTY / forkpty) |
| Win7 jump lists (`winjump.c`) | 810 | Windows-only chrome |
| `ttpcmn` shared-memory IPC | 2,865 | Single-process design removes the need |

**Kept but never rewritten:** B-Plus and Quick-VAN. Tera Term is essentially the
last implementation on earth — no counterparty to test against and nothing to
learn from rewriting them. Vendor the C, mark them best-effort.

---

## Compatibility and migration

Adoption hinges on "my existing setup just works." Budget real time here.

- **`TERATERM.INI`** — ✅ **done 2026-08-08**, `crates/tt-config/`. Read *and*
  written, and held bug-compatible with `GetPrivateProfile*` against a recorded
  real implementation rather than against a reading of the documentation: 98 of
  `ini-audit/`'s 104 cases match byte for byte and the six that do not are
  deliberate, each with a reason on file. **This entry used to say "no quote
  stripping", and that was wrong** — a matched pair is discarded. Hand-rolled,
  as the plan said: a generic INI crate gets the duplicate-key rule, the quote
  stripping, the empty-value rule and the comment rules wrong, and every one of
  those is a setting the user never changed, changing. New settings go in an
  additive section so round-tripping with real Tera Term survives. **Wired to
  the running terminal and to a dialog on the same day**: the shell reads
  `$XDG_CONFIG_HOME/sterna/sterna.ini` — Tera Term's format, in the place a
  Linux configuration file belongs, since the executable may be inside a
  read-only AppImage — and `Setup > Save setup` writes it back, touching only
  the keys the schema owns.
- **`KEYBOARD.CNF`** — it's an INI. Read as-is, 1–2 days.
- **Hosts and keys** — read Tera Term's `ssh_known_hosts` *and*
  `~/.ssh/known_hosts`; read `~/.ssh/id_*` and `~/.ssh/config`; write OpenSSH
  format.
- **`.lng` files** — keep the exact format. Do **not** migrate to Qt `.ts`: that
  throws away 17,610 lines of donated translation (14 languages × ~1,150 keys)
  and the translator workflow.
- **TTX plugins** — replace in order: (1) fold the ones that matter into core —
  `TTXProxy` (~1k Rust), `TTXKanjiMenu`, `TTXResizeMenu`, `TTXttyrec`; (2) a
  **Lua plugin API** — menu items, key bindings, connect/disconnect hooks,
  byte-stream filters, settings pages, covering what the 17 samples in
  `TTXSamples/` actually do; (3) WASM component plugins only if someone asks.
- **Docs** — 751 HTML files / 97k lines, 214 of them macro reference. Convert to
  Markdown mechanically; **generate** the settings and macro references from the
  schema and command table.

### TTL: reimplement, don't shim or transpile

TTL is BASIC-shaped — `:labels` with `goto`, one-line `if…then`, an untyped-ish
variable model, 1-based string indexing, and `wait`/`pause` with timeout
semantics stateful against the connection. You cannot shim `goto` into Lua
honestly, and the moment a real `.ttl` fails you've lost the only reason to care
about TTL. Transpiling means incomprehensible errors and owning a
source-to-source compiler forever.

The 232 reserved words in `ttpmacro/ttmparse.h` sound worse than they are: ~42
are keywords and operators (~40 grammar productions); the other ~190 are library
commands of 5–30 lines each mapping 1:1 onto core API calls. Sizing:
lexer/parser/AST 1.5k, interpreter 1.5k, string/int/array builtins 1.5k,
file/dir 1k, connection/terminal 2k, dialogs 0.8k, misc 1k — **~9.3k Rust vs
16.5k C**.

---

## Verification

1. **✅ Differential testing against real Tera Term** — `oracle/` built and
   green, and as of Stage 1 actually wired up: `./run_diff.sh` feeds identical
   byte streams to it and to the Rust engine and diffs the grid dumps *and the
   replies*, in CI on every commit. 103 cases. Since the oracle also takes
   injected mouse, focus and **key** events — and compiles `keyboard.c` for the
   last of those — this covers both halves of the frontend seam. **This is
   the asset the whole project rests on**, and it is now a gate rather than a
   promise.
2. **✅ esctest2** (Dickey's fork of iTerm2's) — 568 conformance assertions over
   a pty, in CI as of 2026-08-08. **365 pass**; every one of the other 203 has a
   written reason in `esctest/expected`, and the gate is *drift* from that file
   rather than a pass rate — a test that starts passing is as much a diff as one
   that starts failing, so a stale entry cannot outlive what it describes.
   See `esctest/README.md`.

   Two things had to be built first, and the second is a decision. **`tt-host`**
   is a terminal with no window — the same `tt-session`-over-pty loop the Qt
   shell runs — because esctest is not a recording: it runs *inside* the
   terminal and reads answers back. And **DECRQCRA**, the rectangular-area
   checksum, which is the only way to read a cell over the wire and **the one
   sequence in `tt-vt` that is not upstream's**: `vtterm.c` has no `CSI * y` at
   all, so it is off by default and only the harness turns it on.

   The plan's guess about the mechanism was wrong in a way worth recording:
   this reads the screen through DECRQCRA, not "via DSR/DECRQSS". Had that been
   checked earlier, the DECRQCRA prerequisite would not have been a surprise.
3. **⬜ vttest** (Dickey) — interactive; manual gate plus screenshot diffing at
   each stage boundary.
4. **🔵 Tera Term's own corpus** — `./run_upstream.sh` runs the escape-sequence
   exercisers in `teraterm/tests/` headless and diffs the two engines over
   their output. Not golden files and not copied into the repo: the scripts are
   executed from the pinned sibling checkout, so the corpus tracks upstream.
   **19 matching, 2 known-divergent, 6 not run**, each with a recorded reason in
   `oracle/upstream.cases`. The prediction above was accurate — `bcetest.sh`,
   `decfra.sh` and the `#38168-deccara-*.sh` trio were among the breakages, and
   three of them turned out to be upstream bugs rather than ours. Of the two
   still divergent, one is `vte` following ECMA-48 where Tera Term's CSI parser
   does not (see below) and the other is spacing combining marks, deferred with
   CJK. **Two of the nine original xfail notes named the wrong cause** — a
   reminder that an xfail reason is a hypothesis until something re-tests it.
   Still to do: the 53 `.ttl` files as the TTL conformance suite, in Stage 2.
5. **✅ Fuzzing and property tests** — `crates/tt-fuzz/`, 2026-08-08. All four
   named invariants are asserted, `cargo-fuzz` runs three targets over the
   parser and the telnet decoder, and the whole thing found **five real bugs on
   the day it was written**, in an engine that had been passing every other gate
   for a week. See `crates/tt-fuzz/README.md`.

   **The property worth recording is not on the plan's list: where the chunk
   boundaries fall must not change the result.** It is not a theoretical
   property — bytes arrive from a socket or a serial port in whatever sizes the
   kernel felt like, so *every* stream is already a chunked stream, and every
   other test in this repository feeds a whole file. That one property found two
   of the five, including the worst: **`vte` 0.15.0 silently drops a byte when
   it resumes a partial UTF-8 sequence.** Its `advance_partial_utf8` prints only
   the first character it decoded and then reports `valid_up_to()` as consumed,
   so anything complete in between is lost. `tt-vt` now holds partial sequences
   back and `vte` never sees one — which is where that decision belonged anyway,
   since `rewrite_c1` already has to know where sequences begin and end.

   The other finding worth carrying forward is a **limit of the differential
   gate**, and it is the first one found: **the dump cannot see width classes.**
   A wide character whose halves have come apart renders exactly like one whose
   have not, so `run_diff.sh` answers `ok` to a broken grid. Dumping upstream's
   `AttrKanji` does not fix it — the bit is set on one write path and not the
   other and is never cleared by a crush, so upstream's own copy is incoherent.
   `Grid::check_wide_pairs` is the only check covering that ground, and it
   caught a real bug there that nothing else could.

   Split deliberately: the **libFuzzer half needs nightly** and runs weekly,
   while the properties and the replay of the corpus and of every committed
   crash artifact are ordinary stable tests gating every push. The fuzzer
   explores; the replay is what stops a fixed bug coming back.
6. **✅ Protocol interop** over a pty: `sz`/`rz`, `sb`/`rb`, `sx`/`rx` (lrzsz)
   for x/y/zmodem, `gkermit` for kermit. Built and green — `xfer/run_tests.sh`,
   10/10 both directions. Use **G-Kermit, not C-Kermit**: C-Kermit sees a pty as
   a tty and drops into interactive mode. Wire it into CI alongside the oracle.

7. **✅ The perf gate** — `bench/`, 2026-08-08, calibrated the way
   `../tine/docs/BENCH.md` describes. Cold start, idle RSS, 10 MB of `cat`, and
   keystroke latency, plus the engine's own throughput on three workloads. The
   numbers are in the README, which is where the plan said to put them.

   **The two halves gate differently, and that is the whole design.** The core
   half is a Rust binary with no window in it, so CI runs it — against an
   *absolute floor* an order of magnitude below a real measurement, which
   catches an accidental quadratic and cannot flake because a shared runner had
   a bad minute. The shell half needs Qt 6.11.1 and a real compositor, so it is
   local only, gated against a same-machine baseline with per-metric budgets.

   Two things it cost to find. **The calibration loop corrects for a slower
   machine, not a busier one**: the first baseline was recorded while a build
   was finishing and came out 14% under the truth while the calibration was
   1.5% slow — a permanently weaker gate that nothing downstream could have
   detected. And **`QFile` cannot read `/proc`** and does not say so, because
   `atEnd()` answers from `size()` and every generated file reports zero, so
   the idle-memory probe confidently measured 0.0 MB.

   The finding, which is about the shell rather than the benchmark, is below.

### Measured — the real shell, 2026-08-08

`bench/baseline.json`. AMD Ryzen 7 7840HS, Fedora 44, Qt 6.11.1, Wayland, a
Release build out of the build tree.

| | |
|---|---|
| exec → first frame | 68 ms |
| idle RSS / PSS, a shell attached | 64.5 / 40.5 MB |
| keystroke → the frame that shows it | 1.03 ms |
| 10 MB out of a pty, painted | 39 MB/s (~390 frames) |
| the engine alone: plain / sgr / fullscreen | 67 / 74 / 84 MB/s |

The synthetic spike below predicted the shape of all of this and was right about
the important part — ~60 MB is Qt's floor and the no-GPU decision holds. What it
could not predict is the one number that turned into a finding, on the day the
gate was written.

**Throughput through the window was dominated by how many frames got painted,
and on X11 that was 8x too many.** The session pumps once per wake of its
notifier, so a burst arrives as one damage per 8 KB read, each on its own turn
of the event loop — one frame per read, and a frame costs about what parsing
8 KB does. Wayland's frame callbacks were already coalescing about eight reads
into a frame; X11 has no such brake and neither has the offscreen platform, so
the same code measured 4 MB/s and 39 MB/s on one machine.

`TerminalView::requestRepaint` now puts a floor of 8 ms under the frame
interval — 125 a second, above any display refresh this will meet:

| platform | frames before | before | frames after | after |
|---|---:|---:|---:|---:|
| wayland | 389 | 27 MB/s | 399 | 40 MB/s |
| xcb | 3006 | 4 MB/s | 431 | 36 MB/s |
| offscreen | 2914 | 7 MB/s | 332 | 42 MB/s |

**It is a floor, not a timer in the idle path**, which matters because the
absence of one is this event loop's whole design. An idle window has not
painted for a long time, so a keystroke still repaints on the spot — 1.03 ms
before, 1.05 after — and the timer exists only while output is outrunning the
floor, in the same way the pending-out retry does.

The obvious alternative was rejected: giving `tt_session_pump` a time budget so
that one wake consumes several reads. It reads until the line is quiet, and on
a transport whose reads *block* — serial and telnet both use a 50 ms timeout —
the second read of a burst can block the UI thread for 50 ms. Coalescing the
frames costs nothing and does not care what the transport is.

Two things survive from before the fix. **A headless number understated the
desktop by 4x**, the opposite of the usual assumption about offscreen being the
fast case, which is one more reason the shell half of the gate is local. And
the spread across platforms is now 15% rather than 9x, so a throughput figure
still has to name its platform — just not as loudly.

**And it turned up a second bug by retiming the shell.** `pty_test` began
failing intermittently on the assertion that a dead shell explains itself: the
kernel closes a dying process's file descriptors — which is what makes the pty
master read `EIO` — *before* it makes the process waitable, so `try_wait` could
be asked microseconds too early and the window said "Disconnected" instead of
"bash exited with status 4". It had been passing over that race for a week.

### Measured baseline — Qt 6 Widgets, 2026-08-07

A bare `QWidget` painting an 80x24 grid with `QPainter` — no GPU, no damage
tracking, no glyph atlas, per-cell `drawText`. A floor, not a ceiling.
**Measured on Qt 6.11.1 / Fedora 44, which is what the desktop actually runs**
(the `sterna-fedora` distrobox, see `CLAUDE.md`).

| | X11 | Wayland |
|---|---|---|
| exec → first paint | 90 ms | 60 ms |
| idle RSS | 59 MB | 62 MB |
| full repaint, every cell dirty | 3.9 ms (255 fps) | 3.9 ms (258 fps) |

**The no-GPU decision holds comfortably.** 255 fps of full-screen repaint, with
none of the obvious optimisations applied, is above any display refresh rate and
about 40x what a 115200 baud link can dirty (11.5 KB/s ≈ 6 screenfuls/s). The
GPU would be spent on the non-bottleneck, exactly as assumed.

**~60 MB idle is Qt's floor, and it is worth stating plainly.** That is mid-pack
among modern terminals but well above Tera Term's Windows footprint, and
"light" is the reason this project exists. It is imposed by the toolkit, not
something the code can optimise away later. Publish it in the README rather than
meeting it in a review.

**Correction: an earlier EGL finding did not survive contact with the real Qt.**
Measured first in the Ubuntu 24.04 container on Qt 6.4.2, the Wayland plugin
loaded Mesa's gallium driver and cost 62 MB of *private* memory, apparently
fixable by steering `QT_WAYLAND_CLIENT_BUFFER_INTEGRATION` off EGL. **None of it
reproduces on 6.11.1** — Mesa is never mapped at all, Wayland costs 3 MB more
than X11 rather than 62, and the variable changes nothing. It was a packaging
artifact of an old Qt. Had it shipped, Stage 1 would have carried a permanent
piece of cargo cult in its startup path. Risk 6 below, caught live on its first
outing; the 6.4.2 numbers (30 ms start, 32 MB RSS) were also wrong by roughly 2x
in the flattering direction.

A colour-run batching variant was measured too and is inconclusive by
construction: the synthetic data changes colour every cell, so runs are length 1
and it collapses into the per-cell path. Real console output has long runs, so
batching can only help.

---

## Risks, ranked by how likely they are to kill the project

1. **Scope. This is the one that kills it.** The failure mode is 18 months
   producing a terminal 90% as good as three existing ones and 40% as good as
   Tera Term. Stage 1 must be narrow and must beat everything else on Linux at
   exactly one thing: **GUI serial console work with real scripting.** If Stage 1
   slips past 5 months, cut features, not the ship date.
2. ~~**Motivation cliff at the dialogs.**~~ — **mitigated 2026-08-08.** 76
   dialogs arrive right after the fun part ends, and the mitigation had to
   exist before it was needed. It does: one schema, and a Qt dialog that builds
   itself from the metadata over the C ABI rather than being generated as C++.
   Adding a setting is a line in `schema/settings.txt` and a citation. What is
   left is 560 lines of that, which is tedious rather than risky — and the risk
   this entry was really about, that the *machinery* would be attempted at the
   moment morale was lowest, is gone.
3. **Old-device SSH behaviour — accepted, not closed.** Spike 5 proved the
   *algorithms* work; it could not test real-device *behaviour*, because there
   is no old device to test against. Non-RFC banners, hang-ups on unexpected
   packets, 30-second key exchange on weak CPUs: all still unknown. **The
   mitigation is the trait seam plus a `libssh2` fallback, which is now the plan
   rather than insurance.** Unchanged by the transport landing: 15 green tests
   against OpenSSH and dropbear say nothing about a 2008 console server, and a
   green suite must not be read as "SSH is done". What the transport did add is
   the two things a real device is most likely to need — a legacy-algorithm
   switch and a generous connect timeout — and a `Transport` seam narrow enough
   that swapping the implementation is one file.
4. ~~**`serialport-rs` gaps**~~ — **measured and downgraded 2026-08-07.** Break,
   modem lines and hotplug all work; the real gaps are four small ones, three
   patchable through the raw fd and one (DSR flow control) that Linux does not
   have at all. The plan's instinct — "assume a platform-specific serial layer
   is needed, don't hope" — was right, but the layer is a few hundred lines, not
   a replacement. See the spike 4 result above. Still open: Windows, and the
   `CH340G_hw_flowctrl` case, which needs a CH340 adapter.
5. **Three build systems** (Cargo, CMake/Qt, vendored C). Mitigate: the `cc`
   crate compiles the C from Cargo, CMake touches only Qt, one `cargo-xtask` on
   top.
6. **Qt version skew in development.** The agent container is Ubuntu 24.04 with
   Qt 6.4.2; the desktop runs 6.11.1. Windowing works from the Ubuntu container,
   which makes it tempting to trust it for everything — don't. **This has already
   produced one false finding and one set of flattering-by-2x numbers**, both
   caught only by re-measuring; see the baseline above. Mitigation exists: the
   `sterna-fedora` distrobox runs Qt 6.11.1, matching the host exactly. Use it
   for anything the shell's behaviour or footprint depends on.

Dropped from this list: **IME/CJK**, formerly risk 3 and the item most likely to
invalidate the toolkit choice. Deferred out of scope, not solved.

**"Why not just use Wine?"** — concede the strong form: for one user it's the
rational zero-effort answer and works acceptably for telnet and SSH today. But
it fails at precisely the differentiator: Wine's serial passthrough has no
reliable `WaitCommEvent`, unreliable modem-line status, poor break signalling
and no USB-serial hotplug propagation. Wine is fine for the parts you don't need
and broken for the part you do.

**Adopt, don't build:** `vte`, `portable-pty`, `russh`, `serialport-rs`, `mlua`,
Qt 6, and tine's CI/packaging pipeline.

**Read, don't fork:** `alacritty_terminal` and `wezterm-term`/`termwiz` encode
*their* terminals' behaviour, not Tera Term's VT320/VT525 depth — which is the
thing being preserved. Note this argument got narrower when CJK was deferred: it
now rests on DEC depth alone, so revisit it honestly rather than by habit if
adopting one of them would save real time. **Watch `libghostty`**: it is trying to
become a reusable terminal core with a C ABI, and if it stabilises before
Stage 3 it could replace `tt-vt` + `tt-grid` outright. Keep that seam clean
enough to find out.

---

## Reference: critical files in `../teraterm`

- `teraterm/teraterm/vtterm.c` — 5,939 LOC state machine; zero Win32 tokens.
  Port target **and** oracle.
- `teraterm/teraterm/vtdisp.h` — the renderer contract. 75 exports, only
  `DispStrA`/`DispStrW` draw. Defines where the core/frontend seam goes.
- `teraterm/teraterm/buffer.c` — 6,143 LOC grid/scrollback semantics.
- `teraterm/ttpfile/filesys_io.h` — the `TFileIO` vtable, the one real interface
  seam; FFI boundary for the vendored protocol C. Sole impl `filesys_win32.cpp`.
- `teraterm/common/tttypes.h` — the 909-line `TTTSet`; source for the generated
  settings schema.
- `teraterm/ttpmacro/ttmparse.h` — TTL grammar and the 232 reserved words.
- `teraterm/common/ttplugin.h` — proof the TTX ABI is unportable.
- `tests/` — 53 `.ttl` scripts + 33 escape-sequence exercisers.
