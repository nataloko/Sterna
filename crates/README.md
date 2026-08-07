# termitta core

The Rust side. Four crates so far, of the eight `PLAN.md` describes.

| Crate | What it is |
|---|---|
| `tt-grid` | Cells, lines, cursor, scroll region, scrollback, alternate screen. No I/O, no escape sequences. |
| `tt-charset` | ISO-2022 designation and invocation, and whether a byte is DEC special graphics. |
| `tt-vt` | The escape-sequence state machine. Byte-level parsing is the `vte` crate; the semantics are ported from Tera Term. |
| `tt-dump` | A CLI that drives `tt-vt` over a byte stream and prints the oracle's dump format. Exists for the differential harness. |

```sh
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
../run_diff.sh                 # the gate that actually matters
```

`cargo` is on `PATH` only for login shells in the dev container — export
`$HOME/.cargo/bin` first.

## The differential harness

`../run_diff.sh` feeds every case in `../oracle/cases/` to **both** engines —
`tt-dump` and `oracle/build/oracle`, the latter being Tera Term's real
`vtterm.c` and `buffer.c` compiled headless — and diffs the two dumps against
each other.

There are no golden files in that loop. The oracle *is* the expected output, so
a new case is an `input` file and an optional `cmd` line, with nothing to bless
and no opportunity to enshrine a wrong answer. (`oracle/run_tests.sh` still
keeps goldens, for a different job: catching the *oracle* drifting when upstream
is bumped or the stub layer changes.)

```sh
../run_diff.sh              # all cases
../run_diff.sh 09 27        # just the ones whose names contain 09 or 27
../run_diff.sh -v 27        # and print both dumps
```

Adding a case:

```sh
mkdir -p ../oracle/cases/39-my-case
printf -- '--cols 20 --rows 4' > ../oracle/cases/39-my-case/cmd
printf 'whatever\033[2Jbytes' > ../oracle/cases/39-my-case/input
../run_diff.sh 39
```

If it fails, the diff tells you what Tera Term does and what we do. Tera Term
wins, every time — that is the whole point. To also add it to the oracle's own
golden suite, run `../oracle/run_tests.sh --bless` and **read the golden it
produces** before committing it.

A case directory holding an `xfail` file is a *known* divergence. The file says
why; the diff is shown but not counted as a failure; and if the two engines ever
agree the case reports `XPASS` and fails, so a stale marker cannot survive.

## Where this is faithful

Being a *port* rather than a fresh VT implementation means some upstream
behaviour looks like a bug until you check. Reproduced deliberately:

- **Erase keeps the current colours but drops bold/underline/reverse.**
  `buffer.c` passes `CurCharAttr.Fore/Back` with `AttrDefault` to `memsetW`.
- **DECSTBM homes the cursor to the screen origin**, not to the top of the
  region it just set, unless origin mode is on. `vtterm.c:2473`.
- **A line feed at the bottom of the scroll region does not clear the pending
  wrap**, because it scrolls instead of calling `MoveCursor`.
- **A combining mark with no base character** gets a U+00A0 base and advances
  the cursor one column.
- **The padding half of a wide character carries no attributes at all.**
  `buffer.c:3400` writes `attr`, `attr2`, `fg` and `bg` as zero, so a
  background-coloured wide character reports its colour on the lead cell and
  nothing on the pad. (The *insert-mode* branch at `:3325` copies the pen onto
  the pad instead. That inconsistency is upstream's; we reproduce both.)
- **Breaking a wide character is not the same as erasing it, and which one
  happens depends on who broke it.** Overwriting, inserting, deleting and
  scrolling all go through `BuffSetChar(b, ' ', 'H')`, which blanks the text
  and the colour indices but leaves the SGR attribute bits untouched and never
  consults the pen. The erase paths go through `EraseKanji`, which paints the
  whole pen — bold included, unlike the `memsetW` that erases the cells around
  it. `Cell::crush` and `Grid::erase_kanji` are the two halves of that split.
- **G1 starts as DEC special graphics**, so a bare SO switches to line drawing
  with no `ESC ( 0` in the stream at all. `charset.cpp:CharSetInit2`.
- **A single shift never ends.** `ParseFirst` clears `SSflag` after one
  character, but the UTF-8 path returns before reaching that code, so one
  `ESC N` redirects every later character to G2 for the rest of the session.
- **DEC special characters keep their raw byte and carry `AttrSpecial`** rather
  than becoming U+25xx, because `DecSpMappingDir` defaults to "do not map".
  Turning `q` into a horizontal line is the renderer's job.
- **On a VT100, 8-bit C1 controls are masked to C0** — `U+008D` is a carriage
  return, not RI, and `U+009B` is an ESC, not a CSI introducer. Above VT100 the
  mask does not apply. `vtterm.c:1053`.
- **`DispFindClosestColor` flips bright and dim.** Truecolor red resolves to
  palette index 1, "dark red", not 9; the drawing path applies the inverse, so
  the round trip is consistent and index 1 is what the cell stores.
- **`SGR 38`/`SGR 48` do not consume their arguments** when the matching colour
  mode is off. 256-colour is on by default so this is not normally visible, but
  turn it off and `ESC [ 38;5;196 m` parses as "38 ignored, 5 = **blink on**,
  196 ignored". `vtterm.c:2239`.

## Known divergences

- **DEL (0x7F) occupies a cell in Tera Term; `vte` discards it.** Only in the
  ground state — inside an escape sequence Tera Term strips it too — so
  reproducing it needs parser state `vte` does not expose. ECMA-48, xterm, VTE
  and alacritty all ignore DEL. Tracked by `oracle/cases/51-del-byte`, which
  carries an `xfail`.
- **Character width comes from the `unicode-width` crate, not Tera Term's
  tables.** Both derive from `EastAsianWidth.txt` and agree on unambiguous
  characters; ambiguous-width policy and emoji presentation will drift.
  Deferred with CJK.
- **Spacing combining marks** (Devanagari and friends), which join the base cell
  *and* advance the cursor, are not modelled — only nonspacing marks are.
- **Kanji and Katakana designations** (`ESC $ ...`, `ESC ( I`) are parsed and
  dropped. Deferred with CJK, and inert on a UTF-8 terminal anyway.
- Not yet implemented at all: DECLRMM, mouse reporting, DCS, and the window
  report sequences `WF_WINDOWREPORT` enables.
