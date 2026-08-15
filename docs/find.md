# Find

The line you want went past four minutes ago. Without a way to search the
terminal, getting it back means starting a log, reproducing the problem, and
opening the file in something else — by which time you are reading a text editor
rather than the console the line came from.

**Ctrl+Shift+F**, or Edit > Find, opens a bar over the bottom of the terminal.
It searches everything the terminal is holding: the page in front of you and all
of the scrollback.

## Using it

| | |
|---|---|
| **Type** | It searches as you go and jumps to the first match |
| **Enter** | The next match. Shift+Enter, the one before |
| **Next / Previous** | The same, for a mouse. Both wrap round the ends |
| **Escape** | Closes the bar and gives the keyboard back to the terminal |

The match you are on is **selected**, which is what scrolls it into view and
what makes Ctrl+Shift+C copy it — closing the bar leaves it selected, so you can
find something and take it without the bar in the way. Every *other* match on
screen is filled in amber.

The label on the right says which match you are on and how many there are, or
says why the pattern will not compile.

## The three boxes

| | |
|---|---|
| **Case** | `Error` stops matching `ERROR` |
| **Whole word** | `err` stops matching `errors` — the match needs a word boundary at each end |
| **Regex** | The pattern is a regular expression rather than text to find |

They are remembered, so somebody who works in regular expressions does not tick
the box again every morning. So are the last twelve patterns, on the dropdown at
the left of the field.

With **Regex** off, everything you type is literal: `10.0.0.1` finds that
address rather than any four numbers, and `(auth)` finds the parentheses.

## The pattern syntax

The same engine, and therefore the same syntax, as
[highlight rules](highlighting.md) — including the two things it does not have,
backreferences and lookaround, which are what guarantee that no pattern you can
write will freeze the terminal on some unlucky output from the far end. That
page has the table and the worked examples.

## What a line is

**A wrapped line is one line.** A command long enough to run onto the next row is
one line to whoever typed it, so it is one line to a pattern: a match that
straddles the wrap is found, and shown on both rows. `$` means the end of the
text, not the right margin, so `failed$` finds a line ending in `failed` rather
than one padded out with spaces.

A very long logical line — the output of something with no newlines in it at all
— is followed for 128 rows and then treated as ending, which is the same bound
highlight rules use.

## What it does not touch

Finding something changes nothing about what the terminal *is*. The session log,
the clipboard, the printer and a macro's `wait` all see exactly what the host
sent; the amber is drawn and never stored. Nothing is searched on the receiving
side either, which is why a pattern typed now finds text that arrived an hour
ago — and why a closed find bar costs nothing at all.

A line that has aged out of the scrollback is simply not found. The buffer's far
end moves as the host prints, and how much is kept is `ScrollBuffSize` on
Setup > Terminal.

## The file

Four keys, in this program's own section:

```ini
[Sterna]
FindColor=0,0,0,255,220,120
FindHistory=ERROR;line protocol;%3Bsemicolon
FindCase=off
FindWholeWord=off
FindRegex=off
```

`FindColor` is `fg_r,fg_g,fg_b,bg_r,bg_g,bg_b` — the pair the matches you are
*not* on are painted in. The one you are on is a selection and uses the
selection's colours.

`FindHistory` is the dropdown, newest first, separated by `;`. A pattern
containing `%` or `;` is written `%25` or `%3B`, since a semicolon is an
ordinary thing to search a console log for.

`[Sterna]` is Sterna's own section; no real Tera Term reads it, and a settings
file shared with one still opens correctly in both. See
[`deviations.md`](deviations.md) for why the feature exists, why the shortcut is
Ctrl+Shift+F rather than Ctrl+F, and why the bar floats over the terminal
instead of sitting under it.
