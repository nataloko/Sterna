# tt-ttl

Tera Term's macro language, as a library. Upstream's `ttpmacro/` — `ttl.cpp`,
`ttmparse.cpp` and `ttmbuff.c`, about 9,200 lines — ported to Rust, with the
outside world behind one trait.

```rust
let mut host = MyHost::new();
let mut it = Interp::new("login.ttl", std::fs::read("login.ttl")?, &mut host);
it.run(&mut host);
```

## Why it is a crate and not a second process

Upstream's macro engine is `ttpmacro.exe`, and it reaches the terminal over
**DDE**: `ttpmacro/ttmdde.c` on one side, `teraterm/ttdde.c` on the other, about
2,600 lines between them and a conversation to keep in step. Here the engine is
a library and the terminal is on the other side of [`ScriptHost`], so there is
one process and nothing to keep in step.

That deletes a class of bug rather than a quantity of code. It also changes one
thing structurally: upstream's `wait` cannot block, because the macro and the
window share a thread, so it parks the macro in a `TTLStatus` state machine that
the window's message loop drives. The interpreter here gets its own thread, so a
host call may simply block, and there is no state for waiting — only for having
finished.

## What is a faithful port and what is not

The port is faithful. TTL is thirty years old, and the scripts that exist were
written against what it *does*, not what it documents. Where a behaviour reads
as a bug it is reproduced and the comment says so; where reproducing it would
mean reading off the end of a buffer, the observable half is reproduced and the
comment says that too.

Deliberately kept:

- **A TTL string is bytes, not text.** `#255` is a legal character escape, so a
  string need not be valid UTF-8 — and must not be, since `send` puts it on the
  wire unchanged. It is also a *C* string, so it stops at its first NUL: that is
  why `strspecial`'s `\0` truncates and why `code2str $01000041` is one byte.
- **A string operand short-circuits the whole expression grammar**, which is
  why `a + b` on strings is a syntax error and TTL has `strconcat` instead.
- **An expression cannot build a string, only name one.** Upstream returns the
  variable id in the same `int` it returns numbers in.
- **Blocks are skipped by executing them, not by seeking past them.** A counter
  suppresses the effect line by line, so a syntax error in a branch that is
  never taken is still an error, and `ElseFlag` increments *`EndIfFlag`* when it
  meets a nested `if` — because that `if`'s own `else` must not end the outer
  skip.
- **`for` steps its variable towards the end value**, so the loop counts down as
  readily as up, and `for i 3 3` runs once.
- **`strsplit` with no count answers 10 having stored 9.** The loop runs one
  field past the limit to throw the remainder away, and `result` is the count it
  reached.

Deliberately not kept, each an out-of-bounds access with no observable result:

- `strtrim` indexes a 256-byte table with a signed `char`, so a trim character
  above 0x7F reads before the start of it.
- `strsplit` reads one past its nine-element token array when the count is
  omitted and there were ten fields. The pointer is handed to a lookup for
  `groupmatchstr10`, which does not exist, so it is never dereferenced.
- `GetFactor` returns a label's *type* beside an uninitialised value. Every
  caller rejects the type first, so the value never escapes; [`expr::Eval`] has
  a `Label` arm carrying nothing for the same reason.

## Where the seam is

[`ScriptHost`] has one method per thing a command needs from outside, rather
than a general channel, so that a host implementing half of it is useful and the
rest reports "Unknown command" instead of pretending. Every method has a
refusing default. [`RecordingHost`] implements the parts that need no terminal
and is what the tests here run against.

**Loading a file is the host's job**, including working out its encoding.
Upstream's `LoadFileU8W` sniffs a BOM and falls back to the active Windows ANSI
code page. Windows reproduces that machine-dependent branch; Unix passes a
BOM-less file through because it has no ACP to ask. The `code_utf8.ttl` /
`code_utf16le-bom.ttl` / `code_cp932.ttl` cases in `../../../teraterm/tests/`
exist because that is a real decision with real files behind it. It does not
belong in a parser.

## What is here so far

| | |
|---|---|
**Every one of the 231 reserved words has an arm.**

| | |
|---|---|
| `lexer.rs`, `rsv.rs` | `ttmparse.cpp`'s tokeniser and the reserved word table |
| `vars.rs` | the variable table — integers, strings, arrays and labels in one namespace |
| `expr.rs` | the eleven precedence levels, `GetFactor` through `GetExpression` |
| `source.rs` | `fileread.cpp`'s `LoadFileU8C` — the macro file's own encoding |
| `cmdline.rs` | `ttmdlg.cpp`'s `ParseParam` — the switches, and what reaches `params[]` |
| `buffer.rs` | `ttmbuff.c` — the include stack, the control stack, the line reader |
| `interp.rs` | `ExecCmnd` — the four skip flags, assignment, and control flow |
| `strcmds.rs`, `sprintf.rs` | the string and integer commands, and C's `printf` |
| `regex.rs`, `regexcmds.rs` | Oniguruma, and the six commands that speak it |
| `wait.rs` | `ttmdde.c`'s matchers — `Wait`, `Wait2`, `WaitN` and the line buffer |
| `conncmds.rs`, `sesscmds.rs`, `sendcmds.rs` | the line: `send` and its variants, the `wait` family, the link, the control lines, the transfers |
| `filecmds.rs`, `files.rs`, `pathcmds.rs` | the sixteen handles, and the filesystem |
| `dlgcmds.rs`, `logcmds.rs`, `termcmds.rs` | the dialogs, the session log, the terminal's odds and ends |
| `envcmds.rs`, `clockcmds.rs`, `cksumcmds.rs`, `pwd.rs`, `pwdcmds.rs` | the environment, the clock, the checksums, the passwords |
| `host.rs` | the seam, and a host that records |

The language is complete, and what was left is no longer left: `tt-macro` is
the host that is a real terminal rather than a recorder, and
[`tt-ctl`](../tt-ctl) is the control socket that lets one be reached from
outside the process — including a `ttpmacro` entry point that parses the
command line in `cmdline.rs` and then asks a window to run the file.

The generated [TTL compatibility reference](../../docs/macro/README.md) carries
the full syntax and command manual. Its command index comes from `rsv.rs`, so
adding a reserved command without documenting it fails the generation check.

## An upstream defect, reproduced

`waitn` turns off the line buffer's clear-on-newline so that it can count bytes
across line breaks, and turns it back on again **only on the success path**.
`ttmmain.cpp`'s timeout arm sets `result` and `inputstr` and never calls
`ClearWaitN`, so after a `waitn` that timed out, every later `inputstr` in that
run accumulates across lines instead of holding one.

It is reproduced, because the rule here is fidelity and because a script cannot
sensibly depend on it either way. It is **not** in `docs/upstream-bugs.md`: that
file holds defects proven by running the two engines against each other, and
this one is a reading of the source that has not been demonstrated against a
real `ttpmacro.exe`. Demonstrate it on Windows in Stage 3 before filing it.

## Tests

```sh
cargo test -p tt-ttl                              # the unit tests
cargo test -p tt-ttl --test scripts               # upstream's own 53 macros
TTL_BLESS=1 cargo test -p tt-ttl --test scripts   # ...rewrite the transcripts
```

The unit tests are TTL source and an expected output, run against
`RecordingHost`.

`tests/scripts.rs` is the conformance suite: the 53 `.ttl` scripts in
`../../../teraterm/tests/`, each run against a host that writes down every
send, dialog and action in order and a golden transcript per script. They are
**not self-checking** — they report to a human through `messagebox`, several
are deliberately full of errors to exercise the error dialog, one asks the user
to choose a file, and most are Shift-JIS — so the transcript is what "pass"
means, and the harness's own header documents the eight decisions that make a
run reproducible. Blessing deliberately fails the run: a golden nobody has read
is worse than no test, which is `oracle/`'s rule and applies here unchanged.

A tree without the `../teraterm` reference checkout **skips** this suite rather
than failing it. `TERATERM_TESTS` points it somewhere else.
