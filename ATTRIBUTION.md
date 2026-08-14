# Attribution

This project is licensed under the 3-clause BSD licence — see `LICENSE`. That
choice is deliberate: the Tera Term code this project vendors carries the same
licence, so the shipped distribution has one licence text rather than two.

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
| Compiled unmodified as a protocol harness | `xfer/` | **Not redistributed.** Same sibling checkout. |
| One local bug fix, applied to a build copy | `oracle/patches/` | Fix only; upstream source unmodified. |
| Behavioural specification for the Rust port | `crates/` | No code copied. |
| **Vendored and shipped** — the file-transfer protocols | `vendor/ttpfile/` | **Redistributed**, verbatim, notices retained. See below. |
| **Vendored and shipped** — the language files | `vendor/lang/` | **Redistributed**, verbatim, project notice retained here. See below. |
| **Transcribed and shipped** — one data table | `crates/tt-config/src/services.rs` | **Redistributed** as data. See below. |
| **Converted and shipped** — the English TTL manual | `docs/macro/` | **Redistributed** as generated Markdown, project notice retained here. See below. |

### `vendor/ttpfile/` — what is shipped

33 files, 11,568 lines, copied unmodified from upstream revision
`827a35b050c974b0fdf2a77ef73ed882301eb6c4` (`v5.6.0-496-g827a35b05`,
2026-08-06) and compiled into `crates/tt-xfer`. **Every one carries the
3-clause BSD notice inline**, checked file by file rather than assumed, and the
notices are retained unaltered. `vendor/ttpfile/sync.sh --check` proves the
copies are still byte-identical to that revision.

This is the only Tera Term code the distribution contains. Because our own
licence is the same 3-clause BSD, the shipped tree needs one licence text
rather than two — which is why `LICENSE` says what it says.

### `vendor/lang/` — what is shipped

14 UTF-8 `.lng` files, copied unmodified from upstream revision
`827a35b050c974b0fdf2a77ef73ed882301eb6c4` (`v5.6.0-496-g827a35b05`,
2026-08-06). `vendor/lang/sync.sh --check` proves the copies are still
byte-identical to that revision.

The files carry no individual headers. Tera Term's copyright page places no
separate terms on translations or contributed content, so the project-wide
3-clause BSD licence applies. The notice at the top of this file and the
repository's `LICENSE` accompany them in source and binary distributions.

### `crates/tt-config/src/services.rs` — one transcribed table

The 317-entry TCP service-name table from `teraterm/common/servicenames.c`,
turned into a Rust array. That file is **Robert O'Callahan's**, 1998–2001, under
the same 3-clause BSD notice the rest carries, with the TeraTerm Project as
later copyright holder; the notice is inline in the original and cited in the
module.

Not `/etc/services` and not `getservbyname`, deliberately: what `/P=telnet` or
`myhost:ssh` means has to be the same number on Linux and on Windows and the
same number it was in 2003, so the specification is upstream's table rather
than the host's. The data is facts about port numbers, but the selection is
upstream's, so it is credited like code.

### `docs/macro/` — the TTL manual

The 214 English macro-manual pages and their one referenced PNG are converted
mechanically from the same pinned upstream revision. The Markdown retains the
TeraTerm Project copyright notice, and `docs/macro/generate.py --check` proves
that the committed reference still matches both that source and `tt-ttl`'s
implemented command table. The generated files are redistributed under the
project-wide 3-clause BSD licence described above; the generator itself is
Sterna code.

## Vendoring clearance

Checked 2026-08-07, against the copyright page above and the files themselves.
**This supersedes an earlier note in this file which said the `.map`/`.tbl` and
`.lng` assets had "no per-file headers" — that was right about the `.lng` files,
wrong about some of the tables, and wrong about what follows from either.**

### `ttpfile/*.c` — clear

Inline 3-clause BSD headers in every file. Nothing further needed beyond
retaining the notices, which we do.

### The 14 `.lng` translation files — clear

They carry no per-file header, and the copyright page states no separate terms
for translations or contributed content. They are therefore covered by the
project-wide 3-clause BSD licence like the rest of the work. Vendor them
verbatim, retain the notice, credit the TeraTerm Project.

### The 49 `.map`/`.tbl` charset tables — mostly not Tera Term's to license

This is the part the earlier note got wrong, and it matters. Only **4** carry a
Tera Term copyright header:

- `uni2sjis.map`, `uni_combining.map`, `unisym2decsp.map`, `unicode_emoji.tbl`

The other 45 are **generated from Unicode Consortium data**, and say so:

- `mapping/cp*.map` — headed `// CP1250.TXT` etc., i.e. derived from Unicode's
  own codepage mapping files
- `unicode_asian_width.tbl` — headed `// this file was generated by
  get_asianwidth_table.pl` from `EastAsianWidth-17.0.0.txt`
- `unicode_block.tbl`, `unicode_combine.tbl`, `unicode_virama.tbl` — same shape,
  from the Unicode Character Database

So they are governed by the **Unicode licence**, not by Tera Term's BSD, and
Tera Term is a downstream consumer of them exactly as we would be. Two
consequences:

1. If these are ever wanted, **regenerate them from the UCD** rather than
   copying Tera Term's copies. It sidesteps the provenance question entirely and
   the generator scripts are in the upstream tree.
2. Record the Unicode licence here at that point, not Tera Term's.

**Moot for now:** CJK is deferred indefinitely (see `PLAN.md`), and these tables
are deferred with it.

## Bugs reported upstream

- `BuffGetAnyLineDataW()` drops everything after the first full-width character,
  truncating session logs of CJK text. See
  `oracle/patches/0001-buffgetanylinedataw-padding.patch` and
  `docs/upstream-bugs.md` for the report as drafted.
  **Status: drafted, not yet filed** — needs a GitHub account to post to
  <https://github.com/TeraTermProject/teraterm/issues>.

## Third-party dependencies

Recorded as they land. Present:

| Dependency | Licence | Linkage |
|---|---|---|
| `serialport` | MPL-2.0 | Rust crate, `serial-audit` |
| `russh` | Apache-2.0 | Rust crate, `ssh-audit` |
| `libc` | MIT OR Apache-2.0 | Rust crate |
| `tokio` | MIT | Rust crate |
| `onig` / `onig_sys` | MIT | Rust crate, `tt-ttl` |
| **Oniguruma** | BSD-2-Clause | **C, statically linked** — vendored inside `onig_sys` |
| `aes`, `ctr`, `pbkdf2`, `hmac`, `sha2` | MIT OR Apache-2.0 | Rust crate, `tt-ttl` |
| `getrandom` | MIT OR Apache-2.0 | Rust crate, `tt-ttl` |
| **libglvnd frontends** | MIT and BSD-1-Clause | **Shared libraries, AppImage only** |

Planned, licences to be confirmed as they are added: `vte`, `portable-pty`,
`mlua`, and Qt 6 (LGPLv3, **dynamically linked** — see the Qt posture note in
`PLAN.md`).

**Oniguruma is the one dependency that is C compiled into the binary rather
than a Rust crate**, and it is listed on its own line because the notice
obligation is its own: BSD-2-Clause requires the copyright notice and the
disclaimer to be reproduced *in the documentation or other materials* of a
binary distribution. Its `COPYING` is
`Copyright (c) 2002-2021 K. Kosako`, and it has to reach the AppImage and the
Windows installer, not only this file. Tera Term vendors and builds the same
library, so this adds no third party the project did not already have — but
upstream's obligation is not ours automatically, and this is where ours is
recorded.

**The AppImage bundles libglvnd's driver-neutral `libOpenGL`, `libEGL`,
`libGLX` and `libGLdispatch` frontends**, but no Mesa or NVIDIA driver. The
runtime sources are MIT licensed and include uthash under its one-clause BSD
licence. Their retained notices ship as `LIBGLVND-LICENSE.txt` beside the
other AppImage documentation.

**`serialport` is MPL-2.0, which is file-level copyleft, and that interacts with
a decision spike 4 already flagged.** Using the crate is unproblematic: MPL
copyleft attaches to *its* files, not to ours, so our code stays 3-clause BSD.
But spike 4 concluded we need a patch layer over it, and if any of that ends up
as a *modification to serialport's own sources* — plausibly `CMSPAR` parity
support, which the crate lacks — those changes must be published under MPL-2.0.
That is fine and we would upstream them regardless; it is recorded so nobody
later assumes the whole tree is uniformly permissive.
