# Quick buttons

A bar of commands one click away. On a console port the same handful of lines
get typed all day — `show version`, `show interfaces`, `save config`, a
`reload` — and a button is the difference between typing one and pressing it.

Tera Term has no equivalent, so this is a deliberate divergence; the reasoning
is entry 7 in [`deviations.md`](deviations.md). What a button *does*, though, is
upstream's: the four kinds are `KEYBOARD.CNF`'s `[User keys]` types, and
pressing a button and pressing a mapped key go through the same code.

## Making one

**Setup > Quick buttons…** is the full editor. The panel is shown by default
even before a button exists, with a **+** that opens the editor on the first
empty row.

The shortest way in is **Edit > New quick button from selection…**: select the
command that just worked, and the editor opens with it filled in.

Once the bar is there, the **+** at the bottom of the panel adds another —
opening the editor on a new, empty button — and a right-click on any button
offers Edit, Duplicate and Remove.

The panel lives **down the right-hand side**, because a terminal's rows are the
scarce dimension: a window is usually far wider than the 80 columns it needs
and exactly as tall as it can be, so a vertical bar costs nothing that is being
used and the labels have room to be words. It is as wide as its widest caption
until you say otherwise.

To give it a different width, **right-click the panel and use Panel width** —
*Set width…* for a number, *Fit to buttons* to go back to measuring the
captions. The same thing is on the Window page of Setup > Preferences, under
`Quick buttons width`.

**The pixels come out of the window, not out of the terminal** — the window
gets wider and the terminal keeps every column and every character it had.
Where the window cannot get any wider, because it is maximised or already at
the edge of the screen, the panel stays where it is instead. That is
deliberate: a terminal narrowed by a column loses whatever was past its right
edge, in the scrollback as well as on screen, and it does not come back when
the panel is made narrow again.

**It will go narrower than its own captions**, and the buttons shorten their
text with an ellipsis when it does — a panel of stubs down the edge of the
screen is a perfectly reasonable thing to want, and one long label should not
widen the other nine buttons. The full caption is still there: it is in the
tooltip whenever the button is too narrow to show it, and it is what the editor
and the settings file hold. There is a floor of 48 pixels, which is about a
target you can still hit.

**The buttons scroll when there are more of them than there is panel**, and the
**+** and the page list stay put underneath. A column of buttons in a plain
layout is a column that demands its own height: before it scrolled, twenty
buttons made the window at least twenty buttons tall, changing to a page with
more of them on it grew the window again, and past the point where the screen
ran out every button shrank instead until they were slivers. The panel takes
the height it is given — the terminal's — and what does not fit is one wheel
away.

**There is no handle to drag**, and that is a decision rather than an omission.
One was built and taken out again: a window cannot move its own left edge on
every desktop, so widening the panel widens the window to the *right* and the
handle never moves out from under the pointer — which is exactly the opposite
of how a handle should feel. The menu does the same job, and it works on a
maximised window, from the keyboard, and from a macro.

## Pages

One flat column is the right shape until it is not. The moment somebody keeps
commands for four different devices — a router, a BMC, a switch, a board on the
end of a serial cable — a single list is something to read rather than a bar to
hit, and the `reload` for one of them is sitting next to the `show version` for
another.

So the panel has pages. Each button is on one, and the panel shows one at a
time.

**A drop-down appears at the bottom of the panel when there is a second page**,
and not before: a control that can only say one thing is chrome in a program
whose claim is being light. It sits under the **+**, where the two things that
work the panel are together and out of the way of the buttons they work on —
and where neither of them scrolls away with a long list. It is also on the
panel's own right-click menu, under **Page**, along with *Add, rename or remove
pages…* — which opens the editor on its page controls.

In the editor, the list on the left is one page's, with the page above it and a
**Pages** menu beside it. A button's own **On page** field moves it, and so
does **Move to page** on its right-click menu on the panel.

**Removing a page keeps its commands.** They move to the page before it — or,
for the first page, onto what was the second — so nothing is lost and nothing
has to ask. Deleting a command is still Remove, which asks by name.

**The page you were on is the page you come back to**, across a restart, per
settings file. So `sterna --ini datacentre.ini` opens where it was left.

A shortcut works from every page. That is deliberate rather than incidental: a
shortcut is a key the terminal stops receiving, and one that came and went with
a drop-down would be a key that works when nobody is looking at it and not when
they are.

### Taking a page somewhere else

**Pages > Export page…** writes the page as an ordinary settings file — one
`[Sterna Buttons]` section, nothing else — and **Import page…** reads one back
as a new page. That means three things at once:

- an exported page can be pasted into a settings file by hand;
- any settings file can be imported, and a file with several pages in it
  arrives as several pages;
- exporting onto a file that already exists replaces its buttons and leaves
  everything else in it alone, so "export this page" and "put these commands in
  the `router.ini` I already have" are one command.

An imported button arrives with no shortcut. The file it came from knows
nothing about the keys this one has already given away.

## What a button can do

| Kind | What happens |
|---|---|
| **Send text** | Typed into the session, exactly as if it had come from the keyboard — so `CRSend` and the terminal's newline mode apply |
| **Send bytes** | Put on the wire unchanged: no encoding, no newline conversion |
| **Run macro** | Starts a `.ttl` or `.lua` file, as Control > Run macro does |
| **Menu command** | Does what a menu item does — send a break, start logging, disconnect |
| **Send file line by line** | Feeds a text file to the far end a line at a time, waiting for the device between lines — see [`sending.md`](sending.md) |

**Send Enter after** is what makes a command run rather than just appear. It is
ticked by default for a new text button, and a **Shift+click** sends the
command *without* it, for when it wants finishing by hand on the far end.

**Ask before running** puts a question in front of the command. It is worth
using on anything that reboots something: a `reload` button sitting next to a
`show version` button is one misclick from an outage.

A button that sends is greyed out while nothing is connected. A menu-command
button is not, because Save setup and the settings dialog work perfectly well
offline.

## Repeating a command

**Repeat** sends the same command more than once: *n* times every *x.x*
seconds, or — with the count wound below one, where it reads **Until
stopped** — for as long as it is left running. A `show clock` every five
seconds while something is being chased down, a keepalive on a console that
drops idle sessions, a `show interfaces` every minute during a change window.

The first send happens the moment the button is pressed; the interval is the
gap between one send and the next, so *3 times every 2 s* takes four seconds
and not six.

While a button is repeating it shows as pressed with a **⟳** after its label,
and its tooltip counts down the sends still to come. **There are four ways it
stops:**

- **press the button again** — the second press is a stop, and it is not
  confirmed even on a button that asks before running;
- **Escape in the terminal**, which stops every run at once. Escape reaches the
  host as usual the rest of the time — the terminal only claims it while
  something is actually repeating;
- **right-click the button > Stop repeating**;
- by itself, when the count runs out, when the connection goes away, or when
  the button list is edited.

A run belongs to the session it was started on. Switching tabs to watch
something else does not redirect it onto the console that happens to be in
front, and closing that tab ends it.

The interval has a floor of 0.1 s and a ceiling of an hour. The floor is not
negotiable through the dialog or the file: it is what stops a mistyped number
turning a button into a flood.

## Shortcuts, and the one thing to know about them

**A shortcut is a key the terminal stops receiving.** Qt gives an action first
refusal on a key sequence, so a button holding `Shift+F1` means the host never
sees `Shift+F1` again — and nothing on screen would say so.

That is why no button ships with one, and why the editor offers `Ctrl+Alt+1`
through `Ctrl+Alt+0` rather than the function keys: `Alt` alone is Meta when
`MetaKey` is on, `Ctrl` alone is how a terminal sends control characters, and
the function keys are exactly what a full-screen program on the far end wants.

The editor says so as a sequence is typed. It warns when the key is already:

- another quick button's;
- a menu item's, or one a Lua plugin installed;
- assigned in the loaded `KEYBOARD.CNF`;
- one the terminal would ordinarily send — unmodified, Shift-only, or any
  function key.

It is a warning and never a refusal. If you know the host does not use `F5`,
take `F5`.

Hiding the bar (**View > Show quick buttons**) hands the keys back: the
shortcuts belong to the bar's own actions, so putting it away releases them.

## In the settings file

Buttons live in the settings file, so `sterna --ini router.ini` is a router
button set and `sterna --ini bmc.ini` is a different one. They are meant to be
edited by hand as well as through the dialog:

```ini
[Sterna Buttons]
Page2Name=BMCs

Button1Label=Show version
Button1Kind=text
Button1Value=show version$0D
Button1Shortcut=Ctrl+Alt+1
Button1Confirm=off

Button2Label=Reload
Button2Kind=text
Button2Value=reload$0D
Button2Confirm=on

Button3Label=Break
Button3Kind=command
Button3Value=50430

Button4Label=Poll
Button4Value=show clock$0D
Button4Repeat=forever
Button4IntervalMs=5000

Button5Label=Power status
Button5Page=2
Button5Value=power status$0D

Button6Label=Base config
Button6Kind=file
Button6Value=/home/me/switch-base.txt
Button6Gate=prompt
Button6Prompt=[#>] $
```

`Button1` … `Button99`, in that order; a gap is skipped. Only `Value` is
required — a button with no `Label` is captioned with its own command. The
ninety-nine are the whole file, every page together, so pages divide them
rather than multiplying them.

**A file that says nothing about pages is a one-page file**, byte for byte what
it was before pages existed — `Page` is only written for a button that is not
on the first page, and `PageNName` only for a page that has been named. Sterna
groups each page's buttons together when it writes the section, so a file that
has pages reads in page order however it was edited.

One thing to know if you go **back** to a Sterna older than pages: it reads a
paged file happily, showing every button in one column, but it does not know
the `Page` keys are there. Saving from it renumbers the buttons and leaves
those keys where they were, so they end up on whichever button now has that
number. Copy the file before downgrading, the same as for any other setting a
newer version wrote.

| Key | Meaning |
|---|---|
| `Label` | What is written on the button. Plain text |
| `Kind` | `text`, `bytes`, `macro`, `command` or `file`. `text` if absent; an unrecognised word drops the button rather than guessing |
| `Value` | The command. `$HH`-escaped for `text` and `bytes`; a file path for `macro` and `file`; a decimal menu id for `command` |
| `Shortcut` | A Qt key sequence, such as `Ctrl+Alt+1`. Absent means none |
| `Confirm` | `on` to ask first. Anything else is off |
| `Repeat` | How many sends one press makes. `1` if absent; `forever` (or `0`) for a run only a person stops; anything unreadable is one send |
| `IntervalMs` | Milliseconds between sends. 1000 if absent, and held between 100 and 3600000. Milliseconds, so the file needs no decimal point — the dialog is where this is seconds |
| `Page` | Which page it is on, from 1. Page 1 if absent, and a number past 99 lands on the last page rather than back on the first |
| `Gate` | `file` only: what to wait for between lines — `none`, `prompt`, `echo` or `quiet`. Absent uses `SendGate` from the settings, and so does anything unrecognised |
| `Prompt` | `file` only: the pattern for `Gate=prompt`. Absent or empty uses `SendGatePattern` |

`Gate` and `Prompt` are written only for a `file` button, so a file with none
of them is byte for byte the file it was before this kind existed. They are the
button's rather than the settings' because two pages of buttons is exactly how
somebody keeps a switch's `#` and a boot loader's silence apart. The
*intervals* are not here: those say how long Sterna is prepared to wait, not
how a particular device shows that it is ready.

Beside the buttons, `Page2Name`, `Page3Name` … name the pages. A page with no
name is called `Page 2` on screen and has no key in the file. **A page exists
when a button says it is on that page, or when it has a name** — which is what
lets you make a page and fill it later.

`Value`'s escape is Tera Term's own — the one `Answerback` and `DelimList` are
stored in. `$0D` is a Return, `$0A` a line feed, `$24` a literal `$`. Anything
the file cannot hold is written that way, and everything else is left legible.

The three settings beside them are ordinary `[Sterna]` keys: `QuickButtons` (on)
shows or hides the bar, `QuickButtonsWidth` is how wide it is in pixels, and
`QuickButtonsPage` is the page it opens on. `0` — the shipped width — means as
wide as the widest button needs, which is where the panel sits until somebody
puts a number there. Pixels rather than a column count because the panel holds
words and not cells, and the same captions want a different number of pixels at
every font size.

## Menu command ids

`Value` for a `command` button is one of Tera Term's own `tt_res.h` numbers.
The editor offers them by name; these are the ones this window implements.

| Id | Command |
|---|---|
| 50110 | New connection… |
| 50111 | Duplicate session |
| 50112 | Local shell |
| 50120 | Start or stop logging |
| 50130 | Send file… |
| 50131 | Receive file… |
| 50190 | Disconnect |
| 50199 | Close the window |
| 50210 | Copy |
| 50230 | Paste |
| 50240 | Paste and send |
| 50310 | Terminal settings… |
| 50330 | Font… |
| 50380 | Save setup |
| 50395 | Load key map… |
| 50430 | Send break |
| 50470 | Run macro… |

## The other two ways to bind a command

Quick buttons are not the only way to put a command on a key, and which one to
reach for depends on what you have.

- **`KEYBOARD.CNF` user keys** are the same four actions with no face on them,
  and they are compatible with a real Tera Term. Use them when the file has to
  work in both programs.
- **[Lua plugins](plugins.md)** (`sterna.menu` and `sterna.key`) run code
  rather than a fixed command, and can decide what to send. Use them when the
  answer depends on something.

Quick buttons are the one of the three that needs neither a text editor nor a
programming language.
