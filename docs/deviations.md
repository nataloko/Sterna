# Deliberate deviations from Tera Term

Everything else in this project is a transcription, and `AGENTS.md`'s traps are
the record of how carefully. This file is the other list: the places where
Sterna does something Tera Term does not **on purpose**, with the reason, so
that a future reader diffing the two programs can tell a decision from a bug.

The rule for being on this list: the divergence is user-visible, it is not
forced by the platform, and reproducing upstream instead would be strictly
easy. A divergence forced by Linux or by Qt is not a deviation — it is a port,
and it belongs in a comment at the code and in `AGENTS.md` if it bites.

Compatibility is unaffected in every entry below: no key changes meaning, and
a `TERATERM.INI` written by either program still opens correctly in the other.

| # | Deviation | Upstream | Since |
|---|---|---|---|
| 1 | The default baud rate is 115200 | 9600 | 0.2.0 |
| 2 | The connect dialog remembers the last connection, across restarts | Only Setup > Save persists anything | 0.2.0 |
| 3 | A bar under the menu: port, connect/disconnect, local echo, line edit | No toolbar at all | 0.2.0 |
| 4 | One, two or four simultaneous connection panels | One connection per window | 0.2.0 |
| 5 | Starting Sterna looks for a signed update, once a day | Nothing contacts a server on its own | 0.2.0 |
| 6 | Highlight rules: user-written regular expressions recolour the screen | Only the host decides a colour; the URL attribute is the one exception | 0.2.0 |
| 7 | Quick buttons: a second bar of user-defined commands | A `KEYBOARD.CNF` user key, with no face on it | 0.2.0 |
| 8 | Editable lines for every connection type | Telnet LINEMODE negotiation only | 0.2.0 |
| 9 | Receive CR defaults to Auto | A bare CR is the only default line ending | 0.2.1 |
| 10 | A terminal-only dark mode | Colours come only from `TERATERM.INI` and the host | 0.2.1 |
| 11 | The right button raises Tera Term's own paste menu | The same menu, behind a key that ships off, so the right button pastes at once | 0.2.4 |
| 12 | Settings-dialog changes can be saved automatically | Only Setup > Save setup persists them | 0.2.5 |

---

## 1. The default baud rate is 115200

`BaudRate`'s default is 115200 where `ttset.c:919` gives 9600.

**Why.** 9600 is the speed a serial console had when Tera Term chose it. The
equipment this program is pointed at — a router, a switch, a BMC, an embedded
board over an FTDI cable — ships 115200, and 9600 is now the exception that
gets typed in rather than the rule that gets left alone. A default nobody keeps
is a default that costs one dialog visit per install.

**What is unchanged.** The key, its absence of bounds, and what a value in the
file means. `BaudRate=9600` opens at 9600 in both programs; a file written by
either is read the same way by the other. Only the value used when the key is
absent differs.

**Where it lives.** `schema/settings.txt`'s `serial.baud` row, and
`SerialParams::default()` in `tt-conn`, which is what the C ABI's
`tt_serial_params_default` hands a frontend. The serial dialog reads the shipped
speed from the ABI rather than carrying a literal of its own, so there is one
place to change and it is the schema.

## 2. The connect dialog remembers the last connection

Opening File > New connection on any transport shows what was last connected
to — after a restart as well as within one run.

**Why.** Upstream's host dialog is seeded from `ts`, and `ts` reaches
`TERATERM.INI` only through Setup > Save. So Tera Term does remember a
connection for as long as the window lives and forgets it on exit, which for
the daily use this project is built for — the same console, several times a day
— means retyping a host or re-picking a port every morning. Sterna writes the
record when a connection actually opens.

**What is remembered.** The serial port's device path, its speed, data bits,
parity, stop bits and flow control; the SSH host, user, port, private key and
the pre-2020 algorithm switch; the telnet host, port and mode. Not the
passwords, and nothing a connection failed to make: for SSH that means the
record is written when the *handshake finishes*, not when it starts, so a host
whose key was refused or whose login failed does not become the next default.
An empty user or a zero port is remembered as such rather than resolved, because
blank means "whatever `~/.ssh/config` says" and that is not the same as empty.

**Where it lives, and the split that matters.** The serial *line* settings are
`[Tera Term]`'s own `BaudRate`, `DataBit`, `Parity`, `StopBit` and `FlowCtrl` —
updated in place, which is already what a `setbaud` in a macro does
(`Session::set_baud`: "a speed changed by a script must be the speed the dialog
shows"). Everything upstream has no key for is in a `[Sterna]` section that
nothing upstream reads. A second copy of a speed would have been a second
answer to which one wins, and `tt-config/tests/upstream.rs` asserts in both
directions that the two sections stay disjoint.

Only the keys that changed are written, the way `SaveVTPos` writes only the
window position on close — so an INI shared with a real Tera Term does not have
every other schema default pinned into it by a connection. A user who wants a
host forgotten clears the value; these are ordinary settings, visible in the
settings dialog's Recent page, not hidden state.

**One consequence worth naming.** `sterna --port /dev/ttyUSB0` with no `--baud`
opens at the remembered speed rather than at the shipped one. That is the point
of the feature, and it is also upstream's own rule for `/C=1`, which takes the
file's `BaudRate`.

## 3. A bar under the menu

Under the menu bar: the serial port as a dropdown, one button that opens or
closes the connection, and Local echo and Line edit check boxes. Tera Term has
no toolbar — its equivalents are a dialog, a menu item and settings pages.

**Why.** These are the connection and input modes that need to be visible while
working on a console port. Picking a port is File > New connection upstream,
closing is File > Disconnect, and local echo is three tabs into Setup. Line
edit is Sterna's own mode, described below. Nothing else is on the bar: it is
not a general toolbar.

**What is unchanged.** The bar decides nothing. Every widget on it is a view of
the session refreshed from the same status update the menu uses, and every click
calls the window method the menu item calls — so the port the bar shows is the
port that is open, the button says what the session is, and the check box is the
live `terminal.local_echo`, which a host's SRM and a macro can also change.
Line edit is likewise the active tab's `terminal.line_edit`; it makes the echo
box show the effective on state without changing the preference underneath.

**Where it lives.** `shell/src/ConnectBar.{h,cpp}`, and one new setting:
`[Sterna] Toolbar` (`window.toolbar`, on by default), which Setup > Show toolbar
writes. The switch exists because chrome nobody can remove does not belong in a
terminal; it is deliberately *not* tied to `PopupMenu` or `HideTitle`, which are
about the menu.

## 4. Simultaneous connection panels

View can show the active connection alone, two equal panels side by side, or
four equal panels in a 2x2 grid. The tabs are still the connections and remain
unlimited; the panel layout decides only which one, two or four are visible.
Hidden sessions keep running.

**Why.** Serial and network work is often comparative: two consoles during a
failover, or a switch, router and two attached hosts during a change. Separate
top-level windows hide the relationship and make the shared menu, macro and
transfer target ambiguous. Panels keep those sessions visible together while
one plainly highlighted pane remains the target of keyboard input and every
window-level action. Broadcast input is deliberately not part of the feature.

**What is unchanged.** A tab still owns exactly one independent `TerminalPage`
and therefore one session, viewport, printer, macro runner, plugin VM and
transfer. A connection that is not in a panel is hidden rather than suspended;
selecting its tab replaces the active panel without closing either session.
Closing, duplication, tab movement and `AutoWinClose` still operate on the tab,
not on a view of it.

**Where it lives.** `shell/src/PanelContainer.{h,cpp}` owns tab order and the
four visible slots; `MainWindow` continues to route its aliases through the
active `TerminalPage`. `[Sterna] PanelLayout=single|two|four`
(`window.panel_layout`) remembers only the layout. A restored multi-panel
window starts with its one ordinary terminal plus connection buttons in the
empty slots; it does not invent or restore sessions.

**Current exposure.** The implementation remains under test, but its View menu
is hidden in 0.2.1 while the interaction is refined. This keeps the work
available without presenting the unfinished layout controls in the ordinary UI.

---

## 5. Starting Sterna looks for a signed update

Three seconds after startup, and at most once every 24 hours, Sterna fetches a
signed release manifest — about 20 KB from the GitHub release page — and offers
the update if there is one. Tera Term contacts nothing on its own; its updates
are a download somebody goes and gets.

**Why.** Sterna ships its own signed updater, and a release nobody hears about
is a security fix nobody installs. Upstream can rely on the package manager or
the habit that put `ttermpro.exe` on the machine; an AppImage in `~/Downloads`
has neither, and Help > Check for Updates is a menu item nobody opens twice.

It is on by default for the same reason, and the cost of that is bounded on
purpose. It is the only thing this program sends anywhere without being asked;
the request goes to the release server and carries nothing but a `Sterna/x.y.z
updater` user agent. It is silent unless there is an update — no progress
dialog, no "you are current", and no complaint about a server it could not
reach, because a box on every launch is how people learn to turn a security
feature off. And it steps aside for a modal dialog, so an offer cannot land on
top of an SSH password prompt; a deliberately hidden `/V` run does not check at
all, because it has nowhere to put an offer without stalling unattended work.

**What is unchanged.** Everything about how an update is verified: the detached
Ed25519 signature over the manifest before its version, URL or size is trusted,
then the artifact's exact size, SHA-256 and its own signature before anything is
replaced or executed. The startup check is a schedule, not a second path — from
the offer onwards it is the code the button runs. Nothing is downloaded without
being agreed to, and turning the schedule off leaves the button working.

**Where it lives.** `MainWindow::checkForUpdatesOnStartup` decides, before
`sterna_updater` is loaded, so a session that is not due a check maps neither Qt
Network nor a TLS backend. `Updater::checkQuietly` is the silent half.
`updateCheckDue` in `shell/src/UpdateSchedule.cpp` is the once-a-day rule, and
two new settings hold it: `[Sterna] CheckUpdatesOnStartup`
(`updates.check_on_startup`, on) and `[Sterna] LastUpdateCheck`
(`updates.last_check`), written when a request goes out rather than when one
succeeds — so an offline machine costs one attempt a day rather than one per
launch. Both are ordinary settings, editable in Setup and by hand: clearing the
stamp means "check at the next start".

## 6. Highlight rules

An ordered list of regular expressions, each with a foreground colour, a
background colour and attributes, applied to what is on the screen. Tera Term
has no pattern or keyword highlighting anywhere: every colour a cell can take is
the host's decision, and the one exception — the URL attribute — is a hard-coded
scan for seven scheme prefixes rather than anything a user can write.

**Why.** On a console port the line that matters arrives in the same colour as
the thousand around it. Upstream's own regex library exists but lives in
`ttpmacro`, a separate process that never sees the screen, so there is nothing
to reproduce here and nothing to be compatible with. See
[`highlighting.md`](highlighting.md) for the user-facing half.

**What is unchanged, and it is the important half.** A rule changes what is
*drawn* and nothing about what the terminal is. Nothing is written into a cell:
the grid still holds exactly what the host sent, so the session log, the
clipboard, the printer, a macro's `wait` and the differential oracle all see an
unhighlighted terminal. Matching happens over the visible rows while they are
painted, which is what makes a new rule colour text that arrived before it —
scrollback included — and what keeps the receive path exactly as fast as it was.

The one place the drawing model is touched is `Theme::resolve`, and there the
rule applies **last**, after upstream's whole priority chain and after the
`UseTextColor` repair, so nothing can take back a colour the user asked for. It
goes through the same reverse flag an SGR colour does, so a selection dragged
across highlighted text still inverts. A rule's bold and underline reach the
font and the stroke but deliberately **not** upstream's bold and underline
*colour pairs* — "underline this" must not also mean "and repaint it magenta".

**Where it lives.** `crates/tt-config/src/highlight.rs` owns the format;
`crates/tt-session/src/highlight.rs` owns the engine and the per-row spans;
`shell/src/HighlightsDialog.{h,cpp}` is the editor. The rules are a
`[Sterna Highlights]` section — its own, because a list is exactly what the
settings schema cannot describe — plus one ordinary `[Sterna]` setting for the
master switch: `Highlighting` (`color.highlighting`, on).

**Two decisions worth defending.** The engine is the Rust `regex` crate rather
than the Oniguruma `tt-ttl` already carries, which costs one syntax difference
(no backreferences, no lookaround) that `highlighting.md` states plainly. It
buys a guarantee: matching runs on the UI thread inside `paintEvent` and the
*far end* chooses the haystack, so a backtracking engine would make a pattern
like `(\w+\s*)+:` a window somebody else can freeze. There is no retry limit to
choose and no rule that can stall the drawing. And rules compose **per channel**
in list order rather than first-rule-takes-the-cell: a rule that only underlines
and a rule that only colours are not in competition, and making them compete
would mean the more specific of the two silently did nothing.

## 7. Quick buttons

A second bar, holding commands the user defined: a label, and text to send, or
bytes, or a macro to run, or a menu command. Each may carry a keyboard shortcut
and may ask before it runs. Tera Term's nearest equivalent is a `KEYBOARD.CNF`
user key — the same four actions, on a key, with nothing on screen.

**Why.** On a console port the same handful of lines get typed all day, and
upstream's answer to that is a file somebody has to hand-edit in a format with
hexadecimal escapes in it. The gap is not the capability, which upstream has:
it is that nothing shows it, so almost nobody has one. See
[`buttons.md`](buttons.md) for the user-facing half.

**What is unchanged, and it is most of it.** A button *is* a user key: the four
kinds are `UserKeyType`, the escape is `Hex2StrW`'s, and pressing one calls the
same `Session::run_user_key` a mapped scan code calls — text through `send_text`
so `CRSend` and LNM apply, bytes raw, a macro started by the window, a menu
command dispatched by id. Nothing about how a command reaches the wire is new,
and a `KEYBOARD.CNF` full of user keys still works exactly as it did.

**Where it lives.** `crates/tt-config/src/buttons.rs` owns the format;
`shell/src/QuickButtonBar.{h,cpp}` is the bar and
`shell/src/QuickButtonsDialog.{h,cpp}` the editor. The list is a
`[Sterna Buttons]` section — its own, because a list is exactly what the
settings schema cannot describe — plus two ordinary `[Sterna]` settings for the
bar itself: `QuickButtons` (`window.quick_buttons`, on) and `QuickButtonsArea`
(`window.quick_buttons_area`, `right` — a terminal's rows are the scarce
dimension, so the bar goes where there is width to spare rather than where the
connect bar is).

**A button can repeat**: *n* sends every *x.x* seconds, or until stopped
(`Repeat` and `IntervalMs`). Upstream has nothing of the kind — the nearest
thing is a macro, which is a file, an interpreter and a window. The clock is
the frontend's (`shell/src/QuickButtonRepeat.{h,cpp}`) and deliberately not the
core's: the engine is a function of its bytes, so it records what a button was
asked for and schedules nothing, the same split the bell's governor makes. A
run stops on a second press, on Escape, from the bar's context menu, when the
list is edited, or when its link goes away, and it stays bound to the session
it started on rather than following the active tab.

**That Escape is the one key this program takes from the host without being
asked to** — and only while a run is going, which is the whole of the
justification. It is the same bargain a shortcut makes (see below), made
temporarily and only when there is something a person is likely to want
stopped in a hurry; `TerminalView::setStopKeyArmed` is where it is claimed and
released, and nothing arms it when nothing is repeating.

**Two decisions worth defending.** The bar does not exist until a button does,
where YAT — the program this borrows the idea from — shows twelve
`<Define...>` placeholders; discoverability is worth a menu item, not permanent
chrome in a terminal whose claim is being light. And no button ships with a
shortcut, where YAT binds `Shift+F1`..`F12`: a Qt action outranks the terminal
widget, so a shortcut is a key the host silently stops receiving, and here
`Shift+F1` is both a legitimate `KEYBOARD.CNF` binding and F13 to the far end.
The editor warns about a sequence the menu, a plugin, the key map or the host
is already using — and warns rather than refuses, because the user knows what
is on the other end and this program does not.

## 8. Editable lines for every connection type

Line edit keeps ordinary printable typing in a small editor at the live
terminal cursor. Backspace, Delete, word movement, selection and the editor
shortcuts Ctrl+A/X/Z/Y operate locally; Return sends the finished line with the
terminal's configured Return sequence. A multi-line paste is a queue of those
same lines, and each stays local until its own Return. Other control keys,
function keys, `KEYBOARD.CNF` mappings, macros, transfers and protocol replies
remain immediate.

**Why.** Serial consoles and command-oriented appliances often need a whole
command corrected before any byte is sent, and telnet's negotiated LINEMODE is
not available on serial, SSH, raw TCP or a local shell. Treating the feature as
a frontend editor gives all connection types the same useful behaviour without
turning the transport into a global transmission gate.

**What is deliberately separate.** `[Tera Term] EnableLineMode`
(`connection.line_mode`) still controls upstream's telnet LINEMODE negotiation
and keeps its original meaning and default. This feature is `[Sterna]
LineEdit=on|off` (`terminal.line_edit`), off by default. While it is active the
accepted line is echoed exactly once without assigning `LocalEcho` or SRM, so
turning it off restores the tab's previous Local echo preference. Drafts belong
to a tab, survive panel and tab switches, and are discarded when that
connection ends or is replaced.

**Where it lives.** `TerminalView` owns the editor, queue and discard prompt;
`ConnectBar` exposes the active tab's switch. `Session::send_edited_line` owns
Return encoding and the one forced echo, through the flat ABI function
`tt_session_send_edited_line`. The setting remains in the common schema, so
Setup, Save setup, TTL/Lua, plugins and duplicated sessions all see the same
value rather than a frontend-only copy.

## 9. Receive CR defaults to Auto

With no `CRReceive` key, Sterna treats CR, LF and CRLF as line endings. Tera
Term defaults to `CR`, where a received LF moves down without returning to the
left margin and a bare CR returns without moving down.

**Why.** Serial devices are split across all three spellings, and the wrong
answer makes the first screen look diagonally shifted or overwrite itself.
Auto recognises the pair without double-spacing it and gives a useful first
connection to each device; an unusual host can still select any exact mode.

**What is unchanged.** The existing `[Tera Term] CRReceive` key and every
explicit value keep their upstream meaning. `CRReceive=CR` therefore restores
Tera Term's default exactly, and a shared INI behaves the same in both programs.
Only the answer when the key is absent — or unrecognised, as upstream's enum
rules require — differs.

**Where it lives.** `tt-config/schema/settings.txt` owns the shipped setting and
`tt-session` maps it into the engine. The bare `tt-vt::Config` retains the
upstream `CR` default for focused compatibility callers; the application and
its flat ABI construct sessions from `tt-config`, so they receive Auto. Oracle
and differential runners remain on `CR`, keeping the compatibility baseline
independent of the product default.

## 10. A terminal-only dark mode

The connection bar can switch every terminal grid in the window between the
configured Tera Term colours and a dark palette. The setting is remembered
immediately, so new tabs and the next launch follow it.

**Why.** The upstream black-on-white default is useful in a light desktop but
hard on the eyes in a dark workspace. A whole-application theme would replace
the user's Qt/desktop theme and make dialogs less native; only the terminal's
large reading surface needs a separate choice.

**What is unchanged.** Cell contents, ANSI colour indices, logging, copying and
printing are untouched. Explicit SGR foregrounds and backgrounds still win;
the dark palette supplies only defaults and the bold, blink, underline, URL,
reverse and cursor pairs. Turning it off reads the live configured colours back
from the session, including host OSC changes.

**Where it lives.** `[Sterna] DarkMode` (`terminal.dark_mode`, off) is applied
by `Theme` after it reads the session's live colours. `ConnectBar` owns the
right-aligned moon/sun action; `MainWindow` applies it to every tab and
remembers the one window-wide preference. No `QApplication` or widget palette
is changed.

## 11. The right button raises the paste menu

A right-click over the terminal opens upstream's own two-item menu — Paste and
Paste&lt;CR&gt; — instead of putting the clipboard on the wire immediately.

**Why.** The menu is Tera Term's, and so is the mechanism: `IDR_PASTEMENU`
(`vtwin.cpp:912`, `:1317`), behind `ConfirmPasteMouseRButton`. Only the shipped
value of that key moves. A right button that pastes the instant it is pressed
has no undo and no preview, and the button is one an X11 user reaches for
expecting a context menu; the cost of getting it wrong is a command running on
a router. Upstream's own default predates that expectation, and the key exists
precisely because upstream thought the immediate paste was worth a way out.

This is the **only** setting in `schema/settings.txt` whose default this port
moves for a reason other than hardware — the baud rate above is the other, and
that one is about equipment rather than about a gesture.

**What is unchanged.** Everything the key means. `ConfirmPasteMouseRButton=off`
gives upstream's straight-to-the-wire right button back, `=on` is what both
programs do with the menu, and `DisablePasteMouseRButton=on` still takes the
button out of the clipboard's business altogether — the menu is a replacement
for that paste, so a right button that was not going to paste does not grow
one. The menu's conditions are upstream's, condition for condition: connected,
no file transfer holding the line, and something on the clipboard.

Paste&lt;CR&gt; itself is upstream's `ID_EDIT_PASTECR` and is not a deviation —
it is also in the Edit menu, and `KEYBOARD.CNF`'s `EditPasteCR` already named
it. The added Return joins the text where `clipboar.c:280` puts it, after the
bracket decision and before the CR normalisation, so `BracketedControlOnly` and
a clipboard that already ends in a newline behave as they do upstream.

**Where it lives.** `schema/settings.txt`'s `clipboard.confirm_paste_rbutton`
row, named in `tt-config/tests/upstream.rs`'s `DEFAULTS_MOVED_ON_PURPOSE` so
the fidelity test reports it as a decision rather than an accident.
`TerminalView::pasteMenuWanted` is the condition and `MainWindow::showPasteMenu`
is the menu, which borrows the Edit menu's two actions rather than building
copies that would drift.

## 12. Optional automatic settings saves

The first time Setup opens for an INI file, Sterna asks whether changes accepted
with the Settings dialog's OK button should also be written to that file. Manual
saving is the default answer. The choice is visible later as `[Sterna]
AutoSaveSettings` (`settings.auto_save_changes`) on the Settings page, and an
explicit answer is recorded immediately so the same file is not asked about
again.

**Why.** Applying a dialog and saving it are separate operations in Tera Term.
That is useful for experiments, but it also makes an ordinary permanent change
easy to lose at exit. Sterna offers the familiar application behaviour without
silently imposing it on a shared setup file.

**What is deliberately narrow.** Automatic saving writes only schema or plugin
rows that the accepted dialog changed successfully. It preserves comments,
ordering, unknown keys and defaults the user did not touch; it does not create
`IniAutoBackup` backups. Cancel writes nothing, and toolbar controls, scripts,
commands, live echo/line-edit state and other changes outside the dialog keep
their existing persistence rules. The option's final value controls its sibling
changes from the same OK, while a change to the option itself is always saved.
Setup > Save setup remains the full, backed-up save.

**Failure behaviour.** A write error is reported after the live changes have
been applied; those changes are not rolled back. If the first answer itself
cannot be recorded, that window suppresses a repeat prompt, but the next launch
asks again because the core INI parser still sees the key as absent.
