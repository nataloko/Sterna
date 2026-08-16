# Changelog

This document records notable user-visible changes to Sterna. Release packages
are available from the [GitHub releases page].

## [Unreleased]

### Fixed

- **A serial adapter unplugged on Windows is noticed.** The session went on
  saying it was connected — nothing there becomes readable when a COM port's
  device leaves, unlike a Unix device node — until somebody typed at it, and
  what came back was `os error 22` rather than a reconnection. It is found
  within a second now, from the clock, and it starts the same wait for the port
  as every other kind of disconnection.

### Changed

- **The quick buttons scroll.** More of them than there was panel used to make
  the window taller to fit, make it taller again on changing to a fuller page,
  and, past the point where the screen ran out, share the panel's height among
  the buttons until each was a sliver. The panel takes the height it is given
  and the rest is one wheel away.
- **The + is at the bottom of the panel and the page list is under it**, where
  neither scrolls away from somebody with enough buttons to need them. The page
  drop-down was at the top.
- The status line paints a port it is **waiting for in amber** rather than in
  the red it uses for a session that is simply down. Red is the state somebody
  has to act on, and a reopen is already acting.

## [0.6.0] - 2026-08-16

### Added

- **A serial port that goes away is opened again by itself when it comes back.**
  Power-cycle a board and its USB adapter leaves `/dev` with it; the session now
  waits for the adapter and reconnects when it returns, keeping the screen and
  the scrollback that explain why the board went down. The switch is *Open the
  port again automatically*, in the Serial half of the New connection dialog,
  and it ships on. Tera Term has had `AutoComPortReconnect` and its four timings for
  years and Sterna has read them since it had a settings file; this is the first
  release that acts on them. The wait for the adapter has no time limit — a
  board switched off overnight is reconnected in the morning — while the opening
  is tried four times and then reported on the terminal's own status line rather
  than in a dialog. Nothing is opened to find out whether the port is back:
  probing a serial port raises DTR, which reboots an Arduino-style board and
  drops a modem's carrier. The port comes back at the speed it was actually
  using, which is not always the speed in the settings file. See
  [`docs/serial-reopen.md`](docs/serial-reopen.md).
- The quick-button panel has **pages**. One flat column is the right shape until
  somebody keeps commands for four different devices; then a `reload` for one of
  them is sitting next to a `show version` for another. A drop-down appears at
  the top of the panel as soon as there is a second page, and the panel's
  right-click menu grows **Page** and **Move to page**. In the editor, the list
  is one page's, with a **Pages** menu beside it for adding, renaming and
  removing one and an **On page** field on every button. The page you were on is
  where the window opens next time, per settings file. Removing a page keeps its
  commands — they move to the page beside it — because deleting a command is
  Remove, which asks by name. A shortcut works from every page: a key the
  terminal has given up must not come back depending on which page is showing.
- A page can be **exported and imported** — Pages > Export page… writes it as an
  ordinary settings file holding one `[Sterna Buttons]` section, and Import
  page… reads one back. So a page can be pasted into a settings file by hand,
  any settings file can be imported as a page, and exporting onto a file that
  already exists replaces its buttons and leaves the rest of it alone. Imported
  buttons arrive without their shortcuts, since the file they came from knows
  nothing about the keys this one has already given away.
- The terminal can stop following the window. View > **Wrap lines**, or "Term
  size = win size" on the Setup > Terminal page — one switch, either name. With
  it on, which is how Sterna has always behaved, lines wrap where the window
  ends and dragging the window narrower changes the terminal. With it off the
  terminal keeps the width it has and lines wrap at *that* width, a horizontal
  scrollbar appears under it, and **the text survives**: narrowing a terminal
  cuts every line at its new right-hand edge, in the scrollback as well as on
  screen, and nothing puts the ends back. It is also how to give a 200-column
  device menu a window that does not have to be 200 columns wide. Turning the
  switch off freezes the terminal at the width it has at that moment, so
  nothing is lost in the act of turning it on.
- The terminal shows its size in characters while the window is being resized —
  `100x30`, in the middle of the terminal, gone a second after the last change.
  With the terminal fixed it reads `100x30 of 132x30`: what fits, and what
  there is. Setup > Window > Show terminal size turns it off.
- Counters in each terminal's status line. The field gives the time since the
  connection opened and how fast data moves in each direction, and a click on
  it opens the other counts: the data received and sent, the lines, the breaks,
  and the send queue, which is the data that waits because flow control holds
  the line. On a serial port there are also the four control lines — **CTS**,
  **DSR**, **CD** and **RI** — which frequently tell you why nothing moves.
  Sterna reads the control lines only while you look at them.

  The counters are about one connection: a new connection starts them at zero,
  and the connection time beside them says which connection the totals are for.
  When a connection stops, the counters keep the totals and the clock stops,
  because "how much did that session move" is a question you ask after the line
  stops. A file transfer is included in the counts, which the session log does
  not record. See [Counters](docs/counters.md).

  **View > Show counters** and the Window page of Setup > Preferences control
  the field, with the `Counters` key. It is on when Sterna is installed.
  `ttctl status` reports the same counts.
- The terminal can show what the wire is carrying. **View > Show control
  characters** puts a two-cell caret mark where each control character went
  past — `^G` for the bell, `^S` for XOFF — while the terminal goes on
  interpreting everything else, so colours, cursor motion and full-screen
  programs keep working. **View > Show line ends** marks the end of every line
  the host ended, spelling out `^M^J`, `^M` or `^J` when control characters are
  shown, so "which line ending is this device sending?" is answerable by
  looking. A line the terminal wrapped at the right margin gets no mark, which
  also tells a wrapped line from a real one at a glance. **View > Remove CR and
  LF marks** drops back to a plain `¶` where the two are more noise than
  information. All three are off by default and are also on the Terminal tab of
  Setup, as `ShowControlChars`, `ShowEol` and `HideCrLf`.

  The marks are annotations, not text: they never reach a selection, the session
  log, the printer, a macro's `wait` or Find, and a search still matches a word a
  mark is sitting inside. Showing control characters does move the host's layout,
  because each mark takes two columns.

### Fixed

- A settings change no longer stops a repeating quick button. Any change at all
  — a font, a colour, a switch in Setup, a macro's `setsetting` — re-read the
  button list and ended every run in progress, with nothing on screen saying
  why. A run now stops only when the list itself changes, which is what the
  documentation always said.
- A host asking for a different terminal size — `CSI 8 t`, which is what a
  program that wants 132 columns sends — now resizes the window to hold it.
  The request reached the terminal but never the window, so the extra columns
  were off the right-hand edge until the next resize discarded them. With the
  terminal fixed to its own width, "Auto win resize" decides instead: on, the
  window follows the host; off, the scrollbar covers the difference.

## [0.5.4] - 2026-08-16

### Fixed

- Changing the quick-button panel's width no longer erases terminal text. The
  panel shared its space with the terminal beside it, so making it wider
  narrowed the terminal — and narrowing a terminal cuts every line at the new
  right-hand edge, in the scrollback as well as on screen, and does not restore
  them when the panel is made narrow again. The width now comes out of the
  **window**: the terminal keeps every column and every character it had. Where
  the window cannot get any wider, because it is maximised or already at the
  edge of the screen, the panel stays as it is rather than taking the columns.
  Showing the panel from View > Show quick buttons had the same effect and is
  fixed the same way, including with several terminals tiled — a case the old
  window-resizing path never covered.

### Changed

- The quick-button panel is fixed to the right-hand side, and its width is now
  a menu item rather than an edge you drag: **right-click the panel > Panel
  width**, with *Set width…* and *Fit to buttons*. It is also on the Window
  page of Setup > Preferences. Both the four-edge move and the drag came from
  the dock the panel lived in, which is the thing that was taking pixels from
  the terminal; a handle that grows the window rather than the terminal cannot
  follow your pointer, so it is a menu instead — one that works on a maximised
  window, from the keyboard and from a macro, none of which the handle did.
  `QuickButtonsArea` is gone and `QuickButtonsWidth` replaces it, in pixels,
  with `0` — the shipped value — meaning as wide as the widest button needs. A
  settings file that still names an edge is read without complaint; the key is
  simply no longer one Sterna looks at.

### Added

- The quick-button panel can be made narrower than its own button labels. The
  labels shorten with an ellipsis rather than holding the panel open, so one
  long caption no longer widens every other button, and a narrow strip of stubs
  down the edge of the screen is now possible. The full label is in the
  button's tooltip whenever it does not fit, and it is what the editor and the
  settings file still hold. The panel will not go below 48 pixels, which is
  about the narrowest thing you can reliably click.

## [0.5.3] - 2026-08-15

### Changed

- The application icon. The tern is now blue instead of phosphor green, and
  the rows of output behind it are brighter. The dark tile and the orange
  cursor block are unchanged. Launchers, window icons and the Windows
  executable all show the new icon.

## [0.5.2] - 2026-08-15

### Fixed

- The View menu's switches are remembered. Show toolbar, Show quick buttons,
  Show line numbers and Highlight matches moved the terminal in front of you
  and nothing else, so the next launch read the settings file and put all four
  back where they had been. Each now writes its own setting as it is ticked,
  the way View > Tiled already did. This is deliberately not governed by the
  automatic-save option on the Settings page: that option is about the Setup
  dialog, whose changes are provisional until its OK button, and a menu tick
  has no OK button behind it to wait for. Only the one key is written, so a
  settings file shared with Tera Term keeps every other line it had.

## [0.5.1] - 2026-08-15

### Changed

- A new application icon. The tern now flies across a dark terminal tile, in
  phosphor green over dim rows of output, with the cursor block in orange
  beside the beak. It is the same bird on the same S-shaped flight path; what
  changed is the ground it is on. The old tile was warm white, which made it
  the one bright square in a dock of dark ones and said nothing about what the
  program is. Windows, Linux launchers and the window itself all take the new
  one.

## [0.5.0] - 2026-08-15

### Added

- Edit > Find (Ctrl+Shift+F) searches the terminal — the page in front of you
  and all of the scrollback — with case, whole-word and regular-expression
  matching, next and previous, and the last twelve patterns on a dropdown. The
  match you are on is selected, so it scrolls into view and Copy takes it; the
  others on screen are filled in amber. Tera Term has nothing of the kind: the
  way to find something in its buffer is to log the session and search the file
  in another program. The shortcut is Ctrl+Shift+F rather than Ctrl+F because a
  shortcut on this window is a key the host stops receiving, and `^F` is a page
  forward in vim and less; Copy and Paste make the same bargain here already.
- Line numbers, in a column beside the terminal. View > Show line numbers turns
  them on, or `[Sterna] LineNumbers` and `LineNumberWidth` on the Terminal tab
  of Setup; the number is the session's own line, counted from the first line
  the host printed, and a line keeps it as it scrolls up into the history. The
  numbers are beside the terminal rather than in it, so they cannot be selected
  or copied and never reach the session log, the printer or a macro's `wait` —
  and turning them on widens the window instead of taking columns off the
  terminal. The column is a fixed six digits wide, so it never re-flows the
  terminal mid-session; a line whose number needs more than that carries none,
  rather than a number with its leading digits missing.
- View > Reset line counter starts the count again: the next line the host
  prints is line 1, so you can reset at a prompt, run a command and read its
  output off as 1, 2, 3. Lines printed before the mark carry no number, and the
  status line says so. Each tab counts its own output, and the mark is not
  saved with the settings.

## [0.4.1] - 2026-08-15

### Fixed

- A settings change no longer clears the terminal when the window is too small
  to hold the terminal size you configured — a large font on a small screen, or
  a window the desktop capped. The quick button panel was rebuilt on every
  settings change, and the few pixels its panel gave up and took back were
  enough to move the terminal by a column; with Clear on resize turned on, each
  of those scrolled the page into the scrollback. Toggling line edit was enough
  to do it.

## [0.4.0] - 2026-08-15

### Added

- File > Log now opens a dialog with the options Tera Term offers, instead of a
  bare file picker: text or binary, overwrite or append, a byte-order mark,
  plain text, timestamps and which clock they use, and whether to write what is
  already on the screen into the file before the live bytes. Log rotation is on
  it too, which Tera Term keeps on a settings page.
- Logging can be paused. File > Pause logging suspends it without closing the
  file, and clicking the `REC` counter in the status line does the same — that
  counter is where Tera Term's Pause button would be if this program had the
  separate logging window it lives on. What arrives while a log is paused is
  not written later; it is not kept, which is also Tera Term's behaviour.
- File > Stop logging is its own item, so a keyboard mapping or a quick button
  can reach starting, pausing and stopping separately.
- Every setting in the Setup dialog now explains itself. Hovering one gives a
  plain-language description of what it changes and when it matters, written to
  the simplified English (ASD-STE100) the project now uses for anything a user
  reads, and the tooltip ends with the setting's default — spelled `(empty)` or
  `Automatic` where the stored value is a blank or a sentinel. A setting Sterna
  carries for file compatibility but does not act on says so, instead of
  describing behaviour you will not get. The search box searches the help too.

### Changed

- A log names itself by the clock. The default file name is now
  `sterna-%Y%m%d_%H%M%S.log` rather than `teraterm.log`, so a second log cannot
  land on the first — which, depending on a setting you were not looking at,
  either overwrote it or appended to it — and a log directory shared with
  anything else says which program wrote which file. The name is still a
  template and a file that sets `LogDefaultName` keeps whatever it says; `&h`
  for the host, `&p` for the port and `&u` for the user still work if you want
  them in there.
- With no log directory configured, the log dialog opens on the one the last
  log was written to, remembered in `[Sterna] LogDir` — instead of the per-user
  directory it fell back to before. A `LogDefaultPath` you have set still
  decides, every time.
- Quick buttons now fill the panel they sit in. Down the left or right edge
  each button is as wide as the panel, so widening it makes the buttons wider
  instead of leaving a strip of empty space beside them, and every button is
  the same size whatever its label; along the top or bottom edge they take the
  panel's height the same way.
- A setting's tooltip no longer carries the schema's own notes, which are the
  citations that prove where a default comes from. Those are developer
  documentation and they stay in the schema and in the generated docs; the
  dialog shows the help and the default. Menu status tips and the connect bar's
  tooltips are rewritten to the same rules.

## [0.3.1] - 2026-08-15

### Added

- Added Edit > Select screen and Edit > Select all. Select screen takes the
  lines the window is showing, so scrolled back it selects the history in front
  of you; Select all takes the scrollback and the page together.

### Changed

- The destination field now follows the terminal in front of it. Each tab or
  tile remembers what it was opened with, so selecting one shows where that
  session is connected rather than wherever the window connected last. It now
  also shows a connection that was not added to the list, which it did not do
  with `RememberConnections=off`.

## [0.3.0] - 2026-08-14

### Added

- The bar under the menu now connects to anything, not just a serial port. Its
  dropdown offers the connections you have actually opened — each with the
  parameters it was opened with — the ports plugged in now, the hosts in
  `~/.ssh/config`, and a local shell; and the field takes anything the command
  line takes, including a whole Tera Term command line when it has a space in
  it. Picking a row fills the field and Connect opens it, so a click on the
  arrow cannot start a connection by itself; committing one while a session is
  live opens it in a new tab or tile rather than closing what is there.
- The list of recent connections is remembered in `[Sterna] Recent`.
  `RememberConnections=off` stops adding to it, and the dropdown's Forget item
  empties it.
- Added View > Tiled, which shows every open connection at once in a grid that
  fits their number. Tiles replace the tab bar rather than sitting under it, so
  no connection is hidden; when the grid is not exactly full, the spare cell
  offers a new connection.
- Each terminal now has its own status line — its name, its connection state,
  and its recording counter — instead of one shared line for the window. A
  transfer result or a message from a background session now appears on the
  terminal it belongs to.
- The terminal background is now a different shade while nothing is connected,
  so an idle terminal is recognisable at a glance in a tiled window. How far it
  moves is `[Sterna] DisconnectedShade`; `0` turns it off.

- A serial port another program already has open is greyed out in the connect
  bar's dropdown, with the holder's name beside it, instead of being offered
  and then failing with a dialog. On Linux that covers anything holding a lock
  — including another Sterna window — plus your own processes that took none;
  a port held by a root-owned process such as ModemManager still looks free.
  On Windows, where the system cannot be asked without opening the port, it
  covers this program's other windows, which is what Tera Term manages too.

### Fixed

- Typing into an interactive host no longer puts every character on a new line.
  The received newline setting now ships as `CRReceive=DETECT`, which works out
  from the first line ending whether the far end means CR, LF or CRLF and stays
  with that answer. The previous default, Tera Term's own `AUTO`, reads a bare
  CR as a line ending for ever — and a shell redrawing its prompt sends one on
  every keystroke. `AUTO` is still available and still behaves as it does in
  Tera Term.
- Disconnecting no longer closes the window. `AutoWinClose` now applies to a
  connection the far end ended, not to one you asked to end, so the connect bar
  is still there to open the next one.
- A file-transfer result is now visible; it had been overwritten in the same
  instant it was written.
- Connect now opens the row you chose last. Choosing a remembered connection
  and then choosing something else opened the remembered one anyway, which
  mattered because the dropdown picks a row by itself as it opens.
- Opening the dropdown during a session no longer greys out Disconnect.
- A shell opened in the AppImage build no longer inherits the AppImage's own
  libraries. Programs run from that shell reported `no version information
  available`, and some — `flatpak` among them — refused to start at all.
- The SSH hosts offered in a picker no longer include ones that can only be
  reached by running another program. On a systemd machine that removes
  `.host` and `machine/.host`, which come from the system-wide configuration
  rather than yours and would have failed if picked.
- A window resized by hand keeps its size when a setting is applied. Clicking
  Local echo or Line edit, or changing anything in Setup, had restored it to
  the size in the settings file.
- Changing the terminal size in Setup now resizes the window.
- Double-clicking the title bar maximises the window on GNOME. The AppImage now
  carries Qt's Adwaita window decoration, which also gives the window the
  desktop's own title bar instead of Qt's fallback.

### Changed

- The serial port dropdown in the toolbar is gone, replaced by the destination
  field above. The port list it offered is still there, under a heading, with
  real adapters first and a bounded tail — an ordinary desktop enumerates
  thirty-two motherboard `ttyS` ports that have nothing attached, and they used
  to bury the one adapter you own.
- `PanelLayout` in `[Sterna]` is now `single` or `tiled`. Files written by
  earlier versions saying `two` or `four` open tiled.
- Setup lists one Preferences item instead of one item per settings page; the
  dialog's own tabs and search box are the way to a page.
- Show toolbar, Show quick buttons and Highlight matches moved from Setup to
  View. Their editors — Highlighting and Quick buttons — stay in Setup.
- Help's Release page moved into the About dialog, beside Check for Updates.

## [0.2.5] - 2026-08-14

### Added

- Replaced the separate transport dialogs with one New Connection screen for
  serial, SSH, telnet, raw TCP, and local shells.
- Added settings search and direct navigation across all built-in pages and
  plugin settings pages.
- Added an opt-in choice to save settings when applying them. Only settings
  changed successfully are written; manual saving remains the default.

### Fixed

- Copying text while per-tab line editing is enabled no longer clears or
  duplicates the locally edited line.
- Applying any setting no longer clears the terminal when `ClearOnResize` is
  enabled and the terminal size did not change.
- Fixed connection-dialog defaults and validation, including settings whose
  valid value is a negative sentinel.
- Fixed several settings persistence edge cases for core and plugin settings.

## [0.2.4] - 2026-08-14

### Added

- Added Edit > Clear screen and Edit > Clear buffer commands.
- Added Tera Term-compatible right-button paste confirmation, enabled by
  default.

### Fixed

- Normalized pasted line endings to the single carriage return a keyboard
  Return key sends.

## [0.2.3] - 2026-08-14

### Added

- Quick buttons can repeat a configured number of times at a configured
  interval, or continue until stopped.
- A repeating quick button can be stopped by pressing it again, pressing
  Escape, editing the button list, or disconnecting its session.

### Changed

- Clarified how regular-expression capture groups work in highlight rules.
- Redrew the terminal dark-mode icon for better visibility.

## [0.2.2] - 2026-08-14

### Changed

- Rebuilt the Linux AppImage against the `manylinux_2_28` baseline with bundled
  Qt 6.11.1, making the stated glibc 2.28 compatibility floor enforceable.
- Bundled the GLVND OpenGL, EGL, and GLX frontends required to start on minimal
  Linux installations.

## [0.2.1] - 2026-08-13

### Added

- Added a terminal-only dark mode that leaves menus and dialogs in the desktop
  theme.
- Added installation instructions and embedded application icons.

### Changed

- Changed automatic receive newline handling to detect CR, LF, and CRLF.
- Hid panel controls when they are not useful and disabled offline input
  controls when no session can receive them.

## [0.2.0] - 2026-08-13

### Added

- Added configurable quick buttons for text, bytes, macros, and menu commands.
- Added ordered regular-expression highlight rules for the screen and
  scrollback.
- Added per-tab line editing for serial, SSH, telnet, raw TCP, and local shell
  sessions.

## [0.1.7] - 2026-08-13

### Changed

- Hid the pane header in single-pane mode and preserved the window geometry
  when switching back to it.

### Fixed

- Restored the configured local-echo state after a connection closes.

## [0.1.6] - 2026-08-13

### Added

- Added one-, two-, and four-pane layouts for viewing simultaneous sessions.
- Added a quiet automatic update check, limited to once per day and shown only
  when an update is available.
- Added the Sterna logo to the About dialog and made disconnected status more
  visible.

## [0.1.5] - 2026-08-13

### Added

- Added an About dialog and moved the manual update check into it.
- Added automated Linux AppImage and Windows installer builds for tagged
  releases.

### Fixed

- Added native Windows coverage for launching a downloaded installer while
  keeping the verified bytes pinned against replacement.

## [0.1.4] - 2026-08-13

### Fixed

- Released the downloaded Windows installer file before launching it, avoiding
  a sharing violation during an update.

## [0.1.3] - 2026-08-13

### Added

- Added a compact connection bar for selecting a serial port, connecting or
  disconnecting, and toggling local echo.

### Changed

- Reordered the menu bar to follow Tera Term and wrapped the settings dialog's
  page tabs onto two rows.

## [0.1.2] - 2026-08-13

### Changed

- Changed the default serial baud rate from 9600 to 115200.
- Remembered the last successful connection across restarts without rewriting
  unrelated settings.

### Fixed

- Opened Windows serial ports for overlapped I/O so an idle receive wait cannot
  freeze the first write.

## [0.1.1] - 2026-08-13

### Added

- Added Lua plugins with menu items, shortcuts, connection hooks, byte-stream
  filters, and custom settings pages.
- Added inline sixel graphics with bounded storage and scrollback support.
- Added signed in-application updates on Linux and Windows, plus AppImage zsync
  metadata.
- Added a generated reference for the supported TTL macro language.

## [0.1.0] - 2026-08-12

Initial public release.

### Added

- Native Qt 6 desktop applications for Linux and Windows, with multiple tabs,
  printing, scrollback, selection, clipboard integration, and 14 interface
  languages.
- Serial, SSH2, telnet, raw TCP, local shell, and Windows named-pipe
  connections, including HTTP, SOCKS, and telnet proxies.
- Tera Term-compatible terminal behavior, `TERATERM.INI` settings,
  `KEYBOARD.CNF` mappings, and Tera Term command-line parsing.
- TTL and Lua scripting, a local control socket, and the `ttctl` and
  `ttpmacro` clients.
- XMODEM, YMODEM, ZMODEM, Kermit, B-Plus, and Quick-VAN file transfers.
- Linux AppImage and Windows installer packages.

[GitHub releases page]: https://github.com/nataloko/Sterna/releases
[Unreleased]: https://github.com/nataloko/Sterna/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/nataloko/Sterna/compare/v0.5.4...v0.6.0
[0.5.4]: https://github.com/nataloko/Sterna/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/nataloko/Sterna/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/nataloko/Sterna/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/nataloko/Sterna/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/nataloko/Sterna/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/nataloko/Sterna/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/nataloko/Sterna/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/nataloko/Sterna/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/nataloko/Sterna/compare/v0.2.5...v0.3.0
[0.2.5]: https://github.com/nataloko/Sterna/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/nataloko/Sterna/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/nataloko/Sterna/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/nataloko/Sterna/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/nataloko/Sterna/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/nataloko/Sterna/compare/v0.1.7...v0.2.0
[0.1.7]: https://github.com/nataloko/Sterna/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/nataloko/Sterna/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/nataloko/Sterna/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/nataloko/Sterna/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/nataloko/Sterna/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/nataloko/Sterna/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/nataloko/Sterna/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/nataloko/Sterna/releases/tag/v0.1.0
