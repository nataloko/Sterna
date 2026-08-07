# termitta core

The Rust side. Three crates so far, of the eight `PLAN.md` describes.

| Crate | What it is |
|---|---|
| `tt-grid` | Cells, lines, cursor, scroll region, scrollback. No I/O, no escape sequences. |
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

## Where this is faithful, and where it is not yet

Being a *port* rather than a fresh VT implementation means some upstream
behaviour looks like a bug until you check. Reproduced deliberately:

- **`SGR 38`/`SGR 48` do not consume their arguments** unless `CF_XTERM256` is
  set, and it is off by default — so `ESC [ 38;5;196 m` is parsed as "38
  (ignored), 5 (**blink on**), 196 (ignored)". `vtterm.c:2239`.
- **Erase keeps the current colours but drops bold/underline/reverse.**
  `buffer.c` passes `CurCharAttr.Fore/Back` with `AttrDefault` to `memsetW`.
- **DECSTBM homes the cursor to the screen origin**, not to the top of the
  region it just set, unless origin mode is on. `vtterm.c:2473`.
- **A line feed at the bottom of the scroll region does not clear the pending
  wrap**, because it scrolls instead of calling `MoveCursor`.
- **A combining mark with no base character** gets a U+00A0 base and advances
  the cursor one column.

Known divergences, none currently reachable by a test case:

- **Character width comes from the `unicode-width` crate, not Tera Term's
  tables.** Both derive from `EastAsianWidth.txt` and agree on unambiguous
  characters; ambiguous-width policy and emoji presentation will drift.
  Deferred with CJK.
- **Spacing combining marks** (Devanagari and friends), which join the base cell
  *and* advance the cursor, are not modelled — only nonspacing marks are.
- **Truecolor `SGR 38;2;r;g;b`** quantises to the xterm-256 cube rather than
  reproducing `DispFindClosestColor`. Unreachable while `CF_XTERM256` is off.
- **Character-set designation** (`ESC ( B` and friends) is parsed and dropped.
  DEC special graphics needs it; that is the next thing `tt-vt` grows.
