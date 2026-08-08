#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
The performance gate: measure, then compare against `baseline.json`.

    ./bench/bench.py                    # measure and compare
    ./bench/bench.py --update           # re-record the baseline (QUIET machine)
    ./bench/bench.py --json out.json    # ...and keep the raw numbers
    ./bench/bench.py --core             # skip the Qt half

Two halves, and they are gated differently on purpose.

The **core** half (`crates/tt-bench`) is a Rust binary with no window in it. It
runs anywhere, including CI, and CI checks it against an absolute floor rather
than against this baseline -- see `tt-bench`'s own `FLOOR_MB_S`.

The **shell** half (`shell/build*/bench_shell`) needs Qt 6.11.1 and a real
compositor, so it is a local gate and never a CI one. Build it in
`termitta-fedora` with `-DCMAKE_BUILD_TYPE=Release`; measured on the container's
Qt 6.4.2 or in a Debug build the numbers are wrong in ways that have already
fooled this project once (see CLAUDE.md).

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
under a different Qt platform, is reported as advisory and cannot fail the run.

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

# How much worse than the baseline a metric may be before it is a regression.
# Each sits above that metric's own noise, which is why they differ: memory is
# nearly exact run to run, a keystroke's latency is the noisiest thing here
# because it waits on a scheduler.
BUDGETS = {
    "core.plain": 0.20,
    "core.sgr": 0.20,
    "core.fullscreen": 0.20,
    "shell.start_ms": 0.30,
    "shell.idle_rss_mb": 0.15,
    "shell.idle_pss_mb": 0.15,
    "shell.latency_ms": 0.40,
    "shell.throughput_mb_s": 0.30,
}

# Metrics where a bigger number is better. Everything else is a cost.
HIGHER_IS_BETTER = {"core.plain", "core.sgr", "core.fullscreen", "shell.throughput_mb_s"}

# Memory does not get faster on a faster machine, so scaling it by the
# calibration would import CPU noise into the one metric that has none.
NOT_CPU_BOUND = {"shell.idle_rss_mb", "shell.idle_pss_mb"}

# A calibration this much worse than the baseline's means the machine is busy,
# not that the code is slow.
UNRELIABLE_ABOVE = 1.5


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


def find_shell_bench() -> Path | None:
    """A Release build tree first: a Debug one measures -O0 Qt calls."""
    for name in ("build-release", "build"):
        exe = ROOT / "shell" / name / "bench_shell"
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
        # `git log` is not where they will look.
        "recorded": date.today().isoformat(),
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

    shell_exe = find_shell_bench()
    if not shell_exe:
        sys.stderr.write(
            "bench: no bench_shell -- the Qt half is skipped.\n"
            "       build it in termitta-fedora:\n"
            "         cmake -S shell -B shell/build-release -G Ninja "
            "-DCMAKE_BUILD_TYPE=Release\n"
            "         cmake --build shell/build-release --target bench_shell\n")
        return record

    shell = run_json([shell_exe, "--json", "--runs", args.runs, "--mb", args.mb])
    if not shell:
        # The likeliest cause by far, and the loader's own message ("version
        # `Qt_6.11' not found") does not say it: `shell/build-release` is a
        # directory both containers can see and only one of them built. Run
        # this in termitta-fedora, or with --core.
        sys.stderr.write(
            f"       {shell_exe.relative_to(ROOT)} may have been built in the "
            "other container.\n       The Qt half belongs in termitta-fedora; "
            "elsewhere use --core.\n")
        return None
    if shell.get("failed_probes"):
        sys.stderr.write(f"bench: {shell['failed_probes']} shell probe(s) failed\n")
        return None
    record["machine"]["qt_platform"] = shell["platform"]
    record["machine"]["qt"] = shell["qt"]
    for key in ("start_ms", "idle_rss_mb", "idle_pss_mb", "latency_ms", "throughput_mb_s"):
        record["metrics"][f"shell.{key}"] = shell[key]
    record["shell_paints"] = shell["throughput_paints"]
    return record


def compare(now: dict, base: dict) -> int:
    """Print a table, and return the number of metrics that regressed."""
    same_cpu = now["machine"]["cpu"] == base["machine"].get("cpu")
    # The Qt *version* is part of this, not only the platform name: 6.4.2 and
    # 6.11.1 both answer "wayland" and are seven releases apart, which is the
    # gap that has already produced one false finding in this project.
    same_qt = (now["machine"].get("qt_platform") == base["machine"].get("qt_platform")
               and now["machine"].get("qt") == base["machine"].get("qt"))
    ratio = now["calib_ms"] / base["calib_ms"]
    reliable = ratio <= UNRELIABLE_ABOVE

    print(f"{'metric':<24} {'baseline':>10} {'now':>10} {'change':>9}")
    regressed = []
    advisory = []
    for key, value in now["metrics"].items():
        want = base["metrics"].get(key)
        if want is None:
            print(f"{key:<24} {'--':>10} {value:>10.2f}      new")
            continue
        # Scale the baseline to this machine before comparing. A slower machine
        # is expected to produce a slower number, and that is not a regression.
        expected = want if key in NOT_CPU_BOUND else (
            want / ratio if key in HIGHER_IS_BETTER else want * ratio)
        change = (value - expected) / expected
        if key in HIGHER_IS_BETTER:
            change = -change
        # A shell metric measured under a different Qt, or a different
        # platform plugin, is not comparable: on this machine the same code
        # measures 36 MB/s under Wayland and 4 under xcb, because one throttles
        # frames and the other does not.
        gates = reliable and same_cpu and (same_qt or not key.startswith("shell."))
        budget = BUDGETS.get(key, 0.30)
        flag = ""
        if change > budget:
            flag = "  REGRESSED" if gates else "  worse (advisory)"
            (regressed if gates else advisory).append(key)
        elif change < -budget:
            flag = "  faster"
        print(f"{key:<24} {want:>10.2f} {value:>10.2f} {change:>+8.0%}{flag}")

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
    if not same_qt and any(k.startswith("shell.") for k in now["metrics"]):
        print(f"ADVISORY for the shell half: the baseline is Qt "
              f"{base['machine'].get('qt')} on "
              f"{base['machine'].get('qt_platform')}, this is "
              f"{now['machine'].get('qt')} on {now['machine'].get('qt_platform')}.")
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
    p.add_argument("--core", action="store_true", help="skip the Qt half")
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
        BASELINE.write_text(json.dumps(record, indent=2) + "\n")
        print(f"recorded {BASELINE.relative_to(ROOT)}:")
        for key, value in record["metrics"].items():
            print(f"  {key:<24} {value:>10.2f}")
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
