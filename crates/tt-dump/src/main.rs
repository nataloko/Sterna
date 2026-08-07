//! tt-dump — the Rust side of the differential harness.
//!
//! Same arguments as `oracle/build/oracle`, same output format, byte for byte:
//!
//! ```text
//! tt-dump [--cols N] [--rows N] [--term ID] [--attrs]
//!         [--crreceive cr|lf|crlf|auto] [FILE]
//! ```
//!
//! `run_diff.sh` runs both engines over every case in `oracle/cases/` and diffs
//! them against each other. Nothing here is a golden file: the oracle *is* the
//! expected output, so a new case needs an input and no blessing.
//!
//! The `# termitta-oracle 1` banner names the *format*, not the producer, so
//! both engines emit it unchanged — otherwise every diff would start with a
//! spurious first-line mismatch.

use std::io::{Read, Write};

use tt_grid::{
    char_width, ATTR2_BACK, ATTR2_FORE, ATTR2_PROTECT, ATTR_BLINK, ATTR_BOLD, ATTR_REVERSE,
    ATTR_SPECIAL, ATTR_UNDER, WIDTH_PAD,
};
use tt_vt::{Config, CrReceive, TermId, Vt};

const USAGE: &str = "usage: tt-dump [--cols N] [--rows N] [--term ID] [--attrs]\n\
                     \x20              [--crreceive cr|lf|crlf|auto] [FILE]\n";

/// `TermWidthMax` / `TermHeightMax` from Tera Term's `ttcommon.h`, so the two
/// engines reject the same sizes.
const TERM_WIDTH_MAX: usize = 500;
const TERM_HEIGHT_MAX: usize = 500;

struct Args {
    cols: usize,
    rows: usize,
    term: String,
    attrs: bool,
    cr_receive: CrReceive,
    path: Option<String>,
}

fn main() {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    let Some(term_id) = TermId::parse(&args.term) else {
        eprintln!("tt-dump: unknown terminal id '{}'", args.term);
        std::process::exit(2);
    };

    if args.cols < 1 || args.cols > TERM_WIDTH_MAX || args.rows < 1 || args.rows > TERM_HEIGHT_MAX {
        eprintln!("tt-dump: size {}x{} out of range", args.cols, args.rows);
        std::process::exit(2);
    }

    let mut input = Vec::new();
    let read = match &args.path {
        Some(p) => std::fs::File::open(p).and_then(|mut f| f.read_to_end(&mut input)),
        None => std::io::stdin().read_to_end(&mut input),
    };
    if let Err(e) = read {
        eprintln!(
            "tt-dump: cannot read {}: {e}",
            args.path.as_deref().unwrap_or("stdin")
        );
        std::process::exit(2);
    }

    let mut vt = Vt::new(Config {
        cols: args.cols,
        rows: args.rows,
        term_id,
        cr_receive: args.cr_receive,
        ..Config::default()
    });
    vt.feed(&input);

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    if let Err(e) = dump(&mut out, &vt, &args) {
        eprintln!("tt-dump: write failed: {e}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        cols: 80,
        rows: 24,
        term: "vt100".to_string(),
        attrs: false,
        cr_receive: CrReceive::Cr,
        path: None,
    };

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        let next = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| format!("tt-dump: {a} needs a value"))
        };
        match a {
            "--cols" => {
                args.cols = next(&mut i)?
                    .parse()
                    .map_err(|_| "tt-dump: bad --cols".to_string())?
            }
            "--rows" => {
                args.rows = next(&mut i)?
                    .parse()
                    .map_err(|_| "tt-dump: bad --rows".to_string())?
            }
            "--term" => args.term = next(&mut i)?,
            "--attrs" => args.attrs = true,
            "--crreceive" => {
                let v = next(&mut i)?;
                args.cr_receive = match v.as_str() {
                    "cr" => CrReceive::Cr,
                    "lf" => CrReceive::Lf,
                    "crlf" => CrReceive::CrLf,
                    "auto" => CrReceive::Auto,
                    _ => return Err(format!("tt-dump: bad --crreceive '{v}'")),
                };
            }
            "--help" => {
                eprint!("{USAGE}");
                return Ok(None);
            }
            _ if !a.starts_with('-') => args.path = Some(a.to_string()),
            _ => return Err(format!("tt-dump: unknown option '{a}'")),
        }
        i += 1;
    }
    Ok(Some(args))
}

fn dump(out: &mut impl Write, vt: &Vt, args: &Args) -> std::io::Result<()> {
    let grid = vt.grid();

    writeln!(out, "# termitta-oracle 1")?;
    // The terminal id is echoed as it was spelled on the command line, which is
    // what the oracle does with its argv string.
    writeln!(out, "# term {} {}x{}", args.term, grid.cols(), grid.rows())?;
    writeln!(out, "# cursor {},{}", grid.cursor.x, grid.cursor.y)?;

    if !vt.title().is_empty() {
        writeln!(out, "# title {}", vt.title())?;
    }

    for y in 0..grid.rows() {
        write!(out, "{y:3} |")?;
        let mut col = 0;
        for cell in grid.line(y) {
            if col >= grid.cols() {
                break;
            }
            if cell.width_class == WIDTH_PAD {
                continue;
            }
            let base = cell.text[0];
            let w = if base == 0 {
                1
            } else {
                char_width(base).max(1)
            };
            if col + w > grid.cols() {
                break;
            }
            if base == 0 {
                out.write_all(b" ")?;
            } else {
                for cp in cell.codepoints() {
                    let mut buf = [0u8; 4];
                    let s = char::from_u32(cp)
                        .unwrap_or('\u{fffd}')
                        .encode_utf8(&mut buf);
                    out.write_all(s.as_bytes())?;
                }
            }
            col += w;
        }
        for _ in col..grid.cols() {
            out.write_all(b" ")?;
        }
        writeln!(out, "|")?;
    }

    if args.attrs {
        writeln!(out, "# attrs")?;
        for y in 0..grid.rows() {
            write!(out, "{y:3} |")?;
            for x in 0..grid.cols() {
                out.write_all(&[attr_char(grid.line(y)[x].attrs)])?;
            }
            writeln!(out, "|")?;
        }
        writeln!(out, "# colors")?;
        for y in 0..grid.rows() {
            write!(out, "{y:3} |")?;
            for x in 0..grid.cols() {
                let cell = grid.line(y)[x];
                if cell.attrs & (ATTR2_FORE | ATTR2_BACK) == 0 {
                    out.write_all(b".")?;
                } else {
                    write!(out, "{:x}", cell.fg & 0xf)?;
                }
            }
            writeln!(out, "|")?;
        }

        // DECSCA's bit gets a section of its own, and only when something is
        // actually protected — it is orthogonal to the rest, so folding it into
        // the one-char-per-cell attribute line would hide a protected bold cell
        // behind its B, and emitting it unconditionally would churn every
        // existing golden for a bit almost no case sets.
        let protected = (0..grid.rows())
            .any(|y| (0..grid.cols()).any(|x| grid.line(y)[x].attrs & ATTR2_PROTECT != 0));
        if protected {
            writeln!(out, "# protect")?;
            for y in 0..grid.rows() {
                write!(out, "{y:3} |")?;
                for x in 0..grid.cols() {
                    let p = grid.line(y)[x].attrs & ATTR2_PROTECT != 0;
                    out.write_all(if p { b"P" } else { b"." })?;
                }
                writeln!(out, "|")?;
            }
        }
    }

    let reply = vt.reply();
    if !reply.is_empty() {
        write!(out, "# reply ")?;
        for &b in reply {
            match b {
                0x1b => write!(out, "<ESC>")?,
                b if !(0x20..0x7f).contains(&b) => write!(out, "<{b:02x}>")?,
                b => out.write_all(&[b])?,
            }
        }
        writeln!(out)?;
    }

    out.flush()
}

/// `oracle/src/main.c:attr_char` — one character per cell, most significant
/// attribute wins. The order is load-bearing: reverse beats bold beats
/// underline, and the colour flags only show when nothing else is set.
fn attr_char(attrs: u32) -> u8 {
    if attrs & ATTR_REVERSE != 0 {
        b'R'
    } else if attrs & ATTR_BOLD != 0 {
        b'B'
    } else if attrs & ATTR_UNDER != 0 {
        b'U'
    } else if attrs & ATTR_BLINK != 0 {
        b'K'
    } else if attrs & ATTR_SPECIAL != 0 {
        b'S'
    } else if attrs & ATTR2_FORE != 0 {
        b'f'
    } else if attrs & ATTR2_BACK != 0 {
        b'b'
    } else {
        b'.'
    }
}
