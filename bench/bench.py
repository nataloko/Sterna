#!/usr/bin/env python3
# Needs Python 3.11 or later. No third-party packages.
"""
The performance gate: measure, then compare against `baseline.json`.

    ./bench/bench.py                    # measure and compare
    ./bench/bench.py --update           # re-record the baseline (QUIET machine)
    ./bench/bench.py --json out.json    # ...and keep the raw numbers
    ./bench/bench.py --core             # skip the shell half entirely
    ./bench/bench.py --frontend qt      # ...or measure just the one

Two halves, and they are gated differently on purpose.

The **core** half (`crates/tt-bench`) is a Rust binary with no window in it. It
runs anywhere, including CI, and CI checks it against an absolute floor rather
than against this baseline -- see `tt-bench`'s own `FLOOR_MB_S`.

The **shell** half needs a real compositor, so it is a local gate and never a
CI one. There is more than one shell: Qt is the shipping frontend and
`shell-iced` is the evaluation `PLAN.md` records. Each records under its own
metric prefix -- `shell.qt.start_ms`, `shell.iced.start_ms` -- so one baseline
holds both and neither displaces the other. Build the Qt one in
`sterna-fedora` with `-DCMAKE_BUILD_TYPE=Release`; measured on the container's
Qt 6.4.2 or in a Debug build the numbers are wrong in ways that have already
fooled this project once (see AGENTS.md).

## What keeps this from being a flaky gate

**Calibration.** Every run measures a fixed unit of integer work first. The
baseline records its own, and a comparison scales every timing by the ratio --
so a baseline recorded on a fast machine still roughly holds on a slow one. If
this machine's calibration is more than 1.5x the baseline's, the machine is
loaded or thermally throttled and the run reports UNRELIABLE rather than
failing.

**Same-machine only, for the hard gate.** `../tine/docs/BENCH.md` tried
cross-machine normalisation and found the calibration loop too unlike the work
being measured to trust. So a baseline recorded on a different machine, or
under a different toolkit build, is reported as advisory and cannot fail the
run. A frontend identifies itself by printing *strings* in its JSON -- Qt's
`platform` and `qt`, iced's `platform`, `iced` and `renderer` -- and a metric
gates only when the whole descriptor matches. Nothing here knows which of those
keys matters, which is the point: a renderer or a winit version added later is
part of the identity without an edit.

**The minimum of K runs**, not the mean, taken inside each half.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = Path(__file__).resolve().parent / "baseline.json"

# The shell frontends, each measured into its own metric prefix. `exes` is
# tried in order; a Release build tree first, because a Debug one measures -O0
# toolkit calls.
FRONTENDS = {
    "qt": {
        "exes": ("shell/build-release/bench_shell", "shell/build/bench_shell"),
        "hint": ("build it in sterna-fedora:\n"
                 "         cmake -S shell -B shell/build-release -G Ninja "
                 "-DCMAKE_BUILD_TYPE=Release\n"
                 "         cmake --build shell/build-release --target bench_shell"),
        # The likeliest cause of a failed run by far, and the loader's own
        # message ("version `Qt_6.11' not found") does not say it:
        # `shell/build-release` is a directory both containers can see and only
        # one of them built.
        "failed": ("may have been built in the other container.\n"
                   "       The Qt half belongs in sterna-fedora; elsewhere use "
                   "--core."),
    },
    "iced": {
        "exes": ("shell-iced/target/release/bench_iced",),
        "hint": ("build it with:\n"
                 "         cargo build --release --bin bench_iced "
                 "--manifest-path shell-iced/Cargo.toml"),
        "failed": "failed; see its stderr above.",
    },
}

# What a frontend's bench may report. Only what it actually prints is recorded,
# so a frontend that does not know a number simply has no such metric -- and an
# allowlist means a typo in a bench binary cannot quietly invent one.
#
# The last four are what Phase H compares packaging on; the Qt bench does not
# print them yet.
SHELL_METRICS = (
    "start_ms", "idle_rss_mb", "idle_pss_mb", "latency_ms", "throughput_mb_s",
    "binary_mb", "package_mb", "ldd_count", "build_s",
)

# How much worse than the baseline a metric may be before it is a regression.
# Each sits above that metric's own noise, which is why they differ: memory is
# nearly exact run to run, a keystroke's latency is the noisiest thing here
# because it waits on a scheduler, and a byte count has no noise at all.
#
# Keyed without the frontend segment -- `shell.start_ms` covers
# `shell.qt.start_ms` and `shell.iced.start_ms` alike, because the budget is a
# property of what is being measured, not of who is measuring it.
BUDGETS = {
    "core.plain": 0.20,
    "core.sgr": 0.20,
    "core.fullscreen": 0.20,
    "shell.start_ms": 0.30,
    "shell.idle_rss_mb": 0.15,
    "shell.idle_pss_mb": 0.15,
    "shell.latency_ms": 0.40,
    "shell.throughput_mb_s": 0.30,
    "shell.binary_mb": 0.10,
    "shell.package_mb": 0.10,
    "shell.ldd_count": 0.05,
    "shell.build_s": 0.30,
}

# Metrics where a bigger number is better. Everything else is a cost.
HIGHER_IS_BETTER = {"core.plain", "core.sgr", "core.fullscreen", "shell.throughput_mb_s"}

# Memory does not get faster on a faster machine, so scaling it by the
# calibration would import CPU noise into the one metric that has none. Nor
# does a binary shrink on one.
NOT_CPU_BOUND = {"shell.idle_rss_mb", "shell.idle_pss_mb",
                 "shell.binary_mb", "shell.package_mb", "shell.ldd_count"}

# A calibration this much worse than the baseline's means the machine is busy,
# not that the code is slow.
UNRELIABLE_ABOVE = 1.5


def kind_of(metric: str) -> str:
    """A metric's name with any frontend segment removed.

    `shell.qt.start_ms` -> `shell.start_ms`; `core.plain` unchanged. This is
    what the budget and direction tables are keyed on.
    """
    parts = metric.split(".")
    return f"{parts[0]}.{parts[-1]}" if len(parts) == 3 else metric


def env_of(metric: str) -> str | None:
    """The `machine` entry a metric's comparability depends on, if any.

    `shell.qt.start_ms` -> `shell.qt`. Core metrics answer None: they have no
    window and no toolkit, so nothing but the CPU makes them incomparable.
    """
    parts = metric.split(".")
    return f"{parts[0]}.{parts[1]}" if len(parts) == 3 else None


def cpu_model() -> str:
    """The CPU as /proc names it, so a baseline knows which machine it is from."""
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def find_core_bench(build: bool) -> Path | None:
    exe = ROOT / "crates" / "target" / "release" / "tt-bench"
    if build:
        cargo = os.environ.get("CARGO", "cargo")
        # `cargo` is on PATH only for login shells in the dev container, which
        # makes a missing binary look like a missing toolchain. It is not.
        env = dict(os.environ, PATH=f"{Path.home()/'.cargo'/'bin'}:{os.environ['PATH']}")
        r = subprocess.run([cargo, "build", "--release", "-p", "tt-bench"],
                           cwd=ROOT / "crates", env=env)
        if r.returncode != 0:
            return None
    return exe if exe.exists() else None


def find_frontend_bench(fe: str) -> Path | None:
    for rel in FRONTENDS[fe]["exes"]:
        exe = ROOT / rel
        if exe.exists():
            return exe
    return None


def run_json(argv: list[str]) -> dict | None:
    proc = subprocess.run([str(a) for a in argv], capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        sys.stderr.write(f"bench: {argv[0]} failed\n")
        return None
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(f"bench: {argv[0]} did not print JSON\n")
        return None


def measure(args) -> dict | None:
    record: dict = {
        # A baseline's age is the first thing anyone reading it wants, and
        # `git log` is not where they will look. It is per *section* because
        # the two frontends need not be measured on the same day -- and once
        # `--update` can carry a section over (see `merge_into`), one date for
        # the file would be a date the carried numbers do not have.
        "recorded": {"core": date.today().isoformat()},
        "machine": {"host": platform.node(), "cpu": cpu_model()},
        "metrics": {},
    }

    core_exe = find_core_bench(build=not args.no_build)
    if not core_exe:
        sys.stderr.write("bench: no tt-bench; `cargo build --release -p tt-bench`\n")
        return None
    core = run_json([core_exe, "--json", "--runs", args.runs, "--mb", args.mb])
    if not core:
        return None
    record["calib_ms"] = core["calib_ms"]
    for name, w in core["workloads"].items():
        record["metrics"][f"core.{name}"] = w["mb_per_s"]

    if args.core:
        return record

    wanted = list(FRONTENDS) if args.frontend == "all" else [args.frontend]
    measured = 0
    for fe in wanted:
        exe = find_frontend_bench(fe)
        if not exe:
            sys.stderr.write(f"bench: no {fe} shell bench -- that half is "
                             f"skipped.\n       {FRONTENDS[fe]['hint']}\n")
            continue
        if not measure_frontend(record, fe, exe, args):
            return None
        measured += 1

    # `--frontend qt` finding no bench_shell is a skipped half; asking for one
    # frontend and measuring nothing is a run that answered nothing at all.
    if args.frontend != "all" and measured == 0:
        return None
    return record


def measure_frontend(record: dict, fe: str, exe: Path, args) -> bool:
    """Measure one frontend into `record`. False means the run is not usable."""
    out = run_json([exe, "--json", "--runs", args.runs, "--mb", args.mb])
    if not out:
        sys.stderr.write(f"       {exe.relative_to(ROOT)} "
                         f"{FRONTENDS[fe]['failed']}\n")
        return False
    if out.get("failed_probes"):
        sys.stderr.write(f"bench: {out['failed_probes']} {fe} probe(s) failed\n")
        return False

    # A frontend identifies itself with strings and measures with numbers, so
    # nothing here has to know that Qt reports a `qt` version and iced reports
    # an `iced` one and a `renderer`. The whole descriptor is what the
    # comparability gate matches on.
    record["machine"][f"shell.{fe}"] = {
        k: v for k, v in out.items() if isinstance(v, str)}
    record["recorded"][f"shell.{fe}"] = date.today().isoformat()
    for key in SHELL_METRICS:
        if key in out:
            record["metrics"][f"shell.{fe}.{key}"] = out[key]
    if "throughput_paints" in out:
        record.setdefault("paints", {})[f"shell.{fe}"] = out["throughput_paints"]
    return True


def section_of(metric: str) -> str:
    """Which half of the baseline a metric belongs to: `core`, or a frontend."""
    return env_of(metric) or "core"


def merge_into(record: dict, base: dict) -> list[str]:
    """Carry sections of `base` that this run did not measure into `record`.

    Without this, `--update` on a machine where only one frontend is built
    deletes the other's numbers -- which is exactly the failure a side-by-side
    baseline exists to avoid, and it is silent. Returns the sections carried,
    so the caller can say so out loud.
    """
    mine = {section_of(k) for k in record["metrics"]}
    carried = sorted({section_of(k) for k in base.get("metrics", {})} - mine)
    for key, value in base.get("metrics", {}).items():
        if section_of(key) in carried:
            record["metrics"][key] = value
    for section in carried:
        if section in base.get("machine", {}):
            record["machine"][section] = base["machine"][section]
        was = base.get("recorded")
        record["recorded"][section] = (
            was if isinstance(was, str) else (was or {}).get(section, "unknown"))
        if section in base.get("paints", {}):
            record.setdefault("paints", {})[section] = base["paints"][section]
    return carried


def describe(env: dict | None) -> str:
    """A frontend's descriptor as one readable phrase."""
    if not env:
        return "not recorded"
    return ", ".join(f"{k} {v}" for k, v in sorted(env.items()))


def compare(now: dict, base: dict) -> int:
    """Print a table, and return the number of metrics that regressed."""
    same_cpu = now["machine"]["cpu"] == base["machine"].get("cpu")
    ratio = now["calib_ms"] / base["calib_ms"]
    reliable = ratio <= UNRELIABLE_ABOVE

    # The toolkit *version* is part of a frontend's identity, not only the
    # platform name: Qt 6.4.2 and 6.11.1 both answer "wayland" and are seven
    # releases apart, which is the gap that has already produced one false
    # finding in this project. Comparing the whole descriptor covers that and
    # whatever the next frontend decides identifies it.
    same_env = {
        fe: now["machine"].get(fe) == base["machine"].get(fe)
        for fe in set(now["machine"]) | set(base["machine"])
        if fe.startswith("shell.")
    }

    print(f"{'metric':<27} {'baseline':>10} {'now':>10} {'change':>9}")
    regressed = []
    advisory = []
    for key, value in now["metrics"].items():
        want = base["metrics"].get(key)
        kind = kind_of(key)
        if want is None:
            print(f"{key:<27} {'--':>10} {value:>10.2f}      new")
            continue
        # Scale the baseline to this machine before comparing. A slower machine
        # is expected to produce a slower number, and that is not a regression.
        expected = want if kind in NOT_CPU_BOUND else (
            want / ratio if kind in HIGHER_IS_BETTER else want * ratio)
        change = (value - expected) / expected
        if kind in HIGHER_IS_BETTER:
            change = -change
        # A shell metric measured under a different toolkit build, or a
        # different platform plugin, is not comparable: on this machine the
        # same code measures 36 MB/s under Wayland and 4 under xcb, because one
        # throttles frames and the other does not.
        env = env_of(key)
        gates = reliable and same_cpu and (env is None or same_env.get(env, False))
        budget = BUDGETS.get(kind, 0.30)
        flag = ""
        if change > budget:
            flag = "  REGRESSED" if gates else "  worse (advisory)"
            (regressed if gates else advisory).append(key)
        elif change < -budget:
            flag = "  faster"
        print(f"{key:<27} {want:>10.2f} {value:>10.2f} {change:>+8.0%}{flag}")

    print()
    print(f"calibration {base['calib_ms']:.1f} ms baseline -> {now['calib_ms']:.1f} ms "
          f"here ({ratio:.2f}x)")

    if not reliable:
        print("UNRELIABLE: this machine is more than 1.5x slower at the fixed "
              "loop than the\nbaseline's was. It is loaded or throttled; re-run "
              "it cooler. Nothing gates.")
        return 0
    if not same_cpu:
        print(f"ADVISORY: the baseline is from {base['machine'].get('cpu')}, "
              f"this is {now['machine']['cpu']}.\nCross-machine numbers do not "
              "gate -- see the note at the top of this script.")
        return 0
    for fe, ok in sorted(same_env.items()):
        # A frontend the baseline has never seen needs no advisory: every one
        # of its metrics has already printed "new".
        if ok or not base["machine"].get(fe):
            continue
        if not any(env_of(k) == fe for k in now["metrics"]):
            continue
        print(f"ADVISORY for {fe}: the baseline is "
              f"{describe(base['machine'].get(fe))}, this is "
              f"{describe(now['machine'].get(fe))}.")
    if advisory:
        print(f"{len(advisory)} metric(s) worse, advisory only: {', '.join(advisory)}")
    if regressed:
        print(f"{len(regressed)} metric(s) regressed: {', '.join(regressed)}")
    return len(regressed)


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--update", action="store_true",
                   help="re-record baseline.json from this run")
    p.add_argument("--json", metavar="PATH", help="also write the raw numbers here")
    p.add_argument("--core", action="store_true", help="skip the shell half")
    p.add_argument("--frontend", choices=("all", *FRONTENDS), default="all",
                   help="which shell to measure (default: every one built)")
    p.add_argument("--no-build", action="store_true", help="do not run cargo first")
    p.add_argument("--runs", default="5", help="runs per measurement (default 5)")
    p.add_argument("--mb", default="10", help="megabytes per throughput run (default 10)")
    args = p.parse_args()

    record = measure(args)
    if record is None:
        return 2

    if args.json:
        Path(args.json).write_text(json.dumps(record, indent=2) + "\n")

    if args.update:
        carried = []
        if BASELINE.exists():
            carried = merge_into(record, json.loads(BASELINE.read_text()))
        BASELINE.write_text(json.dumps(record, indent=2) + "\n")
        print(f"recorded {BASELINE.relative_to(ROOT)}:")
        for key, value in sorted(record["metrics"].items()):
            when = record["recorded"].get(section_of(key), "")
            age = f"  (carried, {when})" if section_of(key) in carried else ""
            print(f"  {key:<27} {value:>10.2f}{age}")
        if carried:
            print(f"\n{len(carried)} section(s) not measured here and carried "
                  f"over unchanged: {', '.join(carried)}.")
        print("\nRead it before committing. A baseline recorded on a busy "
              "machine is a\nweaker gate forever, and nothing downstream can "
              "tell.")
        return 0

    if not BASELINE.exists():
        sys.stderr.write("bench: no baseline yet -- run with --update\n")
        return 2
    return 1 if compare(record, json.loads(BASELINE.read_text())) else 0


if __name__ == "__main__":
    sys.exit(main())
