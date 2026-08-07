//! tt-vt — the escape-sequence state machine.
//!
//! Byte-level parsing is delegated to the `vte` crate (the "adopt, don't build"
//! call in PLAN.md); everything here is *semantics*, and the semantics are the
//! part being ported from Tera Term rather than from the DEC manuals. Where the
//! two disagree, Tera Term wins — `run_diff.sh` diffs this engine against
//! `oracle/`, which is Tera Term's real `vtterm.c` running headless.
//!
//! Comments citing `vtterm.c` line numbers refer to the pinned upstream SHA in
//! `.github/workflows/ci.yml`.

use tt_grid::{
    Grid, Pen, ATTR2_BACK, ATTR2_COLOR_MASK, ATTR2_FORE, ATTR_BLINK, ATTR_BOLD, ATTR_REVERSE,
    ATTR_UNDER, DEFAULT_BG, DEFAULT_FG,
};
use vte::{Params, Perform};

pub mod term_id;
pub use term_id::TermId;

/// What an incoming CR and LF mean. Tera Term's `ts.CRReceive`.
///
/// The default is [`CrReceive::Cr`] — the `else` branch at `ttset.c:643`, not
/// the CRLF the surrounding code suggests. It shifts every row of output, so it
/// is the first thing to suspect when a dump looks uniformly wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrReceive {
    #[default]
    Cr,
    Lf,
    CrLf,
    Auto,
}

/// Tera Term's `ts.ColorFlag`. Both bits are **off** by default, which is not a
/// simplification but the shipped behaviour: with `CF_XTERM256` clear, `SGR 38`
/// and `SGR 48` do nothing *and do not consume their arguments*, so
/// `ESC [ 38;5;196 m` degrades into "38 (ignored), 5 (blink on), 196 (ignored)".
/// That looks like a bug in the port until you check upstream. See
/// `vtterm.c:2239`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColorFlags {
    pub xterm256: bool,
    pub aixterm16: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub cols: usize,
    pub rows: usize,
    pub term_id: TermId,
    pub cr_receive: CrReceive,
    pub color_flags: ColorFlags,
    pub scrollback_max: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cols: 80,
            rows: 24,
            term_id: TermId::Vt100,
            cr_receive: CrReceive::Cr,
            color_flags: ColorFlags::default(),
            scrollback_max: 10_000,
        }
    }
}

/// The terminal. Owns the parser and the grid.
pub struct Vt {
    parser: vte::Parser,
    state: State,
}

impl Vt {
    pub fn new(config: Config) -> Self {
        let grid = Grid::new(config.cols, config.rows, config.scrollback_max);
        Vt {
            parser: vte::Parser::new(),
            state: State {
                grid,
                config,
                ..State::empty()
            },
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.state, bytes);
    }

    pub fn grid(&self) -> &Grid {
        &self.state.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.state.grid
    }

    /// Bytes the terminal wants to send back to the host: DA, DSR, and friends.
    pub fn reply(&self) -> &[u8] {
        &self.state.reply
    }

    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.state.reply)
    }

    /// The last title set by OSC 0 / OSC 2. Empty if never set.
    pub fn title(&self) -> &str {
        &self.state.title
    }
}

struct State {
    grid: Grid,
    config: Config,
    reply: Vec<u8>,
    title: String,
    /// `ts.CRReceive == Auto` keeps one byte of history to collapse CR+LF.
    prev_was_cr: bool,
    prev_was_lf: bool,
    auto_generated_crlf: bool,
    /// The last printable codepoint, for REP.
    last_printed: Option<u32>,
}

impl State {
    fn empty() -> Self {
        State {
            grid: Grid::new(1, 1, 0),
            config: Config::default(),
            reply: Vec::new(),
            title: String::new(),
            prev_was_cr: false,
            prev_was_lf: false,
            auto_generated_crlf: false,
            last_printed: None,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.reply.extend_from_slice(bytes);
    }

    fn send_csi(&mut self, body: &str) {
        self.send(b"\x1b[");
        self.send(body.as_bytes());
    }

    // --- C0 --------------------------------------------------------------

    /// `vtterm.c:725`.
    fn process_cr(&mut self) {
        match self.config.cr_receive {
            CrReceive::Auto => {
                if !self.prev_was_lf || !self.auto_generated_crlf {
                    self.grid.carriage_return();
                    self.grid.line_feed();
                    self.auto_generated_crlf = true;
                } else {
                    self.auto_generated_crlf = false;
                }
            }
            CrReceive::CrLf => {
                self.grid.carriage_return();
                self.grid.line_feed();
            }
            _ => self.grid.carriage_return(),
        }
    }

    /// `vtterm.c:747`.
    fn process_lf(&mut self) {
        match self.config.cr_receive {
            CrReceive::Lf => {
                // "the server sends LF alone" — so LF means CR+LF.
                self.grid.carriage_return();
                self.grid.line_feed();
            }
            CrReceive::Auto => {
                if !self.prev_was_cr || !self.auto_generated_crlf {
                    self.grid.carriage_return();
                    self.grid.line_feed();
                    self.auto_generated_crlf = true;
                } else {
                    self.auto_generated_crlf = false;
                }
            }
            _ => self.grid.line_feed(),
        }
    }

    // --- SGR -------------------------------------------------------------

    /// `vtterm.c:ParseSGRParams`, including the parameter-consumption quirk
    /// described on [`ColorFlags`].
    fn sgr(&mut self, params: &Params) {
        let groups: Vec<Vec<u16>> = params.iter().map(|g| g.to_vec()).collect();
        let groups = if groups.is_empty() {
            vec![vec![0u16]]
        } else {
            groups
        };

        let mut i = 0;
        while i < groups.len() {
            let p = groups[i].first().copied().unwrap_or(0);
            match p {
                0 => {
                    self.grid.pen = Pen::default();
                }
                1 => self.grid.pen.attrs |= ATTR_BOLD,
                4 => self.grid.pen.attrs |= ATTR_UNDER,
                5 => self.grid.pen.attrs |= ATTR_BLINK,
                7 => self.grid.pen.attrs |= ATTR_REVERSE,
                22 => self.grid.pen.attrs &= !ATTR_BOLD,
                24 => self.grid.pen.attrs &= !ATTR_UNDER,
                25 => self.grid.pen.attrs &= !ATTR_BLINK,
                27 => self.grid.pen.attrs &= !ATTR_REVERSE,
                30..=37 => {
                    self.grid.pen.attrs |= ATTR2_FORE;
                    self.grid.pen.fg = (p - 30) as u32;
                }
                38 | 48 => {
                    if self.config.color_flags.xterm256 {
                        if let Some((color, consumed)) = extended_color(&groups, i) {
                            if p == 38 {
                                self.grid.pen.attrs |= ATTR2_FORE;
                                self.grid.pen.fg = color;
                            } else {
                                self.grid.pen.attrs |= ATTR2_BACK;
                                self.grid.pen.bg = color;
                            }
                            i += consumed;
                        }
                    }
                    // With CF_XTERM256 clear, upstream falls straight out of the
                    // switch without touching `i` — so the arguments are parsed
                    // as further SGR parameters. Reproduced deliberately.
                }
                39 => {
                    self.grid.pen.attrs &= !ATTR2_FORE;
                    self.grid.pen.fg = DEFAULT_FG;
                }
                40..=47 => {
                    self.grid.pen.attrs |= ATTR2_BACK;
                    self.grid.pen.bg = (p - 40) as u32;
                }
                49 => {
                    self.grid.pen.attrs &= !ATTR2_BACK;
                    self.grid.pen.bg = DEFAULT_BG;
                }
                90..=97 if self.config.color_flags.aixterm16 => {
                    self.grid.pen.attrs |= ATTR2_FORE;
                    self.grid.pen.fg = (p - 90 + 8) as u32;
                }
                // Order matters: with aixterm16 off, 100 resets both colours;
                // with it on, 100 is bright-black background and falls to the
                // arm below. That fall-through is upstream's, comment and all.
                100 if !self.config.color_flags.aixterm16 => {
                    self.grid.pen.attrs &= !ATTR2_COLOR_MASK;
                    self.grid.pen.fg = DEFAULT_FG;
                    self.grid.pen.bg = DEFAULT_BG;
                }
                100..=107 if self.config.color_flags.aixterm16 => {
                    self.grid.pen.attrs |= ATTR2_BACK;
                    self.grid.pen.bg = (p - 100 + 8) as u32;
                }
                _ => {}
            }
            i += 1;
        }
    }

    // --- modes -----------------------------------------------------------

    fn set_mode(&mut self, private: bool, params: &Params, on: bool) {
        for group in params.iter() {
            let p = group.first().copied().unwrap_or(0);
            if private {
                match p {
                    6 => {
                        // DECOM. Setting it homes the cursor to the region origin.
                        self.grid.origin_mode = on;
                        let (top, _) = self.grid.scroll_region();
                        self.grid.move_cursor(0, if on { top } else { 0 });
                    }
                    7 => self.grid.autowrap = on,
                    _ => {}
                }
            } else if p == 4 {
                self.grid.insert_mode = on;
            }
        }
    }
}

/// Decode `38;2;r;g;b` / `38;5;idx` in all the colon and semicolon spellings
/// upstream accepts. Returns the colour and how many extra parameter groups it
/// swallowed.
fn extended_color(groups: &[Vec<u16>], i: usize) -> Option<(u32, usize)> {
    // Colon form: 38:5:idx arrives as a single group.
    let g = &groups[i];
    if g.len() > 1 {
        return match g[1] {
            2 if g.len() >= 5 => Some((closest_color(g[2], g[3], g[4]), 0)),
            5 if g.len() >= 3 => Some(((g[2] as u32).min(255), 0)),
            _ => None,
        };
    }
    // Semicolon form: 38;5;idx arrives as separate groups.
    let kind = groups.get(i + 1)?.first().copied()?;
    match kind {
        2 => {
            let r = groups.get(i + 2)?.first().copied()?;
            let g_ = groups.get(i + 3)?.first().copied()?;
            let b = groups.get(i + 4)?.first().copied()?;
            Some((closest_color(r, g_, b), 4))
        }
        5 => {
            let idx = groups.get(i + 2)?.first().copied()?;
            Some(((idx as u32).min(255), 2))
        }
        _ => None,
    }
}

/// Placeholder for `DispFindClosestColor`. Truecolor is stored as an xterm-256
/// index upstream because the cell has one byte for it; matching the exact
/// palette search is Stage 1 work and only matters once `xterm256` is enabled.
fn closest_color(r: u16, g: u16, b: u16) -> u32 {
    let q = |v: u16| -> u32 {
        let v = v.min(255) as u32;
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

fn arg(params: &Params, n: usize, default: u16) -> u16 {
    match params.iter().nth(n).and_then(|g| g.first().copied()) {
        Some(0) | None => default,
        Some(v) => v,
    }
}

/// Like [`arg`] but keeps an explicit zero, which ED/EL/TBC need.
fn arg0(params: &Params, n: usize) -> u16 {
    params
        .iter()
        .nth(n)
        .and_then(|g| g.first().copied())
        .unwrap_or(0)
}

impl Perform for State {
    fn print(&mut self, c: char) {
        self.grid.put(c as u32);
        self.last_printed = Some(c as u32);
        self.prev_was_cr = false;
        self.prev_was_lf = false;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {} // BEL — the oracle silences it (IdBeepOff)
            0x08 => self.grid.backspace(),
            0x09 => self.grid.forward_tab(1),
            // LF, VT and FF all line-feed (vtterm.c treats them alike).
            0x0a..=0x0c => {
                self.process_lf();
                self.prev_was_lf = true;
                self.prev_was_cr = false;
                return;
            }
            0x0d => {
                self.process_cr();
                self.prev_was_cr = true;
                self.prev_was_lf = false;
                return;
            }
            _ => {}
        }
        self.prev_was_cr = false;
        self.prev_was_lf = false;
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let private = intermediates.first() == Some(&b'?');
        let gt = intermediates.first() == Some(&b'>');

        match action {
            '@' => self.grid.insert_chars(arg(params, 0, 1) as usize),
            'A' => self.grid.move_up(arg(params, 0, 1) as usize),
            'B' => self.grid.move_down(arg(params, 0, 1) as usize),
            'C' => self.grid.move_right(arg(params, 0, 1) as usize),
            'D' => self.grid.move_left(arg(params, 0, 1) as usize),
            'E' => {
                self.grid.move_down(arg(params, 0, 1) as usize);
                self.grid.carriage_return();
            }
            'F' => {
                self.grid.move_up(arg(params, 0, 1) as usize);
                self.grid.carriage_return();
            }
            'G' | '`' => {
                let x = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_cursor(x, self.grid.cursor.y);
            }
            'H' | 'f' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                let x = arg(params, 1, 1).saturating_sub(1) as usize;
                self.grid.move_cursor_abs(x, y);
            }
            'I' => self.grid.forward_tab(arg(params, 0, 1) as usize),
            'J' => self.grid.erase_display(arg0(params, 0)),
            'K' => self.grid.erase_line(arg0(params, 0)),
            'L' => self.grid.insert_lines(arg(params, 0, 1) as usize),
            'M' => self.grid.delete_lines(arg(params, 0, 1) as usize),
            'P' => self.grid.delete_chars(arg(params, 0, 1) as usize),
            'S' => self.grid.scroll_up(arg(params, 0, 1) as usize),
            'T' => self.grid.scroll_down(arg(params, 0, 1) as usize),
            'X' => self.grid.erase_chars(arg(params, 0, 1) as usize),
            'Z' => self.grid.backward_tab(arg(params, 0, 1) as usize),
            'b' => {
                if let Some(cp) = self.last_printed {
                    for _ in 0..arg(params, 0, 1) {
                        self.grid.put(cp);
                    }
                }
            }
            'c' => {
                if gt {
                    // Secondary DA: VT382(>32) + xterm rev 331 (vtterm.c:2841).
                    self.send_csi(">32;331;0c");
                } else if !private {
                    let da = self.config.term_id.primary_da();
                    self.send(b"\x1b[?");
                    self.send(da.as_bytes());
                    self.send(b"c");
                }
            }
            'd' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_cursor(self.grid.cursor.x, y);
            }
            'g' => match arg0(params, 0) {
                0 => self.grid.clear_tab(),
                3 => self.grid.clear_all_tabs(),
                _ => {}
            },
            'h' => self.set_mode(private, params, true),
            'l' => self.set_mode(private, params, false),
            'm' => {
                if !private && !gt {
                    self.sgr(params)
                }
            }
            'n' => match arg0(params, 0) {
                5 => self.send_csi("0n"),
                6 => {
                    let (x, y) = (self.grid.cursor.x, self.grid.cursor.y);
                    let (top, _) = self.grid.scroll_region();
                    let row = if self.grid.origin_mode { y - top } else { y };
                    let body = format!("{};{}R", row + 1, x + 1);
                    self.send_csi(&body);
                }
                _ => {}
            },
            'r' => {
                let rows = self.grid.rows() as u16;
                let top = arg(params, 0, 1).saturating_sub(1) as usize;
                let bottom = arg(params, 1, rows).saturating_sub(1) as usize;
                self.grid.set_scroll_region(top, bottom);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        // Character-set designation (ESC ( B and friends) is parsed and dropped
        // until tt-charset exists; dropping it silently is wrong for DEC line
        // drawing and is the next thing this file will grow.
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.grid.save_cursor(),
            b'8' => self.grid.restore_cursor(),
            b'D' => self.grid.line_feed(),
            b'E' => {
                self.grid.carriage_return();
                self.grid.line_feed();
            }
            b'H' => self.grid.set_tab(),
            b'M' => self.grid.reverse_index(),
            b'c' => {
                self.grid.reset();
                self.title.clear();
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(&kind) = params.first() else { return };
        let Ok(kind) = std::str::from_utf8(kind).unwrap_or("").parse::<u32>() else {
            return;
        };
        // 0 sets both icon name and window title, 2 the window title. Only the
        // window title reaches `cv.TitleRemoteW`, which is what the oracle dumps.
        if kind == 0 || kind == 2 {
            if let Some(text) = params.get(1) {
                self.title = String::from_utf8_lossy(text).into_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[u8], cols: usize, rows: usize) -> Vt {
        let mut vt = Vt::new(Config {
            cols,
            rows,
            ..Config::default()
        });
        vt.feed(input);
        vt
    }

    fn row(vt: &Vt, y: usize) -> String {
        let mut s = String::new();
        for cell in vt.grid().line(y) {
            if cell.width_class == tt_grid::WIDTH_PAD {
                continue;
            }
            let mut any = false;
            for cp in cell.codepoints() {
                s.push(char::from_u32(cp).unwrap_or('?'));
                any = true;
            }
            if !any {
                s.push(' ');
            }
        }
        s.trim_end().to_string()
    }

    #[test]
    fn bare_cr_is_a_carriage_return_by_default() {
        // The CRReceive=IdCR default: "Hello, world!\rSecond line" overwrites.
        let vt = run(b"Hello, world!\rSecond line", 40, 4);
        assert_eq!(row(&vt, 0), "Second lined!");
        assert_eq!(vt.grid().cursor.x, 11);
    }

    #[test]
    fn primary_da_identifies_as_vt100() {
        let vt = run(b"\x1b[c", 20, 2);
        assert_eq!(vt.reply(), b"\x1b[?1;2c");
    }

    #[test]
    fn cursor_position_report_is_one_based() {
        let vt = run(b"\x1b[10;20H\x1b[6n", 40, 24);
        assert_eq!(vt.reply(), b"\x1b[10;20R");
    }

    #[test]
    fn osc_zero_sets_the_title() {
        let vt = run(b"\x1b]0;My Session\x07text", 20, 2);
        assert_eq!(vt.title(), "My Session");
        assert_eq!(row(&vt, 0), "text");
    }

    #[test]
    fn sgr_38_without_xterm256_leaks_its_arguments() {
        // Not a bug: with ts.ColorFlag == 0, `38` is ignored without consuming
        // `5`, which then turns blink on. Upstream behaviour, reproduced.
        let vt = run(b"\x1b[38;5;196mR", 20, 2);
        let cell = vt.grid().line(0)[0];
        assert_eq!(cell.attrs & ATTR_BLINK, ATTR_BLINK);
        assert_eq!(cell.attrs & ATTR2_COLOR_MASK, 0);
    }

    #[test]
    fn crreceive_lf_treats_lf_as_crlf() {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 4,
            cr_receive: CrReceive::Lf,
            ..Config::default()
        });
        vt.feed(b"one\r\ntwo\r\nthree");
        assert_eq!(row(&vt, 0), "one");
        assert_eq!(row(&vt, 1), "two");
        assert_eq!(row(&vt, 2), "three");
    }

    #[test]
    fn scroll_region_is_set_and_homes_the_cursor() {
        // DECSTBM homes to the screen origin, not the region top, when origin
        // mode is off — vtterm.c:2473.
        let vt = run(b"\x1b[2;4r", 10, 6);
        assert_eq!(vt.grid().scroll_region(), (1, 3));
        assert_eq!((vt.grid().cursor.x, vt.grid().cursor.y), (0, 0));
    }
}
