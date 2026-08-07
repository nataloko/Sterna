# Cross-platform Tera Term successor — Rust core + Qt 6 shell

## Context

Tera Term is a *communications terminal* — it connects **out** to serial ports, telnet,
SSH and named pipes. Its peer group is PuTTY, SecureCRT, minicom and YAT, not
GNOME Terminal or Alacritty. That distinction matters: the gap it fills on Linux is real
and unserved. `minicom`/`picocom` have no scripting and no GUI, `cutecom`/`moserial` are
toys, PuTTY has serial but no scripting or file transfer, and the one tool that genuinely
covers this ground — SecureCRT — is closed and paid.

But Tera Term is Windows-only, and not incidentally so. Verified against the tree:

- **~157k net SLOC** first-party (215k raw), C + C++, no MFC or ATL.
- **`_WIN32` appears zero times.** No `__linux__`, no `__unix__`. There are no
  portability guards because the code has never been asked to compile off Windows.
- **No portability layer exists.** `teraterm/common/tmfc.*` looks like one but is a thin
  `HWND` wrapper (`class TTCWnd { HWND m_hWnd; ... }`), 19 subclasses deep.
- 184 files include `<windows.h>` directly; 220 reference `HWND`.

An in-tree port is therefore not on the table — it would be a rewrite wearing a port's
clothes, and upstream has 30 years of Win32 commitment to defend.

**Two verified findings make a fresh implementation far cheaper than 157k LOC suggests:**

1. **The renderer seam is tiny.** `teraterm/teraterm/vtdisp.h` exports 75 functions, but
   only **two draw text** (`DispStrA`, `DispStrW`). `vtterm.c` — 5,939 LOC running the
   entire VT100/220/320/525 + xterm state machine — contains **zero** `HWND`/`HDC`/
   `windows.h` tokens and makes **zero** drawing calls. It touches 29 display functions,
   none of which paint. The engine↔renderer contract is already narrow and clean.
2. **The file-transfer protocols are already portable.** Win32 token counts:
   `xmodem.c` **0**, and `kermit.c`/`zmodem.c`/`ymodem.c`/`bplus.c`/`quickvan.c` **2 each**
   — all of them `#include <windows.h>` pulling in `BOOL`. A ~30-line shim header
   compiles all 9,777 LOC on Linux unchanged.

**Outcome sought:** a free, native, GUI serial + SSH terminal with real scripting and
legacy file transfer, on Windows and Linux — the ~20% of Tera Term that nothing else on
Linux does, done excellently. Not feature parity.

**Decisions taken:** focused successor scope · Rust core + Qt 6 Widgets shell ·
Windows + Linux · Stage 1 centred on **serial and SSH/telnet** (the user's daily-driver
needs); TTL and file transfer are project differentiators, deferred to Stage 2.

**Project home:** `/home/nata/agents-home/Projects/qtterm` — empty repo, remote
`github.com/nataloko/qtterm.git`, branch `main`, no commits yet. `qtterm` is a working
name; pick the real one before the first public push, since it ends up in the binary
name, the INI path and the desktop file. Tera Term stays read-only at
`../teraterm` as reference, oracle and vendoring source.

---

## Architecture

One process, one core library. The frontend is replaceable because it only ever sees a
flat C ABI over POD types.

```
┌─ frontend: Qt 6 Widgets (C++) ──── swappable: Tauri / TUI / headless ─┐
│  QWidget grid + QPainter glyph atlas · .ui dialogs                    │
│  QInputMethodEvent → ibus/fcitx5 · menus · clipboard                  │
└──────────────────────── C ABI (cbindgen) ─────────────────────────────┘
┌─ teraterm-core (Rust cdylib) ─────────────────────────────────────────┐
│  tt-vt       VT100/220/320/525 + xterm state machine (over `vte`)     │
│  tt-grid     cells, scrollback, selection, BCE, wide/combining        │
│  tt-charset  49 vendored .map/.tbl tables, DEC sets, EAW policy       │
│  tt-conn     serial | ssh (russh) | telnet | pty | pipe    [tokio]    │
│  tt-xfer     FFI → vendored C: x/y/zmodem, kermit, bplus, quickvan    │
│  tt-script   TTL interpreter + mlua over one shared command table     │
│  tt-config   INI (GetPrivateProfile-compatible) + KEYBOARD.CNF        │
│  tt-i18n     .lng loader                                              │
└───────────────────────────────────────────────────────────────────────┘
```

**Crossing core → frontend:** a drained event queue plus a zero-copy read API —
`Damage { rows }` + `snapshot(row) -> &[Cell]` where `Cell` is POD
`{ text: [u32;N], fg, bg, attrs: u32, width_class: u8 }`; OSC/window requests (title, bell,
palette, cursor shape, mouse mode, clipboard); connection lifecycle including
**prompt-needed** (password, keyboard-interactive, host-key verification); transfer
progress; and the five script dialog requests that `teraterm/ttpmacro/` already defines
(`inpdlg`, `msgdlg`, `statdlg`, `ListDlg`, `errdlg`).

**Crossing frontend → core:** `key_event(keysym, mods)` — **the core owns the keymap**,
because `KEYBOARD.CNF` is a compatibility artifact; `paste`, `commit_preedit`,
`selection_get`; `resize(cols, rows)` + `set_cell_metrics(w_px, h_px)`;
`connect`/`disconnect`/`send_file`/`run_script`; `settings_get/set` over one typed key
space; prompt and dialog results.

**Never crosses:** `HWND`, `QWidget`, `HDC`, fonts, glyphs, or pixels beyond
`cell_w`/`cell_h`. The core knows pixel dimensions only for pixel-mode mouse reporting
and window-size escape sequences. `vtterm.c` already proves this holds.

### Why this stack

- **`russh` deletes `ttssh2/` — 62,596 LOC — plus the LibreSSL 4.3.2 fetch-and-build in
  `libs/*.cmake`.** Largest single deletion available anywhere in the project.
- **`tokio` dissolves the `WSAAsyncSelect` problem.** Message-pump-driven sockets are the
  one piece of *architectural* (not API) Win32 coupling, spanning 11 files including
  `teraterm/teraterm/ttwsk.c`, `ttssh2/ttxssh/fwd.c` and `TTProxy/ProxyWSockHook.h`. They
  don't port; they must be redesigned. An async runtime is that redesign, for free.
- **Qt 6 for the shell because CJK IME on Linux decides it.** `QInputMethodEvent` /
  `inputMethodQuery` is the most-tested IME path on Linux with first-class ibus and
  fcitx5 contexts; `QTextLayout`/`QRawFont` sit on HarfBuzz. Ghostty chose GTK4
  specifically to inherit `GtkIMContext` — the strongest available signal that
  hand-rolling IME is a trap. Qt is also good on Windows, which GTK4 is not.
  `uic` + `.ui` files are the natural target for 76 `DIALOG` templates, and `QPrinter`
  makes `teraprn` nearly free later.
- **No GPU renderer.** At 115200 baud you receive 11.5 KB/s. A `QPainter` glyph atlas with
  per-run caching draws a 200×60 grid in well under a millisecond. Keep `QRhiWidget` as a
  documented escape hatch behind the render interface; ship without it.

### The leverage point: one settings schema

`teraterm/common/tttypes.h` is a **909-line `TTTSet` struct**, surfaced by ~13.8k LOC of
dialog code and 76 `DIALOG` templates across 30 `.rc` files. **Do not hand-port these.**

Define one declarative schema (key, type, INI section+name, default, range, `.lng` label
key, help anchor) and generate from it: the Rust `Settings` struct and INI reader/writer,
the Qt dialog pages plus a search box over all settings, the TTL `setsetting`/`getsetting`
and Lua accessors, and the docs table. That turns ~14k LOC of hand-written dialogs into a
schema file plus ~1.5–2k of codegen. **This is the difference between the project
finishing and not** — build it in Stage 2 while morale is high, not Stage 3 when it hurts.

---

## Disposition of the existing tree

| Asset | LOC | Disposition |
|---|---:|---|
| `teraterm/ttpfile/*.c` protocols | 9,777 | **Vendor as C**, call via FFI behind `TFileIO` |
| 49 `.map`/`.tbl` charset tables | data | **Vendor verbatim** — they encode exact round-trip behavior `encoding_rs` doesn't reproduce |
| 14 `.lng` files (`installer/release/lang_utf8/`) | 17,610 | **Vendor verbatim, keep the format** |
| `vtterm.c` + `buffer.c` | 12,082 | **Port logic to Rust** (~14–16k). Reused as *specification* and as a differential-test oracle |
| `ttssh2/` | 62,596 | **Delete** → `russh` |
| `vtdisp.c` + `vtwin.cpp` + dialogs + `.rc` | ~28,000 | **Delete** → Qt + generated dialogs |
| `teraterm/ttpmacro/` | 16,472 | **Port to Rust** (~9–10k) |
| `TTProxy/` | 8,314 | **Delete**, reimplement in core (~1k Rust) |
| `ttptek`, `ttpmenu`, `susie_plugin`, `cygwin/` | ~11,000 | **Drop** |

Net: ~10k LOC of C carried forward, ~30k used as executable specification, ~115k deleted.

---

## Stage 0 — Repo bootstrap + de-risking spikes (3–4 weeks)

**0. Bootstrap `qtterm` (day 1).** Set the git identity locally *before the first commit* —
`git -C ~/agents-home/Projects/qtterm config --local user.email
93059500+nataloko@users.noreply.github.com` and `user.name nataloko`, matching `tine` and
`RATS`. Then lay down the workspace:

```
qtterm/
  Cargo.toml            # workspace: core, ffi, xtask
  crates/
    tt-core/            # vt · grid · charset · conn · config · i18n
    tt-xfer/            # build.rs (cc crate) → vendored C + windows.h shim
    tt-script/          # TTL + mlua (Stage 2)
    tt-ffi/             # cdylib, cbindgen → include/qtterm.h
  shell/                # Qt 6 C++: CMakeLists.txt, grid widget, .ui dialogs
  vendor/teraterm/      # ttpfile/*.c, 49 .map/.tbl, 14 .lng — verbatim + ATTRIBUTION.md
  oracle/               # spike 1: vtterm.c + buffer.c + stub vtdisp.c
  xtask/                # one entry point over cargo + cmake
  .github/workflows/    # linux-x64, windows-x64 (from tine's release.yml)
```

Also: `LICENSE`, a `README` stating plainly that this is a *compatible reimplementation*,
not Tera Term, and `.gitignore` for `target/` + `build/`.

**Licensing check before vendoring anything.** All 8 files in `teraterm/ttpfile/` carry an
inline 3-clause BSD header (`Copyright (C) 1994-1998 T. Teranishi` / `(C) 2007- TeraTerm
Project`), which permits vendoring provided the headers stay intact and the notice is
reproduced — do that in `ATTRIBUTION.md`. Note the repo's own `LICENSE.md` contains no
license text, only a link to `teratermproject.github.io/manual/5/en/about/copyright.html`;
read that page and confirm the `.lng` and `.map`/`.tbl` assets carry the same terms before
copying them, since those have no per-file headers. **Also confirm Qt's LGPLv3 dynamic-link
posture is compatible with whatever licence you pick for `qtterm` itself.**

Then five spikes, each able to kill or redirect the plan. Do them **before** product code.

1. **Headless C oracle — the highest-value week in the plan.** Link
   `teraterm/teraterm/vtterm.c` + `buffer.c` against a stub `vtdisp.c` implementing the 75
   exports as no-ops plus a grid recorder. Verified feasible: `vtterm.c` has zero Win32
   tokens, `buffer.c` has one. This yields **ground-truth "Tera Term behavior" on every
   commit, for the entire life of the project.** No other Win32 rewrite gets this.
2. **`ttpfile` on Linux.** Write the `windows.h` shim (note
   `teraterm/ttpfile/filesys_io.h` also needs `sys/utime.h` → `utime.h`, `struct _utimbuf`
   → `utimbuf`, `struct _stati64` → `stat64`). Compile all six protocols, run zmodem
   against `lrzsz` over a socketpair. Confirms 9,777 LOC of reuse.
3. **Qt IME reality check.** A bare `QWidget` accepting preedit, positioning the candidate
   window, handling commit — tested against **ibus+mozc and fcitx5+mozc, on both Wayland
   and X11**. Learn in week 3, not month 15.
4. **`serialport-rs` audit.** Break signalling, RTS/DTR, modem-line status, hotplug
   enumeration, on both OSes. Upstream carries a `CH340G_hw_flowctrl` branch — budget a
   thin platform-specific serial layer as *likely*, not contingent.
5. **`russh` compatibility sweep.** Point it at old network gear, Dropbear, legacy
   OpenSSH. Users are network engineers talking to crusty embedded servers; if russh
   refuses their KEX the product is useless. Keep the SSH client behind a trait so
   `libssh2` remains a fallback.

Also: **decide the Qt license posture now** (LGPLv3 forces dynamic linking; static needs
commercial or GPL), and stand up CI — copy the matrix from
`/home/nata/agents-home/Projects/tine/.github/workflows/release.yml`, keeping the
linux-x64 and windows-x64 lanes and dropping macOS/Flatpak.

## Stage 1 — The Linux serial + SSH terminal (3–4 months, ~25–30k LOC)

Must be shippable and genuinely useful, not a demo.

- `tt-vt` + `tt-grid`: VT100/220 + core xterm, SGR/256/truecolor, scrollback, selection,
  BCE, wide + combining chars. Ported from `vtterm.c`/`buffer.c` **against the oracle**.
- `tt-conn`: **serial first** (the differentiator), then SSH2 via `russh`, then telnet,
  then local PTY via `portable-pty`.
- Qt shell: one window, grid painter, IME, clipboard, font/colour config, connect dialog,
  serial-port picker with live enumeration.
- **`~/.ssh/config`, `~/.ssh/known_hosts`, `~/.ssh/id_*` support** — Tera Term does not
  have this and it is a major Linux adoption lever.
- Session logging (timestamped, rotation).
- rpm + AppImage. Fedora first.

**Done when:** you delete the Wine shortcut and daily-drive it for serial console work.

Deliberately absent: file transfer, macros, tabs, Windows build, most settings.

## Stage 2 — The differentiators (3–4 months, ~20k LOC)

- **File transfer**: FFI to the vendored C, all six protocols, interop-tested against
  `lrzsz` and `gkermit`.
- **TTL interpreter**: native Rust, **in-process on a thread** — this deletes ~2,600 LOC
  of DDE glue across `teraterm/ttpmacro/ttmdde.c` and `teraterm/teraterm/ttdde.c` and an
  entire class of race conditions. Target: the 53 `.ttl` scripts in `tests/` pass.
- **Lua via `mlua` over the same `ScriptHost` command table** (~500 LOC of extra glue).
- `ttctl` JSON-RPC control socket replacing DDE. Keep a `ttpmacro script.ttl` CLI entry
  point so existing shortcuts and `.bat` wrappers keep working.
- **Settings schema + generated dialogs**, first pass.
- `TERATERM.INI` and `KEYBOARD.CNF` readers.

## Stage 3 — Windows parity (3–4 months, ~15k LOC)

Windows build, ConPTY, Win32 serial edge cases, NSIS installer. All 14 `.lng` languages
wired through unchanged. VT320/VT525 depth and DEC private modes (the long tail
`vtterm.c` covers). Tabs and sessions, session duplication as an in-process concept rather
than `CreateFileMapping`. Built-in HTTP/SOCKS proxy replacing `TTProxy`. Printing.

## Stage 4 — Depth and polish (4–6 months)

CJK completeness (DEC special graphics, ambiguous-width policy, the `unicodebuf-*`
corpus), macro reference docs, Lua plugin API, sixel, self-updater, deb.

**Realistic total to a credible Tera Term replacement: 15–20 months solo with AI
assistance.** Full parity is 3+ years and should be explicitly renounced in the README.

---

## Dropped permanently — say so in the README

| Thing | LOC | Why |
|---|---:|---|
| Tek 4010 (`ttptek` + `tekwin.cpp`) | ~2,900 | No one has a storage-tube workflow in 2026 |
| **TTX C plugin ABI** | — | `teraterm/common/ttplugin.h` hooks are literal Winsock (`Pconnect`, `PWSAAsyncSelect`) and Win32 file-API function tables plus raw `HMENU`. Unportable by construction |
| Susie image plugins | 957 | A 1996 Win32 codec DLL ABI |
| DDE | 2,600 | → `ttctl` JSON-RPC; strictly better and cross-platform |
| SSH1 | — | Broken by design since 1998 |
| `ttpmenu.exe` | 4,831 | It's a launcher; the desktop has one |
| `cygterm` | 2,200 | Superseded by `portable-pty` (ConPTY / forkpty) |
| Win7 jump lists (`winjump.c`) | 810 | Windows-only chrome |
| `ttpcmn` shared-memory IPC | 2,865 | Single-process design removes the need |

**Kept but never rewritten:** B-Plus and Quick-VAN. Tera Term is essentially the last
implementation on earth — there is no counterparty to test against and nothing to learn
from rewriting them. Vendor the C, mark them best-effort.

---

## Compatibility and migration

Adoption hinges on "my existing setup just works." Budget real time here.

- **`TERATERM.INI`** — read *and write* natively, bug-compatible with `GetPrivateProfile*`
  (duplicate-key semantics, no quote stripping, CRLF, encoding fallback). ~600 LOC
  hand-rolled; **do not use a generic INI crate**. Put new settings in an additive section
  so round-tripping with real Tera Term survives.
- **`KEYBOARD.CNF`** — it's an INI. Read as-is, 1–2 days.
- **Hosts and keys** — read Tera Term's `ssh_known_hosts` *and* `~/.ssh/known_hosts`;
  read `~/.ssh/id_*` and `~/.ssh/config`; write OpenSSH format.
- **`.lng` files** — keep the exact format. Do **not** migrate to Qt `.ts`: that throws
  away 17,610 lines of donated translation (14 languages × ~1,150 keys, verified) and the
  translator workflow. A plain lookup or thin `QTranslator` subclass suffices.
- **TTX plugins** — replace in this order: (1) fold the ones that matter into core —
  `TTXProxy` (~1k Rust), `TTXKanjiMenu`, `TTXResizeMenu`, `TTXttyrec`; (2) a **Lua plugin
  API** — menu items, key bindings, connect/disconnect hooks, byte-stream filters,
  settings pages, which covers what the 17 samples in `TTXSamples/` actually do; (3) WASM
  component plugins only if someone asks. Don't build speculatively.
- **Docs** — 751 HTML files / 97k lines, 214 of them macro reference. Convert to Markdown
  mechanically, serve as a static site, and **generate** the settings and macro references
  from the schema and command table.

### TTL: reimplement, don't shim or transpile

TTL is BASIC-shaped — `:labels` with `goto`, one-line `if…then`, an untyped-ish variable
model, 1-based string indexing, and `wait`/`pause` with timeout semantics stateful against
the connection. You cannot shim `goto` into Lua honestly, and the moment a real `.ttl`
fails you've lost the only reason to care about TTL. Transpiling means incomprehensible
errors and owning a source-to-source compiler forever.

The 232 reserved words in `teraterm/ttpmacro/ttmparse.h` sound worse than they are:
~42 are keywords and operators (the actual language, ~40 grammar productions); the other
~190 are library commands of 5–30 lines each, mapping 1:1 onto core API calls. `ttl.cpp`
is large mostly because it's one giant C dispatch switch. Sizing: lexer/parser/AST 1.5k,
interpreter 1.5k, string/int/array builtins 1.5k, file/dir 1k, connection/terminal 2k,
dialogs 0.8k, misc (time, crypto, regex) 1k — **~9.3k Rust vs 16.5k C**.

---

## Verification

The existing project has **zero** unit tests. VT emulation is spec-heavy and
correctness-critical. Six layers, in priority order:

1. **Differential testing against the real Tera Term.** The Stage 0 oracle
   (`vtterm.c` + `buffer.c` + stub `vtdisp.c`). Feed both engines identical byte streams,
   dump grids as text, diff on every commit. **Build this first.**
2. **esctest2** (iTerm2) — ~1000 automated DEC/xterm conformance assertions driven over a
   pty and read back via DSR/DECRQSS. Wire into CI in Stage 1.
3. **vttest** (Dickey) — interactive, so use as a manual gate plus screenshot diffing for
   menus 1–11 at each stage boundary.
4. **The existing corpus, automated from day one.** The 33 `.sh`/`.pl`/`.rb` exercisers in
   `tests/` become golden-file tests — `unicodebuf-combining*.pl`,
   `unicodebuf-east_asian_width.txt`, `bcetest.sh`, `decfra.sh`,
   `#38168-deccara-*.sh` are exactly the CJK and DEC cases that will break. The 53 `.ttl`
   files become the TTL conformance suite.
5. **Fuzzing and property tests.** `cargo-fuzz` on the parser — it eats untrusted network
   bytes. `proptest` invariants on the grid: cursor in bounds, wide-char pairs never split,
   scrollback monotonic, no attribute leaks across BCE.
6. **Protocol interop over a socketpair/pty**: `sz`/`rz` (lrzsz) for x/y/zmodem,
   `gkermit`/`ckermit` for kermit — all present on Fedora.

**Plus a perf gate from Stage 1**, calibrated the way
`/home/nata/agents-home/Projects/tine/docs/BENCH.md` describes: cold start (ms), idle RSS,
time to render 10 MB of `cat`, input-to-present latency. Publish the numbers in the README
— "simple, light, performant" is a claim, so make CI enforce it.

---

## Risks, ranked by how likely they are to kill the project

1. **Scope. This is the one that kills it.** The failure mode is 18 months producing a
   terminal 90% as good as three existing ones and 40% as good as Tera Term. Stage 1 must
   be narrow and must beat everything else on Linux at exactly one thing: **GUI serial
   console work with real scripting.** If Stage 1 slips past 5 months, cut features, not
   the ship date.
2. **Motivation cliff at the dialogs.** 76 dialogs arrive right after the fun part ends.
   The settings-schema codegen is the mitigation and must exist before it's needed.
3. **IME/CJK.** Qt is the best available answer, not a guarantee — fcitx5 on Wayland still
   has edge cases. Mitigated by Stage 0 spike 3.
4. **`serialport-rs` gaps** — break signalling, modem lines, hotplug, vendor-specific flow
   control. Assume a platform-specific serial layer is needed.
5. **`russh` maturity** against old gear. Keep SSH behind a trait; `libssh2` fallback.
6. **Three build systems** (Cargo, CMake/Qt, vendored C). Mitigate: the `cc` crate
   compiles the C from Cargo, CMake touches only Qt, one `cargo-xtask` on top.
7. **Qt licensing** — decide in Stage 0, not Stage 4.

**"Why not just use Wine?"** — concede the strong form: for one user it's the rational
zero-effort answer and works acceptably for telnet and SSH today. But it fails at
precisely the differentiator: Wine's serial passthrough has no reliable `WaitCommEvent`,
unreliable modem-line status, poor break signalling and no USB-serial hotplug
propagation. Layer fcitx5 and Wayland HiDPI on top and it degrades further. Wine is fine
for the parts you don't need and broken for the part you do.

**Fresh project, not a fork or an upstream port.** A fork means carrying 157k LOC you've
decided to delete. Vendor `ttpfile/`, the 49 charset tables, the 14 `.lng` files and the
TTL grammar with loud attribution; offer portability fixes back upstream. Name it
something else — "compatible with Tera Term," not "Tera Term."

**Adopt, don't build:** `vte`, `portable-pty`, `russh`, `serialport-rs`, `mlua`, Qt 6,
and tine's CI/packaging pipeline.

**Read, don't fork:** `alacritty_terminal` and `wezterm-term`/`termwiz` encode *their*
terminals' behavior, not Tera Term's VT320/VT525 and CJK depth — which is the thing being
preserved. **Watch `libghostty`**: it is explicitly trying to become a reusable terminal
core with a C ABI, and if it stabilizes before Stage 3 it could replace `tt-vt` + `tt-grid`
outright. Keep that seam clean enough to find out.

---

## Critical files

Reference material, all under `/home/nata/agents-home/Projects/teraterm/`:

- `teraterm/teraterm/vtterm.c` — 5,939 LOC emulation state machine; zero Win32 tokens.
  Port target **and** differential-test oracle.
- `teraterm/teraterm/vtdisp.h` — the renderer contract. 75 exports, only `DispStrA`/
  `DispStrW` draw. Defines where the core/frontend seam goes.
- `teraterm/teraterm/buffer.c` — 6,143 LOC grid/scrollback semantics; one Win32 token.
- `teraterm/ttpfile/filesys_io.h` — the `TFileIO` vtable, the one real interface seam;
  the FFI boundary for the vendored protocol C. Its single impl is `filesys_win32.cpp`.
- `teraterm/common/tttypes.h` — the 909-line `TTTSet`; source material for the generated
  settings schema replacing ~13.8k LOC of dialogs.
- `teraterm/ttpmacro/ttmparse.h` — TTL grammar and the 232 reserved words; specification
  for the Rust interpreter.
- `teraterm/common/ttplugin.h` — proof the TTX ABI is unportable, justifying Lua.
- `tests/` — 53 `.ttl` scripts + 33 escape-sequence exercisers; the day-one test corpus.
