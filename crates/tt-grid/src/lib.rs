//! tt-grid — the terminal grid: cells, lines, cursor, scroll region, scrollback.
//!
//! The cell layout and the attribute encoding deliberately mirror Tera Term's
//! `buffer.h`, because the differential oracle dumps them and `tt-vt` is being
//! ported against that dump. The attribute bit values in particular are **not**
//! free to renumber: `oracle/src/main.c` prints them.
//!
//! Everything here is pure data movement — no I/O, no escape sequences. The
//! parser lives in `tt-vt`.

use std::collections::VecDeque;

/// Codepoints stored per cell: one base plus up to three combining marks.
///
/// Tera Term grows its per-cell string without a fixed bound. A fixed array
/// keeps `Cell` POD and copyable across the C ABI, which the frontend seam
/// needs; marks past the third are dropped. Raise this if a real corpus wants
/// it — Hebrew and Thai stacks reach three, Zalgo text is not a use case.
pub const CELL_TEXT_MAX: usize = 4;

// --- attributes (buffer.h:42-61) -----------------------------------------
//
// Low byte is Tera Term's `Attr`, second byte its `Attr2`, packed into one word
// so `Cell` stays flat.

pub const ATTR_BOLD: u32 = 0x0001;
pub const ATTR_UNDER: u32 = 0x0002;
pub const ATTR_SPECIAL: u32 = 0x0004;
pub const ATTR_BLINK: u32 = 0x0008;
pub const ATTR_REVERSE: u32 = 0x0010;

/// `AttrSgrMask` (`buffer.h:58`) — the four attributes SGR itself can set, and
/// the only ones a selective erase leaves behind.
pub const ATTR_SGR_MASK: u32 = ATTR_BOLD | ATTR_UNDER | ATTR_BLINK | ATTR_REVERSE;
/// The low byte, which is upstream's `Attr`. Masking only it is what
/// `attr &= ...` means in `buffer.c`.
pub const ATTR_MASK: u32 = 0x00ff;

pub const ATTR2_FORE: u32 = 0x0100;
pub const ATTR2_BACK: u32 = 0x0200;
pub const ATTR2_COLOR_MASK: u32 = ATTR2_FORE | ATTR2_BACK;
/// `Attr2Protect` (`buffer.h:66`) — DECSCA's bit. Selective erase skips a cell
/// carrying it, and SGR 0 deliberately does **not** clear it
/// (`vtterm.c:2178`).
pub const ATTR2_PROTECT: u32 = 0x0400;

/// Tera Term's `AttrDefaultFG` / `AttrDefaultBG` are both 0, not 7/0.
pub const DEFAULT_FG: u32 = 0;
pub const DEFAULT_BG: u32 = 0;

// --- cell width ----------------------------------------------------------

/// One column.
pub const WIDTH_NARROW: u8 = 0;
/// Two columns; the cell to the right is [`WIDTH_PAD`].
pub const WIDTH_WIDE: u8 = 1;
/// The right half of a wide character. Holds no text of its own.
pub const WIDTH_PAD: u8 = 2;

/// Display columns consumed by a codepoint.
///
/// This is the one place where we currently disagree with the oracle by
/// construction: Tera Term uses its own tables in `unicode.cpp`, we use the
/// `unicode-width` crate. Both derive from `EastAsianWidth.txt`, so they agree
/// on unambiguous characters, but ambiguous-width policy and emoji presentation
/// are exactly where they will drift. Resolving that is deferred with CJK; see
/// PLAN.md.
pub fn char_width(cp: u32) -> usize {
    match char::from_u32(cp) {
        Some(c) => unicode_width::UnicodeWidthChar::width(c).unwrap_or(0),
        None => 0,
    }
}

/// Does this codepoint attach to the preceding cell rather than occupying one?
///
/// Only nonspacing marks are handled. Tera Term additionally recognises spacing
/// combining marks (Devanagari and friends), which advance the cursor while
/// still joining the base cell — `buffer.c`'s `combining_type != 1` path. Those
/// are not modelled yet.
pub fn is_combining(cp: u32) -> bool {
    // C0/C1 never reach the grid as text; guard anyway so a stray control
    // cannot be mistaken for a zero-width mark.
    if cp < 0x20 || (0x80..=0x9f).contains(&cp) {
        return false;
    }
    char_width(cp) == 0
}

/// Foreground, background and attribute state applied to newly written cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pen {
    pub fg: u32,
    pub bg: u32,
    pub attrs: u32,
}

impl Default for Pen {
    fn default() -> Self {
        Pen {
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            attrs: 0,
        }
    }
}

/// One screen cell. POD, `repr(C)`: this crosses the C ABI unchanged.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub text: [u32; CELL_TEXT_MAX],
    pub fg: u32,
    pub bg: u32,
    pub attrs: u32,
    pub width_class: u8,
}

impl Cell {
    pub fn blank(pen: Pen) -> Self {
        Cell {
            text: [b' ' as u32, 0, 0, 0],
            fg: pen.fg,
            bg: pen.bg,
            attrs: pen.attrs,
            width_class: WIDTH_NARROW,
        }
    }

    /// The cell an erase leaves behind.
    ///
    /// Tera Term erases to the *current colours* but to *default attributes* —
    /// `buffer.c` passes `CurCharAttr.Fore/Back` and `AttrDefault` to `memsetW`.
    /// So background-colour erase is unconditional here, while bold/underline
    /// never survive an erase. Getting this backwards is invisible until
    /// something paints a coloured background and then clears part of it.
    pub fn erased(pen: Pen) -> Self {
        Cell {
            text: [b' ' as u32, 0, 0, 0],
            fg: pen.fg,
            bg: pen.bg,
            attrs: pen.attrs & ATTR2_COLOR_MASK,
            width_class: WIDTH_NARROW,
        }
    }

    /// Codepoints held by this cell: base first, then combining marks.
    pub fn codepoints(&self) -> impl Iterator<Item = u32> + '_ {
        self.text.iter().copied().take_while(|&c| c != 0)
    }

    /// `buffer.c:BuffSetChar(b, ' ', 'H')` — how the *shift* and *overwrite*
    /// paths break a wide character they cannot keep whole.
    ///
    /// It is deliberately **not** an erase. The text becomes a space and the
    /// width class and colour indices go back to default, but the SGR
    /// attribute bits are left exactly as they were and the pen is never
    /// consulted. Erasing paints the pen over the cell; this does not, and the
    /// two are visible apart the moment a coloured wide character is
    /// overwritten by a narrow one under a different pen.
    pub fn crush(&mut self) {
        self.text = [b' ' as u32, 0, 0, 0];
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.width_class = WIDTH_NARROW;
    }

    /// Append a combining mark. Returns false if the cell is full.
    pub fn push(&mut self, cp: u32) -> bool {
        for slot in self.text.iter_mut() {
            if *slot == 0 {
                *slot = cp;
                return true;
            }
        }
        false
    }
}

/// An inclusive, 0-based rectangle — how the `$`-intermediate operations name
/// the area they act on. On the wire it arrives 1-based as top, left, bottom,
/// right; `tt-vt` does the conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    /// Tera Term's `Wrap`: the last glyph landed on the right margin and the
    /// *next* one wraps. Deferred wrap, not eager — this is why writing exactly
    /// `cols` characters leaves the cursor on the last column, not the next row.
    pub pending_wrap: bool,
}

#[derive(Clone, Copy, Debug)]
struct SavedCursor {
    x: usize,
    y: usize,
    pending_wrap: bool,
    pen: Pen,
    origin_mode: bool,
}

pub type Line = Vec<Cell>;

pub struct Grid {
    cols: usize,
    rows: usize,
    lines: Vec<Line>,
    scrollback: VecDeque<Line>,
    scrollback_max: usize,
    pub cursor: Cursor,
    pub pen: Pen,
    /// Scroll region, 0-based and inclusive. Tera Term's `CursorTop`/`CursorBottom`.
    top: usize,
    bottom: usize,
    pub insert_mode: bool,
    pub autowrap: bool,
    pub origin_mode: bool,
    tabs: Vec<bool>,
    saved: Option<SavedCursor>,
    /// The main screen, parked here while the alternate screen is up.
    /// `buffer.c:BuffSaveScreen`/`BuffRestoreScreen`.
    stashed: Option<Vec<Line>>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, scrollback_max: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let pen = Pen::default();
        Grid {
            cols,
            rows,
            lines: vec![vec![Cell::blank(pen); cols]; rows],
            scrollback: VecDeque::new(),
            scrollback_max,
            cursor: Cursor::default(),
            pen,
            top: 0,
            bottom: rows - 1,
            insert_mode: false,
            // DECAWM is on out of reset.
            autowrap: true,
            origin_mode: false,
            tabs: default_tabs(cols),
            saved: None,
            stashed: None,
        }
    }

    /// `BuffSaveScreen` — park a copy of the visible page.
    pub fn save_screen(&mut self) {
        self.stashed = Some(self.lines.clone());
    }

    /// `BuffRestoreScreen` — put the parked page back. A no-op if nothing was
    /// saved, which is what keeps a stray `ESC [ ? 1047 l` harmless.
    pub fn restore_screen(&mut self) {
        if let Some(lines) = self.stashed.take() {
            self.lines = lines;
        }
    }

    /// `BuffClearScreen`.
    pub fn clear_screen(&mut self) {
        let pen = self.pen;
        for line in &mut self.lines {
            *line = vec![Cell::erased(pen); self.cols];
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn line(&self, y: usize) -> &[Cell] {
        &self.lines[y]
    }

    pub fn scrollback(&self) -> impl Iterator<Item = &Line> {
        self.scrollback.iter()
    }

    /// RIS. Everything except the scrollback, which survives a reset in Tera
    /// Term as it does in xterm.
    pub fn reset(&mut self) {
        self.pen = Pen::default();
        for y in 0..self.rows {
            self.lines[y] = vec![Cell::blank(self.pen); self.cols];
        }
        self.cursor = Cursor::default();
        self.top = 0;
        self.bottom = self.rows - 1;
        self.insert_mode = false;
        self.autowrap = true;
        self.origin_mode = false;
        self.tabs = default_tabs(self.cols);
        self.saved = None;
    }

    // --- cursor ----------------------------------------------------------

    /// `buffer.c:MoveCursor` — note it clears the pending wrap. Every cursor
    /// motion that goes through here does; scrolling does not.
    pub fn move_cursor(&mut self, x: usize, y: usize) {
        self.cursor.x = x.min(self.cols - 1);
        self.cursor.y = y.min(self.rows - 1);
        self.cursor.pending_wrap = false;
    }

    /// Cursor position for CUP/HVP, honouring origin mode.
    pub fn move_cursor_abs(&mut self, x: usize, y: usize) {
        if self.origin_mode {
            let y = (self.top + y).min(self.bottom);
            self.move_cursor(x, y);
        } else {
            self.move_cursor(x, y);
        }
    }

    pub fn move_up(&mut self, n: usize) {
        // Cursor motion stops at the scroll region edge, but only if the cursor
        // started inside it.
        let limit = if self.cursor.y >= self.top {
            self.top
        } else {
            0
        };
        let y = self.cursor.y.saturating_sub(n).max(limit);
        self.move_cursor(self.cursor.x, y);
    }

    pub fn move_down(&mut self, n: usize) {
        let limit = if self.cursor.y <= self.bottom {
            self.bottom
        } else {
            self.rows - 1
        };
        let y = (self.cursor.y + n).min(limit);
        self.move_cursor(self.cursor.x, y);
    }

    pub fn move_left(&mut self, n: usize) {
        let x = self.cursor.x.saturating_sub(n);
        self.move_cursor(x, self.cursor.y);
    }

    pub fn move_right(&mut self, n: usize) {
        let x = (self.cursor.x + n).min(self.cols - 1);
        self.move_cursor(x, self.cursor.y);
    }

    /// `vtterm.c:CarriageReturn`. It only moves — and therefore only clears the
    /// pending wrap — when the cursor is not already at the left margin.
    pub fn carriage_return(&mut self) {
        if self.origin_mode || self.cursor.x > 0 {
            self.move_cursor(0, self.cursor.y);
        }
    }

    /// `vtterm.c:LineFeed`. At the bottom of the scroll region this scrolls
    /// rather than moving, and scrolling does **not** clear the pending wrap —
    /// an upstream quirk we reproduce rather than fix, because the oracle is
    /// ground truth.
    pub fn line_feed(&mut self) {
        if self.cursor.y < self.bottom {
            self.move_cursor(self.cursor.x, self.cursor.y + 1);
        } else if self.cursor.y == self.bottom {
            self.scroll_up(1);
        } else if self.cursor.y + 1 < self.rows {
            self.move_cursor(self.cursor.x, self.cursor.y + 1);
        }
    }

    /// RI. Mirror image of `line_feed`.
    pub fn reverse_index(&mut self) {
        if self.cursor.y > self.top {
            self.move_cursor(self.cursor.x, self.cursor.y - 1);
        } else if self.cursor.y == self.top {
            self.scroll_down(1);
        } else if self.cursor.y > 0 {
            self.move_cursor(self.cursor.x, self.cursor.y - 1);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor.x > 0 {
            self.move_cursor(self.cursor.x - 1, self.cursor.y);
        }
    }

    pub fn save_cursor(&mut self) {
        self.saved = Some(SavedCursor {
            x: self.cursor.x,
            y: self.cursor.y,
            pending_wrap: self.cursor.pending_wrap,
            pen: self.pen,
            origin_mode: self.origin_mode,
        });
    }

    pub fn restore_cursor(&mut self) {
        if let Some(s) = self.saved {
            self.origin_mode = s.origin_mode;
            self.pen = s.pen;
            self.cursor.x = s.x.min(self.cols - 1);
            self.cursor.y = s.y.min(self.rows - 1);
            self.cursor.pending_wrap = s.pending_wrap;
        } else {
            self.move_cursor(0, 0);
        }
    }

    // --- tabs ------------------------------------------------------------

    pub fn set_tab(&mut self) {
        let x = self.cursor.x;
        self.tabs[x] = true;
    }

    pub fn clear_tab(&mut self) {
        let x = self.cursor.x;
        self.tabs[x] = false;
    }

    pub fn clear_all_tabs(&mut self) {
        self.tabs = vec![false; self.cols];
    }

    pub fn forward_tab(&mut self, n: usize) {
        let mut x = self.cursor.x;
        for _ in 0..n.max(1) {
            let mut next = self.cols - 1;
            for probe in (x + 1)..self.cols {
                if self.tabs[probe] {
                    next = probe;
                    break;
                }
            }
            x = next;
        }
        self.move_cursor(x, self.cursor.y);
    }

    pub fn backward_tab(&mut self, n: usize) {
        let mut x = self.cursor.x;
        for _ in 0..n.max(1) {
            let mut next = 0;
            for probe in (0..x).rev() {
                if self.tabs[probe] {
                    next = probe;
                    break;
                }
            }
            x = next;
        }
        self.move_cursor(x, self.cursor.y);
    }

    // --- scroll region ---------------------------------------------------

    pub fn scroll_region(&self) -> (usize, usize) {
        (self.top, self.bottom)
    }

    /// DECSTBM (`vtterm.c:2452`). A region of fewer than two lines is rejected
    /// outright, leaving the previous region in place rather than a degenerate
    /// one — and note where the cursor lands: the *screen* origin unless origin
    /// mode is on, not the top of the new region.
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let max = self.rows - 1;
        let (top, bottom) = (top.min(max), bottom.min(max));
        if top >= bottom {
            return;
        }
        self.top = top;
        self.bottom = bottom;
        if self.origin_mode {
            self.move_cursor(0, self.top);
        } else {
            self.move_cursor(0, 0);
        }
    }

    pub fn reset_scroll_region(&mut self) {
        self.top = 0;
        self.bottom = self.rows - 1;
    }

    pub fn scroll_up(&mut self, n: usize) {
        let pen = self.pen;
        let full_screen = self.top == 0 && self.bottom == self.rows - 1;
        for _ in 0..n.min(self.bottom - self.top + 1) {
            let line = self.lines.remove(self.top);
            if full_screen {
                self.push_scrollback(line);
            }
            self.lines
                .insert(self.bottom, vec![Cell::erased(pen); self.cols]);
        }
    }

    pub fn scroll_down(&mut self, n: usize) {
        let pen = self.pen;
        for _ in 0..n.min(self.bottom - self.top + 1) {
            self.lines.remove(self.bottom);
            self.lines
                .insert(self.top, vec![Cell::erased(pen); self.cols]);
        }
    }

    fn push_scrollback(&mut self, line: Line) {
        if self.scrollback_max == 0 {
            return;
        }
        if self.scrollback.len() == self.scrollback_max {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back(line);
    }

    // --- editing ---------------------------------------------------------

    /// IL. Only acts while the cursor is inside the scroll region.
    pub fn insert_lines(&mut self, n: usize) {
        if self.cursor.y < self.top || self.cursor.y > self.bottom {
            return;
        }
        let pen = self.pen;
        for _ in 0..n.min(self.bottom - self.cursor.y + 1) {
            self.lines.remove(self.bottom);
            self.lines
                .insert(self.cursor.y, vec![Cell::erased(pen); self.cols]);
        }
    }

    /// DL.
    pub fn delete_lines(&mut self, n: usize) {
        if self.cursor.y < self.top || self.cursor.y > self.bottom {
            return;
        }
        let pen = self.pen;
        for _ in 0..n.min(self.bottom - self.cursor.y + 1) {
            self.lines.remove(self.cursor.y);
            self.lines
                .insert(self.bottom, vec![Cell::erased(pen); self.cols]);
        }
    }

    /// ICH — `buffer.c:BuffInsertSpace`.
    ///
    /// Shifting a line sideways can cut a wide character in half at either end,
    /// and upstream destroys the whole character rather than leaving a stray
    /// half. Skipping that leaves an orphaned padding cell, which renders as a
    /// column that is present in the buffer and invisible on screen.
    pub fn insert_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        let cols = self.cols;

        // Inserting *into* the right half of a wide character kills it.
        self.break_pad_at(y, x);

        let count = n.min(cols - x);
        for _ in 0..count {
            self.lines[y].remove(cols - 1);
            self.lines[y].insert(x, Cell::erased(pen));
        }

        // Whatever now sits on the last column has had its other half pushed
        // off the end.
        self.break_lead_at(y, cols - 1);
    }

    /// DCH — `buffer.c:BuffDeleteChars`.
    pub fn delete_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        let cols = self.cols;
        let count = n.min(cols - x);

        // Either half at the cursor, and either half at the far end of the
        // deleted run, loses its partner.
        self.break_pad_at(y, x);
        self.break_lead_at(y, x);
        if count > 1 && x + count < cols {
            self.break_pad_at(y, x + count);
        }

        for _ in 0..count {
            self.lines[y].remove(x);
            self.lines[y].insert(cols - 1, Cell::erased(pen));
        }
    }

    /// DECFI's scroll — shift every row of the scroll region one column left.
    /// `buffer.c:BuffScrollLeft`.
    pub fn scroll_left(&mut self, n: usize) {
        let pen = self.pen;
        let cols = self.cols;
        let count = n.min(cols);
        for y in self.top..=self.bottom {
            self.break_lead_at(y, cols - 1);
            if count < cols {
                self.break_pad_at(y, count);
            }
            for _ in 0..count {
                self.lines[y].remove(0);
                self.lines[y].push(Cell::erased(pen));
            }
        }
    }

    /// DECBI's scroll. `buffer.c:BuffScrollRight`.
    pub fn scroll_right(&mut self, n: usize) {
        let pen = self.pen;
        let cols = self.cols;
        let count = n.min(cols);
        for y in self.top..=self.bottom {
            self.break_lead_at(y, cols - 1);
            for _ in 0..count {
                self.lines[y].pop();
                self.lines[y].insert(0, Cell::erased(pen));
            }
            self.break_lead_at(y, cols - 1);
        }
    }

    /// ECH.
    pub fn erase_chars(&mut self, n: usize) {
        let x = self.cursor.x;
        self.erase_range_in_line(x, n);
    }

    /// EL. 0 = cursor to end, 1 = start to cursor, 2 = whole line.
    pub fn erase_line(&mut self, mode: u16) {
        let x = self.cursor.x;
        match mode {
            0 => self.erase_range_in_line(x, self.cols - x),
            1 => self.erase_range_in_line(0, x + 1),
            2 => self.erase_range_in_line(0, self.cols),
            _ => {}
        }
    }

    /// ED. 0 = cursor to end of screen, 1 = start to cursor, 2 = all.
    ///
    /// `BuffEraseCurToEnd`/`BuffEraseHomeToCur`, which are *not*
    /// `BuffEraseCharsInLine` in a loop: each does a single `EraseKanji` at the
    /// cursor and then a plain fill, so the wide-character handling differs
    /// from EL's at the far end of the range.
    pub fn erase_display(&mut self, mode: u16) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        match mode {
            0 => {
                self.erase_kanji(y, x, 1);
                for cell in &mut self.lines[y][x..] {
                    *cell = Cell::erased(pen);
                }
                for row in (y + 1)..self.rows {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
            }
            1 => {
                // EraseKanji(0): a wide character *starting* at the cursor has
                // its padding outside the erased range, so it goes too.
                self.erase_kanji(y, x, 0);
                for row in 0..y {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
                let end = (x + 1).min(self.cols);
                for cell in &mut self.lines[y][..end] {
                    *cell = Cell::erased(pen);
                }
            }
            // Mode 3 is not an erase at all — see `clear_buffer`, which the
            // parser routes to because it is gated on a setting.
            2 => {
                for row in 0..self.rows {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
            }
            _ => {}
        }
    }

    // --- selective erase (DECSCA / DECSED / DECSEL) ----------------------

    /// One cell of `buffer.c:BuffSelectedEraseCharsInLine`'s inner loop.
    ///
    /// A selective erase is not an erase. A protected cell is left entirely
    /// alone, and an unprotected one is *crushed* and then has its low byte
    /// masked to `AttrSgrMask` — so bold, underline, blink and reverse survive
    /// DECSEL where they would not survive EL, and the pen is never consulted.
    fn selective_erase_cell(&mut self, y: usize, x: usize) {
        let cell = &mut self.lines[y][x];
        if cell.attrs & ATTR2_PROTECT != 0 {
            return;
        }
        cell.crush();
        cell.attrs &= ATTR_SGR_MASK | !ATTR_MASK;
    }

    /// The kanji fixup shared by every selective-erase entry point. Upstream
    /// gates it on the *cursor* cell being unprotected even when the pair it
    /// would break lies elsewhere.
    fn selective_erase_kanji(&mut self, lr: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        if self.lines[y][x].attrs & ATTR2_PROTECT == 0 {
            self.erase_kanji(y, x, lr);
        }
    }

    /// DECSEL. 0 = cursor to end, 1 = start to cursor, 2 = whole line.
    pub fn selective_erase_line(&mut self, mode: u16) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let (start, end) = match mode {
            0 => (x, self.cols),
            1 => (0, (x + 1).min(self.cols)),
            2 => (0, self.cols),
            _ => return,
        };
        self.selective_erase_kanji(1);
        for x in start..end {
            self.selective_erase_cell(y, x);
        }
    }

    /// DECSED 0 — `buffer.c:BuffSelectedEraseCurToEnd`.
    pub fn selective_erase_to_end(&mut self) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        self.selective_erase_kanji(1);
        for col in x..self.cols {
            self.selective_erase_cell(y, col);
        }
        for row in (y + 1)..self.rows {
            for col in 0..self.cols {
                self.selective_erase_cell(row, col);
            }
        }
    }

    /// DECSED 1 — `buffer.c:BuffSelectedEraseHomeToCur`.
    pub fn selective_erase_to_cursor(&mut self) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        self.selective_erase_kanji(0);
        for row in 0..y {
            for col in 0..self.cols {
                self.selective_erase_cell(row, col);
            }
        }
        for col in 0..(x + 1).min(self.cols) {
            self.selective_erase_cell(y, col);
        }
    }

    /// `buffer.c:ClearBuffer` — ED 3 and DECSED 3, and far more than "drop the
    /// scrollback": it wipes the whole allocation, homes the cursor and resets
    /// the scroll region. Gated on `TF_REMOTECLEARSBUFF`, which ships on.
    pub fn clear_buffer(&mut self) {
        let pen = self.pen;
        self.scrollback.clear();
        for line in &mut self.lines {
            *line = vec![Cell::erased(pen); self.cols];
        }
        self.cursor = Cursor::default();
        self.top = 0;
        self.bottom = self.rows - 1;
    }

    // --- rectangular areas (DECSACE / DECCARA / DECRARA / DECFRA / ...) --

    /// The cells an area covers, one `(row, start, end)` span per row with
    /// `end` exclusive, clamped the way `buffer.c` clamps.
    ///
    /// `rect` is DECSACE's `RectangleMode`: true gives the same column range on
    /// every row, false gives a stream that runs from the start column to the
    /// end of its row, through whole rows, and stops at the end column.
    fn area_spans(&self, rect: bool, area: Rect) -> Vec<(usize, usize, usize)> {
        let Rect { x0, y0, .. } = area;
        let x1 = area.x1.min(self.cols - 1);
        let y1 = area.y1.min(self.rows - 1);
        if x0 > x1 || y0 > y1 || y0 >= self.rows {
            return Vec::new();
        }
        (y0..=y1)
            .map(|y| {
                if rect || y0 == y1 {
                    (y, x0, x1 + 1)
                } else if y == y0 {
                    (y, x0, self.cols)
                } else if y == y1 {
                    (y, 0, x1 + 1)
                } else {
                    (y, 0, self.cols)
                }
            })
            .collect()
    }

    /// The two cells an area operation touches *outside* its span because a
    /// wide character straddles the edge. `buffer.c` spells this out at every
    /// call site; it is the same rule each time.
    fn span_edges(&self, y: usize, start: usize, end: usize) -> (Option<usize>, Option<usize>) {
        let left =
            (start > 0 && self.lines[y][start - 1].width_class == WIDTH_WIDE).then(|| start - 1);
        let right =
            (end < self.cols && end > 0 && self.lines[y][end - 1].width_class == WIDTH_WIDE)
                .then_some(end);
        (left, right)
    }

    /// DECCARA (`mask` = `Some`) and DECRARA (`mask` = `None`), over either a
    /// rectangle or a stream — `buffer.c:BuffChangeAttrBox` /
    /// `BuffChangeAttrStream`.
    ///
    /// With a mask, each named bit is replaced and the rest left alone; the
    /// colour indices move only when their `Attr2` bit is in the mask. Without
    /// one, DECRARA **toggles** the named attributes and touches nothing else.
    /// Either way a wide character straddling an edge is changed with the cells
    /// it overlaps, so a half never ends up a different colour from its other
    /// half.
    pub fn change_attr_area(&mut self, rect: bool, area: Rect, attr: Pen, mask: Option<u32>) {
        for (y, start, end) in self.area_spans(rect, area) {
            let (left, right) = self.span_edges(y, start, end);
            for x in left.into_iter().chain(start..end).chain(right) {
                let cell = &mut self.lines[y][x];
                match mask {
                    Some(m) => {
                        cell.attrs = (cell.attrs & !m) | attr.attrs;
                        if m & ATTR2_FORE != 0 {
                            cell.fg = attr.fg;
                        }
                        if m & ATTR2_BACK != 0 {
                            cell.bg = attr.bg;
                        }
                    }
                    None => cell.attrs ^= attr.attrs & ATTR_MASK,
                }
            }
        }
    }

    /// DECFRA — `buffer.c:BuffFillBox`. Fills with the **whole** pen, not the
    /// erase subset, so bold and the protect bit come along.
    pub fn fill_box(&mut self, cp: u32, area: Rect) {
        let pen = self.pen;
        for (y, start, end) in self.area_spans(true, area) {
            let (left, right) = self.span_edges(y, start, end);
            if let Some(x) = left {
                self.lines[y][x].crush();
            }
            if let Some(x) = right {
                self.lines[y][x].crush();
            }
            for x in start..end {
                self.lines[y][x] = Cell {
                    text: [cp, 0, 0, 0],
                    fg: pen.fg,
                    bg: pen.bg,
                    attrs: pen.attrs,
                    width_class: WIDTH_NARROW,
                };
            }
        }
    }

    /// DECERA — `buffer.c:BuffEraseBox`. The straddling halves are blanked with
    /// the *full* pen, the interior with the erase subset, exactly as the two
    /// different helpers upstream uses imply.
    pub fn erase_box(&mut self, area: Rect) {
        let pen = self.pen;
        for (y, start, end) in self.area_spans(true, area) {
            let (left, right) = self.span_edges(y, start, end);
            for x in left.into_iter().chain(right) {
                self.lines[y][x] = Cell::blank(pen);
            }
            for x in start..end {
                self.lines[y][x] = Cell::erased(pen);
            }
        }
    }

    /// DECSERA — `buffer.c:BuffSelectiveEraseBox`.
    pub fn selective_erase_box(&mut self, area: Rect) {
        for (y, start, end) in self.area_spans(true, area) {
            let (left, right) = self.span_edges(y, start, end);
            // The straddling halves are skipped when *they* are protected,
            // which is a different test from the interior's.
            for x in left.into_iter().chain(right) {
                self.selective_erase_cell(y, x);
            }
            for x in start..end {
                self.selective_erase_cell(y, x);
            }
        }
    }

    /// DECCRA — `buffer.c:BuffCopyBox`. Cells are copied whole, attributes and
    /// width class included, with no wide-character fixup at either edge.
    pub fn copy_box(&mut self, src: Rect, dx: usize, dy: usize) {
        let (sx0, sy0) = (src.x0, src.y0);
        let sx1 = src.x1.min(self.cols - 1);
        let sy1 = src.y1.min(self.rows - 1);
        if sx0 > sx1 || sy0 > sy1 || dx > self.cols - 1 || dy > self.rows - 1 {
            return;
        }
        let cols = (sx1 - sx0 + 1).min(self.cols - dx);
        let rows = (sy1 - sy0 + 1).min(self.rows - dy);

        // Copy away from the overlap: downward when the destination is above
        // the source, upward when it is below. Upstream branches on the
        // *column* comparison and only falls back to a row-safe move when the
        // columns are equal; the row order below is what that amounts to.
        let order: Vec<usize> = if dy <= sy0 {
            (0..rows).collect()
        } else {
            (0..rows).rev().collect()
        };
        for i in order {
            let src = self.lines[sy0 + i][sx0..sx0 + cols].to_vec();
            self.lines[dy + i][dx..dx + cols].copy_from_slice(&src);
        }
    }

    // --- writing ---------------------------------------------------------

    /// Write one codepoint at the cursor, wrapping and advancing as Tera Term's
    /// `PutU32NoLog` does.
    pub fn put(&mut self, cp: u32) {
        if self.put_combining(cp) {
            return;
        }

        if self.cursor.pending_wrap {
            self.carriage_return();
            self.line_feed();
            self.cursor.pending_wrap = false;
        }

        let w = char_width(cp).max(1);

        // A double-width glyph with only one column left: Tera Term parks a
        // space in the orphan cell so the glyph is never split in half, then
        // wraps and retries.
        if w == 2 && self.cursor.x + 1 > self.cols - 1 {
            if self.autowrap {
                let (x, y) = (self.cursor.x, self.cursor.y);
                let pen = self.pen;
                self.lines[y][x] = Cell::blank(pen);
                self.carriage_return();
                self.line_feed();
            } else {
                self.cursor.x = 0;
            }
        }

        self.place(cp, w);

        let x = self.cursor.x;
        if w == 1 {
            if x >= self.cols - 1 {
                self.cursor.pending_wrap = self.autowrap;
            } else {
                self.cursor.x = x + 1;
                self.cursor.pending_wrap = false;
            }
        } else if x + 1 >= self.cols - 1 {
            // Cursor lands on the padding half, as it does upstream.
            self.cursor.x = x + 1;
            self.cursor.pending_wrap = self.autowrap;
        } else {
            self.cursor.x = x + 2;
            self.cursor.pending_wrap = false;
        }
    }

    fn place(&mut self, cp: u32, w: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;

        // Landing on either half of an existing wide character must clear the
        // other half, or a padding cell is left orphaned and the line renders
        // one column short.
        self.split_wide_at(y, x);
        if w == 2 {
            self.split_wide_at(y, x + 1);
        }

        if self.insert_mode {
            let cols = self.cols;
            for _ in 0..w.min(cols - x) {
                self.lines[y].remove(cols - 1);
                self.lines[y].insert(x, Cell::erased(pen));
            }
        }

        self.lines[y][x] = Cell {
            text: [cp, 0, 0, 0],
            fg: pen.fg,
            bg: pen.bg,
            attrs: pen.attrs,
            width_class: if w == 2 { WIDTH_WIDE } else { WIDTH_NARROW },
        };
        if w == 2 && x + 1 < self.cols {
            // The padding half is written with *zeroed* attributes, not the
            // pen — `buffer.c:3400` sets `attr`, `attr2`, `fg` and `bg` all to
            // 0. So a background-coloured wide character reports its colour on
            // the lead cell and nothing on the pad, which is visible the moment
            // anything dumps attributes per column.
            self.lines[y][x + 1] = Cell {
                text: [0, 0, 0, 0],
                fg: DEFAULT_FG,
                bg: DEFAULT_BG,
                attrs: 0,
                width_class: WIDTH_PAD,
            };
        }
    }

    /// Attach a combining mark to the base cell. Returns true if it was
    /// consumed, false if the codepoint is a normal spacing character.
    fn put_combining(&mut self, cp: u32) -> bool {
        if !is_combining(cp) {
            return false;
        }
        let y = self.cursor.y;

        // With a wrap pending the cursor is still parked on the character just
        // written; otherwise the base is the cell to its left. Tera Term passes
        // `Wrap` into `IsCombiningChar` for exactly this reason.
        let base = if self.cursor.pending_wrap {
            Some(self.cursor.x)
        } else if self.cursor.x > 0 {
            Some(self.cursor.x - 1)
        } else {
            None
        };

        let Some(mut base) = base else {
            // A mark with nothing to attach to: Tera Term invents a NBSP base
            // and advances one column.
            self.place(0xa0, 1);
            let x = self.cursor.x;
            self.lines[y][x].push(cp);
            if x >= self.cols - 1 {
                self.cursor.pending_wrap = self.autowrap;
            } else {
                self.cursor.x = x + 1;
            }
            return true;
        };

        // Step back off a padding cell onto the wide character that owns it.
        if self.lines[y][base].width_class == WIDTH_PAD && base > 0 {
            base -= 1;
        }
        self.lines[y][base].push(cp);
        true
    }

    /// If `x` holds the **right** half of a wide character, crush the pair.
    /// Leaves a left half alone — the distinction matters when a shift keeps
    /// one side and discards the other.
    fn break_pad_at(&mut self, y: usize, x: usize) {
        if x < self.cols && x > 0 && self.lines[y][x].width_class == WIDTH_PAD {
            self.lines[y][x].crush();
            self.lines[y][x - 1].crush();
        }
    }

    /// If `x` holds the **left** half of a wide character, crush the pair.
    fn break_lead_at(&mut self, y: usize, x: usize) {
        if x < self.cols && self.lines[y][x].width_class == WIDTH_WIDE {
            self.lines[y][x].crush();
            if x + 1 < self.cols {
                self.lines[y][x + 1].crush();
            }
        }
    }

    /// Break a wide character straddling column `x`, from either side. A no-op
    /// if `x` is out of range or holds a narrow cell.
    ///
    /// This is the *overwrite* path — the three `BuffSetChar(p, ' ', 'H')`
    /// pairs at `buffer.c:3221-3270` — so it crushes rather than erases. The
    /// erase paths want [`Grid::erase_kanji`] instead.
    fn split_wide_at(&mut self, y: usize, x: usize) {
        if x >= self.cols {
            return;
        }
        match self.lines[y][x].width_class {
            WIDTH_WIDE => {
                self.lines[y][x].crush();
                if x + 1 < self.cols {
                    self.lines[y][x + 1].crush();
                }
            }
            WIDTH_PAD => {
                self.lines[y][x].crush();
                if x > 0 {
                    self.lines[y][x - 1].crush();
                }
            }
            _ => {}
        }
    }

    /// `buffer.c:EraseKanji` — the *erase* paths' way of breaking a wide
    /// character, which does paint the pen over both halves.
    ///
    /// `lr` is upstream's argument, and it decides which cell is inspected:
    /// `1` asks "does a wide character end at `x`, i.e. is `x` its padding?",
    /// `0` asks "does one start at `x`?". Note it never looks further than one
    /// cell, so a wide character *starting* at the far end of an erased range
    /// is left whole on purpose.
    fn erase_kanji(&mut self, y: usize, x: usize, lr: usize) -> bool {
        if x < lr {
            return false;
        }
        let bx = x - lr;
        if bx >= self.cols || self.lines[y][bx].width_class != WIDTH_WIDE {
            return false;
        }
        // EraseKanji copies the whole pen, bold and all — unlike `memsetW`,
        // which passes AttrDefault. So the two halves can end up carrying
        // attributes the erased range around them does not.
        let pen = self.pen;
        self.lines[y][bx] = Cell::blank(pen);
        if bx + 1 < self.cols {
            self.lines[y][bx + 1] = Cell::blank(pen);
        }
        true
    }

    /// `buffer.c:BuffEraseCharsInLine` — ECH and all three EL modes go through
    /// here. The head check is always at the **cursor**, not at `start`; the
    /// tail check is at the end of the range and only when that is on screen.
    fn erase_range_in_line(&mut self, start: usize, count: usize) {
        let (cx, y) = (self.cursor.x, self.cursor.y);
        self.erase_kanji(y, cx, 1);
        if start + count < self.cols {
            self.erase_kanji(y, start + count, 1);
        }
        let pen = self.pen;
        let end = (start + count).min(self.cols);
        for cell in &mut self.lines[y][start..end] {
            *cell = Cell::erased(pen);
        }
    }
}

fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(|x| x > 0 && x % 8 == 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(grid: &Grid, y: usize) -> String {
        let mut out = String::new();
        let mut col = 0;
        for cell in grid.line(y) {
            if cell.width_class == WIDTH_PAD {
                continue;
            }
            let mut any = false;
            for cp in cell.codepoints() {
                out.push(char::from_u32(cp).unwrap_or('?'));
                any = true;
            }
            if !any {
                out.push(' ');
            }
            col += char_width(cell.text[0]).max(1);
            if col >= grid.cols() {
                break;
            }
        }
        out
    }

    #[test]
    fn deferred_wrap_leaves_cursor_on_the_last_column() {
        let mut g = Grid::new(8, 4, 0);
        for c in "ABCDEFGH".chars() {
            g.put(c as u32);
        }
        assert_eq!((g.cursor.x, g.cursor.y), (7, 0));
        assert!(g.cursor.pending_wrap);
        g.put('I' as u32);
        assert_eq!((g.cursor.x, g.cursor.y), (1, 1));
    }

    #[test]
    fn combining_mark_joins_the_previous_cell() {
        let mut g = Grid::new(10, 2, 0);
        g.put('e' as u32);
        g.put(0x0301);
        assert_eq!(g.cursor.x, 1);
        assert_eq!(g.line(0)[0].text[0], 'e' as u32);
        assert_eq!(g.line(0)[0].text[1], 0x0301);
    }

    #[test]
    fn wide_character_claims_two_columns() {
        let mut g = Grid::new(10, 2, 0);
        g.put(0x4f60); // 你
        assert_eq!(g.cursor.x, 2);
        assert_eq!(g.line(0)[0].width_class, WIDTH_WIDE);
        assert_eq!(g.line(0)[1].width_class, WIDTH_PAD);
    }

    #[test]
    fn overwriting_half_a_wide_character_clears_both_halves() {
        let mut g = Grid::new(10, 2, 0);
        g.put(0x4f60);
        g.move_cursor(1, 0);
        g.put('x' as u32);
        assert_eq!(g.line(0)[0].width_class, WIDTH_NARROW);
        assert_eq!(g.line(0)[0].text[0], b' ' as u32);
        assert_eq!(g.line(0)[1].text[0], 'x' as u32);
        assert_eq!(&render(&g, 0)[..2], " x");
    }

    #[test]
    fn erase_keeps_colour_but_drops_attributes() {
        let mut g = Grid::new(4, 1, 0);
        g.pen = Pen {
            fg: 3,
            bg: 5,
            attrs: ATTR_BOLD | ATTR2_FORE | ATTR2_BACK,
        };
        g.erase_line(2);
        let c = g.line(0)[0];
        assert_eq!(c.fg, 3);
        assert_eq!(c.bg, 5);
        assert_eq!(c.attrs, ATTR2_FORE | ATTR2_BACK);
    }

    #[test]
    fn scroll_region_confines_line_feed() {
        let mut g = Grid::new(4, 6, 0);
        g.set_scroll_region(1, 3);
        assert_eq!(g.cursor.y, 0);
        for _ in 0..10 {
            g.line_feed();
        }
        assert_eq!(g.cursor.y, 3);
    }
}
