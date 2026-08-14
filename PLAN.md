# Sterna — plan and status

Canonical roadmap. Update the status markers as work lands; this file is the
thing a fresh session should read first, together with `AGENTS.md`.

**Last updated:** 2026-08-14 · **Stage:** 4 complete, deliberate deviations
landing (`docs/deviations.md`) · **Commits:** 640

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
data *and* control lines. See `AGENTS.md` for the capability table and the three
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
│  tt-ttl      TTL interpreter, and `ScriptHost` — the shared table     │
│  tt-lua      mlua over the same one, which is why it is glue          │
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
| `TTProxy/` | 8,314 | **Deleted**, reimplemented in core — `tt-conn/src/proxy.rs` and `tt-config/src/cmdline/proxy.rs`, ~900 lines |
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

  **Release builds moved to GitHub Actions, 2026-08-13.** The Linux job now
  uses a digest-pinned `manylinux_2_28` image and builds Qt 6.11.1 from its
  verified source archive. Every ELF in the finished payload is checked for
  maximum `GLIBC_2.28`, `GLIBCXX_3.4.25` and `CXXABI_1.3.11` imports, giving
  the AppImage a real Debian 10 / Ubuntu 20.04 / RHEL 8 / Fedora 29 floor
  rather than inheriting the developer's Fedora. Linux and Windows packages
  build independently. The Linux runner keeps one prepared manylinux
  container across dependency setup, Qt and packaging, so setup is paid once.
  Qt itself uses one compiler worker after two measured 14 GiB on the 15 GiB
  runner and lost it to OOM; the application packaging still uses four. Native
  Windows runs the updater file-lock regression, and one final job creates a
  draft only after all three pass. The Ed25519 root stays local:
  `packaging/release.sh` downloads the exact GitHub-built bytes, signs their
  manifest, checks the six-asset set and publishes the draft. Linux bundles
  GLVND's four driver-neutral ABI frontends but leaves the actual graphics
  driver on the host.

  **Measured from the portable image:** 32 MB on disk in the local
  release-equivalent build. The prior Fedora-built image measured 46 MB RSS /
  39 MB PSS with a shell attached under Wayland and ~144 ms from exec to a
  mapped window; remeasure those two figures after the portable build ships.

  **Two of the three ways this fails are silent**, both now in `AGENTS.md`:
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

Three things it cost to find, all in `AGENTS.md`:

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
  gap between two columns of output selects the gap. Starting anywhere else
  also stops between one-cell and multi-cell characters while `DelimDBCS` is
  on, which is its default; the setting can remove that second boundary.
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
   the failure `AGENTS.md` warns about — "every stub is a place the oracle can
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

### ✅ Stage 2 — the differentiators — **COMPLETE 2026-08-10**

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
  ✅ **done**, 2026-08-09 — `crates/tt-lua/`: the whole of `ScriptHost` behind a
  `tt` table, with the wait family over `tt-ttl`'s own matcher. The extension
  picks the language at `tt_macro_start`, so every way in that already existed
  — Control > Run macro, `/M=`, `ttpmacro`, the control socket — runs a `.lua`
  with no further wiring. See below.
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
- **Settings schema + generated dialogs.** ✅ **schema complete** —
  `crates/tt-config/` (296 addressable settings over 273 upstream keys: all
  272 keys read directly by `ttset.c`, plus `UILanguageFile` read through its
  helper; 39 settings for the terminal,
  2026-08-08, plus the connection, serial and transfer ones the command line
  writes into, 2026-08-09, plus the whole log family, then the whole
  file-transfer family, then the seven the terminal and the two the *transports*
  were already honouring with no key to read, then the clipboard's sixteen,
  2026-08-09, then the bell's, the serial port's, telnet's, the scrollback's
  and the parser's own eight switches, then the painter's four draw-attribute
  switches, the custom ANSI palette, the URL family, the four menu keys and
  the window-position pair and its save switch, the unfocused-cursor switch
  alongside the live cursor renderer, the startup macro's one-shot launch
  state, OSC 52's remote clipboard permissions and notification, and the
  connection-close outcome pair, then the configured mouse pointer, the
  character-width word boundary, protected setup-file saves and the
  active/inactive window-opacity pair and the window-title format word, then
  the shell/menu/broadcast, raw-file, keyboard and font families, and finally
  the encoding, printer, TEK, debug and remaining compatibility keys,
  2026-08-10),
  the map onto a running terminal in `tt-session`, the schema as
  data over the C ABI, and a Qt dialog that builds itself from it.
  `tests/upstream.rs` extracts both lists and reports zero missing and zero
  invented keys rather than trusting this count. CJK, printing and TEK remain
  out of scope; their settings round-trip as compatibility data rather than
  pretending those subsystems exist.
  See below.
- `TERATERM.INI` and `KEYBOARD.CNF` readers. ✅ **both done** — `TERATERM.INI`,
  2026-08-08, held against a real Win32 rather than against a reading of the
  documentation; `KEYBOARD.CNF`, 2026-08-10, parsed through that same INI
  layer and wired through the session, C ABI, Qt shell and TTL `loadkeymap`.

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
discarded, which MSDN documents. Four more findings are in `AGENTS.md`, and one
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

The first 39 settings proved the machinery. Adding a row is a line and a
citation — and the log family, added later the same week, is what that claim
was tested against: seventeen rows, and the work was reading `ttset.c` and
`filesys_log.cpp` rather than anything to do with the schema.

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
- **The dialog writes only what changed.** Applying every field would pin all
  296 known settings into the user's file the first time it was opened, and a
  pinned setting stops following upstream's default for ever.
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
compiler, both recorded in `AGENTS.md`: upstream leans on `<windows.h>` for
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
- **`getspecialfolder` answers the ten XDG has and admits to the six it
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

**The no-BOM branch is platform-shaped.** `LoadFileU8C` tries `CP_ACP` first,
so the encoding of a macro file is a property of the Windows machine that runs
it: on a Japanese Windows a Shift-JIS macro reads correctly and a UTF-8 one
that happens to be valid CP932 is mojibake, and on a Western one almost every
byte sequence is valid CP1252. Windows now reproduces that conversion and its
keep-the-original fallback; Unix, where there is no ANSI code page to ask,
passes a BOM-less file through unchanged. The Stage 3 landing and its portable
test boundary are recorded in the Windows section below.

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

**The halves compose through a string, not a struct**, and that is the thing a
design would get wrong first. TTSSH hooks the parser, runs *before* Tera Term's
own, and blanks what it consumed out of the line — so `ssh://user@host/` is
rewritten **into** a bare `host:22` token, and that is the only reason Tera
Term's own parser can find a host in an SSH URL. `ssh::parse` therefore returns
the options *and the line it left behind*. There turned out to be three of
them rather than two — TTProxy hooks the same pointer and runs before TTSSH —
so `cmdline::parse_all` is what runs them in upstream's order.

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

**Settled 2026-08-11: `ComPort`'s bound is a cross-field rule, so it is not a
row.** The open item was that its ceiling is a *different setting* and its
answer is a reset to 1 rather than a clamp (`ttset.c:1223`) — an out-of-range
`ComPort=` therefore opens the **first** port, not the nearest legal one, which
is a distinction a hand-edited file makes and a clamp would hide. No schema row
can carry "bounded by whatever that other setting loaded", and the order is part
of it: `ComPort` is read at `:916`, `MaxComPort` at `:1218`, the test after
both. It lives in `Settings::normalize` beside the `Debug`/`DebugModes` rule
that was already there for the same reason, which means a loaded file and
`setsetting` agree and moving the *ceiling* re-tests the port under it.

Reading the two together found a real defect in the ceiling itself, which the
open item had been sitting next to. `serial.max_com_port` was `int(4..4096)`,
and upstream is neither of that row's two ends: `ts.MaxComPort` is a `WORD`, so
`GetPrivateProfileInt`'s result wraps in the assignment before either bounds
test runs. Below 4 the row gave the default of 256 rather than upstream's 4,
and `MaxComPort=-1` — which is what somebody writes meaning "no limit" — came
out as **4** where upstream gives 4096, the opposite end of the range. The
narrowing belongs to `ComPort` too, since the reset compares the narrowed value
and `ComPort=65538` is port 2 upstream rather than a number above every ceiling.
**And then the same question asked of every integer row found seventeen more.**
The narrowing turned out not to be a bound at all — it is the *field's* width,
it always runs first, and composing it into a variant per bound was heading for
an enum of pairs. It is now a `uint8`/`uint16` prefix on the ordinary integer
spec, orthogonal to the bound, which retired `Bound::Word`, `WordClamped` and
`WordAlias` and made every remaining combination free. The rows were then found
the way this repository finds transcription errors everywhere else: extract both
lists — every integer key's `ts` field out of `ttset.c`, every field's type out
of `tttypes.h` — and diff, rather than read.

All seventeen, and `AlphaBlend` is the one a user would have seen.
`ts.AlphaBlendInactive` is a **`BYTE`**, so the `max(0, …)`/`min(255, …)` pair
upstream applies next (`ttset.c:1467`) can never fire — it is dead code, and
the schema had copied it as `int_clamp(0..255)` because it is what the rule
looks like. `AlphaBlend=-1` is therefore an *opaque* window upstream and was a
fully transparent one here, and `AlphaBlend=256` is 0 rather than 255. The other
thirteen are keys upstream reads straight into a `WORD` with **no** bounds test
at all — `TCPPort`, `TelPort`, `TelKeepAliveInterval`, the two serial delays, the
four serial reconnect keys, two log-rotation keys, `SendfileDelayTick` and
`MaxBroadcatHistory` — so for those the narrowing is the whole of the rule and a
plain `int` row was simply storing a number the field could not hold. A port of
-1 is 65535.

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

**And a trap that cost a debugging round, now in `AGENTS.md`.** The diagnostic
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

Two commands are still refused, and the list at the bottom of
`tt-macro/src/host.rs` says so. `setserialdelaychar` and `setserialdelayline`
pace what is *sent*, and upstream paces it in `SendMem` — a queue between the
macro and the wire that this port does not have, with three other callers
waiting on it (a paste, `sendfile`, the File menu's send), so it wants building
once for all of them. The other item that used to sit here is done as part of
the title-format work, 2026-08-10: `setbaud` queues the same caption edge as
upstream's `WM_USER_CHANGETITLE`, and the frontend reads the new speed from the
live transport rather than from the file value the port may never have used.

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

Three things the wiring turned up, all in `AGENTS.md`:

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

#### And then a second language, which is what the trait was shaped for

`crates/tt-lua/`, 2026-08-09. The last Stage 2 item, and the cheapest of them:
about 1,700 lines including its tests, because `ScriptHost` had already been
built wide and shallow — one method per command that needs the world — and
that is exactly the shape a second language binds to. Nothing in `tt-macro`
knows Lua exists; the host written for the macro language carries it unchanged,
which `crates/tt-macro/tests/lua.rs` is there to prove.

**Lua is not a second TTL, and the reasoning is `PLAN.md`'s own reasoning
inverted.** The TTL section above refuses to transpile TTL *into* Lua because
you cannot shim `goto` honestly and the moment a real `.ttl` fails you have lost
the only reason to care. The same argument says a Lua binding must not be TTL
wearing Lua's syntax: there is no `result`, no `inputstr`, no 1-based string
indexing. A function returns its answer and a refusal raises, which `pcall`
catches — so `if tt.wait('$ ') then` replaces `wait '$ '` / `if result = 1`.
Anyone who needs TTL's exact behaviour has TTL, and it is a port.

**Only the terminal is exposed.** Roughly half of TTL's 231 reserved words
exist because the language had no standard library — `strlen`, `sprintf`,
`fileopen`, `getenv`, `int2str`, the ten checksums, the regex family. Lua has
those or has `string`/`io` to build them from, and shadowing them with worse
versions would be the wrong half of the trade. What is bound is every
`ScriptHost` method but two: `error`, because a Lua traceback is not one of
`ttmparse.h`'s twenty-one numbered codes, and `random_u32`, because
`math.random` is better and `math.randomseed` makes a test repeatable without a
host having to.

**Eight places it answers something upstream could not**, each stated where it
happens. `getmodemstatus` gives a table or `nil` rather than a bit mask, so
"the port could not be asked" is distinguishable from "all four lines low" —
upstream reports 0 for both and `result` 0 for both, because the arm that would
say otherwise is unreachable. `yesnobox` tells No from the close box.
`logopen` reports `true` for success, where TTL's `result` is 0 for success and
1 for failure and is the only command in the language that way round.
`setbaud 0` and `setflowctrl 7` are refused instead of dropped in silence. A
dialog does not end the script — upstream's `messagebox` halts the macro when
the window is closed because that dialog is the only control a person has over
a running `ttpmacro.exe`, and there is an End button here. And the three
commands that switch on the *first character* of a decimal argument take names,
since the fold already lives in the host's enums.

**Success is one value**, which is the convention that makes the rest compose:
Lua expands a call's last argument to all of its results, so a function
answering `line, nil` would put a `nil` into the argument list of whatever it
is nested in. `io.open`'s shape — one value when it worked, `nil` plus the
detail when it did not — is what makes `tt.send(tt.recvln())` mean what it
looks like. `tt.waitln` is the single exception and returns `line, index`.

**One entry point, and the extension picks the language.** `tt_macro_start`
runs `.lua` through `tt-lua` and everything else through `tt-ttl`, which
includes every extensionless name because `FitTTLFileName` has already made
those `.TTL`. So Control > Run macro, `/M=`, `ttpmacro` and the control
socket's `macro.run` all took Lua with no change: a frontend that had to know
which language a file was in would be asking the user a question the file
already answers. `MacroError` grew a `message` and a `code`, and `code` 0 is
"not one of upstream's" — the shell's error dialog drops the position for those,
because a Lua error carries its own `file:line:` in the message.

**Three things Lua needed changed about itself**, and the third is the one that
cost thought. `print` writes on the *terminal* through `disp_str`, with `\n`
expanded to `CR LF`, because stdout from a window launched off a desktop menu
is nowhere — the same silent-diagnostic trap as `qWarning` under journald.
`os.exit` is removed, since the script is a thread inside the terminal. And a
runaway loop still answers End: TTL checks `cancelled` once per line and Lua
has no such seam, so a debug hook does it every few thousand instructions. That
hook must be `'static` — `mlua` stores it in the `Lua` — so it cannot capture
the borrowed host every other callback here does, and it calls a *scoped*
function out of the registry instead. `pcall` catches an error raised from a
hook as readily as any other, so `Script::run` asks the host again at the
boundary; that makes the answer honest and cannot make the script stop sooner,
because Lua has no uncatchable error.

`mlua` is vendored (`lua54` + `vendored`), so Lua 5.4 is compiled from source
with `cc` rather than probed for with `pkg-config` — the two containers here do
not have the same packages, and a build that depends on which one it is in is
the thing this tree spends the most effort avoiding.

#### And then the log stopped being three hardcoded choices

`crates/tt-config/schema/settings.txt`, `tt-session/src/settings.rs`,
`tt-session/src/logname.rs` and the shell, 2026-08-09. Seventeen settings, 60
to 77, and the first family added since the machinery was built — which was the
point of building it, so the interesting part is what the *citations* turned up
rather than the rows themselves.

**The window had been assembling its own options and getting six keys wrong.**
`MainWindow` set `TT_LOG_TIMESTAMP_ELAPSED` in two places with a comment saying
those were `TERATERM.INI` keys and would become choices when the schema
existed. The schema existed. So `tt_session_log_start` now takes a **null
`options`** to mean "however the settings say", which is the only thing the
window has ever wanted, and the struct stays for whoever wants to override one
field. `LogTimestampFormat` is the reason a null is better than a filled-in
struct rather than merely tidier: it is a string, and a `#[repr(C)]` struct a
caller allocates cannot hold one.

**`LogDefaultName` is a template, and `teraterm.log` is what a template looks
like when nobody has edited it.** `FLogGetLogFilename` runs four passes over a
name before opening it — `strftime`, then `&h`/`&p`/`&u` for the connection,
then a sweep for characters a file name cannot hold, then a join against the
log directory. Anybody logging more than one console ends up with a
`&h-%Y%m%d.log`, and a port that took the name literally would put every
session in one file. `logname.rs` is that, and it is what makes `LogAutoStart`
possible at all: a log that starts by itself needs a name from somewhere.

**The finding that cost the most thought: there are two `strftime`s upstream
and they are not the same one.** A log *file name* is validated against
`IsValidStrftimeCode`'s Visual Studio 2005 table and handed to the C runtime; a
log *timestamp* goes through `ttstrftime`, which is Tera Term's own
twelve-conversion implementation. They disagree in **both** directions —
upstream's own `%N` works in a timestamp and is deleted from a file name, `%e`
is the other way round, and ten conversions (`%j`, `%p`, `%U`, `%W`, `%x`,
`%X`, `%z`, `%Z`, `%A`, `%c`, `%I`) work in a name and come back as literal
text in a timestamp. The shipped `LogTimestampFormat` ends in `%N`, so pasting
it into `LogDefaultName` silently loses the milliseconds. Both reproduced;
neither is documented upstream.

**Four more of the settings-trap family**, all now in `AGENTS.md`:
`LogRotateSize` is in bytes whatever `LogRotateSizeType` says, so scaling it
turns 1 MB into a terabyte; a `LogRotateStep` of zero is **ten thousand**
generations rather than none; `LogTypePlainText` is one byte and it is a BS,
which means the setting named after the log also decides what a macro's `wait`
matches; and `LogTimestampType`'s *empty* value consults a second key,
`LogTimestampUTC`, which is why the schema gives the empty string a variant of
its own.

`TIMESTAMP_ELAPSED_CONNECTED` became a variant rather than being folded into
the log's own clock. Upstream reads `cv.ConnectedTime` at every stamp, so a
log left open across a reconnect restarts its count — and the elapsed layout
gained `strelapsedW`'s leading days field, which had been quietly dropped.

Three of the seventeen are read and written and act on nothing, each saying so
where it is declared: `LogHideDialog` (this port has a status-bar indicator
rather than a progress window), `LogIncludeScreenBuffer` (the function upstream
does it with is two of the five upstream bugs on file) and `LogLockExclusive`
plus `DeferredLogWriteMode` (Win32 share modes and a writer thread).

#### And four keys that were in no Tera Term

`crates/tt-config/tests/upstream.rs`, 2026-08-09. The schema is a
transcription of `ttset.c`, and `AGENTS.md` already says that the way to check
a transcription is to extract both lists and diff them rather than to read
them. Nobody had done it for this one. Four of the first 77 keys —
`AltScreenBuffer`, `EnableUnderlineAttrColor`, `RemoteClearsBuffer` and
`WindowChangeSequence` — appear **nowhere in upstream's 157k lines**. The real
spellings are `AlternateScreenBuffer`, `UnderlineAttrColor`,
`ClearScrollBufferFromRemote` and `WindowCtrlSequence`.

**A wrong key cannot fail loudly**, which is why they survived: reading one
upstream never writes gives the default from a file that sets the setting, and
writing it puts a line in somebody's `TERATERM.INI` that their own Tera Term
ignores. Both halves are silent.

One of the four had a second defect hiding behind the first.
`UnderlineAttrColor` is `GetOnOff(..., TRUE)` and the schema said **off** — the
invented name meant nobody had ever looked at the call. So the same test now
checks every bool default against the last argument of its `GetOnOff`, which
is the whole of what a value in the file means (`ttset.c:344`). Every other
default in the schema was already right, `TCPPort`'s deliberate initialiser
trap included.

It also prints how far the transcription has got — 124 settings over 112 keys,
against the 256 `ttset.c` reads — so "the rest of the settings" has a number
that cannot go stale in a comment.

#### And then the file-transfer family, and a bound the schema could not hold

`crates/tt-config/schema/settings.txt`, `tt-session/src/xfer.rs`, the C ABI
and `shell/src/XferDialog.cpp`, 2026-08-09. Forty settings, and the second
family added since the machinery was built.

`xfer_options` had had a `_settings` with an underscore on it since file
transfer landed, and `tt-xfer::Options` carried upstream's defaults hardcoded
with a `ttset.c` citation each. Those citations turned out to be worth having:
the first test written here asserts that an *empty* file produces exactly
`Options::default()`, which is the only thing that would ever notice the two
independent transcriptions of upstream's defaults drifting apart.

**The timeouts needed a bound the schema had no way to say.** Every bounded
int in the file until now takes the *default* below its floor
(`ttset.c:615`) — that is `int(lo..hi)`, and it is why `TerminalSize=0,0` is
80x24 rather than 1x1. The three timeout sets do the opposite: `GetNthNum2`
and then `if (v < 1) v = 1`, a real clamp, so `XmodemTimeouts=0,0,0,0,0` is
five one-second timeouts rather than `10,3,10,20,60`. That is `int_min(lo)`
and `schema::floored`, and it is a separate spelling rather than a special
case of the other because the two disagree about exactly the values a
hand-edited file is likely to hold. `ZmodemTimeouts`' second field floors at
**0** rather than 1, because 0 is meaningful there: it is how "never time out"
is spelt on a network link.

**`XmodemOpt`'s default is plain checksum**, which is the `else` branch of an
`_stricmp` chain against an empty default (`ttset.c:1039`) — the same trap as
`CRReceive` and `BSKey`, and the eighth member of that family. Both this
port's `XmodemOpt::default()` and its XMODEM dialog had assumed CRC, and they
had assumed it *differently*, so a job built from `Default::default()` and one
built from the dialog could disagree. Now there is one answer. Upstream's
writer emits `checksum` (`ttset.c:2594`), a spelling its own reader has no arm
for; the value round-trips solely because anything unmatched takes the
default, and the schema keeps that spelling for that reason.

**And XMODEM's binary flag is not everyone else's.** `XmodemBin` is on,
`TransBin` is off, and `filesys_proto.cpp:324` derives XMODEM's *text* flag as
`1 - XmodemBin`. A port that folded them into one setting would ship XMODEM
translating line endings or ZMODEM not translating them, in silence.

Two more of the family, neither reproduced and both saying so where they are
declared: `BPAuto` rewrites `ts.Answerback` to B-Plus's own trigger from what
is nominally a transfer setting (`ttset.c:1132`), and `FTHideDialog` asks for
a progress window this port does not have.

The dialog now seeds itself through `tt_session_xfer_defaults` instead of
inventing `binary`, the block format and the raw capture's wait — the three
values it had hardcoded. `job.auto_start` stays hardcoded false and now says
why: it means "the peer's trigger has already gone past in the stream", which
is true of a transfer the terminal started by itself and never of one a person
picked from a menu.

#### And nine the terminal and the transports were already honouring

`crates/tt-config/`, `tt-vt`, `tt-session/src/settings.rs` and the shell,
2026-08-09. A different way of choosing what to transcribe next: rather than
taking the next family out of `ttset.c`, take the fields of `tt-vt::Config`
that `vt_config` was leaving at whatever `Config::default()` held, because the
schema had no key for them. There were seven, and three of them carried a
comment saying so. `vt_config` now names every field the schema can reach: what
is left taking `base` is the cell size the frontend measured, `decrqcra` for
the conformance harness, and `japanese`, none of which is a `ttset.c` key.

**Four were a line each.** `EnableANSIColor`, `DisableAppKeypad`,
`DisableAppCursor` and `MaxBuffSize`. Two of them have a shape worth knowing:

- `EnableANSIColor` is **not** a parse gate like the three colour flags beside
  it. `SGR 30-37` still stores its colour in the cell and `vtdisp.c:2417`
  declines to draw with it, so the screen is the normal pair while the buffer
  says otherwise. `Theme::applySettings` reads it for that reason — the flag
  was already in `Theme` with a comment about having no key. The two reports
  that would name a colour go quiet with it (`vtterm.c:4332`, `:4451`), which
  is how a host is told.
- `MaxBuffSize` is **two ceilings**. `buffer.c:511` caps the buffer's line
  count with it and `:4977` caps the *terminal's row count* with the same
  number, so `MaxBuffSize=30` is a thirty-row terminal in a window of any size.
  It is applied in that order — rows first, total after — so a small ceiling
  gives no history rather than negative history. It also needed an open-ended
  range: `ttset.c:1214` takes the default below 24 and has no ceiling of its
  own, which is `int(24..)`.

**One was a list, and it stayed a string.** `ISO2022ShiftFunction` is the only
key in `ttset.c` shaped this way — comma-separated names with `+`/`-` prefixes,
plus `on`/`all` and `off`/`none`, which assign the whole word — so a
`flags(...)` type in the generator would have served one row. `ShiftFlags`
already holds the nine names and their bits, and `parse_ini`/`to_ini` went
there. **The list starts from nothing whatever the default says**: the `"on"`
at `ttset.c:1875` is the string used when the key is *absent*, and a key that
is present starts the loop at `ISO2022_SHIFT_NONE`, so
`ISO2022ShiftFunction=-SS2` is a terminal with every shift disabled rather than
all but one.

**Two needed the schema to grow a spelling first.** `AcceptTitleChangeRequest`
and `TitleReportSequence` were booleans in `Config`, each with a doc comment
saying the real thing "needs the settings surface". `ttset.c:1568` reads the
first with a default of `overwrite` and then compares down a chain whose `else`
is **off** — so an absent key and a *misspelt* one are two different settings,
and the schema had one fallback, the default. `*` is the else arm now, written
`off/*=Off`, and `AcceptTitleChangeRequest=ovewrite` reads as a terminal that
ignores every OSC title, which is what the user's own Tera Term does with it.

That is the family this document keeps returning to — `CRReceive`, `BSKey`, the
flag words, `GetOnOff`, `/AUTOWINCLOSE=1`, `IdTitleReportEmpty`, `XmodemOpt` —
and the first member of it the schema could not express rather than merely get
wrong.

Three behaviours arrived with the enums, all of them upstream and none of them
obvious:

- `off` **discards** the host's title rather than hiding it (`vtterm.c:5112`
  gates the arm before `cv.TitleRemoteW` is touched), and it takes `CSI 22 t`
  and `CSI 23 t` down with it. So turning the setting on afterwards does not
  reveal a title that arrived while it was off.
- The window and the report disagree about an *empty* host title. The window
  falls back to the file's under every mode (`ttwinman.c:101`); the report only
  under `overwrite`, so `ahead` answers `CSI 21 t` with a **leading space**
  (`vtterm.c:2683`). Reproduced rather than tidied — the reply goes on
  somebody's wire.
- **`gettitle` cannot see the title the host set.** `CmdGetTitle` answers with
  `ts.Title` (`ttdde.c:646`) and `settitle` writes it (`:636`, clearing the
  host's under `overwrite`), while an OSC writes `cv.TitleRemoteW`. This port
  had both going through the parser, which is the obvious build and is wrong
  under `ahead` and `last` — and the test that asserted it was asserting the
  wrong thing.

`Vt::title()` is `remote_title()` now, with `window_title()` for the
combination: the frontend wants the second, the differential dump wants the
first, and `MainWindow` accordingly stops composing a title of its own. The one
thing it still decides is that `Title=`'s default is upstream's *product name*
and means "no opinion" rather than "Tera Term".

**And two more, one layer out, where the transports were doing the same
thing.** `TelnetParams::term_type` and `::speed` were `TelnetParams::default()`'s
— `TermType` and `TerminalSpeed`. The first made a divergence visible rather
than merely wiring it up: upstream ships plain **`xterm`** (`ttset.c:961`) and
this port had hardcoded `xterm-256color`, which is a defensible answer and not
one a constant should be giving, since it decides what every curses program on
the far end believes about the terminal. TTSSH has no terminal type of its own
— `ssh.c:8593` puts `ts.TermType` straight into the `pty-req` — so the one key
reaches both transports.

`TerminalSpeed` is a string in the schema for a reason no other multi-field key
has: **the second field's default is the first field's value.** `GetNthNum`
gives 0 for a field that is not there (`ttlib_static_cpp.cpp:1182`) and
`ttset.c:1946` then assigns the input speed, so `TerminalSpeed=57600` is 57600
in both directions. Two `int` rows would have to default the second to a
constant, and any constant turns that line into a terminal claiming two
different speeds. `telnet_params` has no `..default()` left either, so a field
added to `TelnetParams` is a compile error rather than a setting the file
cannot reach.

**And one divergence found on the way, which is a genuine engine bug rather
than a setting.** `vtterm.c:5109` is `case 0: case 1: case 2:` falling into one
arm, so **OSC 1 — "change icon name" — sets the window title**. This engine
took only 0 and 2, with a comment asserting the opposite, and nothing caught it
because no differential case had ever sent an OSC 1. Case 107 does.

#### And the clipboard, where a line break is not what it looks like

`crates/tt-config/`, `tt-grid`, `tt-vt`, `tt-session` and `shell/`,
2026-08-09. Sixteen keys — upstream's "Copy and Paste" page — and the first
family since the log where most of them had behaviour waiting for them rather
than behaviour to be written.

**The headline is one line of `clipboar.c` this port did not have.**
`NormalizeLineBreakCR` (`ttlib_static_cpp.cpp:535`, called at `:289`) maps
`LF` and `CR LF` alike onto a single **`CR`** before the brackets go on. A
terminal sends what a keyboard sends and the Return key is a CR; queueing the
clipboard's own bytes — which is what this did — puts a byte on the wire that
no key produces, under every `CRSend` setting including the default. It reads
as correct, because a newline is what a line ending is called everywhere else,
and it is the same mistake `Vt::encode_text` already had one layer up in the
control socket.

`TrimTrailingNLonPaste` cuts **every** trailing break rather than one
(`clipboar.c:55`), and it runs *before* the decision to confirm — so with it on
a copied line with its newline still attached is pasted without a question,
because by then there is no newline.

**`BracketedSupport` is a second gate on `DECSET 2004`.** `clipboar.c:265`
tests the setting and *then* the mode, so a host that asked for bracketed paste
gets an unbracketed one when the key is off. It ships on, which is exactly why
a port that omits it looks right until somebody turns it off.
`BracketedControlOnly` narrows it to a paste holding a control character, so a
pasted word goes bare and a pasted block does not.

**`EnableContinuedLineCopy` is two things and only one of them is copying.**
It is upstream's `logFlag`, the argument threaded through `CarriageReturn` and
`LineFeed` (`vtterm.c:675`, `:688`): TRUE for a CR or LF off the wire, FALSE
for the pair the terminal invents at a wrap, and with the setting on only the
invented pair is kept out of the log and the macro tap. So the key named after
*copying* decides whether a script's `wait` matches a wrapped line as the one
line the host sent or as the two the terminal drew — the same shape as
`LogTypePlainText`, which is named after the log and does the same thing to the
same tap. `Vt::carriage_return` had carried a comment since it was written
saying the argument was not needed because the port had not adopted the
setting; that comment was the marker for this work, the way three fields in
`Config` were for the last piece.

The other half needed the grid to grow `ATTR_LINE_CONTINUED` — `buffer.h:50`'s
own bit, set on the last cell of the row a break left and the first cell of the
row it landed on. Nothing draws it; the frontend reads it to join two rows that
are one line. Upstream gates the *clear* on the setting and sets the bit
ungated, which leaves the flag stale precisely where nothing reads it; clearing
always is the same terminal and a grid that means what it says.

**Two defaults are the opposite way round from every Linux terminal**, and
neither is a bug. `DisablePasteMouseMButton` ships **on** and
`DisablePasteMouseRButton` ships off (`ttset.c:1425`, `:1422`), so Tera Term
pastes on the right button and not on the middle. This shell had hardcoded the
X11 convention with a comment defending it; that divergence ends the way
`keyboard.meta`'s did — faithful by default, one line in the file away from the
other behaviour. Both pastes also move onto the button coming *up*, where
upstream does them. `SelectOnlyByLButton` carries a second half its name does
not mention: with it on, a middle or right button coming up over a standing
selection must **not** copy it (`vtwin.cpp:819`), which is the bug that arm was
added for.

**`ConfirmChangePaste` ships on and had nothing behind it, which is worse than
a setting that does nothing** — a safety feature the file claims is enabled. A
terminal cannot tell a host that what arrived was pasted, so a shell runs every
line of a pasted block the moment it lands; showing it first is the only
defence there is. `shell/src/PasteDialog.cpp` is that box, editable rather than
yes/no because upstream's returns the edited string and the common use is
deleting a trailing newline off something copied out of a wiki.
`ConfirmChangePasteStringFile` is the dictionary of extra triggers — one
substring per line, `wcsstr`, first hit wins — and `PasteDialogSize` is written
back when the box closes, which is the whole reason upstream made it a setting.

The first run of the new render tests **hung**, which was the feature working:
a right-button release pasted a clipboard the previous test had left holding
`one\ntwo`, and the dialog sat waiting for somebody to say yes. In a test that
is not a failure, it is a hang — the same shape as the modal dialog inside
`tt_ctl_service`.

`PasteDelayPerLine` needed a bound the schema had no spelling for. Every other
bounded int is `ttset.c:615`'s (below the floor takes the default) or
`:1822`'s (below the floor takes the floor); this one is
`min(max(0, v), 5000)`, a clamp at both ends, and it disagrees with the first
below the floor and with the second above the ceiling. That is `int_clamp`,
and it is the third of three rather than a special case of either.

Five of the sixteen are read and written and act on nothing yet, each saying so
where it is declared: `SelectOnActivate` (Qt delivers no `WM_MOUSEACTIVATE`),
`MouseSelectStartDelay` (which ships at 0, the behaviour there already),
`PasteDelayPerLine` (pacing means handing the send path a schedule),
`ConfirmChangePasteCR` (there is no `Paste<CR>` command to confirm) and half of
`ConfirmPasteMouseRButton` — the suppression is honoured and the menu it should
raise instead does not exist, so setting it gives a right button that does
nothing rather than one that offers a choice.

141 settings over 128 keys, 137 to go.

#### The bell, which is four settings deep before it makes a sound

`crates/tt-config/`, `tt-vt`, `tt-session`, the C ABI and `shell/`,
2026-08-09. Eight keys — seven for the bell and one for the other control that
answers back — and the first family in a while where the *engine* had nothing
at all: `0x07` was an empty match arm with a comment saying the oracle silences
it, and `0x05` was not an arm.

**A bell is not a beep, it is a governor with a beep behind it.** `RingBell`
(`vtterm.c:5791`) is three clocks and three settings, and it exists for the
case `teraterm-term.html` names outright: a binary file shown by mistake is
thousands of BELs, and a terminal that honours every one of them cannot be used
until it stops. Two things about it are surprising and both are the code rather
than the prose:

- **The bell that trips the limit still sounds.** The inner test decides the
  *next* one's fate and the switch that makes the noise sits outside it
  (`vtterm.c:5800`), so `BeepOverUsedCount=5` is heard six times. The manual's
  worked example says five.
- **Suppression measures quiet, not elapsed time.** The arm that finds itself
  suppressed assigns `now` to the clock it just tested (`:5796`), so every bell
  arriving during the silence pushes the end of it further out — a host beeping
  steadily is silenced until it stops and for `BeepSuppressTime` afterwards.
  The manual reads as a fixed delay. The port follows the code here rather than
  the manual, unlike the three places it does the opposite: a governor that let
  a runaway through every five seconds would not do the job it exists for, and
  there is no harm on the other side of the choice.

The governor is in `tt-session` and not in the engine, which is the same line
`SessionLog`'s timestamps are on: it needs a clock, and `Vt` having none is
what lets the differential suite and the fuzzers treat it as a function of its
bytes. So `Vt::take_bells` hands over a **count** — one step of the state
machine per BEL, because thinning the burst in the engine would leave the
terminal audible through the next one — and the session runs the governor
against a single `Instant` and emits at most one `Event::Bell`. Two beeps in
the same millisecond are one beep; two *steps* are not one step.

`BellRequests` carries a `reset` flag beside the count for the one part of
`ResetTerminal` that lives outside the engine (`vtterm.c:348` puts both clocks
back). It is also the one place the bell diverges, stated where it is declared:
a chunk holding a RIS *and* bells collapses to "reset, then this many", so
bells that arrived before the RIS are counted against the state it cleared.

**`Answerback` is stored as hex and this port had no decoder.** `Hex2Str`
(`ttlib.c:406`) reads `$` as the lead of a two-digit escape, which is how a
setting whose value is arbitrary bytes survives a file format with no way to
write a control character. Three of its behaviours are worth knowing and none
is an error: a digit that is not hexadecimal is **0**, so `$ZZ` is a NUL; a `$`
with fewer than two characters behind it borrows `'0'` for each one it is
missing, so a trailing `$` is a NUL and `$A` is `0xA0`; and `$` is the only
escape, which is why upstream's own `DelimList` default opens `$20!"#$24%…`.

That second reader is why `hex_decode` is in `tt-config` rather than beside the
answerback, and it turned up a setting that was already in the schema and
reaching nobody: **`keyboard.word_delimiters` was a hardcoded constant in
`TerminalView`**, decoded by hand into the source. So a user who changed the
key got the old list, and one who read the raw value got `$`, `2` and `0` as
three delimiters and no space among them. `Session::word_delimiters` decodes it
and `tt_session_word_delimiters` is its own ABI call for that reason — the
generic `tt_session_setting` hands back the file's spelling, which is exactly
what a frontend must not use here.

Upstream has **two** decoders and the difference is not cosmetic:
`Answerback` goes on the wire and is bytes, `DelimList` is compared against
what is on the screen and is characters (`Hex2StrW`,
`ttlib_static_cpp.cpp:837`). `$E9` is one byte in the first and U+00E9 in the
second. `hex_decode` and `hex_decode_str` are that pair.

**And a thirtieth defect, in the second of them.** `Hex2StrW` grows its buffer
in 512-`wchar_t` steps under a `wp + 1 > str_len` test and then writes its
terminator at `Str[wp]` *after* the loop — so a value whose decoded length is
an exact multiple of 512 puts a NUL two bytes past the allocation. Reachable
from `DelimList` and from `keyboard.c:856`, which runs user-defined key strings
through the same function. Found by reading, like the `ttpmacro` list, so it is
in this file rather than in `docs/upstream-bugs.md`.

The value is held in the file's own spelling and decoded at the point of use.
Upstream re-encodes on write (`Str2Hex`, `ttset.c:2156`), so a file saying
`Answerback=A` comes back `A` and one saying `$41` also comes back `A`; this
one gives the user their own line back, which is a divergence in the file's
text and not in its meaning.

It is also the one setting in the file another setting **overwrites**:
`ttset.c:1132` replaces `ts.Answerback` outright with B Plus's five-byte
activation string when `BPAuto=on`, a hundred lines after reading the key. A
file that sets both loses this one without a word.

**And a twenty-ninth defect, in `vtterm.c` and reachable from the wire.**
`RingBell(int type)` never reads `type`; it switches on `ts.Beep`. The only
caller that passes anything other than the setting is `ESC g` — GNU screen's
visual bell — which asks for `IdBeepVisual` at `vtterm.c:1561` and gets an
audible beep under the default, or nothing at all when the bell is off. So the
one sequence whose entire purpose is to flash the screen is the one that
cannot. Reproduced, because it is what a user of Tera Term sees, and written up
with the rest; it wants a `-Wunused-parameter` more than it wants a patch.

`ESC g` has a second consequence that is upstream and not a defect: it reaches
the governor *without* the `ts.Beep != IdBeepOff` test that guards the BEL path
(`vtterm.c:1077`), so a stream of them silently spends a terminal's allowance
while the bell is switched off.

`BeepOnConnect` is named after connecting and tests the port type first
(`vtwin.cpp:3018`, `:3658`), so **the serial console this project exists for is
the one link it never fires on** — and it bypasses `RingBell` entirely, so it
is always audible, never a flash, and neither thinned by the governor nor
counted against it. Written here as "not a serial port" rather than as
`PortType == IdTCPIP`, because upstream's three port types leave no case
between the two readings and a local pty is a link it has no word for; CygTerm,
which is the same thing there, arrives over a TCP socket and beeps.

The visual bell is upstream's mechanism and not a colour of its own:
`VisualBell` toggles `CF_REVERSEVIDEO` — DECSCNM's own flag — either side of a
`Sleep` (`vtterm.c:5784`), so `TerminalView` paints it as an XOR and a flash on
an already-reversed screen shows it the normal way round. The difference is
that upstream sleeps on the thread that is parsing and this is a timer, so
output keeps arriving underneath the flash.

One of the eight is read and written and acts on nothing, saying so where it is
declared: `NotifySound` belongs to the tray notification (`vtwin.cpp:725`),
which does not exist here.

149 settings over 136 keys, 129 to go.

#### And the serial port, which is the transport this project exists for

`crates/tt-config/`, `tt-conn`, `tt-session`, the C ABI and `shell/`,
2026-08-09. Nine keys — the two control lines, the purge on open, the break's
length, and the five-key reconnect state machine — and the family where the
*settings* half was the missing half: `tt-conn` has modelled `fDtrControl` and
`fRtsControl` since the serial spike and nothing could read them out of a file.

**The default of a control line is a sentinel, and it is the `TCPPort` trap
the right way up.** `FlowCtrlRTS` and `FlowCtrlDTR` are read with a default of
`-1` (`ttset.c:2034`, `:2042`), which is not a `DCB` value: it means "derive
from the flow control", RTS taking Handshake under `FlowCtrl=hard` and DTR
under `FlowCtrl=dsrdtr`. Unlike `TCPPort`'s default of `ts->TelPort`, the
derivation really does see the file — `FlowCtrl` is read at `:943`, eleven
hundred lines earlier — so here the read *order* is the answer rather than the
lie. The schema keeps the sentinel and `tt-session`'s `serial_params` resolves
it, the same call `connection.terminal_speed` makes and for the same reason:
the schema has no way to say "the default is another setting".

One consequence is upstream's and is a step further on. Upstream resolves at
load and its writer emits the concrete number, so **saving pins the line**: a
file that derived RTS from `FlowCtrl=hard` comes back saying `FlowCtrlRTS=2`,
and changing the flow control afterwards no longer moves it. This port keeps
the `-1`, which a real Tera Term reads exactly as it reads an absent key — a
divergence in the file's text that makes the file mean the same thing in both
programs for longer.

**And an out-of-range value discards every serial setting in the file, in
silence.** `CommResetSerial` puts `ts->FlowCtrlRTS` straight into the `DCB` and
never looks at what `SetCommState` said about it (`commlib.c:240`), so a
hand-edited `FlowCtrlRTS=9` makes Windows refuse the whole structure and the
port keeps the baud, parity, stop bits and flow control it already had. Not
reproduced — `pin_control` reads anything it does not know as Enable — because
the symptom is every other setting going missing at once and the cause is one
line nobody would look at. It is the same shape as `TermIDGetID` never failing:
upstream declines to error and the user gets a terminal that is quietly not the
one they configured.

RTS has a fourth value that DTR does not, and it is the one Linux cannot do
through termios: `RTS_CONTROL_TOGGLE` is half-duplex RS-485 keying, which is
`TIOCSRS485` rather than a `c_cflag` bit. **Probed on the rig rather than
guessed**: the FTDI Quad RS232-HS answers `ENOTTY` to even `TIOCGRS485`, so
there is no hardware here to test an implementation against and `PinControl`
carries the variant while leaving the line where the kernel put it. The mapping
is written down in the enum for whoever has an 8250.

`ClearComBuffOnOpen` decides whether what the driver already had is the
session's first bytes or is thrown away, and it is on. Off is a real choice on
a console server — that buffer is what the far end said before anybody was
watching, and often the only copy — which is why upstream marks the port
readable instead when it is off (`commlib.c:477`'s `cv->RRQ`). It gates the
purge on **open** only: Control > Reset port passes TRUE whatever the setting
says (`vtwin.cpp:4913`), so it is not the answer to "does resetting the port
clear it". The hardware test in `tt-session/tests/serial_loopback.rs` is the
only kind that can settle this: the setting acts on a queue a memory transport
does not have, and the two answers are otherwise identical from the session's
side.

**`SendBreakTime` found three durations in this port and none of them was the
file's** — 300 ms in `MainWindow.cpp`, 250 in `tt-macro`'s host under a comment
claiming that was upstream's, and whatever a caller of the ABI passed. Upstream
has exactly one break length and every way of asking reaches it: the menu, the
accelerator, and a macro's `sendbreak`, which posts the menu command through
DDE (`ttdde.c:801`) rather than carrying a length. So `tt_session_send_break`
lost its `ms` parameter — an argument no caller had a right answer for is the
same defect as `RingBell`'s dead `type`, one layer up and in our own code. Same
shape as the hardcoded word-delimiter list the previous pass found in
`TerminalView`, and the second time in two sessions that a constant turned out
to be a setting.

The five reconnect keys are carried and not yet run, said plainly where they
are declared: upstream drives the state machine from `WM_DEVICECHANGE`
(`vtwin.cpp:311`) and the Linux half is a udev monitor this port has not
built. Three things in them are worth knowing before it is:

- **"Illegal" is about the notification, not about a value.** Some drivers send
  `DBT_DEVTYP_DEVICEINTERFACE` and never the `DBT_DEVTYP_PORT` that would say
  *which* port arrived, so `AutoComPortReconnectDelayIllegal` is the longer
  wait taken when the port number is unknown and the reopen is a guess.
- **`RetryCount` is retries and the name is honest**, unlike
  `BeepOverUsedCount` — three is four tries. But an attempt where the port is
  still absent costs a retry without opening anything (`vtwin.cpp:475`'s
  `CheckComPort` guard), and the *last* attempt is the one allowed to raise the
  error box, because the suppression tests `retry_left_ != 0` (`:481`).
- The four `int`s are `WORD` in `tttypes.h:602`, so upstream truncates them to
  16 bits and a two-minute retry interval written as `120000` is 54464 ms
  there. Not reproduced; the schema has no type for it and the divergence only
  exists for values nobody means.

158 settings over 145 keys, 120 to go.

#### And telnet, where two keys turn out to be four states

`crates/tt-config/`, `tt-conn`, `tt-vt`, `tt-session`, the C ABI and `shell/`,
2026-08-10. Eight keys — the two that decide how much of the protocol is
spoken, the echo, the log, the keepalive, the two a *non*-telnet TCP port
applies to the terminal, and the confirmation before dropping a session. The
transport was built in Stage 1 and has been carrying hardcoded answers to five
of them since.

**`Telnet=off` is not a raw socket, and finding that out corrected the port.**
`ts.Telnet` becomes `cv->TelFlag` at open (`commlib.c:340`) and `TelAutoDetect`
becomes `cv->TelAutoDetect` beside it (`:323`) — *unconditionally*, with no
reference to the first — and then `ttcmn.c:590` reads
`!cv->TelFlag && cv->TelAutoDetect` and turns the framing on at the first
`0xFF`. Both keys ship on, so `/T=0` gives a session that starts as data and
becomes telnet the moment anything sends an `IAC`. The proof that this is
deliberate rather than an oversight is in TTSSH, which clears the flag by hand
(`ttxssh.c:981`) under a comment saying the line "should not be needed because
Tera Term's CommLib should find `ts->Telnet == 0`" — it is needed, because an
SSH stream is full of `0xFF`.

So the framing and the negotiation are two questions and there are **four**
answers, not two. The third is the one this port did not have: `Telnet=on` at a
port that is not the telnet port, which is IAC framing with not a word offered
— and that is the *ordinary* state of a console server, since the opening burst
goes out only when `ts.TCPPort == ts.TelPort` (`vtwin.cpp:3666`). `TelnetMode`
had been answering `Auto` there, so until a host happened to send an `IAC` a
`CR NUL` reached the terminal as two characters rather than as a line ending.
`TelnetMode::of` is the table, and `for_port` is now one call into it.

**`TelEcho` is not "echo locally".** It is "let the `ECHO` option decide", and
it works in both directions. With it off — the shipped state — `WILL ECHO` and
`WONT ECHO` change nothing locally, because both arms test it first
(`telnet.c:411`, `:497`); with it on, the negotiated state assigns
`ts.LocalEcho`, and the opening burst runs `TelChangeEcho` (`:845`) instead of
asking flat: ask the server to echo only if the terminal is not already doing
it, and ask it to **stop** if it is. So a `LocalEcho=on` file opens with
`DONT ECHO`, the opposite request from the default's. The setting and the
protocol state are one variable, which is the shape `ts.BSKey` and DECBKM
already had — hence `TransportEvent::LocalEcho` as an event at the two points
upstream assigns, rather than a state the session polls. Polling would re-assert
the transport's answer over the top of a host's `ESC [ 12 h` a moment after it
arrived.

**The keepalive measures quiet, not elapsed time**, and it is the second
governor in this port to do so after the bell's. `telnet.c:913` compares against
`cv.LastSendTime`, which `commlib.c:1062` stamps for every telnet send —
including the NOP itself — so a session being typed at sends none at all. It
also runs only where the burst ran, because `TelStartKeepAliveThread` is called
inside that same `TCPPort == TelPort` arm: a telnet-framed console port gets no
NOPs however the interval is set. Reproduced rather than tidied.

It needed something the frontend did not have. `Session::pump` runs when the
descriptor says bytes arrived, and an idle link produces no wakeup — which is
precisely the link a keepalive exists for. So `tt_session_tick` is a new ABI
call and the shell runs it on a one-second `Qt::VeryCoarseTimer`: the first
wakeup in the window's idle path, and the class comment that said there was
none has been corrected rather than left to be discovered.

**`TELNET.LOG` is one half of a conversation.** All eight `TelWriteLog` calls
sit directly after a `CommRawOut` and nothing on the receive path logs at all,
so the `>` leading each record has no inbound counterpart. A file that reads
like a negotiation trace holds only what Tera Term said, and building the
obvious thing — logging both directions — would produce a file upstream never
writes.

**`TCPLocalEcho` and `TCPCRSend` do not sit beside the terminal's settings;
they spend them.** `vtwin.cpp:3696` assigns `ts.LocalEcho` and `ts.CRSend` when
a non-telnet TCP connection opens and `:3589` puts `ts.LocalEcho_ini` and
`ts.CRSend_ini` back when it closes — upstream keeps a second copy of the file's
value for exactly this. Here `Session::settings` already *is* that copy, since
it holds the file rather than the live terminal. Two details that a
straightforward build gets wrong: **off is not a value**, so a connection where
the key was unset borrows nothing and gives nothing back and a host's own SRM
survives the disconnect (upstream's `TCPLocalEchoUsed`/`TCPCRSendUsed`); and
`TCPCRSend` moves the keyboard's line ending **without** moving LNM, because
`LFMode` is a separate variable seeded from `ts.CRSend` at reset and nowhere
else (`vtterm.c:285`). `SM 20` moves the pair; this does not, so DECRQM goes on
reporting mode 20 reset while Return sends CR LF.

`ConfirmDisconnect` is **TCP only** — both tests are `cv.PortType==IdTCPIP`
(`vtwin.cpp:1668`, `:4448`) — which is why `tt_session_link_kind` exists rather
than a bool: it is upstream's `cv.PortType`, and `BeepOnConnect` is conditioned
on the same thing. A macro's `disconnect` can raise the dialog upstream
(`ttdde.c:634` passes the argument through) and cannot here, which is the
deliberate divergence already written down for the control socket: a modal
dialog raised from inside a request holds the requester open.

One transcription note that cost nothing this time and is worth knowing: **five
of upstream's keys are written in a case their own readers do not use** —
`Historylist`, `Metakey`, `XmodemRcvCommand`, `YmodemRcvCommand` and
`ZmodemRcvCommand` against readers spelling them `HistoryList`, `MetaKey` and
`X/Y/ZModemRcvCommand`. Four of the five were already in the schema. It is
harmless only because `GetPrivateProfile*` matches key names case-insensitively
and `Ini` reproduces that, which `ini-audit/` measured rather than assumed.

166 settings over 153 keys, 112 to go.

#### And the scrollback, where the harness had to be widened first

`oracle/`, `crates/tt-config/`, `tt-grid`, `tt-vt`, `tt-session`, the C ABI and
`shell/`, 2026-08-10. Six keys — two the terminal spends on the buffer, two the
view spends on itself, two the wheel's — and the first family in a while where
the *differential suite* was the thing that had to change before anything else
could be trusted.

**The dump could not see the history, and one of the six is entirely about
it.** `--scrollback` was in the oracle's usage text and had never been
implemented; it is now, on both engines, printing the lines that have left the
page oldest first and numbered backwards from it. Off by default, so nothing
already in `cases/` moved. It found a bug on its first run.

**`BuffClearScreen` is a scroll and this port was erasing.** `buffer.c:4021` is
`BuffScroll(NumOfLines, NumOfLines-1)`: `ED 2` moves the whole page into the
history and comes back blank, which is why `clear` at a Tera Term keeps what
was on the screen and why people who use it expect that. `Grid::clear_screen`
filled the rows in place — and a blank page compares equal to a blank page, so
112 differential cases and 568 conformance assertions all passed over it.
Case 108 is the fix's proof and 109 records the half nobody would guess:
`DECSET 1049` calls `BuffClearScreen` on the way **in and out**
(`vtterm.c:3044`, `:3202`), so quitting `vim` leaves two pages in the history,
the screen before it and vim's last.

That is the second blind spot on file, after the wide-character pairing that
`Grid::check_wide_pairs` covers, and the two want reading together: **the
question to ask of a green case is what it cannot see.** This one is now
openable per case, which the other is not.

**Two of the six are named after something they do not do.**
`ScrollWindowClearScreen` sounds like the gate on whether a clear screen
scrolls, and `case 2` calls `BuffClearScreen` whatever it says; what the key
decides is whether an `ED 0` with the cursor at the home position is *promoted*
to a clear (`vtterm.c:1728`) — which is what `ESC [ H ESC [ J` is, and a good
many programs send that in place of `ESC [ 2 J`. And `ClearOnResize` clears on
a resize that changed no size, because the `BuffScroll` and the cursor-home sit
outside the `if (size changed)` block (`buffer.c:5028`). The consequence
reaches the harness: upstream makes a `BuffChangeTerminalSize` call on its way
to its first screen, so with the key on a blank page is in the history before a
byte has arrived, and `tt-dump` now makes the same call. It is also why DECCOLM
skips its own clear when the flag is on — the resize has already done one, and
upstream says so in a comment.

The oracle grew `--clearonresize` and `--noscrollwindowclear` to reach the
non-default half of each, the way `--crreceive` already did. Four cases,
108-114, and none of them needed a golden.

**`AutoScrollOnlyInBottomLine` ships off, and this port has been shipping `on`
since the viewport was built.** Upstream calls `DispScrollToCursor` from
`MoveCursor` and `MoveRight` on every step (`buffer.c:3794`, `:3805`) and
leaves `NewOrgY` where it was when the page scrolls (`:3866`), so output while
scrolled back pulls the reader back down — the thing the key was added to
switch off, in 2008. Reproduced rather than kept, on the rule this document
already applies to the two mouse-paste buttons: upstream's default is the
file's to change and is not a bug. It is the *minimum* scroll rather than a
jump to the bottom, which is invisible while a host prints lines and visible
when a full-screen program draws at the top; both are tested.

It cost one thing worth writing down. `Session::follow_scroll` was doing two
jobs — re-anchoring on a resize and following the cursor — and `set_settings`
called it for the first. With the cursor following added, opening the settings
dialog on a terminal whose cursor is on the last row snapped the reader back to
live. `reanchor_after_resize` is the half a settings change wants, and
upstream's `SetupTerm` does not call `DispScrollToCursor` either.

**The wheel had a hardcoded constant and a mode with nothing to act on it.**
`TerminalView` scrolled by `QApplication::wheelScrollLines()`, which is the
desktop's answer to a different question; `MouseWheelScrollLine` is Tera
Term's, and it applies **only to a notch that arrived alone** — `vtwin.cpp:2536`
multiplies under `line == 1`, so a flick fast enough to coalesce two notches
into one message scrolls two lines rather than six. Reproduced quirk and all,
because the alternative is a wheel that behaves differently here at exactly the
speeds people scroll fastest. The guard is `> 0` rather than a clamp, so a 0 or
a negative value is one line per notch. And it is the step for something with
no other name: over the title bar the wheel changes the window's opacity by
this many units of 255 (`vtwin.cpp:2500`) — one setting, two meanings, the way
`TelEcho` and `ts.BSKey` each have.

`DECSET 7786` was in the engine and unreachable: a host that asked for the
wheel as cursor keys got the window's scrollback instead. `WheelToCursorMode`
is four terms (`vtterm.c:5847`) and the frontend has no business assembling
them, so `tt_session_wheel_to_cursor` is the whole predicate and takes the
modifiers the way `tt_session_mouse` does — Ctrl under
`DisableWheelToCursorByCtrl` is the escape hatch that reaches the terminal's
own history while a full-screen program is up, and it is a setting rather than
a convention.

One of the six is carried and acts on nothing, said where it is declared:
`ScrollThreshold` is a repaint coalescer counted in lines (`vtdisp.c:3132`),
which is `TerminalView`'s 8 ms frame floor measuring the same thing in the unit
a compositor cares about.

And `run_abi.sh` had been red since the telnet pass: two checks still expected
`TT_TELNET_AUTO` at a port that is not 23, from before `TelnetMode::Framed`
existed. Corrected here rather than left.

172 settings over 159 keys, 106 to go.

#### And the parser's own switches, which found three older bugs under them

`oracle/`, `crates/tt-config/`, `tt-grid`, `tt-vt`, `tt-session` and
`tt-dump`, 2026-08-10. Eight keys, and the first family where the *engine* was
the thing that had to be corrected before the settings could be trusted rather
than the other way round.

The eight are `BackWrap`, `VTCompatTab`, `TabStopModifySequence`,
`UseInvalidDECRQSSResponse`, `TerminalUID`, `LockTUID`, `AutoInvoke` and
`MaxOSCBufferSize` — everything left in `ttset.c` that changes what the VT
state machine *does* rather than what the window draws. Two of them the engine
was already honouring with the default hardcoded and a comment saying so
(`BackWrap`'s dead arm in `Grid::backspace`, `VTCompatTab`'s in
`forward_tab`); four had no code at all; and two are sequences that were
unreachable.

**The oracle answered the tertiary DA with eight spaces.** `ts.TerminalUID`
was never set in `settings_defaults`, and a zeroed `char[9]` through
`_snprintf_s_l("!|%8s", …)` is a right-aligned empty string. Same shape as
every other missing default on the list, and it would have made the port
agree with a Tera Term that has no unit ID. `ttset.c:1688` validates as it
reads — eight characters, all hex, upper-cased — and falls back to
`FFFFFFFF`, so the default is also what an invalid key gives, in the file and
again at `vtterm.c:4567` when DECSTUI arrives off the wire. `LockTUID` ships
**on**, so as Tera Term ships that sequence is read and dropped: the identity
a terminal answers with is not the host's to change unless the file says so.

**Three bugs came out from under the family, all older than it and none
covered by a case.**

The first is the widest. **A broken multi-byte sequence is one U+FFFD per
byte** in Tera Term's decoder and one per maximal subpart in `vte`, so
`E2 82 'b'` was one replacement character here and two upstream, and every
wider sequence widened the gap — three for a cut emoji. Ordinary text on a
line that is dropping bytes, which is the case the replacement character
exists for. Case 97 could not see it, because the only broken sequence in
`cases/` was a *bare C1 byte* and that is one byte either way. The fix is four
lines in `rewrite_c1`, which already tracks where the sequence started because
`Vt::held` needs it.

The second: **an OSC's string is everything after the first semicolon**, and
`vte` splits on every one — so a window title of `a;b` had been arriving as
`a` since titles were implemented. `Vt::osc_string` is the join, and it is
also where `MaxOSCBufferSize` bounds the result, one byte short of the setting
because upstream's test is `StrLen + 1 < StrBuffSize`.

The third: **`CSI = c` never reached its arm.** `csi_plain` drops anything
carrying an intermediate — right, since running a sequence as its
no-intermediate namesake is worse than dropping it — and `vte` reports `?`,
`>` and `=` there because it has nowhere else to put a private marker. So the
tertiary DA was unreachable with its code sitting in front of us looking
correct, which is the second time this port has had a feature that was
"implemented" and dead. `CSI > 1 c` also answered where upstream stays silent:
the primary DA takes any parameter and the other two insist on zero.

**HTS is the one C1 that must not be folded**, and that is the design decision
worth keeping. `rewrite_c1` turns every 8-bit control into its `ESC` form so
one arm can answer for both spellings — but `TABF_HTS7` and `TABF_HTS8` are
*different bits*, so the fold is exactly what makes the setting unenforceable.
`0x88` now goes through raw and `Perform::execute` answers for it, which is
the only channel `vte` has that an `ESC H` cannot arrive on; the refusal stays
in `rewrite_c1`, where the byte is still eight-bit. The first attempt gated the
folded `ESC H` on `TABF_HTS7` and let each spelling through under the other's
key — and the first differential case for it passed anyway, because the stop
it set happened to be one of the default ones. Both HTS cases now put the
refused spelling somewhere the screen can show it.

Two more that are named after less than they do. `VTCompatTab` is two changes
and the second is not "leave the wrap alone" — `buffer.c:5211` stashes `Wrap`
and puts it back after `MoveCursor` has cleared it, so a tab on a line that was
already full comes out still full. And `AutoInvoke`'s G0→GL shift sits
*outside* the switch that handled the designation, so `ESC ( Z` invokes too,
and it is the one locking shift `ts.ISO2022Flag` does not gate.

Thirteen differential cases (115–127) and one `xfail` (128), which is the half
of the UTF-8 fix that cannot be made here: a sequence cut off by an **OSC
terminator** is decoded at stream level in this port and at string level
upstream, so upstream never sees the terminator break it and discards the tail
without a word. Fixing it means `rewrite_c1` knowing which parser state it is
in, which `vte` does not expose — the same wall case 51 hit for DEL. Costs two
replacement characters at the end of a title whose last character the sender
cut in half.

One thing the setting cannot do here, said where it is declared: **the
allocation bound is `vte`'s and is not enforced.** Under the `std` feature
`vte` collects an OSC into an unbounded `Vec`, so an OSC that never terminates
still grows without limit; `MaxOSCBufferSize` reproduces the *truncation*,
which is what the terminal shows, and not the ceiling, which is what the
setting exists for. Worth revisiting if the parser is ever forked — it is the
same fork the two `xfail`s want.

180 settings over 167 keys, 105 to go. The earlier 98 omitted seven keys read
through Win32's wide-character APIs; the extraction guard now covers those
call shapes too.

#### Four draw-attribute switches that the painter had hardcoded

`crates/tt-config/` and `shell/`, 2026-08-10. `EnableBold`,
`UnderlineAttrFont`, `UseTextColor` and `UseNormalBGColor`: four settings whose
entire effect is after the grid, where the differential dump cannot see it.
`render_test` is their oracle, with grabbed pixels rather than a second reading
of the code.

**Bold and underline are two switches each.** `EnableBoldAttrColor` and
`UnderlineAttrColor` choose the attribute's colour pair; `EnableBold` and
`UnderlineAttrFont` independently choose the bold or underlined face. All four
ship on, so the shell's hardcoded bold face and underline looked right until a
file turned one off. They now gate the font at the last point before a run is
painted, leaving the attribute in the cell and its independently enabled colour
untouched.

**`UseTextColor` is much narrower than its documentation sounds.** After both
explicit SGR colours have been applied, `vtdisp.c:2542` repairs a same-colour
pair only when the two indices match and the foreground is 0, 7 or 15 — black,
white or bright white. Red-on-red stays red-on-red. Under selection, SGR 7 or
DECSCNM it substitutes the configured *reverse* pair even when
`EnableReverseAttrColor=off`, because this arm runs after the ordinary reverse
colour gate. Both the ordering and the exception are pinned in the pixel test.

`UseNormalBGColor` is the simpler sibling: when a bold, blink, underline or URL
colour pair wins, use the normal text background instead of that pair's own
background. Reversal turns that normal background into the foreground; an
explicit SGR background still overrides it afterwards.

184 settings over 171 keys, 101 to go.

#### `ANSIColor`, where a drawing setting changes the parser's answer

`crates/tt-config/`, `tt-vt`, `tt-session`, the C ABI and the shell,
2026-08-10. One setting, but one that crosses every layer because Tera Term
stores a palette *index* in each cell: `SGR 38;2;r;g;b` resolves to its nearest
index while the escape sequence is parsed, and the painter later turns that
same index back into RGB. Giving only the painter the custom palette would make
those two halves disagree.

**The value is a small language with two C buffer limits.** `ttset.c:797`
reads at most 259 bytes into `Temp[MAX_PATH]`, divides complete comma-separated
fields into `(id,r,g,b)` groups, and then lets `GetNthNum` see only fourteen
bytes of each field through `char T[15]`. An ID is masked with `& 15`, each
channel narrows to `BYTE`, a failed number is zero, an incomplete group is
ignored, a duplicate ID wins last, and a partial list leaves the other live
entries alone. `tt-session::ansi_palette` reproduces all of those rather than
turning the value into a stricter list the user's own terminal would accept.

**The first sixteen entries have two orders.** `ts.ANSIColor[16]` uses the
legacy table in which 1 is bright red and 9 is dark red;
`vtdisp.c:GetIndex256From16` places those at 9 and 1 in the drawing table. The
session holds the already-permuted 256-entry table so `DispFindClosestColor`'s
ported search and Qt's painter consume one source of truth. Entries 16–255
remain the fixed xterm cube and greyscale ramp.

The C ABI now exposes `tt_session_palette_rgb`; the old `tt_palette_rgb`
remains as the compiled-in fallback for a caller that has no session. Qt asks
the live session whenever settings are applied and carries no parser of its
own. Rust tests pin the buffer and narrowing quirks, the C test pins the ABI
and legacy permutation, and `render_test` pins both a custom SGR colour and a
truecolor value resolved and painted through the same custom table.

185 settings over 172 keys, 100 to go.

#### URLs, where recognition, paint and clicking are three different switches

`crates/tt-config/`, `tt-grid`, `tt-session`, the C ABI and the shell,
2026-08-10. Eight keys: the colour pair, its enable, the independent underline,
the click gate, a custom browser and its arguments, and two split-URL settings
that current upstream reads and writes but never consults.

**`EnableClickableUrl` does not enable URL recognition.** `buffer.c:3430` runs
the detector on every character written, regardless of that key; the grid
therefore always carries `AttrURL`. `EnableURLColor` and `URLUnderline` decide
two independent parts of painting, while `EnableClickableUrl` — off in the
shipping file — decides only whether hovering gets a hand and double-clicking
launches. Treating it as one master switch gives three wrong terminals for the
price of one plausible name.

The detector is upstream's incremental one rather than a regular expression:
seven lower-case schemes, and the ASCII table from `isURLchar`. A URL may cross
an automatic wrap because `AttrLineContinued` joins the cells. **And it has a
visible pointer-zero edge.** When a URL begins in the allocation's first cell,
the rescan at `buffer.c:2658` stops at pointer zero and then increments anyway;
growing `http://` by one character clears every URL bit except the first.
`sftp://` and `tftp://` are stranger still because the rescan from character
two finds the `ftp://` suffix. Differential case 130 records the behavior
rather than cleaning it up invisibly.

Launching reads the marked run back instead of parsing it a second time. That
inherits an unrelated copy setting: `BuffGetStringForCB` joins a wrapped URL
only when `EnableContinuedLineCopy` is on, and with it off inserts the exact
`CR CR LF` sequence its clipboard path writes. `JoinSplitURL` and
`JoinSplitURLIgnoreEOLChar` sound like the answer and are not — no current
upstream code reads either after load. They round-trip here and act on nothing
for the same reason.

The custom executable is tried only for HTTP, HTTPS and FTP. SFTP, TFTP, NEWS
and MMS always go to the desktop handler, and a failed custom launch falls back
there too (`buffer.c:4084`). The Qt test captures the double-click without
opening a real browser and launches itself as a detached helper to pin argument
ordering.

193 settings over 180 keys, 92 to go.

#### The menu bar, where hiding it and getting it back are three keys

`crates/tt-config/` and the shell, 2026-08-10. Four keys: the ordinary menu
bar, the Ctrl+left-click replacement, the recovery command, and the dynamic
list of other windows.

**`PopupMenu` does not enable a popup.** It hides the ordinary bar;
`EnablePopupMenu` independently decides whether Ctrl+left-click opens the full
menu while that bar is gone (`vtwin.cpp:863`). `HideTitle` removes the menu bar
too, without changing `PopupMenu` (`:3461`), so the replacement predicate has
three terms rather than one. It also runs before mouse reporting upstream: a
full-screen program asking for Ctrl-modified clicks cannot take away the only
route back to the terminal's own controls.

There is still only one menu tree. Qt associates the menu bar's existing top-
level actions with a temporary `QMenu`, so enabled states, shortcuts and new
commands cannot drift between two copies. The matching mouse release normally
belongs to that popup's grab and is guarded from leaking to the host if it
comes back to the terminal view.

**`EnableShowMenu` is neither of those switches.** Upstream adds "Show menu
bar" to the Win32 system menu while the bar is hidden (`vtwin.cpp:3509`). A Qt
client cannot add application actions to the compositor-owned system menu, so
the shell puts the same recovery command at the bottom of the Ctrl+left-click
popup. Like upstream it clears only `PopupMenu`; a hidden title still keeps the
bar hidden.

`WindowMenu` ships on and is carried but acts on nothing. Upstream fills it
with every VT and TEK window; this process has one terminal window and no TEK
window, so Stage 3's multi-session UI is the first honest thing it could list.
The offscreen window test pins the three settings that do act now, including
the reuse of the menu bar's actual `QAction`s.

197 settings over 184 keys, 88 to go.

#### Window position, where remembering it has two different save paths

`crates/tt-config/`, the C ABI and the shell, 2026-08-10. `SaveVTWinPos` and
the two fields of `VTPos` cross the file, a live Qt window and a close event,
so treating them as three ordinary values would get both the read gate and the
thing being saved wrong.

**`SaveVTWinPos` gates writing, not reading.** `ttset.c:598` reads `VTPos`
unconditionally and only reads the switch ten lines later. A file with
`SaveVTWinPos=off` therefore still opens at its saved position; the old line is
merely left byte-for-byte alone on both Save setup and close. The generated
writer gained `write-if=` for that condition, rather than parsing and writing
the line back with its matched quotes stripped. When the switch is on, Save
setup writes every known setting and takes `VTPos` from the live window.

**A missing key and a missing field have different defaults.** With no
`VTPos`, `GetPrivateProfileString` supplies
`-2147483648,-2147483648` (`CW_USEDEFAULT` twice); with `VTPos=12`,
`GetNthNum` gets an empty second field and writes zero, making `(12,0)`.
`GetNthNum2`, used by the transfer timeouts, instead supplies its caller's
per-field default. The schema now spells those as `int_zero` and `int`
respectively. The audit also corrected the two earlier `GetNthNum` users:
`TerminalSize` and `PasteDialogSize` no longer borrow a field default that
upstream never gives them (the terminal-size range check subsequently turns a
zero row count into 24).

**Closing is not a shortened Save setup.** Upstream's `SaveVTPos` writes only
`VTPos` and `TerminalSize`, and does nothing at all when the switch is off
(`ttset.c:3338`). The C ABI has a separate close-only operation for that
reason. Both save paths snapshot the grid's live columns and rows: the settings
object is the last loaded configuration, while upstream's
`TerminalWidth`/`TerminalHeight` are live variables, so saving the snapshot
after dragging an 80x24 window to 132x50 would confidently put 80x24 back.

There are two platform edges. Upstream rejects a point that has fallen beyond
the virtual desktop, tolerates one up to twenty pixels above or left of it and
clamps that one to the edge (`vtdisp.c:1517`); the shell does the same, so
removing a monitor does not strand the next window. Wayland has no client-side
position request or meaningful `pos()` answer at all. Under Wayland the shell
neither applies nor replaces `VTPos`, preserving a useful X11/Windows value
while still saving the live terminal size. `/X` and `/Y` retain their later
command-line override on platforms which can place a window. The offscreen Qt
test pins restoration, both save paths, the write gate and the virtual-screen
edges; the C ABI test pins exact preservation for the Wayland-shaped call.

200 settings over 186 keys, 86 to go.

#### The cursor, whose file settings become host-controlled live state

`crates/tt-config/`, `tt-vt`, the C ABI and the shell, 2026-08-10. One new key,
`KillFocusCursor`, completed the family already holding `CursorShape` and
`NonblinkingCursor`; making the family act exposed an older parser hole under
it. The shell had painted a permanent block whatever all three said.

**An inactive cursor has a shape of its own.** `KillFocusCursor=on` does not
keep the selected block, bar or underline: `CaretKillFocus` (`vtdisp.c:1872`)
draws a full-cell outline regardless. Off means an unfocused window has no
cursor at all. The active vertical and horizontal forms use upstream's exact
two-pixel `CurWidth`, including a bar that stays two pixels wide on a
double-width character; only the block and underline span both cells.

**The painter cannot read the file values after startup.** With
`CursorCtrlSequence=on`, DECSCUSR replaces both shape and blinking and DECSET
12 replaces the latter, using the same variables `ttset.c` loaded. `TtCursor`
therefore carries the live style beside the live position rather than making
Qt assemble it from three raw settings. The blink phase follows Qt's desktop
caret flash time and its timer exists only while a blinking cursor is visible
in the focused live page; non-blinking, unfocused, hidden and scrolled-away
cursors leave no periodic wakeup.

That live test found **DECSCUSR had never reached the code which reported its
result**. Its space is a real CSI intermediate, and `csi_plain` correctly drops
unknown intermediates to keep a sequence such as `CSI $ r` from becoming its
plain namesake. DECSCUSR consequently needs its own `(space, q)` dispatch arm,
just as DECSCA and DECSTR do. All seven valid parameters, the gate and the
ignored invalid arm are pinned in `tt-vt`; the C test then proves the changed
style crosses the ABI, and the Qt pixel test proves both shapes, focus states
and blink choices reach the screen.

201 settings over 187 keys, 85 to go.

#### The startup macro, where “unset” means inherit rather than stop

`crates/tt-config/` and the shell, 2026-08-10. `StartupMacro` was already all
over the command-line model: `/M=` could name one, `/M` could ask for one and a
`/D=` topic produced a distinct `TT_MACRO_CLEARED` state specifically to cancel
the file's value. The file value itself was absent. `TT_MACRO_UNSET` and
`TT_MACRO_CLEARED` consequently reached the same no-op arm, under a comment
explaining a cancellation which had nothing to cancel.

All four states now act. Unset inherits `macro.startup_file`; cleared runs
nothing; prompt opens the existing two-language picker; file runs the command
line's override. The setting stays in the settings object after it is used:
the command line is one-shot launch state, and saving some unrelated change
after an automatic macro ran must not quietly erase the next launch's macro.

**The apparent connection order hides a second process upstream.**
`CVTWindow::Startup` launches TTPMACRO first with `/S`; the macro's DDE
`CmdInit` then posts `WM_USER_COMMSTART` (`ttdde.c:657`). Here the macro and
terminal already share an in-process link, so the shell starts the connection
attempt and starts the macro immediately afterwards. It does not wait for the
connection to finish: the first line is commonly a `wait`, and an idle `/DS`
window must also be able to run a macro whose job is to issue `connect` itself.

Relative names no longer depend on whatever working directory a desktop
launcher happened to supply. Upstream changes its process directory to
`HomeDirW` (`teraterm.cpp:135`) and resolves `/M=` there explicitly; Sterna
resolves both the file value and `/M=` beside the active INI instead of changing
the process-wide directory. TTPMACRO's other filename rules still apply: a
missing extension gets `.TTL`, and **any name whose first character is `*`**
opens the picker (`ttmmain.cpp:285`), not only a value equal to `*`. The Qt
command-line test pins inheritance, override and cancellation with relative
paths against an idle window.

202 settings over 188 keys, 84 to go.

#### The remote clipboard, where notification is not permission

`crates/tt-config/`, `tt-vt`, `tt-session`, the C ABI and the shell,
2026-08-10. `ClipboardAccessFromRemote` is the four-state permission behind
OSC 52 — off, read, write, or both — while `NotifyClipboardAccess` is an
independent switch over the notice. Tera Term ships with access **off** and
notification **on**, so a rejected attempt is visible rather than silently
giving a remote process the clipboard. `/OSC52=` is now the launch-time
override its parser already promised; an unrecognised value clears both bits.

The terminal parses and authorises the sequence but never touches the desktop.
It drains typed clipboard requests through `tt-session` and the flat ABI; the
Qt session handles them on the GUI thread, writes decoded text to the system
clipboard, or reads it and immediately returns an OSC 52 reply. Accepted and
rejected reads and writes retain upstream's notification distinction. The
shell renders those notices in its existing status surface rather than adding
a second notification subsystem for one setting.

**OSC 52's syntax has three quiet traps.** `Pc` accepts only `c`, `p`, `s` and
digits 0–7 before its semicolon. A read is only a payload equal to exactly
`?`; anything else is a write. And `b64decode` is not a strict RFC decoder: it
skips whitespace, stops at the first invalid byte (including `=`), and still
decodes an incomplete final group, so malformed input can write a valid prefix
or an empty clipboard. The port reproduces that rather than turning a remote
write into a parser error upstream never reports.

Replies preserve `Pc`, encode Sterna's UTF-8 text, and end in ST even when the
request used BEL. Upstream builds the prefix in `char hdr[20]`, so a selector
longer than thirteen bytes is accepted and notified but gets no reply; its
`IsTextW` check likewise allows an empty string and refuses binary controls.
The core, session, generated C header and Qt tests each pin their own side of
that boundary.

204 settings over 190 keys, 82 to go.

#### A disconnect either closes the window or clears its page

`crates/tt-config/`, `tt-session`, the C ABI and the shell, 2026-08-10.
`AutoWinClose` was already parsed — including `/AUTOWINCLOSE=` — but had no
window behavior behind it. `ClearScreenOnCloseConnection` was the remaining
key in the same branch. Both now act whether the far end disappears, a write
discovers the dead link, or the user chooses Disconnect.

**Auto-close is network-only.** `vtwin.cpp:3020` tests `PortType==IdTCPIP`, so
SSH, telnet and raw TCP request a window close while a serial port and a local
pty stay open. This is independent of `ConfirmDisconnect`: confirmation asks
before a deliberate TCP disconnect; auto-close decides what happens after it.
The core emits a close request rather than pretending it owns a window, and
the Qt shell retains upstream's `IsWindowEnabled` guard so a socket dying in a
modal dialog's nested loop does not close the disabled parent out from under
the dialog.

**Clear screen still means scroll.** When enabled, the live page moves into
history, a blank page takes its place and the cursor goes home — the same
`BuffClearScreen` path as Edit > Clear screen, not an erase in place. The core
does this before an auto-close request too: it is invisible when the window
closes and gives the right fallback when a disabled window cannot. The shared
disconnect path also restores TCP's borrowed echo/CR settings and ends a file
transfer on write-side disconnects, which the older write-error arm had
skipped.

205 settings over 191 keys, 81 to go.

#### The URL hand returns to the configured pointer

`crates/tt-config/` and the shell, 2026-08-10. `MouseCursor` chooses the
ordinary pointer over the terminal from `ARROW`, `IBEAM`, `CROSS` and `HAND`,
case-insensitively. `EnableClickableUrl` temporarily replaces whichever one
was chosen with a hand while the pointer is over a marked URL; moving away now
restores the configured pointer instead of the I-beam the shell had hardcoded.

**Four known values do not make this an enum in the file.** `ttset.c:1460`
copies the raw spelling into `MouseCursorName`, and `SetMouseCursor`
(`vtwin.cpp:159`) simply leaves the existing cursor alone when it recognises
none of them. The schema therefore keeps a string: lowercase names continue
to work, and an unknown hand-edited value survives both a save and a live
settings change without being normalised to the default.

206 settings over 192 keys, 80 to go.

#### Character width is an optional word boundary

`crates/tt-config/` and the shell, 2026-08-10. `DelimDBCS` controls the width
boundary in a double-clicked word rather than decoding any DBCS. As shipped,
`abc北京def` is three selectable runs: one-cell text, multi-cell text, then
one-cell text. Turning it off makes the whole non-delimiter string one word.

**It applies to only one of `CheckDelimiterChar`'s two arms.** A selection
starting on a delimiter still takes consecutive copies of that same character,
whatever their widths; a selection starting anywhere else takes
non-delimiters and conditionally stops when `b->cell == 1` changes
(`buffer.c:4479`). A wide character also remains indivisible when the setting
is off: clicking its padding cell first resolves to its leading cell.

207 settings over 193 keys, 79 to go.

#### Save setup keeps the file it is about to replace

`crates/tt-config/` and the shell, 2026-08-10. `IniAutoBackup` ships on, and
Setup > Save setup now copies an existing active INI byte-for-byte before
writing it. The sibling has upstream's local-time name,
`YYYYMMDDTHHMMSS+zzzz_<original-name>`; a second save in the same second keeps
the first copy rather than replacing it.

**This is narrower than "back up every settings write."** Upstream enters the
branch only while overwriting the same file from Save setup
(`vtwin.cpp:4738`). A first save has nothing to copy, and `SaveVTPos` on window
close writes the small geometry pair without making one. The copy is also
best-effort: a collision or an unwritable sibling does not prevent the actual
save, matching `CreateBakupFile` ignoring `CopyFileW`'s answer. The Qt test
drives the menu action and proves both the preserved old bytes and the literal
`IniAutoBackup=off` case.

208 settings over 194 keys, 78 to go.

#### Focus switches the window's opacity

`crates/tt-config/` and the shell, 2026-08-10. `AlphaBlend` is the inactive
window's opacity and `AlphaBlendActive` is the focused one's, both clamped to
0..255 and mapped onto Qt's 0.0..1.0 property. Startup uses the active value;
`WindowActivate` and `WindowDeactivate` switch it thereafter, and a live
settings change reapplies the state the window is in.

**The active default is another loaded setting, not 255.** `ttset.c:1471`
passes the already-clamped inactive value to `GetPrivateProfileInt`, so
`AlphaBlend=120` with no active key makes both states 120. An empty active key
inherits too, while a non-numeric one is zero under Win32's separate integer
rule. The schema now says that relationship with `default-from=` rather than
special-casing these names in the reader.

There is one platform limit the property test cannot reveal. Fedora's Qt 6.11
X11 backend implements `QXcbWindow::setOpacity`; its native Wayland backend has
no corresponding override or alpha-modifier protocol, so it remembers
`windowOpacity()` but sends the compositor nothing. The settings visibly act
under xcb (and on platforms whose Qt backend supports opacity); on native
Wayland they round-trip and switch internally but the window remains opaque.

210 settings over 196 keys, 76 to go.

#### The endpoint is part of the window title

`crates/tt-config/`, `tt-session`, the C ABI and the shell, 2026-08-10.
`TitleFormat` is a six-bit word controlling the endpoint, session number,
`VT`/`TEK` suffix, title/endpoint order, TCP port and serial speed. Its shipped
value is 13, so the ordinary caption is `<endpoint> - <title> VT`; a window
with no ready line says `<title> - [connecting...] VT` or
`<title> - [disconnected] VT` instead. The endpoint order bit does not move
those state messages, which sit in an earlier arm of `ChangeTitle`.

**The integer narrows into a `WORD`.** `TitleFormat=-1` is 65535 and
`TitleFormat=65537` is 1 before upstream writes it back. The schema gained a
`uint16` spelling rather than treating that as a clamp: both a lower-bound
default and a ceiling would disagree with C assignment. Unknown bits 6–15
remain preserved even though the dialog knows only the lower six.

The configured title and the host's OSC title are still combined by the core;
the shell adds the pieces which depend on a window and a connection. Upstream's
default `Title=Tera Term` remains a product-name sentinel, so it becomes
`Sterna` even under `ahead` and `last`, without replacing the same words inside
an unrelated remote title. A local pty has no upstream port type—CygTerm is TCP
there—so its command description is used as the useful endpoint equivalent.

Serial speed is read from the transport, not `serial.baud`: `--baud` can open
the line at a value the loaded settings never saw, and a macro can change it
again with `setbaud`. That command now raises a title event after the successful
reset, matching `ttdde.c:988`, so the caption changes immediately rather than
waiting for unrelated terminal output.

211 settings over 197 keys, 75 to go.

#### Thirty-seven settings, and the remaining list is shorter than this batch

`crates/tt-config/`, the C ABI and the shell, 2026-08-10. Three related passes
added 37 settings over 37 upstream keys: eighteen for the shell, menus and
broadcast window; eleven for raw file send/receive; and eight for terminal,
keyboard and font behavior. The schema now stands at **248 settings over 234
keys, with 38 of `ttset.c`'s 272 keys to go**.

The keys with an existing surface act now. `AcceleratorNewConnection`,
`AcceleratorCygwinConnection` and the Send break accelerator change their Qt
shortcuts live; the matching disable-menu switches change action availability.
`FileSendFilter` is converted from Tera Term's `*.txt;*.log` spelling to Qt's
name filter and every protocol send picker starts from the expanded `FileDir`,
falling back to Downloads as upstream does. The rest of the raw-file family is
carried faithfully for the raw send/capture UI which is not built yet; it does
not get incorrectly folded into X/Y/ZMODEM or Kermit.

`MetaKey` and `Meta8Bit` are live and deliberately separate. Meta itself ships
off; once enabled, `Meta8Bit=off` prefixes ESC, `raw` sets bit 7 on the byte,
and `text` sets U+0080 before UTF-8 encoding. The raw arm required a binary
`tt_session_send_bytes` ABI rather than abusing the text path. Left and right
Alt are remembered from native key events, `StrictKeyMapping` suppresses the
built-in special-key fallback, and `DeleteKey` remains upstream's exception.
The Qt pty test asserts the actual bytes for all three Meta encodings and both
keyboard switches.

`FontQuality` reaches Qt's rasterisation strategy. ClearType becomes an
explicit antialias request off Windows, leaving subpixel details to the native
paint engine instead of promising a Windows renderer on every platform.
`DrawingResizedFont` controls whether a fallback glyph whose natural advance
misses its cell box is stretched horizontally into it; the painter's ordinary
monospace run spacing remains load-bearing and independent.

One setting required a schema feature rather than a row. Upstream reads
`"CygwinDirectory "` with a trailing space (`ttset.c:1476`) and writes
`"CygwinDirectory"` without it (`:2250`). Backtick-quoted keys now preserve
significant whitespace and `write-key=` records the different output spelling,
so loading and saving reproduce both halves instead of normalising away a bug
in a shared file.

#### The last thirty-eight keys

`crates/tt-config/`, `tt-vt`, `tt-session`, the C ABI and the shell,
2026-08-10. Three coherent passes closed the extracted upstream list: eighteen
encoding keys, eleven printer/TEK and other legacy keys, then the final nine
keys. Tuple-valued keys become separately addressable fields, so those 38 keys
added 47 settings and brought the schema from 248/234 to **295 settings over
all 272 keys**. The upstream-list test now reports zero missing and zero
invented keys.

The encoding rows keep every spelling and fallback Tera Term accepts, including
its exact-case code-page names and wrapping integer words. CJK remains deferred,
so those values round-trip without claiming that the missing conversion tables
act. The same honest boundary applies to the printer and TEK families: the file
is safe to share with Tera Term, while Sterna does not expose a printer or a TEK
window it does not have.

The keys with a live surface do act. `VTFontSpace`'s four signed margins expand
the cell and move the glyph within it; resized fallback glyphs still target the
natural font box, not that padded cell. `Debug` and `DebugModes` drive the raw
receive display, Shift+Escape cycles only the admitted modes with the upstream
beep, and TTL's `setdebug` selects a mode directly as upstream does. Hex mode
prints `XX `, normal mode uses caret notation and reverse video for high-bit
bytes, and no-output mode consumes the stream without parsing it. `VTIcon` is
also accepted by the Tera Term command-line parser.

The remaining compatibility-only values — drawing API/code page, source
version, maximized-window workaround, printer resolution and icon selection —
are carried and saved with their upstream conversion rules. In particular,
`MaximizedBugTweak=on` means numeric 2 and every other spelling goes through
`atoi` before narrowing to a `WORD`; treating it as a bool would silently alter
a shared file.

#### `KEYBOARD.CNF`, where a key is a physical scan code

`crates/tt-config/`, `tt-session`, the C ABI, the Qt shell and `tt-macro`,
2026-08-10. The second compatibility file now reads through the same measured
INI layer as `TERATERM.INI`: every fixed section and all 99 `[User keys]`
entries, including binary, text, macro and menu-command actions. Its small
parser quirks are reproduced too. Fixed values are read through a ten-character
buffer and only exact `off` disables one; user values get 255 characters and
any `off...` prefix disables one; signed numbers narrow into words; and when
two entries name the same physical code the higher internal Tera Term key id
wins, not the later line in the file.

The map lives on the session because terminal keys have to use the live
application-cursor/keypad and 7/8-bit modes. The C ABI reports the non-wire
actions to the frontend instead of pretending a terminal owns a clipboard,
menu or macro runner. Qt translates Wayland's evdev codes, X11's evdev-plus-8
codes and Windows' set-1 codes into the PC/AT numbers the file stores, then ORs
the same Shift, Ctrl and Alt bits upstream does. The real-pty test proves a
remapped function key and a modified user key reach the far end as the expected
bytes.

The shell loads `KEYBOARD.CNF` beside the active settings file by default,
accepts `/K=` with upstream's relative-path and implicit `.CNF` rules, warns
about duplicate physical assignments and exposes Setup > Load key map. This is
what makes `StrictKeyMapping` useful rather than merely suppressive. TTL's
`loadkeymap` replaces the same live map and resolves a relative name through
`changedir`, as its other file commands do. DEC UDK definitions and a few local
window shortcuts still wait for their own missing subsystems; the file reader
does not claim those exist.

#### The language files stay language files

`vendor/lang/`, `crates/tt-i18n/`, the C ABI and the Qt shell, 2026-08-10.
All 14 UTF-8 `.lng` files are vendored byte-for-byte at the same named upstream
revision as the protocol C, with `sync.sh --check` as their drift gate. They
are installed as data rather than converted to Qt `.ts`, preserving the 17,610
lines of existing translation and the upstream translator workflow.

The format has no new parser. `tt-i18n` reads through `tt-config::Ini`, so its
duplicate sections, quotes and empty values have the same measured Win32
behavior as `TERATERM.INI`, then restores upstream's four escapes: backslash,
newline, tab and NUL. The last matters to common-file-dialog filters, which is
why `tt_i18n_text` returns a borrowed UTF-8 span with an explicit length rather
than a C string. The C test proves two embedded NULs cross intact.

`UILanguageFile` is now the schema's 296th setting. It is the one key whose
read `ttset.c` delegates to `GetUILanguageFileFullW`; its upstream fallback is
`lang\Default.lng`, and relative Windows-style values resolve against the
active setup, the executable and Sterna's installed data directory. The setup
dialog offers every shipped catalog by its own `[Info] language` name, unique
field labels translate, and the main menus retranslate live.

Menu strings are adapted at the presentation boundary: Win32 mnemonic markers
and printed `Alt+…` captions are removed because Sterna reserves Alt for the
terminal and owns real shortcuts on `QAction`. This includes the complete
Japanese-style `設定(&S)` marker rather than leaving a stray `(S)`.

This completes Stage 3's language-file item. The main menus, generated settings
UI, serial/SSH/telnet connection forms, SSH prompts, transfer dialogs, macro
dialogs, paste confirmation, disconnect confirmation and common file-picker
captions all use the catalog where an upstream key means the same thing. The
tests load the real Japanese file and exercise each family; the SSH and macro
dialogs were also rendered and read.

The boundary is deliberate: Sterna's ssh-agent, legacy-algorithm and
telnet-mode controls, its safer host-key explanation, Lua file filters and
other new copy have no upstream translation. They retain the clear source text
instead of being assigned a nearby key with a different meaning. Translating
those would be a Sterna catalog extension, not more wiring of Tera Term's
unchanged files.

### ✅ Stage 3 — Windows parity (3–4 months, ~15k LOC)

Windows build, ConPTY, Win32 serial edge cases, NSIS installer. All 14 `.lng`
languages wired through unchanged. VT320/VT525 depth and DEC private modes.
Tabs and sessions; session duplication as an in-process concept rather than
`CreateFileMapping`. Built-in HTTP/SOCKS proxy replacing `TTProxy`. Printing.

**Windows build, first blocker cleared 2026-08-10.** `tt-xfer` had still
force-included the POSIX `windows.h`/Secure-CRT shim and compiled
`fileio_posix.c` for every target. It now selects the real Windows SDK, MSVC's
C++ runtime and a wide-path file backend there, while POSIX keeps the existing
shim. The common host redirects the protocols' `MessageBox` and window-timer
calls on both sides, so the core cannot put up an unmanaged Win32 window. The
vendored files remain byte-for-byte upstream; `x86_64-pc-windows-gnu` now
checks this crate cleanly and the Linux interop suite remains 12/12. This is a
transfer-layer landing, not yet a shippable Windows shell.

The next compiler stop was `tt-conn`: SSH's self-pipe and agent discovery and
the pty's name were Unix-only. They are now explicit platform seams. Unix keeps
the non-blocking pipe and `SSH_AUTH_SOCK`; Windows keeps the same synchronous
SSH state machine, uses russh's Pageant transport, and reports no pollable file
descriptor. `portable-pty`'s ConPTY construction and resize path compile, while
its byte I/O and a frontend wakeup remain open. The fork/`poll(2)` pty suite and
the SSH descriptor assertion are gated as POSIX tests rather than being made
to pass vacuously on Windows. `tt-conn` now cross-checks cleanly for the full
Windows target, including all targets.

The third stop was in tests and log naming, not in the session state machine.
The Unix transfer, pty and serial harnesses now say so at the crate boundary
instead of asking a Windows compiler for `poll(2)` and `/bin/sh`. Log file
templates still go through the platform C runtime as upstream's do: Unix keeps
`libc::strftime`, and Windows supplies its native nine-field `tm` to the CRT,
including MSVC's `%#d` spelling. The ordinary date expansion remains covered
on Linux; the Windows branch is cross-compiled but still needs a native MSVC
run before it is called proven.

`tt-host` was the next small stop: its nominal non-Unix wait function still
named Unix's `RawFd`, and the run loop called the Unix-only session accessor
unconditionally. Unix still sleeps on `poll(2)`; Windows now uses a bounded
sleep, explicitly temporary until ConPTY supplies byte I/O and a native
frontend wakeup. This makes the harness compile there without pretending that
the missing transport path works.

The control socket is now a local byte-stream abstraction rather than Unix
types threaded through the client and server. Unix keeps its `0700`/`0600`
socket and `SO_PEERCRED`; Windows binds a byte-mode named pipe under
`\\.\pipe\sterna-<session>-<name>`, refuses remote clients and impersonates an
accepted client only long enough to compare its token user SID. Named pipes
have no stale files, and `FILE_FLAG_FIRST_PIPE_INSTANCE` turns a duplicate
topic into the same address-in-use answer. Pipe enumeration preserves
`ttctl ls` and the refuse-to-guess rule. `ttpmacro.exe` reads
`GetCommandLineW` and runs the upstream tokeniser instead of accidentally
using Windows' different argv quoting rules. The Unix crate and CLI suites are
still 49/49 and 12/12; the named-pipe tests cross-compile but need the native
Windows runner. The frontend wakeup was still open at this point:
`tt-ctl`'s Windows channel had no wait handle in this transport landing; the
native-event change below completes that half of the Windows `Control` object.

The flat ABI's SSH-connect poll function was the next compiler stop: unlike
the session, macro and control variants, it called the Unix-only accessor on
every target. All four descriptor spellings now make the same honest promise:
an fd on Unix and `-1` on Windows. That compiler fix deliberately did not
claim a Windows frontend could sleep efficiently yet; the single native-event
follow-up spanning SSH, macros, control and the Qt notifier is recorded below.

`cargo check` was not the whole Windows gate: linking every test found MinGW
skipping all seven protocol constructors. The vendored C and C++ archives were
emitted before the host archive that first referenced `XCreate` and friends,
and MinGW scans an archive only once. Reversing them is still wrong because
`protolog.cpp` reaches back into the C archive for `ToWcharA`/`ToWcharU8`.
All three native archives are now linked whole: every protocol is
runtime-selectable and belongs in the library, and the small host archive must
travel with their callbacks even in a downstream binary that never starts a
transfer. The complete Windows-target test link is the gate for this fix;
`cargo check` cannot see it.

The linked binaries run under Wine far enough to exercise the platform seams
(not to substitute Wine for Windows). All nine transfer unit tests pass; five
of the six named-pipe server lifecycle tests pass, plus the direct pipe and
token-SID check. Wine 9 itself does not implement the two namespace operations
those remaining assertions use: `FindFirstFileW` on `\\.\pipe` returns
`ERROR_BAD_DEV_TYPE`, and `FILE_FLAG_FIRST_PIPE_INSTANCE` is ignored. Keep the
native Windows tests as the authority for both.

That run found a real test defect beside the Wine gaps: `tt-session`'s log-name
tests used `/var` and `/tmp` as though they were absolute on every platform.
They now use platform-native temporary paths and cover MSVC's `%#d` directly.
The production path is complete too: local log timestamps use
`GetTimeZoneInformation`, `&u` uses `GetUserNameW`, and the fallback log
directory is `%LOCALAPPDATA%\sterna` rather than an XDG path interpreted under
Windows rules. `FileDir`'s `%VAR%` references are expanded before its existence
check, at the same point `GetTermLogDir` does it upstream; `LogDefaultPath`
deliberately is not expanded, also matching that function.

The native frontend wakeup is no longer a placeholder. SSH, macro and control
channels own manual-reset Win32 events, signal them only after publishing work,
and reset them before the frontend drains that work; a racing post therefore
leaves either a queued job or a signalled event, never a sleeping window. The
flat ABI exposes borrowed `*_wait_handle` spellings alongside the Unix fds,
and the Qt shell selects `QWinEventNotifier` on Windows. SSH preserves the same
event across connection setup and the running session, just as Unix preserves
its self-pipe. All three set/wait/reset tests pass through the MinGW binaries
under Wine, the Windows crates link cleanly, and the Unix Qt build remains
clean with its pty, macro and control event-loop tests passing. A native
Windows Qt build is still required before calling the frontend path proven;
the transports which still needed native Windows waits are recorded below.

ConPTY is now a byte transport rather than a type that merely constructs. The
anonymous pipes `portable-pty` creates are synchronous, so one blocking worker
owns each direction: the reader feeds a bounded 1 MiB queue and signals a
manual-reset event, while the writer's small bounded queue turns saturation
back into the short-write path the session already retries. EOF is an ordered
message behind the final bytes and is re-signalled when a manual event
coalesces both, so the last line cannot hide the child's exit. `tt-host` waits
on that same event instead of its temporary Windows sleep, and the Qt shell
gets it through the session's existing `*_wait_handle` ABI.

The worker, ordering and event transition pass as a MinGW binary under Wine.
The two real `cmd.exe` integration cases compile and are written to answer
ConPTY's initial `CSI 6 n` cursor query, but Wine 9 cannot run them: its console
host rejects the internal `--inheritcursor` switch selected by `portable-pty`
and closes the output pipe empty. Keep the native Windows runner as the
authority; Wine's failure happens below Sterna before the child produces a
byte.

Windows telnet no longer makes the frontend poll a synchronous Winsock socket.
Its read side shares ConPTY's bounded 1 MiB worker queue and manual-reset event,
using a blocking socket clone while the original retains ordinary timed writes.
That deliberately avoids `WSAEventSelect`, whose forced nonblocking mode would
turn every protocol reply into a partial-write state machine. Windows does not
set Unix's 50 ms read timeout on the clone, and `Drop` shuts down the underlying
connection because closing the original handle alone cannot wake a cloned one.
The local Winsock test proves a quiet connection leaves the event unsignalled,
then orders data and EOF as two wakes; it and the shared ConPTY-worker regression
pass from MinGW binaries under Wine. Serial is now the last transport without a
native Windows wakeup.

Serial closes that last transport wakeup. `serialport-rs` opens a synchronous
COM handle, which is not itself a receive-readiness object, so a worker blocks
in `WaitCommEvent` on a duplicate and publishes one bounded notice at a time.
It waits for the frontend's acknowledgement before arming again, matching Tera
Term's `CommThread`/`ReadEnd` handshake, and `SetCommMask(handle, 0)` cancels it
on close. Breaks come from the native line event; Windows bytes no longer pass
through Linux's `PARMRK` decoder, where an ordinary `0xFF` was otherwise held
as the start of a three-byte escape. A 64 KiB read matches upstream's input
buffer and avoids `bytes_to_read()`: that serialport call uses
`ClearCommError`, which could clear a later break before the worker observed it.

The complete Windows workspace links, and a hardware test asserts idle, data
and reset transitions while sending a literal `0xFF` over a COM loopback pair.
It still needs the native Windows runner. Wine's PTY-backed COM mapping rejects
ordinary port setup with `ERROR_NOT_SUPPORTED` before the event worker can run,
so it cannot serve even as the focused smoke test it did for sockets and plain
Win32 events.

The other half of Windows serial setup no longer goes through portable setters
which cannot name the settings. One zero-initialised DCB now carries the baud,
5–8 data bits, all five parity modes, stop bits, native CTS or DSR output flow,
independent DTR/RTS modes, custom XON/XOFF bytes and upstream's 768/3328
thresholds into a single `SetCommState`. Applying one setter at a time had two
bad answers: MARK/SPACE was rejected despite native support, while the data
bits and XON/XOFF bytes were silently left at the driver's old values. A single
invalid field could also leave every earlier setter applied. DTR toggle now
fails before touching the port because Win32 has no such control value.

Success is not trusted: `GetCommState` reads the controlled fields back and an
adapter which silently keeps an old value produces a named unsupported-setting
error. The same native hardware file which covers `WaitCommEvent` opens a
115200 7-mark-2 port with DSR/DTR flow and non-default software-flow bytes, so
the readback itself is the assertion. Pure DCB construction and its CTS/DSR bit
split pass under Wine; the driver readback still requires native Windows for
the COM-emulation reason above.

Windows also keeps the error at the COM-port open boundary now. The portable
crate collapses `ERROR_FILE_NOT_FOUND`, `ERROR_PATH_NOT_FOUND` and
`ERROR_ACCESS_DENIED` into one `NoDevice` value and retains only the localized
message. The Unix fallback then tried `Path::exists("COM3")`, which is always
false, so an exclusively held port was reported as unplugged. The Windows path
uses the same exclusive `CreateFileW` call directly: access denied and sharing
violation are the actionable “in use” error, while missing/path/invalid-name is
disconnected and every other failure retains its native source. A missing COM
test passes under Wine; the second-open-is-busy assertion sits with the native
loopback cases because Wine cannot finish configuring its PTY-backed port.

Windows serial output is bounded now as well. `serialport-rs` gives a COM port
one timeout for reads and writes, so a short caller write quietly inherited the
cached 50 ms read value; it also implements flush with `FlushFileBuffers`,
which can wait forever while CTS or DSR holds the driver queue. A write now
temporarily changes only the Win32 write timeout and restores the full original
`COMMTIMEOUTS` even after failure. Flush polls `COMSTAT.cbOutQue` to its own
deadline. Because that snapshot is obtained through the destructive
`ClearCommError`, any `CE_BREAK` it observes is put onto the existing manual
event without consuming a worker notice. Pure timeout and event-ordering tests
pass under Wine; a native loopback case lowers CTS against a five-second read
timeout and requires both a 40 ms write and flush to return promptly.

TTL's local-address commands no longer report `result=-1` unconditionally on
Windows. The IPv4 half uses upstream's Winsock startup, datagram socket and
`SIO_GET_INTERFACE_LIST` path, including its 30-interface ceiling and its
up/non-loopback filter. The IPv6 half uses the same fixed 256-entry
`GetAdaptersAddresses` buffer and, unlike the Linux approximation, applies the
native `IP_ADAPTER_ADDRESS_DNS_ELIGIBLE` flag before rendering all sixteen
bytes in upstream's long form. API-query failures remain a successful empty
list just as they are upstream; only Winsock initialisation failure is “cannot
retrieve.” Both families execute from the MinGW test binary under Wine, while
Linux's existing interface assertions remain green.

`getspecialfolder` now asks the Windows shell on Windows instead of running the
XDG approximation there. Its sixteen case-insensitive names map one-for-one to
upstream's known-folder IDs, including the seven Windows concepts which
correctly have no Linux answer. Returned task memory is copied as UTF-8 and
freed on both success and failure. The command's stranger outer contract is
unchanged: it still writes an empty string for an unknown or unavailable
folder and reports `result=1` regardless, because `GetSpecialFolder` itself
returns a literal one. Wine resolves all sixteen to Windows-absolute paths;
the Unix XDG mapping and tests remain separate.

The TTL unit suite now asks each platform its own questions around `exec` and
paths. Windows runs `cmd.exe`, verifies the requested current directory with a
marker, expects `makepath`'s native backslashes and expects `filestat` to name
the temporary directory's drive; Unix retains `/bin/true`, `/bin/sh`, slash
joins and `?`. These are test corrections rather than conditional production
answers—the generic implementations were already returning the Windows forms.
The MinGW binary now reaches 331/332 under Wine; the one remaining failure is
the real `fileunlock` mismatch handled next.

That last TTL failure was an API mismatch rather than a Wine exception. Rust's
standard whole-file lock uses `LockFileEx`; upstream uses `LockFile` and then
`UnlockFile` over `(0, 0, DWORD_MAX, DWORD_MAX)`. Wine accepted the first form
but refused the unlock, so one handle reported a successful `filelock` followed
by a failed `fileunlock`. Windows now uses upstream's exact pair and range;
Unix retains its advisory standard-library lock. The complete MinGW TTL unit
binary is 332/332 under Wine, as is the native Linux run, and the upstream
script suite remains green.

TTL's `exec` show mode is now real on Windows too. The portable process builder
cannot set `STARTUPINFO.wShowWindow` on stable Rust, so the Windows branch uses
`CreateProcessW` directly, keeps the macro's original raw command line, and
passes `hide`, `minimize`, `maximize` and the default `show` through with
`STARTF_USESHOWWINDOW`. A child-process smoke test reads its own startup info
and verifies all five paths under Wine. The complete Windows TTL binary is now
333/333 there; native Linux remains 332/332 plus the upstream script suite,
and every Windows workspace target still links.

The upstream TTL script gate can now finish on Windows rather than opening
Notepad and waiting forever. `#35797.ttl` is the suite's one external `exec`;
the harness replaces only its program name with a guaranteed miss on both
platforms, preserving the parse, wait and failure path without launching a GUI.
The native suite remains green. The first MinGW suite under Wine completed in
6.7 seconds and exposed 11 transcript differences: six machine paths plus five
real platform shapes (drive syntax, special folders and path separators).

The transcript portability pass now recognises paths after `esc` has doubled
every Windows separator, as well as raw command-line paths, and canonicalises
the separator immediately after `<dir>`, `<home>` and `<exedir>`. The six
machine-shaped diffs disappear without changing a Linux golden; a direct unit
case guards both Windows spellings. Five deliberately platform-shaped scripts
remain. They are recorded rather than blessed from Wine; a native Windows run
remains the authority for their expected transcripts.

That boundary is now executable. On Windows the script harness still runs all
53 scripts and compares them against the reviewed portable goldens, then
requires the divergence names to be exactly `#31050`, `#31971`, `#39452`,
`getspecialfolder` and `spfolder`. A missing member fails just like a new one,
so 48 common transcripts are a byte-for-byte gate without pretending Wine
defines the other five. The MinGW test is green under Wine in 6.9 seconds and
reports the 48/5 split. `TTL_BLESS` aborts immediately on Windows, before it can
replace reviewed files with platform-shaped or Wine-specific answers.

The session and macro integration tests now ask Windows for Windows-shaped
fixtures too. The session opener runs `cmd.exe` rather than `sh`, CygTerm's
directory cases use the platform temporary directory rather than `/tmp`, and
the serial command-line case compares `/C=1` with the enumerator's exact first
port instead of requiring `/dev/`. This matters even under Wine: its `Z:` drive
made `/bin/sh` and `/tmp` appear to work in a Windows binary, hiding tests that
would fail on native Windows. The complete Windows `tt-session` suite is now
167/167 under Wine; the 43 non-ConPTY `tt-macro` checks pass there as well.
The remaining macro connection case now names `cmd.exe` correctly but still
needs native Windows because it reads real ConPTY output, below the same Wine 9
console-host limit already recorded for `tt-conn`. Both native Linux packages
remain green.

The standalone Lua surface is Windows-clean as well. Its MinGW test binary
passes all 59 unit cases plus the documentation example under Wine, covering
byte strings, neighbouring `require`, cancellation hooks, dialogs, logs,
serial controls and transfer plans. The threaded seven-case Lua/session join
is part of the 43 macro checks above; together these leave no Lua-specific
native-Windows exception to carry forward.

The configuration and command-line suite no longer requires `GetFilePath`'s
inserted separator to be `/` on every target. The implementation was already
using the platform separator—the Windows binary correctly produced `\`—and
the test now checks that target-shaped join while still preserving separators
which came from the supplied path. All 122 configuration checks Wine can run
are green, including the recorded Win32 INI answers. The one excluded check
spawns `rustfmt`; Wine cannot run the installed Linux executable, so its
Windows half remains a native-toolchain check rather than being weakened. The
native Linux suite passes all 123 checks, including that generator guard, and
both target clippy passes are clean.

The flat ABI now has a real Windows consumer rather than a zero-test Rust DLL
build. A MinGW C11 program includes the generated header, links and loads
`sterna.dll`, then drives the screen/event surface, Windows temporary files,
settings, logging, command-line resolution, serial enumeration and missing-COM
mapping. It also waits for a threaded macro through the exported Win32 event
and sends raw JSON through a `CreateFile` named-pipe client, servicing the
control event until the reply comes back. The same header is compiled as
C++17. That focused harness passes under Wine, while the unchanged native
Linux C/C++ ABI harness remains green including its pty and ZMODEM paths.
This does not move the native-Windows boundary: ConPTY, real COM hardware and
Wine's two missing pipe-namespace operations still require the Windows runner.

That ABI source is now wired into the existing native `windows-latest` job as
well. A PowerShell runner activates the installed Visual Studio toolchain,
builds the MSVC DLL/import library, treats C11 and C++17 header warnings as
errors, and runs the Win32 consumer beside the DLL. The same job now installs
`clippy` and `rustfmt` explicitly, lints every Windows target, runs the whole
native Rust workspace, and checks that cbindgen left the committed header
unchanged. The MinGW/Wine path remains green locally; the MSVC path is written
but cannot be called verified until that native job runs on a pushed commit.
*(It has since run, and is now verified — see the end of this section.)*

That job has now run, and it found one thing before it could reach any of the
above: `std::fs::canonicalize` answers with a `\\?\` verbatim path on Windows
and `cl.exe` cannot open a source file spelt that way, so `tt-xfer`'s build
script stopped at the first vendored protocol. It reports the failure as
`C1083: Cannot open source file: '\\raw.c'`, naming a path that exists nowhere
— and MinGW accepts the prefix, which is why every cross build was green. The
prefix is stripped for the drive spelling only; `\\?\UNC\server\share` needs
it. Whether anything downstream of that first compile passes under MSVC is
still open: the run got no further.

Two failures in the shell job were the same kind of thing — a gate that had
never been able to speak. Its `VTFontSpace` render case measured the left
margin from column 0, where a glyph that overhangs its own advance clamps the
search at zero; DejaVu Sans Mono's `A` does exactly that at the size Qt 6.4.2
picks, so CI read three pixels of margin as two while the painter was right
throughout. And the job installed no `uv`, so `telnet-audit`'s PEP 723 `inetd.py`
died the moment it started while `servers.sh` printed that the servers were up
— surfacing three minutes later as a connection refused. `servers.sh` now waits
for the listeners and prints the child's log when they never arrive. `print_test`
was also written and never gated; it is a CI step now.

With the shell job green, the Windows one has advanced twice more. It lints
clean — so the vendored Tera Term C does compile under MSVC — and then found
two things a Linux checkout cannot express. `core.autocrlf` is on by default on
the runner, so `generated.rs` arrived with CRLF and its freshness test reported
a file nobody had touched as stale; `.gitattributes` now pins our own sources,
the generated header and the TTL transcripts to LF, and marks the vendored
tree, the case inputs and `win32.txt` as bytes to leave alone. Behind that,
both ConPTY tests spent their whole deadline: ConPTY's output pipe belongs to
the console host rather than to the child, so `cmd.exe` exits, its output
arrives, and the reader blocks in `ReadFile` for ever. Closing the
pseudoconsole once the child is reaped is what ends it, and it keeps the
trailing bytes ahead of the disconnect because the host flushes before it
closes. That check is on `tick` as well as on the quiet read path — a child
that exits without printing produces no wakeup to read on. Twelve test
binaries reported before the ConPTY failure, against six before the CRLF one;
the rest of the workspace is still unrun there.

Sixteen now, and the control socket was next: five failures with one cause and
one on its own. `ImpersonateNamedPipeClient` answers `ERROR_CANNOT_IMPERSONATE`
until something has been read from the pipe, so the peer check at accept could
never pass — every connection was refused as an intruder and every client saw
the window hang up on it. It runs on the first line now, before that line is
parsed or answered, while Unix keeps the stricter pre-read order that
`SO_PEERCRED` allows. The separate one: `FindFirstFile` on `\\.\pipe` reports an
empty match as `ERROR_NO_MORE_FILES` rather than `ERROR_FILE_NOT_FOUND`, so a
machine with no window open failed every client that had to look.

Worth recording as method rather than as fact: the first of those took a round
trip to diagnose because a refusal and a broken check were the same `false`.
Making the check keep its reason turned a guess about Win32 semantics into
Windows stating the rule in its own error text. That is the cheaper move
whenever the only machine that can answer is a CI runner.

Nineteen binaries, and `\\?\` again — the third place it has surfaced.
`ttpmacro` and `ttctl` both resolve a macro's path before sending it, because a
relative name means what it says in the shell it was typed in; `canonicalize`
answers with the verbatim form, so the window was handed a spelling
`ttpmacro.exe` never produced and the macro would see it as its own name in
`params[1]`. Both clients share `full_path` now. **Still open, and deliberately
not fixed on a hunch: `tt-ttl`'s `set_dir` canonicalises into `cur_dir`**, which
`getdir` reports — upstream's `GetCurrentDirectory` never returns a verbatim
path, so this is the same defect one crate over, but whether it moves a golden
is a question the Windows TTL gate has not been reached to answer yet.

Then the run stopped being a failure and became a hang: 74 minutes in
`tt-macro`'s `connect.rs`, against about eleven for the whole job. Two separate
things, and only one of them is fixed.

**The test was opening a real serial port, and that is fixed.** `/BAUD=` selects
the serial port as well as setting the speed — faithfully, it is what upstream's
does — and it sat *after* the host name, so word order made a nominally TCP test
into a serial one. On the runner that is `COM1`; here it is `/dev/ttyS0`, which
on the rig machine is a unit test reaching for whatever is plugged in. The
options go first now, which is the order dependence the test is named for. The
whole file runs in five seconds again, verified natively and under Wine.

**A serial open blocking the caller indefinitely is not fixed, and needs a
Windows machine.** `Session::connect` is serviced on the frontend's thread, so
whatever blocked took the window's event loop with it — in the test that meant
the harness's own ten-second limit could not fire, because the thread that
checks it was the thread that was stuck. Wine faults instead of hanging, inside
its own DLL and with no usable backtrace, and `AGENTS.md` already says Wine's
PTY-backed COM mapping is not evidence about Windows; so this wants
`tests/serial_windows.rs` and a real port rather than another guess. What is
worth suspecting first is the pair the traps already name: `CreateFileW` on a
device whose driver blocks, and the `WaitCommEvent` worker's acknowledgement
handshake in `WindowsSerialWake::start`.

Two smaller things came out of it. Every CI job now has `timeout-minutes: 30`,
because `cargo test` has no per-test timeout and one stuck test otherwise takes
the six-hour default with it. And the shell job failed once, separately, with
`malloc_consolidate(): unaligned fastbin chunk detected` in `cmdline_test` —
heap corruption, on Linux, in a test that passes ten times out of ten locally on
the same Qt. Intermittent, unrelated to anything in this section — and now closed. It was a
real use-after-free rather than a test artefact: `Session` and `Macro` are both
children of `MainWindow`, `QObjectPrivate::deleteChildren` deletes in creation
order, the session is created first, and `~Macro` calls `Session::unlinkMacro`
to take the terminal's tap off. So a window closed with a macro still running
read a freed session — which is a script outliving its window, not an exotic
case. It only corrupts the heap once something else claims that memory, hence
the intermittency and hence ten clean runs locally. `~MainWindow` now tears the
control socket down and then the macro, mirroring the constructor. Twelve plain
ASan runs found nothing; a probe that *forces* the condition — a `pause 30`
startup macro and an immediate teardown — failed on the first attempt and named
the free site, and it is now a permanent case in `cmdline_test`.

With the connect hang gone the Windows job reached 33 test binaries, against 19
before it, and stopped on a clock rather than a defect: the transfer's deadline
is `GetTickCount64` there, faithfully, since upstream's `FTSetTimeOut` is
`SetTimer` — one system tick of resolution, about 15.6 ms, on a counter
`Instant`'s QPC knows nothing about. A one-second auto-stop measured 993 ms. The
assertion now allows a tick; what it is for is a `recvfile` that returns as soon
as it starts, which misses by three orders of magnitude.

51 binaries after that, and the first thing the native runner has contradicted
rather than merely exposed. `expandenv`'s delimiter rule was recorded as "an
unknown name's closing percent is consumed and cannot also open the next name",
and `ExpandEnvironmentStringsW` does the opposite: it resumes scanning *at* the
delimiter, so `%UNSET%KNOWN%` is `%UNSET` followed by `KNOWN`'s value. The Unix
mirror implemented the wrong rule and 335 of 336 tests agreed with it, because
the only input that can separate the two is two names in a row with the first
one unset. That case is now the last assertion in the test with a note saying
what it is for, and the trap in `AGENTS.md` is corrected rather than deleted —
the rule had been measured somewhere that was not Windows.

**And with that the whole Rust workspace passes on native Windows** — 69 test
binaries, `cargo fmt` and `clippy` clean on every Windows target, which is the
first time any of it has been true. Eight defects between the first run and this
one, every one of them real and none of them findable anywhere else: a verbatim
path MinGW accepts, CRLF from the runner's own checkout, ConPTY's pipe belonging
to the console host, `ERROR_NO_MORE_FILES` from an empty pipe namespace, a peer
check that could not run before the first read, a test opening `COM1`,
`GetTickCount64` against QPC, and `ExpandEnvironmentStringsW`'s delimiter rule.

The frontier is now the step after it, `run_abi_windows.ps1`, which had never
run either: it passed `--profile debug`, and `debug` is the *directory* a dev
build lands in rather than a profile name — cargo reserves it. The flag goes in
only when `PROFILE` is set now, as `run_abi.sh` already did. The variable was
also called `$profile`, which is one of PowerShell's automatic variables. Behind
that, `/WX` turned MSVC's `fopen` deprecation into an error — answered with
`_CRT_SECURE_NO_WARNINGS` rather than `fopen_s`, because MinGW compiles the same
file for the Wine harness, and because the warnings that compile exists to catch
are the generated header's.

**And with that the run is green — all of it, for the first time.** The
Windows job's four steps all pass: `fmt` and `clippy`, the whole workspace's
tests, the MSVC C11 and C++17 compile of the generated header driving the real
DLL through its Win32 event handles and named-pipe control channel, and the
check that cbindgen left the committed header alone. The line in the section
above — "the MSVC path is written but cannot be called verified until that
native job runs on a pushed commit" — is now answered: it ran, it was wrong in
ten places, and it is right.

The ten, in the order the runner found them, because the order is the point:
each one was hiding the next, and none of them is visible from Linux or from
Wine. A `\\?\` verbatim path `cl.exe` cannot open; CRLF from the runner's own
checkout against a byte-for-byte comparison; ConPTY's output pipe belonging to
the console host rather than to the child; `ERROR_NO_MORE_FILES` from an empty
pipe namespace; a peer check that cannot run before the first read; a `connect`
test opening `COM1`; `GetTickCount64` measured against QPC;
`ExpandEnvironmentStringsW`'s delimiter rule, which this file had recorded
backwards; `--profile debug`, which cargo reserves; and `/WX` on a CRT
deprecation. Two of them — the ConPTY hangup and the named-pipe peer check —
are product defects rather than test or harness defects, and both would have
presented to a user as a window that stopped responding.

Still open from this sweep, and both written up above: whether a Windows serial
open can block its caller indefinitely, which needs a real port; and
`tt-ttl`'s `set_dir`, which canonicalises into `cur_dir` and so can report a
verbatim path to a macro's `getdir`.

The last deferred TTL file branch is now back where it has meaning. On Windows,
a macro with no BOM is converted from `GetACP()` exactly where the initial file
and every `include` enter the buffer. Legacy code pages go through
`MultiByteToWideChar(MB_ERR_INVALID_CHARS)` and then to UTF-8; CP65001 takes
upstream's own decoder, including its ASCII-`?` replacement and strange
surrogate-pair detour. If the ACP refuses the bytes, the original bytes
survive; Unix retains its existing pass-through. The Windows unit binary
generates bytes in its live ACP and also exercises a rejected CP932 lead byte
and the CP65001 edges, so this is not a compile-only branch.

The 53-script gate deliberately tests a different layer: its upstream fixtures
mix BOM-less CP932 and UTF-8, which no one Windows ACP can interpret without
changing at least one set. The harness adds a BOM only to its private Windows
copies, leaving the read-only checkout untouched and keeping 48 language
transcripts machine-independent; the decoder's unit cases own the real ACP
semantics. Wine's usual CP1252 is therefore neither blessed nor added to the
five-name platform allowlist. The complete MinGW unit binary is 335/335 under
Wine and the script gate retains its 48/5 split; native Linux remains 332 unit
checks plus both script-harness checks, and both target clippy passes are clean.

Reading that CP65001 path corrected the UTF-16 BOM branch beside it as a
separate change. `ToU8W` sounds like a thin
`WideCharToMultiByte(CP_UTF8, 0)` wrapper and is not: `_WideCharToMultiByte`
routes UTF-8 through Tera Term's own `WideCharToMBCP`, which emits ASCII `?`
for an unpaired surrogate. Rust's `from_utf16_lossy` had emitted U+FFFD in the
one damaged-file case specifically preserved by `source.rs`. Both byte orders
now use the shared upstream-shaped encoder and the focused regression runs on
Unix and Windows.

TTL `expandenv` no longer carries its Stage 2 delimiter guess into Windows.
That target now calls `ExpandEnvironmentStringsW`, exactly as `TTLExpandEnv`
does, with the same permissive UTF-8-to-wide and wide-to-UTF-8 helpers on each
side. Unix retains the small portable parser and a shared case pins it to the
API's answer: the closing percent of an unknown name is consumed, not reused
as the opener of the next name. A Windows-only case also covers non-ASCII
environment values and upstream's `?` replacement for an invalid TTL byte.
Those focused MinGW cases pass under Wine and the 53-script gate keeps its
48/5 split; the native Windows job remains the authority for the kernel API.

The Qt shell builds for Windows now, which was the last layer that had never
been compiled there at all. Fedora's `mingw64-qt6-qtbase` is 6.11.1 — the same
version as the native one, so this is the desktop's Qt cross-compiled rather
than an older one standing in — and `sterna.exe` plus seven test binaries link
against `sterna.dll` and Qt with no warnings. Four things were wrong and none
of them was in the C++: cargo's library names are not CMake's, and
`CMAKE_SHARED_LIBRARY_PREFIX` composes `libsterna.dll`, which nothing writes;
the import library is what a Windows link actually consumes and had to be
declared a byproduct before the generator would believe in it; `--target`
belongs only to a cross build, since passing the host's own moves every output
path; and the DLL has to be copied beside the executables, because Windows has
no rpath to point at the cargo tree.

`sterna.exe` is a GUI-subsystem binary, as `ttermpro.exe` is — a console
subsystem would open a console window behind every desktop-launched session,
and closing it would kill the terminal. That leaves the windowless `/V`
diagnostics with no stderr to fall back to, so `MainWindow::note` now asks
`QCommandLineParser`'s own question — an inherited console, or standard
handles named in the startup information — and puts up a parentless message
box when the answer is no.

`cmdline_test`'s listening socket is the platform's own on both sides now:
Winsock's `SOCKET` is unsigned, so the POSIX "no socket" test could never fire
there. `control_test` was left UNIX-only at this point, on the grounds that
shimming its client half would compile and prove nothing about the transport
that is actually used; it has since been written twice instead — see below.
The Linux build, its four event-loop tests and the Windows cross build are all
green; nothing here has been *run* on Windows yet.

The shell's test fixtures then split the same way the crates' did. `cmd.exe`
rather than `/bin/sh` in `macro_test` and `cmdline_test`, and the platform's
temporary directory rather than `/tmp` — the reason is Wine's `Z:` drive,
which makes the Unix spellings work in the emulator and nowhere else, so
keeping them would hide exactly the failures a Windows run exists to find.
`cmdline_test`'s title assertion had to move with its fixture, since the
caption is the program's basename and its arguments.

`pty_test` and `xfer_test` are UNIX-only, and unlike `control_test` the
obstacle is not the transport. Every `pty_test` case is a shell script —
`stty raw -echo` to stop the child echoing, `od -An -t x1` to make the bytes
visible, `stty size` to observe a resize — and naming a Windows shell would
compile and then assert against output that was never going to arrive. The
Windows equivalent is a different test; `macro_test` is what drives ConPTY
through this event loop meanwhile. `xfer_test` needs `rz`, which is lrzsz, and
the protocols themselves are already covered on Windows by `tt-xfer`'s own
tests. The rpath block is UNIX-only too, for both reasons at once: an rpath is
a Unix concept and `pty_test` is no longer a target to name there.

`cmdline_test` now prints *why* a local shell would not open, because the
three checks after it all hang off that one and a failure to spawn otherwise
reads as three separate title bugs. The whole Linux suite — render, pty,
macro, cmdline, control, xfer — is green after the change.

The Windows shell has now been *run*, under Fedora's Wine 11 in the same
container, with the offscreen platform. `cmdline_test` — the whole Tera Term
command line, from `argv` to a window that has connected — passes every check
but one, `macro_test` fails only its two shell-driving cases, `telnet_test`
skips cleanly, and `render_test` executes its entire suite with six failures.
That is the first evidence that this frontend does anything at all on Windows,
and it took two harness corrections to get: `WINEPATH` is a list of *Windows*
paths and a Unix one silently replaces `PATH` rather than adding to it, and
`wineboot` does not finish in this container, so the prefix's registry `PATH`
is never written either. Between them the process had no `PATH`, and what that
looked like from inside Sterna was `CreateProcessW "cmd.exe /c pause" … File
not found` about a `cmd.exe` sitting in `C:\windows\system32`.

Three of the four remaining failures are Wine's rather than ours.
`render_test`'s six are font metrics — ink in cells that should be blank, an
underline no shorter than a letter, two cells that should differ and do not —
and Wine's font stack is not Windows', so they are not Wine's question to
answer; it also faults on exit, in a teardown no assertion had reached.
`macro_test`'s are the two cases where a macro types at `cmd.exe` and waits
for the echo: the connection opens and the caption names it, and then the
screen stays blank, which is the same "Wine's console host does not deliver
ConPTY output" limit already recorded for `tt-conn`. Everything else in that
file — the dialogs, the notifier's re-entrancy, a macro starting and stopping
— passes.

The fourth is ours, and it is the kind of thing a Windows run exists to find.
`cmdline_test`'s last check fails with *"failed to resize console to 80x24:
HRESULT: -2147467263"* — `E_NOTIMPL`, Wine's answer for `ResizePseudoConsole`
— from a call that was setting `terminal.title`. `Session::set_settings`
assigned the settings and reconfigured the engine and *then* propagated the
transport's resize error with `?`, so a caller that saw the failure and
concluded nothing had happened was wrong: everything happened except telling
the far end.

**Settled: applying settings cannot fail, and the missing `Result` is the
answer.** The question was what a partial apply should report, and the honest
reading is that there is no partial apply — the only call under there that can
refuse is the notification, and by the time it runs everything local has
already happened. Both failures behind it are answered elsewhere: a link that
has really gone reports `Event::Disconnected` at the next pump, and a platform
without the call is not the session's news to break. Only two transports can
refuse at all — telnet's NAWS write and the pty's `TIOCSWINSZ` or
`ResizePseudoConsole` — since SSH already discards its own send error and
serial has nothing to tell. Upstream has no error path here either, and the
`?` was the outlier in this port rather than the careful one: `Session::connect`
and the macro host's `connect` had both discarded the same error on their own,
the second with a comment saying it was the transport's and not the command's
to report. So `set_settings` returns nothing, `set_setting` returns the `bool`
that says whether the *name* was in the schema, and the three C entry points
that wrapped them can no longer fail on this path. `cmdline_test` passes
outright under Wine now, which leaves that run with no failure that is ours.

`control_test` now builds and runs on Windows too, which closes the one test
that was deliberately left behind. `Control.cpp` already had the Windows half
— `tt_ctl_wait_handle` and a `QWinEventNotifier` — so the missing piece was
only a client, and it is written twice rather than shimmed: `openEnd`,
`writeAll`, `readAvailable` and a hang-up test, four calls, `sockaddr_un` and
`poll(2)` against `CreateFileW` and `PeekNamedPipe`. The peek is not
decoration. The handle is synchronous, so a `ReadFile` with an empty pipe
behind it blocks the thread that was going to produce the answer — which is
the same shape as the reason the waiting *above* those four calls is shared:
a client on the window's own thread has to wait in the event loop, and that is
a fact about the window rather than about the address. Two smaller
differences: `Scratch` is a no-op on Windows, because the pipe namespace has
no directory to redirect and a pipe leaves nothing behind to go stale, and
"the endpoint is gone" is `WaitNamedPipeW` reporting `ERROR_FILE_NOT_FOUND`
rather than a missing file — every instance busy is a different answer and
only one of the two is what a closed window produces. All eight cases pass
under Wine, which is the one of the four binaries where Wine is a fair
witness: named pipes, `QWinEventNotifier` and a queued close are all things it
implements properly, unlike its fonts and its console host.

The colour OSCs are answered now — `OSC 4`, `OSC 5`, `OSC 10`–`19` and the
`104`/`105`/`110`–`119` resets — which is the first thing a host can change
about this terminal's appearance and the last large family of sequences
upstream implements that this port had not touched. `tt-vt` holds the live
colours because upstream's handler does: `vtdraw_t` keeps six pairs and a
256-entry table, `ts` keeps what the settings asked for, and a reset copies the
second over the first. That split is not a detail — the palette decides which
*index* a truecolor SGR resolves to, so a host repainting it changes the grid
and not only how it looks.

Four things in that family are not what their names promise, and all four are
read off `vtterm.c` and `vtdisp.c` rather than guessed. `XsParseColor` accepts
`rgb:` and `#` and nothing else — `rgbi:`'s arm is in the source commented out
and no CIE or TekHVC form was ever written — and its `rgb:` guard is
`_strnicmp` while its parse is a `sscanf` against a lower-case literal, so
`RGB:0/0/0` passes the first test and fails the second. `OSC 10;a;b;c` walks
its own number along the list, so it is a foreground *and* a background and
then a cursor colour that has no arm at all. `OSC 104;` is not `OSC 104`: an
empty parameter string is still a parameter string, so it resets palette entry
0 alone. And `OSC 105`'s "all" is three colours — bold and blink foregrounds
and the reverse background — not the four `OSC 5` can set, so the underline
foreground is a colour the matching reset cannot put back.

**A thirty-first upstream defect, and it is why `esctest`'s dynamic-colour
tests cannot pass here: a host cannot read back a colour it just set.**
`DispSetColor` writes `vtdraw_t`'s live pair (`vtdisp.c:3376`) and
`DispGetColor` reads `ts` (`:3561`), so `OSC 10;#ff0000` followed by `OSC 10;?`
answers with the *configured* foreground. The paint moves and the report does
not. Only the palette round-trips, because both halves of it are
`vt->ANSIColor`, and Tek does, because its setter happens to write the same
`ts` field the getter reads. Reproduced, since the alternative is a terminal
that reports something Tera Term never reports; found by reading, so it belongs
here rather than in `docs/upstream-bugs.md`.

The oracle could not have arbitrated any of this an hour earlier, and that is
the more useful half of the change. `vtdisp.c` is not compiled into it, so
those three functions live in `stubs_manual.c` — and the version there was
convenient rather than transcribed: one flat array indexed by the `CS_` number,
so a dynamic colour *could* be read back; no eight-colour permutation; and a
`DispResetColor` that ignored its argument and put the whole table back. The
same trap `DispFindClosestColor` fell into in that file, which held xterm's
palette until it was caught. All three are transcriptions now, `ts`'s colour
defaults are in `settings_defaults()` beside the flag words, and
`esctest/run_diff.sh` went from 25 disagreements to 5 — the two engines now
agree byte-for-byte on every colour test, including the read-back defect.

`esctest` itself needed a patch, in a new `esctest/patches/` following
`oracle/patches/`'s convention. A test that fails part-way through reading a
reply leaves the rest of it in the pty and esctest never takes it out, so the
next test's `reset()` reads a stale byte where it expected a CSI and leaves its
own reply behind in turn: the first run that answered a colour query went from
365 passing to **68**, with the log full of `Read f (0x66), expected CSI` in
tests that have nothing to do with colour. It is a property of the harness
rather than of any terminal — an unexpected answer is all it takes — so the
patch drains whatever is readable before each reset. With it, 379 of 568 pass,
up from 365, and the fourteen that moved are the six `ChangeColor` forms
upstream can parse, five `ChangeSpecialColor` ones, `ResetColor_Standard` and
two more.

Answering `DCS + q` — XTGETTCAP — came with it, because esctest asks for the
`Co` capability before it can name a special colour. Upstream implements
exactly one capability under two spellings and answers it out of the colour
flags rather than the palette: 256, 16 or 8, and **nothing at all** when
`EnableANSIColor` is off, which is the one place on the wire that setting is
visible at all. It changed no esctest result, because esctest compares the
reply against its own upper-case request while every terminal answers in lower
case, so `GetIndexedColors()` is 16 for xterm too.

The frontend reads the live colours rather than the settings now, through a
`tt_session_color_rgb` beside the existing palette call and a
`TT_EVENT_KIND_COLORS_CHANGED` that says when the cache is stale — separate
from `Damage`, because re-reading 262 values on every pump would pay constantly
for something that happens once a session. One deliberate divergence:
`ResetSetup`'s `BGInitialize(FALSE)` is inside an `#if 0` upstream, so applying
settings in Tera Term leaves every live colour alone and a colour changed in
the dialog does not appear until Restore setup or Reset terminal. The comment
says it was removed to keep a startup-only *theme* alive, and this port has no
themes; copying it would buy a settings dialog whose colour tab silently does
nothing.

XTWINOPS is answered now — `CSI 1`-`10 t` and `CSI 11`/`13`/`14`/`15`/`16`/`19 t`
alongside the `8`/`18`/`20`-`23` this port already had. It was the largest
remaining cluster in `esctest/expected` that was ours rather than upstream's
decision, and it is the family whose whole point is that the *frontend* is the
authority: a VT engine has no window, and every one of these either describes
one or moves one.

So it crosses the seam in both directions, and the two directions are not
symmetric. The **reports** are a snapshot the frontend pushes on every move,
resize and window-state change (`tt_session_set_window_metrics`), because the
answer to `CSI 14 t` is composed while the sequence is being parsed and there
is nowhere inside `advance` to call into a toolkit. The **actions** are a
bounded queue the frontend drains, the same split `Vt::take_bells` makes for
the same reason. A frontend that never pushes gets a documented notional
window — no chrome, at the origin, 8x16 cells, 1920x1080 work area — and the
oracle's stubs answer with exactly those numbers, so `esctest/run_diff.sh`
compares the two engines on the logic rather than on a desktop neither has.
It now says **568 agree, 0 differ**: the five `XtermWinops_ResizePixels`
disagreements that had been standing since before the colour work are gone,
and nothing replaced them.

Four things in the family are not what they look like. `CSI 13 t` reports x
then y while every size report is **height then width**, which reads as a typo
in upstream and in xterm and is neither. `CSI 13 t`'s sub-parameter 2 is the
text area and 0/1 the frame, while `CSI 14 t`'s are the other way round — also
xterm's, also not a slip. `CSI 10 t` is **not full screen**: upstream's comment
says a PuTTY-style one is what it ought to be and that maximising is the
shortcut it took, so 9 and 10 are one operation with 10 having a toggle 9 has
not. And an unrecognised sub-parameter answers *nothing at all* — the
`default: return` in cases 13 and 14 — rather than falling back to the plain
form.

`CSI 8 t` was already implemented and was half-broken: the engine resized the
grid, correctly, because upstream's `ChangeTerminalSize` does and because the
differential dump is taken at `NumOfColumns`/`NumOfLines` — but nothing told
the window, so the painter drew the new number of cells into the old widget
until some other resize undid it. `Vt::take_terminal_resized` is the flag that
fixes it, and it is set by that sequence alone rather than by `Session::resize`,
so a window resizing itself in response does not come back round as another
request.

Sixteen of the eighteen `XtermWinopsTests` still fail and their reasons are
now four different sentences rather than one. Two now pass. Nine of the sixteen
need a real window, which `tt-host` has not got — they are exercised against
one in `shell/tests/pty_test.cpp` instead, where a child process asks
`CSI 14 t` and reads the answer off its own stdin. The rest are upstream's:
`CSI 9;2 t` and `9;3 t` are not operations Tera Term has, `CSI 8 t` substitutes
the 24x80 default for a zero axis where xterm reads it as "the maximum in that
direction", and the switch ends at case 23, so `CSI 24 t` — DECSLPP everywhere
else — does nothing.

One defect in the colour work above was found while wiring this: **the pump
never emitted `ColorsChanged`.** Only `feed` did, which is the path tests and
the local echo take, so an `OSC 4` arriving over a real connection moved the
palette and told the painter nothing. Fixed, with the regression test on the
transport path rather than on `feed`.

Printing is answered now — the five media-copy sequences, both modes they
turn on, and a Qt frontend at the end of them — which closes the last item in
Stage 3's scope list that had not been touched at all. `CSI 0 i` prints the
screen, `CSI 5 i`/`CSI 4 i` are printer controller mode, `CSI ? 5 i`/`CSI ? 4 i`
are auto print, `CSI ? 1 i` prints one line, and DECPEX chooses which rectangle
the first of those means.

**The mode named after taking the stream away does not take it away**, and
that is the thing to know about this family. Printer controller mode stops the
terminal *executing* controls — they reach the printer uninterpreted, so a line
feed does not feed a line and an `ESC [ 2 J` clears nothing — while printable
characters go on reaching the screen and are copied to the printer through
`OutputLogUTF32` (`vtterm.c:487`), the same tap the session log and the macro
language read. Building it as "send the bytes to the printer instead" gives a
terminal that goes blank for the length of a print job.

It crosses the seam the way the window operations do, for the same reason and
with one queue instead of two: the engine has no printer, so it emits an
ordered `Open`/`Write`/`Close` list and `Printer.cpp` — upstream's
`teraprn.cpp` minus the parts that were Win32 by necessity — is what has one.
`PassThruPort` chooses the destination exactly as `PrnFileStart` does: a named
device gets the code points as they were sent, and an empty setting means the
platform's printer through `QPrinter` with `PrnMargin`'s four numbers and
`PrnConvFF` deciding whether a form feed starts a page. `PassThruDelay` is real
and load-bearing: auto print closes and reopens a job around every line, so
without the wait each one would be a page.

Two settings decide more than their names suggest. `PrinterCtrlSequence` gates
four of the five sequences and **not** `CSI ? 4 i`, so a host can always turn
auto print off again; and `PassThruPort` is not a gate on printing at all but
on *parsing* — `DirectPrn` is sampled when the controller starts and decides
whether the locking shifts and ISO-2022 designations arriving during the job
are the terminal's to interpret or bytes the printer should receive.
Differential cases 133 and 134 are the same input under the two answers, and
the oracle grew `--printerctrl` and `--passthruport` to arbitrate them. Case
132 is the mode itself, which the dump *can* see: a terminal that stops
executing controls is a terminal whose screen stops changing.

**A thirty-second upstream defect came out of reading it, and it is the worst
one on the list: `BuffDumpCurrentLine` (`buffer.c:2400`) smashes the stack.**
Auto print calls it at every line feed and `CSI ? 1 i` calls it directly, so it
is reachable from the wire wherever `PrinterCtrlSequence` is on. Four faults in
twenty-eight lines, all about double-byte characters: `char bufA[TermWidthMax+1]`
is a thousand and one bytes holding up to two per column, so a wide line of
full-width text runs about five hundred bytes off the end with content the host
chose; the fill writes the **low** byte of a double-byte code twice where
`buffer.c:3597` a hundred lines away writes the high byte and then the low one;
the write loop is bounded by the column count rather than by the bytes the fill
produced; and a padding cell's zero reaches `WriteToPrnFile(0, FALSE)`, which
is the *clear the buffer* form, discarding the line so far. `あab` prints as
`a`. **Not reproduced**, and it is the only entry on that list whose reason is
that reproducing it means reproducing a remote stack overflow — this port
prints what upstream meant to print, which for any line without a full-width
character is byte-for-byte the same.

Two smaller things are recorded rather than fixed. `ResetTerminal` clears
`PrinterMode` and no host can reach that code: while the controller has the
stream an `ESC c` is four bytes of printer data, so the flag only ever clears
for Reset terminal on the menu — and it neither closes the job nor stops auto
print, so a RIS mid-job leaves a spool nothing will print. And whether a
wrapped line breaks in the printer's copy depends on whether a *log or a macro*
is running, because the wrap's `CarriageReturn`/`LineFeed` pair sits behind
`NeedsOutputBufs()` and that function does not count the printer.

**It cost throughput, and the first version cost twice what the second does.**
Auto print has to snapshot the line *before* the character lands, because
upstream's wrap calls `LineFeed(LF, FALSE)` explicitly and the dump is at the
top of that function — and holding that snapshot in an `Option<String>` local
put a destructor in every character's stack frame whether or not anything was
printing. Measured at 8% of `core.plain` against the commit before this work,
and 4% of it was that one local; it is a field now, written only under the
flag, and the printer's copy of a character is behind an explicit test at the
call site rather than inside the function. What is left is about 3%, which no
single line accounts for and which is the sort of code-layout shift any change
to a hot parser produces. The gate's budget is 30% and it passes with the
margin it had. The lesson is the local, not the branch: **a `String` that
merely might be filled is not free in the caller that never fills it.**

`PrnFont` is the one printer key still not in the schema: `ReadFont` packs a
name, two sizes and a Windows character set into one value and the generator
has no type for it, so pages are set in monospace and the key is left alone
rather than being given an invented spelling that the user's own Tera Term
would ignore. `shell/tests/print_test.cpp` is the end-to-end gate and needs no
printer — `PassThruPort` points at a file, which is what `PrintFileDirect`
thinks a printer is.

**There is a Windows artifact now, 2026-08-12** — an NSIS installer, 33 MB,
carrying 114 MB across 55 installed files: the shell, the core, `ttctl`,
`ttpmacro`, all
fourteen `.lng` files, the Qt plugins that are not optional, about thirty DLLs
and the licences. `packaging/windows/`, and its README is the second half of
this entry.

**It is cross-built, and that decided the format.** Upstream ships an Inno
Setup script and `iscc.exe` is a Windows program, so matching upstream would
have put Wine on the release path — and Wine has manufactured enough false
findings on this project already. `makensis` is a Linux binary, so the whole
artifact comes out of native tools. The stub is amd64 rather than the
customary x86, which is two departures from convention in one file and the
second follows from the first: an x86 stub cannot be *started* by the only
Wine here, which is 64-bit with no WOW64, and a release artifact nobody can
run before releasing it is the wrong trade for supporting a 32-bit Windows
that could not run the 64-bit program inside it either.

**The DLL set is closed out of the import tables rather than listed.** Qt's own
deployment tooling does not exist for this target — `windeployqt` is a Windows
program and the MinGW package ships no `qtpaths`, which CMake warns about
during configuration. So the build walks `objdump -p` to a fixed point, and
the rule for ours-versus-Windows' is whether the MinGW sysroot has the file.
Forty-five names are left unresolved and every one is a real part of Windows:
the API sets, `d3d11`/`d3d12`/`dxgi`/`DWrite`, `WINSPOOL.DRV`, `UxTheme`.

Verified under the Ubuntu container's Wine, which is the half of this that can
be checked from Linux: a silent install lands 55 files and writes the
uninstall entry Add > Remove Programs reads; the installed `sterna.exe` starts
and stays up, which is what says every DLL resolved and Qt found its platform
plugin; and the uninstaller removes what it installed, leaves a file the user
put in the program folder alone, and leaves the folder holding it. What Wine
cannot answer is how any of it *behaves*, which is a Stage 3 item for a real
machine along with the serial open.

Three things it deliberately does not do, and one it cannot. It does not touch
`PATH`, because `ttctl` and `ttpmacro` sit beside the executable and NSIS's own
documentation describes how a naive registry `PATH` edit truncates somebody's.
It does not touch `sterna.ini`, which is per-user and under AppData, so an
uninstall that is really an upgrade does not take the settings with it. And it
is **unsigned**: SmartScreen will warn and the UAC prompt will say "Unknown
publisher" until there is a certificate, which needs a legal entity. The build
does not have to move when there is one — `osslsigncode` signs on Linux.

`sterna.exe` also carries a version resource now. It already had its icon; what
it had no version, company or description, so an executable whose installer
writes all three into the registry declared none of them itself.

**And it offers `.ttl` to Explorer, off by default** — `/S /ASSOC` for a silent
install, a components entry otherwise. Off is upstream's answer too, and the
reason is better than the one its comment gives: `.ttl` is Turtle as well as
Tera Term, so the registration is the additive `OpenWithProgids` form and the
uninstall gives the extension keys back `/ifempty`. Checked against a `.ttl`
seeded with another program's ProgID, which survives both halves untouched.
The command is `sterna.exe /M="%1"` where upstream's is `ttpmacro.exe "%1"`,
because upstream's `ttpmacro` is the interpreter and this port's is a client of
a running window — the literal registration would find nothing to talk to on a
machine with no window open, and would say so into a console Explorer created
and destroyed in the same instant. What that costs is one open question rather
than a defect: a macro launched from Explorer opens a *new* window where
upstream would run it in the session already on screen, and closing that gap
means giving `ttpmacro` a fallback that starts a window — which changes what a
`.bat` wrapper does today, so it wants deciding rather than assuming.

**The proxy is in the core now, 2026-08-12** — HTTP `CONNECT`, SOCKS4/4a,
SOCKS5 and the prompt-driven "telnet proxy", in `crates/tt-conn/src/proxy.rs`,
with `[TTProxy]`'s twelve keys in the settings schema and both TCP transports
dialling through `proxy::dial`. This is `TTProxy/`'s 8,314 lines answered in
about 600, which is roughly the estimate on the disposition table and worth
saying why rather than claiming efficiency.

**Upstream's proxy is a Winsock hook, and that is where its size goes.** It
replaces `connect`, `gethostbyname`, `WSAAsyncGetHostByName`,
`WSAAsyncGetAddrInfo`, `send`, `recv`, `sendto`, `recvfrom`, `WSAAsyncSelect`
and `closesocket`, so that Tera Term underneath goes on believing it dialled
the host directly — and most of that machinery exists to recover, behind the
API, the host name the terminal asked for, because by the time `connect(2)`
runs the name is a `sockaddr` and a name is what a proxy needs. The four
`begin_relay_*` functions are three hundred lines of the file. Here the
transports say where they are going, so what is left is the four wire formats.

**It is also why TTSSH has no proxy of its own**: two plugins in one process,
neither aware of the other, both under the same hook. `SshParams::proxy` names
that seam, and the handshake runs on a **blocking** socket even for SSH —
`spawn_blocking`, then `client::connect_stream` — so the wire formats have one
implementation rather than a synchronous copy for telnet and an async copy for
SSH. Two copies of a wire format drift.

**The `[TTProxy]` section brought a schema feature with it, and the round-trip
test is what found the need.** It is the first section that is not
`[Tera Term]`, and its strings go through the plugin's own INI layer: `YCL`'s
`IniFile::setString` C-escapes every value and wraps it in double quotes
(`YCL/IniFile.h:258`), `getString` unescapes what comes back. That is
load-bearing — `GetPrivateProfileString` trims whitespace, so
`TelnetConnectedMessage=-- Connected to ` loses the trailing space that makes
it a prompt rather than a prefix of one, and `every_setting_round_trips_through_a_file`
failed on exactly that. So the schema grew `string_esc`, the generator refuses
a section that mixes it with `string`, backticks now quote a *default* as well
as a key, and `tests/upstream.rs` learned which upstream file reads which
section — checking `ProxyType` against `ttset.c` would have called all twelve
keys invented.

**Four upstream defects came out of reading it, thirty-three to thirty-six on
the list in `AGENTS.md`, and the first two are reachable from the shipped
dialog with nothing hand-edited.** An absent `ProxyPort` dials the proxy at the
correct default port and then **skips the relay entirely** — `getPort()`
supplies the default for the address (`:1770`) and the guard tests the raw
stored port (`:1792`), while the dialog explicitly permits an empty port box
(`:956`). A username with a blank password is `strlen(NULL)` on the first HTTP
proxy connection, because `begin_relay_http` reads `proxy.pass` under a test of
the *user* alone (`:1275`) and the dialog stores a blank password box as NULL
(`:1013`). Every SOCKS reply is read with one `recv` whose own comment says the
count may be short, and the callers check only for the error — so a SOCKS4
reply split across two segments reads its result byte out of uninitialised
stack, which can read as 90, granted. And `ProxyType=http+ssl` and its four
siblings parse into types the relay `switch` has no arm for, reaching
`default: result = 0` — connected, no handshake — because `SSLSocket.h` and
`SSLLIB.h` are in the tree, included by nothing and in no build file. A fifth
is small: `atoi(strchr(buf, ' '))` faults when an HTTP proxy's first line has
no space in it. None is reproduced; all five are stated in the module, the
schema and `crates/tt-conn/README.md`, which are the three places somebody
comparing the implementations will look.

One thing that **is** reproduced and reads like a defect: an unrecognised
`ProxyType` is no proxy at all, so a typo is a direct connection rather than a
refusal. That is the schema's ordinary rule for an enumerated setting and it is
upstream's; `none` exists as the spelling that says so deliberately. It is the
one place that rule has a cost worth naming, because the user believes they are
behind a proxy and is not.

**The command line landed the same day**, in
`crates/tt-config/src/cmdline/proxy.rs`: `-proxy=<url>`, `-noproxy` and the
bare `socks5://p:1080/realhost` token, with `/` and `-` both leading and
`proxy` matched case-insensitively — which `ssh` is not, so `/PROXY=` works
where `/SSH` does nothing.

**It is a third parser and not a third branch, and the three run in an order
that is not the one it looks like.** Each plugin hooks `_ParseParam` and blanks
what it consumed before handing the line on, so they compose through the string
rather than through a struct; `TTXInternalGetSetupHooks` installs them from the
**end** of the plugin table (`ttplug.cpp:664`), so the lowest `TTXExports`
order hooks last and is called first — TTProxy is 10 with `/* load first */`
beside it, TTSSH is 2500. `cmdline::parse_all` is therefore the only way to ask
for any of them, and `ssh::parse_both` is gone; `Parsed` is what four call sites
now take apart. The shared machinery — `Action`, `apply_edits`, `switch_body`,
`percent_decode` — moved to `cmdline/mod.rs`, because two plugins doing the same
thing to the same line should not be two copies of how.

The parsing, which is `ProxyInfo::parse` plus `parseURL`:

- `ProxyInfo::parse` (`ProxyWSockHook.h:271`) needs `://` or it returns NULL
  and **leaves any configured proxy alone**, which is what keeps an ordinary
  host name from clearing one. An unrecognised scheme does the same.
- The `@` search runs over the whole remainder *including* the `/realhost`
  part, so `socks5://p:1080/user@host` takes `p:1080/user` as the credentials.
  They are percent-decoded; the same credentials given as settings are not.
- The host loop reassigns `proxy.host` at every unbracketed colon, so the last
  colon separates the port and `a:b:80` is a host of `b`. A `[v6]` literal has
  its brackets stripped **only** in that arm — with no port it keeps them and
  is a name nothing resolves. `parsePort` refuses a leading zero, an empty
  field and 0 itself, and a refusal discards the whole URL rather than
  defaulting.
- `none://` and `ssl://` are the two types that need no host at all. The six
  SSL spellings are in the table, so they clear a configured proxy exactly as
  the others do, and they resolve here to no proxy — which is the schema's
  answer for the same spellings and is the thirty-sixth defect not reproduced.
- With no `/` the function returns the **whole URL** as the real host, and
  `parseURL(url, FALSE)` then sets the type to `TYPE_NONE` and assigns it — so
  a bare URL with no `/realhost` silently **clears** a proxy an earlier
  `-proxy=` set, and the token is left in the line for Tera Term to misparse.
  Upstream's own documentation lists that form under "isn't supported", because
  it collides with Tera Term's `telnet://host`; reproduced.
- `-proxy=` discards the return value, so any `/realhost` after the proxy is
  thrown away. Reproduced.
- The recovered host is applied last and only if Tera Term's own parser found
  none (`TTProxy.h:181`) — a rule about two parsers rather than about either,
  so `parse_all` owns it.
- `instance().defaultProxy = proxy` assigns the **whole** record, so a
  `-proxy=` naming no credentials clears a `ProxyUser` and `ProxyPass` the file
  had. Reproduced, and it is the one place in this parser where applying the
  command line over the settings is a replacement rather than an overlay.

**A thirty-seventh upstream defect came out of writing it, and it is one
character wide: `-proxy=socks5://p:1080/` is no proxy at all.** `parseURL`'s
second argument exists to tell its two callers apart, and the arm testing
whether the real host is *empty* does not consult it (`:2143`) — it assigns
`TYPE_NONE` over the type it has just read. So a trailing slash decides, in
silence, against the thing the option was written to ask for. **Not
reproduced**, and it is the smallest divergence on the list: the harm is
one-sided, no documented form of the option carries a trailing slash, and
`-noproxy` and `-proxy=none://` are how "no proxy" is said. The bare-token half
of the same arm is reproduced, because there the empty answer is about a token
Tera Term is about to see.

**`DebugLog` landed the day after, and it is the only diagnostic a failing
handshake has.** All of the above happens before the terminal has a session, so
a refusal reaches the user as one sentence in a message box — no screen, no
session log, and a transport that never opened. The key names a file and
`proxy::Trace` appends every byte of the handshake to it in `TTProxy/Logger.h`'s
two record formats: `send: [ 05 01 00 ]` for the two SOCKS relays and
`send: "CONNECT h:22 HTTP/1.1\r\n"` for HTTP and the telnet proxy, which is the
division upstream draws by calling `sendToSocket` in one and
`sendToSocketFormat` in the other. Both are byte-exact, so a trace taken here
can be read beside one taken from Tera Term against the same proxy — which is
the strongest use the file has, and the reason the format was reproduced rather
than improved on. It is appended to and never truncated, as upstream's is, so
one file holds every attempt; there is deliberately no record between one
handshake and the next, because a delimiter would be a line no Tera Term
writes. **The credentials are in it**, Base64 being a spelling rather than a
cipher, and that is worth knowing before sending one to somebody.

Reproducing the format cost a small refactor rather than a `trace` argument on
each of the four I/O helpers: `Wire` is the socket and the transcript together,
so no relay can write a byte it did not record or record one it did not write.
It also pays for itself in the one place the two implementations differ on the
wire — a record is written per underlying read, exactly as upstream's is, so a
reply that arrived in two segments is two records here and two records and a
wrong answer there.

Two departures, both stated where they are made. Upstream opens the file while
*reading* the INI file, so the key alone leaves an empty file behind in a
session that never connects; here the first handshake with something to say
creates it. And a path that will not open leaves the handshake untraced and
connecting, which is `Logger::open` keeping its `INVALID_HANDLE_VALUE` — a
diagnostic that can break a session is not one.

**It also found where the file goes, which was wrong in two other places.**
A relative `DebugLog` resolves against `ts.LogDirW` (`TTProxy.h:198`), and
`ts.LogDirW` is `GetLogDirW()` — the **program's** log and dump directory,
which takes no settings at all. `GetTermLogDir` is the *terminal's* and
consults `LogDefaultPath` and then `FileDir` before falling back to it, so the
two coincide exactly when neither key is set: every default install and no
configured one. `tttypes.h:579` says so in as many words and this port had one
function for both, which put `TELNET.LOG` (`telnet.c:129`) and the six protocol
logs (`ttpfile/zmodem.c:815` and its siblings) wherever the session log was
configured to go. `logname::program_log_dir` is the second answer now.

Interop is against in-process servers rather than a real `squid` or `dante`,
for the reason `oracle/` exists — a test that needs a daemon installed is a
test that does not run. `tests/proxy.rs` drives all four protocols over a real
socket and every case asserts what a byte-level test cannot: **after `dial`
returns, the next byte read is the session's first byte.** A handshake that
leaves one byte behind gives a first screen with a stray character on it and an
SSH key exchange that fails with a protocol error, and neither points at the
proxy.

**And there is one check against a server nobody here wrote**, which the four
above cannot be: `a_real_socks_server_agrees` drives OpenSSH's `ssh -D`, which
speaks SOCKS4 and SOCKS5 both and auto-detects by the version byte. Four cases
— each protocol under `SocksResolve=local` with a literal and under `remote`
with a name — and each one sends as well as receives, because a tunnel that
only ever carries the server's greeting passes a handshake test without being
a tunnel. Skipped without `TT_SOCKS_PROXY`; the recipe for the throwaway
`sshd` is on the test. It needs no `sudo` and no account, unlike the SSH rig,
because the SSH server it starts only has to accept a forwarding request from
the user who started it. Run 2026-08-12: all four passed first time, which is
the first evidence in this file that the SOCKS reading is right rather than
merely self-consistent.

**Tabs close Stage 3, 2026-08-12.** A `TerminalPage` owns one `Session`,
`TerminalView`, scrollbar, `Printer`, `Macro` and transfer dialog. That is the
unit rather than the widget alone: output, scrollback, printing, an interpreter
and a modeless protocol dialog all have state which must keep belonging to the
line that created it after another tab is selected. `Macro` is declared before
`Session` so it is destroyed first; a running interpreter must not retain a
session whose page has already started coming apart.

The lightweight tab bar hides itself for one page and remains movable and
closable for two. Its pages can now be assigned to one, two side-by-side, or
four 2x2 equal panels; the tabs remain unlimited and hidden sessions keep
pumping. Menus, status, title, key-map actions and the window-wide control
socket follow the highlighted pane, while signals that only report background
work update that page's tab and pane title without stealing focus. Opening a
connection from a live page creates another page rather than replacing the
line. Closing asks about the target page without displaying a hidden one, and
network `AutoWinClose` removes only its page unless it was the last one. The
tests pin the property underneath all of this: bytes fed to one session never
appear in another's grid.

**Simultaneous panels landed after the roadmap, 2026-08-13.** View chooses
Single, 2 panels or 4 panels without reserving a terminal shortcut. Expansion
and reduction keep the active connection first, then visible connections and
then tab order; selecting a hidden tab replaces the active slot, and closing a
visible tab refills from the hidden tabs before showing a connection tile.
Empty tiles allocate no session until Serial, SSH, Telnet or Local shell is
accepted. `[Sterna] PanelLayout` is synchronized over every page and persisted
as a targeted one-key INI update. Each visible page refits to its panel and gets
its own client geometry and window-metric snapshot; changing panels never
resizes the top-level window. `tabs_test` covers the assignment model and the
integrated routing on both targets.

**Duplicate session follows upstream's narrower rule: live SSH and telnet
only.** Serial and local shells have no Duplicate action. The destination gets
the source page's in-memory settings and live grid size, then reopens the same
target; SSH authentication is asked again because Sterna does not retain a
password after answering a prompt. The implementation also found a seam the
proxy tests above could not see: the Qt SSH launcher called the connection ABI
without the destination session, so it had no way to read `[TTProxy]` and
dialled directly. `tt_ssh_connect_for_session` now applies the session's live
proxy to ordinary and duplicated SSH opens, matching telnet's existing path.

`tabs_test` opens two real loopback telnet connections, changes a setting only
in memory, duplicates the first, and asserts independent grids, the copied live
value and active-page action routing. The full Qt 6.11.1 suite is 10/10 on
Fedora; the Windows shell cross-build is clean and that same test says `tabs
ok` under Wine. The live-settings ABI is also compiled and driven from C.

**Stage 3 is complete.** Every item in its scope line now has a shipped path
and an executable test boundary: Windows, ConPTY and serial, the installer,
languages, terminal depth, tabs and duplication, proxying, and printing.

### ✅ Stage 4 — depth and polish — **COMPLETE 2026-08-12**

DEC special graphics (line drawing — not CJK) already landed with the Stage 1
VT engine and renderer. The Lua plugin surface is complete as of 2026-08-12:
menu items, global key bindings, connect/disconnect hooks, binary-safe
byte-stream filters and custom settings pages. Ordinary callbacks retain a VM
per terminal tab; filters use an isolated, bounded fast-path VM so a wait or
dialog cannot stop terminal I/O, with scalar filter and setting controls shared
between the two. Typed plugin pages join the generated settings dialog, read
and preserve the active INI, apply live, and copy with a duplicated tab. Sixel
and the signed self-updater landed on 2026-08-12. **No deb** — the AppImage-only
decision in Stage 1 covers this too.

**Sixel is inline, bounded and scrollback-aware, 2026-08-12.** `tt-vt` streams
a DEC sixel DCS directly into an RGBA raster: repeat, raster attributes,
RGB/DEC-HLS color registers, transparent or opaque background, graphics CR and
graphics newline are all covered. A malformed or unterminated stream cannot
become an unbounded input buffer, one raster is capped at 4096×4096 / 64 MiB,
and the image history is capped at 128 MiB oldest-first. Netpbm 11.5.2's
`ppmtosixel` output is a regression fixture rather than only a hand-written
grammar test.

The image is anchored to an absolute grid line and follows ordinary text into
history. Cell snapshots make later text and erase operations punch out the
corresponding image tiles without teaching every grid edit about images. The
main and alternate screens keep separate image sets, and reset, resize and
history eviction clean them up. The flat ABI lends RGBA8888 storage to the Qt
shell, which paints text first, the image second and the cursor last; the render
test proves both the red pixels and a later text cell replacing them.

Modern xterm scrolling semantics won over the older DEC manual's opposite
description: scrolling is on after reset, the cursor-relative image follows
history, and `DECSET ?80` fixes it at the page origin without moving the
cursor. `XTSMGRAPHICS` reports 256 registers plus current and maximum geometry.
Primary DA remains Tera Term's byte-exact answer, so applications which require
xterm's `;4` marker must be told to emit sixel; those which query the capability
directly get a useful answer. The core, C ABI, Qt renderer and an external
encoder each have an executable boundary. See `docs/sixel.md`.

**Signed updates close Stage 4, 2026-08-12.** Help > Check for Updates loads a
local updater library on demand, verifies a detached Ed25519 signature before
trusting the manifest's version, URL or size, and then checks the selected
artifact's exact size, SHA-256 and its own signature.

**A startup check joined it on 2026-08-13**, on by default: a signed release
nobody hears about is a security fix nobody installs. `[Sterna]
CheckUpdatesOnStartup` is the switch and `LastUpdateCheck` the schedule — one
check per 24 hours, three seconds after startup, skipped while a modal dialog is
up so that an offer cannot land on an SSH password prompt, and omitted for a
deliberately hidden `/V` run. It is silent unless there is an update: no progress
dialog, no "you are current", and no complaint about an unreachable server,
because a box on every launch is how a security feature gets turned off. The
stamp is written when the request goes out rather than when it succeeds, so a
machine that is offline costs one attempt a day. The decision is made in the
terminal, before the library is loaded — a session with the switch off, or one
already checked today, still maps neither Qt Network nor a TLS backend (verified
from `/proc/<pid>/maps`). The 256 KiB manifest, 1 KiB signature and 128 MiB
artifact ceilings are enforced while bytes arrive, not after an unbounded
download. The signing tool derives the public half from the encrypted release
key and refuses a key that does not match the one compiled into the C ABI; its
public fixture crosses that ABI in both Rust and Qt tests.

On Linux, `QSaveFile` writes beside the running AppImage, restores its execute
permissions before the atomic rename and leaves the mounted old image running
until the next start. On Windows, the verified NSIS installer waits for the old
process before uninstalling anything, upgrades silently and restarts through
Explorer rather than leaving an elevated terminal. It also has to be let go of
before it can be started: a `QTemporaryFile` keeps its file open whatever
`close()` suggests, and Windows refuses an image section for a file another
handle holds open for writing, so until the download was detached the installer
could not run at all — a failure only native Windows can see, and one the shell
reports in its own words rather than ours (fixed 2026-08-13). Loose builds open
the release page instead of guessing what to replace. Qt Network and its TLS
stack are absent from a terminal that is not due a check: linking them directly
measured about 5 MB more idle PSS, so `sterna_updater` is loaded only for an
explicit or scheduled check. Both packages carry the TLS plugin their platform
needs, including the Windows Schannel backend which import-table discovery
cannot see.

**The macro reference is generated, 2026-08-12.** `docs/macro/` converts all
214 pages of the pinned English Tera Term macro manual to Markdown and retains
its one diagram. The command index takes its 209 accepted spellings from
`tt-ttl`'s live reserved-word table, maps them to 208 implementations, and
generation fails if either the interpreter or the upstream index names a
command the other does not. CI also byte-compares the committed tree against
the pinned manual. The visible compatibility note distinguishes command
semantics, which Sterna keeps, from upstream executable and UI instructions.

**Stage 4 is complete.** Its scope now has executable boundaries for DEC line
graphics, Lua extensions, sixel and both signed package-update paths. The
four-stage roadmap is complete without widening into deb packaging or any of
the permanently dropped compatibility surfaces below.

**Realistic total to a credible replacement: 15–20 months solo with AI
assistance.** Full parity is 3+ years and should be explicitly renounced in the
README.

---

## 🟢 Deliberate deviations — the work after the roadmap

The four stages were about being *the same*: every default transcribed, every
quirk reproduced, and `AGENTS.md`'s trap list is the receipt. With that done, the
program can start being *better* where being the same costs the user something.
Each divergence is written up in **`docs/deviations.md`** with its reason, what
stays compatible, and where it lives — so that somebody diffing the two programs
can tell a decision from a bug, which is exactly what four stages of
transcription would otherwise make impossible.

The rule for going on that list: user-visible, not forced by the platform, and
reproducing upstream instead would have been easy. A divergence Linux or Qt
forces is a port, and belongs in a code comment and in `AGENTS.md`.

Eight so far, all 2026-08-13:

1. **The default baud rate is 115200**, where `ttset.c:919` gives 9600. The
   key, its parse and its absence of bounds are unchanged, so `BaudRate=9600`
   still opens at 9600 in both programs — only the value used when the key is
   absent moved. Nothing this program gets pointed at ships a 9600 console any
   more.
2. **The connect dialogs remember the last connection, across restarts.**
   Upstream's host dialog is seeded from `ts`, and `ts` reaches the file only
   through Setup > Save, so Tera Term forgets on exit. Sterna writes the record
   when a link actually opens: the serial line settings into `[Tera Term]`'s own
   `BaudRate` family — which is already what a macro's `setbaud` does — and the
   endpoints upstream has no key for into a `[Sterna]` section nothing upstream
   reads. `tt_session_settings_remember` writes **only** those keys and leaves
   the file alone when it already says them, so an INI shared with a real Tera
   Term is not handed every other schema default by a connection.
3. **A small serial toolbar** keeps the selected port, connect/disconnect and
   local echo in reach without changing what the existing actions do.
4. **One, two or four simultaneous panels** show several independent tab
   sessions in one window while keeping one highlighted active target. Hidden
   tabs continue running, and the layout remembers no connections of its own.
5. **A signed update check at startup**, on by default and limited to once per
   day, stays silent unless it has a release to offer. The manual check remains
   available when the schedule is off.
6. **Highlight rules**: an ordered list of regular expressions, each with a
   foreground colour, a background colour and attributes, applied to what is on
   the screen. Upstream has no pattern or keyword highlighting anywhere — every
   colour a cell can take is the host's decision, and its regex library lives in
   `ttpmacro`, a separate process that never sees the screen. Nothing is written
   into a cell: matching happens over the visible rows *while they are painted*,
   so the grid still holds what the host sent, the log and the clipboard and the
   oracle see an unhighlighted terminal, the receive path costs nothing, and a
   rule written now colours what arrived an hour ago. The engine is the Rust
   `regex` crate rather than `tt-ttl`'s Oniguruma, which costs backreferences
   and lookaround and buys a linear-time guarantee — this runs on the UI thread
   and the far end chooses the haystack.
7. **Quick buttons**, a second bar of commands the user defined — text, bytes,
   a macro or a menu command, each optionally on a shortcut and optionally
   asking first. Upstream's equivalent is a `KEYBOARD.CNF` user key, and a
   button *is* one: same four kinds, same escape, same `run_user_key`. What is
   new is that it has a face, an editor and a list of its own
   (`[Sterna Buttons]`, which is a list and so cannot be schema rows). Two
   choices are argued in `docs/deviations.md` — the bar does not exist until a
   button does, and no button ships with a shortcut, because a Qt action
   outranks the terminal widget and takes the key from the host silently.
   A button can also **repeat** — *n* sends every *x.x* seconds, or until it is
   stopped. The count and the interval are two more keys in the same section;
   the clock is not, and cannot be: the engine is a function of its bytes, so
   `QuickButtonRepeat` schedules and the core only records what was asked for,
   the same split the bell's governor makes. A run stops on a second press, on
   Escape in the terminal, when the list is edited, or when the link it was
   sending down goes away — and it stays bound to the session it started on
   rather than following whichever tab is in front.
8. **Editable lines for every connection type**, a small local editor at the
   terminal cursor that holds printable input until Return. It is deliberately
   separate from telnet LINEMODE: serial, SSH, raw TCP and local shells get the
   same correction-before-send behaviour, while function keys, mapped keys,
   protocol replies and all non-editor control keys keep their immediate paths.
   The accepted line is echoed once without assigning LocalEcho or SRM, so the
   tab's prior echo preference returns when the editor is turned off.

`[Sterna]` is the first invented section in the schema, and
`tt-config/tests/upstream.rs` now asserts in both directions: an upstream
section's keys must exist upstream, and `[Sterna]`'s must not — a key in both
places would be a second answer only one of which a real Tera Term can see.

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
- **`KEYBOARD.CNF`** — ✅ **done 2026-08-10**, `crates/tt-config/` through the
  same `GetPrivateProfile*`-compatible INI layer, then wired to the session,
  C ABI, Qt shell and TTL `loadkeymap`. Physical scan codes stay physical
  across Wayland, X11 and Windows; duplicate resolution and the two different
  `off` parsing rules match upstream.
- **Hosts and keys** — read Tera Term's `ssh_known_hosts` *and*
  `~/.ssh/known_hosts`; read `~/.ssh/id_*` and `~/.ssh/config`; write OpenSSH
  format.
- **`.lng` files** — ✅ **done 2026-08-10.** The exact 14 files are
  vendored, loaded through `tt-i18n`, installed with the shell, selected by the
  compatible `UILanguageFile` setting, and used by the main menus and generated
  settings UI. Connection forms and prompts, transfer and macro dialogs, paste
  and disconnect confirmation, and common file-picker captions use every
  catalog key whose upstream field has the same meaning. Sterna-only text stays
  source-language rather than taking an inaccurate key. Do **not** migrate to
  Qt `.ts`: that throws away 17,610 lines of donated translation and the
  translator workflow.
- **TTX plugins** — replace in order: (1) fold the ones that matter into core —
  `TTXProxy` (~1k Rust), `TTXKanjiMenu`, `TTXResizeMenu`, `TTXttyrec`; (2) a
  **Lua plugin API** — menu items, key bindings, connect/disconnect hooks,
  byte-stream filters, settings pages, covering what the 17 samples in
  `TTXSamples/` actually do; (3) WASM component plugins only if someone asks.
  All five surfaces are **done 2026-08-12**: direct `.lua` files load in
  filename order, each tab retains its own callback and stream state, Qt
  installs stable menu paths and portable shortcuts, lifecycle edges queue
  rather than disappearing behind an active callback, and ordered input/output
  filters cover the terminal stream without touching file-transfer packets.
  Filter failures disable only that callback and pass bytes through. Typed
  bool, bounded-int, string and enum pages join Setup, share live state with
  both Lua VMs, and persist in plugin-owned INI sections without disturbing
  the rest of the file. The Lua command surface itself has been here since
  Stage 2 (`crates/tt-lua/`); hooks are a separate layer above it.
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
   replies*, in CI on every commit. 134 cases, two of them `xfail`. Since the
   oracle also takes
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
   **20 matching, 2 known-divergent, 5 not run**, each with a recorded reason in
   `oracle/upstream.cases`. The prediction above was accurate — `bcetest.sh`,
   `decfra.sh` and the `#38168-deccara-*.sh` trio were among the breakages, and
   three of them turned out to be upstream bugs rather than ours. Of the two
   still divergent, one is `vte` following ECMA-48 where Tera Term's CSI parser
   does not (see below) and the other is spacing combining marks, deferred with
   CJK. **Two of the nine original xfail notes named the wrong cause** — a
   reminder that an xfail reason is a hypothesis until something re-tests it.
   The 53 `.ttl` files now run as the TTL conformance suite in
   `crates/tt-ttl/tests/scripts.rs`; each has a reviewed transcript.
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
(the `sterna-fedora` distrobox, see `AGENTS.md`).

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
