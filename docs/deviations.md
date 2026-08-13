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
| 1 | The default baud rate is 115200 | 9600 | unreleased |
| 2 | The connect dialogs remember the last connection, across restarts | Only Setup > Save persists anything | unreleased |
| 3 | A bar under the menu: port, connect/disconnect, local echo | No toolbar at all | unreleased |
| 4 | One, two or four simultaneous connection panels | One connection per window | unreleased |
| 5 | Starting Sterna looks for a signed update, once a day | Nothing contacts a server on its own | unreleased |
| 6 | Highlight rules: user-written regular expressions recolour the screen | Only the host decides a colour; the URL attribute is the one exception | unreleased |

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

## 2. The connect dialogs remember the last connection

Opening File > Connect to serial port, SSH or telnet shows what was last
connected to — after a restart as well as within one run.

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
closes the connection, and a Local echo check box. Tera Term has no toolbar —
those three are a dialog, a menu item, and a check box on a settings tab.

**Why.** They are the three things that get used every few minutes on a console
port, and each of them costs a dialog upstream: picking a port is File > New
connection, closing is File > Disconnect, and local echo is three tabs into
Setup. Nothing else is on the bar for the same reason — it is not a general
toolbar, and a fourth item would be one somebody has to explain.

**What is unchanged.** The bar decides nothing. Every widget on it is a view of
the session refreshed from the same status update the menu uses, and every click
calls the window method the menu item calls — so the port the bar shows is the
port that is open, the button says what the session is, and the check box is the
live `terminal.local_echo`, which a host's SRM and a macro can also change.

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
