//! tt-bench — the core half of the performance gate.
//!
//! Answers one question: **did the engine get meaningfully slower?** It is a
//! regression detector, not a profiler, and it is deliberately the half that
//! has no window in it — the Qt shell's numbers depend on a Qt version this
//! container does not have (see `AGENTS.md`), while this half is the same
//! everywhere and can gate CI.
//!
//! ```text
//! tt-bench [--json] [--runs K] [--mb N] [--workload NAME] [--check]
//! ```
//!
//! Three things about the method are decisions rather than details:
//!
//! - **The minimum of K runs**, after one discarded warm-up. The fastest run is
//!   the one with the least interference from everything else on the machine;
//!   the mean measures the machine's other tenants as much as the code.
//! - **The stream arrives in 8 KB chunks**, the size `pty/mod.rs:263` reads,
//!   rather than as one slice. Feeding a whole file is not what any transport
//!   does, and it skips `Vt::held` entirely — the path that exists because
//!   `vte` drops bytes when it resumes a split UTF-8 sequence.
//! - **The corpus is generated, not committed.** Ten megabytes per workload
//!   from a fixed seed: identical on every machine and in every run, and no
//!   large binary in the repository.
//!
//! `--check` compares against the floors below, which is what CI runs. The
//! floors are a claim about the *code* — an accidental quadratic, a per-byte
//! allocation — not about the machine, so they sit far enough below a real
//! measurement that a slow shared runner cannot trip them.

use std::hint::black_box;
use std::io::Write;
use std::time::{Duration, Instant};

use tt_vt::{Config, CrReceive, TermId, Vt};

/// Megabytes per second below which something is structurally wrong rather
/// than merely slow — an accidental quadratic, a per-byte allocation, a
/// scrollback that copies. One number for all three workloads, because they
/// measure within 20% of each other and a floor that tracked each of them
/// would be a baseline wearing a floor's clothes.
///
/// It sits an order of magnitude under what the development machine measures
/// (64–78 MB/s, 2026-08-08). A shared CI runner is slower than a desktop and
/// how much slower is not something this gate should have an opinion about;
/// the *baseline* in `bench/` is where same-machine drift is caught.
///
/// Raising this is a decision, not maintenance. Ratcheting it towards the
/// machine of the day is how a floor becomes a flaky gate.
const FLOOR_MB_S: f64 = 5.0;

/// Iterations of the calibration loop. Sized to land in the low tens of
/// milliseconds on a current desktop: long enough to swamp the clock, short
/// enough that running it before every measurement costs nothing.
const CALIB_ITERS: u64 = 40_000_000;

const USAGE: &str = "usage: tt-bench [--json] [--runs K] [--mb N] [--workload NAME] [--check]\n\
                     \x20      tt-bench --emit NAME [--mb N]   # the corpus, on stdout\n\
                     \x20              workloads: plain, sgr, fullscreen\n";

fn main() {
    let args = match Args::parse() {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(msg) => {
            eprint!("{msg}");
            std::process::exit(2);
        }
    };

    // The corpus, on stdout and nothing else. `shell/tests/bench_shell.cpp`
    // runs this on the far end of a pty, so the bytes the *window* has to get
    // through are the same bytes measured here — which is what makes the
    // difference between the two numbers mean "the window".
    if let Some(name) = args.emit {
        let corpus = generate(&name, args.mb);
        if let Err(e) = std::io::stdout().write_all(&corpus) {
            // A reader that hung up is how this ends when the window closes
            // early. It is not a failure worth a message.
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                eprintln!("tt-bench: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // A debug build measures rustc's inlining decisions, not the engine's
    // shape, and the number it gives is wrong by about an order of magnitude —
    // more than any regression this exists to catch. Refusing is better than a
    // footnote nobody reads.
    //
    // After `--emit`, deliberately: generating bytes is not a measurement, and
    // the shell's benchmark needs the corpus whichever way its own build tree
    // was configured.
    if cfg!(debug_assertions) {
        eprintln!(
            "tt-bench: built without optimisation — the numbers would be \
             meaningless.\n          run `cargo run --release -p tt-bench`."
        );
        std::process::exit(2);
    }

    let calib = calibrate();

    let mut results = Vec::new();
    for name in &args.workloads {
        let corpus = generate(name, args.mb);
        let best = best_of(args.runs, || feed(&corpus));
        let mb = corpus.len() as f64 / (1024.0 * 1024.0);
        results.push(Measurement {
            name: name.clone(),
            bytes: corpus.len(),
            secs: best.as_secs_f64(),
            mb_per_s: mb / best.as_secs_f64(),
        });
    }

    if args.json {
        print_json(calib, &results);
    } else {
        print_table(calib, &results);
    }

    if args.check {
        let under: Vec<&Measurement> = results.iter().filter(|m| m.mb_per_s < FLOOR_MB_S).collect();
        for m in &under {
            eprintln!(
                "tt-bench: {} at {:.1} MB/s is under the {FLOOR_MB_S:.1} MB/s floor",
                m.name, m.mb_per_s
            );
        }
        if !under.is_empty() {
            std::process::exit(1);
        }
    }
}

struct Measurement {
    name: String,
    bytes: usize,
    secs: f64,
    mb_per_s: f64,
}

// --- the measurement --------------------------------------------------------

/// One warm-up, discarded, then the fastest of `runs`.
fn best_of(runs: u32, mut f: impl FnMut() -> Duration) -> Duration {
    f();
    (0..runs).map(|_| f()).min().unwrap_or_default()
}

/// The whole of what is timed: a fresh engine, then the corpus in transport-
/// sized chunks.
///
/// The scrollback is upstream's shipped ceiling, so ten megabytes of lines
/// wraps the ring some fifteen times over. That is the point — a scrollback
/// that costs O(depth) per line is invisible in a suite that never fills one.
fn feed(corpus: &[u8]) -> Duration {
    let mut vt = Vt::new(Config {
        cols: 80,
        rows: 24,
        term_id: TermId::Vt100,
        cr_receive: CrReceive::Cr,
        ..Config::default()
    });

    let start = Instant::now();
    for chunk in corpus.chunks(8192) {
        vt.feed(chunk);
        // The replies a stream like this provokes are few, but a benchmark
        // that let them accumulate would be measuring a `Vec` nobody drains.
        // The session drains it every pump.
        if !vt.reply().is_empty() {
            black_box(vt.take_reply());
        }
    }
    let elapsed = start.elapsed();

    // Nothing here can be elided across the crate boundary, but reading the
    // grid back also asserts the workload reached it: a corpus that parsed to
    // an empty screen would otherwise post a very impressive number.
    black_box(vt.grid().line(0).len());
    black_box(vt.grid().scrolled_off());
    elapsed
}

/// A fixed unit of integer work, measured on this machine right now.
///
/// Every metric is reported raw *and* divided by this, and the baseline stores
/// the normalised figure — so a baseline recorded on a fast machine still
/// roughly holds on a slow one, and a machine that is merely busy today shows
/// up as a slow calibration rather than as a regression in the engine.
///
/// Borrowed from `../tine/docs/BENCH.md`, which arrived at it the hard way.
fn calibrate() -> Duration {
    let run = || {
        let start = Instant::now();
        let mut x: u64 = 0x243f_6a88_85a3_08d3;
        for _ in 0..CALIB_ITERS {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        }
        black_box(x);
        start.elapsed()
    };
    best_of(2, run)
}

// --- the corpora ------------------------------------------------------------

/// A linear congruential generator, so a corpus is byte-identical on every
/// machine without carrying ten megabytes in the repository.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn upto(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "kernel",
    "usb",
    "serial",
    "ttyUSB0",
    "probe",
    "attached",
    "driver",
    "ftdi_sio",
    "device",
    "descriptor",
    "endpoint",
    "bulk",
    "interrupt",
    "config",
    "failed",
    "retry",
    "timeout",
    "reset",
    "link",
    "up",
    "eth0",
    "dhcp",
    "lease",
    "route",
    "gateway",
    "resolv",
    "mount",
    "ext4",
    "clean",
    "blocks",
    "systemd",
    "started",
    "session",
    "login",
    "shell",
    "bash",
    "make",
    "cc",
    "warning",
    "error",
    "note",
    "expected",
    "here",
    "0x7fff",
    "returned",
    "exit",
    "status",
];

fn generate(name: &str, mb: usize) -> Vec<u8> {
    let target = mb * 1024 * 1024;
    let mut rng = Lcg(0x5eed_1234_5678_9abc);
    let mut out = Vec::with_capacity(target + 4096);
    while out.len() < target {
        match name {
            "plain" => line(&mut out, &mut rng, false),
            "sgr" => line(&mut out, &mut rng, true),
            "fullscreen" => frame(&mut out, &mut rng),
            _ => unreachable!("workload names are validated in parse()"),
        }
    }
    // Cut to exactly the size asked for, mid-sequence if that is where it
    // falls. `--emit` hands this same corpus to the shell's benchmark through
    // a pty, and the two numbers are only subtractable if both engines are
    // given the same bytes down to the last one.
    out.truncate(target);
    out
}

/// A line of a log, CRLF-terminated because that is what arrives from a pty
/// (ONLCR) and from every console server on a serial line. With `--workload
/// sgr`, the colouring a build log or `ls --color` puts on it: 256-colour SGR,
/// which is the mode Tera Term ships with on.
fn line(out: &mut Vec<u8>, rng: &mut Lcg, colour: bool) {
    let words = 4 + rng.upto(9) as usize;
    for i in 0..words {
        if i > 0 {
            out.push(b' ');
        }
        if colour && rng.upto(3) == 0 {
            out.extend_from_slice(b"\x1b[38;5;");
            out.extend_from_slice(rng.upto(256).to_string().as_bytes());
            out.push(b'm');
        }
        out.extend_from_slice(WORDS[rng.upto(WORDS.len() as u64) as usize].as_bytes());
    }
    if colour {
        out.extend_from_slice(b"\x1b[0m");
    }
    out.extend_from_slice(b"\r\n");
}

/// What a full-screen program repainting itself looks like: absolute cursor
/// positioning, erase-to-end-of-line, a reverse-video header, and the cursor
/// hidden across the frame. `htop` and `vim` in a loop, roughly.
///
/// It is here because it is the workload the scrolling ones cannot reach — no
/// line ever leaves the page, so the cost is all in the escape parser and in
/// writing over cells that already hold something.
fn frame(out: &mut Vec<u8>, rng: &mut Lcg) {
    out.extend_from_slice(b"\x1b[?25l\x1b[H");
    for row in 1..=24 {
        out.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        if row == 1 {
            out.extend_from_slice(b"\x1b[7m");
        }
        let words = 3 + rng.upto(6) as usize;
        for i in 0..words {
            if i > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(WORDS[rng.upto(WORDS.len() as u64) as usize].as_bytes());
        }
        if row == 1 {
            out.extend_from_slice(b"\x1b[0m");
        }
        out.extend_from_slice(b"\x1b[K");
    }
    out.extend_from_slice(b"\x1b[?25h");
}

// --- reporting --------------------------------------------------------------

fn print_table(calib: Duration, results: &[Measurement]) {
    let calib_ms = calib.as_secs_f64() * 1000.0;
    println!("calibration  {calib_ms:8.1} ms  (a fixed unit of work on this machine)");
    println!();
    println!(
        "{:<12} {:>10} {:>12} {:>14}",
        "workload", "MB", "seconds", "MB/s"
    );
    for m in results {
        println!(
            "{:<12} {:>10.1} {:>12.3} {:>14.1}",
            m.name,
            m.bytes as f64 / (1024.0 * 1024.0),
            m.secs,
            m.mb_per_s
        );
    }
}

/// Hand-written rather than `serde`, because the whole document is four keys
/// and a list of four-key objects, and a dependency here would be carried by
/// every build of the workspace to serialise it.
fn print_json(calib: Duration, results: &[Measurement]) {
    println!("{{");
    println!("  \"calib_ms\": {:.3},", calib.as_secs_f64() * 1000.0);
    println!("  \"workloads\": {{");
    for (i, m) in results.iter().enumerate() {
        let comma = if i + 1 == results.len() { "" } else { "," };
        println!(
            "    \"{}\": {{ \"bytes\": {}, \"secs\": {:.6}, \"mb_per_s\": {:.3} }}{}",
            m.name, m.bytes, m.secs, m.mb_per_s, comma
        );
    }
    println!("  }}");
    println!("}}");
}

// --- arguments --------------------------------------------------------------

struct Args {
    json: bool,
    check: bool,
    runs: u32,
    mb: usize,
    workloads: Vec<String>,
    /// Write the corpus to stdout instead of measuring anything.
    emit: Option<String>,
}

const ALL_WORKLOADS: &[&str] = &["plain", "sgr", "fullscreen"];

impl Args {
    fn parse() -> Result<Option<Args>, String> {
        let mut args = Args {
            json: false,
            check: false,
            runs: 5,
            mb: 10,
            workloads: Vec::new(),
            emit: None,
        };
        let mut argv = std::env::args().skip(1);
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    print!("{USAGE}");
                    return Ok(None);
                }
                "--json" => args.json = true,
                "--check" => args.check = true,
                "--runs" => args.runs = next_num(&mut argv, "--runs")? as u32,
                "--mb" => args.mb = next_num(&mut argv, "--mb")?,
                "--workload" | "--emit" => {
                    let name = argv
                        .next()
                        .ok_or_else(|| format!("tt-bench: {arg} needs a name\n{USAGE}"))?;
                    if !ALL_WORKLOADS.contains(&name.as_str()) {
                        return Err(format!("tt-bench: unknown workload '{name}'\n{USAGE}"));
                    }
                    if arg == "--emit" {
                        args.emit = Some(name);
                    } else {
                        args.workloads.push(name);
                    }
                }
                _ => return Err(format!("tt-bench: unexpected argument '{arg}'\n{USAGE}")),
            }
        }
        if args.workloads.is_empty() {
            args.workloads = ALL_WORKLOADS.iter().map(|s| s.to_string()).collect();
        }
        if args.runs == 0 || args.mb == 0 {
            return Err("tt-bench: --runs and --mb must be at least 1\n".to_string());
        }
        Ok(Some(args))
    }
}

fn next_num(argv: &mut impl Iterator<Item = String>, flag: &str) -> Result<usize, String> {
    argv.next()
        .ok_or_else(|| format!("tt-bench: {flag} needs a number\n{USAGE}"))?
        .parse()
        .map_err(|_| format!("tt-bench: {flag} needs a number\n{USAGE}"))
}
