# tt-fuzz — properties, and the fuzzers that explore them

`run_diff.sh` asks *is this what Tera Term does?* and can only ask it about
cases somebody wrote. This asks a cheaper question a machine can put to any byte
string at all: **is the engine still self-consistent?**

That matters here more than in most codebases, because the parser's whole job is
reading bytes nobody chose. A serial console at the wrong baud rate, a device
that reboots mid-escape-sequence and a hostile SSH server all deliver the same
thing.

```sh
cd crates
cargo test -p tt-fuzz                # the properties, on stable, in CI
cd fuzz && ./seed.sh                 # corpus out of oracle/cases/
cargo +nightly fuzz run vt_stream -- -max_total_time=300
cargo +nightly fuzz run vt_chunks
cargo +nightly fuzz run telnet
cargo +nightly fuzz tmin vt_stream artifacts/vt_stream/<file>
```

`cargo-fuzz` needs nightly and a few minutes; the property tests need neither,
which is what puts them in CI. **The fuzzer explores; the tests are what stop a
fixed bug coming back.**

## The properties

One set of definitions in `src/lib.rs`, used by both, so the fuzz target and the
test suite cannot drift about what is being asserted.

| | |
|---|---|
| `vt_stream` | No panic, and `Grid::check_invariants` after every chunk |
| `vt_chunking` | Where the chunk boundaries fall must not change the result |
| `vt_wide_pairs` | No wide character left half-written — over a *narrower* set of streams; see below |
| `telnet_chunking` | The same boundary question for the telnet decoder |

**`vt_chunking` is the one that earns its keep.** Bytes arrive from a socket or
a serial port in whatever sizes the kernel felt like, so every stream is already
a chunked stream — and the engine keeps real state across the boundary: `vte`'s
parser position, and `tt-vt`'s own `pending_c2`, `utf8_left` and `held`, which
exist precisely because a UTF-8 sequence can be cut in half. A bug there is
invisible to every test that feeds a whole file, which is every other test in
this repository.

`vt_stream` covers ground `run_diff.sh` cannot reach at all, for a simple
reason: a stream that panics produces no dump to diff.

## What it found

Five, in the first session it existed.

1. **An alternate-screen restore replaced the page instead of copying into it.**
   `CSI ? 1047 h`, a resize, `CSI ? 1047 l`, one character — and the grid held a
   page with the wrong number of rows, so the write panicked. Upstream clips to
   `min(saved, current)` on both axes (`buffer.c:5423`). Four escape sequences.
2. **`vte` 0.15.0 drops a byte when it resumes a partial sequence.**
   `advance_partial_utf8` prints only the first character of what it decoded and
   then reports `valid_up_to()` as consumed, so anything complete in between is
   lost: `[.. C3] [A9 'a' E4 B8 80]` prints `é一` and eats the `a`. Silent data
   loss on a UTF-8 console. `tt-vt` now holds partial sequences back so `vte`
   never sees one. Report drafted in `docs/vte-bug.md`; filing needs a GitHub
   account, like the Tera Term ones.
3. **A parked space did not break the wide cell under it.** With one column
   left, upstream parks a space and retries — via a recursive `BuffPutUnicode`
   (`vtterm.c:896`), so the space goes through the crushes at the top of the
   write path. Ours wrote the cell directly and left half a glyph, which is the
   one thing that branch exists to prevent. Case 106.
4. **An insert shift pushed half a wide character to the margin.** Upstream
   crushes a lead at `LineEnd - 1` before the shift (`buffer.c:3298`).
5. **A resize arriving on the wire did not re-anchor the viewport.**
   `Session::resize` handles it; DECCOLM and XTWINOPS reach `Grid::resize` from
   inside the parser and skipped it.

Only one of the five (3) is visible to the differential suite. That is the point
of having both.

## The wide/pad pairing is not an invariant

Worth stating plainly, because it looks like one and someone will try to
"fix" it. Tera Term leaves wide characters half-written in three places, and the
port reproduces all three:

- **DECCRA** is a bare `memcpyW` with no fixup in it or its caller.
- **The alternate-screen restore** clips columns, so the destination's own
  padding can outlive the cell it belonged to.
- **A double-width insert** shifts by two while the guard only crushes one.

So `Grid::check_wide_pairs` is a separate method rather than part of
`check_invariants`, and the property that uses it excludes those three.

**And the oracle cannot arbitrate any of it.** A lead with no padding still
prints as one glyph in two columns and a padding cell prints as nothing, so a
row whose halves have come apart renders exactly like one whose have not —
`run_diff.sh` says `ok`. Dumping upstream's `AttrKanji` bit does not rescue it
either: the bit is set on the non-insert write path and not the insert one
(`Attr_Attr` is the pen's byte alone) and `BuffSetChar` never clears it, so
upstream's own copy is incoherent. `check_wide_pairs` is the only check that
covers this ground, and finding 4 above is a bug it caught that nothing else
could.

## Corpus and artifacts

`fuzz/seed.sh` builds the corpora from `oracle/cases/*/input` — 106 streams
already aimed at the engine's corners, which beats starting from nothing.
`corpus/` is gitignored; `artifacts/` is not, because a committed crash file
becomes a permanent regression case replayed by `tests/corpus.rs` with no test
written for it. `tests/props.proptest-regressions` does the same job for
proptest and is committed for the same reason.
