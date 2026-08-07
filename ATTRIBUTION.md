# Attribution

## Tera Term

This project is a compatible reimplementation of
[Tera Term](https://teratermproject.github.io/). It is **not** affiliated with
or endorsed by the TeraTerm Project.

Tera Term is:

> Copyright (C) 1994-1998 T. Teranishi
> (C) 2004- TeraTerm Project
> All rights reserved.

Its sources carry a 3-clause BSD licence inline in each file. The full text is
reproduced in every file we build or vendor; the project's own `LICENSE.md`
points to
<https://teratermproject.github.io/manual/5/en/about/copyright.html>.

### How Tera Term code is used here

| Use | Where | Status |
|---|---|---|
| Compiled unmodified as a test oracle | `oracle/` | **Not redistributed.** Built from a sibling checkout at `../teraterm`. |
| One local bug fix, applied to a build copy | `oracle/patches/` | Fix only; upstream source unmodified. |
| Behavioural specification for the Rust port | `crates/` | No code copied. |

Nothing under `vendor/` yet.

> **TODO before vendoring anything.** The plan calls for vendoring
> `teraterm/ttpfile/*.c` (the file-transfer protocols), the 49 `.map`/`.tbl`
> charset tables, and the 14 `.lng` translation files. The `ttpfile` sources
> carry inline 3-clause BSD headers and are clear. **The `.lng` and `.map`/`.tbl`
> assets have no per-file headers** — confirm their terms against the copyright
> page above before copying them in, and record the outcome here.

### Bugs reported upstream

- `BuffGetAnyLineDataW()` drops everything after the first full-width character,
  truncating session logs of CJK text. See
  `oracle/patches/0001-buffgetanylinedataw-padding.patch`.
  **Status: not yet reported.**

## Third-party dependencies

None yet. Planned, with their licences to be recorded here as they land:
`vte`, `portable-pty`, `russh`, `serialport-rs`, `mlua`, `tokio`, and Qt 6
(LGPLv3, dynamically linked).
