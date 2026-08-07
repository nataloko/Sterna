# termitta — plan and status

Canonical roadmap. Update the status markers as work lands; this file is the
thing a fresh session should read first, together with `CLAUDE.md`.

**Last updated:** 2026-08-07 · **Stage:** 0 (de-risking) · **Commits:** 11

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

### Open — decide before they get expensive

- **Project name.** `termitta` is taken in the wild (an existing Qt terminal, plus
  `qtermwidget`). It ends up in the binary name, config path and desktop file.
  Cheapest to change now.
- **Licence.** No `LICENSE` file yet. Cheapest with one contributor. Qt LGPLv3
  permits dynamic linking under any licence; Tera Term's vendored code is
  3-clause BSD.
- **Qt licensing posture.** LGPLv3 forces dynamic linking; static needs
  commercial or GPL. Affects "light" (≈30 MB Qt runtime bundled on Windows;
  free on Fedora, it's in-distro).
- **Vendoring clearance.** `ttpfile/*.c` carry inline 3-clause BSD headers and
  are clear. The 14 `.lng` and 49 `.map`/`.tbl` files have **no per-file
  headers** — confirm against the copyright page before copying. See
  `ATTRIBUTION.md`.
- **Report the `BuffGetAnyLineDataW` bug upstream.** See below.

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

### ✅ Stage 0 — bootstrap + de-risking (3–4 weeks)

Spike 1 delivered `oracle/` — see `oracle/README.md`. Result exceeded the plan:
**15,325 lines compile unmodified**, not the 12,082 estimated, because
`charset.cpp` and `unicode.cpp` came along free (and they carry the CJK width
and ISO-2022 behaviour, so that matters).

Only spike 5 remains. Also still open: Qt licence posture, and CI — copy the
matrix from `../tine/.github/workflows/release.yml`, keep linux-x64 and
windows-x64, drop macOS/Flatpak.

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

#### Spike 4 result — adopt `serialport-rs`, with a thin patch layer

Audited 2026-08-07 against an FTDI Quad RS232-HS with two ports wired
back-to-back, and — this is the point — **against the requirement in
`commlib.c`**, not a generic wishlist. What Stage 1 needs works: enumeration
with USB vid/pid/manufacturer, 5–8 data bits, 1/2 stop bits, none/odd/even
parity, both flow-control modes, latched `set_break`/`clear_break` matching
`SetCommBreak` semantics, DTR→DSR and RTS→CTS both observable, honest timeouts,
and every baud rate exact on readback from 300 to 3000000 including a
non-standard 250000. Hotplug: removal and re-attach both seen by
re-enumeration.

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

### ⬜ Stage 1 — the Linux serial + SSH terminal (3–4 months, ~25–30k LOC)

Must be shippable and genuinely useful, not a demo.

- `tt-vt` + `tt-grid`: VT100/220 + core xterm, SGR/256/truecolor, scrollback,
  selection, BCE, wide + combining chars. Ported **against the oracle**.
- `tt-conn`: **serial first** (the differentiator), then SSH2 via `russh`, then
  telnet, then local PTY via `portable-pty`. Serial is `serialport-rs` plus the
  patch layer spike 4 specified: `CMSPAR` parity, `PARMRK` break detection,
  `VSTART`/`VSTOP`, a userspace DSR-flow shim, and by-path port identity.
- Qt shell: one window, grid painter, clipboard, font/colour config,
  connect dialog, serial-port picker with live enumeration.
- **`~/.ssh/config`, `~/.ssh/known_hosts`, `~/.ssh/id_*`** — Tera Term lacks
  this and it is a major Linux adoption lever.
- Session logging (timestamped, rotation).
- rpm + AppImage. Fedora first.

**Done when:** the Wine shortcut gets deleted and it's daily-driven for serial
console work.

Deliberately absent: file transfer, macros, tabs, Windows build, most settings.

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
Lua plugin API, sixel, self-updater, deb.

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

1. **✅ Differential testing against real Tera Term** — `oracle/`, built and
   green. Feed identical byte streams to it and to the Rust engine, diff the
   grid dumps, on every commit. **This is the asset the whole project rests on.**
2. **⬜ esctest2** (iTerm2) — ~1000 automated DEC/xterm conformance assertions
   over a pty, read back via DSR/DECRQSS. Wire into CI in Stage 1.
3. **⬜ vttest** (Dickey) — interactive; manual gate plus screenshot diffing at
   each stage boundary.
4. **⬜ Tera Term's own corpus** — the 33 `.sh`/`.pl`/`.rb` exercisers in
   `teraterm/tests/` as golden-file tests. `unicodebuf-combining*.pl`,
   `bcetest.sh`, `decfra.sh` and `#38168-deccara-*.sh` are the ones that will
   break and all stay in scope; `unicodebuf-east_asian_width.txt` is deferred
   with CJK. The 53 `.ttl` files as the TTL conformance suite.
5. **⬜ Fuzzing and property tests.** `cargo-fuzz` on the parser — it eats
   untrusted network bytes. `proptest` invariants: cursor in bounds, wide-char
   pairs never split, scrollback monotonic, no attribute leaks across BCE.
6. **✅ Protocol interop** over a pty: `sz`/`rz`, `sb`/`rb`, `sx`/`rx` (lrzsz)
   for x/y/zmodem, `gkermit` for kermit. Built and green — `xfer/run_tests.sh`,
   10/10 both directions. Use **G-Kermit, not C-Kermit**: C-Kermit sees a pty as
   a tty and drops into interactive mode. Wire it into CI alongside the oracle.

**Plus a perf gate from Stage 1**, calibrated the way `../tine/docs/BENCH.md`
describes: cold start (ms), idle RSS, time to render 10 MB of `cat`,
input-to-present latency. Publish the numbers in the README.

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
   rather than insurance.** Do not let a green spike 5 read as "SSH is done".
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
