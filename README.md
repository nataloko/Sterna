# Sterna

Sterna is a desktop communications terminal for Linux and Windows. It connects
to serial devices, SSH servers, telnet servers, named pipes, and local shells.
It is compatible with Tera Term settings and Tera Term scripts.

## Functions

Sterna has these functions:

- Serial communication with break signals, modem-line control, flow control,
  and hotplug detection
- SSH2 and telnet, with `~/.ssh/config` aliases and `known_hosts`
- More than one session in movable tabs, with a copy function for SSH and
  telnet sessions
- [TTL scripts](docs/macro/README.md), which let you use your `.ttl` files
  without changes
- [Lua plugins](docs/plugins.md) for menus, global shortcuts, connection
  hooks, byte-stream filters, and settings pages
- XMODEM, YMODEM, ZMODEM, Kermit, B-Plus, and Quick-VAN file transfers
- [Send a file a line at a time](docs/sending.md), which holds each line until
  the device prints its prompt, sends the line back, or becomes quiet
- [Sixel graphics](docs/sixel.md) in the terminal, and images in the
  scrollback
- [Find](docs/find.md), which finds text on the screen and in the scrollback
  with case, whole-word, and regular-expression modes
- [Serial ports that open again by themselves](docs/serial-reopen.md) when you
  power-cycle the equipment, keeping the screen and the scrollback
- [Counters](docs/counters.md) in each terminal's status line, which give the
  connection time, the data rates, the data received and sent, and the four
  control lines of a serial port
- [Highlight rules](docs/highlighting.md), which are regular expressions that
  change the colors on the screen and in the scrollback
- [Quick buttons](docs/buttons.md), a bar of commands that you click to start,
  each one with an optional keyboard shortcut
- Line edit in each tab, which holds a command until you push Return, over
  serial, SSH, telnet, raw TCP, or a local shell
- A dark mode for the terminal only, which keeps the desktop theme in the
  menus and the dialogs
- `KEYBOARD.CNF` key mappings and `TERATERM.INI`-compatible settings
- A printer function, a local control socket, and a signed updater that does a
  check one time each day
- An interface that uses the 14 language catalogs of Tera Term.

[docs/deviations.md](docs/deviations.md) gives the compatibility notes and the
deviations from Tera Term. Each deviation is a decision and not a defect.
[ATTRIBUTION.md](ATTRIBUTION.md) gives the attribution and the licenses for the
Tera Term components in Sterna. [CHANGELOG.md](CHANGELOG.md) records the
changes in each release.

## Installation

The [GitHub releases page](https://github.com/nataloko/Sterna/releases/latest)
has the newest release.

For Windows, do these steps:

1. Download the `x86_64-setup.exe` installer.
2. Start the installer.

For Linux, do these steps:

1. Download the `x86_64.AppImage` file.
2. Set the execute permission with `chmod +x sterna-*.AppImage`.
3. Start the AppImage.

## Architecture

Sterna has two parts: a Rust core and a Qt 6 Widgets desktop application. A
flat C ABI connects the two parts.

The core does the terminal emulation, the scrollback, the transports, the
file-transfer protocols, the scripts, the configuration, and the localization.
The desktop application shows the terminal on the screen. It also does the
dialogs, the menus, the clipboard, and the integration with the operating
system.

## Compilation

Use CMake and Ninja to compile the desktop application:

```sh
cmake -S shell -B shell/build -G Ninja
cmake --build shell/build
```

Rust, Cargo, CMake, Ninja, and the Qt 6 Widgets, PrintSupport, and Network
development packages are necessary.

## License and attribution

Sterna has the 3-clause BSD license. [LICENSE](LICENSE) and
[ATTRIBUTION.md](ATTRIBUTION.md) give the license and the notices for the
components in Sterna.

Tera Term is © 1994-1998 T. Teranishi and © the TeraTerm Project. Sterna is
not affiliated with or endorsed by the TeraTerm Project.
