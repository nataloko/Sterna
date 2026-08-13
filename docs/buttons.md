# Quick buttons

A bar of commands one click away. On a console port the same handful of lines
get typed all day — `show version`, `show interfaces`, `save config`, a
`reload` — and a button is the difference between typing one and pressing it.

Tera Term has no equivalent, so this is a deliberate divergence; the reasoning
is entry 5 in [`deviations.md`](deviations.md). What a button *does*, though, is
upstream's: the four kinds are `KEYBOARD.CNF`'s `[User keys]` types, and
pressing a button and pressing a mapped key go through the same code.

## Making one

**Setup > Quick buttons…** is the editor. There is no bar until a button
exists, so that is also where the feature is found.

The shortest way in is **Edit > New quick button from selection…**: select the
command that just worked, and the editor opens with it filled in.

Once the bar is there, the **+** at its end adds another, and a right-click on
any button offers Edit, Duplicate and Remove. The bar can be dragged to any of
the four window edges; on the left or the right it costs no terminal rows,
which on a short window is worth knowing. Where it was left is remembered.

## What a button can do

| Kind | What happens |
|---|---|
| **Send text** | Typed into the session, exactly as if it had come from the keyboard — so `CRSend` and the terminal's newline mode apply |
| **Send bytes** | Put on the wire unchanged: no encoding, no newline conversion |
| **Run macro** | Starts a `.ttl` or `.lua` file, as Control > Run macro does |
| **Menu command** | Does what a menu item does — send a break, start logging, disconnect |

**Send Enter after** is what makes a command run rather than just appear. It is
ticked by default for a new text button, and a **Shift+click** sends the
command *without* it, for when it wants finishing by hand on the far end.

**Ask before running** puts a question in front of the command. It is worth
using on anything that reboots something: a `reload` button sitting next to a
`show version` button is one misclick from an outage.

A button that sends is greyed out while nothing is connected. A menu-command
button is not, because Save setup and the settings dialog work perfectly well
offline.

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

Hiding the bar (**Setup > Show quick buttons**) hands the keys back: the
shortcuts belong to the bar's own actions, so putting it away releases them.

## In the settings file

Buttons live in the settings file, so `sterna --ini router.ini` is a router
button set and `sterna --ini bmc.ini` is a different one. They are meant to be
edited by hand as well as through the dialog:

```ini
[Sterna Buttons]
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
```

`Button1` … `Button99`, in that order; a gap is skipped. Only `Value` is
required — a button with no `Label` is captioned with its own command.

| Key | Meaning |
|---|---|
| `Label` | What is written on the button. Plain text |
| `Kind` | `text`, `bytes`, `macro` or `command`. `text` if absent; an unrecognised word drops the button rather than guessing |
| `Value` | The command. `$HH`-escaped for `text` and `bytes`; a file path for `macro`; a decimal menu id for `command` |
| `Shortcut` | A Qt key sequence, such as `Ctrl+Alt+1`. Absent means none |
| `Confirm` | `on` to ask first. Anything else is off |

`Value`'s escape is Tera Term's own — the one `Answerback` and `DelimList` are
stored in. `$0D` is a Return, `$0A` a line feed, `$24` a literal `$`. Anything
the file cannot hold is written that way, and everything else is left legible.

The two settings beside them are ordinary `[Sterna]` keys: `QuickButtons`
(on) shows or hides the bar, and `QuickButtonsArea` (`top`, `bottom`, `left`,
`right`) is the edge it opens on.

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
