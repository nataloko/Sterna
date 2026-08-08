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

use tt_charset::{gset_from_intermediate, sbcs_final, Iso2022, Iso2022State, Shift, ShiftFlags};
use tt_grid::{
    Grid, Pen, Rect, ATTR2_BACK, ATTR2_COLOR_MASK, ATTR2_FORE, ATTR2_PROTECT, ATTR_BLINK,
    ATTR_BOLD, ATTR_REVERSE, ATTR_SGR_MASK, ATTR_SPECIAL, ATTR_UNDER, DEFAULT_BG, DEFAULT_FG,
};
use vte::{Params, Perform};

pub mod palette;
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

/// Tera Term's `ts.ColorFlag`, or the two bits of it that change how SGR parses.
///
/// `Xterm256Color` defaults to **on** (`ttset.c:743`) and `Aixterm16Color` to
/// off (`:739`). The asymmetry is load-bearing in an unobvious way: when a bit
/// is clear, the corresponding SGR parameter is ignored *without consuming its
/// arguments*, so with 256-colour disabled `ESC [ 38;5;196 m` would be read as
/// "38 ignored, 5 = blink on, 196 ignored". `vtterm.c:2239`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorFlags {
    pub xterm256: bool,
    pub aixterm16: bool,
    /// `CF_PCBOLD16` (`ttset.c` key `PcBoldColor`, default off). Bold plus a
    /// colour below 8 means the bright version of it.
    pub pc_bold16: bool,
    /// `CF_ANSICOLOR` (key `EnableANSIColor`, default on). Without it DECRQSS
    /// reports no colour at all, whatever the pen holds.
    pub ansi_color: bool,
}

impl ColorFlags {
    /// `CF_FULLCOLOR` — any of PC-bold-16, aixterm-16 or xterm-256. It gates
    /// the bright/dim flip in the nearest-colour search, so 256-colour being on
    /// by default means the flip is on by default too.
    pub fn full_color(self) -> bool {
        self.xterm256 || self.aixterm16 || self.pc_bold16
    }
}

impl Default for ColorFlags {
    fn default() -> Self {
        ColorFlags {
            xterm256: true,
            aixterm16: false,
            pc_bold16: false,
            ansi_color: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub cols: usize,
    pub rows: usize,
    pub term_id: TermId,
    pub cr_receive: CrReceive,
    pub color_flags: ColorFlags,
    /// `ts.ISO2022Flag`. Defaults to every shift enabled.
    pub iso2022_flags: ShiftFlags,
    /// `LangIsJapanese(ts.KanjiCode)`. False for a UTF-8 terminal, which is all
    /// we support; it gates the Katakana designations only.
    pub japanese: bool,
    /// `TF_ACCEPT8BITCTRL` (`ttset.c:1075`, key default on).
    pub accept_8bit_ctrl: bool,
    /// `TF_ALTSCR` (`ttset.c:1681`, key default on).
    pub alt_screen_enabled: bool,
    /// `TF_REMOTECLEARSBUFF` (`ttset.c:1950`, key default on). Gates `ED 3`.
    pub remote_clears_buffer: bool,
    /// `WF_WINDOWCHANGE` (`ttset.c:1653`, key default on). Gates the XTWINOPS
    /// operations that *change* something, including the resize.
    pub window_change: bool,
    /// `WF_WINDOWREPORT` (`ttset.c:1661`, key default on). Gates the ones that
    /// answer back.
    pub window_report: bool,
    /// `ts.Send8BitCtrl` (`ttset.c:1283`, key default off). Above VT level 1 it
    /// decides whether replies use 8-bit C1 introducers; DECSCL can turn it on
    /// as well.
    pub send_8bit_ctrl: bool,
    /// `ts.CursorShape` in DECSCUSR's numbering — 1 block, 3 underline, 5 bar.
    /// `ttset.c:725`'s else branch is `IdBlkCur`. Only DECRQSS reads it; the
    /// shape itself is the frontend's business.
    pub cursor_shape: u16,
    /// `ts.NonblinkingCursor` (`ttset.c:1227`, key default off). Adds one to
    /// the DECSCUSR value.
    pub nonblinking_cursor: bool,
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
            iso2022_flags: ShiftFlags::ALL,
            japanese: false,
            accept_8bit_ctrl: true,
            alt_screen_enabled: true,
            remote_clears_buffer: true,
            window_change: true,
            window_report: true,
            send_8bit_ctrl: false,
            cursor_shape: 1,
            nonblinking_cursor: false,
            // ttset.c:1213 MaxBuffSize. Not ttset.c:750's ScrollBuffSize (100),
            // which is the *initial* depth the user can grow up to this.
            scrollback_max: 10_000,
        }
    }
}

/// The terminal. Owns the parser and the grid.
pub struct Vt {
    parser: vte::Parser,
    state: State,
    /// A `0xC2` seen at the end of the previous chunk. Without this, feeding
    /// `[0xC2]` then `[0x8D]` would print a replacement character where a
    /// single call would have produced a carriage return.
    pending_c2: bool,
}

impl Vt {
    pub fn new(config: Config) -> Self {
        let grid = Grid::new(config.cols, config.rows, config.scrollback_max);
        // `vtterm.c:ChangeTerminalID` — level 1 never sends 8-bit controls,
        // whatever the setting says.
        let vt_level = config.term_id.vt_level();
        let send_8bit = vt_level >= 2 && config.send_8bit_ctrl;
        Vt {
            parser: vte::Parser::new(),
            state: State {
                grid,
                config,
                vt_level,
                send_8bit,
                ..State::empty()
            },
            pending_c2: false,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // 0xC2 is the only lead byte that can produce a C1 codepoint, so a
        // stream without one needs no rewriting at all — which is almost all
        // of them.
        if !self.pending_c2 && !bytes.contains(&0xc2) {
            self.parser.advance(&mut self.state, bytes);
            return;
        }
        let rewritten = self.rewrite_c1(bytes);
        self.parser.advance(&mut self.state, &rewritten);
    }

    /// Fold 8-bit C1 controls into something `vte` can act on.
    ///
    /// `vtterm.c:1053`: on a non-English terminal — which UTF-8 is, for that
    /// predicate — a C1 byte is dropped outright when `TF_ACCEPT8BITCTRL` is
    /// clear, and masked to `b & 0x7F` when the terminal's VT level is below 2.
    /// So on the default VT100, `U+008D` is a **carriage return**, not RI, and
    /// `U+009B` is an ESC rather than a CSI introducer. Verified against the
    /// oracle across all 32 C1 codes rather than assumed.
    ///
    /// At level 2 and up the mask does not apply and the control keeps its C1
    /// meaning; we hand those to `vte` in the equivalent `ESC Fe` form, since
    /// its parser reaches the same states either way.
    ///
    /// This runs over the whole stream, including OSC and DCS payloads. Tera
    /// Term decodes UTF-8 before its escape parser too, so it has the same
    /// property; a C1 inside a string is mangled by both.
    fn rewrite_c1(&mut self, bytes: &[u8]) -> Vec<u8> {
        let accept = self.state.config.accept_8bit_ctrl;
        let level = self.state.vt_level;
        let mut out = Vec::with_capacity(bytes.len());

        for &b in bytes {
            if self.pending_c2 {
                self.pending_c2 = false;
                if (0x80..=0x9f).contains(&b) {
                    if !accept {
                        continue; // dropped, as upstream drops it
                    } else if level < 2 {
                        out.push(b & 0x7f);
                    } else {
                        out.push(0x1b);
                        out.push(b - 0x40);
                    }
                    continue;
                }
                // Not a C1 after all — put the lead byte back and fall through
                // so this byte is handled normally.
                out.push(0xc2);
            }
            if b == 0xc2 {
                self.pending_c2 = true;
            } else {
                out.push(b);
            }
        }
        out
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
    charset: Iso2022,
    /// DECSC saves the G-sets alongside the cursor — `vtterm.c:228`.
    saved_charset: Option<Iso2022State>,
    alt_screen: bool,
    reply: Vec<u8>,
    title: String,
    /// `ts.CRReceive == Auto` keeps one byte of history to collapse CR+LF.
    prev_was_cr: bool,
    prev_was_lf: bool,
    auto_generated_crlf: bool,
    /// The last printable codepoint, for REP.
    last_printed: Option<u32>,
    /// DECSACE's `RectangleMode` (`vtterm.c:113`). False — stream — out of
    /// reset, and it decides how DECCARA and DECRARA read their rectangle.
    rect_mode: bool,
    /// DECLRMM's `LRMarginMode` (`vtterm.c:112`). While it is off, `CSI Ps ; Ps s`
    /// is SCP (save cursor) rather than DECSLRM, and there are no margins to
    /// set — which is why the mode exists at all.
    lr_margin_mode: bool,
    /// `VTlevel`. Starts at the terminal id's level and can only be *lowered*
    /// by DECSCL, never raised past what the id claims.
    vt_level: u8,
    /// `Send8BitMode`. Only ever true above level 1, and only when the setting
    /// or DECSCL says so.
    send_8bit: bool,
    /// The intermediate and final byte of the DCS being collected, and its
    /// payload. `None` when no DCS is open.
    dcs: Option<(Option<u8>, char)>,
    dcs_buf: Vec<u8>,
}

impl State {
    fn empty() -> Self {
        State {
            grid: Grid::new(1, 1, 0),
            config: Config::default(),
            charset: Iso2022::new(),
            saved_charset: None,
            alt_screen: false,
            reply: Vec::new(),
            title: String::new(),
            prev_was_cr: false,
            prev_was_lf: false,
            auto_generated_crlf: false,
            last_printed: None,
            rect_mode: false,
            lr_margin_mode: false,
            vt_level: 1,
            send_8bit: false,
            dcs: None,
            dcs_buf: Vec::new(),
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.reply.extend_from_slice(bytes);
    }

    /// `vtterm.c:SendCSIstr` — the introducer is the 8-bit CSI once the host
    /// has asked for one, via S8C1T or DECSCL. Every CSI reply goes through
    /// here **except** the Primary DA, which upstream builds by hand with a
    /// stricter rule of its own; see `'c'` in [`State::csi_dispatch`].
    fn send_csi(&mut self, body: &str) {
        if self.send_8bit {
            self.send(&[0x9b]);
        } else {
            self.send(b"\x1b[");
        }
        self.send(body.as_bytes());
    }

    /// DECBI/DECFI act only inside the scroll region *and* between the
    /// margins — `vtterm.c:1484`.
    fn cursor_in_region(&self) -> bool {
        let (top, bottom) = self.grid.scroll_region();
        let (left, right) = self.grid.margins();
        (top..=bottom).contains(&self.grid.cursor.y) && (left..=right).contains(&self.grid.cursor.x)
    }

    /// Every locking and single shift is gated on `ts.ISO2022Flag` upstream, at
    /// the call site rather than inside the charset code. Same split here.
    fn shift(&mut self, shift: Shift) {
        if self.config.iso2022_flags.allows(shift) {
            self.charset.invoke(shift);
        }
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

    /// SGR — `ParseSGRParams` applied to the pen, with no mask.
    fn sgr(&mut self, params: &Params) {
        let groups = sgr_groups(params);
        let mut pen = self.grid.pen;
        self.parse_sgr_params(&groups, 0, &mut pen, &mut 0);
        self.grid.pen = pen;
    }

    /// `vtterm.c:ParseSGRParams`, including the parameter-consumption quirk
    /// described on [`ColorFlags`].
    ///
    /// `attr` accumulates onto whatever it starts as — the pen for SGR, a
    /// cleared attribute for DECCARA — and `mask` gathers the bits the
    /// parameters actually *named*, which is what tells DECCARA which of them
    /// to write over the cells it covers and which to leave.
    fn parse_sgr_params(&self, groups: &[Vec<u16>], start: usize, attr: &mut Pen, mask: &mut u32) {
        let mut i = start;
        while i < groups.len() {
            let p = groups[i].first().copied().unwrap_or(0);
            match p {
                0 => {
                    // The protect bit survives SGR 0 — `vtterm.c:2178` ORs it
                    // back in explicitly, so DECSCA outlives an attribute
                    // reset and only another DECSCA clears it.
                    let protect = attr.attrs & ATTR2_PROTECT;
                    *attr = Pen::default();
                    attr.attrs |= protect;
                    // Assignment, not an OR: SGR 0 resets the mask too.
                    *mask = ATTR_SGR_MASK | ATTR2_COLOR_MASK;
                }
                1 => set(attr, mask, ATTR_BOLD, true),
                4 => set(attr, mask, ATTR_UNDER, true),
                5 => set(attr, mask, ATTR_BLINK, true),
                7 => set(attr, mask, ATTR_REVERSE, true),
                22 => set(attr, mask, ATTR_BOLD, false),
                24 => set(attr, mask, ATTR_UNDER, false),
                25 => set(attr, mask, ATTR_BLINK, false),
                27 => set(attr, mask, ATTR_REVERSE, false),
                30..=37 => {
                    set(attr, mask, ATTR2_FORE, true);
                    attr.fg = (p - 30) as u32;
                }
                38 | 48 => {
                    if self.config.color_flags.xterm256 {
                        let full = self.config.color_flags.full_color();
                        if let Some((color, consumed)) = extended_color(groups, i, full) {
                            if p == 38 {
                                set(attr, mask, ATTR2_FORE, true);
                                attr.fg = color;
                            } else {
                                set(attr, mask, ATTR2_BACK, true);
                                attr.bg = color;
                            }
                            i += consumed;
                        }
                    }
                    // With CF_XTERM256 clear, upstream falls straight out of the
                    // switch without touching `i` — so the arguments are parsed
                    // as further SGR parameters. Reproduced deliberately.
                }
                39 => {
                    set(attr, mask, ATTR2_FORE, false);
                    attr.fg = DEFAULT_FG;
                }
                40..=47 => {
                    set(attr, mask, ATTR2_BACK, true);
                    attr.bg = (p - 40) as u32;
                }
                49 => {
                    set(attr, mask, ATTR2_BACK, false);
                    attr.bg = DEFAULT_BG;
                }
                90..=97 if self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_FORE, true);
                    attr.fg = (p - 90 + 8) as u32;
                }
                // Order matters: with aixterm16 off, 100 resets both colours;
                // with it on, 100 is bright-black background and falls to the
                // arm below. That fall-through is upstream's, comment and all.
                100 if !self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_COLOR_MASK, false);
                    attr.fg = DEFAULT_FG;
                    attr.bg = DEFAULT_BG;
                }
                100..=107 if self.config.color_flags.aixterm16 => {
                    set(attr, mask, ATTR2_BACK, true);
                    attr.bg = (p - 100 + 8) as u32;
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
                    // DECLRMM. Turning it off also throws the margins away
                    // (`vtterm.c:3168`); turning it on does not set them.
                    69 => {
                        self.lr_margin_mode = on;
                        if !on {
                            self.grid.reset_lr_margins();
                        }
                    }
                    47 | 1047 | 1048 | 1049 => self.alt_screen(p, on),
                    _ => {}
                }
            } else if p == 4 {
                self.grid.insert_mode = on;
            }
        }
    }

    /// `vtterm.c:2970` / `:3030` / `:3144` / `:3194`.
    ///
    /// The `!alt`/`alt` guards are upstream's and they matter: a second
    /// `ESC [ ? 1049 h` while already on the alternate screen must not stash
    /// the alternate screen over the saved main one. Programs that re-arm the
    /// mode on redraw do exactly that.
    fn alt_screen(&mut self, mode: u16, on: bool) {
        if !self.config.alt_screen_enabled {
            return;
        }
        match (mode, on) {
            // 1048 is the cursor half alone, and shares DECSC's slot.
            (1048, true) => self.save_cursor(),
            (1048, false) => self.restore_cursor(),

            (47 | 1047, true) if !self.alt_screen => {
                self.grid.save_screen();
                self.alt_screen = true;
            }
            (47 | 1047, false) if self.alt_screen => {
                self.grid.restore_screen();
                self.alt_screen = false;
            }
            (1049, true) if !self.alt_screen => {
                self.save_cursor();
                self.grid.save_screen();
                self.grid.clear_screen();
                self.alt_screen = true;
            }
            (1049, false) if self.alt_screen => {
                self.grid.clear_screen();
                self.grid.restore_screen();
                self.alt_screen = false;
                self.restore_cursor();
            }
            _ => {}
        }
    }

    /// The `top, left, bottom, right` quadruple every rectangular operation
    /// opens with: 1-based and inclusive on the wire, 0-based here. `None`
    /// means the rectangle is inside out, which upstream treats as "do
    /// nothing" rather than as an empty region.
    ///
    /// Note the two clamps differ. The top-left corner uses `CheckParamVal`,
    /// where an omitted parameter means 1; the bottom-right uses
    /// `CheckParamValMax`, where it means the far edge.
    fn area_rect(&self, params: &Params, first: usize) -> Option<Rect> {
        let rows = self.grid.rows() as u16;
        let cols = self.grid.cols() as u16;
        let mut top = check_param_val(arg0(params, first), rows);
        let left = check_param_val(arg0(params, first + 1), cols);
        let mut bottom = check_param_val_max(arg0(params, first + 2), rows);
        let right = check_param_val_max(arg0(params, first + 3), cols);
        if top > bottom || left > right {
            return None;
        }
        if self.grid.origin_mode {
            let (region_top, region_bottom) = self.grid.scroll_region();
            top = origin_shift(top, region_top, region_bottom);
            bottom = origin_shift(bottom, region_top, region_bottom);
        }
        Some(Rect {
            x0: left as usize - 1,
            y0: top as usize - 1,
            x1: right as usize - 1,
            y1: bottom as usize - 1,
        })
    }

    /// `vtterm.c:SoftReset` — DECSTR (`CSI ! p`), and the first half of
    /// DECSCL.
    fn soft_reset(&mut self) {
        self.grid.soft_reset();
        self.charset.reset();
        self.saved_charset = Some(self.charset.save());
    }

    /// `vtterm.c:SendDCSstr` — `ESC P … ESC \`, or the 8-bit `DCS … ST` when
    /// the terminal has been told it may.
    fn send_dcs(&mut self, body: &str) {
        if self.send_8bit {
            self.send(&[0x90]);
            self.send(body.as_bytes());
            self.send(&[0x9c]);
        } else {
            self.send(b"\x1bP");
            self.send(body.as_bytes());
            self.send(b"\x1b\\");
        }
    }

    /// DECRQSS — `vtterm.c:RequestStatusString`. The request names a setting
    /// by its own final bytes; the reply is `1$r<value><those same bytes>`,
    /// or a bare `0$r` for anything not recognised.
    fn decrqss(&mut self, req: &[u8]) {
        let body = match req {
            [b' ', b'q', ..] => {
                // DECSCUSR. Non-blinking is the odd numbering's +1.
                let n = self.config.cursor_shape + u16::from(self.config.nonblinking_cursor);
                Some(format!("1$r{n} q"))
            }
            [b'"', b'p', ..] => {
                let eight = if self.vt_level > 1 && self.send_8bit {
                    0
                } else {
                    1
                };
                Some(format!("1$r6{};{}\"p", self.vt_level, eight))
            }
            [b'"', b'q', ..] => {
                let on = u8::from(self.grid.pen.attrs & ATTR2_PROTECT != 0);
                Some(format!("1$r{on}\"q"))
            }
            [b'*', b'x', ..] => Some(format!("1$r{}*x", if self.rect_mode { 2 } else { 0 })),
            // These three insist on being the whole request, unlike the four
            // above, which only look at their first two bytes.
            [b'm'] => Some(self.sgr_report()),
            [b'r'] => {
                let (top, bottom) = self.grid.scroll_region();
                Some(format!("1$r{};{}r", top + 1, bottom + 1))
            }
            [b's'] => {
                let (left, right) = self.grid.margins();
                Some(format!("1$r{};{}s", left + 1, right + 1))
            }
            _ => None,
        };
        let body = body.unwrap_or_else(|| "0$r".to_string());
        self.send_dcs(&body);
    }

    /// The SGR half of DECRQSS, which rebuilds a parameter string from the
    /// pen. Every colour branch is gated on a different `ColorFlag`, so the
    /// same pen reports differently depending on which of them are on.
    fn sgr_report(&self) -> String {
        let pen = self.grid.pen;
        let cf = self.config.color_flags;
        let mut out = String::from("1$r0");
        for (bit, code) in [
            (ATTR_BOLD, "1"),
            (ATTR_UNDER, "4"),
            (ATTR_BLINK, "5"),
            (ATTR_REVERSE, "7"),
        ] {
            if pen.attrs & bit != 0 {
                out.push(';');
                out.push_str(code);
            }
        }

        let colour = |set: bool, mut c: u32, brighten: bool, low: u32, high: u32, ext: u32| {
            if !set || !cf.ansi_color {
                return String::new();
            }
            if c <= 7 && brighten && cf.pc_bold16 {
                c += 8;
            }
            if c <= 7 {
                format!(";{}", low + c)
            } else if c <= 15 {
                if cf.aixterm16 {
                    format!(";{}", high + c - 8)
                } else if cf.xterm256 {
                    format!(";{ext};5;{c}")
                } else if cf.pc_bold16 {
                    format!(";{}", low + c - 8)
                } else {
                    String::new()
                }
            } else if cf.xterm256 {
                format!(";{ext};5;{c}")
            } else {
                String::new()
            }
        };

        // Bold brightens the foreground, blink the background — upstream's
        // pairing, and it is not a typo.
        out.push_str(&colour(
            pen.attrs & ATTR2_FORE != 0,
            pen.fg,
            pen.attrs & ATTR_BOLD != 0,
            30,
            90,
            38,
        ));
        out.push_str(&colour(
            pen.attrs & ATTR2_BACK != 0,
            pen.bg,
            pen.attrs & ATTR_BLINK != 0,
            40,
            100,
            48,
        ));
        out.push('m');
        out
    }

    /// `vtterm.c:CSSunSequence` — XTWINOPS, `CSI Ps ; ... t`.
    ///
    /// Only the two operations that mean something without a window are here.
    /// Everything else in that switch asks the display layer where the window
    /// is or moves it, and the answers would come from the oracle's stubs
    /// rather than from Tera Term, so matching them would be matching a stub.
    fn window_op(&mut self, params: &Params) {
        match arg0(params, 0) {
            // Set terminal size. A height or width of 0 or 1 is refused and
            // replaced by the 24x80 default rather than honoured.
            8 if self.config.window_change => {
                let mut rows = arg0(params, 1);
                let mut cols = arg0(params, 2);
                if rows <= 1 {
                    rows = 24;
                }
                if cols <= 1 {
                    cols = 80;
                }
                self.grid.resize(cols as usize, rows as usize);
            }
            // Report terminal size, in the same spelling that sets it.
            18 if self.config.window_report => {
                let body = format!("8;{};{}t", self.grid.rows(), self.grid.cols());
                self.send_csi(&body);
            }
            _ => {}
        }
    }

    /// `vtterm.c:CSDol` — the `$`-intermediate family, which is every
    /// rectangular area operation.
    fn csi_dollar(&mut self, params: &Params, action: char) {
        match action {
            // DECCARA (change) and DECRARA (toggle). Both take the SGR
            // parameters that follow the rectangle, from the fifth onward.
            'r' | 't' => {
                let Some(area) = self.area_rect(params, 0) else {
                    return;
                };
                let groups = sgr_groups(params);
                let mut attr = Pen {
                    fg: DEFAULT_FG,
                    bg: DEFAULT_BG,
                    attrs: 0,
                };
                let mut mask = 0u32;
                self.parse_sgr_params(&groups, 4, &mut attr, &mut mask);
                let keep = ATTR_SGR_MASK | ATTR2_COLOR_MASK;
                attr.attrs &= keep;
                let rect = self.rect_mode;
                if action == 'r' {
                    self.grid
                        .change_attr_area(rect, area, attr, Some(mask & keep));
                } else {
                    self.grid.change_attr_area(rect, area, attr, None);
                }
            }
            // DECCRA. Eight parameters, of which the two page numbers are
            // parsed and ignored — there is only ever one page.
            'v' => {
                let rows = self.grid.rows() as u16;
                let cols = self.grid.cols() as u16;
                let mut sy0 = check_param_val(arg0(params, 0), rows);
                let sx0 = check_param_val(arg0(params, 1), cols);
                let mut sy1 = check_param_val_max(arg0(params, 2), rows);
                let sx1 = check_param_val_max(arg0(params, 3), cols);
                let mut dy = check_param_val(arg0(params, 5), rows);
                let dx = check_param_val(arg0(params, 6), cols);
                if sy0 > sy1 || sx0 > sx1 {
                    return;
                }
                if self.grid.origin_mode {
                    let (top, bottom) = self.grid.scroll_region();
                    sy0 = origin_shift(sy0, top, bottom);
                    sy1 = origin_shift(sy1, top, bottom);
                    dy = origin_shift(dy, top, bottom);
                    // Trim the source rather than the destination, so the copy
                    // stops at the bottom margin instead of crossing it.
                    if (dy + sy1 - sy0) as usize > bottom {
                        sy1 = sy0 + bottom as u16 - dy + 1;
                    }
                }
                let src = Rect {
                    x0: sx0 as usize - 1,
                    y0: sy0 as usize - 1,
                    x1: sx1 as usize - 1,
                    y1: sy1 as usize - 1,
                };
                self.grid.copy_box(src, dx as usize - 1, dy as usize - 1);
            }
            // DECFRA. The fill character comes first and is rejected outright
            // if it is a control code — including the C1 range, which is why
            // the test has a hole in the middle.
            'x' => {
                let ch = arg0(params, 0);
                if !((32..=127).contains(&ch) || (160..=255).contains(&ch)) {
                    return;
                }
                let Some(area) = self.area_rect(params, 1) else {
                    return;
                };
                self.grid.fill_box(ch as u32, area);
            }
            // DECERA and DECSERA.
            'z' | '{' => {
                let Some(area) = self.area_rect(params, 0) else {
                    return;
                };
                if action == 'z' {
                    self.grid.erase_box(area);
                } else {
                    self.grid.selective_erase_box(area);
                }
            }
            _ => {}
        }
    }

    /// Every final byte that carries no intermediate, or ignores the one it
    /// has because upstream does.
    fn csi_plain(
        &mut self,
        params: &Params,
        private: bool,
        gt: bool,
        inter: Option<u8>,
        action: char,
    ) {
        // DECSTBM and friends take no intermediate; anything that arrives with
        // one is a sequence we have not ported, and running it as its
        // no-intermediate namesake would be worse than dropping it.
        if matches!(inter, Some(b) if b != b'?' && b != b'>') {
            return;
        }
        match action {
            '@' => self.grid.insert_chars(arg(params, 0, 1) as usize),
            'A' => self.grid.move_up(arg(params, 0, 1) as usize),
            'B' => self.grid.move_down(arg(params, 0, 1) as usize),
            'C' => self.grid.move_right(arg(params, 0, 1) as usize),
            'D' => self.grid.move_left(arg(params, 0, 1) as usize),
            // CNL and CPL move to the *left margin*, and do it before the
            // vertical move rather than after — `vtterm.c:1691`.
            'E' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_down(arg(params, 0, 1) as usize);
            }
            'F' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_up(arg(params, 0, 1) as usize);
            }
            'G' | '`' => {
                let x = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_to_column(x);
            }
            'H' | 'f' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                let x = arg(params, 1, 1).saturating_sub(1) as usize;
                self.grid.move_cursor_abs(x, y);
            }
            'I' => self.grid.forward_tab(arg(params, 0, 1) as usize),
            // ED, and DECSED under `?`. Mode 3 is not an erase at all: it is
            // `ClearBuffer`, gated on TF_REMOTECLEARSBUFF, and it homes the
            // cursor and resets the scroll region on the way out.
            'J' => match (private, arg0(params, 0)) {
                (_, 3) => {
                    if self.config.remote_clears_buffer {
                        self.grid.clear_buffer();
                    }
                }
                (true, 0) => self.grid.selective_erase_to_end(),
                (true, 1) => self.grid.selective_erase_to_cursor(),
                (true, 2) => {
                    self.grid.selective_erase_to_cursor();
                    self.grid.selective_erase_to_end();
                }
                (true, _) => {}
                (false, mode) => self.grid.erase_display(mode),
            },
            // EL, and DECSEL under `?`.
            'K' => {
                let mode = arg0(params, 0);
                if private {
                    self.grid.selective_erase_line(mode);
                } else {
                    self.grid.erase_line(mode);
                }
            }
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
                    // `vtterm.c:AnswerTerminalType` does *not* use SendCSIstr.
                    // It writes its own introducer, and gates the 8-bit form on
                    // `ts.TerminalID >= IdVT320` as well as on Send8BitMode —
                    // so a VT220 told S8C1T answers DSR with `9B` and Primary
                    // DA with `ESC [`. Note the test is the terminal *id*, not
                    // the VT level, so DECSCL cannot change it.
                    let da = self.config.term_id.primary_da();
                    if self.send_8bit && self.config.term_id.ordinal() >= TermId::Vt320.ordinal() {
                        self.send(&[0x9b, b'?']);
                    } else {
                        self.send(b"\x1b[?");
                    }
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
                    let (left, _) = self.grid.margins();
                    let (col, row) = if self.grid.origin_mode {
                        (x.saturating_sub(left), y.saturating_sub(top))
                    } else {
                        (x, y)
                    };
                    let body = format!("{};{}R", row + 1, col + 1);
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
            't' => self.window_op(params),
            // DECSLRM, but only while DECLRMM is on: otherwise the same final
            // byte is SCP, the ANSI.SYS save-cursor. `vtterm.c:4115`.
            's' => {
                if self.lr_margin_mode {
                    let cols = self.grid.cols() as u16;
                    let left = check_param_val(arg0(params, 0), cols);
                    let right = check_param_val_max(arg0(params, 1), cols);
                    self.grid
                        .set_lr_margins(left as usize - 1, right as usize - 1);
                } else {
                    self.save_cursor();
                }
            }
            'u' => self.restore_cursor(),
            _ => {}
        }
    }

    /// DECSC, and the save half of `ESC [ ? 1048 h` — upstream shares the slot,
    /// charset state included.
    fn save_cursor(&mut self) {
        self.grid.save_cursor();
        self.saved_charset = Some(self.charset.save());
    }

    fn restore_cursor(&mut self) {
        self.grid.restore_cursor();
        if let Some(s) = self.saved_charset {
            self.charset.restore(s);
        }
    }
}

/// Decode `38;2;r;g;b` / `38;5;idx` in all the colon and semicolon spellings
/// upstream accepts. Returns the colour and how many extra parameter groups it
/// swallowed.
fn extended_color(groups: &[Vec<u16>], i: usize, full_color: bool) -> Option<(u32, usize)> {
    let rgb =
        |r: u16, g: u16, b: u16| palette::find_closest(r as i32, g as i32, b as i32, full_color);

    // Colon form: 38:5:idx arrives as a single group.
    let g = &groups[i];
    if g.len() > 1 {
        return match g[1] {
            2 if g.len() >= 5 => Some((rgb(g[2], g[3], g[4])?, 0)),
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
            Some((rgb(r, g_, b)?, 4))
        }
        5 => {
            let idx = groups.get(i + 2)?.first().copied()?;
            Some(((idx as u32).min(255), 2))
        }
        _ => None,
    }
}

/// Set or clear `bits` on the attribute and record them in the mask either way
/// — upstream's `attr->Attr |= X; mask->Attr |= X` pair, which is what makes
/// "bold off" and "bold on" both *mention* bold as far as DECCARA cares.
fn set(attr: &mut Pen, mask: &mut u32, bits: u32, on: bool) {
    if on {
        attr.attrs |= bits;
    } else {
        attr.attrs &= !bits;
    }
    *mask |= bits;
}

/// The parameter list as groups, with an absent list standing in for a bare
/// `0` the way `CSI m` does.
fn sgr_groups(params: &Params) -> Vec<Vec<u16>> {
    let groups: Vec<Vec<u16>> = params.iter().map(|g| g.to_vec()).collect();
    if groups.is_empty() {
        vec![vec![0u16]]
    } else {
        groups
    }
}

/// Origin mode moves a rectangle's row down to the scroll region, and parks it
/// one past the bottom margin rather than on it when it would overshoot —
/// which is upstream's arithmetic, 1-based row against 0-based margin and all.
fn origin_shift(row: u16, region_top: usize, region_bottom: usize) -> u16 {
    let shifted = row + region_top as u16;
    if shifted as usize > region_bottom {
        region_bottom as u16 + 1
    } else {
        shifted
    }
}

/// `vtterm.c:CheckParamVal` — zero means one, out of range means the maximum.
fn check_param_val(p: u16, max: u16) -> u16 {
    if p == 0 {
        1
    } else if p > max {
        max
    } else {
        p
    }
}

/// `vtterm.c:CheckParamValMax` — zero means the *maximum*, not one. The
/// difference is what makes an omitted bottom-right corner mean "the far
/// corner" while an omitted top-left means "the origin".
fn check_param_val_max(p: u16, max: u16) -> u16 {
    if p == 0 || p > max {
        max
    } else {
        p
    }
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
        let cp = c as u32;
        // `vtterm.c:788` only consults the charset for codepoints that could
        // have come from a single byte; anything above U+00FF is text by
        // definition and never DEC special graphics.
        let special = cp <= 0xff && self.charset.is_special(cp);
        if special {
            // Upstream builds a throwaway attribute for the one character
            // (`CharAttrTmp`), leaving the pen alone. Same here.
            let pen = self.grid.pen.attrs;
            self.grid.pen.attrs |= ATTR_SPECIAL;
            self.grid.put(cp);
            self.grid.pen.attrs = pen;
        } else {
            self.grid.put(cp);
        }
        self.last_printed = Some(cp);
        self.prev_was_cr = false;
        self.prev_was_lf = false;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {} // BEL — the oracle silences it (IdBeepOff)
            0x08 => self.grid.backspace(),
            0x09 => {
                // `vtterm.c:Tab()` — a plain HT takes the *pending wrap first*
                // and only then tabs, so a tab arriving on a full line starts
                // the next one. CHT (`CSI Ps I`) does not do this; it calls
                // `CursorForwardTab` directly. `ts.VTCompatTab` would suppress
                // it, but it is off by default.
                if self.grid.cursor.pending_wrap {
                    self.grid.carriage_return();
                    self.grid.line_feed();
                    self.grid.cursor.pending_wrap = false;
                }
                self.grid.forward_tab(1);
            }
            0x0e => self.shift(Shift::Ls1), // SO
            0x0f => self.shift(Shift::Ls0), // SI
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
        let inter = intermediates.first().copied();
        let private = inter == Some(b'?');
        let gt = inter == Some(b'>');

        // Intermediates change what a final byte means, and silently ignoring
        // them is how `CSI ... $ r` (DECCARA) ends up reprogramming the scroll
        // region. Everything below assumes no intermediate unless it says so.
        match (inter, action) {
            // DECSCA — `vtterm.c:3335`. 0 and 2 both clear.
            (Some(b'"'), 'q') => match arg0(params, 0) {
                0 | 2 => self.grid.pen.attrs &= !ATTR2_PROTECT,
                1 => self.grid.pen.attrs |= ATTR2_PROTECT,
                _ => {}
            },
            // DECSACE — `vtterm.c:CSAster`. Anything but 0, 1 and 2 leaves the
            // mode alone rather than resetting it.
            (Some(b'*'), 'x') => match arg0(params, 0) {
                0 | 1 => self.rect_mode = false,
                2 => self.rect_mode = true,
                _ => {}
            },
            // DECSTR, the soft reset.
            (Some(b'!'), 'p') => self.soft_reset(),
            // DECSCL. It soft-resets, re-reads the terminal id's level, and
            // can then only *lower* it — `CSI 61"p` forces level 1.
            (Some(b'"'), 'p') => {
                self.soft_reset();
                self.vt_level = self.config.term_id.vt_level();
                self.send_8bit = self.vt_level >= 2 && self.config.send_8bit_ctrl;
                match arg0(params, 0) {
                    p @ 61..=65 => {
                        let want = (p - 60) as u8;
                        if self.vt_level > want {
                            self.vt_level = want;
                        }
                    }
                    _ => self.vt_level = 1,
                }
                self.send_8bit = !(self.vt_level < 2 || arg0(params, 1) == 1);
            }
            (Some(b'$'), _) => self.csi_dollar(params, action),
            _ => self.csi_plain(params, private, gt, inter, action),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }

        // DECALN — `ESC # 8`. It fills the screen with `E` and resets both
        // margin pairs, which is why it is here and not with the charsets.
        if intermediates.first() == Some(&b'#') {
            if byte == b'8' {
                self.grid.fill_with_e();
                self.grid.reset_scroll_region();
                self.grid.reset_lr_margins();
                self.grid.move_cursor(0, 0);
            }
            return;
        }

        // S7C1T / S8C1T — `ESC SP F` and `ESC SP G` (`vtterm.c:ESCSpace`).
        // The 8-bit form is refused below VT level 2, and DECSCL having
        // lowered the level is enough to refuse it.
        if intermediates.first() == Some(&b' ') {
            match byte {
                b'F' => self.send_8bit = false,
                b'G' if self.vt_level >= 2 => self.send_8bit = true,
                _ => {}
            }
            return;
        }

        // Single-byte character-set designation: ESC ( ) * + <final>.
        if let Some(&i) = intermediates.first() {
            if matches!(i, b'(' | b')' | b'*' | b'+') && intermediates.len() == 1 {
                if let Some(cs) = sbcs_final(byte, self.config.japanese) {
                    self.charset.designate(gset_from_intermediate(i), cs);
                }
                // TF_AUTOINVOKE would fold G0 into GL here, but the key
                // defaults off (ttset.c:1102) so there is nothing to do.
            }
            // Multi-byte designations (ESC $ ...) are Kanji, deferred with CJK.
            return;
        }

        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'D' => self.grid.line_feed(),
            b'E' => {
                self.grid.carriage_return();
                self.grid.line_feed();
            }
            b'H' => self.grid.set_tab(),
            b'M' => self.grid.reverse_index(),
            // DECBI / DECFI (vtterm.c:1482, :1493). Both are no-ops when the
            // cursor is outside the scroll region, and scroll the region
            // sideways rather than moving when it is already on the margin.
            b'6' => {
                if self.cursor_in_region() {
                    if self.grid.cursor.x == self.grid.margins().0 {
                        self.grid.scroll_right(1);
                    } else {
                        self.grid.move_left(1);
                    }
                }
            }
            b'9' => {
                if self.cursor_in_region() {
                    if self.grid.cursor.x == self.grid.margins().1 {
                        self.grid.scroll_left(1);
                    } else {
                        self.grid.move_right(1);
                    }
                }
            }
            b'N' => self.shift(Shift::Ss2),
            b'O' => self.shift(Shift::Ss3),
            b'n' => self.shift(Shift::Ls2),
            b'o' => self.shift(Shift::Ls3),
            b'|' => self.shift(Shift::Ls3r),
            b'}' => self.shift(Shift::Ls2r),
            b'~' => self.shift(Shift::Ls1r),
            b'c' => {
                self.grid.reset();
                self.charset.reset();
                self.saved_charset = None;
                self.title.clear();
                self.rect_mode = false;
                self.lr_margin_mode = false;
                // `ResetTerminal` ends with `ChangeTerminalID()`
                // (`vtterm.c:5850`), which recomputes both from the terminal
                // *id* — so RIS undoes a DECSCL that lowered the level, and
                // undoes an S8C1T only as far as the configured default.
                self.vt_level = self.config.term_id.vt_level();
                self.send_8bit = self.vt_level >= 2 && self.config.send_8bit_ctrl;
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        self.dcs = Some((intermediates.first().copied(), action));
        self.dcs_buf.clear();
    }

    fn put(&mut self, byte: u8) {
        // `ts.MaxOSCBufferSize` bounds the OSC buffer upstream; the DCS one is
        // a fixed 256 in `DeviceControl`. Either way an unterminated DCS must
        // not be able to grow without limit.
        if self.dcs.is_some() && self.dcs_buf.len() < 256 {
            self.dcs_buf.push(byte);
        }
    }

    fn unhook(&mut self) {
        let Some((inter, action)) = self.dcs.take() else {
            return;
        };
        // `+q` is xterm's termcap query and `!{` is DECSTUI; neither is
        // implemented, and both are dropped rather than answered wrongly.
        if inter == Some(b'$') && action == 'q' {
            let req = std::mem::take(&mut self.dcs_buf);
            self.decrqss(&req);
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
    fn sgr_38_sets_a_256_colour_foreground_by_default() {
        let vt = run(b"\x1b[38;5;196mR", 20, 2);
        let cell = vt.grid().line(0)[0];
        assert_eq!(cell.attrs & ATTR2_FORE, ATTR2_FORE);
        assert_eq!(cell.fg, 196);
        assert_eq!(cell.attrs & ATTR_BLINK, 0);
    }

    #[test]
    fn sgr_38_leaks_its_arguments_when_xterm256_is_off() {
        // Not a bug, and the reason ColorFlags is modelled at all: with the bit
        // clear, `38` is ignored *without consuming* `5`, which then turns
        // blink on. vtterm.c:2239.
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 2,
            color_flags: ColorFlags {
                xterm256: false,
                ..ColorFlags::default()
            },
            ..Config::default()
        });
        vt.feed(b"\x1b[38;5;196mR");
        let cell = vt.grid().line(0)[0];
        assert_eq!(cell.attrs & ATTR_BLINK, ATTR_BLINK);
        assert_eq!(cell.attrs & ATTR2_COLOR_MASK, 0);
    }

    #[test]
    fn so_switches_to_line_drawing_and_si_switches_back() {
        let vt = run(b"\x0eqq\x0fqq", 12, 1);
        let attrs: Vec<u32> = (0..4)
            .map(|x| vt.grid().line(0)[x].attrs & ATTR_SPECIAL)
            .collect();
        assert_eq!(attrs, vec![ATTR_SPECIAL, ATTR_SPECIAL, 0, 0]);
    }

    #[test]
    fn esc_open_paren_zero_designates_dec_special_graphics() {
        let vt = run(b"\x1b(0qq\x1b(Bqq", 12, 1);
        let attrs: Vec<u32> = (0..4)
            .map(|x| vt.grid().line(0)[x].attrs & ATTR_SPECIAL)
            .collect();
        assert_eq!(attrs, vec![ATTR_SPECIAL, ATTR_SPECIAL, 0, 0]);
        // The byte is stored as-is; mapping it to U+2500 is the renderer's job,
        // because DecSpMappingDir defaults to "do not map".
        assert_eq!(vt.grid().line(0)[0].text[0], b'q' as u32);
    }

    #[test]
    fn c1_controls_fold_to_c0_on_a_vt100() {
        // U+008D is 0x0D once masked, so it is a carriage return and C lands on
        // top of A. On a VT220 the mask does not apply and it would be RI.
        let vt = run("A\u{84}B\u{8d}C".as_bytes(), 16, 3);
        assert_eq!(row(&vt, 0), "CB");
        assert_eq!(vt.grid().cursor.x, 1);
    }

    #[test]
    fn c1_controls_keep_their_meaning_above_vt100() {
        let mut vt = Vt::new(Config {
            cols: 16,
            rows: 3,
            term_id: TermId::Vt220,
            ..Config::default()
        });
        vt.feed("A\u{8d}B".as_bytes()); // U+008D = RI at level 2
        assert_eq!(row(&vt, 0), " B");
        assert_eq!(row(&vt, 1), "A");
    }

    #[test]
    fn a_split_c1_survives_the_chunk_boundary() {
        let mut vt = Vt::new(Config {
            cols: 16,
            rows: 3,
            ..Config::default()
        });
        vt.feed(b"A\xc2");
        vt.feed(b"\x8dC");
        assert_eq!(row(&vt, 0), "C");
    }

    #[test]
    fn alt_screen_hides_its_contents_and_restores_the_cursor() {
        let vt = run(b"main\x1b[?1049hALT\x1b[?1049l", 12, 3);
        assert_eq!(row(&vt, 0), "main");
        assert_eq!(vt.grid().cursor.x, 4);
    }

    #[test]
    fn re_entering_the_alt_screen_does_not_clobber_the_saved_main() {
        // A program that re-arms 1049 on every redraw would otherwise stash the
        // alternate screen over the main one and lose it.
        let vt = run(b"main\x1b[?1049hALT\x1b[?1049hMORE\x1b[?1049l", 12, 3);
        assert_eq!(row(&vt, 0), "main");
    }

    #[test]
    fn truecolor_resolves_through_the_palette() {
        let vt = run(b"\x1b[38;2;255;0;0mR", 16, 2);
        assert_eq!(vt.grid().line(0)[0].fg, 1);
    }

    #[test]
    fn decsc_restores_the_g_sets() {
        let vt = run(b"\x1b(0\x1b7\x1b(Bq\x1b8q", 12, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_SPECIAL, ATTR_SPECIAL);
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
    fn decsca_survives_sgr_zero_and_only_decsca_clears_it() {
        // vtterm.c:2178 ORs the protect bit back in after SGR 0, so a program
        // that resets attributes between fields keeps its protected regions.
        let vt = run(b"\x1b[1\"q\x1b[0ma\x1b[2\"qb", 4, 1);
        assert_eq!(
            vt.grid().line(0)[0].attrs & tt_grid::ATTR2_PROTECT,
            tt_grid::ATTR2_PROTECT
        );
        assert_eq!(vt.grid().line(0)[1].attrs & tt_grid::ATTR2_PROTECT, 0);
    }

    #[test]
    fn decsel_skips_protected_cells_and_el_does_not() {
        let vt = run(b"\x1b[0\"qAA\x1b[1\"qBB\x1b[0\"qCC\x1b[1;1H\x1b[?2K", 6, 2);
        assert_eq!(row(&vt, 0), "  BB");
        let vt = run(b"\x1b[0\"qAA\x1b[1\"qBB\x1b[0\"qCC\x1b[1;1H\x1b[2K", 6, 2);
        assert_eq!(row(&vt, 0), "");
    }

    #[test]
    fn selective_erase_keeps_the_sgr_bits_where_a_plain_erase_drops_them() {
        // BuffSelectedEraseCharsInLine masks to AttrSgrMask instead of
        // painting the pen, so bold outlives DECSEL.
        let vt = run(b"\x1b[1mA\x1b[1;1H\x1b[?2K", 4, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_BOLD, ATTR_BOLD);
        let vt = run(b"\x1b[1mA\x1b[1;1H\x1b[2K", 4, 1);
        assert_eq!(vt.grid().line(0)[0].attrs & ATTR_BOLD, 0);
    }

    #[test]
    fn decrqss_answers_for_a_known_setting_and_refuses_an_unknown_one() {
        let vt = run(b"\x1b[2;5r\x1bP$qr\x1b\\", 20, 8);
        assert_eq!(vt.reply(), b"\x1bP1$r2;5r\x1b\\");
        let vt = run(b"\x1bP$qZ\x1b\\", 20, 4);
        assert_eq!(vt.reply(), b"\x1bP0$r\x1b\\");
    }

    #[test]
    fn decstr_reloads_the_saved_cursor_with_the_origin() {
        // SoftReset saves the cursor at 0,0 rather than where it is, so a
        // DECRC straight afterwards homes it. Surprising, and upstream's.
        let vt = run(b"\x1b[3;3Hx\x1b[!p\x1b8X", 12, 4);
        assert_eq!(row(&vt, 0), "X");
        assert_eq!(vt.grid().margins(), (0, 11));
    }

    #[test]
    fn decslrm_moves_the_wrap_point_and_the_carriage_return() {
        // DECSLRM needs DECLRMM on first; without it `CSI 4;10s` is SCP.
        let vt = run(b"\x1b[?69h\x1b[4;10s\x1b[1;4HABCDEFGHIJ", 16, 3);
        assert_eq!(row(&vt, 0), "   ABCDEFG");
        assert_eq!(row(&vt, 1), "   HIJ");
    }

    #[test]
    fn decslrm_is_scp_while_declrmm_is_off() {
        let vt = run(b"\x1b[2;3Hx\x1b[5;5s\x1b[1;1Hy\x1b[uz", 16, 6);
        assert_eq!(vt.grid().margins(), (0, 15));
        // The cursor came back to where `CSI 5;5s` saved it, after the `y`.
        assert_eq!(row(&vt, 1), "  xz");
    }

    #[test]
    fn a_tab_at_the_end_of_a_line_wraps_before_it_tabs() {
        // `vtterm.c:Tab()` takes the pending wrap first. Getting this wrong
        // leaves the tab on the old row and the next character a row too high.
        let vt = run(b"\x1b[1;1H\t\t\tX", 16, 3);
        assert_eq!(row(&vt, 0), "");
        assert_eq!(row(&vt, 1), "        X");
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
