# termitta

A cross-platform **communications terminal** — serial, SSH, telnet and local
shell — for Linux and Windows. Compatible with Tera Term; not Tera Term.

> **Status: Stage 1 in progress — early, but it runs.** Stage 0 built the
> groundwork: a differential-test oracle running Tera Term's real VT engine
> (`oracle/`), a harness running its real file-transfer protocols (`xfer/`),
> and audits of the serial and SSH layers (`serial-audit/`, `ssh-audit/`).
> Stage 1 has the VT engine (`crates/`), diffed against the oracle on every
> commit; all four transports — serial, SSH, telnet and a local shell; and a Qt
> window that opens them. No file transfer, no scripting, no settings, no
> packaging. See [PLAN.md](PLAN.md).

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
┌─ termitta-core (Rust) ──────────────────────────────────────┐
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

Two of those are worth stating plainly rather than meeting in a review.
**~60 MB is Qt's floor** — mid-pack among modern terminals, well above Tera
Term on Windows, and imposed by the toolkit rather than by anything the code
can optimise away. And **throughput through the window is 6–9x better under
Wayland than under X11**, because Wayland's frame callbacks coalesce repaints
and X11 has no such brake.

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
