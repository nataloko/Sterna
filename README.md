# termitta

A cross-platform **communications terminal** — serial, SSH, telnet and local
shell — for Linux and Windows. Compatible with Tera Term; not Tera Term.

> **Status: Stage 0 complete — nothing is usable yet.** What exists is the
> groundwork: a differential-test oracle running Tera Term's real VT engine
> (`oracle/`), a harness running its real file-transfer protocols (`xfer/`),
> and audits of the serial and SSH layers (`serial-audit/`, `ssh-audit/`).
> The terminal itself starts in Stage 1. See [PLAN.md](PLAN.md).

The name is settled. See [PLAN.md](PLAN.md) for scope and status,
[ATTRIBUTION.md](ATTRIBUTION.md) for what is borrowed from Tera Term and under
which terms.

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
- real scripting — the TTL language, so existing `.ttl` scripts run unchanged
- the legacy file-transfer suite: XMODEM, YMODEM, ZMODEM, Kermit, B-Plus, Quick-VAN
- deep CJK support

Explicitly **out of scope**: Tektronix 4010 emulation, the TTX C plugin ABI
(its hooks are literal Winsock function tables — unportable by construction),
Susie image plugins, DDE, and SSH1.

## Architecture

A Rust core behind a flat C ABI, with a Qt 6 Widgets shell. The frontend is
replaceable because it only ever sees POD types.

```
┌─ Qt 6 Widgets (C++) ──── swappable: Tauri / TUI / headless ─┐
│  grid painter · .ui dialogs · IME (ibus/fcitx5) · clipboard │
└──────────────────── C ABI (cbindgen) ───────────────────────┘
┌─ termitta-core (Rust) ────────────────────────────────────────┐
│  vt · grid · charset · conn · xfer · script · config · i18n │
└─────────────────────────────────────────────────────────────┘
```

Qt because CJK input-method support on Linux decides it: `QInputMethodEvent` is
the most-tested IME path there is. Ghostty chose GTK4 for the same reason, and
GTK4 is not good on Windows. No GPU renderer — at 115200 baud you receive
11.5 KB/s, and a `QPainter` glyph atlas draws a 200×60 grid in well under a
millisecond.

## Verification

Rewriting a VT emulator is a correctness problem. Four layers, from day one:

1. **[Differential testing against real Tera Term](oracle/README.md)** — its
   actual `vtterm.c` and `buffer.c`, built headless on Linux. Ground truth for
   free, on every commit.
2. **esctest2** (iTerm2) — automated DEC/xterm conformance.
3. **Tera Term's own test corpus** — the `.sh`/`.pl` escape-sequence exercisers
   and the 53 `.ttl` scripts.
4. **Fuzzing** — the parser eats untrusted bytes off the network.

Plus a performance gate: cold start, idle RSS, throughput, input latency.
"Simple, light, performant" is a claim; CI should enforce it.

## Build

```sh
cd oracle && make && make test
```

Needs a sibling Tera Term checkout at `../teraterm`.

## Licence and attribution

See [ATTRIBUTION.md](ATTRIBUTION.md). Tera Term is © 1994-1998 T. Teranishi and
© the TeraTerm Project under a 3-clause BSD licence. This project is a
compatible reimplementation and is not affiliated with or endorsed by the
TeraTerm Project.
