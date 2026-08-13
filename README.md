# Sterna

Sterna is a native communications terminal for Linux and Windows. It connects
to serial devices, SSH and telnet servers, named pipes, and local shells while
supporting Tera Term-compatible workflows and configuration.

## Features

- Serial communication with break signalling, modem-line control, flow
  control, and hotplug handling
- SSH2 and telnet, including `~/.ssh/config` aliases and `known_hosts`
- Multiple sessions in movable tabs, with SSH and telnet session duplication
- [TTL scripting](docs/macro/README.md) for existing `.ttl` scripts
- [Lua plugins](docs/plugins.md) for menus, global shortcuts, connection hooks,
  byte-stream filters, and settings pages
- XMODEM, YMODEM, ZMODEM, Kermit, B-Plus, and Quick-VAN file transfers
- Inline [sixel graphics](docs/sixel.md), including images in scrollback
- [Highlight rules](docs/highlighting.md): regular expressions that recolour
  the screen and the scrollback
- [Quick buttons](docs/buttons.md): a bar of commands one click away, each one
  optionally on a keyboard shortcut
- Per-tab line editing that holds an editable command locally until Return,
  over serial, SSH, telnet, raw TCP, or a local shell
- A terminal-only dark mode that leaves menus and dialogs in the desktop theme
- `KEYBOARD.CNF` key mappings and `TERATERM.INI`-compatible settings
- Printing, a local control socket, and a signed updater that checks once a day
- Localized interface using Tera Term's 14 language catalogs

Compatibility notes and intentional differences from Tera Term are documented
in [docs/deviations.md](docs/deviations.md). Attribution and licensing details
for incorporated Tera Term components are in
[ATTRIBUTION.md](ATTRIBUTION.md).

## Installation

Download the latest release from the
[GitHub releases page](https://github.com/nataloko/Sterna/releases/latest).

- **Windows:** download and run the `x86_64-setup.exe` installer.
- **Linux:** download the `x86_64.AppImage`, make it executable with
  `chmod +x sterna-*.AppImage`, then run it.

## Architecture

Sterna has a Rust core exposed through a flat C ABI and a Qt 6 Widgets desktop
application.

The core handles terminal emulation, scrollback, transports, file-transfer
protocols, scripting, configuration, and localization. The desktop application
handles rendering, dialogs, menus, the clipboard, and platform integration.

## Build

Build the desktop application with CMake and Ninja:

```sh
cmake -S shell -B shell/build -G Ninja
cmake --build shell/build
```

This requires Rust, Cargo, CMake, Ninja, and the Qt 6 Widgets, PrintSupport, and
Network development packages.

## Licence and attribution

Sterna is distributed under the 3-clause BSD licence. See [LICENSE](LICENSE)
and [ATTRIBUTION.md](ATTRIBUTION.md) for the licence and notices covering the
incorporated components.

Tera Term is © 1994-1998 T. Teranishi and © the TeraTerm Project. Sterna is
not affiliated with or endorsed by the TeraTerm Project.
