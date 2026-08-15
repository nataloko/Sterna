# Changelog

This document records notable user-visible changes to Sterna. Release packages
are available from the [GitHub releases page].

## [Unreleased]

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
[Unreleased]: https://github.com/nataloko/Sterna/compare/v0.3.0...HEAD
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
