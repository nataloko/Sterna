# tt-config

`TERATERM.INI`, and the schema that says what is in it.

```sh
cargo test -p tt-config                          # 17 tests, no Wine needed
cargo run -p tt-config --bin gen-settings        # after editing the schema
cd ../../ini-audit && ./run.sh                   # re-check the Win32 record
```

Two halves, and they are separate on purpose. `ini.rs` is a *file format*.
`schema.rs` plus `schema/settings.txt` are a *list of settings*. Neither knows
about the terminal — this crate does not depend on `tt-vt`, so the schema stays
a description of a file rather than of a running program, and the wiring
between them lives one layer up.

## The INI layer is not an INI parser

It is `GetPrivateProfile*`, reproduced. Upstream calls the Win32 API directly
(`common/inifile_com.cpp`, `ttpset/ttset.c`) and ships no portable version, so
the oracle cannot arbitrate this one: it stubs the profile calls and takes every
default, which is right for comparing parsers and useless for comparing file
handling.

So `ini-audit/` asks a real implementation instead — a mingw-built exerciser
run under Wine, 104 cases, recorded. `tests/win32.rs` replays the same battery
here and diffs. **98 match byte for byte**; the six that do not are in
`ini-audit/divergences.txt` with a reason each, and the gate fails in both
directions so a reason cannot outlive the behaviour it describes.

The full findings are in `ini-audit/README.md`. The four that decide whether a
generic crate could have been used instead:

- **The first duplicate wins**, for keys *and* sections — and a second `[Tera
  Term]` block is not merged, so a key that appears only in it is invisible.
- **A matched pair of quotes is stripped**, single or double, and an unmatched
  one is not. `PLAN.md` had this backwards.
- **`Key=` is an empty string, not the default.** Upstream leans on it:
  `ts.BSKey` is read with an empty fallback and only the literal `DEL` takes
  the other arm.
- **A comment is only a comment to enumeration.** `;A=1` is an entry whose key
  is `;A`, and a lookup for that name finds it.

The file is kept as **lines**, not as a map, because the faithful write is the
harder half: comments survive, an existing key keeps its own spelling, only the
first of a duplicate pair is touched, and nothing the caller did not ask about
is rewritten. Three of the six deliberate divergences are that last rule
holding — Win32 rewrites every line ending in the file, and normalises `[ s ]`
to `[s]`, when asked to set one unrelated key.

A file that is not valid UTF-8 is held as **Latin-1**. That is not a claim about
what the bytes mean — a Japanese Tera Term 4 wrote Shift-JIS and Latin-1 will
render it as nonsense — it is the only decoding under which every byte survives
and comes back unchanged, so the settings nobody touched are not quietly
rewritten. A lossy decode would turn each one into U+FFFD and destroy it on the
first save.

## The schema is the leverage point

`common/tttypes.h` is a 909-line `TTTSet`, surfaced by ~13.8k lines of dialog
code across 76 `DIALOG` templates in 30 `.rc` files. `PLAN.md` calls hand-porting
that "the difference between the project finishing and not", and risk 2 is the
motivation cliff at the dialogs.

So `schema/settings.txt` is one line per setting — name, type, INI section and
key, default, `.lng` label — with the comment lines above it as its
documentation. `src/bin/gen-settings.rs` turns that into `src/generated.rs`:
the struct, the defaults, `load`, `store`, name-addressed `get_str`/`set_str`,
one enum per enumerated setting, and `FIELDS`, a metadata table.

**`FIELDS` is the point.** The dialog builds itself from it, `setsetting` and
`getsetting` resolve through it, and the documentation table is printed from it,
so the list of settings exists exactly once. A dialog *generated* as C++ would
be a second copy to keep in step across two build systems; a dialog that reads
the metadata over the C ABI has nothing to keep in step.

The generated file is **committed**, and a test fails when it is stale — the
same arrangement as `tt-ffi`'s header, and for the same reason. Wiring a
generator into Cargo *and* CMake is how `PLAN.md`'s risk 5 starts.

### Every default carries the line that proves it

Because four of them are not where they look, and `CLAUDE.md` has a trap for
each: `CRReceive` and `BSKey` and `CursorShape` are `else` branches, and the
flag words are zeroed at the top of `ttset.c` and built up from per-key calls a
thousand lines below. A schema is only worth having if its defaults are
upstream's, so the citation is not decoration — it is what a reviewer checks.

**`GetOnOff` is default-biased** (`ttset.c:344`), which was found writing this
and is now a trap in its own right. With a default of on, anything that is not
literally `off` is on; with a default of off, only literally `on` is on. So
`Key=1` means **opposite things** for two settings that differ only in their
default, and `Key=yes` reads as off for half of them. It also reads into a
four-byte buffer, so only the first three characters are compared and `offline`
is `off`. All of it is reproduced, because a file that says `offline` is one
somebody's Tera Term is already treating as off.

## Still to come

- **Wiring**, which is the next step: `Settings` onto `tt_vt::Config` and onto
  the shell's `Theme`, both of which currently hold their own hard-coded copies
  of these values. That is where `keyboard.backspace` stops being a note in
  `shell/README.md` about how backspace does the wrong thing on Linux.
- **The rest of the settings.** 39 of roughly 600. The machinery is the
  expensive part and it is done; adding a row is a line and a citation.
- **The dialog**, built from `FIELDS` over the C ABI, with the search box
  `PLAN.md` asks for — which is worth more than the tabs, since nobody can find
  anything in 76 dialogs.
- **`KEYBOARD.CNF`**, which is an INI and reads with the same layer.
