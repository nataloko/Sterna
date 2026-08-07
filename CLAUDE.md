# Working notes for qtterm

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

## What this is

A cross-platform Tera Term successor: Rust core + flat C ABI + Qt 6 Widgets
shell, Linux and Windows. **Not** a fork of Tera Term and **not** aiming at
parity — see `PLAN.md` for scope.

`qtterm` is a working name; the real one is still undecided (the current one is
already taken in the wild).

## Ground rules

1. **`../teraterm` is read-only reference.** Never edit it. It is a sibling
   checkout of upstream Tera Term, used three ways: compiled unmodified as the
   test oracle, vendored for specific subsystems, and read as the behavioural
   specification. If a build needs a change to it, that change goes in
   `oracle/patches/` and is applied to a copy under `oracle/build/patched/`.
2. **Prefer compiling real Tera Term code over reimplementing it.** In the
   oracle, adding a source to `TT_CXX` in the Makefile always beats writing a
   stub. Every stub is a place the oracle can lie about ground truth.
3. **Never bless a golden you have not read.** `./run_tests.sh --bless`
   regenerates `cases/*/expected`. A wrong golden is worse than no test.
4. **The oracle's settings are load-bearing.** `main.c:settings_defaults()`
   mirrors `ttpset/ttset.c`'s per-key fallbacks. If a dump looks subtly wrong,
   suspect a setting before suspecting the parser. See the traps below.
5. **Attribution and licensing are not paperwork.** Before vendoring anything
   from Tera Term, check `ATTRIBUTION.md` — the `.lng` and `.map`/`.tbl` assets
   have no per-file licence headers, unlike `ttpfile`.
6. **Git identity is set per-repo** to the GitHub noreply address, already
   configured locally. Don't change it.

## Build and test

```sh
cd oracle
make            # build build/oracle
make test       # 18 regression cases
make stubs      # regenerate the stub layer after upstream headers change
```

Needs `gcc` and Python 3.11+. Nothing else — no Rust or Qt yet.

Rust, cmake, Qt 6, lrzsz and ckermit are installed in the dev container.
**Note the container is Ubuntu 24.04 while the target desktop is Fedora** —
package names and Qt versions differ, and anything needing a real desktop
session (IME) or hardware (serial) cannot be tested here at all.

## Traps

These cost real debugging time. Each is a place where the failure looks like
something other than what it is.

- **`UTF32ToUTF16` is not optional.** `buffer.c:234` uses it to fill
  `buff_char_t::wc2`, and `expand_wchar()` reads back from `wc2`, not `u32`.
  Stub it and you get a screen holding all the right codepoints that renders
  **entirely blank** — which looks exactly like a broken parser.
- **`_WideCharToMultiByte` is dereferenced with no NULL check**
  (`buffer.c:3076`), so a stub returning NULL segfaults on the first combining
  character.
- **`CRReceive`'s real default is `IdCR`** — the `else` branch at
  `ttset.c:643`, not the `IdCRLF` the surrounding code suggests. It shifts every
  row in the dump. With `IdCR` a bare CR is a carriage *return*, so
  `"Hello, world!\rSecond line"` correctly yields `Second lined!`.
- **`AcceptTitleChangeRequest` defaults to `overwrite`**, not off
  (`ttset.c:1568`). Zero means OSC title changes are silently ignored.
- **`buffer.c:134` hardcodes `CodePage = 932`** (Shift-JIS). Call
  `BuffSetDispCodePage()`.
- **`WinWidth`/`WinHeight` ≠ `NumOfColumns`/`NumOfLines`.** The first pair is
  the visible window in cells, the second is the terminal size, and only
  `BuffChangeTerminalSize` owns the latter. `DispChangeWinSize` must **not**
  call `BuffChangeWinSize` — that recurses infinitely against `buffer.c:4956`.
- **`BuffGetAnyLineDataW` takes an absolute buffer index**;
  `BuffGetCursorCharAttr` is screen-relative. `PageStart` maps between them.
- **`vtterm.c` owns `CharSetInit`** — it holds `charset_data`. The runner must
  not call it.
- **Make's VPATH beats pattern rules.** Patched sources need *explicit* rules or
  the generic `%.o: %.c` finds the unpatched original via VPATH and silently
  wins.

## Bug found upstream, not yet reported

`BuffGetAnyLineDataW()` (`buffer.c:5832`) does `continue` without advancing `b`
on padding cells, so it parks on the padding cell after a full-width character
and drops the rest of the line. Only caller is `filesys_log.cpp:443` — so
**Tera Term's session logging truncates any line at its first CJK character.**

One-line fix in `oracle/patches/0001-buffgetanylinedataw-padding.patch`.
Reporting it upstream is an open item in `PLAN.md`.

## Layout

```
PLAN.md          roadmap + status — read first
ATTRIBUTION.md   licensing, and what still needs clearing before vendoring
oracle/          Tera Term's real VT engine, headless on Linux (see its README)
crates/          Rust core — not started
shell/           Qt 6 shell — not started
vendor/          vendored Tera Term subsystems — empty, see ATTRIBUTION.md first
```
