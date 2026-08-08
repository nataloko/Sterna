# termitta — plan and status

Canonical roadmap. Update the status markers as work lands; this file is the
thing a fresh session should read first, together with `CLAUDE.md`.

**Last updated:** 2026-08-08 · **Stage:** 1 in progress · **Commits:** 106

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

- **Project name: `termitta`.** The working name `qtterm` collided with an
  existing Qt terminal and with `qtermwidget`, and tied the project to a toolkit
  the architecture deliberately treats as swappable. Upstream is
  <https://github.com/nataloko/termitta>. Accepted cost: `termite` is a known
  (archived) VTE terminal one letter away, so search results will mix a little.
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
┌─ termitta-core (Rust cdylib) ───────────────────────────────────────────┐
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
the oracle's existing `winshim`, which turned out to already cover most of what
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

- `tt-vt` + `tt-grid`: VT100/220 + core xterm, SGR/256/truecolor, scrollback,
  selection, BCE, wide + combining chars. Ported **against the oracle**.
  ✅ **done for Stage 1's purposes** — 102 differential cases and 365 of
  esctest's 568, see below. What
  remains is selection, which is a frontend concept the grid only has to
  support.
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
  `libtermitta.so` plus a generated, committed, CI-gated header, exercised from
  C and C++ rather than from Rust. See `crates/tt-ffi/README.md` and below.
- Qt shell: one window, grid painter, clipboard, font/colour config,
  connect dialog, serial-port picker with live enumeration. ✅ **done for
  Stage 1's purposes** — all of that, plus scrollback with a scrollbar, wheel
  and `Shift+PageUp`, the SSH connect path with its host-key and authentication
  dialogs, telnet, and a local shell on a menu item. See `shell/README.md` and
  below.
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
  not pay. The base is `termitta-fedora`, so the **glibc floor is 2.43**: this
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

`crates/tt-ffi/`, 2026-08-08. `libtermitta.so` and a generated
`include/termitta.h`: session lifecycle, zero-copy row reads, the key and mouse
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
it is open, and a status line. Built and run in the `termitta-fedora`
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

### ⬜ Stage 2 — the differentiators (3–4 months, ~20k LOC)

- **File transfer**: FFI to the vendored C, all six protocols, interop-tested
  against `lrzsz` and `gkermit`.
- **TTL interpreter**: native Rust, **in-process on a thread** — deletes ~2,600
  LOC of DDE glue (`ttpmacro/ttmdde.c` + `teraterm/ttdde.c`) and a whole class
  of races. Target: the 53 `.ttl` scripts in `teraterm/tests/` pass.
- **Lua via `mlua`** over the same `ScriptHost` command table (~500 LOC glue).
- `ttctl` JSON-RPC control socket replacing DDE. Keep a `ttpmacro script.ttl`
  CLI entry point so existing shortcuts and `.bat` wrappers keep working.
- **Settings schema + generated dialogs**, first pass.
- `TERATERM.INI` and `KEYBOARD.CNF` readers.

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

- **`TERATERM.INI`** — read *and write* natively, bug-compatible with
  `GetPrivateProfile*` (duplicate-key semantics, no quote stripping, CRLF,
  encoding fallback). ~600 LOC hand-rolled; **do not use a generic INI crate**.
  New settings go in an additive section so round-tripping with real Tera Term
  survives.
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
could not predict is the one number that turned into a finding.

**Throughput through the window is dominated by how many frames get painted,
and on X11 that is 8x too many.** The same binary, the same machine, the same
ten megabytes:

| platform | frames | throughput |
|---|---:|---:|
| wayland | ~390 | 27–39 MB/s |
| offscreen | ~2900 | 7 MB/s |
| xcb | ~3000 | 4 MB/s |

Wayland's frame callbacks throttle repainting to the compositor, so several
8 KB reads coalesce into one frame. X11 has no such brake, and the session
pumps once per notifier wake — so every read is its own turn of the event loop
and its own frame, and a burst is absorbed 6–9x more slowly.

**The fix is in the shell, not in the engine**, and it is the one the event
loop's design deliberately left open: `pump` takes a budget, the shell passes
zero, and zero means "read once and repaint". A small time budget would let a
burst coalesce on every platform while still repainting at well over 100 fps.
Worth doing before Stage 1 ships, and worth measuring rather than assuming —
the harness that found it is the harness that can prove it.

It also means **a headless CI number would understate the desktop by 4x**,
which is the opposite of the usual assumption about offscreen being the fast
case. One more reason the shell half of the gate is local.

### Measured baseline — Qt 6 Widgets, 2026-08-07

A bare `QWidget` painting an 80x24 grid with `QPainter` — no GPU, no damage
tracking, no glyph atlas, per-cell `drawText`. A floor, not a ceiling.
**Measured on Qt 6.11.1 / Fedora 44, which is what the desktop actually runs**
(the `termitta-fedora` distrobox, see `CLAUDE.md`).

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
2. **Motivation cliff at the dialogs.** 76 dialogs arrive right after the fun
   part ends. The settings-schema codegen is the mitigation and must exist
   before it's needed.
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
   `termitta-fedora` distrobox runs Qt 6.11.1, matching the host exactly. Use it
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
