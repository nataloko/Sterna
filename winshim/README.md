# winshim — what Tera Term's C needs from Windows, and nothing else

Three consumers compile Tera Term sources on Linux and all three need the same
small layer underneath:

| Consumer | What it compiles |
|---|---|
| `oracle/` | the VT engine — `vtterm.c`, `buffer.c` and friends |
| `xfer/` | the file-transfer protocols, as a standalone exerciser |
| `crates/tt-xfer/` | the same protocols, vendored and shipped |

It lived under `oracle/` until the third consumer arrived. It is not the
oracle's: a shipped crate must not reach into the test harness for its build,
and the Win32 surface the protocols need turned out to be a *subset* of the VT
engine's, so there was never a second layer to write.

## What is here

```
windows.h        the types, the constants, and about three real functions
msvc_compat.h    force-included; the dialect differences, not the API
msvc_crt.[ch]    MSVC's Secure CRT — strncpy_s, _snprintf_s_l, _stati64, ...
swscanf_s.c      the one Secure CRT function that needs a real parser
winshim.c        the handful of Win32 calls with behaviour worth having
codeconv_min.c   the eight codeconv entry points anything actually calls
setupapi.h       an empty shell; only enumeration includes it
crtdbg.h         likewise
sys/             MSVC spells two headers differently
```

**This is not an emulation and must not become one.** `windows.h` is types and
constants; the rule is that anything with *behaviour* goes in `winshim.c` or
`stubs_manual.c` next to a note saying what the real one does, because a stub
that quietly returns zero is how a harness starts lying. `oracle/README.md` has
three worked examples of exactly that happening, and `CLAUDE.md` has more.

`codeconv_min.c` sits here rather than with the oracle because
`ttpfile/protolog.cpp` needs two of the same entry points for its path
handling. Its legacy CJK paths are deliberately unimplemented and say so —
they want Tera Term's `.map`/`.tbl` tables, which `ATTRIBUTION.md` says to
regenerate from the UCD rather than copy, and CJK is out of scope anyway.

## Changing it

**Re-run `oracle/run_tests.sh` after touching anything here.** The oracle is
the thing that must not regress: it is the ground truth the Rust engine is
diffed against, so a shim change that shifts its behaviour shifts the
specification. `xfer/run_tests.sh` and `cargo test -p tt-xfer` cover the other
two consumers.
