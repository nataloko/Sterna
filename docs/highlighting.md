# Highlighting

Console output is undifferentiated. The line that matters — `%LINK-3-UPDOWN`,
an `ERROR`, a MAC address that should not be there — arrives in the same colour
as the thousand around it, and the only way to find it is to read them all.

A **highlight rule** is a regular expression and what to do to what it matches.
Rules are yours rather than the host's: they colour what is on the screen,
including text that arrived before the rule existed, and including the
scrollback.

Setup > Highlighting. Setup > Highlight matches turns the lot off without
deleting any of them.

## What a rule can do

| | |
|---|---|
| **Matches** | A regular expression, or plain text if you tick the box |
| **Text colour** | Left alone unless the box beside it is ticked |
| **Background** | The same |
| **Style** | Bold, underline, reverse — a rule can mark without spending a colour |
| **Colour the whole line** | The line the match sits on, not just the match |
| **Apply to** | The entire match by default, or just one capture group — a parenthesized part of the pattern, numbered from left to right |

The sample box at the bottom is coloured by the same engine the terminal uses,
so what you see there is what will happen.

**Order is priority.** Drag a rule up or down in the list. The first rule to
claim a cell's foreground keeps it, and the same for its background — so a rule
that only underlines and a rule that only colours compose, rather than one
silently swallowing the other. Put specific rules above general ones.

**A rule matches the whole logical line.** A command long enough to wrap is one
line to whoever typed it, so it is one line to a pattern; a match that straddles
the wrap is coloured on both rows. Trailing blanks are not part of the line, so
`ERROR$` matches a line ending in `ERROR` rather than one padded out to the
right margin.

**Highlighting is drawing and nothing else.** The session log, the clipboard,
the printer and a macro's `wait` all see exactly what the host sent. Nothing a
rule does can change what the terminal *is*.

## The pattern syntax

Rust `regex` syntax, which is Perl-like:

```
^ $ . * + ? | ( ) [ ] { }      the usual
\d \w \s \b                    digits, word characters, whitespace, boundary
\p{L} \p{Greek}                Unicode classes (see below)
(?i) (?s) (?x)                 case-insensitive, dot-matches-newline, verbose
(?:…)  (?P<name>…)             non-capturing and named groups
```

Two things it does **not** have, and they are the same two the engine gives up
to guarantee it can never stall the window: **backreferences** (`\1`) and
**lookaround** (`(?=…)`, `(?<=…)`). Every pattern runs in time proportional to
the length of the line, so no rule you can write will freeze the terminal on
some unlucky output from the far end.

This is a different engine from the one `waitregex` uses in a macro, which is
Oniguruma and does have those two. For everything short of them — alternation,
classes, repetition, anchors, groups — the two agree.

Of the Unicode tables, `\p{L}`, `\p{Nd}` and the other general categories are
built in, as are `\d \w \s \b` and case-insensitive matching for non-ASCII.
Script names (`\p{Greek}`) and ages are not; a pattern using one will not
compile, and the editor says so under the field as you type.

## Some rules worth having

```
\b(ERROR|FATAL|CRITICAL)\b          red, whole line
\b(WARN|WARNING)\b                  amber
%[A-Z]+-[0-9]-[A-Z_]+               a Cisco facility code, bold
\d{1,3}(\.\d{1,3}){3}               an IPv4 address
([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}  a MAC address
line protocol is (\w+)              group 1, so only the state is coloured
```

## The file

Rules live in the settings file, in a section of their own, so `--ini` gives you
a different set per profile. It is meant to be edited by hand; the dialog is the
convenience.

```ini
[Sterna Highlights]
Highlight1Label=Errors
Highlight1Pattern=\b(ERROR|FATAL|CRITICAL)\b
Highlight1Fore=255,80,80
Highlight1Style=bold
Highlight1Scope=line

Highlight2Label=Interface state
Highlight2Pattern=line protocol is (\w+)
Highlight2Group=1
Highlight2Back=0,80,0
Highlight2Enabled=off
```

`Highlight1` to `Highlight99`, in that order, which is their priority. A gap is
skipped. Every key but `Pattern` is optional:

| Key | |
|---|---|
| `Label` | For the editor's list. The pattern is shown when it is empty |
| `Pattern` | The expression, or plain text under `Literal` |
| `Literal` | `on` means the pattern is text, not a pattern |
| `IgnoreCase` | `on` matches without regard to case |
| `Fore`, `Back` | `r,g,b`. **Absent means leave that one alone** |
| `Style` | Any of `bold`, `underline`, `reverse`, comma-separated |
| `Scope` | `match` or `line` |
| `Group` | Which capture group to colour; `0` is the whole match |
| `Enabled` | `off` keeps the rule without applying it |

A pattern the engine will not compile is reported once, on startup, and that
rule does nothing — the others keep working. The editor will not save one, so
the only way to get there is by hand.

`[Sterna Highlights]` is one of this program's own sections; no real Tera Term
reads it, and a settings file shared with one still opens correctly in both. See
[`deviations.md`](deviations.md) for why the feature exists at all.

## What it costs

Nothing while nothing is happening, and very little while it is. The patterns
run over the rows that are on screen, as they are drawn — there is no cost on
the receiving side at all, which is why a rule you write now colours what
arrived an hour ago. All your rules are tested in one pass over each line, and a
line that matches nothing costs about as much as searching it for a single
character.
