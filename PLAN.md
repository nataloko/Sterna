# Sterna — plan and status

Canonical roadmap and live status. Update the status markers as work lands;
this file is the thing a fresh session should read first, together with
`AGENTS.md`. The stage-by-stage build narrative that used to fill it — every
landing, finding and measurement from stages 0–4 — moved verbatim to
**`docs/history.md`** on 2026-08-15; read that for the story behind a
decision, not to start a session.

**Last updated:** 2026-08-16 · **Stage:** 4 complete, deliberate deviations
landing (`docs/deviations.md`) · **Commits:** 857

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
  (see `docs/history.md`'s Stage 1). So ~30 MB of Qt rides in the Windows installer and a
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

All four stages are complete; `docs/history.md` holds the full narrative —
what landed when, what each landing found, and the measurements. The
summaries below are for orientation.

- **Stage 0 — bootstrap + de-risking. Complete 2026-08-07.** Tera Term's VT
  engine runs headless as the differential oracle (`oracle/`, 15,325 lines
  compiled unmodified) and the `ttpfile` protocols interoperate on Linux
  (`xfer/`, 10/10 against `lrzsz`/`gkermit`). `serialport-rs` was audited on
  real hardware (adopt, plus a thin patch layer) and `russh` against real
  servers. CJK deferred indefinitely.
- **Stage 1 — the Linux serial + SSH terminal. Deliverables done
  2026-08-08.** All four transports, the VT engine ported against the oracle,
  the C ABI, the Qt shell, `~/.ssh/*` integration, session logging, the
  AppImage, the perf gate. The stage's own "done when" — the Wine shortcut
  deleted and Sterna daily-driven for serial console work — is the user's
  call, not a checklist item; nothing else remains.
- **Stage 2 — the differentiators. Complete 2026-08-10.** File transfer over
  the vendored C; the whole TTL language (231 reserved words, upstream's 53
  scripts against reviewed transcripts) and Lua over the same `ScriptHost`;
  `ttctl` and `ttpmacro`; both command-line parsers; the settings schema —
  296 settings over all 272 `ttset.c` keys, generated end to end.
- **Stage 3 — Windows parity. Complete 2026-08-12.** The whole Rust workspace
  green on native Windows, ConPTY, Win32 serial, the NSIS installer, all 14
  `.lng` languages, the colour OSCs, XTWINOPS, printing, the proxy, tabs and
  session duplication.
- **Stage 4 — depth and polish. Complete 2026-08-12.** The Lua plugin
  surface, sixel, the signed self-updater with its startup check, the
  generated macro reference; settings navigation and optional persistence
  followed 2026-08-14.

**Full parity stays explicitly renounced** — it is 3+ years for ground other
programs already cover, and the README says so.

### Open items

The live remainder, collected here so it cannot hide in the narrative:

- **File the upstream bug reports** — needs a GitHub account (the user). The
  five proved ones are drafted in `docs/upstream-bugs.md`, memory-safety
  first; demonstrate the found-by-reading list against a real
  `ttpmacro.exe`/Tera Term before filing those.
- **Whether a Windows serial open can block its caller indefinitely** needs a
  real COM port (`crates/tt-conn/tests/serial_windows.rs`); Wine faults
  instead of answering.
- **`tt-ttl`'s `set_dir` canonicalises into `cur_dir`**, so `getdir` can
  report a `\\?\` verbatim path on Windows — deliberately not fixed on a
  hunch; the Windows TTL gate decides whether it moves a golden.
- **Five TTL transcripts are platform-shaped and recorded from Wine**
  (`#31050`, `#31971`, `#39452`, `getspecialfolder`, `spfolder`); a native
  Windows run is the authority for them.
- **`ini-audit`'s two divergences are Wine's alone** (line-ending rewrite on
  write, `[ s ]` → `[s]`) — re-run the battery on native Windows;
  `exercise.exe` compiles there.
- **Serial auto-reconnect**: the five keys are carried and not yet run; the
  Linux half is a udev monitor this port has not built.
- **`sendfile` and the two serial send delays** stay refused in the macro
  host pending a `SendMem`-shaped send queue with its four callers;
  `crates/tt-macro/src/host.rs` keeps the list.
- **The text session log lacks the tapped `HT`, `BS` and wrap line break**
  the macro tap has — a divergence rather than a choice; it is `LogOptions`'
  neighbourhood when somebody gets to it.
- **`PrnFont`** is the one printer key not in the schema — `ReadFont` packs a
  name, two sizes and a charset into one value the generator has no type for.
- **The Windows installer is unsigned** until there is a legal entity for a
  certificate; `osslsigncode` signs on Linux when one exists.
- **`.ttl` opened from Explorer starts a new window** where upstream runs the
  macro in the session already on screen; a `ttpmacro` fallback that starts a
  window changes what a `.bat` wrapper does today, so it wants deciding
  rather than assuming.

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

**`docs/deviations.md` is the canonical list** — twenty-one entries as of
2026-08-16, each with its reason, what stays compatible, and where it lives.
The write-ups that used to sit here — the first batch of eight, Find, line
numbers and the counters — are in `docs/history.md`.

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
   a pty, in CI as of 2026-08-08 — **381 pass** as of 2026-08-15; every one of
   the other 187 has a
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
   `oracle/upstream.cases`. The plan's prediction was accurate — `bcetest.sh`,
   `decfra.sh` and the `#38168-deccara-*.sh` trio were among the breakages, and
   three of them turned out to be upstream bugs rather than ours. Of the two
   still divergent, one is `vte` following ECMA-48 where Tera Term's CSI parser
   does not (see `docs/history.md`) and the other is spacing combining marks, deferred with
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

   The finding, which is about the shell rather than the benchmark, is
   recorded with the measurements.

The measurements themselves — the shell and engine numbers behind the gate,
the Qt 6 Widgets baseline, and the findings each produced — are in
`docs/history.md`'s Measurements section; `bench/baseline.json` is the live
local baseline.

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
   a replacement. See the spike 4 result in `docs/history.md`. Still open: Windows, and the
   `CH340G_hw_flowctrl` case, which needs a CH340 adapter.
5. **Three build systems** (Cargo, CMake/Qt, vendored C). Mitigate: the `cc`
   crate compiles the C from Cargo, CMake touches only Qt, one `cargo-xtask` on
   top.
6. **Qt version skew in development.** The agent container is Ubuntu 24.04 with
   Qt 6.4.2; the desktop runs 6.11.1. Windowing works from the Ubuntu container,
   which makes it tempting to trust it for everything — don't. **This has already
   produced one false finding and one set of flattering-by-2x numbers**, both
   caught only by re-measuring; see `docs/history.md`'s Measurements. Mitigation exists: the
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
