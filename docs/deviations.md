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
| 3 | A bar under the menu: destination, connect/disconnect, local echo, line edit | No toolbar at all | 0.2.0 |
| 4 | Tiled connections, and one status line per terminal | One connection per window, one status bar | 0.2.0 |
| 5 | Starting Sterna looks for a signed update, once a day | Nothing contacts a server on its own | 0.2.0 |
| 6 | Highlight rules: user-written regular expressions recolour the screen | Only the host decides a colour; the URL attribute is the one exception | 0.2.0 |
| 7 | Quick buttons: a second bar of user-defined commands | A `KEYBOARD.CNF` user key, with no face on it | 0.2.0 |
| 8 | Editable lines for every connection type | Telnet LINEMODE negotiation only | 0.2.0 |
| 9 | Receive CR defaults to Auto | A bare CR is the only default line ending | 0.2.1 |
| 10 | A terminal-only dark mode | Colours come only from `TERATERM.INI` and the host | 0.2.1 |
| 11 | The right button raises a Copy/Paste context menu | Tera Term's two-item paste menu, behind a key that ships off, so the right button pastes at once | 0.2.4 |
| 12 | Settings-dialog changes can be saved automatically | Only Setup > Save setup persists them | 0.2.5 |
| 13 | The terminal background is a different shade while nothing is connected | The background is what the file and the host say, always | 0.2.6 |
| 14 | A bare host name means SSH | It means telnet | 0.2.0 |
| 15 | Disconnecting does not close the window | `AutoWinClose` closes it however the line ended | 0.3.0 |
| 16 | A port another program holds is greyed, not hidden | The row is hidden, and only other Tera Term windows count | 0.3.0 |
| 17 | The log dialog also asks about rotation | Rotation is on the Setup page only | 0.3.2 |
| 18 | A log names itself by the clock and remembers its directory | `teraterm.log`, in whatever directory the settings resolve to | 0.3.2 |
| 19 | Edit > Find searches the screen and the scrollback | Nothing searches the buffer; the log and another program do | 0.3.2 |
| 20 | An optional column of line numbers down the left of the terminal | Nothing numbers the lines | 0.3.2 |
| 21 | The control characters and the line endings on the wire can be shown in the terminal | Only debug display mode shows a control byte, and it stops emulating to do it | 0.5.5 |

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

**What is remembered.** Two things, and they are not the same thing. One
record per transport seeds the connect dialog: the serial port's device path,
its speed, data bits, parity, stop bits and flow control; the SSH host, user,
port, private key and the pre-2020 algorithm switch; the telnet host, port and
mode. Beside them, since 0.2.6, a *list* of the last ten connections opened —
`[Sterna] Recent` — which is what the toolbar's dropdown offers. A list has to
say which kind each entry is and carry that entry's own parameters, because a
serial console at 9600 and a router at 115200 are two lines in it; each record
holds exactly the fields the dialog asks for and no more, so everything else
still comes from the settings and a setting that changes changes for a
remembered connection too. `RememberConnections=off` stops adding to it and
leaves what is there. Not the
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

Under the menu bar: where to connect as one editable dropdown, one button that
opens or closes the connection, and Local echo and Line edit check boxes. Tera
Term has no toolbar — its equivalents are a dialog, a menu item and settings
pages.

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

**What the field takes.** The dropdown is four groups: the connections
actually opened (newest first, with their own parameters), the serial ports
plugged in at the moment the list opens, the hosts in `~/.ssh/config`, and a
local shell — then New connection and Forget. Typing is the escape hatch and
inherits that kind's last parameters rather than carrying its own: `myrouter`,
`ssh://user@host:22`, `telnet://host:2323`, `/dev/ttyUSB0`, `COM3`, `shell`.

**Choosing a row fills the field; Connect is what connects.** A combo's popup
opens *under the pointer*, so the release that opened it lands on a row and
Qt reports a choice nobody made — one click on the arrow would otherwise dial
a host. The second click is the price of that, and it buys something as well:
the destination can be read before it is committed. A row picked out of the
list stays a *record* until then, so pressing Connect opens it with the
identity and the flags it carries rather than re-reading its own label, which
has spaces in it and would parse as a command line. That record lasts exactly
until the next thing anybody says — choosing another row or typing over it —
because the row the popup picked on its way open is a row the user did not
choose, and it must not outlive the one they did.

**One word or a command line, and never half of each.** A destination with a
space in it is handed to Tera Term's parser whole, which is how
`/ssh /auth=publickey myrouter` works in the field, and it is the same switch
`sterna`'s own command line makes when it sees a `/OPTION`. The reason the two
cannot be merged is deviation 14: a bare host name is SSH in this program's
vocabulary and telnet in Tera Term's, so a line is read one way or the other.

**A live session is not closed by going somewhere else.** Picking or typing a
destination while a connection is open puts the new one in a new tab or tile;
Disconnect is the only thing that closes what is there. The port list this
replaced could not raise the question, because it greyed itself out whenever a
session was live.

**Where it lives.** `shell/src/ConnectBar.{h,cpp}`, `shell/src/Recent.{h,cpp}`
for the records, and one new setting:
`[Sterna] Toolbar` (`window.toolbar`, on by default), which View > Show toolbar
writes. The switch exists because chrome nobody can remove does not belong in a
terminal; it is deliberately *not* tied to `PopupMenu` or `HideTitle`, which are
about the menu.

## 4. Tiled connections, and one status line per terminal

View offers **Tiled**. With it off, this window shows one connection and a tab
bar over the rest, which is the ordinary case. With it on, **the tab bar is
gone and every connection has a tile**: the grid is the smallest square-ish
rectangle that holds them — one, two side by side, 2x2, 2x3, 3x3, and onwards
— and when the rectangle does not come out even the last cell carries Serial,
SSH, Telnet and Local shell buttons, widened to fill the rest of its row. At
one, two, four, six and nine connections it comes out even and there is no
such cell; File > New connection is the route then, and it re-tiles.

**Why.** Serial and network work is often comparative: two consoles during a
failover, or a switch, router and two attached hosts during a change. Separate
top-level windows hide the relationship and make the shared menu, macro and
transfer target ambiguous. Tiles keep those sessions visible together while
one plainly marked terminal remains the target of keyboard input and every
window-level action. Broadcast input is deliberately not part of the feature.

**The two are exclusive, and that is the change from 0.2.x.** Panels were
previously a view *onto* the tabs: a tiled window had a tab bar as well, and a
connection past the fourth went on running where nobody could see it and could
only be reached by evicting a visible one. There were two answers to "where
are my connections". Now tiles *are* the connections — no hidden session, and
the tile order is the tab order, so dragging a tab decides which tile a
connection gets.

**Each terminal carries its own status line**, along its own bottom edge: what
the connection is called, whether it is up, and its `REC` counter while it is
logging, plus anything that terminal has to say — a transfer's result, a
macro's or a plugin's complaint. The window has no status bar of its own.
With one terminal it sits where a status bar would, so nothing looks different;
with several it is the only arrangement that can say which session a fact is
about. The `REC` counter blinks red while its log is open, keeping a recording
that was left running visible even when no new bytes move its count. It is also
the active-tile marker, so a tile has one row of chrome rather than a title
above and a status below.

**What is unchanged.** A tile still owns exactly one independent
`TerminalPage` and therefore one session, viewport, printer, macro runner,
plugin VM and transfer. Closing, duplication, tab movement and `AutoWinClose`
still operate on the connection, not on a view of it.

**Where it lives.** `shell/src/PanelContainer.{h,cpp}` owns the order and the
grid; `shell/src/PageStatusBar.{h,cpp}` is the strip, owned by its
`TerminalPage`; `MainWindow` continues to route its aliases through the active
page. `[Sterna] PanelLayout=single|tiled` (`window.panel_layout`) remembers
only the mode. **A 0.2.x file saying `two` or `four` opens tiled** and is
rewritten as `tiled` the first time the layout changes; `[Sterna]` is a section
nothing upstream reads, so no compatibility promise is affected. A restored
tiled window starts with its one ordinary terminal; it does not invent or
restore sessions.

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
`shell/src/QuickButtonsDialog.{h,cpp}` the editor. The list is a `[Sterna Buttons]` section — its own, because a list is
exactly what the settings schema cannot describe — plus two ordinary `[Sterna]`
settings for the bar itself: `QuickButtons` (`window.quick_buttons`, on) and
`QuickButtonsWidth` (`window.quick_buttons_width`, `0` for as wide as the
buttons need). The visibility setting applies even to an empty list: the panel
then consists of the Add button, so the feature can be discovered and its first
command defined without going through Setup.

**The panel is fixed to the right and its width comes out of the window.** A
terminal's rows are the scarce dimension, so it goes where there is width to
spare rather than where the connect bar is; there is no setting for the other
three edges because there is no dock to drag it to one. It shared a
`QDockWidget` with the terminals until 0.5.4, and a dock separator divides the
client area — so every pixel given to the panel came off a terminal that is
fitted to what is left in whole cells, and `Grid::resize` truncates every line
it shortens in the page *and* the scrollback. `MainWindow::resizeQuickPanel`
moves the window's outer edge instead, and stops where the window can no longer
grow. A change to some chrome is not one anybody expects to destroy text.

**And there is no handle**, which is a decision. One was built: with the
window's left edge pinned, growing the panel by N grows the window by N, so the
handle's screen position never moves and the window's far edge shoots out —
the truthful rendering of the rule and nothing like what a splitter feels like.
Making it feel right wants a rubber band that follows the pointer, or a window
that grows leftward, and `QWidget::move()` is silently ignored on Wayland. The
panel's own context menu carries **Panel width > Fit to buttons / Set width…**
instead, and reaches places the handle could not: a maximised window, the
keyboard, a macro.

**A button shortens its caption rather than holding the panel open.**
`QToolButton` refuses to be narrower than its own text and says so through
`minimumSizeHint`, which is the panel's minimum too — so the longest label on
the bar decided how narrow the whole panel could be, and one wordy button
widened the other nine. `BarButton` drops that demand and elides at paint time;
`text()` keeps the real caption, so the tooltip, the editor and the settings
file are unaffected and only the pixels are short. The floor is then a fixed 48
pixels — a target a pointer can hit — rather than a number that moved whenever
somebody renamed a button.

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

## 9. Receive CR defaults to Detect, a mode of Sterna's own

With no `CRReceive` key, Sterna works out which of CR, LF and CRLF the far end
means from the first line ending it sends, and behaves as that exact mode from
then on. The spelling is `CRReceive=DETECT`, and it is a fifth value beside
upstream's four. Tera Term defaults to `CR`, where a received LF moves down
without returning to the left margin and a bare CR returns without moving down.

**Why.** Serial devices are split across all three spellings, and the wrong
answer makes the first screen look diagonally shifted or overwrite itself.
Detect recognises the pair without double-spacing it and gives a useful first
connection to each device; an unusual host can still select any exact mode.

**Why it is not upstream's own AUTO.** Tera Term has a fourth value that sounds
like this one (`vtterm.c:727`, "AUTO CR/LF mode", 2012): a CR or an LF generates
CR+LF, and the opposite immediately after it is ignored. It never stops guessing,
so a bare CR is a line ending for the whole session — and a bare CR is a cursor
motion far more often than it is a line ending. An interactive shell redrawing
its prompt sends one per keystroke, so under AUTO every keystroke lands on a new
line; a progress bar walks down the screen. It is upstream defect 38.

**How the decision is made.** The evidence is the first LF: with a CR
immediately before it the far end spells its endings `CR LF` and the mode
becomes `CR`, the reference default; with anything else before it the far end
spells them `LF` and the mode becomes `LF`. A device that sends CR and never LF
— the case the mode exists for — never resolves, so its CR goes on breaking the
line. The decision belongs to the connection: opening a new one puts it back to
undecided, because the next far end need not agree with the last.

**What is unchanged.** The existing `[Tera Term] CRReceive` key and every
upstream value keep their upstream meaning, `AUTO` included — a file saying
`CRReceive=AUTO` renders identically in the two programs, defect and all, and
differential case 33 is that comparison. `CRReceive=CR` restores Tera Term's
default exactly. Only the answer when the key is absent — or unrecognised, as
upstream's enum rules require — differs. Tera Term reading a file that says
`DETECT` takes its own `else`, which is `CR`.

**Where it lives.** `tt-config/schema/settings.txt` owns the shipped setting,
`tt-session` maps it into the engine and clears the decision when a connection
opens, and `tt-vt::State::cr_mode` is the resolution itself. The bare
`tt-vt::Config` retains the upstream `CR` default for focused compatibility
callers; the application and its flat ABI construct sessions from `tt-config`,
so they receive Detect. Oracle and differential runners remain on `CR`, keeping
the compatibility baseline independent of the product default.

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

## 11. The right button raises a Copy/Paste menu

A right-click over the terminal opens Copy followed by upstream's Paste and
Paste&lt;CR&gt; commands instead of putting the clipboard on the wire immediately.
Copy is greyed out when there is no selection; both paste commands are greyed
out when there is nothing they can send.

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

**What is unchanged.** With no selection, `ConfirmPasteMouseRButton=off` gives
upstream's straight-to-the-wire right button back, and
`DisablePasteMouseRButton=on` takes the button out of the clipboard's business.
The paste commands use upstream's availability conditions, condition for
condition: confirmation-menu mode is on, the session is connected, no file
transfer holds the line, and the clipboard contains text.

**What Copy adds.** A standing selection can raise the menu independently of
those paste conditions, including on a disconnected session or with an empty
clipboard; the paste pair is disabled there. Conversely, when paste makes the
menu available without a selection, Copy remains present but disabled. The
menu borrows Edit's real actions rather than duplicating their translated text
or shortcuts, and restores their ordinary enabled state as soon as it closes.

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

**The View menu does not wait for it.** Tiled, Show toolbar, Show quick
buttons, Show line numbers and Highlight matches write their one key to the
settings file as they are ticked, whatever this option says. A dialog change is
provisional until its OK button, which is what makes an automatic save a
question worth asking; a menu tick has no OK button behind it, and a switch
that forgets what it was set to at the next launch is not a switch. Each writes
only its own key (`MainWindow::setViewSwitch`), so a shared file keeps every
other line it had.

**Failure behaviour.** A write error is reported after the live changes have
been applied; those changes are not rolled back. If the first answer itself
cannot be recorded, that window suppresses a repeat prompt, but the next launch
asks again because the core INI parser still sees the key as absent.

---

## 13. An idle terminal is a different shade

While a terminal has no connection, the background of every cell the host did
not colour is painted `color.disconnected_shade` percent of the way from
`color.normal`'s background towards its foreground — 12 percent by default, and
`0` turns it off. Nothing else changes: the text keeps its colour, and a cell
the host or a highlight rule coloured keeps the colour it was given.

**Why.** Tera Term's window is one session, and closing the connection usually
closes the window, so "is anything on the other end" is rarely a question there.
Here one window holds several sessions and a tiled window shows them at once,
where a terminal whose device has gone is otherwise indistinguishable from one
that is merely quiet — the last screenful is still on it. The status line under
each terminal already says so in words; this is the same fact at a glance,
across a grid, without reading.

**Why towards the foreground.** A `#000` background cannot be darkened and a
`#fff` one cannot be lightened, and `QColor::lighter` scales the HSV value, so
on the commonest terminal theme of all a factor would have produced no shade at
all. The configured foreground is the one colour someone choosing a theme has
guaranteed is visible against the background. A consequence worth knowing: a
cell already painted in the foreground colour — a reversed one, or the whole
screen under DECSCNM — does not move, because it is already at the far end of
that blend.

**What is unchanged.** The key is `[Sterna] DisconnectedShade`, which upstream
neither reads nor writes, and nothing else here has a `TERATERM.INI` meaning.
The shade lives in the painter alone: the grid, the session log, a macro's
`wait` and every report a host can ask for are untouched, so nothing on the
wire can tell whether it is applied.

## 14. A bare host name means SSH

`sterna myrouter` opens an SSH session, and so does typing `myrouter` into the
connect bar. `ttermpro myrouter` opens telnet.

**Why.** The token is what somebody would type after `ssh`, including
`user@host:port` and an alias out of `~/.ssh/config`, and a terminal shipped in
2026 whose bare default is an unencrypted protocol is a terminal that will one
day send a password in the clear because a habit carried. Upstream's default
predates that being unacceptable; `/T=1`, `telnet://` and Tera Term's own
command line all still reach telnet deliberately.

**What is unchanged.** Tera Term's command line is read by Tera Term's rules,
whole — `sterna /ssh myhost` and `sterna myhost` are different vocabularies and
a `/OPTION` anywhere switches between them, in the connect bar's field as much
as in `argv`. So a converted shortcut behaves as it did.

## 15. Disconnecting does not close the window

`AutoWinClose` closes a network window when the connection ends. Here it applies
only to a connection that ended on its own — the far end hanging up, the socket
dropping. Choosing Disconnect, or a macro or `ttctl` asking for one, leaves the
window open with nothing connected.

**Why.** Upstream's Disconnect posts the same `FD_CLOSE` a lost line does
(`vtwin.cpp:4462`, into the `IdComEndTimer` arm at `:3023`), so the setting
cannot tell them apart. Sterna's window outlives its connections by design: the
connect bar is above the terminal offering the next one, the recent list holds
what was opened before, and a window with one session is the whole application.
Quitting because somebody hung up puts the button that reconnects on a window
that no longer exists, and takes the scrollback of what just happened with it.

**What is unchanged.** A far end that closes the connection still closes the
window, which is the case the setting was written for and what every terminal
emulator does when a shell exits; `AutoWinClose=off` still keeps the window in
that case too. `ClearScreenOnCloseConnection`, the connect beep and the borrowed
`TCPLocalEcho`/`TCPCRSend` values are applied by both paths exactly as before,
and `ConfirmDisconnect` still asks first.

**Where it lives.** `tt-session::Session::connection_closed` takes the one
`asked` flag; `Session::disconnect` is the only caller that passes it.

## 16. A port another program holds is greyed, not hidden

The connect bar's dropdown shows a serial port something else has open as a
disabled row that names the holder — `/dev/ttyUSB0 — FT4232H (in use by
minicom)`. Tera Term answers the same question and *hides* the row instead
(`hostdlg.c:180`, "使用中のポートは表示しない").

**Why.** A port that vanishes from the list looks like an adapter that came
unplugged, and the user goes to check the cable; a greyed row naming `minicom`
tells them what to close. Hiding also moves the rows underneath, and the
remembered connections carry their index as their payload — greying removes
nothing, so the row above a hidden one would have become the wrong record.

**What it can see, and what it cannot.** Linux is asked twice: `/proc/locks`
names every holder that took an `flock`, whatever user it belongs to — which
includes every Sterna window, since `serialport-rs` locks as well as setting
`TIOCEXCL` — and a sweep of `/proc/<pid>/fd` names this user's own processes
whether they locked or not. Windows has no non-destructive question to ask the
system at all, so it sees only what this program's other windows publish, which
is exactly Tera Term's own reach: a claim file beside each window's control
socket, read back through the endpoint list so a crashed window stops claiming
its port. Nothing opens a device to find out — opening raises DTR for the life
of the probe, which reboots an Arduino-style board.

A root-owned holder that took no lock is invisible to both Linux sources, and a
holder that took no exclusive lock at all does not in fact stop the open. So
this is **advice, not a gate**: the field still accepts a typed path, Connect
stays live, the New Connection dialog lists every port, and the error on the
connect path — `is in use by another program` — is still where the truth lives.

**Where it lives.** `tt_conn::serial::holders` asks the kernel,
`tt_ctl::claim` is the published half, `tt_serial_holders` unions them, and
`ConnectBar::rescanBusy` asks as the dropdown opens — never on the connect
path, which `setRecents` also reaches.

---

## 17. The log dialog also asks about rotation

`File > Log...` carries `LogRotate`, `LogRotateSize`, `LogRotateSizeType` and
`LogRotateStep` alongside the mode and timestamp questions. Tera Term's
`IDD_LOGDLG` has none of them; they live on Setup > Additional settings > Log
(`log_pp.cpp`), and the log dialog is start-time options only.

**Why.** Whether a capture should roll over is a fact about the capture, not
about the installation. A week on a router's console and ten seconds of a boot
log want different answers, and the moment somebody knows which one they are
starting is the moment the dialog is open. Sending them to a settings page
first — and back afterwards, to undo it — is the sort of trip that ends with
one enormous file.

The other three controls the Setup page has and the log dialog does not
(`LogDefaultName`, `LogDefaultPath`, `LogAutoStart`) stay where upstream keeps
them: those really are about the installation.

**What is unchanged.** The four keys, their meanings and their traps. The size
is stored in bytes whatever the unit combo says — the dialog multiplies on the
way in and divides on the way out, exactly as `log_pp.cpp:156` does — and a
`LogRotateStep` of zero is still upstream's ten thousand generations rather
than none, which the spin box's zero says out loud instead of leaving to be
discovered. The Setup page still edits the same keys, and a file written by
either program still opens in the other.

---

## 18. A log names itself by the clock and remembers its directory

`LogDefaultName` ships as `sterna-%Y%m%d_%H%M%S.log` where `ttset.c:1018` gives
`teraterm.log`, and the directory the last log was written to is kept in
`[Sterna] LogDir` and used when `LogDefaultPath` is empty. Upstream remembers
nothing: `GetTermLogDir` answers the same three-way question every time.

**Why the name.** One fixed name means every log lands on the last one, and
whether that overwrites it or appends to it is decided by `LogAppend` — a
setting the person starting the log is not looking at. Both outcomes are bad in
different ways and neither is visible until afterwards. A template that carries
the clock cannot collide, and the shape is not an invention: it is one of
upstream's own Setup presets (`log_pp.cpp:125`). The program's name goes in
front of it because a log directory is rarely one program's — a bare
`20260815_143022.log` beside the same name from something else is a file you
have to open to identify.

**And without the `&h` that preset has**, which is the tempting part for
anybody logging more than one console. `&h` is `ts.HostName`, which here is the
path the port was *opened* by: the connect bar opens a serial port through its
`/dev/serial/by-path/` name so that a replug cannot move it, and the sweep for
characters a file name cannot hold then turns
`pci-0000:c8:00.3-usb-0:1.3.2:1.0-port0` into thirty-eight characters of
underscores on the end of every log. A local shell has no host name at all and
would leave a bare separator. Both are fine in a template somebody chose, and
`&h`, `&p` and `&u` all still work; neither is fine in the one everybody gets.

**Why the directory.** The same reason the connect dialog remembers its last
connection (deviation 2): with nothing configured, upstream's answer is
`GetTermLogDir`'s chain, which ends in a per-user directory nobody chose and
few people could name. A directory somebody browsed to is a better answer to
"where do logs go" than that is.

**But only when nothing is configured.** `LogDefaultPath` wins whenever it is
set: somebody who named a log directory has said where logs go, and a
remembered one that silently overrode it would make the setting look broken
with nothing on screen to explain why. What the memory replaces is the *rest*
of the chain, not the setting. It is recorded either way, so clearing
`LogDefaultPath` later falls back to somewhere real rather than to the per-user
directory again; and only the dialog writes it — a `/L=` path or an
auto-started template is a script's choice and must not retarget the next log a
person opens.

**What is unchanged.** Every key, every rule and every expander. The name is
still a template put through `strftime`, then `&h`/`&p`/`&u`, then the sweep
for characters a file name cannot hold; a file that sets `LogDefaultName` keeps
whatever it says, `teraterm.log` included. `[Sterna]` is this program's own
section and nothing upstream reads it.

---

## 19. Edit > Find searches the screen and the scrollback

Ctrl+Shift+F opens a bar over the bottom of the terminal: a pattern, case,
whole-word and regular-expression switches, previous and next, and a count.
Tera Term has nothing of the kind — the way to find something in a Tera Term
buffer is to log the session and search the file in another program.

**Why.** That workaround loses both halves of what somebody wants. It loses the
context — you find the line and are now looking at a text editor rather than at
the terminal it came from — and it loses the ability to go *there*, scroll
around it, and copy it. Every other terminal on either platform has this, and a
console session is exactly the kind of text people need to search: you scrolled
past the error four minutes ago and you want it back.

**Why Ctrl+Shift+F and not Ctrl+F.** A `QAction` shortcut silently outranks
`TerminalView::keyPressEvent`, so every shortcut this window installs is a key
the far end stops receiving — permanently, in every session, whether or not the
bar is open. `^F` is forward-a-character in readline and a page forward in vim
and less, which makes it one of the worst keys in the set to take. Ctrl+Shift is
the bargain Copy and Paste already make here for the same reason, so the
terminal's own answer to "what does Ctrl+letter do" is unchanged: it goes to the
host.

**Why the bar floats over the terminal rather than sitting under it.** A bar in
the page's layout would take a row from the grid, which is a resize — and a
resize sends a scrolled-back view live, drops the selection, and moves
`TerminalSize` with it, because upstream's `ts.TerminalWidth`/`Height` are live
variables that `BuffChangeTerminalSize` assigns (`buffer.c:5022`). So *closing*
the bar would throw away the position you had just searched to, which is the one
thing a find feature must not do. Floating costs the bottom row of view while
the bar is open and nothing at all when it is closed, and it works in a tiled
window, where growing the window to make room is not available.
`find_test.cpp`'s `test_the_bar_does_not_resize_the_terminal` is that argument
as an assertion.

**The current match is the selection.** Not a fourth kind of coloured text: it
is scrolled to, painted, and copied by machinery that already exists, and
Ctrl+Shift+C after a search takes the match. The *other* matches are painted in
`[Sterna] FindColor`, and they are the only thing the feature adds to the
painter.

**And they are painted by the same engine the highlight rules use** (deviation
6), over the same logical lines — so `^` and `$` mean the same thing in a find
field as in a rule, a match that straddles a soft wrap is found once and shown
on both rows, and the pattern syntax needs describing once. What Find does *not*
share is `Highlighting`: a search that painted nothing because View > Highlight
matches happened to be off would be undiagnosable from the screen.

**What it costs when it is closed:** one comparison per row painted, and
nothing on the receive path — matching happens while drawing, which is what
lets a pattern typed now find text that arrived an hour ago.

**Compatibility.** `FindColor`, `FindHistory` and the three switches are keys in
`[Sterna]`, this program's own section. No real Tera Term reads it, and a
settings file shared with one still opens correctly in both.

---

## 20. An optional column of line numbers down the left of the terminal

`terminal.line_numbers` puts a gutter beside the terminal, numbering each
visible row; `terminal.line_number_width` says how many digits it reserves.
Both are `[Sterna]` keys, both land on the Terminal tab of Setup, and the
switch is also View > Show line numbers.

**Why it is worth a deviation.** What this program gets pointed at is a
console: a switch, a bootloader, a device that answers in walls of text. "The
error is about forty lines back" and "read me the line after the banner" are
how people talk about that output, and with nothing numbering it the only way
to act on either sentence is to count with a finger on the screen. It costs a
few columns when it is on and nothing at all when it is off, which is how it
ships.

**The number is the absolute session line, counted from 1.** Line 1 is the
first line the host printed, and a line keeps its number as it scrolls up into
history — so a number somebody writes down stays true for as long as the
session does. That is not a new idea in this code: it is `Grid::scrolled_off`,
which the selection already uses for the same reason, so that a highlight
survives the output scrolling underneath it. The gutter reads it through
`Session::lineAt` and adds one, because the core calls the first line zero and
nothing a person uses does.

**Unless somebody restarts the count, which View > Reset line counter does.**
The mark it sets is one line *below* the cursor, so the next line the host
prints is line 1 — a counter is reset before the thing it is going to count,
and at a prompt the line you are standing on is the prompt, not the output you
are about to ask for. That is the sentence the item is for: reset, run the
command, and its first line of output is 1.

Everything printed before the mark then carries no number at all, rather than a
zero or a negative one. It was printed before there was a counter to count it,
and a minus sign would spend a quarter of the field on a distance nobody asked
for — on screen far longer than the blank is, since the blank ends at the
host's next line and the negatives would follow the mark up into the history
for the rest of the session. The visible effect of a reset at a prompt on the
bottom row is therefore a gutter that goes blank, which is why the status line
says `Line numbers restart at the next line`: a column of numbers that vanishes
with nothing to explain it reads as a bug.

The mark belongs to the tab and to the moment. Each console is counting its own
output, so resetting one leaves the others alone; and it is a command rather
than a setting, so nothing writes it to the file — a saved mark would number
the next session from a point that never happened in it. It does cost the one
promise above: a number written down before a reset stops being true. That is
the point of asking for one, and nothing else moves a number.

**The numbers are not in the terminal, and that is the whole design.** The
gutter is a separate widget beside the view rather than columns reserved inside
it. It follows that they cannot be selected, cannot be copied, are not in the
session log, never reach the printer, and are invisible to a macro's `wait` —
every one of those reads the grid, and the grid has never heard of them. The
alternative, an origin offset inside `TerminalView`, would have put that
guarantee at the mercy of a dozen coordinate conversions: the painter, both
hit-testing functions, the five places raw pixels are handed to the core's
mouse reporting, sixel and cursor placement, the line editor and the refit. Any
one of them wrong is a paste with line numbers in it, and only some of them
would look wrong on screen.

**Turning it on widens the window; it does not narrow the terminal.** A
configured 80x24 stays 80x24 and the window grows by the width of the gutter.
The alternative is worse than it first looks: the terminal would refit to 75
columns, and `TerminalSize` follows a refit — so the gutter would quietly cost
five columns *permanently*, and turning it off again would not give them back.
On a window that cannot grow, tiled or maximised, the terminal does give up the
columns, which is the answer every other constraint gets here.

**The width is fixed rather than measured.** A gutter that sized itself to the
largest number on screen would gain a column at line 1000 and re-flow the
terminal underneath whoever was reading it.

**And a number that does not fit is left out, not cut down.** The column sits
at the window's left edge and Qt clips a widget's painting to its own rectangle,
so a number drawn from a negative column does not hang off the side — it loses
its leading digits and lands looking like a smaller number that is perfectly
plausible. This shipped at four digits, where line 10001 read `0001`: two
different lines wearing one number, silently, and a line number a person can say
out loud is the whole point of the column. So it takes the rule the reset mark
already set — a number this column cannot state honestly gets no number — and
the default is six digits, a million lines. A session's line number has no
ceiling and four of them is a few minutes of `cat`, which is a gutter that goes
quietly blank halfway through the job it was turned on for.

**Two things it decides rather than discovers.** A row that a wrap landed on
gets its own number, because the core numbers grid lines and a wrapped row is
one — the clipboard still joins the two when `EnableContinuedLineCopy` says to,
so the two answers are allowed to differ. And growing the terminal renumbers
what is already on screen, because `Grid::resize` pulls lines back out of the
scrollback without adjusting the counter; the selection copes with that by
dropping itself, and the gutter simply shows the new numbers.

**What is unchanged.** Nothing upstream reads either key, no cell is written,
and no existing setting changes meaning. A `TERATERM.INI` shared with a real
Tera Term opens identically in both programs, with the gutter absent from the
one that has never heard of it.

## 21. The control characters and the line endings on the wire can be shown

Three `[Sterna]` keys, all off by default, all on the Terminal tab of Setup, and
all three also on the View menu:

- `terminal.show_control_chars` — every control character the terminal executes
  leaves a two-cell caret mark where it happened: `^G` for BEL, `^S` for XOFF.
- `terminal.show_eol` — a mark past the end of every line the host ended.
- `terminal.hide_cr_lf` — narrows that mark from the spelling of what actually
  arrived back to a plain `¶`.

What lands at the end of a line is one table, and each switch has one job:

| `show_control_chars` | `hide_cr_lf` | `show_eol` | at the end of the line |
|---|---|---|---|
| on | off | any | `^M^J`, `^M` or `^J` — what really ended it |
| on | on | on | `¶` |
| on | on | off | nothing |
| off | any | on | `¶` |
| off | any | off | nothing |

**Why.** A terminal cannot answer "which line ending is this thing sending",
because answering it correctly is the one thing a terminal does. CR moves the
carriage, LF feeds the line, and by the time either has reached the screen the
evidence is gone. The same is true of everything else a console sends and a
terminal quietly absorbs: the XON/XOFF a flow-control problem is made of, the
NUL padding an old device pads with, the bell in a loop. Every one of those is a
byte somebody debugging a link needs to see, and none of them is anywhere in the
program that received it. The session log does not help — its text half is the
macro tap, which is printed characters with the controls already executed and no
escape sequences at all — and neither does the one thing upstream has.

**Upstream has debug display mode, and it is not this.** `charset.cpp`'s
`PutDebugChar` writes exactly these caret marks, and this port reproduces it,
Shift+Escape and all. But debug display *replaces* the stream: `Vt::feed` returns
before the parser runs, so escape sequences stop being interpreted and the
terminal stops being a terminal. That is the right tool for reading a protocol
trace and the wrong one for watching a device you are also talking to. These
three annotate a terminal that is still working — colours, cursor motion and
full-screen programs all keep running — which is the whole reason they are
switches in the settings file rather than a fourth debug mode.

**The caret spelling is upstream's, and it is not `␍`.** Unicode has a Control
Pictures block, one cell per control and much prettier than two. It is also
absent from DejaVu Sans Mono:

```console
$ fc-match -f '%{family}\n' 'DejaVu Sans Mono:charset=240d'
Cascadia Code
```

A CI runner carries only `fonts-dejavu-core` and a minimal host may carry no
more, so `␍` would arrive from whatever family fontconfig could find, or as a
box. Caret notation needs nothing but ASCII, and it is already the spelling
somebody who has used debug display mode knows. The one-cell mark that *is* used
— `¶` at the end of a line — is U+00B6, Latin-1, present everywhere, and the
mark every text editor showing line ends has used for thirty years.

**Two mechanisms, because the two cases are not alike.** A mid-line mark has to
be a real cell: the parser consumes control bytes, so there is nothing left in
the grid for a painter to substitute. A line ending cannot be one — after
`hello\r` the CR has already moved the cursor to column 0, so a mark for the LF
that follows would land on top of the line. So the ending is recorded as two
attribute bits on the row's first cell (`ATTR_EOL_CR`, `ATTR_EOL_LF` — the trick
`ATTR_LINE_CONTINUED` already uses for the mirror-image question) and *painted*
past the last character. That costs the row no column and, like the line-number
gutter of deviation 20, reaches no clipboard, no log, no printer, no macro and
no Find.

**A soft wrap gets no mark, and that is the second thing the mark is for.** A
line the terminal broke at the right margin and a line the host ended look
identical on screen and always have. With `show_eol` on they stop looking
identical, which answers a question people ask far more often than they ask
about CR.

**The bits are recorded whatever the switches say.** They cost one `|=` per line
ending and nothing can see them otherwise, and recording them always is what
lets ticking Show line ends explain the screenful *already* on display. A mark
that only appeared on lines arriving after the menu item was ticked would be
useless: "what is this thing sending?" is a question asked after the strange
output, not before it.

**Four bytes deliberately get no mark.** CR and LF are the line's, above. BS
moves the cursor *left*, so its `^H` is stepped back onto and the `H` overwritten
by whatever lands next — leaving a bare `^` one column right of the erasure, and
on the classic rubout echo `BS SP BS` a trail of them marching across the line,
hiding the very erasure the mark was for. `0x88` is HTS in its 8-bit spelling,
the one byte above C0 that reaches the executor; caret notation has no form for
it that is not a backspace's, and upstream's debug display says `^H` in reverse
video. A mark that cannot be told from another byte's is worse than no mark.

**And a great deal cannot be marked at all, which is the price of still
emulating.** Only what the terminal *executes* gets a mark. A C0 inside a
sequence does — `ESC [ 1 BEL m` marks the bell and still turns bold on, which is
upstream's own behaviour — but `ESC` itself, a sequence's parameter and final
bytes, an OSC's terminating BEL, and the 8-bit C1 forms that `rewrite_c1` folds
before the parser sees them, never reach it. Anything that must see *every* byte
wants a tap on the transport, which is a different feature.

**A mark that does not fit is not written.** Two columns or none. Letting the
write path wrap would invent a line break whose `CR LF` the character path taps
and this one deliberately does not — a break the session log and every macro
`wait` would never hear about, spent on annotating one byte. The painted
end-of-line mark takes the same rule for the reason the gutter learnt it: a
widget clips its own painting, so half a `^M` would be a lie about what ended the
line rather than a mark hanging off the edge.

**Nothing that reads the grid as text can see a mark.** They are cells, so the
promise deviation 20 keeps by putting the gutter in another widget is kept here
by an attribute bit instead: `ATTR_CONTROL` is skipped by the clipboard, the
printer's line dump, `LogIncludeScreenBuffer`, `ttctl`'s screen read, DECRQCRA's
checksum, and the flatten that Find and the highlight rules match over. The
session log and the macro tap need no check at all — the path that writes a mark
does not tap, so a run with every switch on is byte-identical, on the wire and in
the file, to a run with them off. A pleasant consequence of the Find one: `ERR`
BEL `OR` still matches `ERROR`, and the highlight paints both halves, reaching
around the mark rather than over it.

**Turning marks on moves the host's layout, and that was accepted rather than
special-cased.** Each mark takes two columns, so text after it shifts right, and
a full-screen program running under it will look wrong until it redraws. The
alternate screen is not excluded either. The switch ships off and exists for a
device console; hiding it from `vim` would add a rule without changing what the
feature is, and `hide_cr_lf` already exists to remove the noisiest two-thirds of
it on a device that ends every line with CR LF.

**What is unchanged.** Nothing upstream reads any of the three keys, the marks
are invisible to every reader of the grid's text, and no existing setting changes
meaning. A `TERATERM.INI` shared with a real Tera Term opens identically in both
programs.
