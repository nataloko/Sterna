# Sterna oracle

Builds Tera Term's **real** VT engine headless on Linux, so the Rust
reimplementation can be diffed against ground truth on every commit.

```
printf 'Hi\033[3;5HThere' | ./build/oracle --cols 20 --rows 4
```
```
# sterna-oracle 1
# term vt100 20x4
# cursor 9,2
  0 |Hi                  |
  1 |                    |
  2 |    There           |
  3 |                    |
```

## Why this exists

Rewriting a VT emulator is a correctness problem, not a coding problem. Tera
Term encodes 30 years of accumulated behaviour — VT100/220/320/525, DEC private
modes, east-asian width policy — most of it unwritten anywhere but the source.
A conformance suite tells you whether you match *the spec*. This tells you
whether you match *Tera Term*.

It works because `vtterm.c` (5,939 lines, the whole escape-sequence state
machine) contains **zero** `HWND`/`HDC`/`windows.h` tokens and makes **zero**
drawing calls. The engine was already separable; nobody had separated it.

## Build and test

```sh
make          # build build/oracle
make test     # run the regression suite (72 cases)
./run_tests.sh --bless   # regenerate goldens after an intentional change
```

Needs `gcc` and Python 3.11+ (for the two generator scripts). Nothing else.

## What gets compiled

**16,976 lines of Tera Term**, all but five of them unmodified — no `#ifdef`
sprinkling, no forked copies:

| Source | Lines | Why |
|---|---:|---|
| `teraterm/vtterm.c` | 5,939 | The escape-sequence state machine |
| `teraterm/buffer.c` | 6,143 | Grid, scrollback, attributes, wide/combining |
| `teraterm/keyboard.c` | 1,651 | The key table — bytes *out*, via `src/keys.c` |
| `teraterm/charset.cpp` | 1,082 | ISO-2022 designation and invocation |
| `teraterm/unicode.cpp` | 979 | East-asian width, combining, emoji, virama tables |
| `common/ttlib_charset.cpp`, `teraterm/checkeol.cpp`, `common/asprintf.cpp`, `common/tttypes_termid.cpp`, `common/makeoutputstring.cpp` | 1,182 | Support |

`vtterm.c` and `buffer.c` are compiled from patched *copies* under
`build/patched/` — five one-line fixes for defects drafted in
`docs/upstream-bugs.md`, and nothing else. Tera Term's tree is never modified.

Adding a real Tera Term source to `TT_CXX` in the Makefile is **always**
preferable to reimplementing its behaviour in a stub. Every stub is a place the
oracle can lie, and this README has three worked examples of it doing so.

## Layout

```
../winshim/  The portability layer, shared with xfer/ and crates/tt-xfer/:
             a <windows.h> that is types-and-three-functions rather than an
             emulation, MSVC's Secure CRT, and codeconv_min.c.
src/
  main.c              runner + the dump format
  stubs_manual.c      symbols whose behaviour the grid observes -- REAL logic
  stubs_generated.c   no-op stubs, generated; do not edit
patches/     Local fixes to Tera Term, applied to a COPY under build/patched/.
cases/       Regression cases: input + golden dump.
```

Two generators keep the boring parts honest:

- `gen_stubs.py` reads prototypes out of Tera Term's own headers, so stub
  signatures cannot drift from what `vtterm.c` was compiled against. It reads
  `build/stubs_manual.o` to learn what is already hand-written, so the two stub
  layers cannot collide.
- `apply_patches.py` applies local fixes as exact string replacements that must
  match **exactly once**, and fails loudly otherwise.

## Determinism

A test oracle that depends on wall time is not an oracle.

- `GetTickCount`/`Sleep` are backed by a virtual clock, frozen by default.
  `vtterm.c` uses them for bell throttling.
- The bell is off (`IdBeepOff`).
- There is no INI file, so `GetPrivateProfileIntW` returns every default —
  which is exactly the configuration we want to compare against.

## Settings

`main.c:settings_defaults()` mirrors `ttpset/ttset.c`'s per-key fallbacks for
the 67 `ts.*` fields `vtterm.c` and `buffer.c` read. **These are load-bearing.**
`CRReceive` alone shifts every row in the dump, and its real default is `IdCR`
(the `else` branch at `ttset.c:643`), not the `IdCRLF` you would guess from
reading the surrounding code. With `IdCR`, a bare CR is a carriage return, so
`"Hello, world!\rSecond line"` correctly yields `Second lined!`.

## Injected input events

A screen dump has no mouse, so mouse reporting would be the one part of the VT
engine with no differential coverage. Instead of hand-checking it, a case can
put directives **in the byte stream**, wrapped in an APC string the runner
strips before the terminal sees it:

```
ESC _ tt.mouse <down|up|move|wheel|stat> <button> <x> <y> ESC \
ESC _ tt.mods  [shift] [ctrl] [alt]                       ESC \
ESC _ tt.focus <in|out>                                   ESC \
ESC _ tt.key   <name>                                     ESC \
```

Bytes on either side are fed and fully parsed first, so a directive sees exactly
the terminal state the preceding bytes produced — modes can be changed between
clicks. Anything after `ESC _` that does not start `tt.` is passed straight
through.

`x` and `y` are **window pixels**, not cells: that is what `MouseReport` takes,
and SGR-pixel mode (`DECSET 1016`) reports them back unconverted. The cell is a
nominal 8x16 (`ORACLE_CELL_W`/`_H` in `oracle.h`), so cell (3,5) is `24,80`.
Buttons are upstream's numbering — 0 left, 1 middle, 2 right, 3 release — and
for a wheel event 0 is up and 1 is down.

```sh
printf '\033[?1000h\033[?1006h\033_tt.mouse down 0 24 80\033\\' | oracle --cols 10 --rows 8
# reply <ESC>[<0;4;6M
```

### `tt.key` — the real key table

`tt.key` runs Tera Term's own `keyboard.c:GetKeyStr()` and puts the result in
the reply stream, under whatever modes the preceding bytes left the terminal
in. So `CSI ? 1 h` then `tt.key up` is the application-cursor form, and the
Rust engine has to agree.

`keyboard.c` is compiled whole, in `src/keys.c` — which `#include`s it, and
that is deliberate. The table lives in `GetKeyStr()`, which is `static`, and it
is 74 key cases each spelling out its sequence three times over for
application-cursor, application-keypad and 8-bit-controls mode. Including the
translation unit reaches it without editing upstream (ground rule 1) and
without retyping 200-odd escape sequences into a stub (ground rule 2).
`keyboard.c` must therefore **not** also appear in the Makefile's `TT_C`.

Driving the public `KeyCodeSend()` instead would have been the obvious move and
is worse: it routes through `SendMemBinary`/`SendMemStart`, the delayed-send
queue, so it drags an async subsystem and its stubs into an answer that is
decided before any of that.

Compiling it turned up two more stand-ins that had quietly been wrong:

- **`keyboard.c` owns `AutoRepeatMode`, `AppliKeyMode`, `AppliCursorMode`,
  `AppliEscapeMode` and `Send8BitMode`**, and `stubs_manual.c` had been
  defining them. So `vtterm.c` set a mode and the real key table would never
  have seen it. `AppliEscapeMode` was declared `BOOL` here against upstream's
  `int`, too — a type mismatch that links silently in C.
- **`ShiftKey`/`ControlKey`/`AltKey` are upstream's**, resting on
  `GetAsyncKeyState`. The oracle now sets a key-state array and lets
  upstream's own definitions run, `MetaKey`'s left/right variants included,
  instead of substituting three booleans.

Two things had to be fixed before mouse injection worked, both of them the
"stubs lie" failure mode:

- `ShiftKey`/`ControlKey`/`AltKey` are **functions** in `keyboard.h`, and were
  defined here as `BOOL` variables. That links, and jumps into the data section
  the first time anything calls one. Nothing ever did, because no headless run
  reached the mouse path.
- `DispConvWinToScreen`/`DispConvScreenToWin` were empty generated stubs that
  never stored through their out-parameters, so `MouseReport` read an
  uninitialised position off the stack. Both now follow `vtdisp.c`.

## Bug found in Tera Term

`BuffGetAnyLineDataW()` (`buffer.c:5832`) advances its cell pointer `b` only on
the non-padding path. On reaching the padding cell that follows a full-width
character it does `continue` **without advancing `b`**, so it parks there and
silently drops the rest of the line.

The only caller is `filesys_log.cpp:443` — so **Tera Term's session logging
truncates any line at its first CJK character.** Real data loss, in a terminal
whose CJK support is a headline feature.

```
printf 'ASCII \344\275\240\345\245\275 world' | oracle --cols 30 --rows 2
  before:  |ASCII 你                      |
  after:   |ASCII 你好 world              |
```

The one-line fix is `patches/0001-buffgetanylinedataw-padding.patch`, applied to
the build copy so the oracle reports true screen contents. **Not yet reported
upstream** — it should be.

## Known limitations

- **Legacy CJK codepages are not supported.** `UTF32ToMBCP` and friends need
  Tera Term's vendored `.map`/`.tbl` tables; the oracle runs UTF-8 only and
  warns once, degrading unmappable cells to `'?'` exactly as Tera Term does.
  Anything reached that genuinely cannot be faked (`CP932ToUTF32`, `ToWcharA`)
  aborts rather than returning quietly wrong data.
- **Scrollback is not dumped**, only the visible page.
- **Input is one-shot.** Feeding bytes in timed chunks (to exercise split
  escape sequences across reads) needs a driver mode.
- `swscanf_s` implements only the `%s` and `%[^set]` subset that
  `unicode.cpp`'s config loader uses, and returns `EOF` on anything else.

## Attribution

Tera Term is © 1994-1998 T. Teranishi and © the TeraTerm Project, 3-clause BSD.
The sources here are compiled from a sibling checkout (`../../teraterm`) and are
not redistributed in this repository. See `../ATTRIBUTION.md`.
