# ini-audit

What `GetPrivateProfile*` actually does, asked of a real implementation rather
than of the documentation.

```sh
./run.sh              # build, run under Wine, diff against win32.txt
./run.sh --record     # ...and rewrite win32.txt with what came back
```

Needs `wine64` and `mingw-w64`, both from the Ubuntu archive. Without them
`run.sh` exits 127 and says so, which is why this is not in CI.

## Why this exists

`PLAN.md` asks for `TERATERM.INI` to be read **and written** natively and
"bug-compatible with `GetPrivateProfile*`". That is a claim about an API, and
Tera Term gives no help with it: `common/inifile_com.cpp` and `ttpset/ttset.c`
call the Win32 functions directly and there is no portable implementation
anywhere in the 157k lines. So the oracle cannot answer this one — it stubs
`GetPrivateProfileIntW` to return the default, which is exactly right for
comparing *parsers* and useless for comparing *file handling*.

Getting it wrong is not a cosmetic failure. The first thing this code will do
to a new user is read the `TERATERM.INI` they already have and then write it
back, and a wrong duplicate-key rule or a dropped comment silently changes
settings or destroys a file that took years to accumulate.

The battery lives in `cases.txt` as **data**, so the same 104 questions can be
put to the Rust implementation and the answers diffed. Same argument as
`run_diff.sh`: put the check where a mistake is visible rather than where it is
plausible.

## Wine is not Windows

This is measured against Wine 9.0's reimplementation, not against Microsoft's.
Two things follow, and both are recorded rather than hoped away:

- **Where Wine and MSDN agree, the answer is almost certainly right.** The
  quote stripping below is documented behaviour that Wine reproduces.
- **Where they say nothing — comment handling, what a write does to a file's
  line endings, whether `[ s ]` survives being written to — Wine is the only
  witness available here, and a difference from Windows would show up as a
  behaviour difference on the platform this project has not built for yet.**
  Every such answer is flagged below. Re-run the battery on Windows in Stage 3;
  `win32.txt` is the thing to diff, and `exercise.exe` compiles there natively.

Wine is also, incidentally, what runs the Tera Term this project is trying to
replace on this machine — so it is not an arbitrary stand-in.

## What it found

**The plan was wrong about quoting.** `PLAN.md` said "no quote stripping". A
*matched* pair of leading and trailing quotes, single or double, **is**
discarded — `Key="value"` reads back as `value` — which MSDN documents and this
confirms. Unmatched, mismatched and interior quotes are kept verbatim, and
`""value""` loses exactly one pair. A reader that keeps the quotes puts literal
`"` characters into every quoted setting; one that strips unconditionally
mangles a value that legitimately starts with one.

The rest, in the order they would bite:

| | |
|---|---|
| Duplicate key | **The first wins**, not the last |
| Duplicate section | **The first wins, and the second is not merged** — a key that appears only in the second copy is invisible |
| Enumeration | ...but a key listing reports duplicates twice, and section names likewise |
| Case | Section and key names match case-insensitively |
| Whitespace | Trimmed around the key, the value, the section name in the file **and** in the query — but never collapsed inside a name |
| Empty value | `Key=` returns an **empty string, not the default**. This is the trap `CLAUDE.md` records for `BSKey`, at the API level |
| Comments | `;` starts one; **`#` does not** — `#B=2` is a key called `#B` |
| Trailing comment | `Key=value ; note` keeps the whole thing, comment and all |
| ...and a comment is only a comment to *enumeration* | A lookup has no notion of one: `;A=1` is an entry whose key is `;A`, and asking for `;A` returns `1`. Asking for `A` misses because the names differ, which is why this is invisible until something enumerates |
| A line with no `=` | Not an entry at all — neither enumerated nor found |
| A key before any section | Belongs to a section whose name is empty. A literal `[]` starts a *different* section that is unreachable and unlisted, so the keys after it are lost |
| `[s] junk` | The junk after `]` is ignored, so the section is `s` |
| `[s` | Unterminated: the section does not exist |
| Line endings | LF-only and CR-only files both parse |
| Encoding | A UTF-16 BOM is honoured; anything else is read in the ANSI codepage, so a UTF-8 file without a BOM comes back as mojibake |
| Small buffer | Returns `size - 1` characters and NUL-terminates; a buffer of 1 returns nothing |

`GetPrivateProfileInt` is its own parser and shares almost nothing with the
string one:

| | |
|---|---|
| `42abc` | 42 — trailing junk is ignored |
| `abc42` | **0, not the default** — a value that fails to parse is zero |
| `0x1f` | 31 — hex is accepted |
| `-5` | 4294967291 — it returns unsigned, so a negative wraps |
| `+5` | 5 |
| `"42"` | 42 — the quote stripping happens first |
| `Key=` | The default, unlike the string call |
| `99999999999` | 1215752191 — overflow wraps |

### Writing, which is the half that can destroy a file

- **Comments and unrelated sections survive**, and an existing key is updated in
  place rather than appended.
- **The original case of an existing key is kept**: writing `keyname` to a file
  holding `KeyName` leaves `KeyName=9`.
- **Only the first of a duplicate pair is updated**; the second stays and, since
  the first wins on read, becomes permanently unreachable.
- **A new key goes at the end of its section**; a new section at the end of the
  file.
- **The value is written raw.** A value containing CR LF produces two lines and
  corrupts the file — so a writer has to reject or escape them, and Tera Term's
  own `Str2HexW` for `DelimList` is that escaping.
- **A UTF-16 file stays UTF-16 and a BOM survives**; an 8-bit file stays 8-bit
  and a non-ASCII value is transcoded to the ANSI codepage, losing anything not
  representable there.
- **Wine-only, unconfirmed on Windows:** an LF-only file is rewritten entirely
  with CRLF, and `[ s ]` is normalised to `[s]`. Both rewrite lines the caller
  did not ask about. If Windows does not do this, matching Wine would be the
  wrong choice — check in Stage 3 before relying on either.

## What this means for `tt-config`

Not "implement an INI parser". Implement **this** one, and prove it with the
same battery: `cases.txt` in, `win32.txt` out, diffed. A generic INI crate gets
at least the duplicate-key rule, the quote stripping, the empty-value rule and
the comment rules wrong, and each of those is a settings change the user never
made.

Two deliberate divergences are already visible and belong in the schema rather
than in the parser: writing a value containing CR LF should fail loudly instead
of corrupting the file, and a new file should be written UTF-8 with a BOM
rather than in whatever the ANSI codepage happens to be.
