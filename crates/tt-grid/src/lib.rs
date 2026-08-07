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

pub const ATTR2_FORE: u32 = 0x0100;
pub const ATTR2_BACK: u32 = 0x0200;
pub const ATTR2_COLOR_MASK: u32 = ATTR2_FORE | ATTR2_BACK;

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

    /// ICH.
    pub fn insert_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        let cols = self.cols;
        for _ in 0..n.min(cols - x) {
            self.lines[y].remove(cols - 1);
            self.lines[y].insert(x, Cell::erased(pen));
        }
    }

    /// DCH.
    pub fn delete_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        let cols = self.cols;
        for _ in 0..n.min(cols - x) {
            self.lines[y].remove(x);
            self.lines[y].insert(cols - 1, Cell::erased(pen));
        }
    }

    /// ECH.
    pub fn erase_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        let end = (x + n).min(self.cols);
        self.split_wide_at(y, x);
        self.split_wide_at(y, end);
        for cell in &mut self.lines[y][x..end] {
            *cell = Cell::erased(pen);
        }
    }

    /// EL. 0 = cursor to end, 1 = start to cursor, 2 = whole line.
    pub fn erase_line(&mut self, mode: u16) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let (start, end) = match mode {
            0 => (x, self.cols),
            1 => (0, (x + 1).min(self.cols)),
            2 => (0, self.cols),
            _ => return,
        };
        let pen = self.pen;
        self.split_wide_at(y, start);
        self.split_wide_at(y, end);
        for cell in &mut self.lines[y][start..end] {
            *cell = Cell::erased(pen);
        }
    }

    /// ED. 0 = cursor to end of screen, 1 = start to cursor, 2 = all.
    pub fn erase_display(&mut self, mode: u16) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let pen = self.pen;
        match mode {
            0 => {
                self.erase_line(0);
                for row in (y + 1)..self.rows {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
            }
            1 => {
                for row in 0..y {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
                self.erase_line(1);
                let _ = x;
            }
            2 | 3 => {
                for row in 0..self.rows {
                    self.lines[row] = vec![Cell::erased(pen); self.cols];
                }
            }
            _ => {}
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
            self.lines[y][x + 1] = Cell {
                text: [0, 0, 0, 0],
                fg: pen.fg,
                bg: pen.bg,
                attrs: pen.attrs,
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

    /// Break a wide character straddling column `x`, replacing both halves with
    /// blanks. A no-op if `x` is out of range or holds a narrow cell.
    fn split_wide_at(&mut self, y: usize, x: usize) {
        if x >= self.cols {
            return;
        }
        let pen = self.pen;
        match self.lines[y][x].width_class {
            WIDTH_WIDE => {
                self.lines[y][x] = Cell::erased(pen);
                if x + 1 < self.cols {
                    self.lines[y][x + 1] = Cell::erased(pen);
                }
            }
            WIDTH_PAD => {
                self.lines[y][x] = Cell::erased(pen);
                if x > 0 {
                    self.lines[y][x - 1] = Cell::erased(pen);
                }
            }
            _ => {}
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
