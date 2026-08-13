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
