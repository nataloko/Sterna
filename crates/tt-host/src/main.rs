//! tt-host — a terminal with no window, so that a program can be run *inside*
//! one.
//!
//! ```text
//! tt-host [--cols N] [--rows N] [--term NAME] [--term-id ID]
//!         [--decrqcra] [--timeout SECS] [--dump] -- COMMAND [ARG...]
//! ```
//!
//! `tt-dump` drives the engine over a byte stream and prints what the grid
//! ended up holding. That is enough for the differential suite, whose cases are
//! recordings, and not enough for a **conformance** suite, whose whole method
//! is to write an escape sequence and then read the answer back. `esctest` runs
//! as an ordinary program on a pty and talks to whatever terminal is on the
//! other end; this is that terminal.
//!
//! It is deliberately the same stack the Qt shell runs — [`Session`] over
//! `tt-conn`'s pty, waiting on `poll_fd` — rather than a private loop around
//! [`Vt`](tt_vt::Vt). A conformance suite that exercised a second, simpler
//! implementation of the loop would be answering a question nobody asked.
//!
//! The exit status is about the *hosting*, not about the program: 0 once the
//! child has hung the line up, 1 if it had to be given up on, 2 for a bad
//! command line. Whatever the program concluded is in whatever the program
//! wrote.

use std::time::{Duration, Instant};

use tt_conn::pty::{PtyConn, PtyParams};
use tt_session::{Event, Session};
use tt_vt::{Config, TermId};

const USAGE: &str = "usage: tt-host [--cols N] [--rows N] [--term NAME] [--term-id ID]\n\
                     \x20              [--decrqcra] [--timeout SECS] [--dump] -- COMMAND [ARG...]\n";

struct Args {
    cols: usize,
    rows: usize,
    term: String,
    term_id: TermId,
    decrqcra: bool,
    timeout: Duration,
    dump: bool,
    argv: Vec<String>,
}

fn main() -> std::process::ExitCode {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return std::process::ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            return std::process::ExitCode::from(2);
        }
    };

    let conn = match PtyConn::open(&PtyParams {
        argv: args.argv.clone(),
        term: args.term.clone(),
        // The program is named on the command line, so it is run as itself.
        // A login shell would rewrite argv[0] and look that name up on PATH.
        login_shell: false,
        cols: args.cols as u16,
        rows: args.rows as u16,
        ..PtyParams::default()
    }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tt-host: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let mut session = Session::new(Config {
        cols: args.cols,
        rows: args.rows,
        term_id: args.term_id,
        decrqcra: args.decrqcra,
        ..Config::default()
    });
    session.connect(Box::new(conn));

    let code = run(&mut session, args.timeout);

    if args.dump {
        dump(&session);
    }
    if let Some(note) = session.close_note() {
        eprintln!("tt-host: {note}");
    }
    code
}

/// Pump until the child hangs up, or until the deadline says it never will.
fn run(session: &mut Session, timeout: Duration) -> std::process::ExitCode {
    let deadline = Instant::now() + timeout;

    while session.is_connected() {
        if Instant::now() >= deadline {
            eprintln!("tt-host: gave up after {} seconds", timeout.as_secs());
            return std::process::ExitCode::from(1);
        }

        // The same wait the shell does with a `QSocketNotifier`, and for the
        // same reason: a pump with nothing to read returns straight away, so
        // looping on it without waiting for the descriptor is a busy loop.
        //
        // The one thing a readable descriptor cannot cover is output the far
        // end refused. A short write leaves bytes pending and nothing will
        // arrive to wake us — the child is busy writing, not reading — so the
        // wait becomes a short one until they have gone out.
        let wait = if session.pending_out() > 0 { 20 } else { 200 };
        if let Some(fd) = session.poll_fd() {
            wait_readable(fd, wait);
        }

        // A budget of zero reads exactly once. A burst therefore arrives over
        // several turns of this loop, which is what keeps one screenful of
        // output from starving the reply path.
        if let Err(e) = session.pump(Duration::ZERO) {
            eprintln!("tt-host: {e}");
            return std::process::ExitCode::from(1);
        }
        for ev in session.drain_events() {
            if let Event::Disconnected = ev {
                return std::process::ExitCode::SUCCESS;
            }
        }
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(unix)]
fn wait_readable(fd: std::os::unix::io::RawFd, ms: libc::c_int) {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pfd, 1, ms) };
}

#[cfg(not(unix))]
fn wait_readable(_fd: std::os::unix::io::RawFd, _ms: i32) {}

/// The final screen, for looking at when a run goes wrong. Not a dump format —
/// `tt-dump` owns that, and it is the one the two engines are compared in.
fn dump(session: &Session) {
    let grid = session.grid();
    for y in 0..grid.rows() {
        let mut line = String::new();
        for cell in grid.line(y) {
            if cell.width_class == tt_grid::WIDTH_PAD {
                continue;
            }
            match cell.codepoints().next() {
                Some(cp) => line.push(char::from_u32(cp).unwrap_or('?')),
                None => line.push(' '),
            }
        }
        eprintln!("{}", line.trim_end());
    }
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        cols: 80,
        rows: 24,
        term: PtyParams::default().term,
        term_id: TermId::Vt100,
        decrqcra: false,
        timeout: Duration::from_secs(900),
        dump: false,
        argv: Vec::new(),
    };

    let mut it = std::env::args().skip(1);
    let want = |it: &mut dyn Iterator<Item = String>, flag: &str| -> Result<String, String> {
        it.next()
            .ok_or_else(|| format!("tt-host: {flag} needs a value"))
    };

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "--cols" => args.cols = number(&want(&mut it, "--cols")?, "--cols")?,
            "--rows" => args.rows = number(&want(&mut it, "--rows")?, "--rows")?,
            "--term" => args.term = want(&mut it, "--term")?,
            "--term-id" => {
                let name = want(&mut it, "--term-id")?;
                args.term_id = TermId::parse(&name)
                    .ok_or_else(|| format!("tt-host: unknown terminal id '{name}'"))?;
            }
            "--decrqcra" => args.decrqcra = true,
            "--timeout" => {
                args.timeout =
                    Duration::from_secs(number(&want(&mut it, "--timeout")?, "--timeout")? as u64)
            }
            "--dump" => args.dump = true,
            "--" => {
                args.argv.extend(it.by_ref());
                break;
            }
            other => return Err(format!("tt-host: unknown option '{other}'\n{USAGE}")),
        }
    }

    if args.argv.is_empty() {
        return Err(format!("tt-host: nothing to run\n{USAGE}"));
    }
    Ok(Some(args))
}

fn number(s: &str, flag: &str) -> Result<usize, String> {
    s.parse()
        .map_err(|_| format!("tt-host: {flag} wants a number, got '{s}'"))
}
