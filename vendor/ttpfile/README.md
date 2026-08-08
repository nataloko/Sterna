# vendor/ttpfile — Tera Term's file-transfer protocols, verbatim

X/Y/ZMODEM, Kermit, B-Plus and Quick-VAN, copied unmodified from Tera Term and
compiled into `crates/tt-xfer`. **These files are not ours and are not edited.**

| | |
|---|---|
| Upstream | <https://github.com/TeraTermProject/teraterm> |
| Revision | `827a35b050c974b0fdf2a77ef73ed882301eb6c4` (`v5.6.0-496-g827a35b05`, 2026-08-06) |
| Licence | 3-clause BSD, inline in every file — see `ATTRIBUTION.md` |
| Copied | 2026-08-08 by `./sync.sh` |

## Why vendored rather than rewritten

Six protocols, 8,409 lines, and **two of them have no surviving counterparty
anywhere to test a rewrite against** — B-Plus was CompuServe's and Quick-VAN was
NIFTY-Serve's, and both services are gone. A rewrite of those two could only be
checked against a reading of the source it replaced, which is a worse artifact
than the source. `xfer/` (Stage 0 spike 2) established that the C compiles
unmodified on Linux and interoperates in both directions with `lrzsz` and
G-Kermit, 10 cases out of 10.

## What is here, and why each file

```
ttpfile/     the protocols themselves
  xmodem.c ymodem.c zmodem.c kermit.c bplus.c quickvan.c
  ftlib.c        shared helpers — CRC tables, timeouts, filename handling
  raw.c          "protocol"-less send/receive, same vtable
  protolog.c++   the transfer log, reached only when LogFlag is set
  *.h            their headers, plus filesys_io.h — the TFileIO vtable
teraterm/
  filesys_proto.h  the three vtables the whole subsystem attaches through
  filesys.h filesys_log.h   pulled in transitively
common/
  tttypes.h        TTTSet and TComVar, which Init() reads
  asprintf.cpp     protolog's string building
  ...              the rest are transitive includes, headers only
```

Nothing else from upstream is here. The set was computed with `gcc -MM` rather
than guessed, and `sync.sh` recomputes it.

## Keeping it honest

```sh
./sync.sh --check    # diff the copies against ../../../teraterm — silent if clean
./sync.sh            # re-copy, then read the diff before committing it
```

`--check` is what to run when upstream moves. A vendored tree that has silently
diverged is worse than no vendored tree: the differential suite compares our
Rust VT engine against upstream's, but nothing compares *these* files against
anything, because they **are** the implementation. The only guard is that they
stay byte-identical to a named revision.

If a local fix ever becomes necessary, it goes in a patch file applied to a
build copy — the arrangement `oracle/patches/` already uses — and never as an
edit here.
