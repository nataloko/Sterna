# bench

The performance gate. `PLAN.md` says "simple, light, performant" is a claim and
that CI should enforce it; this is where the numbers come from.

```sh
./bench/bench.py                 # measure, and compare against baseline.json
./bench/bench.py --core          # the Rust half only — no Qt, runs anywhere
./bench/bench.py --update        # re-record the baseline, on a QUIET machine
./bench/bench.py --json out.json # keep the raw numbers as well

crates/target/release/tt-bench   # the core half, on its own
shell/build-release/bench_shell  # the shell half, on its own
```

The Qt half needs the `termitta-fedora` container and its `$HOME/.local/bin` on
`PATH` for `uv`:

```sh
distrobox-host-exec distrobox enter termitta-fedora --no-tty -- bash -lc '
  export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
  cd ~/Projects/termitta
  cmake -S shell -B shell/build-release -G Ninja -DCMAKE_BUILD_TYPE=Release
  cmake --build shell/build-release --target bench_shell
  ./bench/bench.py'
```

## The two halves are gated differently, and that is the design

**The core half** — `crates/tt-bench` — feeds ten megabytes through `tt-vt` and
`tt-grid` in 8 KB chunks, the size a pty read returns. No window, no display,
the same everywhere. It is the half CI can run, and CI checks it against an
**absolute floor** (`FLOOR_MB_S`, an order of magnitude below a real
measurement) rather than against the baseline here: a floor catches an
accidental quadratic or a per-byte allocation, and cannot flake because a
shared runner had a bad minute.

**The shell half** — `shell/tests/bench_shell.cpp` — measures the window, and
so is a **local** gate and never a CI one. Qt 6.4.2 in the Ubuntu container is
seven releases behind the desktop's 6.11.1 and has already produced one false
finding and one set of numbers flattering by 2x (see `CLAUDE.md`). Build it in
`termitta-fedora`, in Release, and run it on the real desktop.

## What is measured

| | |
|---|---|
| `core.plain` | a scrolling log, CRLF-terminated, the `cat` case |
| `core.sgr` | the same coloured with 256-colour SGR — a build log, `ls --color` |
| `core.fullscreen` | a program repainting 24 rows in place; nothing ever scrolls |
| `shell.start_ms` | **exec** to the first frame — the dynamic loader included |
| `shell.idle_rss_mb` / `pss_mb` | with a shell attached and nothing arriving |
| `shell.latency_ms` | a keystroke to the frame that shows it |
| `shell.throughput_mb_s` | 10 MB out of a pty, painted, first byte to hangup |

The corpus is generated from a fixed seed rather than committed, and the shell's
throughput runs `tt-bench --emit` on the far end of the pty — **the same bytes**
the engine is measured on alone, so the two numbers subtract into what the
window costs.

Three measurement decisions worth knowing:

- **The minimum of K runs**, after a discarded warm-up. The fastest run is the
  one least disturbed by everything else on the machine; a mean measures the
  machine's other tenants.
- **Each shell metric is a child process.** Most of a cold start happens before
  `main` — the loader resolving Qt is a real part of what a user waits for — so
  only a launcher can time it.
- **The throughput clock starts at the first byte**, not at the fork. Spawning
  the emitter and letting it generate ten megabytes is tens of milliseconds
  belonging to neither engine nor window, and including them made a 2 MB run
  look four times slower than a 10 MB one.

## What keeps it from being a flaky gate

Borrowed wholesale from `../tine/docs/BENCH.md`, which arrived at all of this
the hard way.

- **Calibration.** Every run first measures a fixed unit of integer work. The
  baseline records its own, and every timing is scaled by the ratio — so a
  baseline recorded on a fast machine still roughly holds on a slow one. Above
  **1.5x** the run reports `UNRELIABLE` and gates nothing: the machine is
  loaded or throttled, which is not a fact about the code.
- **Same machine for the hard gate.** tine tried cross-machine normalisation
  and found the calibration loop too unlike the work being measured to trust
  it. A baseline from a different CPU is advisory and cannot fail a run.
- **The Qt version is part of the machine's identity**, not just the platform
  name: 6.4.2 and 6.11.1 both answer `"wayland"`. A shell metric measured under
  a different Qt is advisory; the core metrics still gate.
- **Budgets are per metric**, each above that metric's own noise: 15% for
  memory, which is nearly exact run to run, and 40% for keystroke latency,
  which waits on a scheduler.

**Record the baseline on a quiet machine.** The first one taken here was
recorded while a `cmake` build was finishing and came out 14% under the truth,
which would have been a permanently weaker gate that nothing downstream could
detect. The calibration loop did *not* catch it — it was 1.5% slow while the
engine was 14% slow — which is the honest limit of that technique: it corrects
for a slower machine, not for a busier one.

## The numbers, and the one that is a finding

Measured 2026-08-08 on an AMD Ryzen 7 7840HS, Fedora 44, Qt 6.11.1, Wayland —
`baseline.json`.

| | |
|---|---|
| core, plain / sgr / fullscreen | 67 / 74 / 84 MB/s |
| shell, exec → first frame | 68 ms |
| shell, idle RSS / PSS | 64.5 / 40.5 MB |
| shell, keystroke → frame | 1.03 ms |
| shell, 10 MB out of a pty | 39 MB/s, in ~390 frames |

**Throughput through the window is dominated by how many frames get painted,
and that is a platform property, not a code one.** The same binary on the same
machine over the same 10 MB:

| platform | frames | throughput |
|---|---:|---:|
| wayland | ~390 | 27–39 MB/s |
| offscreen | ~2900 | 7 MB/s |
| xcb | ~3000 | 4 MB/s |

Wayland's frame callbacks throttle repainting to the compositor, so several
8 KB reads coalesce into one frame. X11 and the offscreen platform have no such
brake and paint once per read — the session pumps once per notifier wake, so
each read is its own turn of the event loop and its own frame. **X11 is
therefore 6–9x slower at absorbing a burst**, which is a real finding about the
shell rather than about the benchmark, and it is on `PLAN.md`'s list.

It also means a headless CI number would understate the desktop's by 4x, in the
opposite direction from the usual assumption about offscreen being faster.
