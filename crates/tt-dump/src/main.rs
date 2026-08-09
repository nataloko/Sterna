//! tt-dump — the Rust side of the differential harness.
//!
//! Same arguments as `oracle/build/oracle`, same output format, byte for byte:
//!
//! ```text
//! tt-dump [--cols N] [--rows N] [--term ID] [--attrs] [--scrollback]
//!         [--crreceive cr|lf|crlf|auto] [--clearonresize]
//!         [--noscrollwindowclear] [FILE]
//! ```
//!
//! `run_diff.sh` runs both engines over every case in `oracle/cases/` and diffs
//! them against each other. Nothing here is a golden file: the oracle *is* the
//! expected output, so a new case needs an input and no blessing.
//!
//! The `# sterna-oracle 1` banner names the *format*, not the producer, so
//! both engines emit it unchanged — otherwise every diff would start with a
//! spurious first-line mismatch.

use std::io::{Read, Write};

use tt_grid::{
    char_width, Cell, ATTR2_BACK, ATTR2_FORE, ATTR2_PROTECT, ATTR_BLINK, ATTR_BOLD, ATTR_REVERSE,
    ATTR_SPECIAL, ATTR_UNDER, WIDTH_PAD,
};
use tt_vt::{Config, CrReceive, Key, Modifiers, MouseEvent, TermId, Vt};

const USAGE: &str = "usage: tt-dump [--cols N] [--rows N] [--term ID] [--attrs]\n\
                     \x20              [--scrollback] [--crreceive cr|lf|crlf|auto]\n\
                     \x20              [--clearonresize] [--noscrollwindowclear] [FILE]\n";

/// `TermWidthMax` / `TermHeightMax` from Tera Term's `ttcommon.h`, so the two
/// engines reject the same sizes.
const TERM_WIDTH_MAX: usize = 500;
const TERM_HEIGHT_MAX: usize = 500;

struct Args {
    cols: usize,
    rows: usize,
    term: String,
    attrs: bool,
    scrollback: bool,
    clear_on_resize: bool,
    home_erase_clears_screen: bool,
    cr_receive: CrReceive,
    path: Option<String>,
}

/// `ts.ScrollBuffSize`, which the oracle sets to upstream's default of 100
/// (`ttset.c:750`) and which is the **whole** buffer, page included. The grid
/// counts the lines beyond the page instead, so the conversion is upstream's
/// own — `buffer.c:641` grows the buffer to hold the page rather than shrinking
/// the page to the buffer.
///
/// Both engines have to agree on it or a case that scrolls past the history
/// and then grows the page diverges on what came back out of it.
const SCROLL_BUFF_SIZE: usize = 100;

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
        scrollback_max: SCROLL_BUFF_SIZE.saturating_sub(args.rows),
        clear_on_resize: args.clear_on_resize,
        home_erase_clears_screen: args.home_erase_clears_screen,
        ..Config::default()
    });
    // `oracle/src/main.c` opens with `BuffChangeTerminalSize(cols, rows)` after
    // `ResetTerminal`, which is what a real Tera Term does on the way to its
    // first screen. A no-op at the size the terminal already has — except
    // under `--clearonresize`, where the scroll is outside the block that
    // tests for a change and a blank page goes into the history before a byte
    // has arrived. That is upstream's startup rather than upstream's parse,
    // and the two engines have to make the same one.
    vt.grid_mut().resize(args.cols, args.rows);
    run_stream(&mut vt, &input);

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    if let Err(e) = dump(&mut out, &vt, &args) {
        eprintln!("tt-dump: write failed: {e}");
        std::process::exit(1);
    }
}

/// Injected input events, mirroring `oracle/src/main.c:run_stream`.
///
/// A dump has no mouse, so a case can carry directives inside the byte stream
/// and the runner strips them before the terminal sees them:
///
/// ```text
/// ESC _ tt.mouse <down|up|move|wheel|stat> <button> <x> <y> ESC \
/// ESC _ tt.mods  [shift] [ctrl] [alt]                       ESC \
/// ESC _ tt.focus <in|out>                                   ESC \
/// ESC _ tt.key   <name>                                     ESC \
/// ```
///
/// `x`/`y` are window pixels, on the nominal 8x16 cell both engines use. The
/// bytes on either side are fed first, so a directive sees exactly the state
/// the preceding stream produced. Anything after `ESC _` that is not `tt.` is
/// passed through.
fn run_stream(vt: &mut Vt, input: &[u8]) {
    let mut mods = Modifiers::default();
    let (mut i, mut seg) = (0usize, 0usize);

    while i + 1 < input.len() {
        if !(input[i] == 0x1b && input[i + 1] == b'_') || !input[i + 2..].starts_with(b"tt.") {
            i += 1;
            continue;
        }
        let body = i + 2;
        let Some(end) = (body..input.len().saturating_sub(1))
            .find(|&j| input[j] == 0x1b && input[j + 1] == b'\\')
        else {
            fail("unterminated tt. directive");
        };

        if i > seg {
            vt.feed(&input[seg..i]);
        }
        run_directive(vt, &mut mods, &input[body..end]);
        i = end + 2;
        seg = i;
    }

    if input.len() > seg {
        vt.feed(&input[seg..]);
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("tt-dump: {msg}");
    std::process::exit(2);
}

fn run_directive(vt: &mut Vt, mods: &mut Modifiers, body: &[u8]) {
    let text = String::from_utf8_lossy(body);
    let tok: Vec<&str> = text.split_whitespace().collect();
    let Some(&kind) = tok.first() else { return };

    match kind {
        "tt.mods" => {
            *mods = Modifiers::default();
            for &t in &tok[1..] {
                match t {
                    "shift" => mods.shift = true,
                    "ctrl" => mods.ctrl = true,
                    "alt" => mods.alt = true,
                    _ => fail("unknown modifier in tt.mods"),
                }
            }
        }
        "tt.key" => {
            let Some(name) = tok.get(1) else {
                fail("tt.key wants a key name")
            };
            let Some(key) = Key::parse(name) else {
                fail(&format!("unknown key '{name}'"))
            };
            // Straight into the reply stream, where the oracle's
            // CommBinaryOut puts it too, so the dumps line up.
            if let Some(bytes) = vt.key(key) {
                vt.push_reply(&bytes);
            }
        }
        "tt.focus" => match tok.get(1) {
            Some(&"in") => vt.focus_event(true),
            Some(&"out") => vt.focus_event(false),
            _ => fail("tt.focus wants in|out"),
        },
        "tt.mouse" => {
            if tok.len() != 5 {
                fail("tt.mouse wants event button x y");
            }
            let event = match tok[1] {
                "stat" => MouseEvent::CurStat,
                "down" => MouseEvent::Press,
                "up" => MouseEvent::Release,
                "move" => MouseEvent::Move,
                "wheel" => MouseEvent::Wheel,
                _ => fail("unknown tt.mouse event"),
            };
            let num = |s: &str| {
                s.parse::<i32>()
                    .unwrap_or_else(|_| fail("tt.mouse wants numbers"))
            };
            vt.mouse_event(event, num(tok[2]) as u8, num(tok[3]), num(tok[4]), *mods);
        }
        _ => fail("unknown tt. directive"),
    }
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        cols: 80,
        rows: 24,
        term: "vt100".to_string(),
        attrs: false,
        scrollback: false,
        clear_on_resize: false,
        home_erase_clears_screen: true,
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
            "--scrollback" => args.scrollback = true,
            "--clearonresize" => args.clear_on_resize = true,
            "--noscrollwindowclear" => args.home_erase_clears_screen = false,
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

    writeln!(out, "# sterna-oracle 1")?;
    // The terminal id is echoed as it was spelled on the command line, which is
    // what the oracle does with its argv string.
    writeln!(out, "# term {} {}x{}", args.term, grid.cols(), grid.rows())?;
    writeln!(out, "# cursor {},{}", grid.cursor.x, grid.cursor.y)?;

    if !vt.remote_title().is_empty() {
        writeln!(out, "# title {}", vt.remote_title())?;
    }

    // The lines that have left the page, oldest first and numbered backwards
    // from it — `oracle/src/main.c:dump`'s section, and off by default for the
    // same reason: it answers a question most cases do not ask.
    if args.scrollback {
        let history = grid.scrollback_len();
        writeln!(out, "# scrollback {history}")?;
        for (i, line) in grid.scrollback().enumerate() {
            dump_row(out, line, i as isize - history as isize, grid.cols())?;
        }
    }

    for y in 0..grid.rows() {
        dump_row(out, grid.line(y), y as isize, grid.cols())?;
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

/// One row, under its own label — `oracle/src/main.c:dump_row`.
///
/// Padding cells are skipped rather than printed, and the trailing fill counts
/// **display columns** rather than array indices, because a wide character
/// occupies two of the second and one of the first.
fn dump_row(out: &mut impl Write, line: &[Cell], label: isize, cols: usize) -> std::io::Result<()> {
    write!(out, "{label:3} |")?;
    let mut col = 0;
    for cell in line {
        if col >= cols {
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
        if col + w > cols {
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
    for _ in col..cols {
        out.write_all(b" ")?;
    }
    writeln!(out, "|")
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
