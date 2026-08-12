# Sterna

A cross-platform **communications terminal** — serial, SSH, telnet and local
shell — for Linux and Windows. Compatible with Tera Term; not Tera Term.

> **Status: Stage 4 underway; Stage 3 complete.** The Linux application has
> all four transports, the oracle-diffed VT engine, six file-transfer
> protocols, TTL and Lua scripting, the generated settings UI, a control
> socket, a Lua plugin API for menus, global keys, connection hooks and
> byte-stream filters plus custom settings pages, an
> AppImage, `KEYBOARD.CNF`, and all compatible surfaces wired to Tera Term's
> 14 `.lng` catalogs. The Windows build passes its native CI, ships as an NSIS
> installer, and printing is wired end to end. Both platforms have multiple
> sessions in movable tabs, including live SSH/telnet session duplication.
> See [PLAN.md](PLAN.md).

The name is settled. See [PLAN.md](PLAN.md) for scope and status,
[ATTRIBUTION.md](ATTRIBUTION.md) for what is borrowed from Tera Term and under
which terms, and [AGENTS.md](AGENTS.md) for the working agreements and the list
of traps — which is written for coding agents and is the fastest way for a
person to find out where this codebase bites, too.

## Why

[Tera Term](https://teratermproject.github.io/) is the best free GUI serial +
SSH terminal there is, and it is Windows-only — not incidentally, but
structurally: `_WIN32` appears zero times in its 157k lines because the code has
never been asked to compile anywhere else.

On Linux the gap is real. `minicom` and `picocom` have no scripting and no GUI,
`cutecom` and `moserial` are toys, PuTTY has serial but neither scripting nor
file transfer, and the one tool that genuinely covers this ground — SecureCRT —
is closed and paid.

The aim is **not** feature parity with Tera Term. It is the ~20% of Tera Term
that nothing else on Linux does, done well:

- first-class serial (break signalling, modem lines, flow control, hotplug)
- SSH2 and telnet, plus `~/.ssh/config` and `known_hosts` (which Tera Term lacks)
- real scripting — the [TTL language](docs/macro/README.md), so existing `.ttl`
  scripts run unchanged
- persistent [Lua plugins](docs/plugins.md) with menu actions, global shortcuts,
  connect/disconnect hooks, binary-safe stream filters and typed settings pages
- the legacy file-transfer suite: XMODEM, YMODEM, ZMODEM, Kermit, B-Plus, Quick-VAN

CJK input methods and the charset tables are **deferred indefinitely** — see
[PLAN.md](PLAN.md). Wide and combining character handling in the grid stays in
scope regardless; box drawing, emoji and accents need it.

Explicitly **out of scope**: Tektronix 4010 emulation, the TTX C plugin ABI
(its hooks are literal Winsock function tables — unportable by construction),
Susie image plugins, DDE, and SSH1.

## Architecture

A Rust core behind a flat C ABI, with a Qt 6 Widgets shell. The frontend is
replaceable because it only ever sees POD types.

```
┌─ Qt 6 Widgets (C++) ──── swappable: Tauri / TUI / headless ─┐
│  grid painter · .ui dialogs · clipboard · menus              │
└──────────────────── C ABI (cbindgen) ───────────────────────┘
┌─ Sterna core (Rust) ────────────────────────────────────────┐
│  vt · grid · charset · conn · xfer · script · config · i18n │
└─────────────────────────────────────────────────────────────┘
```

Qt because it is strong on Windows *and* Linux at once, unlike GTK4, and because
Tera Term's settings surface is 76 dialogs over a 909-line struct — which is
what Widgets is good at and what the Rust-native toolkits are not. No GPU
renderer: at 115200 baud you receive 11.5 KB/s, and a plain `QPainter` grid
measures 255 fps of full-screen repaint on the target Qt, roughly 40× the
headroom needed.

## Verification

Rewriting a VT emulator is a correctness problem. Four layers, from day one:

1. **[Differential testing against real Tera Term](oracle/README.md)** — its
   actual `vtterm.c` and `buffer.c`, built headless on Linux. `./run_diff.sh`
   feeds every case to both engines and diffs the dumps against each other, in
   CI on every commit. No golden files: the oracle is the expected output.
2. **esctest2** (iTerm2) — automated DEC/xterm conformance.
3. **Tera Term's own test corpus** — the `.sh`/`.pl` escape-sequence exercisers
   and the 53 `.ttl` scripts.
4. **Fuzzing** — the parser eats untrusted bytes off the network.

## Performance

"Simple, light, performant" is a claim, so it is measured — see
[bench/README.md](bench/README.md). On an AMD Ryzen 7 7840HS, Fedora 44,
Qt 6.11.1 under Wayland, 2026-08-08:

| | |
|---|---|
| exec → first frame | 68 ms |
| idle RSS / PSS, with a shell attached | 64.5 / 40.5 MB |
| keystroke → the frame that shows it | 1.03 ms |
| 10 MB out of a pty, painted | 39 MB/s |
| the VT engine alone, 10 MB | 67–84 MB/s |

**~60 MB is Qt's floor**, and that is worth stating plainly rather than meeting
in a review: mid-pack among modern terminals, well above Tera Term on Windows,
and imposed by the toolkit rather than by anything the code can optimise away.

The gate earned its keep on the day it was written. It found the window
painting a frame for every 8 KB read — which Wayland's frame callbacks were
hiding and X11 was not — and a floor under the frame interval took X11 from
4 MB/s to 36 with no change to keystroke latency.

CI enforces an absolute floor on the engine half — the shell half depends on a
Qt version no CI runner has, so it is a local gate against a recorded baseline.

## Build

```sh
cd oracle && make && make test    # Tera Term's VT engine, headless
cd crates && cargo test           # the Rust core
./run_diff.sh                     # the two, diffed against each other
./bench/bench.py --core           # the engine, against the recorded baseline
```

Needs a sibling Tera Term checkout at `../teraterm`.

## Licence and attribution

See [ATTRIBUTION.md](ATTRIBUTION.md). Tera Term is © 1994-1998 T. Teranishi and
© the TeraTerm Project under a 3-clause BSD licence. This project is a
compatible reimplementation and is not affiliated with or endorsed by the
TeraTerm Project.
