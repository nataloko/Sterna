# esctest — the conformance suite, and what it is allowed to decide

```sh
./run_tests.sh                # every test, compared against `expected`
./run_tests.sh CUPTests       # just the ones matching (it is a regex)
./run_tests.sh --bless        # rewrite `expected` from what just happened
```

`esctest` is iTerm2's conformance suite, in [Thomas Dickey's maintained
fork](https://github.com/ThomasDickey/esctest2), pinned to a SHA in
`run_tests.sh` for the same reason the oracle pins Tera Term: the suite is the
expectation, so an upstream commit must not be able to turn the gate red with no
change on our side.

It is **not** the differential suite and does not outrank it. Its stated target
is "xterm, but without the bugs George Nachman minded" — which is not Tera Term,
and where the two disagree the oracle wins. So a failure here is a *question*,
and `expected` is where the answers are written down.

## What it needed that nothing else did

A recording cannot ask a question. `run_diff.sh` feeds a byte stream to both
engines and compares what the grid ended up holding; esctest writes a sequence
and then **reads the answer back** — cursor position, mode state, and the
contents of any single cell. It runs as an ordinary program on a pty and talks
to whatever terminal is on the other end.

So two things had to exist first:

- **`crates/tt-host`** — a terminal with no window. The same stack the Qt shell
  runs (`tt-session` over `tt-conn`'s pty, waiting on `poll_fd`), so what the
  suite exercises is the real loop and not a simpler one written for testing.
- **DECRQCRA** (`CSI Pid;Pp;Pt;Pl;Pb;Pr * y`), the rectangular-area checksum —
  **the only way to read a cell back over the wire**, and the one sequence in
  `tt-vt` that is not upstream's. `vtterm.c` has no `CSI * y` at all, so it is
  off by default and a real connection stays byte-for-byte Tera Term;
  `run_tests.sh` passes `--decrqcra` and nothing else does.

Three conventions come with DECRQCRA and none of them can be read off Tera Term,
so they are decided in `Config::decrqcra`'s documentation and repeated here:
the sum is over **characters only**, not attributes (esctest asserts that one
cell's checksum *is* its character code); it is the **plain sum**, not xterm's
pre-#279 two's complement; and an **erased cell counts as a space**, because
that is what an erase leaves in the grid. `--xterm-checksum 334` is what tells
esctest to expect the last two.

## Reading `expected`

One line per test that does not pass, `status name # reason`, and three statuses:

| | |
|---|---|
| `fail` | We do not do what esctest wants. The reason says whose decision that is. |
| `known-bug` | **esctest's** own annotation — it expects the terminal we claimed to be (xterm) to fail this. Nothing to do with us. |
| `skipped` | The test wanted a higher VT level than `run_tests.sh` asks for. |

A test that starts failing is a diff, and so is a test that starts *passing* —
a stale entry must not outlive the thing it describes, which is the same rule
the differential suite's `xfail` files follow.

**Every `fail` line needs a reason, and `--bless` will not invent one.** An
unexplained failure is a bug nobody has looked at yet.

## How a failure gets adjudicated

Not by reading esctest and deciding. By asking the oracle, which is the only
thing that knows what Tera Term does:

1. Run the suite with `--test-case-dir`, which writes each test's byte stream to
   a file. That stream is the test's *stimulus* — the replies it read back are
   deliberately excluded.
2. Feed that stream to both engines. If they agree, the failure is Tera Term
   not being xterm, and the reason says which upstream decision it is. If they
   disagree, **it is our bug**, and it gets a case in `oracle/cases/` and a fix.

That is how five sequences went in that the port had simply missed — HPR, VPR,
HPB and VPB, and a VPA that had lost origin mode — plus DECDSR, where we had
been answering the plain DSR reports to the private `CSI ? Ps n` form that
upstream reserves for the locator.

## Traps

- **The child does not start in this directory.** A pty is a terminal, not a
  shell, and `tt-host` starts the program in the user's home — so every path
  handed to esctest is absolute. A relative one fails as "the run did not
  finish", which looks like the harness rather than like a working directory.
- **`--expected-terminal` has three values and none of them is "something
  else".** xterm it is, and its terminal-specific expectations come along:
  `blank()` is a space rather than a NUL, and a handful of tests assert
  xterm-only behaviour outright. That is a floor on the failure count, not a
  bug to chase.
- **esctest always exits 0.** Its counts are prose in the log and its verdict
  is per-test, so the log's own lines are the result and the process status
  says nothing.
- **A test that fails to *read* a reply fails with a one-second timeout**, so a
  category we do not answer at all costs a second per test rather than a
  failure per test. The full run is about a minute and a half; if it becomes
  ten, something has stopped answering.
