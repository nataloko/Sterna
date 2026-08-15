# Changelog

This document records notable user-visible changes to Sterna. Release packages
are available from the [GitHub releases page].

## [Unreleased]

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
[Unreleased]: https://github.com/nataloko/Sterna/compare/v0.5.2...HEAD
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
