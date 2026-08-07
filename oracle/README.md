# termitta oracle

Builds Tera Term's **real** VT engine headless on Linux, so the Rust
reimplementation can be diffed against ground truth on every commit.

```
printf 'Hi\033[3;5HThere' | ./build/oracle --cols 20 --rows 4
```
```
# termitta-oracle 1
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
make test     # run the regression suite (18 cases)
./run_tests.sh --bless   # regenerate goldens after an intentional change
```

Needs `gcc` and Python 3.11+ (for the two generator scripts). Nothing else.

## What gets compiled

**15,325 lines of Tera Term, unmodified** — no `#ifdef` sprinkling, no forked
copies:

| Source | Lines | Why |
|---|---:|---|
| `teraterm/vtterm.c` | 5,939 | The escape-sequence state machine |
| `teraterm/buffer.c` | 6,143 | Grid, scrollback, attributes, wide/combining |
| `teraterm/charset.cpp` | 1,082 | ISO-2022 designation and invocation |
| `teraterm/unicode.cpp` | 979 | East-asian width, combining, emoji, virama tables |
| `common/ttlib_charset.cpp`, `teraterm/checkeol.cpp`, `common/asprintf.cpp`, `common/tttypes_termid.cpp`, `common/makeoutputstring.cpp` | 1,182 | Support |

Adding a real Tera Term source to `TT_CXX` in the Makefile is **always**
preferable to reimplementing its behaviour in a stub. Every stub is a place the
oracle can lie.

## Layout

```
winshim/     A <windows.h> that is types-and-three-functions, not an emulation.
             Plus MSVC Secure CRT (strncpy_s, _snprintf_s_l, ...) and swscanf_s.
src/
  main.c              runner + the dump format
  stubs_manual.c      symbols whose behaviour the grid observes -- REAL logic
  stubs_generated.c   no-op stubs, generated; do not edit
  codeconv_min.c      the eight codeconv entry points that are actually needed
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
